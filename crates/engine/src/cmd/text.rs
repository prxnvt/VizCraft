//! Text commands: character format (font family, size, bold, italic, underline, strikethrough,
//! colour, highlight), size steps, letter case conversion, paragraph layout (alignment, bullets,
//! indent, direction, margins), the text block, symbols and fields inserted into text, and
//! standalone text boxes.
//!
//! Format and paragraph settings are style fields, so they apply to the whole text of each target.
//! Case conversion rewrites the stored text (and resets the drawn `textCase`, so the result shows);
//! field placeholders (see `vizcraft_diagram::text_fields`) are kept intact by it. Text changes go
//! through the same path as `shape.setText` and `connector.setText`: a view of a model element or
//! relationship renames it.
//!
//! Given a `compartment` id, the character, size, case, alignment and insert commands act on that
//! compartment of each target shape instead (see `cmd::compartment`): its text or rows, and its
//! size, bold, colour and alignment overrides.

use serde_json::{Value, json};
use vizcraft_diagram::text_fields::{CaseChange, FIELDS, Field, TEXT_BOX_STENCIL, change_case};
use vizcraft_diagram::{Bullets, Color, HAlign, Margins, Shape, ShapeKind, Style, TextBlock, TextCase, TextDirection, VAlign};

use super::compartment as band;
use super::{Kinds, MAX_DELTA, has_shapes, num, opt_bool, opt_str, page_mut, req_num, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all text commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("text.setFormat", "Font", "Home > Font", set_format)
            .params(r##"{"ids"?: [string], "fontFamily"?: string ("" = default), "size"?: n, "bold"?: bool, "italic"?: bool, "underline"?: bool, "strikethrough"?: bool, "color"?: "#rrggbb", "highlight"?: "#rrggbb"|false|"none", "compartment"?: id}; shapes and connectors; with compartment, that band of each shape (size, bold, color only)"##)
            .when(has_shapes),
        CommandSpec::new("text.grow", "Font Size Step", "Home > Font", grow)
            .params(r#"{"ids"?: [string], "step": 1|-1, "compartment"?: id} -> {"sizes"}; to the next standard size up or down"#)
            .when(has_shapes),
        CommandSpec::new("text.setCase", "Change Case", "Home > Font", set_case)
            .params(r#"{"ids"?: [string], "case": "sentence"|"lower"|"upper"|"toggle"|"title", "compartment"?: id}; rewrites the stored text (a compartment's text or rows), field placeholders kept"#)
            .when(has_shapes),
        CommandSpec::new("text.setParagraph", "Paragraph", "Home > Paragraph", set_paragraph)
            .params(r#"{"ids"?: [string], "hAlign"?: "left"|"center"|"right"|"justify", "vAlign"?: "top"|"middle"|"bottom", "bullets"?: "none"|"disc"|"dash"|"number"|bool, "indent"?: n, "direction"?: "ltr"|"rtl"|"vertical", "margins"?: n|{"left"?, "top"?, "right"?, "bottom"?}, "compartment"?: id}; with compartment, hAlign only"#)
            .when(has_shapes),
        CommandSpec::new("text.setBlock", "Text Block", "Home > Tools", set_block)
            .params(r#"{"id": string, "dx"?: n, "dy"?: n, "w"?: n, "h"?: n, "angle"?: degrees, "reset"?: bool} -> {"id", "textBlock"}; dx, dy offset the block centre from the frame centre; reset puts the text back in the frame"#)
            .when(has_shapes),
        CommandSpec::new("text.insertSymbol", "Symbol", "Insert > Text", insert_symbol)
            .params(r#"{"ids"?: [string], "symbol"?: string, "at"?: n, "compartment"?: id} -> {"ids"}; appended, or inserted at character `at`; no symbol opens the Symbol dialog"#)
            .when(has_shapes),
        CommandSpec::new("text.insertField", "Field", "Insert > Text", insert_field)
            .params(r#"{"ids"?: [string], "field": "date"|"time"|"pageName"|"pageNumber"|"pageCount"|"title"|"width"|"height"|"data", "key"?: string, "at"?: n, "compartment"?: id} -> {"ids", "placeholder"}; data needs key"#)
            .when(has_shapes),
        CommandSpec::new("shape.addTextBox", "Text Box", "Insert > Text", add_text_box)
            .params(r#"{"x": n, "y": n, "w"?: n, "h"?: n, "text"?: string, "id"?: string} -> {"id"}; a shape with no fill and no line; x, y is the top-left"#),
    ]
}

/// Font sizes the size step moves between, in points.
pub const SIZE_STEPS: &[f64] =
    &[6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 48.0, 54.0, 60.0, 72.0, 96.0];
/// Default size of a text box clicked onto the page.
pub const TEXT_BOX_W: f64 = 120.0;
/// Default height of a text box.
pub const TEXT_BOX_H: f64 = 30.0;
/// Longest symbol inserted, in bytes.
const MAX_SYMBOL: usize = 64;

fn bad(k: &str, what: &str) -> EngineError {
    EngineError::Params(format!("`{k}` must be {what}"))
}

fn color_param(v: &Value, k: &str) -> Result<Option<Color>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => Color::parse(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not a #rrggbb colour", short(t)))),
    }
}

fn named<T>(v: &Value, k: &str, from: fn(&str) -> Option<T>, names: &str) -> Result<Option<T>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => from(t).map(Some).ok_or_else(|| bad(k, &format!("one of {names}"))),
    }
}

/// Character format given to `text.setFormat`, checked before anything changes.
#[derive(Default)]
struct FormatPatch {
    family: Option<String>,
    size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strikethrough: Option<bool>,
    color: Option<Color>,
    highlight: Option<Color>,
}

impl FormatPatch {
    fn parse(v: &Value) -> Result<Self, EngineError> {
        let highlight = match v.get("highlight") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(false)) => Some(Color::TRANSPARENT),
            Some(Value::String(t)) if t == "none" => Some(Color::TRANSPARENT),
            Some(Value::String(_)) => color_param(v, "highlight")?,
            Some(_) => return Err(bad("highlight", "a #rrggbb colour, false or \"none\"")),
        };
        let p = FormatPatch {
            family: opt_str(v, "fontFamily")?.map(|f| f.trim().to_string()),
            size: num(v, "size")?,
            bold: opt_bool(v, "bold")?,
            italic: opt_bool(v, "italic")?,
            underline: opt_bool(v, "underline")?,
            strikethrough: opt_bool(v, "strikethrough")?,
            color: color_param(v, "color")?,
            highlight,
        };
        let empty = p.family.is_none()
            && p.size.is_none()
            && p.bold.is_none()
            && p.italic.is_none()
            && p.underline.is_none()
            && p.strikethrough.is_none()
            && p.color.is_none()
            && p.highlight.is_none();
        if empty {
            return Err(EngineError::Params(
                "give at least one of fontFamily, size, bold, italic, underline, strikethrough, color, highlight".into(),
            ));
        }
        Ok(p)
    }

    fn apply(&self, st: &mut Style) {
        if let Some(f) = &self.family {
            st.font_family = f.clone();
        }
        if let Some(v) = self.size {
            st.font_size = v;
        }
        if let Some(v) = self.bold {
            st.bold = v;
        }
        if let Some(v) = self.italic {
            st.italic = v;
        }
        if let Some(v) = self.underline {
            st.underline = v;
        }
        if let Some(v) = self.strikethrough {
            st.strikethrough = v;
        }
        if let Some(c) = self.color {
            st.font_color = c;
        }
        if let Some(c) = self.highlight {
            st.highlight = c;
        }
        st.sanitize();
    }
}

/// Run `f` on the style of every target (shape or connector), then sanitise the item.
fn restyle(s: &mut Session, ids: &[String], f: impl Fn(&mut Style)) -> Result<(), EngineError> {
    super::layer::ensure_unlocked(s, ids)?;
    let page = page_mut(s)?;
    for id in ids {
        if let Some(sh) = page.shape_mut(id) {
            f(&mut sh.style);
            sh.sanitize();
        } else if let Some(c) = page.connector_mut(id) {
            f(&mut c.style);
            c.sanitize();
        }
    }
    Ok(())
}

fn set_format(s: &mut Session, v: &Value) -> CmdResult {
    let patch = FormatPatch::parse(v)?;
    if let Some(cid) = band::param(v)? {
        if patch.family.is_some() || patch.italic.is_some() || patch.underline.is_some() || patch.strikethrough.is_some() || patch.highlight.is_some()
        {
            return Err(EngineError::Params("with `compartment`, give size, bold or color".into()));
        }
        let ids = targets(s, v, Kinds::Shapes)?;
        let ids = band::each(s, &ids, &cid, |c, _| {
            c.font_size = patch.size.or(c.font_size);
            c.bold = patch.bold.or(c.bold);
            c.font_color = patch.color.or(c.font_color);
        })?;
        return with_sel(s, json!({"styled": ids}));
    }
    let ids = targets(s, v, Kinds::Both)?;
    restyle(s, &ids, |st| patch.apply(st))?;
    with_sel(s, json!({"styled": ids}))
}

/// The next standard size above (`up`) or below `size`; past the ends of [`SIZE_STEPS`] it moves
/// by the last gap.
pub fn step_size(size: f64, up: bool) -> f64 {
    let next = if up { SIZE_STEPS.iter().copied().find(|s| *s > size + 1e-9) } else { SIZE_STEPS.iter().rev().copied().find(|s| *s < size - 1e-9) };
    let out = next.unwrap_or(if up { size + 24.0 } else { size - 1.0 });
    out.clamp(1.0, Style::MAX_FONT_SIZE)
}

fn grow(s: &mut Session, v: &Value) -> CmdResult {
    let step = req_num(v, "step")?;
    if step != 1.0 && step != -1.0 {
        return Err(bad("step", "1 or -1"));
    }
    let up = step > 0.0;
    if let Some(cid) = band::param(v)? {
        let ids = targets(s, v, Kinds::Shapes)?;
        let mut sizes = Vec::new();
        let ids = band::each(s, &ids, &cid, |c, base| {
            let next = step_size(c.font_size.unwrap_or(base.font_size), up);
            c.font_size = Some(next);
            sizes.push(next);
        })?;
        return with_sel(s, json!({"styled": ids, "sizes": sizes}));
    }
    let ids = targets(s, v, Kinds::Both)?;
    restyle(s, &ids, |st| st.font_size = step_size(st.font_size, up))?;
    let page = page_mut(s)?;
    let sizes: Vec<f64> =
        ids.iter().filter_map(|id| page.shape(id).map(|x| x.style.font_size).or_else(|| page.connector(id).map(|c| c.style.font_size))).collect();
    with_sel(s, json!({"styled": ids, "sizes": sizes}))
}

/// Set the stored text of a shape or connector the way `shape.setText` and `connector.setText`
/// do: a view of a model element or relationship renames it (and so every view of it).
pub fn put_text(s: &mut Session, id: &str, text: &str) -> Result<(), EngineError> {
    super::protect::check(s, [id], super::protect::Lock::Text)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(sh) = page.shape(id) {
        match sh.element.clone().filter(|e| s.doc.model.element(e).is_some()) {
            Some(e) => super::model::rename_element(&mut s.doc, &e, text)?,
            None => {
                if let Some(sh) = page_mut(s)?.shape_mut(id) {
                    sh.text = text.to_string();
                    sh.sanitize();
                }
            }
        }
    } else if let Some(c) = page.connector(id) {
        match c.relationship.clone().filter(|r| s.doc.model.relationship(r).is_some()) {
            Some(r) => super::model::rename_relationship(&mut s.doc, &r, text)?,
            None => {
                if let Some(c) = page_mut(s)?.connector_mut(id) {
                    c.text = text.to_string();
                    c.sanitize();
                }
            }
        }
    } else {
        return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(id))));
    }
    Ok(())
}

