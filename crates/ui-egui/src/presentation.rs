//! Presentation mode (`ui.present`): the window shows the drawing and nothing else. Title bar,
//! menus, ribbon, side panes, dock and status bar are not drawn, rulers and the grid are hidden,
//! and the window asks for full screen. Left, Right, PageUp and PageDown switch pages, Escape
//! leaves. Leaving puts the pan and zoom back as they were.
//!
//! [`frame`] is the one call [`crate::CraftApp::ui`] makes each frame; it also carries out the
//! enter and leave steps, so a command only has to flip [`crate::CraftApp::presenting`].

use egui::{Key, Modifiers, Ui};
use serde_json::json;

use crate::{CraftApp, ViewportCmdExt, canvas, theme};

/// What entering saved, and how many frames until the page is fitted.
#[derive(Clone, Copy)]
struct Entered {
    zoom: f32,
    pan: [f32; 2],
    /// Frames to wait before fitting: the canvas needs one frame at its new size first.
    fit_in: u8,
}

/// Per window, because every window of the app shares one egui context.
fn key(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("presentation", ctx.viewport_id()))
}

/// The page to show after `step` (-1 or 1) from `current` among `count` pages: it stops at the
/// first and last page instead of wrapping, like a slide show.
pub fn step_page(current: usize, count: usize, step: i32) -> usize {
    let last = count.saturating_sub(1);
    let next = if step < 0 { current.saturating_sub(1) } else { current.saturating_add(1) };
    next.min(last)
}

/// Draw the presentation if one is on (returning true), after carrying out entering or leaving.
pub fn frame(app: &mut CraftApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let id = key(&ctx);
    let entered: Option<Entered> = ctx.data(|d| d.get_temp(id));
    match (app.presenting, entered) {
        (true, None) => {
            app.backstage = false;
            ctx.data_mut(|d| d.insert_temp(id, Entered { zoom: app.session.view.zoom, pan: app.session.view.pan, fit_in: 1 }));
            ctx.viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            ctx.request_repaint();
        }
        (false, Some(was)) => {
            ctx.data_mut(|d| d.remove::<Entered>(id));
            ctx.viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            app.session.view.zoom = was.zoom;
            app.session.view.pan = was.pan;
            return false;
        }
        (false, None) => return false,
        (true, Some(mut was)) => {
            if was.fit_in > 0 {
                was.fit_in -= 1;
                if was.fit_in == 0 {
                    let _ = app.run("view.zoomFit", json!({}));
                }
                ctx.data_mut(|d| d.insert_temp(id, was));
                ctx.request_repaint();
            }
        }
    }
    keys(app, &ctx);
    if !app.presenting {
        return true;
    }
    let t = theme::Tokens::get(&ctx);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
        // The aids are for editing; hidden while presenting and put back for the next frame.
        let saved = app.session.view.aids;
        let aids = &mut app.session.view.aids;
        (aids.rulers, aids.grid, aids.guides, aids.page_breaks, aids.connection_points, aids.auto_connect) =
            (false, false, false, false, false, false);
        canvas::show(app, ui);
        app.session.view.aids = saved;
    });
    true
}

/// Page keys and Escape. Consumed first, so the canvas does not nudge the selection with the
/// arrows.
fn keys(app: &mut CraftApp, ctx: &egui::Context) {
    let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
    if pressed(Key::Escape) {
        app.presenting = false;
        return;
    }
    let back = pressed(Key::ArrowLeft) | pressed(Key::PageUp);
    let forward = pressed(Key::ArrowRight) | pressed(Key::PageDown);
    let step = match (back, forward) {
        (true, false) => -1,
        (false, true) => 1,
        _ => return,
    };
    let (current, count) = (app.session.doc.active_index(), app.session.doc.pages.len());
    let next = step_page(current, count, step);
    if next != current {
        let _ = app.run("page.setActive", json!({"page": next}));
        let _ = app.run("view.zoomFit", json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_stops_at_both_ends() {
        assert_eq!(step_page(0, 3, -1), 0);
        assert_eq!(step_page(0, 3, 1), 1);
        assert_eq!(step_page(2, 3, 1), 2);
        assert_eq!(step_page(1, 3, -1), 0);
        assert_eq!(step_page(0, 0, 1), 0, "no pages is not a panic");
        assert_eq!(step_page(usize::MAX, 3, 1), 2);
    }
}
