//! Shape commands: add, delete, move, resize, rotate, text, style, and a stencil search.
//!
//! Positions are world units (pixels at 100% zoom), y down. A shape's `x, y` is the top-left of
//! its unrotated frame; rotation turns the frame about its centre. Connectors need no bookkeeping
//! here: routing is derived from shape geometry every time it is drawn, so a glued end follows a
//! moved, resized or rotated shape on its own.

use serde_json::{Value, json};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{Color, End, MAX_EXTENT, Style};

use super::{Kinds, MAX_DELTA, MIN_SIZE, delete_items, has_shapes, num, opt_bool, opt_str, page_mut, req_num, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all shape commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("shape.add", "Add Shape", "Insert > Shapes", add)
            .params(r#"{"stencil": string, "x": n, "y": n, "w"?: n, "h"?: n, "text"?: string, "id"?: string} -> {"id"}; x, y is the top-left"#),
        CommandSpec::new("shape.delete", "Delete Shape", "Home > Editing", delete).params(r#"{"ids"?: [string]} (default: selection)"#).when(has_shapes),
        CommandSpec::new("shape.move", "Move", "Home > Arrange", move_shapes)
            .params(r#"{"ids"?: [string], "dx": n, "dy": n} or {"id": string, "x": n, "y": n}"#)
            .when(has_shapes),
        CommandSpec::new("shape.resize", "Resize", "Home > Arrange", resize).params(r#"{"id": string, "x"?: n, "y"?: n, "w": n, "h": n}"#).when(has_shapes),
        CommandSpec::new("shape.rotate", "Rotate", "Home > Arrange", rotate).params(r#"{"ids"?: [string], "angle": degrees}"#).when(has_shapes),
        CommandSpec::new("shape.setText", "Edit Text", "Home > Font", set_text).params(r#"{"id": string, "text": string}"#).when(has_shapes),
        CommandSpec::new("shape.setStyle", "Shape Style", "Home > Shape Styles", set_style)
            .params(
                r##"{"ids"?: [string], "fill"?: "#rrggbb", "line"?: "#rrggbb", "lineWidth"?: n, "fontSize"?: n, "fontColor"?: "#rrggbb", "bold"?: bool, "italic"?: bool}"##,
            )
            .when(has_shapes),
        CommandSpec::new("shape.search", "Search Shapes", "Shapes pane", search).params(
            r#"{"query": string, "limit"?: n (default 20, at most 200)} -> {"masters": [{"id", "label", "category", "keywords", "w", "h", "style"}]}; style is the draw.io style string"#,
        ),
    ]
}

/// Results returned when `limit` is not given.
const DEFAULT_SEARCH_LIMIT: usize = 20;
/// Most results one search returns, whatever `limit` asks for.
const MAX_SEARCH_LIMIT: usize = 200;
/// Longest query looked at; real queries are a few words, and the rest is ignored.
const MAX_QUERY_CHARS: usize = 1000;

/// Stencil masters matching every word of `query`, with the draw.io style that opens as each.
/// It reads the library only, so the document never changes.
fn search(_s: &mut Session, v: &Value) -> CmdResult {
    let query: String = p::req_str(v, "query")?.chars().take(MAX_QUERY_CHARS).collect();
    let limit = match num(v, "limit")? {
        None => DEFAULT_SEARCH_LIMIT,
        // Clamped as a float first so a huge or negative number cannot overflow the cast.
        Some(n) => n.clamp(1.0, MAX_SEARCH_LIMIT as f64) as usize,
    };
    let masters: Vec<Value> = vizcraft_stencils::search(&query)
        .into_iter()
        .take(limit)
        .map(|m| {
            json!({
                "id": m.id,
                "label": m.label,
                "category": m.category,
                "keywords": m.keywords,
                "w": m.w,
                "h": m.h,
                "style": vizcraft_formats::drawio::master_style(m),
            })
        })
        .collect();
    Ok(json!({"masters": masters}))
}

fn coord(v: f64) -> f64 {
    v.clamp(-MAX_COORD, MAX_COORD)
}

fn size(v: f64) -> f64 {
    v.clamp(MIN_SIZE, MAX_EXTENT)
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let stencil = p::req_str(v, "stencil")?;
    if vizcraft_stencils::master(stencil).is_none() {
        return Err(EngineError::Params(format!("unknown stencil `{}`", short(stencil))));
    }
    let x = coord(req_num(v, "x")?);
    let y = coord(req_num(v, "y")?);
    let w = num(v, "w")?.map(size);
    let h = num(v, "h")?.map(size);
    let text = opt_str(v, "text")?;
    let id = super::new_id(s, v, "s")?;
    let mut shape = vizcraft_stencils::new_shape(stencil, id.clone(), x, y, w, h)
        .ok_or_else(|| EngineError::Params(format!("unknown stencil `{}`", short(stencil))))?;
    if let Some(t) = text {
        shape.text = t.to_string();
    }
    shape.sanitize();
    page_mut(s)?.shapes.push(shape);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Shapes)?;
    delete_items(s, &ids)
}

/// `{id, x, y}` puts one shape's top-left at `x, y`; otherwise `{ids?, dx, dy}` moves the shapes
/// by that much, and the free ends of any connectors among `ids` with them.
fn move_shapes(s: &mut Session, v: &Value) -> CmdResult {
    if v.get("id").is_some() {
        let id = p::req_str(v, "id")?;
        let x = coord(req_num(v, "x")?);
        let y = coord(req_num(v, "y")?);
        let sh = page_mut(s)?.shape_mut(id).ok_or_else(|| not_a_shape(id))?;
        sh.x = x;
        sh.y = y;
        sh.sanitize();
        let id = id.to_string();
        return with_sel(s, json!({"moved": [id]}));
    }
    let dx = num(v, "dx")?;
    let dy = num(v, "dy")?;
    if dx.is_none() && dy.is_none() {
        return Err(EngineError::Params("give `dx` and `dy`, or `id`, `x` and `y`".into()));
    }
    let dx = dx.unwrap_or(0.0).clamp(-MAX_DELTA, MAX_DELTA);
    let dy = dy.unwrap_or(0.0).clamp(-MAX_DELTA, MAX_DELTA);
    let ids = targets(s, v, Kinds::Both)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            sh.x = coord(sh.x + dx);
            sh.y = coord(sh.y + dy);
            sh.sanitize();
        } else if let Some(c) = page.connector_mut(id) {
            for e in [&mut c.start, &mut c.end] {
                if let End::Free { x, y } = e {
                    *x = coord(*x + dx);
                    *y = coord(*y + dy);
                }
            }
            c.sanitize();
        }
    }
    with_sel(s, json!({"moved": ids}))
}

fn resize(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let w = size(req_num(v, "w")?);
    let h = size(req_num(v, "h")?);
    let x = num(v, "x")?.map(coord);
    let y = num(v, "y")?.map(coord);
    let sh = page_mut(s)?.shape_mut(id).ok_or_else(|| not_a_shape(id))?;
    sh.w = w;
    sh.h = h;
    if let Some(x) = x {
        sh.x = x;
    }
    if let Some(y) = y {
        sh.y = y;
    }
    sh.sanitize();
    let frame = json!({"id": sh.id, "x": sh.x, "y": sh.y, "w": sh.w, "h": sh.h});
    with_sel(s, frame)
}

fn rotate(s: &mut Session, v: &Value) -> CmdResult {
    let angle = req_num(v, "angle")?;
    let ids = targets(s, v, Kinds::Shapes)?;
    let page = page_mut(s)?;
    let mut out = 0.0;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            sh.angle_deg = angle;
            sh.sanitize();
            out = sh.angle_deg;
        }
    }
    with_sel(s, json!({"rotated": ids, "angle": out}))
}

