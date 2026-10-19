//! The Nets tab: every net of the model with its number, name, kind and pin count (what
//! `net.list` reports), the members of the selected net (`net.members`), Go to, which selects
//! the net's wires on its page, and the number edited in place through `net.set`.
//!
//! The rows come from the session's net index (`Session.nets`, rebuilt by the sync pass after
//! every change), so a frame runs no command. The tab keeps only which net is selected and the
//! number being typed.

use egui::{RichText, Ui};
use serde_json::{Value, json};
use vizcraft_engine::Session;

use super::DockTab;
use crate::CraftApp;
use crate::theme::Tokens;

/// The Nets tab's state: the selected net and the number being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetsState {
    /// The net whose members show at the right; empty for none.
    pub selected: String,
    /// The net whose number is being typed, and the text so far.
    edit: Option<(String, String)>,
}

/// One net as the tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The net's id.
    pub id: String,
    /// Its number (`101`), empty for none.
    pub number: String,
    /// Its name (a potential such as `L1`), empty for none.
    pub name: String,
    /// `wire`, `potential`, `pipe`, `fluid` or `signal`.
    pub kind: String,
    /// How many pins its segments are glued to.
    pub pins: usize,
    /// How many segments (connectors) it has.
    pub segments: usize,
    /// The pages it is drawn on, by id.
    pub pages: Vec<String>,
}

