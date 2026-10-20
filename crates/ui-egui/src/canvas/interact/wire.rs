//! The Wire and Pipe tools, the engineering keys behind them and dropping a device on a wire.
//!
//! A drag with the Wire tool ([`Tool::Wire`]) starts on a pin (a shape's connection point), a
//! junction, a point on a wire or empty page, and ends on any of those. Free ends land on the
//! page's symbol grid module (`snap::module_step`); the preview is the page with the wire added,
//! split where it meets another wire (`net::split_at`), so what is drawn is what the release
//! commits: one `wire.draw`. The tool stays armed for the next wire; Escape returns to the
//! pointer. Pipe is the same tool drawing net kind `pipe`.
//!
//! `ui.wire`, `ui.pipe`, `ui.rotateQuarter`, `ui.mirrorX` and `ui.mirrorY` are the commands the
//! bare-letter keys W, P, R, X and Y run (see `crate::keys`). A device dropped on a wire is
//! spliced into it with `wire.splice` ([`splice_if_on_wire`]); deleting it heals the wire through
//! `shape.delete`'s default.

use serde_json::{Value, json};
use vizcraft_diagram::{self as dg, Arrow, Connector, End, Page, Point, Route, net, snap};

use super::{Drag, GRAB, PREVIEW_ID, run};
use crate::{CraftApp, Tool};

/// Id of the junction a preview split adds (never in the document).
pub const PREVIEW_JUNCTION: &str = "__preview_junction";

/// One end of a wire being drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum WireEnd {
    /// Connection point `1` of shape `0`.
    Pin(String, usize),
    /// A junction shape.
    Junction(String),
    /// A point on a connector, where it will be split at a new junction.
    OnWire(String, Point),
    /// A free point, on the module grid when the page has one.
    Free(Point),
}

impl WireEnd {
    /// The end as `wire.draw` takes it.
    pub fn to_json(&self) -> Value {
        match self {
            WireEnd::Pin(s, i) => json!({"shape": s, "point": i}),
            WireEnd::Junction(j) => json!({"junction": j}),
            WireEnd::OnWire(c, p) => json!({"connector": c, "x": p.x, "y": p.y}),
            WireEnd::Free(p) => json!({"x": p.x, "y": p.y}),
        }
    }

    /// Where the end is on the page.
    pub fn pos(&self, page: &Page) -> Option<Point> {
        match self {
            WireEnd::Pin(s, i) => page.shape(s)?.connection_point_world(*i),
            WireEnd::Junction(j) => Some(page.shape(j)?.center()),
            WireEnd::OnWire(_, p) | WireEnd::Free(p) => Some(*p),
        }
    }
}

/// A free point on the page's module grid, else on whole page units.
pub fn snap_free(page: &Page, p: Point) -> Point {
    match snap::module_step(page) {
        Some(m) => Point::new(snap::round_to(p.x, m), snap::round_to(p.y, m)),
        None => Point::new(p.x.round(), p.y.round()),
    }
}

/// What a wire end at `w` attaches to: a pin or junction within reach, else a wire under it
/// (projected onto the wire, on the module where the wire runs along it), else a free point on
/// the module grid.
pub fn target(page: &Page, w: Point, zoom: f32) -> WireEnd {
    let tol = f64::from(GRAB) / f64::from(zoom.max(0.01));
    if let Some(s) = dg::glue_shape_at(page, w, tol) {
        if net::is_junction(s) {
            return WireEnd::Junction(s.id.clone());
        }
        if let Some(i) = dg::connection_point_at(s, w, tol * 1.5) {
            return WireEnd::Pin(s.id.clone(), i);
        }
    }
    if let Some(c) = dg::connector_at(page, w, tol).filter(|c| c.id != PREVIEW_ID) {
        let poly = dg::route_polyline(page, c);
        if let Some(hit) = net::project(&poly, snap_free(page, w)).or_else(|| net::project(&poly, w)) {
            return WireEnd::OnWire(c.id.clone(), hit.point);
        }
    }
    WireEnd::Free(snap_free(page, w))
}

/// The page end a preview wire attaches to for `e`, splitting a wire on `p` when `e` is on one.
fn preview_end(p: &mut Page, e: &WireEnd, junction: &str, dot: bool) -> End {
    match e {
        WireEnd::Pin(s, i) => End::point(s.clone(), *i),
        WireEnd::Junction(j) => End::point(j.clone(), 0),
        WireEnd::OnWire(c, at) => match net::split_at(p, c, *at, &format!("{junction}_c"), junction, dot) {
            Some(_) => End::point(junction, 0),
            None => End::free(*at),
        },
        WireEnd::Free(at) => End::free(*at),
    }
}

