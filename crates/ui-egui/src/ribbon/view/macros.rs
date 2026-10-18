//! View > Macros: record, save, run and delete macros, and the add-ons.

use serde_json::json;
use vizcraft_engine::cmd::r#macro;

use crate::CraftApp;
use crate::ribbon::Control::{self, *};
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item, toggle};

/// The View tab's Macros group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Macros",
        vec![Stack(vec![
            Row(vec![
                Control::Custom(record),
                Button { id: "ui.dialog", icon: "macroSave", label: "Save Macro", size: Size::Icon, params: |_| json!({"name": "saveMacro"}), checked: None },
            ]),
            Row(vec![Control::Custom(run_menu), Control::Custom(delete_menu), Control::Custom(add_ons)]),
        ])],
    )
}

/// Record and Stop in one button: checked and reading Stop while a script is being recorded.
fn record(app: &mut CraftApp, ui: &mut egui::Ui) {
    let on = app.session.is_recording();
    let (icon, label) = if on { ("macroStop", "Stop Recording") } else { ("macroRecord", "Record Macro") };
    toggle(ui, app, icon, None, label, "ui.macroRecord", json!({}), on);
}

/// The saved macros as a menu that runs the one chosen.
fn run_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "macroRun", "", "Run Macro", false, |ui, app| {
        let names = r#macro::names();
        if names.is_empty() {
            ui.add_enabled(false, egui::Button::new("No saved macros"));
        }
        for name in names {
            if ui.button(&name).clicked() {
                let _ = app.run("macro.run", json!({"name": name}));
                ui.close();
            }
        }
    });
}

/// The saved macros as a menu that deletes the one chosen.
fn delete_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "macroDelete", "", "Delete Macro", false, |ui, app| {
        let names = r#macro::names();
        if names.is_empty() {
            ui.add_enabled(false, egui::Button::new("No saved macros"));
        }
        for name in names {
            if ui.button(&name).clicked() {
                let _ = app.run("macro.delete", json!({"name": name}));
                ui.close();
            }
        }
    });
}

/// Add-Ons: the dialogs that number, array and move the selected shapes.
fn add_ons(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "addOns", "", "Add-Ons", false, |ui, app| {
        menu_item(ui, app, "Number Shapes...", "ui.dialog", json!({"name": "numberShapes"}), None);
        menu_item(ui, app, "Array Shapes...", "ui.dialog", json!({"name": "arrayShapes"}), None);
        menu_item(ui, app, "Move Shapes...", "ui.dialog", json!({"name": "moveShapes"}), None);
    });
}