fn set_text(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let text = p::req_str(v, "text")?;
    let sh = page_mut(s)?.shape_mut(id).ok_or_else(|| not_a_shape(id))?;
    sh.text = text.to_string();
    sh.sanitize();
    let id = id.to_string();
    with_sel(s, json!({"id": id}))
}

/// Style fields given to `shape.setStyle`, parsed and checked before anything changes.
#[derive(Default)]
struct StylePatch {
    fill: Option<Color>,
    line: Option<Color>,
    line_width: Option<f64>,
    font_size: Option<f64>,
    font_color: Option<Color>,
    bold: Option<bool>,
    italic: Option<bool>,
}

impl StylePatch {
    fn parse(v: &Value) -> Result<Self, EngineError> {
        let color = |k: &str| -> Result<Option<Color>, EngineError> {
            match opt_str(v, k)? {
                None => Ok(None),
                Some(t) => Color::parse(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not a #rrggbb colour", short(t)))),
            }
        };
        let patch = StylePatch {
            fill: color("fill")?,
            line: color("line")?,
            line_width: num(v, "lineWidth")?,
            font_size: num(v, "fontSize")?,
            font_color: color("fontColor")?,
            bold: opt_bool(v, "bold")?,
            italic: opt_bool(v, "italic")?,
        };
        let empty = patch.fill.is_none()
            && patch.line.is_none()
            && patch.line_width.is_none()
            && patch.font_size.is_none()
            && patch.font_color.is_none()
            && patch.bold.is_none()
            && patch.italic.is_none();
        if empty {
            return Err(EngineError::Params("give at least one of fill, line, lineWidth, fontSize, fontColor, bold, italic".into()));
        }
        Ok(patch)
    }

    /// Apply to a style; `fill` only when `with_fill` (a connector's fill is unused).
    fn apply(&self, st: &mut Style, with_fill: bool) {
        if with_fill && let Some(c) = self.fill {
            st.fill = c;
        }
        if let Some(c) = self.line {
            st.line = c;
        }
        if let Some(w) = self.line_width {
            st.line_width = w;
        }
        if let Some(f) = self.font_size {
            st.font_size = f;
        }
        if let Some(c) = self.font_color {
            st.font_color = c;
        }
        if let Some(b) = self.bold {
            st.bold = b;
        }
        if let Some(i) = self.italic {
            st.italic = i;
        }
        st.sanitize();
    }
}

/// Style shapes and connectors: `ids` may name either (connectors take everything but `fill`).
fn set_style(s: &mut Session, v: &Value) -> CmdResult {
    let patch = StylePatch::parse(v)?;
    let ids = targets(s, v, Kinds::Both)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(sh) = page.shape_mut(id) {
            patch.apply(&mut sh.style, true);
            sh.sanitize();
        } else if let Some(c) = page.connector_mut(id) {
            patch.apply(&mut c.style, false);
            c.sanitize();
        }
    }
    with_sel(s, json!({"styled": ids}))
}

