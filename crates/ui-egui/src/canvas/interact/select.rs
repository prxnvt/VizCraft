//! Selection: the single selected shape or connector, clicks (Shift adds or removes), the
//! marquee and the freehand lasso.
//!
//! Groups: a click on a member selects its outermost group; a second click selects the next
//! group down, then the member itself. Once a member is selected, a click on another member of
//! the same group selects that member directly. A marquee selects whole groups.

use egui::{Modifiers, Pos2};
use serde_json::json;
use vizcraft_diagram::{self as dg, Connector, Hit, Page, Point, Shape};

use super::{Drag, HIT, run, start_editing};
use crate::canvas::Viewport2D;
use crate::canvas::draw::point;
use crate::{CraftApp, Tool};

/// The single selected shape, if exactly one item is selected and it is a shape.
pub fn single_shape<'a>(app: &CraftApp, page: &'a Page) -> Option<&'a Shape> {
    match app.session.sel.items.as_slice() {
        [id] => page.shape(id),
        _ => None,
    }
}

/// The single selected connector, if exactly one item is selected and it is a connector.
pub fn single_connector<'a>(app: &CraftApp, page: &'a Page) -> Option<&'a Connector> {
    match app.session.sel.items.as_slice() {
        [id] => page.connector(id),
        _ => None,
    }
}

/// The marquee rectangle of a drag, if it is one.
pub fn marquee_rect(drag: &Drag) -> Option<dg::Rect> {
    match drag {
        Drag::Marquee { from, to, .. } | Drag::TextBox { from, to } | Drag::Ladder { from, to } => Some(dg::Rect::from_points(*from, *to)),
        _ => None,
    }
}

/// Whether segment `a`-`b` touches `r`.
pub(super) fn segment_hits_rect(a: Point, b: Point, r: &dg::Rect) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    let corners = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)];
    let crosses = |p: Point, q: Point| {
        let d1 = (b - a).cross(p - a);
        let d2 = (b - a).cross(q - a);
        let d3 = (q - p).cross(a - p);
        let d4 = (q - p).cross(b - p);
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    (0..4).any(|i| crosses(corners[i], corners[(i + 1) % 4]))
}

/// The groups around shape `id`, outermost first, then `id` itself.
fn chain(page: &Page, id: &str) -> Vec<String> {
    let mut c = dg::arrange::group_chain(page, id);
    c.reverse();
    c.push(id.to_string());
    c
}

/// How deep in `chain` the selection already is: `(k, true)` when `chain[k]` is selected,
/// `(k, false)` when a member of `chain[k]` is (the user is working inside that group). The
/// deepest such level wins.
fn depth(page: &Page, chain: &[String], sel: &[String]) -> Option<(usize, bool)> {
    (0..chain.len()).rev().find_map(|k| {
        let g = chain.get(k)?;
        if sel.contains(g) {
            Some((k, true))
        } else if sel.iter().any(|s| page.shape(s).is_some_and(|x| x.parent.as_ref() == Some(g))) {
            Some((k, false))
        } else {
            None
        }
    })
}

/// What a click on shape `id` selects, given the current selection `sel`: the outermost group
/// around it, or one level further down when that level (or a sibling in it) is selected.
pub fn click_target(page: &Page, id: &str, sel: &[String]) -> String {
    let id = &dg::sequence::owner(page, id);
    let c = chain(page, id);
    let last = c.len().saturating_sub(1);
    let k = match depth(page, &c, sel) {
        None => 0,
        Some((k, _)) => (k + 1).min(last),
    };
    c.get(k).cloned().unwrap_or_else(|| id.to_string())
}

/// What a press-and-drag on shape `id` moves: the selected level of its group chain, else the
/// shape at the level the user is working in, else the outermost group.
pub fn press_target(page: &Page, id: &str, sel: &[String]) -> String {
    let id = &dg::sequence::owner(page, id);
    let c = chain(page, id);
    let last = c.len().saturating_sub(1);
    let k = match depth(page, &c, sel) {
        None => 0,
        Some((k, true)) => k,
        Some((k, false)) => (k + 1).min(last),
    };
    c.get(k).cloned().unwrap_or_else(|| id.to_string())
}

/// Ids a marquee over `r` selects: shapes whose outline bounds touch it (a member stands for its
/// outermost group) and connectors whose route crosses it, shapes first. Hidden shapes and
/// shapes on a locked layer are skipped.
pub fn marquee_ids(page: &Page, r: &dg::Rect) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let shapes = page
        .shapes
        .iter()
        .filter(|s| dg::layer::selectable(page, s) && s.bounds().intersects(r))
        .map(|s| chain(page, &s.id).into_iter().next().unwrap_or_else(|| s.id.clone()))
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>()
        .into_iter();
    let connectors = page
        .connectors
        .iter()
        .filter(|c| dg::route_polyline(page, c).windows(2).any(|w| matches!(w, [a, b] if segment_hits_rect(*a, *b, r))))
        .map(|c| c.id.clone());
    shapes.chain(connectors).collect()
}