/// The stored text of a shape or connector on the active page.
fn text_of(s: &Session, id: &str) -> Option<String> {
    let page = s.doc.page()?;
    page.shape(id).map(|x| x.text.clone()).or_else(|| page.connector(id).map(|c| c.text.clone()))
}

fn set_case(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "case")?;
    let change = CaseChange::parse(name).ok_or_else(|| bad("case", "one of sentence, lower, upper, toggle, title"))?;
    if let Some(cid) = band::param(v)? {
        let ids = targets(s, v, Kinds::Shapes)?;
        let ids = band::each(s, &ids, &cid, |c, _| {
            c.text = change_case(&c.text, change);
            c.rows.iter_mut().for_each(|r| *r = change_case(r, change));
        })?;
        return with_sel(s, json!({"changed": ids}));
    }
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
    for id in &ids {
        if let Some(t) = text_of(s, id) {
            put_text(s, id, &change_case(&t, change))?;
        }
    }
    // The drawn case would hide the change; the stored text now carries it.
    restyle(s, &ids, |st| st.text_case = TextCase::Normal)?;
    with_sel(s, json!({"changed": ids}))
}

/// Paragraph settings given to `text.setParagraph`.
#[derive(Default)]
struct ParagraphPatch {
    h_align: Option<HAlign>,
    v_align: Option<VAlign>,
    bullets: Option<Bullets>,
    indent: Option<f64>,
    direction: Option<TextDirection>,
    margins: Option<MarginsPatch>,
}