fn not_a_shape(id: &str) -> EngineError {
    EngineError::Params(format!("`{}` is not a shape on this page", short(id)))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Session;

    fn masters(r: &Value) -> Vec<Value> {
        r["masters"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn search_finds_masters_with_styles_and_leaves_the_document_alone() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let r = s.run("shape.search", &json!({"query": "rectangle"})).unwrap();
        let found = masters(&r);
        let rect = found.iter().find(|m| m["id"] == "basic.rectangle").unwrap_or_else(|| panic!("{r}"));
        assert_eq!(rect["category"], "Shapes");
        assert!(rect["w"].as_f64().unwrap() > 0.0 && rect["h"].as_f64().unwrap() > 0.0);
        assert!(rect["keywords"].is_array());
        assert!(rect["style"].as_str().unwrap().contains("vizcraft=basic.rectangle"), "{rect}");
        assert!(masters(&s.run("shape.search", &json!({"query": "no such shape anywhere"})).unwrap()).is_empty());
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }

    #[test]
    fn search_limit_is_clamped() {
        let mut s = Session::new(crate::sample::sample_document());
        let all = masters(&s.run("shape.search", &json!({"query": "", "limit": 1e300})).unwrap()).len();
        assert!(all > 1 && all <= 200);
        assert_eq!(masters(&s.run("shape.search", &json!({"query": "", "limit": 1})).unwrap()).len(), 1);
        assert_eq!(masters(&s.run("shape.search", &json!({"query": "", "limit": -5})).unwrap()).len(), 1);
        assert_eq!(masters(&s.run("shape.search", &json!({"query": ""})).unwrap()).len(), all.min(20));
        for bad in [json!({}), json!({"query": 3}), json!({"query": "x", "limit": "many"})] {
            assert!(s.run("shape.search", &bad).is_err(), "{bad}");
        }
        assert!(s.run("shape.search", &json!({"query": "a ".repeat(100_000)})).is_ok());
    }
}
