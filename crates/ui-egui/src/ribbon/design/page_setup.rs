//! Design > Page Setup: orientation, paper size, auto size, and a launcher for the Page Setup
//! dialog. Every entry runs `page.setup` (or `page.fitToDrawing`) on the active page.

use egui::Ui;
use serde_json::json;
use vizcraft_diagram::Orientation;

use crate::CraftApp;
use crate::page_setup::{PRESETS, preset_of, preset_params};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};
use crate::widgets::{big_label, menu_button, menu_item};

fn orientation(app: &CraftApp) -> Orientation {
    app.session.doc.page().map_or(Orientation::Landscape, |p| if p.height > p.width { Orientation::Portrait } else { p.orientation })
}

/// The Size menu: each preset in the page's current orientation (the one matching the page is
/// checked), then Fit to Drawing. Built when
/// drawn because the params depend on the orientation.
fn size_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "zoomFit", &big_label("Size"), "Size", true, |ui, app| {
        let o = orientation(app);
        let current = app.session.doc.page().and_then(|p| preset_of(p.width, p.height));
        for (name, w, h) in PRESETS {
            menu_item(ui, app, name, "page.setup", preset_params(*w, *h, o), Some(current == Some(*name)));
        }
        ui.separator();
        menu_item(ui, app, "Fit to Drawing", "page.fitToDrawing", json!({}), None);
        menu_item(ui, app, "More Page Sizes...", "page.setup", json!({}), None);
    });
}

fn auto_size_on(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.auto_size)
}

/// The Design tab's Page Setup group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Page Setup",
        vec![
            Menu {
                label: "Orientation",
                icon: "rotateRight",
                big: true,
                menu: vec![
                    MenuEntry::new("Portrait", "page.setup", json!({"orientation": "portrait"})).checked(|a| orientation(a) == Orientation::Portrait),
                    MenuEntry::new("Landscape", "page.setup", json!({"orientation": "landscape"})).checked(|a| orientation(a) == Orientation::Landscape),
                ],
            },
            Custom(size_menu),
            Stack(vec![
                Button {
                    id: "page.setup",
                    icon: "zoomActual",
                    label: "Auto Size",
                    size: Size::Small,
                    params: |app| json!({"autoSize": !auto_size_on(app)}),
                    checked: Some(auto_size_on),
                },
                Button { id: "page.setup", icon: "settings", label: "Page Setup...", size: Size::Small, params: |_| json!({}), checked: None },
            ]),
        ],
    )
}