/// New margins: all four sides, or some of them.
#[derive(Default)]
struct MarginsPatch {
    left: Option<f64>,
    top: Option<f64>,
    right: Option<f64>,
    bottom: Option<f64>,
}

impl ParagraphPatch {
    fn parse(v: &Value) -> Result<Self, EngineError> {
        let bullets = match v.get("bullets") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(if *b { Bullets::Disc } else { Bullets::None }),
            Some(_) => named(v, "bullets", Bullets::from_name, "none, disc, dash, number, true, false")?,
        };
        let margins = match v.get("margins") {
            None | Some(Value::Null) => None,
            Some(Value::Number(_)) => {
                let m = num(v, "margins")?;
                Some(MarginsPatch { left: m, top: m, right: m, bottom: m })
            }
            Some(o @ Value::Object(_)) => {
                let m = MarginsPatch { left: num(o, "left")?, top: num(o, "top")?, right: num(o, "right")?, bottom: num(o, "bottom")? };
                if m.left.is_none() && m.top.is_none() && m.right.is_none() && m.bottom.is_none() {
                    return Err(bad("margins", "a number or {left?, top?, right?, bottom?} with at least one side"));
                }
                Some(m)
            }
            Some(_) => return Err(bad("margins", "a number or {left?, top?, right?, bottom?}")),
        };
        let p = ParagraphPatch {
            h_align: named(v, "hAlign", HAlign::from_name, "left, center, right, justify")?,
            v_align: named(v, "vAlign", VAlign::from_name, "top, middle, bottom")?,
            bullets,
            indent: num(v, "indent")?,
            direction: named(v, "direction", TextDirection::from_name, "ltr, rtl, vertical")?,
            margins,
        };
        if p.h_align.is_none() && p.v_align.is_none() && p.bullets.is_none() && p.indent.is_none() && p.direction.is_none() && p.margins.is_none() {
            return Err(EngineError::Params("give at least one of hAlign, vAlign, bullets, indent, direction, margins".into()));
        }
        Ok(p)
    }

    fn apply(&self, st: &mut Style) {
        if let Some(a) = self.h_align {
            st.h_align = a;
        }
        if let Some(a) = self.v_align {
            st.v_align = a;
        }
        if let Some(b) = self.bullets {
            st.bullets = b;
        }
        if let Some(i) = self.indent {
            st.indent = i;
        }
        if let Some(d) = self.direction {
            st.text_direction = d;
        }
        if let Some(m) = &self.margins {
            let Margins { left, top, right, bottom } = st.text_margins;
            st.text_margins = Margins {
                left: m.left.unwrap_or(left),
                top: m.top.unwrap_or(top),
                right: m.right.unwrap_or(right),
                bottom: m.bottom.unwrap_or(bottom),
            };
        }
        st.sanitize();
    }
}

