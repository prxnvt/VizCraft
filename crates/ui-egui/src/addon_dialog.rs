//! The small dialogs of View > Macros: Number Shapes, Array Shapes, Move Shapes and the name
//! prompt of Save Macro. Each opens from `ui.dialog {name}` (`numberShapes`, `arrayShapes`,
//! `moveShapes`, `saveMacro`) and ends by running the engine command with its settings, so an
//! agent gets the same result without the dialog.

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::Tokens;

/// Gap, in page units, left between copies when Array Shapes opens.
const ARRAY_GAP: f64 = 20.0;

/// Which dialog is open and its editable settings.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum State {
    /// `addon.numberShapes`.
    Number {
        /// First number.
        start: i32,
        /// The number follows the text instead of leading it.
        after: bool,
        /// Text between the number and the shape's text.
        separator: String,
        /// Strip a number already there first.
        renumber: bool,
    },
    /// `addon.arrayShapes`.
    Array {
        /// Rows of copies.
        rows: u32,
        /// Columns of copies.
        cols: u32,
        /// Horizontal pitch.
        dx: f64,
        /// Vertical pitch.
        dy: f64,
    },
    /// `addon.moveShapes`.
    Move {
        /// Move by a distance and an angle instead of an offset.
        polar: bool,
        /// Offset right.
        dx: f64,
        /// Offset down.
        dy: f64,
        /// Distance.
        distance: f64,
        /// Angle in degrees, counterclockwise from right.
        angle: f64,
        /// Leave the originals and move copies.
        copy: bool,
    },
    /// `macro.save` for the script just recorded.
    SaveMacro {
        /// The name to keep it under.
        name: String,
    },
}

impl State {
    /// The dialog for `name` (`numberShapes`, `arrayShapes`, `moveShapes`, `saveMacro`), seeded
    /// from the app; `None` for any other name.
    pub fn open(name: &str, app: &CraftApp) -> Option<State> {
        Some(match name {
            "numberShapes" => State::Number { start: 1, after: false, separator: " ".into(), renumber: false },
            "arrayShapes" => {
                // A pitch that leaves a gap between the selected shape and its neighbour.
                let (w, h) = selection_size(app);
                State::Array { rows: 2, cols: 2, dx: (w + ARRAY_GAP).round(), dy: (h + ARRAY_GAP).round() }
            }
            "moveShapes" => State::Move { polar: false, dx: 20.0, dy: 20.0, distance: 50.0, angle: 0.0, copy: false },
            "saveMacro" => State::SaveMacro { name: String::new() },
            _ => return None,
        })
    }

    /// The name `ui.dialog {name}` opens it by.
    pub fn name(&self) -> &'static str {
        match self {
            State::Number { .. } => "numberShapes",
            State::Array { .. } => "arrayShapes",
            State::Move { .. } => "moveShapes",
            State::SaveMacro { .. } => "saveMacro",
        }
    }

    /// The dialog's title.
    pub fn title(&self) -> &'static str {
        match self {
            State::Number { .. } => "Number Shapes",
            State::Array { .. } => "Array Shapes",
            State::Move { .. } => "Move Shapes",
            State::SaveMacro { .. } => "Save Macro",
        }
    }

    /// The default button's text.
    pub fn ok_label(&self) -> &'static str {
        match self {
            State::SaveMacro { .. } => "Save",
            _ => "OK",
        }
    }
}

/// Width and height of the first selected shape, zero with none.
fn selection_size(app: &CraftApp) -> (f64, f64) {
    let page = app.session.doc.page();
    app.session.sel.items.iter().find_map(|id| page.and_then(|p| p.shape(id))).map_or((0.0, 0.0), |s| (s.w, s.h))
}