/// Every net of the model, in model order.
pub fn rows(session: &Session) -> Vec<Row> {
    let ix = &session.nets;
    session
        .doc
        .model
        .nets
        .iter()
        .map(|n| {
            let e = ix.nets.get(&n.id);
            Row {
                id: n.id.clone(),
                number: n.number.clone(),
                name: n.name.clone(),
                kind: n.kind.name().to_string(),
                pins: e.map_or(0, |e| e.pins.len()),
                segments: e.map_or(0, |e| e.segments.len()),
                pages: e.map(|e| e.pages.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

/// The members of net `id` as `net.members` gives them: segments, pins and junctions.
pub fn members(session: &Session, id: &str) -> Value {
    let e = session.nets.nets.get(id).cloned().unwrap_or_default();
    let pins: Vec<Value> = e.pins.iter().map(|(s, p)| json!({"shape": s, "point": p})).collect();
    json!({"net": id, "segments": e.segments, "pins": pins, "junctions": e.junctions})
}

/// The page to show net `id` on (the active page when the net is drawn there, else its first
/// page) and the items of the net on that page: its segments and junctions.
pub fn goto_target(session: &Session, id: &str) -> Option<(String, Vec<String>)> {
    let e = session.nets.nets.get(id)?;
    let active = session.doc.page().map(|p| p.id.clone()).unwrap_or_default();
    let page_id = if e.pages.contains(&active) { active } else { e.pages.first()?.clone() };
    let page = session.doc.pages.iter().find(|p| p.id == page_id)?;
    let items: Vec<String> = e.segments.iter().chain(e.junctions.iter()).filter(|i| page.contains_id(i)).cloned().collect();
    (!items.is_empty()).then_some((page_id, items))
}

/// Go to net `id`: switch to its page and select its wires there.
pub(crate) fn goto(app: &mut CraftApp, id: &str) {
    let Some((page, items)) = goto_target(&app.session, id) else {
        app.status(format!("Net {id} is not drawn on any page"));
        return;
    };
    if app.session.doc.page().is_none_or(|p| p.id != page) {
        let _ = app.run("page.setActive", json!({"page": page}));
    }
    let _ = app.run("edit.select", json!({"ids": items}));
}

/// The name a pin's shape goes by in the member list: its text, else its name, else its id.
fn shape_label(session: &Session, id: &str) -> String {
    session
        .doc
        .pages
        .iter()
        .find_map(|p| p.shape(id))
        .map(|s| {
            let text = s.text.lines().next().unwrap_or("").trim();
            if !text.is_empty() {
                text.to_string()
            } else if !s.name.is_empty() {
                s.name.clone()
            } else {
                s.id.clone()
            }
        })
        .unwrap_or_else(|| id.to_string())
}

/// Draw the tab.
pub(crate) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let list = rows(&app.session);
    if list.is_empty() {
        ui.label(RichText::new("No nets yet. Draw a wire with the Wire tool (W) or assign connectors to a net.").color(t.text_dim));
        return;
    }
    if !list.iter().any(|r| r.id == app.ui.dock.state.nets.selected) {
        app.ui.dock.state.nets.selected.clear();
    }
    let mut runs: Vec<(&'static str, Value)> = Vec::new();
    let mut go: Option<String> = None;
    ui.columns(2, |cols| {
        let [left, right] = cols else { return };
        egui::ScrollArea::vertical().id_salt("nets-list").auto_shrink([false, false]).show(left, |ui| {
            egui::Grid::new("nets-grid").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                for h in ["Number", "Name", "Kind", "Pins", "Segments", ""] {
                    ui.label(RichText::new(h).small().color(t.text_dim));
                }
                ui.end_row();
                for r in &list {
                    number_cell(app, ui, r, &mut runs);
                    let on = app.ui.dock.state.nets.selected == r.id;
                    let name = if r.name.is_empty() { r.id.as_str() } else { r.name.as_str() };
                    if ui.selectable_label(on, name).on_hover_text(format!("Net {}", r.id)).clicked() {
                        app.ui.dock.state.nets.selected = r.id.clone();
                    }
                    ui.label(&r.kind);
                    ui.label(r.pins.to_string());
                    ui.label(r.segments.to_string());
                    let drawn = !r.pages.is_empty();
                    let b = ui.add_enabled(drawn, egui::Button::new("Go to").small());
                    let b = if drawn { b.on_hover_text("Select this net's wires on its page") } else { b.on_disabled_hover_text("Drawn on no page") };
                    if b.clicked() {
                        app.ui.dock.state.nets.selected = r.id.clone();
                        go = Some(r.id.clone());
                    }
                    ui.end_row();
                }
            });
        });
        members_panel(app, right, &t);
    });
    for (id, p) in runs {
        let _ = app.run(id, p);
    }
    if let Some(id) = go {
        goto(app, &id);
    }
}

/// The number column: a click starts editing; Enter or leaving the field commits through
/// `net.set`, Escape cancels.
fn number_cell(app: &mut CraftApp, ui: &mut Ui, r: &Row, runs: &mut Vec<(&'static str, Value)>) {
    let st = &mut app.ui.dock.state.nets;
    match st.edit.as_mut().filter(|(id, _)| *id == r.id) {
        Some((_, text)) => {
            let resp = ui.add(egui::TextEdit::singleline(text).desired_width(60.0).id_salt(("net-number", &r.id)));
            resp.request_focus();
            let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if escape {
                st.edit = None;
            } else if enter || resp.lost_focus() {
                let text = text.trim().to_string();
                st.edit = None;
                if text != r.number {
                    runs.push(("net.set", json!({"net": r.id, "number": text})));
                }
            }
        }
        None => {
            let shown = if r.number.is_empty() { "-" } else { r.number.as_str() };
            let resp = ui.add(egui::Button::new(shown).frame(false).min_size(egui::vec2(60.0, 0.0)));
            let resp = resp.on_hover_text("Click to change the net number");
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Number of net {}", r.id)));
            if resp.clicked() {
                st.edit = Some((r.id.clone(), r.number.clone()));
                st.selected = r.id.clone();
            }
        }
    }
}

/// The selected net's members, or a hint.
fn members_panel(app: &CraftApp, ui: &mut Ui, t: &Tokens) {
    let id = app.ui.dock.state.nets.selected.clone();
    if id.is_empty() {
        ui.label(RichText::new("Select a net to see its members.").color(t.text_dim));
        return;
    }
    let m = members(&app.session, &id);
    let list = |k: &str| m[k].as_array().cloned().unwrap_or_default();
    egui::ScrollArea::vertical().id_salt("nets-members").auto_shrink([false, false]).show(ui, |ui| {
        ui.label(RichText::new(format!("Members of {id}")).strong());
        let pins = list("pins");
        ui.label(RichText::new(format!("Pins ({})", pins.len())).small().color(t.text_dim));
        for p in &pins {
            let shape = p["shape"].as_str().unwrap_or_default();
            ui.label(format!("{} pin {}", shape_label(&app.session, shape), p["point"].as_u64().unwrap_or(0) + 1));
        }
        for (k, title) in [("segments", "Segments"), ("junctions", "Junctions")] {
            let items = list(k);
            ui.label(RichText::new(format!("{title} ({})", items.len())).small().color(t.text_dim));
            let text: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if !text.is_empty() {
                ui.label(RichText::new(text.join(", ")).monospace());
            }
        }
    });
}

/// The tab as `ui.inspect` and `ui.nets` report it.
pub fn inspect(app: &CraftApp) -> Value {
    let st = &app.ui.dock.state.nets;
    let list: Vec<Value> = rows(&app.session)
        .iter()
        .map(|r| json!({"id": r.id, "number": r.number, "name": r.name, "kind": r.kind, "pins": r.pins, "segments": r.segments}))
        .collect();
    let members = (!st.selected.is_empty()).then(|| members(&app.session, &st.selected));
    json!({"selected": st.selected, "editing": st.edit.as_ref().map(|(id, _)| id), "nets": list, "members": members})
}

/// `ui.nets {net?, goto?: bool}`: open the dock on the Nets tab, select `net` there (and with
/// `goto` select its wires on its page); the reply is the tab as `ui.inspect` reports it. An
/// unknown net is an error and changes nothing.
pub fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    match p.get("net") {
        None | Some(Value::Null) => {}
        Some(Value::String(id)) => {
            if app.session.doc.model.net(id).is_none() {
                return Err(format!("no net `{}`", id.chars().take(64).collect::<String>()));
            }
            app.ui.dock.state.nets.selected = id.clone();
            if p.get("goto").and_then(Value::as_bool).unwrap_or(false) {
                goto(app, id);
            }
        }
        Some(_) => return Err("`net` must be a net id".into()),
    }
    super::open_on(app, DockTab::Nets);
    Ok(inspect(app))
}