/// Most points one lasso keeps; past this the path stops growing, so a long drag cannot make the
/// containment test slow.
const MAX_LASSO_POINTS: usize = 4096;

/// Add pointer position `w` to a lasso path, skipping points closer than two screen points to the
/// last one (so a slow drag does not pile up points).
pub(super) fn extend_lasso(points: &mut Vec<Point>, w: Point, zoom: f32) {
    let min = f64::from(2.0 / zoom.max(0.01));
    if points.len() < MAX_LASSO_POINTS && points.last().is_none_or(|p| (*p - w).length() >= min) {
        points.push(w);
    }
}

/// Whether `p` is inside the closed polygon `poly` (even-odd rule).
fn inside(poly: &[Point], p: Point) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (Some(a), Some(b)) = (poly.get(i), poly.get((i + n - 1) % n)) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
    }
    odd
}

/// Ids a lasso through `poly` selects: shapes whose whole outline is inside it (a member stands
/// for its outermost group) and connectors whose whole route is, shapes first. Hidden and locked
/// shapes are skipped, as for the marquee.
pub fn lasso_ids(page: &Page, poly: &[Point]) -> Vec<String> {
    if poly.len() < 3 {
        return Vec::new();
    }
    let enclosed = |pts: &[Point]| !pts.is_empty() && pts.iter().all(|p| inside(poly, *p));
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = page
        .shapes
        .iter()
        .filter(|s| dg::layer::selectable(page, s) && !dg::arrange::is_group(s))
        .filter(|s| enclosed(&s.outline_world().flatten(1.0).into_iter().flat_map(|pl| pl.points).collect::<Vec<_>>()))
        .map(|s| chain(page, &s.id).into_iter().next().unwrap_or_else(|| s.id.clone()))
        .filter(|id| seen.insert(id.clone()))
        .collect();
    out.extend(page.connectors.iter().filter(|c| enclosed(&dg::route_polyline(page, c))).map(|c| c.id.clone()));
    out
}

