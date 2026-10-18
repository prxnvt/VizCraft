//! The Find and Replace dialog: search text, replacement, match case, whole word, shape data and
//! scope, with Find Next, Replace and Replace All. It opens from `ui.find` (Mod+F, or Find in the
//! Home tab's Editing group) and does its work through `edit.find` and `edit.replace`; each hit it
//! reaches is selected and panned to, switching page when the hit is on another one.

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::Tokens;

/// One hit of the last search.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Hit {
    /// Page id.
    pub page: String,
    /// Shape or connector id.
    pub id: String,
    /// `text` or `data:<key>`.
    pub field: String,
}

/// The dialog's fields and the last search's hits.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Find what.
    pub text: String,
    /// Replace with.
    pub with: String,
    /// Match case.
    pub match_case: bool,
    /// Whole words only.
    pub whole_word: bool,
    /// Search shape data values too.
    pub in_data: bool,
    /// Every page instead of the current one.
    pub document: bool,
    /// Hits of the last search.
    pub hits: Vec<Hit>,
    /// The hit last shown, an index into `hits`.
    pub current: Option<usize>,
    /// The params the hits were found with; a change in the fields starts a new search.
    #[serde(skip)]
    searched: Option<Value>,
    /// What the last action found or changed.
    pub message: String,
}

impl State {
    /// The `edit.find` params for the fields.
    fn params(&self) -> Value {
        json!({
            "text": self.text,
            "matchCase": self.match_case,
            "wholeWord": self.whole_word,
            "inData": self.in_data,
            "scope": if self.document { "document" } else { "page" },
        })
    }
}

/// The dialog, opened empty.
pub fn open(_: &CraftApp) -> State {
    State::default()
}

/// Run the search for the fields; `false` (with a message) when there is nothing to search for.
fn search(app: &mut CraftApp, s: &mut State) -> bool {
    if s.text.is_empty() {
        s.message = "Type the text to find.".into();
        return false;
    }
    let params = s.params();
    match app.run("edit.find", params.clone()) {
        Ok(r) => {
            s.hits = r.get("hits").and_then(Value::as_array).map(|a| a.iter().filter_map(hit_of).collect()).unwrap_or_default();
            s.searched = Some(params);
            true
        }
        Err(e) => {
            s.message = e;
            false
        }
    }
}

fn hit_of(v: &Value) -> Option<Hit> {
    let f = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Hit { page: f("page")?, id: f("id")?, field: f("field")? })
}

/// Select hit `i` and bring it into view, switching page when needed.
fn show_hit(app: &mut CraftApp, s: &mut State, i: usize) {
    let Some(h) = s.hits.get(i).cloned() else { return };
    s.current = Some(i);
    s.message = format!("{} of {}", i + 1, s.hits.len());
    if app.session.doc.page().is_some_and(|p| p.id != h.page) {
        let _ = app.run("page.setActive", json!({"page": h.page}));
    }
    // A hidden or locked item cannot be selected; it is still panned to.
    let _ = app.run("edit.select", json!({"ids": [h.id]}));
    let centre = app.session.doc.page().and_then(|p| match p.shape(&h.id) {
        Some(sh) => Some(sh.bounds().center()),
        None => p.connector(&h.id).and_then(|c| vizcraft_diagram::Rect::bounding(vizcraft_diagram::route_polyline(p, c)).map(|r| r.center())),
    });
    if let Some(c) = centre {
        // The dialog sits over the middle of the window, so the hit goes above it, near the top
        // of the canvas.
        let lift = app.session.view.canvas.map_or(0.0, |[_, h]| f64::from(h * 0.42 / app.session.view.zoom.max(0.01)));
        let _ = app.run("view.pan", json!({"x": c.x, "y": c.y + lift}));
    }
}

/// Find Next: the hit after the current one (wrapping), or the first of a new search.
pub fn find_next(app: &mut CraftApp, s: &mut State) {
    let fresh = s.searched.as_ref() != Some(&s.params());
    if fresh {
        s.current = None;
        if !search(app, s) {
            return;
        }
    }
    if s.hits.is_empty() {
        s.current = None;
        s.message = "No matches.".into();
        return;
    }
    let next = s.current.map_or(0, |c| (c + 1) % s.hits.len());
    show_hit(app, s, next);
}

/// Replace: the current hit's matches, then on to the next hit. With no current hit it finds
/// first, as Visio does.
pub fn replace(app: &mut CraftApp, s: &mut State) {
    let current = if s.searched.as_ref() == Some(&s.params()) { s.current.and_then(|i| s.hits.get(i).cloned().map(|h| (i, h))) } else { None };
    let Some((i, h)) = current else {
        find_next(app, s);
        return;
    };
    let mut p = s.params();
    p["with"] = json!(s.with);
    p["at"] = json!({"page": h.page, "id": h.id, "field": h.field});
    if let Err(e) = app.run("edit.replace", p) {
        s.message = e;
        return;
    }
    if !search(app, s) {
        return;
    }
    if s.hits.is_empty() {
        s.current = None;
        s.message = "Replaced the last match.".into();
        return;
    }
    // The replaced hit is gone (or kept its place when the replacement still matches); the next
    // one now sits at the same index.
    let still = s.hits.get(i).is_some_and(|x| *x == h);
    let next = (if still { i + 1 } else { i }) % s.hits.len();
    show_hit(app, s, next);
}

