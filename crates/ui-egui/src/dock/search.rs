//! The Search tab: `edit.find` results as rows. A click selects the hit on its page; Replace opens
//! the Find dialog with the same text.

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};

use super::DockTab;
use crate::CraftApp;
use crate::theme::Tokens;

/// Most hits the tab lists, so a one-letter query on a huge document stays drawable.
const MAX_HITS: usize = 500;

/// One search hit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// Page id.
    pub page: String,
    /// Shape or connector id.
    pub id: String,
    /// `text` or `data:<key>`.
    pub field: String,
    /// The text that matched.
    pub value: String,
}

fn hit_of(v: &Value) -> Option<Hit> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Hit { page: s("page")?, id: s("id")?, field: s("field")?, value: s("value").unwrap_or_default() })
}

/// Run the search for the field and scope, unless the hits already belong to this query, scope and
/// revision.
pub(crate) fn refresh(app: &mut CraftApp) {
    let (q, doc, rev) = (app.ui.dock.state.query.clone(), app.ui.dock.state.document, app.session.rev());
    if app.ui.dock.state.searched.as_ref() == Some(&(q.clone(), doc, rev)) {
        return;
    }
    let hits = if q.is_empty() {
        Vec::new()
    } else {
        let params = json!({"text": q, "scope": if doc { "document" } else { "page" }, "inData": true});
        match app.run("edit.find", params) {
            Ok(r) => r.get("hits").and_then(Value::as_array).map(|a| a.iter().filter_map(hit_of).take(MAX_HITS).collect()).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    };
    app.ui.dock.state.hits = hits;
    app.ui.dock.state.searched = Some((q, doc, rev));
}

/// `ui.search {query, scope?}`: show the Search tab for the query and report the hits.
pub(super) fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    if let Some(q) = p.get("query").and_then(Value::as_str) {
        app.ui.dock.state.query = q.chars().take(200).collect();
    }
    match p.get("scope").and_then(Value::as_str) {
        None => {}
        Some("page") => app.ui.dock.state.document = false,
        Some("document") => app.ui.dock.state.document = true,
        Some(other) => return Err(format!("unknown search scope {other:?} (page or document)")),
    }
    super::open_on(app, DockTab::Search);
    refresh(app);
    let s = &app.ui.dock.state;
    Ok(json!({"query": s.query, "scope": if s.document { "document" } else { "page" }, "hits": s.hits}))
}

/// Go to the hit's page and select its item.
fn select_hit(app: &mut CraftApp, hit: &Hit) {
    let _ = app.run("page.setActive", json!({"page": hit.page}));
    let _ = app.run("edit.select", json!({"ids": [hit.id]}));
}

/// The Search tab.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let s = &mut app.ui.dock.state;
        ui.add(egui::TextEdit::singleline(&mut s.query).hint_text("Search text and shape data").desired_width(260.0));
        ui.selectable_value(&mut s.document, false, "Page");
        ui.selectable_value(&mut s.document, true, "Document");
        if ui.button("Replace").clicked() {
            let (q, doc) = (s.query.clone(), s.document);
            app.dialog = crate::dialogs::Dialog::open("find", app);
            if let Some(crate::dialogs::Dialog::Find(f)) = &mut app.dialog {
                f.text = q;
                f.document = doc;
            }
        }
        let n = app.ui.dock.state.hits.len();
        ui.label(egui::RichText::new(format!("{n} {}", if n == 1 { "hit" } else { "hits" })).color(t.text_dim));
    });
    refresh(app);
    let hits = app.ui.dock.state.hits.clone();
    if hits.is_empty() {
        ui.add_space(10.0);
        let msg = if app.ui.dock.state.query.is_empty() { "Type to search the text and data of shapes and connectors" } else { "No matches" };
        ui.label(egui::RichText::new(msg).color(t.text_dim));
        return;
    }
    let mut picked = None;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("search_grid").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for h in ["Page", "Item", "Text", "Data key"] {
                ui.label(egui::RichText::new(h).small().color(t.text_dim));
            }
            ui.end_row();
            for hit in &hits {
                let page = app.session.doc.pages.iter().find(|p| p.id == hit.page).map(|p| p.name.clone()).unwrap_or_default();
                ui.label(page);
                ui.label(&hit.id);
                let clicked = ui.scope(|ui| {
                    ui.set_max_width(360.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.selectable_label(false, &hit.value).clicked()
                });
                if clicked.inner {
                    picked = Some(hit.clone());
                }
                ui.label(hit.field.strip_prefix("data:").unwrap_or(""));
                ui.end_row();
            }
        });
    });
    if let Some(h) = picked {
        select_hit(app, &h);
    }
}
