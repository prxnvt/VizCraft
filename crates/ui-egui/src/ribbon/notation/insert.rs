//! Notation > Insert: one menu per palette group of the active pack. Each item adds the shape of
//! that element type at the visible centre of the page (`shape.add` with the type's master, or
//! the master of the page's rendering variant).

use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets::{menu_button, menu_item};

/// Menus down the column, three to a column like the Process tab's stacks.
const PER_COLUMN: usize = 3;

fn menus(app: &mut CraftApp, ui: &mut egui::Ui) {
    let pal = crate::palette::for_app(app);
    // Only the pack's own groups (scoped to the viewpoint as the pane is): the connector and
    // Drawing groups belong to the palette pane.
    let groups: Vec<crate::palette::Group> =
        pal.groups().filter(|g| !["connectors", "drawing", crate::palette::MORE_CONNECTORS].contains(&g.id.as_str())).cloned().collect();
    ui.horizontal(|ui| {
        for column in groups.chunks(PER_COLUMN) {
            ui.vertical(|ui| {
                for g in column {
                    menu_button(ui, app, "shape", &g.label, &g.label, false, |ui, app| {
                        let at = crate::canvas::insert_point(app);
                        let variant = super::variant::current(app);
                        for item in &g.items {
                            if let crate::palette::What::Master(m) = item.what {
                                // The page's rendering variant may draw the type with another glyph.
                                let m = vizcraft_stencils::variant_master(m.id, &variant).unwrap_or(m);
                                let (x, y) = ((at.x - m.w / 2.0).round(), (at.y - m.h / 2.0).round());
                                menu_item(ui, app, &item.label, "shape.add", json!({"stencil": m.id, "x": x, "y": y}), None);
                            }
                        }
                    });
                }
            });
        }
    });
}

/// The notation tab's Insert group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Insert", vec![Custom(menus)])
}
