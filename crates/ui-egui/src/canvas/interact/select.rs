//! Selection: the single selected shape or connector, clicks (Shift adds or removes) and the
//! marquee.
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
        Drag::Marquee { from, to, .. } => Some(dg::Rect::from_points(*from, *to)),
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
/// outermost group) and connectors whose route crosses it, shapes first.
pub fn marquee_ids(page: &Page, r: &dg::Rect) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let shapes = page
        .shapes
        .iter()
        .filter(|s| s.bounds().intersects(r))
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

/// Click (press and release without dragging) at `at`.
pub(super) fn click(app: &mut CraftApp, vp: &Viewport2D, at: Pos2, m: Modifiers) {
    let Some(page) = app.session.doc.page() else { return };
    let w = point(vp.screen_to_world(at));
    let hit = dg::hit(page, w, f64::from(HIT / vp.zoom));
    if app.tool == Tool::Text {
        if let Some(h) = hit {
            start_editing(app, h);
        }
        // There is no text-only master in the catalog yet, so a click on empty space adds nothing.
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
        let all = marquee_ids(page, &page.rect());
        assert_eq!(all.len(), page.shapes.len() + page.connectors.len());
        assert!(marquee_ids(page, &dg::Rect::from_xywh(5.0, 5.0, 10.0, 10.0)).is_empty());
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
    fn marquee_selects_whole_groups() {
        let p = nested();
        assert_eq!(marquee_ids(&p, &dg::Rect::from_xywh(-10.0, -10.0, 70.0, 70.0)), v(&["g2"]));
    }
}