fn set_paragraph(s: &mut Session, v: &Value) -> CmdResult {
    let patch = ParagraphPatch::parse(v)?;
    if let Some(cid) = band::param(v)? {
        let Some(align) = patch.h_align else { return Err(EngineError::Params("with `compartment`, give hAlign".into())) };
        if patch.v_align.is_some() || patch.bullets.is_some() || patch.indent.is_some() || patch.direction.is_some() || patch.margins.is_some() {
            return Err(EngineError::Params("with `compartment`, give hAlign only".into()));
        }
        let ids = targets(s, v, Kinds::Shapes)?;
        let ids = band::each(s, &ids, &cid, |c, _| c.align = align)?;
        return with_sel(s, json!({"styled": ids}));
    }
    let ids = targets(s, v, Kinds::Both)?;
    restyle(s, &ids, |st| patch.apply(st))?;
    with_sel(s, json!({"styled": ids}))
}

fn set_block(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let reset = opt_bool(v, "reset")?.unwrap_or(false);
    let d = |k: &str| -> Result<Option<f64>, EngineError> { Ok(num(v, k)?.map(|x| x.clamp(-MAX_DELTA, MAX_DELTA))) };
    let (dx, dy, w, h, angle) = (d("dx")?, d("dy")?, d("w")?, d("h")?, d("angle")?);
    if !reset && dx.is_none() && dy.is_none() && w.is_none() && h.is_none() && angle.is_none() {
        return Err(EngineError::Params("give at least one of dx, dy, w, h, angle, or reset: true".into()));
    }
    super::layer::ensure_unlocked(s, [id])?;
    let sh = page_mut(s)?.shape_mut(id).ok_or_else(|| EngineError::Params(format!("`{}` is not a shape on this page", short(id))))?;
    if reset {
        sh.text_block = None;
    } else {
        let mut b = sh.text_block.unwrap_or(TextBlock { dx: 0.0, dy: 0.0, w: sh.w, h: sh.h, angle_deg: 0.0 });
        b.dx = dx.unwrap_or(b.dx);
        b.dy = dy.unwrap_or(b.dy);
        b.w = w.unwrap_or(b.w).max(super::MIN_SIZE);
        b.h = h.unwrap_or(b.h).max(super::MIN_SIZE);
        b.angle_deg = angle.unwrap_or(b.angle_deg);
        sh.text_block = Some(b);
    }
    sh.sanitize();
    let out = json!({"id": sh.id, "textBlock": sh.text_block});
    with_sel(s, out)
}

