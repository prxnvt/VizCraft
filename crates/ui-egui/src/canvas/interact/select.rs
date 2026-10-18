//! Selection: the single selected shape or connector, clicks (Shift adds or removes) and the
//! marquee.

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

/// Ids a marquee over `r` selects: shapes whose outline bounds touch it and connectors whose
/// route crosses it, shapes first.
pub fn marquee_ids(page: &Page, r: &dg::Rect) -> Vec<String> {
    let shapes = page.shapes.iter().filter(|s| s.bounds().intersects(r)).map(|s| s.id.clone());
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
}
