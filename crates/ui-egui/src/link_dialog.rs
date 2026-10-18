//! The Link dialog: a shape's hyperlinks (kind, target, label) and its screen tip. It opens from
//! `link.add` with no target (Insert > Links) and ends by running one `edit.batch` of `link.*`
//! and `shape.setScreenTip` commands, so an agent gets the same result without it and the whole
//! edit is one undo step.

use egui::Ui;
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_diagram::{Hyperlink, LinkKind};

use crate::CraftApp;
use crate::theme::Tokens;

/// Longest shape list the picker offers, so a huge document cannot make the dialog crawl.
const MAX_PICKS: usize = 500;

/// The kinds in picker order, with their names.
const KINDS: [(LinkKind, &str); 5] =
    [(LinkKind::Url, "Web address"), (LinkKind::File, "File"), (LinkKind::Page, "Page"), (LinkKind::Shape, "Shape"), (LinkKind::Element, "Element")];

/// One link row being edited.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// What the target names.
    pub kind: String,
    /// The address, path or id.
    pub target: String,
    /// The text shown for the link.
    pub label: String,
}

/// The dialog's editable copy of one shape's links.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The shape being edited; `None` when nothing was selected.
    pub shape: Option<String>,
    /// How many links the shape had when the dialog opened (they are replaced on OK).
    pub original: usize,
    /// The links.
    pub rows: Vec<Row>,
    /// The screen tip.
    pub screen_tip: String,
    /// The last error from OK, shown in the dialog.
    pub error: String,
}

/// The dialog seeded from the primary selection.
pub fn open(app: &CraftApp) -> State {
    let page = app.session.doc.page();
    let shape = app.session.sel.primary.as_ref().and_then(|id| page.and_then(|p| p.shape(id)));
    match shape {
        Some(sh) => State {
            shape: Some(sh.id.clone()),
            original: sh.links.len(),
            rows: sh.links.iter().map(|l| Row { kind: l.kind.name().into(), target: l.target.clone(), label: l.label.clone() }).collect(),
            screen_tip: sh.screen_tip.clone(),
            error: String::new(),
        },
        None => State { shape: None, original: 0, rows: Vec::new(), screen_tip: String::new(), error: String::new() },
    }
}

fn kind_name(kind: &str) -> &'static str {
    KINDS.iter().find(|(k, _)| k.name() == kind).map_or(KINDS[0].1, |(_, n)| n)
}

/// The target picker or field of one row.
fn target_field(ui: &mut Ui, app: &CraftApp, i: usize, row: &mut Row) {
    let doc = &app.session.doc;
    match LinkKind::from_name(&row.kind).unwrap_or_default() {
        LinkKind::Page => {
            let shown = doc.pages.iter().find(|p| p.id == row.target).map_or("Choose a page", |p| p.name.as_str()).to_string();
            egui::ComboBox::from_id_salt(("link_page", i)).selected_text(shown).width(200.0).show_ui(ui, |ui| {
                for p in &doc.pages {
                    ui.selectable_value(&mut row.target, p.id.clone(), &p.name);
                }
            });
        }
        LinkKind::Shape => {
            let name =
                |id: &str| doc.shape(id).map(|s| if s.text.is_empty() { id.to_string() } else { format!("{} ({id})", s.text.replace('\n', " ")) });
            let shown = name(&row.target).unwrap_or_else(|| "Choose a shape".into());
            egui::ComboBox::from_id_salt(("link_shape", i)).selected_text(shown).width(200.0).show_ui(ui, |ui| {
                for s in doc.pages.iter().flat_map(|p| p.shapes.iter()).take(MAX_PICKS) {
                    let label = name(&s.id).unwrap_or_else(|| s.id.clone());
                    ui.selectable_value(&mut row.target, s.id.clone(), label);
                }
            });
        }
        LinkKind::Url | LinkKind::File | LinkKind::Element => {
            ui.add_sized([200.0, 22.0], egui::TextEdit::singleline(&mut row.target));
        }
    }
}