/// The optional `at` param: a character index, clamped later to each text's length.
fn at_param(v: &Value) -> Result<Option<usize>, EngineError> {
    Ok(num(v, "at")?.map(|a| a.clamp(0.0, vizcraft_diagram::MAX_TEXT as f64) as usize))
}

/// `text` with `insert` put at character `at` (the end when `None` or past the end).
fn insert_at(text: &str, insert: &str, at: Option<usize>) -> String {
    let byte = at.and_then(|a| text.char_indices().nth(a).map(|(i, _)| i)).unwrap_or(text.len());
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(text.get(..byte).unwrap_or(text));
    out.push_str(insert);
    out.push_str(text.get(byte..).unwrap_or(""));
    out
}

/// Insert `piece` into the text of every target.
fn insert_text(s: &mut Session, v: &Value, piece: &str) -> Result<Vec<String>, EngineError> {
    let at = at_param(v)?;
    if let Some(cid) = band::param(v)? {
        let ids = targets(s, v, Kinds::Shapes)?;
        return band::each(s, &ids, &cid, |c, _| {
            let text = insert_at(&c.body_text(), piece, at);
            c.set_body_text(&text);
        });
    }
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
    for id in &ids {
        if let Some(t) = text_of(s, id) {
            put_text(s, id, &insert_at(&t, piece, at))?;
        }
    }
    Ok(ids)
}

fn insert_symbol(s: &mut Session, v: &Value) -> CmdResult {
    let Some(symbol) = opt_str(v, "symbol")? else {
        s.ui_requests.push(json!({"open": "symbol"}));
        return Ok(json!({"dialog": "symbol"}));
    };
    if symbol.is_empty() || symbol.len() > MAX_SYMBOL {
        return Err(bad("symbol", &format!("1 to {MAX_SYMBOL} bytes of text")));
    }
    let symbol = symbol.to_string();
    let ids = insert_text(s, v, &symbol)?;
    with_sel(s, json!({"ids": ids}))
}

fn insert_field(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "field")?;
    let key = opt_str(v, "key")?;
    let field = Field::parse(name, key).ok_or_else(|| {
        let names: Vec<&str> = FIELDS.iter().map(|(n, _)| *n).collect();
        if name == "data" {
            bad("key", "a non-empty name without braces for the data field")
        } else {
            bad("field", &format!("one of {}", names.join(", ")))
        }
    })?;
    let placeholder = field.placeholder();
    let ids = insert_text(s, v, &placeholder)?;
    with_sel(s, json!({"ids": ids, "placeholder": placeholder}))
}

/// A text box shape: a rectangle with no fill and no line, default text style.
pub fn text_box(id: String, x: f64, y: f64, w: f64, h: f64) -> Shape {
    let mut sh = Shape::new(id, ShapeKind::Rectangle, x, y, w, h);
    sh.stencil = TEXT_BOX_STENCIL.to_string();
    sh.style.fill = Color::TRANSPARENT;
    sh.style.line = Color::TRANSPARENT;
    sh.sanitize();
    sh
}