/// Click (press and release without dragging) at `at`.
pub(super) fn click(app: &mut CraftApp, vp: &Viewport2D, at: Pos2, m: Modifiers) {
    let Some(page) = app.session.doc.page() else { return };
    let w = point(vp.screen_to_world(at));
    let hit = dg::hit(page, w, f64::from(HIT / vp.zoom));
    if let Tool::FormatPainter { keep } = app.tool {
        super::painter::apply(app, hit, keep);
        return;
    }
    if app.tool == Tool::Text {
        match hit {
            Some(h) => start_editing(app, h),
            // On empty page: a new text box centred on the click, edited straight away.
            None => super::tools::add_text_box(app, w, None),
        }
        return;
    }
    let sel = app.session.sel.items.clone();
    // A click on a group member picks the group level (see the module doc).
    let hit = match hit {
        Some(Hit::Shape(id)) if m.shift => Some(Hit::Shape(press_target(page, &id, &sel))),
        Some(Hit::Shape(id)) => Some(Hit::Shape(click_target(page, &id, &sel))),
        other => other,
    };
    match hit {
        Some(Hit::Shape(id) | Hit::Connector(id)) if m.shift => {
            if sel.contains(&id) {
                let rest: Vec<String> = sel.into_iter().filter(|i| *i != id).collect();
                if rest.is_empty() {
                    run(app, "edit.selectNone", json!({}));
                } else {
                    run(app, "edit.select", json!({"ids": rest}));
                }
            } else {
                run(app, "edit.select", json!({"ids": [id], "add": true}));
            }
        }
        Some(Hit::Shape(id) | Hit::Connector(id)) => {
            if sel != [id.clone()] {
                run(app, "edit.select", json!({"ids": [id]}));
            }
        }
        None => {
            if !m.shift && !sel.is_empty() {
                run(app, "edit.selectNone", json!({}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_touches_shapes_and_crossing_connectors() {
        let doc = vizcraft_engine::sample::sample_document();
        let page = doc.page().unwrap();
        let first = &page.shapes[0];
        let r = first.bounds().inflate(-5.0);
        let ids = marquee_ids(page, &r);
        assert_eq!(ids, vec![first.id.clone()]);
        let all = marquee_ids(page, &page.frame());
        assert_eq!(all.len(), page.shapes.len() + page.connectors.len());
        assert!(marquee_ids(page, &dg::Rect::from_xywh(5.0, 5.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn marquee_skips_hidden_and_locked_shapes() {
        let mut doc = vizcraft_engine::sample::sample_document();
        let page = doc.page_mut().unwrap();
        page.layers = vec![
            dg::Layer { id: "h".into(), visible: false, ..dg::Layer::default() },
            dg::Layer { id: "k".into(), locked: true, ..dg::Layer::default() },
        ];
        page.shapes[0].layers = vec!["h".into()];
        page.shapes[1].layers = vec!["k".into()];
        let (hidden, locked) = (page.shapes[0].id.clone(), page.shapes[1].id.clone());
        let ids = marquee_ids(page, &page.frame());
        assert!(!ids.contains(&hidden) && !ids.contains(&locked));
        assert_eq!(ids.len(), page.shapes.len() + page.connectors.len() - 2);
    }

    /// Shapes `a`, `b` in group `g1`, which is in `g2` with `c`.
    fn nested() -> Page {
        let mut p = Page::new("p1", "Page 1");
        for (id, x) in [("a", 0.0), ("b", 100.0), ("c", 300.0)] {
            p.shapes.push(Shape::new(id, dg::ShapeKind::Rectangle, x, 0.0, 50.0, 50.0));
        }
        dg::arrange::group(&mut p, &["a".into(), "b".into()], "g1").unwrap();
        dg::arrange::group(&mut p, &["g1".into(), "c".into()], "g2").unwrap();
        p
    }

    fn v(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clicks_walk_down_nested_groups() {
        let p = nested();
        assert_eq!(click_target(&p, "a", &[]), "g2");
        assert_eq!(click_target(&p, "a", &v(&["g2"])), "g1");
        assert_eq!(click_target(&p, "a", &v(&["g1"])), "a");
        assert_eq!(click_target(&p, "a", &v(&["a"])), "a");
        // Working inside g1: a click on the sibling selects it directly.
        assert_eq!(click_target(&p, "b", &v(&["a"])), "b");
        // c is a direct member of g2.
        assert_eq!(click_target(&p, "c", &v(&["g1"])), "c");
        assert_eq!(click_target(&p, "c", &v(&["zzz"])), "g2");
        // A shape in no group is itself.
        let mut q = p.clone();
        q.shapes.push(Shape::new("d", dg::ShapeKind::Rectangle, 0.0, 200.0, 10.0, 10.0));
        assert_eq!(click_target(&q, "d", &v(&["g2"])), "d");
    }

    #[test]
    fn drags_move_the_selected_level() {
        let p = nested();
        assert_eq!(press_target(&p, "a", &[]), "g2");
        assert_eq!(press_target(&p, "a", &v(&["g2"])), "g2");
        assert_eq!(press_target(&p, "a", &v(&["g1"])), "g1");
        assert_eq!(press_target(&p, "b", &v(&["a"])), "b");
        assert_eq!(press_target(&p, "a", &v(&["a"])), "a");
    }

    #[test]
    fn lasso_selects_enclosed_items_only() {
        let p = nested();
        let around = |x0: f64, y0: f64, x1: f64, y1: f64| vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)];
        // Around `c` only: c is a direct member of g2, so the group is selected.
        assert_eq!(lasso_ids(&p, &around(290.0, -10.0, 360.0, 60.0)), v(&["g2"]));
        // Cutting through `a` selects nothing.
        assert!(lasso_ids(&p, &around(10.0, -10.0, 40.0, 60.0)).is_empty());
        // A triangle around a lone shape.
        let mut q = Page::new("p", "P");
        q.shapes.push(Shape::new("t", dg::ShapeKind::Rectangle, 40.0, 40.0, 20.0, 20.0));
        assert_eq!(lasso_ids(&q, &[Point::new(0.0, 0.0), Point::new(200.0, 0.0), Point::new(0.0, 200.0)]), v(&["t"]));
        assert!(lasso_ids(&q, &[Point::new(0.0, 0.0), Point::new(200.0, 0.0)]).is_empty(), "two points enclose nothing");
        let mut pts = vec![Point::ZERO];
        extend_lasso(&mut pts, Point::new(0.5, 0.0), 1.0);
        assert_eq!(pts.len(), 1, "points closer than two screen points are skipped");
        extend_lasso(&mut pts, Point::new(5.0, 0.0), 1.0);
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn marquee_selects_whole_groups() {
        let p = nested();
        assert_eq!(marquee_ids(&p, &dg::Rect::from_xywh(-10.0, -10.0, 70.0, 70.0)), v(&["g2"]));
    }
}