/// The links above the OK and Cancel buttons.
pub fn body(ui: &mut Ui, app: &CraftApp, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    if s.shape.is_none() {
        ui.label(egui::RichText::new("Select a shape first.").color(t.text_dim));
        return;
    }
    let mut remove = None;
    egui::Grid::new("link_rows").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
        for (i, row) in s.rows.iter_mut().enumerate() {
            egui::ComboBox::from_id_salt(("link_kind", i)).selected_text(kind_name(&row.kind)).width(110.0).show_ui(ui, |ui| {
                for (k, name) in KINDS {
                    if ui.selectable_label(row.kind == k.name(), name).clicked() && row.kind != k.name() {
                        row.kind = k.name().into();
                        row.target.clear();
                    }
                }
            });
            target_field(ui, app, i, row);
            ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut row.label).hint_text("Label"));
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        s.rows.remove(i);
    }
    if s.rows.len() < Hyperlink::MAX_PER_SHAPE && ui.button("Add link").clicked() {
        s.rows.push(Row { kind: LinkKind::Url.name().into(), target: String::new(), label: String::new() });
    }
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Screen tip").color(t.text_dim));
    ui.add(egui::TextEdit::multiline(&mut s.screen_tip).desired_rows(2).desired_width(f32::INFINITY));
    if !s.error.is_empty() {
        ui.label(egui::RichText::new(&s.error).color(t.red));
    }
}

/// The batch OK runs: drop the old links from the end, add the new ones, set the screen tip.
fn commands(s: &State, shape: &str) -> Vec<Value> {
    let mut steps: Vec<Value> = Vec::new();
    for i in (0..s.original).rev() {
        steps.push(json!({"command": "link.remove", "params": {"id": shape, "index": i}}));
    }
    for r in s.rows.iter().filter(|r| !r.target.trim().is_empty()) {
        steps.push(json!({"command": "link.add", "params": {"id": shape, "kind": r.kind, "target": r.target, "label": r.label}}));
    }
    steps.push(json!({"command": "shape.setScreenTip", "params": {"ids": [shape], "text": s.screen_tip}}));
    steps
}

/// After the buttons: on OK run the batch. Returns whether to close; a refused link keeps the
/// dialog open with the reason and changes nothing.
pub fn finish(app: &mut CraftApp, s: &mut State, ok: bool, cancel: bool) -> bool {
    let Some(shape) = s.shape.clone() else { return ok || cancel };
    if ok {
        match app.run("edit.batch", json!({"label": "Edit Links", "commands": commands(s, &shape)})) {
            Ok(_) => return true,
            Err(e) => {
                s.error = e;
                return false;
            }
        }
    }
    cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn ok_replaces_the_links_in_one_step_and_a_bad_link_keeps_the_dialog_open() {
        let mut app = app();
        let id = app.session.doc.page().and_then(|p| p.shapes.first()).map(|s| s.id.clone()).unwrap();
        app.run("edit.select", json!({"ids": [id]})).unwrap();
        let mut s = open(&app);
        assert_eq!(s.shape.as_deref(), Some(id.as_str()));
        s.rows.push(Row { kind: "url".into(), target: "https://example.com".into(), label: "Site".into() });
        s.screen_tip = "Hello".into();
        assert!(!finish(&mut app, &mut s, false, false));
        assert!(app.session.doc.shape(&id).unwrap().links.is_empty(), "nothing changes before OK");
        assert!(finish(&mut app, &mut s, true, false));
        let sh = app.session.doc.shape(&id).unwrap();
        assert_eq!((sh.links.len(), sh.screen_tip.as_str()), (1, "Hello"));
        let mut s = open(&app);
        s.rows[0].target = "javascript:x".into();
        assert!(!finish(&mut app, &mut s, true, false));
        assert!(!s.error.is_empty());
        assert_eq!(app.session.doc.shape(&id).unwrap().links[0].target, "https://example.com");
    }
}