fn add_text_box(s: &mut Session, v: &Value) -> CmdResult {
    let x = super::shape::coord(req_num(v, "x")?);
    let y = super::shape::coord(req_num(v, "y")?);
    let w = num(v, "w")?.map_or(TEXT_BOX_W, super::shape::size);
    let h = num(v, "h")?.map_or(TEXT_BOX_H, super::shape::size);
    let text = opt_str(v, "text")?.map(str::to_string);
    let id = super::new_id(s, v, "s")?;
    let mut sh = text_box(id.clone(), x, y, w, h);
    if let Some(t) = text {
        sh.text = t;
        sh.sanitize();
    }
    page_mut(s)?.shapes.push(sh);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Document, End, Page, Shape, ShapeKind};

    use super::*;

    fn session() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
        a.text = "hello World".into();
        page.shapes.push(a);
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 60.0));
        let mut c = vizcraft_diagram::Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.text = "link".into();
        page.connectors.push(c);
        doc.pages.push(Page::new("p2", "Second"));
        doc.sanitize();
        Session::new(doc)
    }

    fn shape<'a>(s: &'a Session, id: &str) -> &'a Shape {
        s.doc.page().unwrap().shape(id).unwrap()
    }

    #[test]
    fn set_format_sets_every_character_key_on_shapes_and_connectors() {
        let mut s = session();
        s.run(
            "text.setFormat",
            &json!({"ids": ["a", "c"], "fontFamily": " Georgia ", "size": 18, "bold": true, "italic": true, "underline": true, "strikethrough": true, "color": "#ff0000", "highlight": "#ffff00"}),
        )
        .unwrap();
        let st = &shape(&s, "a").style;
        assert_eq!(
            (st.font_family.as_str(), st.font_size, st.bold, st.italic, st.underline, st.strikethrough),
            ("Georgia", 18.0, true, true, true, true)
        );
        assert_eq!((st.font_color, st.highlight), (Color::rgb(255, 0, 0), Color::rgb(255, 255, 0)));
        assert!(s.doc.page().unwrap().connector("c").unwrap().style.underline);
        s.run("text.setFormat", &json!({"ids": ["a"], "highlight": false, "size": 1e9})).unwrap();
        assert_eq!(shape(&s, "a").style.highlight.a, 0);
        assert_eq!(shape(&s, "a").style.font_size, Style::MAX_FONT_SIZE);
        assert!(s.run("text.setFormat", &json!({"ids": ["a"]})).is_err());
        assert!(s.run("text.setFormat", &json!({"ids": ["a"], "color": "red"})).is_err());
        assert!(s.run("text.setFormat", &json!({"ids": ["a"], "highlight": 3})).is_err());
    }

    #[test]
    fn grow_steps_through_standard_sizes() {
        let mut s = session();
        let r = s.run("text.grow", &json!({"ids": ["a"], "step": 1})).unwrap();
        assert_eq!(r["sizes"], json!([14.0]));
        s.run("text.grow", &json!({"ids": ["a"], "step": -1})).unwrap();
        s.run("text.grow", &json!({"ids": ["a"], "step": -1})).unwrap();
        assert_eq!(shape(&s, "a").style.font_size, 11.0);
        assert!(s.run("text.grow", &json!({"ids": ["a"], "step": 2})).is_err());
        assert_eq!(step_size(13.0, true), 14.0);
        assert_eq!(step_size(13.0, false), 12.0);
        assert_eq!(step_size(96.0, true), 120.0);
        assert_eq!(step_size(1.0, false), 1.0);
    }

    #[test]
    fn set_case_rewrites_text_and_resets_the_drawn_case() {
        let mut s = session();
        s.run("text.setParagraph", &json!({"ids": ["a"], "hAlign": "left"})).unwrap();
        s.doc.page_mut().unwrap().shape_mut("a").unwrap().style.text_case = TextCase::Upper;
        s.run("text.setCase", &json!({"ids": ["a", "c"], "case": "upper"})).unwrap();
        assert_eq!(shape(&s, "a").text, "HELLO WORLD");
        assert_eq!(shape(&s, "a").style.text_case, TextCase::Normal);
        assert_eq!(s.doc.page().unwrap().connector("c").unwrap().text, "LINK");
        for (case, want) in [("lower", "hello world"), ("title", "Hello World"), ("toggle", "hELLO wORLD"), ("sentence", "Hello world")] {
            s.run("text.setCase", &json!({"ids": ["a"], "case": case})).unwrap();
            assert_eq!(shape(&s, "a").text, want, "{case}");
        }
        assert!(s.run("text.setCase", &json!({"ids": ["a"], "case": "shout"})).is_err());
        // One undo step per command.
        s.undo();
        assert_eq!(shape(&s, "a").text, "hELLO wORLD");
    }

    #[test]
    fn set_paragraph_sets_layout_keys() {
        let mut s = session();
        s.run(
            "text.setParagraph",
            &json!({"ids": ["a"], "hAlign": "justify", "vAlign": "bottom", "bullets": "number", "indent": 12, "direction": "vertical", "margins": {"left": 2, "bottom": 9}}),
        )
        .unwrap();
        let st = &shape(&s, "a").style;
        assert_eq!(
            (st.h_align, st.v_align, st.bullets, st.indent, st.text_direction),
            (HAlign::Justify, VAlign::Bottom, Bullets::Number, 12.0, TextDirection::Vertical)
        );
        assert_eq!(st.text_margins, Margins { left: 2.0, top: 4.0, right: 4.0, bottom: 9.0 });
        s.run("text.setParagraph", &json!({"ids": ["a"], "margins": 0, "bullets": true, "indent": -5})).unwrap();
        let st = &shape(&s, "a").style;
        assert_eq!((st.text_margins.top, st.bullets, st.indent), (0.0, Bullets::Disc, 0.0));
        assert!(s.run("text.setParagraph", &json!({"ids": ["a"], "hAlign": "middle"})).is_err());
        assert!(s.run("text.setParagraph", &json!({"ids": ["a"], "margins": {}})).is_err());
        assert!(s.run("text.setParagraph", &json!({"ids": ["a"]})).is_err());
    }

    #[test]
    fn text_block_round_trips_and_resets() {
        let mut s = session();
        let r = s.run("text.setBlock", &json!({"id": "a", "dx": 10, "dy": -5, "w": 40, "angle": 450})).unwrap();
        assert_eq!(r["textBlock"], json!({"dx": 10.0, "dy": -5.0, "w": 40.0, "h": 60.0, "angleDeg": 90.0}));
        let (rect, angle) = shape(&s, "a").text_rect();
        assert_eq!((rect.x0, rect.y0, rect.width(), rect.height(), angle), (40.0, -5.0, 40.0, 60.0, 90.0));
        // Through the native file and back.
        let bytes = vizcraft_formats::native::save(&s.doc).unwrap();
        let back = vizcraft_formats::native::load(&bytes).unwrap();
        assert_eq!(back.page().unwrap().shape("a").unwrap().text_block, shape(&s, "a").text_block);
        s.run("text.setBlock", &json!({"id": "a", "w": -3})).unwrap();
        assert_eq!(shape(&s, "a").text_block.unwrap().w, 1.0);
        s.run("text.setBlock", &json!({"id": "a", "reset": true})).unwrap();
        assert_eq!(shape(&s, "a").text_block, None);
        assert!(s.run("text.setBlock", &json!({"id": "a"})).is_err());
        assert!(s.run("text.setBlock", &json!({"id": "c", "dx": 1})).is_err());
    }

    #[test]
    fn insert_symbol_appends_or_inserts_and_opens_the_dialog_without_one() {
        let mut s = session();
        s.run("text.insertSymbol", &json!({"ids": ["a", "b"], "symbol": "\u{3a9}"})).unwrap();
        assert_eq!(shape(&s, "a").text, "hello World\u{3a9}");
        assert_eq!(shape(&s, "b").text, "\u{3a9}");
        s.run("text.insertSymbol", &json!({"ids": ["a"], "symbol": "\u{2192}", "at": 5})).unwrap();
        assert_eq!(shape(&s, "a").text, "hello\u{2192} World\u{3a9}");
        s.run("text.insertSymbol", &json!({"ids": ["a"], "symbol": "!", "at": 1e9})).unwrap();
        assert!(shape(&s, "a").text.ends_with('!'));
        assert_eq!(s.run("text.insertSymbol", &json!({})).unwrap(), json!({"dialog": "symbol"}));
        assert_eq!(s.ui_requests.pop(), Some(json!({"open": "symbol"})));
        assert!(s.run("text.insertSymbol", &json!({"ids": ["a"], "symbol": ""})).is_err());
        assert!(s.run("text.insertSymbol", &json!({"ids": ["a"], "symbol": "x".repeat(65)})).is_err());
    }

    #[test]
    fn insert_field_puts_a_placeholder_that_expands() {
        let mut s = session();
        let r = s.run("text.insertField", &json!({"ids": ["b"], "field": "pageName"})).unwrap();
        assert_eq!(r["placeholder"], "{{pageName}}");
        s.run("text.insertField", &json!({"ids": ["b"], "field": "data", "key": "cost"})).unwrap();
        assert_eq!(shape(&s, "b").text, "{{pageName}}{{data:cost}}");
        let ctx = vizcraft_diagram::text_fields::FieldCtx::new(&s.doc, 0, None);
        let b = shape(&s, "b");
        assert_eq!(vizcraft_diagram::text_fields::expand(&b.text, b, &ctx), "Page-1");
        assert!(s.run("text.insertField", &json!({"ids": ["b"], "field": "data"})).is_err());
        assert!(s.run("text.insertField", &json!({"ids": ["b"], "field": "weather"})).is_err());
    }

    #[test]
    fn add_text_box_makes_a_text_only_shape() {
        let mut s = session();
        let r = s.run("shape.addTextBox", &json!({"x": 10, "y": 20, "text": "Note", "id": "t1"})).unwrap();
        assert_eq!(r["id"], "t1");
        let t = shape(&s, "t1");
        assert_eq!((t.x, t.y, t.w, t.h, t.text.as_str(), t.stencil.as_str()), (10.0, 20.0, TEXT_BOX_W, TEXT_BOX_H, "Note", TEXT_BOX_STENCIL));
        assert_eq!((t.style.fill.a, t.style.line.a), (0, 0));
        assert_eq!(s.sel.items, vec!["t1".to_string()]);
        s.run("shape.addTextBox", &json!({"x": 0, "y": 0, "w": 300, "h": 0})).unwrap();
        assert!(s.run("shape.addTextBox", &json!({"x": 0})).is_err());
        assert!(s.run("shape.addTextBox", &json!({"x": 0, "y": 0, "id": "t1"})).is_err());
    }

    #[test]
    fn locked_layers_refuse_text_changes() {
        let mut s = session();
        s.run("layer.add", &json!({"name": "L"})).unwrap();
        let layer = s.doc.page().unwrap().layers[0].id.clone();
        s.run("layer.assign", &json!({"ids": ["a"], "layer": layer})).unwrap();
        s.run("layer.set", &json!({"layer": layer, "locked": true})).unwrap();
        assert!(s.run("text.setFormat", &json!({"ids": ["a"], "bold": true})).is_err());
        assert!(s.run("text.setCase", &json!({"ids": ["a"], "case": "upper"})).is_err());
        assert!(s.run("text.setBlock", &json!({"id": "a", "dx": 3})).is_err());
    }

    #[test]
    fn text_commands_act_on_a_compartment_when_given_one() {
        let mut s = session();
        s.run("compartment.add", &json!({"id": "a", "kind": "list", "compartmentId": "attrs", "rows": ["id: int", "total"]})).unwrap();
        s.run("text.setFormat", &json!({"ids": ["a"], "compartment": "attrs", "size": 9, "bold": true, "color": "#112233"})).unwrap();
        let c = shape(&s, "a").compartments[0].clone();
        assert_eq!((c.font_size, c.bold, c.font_color), (Some(9.0), Some(true), Some(Color::rgb(0x11, 0x22, 0x33))));
        assert_eq!(shape(&s, "a").style.font_size, 12.0, "the main text keeps its size");
        assert!(s.run("text.setFormat", &json!({"ids": ["a"], "compartment": "attrs", "italic": true})).is_err());
        let r = s.run("text.grow", &json!({"ids": ["a"], "compartment": "attrs", "step": 1})).unwrap();
        assert_eq!(r["sizes"], json!([10.0]));
        s.run("text.setCase", &json!({"ids": ["a"], "compartment": "attrs", "case": "upper"})).unwrap();
        assert_eq!(shape(&s, "a").compartments[0].rows, vec!["ID: INT".to_string(), "TOTAL".to_string()]);
        assert_eq!(shape(&s, "a").text, "hello World", "the main text is untouched");
        s.run("text.setParagraph", &json!({"ids": ["a"], "compartment": "attrs", "hAlign": "right"})).unwrap();
        assert_eq!(shape(&s, "a").compartments[0].align, HAlign::Right);
        assert!(s.run("text.setParagraph", &json!({"ids": ["a"], "compartment": "attrs", "vAlign": "top"})).is_err());
        s.run("text.insertSymbol", &json!({"ids": ["a"], "compartment": "attrs", "symbol": "\u{3a9}", "at": 0})).unwrap();
        assert_eq!(shape(&s, "a").compartments[0].rows[0], "\u{3a9}ID: INT");
        s.run("text.insertField", &json!({"ids": ["a"], "compartment": "attrs", "field": "pageName"})).unwrap();
        assert!(shape(&s, "a").compartments[0].rows[1].ends_with("{{pageName}}"));
        assert!(s.run("text.setCase", &json!({"ids": ["a"], "compartment": "nope", "case": "lower"})).is_err());
        assert!(s.run("text.setCase", &json!({"ids": ["c"], "compartment": "attrs", "case": "lower"})).is_err(), "connectors have none");
    }
}
