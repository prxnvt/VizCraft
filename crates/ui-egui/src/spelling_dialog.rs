//! The Spelling dialog: the next word the document's spelling check does not know, its
//! suggestions, and Ignore, Ignore All, Change and Change All. It opens from `ui.spelling` (F7, or
//! Spelling in the Review tab) and does its work through `proof.check`, `proof.ignore` and the
//! text commands; each word it reaches is selected and panned to, switching page when needed.
//!
//! A change rewrites the whole text of the field the word is in (`shape.setText`,
//! `connector.setText` or `compartment.set`), so it is one undo step and a shape that draws a
//! model element renames the element, as typing the text would.

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::{Tokens, semibold};

/// Most words Change All replaces in one go, so a replacement that is itself misspelled cannot
/// loop for ever.
const MAX_CHANGE_ALL: usize = 2000;

/// One word the check did not know.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Item {
    /// Page id.
    pub page: String,
    /// Shape or connector id.
    pub id: String,
    /// `shape` or `connector`.
    pub kind: String,
    /// `text`, `title:<compartment>`, `body:<compartment>` or `row:<compartment>:<n>`.
    pub field: String,
    /// The word as written.
    pub word: String,
    /// Offset of the first char in the field, in chars.
    pub start: usize,
    /// Offset after the last char.
    pub end: usize,
    /// Suggested replacements, best first.
    pub suggestions: Vec<String>,
}

/// The dialog's words and fields.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The words found, in document order.
    pub items: Vec<Item>,
    /// The word showing, an index into `items`.
    pub current: usize,
    /// Change to.
    pub replacement: String,
    /// What the last action did.
    pub message: String,
    /// Words skipped with Ignore, as `page/id/field/start`; a new check leaves them out.
    #[serde(skip)]
    skipped: Vec<String>,
    /// The first check has not run yet; the dialog is built from a shared reference to the app,
    /// so the body runs it on the first frame.
    #[serde(skip)]
    fresh: bool,
}

fn item_of(v: &Value) -> Option<Item> {
    let f = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let n = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as usize);
    Some(Item {
        page: f("page")?,
        id: f("id")?,
        kind: f("kind")?,
        field: f("field")?,
        word: f("word")?,
        start: n("start")?,
        end: n("end")?,
        suggestions: v
            .get("suggestions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
    })
}

fn skip_key(i: &Item) -> String {
    format!("{}/{}/{}/{}", i.page, i.id, i.field, i.start)
}

/// The dialog, before its first check (see [`start`]).
pub fn open(_: &CraftApp) -> State {
    State { fresh: true, ..State::default() }
}

/// Run the first check over the document and show its first unknown word.
pub fn start(app: &mut CraftApp, s: &mut State) {
    s.fresh = false;
    refresh(app, s);
    if s.items.is_empty() && s.message.is_empty() {
        s.message = "No spelling errors found.".into();
    }
    show_current(app, s);
}

/// Run the check over the whole document and keep the words not skipped.
fn refresh(app: &mut CraftApp, s: &mut State) {
    match app.run("proof.check", json!({"all": true})) {
        Ok(r) => {
            s.items = r.get("misspellings").and_then(Value::as_array).map(|a| a.iter().filter_map(item_of).collect()).unwrap_or_default();
            s.items.retain(|i| !s.skipped.contains(&skip_key(i)));
            let notes = r.get("notes").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("");
            if !notes.is_empty() && s.items.is_empty() {
                s.message = notes.to_string();
            }
        }
        Err(e) => {
            s.items.clear();
            s.message = e;
        }
    }
    s.current = s.current.min(s.items.len().saturating_sub(1));
}

