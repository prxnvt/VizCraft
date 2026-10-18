//! What a press starts with each canvas tool: a handle drag, a connector, a marquee, or a
//! select-and-move, decided against the page before any command runs.

use egui::{Modifiers, Pos2};
use vizcraft_diagram::{self as dg, End, Hit, Page, Point};

use super::{Drag, GRAB, HANDLES, HIT, handle_world, rotate_handle_world, single_connector, single_shape, target_end};
use crate::canvas::Viewport2D;
use crate::canvas::draw::{point, pos};
use crate::{CraftApp, Tool};

/// What a press at `w` would start, decided against the page before any command runs.
pub(super) enum Start {
    /// Start this drag.
    Drag(Drag),
    /// Select `id` (adding with Shift), then move the selection.
    SelectAndMove(String, bool),
}

pub(super) fn near(vp: &Viewport2D, a: Point, screen: Pos2) -> bool {
    vp.world_to_screen(pos(a)).distance(screen) <= GRAB
}

pub(super) fn decide(app: &CraftApp, page: &Page, vp: &Viewport2D, at: Pos2, m: Modifiers) -> Option<Start> {
    let w = point(vp.screen_to_world(at));
    let zoom = vp.zoom;
    let tol = f64::from(GRAB / zoom);
    match app.tool {
        Tool::Text => None,
        Tool::Connector => Some(Start::Drag(Drag::Connect { from: target_end(page, w, zoom), to: w })),
        Tool::Pointer => {
            if let Some(s) = single_shape(app, page) {
                if near(vp, rotate_handle_world(s, zoom), at) {
                    return Some(Start::Drag(Drag::Rotate { id: s.id.clone(), to: w, snap: m.shift }));
                }
                if let Some(i) = (0..HANDLES.len()).find(|i| handle_world(s, *i).is_some_and(|h| near(vp, h, at))) {
                    return Some(Start::Drag(Drag::Resize { id: s.id.clone(), handle: i, to: w, keep_aspect: m.shift }));
                }
            }
            if let Some(c) = single_connector(app, page) {
                let ends = dg::resolve_ends(page, c);
                for (start, p) in [(false, ends.end.pos), (true, ends.start.pos)] {
                    if near(vp, p, at) {
                        return Some(Start::Drag(Drag::Reglue { id: c.id.clone(), start, to: w }));
                    }
                }
            }
            if let Some(s) = dg::glue_shape_at(page, w, tol)
                && let Some(i) = dg::connection_point_at(s, w, tol)
            {
                return Some(Start::Drag(Drag::Connect { from: End::point(s.id.clone(), i), to: w }));
            }
            match dg::hit(page, w, f64::from(HIT / zoom)) {
                Some(Hit::Shape(id)) => Some(Start::SelectAndMove(super::select::press_target(page, &id, &app.session.sel.items), m.shift)),
                Some(Hit::Connector(id)) => Some(Start::SelectAndMove(id, m.shift)),
                None => Some(Start::Drag(Drag::Marquee { from: w, to: w, add: m.shift })),
            }
        }
    }
}
