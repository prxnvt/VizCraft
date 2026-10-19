//! Sheet > Sheet: the active page's drawing sheet. Standard (ISO 5457 or ASME Y14.1, or none)
//! and Size menus built from `sheet.presets` (ISO A3 first, every ASME size offered, both
//! orientations), and Title Block (`sheet.titleBlock`: ISO 7200, ASME or none; Redraw Frame runs
//! `sheet.frames`).

use egui::Ui;
use serde_json::{Value, json};
use vizcraft_diagram::Orientation;
use vizcraft_diagram::sheet::{Sheet, SheetStandard, TitleBlockKind};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets::{menu_button, menu_item};

/// The active page's sheet.
fn sheet(app: &CraftApp) -> Option<Sheet> {
    app.session.doc.page().and_then(|p| p.sheet)
}

/// `sheet.presets`, read when a menu opens (the reply is the standards and their sizes).
fn presets(app: &mut CraftApp) -> Value {
    app.session.run("sheet.presets", &json!({})).unwrap_or(Value::Null)
}

/// The Standard menu: one row per standard of `sheet.presets` (switching keeps the page's
/// orientation and takes the standard's usual size), then None, which removes the sheet.
fn standard_menu(app: &mut CraftApp, ui: &mut Ui) {
    let label = sheet(app).map_or("Standard", |s| match s.standard {
        SheetStandard::Iso5457 => "ISO",
        SheetStandard::AsmeY14_1 => "ASME",
    });
    menu_button(ui, app, "board", label, "Sheet Standard", false, |ui, app| {
        let current = sheet(app).map(|s| s.standard);
        for st in presets(app)["standards"].as_array().cloned().unwrap_or_default() {
            let id = st["id"].as_str().unwrap_or_default();
            let on = current.is_some_and(|c| c.short() == id);
            let text = st["label"].as_str().unwrap_or(id).to_string();
            menu_item(ui, app, &text, "sheet.set", json!({"standard": id}), Some(on));
        }
        ui.separator();
        menu_item(ui, app, "No Sheet", "sheet.set", json!({"off": true}), Some(current.is_none()));
    });
}

/// The Size menu: the sizes of the page's standard (ISO when the page has no sheet yet) in its
/// orientation, then the other standard's sizes, then the two orientations.
fn size_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "pageWidth", "Size", "Sheet Size", false, |ui, app| {
        let cur = sheet(app);
        let orientation = cur.map_or("landscape", |s| if s.orientation == Orientation::Portrait { "portrait" } else { "landscape" });
        let current = cur.map(|s| s.preset_id()).unwrap_or_default();
        let mine = cur.map_or(SheetStandard::Iso5457, |s| s.standard).short();
        let mut standards = presets(app)["standards"].as_array().cloned().unwrap_or_default();
        // The page's own standard first; every other one stays offered below it.
        standards.sort_by_key(|s| s["id"] != mine);
        for (i, st) in standards.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let sid = st["id"].as_str().unwrap_or_default();
            ui.label(egui::RichText::new(st["label"].as_str().unwrap_or(sid)).small());
            for sz in st["sizes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let id = format!("{sid}.{}.{orientation}", sz["id"].as_str().unwrap_or_default());
                let unit = sz["unit"].as_str().unwrap_or_default();
                let dim = |k: &str| sz[k].as_f64().unwrap_or(0.0);
                let text = format!("{}  ({} x {} {unit})", sz["name"].as_str().unwrap_or_default(), dim("width"), dim("height"));
                menu_item(ui, app, &text, "sheet.set", json!({"sheet": id}), Some(id == current));
            }
        }
        ui.separator();
        for o in ["landscape", "portrait"] {
            let label = if o == "landscape" { "Landscape" } else { "Portrait" };
            menu_item(ui, app, label, "sheet.set", json!({"orientation": o}), Some(cur.is_some() && orientation == o));
        }
    });
}

fn title_block_is(app: &CraftApp, kind: TitleBlockKind) -> bool {
    sheet(app).is_some_and(|s| s.title_block == kind)
}

/// The Sheet tab's Sheet group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Sheet",
        vec![
            Stack(vec![
                Custom(standard_menu),
                Custom(size_menu),
                Menu {
                    label: "Title Block",
                    icon: "headingStyle",
                    big: false,
                    menu: vec![
                        crate::ribbon::MenuEntry::new("ISO 7200", "sheet.titleBlock", json!({"kind": "iso7200"}))
                            .checked(|a| title_block_is(a, TitleBlockKind::Iso7200)),
                        crate::ribbon::MenuEntry::new("ASME Y14.1", "sheet.titleBlock", json!({"kind": "asme"}))
                            .checked(|a| title_block_is(a, TitleBlockKind::Asme)),
                        crate::ribbon::MenuEntry::new("None", "sheet.titleBlock", json!({"kind": "none"}))
                            .checked(|a| title_block_is(a, TitleBlockKind::None)),
                        crate::ribbon::MenuEntry::new("Redraw Frame", "sheet.frames", json!({})),
                    ],
                },
            ]),
        ],
    )
}
