//! Ink (Review > Ink and the Pen, Highlighter and Eraser tools): add a stroke, erase strokes and
//! turn strokes into shapes. The geometry (simplifying, recognising) is in
//! [`vizcraft_diagram::ink`].
//!
//! A pen stroke goes on top of the page; a highlighter stroke goes to the back, so it marks the
//! shapes without covering them. Each command is one undo step, so a stroke drawn with a tool
//! undoes in one step.

use serde_json::{Value, json};
use vizcraft_diagram::ink::{self, Pen, Recognized};
use vizcraft_diagram::{Arrow, Color, Connector, End, Point, Route, Shape, ShapeKind};

use super::{delete_items, has_shapes, num, opt_bool, opt_str, page_mut, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Default pen colour.
pub const PEN_COLOR: Color = Color::rgb(0x1f, 0x2a, 0x44);
/// Default highlighter colour.
pub const HIGHLIGHTER_COLOR: Color = Color::rgb(0xff, 0xd8, 0x2e);

/// The ink commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ink.add", "Add Ink Stroke", "Review > Ink", add)
            .params(r##"{"points": [[x, y]] or [{"x", "y"}] (world coordinates, at most 50000), "width"?: n (pen, default 2), "color"?: "#rrggbb", "highlighter"?: bool, "id"?: string} -> {"id", "points"}; the points are simplified (1.5 units, at most 4096 kept)"##),
        CommandSpec::new("ink.erase", "Erase Ink", "Review > Ink", erase)
            .params(r#"{"ids": [ink stroke ids]} -> {"deleted": [ids]}; ids that are not strokes are refused"#)
            .when(has_shapes),
        CommandSpec::new("ink.toShapes", "Convert to Shapes", "Review > Ink", to_shapes)
            .params(r#"{"ids"?: [stroke ids] (default: the selected strokes, else every stroke on the page)} -> {"ids": [new shapes and connectors], "removed": [strokes], "kept": [strokes left as ink]}; a closed stroke near an ellipse becomes an Ellipse, one with four corners a Rectangle, a nearly straight one a Line connector"#)
            .when(has_shapes),
    ]
}

/// The `points` param as world points.
fn points(v: &Value) -> Result<Vec<Point>, EngineError> {
    let list = v.get("points").and_then(Value::as_array).ok_or_else(|| EngineError::Params("`points` (a list of [x, y]) is required".into()))?;
    if list.len() > ink::MAX_INPUT_POINTS {
        return Err(EngineError::Params(format!("a stroke takes at most {} points", ink::MAX_INPUT_POINTS)));
    }
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let (x, y) = match item {
            Value::Array(a) => (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)),
            Value::Object(_) => (item.get("x").and_then(Value::as_f64), item.get("y").and_then(Value::as_f64)),
            _ => (None, None),
        };
        match (x, y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => out.push(Point::new(super::shape::coord(x), super::shape::coord(y))),
            _ => return Err(EngineError::Params("every point is [x, y] or {x, y} with finite numbers".into())),
        }
    }
    if out.is_empty() {
        return Err(EngineError::Params("a stroke needs at least one point".into()));
    }
    Ok(out)
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let pts = ink::prepare(&points(v)?);
    let pen = if opt_bool(v, "highlighter")? == Some(true) { Pen::Highlighter } else { Pen::Pen };
    let width = num(v, "width")?.unwrap_or(ink::PEN_WIDTH).clamp(0.25, 100.0);
    let color = match opt_str(v, "color")? {
        Some(c) => Color::parse(c).ok_or_else(|| EngineError::Params(format!("`{}` is not a colour", short(c))))?,
        None if pen == Pen::Highlighter => HIGHLIGHTER_COLOR,
        None => PEN_COLOR,
    };
    let id = super::new_id(s, v, "s")?;
    let sh = ink::stroke_shape(id.clone(), &pts, pen, width, color).ok_or_else(|| EngineError::Params("a stroke needs at least one point".into()))?;
    let page = page_mut(s)?;
    match pen {
        Pen::Highlighter => page.shapes.insert(0, sh),
        Pen::Pen => page.shapes.push(sh),
    }
    with_sel(s, json!({"id": id, "points": pts.len()}))
}