/// `page` with the wire from `from` to the end at `to` drawn in, for the drag preview.
pub fn preview(page: &Page, from: &WireEnd, to: Point, pipe: bool, zoom: f32) -> Page {
    let mut p = page.clone();
    let end = target(page, to, zoom);
    let style = page.connectors.iter().find(|c| c.net.is_some()).map(|c| c.style.clone());
    let start = preview_end(&mut p, from, PREVIEW_JUNCTION, !pipe);
    let end = preview_end(&mut p, &end, &format!("{PREVIEW_JUNCTION}2"), !pipe);
    let mut c = Connector { route: Route::Orthogonal, start_arrow: Arrow::None, end_arrow: Arrow::None, ..Connector::new(PREVIEW_ID, start, end) };
    if let Some(st) = style {
        c.style = st;
    }
    p.connectors.push(c);
    p
}

/// Whether a wire from `a` to `b` is worth drawing: not from a pin back to itself, not a click
/// in one spot.
fn worth_drawing(page: &Page, a: &WireEnd, b: &WireEnd) -> bool {
    if a == b {
        return false;
    }
    match (a.pos(page), b.pos(page)) {
        (Some(p), Some(q)) => p.distance(q) > 1e-6 || !matches!((a, b), (WireEnd::Free(_), WireEnd::Free(_))),
        _ => false,
    }
}

/// Commit a finished wire drag as one `wire.draw`.
pub(super) fn finish(app: &mut CraftApp, from: WireEnd, to: Point, pipe: bool, zoom: f32) {
    let Some(page) = app.session.doc.page() else { return };
    let end = target(page, to, zoom);
    if !worth_drawing(page, &from, &end) {
        return;
    }
    let mut params = json!({"from": from.to_json(), "to": end.to_json()});
    // The armed line kind, else the page's first kind of this tool, draws the wire; with none a
    // pipe is a plain pipe net.
    match line_type(app, pipe) {
        Some(t) => params["type"] = json!(t),
        None if pipe => params["kind"] = json!("pipe"),
        None => {}
    }
    run(app, "wire.draw", params);
}

/// The connector type the Wire (or Pipe) tool draws: the line armed with `ui.tool {line}`, else
/// the first of [`line_kinds`] for the page (its viewpoint and variant), else none.
pub fn line_type(app: &CraftApp, pipe: bool) -> Option<String> {
    app.line_type.clone().or_else(|| default_line(app, pipe))
}

/// The connector kinds of the active page's pack that the Wire (`pipe` false: wire and potential
/// nets) or Pipe tool (pipe and fluid nets) draws, those the page's viewpoint and variant hold.
pub fn line_kinds(app: &CraftApp, pipe: bool) -> Vec<&'static vizcraft_notation::ConnectorType> {
    let cat = vizcraft_notation::catalog();
    let Some(vp_id) = crate::palette::active_viewpoint(app) else { return Vec::new() };
    let Some(pack) = cat.pack(vp_id.split_once('.').map_or("", |(p, _)| p)) else { return Vec::new() };
    let vp = pack.viewpoint(vp_id);
    let variant = crate::palette::active_variant(app);
    let wanted: &[&str] = if pipe { &["pipe", "fluid"] } else { &["wire", "potential"] };
    pack.connector_types
        .iter()
        .filter(|k| k.net.as_deref().is_some_and(|n| wanted.contains(&n)))
        .filter(|k| crate::palette::kind_in_scope(pack, k, vp, variant))
        .collect()
}

/// The kind W or P draws when none is armed: the first of [`line_kinds`].
pub fn default_line(app: &CraftApp, pipe: bool) -> Option<String> {
    line_kinds(app, pipe).first().map(|k| k.id.clone())
}

/// The drag a press with the Wire tool starts.
pub(super) fn start(page: &Page, w: Point, zoom: f32, pipe: bool) -> Drag {
    Drag::Wire { from: target(page, w, zoom), to: w, pipe }
}

/// Alignment lines for the end of a wire drag at `to`: lines to the pins and the wire's start it
/// lines up with (`snap::Kind::Point`), and, on a page with a module, a short cross on the module
/// point it lands on (`snap::Kind::Module`).
pub fn align_lines(page: &Page, from: &WireEnd, to: Point, zoom: f32) -> Vec<snap::AlignLine> {
    let end = target(page, to, zoom);
    let Some(at) = end.pos(page) else { return Vec::new() };
    let mut t = snap::Targets::default();
    let visible = |s: &&dg::Shape| page.shape_visible(s) && !net::is_junction(s);
    t.points = page.shapes.iter().filter(visible).flat_map(|s| s.connection_points_world()).take(20_000).collect();
    t.points.extend(from.pos(page));
    let mut lines = snap::snap_point(at, &t, 0.5).lines;
    if let (Some(m), WireEnd::Free(_)) = (snap::module_step(page), &end) {
        for vertical in [true, false] {
            let (c, o) = if vertical { (at.x, at.y) } else { (at.y, at.x) };
            lines.push(snap::AlignLine { vertical, at: c, from: o - 2.0 * m, to: o + 2.0 * m, kind: snap::Kind::Module });
        }
    }
    lines
}

