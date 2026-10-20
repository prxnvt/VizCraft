//! The Ladder tool ([`crate::Tool::Ladder`]): a drag draws a ladder frame. While dragging, the
//! rectangle and its two rails show; the release commits one `ladder.insert` with the rung count
//! the height holds at the default spacing (four modules) and the rails as far apart as the
//! width, then returns to the pointer. Escape returns to the pointer before that.
//!
//! A device dropped from the palette or moved into a ladder frame is set on the nearest rung in
//! the same undo step (`ladder.place` inside the `edit.batch`, see `membership.rs`).

use serde_json::json;
use vizcraft_diagram::snap::module_step;
use vizcraft_diagram::{self as dg, Document, Page, Point};

use super::run;
use crate::{CraftApp, Tool};

/// The ladder element type.
const LADDER: &str = "electrical.Ladder";

/// Modules from rung to rung the tool asks for.
const SPACING: f64 = 4.0;

/// Modules of margin left of the first rail, where the rung numbers go.
const MARGIN: f64 = 3.0;

/// Smallest drag that sizes a frame; anything less is a click and gets the default ladder.
const MIN_DRAG: f64 = 20.0;

/// The `ladder.insert` params for a drag from `a` to `b` on `page`.
pub fn insert_params(page: &Page, a: Point, b: Point) -> serde_json::Value {
    let r = dg::Rect::from_points(a, b);
    let m = module_step(page).unwrap_or(10.0);
    if r.width() < MIN_DRAG || r.height() < MIN_DRAG {
        return json!({"x": r.x0, "y": r.y0});
    }
    let rungs = (r.height() / (SPACING * m)).round().clamp(1.0, 200.0);
    let width = (r.width() / m - MARGIN).round().clamp(8.0, 400.0);
    json!({"x": r.x0, "y": r.y0, "rungs": rungs, "spacing": SPACING, "width": width})
}

/// Commit a finished Ladder drag and return to the pointer.
pub(super) fn finish(app: &mut CraftApp, from: Point, to: Point) {
    let Some(page) = app.session.doc.page() else { return };
    let params = insert_params(page, from, to);
    run(app, "ladder.insert", params);
    app.tool = Tool::Pointer;
}

/// Whether `p` lies inside a ladder frame drawn on `page`.
pub fn in_ladder(doc: &Document, page: &Page, p: Point) -> bool {
    page.shapes.iter().any(|s| {
        s.element.as_deref().and_then(|e| doc.model.element(e)).is_some_and(|e| e.type_id == LADDER)
            && dg::Rect::new(s.x, s.y, s.x + s.w, s.y + s.h).contains(p)
    })
}

/// Whether shape `id` on `page` is a ladder frame.
pub fn is_ladder(doc: &Document, page: &Page, id: &str) -> bool {
    page.shape(id).and_then(|s| s.element.as_deref()).and_then(|e| doc.model.element(e)).is_some_and(|e| e.type_id == LADDER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_sizes_rungs_and_width_and_a_click_takes_the_defaults() {
        let view = vizcraft_diagram::vizcraft_model::View { module: vizcraft_diagram::vizcraft_model::Module::Inch0_1, ..Default::default() };
        let page = Page { view: Some(view), ..Page::default() };
        let p = insert_params(&page, Point::new(100.0, 50.0), Point::new(530.0, 450.0));
        assert_eq!((p["rungs"].as_f64(), p["width"].as_f64()), (Some(10.0), Some(40.0)));
        let click = insert_params(&page, Point::new(100.0, 50.0), Point::new(102.0, 51.0));
        assert!(click.get("rungs").is_none());
    }
}