/// Replace All: every match in scope, in one undo step.
pub fn replace_all(app: &mut CraftApp, s: &mut State) {
    if s.text.is_empty() {
        s.message = "Type the text to find.".into();
        return;
    }
    let mut p = s.params();
    p["with"] = json!(s.with);
    p["all"] = json!(true);
    match app.run("edit.replace", p) {
        Ok(r) => {
            let n = r.get("replaced").and_then(Value::as_u64).unwrap_or(0);
            let skipped = r.get("skipped").and_then(Value::as_array).map_or(0, Vec::len);
            s.message = match (n, skipped) {
                (0, 0) => "No matches.".into(),
                (n, 0) => format!("Replaced {n}."),
                (n, k) => format!("Replaced {n}; {k} locked item(s) left alone."),
            };
            s.hits.clear();
            s.current = None;
            s.searched = None;
        }
        Err(e) => s.message = e,
    }
}

/// The dialog body. `enter` is Find Next. Returns true to close.
pub fn body(app: &mut CraftApp, ui: &mut Ui, s: &mut State, enter: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    egui::Grid::new("find_fields").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        ui.label(egui::RichText::new("Find what").color(t.text_dim));
        let r = ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(260.0));
        if s.text.is_empty() && s.hits.is_empty() && !r.has_focus() && s.message.is_empty() {
            r.request_focus();
        }
        ui.end_row();
        ui.label(egui::RichText::new("Replace with").color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut s.with).desired_width(260.0));
        ui.end_row();
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut s.match_case, "Match case");
        ui.checkbox(&mut s.whole_word, "Whole word");
        ui.checkbox(&mut s.in_data, "Shape data");
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Search").color(t.text_dim));
        ui.radio_value(&mut s.document, false, "Current page");
        ui.radio_value(&mut s.document, true, "All pages");
    });
    ui.label(egui::RichText::new(if s.message.is_empty() { " " } else { s.message.as_str() }).color(t.text_dim));
    ui.add_space(4.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let b = |ui: &mut Ui, label: &str| ui.add(egui::Button::new(label).min_size(vec2(84.0, 24.0))).clicked();
        if b(ui, "Replace") {
            replace(app, s);
        }
        if b(ui, "Replace All") {
            replace_all(app, s);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new("Close").min_size(vec2(72.0, 24.0))).clicked() {
                close = true;
            }
            if crate::widgets::primary_button(ui, "Find Next").clicked() || enter {
                find_next(app, s);
            }
        });
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        for (x, text) in [(0, "Pump A"), (300, "Valve")] {
            app.run("shape.add", json!({"stencil": "basic.rectangle", "x": x, "y": 0, "text": text})).unwrap();
        }
        app.run("page.add", json!({})).unwrap();
        app.run("shape.add", json!({"stencil": "basic.rectangle", "x": 500, "y": 400, "text": "pump B"})).unwrap();
        app.run("page.setActive", json!({"page": 0})).unwrap();
        app
    }

    #[test]
    fn find_next_walks_hits_across_pages_and_pans_to_them() {
        let mut app = app();
        let mut s = State { text: "pump".into(), document: true, ..State::default() };
        find_next(&mut app, &mut s);
        assert_eq!(s.hits.len(), 2);
        assert_eq!(s.message, "1 of 2");
        let first = app.session.sel.items.clone();
        assert_eq!(app.session.doc.page().unwrap().shape(&first[0]).unwrap().text, "Pump A");
        find_next(&mut app, &mut s);
        assert_eq!(app.session.doc.active_index(), 1, "the second hit switched page");
        let sh = app.session.doc.page().unwrap().shape(&app.session.sel.items[0]).unwrap().clone();
        assert_eq!(sh.text, "pump B");
        assert_eq!(app.session.view.pan, [sh.center().x as f32, sh.center().y as f32], "panned to the hit");
        find_next(&mut app, &mut s);
        assert_eq!(s.current, Some(0), "wraps to the first hit");
        s.match_case = true;
        find_next(&mut app, &mut s);
        assert_eq!(s.hits.len(), 1, "a changed option starts a new search");
        s.text = "nothing".into();
        find_next(&mut app, &mut s);
        assert_eq!(s.message, "No matches.");
    }

    #[test]
    fn replace_and_replace_all() {
        let mut app = app();
        let mut s = State { text: "pump".into(), with: "Fan".into(), document: true, ..State::default() };
        replace(&mut app, &mut s);
        assert_eq!(s.current, Some(0), "the first Replace only finds");
        replace(&mut app, &mut s);
        assert_eq!(app.session.doc.pages[0].shapes[0].text, "Fan A");
        assert_eq!(s.hits.len(), 1);
        assert_eq!(app.session.doc.active_index(), 1, "moved on to the next hit");
        app.run("edit.undo", json!({})).unwrap();
        replace_all(&mut app, &mut s);
        assert_eq!(s.message, "Replaced 2.");
        assert!(app.session.doc.pages.iter().flat_map(|p| &p.shapes).all(|x| !x.text.to_lowercase().contains("pump")));
    }

    #[test]
    fn ui_find_opens_the_dialog() {
        let mut app = app();
        app.run("ui.find", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("find"));
    }
}