/// Every id in `ids` must be an ink stroke on the active page.
fn strokes(s: &Session, ids: &[String]) -> Result<(), EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    match ids.iter().find(|id| !page.shape(id).is_some_and(ink::is_stroke)) {
        Some(bad) => Err(EngineError::Params(format!("`{}` is not an ink stroke", short(bad)))),
        None => Ok(()),
    }
}

fn erase(s: &mut Session, v: &Value) -> CmdResult {
    let ids = super::ids_param(v)?.ok_or_else(|| EngineError::Params("`ids` (stroke ids) is required".into()))?;
    if ids.is_empty() {
        return Err(EngineError::Params("`ids` names no stroke".into()));
    }
    strokes(s, &ids)?;
    super::layer::ensure_unlocked(s, &ids)?;
    super::protect::check(s, &ids, super::protect::Lock::Delete)?;
    delete_items(s, &ids)
}

fn to_shapes(s: &mut Session, v: &Value) -> CmdResult {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let ids: Vec<String> = match super::ids_param(v)? {
        Some(ids) => {
            strokes(s, &ids)?;
            ids
        }
        None => {
            let selected: Vec<String> = s.sel.items.iter().filter(|id| page.shape(id).is_some_and(ink::is_stroke)).cloned().collect();
            if selected.is_empty() { page.shapes.iter().filter(|x| ink::is_stroke(x)).map(|x| x.id.clone()).collect() } else { selected }
        }
    };
    if ids.is_empty() {
        return Err(EngineError::Params("there is no ink on this page".into()));
    }
    super::layer::ensure_unlocked(s, &ids)?;
    super::protect::check(s, &ids, super::protect::Lock::Delete)?;
    let mut made_shapes: Vec<(String, Shape)> = Vec::new();
    let mut made_lines: Vec<Connector> = Vec::new();
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for id in &ids {
        let Some(stroke) = s.doc.page().and_then(|p| p.shape(id)).cloned() else { continue };
        let pts = ink::stroke_points(&stroke);
        let line = Color { a: 255, ..stroke.style.line };
        let width = stroke.style.line_width;
        match ink::recognize(&pts) {
            Some(Recognized::Ellipse(r)) | Some(Recognized::Rectangle(r)) if r.width() >= 1.0 && r.height() >= 1.0 => {
                let kind = if matches!(ink::recognize(&pts), Some(Recognized::Ellipse(_))) { ShapeKind::Ellipse } else { ShapeKind::Rectangle };
                let mut sh = Shape::new(s.doc.next_id("s"), kind, r.x0, r.y0, r.width(), r.height());
                sh.style.fill = Color::TRANSPARENT;
                sh.style.line = line;
                sh.style.line_width = width;
                sh.layers = stroke.layers.clone();
                sh.sanitize();
                made_shapes.push((id.clone(), sh));
                removed.push(id.clone());
            }
            Some(Recognized::Line(a, b)) => {
                let mut c = Connector::new(s.doc.next_id("c"), End::Free { x: a.x, y: a.y }, End::Free { x: b.x, y: b.y });
                c.route = Route::Straight;
                c.end_arrow = Arrow::None;
                c.start_arrow = Arrow::None;
                c.style.line = line;
                c.style.line_width = width;
                c.sanitize();
                made_lines.push(c);
                removed.push(id.clone());
            }
            _ => kept.push(id.clone()),
        }
    }
    let mut new_ids: Vec<String> = Vec::new();
    let page = page_mut(s)?;
    // A recognised shape takes its stroke's place in the z order.
    for (stroke, sh) in made_shapes {
        new_ids.push(sh.id.clone());
        match page.shapes.iter().position(|x| x.id == stroke) {
            Some(i) => {
                if let Some(slot) = page.shapes.get_mut(i) {
                    *slot = sh;
                }
            }
            None => page.shapes.push(sh),
        }
    }
    let lines: Vec<String> = removed.iter().filter(|id| page.shape(id).is_some_and(ink::is_stroke)).cloned().collect();
    super::delete_on_page(page, &lines);
    for c in made_lines {
        new_ids.push(c.id.clone());
        page.connectors.push(c);
    }
    super::select(s, new_ids.clone());
    with_sel(s, json!({"ids": new_ids, "removed": removed, "kept": kept}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::ShapeKind;
    use vizcraft_diagram::ink::is_stroke;

    use crate::Session;

    fn square() -> Vec<[f64; 2]> {
        let mut v = Vec::new();
        for i in 0..=40 {
            v.push([100.0 + f64::from(i) * 2.5, 100.0]);
        }
        for i in 0..=40 {
            v.push([200.0, 100.0 + f64::from(i) * 2.5]);
        }
        for i in 0..=40 {
            v.push([200.0 - f64::from(i) * 2.5, 200.0]);
        }
        for i in 0..=40 {
            v.push([100.0, 200.0 - f64::from(i) * 2.5]);
        }
        v
    }

    fn circle() -> Vec<[f64; 2]> {
        (0..=90)
            .map(|i| {
                let a = f64::from(i) / 90.0 * std::f64::consts::TAU;
                [400.0 + 60.0 * a.cos(), 150.0 + 45.0 * a.sin()]
            })
            .collect()
    }

    #[test]
    fn a_stroke_commits_as_one_shape_and_undoes_in_one_step() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let before = s.doc.page().unwrap().shapes.len();
        let pts: Vec<[f64; 2]> = (0..200).map(|i| [f64::from(i), (f64::from(i) / 10.0).sin() * 20.0]).collect();
        let r = s.run("ink.add", &json!({"points": pts, "width": 3, "color": "#ff0000"})).unwrap();
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.len(), before + 1);
        let sh = page.shape(r["id"].as_str().unwrap()).unwrap();
        assert!(is_stroke(sh));
        assert_eq!(sh.kind, ShapeKind::Freeform);
        assert!(r["points"].as_u64().unwrap() < 200, "simplified");
        assert!(!sh.path.as_ref().unwrap().closed);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), before);
    }

    #[test]
    fn a_highlighter_stroke_goes_behind_and_is_wide_and_see_through() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 0, "y": 0})).unwrap();
        let r = s.run("ink.add", &json!({"points": [[0, 0], [50, 10]], "highlighter": true})).unwrap();
        let page = s.doc.page().unwrap();
        let first = page.shapes.first().unwrap();
        assert_eq!(first.id, r["id"].as_str().unwrap());
        assert_eq!(first.style.line_width, 12.0);
        assert_eq!(first.style.line_transparency, 0.5);
        assert_eq!(first.ext.get("vizcraft.ink").map(String::as_str), Some("highlighter"));
    }

    #[test]
    fn to_shapes_turns_a_square_and_a_circle_into_a_rectangle_and_an_ellipse() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let a = s.run("ink.add", &json!({"points": square()})).unwrap()["id"].as_str().unwrap().to_string();
        let b = s.run("ink.add", &json!({"points": circle()})).unwrap()["id"].as_str().unwrap().to_string();
        let line = s.run("ink.add", &json!({"points": [[0, 300], [50, 301], [100, 300], [150, 302]]})).unwrap()["id"].as_str().unwrap().to_string();
        let r = s.run("ink.toShapes", &json!({"ids": [a, b, line]})).unwrap();
        let ids: Vec<&str> = r["ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert_eq!(ids.len(), 3, "{r}");
        let page = s.doc.page().unwrap();
        assert_eq!(page.shape(ids[0]).unwrap().kind, ShapeKind::Rectangle);
        assert_eq!(page.shape(ids[1]).unwrap().kind, ShapeKind::Ellipse);
        assert!(page.connector(ids[2]).is_some());
        assert!(!page.shapes.iter().any(is_stroke));
        let rect = page.shape(ids[0]).unwrap();
        assert!((rect.w - 100.0).abs() < 1.0 && (rect.h - 100.0).abs() < 1.0);
    }

    #[test]
    fn erase_deletes_strokes_only() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let id = s.run("ink.add", &json!({"points": [[0, 0], [10, 10]]})).unwrap()["id"].as_str().unwrap().to_string();
        let other = s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 0, "y": 0})).unwrap()["id"].as_str().unwrap().to_string();
        assert!(s.run("ink.erase", &json!({"ids": [other]})).is_err());
        s.run("ink.erase", &json!({"ids": [id.clone()]})).unwrap();
        assert!(s.doc.page().unwrap().shape(&id).is_none());
        assert!(s.run("ink.add", &json!({"points": [[0, "x"]]})).is_err());
        assert!(s.run("ink.add", &json!({"points": []})).is_err());
    }
}