/// After a shape with two or more connection points lands on the page (dropped or moved), splice
/// it into the wire it sits on with `wire.splice`: a wire (a connector with a net) not already
/// glued to it, whose route crosses the shape's frame. Nothing happens when no wire runs under it.
pub fn splice_if_on_wire(app: &mut CraftApp, shape: &str) {
    let Some(page) = app.session.doc.page() else { return };
    let Some(s) = page.shape(shape) else { return };
    if s.connection_points().len() < 2 || net::is_junction(s) {
        return;
    }
    let frame = s.bounds();
    let wire = page.connectors.iter().filter(|c| c.net.is_some() && !c.touches(shape)).find(|c| {
        let poly = dg::route_polyline(page, c);
        poly.windows(2).any(|w| match (w.first(), w.get(1)) {
            (Some(a), Some(b)) => frame.contains(a.midpoint(*b)) || frame.contains(*a) || frame.contains(*b) || crosses(&frame, *a, *b),
            _ => false,
        })
    });
    let Some(c) = wire.map(|c| c.id.clone()) else { return };
    run(app, "wire.splice", json!({"shape": shape, "connector": c}));
}

/// Whether the axis-aligned or slanted segment `a`-`b` crosses rectangle `r`.
fn crosses(r: &dg::Rect, a: Point, b: Point) -> bool {
    let steps = 16;
    (0..=steps).any(|i| {
        let t = f64::from(i) / f64::from(steps);
        r.contains(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
    })
}

/// Run `id` on the selected shapes, or say what to select when none is.
fn on_selection(app: &mut CraftApp, id: &str, params: Value, what: &str) -> Result<Value, String> {
    let has_shape = app.session.doc.page().is_some_and(|p| app.session.sel.items.iter().any(|i| p.shape(i).is_some()));
    if !has_shape {
        app.status(format!("Select a shape to {what}"));
        return Ok(json!({"shapes": []}));
    }
    app.run(id, params)
}

/// `ui.wire`, `ui.pipe`, `ui.rotateQuarter`, `ui.mirrorX` and `ui.mirrorY`; `None` for any other id.
pub fn ui_command(app: &mut CraftApp, id: &str, _p: &Value) -> Option<Result<Value, String>> {
    Some(match id {
        "ui.wire" | "ui.pipe" => {
            let pipe = id == "ui.pipe";
            app.tool = Tool::Wire { pipe };
            app.canvas.painter_from = None;
            app.connector_kind.clear();
            app.line_type = None;
            Ok(json!({"tool": app.tool.name()}))
        }
        "ui.rotateQuarter" => on_selection(app, "shape.rotate", json!({"turns": 1}), "rotate"),
        "ui.mirrorX" => on_selection(app, "shape.mirror", json!({"axis": "x"}), "mirror"),
        "ui.mirrorY" => on_selection(app, "shape.mirror", json!({"axis": "y"}), "mirror"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Page {
        let mut p = Page::new("p1", "A");
        let mut s = dg::Shape::new("d", dg::ShapeKind::Rectangle, 100.0, 100.0, 40.0, 20.0);
        s.connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
        p.shapes.push(s);
        let mut c = Connector::new("w", End::free(Point::new(0.0, 300.0)), End::free(Point::new(400.0, 300.0)));
        c.route = Route::Orthogonal;
        c.net = Some("n1".into());
        p.connectors.push(c);
        p
    }

    #[test]
    fn targets_are_pins_wires_or_free_points() {
        let p = page();
        assert_eq!(target(&p, Point::new(101.0, 110.0), 1.0), WireEnd::Pin("d".into(), 0));
        assert!(matches!(target(&p, Point::new(200.4, 301.0), 1.0), WireEnd::OnWire(ref c, at) if c == "w" && (at.y - 300.0).abs() < 1e-9));
        assert_eq!(target(&p, Point::new(500.3, 500.6), 1.0), WireEnd::Free(Point::new(500.0, 501.0)));
        assert!(!worth_drawing(&p, &WireEnd::Pin("d".into(), 0), &WireEnd::Pin("d".into(), 0)));
        assert!(worth_drawing(&p, &WireEnd::Pin("d".into(), 0), &WireEnd::Free(Point::new(10.0, 10.0))));
    }

    #[test]
    fn the_preview_splits_the_wire_it_ends_on() {
        let p = page();
        let pre = preview(&p, &WireEnd::Pin("d".into(), 1), Point::new(200.0, 300.0), false, 1.0);
        assert!(pre.shapes.iter().any(net::is_junction), "a junction where the wire meets the other");
        assert_eq!(pre.connectors.len(), 3, "the split wire's two halves and the new one");
    }
}
