//! View > Window: a second window on the same file, tiling, cascading and switching.

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::{self, *};
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item};

/// The View tab's Window group: three icon buttons over the Switch Windows menu.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Window",
        vec![Stack(vec![
            Row(vec![
                Button { id: "ui.window.new", icon: "newWindow", label: "New Window", size: Size::Icon, params: |_| json!({"same": true}), checked: None },
                Button { id: "ui.window.arrangeAll", icon: "arrangeAll", label: "Arrange All", size: Size::Icon, params: |_| json!({}), checked: None },
                Button { id: "ui.window.cascade", icon: "cascade", label: "Cascade", size: Size::Icon, params: |_| json!({}), checked: None },
            ]),
            Control::Custom(menu),
        ])],
    )
}

/// Switch Windows: one row per open window.
fn menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "switchWindows", "Switch", "Switch Windows", false, |ui, app| {
        let titles = app.window_titles.clone();
        if titles.is_empty() {
            ui.add_enabled(false, egui::Button::new("This is the only window"));
        }
        for (i, title) in titles.iter().enumerate() {
            if ui.button(format!("{}  {}", i + 1, title)).clicked() {
                let _ = app.run("ui.window.switch", json!({"index": i}));
                ui.close();
            }
        }
        ui.separator();
        menu_item(ui, app, "Arrange All", "ui.window.arrangeAll", json!({}), None);
        menu_item(ui, app, "Cascade", "ui.window.cascade", json!({}), None);
    });
}
