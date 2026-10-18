//! Shape operations (Developer > Shape Design > Operations): union, combine, fragment,
//! intersect, subtract, offset, join and trim, over the geometry in
//! [`vizcraft_diagram::shapeop`].
//!
//! Every operation acts on `ids` (default: the selected shapes, in selection order), replaces
//! them with freeform results in one undo step, and selects the results. A result takes the first
//! input's style, text, data, layers and parent and sits where the first input sat in the z
//! order; connectors glued to the inputs keep their ends where they were drawn (see
//! [`super::delete_on_page`]). Inputs that are locked against deletion, on a locked layer, or
//! groups are refused before anything changes.

use serde_json::{Value, json};
use vizcraft_diagram::geom::Path;
use vizcraft_diagram::shapeop::{self as op, BoolOp};
use vizcraft_diagram::{Shape, shape_fields::Freeform};

use super::protect::{Lock, check};
use super::{Kinds, delete_on_page, drop_undrawn_relationships, has_shapes, num, page_mut, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The shape operation commands.
pub fn specs() -> Vec<CommandSpec> {
    const IDS: &str = r#"{"ids"?: [string] (default: the selected shapes)} -> {"ids": [new shape ids], "removed": [ids]}"#;
    vec![
        CommandSpec::new("shapeop.union", "Union", "Developer > Shape Design > Operations", union).params(IDS).when(has_shapes),
        CommandSpec::new("shapeop.combine", "Combine", "Developer > Shape Design > Operations", combine).params(IDS).when(has_shapes),
        CommandSpec::new("shapeop.fragment", "Fragment", "Developer > Shape Design > Operations", fragment)
            .params(r#"{"ids"?: [string]} -> {"ids": [one new shape per piece], "removed": [ids]}"#)
            .when(has_shapes),
        CommandSpec::new("shapeop.intersect", "Intersect", "Developer > Shape Design > Operations", intersect).params(IDS).when(has_shapes),
        CommandSpec::new("shapeop.subtract", "Subtract", "Developer > Shape Design > Operations", subtract)
            .params(r#"{"ids"?: [string]} -> {"ids", "removed"}; the first shape (selection order) minus the others"#)
            .when(has_shapes),
        CommandSpec::new("shapeop.offset", "Offset", "Developer > Shape Design > Operations", offset)
            .params(r#"{"ids"?: [string], "distance": n (negative shrinks; at most 10000 either way)} -> {"ids", "removed"}; each shape is replaced by its outline moved out by distance"#)
            .when(has_shapes),
        CommandSpec::new("shapeop.join", "Join", "Developer > Shape Design > Operations", join)
            .params(r#"{"ids"?: [string]} -> {"ids", "removed"}; lines whose ends lie within 2 units are joined end to end into one shape"#)
            .when(has_shapes),
        CommandSpec::new("shapeop.trim", "Trim", "Developer > Shape Design > Operations", trim)
            .params(r#"{"ids"?: [string]} -> {"ids", "removed"}; lines and outlines are cut where they cross, one open shape per piece"#)
            .when(has_shapes),
    ]
}

/// The shapes an operation takes, checked: at least `min` of them, none a group, none locked
/// against deletion or on a locked layer, and at most [`op::MAX_INPUTS`].
fn inputs(s: &Session, v: &Value, min: usize) -> Result<Vec<String>, EngineError> {
    let ids = targets(s, v, Kinds::Shapes)?;
    if ids.len() < min {
        return Err(EngineError::Params(format!("select at least {min} shapes (or pass `ids`)")));
    }
    if ids.len() > op::MAX_INPUTS {
        return Err(EngineError::Params(format!("an operation takes at most {} shapes", op::MAX_INPUTS)));
    }
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(g) = ids.iter().find(|id| page.shapes.iter().any(|x| x.parent.as_deref() == Some(id.as_str()))) {
        return Err(EngineError::Params(format!("`{}` is a group or container; ungroup it first", short(g))));
    }
    super::layer::ensure_unlocked(s, &ids)?;
    check(s, &ids, Lock::Delete)?;
    Ok(ids)
}

/// One result before it gets an id: a path in world coordinates and whether it is closed.
type Piece = (Path, bool);

/// Replace `ids` with `pieces` (styled after the first input) at the first input's place in the z
/// order, select them, and report.
fn replace(s: &mut Session, ids: &[String], pieces: Vec<Piece>) -> CmdResult {
    if pieces.is_empty() {
        return Err(EngineError::Params("the result covers nothing; the shapes do not overlap".into()));
    }
    if pieces.iter().any(|(p, _)| p.els.len() > Freeform::MAX_ELS) {
        return Err(EngineError::Params(format!("the result has more than {} path points; simplify the shapes first", Freeform::MAX_ELS)));
    }
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let first =
        ids.first().and_then(|id| page.shape(id)).cloned().ok_or_else(|| EngineError::Params("select a shape first (or pass `ids`)".into()))?;
    let at = page.shapes.iter().position(|x| ids.contains(&x.id)).unwrap_or(page.shapes.len());
    let mut made: Vec<Shape> = Vec::new();
    for (path, closed) in &pieces {
        let id = s.doc.next_id("s");
        if let Some(sh) = op::freeform(&first, id, path, *closed) {
            made.push(sh);
        }
    }
    if made.is_empty() {
        return Err(EngineError::Params("the result covers nothing".into()));
    }
    let new_ids: Vec<String> = made.iter().map(|x| x.id.clone()).collect();
    let page = page_mut(s)?;
    let d = delete_on_page(page, ids);
    // The inputs are gone, so the first input's place is at or before `at`.
    let at = at.min(page.shapes.len());
    for (k, mut sh) in made.into_iter().enumerate() {
        if sh.parent.as_deref().is_some_and(|p| page.shape(p).is_none()) {
            sh.parent = None;
        }
        page.shapes.insert(at.saturating_add(k).min(page.shapes.len()), sh);
    }
    // A group that holds a result refits around it.
    vizcraft_diagram::arrange::tidy_groups(page);
    drop_undrawn_relationships(&mut s.doc, &d.relationships);
    select(s, new_ids.clone());
    with_sel(s, json!({"ids": new_ids, "removed": ids}))
}

/// The page shapes `ids` name, in that order.
fn shapes_of(s: &Session, ids: &[String]) -> Vec<Shape> {
    let Some(page) = s.doc.page() else { return Vec::new() };
    ids.iter().filter_map(|id| page.shape(id)).cloned().collect()
}

fn boolean(s: &mut Session, v: &Value, which: BoolOp) -> CmdResult {
    let ids = inputs(s, v, 2)?;
    let list = shapes_of(s, &ids);
    let refs: Vec<&Shape> = list.iter().collect();
    let regions = op::boolean(which, &refs);
    let pieces = if regions.is_empty() { Vec::new() } else { vec![(op::regions_path(&regions), true)] };
    replace(s, &ids, pieces)
}

fn union(s: &mut Session, v: &Value) -> CmdResult {
    boolean(s, v, BoolOp::Union)
}

fn combine(s: &mut Session, v: &Value) -> CmdResult {
    boolean(s, v, BoolOp::Combine)
}

fn intersect(s: &mut Session, v: &Value) -> CmdResult {
    boolean(s, v, BoolOp::Intersect)
}

fn subtract(s: &mut Session, v: &Value) -> CmdResult {
    boolean(s, v, BoolOp::Subtract)
}

fn fragment(s: &mut Session, v: &Value) -> CmdResult {
    let ids = inputs(s, v, 2)?;
    let list = shapes_of(s, &ids);
    let refs: Vec<&Shape> = list.iter().collect();
    let pieces = op::fragment(&refs).into_iter().map(|r| (op::regions_path(std::slice::from_ref(&r)), true)).collect();
    replace(s, &ids, pieces)
}

fn offset(s: &mut Session, v: &Value) -> CmdResult {
    let distance = num(v, "distance")?.ok_or_else(|| EngineError::Params("`distance` (number) is required".into()))?;
    let ids = inputs(s, v, 1)?;
    let list = shapes_of(s, &ids);
    // Each shape becomes its own offset outline, styled after itself.
    let mut out_ids: Vec<String> = Vec::new();
    let mut paths = Vec::new();
    for sh in &list {
        let r = op::offset(sh, distance);
        if r.is_empty() {
            return Err(EngineError::Params(format!("an offset of {distance} swallows `{}`", short(&sh.id))));
        }
        paths.push((sh.id.clone(), op::regions_path(&r)));
    }
    for (id, path) in paths {
        let r = replace(s, std::slice::from_ref(&id), vec![(path, true)])?;
        if let Some(a) = r.get("ids").and_then(Value::as_array) {
            out_ids.extend(a.iter().filter_map(|x| x.as_str().map(str::to_string)));
        }
    }
    select(s, out_ids.clone());
    with_sel(s, json!({"ids": out_ids, "removed": ids}))
}

fn join(s: &mut Session, v: &Value) -> CmdResult {
    let ids = inputs(s, v, 1)?;
    let list = shapes_of(s, &ids);
    let refs: Vec<&Shape> = list.iter().collect();
    let pieces = op::join(&refs, op::JOIN_TOLERANCE).into_iter().collect();
    replace(s, &ids, pieces)
}

fn trim(s: &mut Session, v: &Value) -> CmdResult {
    let ids = inputs(s, v, 1)?;
    let list = shapes_of(s, &ids);
    let refs: Vec<&Shape> = list.iter().collect();
    let pieces = op::trim(&refs).iter().map(|pts| (op::polyline_path(pts), false)).collect();
    replace(s, &ids, pieces)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::geom::{Path, Point};
    use vizcraft_diagram::{Connector, Document, End, Shape, ShapeKind};

    use crate::Session;

    fn two_overlapping() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        page.board = vizcraft_diagram::Board::Fixed;
        page.width = 200.0;
        page.height = 200.0;
        let mut a = Shape::new("a", ShapeKind::Rectangle, 10.0, 10.0, 100.0, 100.0);
        a.text = "A".into();
        a.style.fill = vizcraft_diagram::Color::rgb(0xdd, 0xee, 0xff);
        page.shapes.push(a);
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 60.0, 60.0, 100.0, 100.0));
        doc.id_seq = 10;
        Session::new(doc)
    }

    fn ids(r: &serde_json::Value) -> Vec<String> {
        r["ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn union_of_two_overlapping_rectangles_is_one_freeform_of_their_area() {
        let mut s = two_overlapping();
        let r = s.run("shapeop.union", &json!({"ids": ["a", "b"]})).unwrap();
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.len(), 1);
        let u = &page.shapes[0];
        assert_eq!(ids(&r), vec![u.id.clone()]);
        assert_eq!(u.kind, ShapeKind::Freeform);
        assert_eq!((u.text.as_str(), u.style.fill), ("A", vizcraft_diagram::Color::rgb(0xdd, 0xee, 0xff)));
        let area = vizcraft_diagram::shapeop::area(&u.outline_world());
        assert!((area - 17_500.0).abs() <= 175.0, "{area}");
        assert_eq!(s.sel.items, vec![u.id.clone()]);
        // One undo step brings both inputs back.
        s.run("edit.undo", &json!({})).unwrap();
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn subtract_leaves_a_hole_drawn_even_odd() {
        let mut s = two_overlapping();
        s.run("shape.resize", &json!({"id": "b", "x": 35, "y": 35, "w": 50, "h": 50})).unwrap();
        s.run("edit.select", &json!({"ids": ["a", "b"]})).unwrap();
        s.run("shapeop.subtract", &json!({})).unwrap();
        let page = s.doc.page().unwrap();
        let ring = page.shapes[0].outline_world();
        assert_eq!(ring.els.iter().filter(|e| matches!(e, vizcraft_diagram::PathEl::MoveTo(_))).count(), 2);
        assert!(vizcraft_diagram::shapeop::contains_even_odd(&ring, Point::new(20.0, 20.0)));
        assert!(!vizcraft_diagram::shapeop::contains_even_odd(&ring, Point::new(60.0, 60.0)));
        assert!(vizcraft_diagram::shape_at(page, Point::new(20.0, 20.0), 0.0).is_some());
    }

    #[test]
    fn fragment_of_two_overlapping_rectangles_gives_three() {
        let mut s = two_overlapping();
        let r = s.run("shapeop.fragment", &json!({"ids": ["a", "b"]})).unwrap();
        assert_eq!(ids(&r).len(), 3);
        assert_eq!(s.doc.page().unwrap().shapes.len(), 3);
        assert!(s.doc.page().unwrap().shapes.iter().all(|x| x.kind == ShapeKind::Freeform && x.text == "A"));
    }

    /// The three pieces of two overlapping rectangles, each a freeform styled after the first.
    const FRAGMENT_GOLDEN: &str = r##"  <g id="s11">
    <path d="M60 110 L60 60 L110 60 L110 110 Z" fill="#ddeeff" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="85" y="89.2">A</tspan></text>
  </g>
  <g id="s12">
    <path d="M10 110 L10 10 L110 10 L110 60 L60 60 L60 110 Z" fill="#ddeeff" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="60" y="64.2">A</tspan></text>
  </g>
  <g id="s13">
    <path d="M60 160 L60 110 L110 110 L110 60 L160 60 L160 160 Z" fill="#ddeeff" stroke="#3a4a5c" stroke-width="1.5"/>
    <text text-anchor="middle" font-family="Inter, sans-serif" font-size="12" fill="#1e2630" xml:space="preserve"><tspan x="110" y="114.2">A</tspan></text>
  </g>
</svg>"##;

    #[test]
    fn golden_svg_of_a_fragment_result() {
        let mut s = two_overlapping();
        s.run("shapeop.fragment", &json!({"ids": ["a", "b"]})).unwrap();
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        let start = svg.find("  <g id=").unwrap();
        let body = svg[start..].trim_end();
        assert_eq!(body, FRAGMENT_GOLDEN, "{body}");
    }

    #[test]
    fn intersect_of_disjoint_shapes_is_refused_and_changes_nothing() {
        let mut s = two_overlapping();
        s.run("shape.move", &json!({"ids": ["b"], "dx": 500, "dy": 0})).unwrap();
        assert!(s.run("shapeop.intersect", &json!({"ids": ["a", "b"]})).is_err());
        assert_eq!(s.doc.page().unwrap().shapes.len(), 2);
        s.run("shape.move", &json!({"ids": ["b"], "dx": -500, "dy": 0})).unwrap();
        s.run("shapeop.intersect", &json!({"ids": ["a", "b"]})).unwrap();
        let b = s.doc.page().unwrap().shapes[0].bounds();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (60.0, 60.0, 110.0, 110.0));
    }

    #[test]
    fn offset_grows_the_bounds_by_the_distance() {
        let mut s = two_overlapping();
        s.run("shapeop.offset", &json!({"ids": ["a"], "distance": 10})).unwrap();
        let page = s.doc.page().unwrap();
        let grown = page.shapes.iter().find(|x| x.kind == ShapeKind::Freeform).unwrap().bounds();
        for (got, want) in [(grown.x0, 0.0), (grown.y0, 0.0), (grown.x1, 120.0), (grown.y1, 120.0)] {
            assert!((got - want).abs() < 0.5, "{grown:?}");
        }
        assert!(s.run("shapeop.offset", &json!({"ids": ["b"], "distance": -60})).is_err());
        assert!(s.run("shapeop.offset", &json!({"ids": ["b"]})).is_err());
    }

    fn line(s: &mut Session, id: &str, a: Point, b: Point) {
        let sh = vizcraft_diagram::shapeop::freeform(&Shape::default(), id.into(), &Path::polyline(&[a, b]), false).unwrap();
        s.doc.page_mut().unwrap().shapes.push(sh);
    }

    #[test]
    fn join_connects_lines_end_to_end_and_closes_a_loop() {
        let mut s = Session::new(Document::new());
        line(&mut s, "l1", Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        line(&mut s, "l2", Point::new(100.0, 1.0), Point::new(100.0, 100.0));
        line(&mut s, "l3", Point::new(0.0, 0.0), Point::new(99.0, 100.0));
        let r = s.run("shapeop.join", &json!({"ids": ["l1", "l2", "l3"]})).unwrap();
        assert_eq!(ids(&r).len(), 1);
        let page = s.doc.page().unwrap();
        let j = &page.shapes[0];
        let f = j.path.as_ref().unwrap();
        assert!(f.closed, "{f:?}");
        assert_eq!(f.els.iter().filter(|e| matches!(e, vizcraft_diagram::PathEl::MoveTo(_))).count(), 1);
    }

    #[test]
    fn trim_cuts_crossing_lines_into_open_pieces() {
        let mut s = Session::new(Document::new());
        line(&mut s, "h", Point::new(0.0, 50.0), Point::new(100.0, 50.0));
        line(&mut s, "v", Point::new(50.0, 0.0), Point::new(50.0, 100.0));
        let r = s.run("shapeop.trim", &json!({"ids": ["h", "v"]})).unwrap();
        assert_eq!(ids(&r).len(), 4);
        let page = s.doc.page().unwrap();
        assert!(page.shapes.iter().all(|x| x.path.as_ref().is_some_and(|f| !f.closed) && x.style.fill.a == 0));
        assert!(page.shapes.iter().all(|x| (x.w - 50.0).abs() < 1e-9 || (x.h - 50.0).abs() < 1e-9));
    }

    #[test]
    fn a_freeform_round_trips_natively_and_through_drawio() {
        let mut s = two_overlapping();
        s.run("shapeop.combine", &json!({"ids": ["a", "b"]})).unwrap();
        let want = s.doc.page().unwrap().shapes[0].clone();
        let native = crate::io::load_bytes("x.vizcraft", &crate::io::save_bytes("x.vizcraft", &s.doc).unwrap()).unwrap();
        assert_eq!(native.page().unwrap().shapes[0], want);
        let drawio = crate::io::load_bytes("x.drawio", &crate::io::save_bytes("x.drawio", &s.doc).unwrap()).unwrap();
        let got = &drawio.page().unwrap().shapes[0];
        assert_eq!((got.kind, &got.path, got.frame()), (want.kind, &want.path, want.frame()));
    }

    #[test]
    fn locked_inputs_and_groups_are_refused() {
        let mut s = two_overlapping();
        s.run("shape.setLocks", &json!({"ids": ["b"], "delete": true})).unwrap();
        assert!(s.run("shapeop.union", &json!({"ids": ["a", "b"]})).is_err());
        assert!(s.run("shapeop.union", &json!({"ids": ["a"]})).is_err());
        assert_eq!(s.doc.page().unwrap().shapes.len(), 2);
    }

    #[test]
    fn connectors_glued_to_inputs_keep_their_ends() {
        let mut s = two_overlapping();
        s.doc.page_mut().unwrap().shapes.push(Shape::new("c", ShapeKind::Ellipse, 300.0, 300.0, 40.0, 40.0));
        s.doc.page_mut().unwrap().connectors.push(Connector::new("k", End::dynamic("a"), End::dynamic("c")));
        s.run("shapeop.union", &json!({"ids": ["a", "b"]})).unwrap();
        let page = s.doc.page().unwrap();
        let k = page.connector("k").unwrap();
        assert!(matches!(k.start, End::Free { .. }) && k.end.shape() == Some("c"));
    }
}