/// Select the current word's item, pan to it and fill Change to with the best suggestion.
fn show_current(app: &mut CraftApp, s: &mut State) {
    let Some(item) = s.items.get(s.current).cloned() else {
        s.replacement.clear();
        return;
    };
    s.replacement = item.suggestions.first().cloned().unwrap_or_else(|| item.word.clone());
    if app.session.doc.page().is_some_and(|p| p.id != item.page) {
        let _ = app.run("page.setActive", json!({"page": item.page}));
    }
    // A hidden or locked item cannot be selected; it is still panned to.
    let _ = app.run("edit.select", json!({"ids": [item.id]}));
    let centre = app.session.doc.page().and_then(|p| match p.shape(&item.id) {
        Some(sh) => Some(sh.bounds().center()),
        None => p.connector(&item.id).and_then(|c| vizcraft_diagram::Rect::bounding(vizcraft_diagram::route_polyline(p, c)).map(|r| r.center())),
    });
    if let Some(c) = centre {
        let lift = app.session.view.canvas.map_or(0.0, |[_, h]| f64::from(h * 0.42 / app.session.view.zoom.max(0.01)));
        let _ = app.run("view.pan", json!({"x": c.x, "y": c.y + lift}));
    }
}

/// The text of the field `item` points into.
fn field_text(app: &CraftApp, item: &Item) -> Option<String> {
    let doc = &app.session.doc;
    let page = doc.pages.iter().find(|p| p.id == item.page)?;
    if item.kind == "connector" {
        return page.connector(&item.id).map(|c| c.text.clone());
    }
    let sh = page.shape(&item.id)?;
    let mut parts = item.field.split(':');
    let comp = |cid: Option<&str>| sh.compartments.iter().find(|c| Some(c.id.as_str()) == cid);
    match parts.next()? {
        "text" => Some(sh.text.clone()),
        "title" => comp(parts.next())?.title.clone(),
        "body" => Some(comp(parts.next())?.text.clone()),
        "row" => {
            let c = comp(parts.next())?;
            c.rows.get(parts.next()?.parse::<usize>().ok()?).cloned()
        }
        _ => None,
    }
}

/// `text` with the chars `start..end` (which must read `word`) replaced by `with`.
fn splice(text: &str, item: &Item, with: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let found: String = chars.get(item.start..item.end)?.iter().collect();
    if found != item.word {
        return None;
    }
    let mut out: String = chars.get(..item.start)?.iter().collect();
    out.push_str(with);
    out.extend(chars.get(item.end..)?);
    Some(out)
}

/// Replace one word, through the command that edits the field it is in.
fn change_item(app: &mut CraftApp, item: &Item, with: &str) -> Result<(), String> {
    let text = field_text(app, item).ok_or("The text changed since the check; check again.")?;
    let new = splice(&text, item, with).ok_or("The text changed since the check; check again.")?;
    if app.session.doc.page().is_some_and(|p| p.id != item.page) {
        app.run("page.setActive", json!({"page": item.page}))?;
    }
    if item.kind == "connector" {
        return app.run("connector.setText", json!({"id": item.id, "text": new})).map(drop);
    }
    let mut parts = item.field.split(':');
    let kind = parts.next().unwrap_or("");
    let cid = parts.next().unwrap_or("");
    match kind {
        "text" => app.run("shape.setText", json!({"id": item.id, "text": new})),
        "title" => app.run("compartment.set", json!({"id": item.id, "compartment": cid, "title": new})),
        "body" => app.run("compartment.set", json!({"id": item.id, "compartment": cid, "text": new})),
        "row" => {
            let n: usize = parts.next().and_then(|x| x.parse().ok()).ok_or("Bad row.")?;
            let mut rows = app
                .session
                .doc
                .pages
                .iter()
                .find(|p| p.id == item.page)
                .and_then(|p| p.shape(&item.id))
                .and_then(|sh| sh.compartments.iter().find(|c| c.id == cid))
                .map(|c| c.rows.clone())
                .ok_or("The text changed since the check; check again.")?;
            *rows.get_mut(n).ok_or("The text changed since the check; check again.")? = new;
            app.run("compartment.set", json!({"id": item.id, "compartment": cid, "rows": rows}))
        }
        _ => Err("Unknown field.".to_string()),
    }
    .map(drop)
}

/// Change: replace the current word with Change to, then on to the next.
pub fn change(app: &mut CraftApp, s: &mut State) {
    let Some(item) = s.items.get(s.current).cloned() else { return };
    let with = s.replacement.clone();
    if let Err(e) = change_item(app, &item, &with) {
        s.message = e;
        return;
    }
    s.message = format!("Changed {} to {}.", item.word, if with.is_empty() { "nothing" } else { with.as_str() });
    refresh(app, s);
    advance_message(s);
    show_current(app, s);
}

