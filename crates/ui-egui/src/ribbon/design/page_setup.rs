//! Design > Page Setup: small menus for orientation, board and paper size (with the drawing
//! sheet sizes of every sheet standard), auto size, and a
//! launcher for the Page Setup dialog. Every entry runs `page.setup` (or `page.fitToDrawing`) on
//! the active page.

use egui::Ui;
use serde_json::json;
use vizcraft_diagram::{Board, Orientation};

use crate::CraftApp;
use crate::page_setup::{PRESETS, preset_of, preset_params};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};
use crate::widgets::{menu_button, menu_item};

fn orientation(app: &CraftApp) -> Orientation {
    app.session.doc.page().map_or(Orientation::Landscape, |p| if p.height > p.width { Orientation::Portrait } else { p.orientation })
}

/// The Size menu: each preset in the page's current orientation (the one matching the page is
/// checked), then Fit to Drawing. Built when drawn because the params depend on the orientation.
fn size_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "zoomFit", "Size", "Size", false, |ui, app| {
        let o = orientation(app);
        let current = app.session.doc.page().and_then(|p| preset_of(p.width, p.height));
        for (name, w, h) in PRESETS {
            menu_item(ui, app, name, "page.setup", preset_params(*w, *h, o), Some(current == Some(*name)));
        }
        ui.separator();
        sheet_sizes(app, ui, o);
        ui.separator();
        menu_item(ui, app, "Fit to Drawing", "page.fitToDrawing", json!({}), None);
        menu_item(ui, app, "More Page Sizes...", "page.setup", json!({}), None);
    });
}

/// One submenu of drawing sheet sizes per standard (`sheet.presets`), each choice running
/// `sheet.set` with the preset in the page's orientation; the page's sheet is checked.
fn sheet_sizes(app: &mut CraftApp, ui: &mut Ui, o: Orientation) {
    let orientation = if o == Orientation::Portrait { "portrait" } else { "landscape" };
    let current = app.session.doc.page().and_then(|p| p.sheet).map(|s| s.preset_id()).unwrap_or_default();
    let presets = app.session.run("sheet.presets", &json!({})).unwrap_or_default();
    for st in presets["standards"].as_array().cloned().unwrap_or_default() {
        let sid = st["id"].as_str().unwrap_or_default().to_string();
        let label = format!("{} Sheets", st["label"].as_str().unwrap_or(&sid));
        ui.menu_button(label, |ui| {
            for sz in st["sizes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let id = format!("{sid}.{}.{orientation}", sz["id"].as_str().unwrap_or_default());
                let name = sz["name"].as_str().unwrap_or_default();
                menu_item(ui, app, name, "sheet.set", json!({"sheet": id}), Some(id == current));
            }
        });
    }
}

fn board_is(app: &CraftApp, board: Board) -> bool {
    app.session.doc.page().is_some_and(|p| p.board == board)
}

fn auto_size_on(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.auto_size)
}

/// The Design tab's Page Setup group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Page Setup",
        vec![
            Stack(vec![
                Menu {
                    label: "Orientation",
                    icon: "rotateRight",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Portrait", "page.setup", json!({"orientation": "portrait"}))
                            .checked(|a| orientation(a) == Orientation::Portrait),
                        MenuEntry::new("Landscape", "page.setup", json!({"orientation": "landscape"}))
                            .checked(|a| orientation(a) == Orientation::Landscape),
                    ],
                },
                Menu {
                    label: "Board",
                    icon: "board",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Infinite", "page.setup", json!({"board": "infinite"})).checked(|a| board_is(a, Board::Infinite)),
                        MenuEntry::new("Fixed Size", "page.setup", json!({"board": "fixed"})).checked(|a| board_is(a, Board::Fixed)),
                    ],
                },
                Custom(size_menu),
            ]),
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
