//! The Snap and Glue dialog: snapping on or off, what items snap to, and how strongly. It opens
//! from `view.snapSettings` (the View tab's Visual Aids launcher) and ends by running
//! `view.snap` with every setting, so an agent gets the same result without it.

use egui::Ui;
use serde::Serialize;
use serde_json::json;
use vizcraft_engine::cmd::view::{Aids, MAX_SNAP_STRENGTH, MIN_SNAP_STRENGTH};

use crate::CraftApp;
use crate::theme::Tokens;

/// The dialog's editable copy of the snap settings.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Snapping as a whole.
    pub snap: bool,
    /// Snap to the grid.
    pub grid: bool,
    /// Snap to guides.
    pub guides: bool,
    /// Snap to shape edges and centres.
    pub shapes: bool,
    /// Snap to connection points.
    pub points: bool,
    /// Reach in screen points.
    pub strength: f32,
}

impl State {
    /// Seeded from the view's current settings.
    pub fn from_aids(a: &Aids) -> Self {
        State { snap: a.snap, grid: a.snap_grid, guides: a.snap_guides, shapes: a.snap_shapes, points: a.snap_points, strength: a.snap_strength }
    }
}

/// The dialog seeded from `app`.
pub fn open(app: &CraftApp) -> State {
    State::from_aids(&app.session.view.aids)
}

/// The settings above the OK and Cancel buttons.
pub fn body(ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    ui.checkbox(&mut s.snap, "Snap");
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Snap to").color(t.text_dim));
    ui.add_enabled_ui(s.snap, |ui| {
        ui.indent("snap_to", |ui| {
            ui.checkbox(&mut s.grid, "Grid");
            ui.checkbox(&mut s.guides, "Guides");
            ui.checkbox(&mut s.shapes, "Shape edges and centres");
            ui.checkbox(&mut s.points, "Connection points");
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Snap strength").color(t.text_dim));
        crate::widgets::slider(ui, egui::Slider::new(&mut s.strength, MIN_SNAP_STRENGTH..=MAX_SNAP_STRENGTH).suffix(" pt"), 320.0);
    });
}

/// After the buttons: on OK run `view.snap` with every setting. Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok {
        let _ = app.run(
            "view.snap",
            json!({"value": s.snap, "grid": s.grid, "guides": s.guides, "shapes": s.shapes, "points": s.points, "strength": s.strength}),
        );
    }
    ok || cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_applies_every_setting() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let mut s = open(&app);
        assert_eq!(s, State::from_aids(&Aids::default()));
        s.grid = false;
        s.strength = 20.0;
        assert!(!finish(&mut app, &s, false, false));
        assert!(app.session.view.aids.snap_grid, "nothing changes before OK");
        assert!(finish(&mut app, &s, true, false));
        assert!(!app.session.view.aids.snap_grid);
        assert_eq!(app.session.view.aids.snap_strength, 20.0);
        assert!(app.session.view.aids.snap, "OK with snapping on leaves it on");
    }

    #[test]
    fn the_command_opens_the_dialog() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        app.run("view.snapSettings", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("snapSettings"));
    }
}