/// The settings above the OK and Cancel buttons.
pub fn body(ui: &mut Ui, app: &CraftApp, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    let dim = |text: &str| egui::RichText::new(text.to_string()).color(t.text_dim);
    match s {
        State::Number { start, after, separator, renumber } => {
            egui::Grid::new("number_shapes").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(dim("Start at"));
                ui.add(egui::DragValue::new(start).range(-1_000_000..=1_000_000));
                ui.end_row();
                ui.label(dim("Number goes"));
                ui.horizontal(|ui| {
                    ui.radio_value(after, false, "Before the text");
                    ui.radio_value(after, true, "After the text");
                });
                ui.end_row();
                ui.label(dim("Separator"));
                ui.add(egui::TextEdit::singleline(separator).desired_width(80.0).char_limit(16));
                ui.end_row();
            });
            ui.checkbox(renumber, "Replace numbers already there");
            ui.label(dim("Shapes are numbered from the back of the page to the front."));
        }
        State::Array { rows, cols, dx, dy } => {
            egui::Grid::new("array_shapes").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(dim("Rows"));
                ui.add(egui::DragValue::new(rows).range(1..=100));
                ui.end_row();
                ui.label(dim("Columns"));
                ui.add(egui::DragValue::new(cols).range(1..=100));
                ui.end_row();
                ui.label(dim("Horizontal pitch"));
                ui.add(egui::DragValue::new(dx).speed(1.0));
                ui.end_row();
                ui.label(dim("Vertical pitch"));
                ui.add(egui::DragValue::new(dy).speed(1.0));
                ui.end_row();
            });
            let copies = u64::from(*rows).saturating_mul(u64::from(*cols)).saturating_sub(1);
            ui.label(dim(&format!("Makes {copies} copies of the selection.")));
        }
        State::Move { polar, dx, dy, distance, angle, copy } => {
            ui.horizontal(|ui| {
                ui.radio_value(polar, false, "By an offset");
                ui.radio_value(polar, true, "By a distance and angle");
            });
            egui::Grid::new("move_shapes").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                if *polar {
                    ui.label(dim("Distance"));
                    ui.add(egui::DragValue::new(distance).speed(1.0));
                    ui.end_row();
                    ui.label(dim("Angle"));
                    ui.add(egui::DragValue::new(angle).speed(1.0).suffix(" deg"));
                    ui.end_row();
                } else {
                    ui.label(dim("Right"));
                    ui.add(egui::DragValue::new(dx).speed(1.0));
                    ui.end_row();
                    ui.label(dim("Down"));
                    ui.add(egui::DragValue::new(dy).speed(1.0));
                    ui.end_row();
                }
            });
            ui.checkbox(copy, "Move copies and leave the originals");
        }
        State::SaveMacro { name } => {
            ui.label(dim(if app.last_script.is_some() { "Name for the script you just recorded" } else { "Name for the recording in progress" }));
            ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY).hint_text("Macro name"));
        }
    }
}

/// The command and params a dialog's OK runs.
pub fn command(app: &CraftApp, s: &State) -> (&'static str, Value) {
    match s {
        State::Number { start, after, separator, renumber } => (
            "addon.numberShapes",
            json!({"start": start, "position": if *after { "after" } else { "before" }, "separator": separator, "renumber": renumber}),
        ),
        State::Array { rows, cols, dx, dy } => ("addon.arrayShapes", json!({"rows": rows, "cols": cols, "dx": dx, "dy": dy})),
        State::Move { polar, dx, dy, distance, angle, copy } => {
            if *polar {
                ("addon.moveShapes", json!({"distance": distance, "angle": angle, "copy": copy}))
            } else {
                ("addon.moveShapes", json!({"dx": dx, "dy": dy, "copy": copy}))
            }
        }
        State::SaveMacro { name } => match &app.last_script {
            Some(script) => ("macro.save", json!({"name": name, "script": script})),
            None => ("macro.save", json!({"name": name})),
        },
    }
}

/// After the buttons: on OK run the command. Returns whether to close; a name left empty keeps
/// the Save Macro dialog open.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if ok {
        if matches!(s, State::SaveMacro { name } if name.trim().is_empty()) {
            return false;
        }
        let (id, params) = command(app, s);
        if app.run(id, params).is_ok() && matches!(s, State::SaveMacro { .. }) {
            app.last_script = None;
        }
    }
    ok || cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn each_dialog_opens_by_name_and_ok_runs_its_command() {
        let mut a = app();
        let first = a.session.doc.page().unwrap().shapes[0].id.clone();
        a.run("edit.select", json!({"ids": [first]})).unwrap();
        for name in ["numberShapes", "arrayShapes", "moveShapes", "saveMacro"] {
            a.run("ui.dialog", json!({"name": name})).unwrap();
            assert_eq!(a.dialog.as_ref().map(|d| d.name()), Some(name));
        }
        let before = a.session.doc.page().unwrap().shapes.len();
        let s = State::open("arrayShapes", &a).unwrap();
        assert!(!finish(&mut a, &s, false, false));
        assert!(finish(&mut a, &s, true, false));
        assert_eq!(a.session.doc.page().unwrap().shapes.len(), before + 3, "2 by 2 makes three copies");
        let s = State::open("numberShapes", &a).unwrap();
        assert!(finish(&mut a, &s, true, false));
        assert!(State::open("nothing", &a).is_none());
    }

    #[test]
    fn save_macro_waits_for_a_name() {
        let mut a = app();
        a.last_script = Some(vec![json!({"command": "edit.selectAll", "params": {}})]);
        let empty = State::SaveMacro { name: "  ".into() };
        assert!(!finish(&mut a, &empty, true, false), "no name, the dialog stays");
        assert!(a.last_script.is_some());
    }
}
