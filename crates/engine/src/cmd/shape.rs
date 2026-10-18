//! Shape commands: add, delete, move, resize, rotate, text, style, and read-only search, get and catalog.
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
            .params(r#"{"stencil": string, "x": n, "y": n, "w"?: n, "h"?: n, "text"?: string, "id"?: string} -> {"id", "element"?}; x, y is the top-left; a master of a notation type (Flowchart) also creates an element named text"#),
        CommandSpec::new("shape.delete", "Delete Shape", "Home > Editing", delete).params(r#"{"ids"?: [string]} (default: selection)"#).when(has_shapes),
        CommandSpec::new("shape.move", "Move", "Home > Arrange", move_shapes)
            .params(r#"{"ids"?: [string], "dx": n, "dy": n} or {"id": string, "x": n, "y": n}"#)
            .when(has_shapes),
        CommandSpec::new("shape.resize", "Resize", "Home > Arrange", resize).params(r#"{"id": string, "x"?: n, "y"?: n, "w": n, "h": n}"#).when(has_shapes),
        CommandSpec::new("shape.rotate", "Rotate", "Home > Arrange", rotate).params(r#"{"ids"?: [string], "angle": degrees}"#).when(has_shapes),
        CommandSpec::new("shape.setText", "Edit Text", "Home > Font", set_text).params(r#"{"id": string, "text": string}; on a shape of an element, renames the element and every view of it"#).when(has_shapes),
        CommandSpec::new("shape.setStyle", "Shape Style", "Home > Shape Styles", set_style)
            .params(
                r##"{"ids"?: [string], "fill"?: "#rrggbb", "line"?: "#rrggbb", "lineWidth"?: n, "fontSize"?: n, "fontColor"?: "#rrggbb", "bold"?: bool, "italic"?: bool, "dash"?: "solid"|"dashed"|"dotted"|"dashDot"|"longDash", "lineCap"?: "butt"|"round"|"square", "cornerRadius"?: n, "gradient"?: false|true|"linear"|"radial"|{"kind"?, "angle"?, "stops": [{"offset", "color"}]}, "fillTransparency"?: 0..1, "lineTransparency"?: 0..1, "shadow"?: false|true|{"dx"?, "dy"?, "blur"?, "color"?}, "softEdges"?: n, "glow"?: false|true|radius|{"radius"?, "color"?}}; numbers are clamped; connectors take the line keys only"##,
            )
            .when(has_shapes),
        CommandSpec::new("shape.search", "Search Shapes", "Shapes pane", search).params(
            r#"{"query": string, "limit"?: n (default 20, at most 200)} -> {"masters": [{"id", "label", "category", "keywords", "w", "h", "style"}]}; style is the draw.io style string"#,
        ),
        CommandSpec::new("shape.get", "Get Shape", "Shape", get).params(
            r#"{"id": string, "page"?: id|name|index (default: any page)} -> {"page", "shape": {"id", "master", "kind", "x", "y", "w", "h", "rotation", "text", "style", "data", "connectionPoints"}, "connections": [{"connector", "end", "from", "to"}]}"#,
        ),
        CommandSpec::new("shape.catalog", "Stencil Catalog", "Shapes pane", catalog).params(
            r#"{} -> {"sets": [{"id", "name", "categories", "masters": [{"id", "label", "category", "keywords", "w", "h", "style"}]}]}"#,
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
    let masters: Vec<Value> = vizcraft_stencils::search(&query).into_iter().take(limit).map(master_json).collect();
    Ok(json!({"masters": masters}))
}

/// A master as the search and catalog commands report it.
fn master_json(m: &vizcraft_stencils::Master) -> Value {
    json!({
        "id": m.id,
        "label": m.label,
        "category": m.category,
        "keywords": m.keywords,
        "w": m.w,
        "h": m.h,
        "style": vizcraft_formats::drawio::master_style(m),
    })
}

/// A coordinate clamped to the model's range.
pub(super) fn coord(v: f64) -> f64 {
    v.clamp(-MAX_COORD, MAX_COORD)
}

/// A width or height clamped to [`MIN_SIZE`, `MAX_EXTENT`].
pub(super) fn size(v: f64) -> f64 {
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
    // A master whose glyph names an element type (every Flowchart master) draws a new element of
    // that type; Basic masters stay free drawing.
    let element = match super::model::cat().type_for_master(stencil) {
        Some(type_id) => {
            let ne = super::model::NewElement { type_id: type_id.to_string(), name: shape.text.clone(), ..Default::default() };
            let (eid, _) = super::model::create_element(&mut s.doc, ne)?;
            shape.element = Some(eid.clone());
            Some(eid)
        }
        None => None,
    };
    page_mut(s)?.shapes.push(shape);
    select(s, vec![id.clone()]);
    match element {
        Some(e) => with_sel(s, json!({"id": id, "element": e})),
        None => with_sel(s, json!({"id": id})),
    }
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
        super::layer::ensure_unlocked(s, [id])?;
        let page = page_mut(s)?;
        let sh = page.shape(id).ok_or_else(|| not_a_shape(id))?;
        let (dx, dy) = (x - sh.x, y - sh.y);
        let id = id.to_string();
        // A group carries its members.
        super::arrange::move_shapes(page, std::slice::from_ref(&id), dx, dy);
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
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    // Shapes move through the arrange code so a group carries its members.
    let shape_ids: Vec<String> = ids.iter().filter(|id| page.shape(id).is_some()).cloned().collect();
    super::arrange::move_shapes(page, &shape_ids, dx, dy);
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
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
    super::layer::ensure_unlocked(s, [id])?;
    let page = page_mut(s)?;
    let sh = page.shape(id).ok_or_else(|| not_a_shape(id))?;
    let (x, y) = (x.unwrap_or(sh.x), y.unwrap_or(sh.y));
    // A group scales its members.
    vizcraft_diagram::arrange::set_frame(page, id, x, y, w, h);
    let sh = page.shape(id).ok_or_else(|| not_a_shape(id))?;
    let frame = json!({"id": sh.id, "x": sh.x, "y": sh.y, "w": sh.w, "h": sh.h});
    with_sel(s, frame)
}

fn rotate(s: &mut Session, v: &Value) -> CmdResult {
    let angle = req_num(v, "angle")?;
    let ids = targets(s, v, Kinds::Shapes)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    let mut out = 0.0;
    for id in &ids {
        // A group turns its members with it.
        vizcraft_diagram::arrange::set_angle(page, id, angle.rem_euclid(360.0));
        if let Some(sh) = page.shape(id) {
            out = sh.angle_deg;
        }
    }
    with_sel(s, json!({"rotated": ids, "angle": out}))
}

fn set_text(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let text = p::req_str(v, "text")?;
    super::layer::ensure_unlocked(s, [id])?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let element = page.shape(id).ok_or_else(|| not_a_shape(id))?.element.clone().filter(|e| s.doc.model.element(e).is_some());
    // A shape that draws an element shows the element's name: renaming the element updates
    // this shape and every other view of it.
    match element {
        Some(e) => super::model::rename_element(&mut s.doc, &e, text)?,
        None => {
            let sh = page_mut(s)?.shape_mut(id).ok_or_else(|| not_a_shape(id))?;
            sh.text = text.to_string();
            sh.sanitize();
        }
    }
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
    /// Line and fill effects (dash, gradient, shadow and the rest).
    effects: super::effects::EffectPatch,
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
            effects: super::effects::EffectPatch::parse(v)?,
        };
        let empty = patch.fill.is_none()
            && patch.line.is_none()
            && patch.line_width.is_none()
            && patch.font_size.is_none()
            && patch.font_color.is_none()
            && patch.bold.is_none()
            && patch.italic.is_none()
            && patch.effects.is_empty();
        if empty {
            return Err(EngineError::Params(format!(
                "give at least one of fill, line, lineWidth, fontSize, fontColor, bold, italic, {}",
                super::effects::KEYS
            )));
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
        // After the fill, so a gradient made from the fill uses the new colour.
        self.effects.apply(st, with_fill);
        st.sanitize();
    }
}

/// Style shapes and connectors: `ids` may name either (connectors take everything but `fill`).
fn set_style(s: &mut Session, v: &Value) -> CmdResult {
    let patch = StylePatch::parse(v)?;
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
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

/// One shape with its page and the connectors glued to it. It only reads, so the document never
/// changes. A missing shape is `no shape `<id>``, the text the MCP resources look for.
fn get(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let pages: Vec<&vizcraft_diagram::Page> = match v.get("page") {
        None | Some(Value::Null) => s.doc.pages.iter().collect(),
        Some(r) => {
            let r = match r {
                Value::String(t) => t.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
            };
            let index = s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))?;
            s.doc.pages.get(index).into_iter().collect()
        }
    };
    let (page, sh) =
        pages.into_iter().find_map(|pg| pg.shape(id).map(|sh| (pg, sh))).ok_or_else(|| EngineError::Params(format!("no shape `{}`", short(id))))?;
    let connections: Vec<Value> = page
        .connectors
        .iter()
        .filter_map(|c| {
            let (from, to) = (c.start.shape(), c.end.shape());
            let end = match (from == Some(id), to == Some(id)) {
                (true, true) => "both",
                (true, false) => "start",
                (false, true) => "end",
                (false, false) => return None,
            };
            Some(json!({"connector": c.id, "end": end, "from": from, "to": to}))
        })
        .collect();
    let shape = json!({
        "id": sh.id,
        "master": sh.stencil,
        "kind": sh.kind,
        "x": sh.x,
        "y": sh.y,
        "w": sh.w,
        "h": sh.h,
        "rotation": sh.angle_deg,
        "text": sh.text,
        "style": sh.style,
        "data": sh.ext,
        "connectionPoints": sh.connection_points(),
    });
    Ok(json!({"page": page.id, "shape": shape, "connections": connections}))
}

/// Every stencil set with its categories and masters. It reads the library only.
fn catalog(_s: &mut Session, _v: &Value) -> CmdResult {
    let sets: Vec<Value> = vizcraft_stencils::sets()
        .iter()
        .map(|set| json!({"id": set.id, "name": set.name, "categories": set.categories, "masters": set.masters.iter().map(master_json).collect::<Vec<_>>()}))
        .collect();
    Ok(json!({"sets": sets}))
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

    #[test]
    fn get_reports_a_shape_and_its_connections_and_changes_nothing() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let page = s.doc.page().unwrap().id.clone();
        let decision = s.doc.page().unwrap().shapes.iter().find(|sh| sh.stencil == "flowchart.decision").unwrap().id.clone();
        let r = s.run("shape.get", &json!({"id": decision})).unwrap();
        assert_eq!(r["page"], page.as_str());
        let sh = &r["shape"];
        assert_eq!(sh["id"], decision.as_str());
        assert_eq!(sh["master"], "flowchart.decision");
        assert_eq!(sh["text"], "Valid?");
        assert!(sh["kind"].is_string() && sh["style"]["fill"].is_string() && sh["data"].is_object(), "{sh}");
        assert_eq!(sh["connectionPoints"].as_array().unwrap().len(), 4);
        assert_eq!(sh["rotation"], 0.0);
        // In from the process above it, out to "Save diagram" and "Fix input".
        let conns = r["connections"].as_array().unwrap();
        assert_eq!(conns.len(), 3, "{r}");
        assert_eq!(conns.iter().filter(|c| c["end"] == "start").count(), 2);
        assert!(conns.iter().any(|c| c["end"] == "end" && c["to"] == decision.as_str()));
        // The page param narrows the search and accepts an index.
        assert_eq!(s.run("shape.get", &json!({"id": decision, "page": 0})).unwrap()["page"], page.as_str());
        assert_eq!(s.doc, before);
        assert!(!s.can_undo() && !s.dirty);
    }

    #[test]
    fn get_rejects_missing_shapes_pages_and_junk() {
        let mut s = Session::new(crate::sample::sample_document());
        let id = s.doc.page().unwrap().shapes[0].id.clone();
        let err = s.run("shape.get", &json!({"id": "nope"})).unwrap_err().to_string();
        assert!(err.contains("no shape `nope`"), "{err}");
        let err = s.run("shape.get", &json!({"id": id, "page": "Elsewhere"})).unwrap_err().to_string();
        assert!(err.contains("no page `Elsewhere`"), "{err}");
        // A connector id is not a shape.
        let conn = s.doc.page().unwrap().connectors[0].id.clone();
        assert!(s.run("shape.get", &json!({"id": conn})).is_err());
        for bad in [json!({}), json!({"id": 3}), json!({"id": id, "page": true}), json!({"id": "x".repeat(1_000_000)})] {
            assert!(s.run("shape.get", &bad).is_err(), "{bad}");
        }
        let err = s.run("shape.get", &json!({"id": "\u{4e2d}".repeat(1000)})).unwrap_err().to_string();
        assert!(err.len() < 400, "the error quotes a short id");
    }

    #[test]
    fn catalog_lists_every_set_and_master() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let r = s.run("shape.catalog", &json!({"ignored": [1, 2]})).unwrap();
        let sets = r["sets"].as_array().unwrap();
        assert_eq!(sets.len(), vizcraft_stencils::sets().len());
        let total: usize = sets.iter().map(|set| set["masters"].as_array().unwrap().len()).sum();
        assert_eq!(total, vizcraft_stencils::search("").len());
        let flow = sets.iter().find(|set| set["id"] == "flowchart").unwrap();
        assert_eq!(flow["categories"], json!(["Steps", "Data", "Links"]));
        let decision = flow["masters"].as_array().unwrap().iter().find(|m| m["id"] == "flowchart.decision").unwrap();
        assert!(decision["style"].as_str().unwrap().contains("vizcraft=flowchart.decision"));
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }
}