/// Change All: replace every occurrence of the current word (as written) in the document.
pub fn change_all(app: &mut CraftApp, s: &mut State) {
    let Some(word) = s.items.get(s.current).map(|i| i.word.clone()) else { return };
    let with = s.replacement.clone();
    let mut n = 0;
    while n < MAX_CHANGE_ALL {
        let Some(item) = s.items.iter().find(|i| i.word == word).cloned() else { break };
        if let Err(e) = change_item(app, &item, &with) {
            s.message = e;
            refresh(app, s);
            show_current(app, s);
            return;
        }
        n += 1;
        refresh(app, s);
    }
    s.message = format!("Changed {n} occurrence(s) of {word}.");
    advance_message(s);
    show_current(app, s);
}

/// Ignore: leave this word as it is and go on to the next.
pub fn ignore(app: &mut CraftApp, s: &mut State) {
    let Some(item) = s.items.get(s.current).cloned() else { return };
    s.skipped.push(skip_key(&item));
    s.items.remove(s.current);
    s.current = s.current.min(s.items.len().saturating_sub(1));
    advance_message(s);
    show_current(app, s);
}

/// Ignore All: accept the word everywhere in this document from now on.
pub fn ignore_all(app: &mut CraftApp, s: &mut State) {
    let Some(word) = s.items.get(s.current).map(|i| i.word.clone()) else { return };
    if let Err(e) = app.run("proof.ignore", json!({"word": word})) {
        s.message = e;
        return;
    }
    s.message = format!("Ignoring {word} in this document.");
    refresh(app, s);
    advance_message(s);
    show_current(app, s);
}

/// When no words are left, say so.
fn advance_message(s: &mut State) {
    if s.items.is_empty() {
        s.message = "The spelling check is complete.".into();
    }
}

/// The text around a word, shortened to a line, for the context under it.
fn context(app: &CraftApp, item: &Item) -> String {
    let text = field_text(app, item).unwrap_or_default();
    let chars: Vec<char> = text.chars().collect();
    let from = item.start.saturating_sub(30);
    let to = (item.end + 30).min(chars.len());
    let mid: String = chars.get(from..to).unwrap_or_default().iter().map(|c| if c.is_control() { ' ' } else { *c }).collect();
    format!("{}{}{}", if from > 0 { "..." } else { "" }, mid, if to < chars.len() { "..." } else { "" })
}

