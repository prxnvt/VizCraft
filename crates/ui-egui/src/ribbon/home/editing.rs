//! Home > Editing: Find, the Select menu (select all, select by type, lasso and area select),
//! the Layers menu and the Change Shape gallery.

use egui::{Rect, Sense, vec2};
use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};
use crate::theme::Tokens;
use crate::widgets::{big_label, menu_button};
use crate::{CraftApp, icons};

/// The Home tab's Editing group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Editing",
        vec![
            Stack(vec![
                Small { id: "ui.find", icon: "find" },
                Menu {
                    label: "Select",
                    icon: "pointer",
                    big: false,
                    menu: vec![
                        MenuEntry::new("Select All", "edit.selectAll", json!({})),
                        MenuEntry::new("Select Shapes", "edit.selectByType", json!({"kind": "shapes"})),
                        MenuEntry::new("Select Connectors", "edit.selectByType", json!({"kind": "connectors"})),
                        MenuEntry::new("Select Groups", "edit.selectByType", json!({"kind": "groups"})),
                        MenuEntry::new("Select Text", "edit.selectByType", json!({"kind": "text"})),
                        MenuEntry::new("Lasso Select", "ui.tool", json!({"tool": "lasso"})),
                        MenuEntry::new("Area Select", "ui.tool", json!({"tool": "area"})),
                    ],
                },
                super::layers::control(),
            ]),
            Custom(change_shape),
        ],
    )
}

/// Side of one master cell in the Change Shape gallery.
const CELL: f32 = 30.0;
/// Masters per gallery row.
const COLS: usize = 8;

/// The Change Shape gallery: every stencil set's masters, drawn as their outlines; a click runs
/// `edit.changeShape` on the selected shapes.
fn change_shape(app: &mut CraftApp, ui: &mut egui::Ui) {
    let on = app.session.doc.page().is_some_and(|p| app.session.sel.items.iter().any(|id| p.shape(id).is_some()));
    ui.add_enabled_ui(on, |ui| {
        menu_button(ui, app, "shape", &big_label("Change Shape"), "Change Shape", true, |ui, app| {
            let t = Tokens::get(ui.ctx());
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for set in vizcraft_stencils::sets() {
                    ui.label(egui::RichText::new(set.name).small().weak());
                    for row in set.masters.chunks(COLS) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            for m in row {
                                let (r, resp) = ui.allocate_exact_size(vec2(CELL, CELL), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, 3.0, t.hover);
                                }
                                icons::paint_master(ui.painter(), Rect::from_center_size(r.center(), vec2(20.0, 20.0)), m, t.icon, t.accent);
                                let resp = resp.on_hover_text(m.label);
                                if resp.clicked() {
                                    let _ = app.run("edit.changeShape", json!({"stencil": m.id}));
                                    ui.close();
                                }
                            }
                        });
                    }
                }
            });
        });
    });
}
