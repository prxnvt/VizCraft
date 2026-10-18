//! The External Data pane: the rows of a linked table, each marked when a shape holds its key,
//! with Link to Selection, Refresh, Unlink and Link Data.
//!
//! The table is read once per link and refresh (kept in egui's frame memory, keyed by the link's
//! path and refresh time), so the pane does not reread the file every frame. Every button runs a
//! command (`datalink.bind`, `datalink.refresh`, `datalink.unlink`, `ui.linkData`).

use std::sync::Arc;

use egui::Ui;
use serde_json::json;
use vizcraft_engine::cmd::datalink::Table;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The External Data pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("externalData", "External Data", Side::Right, 320.0, external_data);

/// Most rows the pane lists; the table itself may hold more.
const MAX_SHOWN: usize = 500;

/// Which link and row the pane has picked, kept in egui memory.
#[derive(Clone, Default)]
struct Pick {
    link: Option<String>,
    row: Option<usize>,
}

fn pick_id() -> egui::Id {
    egui::Id::new("external-data-pick")
}

/// The table of `link`, read once per path and refresh time.
fn table(ui: &Ui, link: &vizcraft_diagram::data_graphic::DataLink) -> Arc<Result<Table, String>> {
    let key = egui::Id::new(("external-data-table", &link.id, &link.path, &link.last_refresh, &link.sheet));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<Arc<Result<Table, String>>>(key)) {
        return t;
    }
    let t = Arc::new(vizcraft_engine::cmd::datalink::link_table(link).map_err(|e| e.to_string()));
    ui.ctx().data_mut(|d| d.insert_temp(key, t.clone()));
    t
}

fn external_data(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut pick: Pick = ui.ctx().data(|d| d.get_temp(pick_id())).unwrap_or_default();
    let links = app.session.doc.data_links.clone();
    if links.is_empty() {
        ui.label(egui::RichText::new("No linked data. Link a CSV or Excel table to the shapes' data.").color(t.text_dim));
        if ui.button("Link Data...").clicked() {
            let _ = app.run("ui.linkData", json!({}));
        }
        return;
    }
    let link = pick.link.as_ref().and_then(|id| links.iter().find(|l| l.id == *id)).or(links.first()).cloned();
    let Some(link) = link else { return };
    if links.len() > 1 {
        egui::ComboBox::from_id_salt("external-data-link").selected_text(file_name(&link.path)).width(260.0).show_ui(ui, |ui| {
            for l in &links {
                if ui.selectable_label(l.id == link.id, file_name(&l.path)).clicked() {
                    pick = Pick { link: Some(l.id.clone()), row: None };
                }
            }
        });
    } else {
        ui.label(egui::RichText::new(file_name(&link.path)).strong());
    }
    ui.label(egui::RichText::new(format!("Key: {} = shape data {}", link.key_column, link.data_key)).small().color(t.text_dim));
    let shape = app.session.sel.primary.clone().filter(|id| app.session.doc.page().is_some_and(|p| p.shape(id).is_some()));
    ui.horizontal(|ui| {
        let can_bind = shape.is_some() && pick.row.is_some();
        if ui.add_enabled(can_bind, egui::Button::new("Link to Selection")).clicked()
            && let (Some(sh), Some(row)) = (&shape, pick.row)
        {
            let _ = app.run("datalink.bind", json!({"id": link.id, "row": row, "shape": sh}));
        }
        if ui.button("Refresh").clicked() {
            let _ = app.run("datalink.refresh", json!({"id": link.id}));
        }
        if ui.button("Unlink").clicked() {
            let _ = app.run("datalink.unlink", json!({"id": link.id}));
            pick = Pick::default();
        }
        if ui.button("Link Data...").clicked() {
            let _ = app.run("ui.linkData", json!({}));
        }
    });
    ui.add_space(4.0);
    let table = table(ui, &link);
    match table.as_ref() {
        Err(e) => {
            ui.label(egui::RichText::new(e).color(t.red));
        }
        Ok(tab) => {
            let linked = vizcraft_engine::cmd::datalink::linked_keys(&app.session.doc, &link);
            let kc = tab.column(&link.key_column);
            egui::ScrollArea::both().auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("external-data-rows").striped(true).num_columns(tab.columns.len() + 1).spacing([10.0, 2.0]).show(ui, |ui| {
                    ui.label("");
                    for c in &tab.columns {
                        ui.label(egui::RichText::new(&c.name).strong());
                    }
                    ui.end_row();
                    for (ri, row) in tab.rows.iter().enumerate().take(MAX_SHOWN) {
                        let is_linked = kc.is_some_and(|k| linked.contains(tab.cell(ri, k).trim()));
                        let mark = if is_linked { "Linked" } else { "" };
                        if ui.selectable_label(pick.row == Some(ri), egui::RichText::new(mark).small().color(t.accent)).clicked() {
                            pick.row = Some(ri);
                        }
                        for cell in row {
                            ui.label(cell.as_str());
                        }
                        ui.end_row();
                    }
                });
                if tab.rows.len() > MAX_SHOWN {
                    ui.label(egui::RichText::new(format!("{} more rows not listed", tab.rows.len() - MAX_SHOWN)).color(t.text_dim));
                }
            });
        }
    }
    pick.link = Some(link.id.clone());
    ui.ctx().data_mut(|d| d.insert_temp(pick_id(), pick));
}

/// The last part of a path, for the header.
fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}