/// The dialog body. Returns true to close.
pub fn body(app: &mut CraftApp, ui: &mut Ui, s: &mut State) -> bool {
    let t = Tokens::get(ui.ctx());
    if s.fresh {
        start(app, s);
    }
    let mut close = false;
    let Some(item) = s.items.get(s.current).cloned() else {
        ui.label(egui::RichText::new(if s.message.is_empty() { "No spelling errors found." } else { s.message.as_str() }).color(t.text));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::primary_button(ui, "Close").clicked() {
                close = true;
            }
        });
        return close;
    };
    ui.label(egui::RichText::new(format!("Not in Dictionary ({} of {})", s.current + 1, s.items.len())).color(t.text_dim));
    ui.label(egui::RichText::new(&item.word).font(semibold(18.0)).color(t.red));
    ui.label(egui::RichText::new(context(app, &item)).color(t.text_dim));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Change to").color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut s.replacement).desired_width(260.0));
    });
    ui.label(egui::RichText::new("Suggestions").color(t.text_dim));
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.border)).corner_radius(4.0).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().max_height(110.0).id_salt("spelling_suggestions").show(ui, |ui| {
            ui.set_width(ui.available_width());
            if item.suggestions.is_empty() {
                ui.label(egui::RichText::new("(no suggestions)").color(t.text_dim));
            }
            for sug in &item.suggestions {
                if ui.selectable_label(s.replacement == *sug, sug).clicked() {
                    s.replacement = sug.clone();
                }
            }
        });
    });
    ui.label(egui::RichText::new(if s.message.is_empty() { " " } else { s.message.as_str() }).color(t.text_dim));
    ui.add_space(4.0);
    let b = |ui: &mut Ui, label: &str| ui.add(egui::Button::new(label).min_size(vec2(84.0, 24.0))).clicked();
    ui.horizontal(|ui| {
        if b(ui, "Ignore") {
            ignore(app, s);
        }
        if b(ui, "Ignore All") {
            ignore_all(app, s);
        }
        if b(ui, "Change") {
            change(app, s);
        }
        if b(ui, "Change All") {
            change_all(app, s);
        }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add(egui::Button::new("Close").min_size(vec2(72.0, 24.0))).clicked() {
            close = true;
        }
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        for (x, text) in [(0, "Send teh data"), (300, "All fine")] {
            app.run("shape.add", json!({"stencil": "basic.rectangle", "x": x, "y": 0, "text": text})).unwrap();
        }
        let second = app.session.doc.pages[0].shapes[1].id.clone();
        app.run("compartment.add", json!({"id": second, "kind": "list", "rows": ["name", "adress"]})).unwrap();
        app.run("page.add", json!({})).unwrap();
        app.run("shape.add", json!({"stencil": "basic.rectangle", "x": 500, "y": 400, "text": "teh end"})).unwrap();
        app.run("page.setActive", json!({"page": 0})).unwrap();
        app
    }

    fn started(app: &mut CraftApp) -> State {
        let mut s = open(app);
        start(app, &mut s);
        s
    }

    #[test]
    fn open_finds_the_first_word_selects_it_and_offers_the_suggestion() {
        let mut app = app();
        let s = started(&mut app);
        assert_eq!(s.items.len(), 3);
        assert_eq!((s.items[0].word.as_str(), s.replacement.as_str()), ("teh", "the"));
        let id = s.items[0].id.clone();
        assert_eq!(app.session.sel.items, vec![id]);
    }

    #[test]
    fn change_rewrites_the_word_in_one_undo_step_and_moves_on() {
        let mut app = app();
        let mut s = started(&mut app);
        change(&mut app, &mut s);
        assert_eq!(app.session.doc.pages[0].shapes[0].text, "Send the data");
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.items[0].word, "adress", "on to the next word");
        change(&mut app, &mut s);
        assert_eq!(app.session.doc.pages[0].shapes[1].compartments[0].rows[1], "address");
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.session.doc.pages[0].shapes[1].compartments[0].rows[1], "adress");
        assert_eq!(app.session.doc.pages[0].shapes[0].text, "Send the data", "one step undone, not both");
    }

    #[test]
    fn change_all_reaches_other_pages() {
        let mut app = app();
        let mut s = started(&mut app);
        change_all(&mut app, &mut s);
        assert_eq!(s.message, "Changed 2 occurrence(s) of teh.");
        assert_eq!(app.session.doc.pages[0].shapes[0].text, "Send the data");
        assert_eq!(app.session.doc.pages[1].shapes[0].text, "the end");
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn ignore_skips_one_word_and_ignore_all_remembers_it_in_the_document() {
        let mut app = app();
        let mut s = started(&mut app);
        ignore(&mut app, &mut s);
        assert_eq!(s.items[0].word, "adress");
        assert_eq!(s.items.len(), 2);
        s.current = 1;
        ignore_all(&mut app, &mut s);
        assert_eq!(app.session.doc.ignored_words, ["teh"]);
        assert_eq!(s.items.len(), 1, "every teh is gone from the list, the skipped adress remains");
        ignore(&mut app, &mut s);
        assert!(s.items.is_empty());
        assert_eq!(s.message, "The spelling check is complete.");
    }

    #[test]
    fn a_changed_text_is_not_overwritten_blindly() {
        let mut app = app();
        let mut s = started(&mut app);
        let id = s.items[0].id.clone();
        app.run("shape.setText", json!({"id": id, "text": "Totally different"})).unwrap();
        change(&mut app, &mut s);
        assert!(s.message.contains("changed since the check"), "{}", s.message);
        assert_eq!(app.session.doc.pages[0].shapes[0].text, "Totally different");
    }

    #[test]
    fn a_clean_document_says_so() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        let s = started(&mut app);
        assert!(s.items.is_empty());
        assert_eq!(s.message, "No spelling errors found.");
    }

    #[test]
    fn ui_spelling_opens_the_dialog() {
        let mut app = app();
        app.run("ui.spelling", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("spelling"));
    }
}
