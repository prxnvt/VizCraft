//! The Model Tree pane (pane key `navigation`, so `view.navigationPane`, `ui.pane.navigation` and
//! saved layouts keep working).
//!
//! Two branches at the root. Model is the containment tree from the semantic model: root elements
//! (grouped by type when the roots have more than one), each row showing the element's name, tag
//! and the pages that draw it. Diagrams lists every page with its shapes and connectors. A
//! document with no model shows only Diagrams.
//!
//! Mode: Model shows everything; Context follows the canvas selection and narrows the Model
//! branch to the selected items' elements, their owners and their neighbours, opened up.
//!
//! Every change is a command (`page.setActive`, `edit.select`, `element.rename`, `model.move`,
//! `model.views`, `element.deleteFromView`, `model.delete`); the pane keeps only which rows are
//! open, the mode, and the rename and confirmation state ([`TreeState`]). Rows are virtualised
//! with `ScrollArea::show_rows` and only the visible ones build strings, so a model of ten
//! thousand elements stays smooth. `ui.tree` drives the pane from the control channel.

use std::collections::{BTreeSet, HashMap, HashSet};

use egui::{Align, Layout, Sense, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{Document, Page};

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;
use crate::widgets::check_mark;

/// The Model Tree pane. The key stays `navigation`.
pub(super) const SPEC: PaneSpec = PaneSpec::new("navigation", "Model Tree", Side::Left, 240.0, model_tree);

const ROW_H: f32 = 20.0;
const INDENT: f32 = 14.0;
/// Deepest containment level drawn; `model.tree` stops at the same depth.
const MAX_DEPTH: usize = 64;
/// Longest id taken from a `ui.tree` list, and most ids taken from one.
const MAX_ID: usize = 256;
const MAX_IDS: usize = 10_000;
/// Longest row label, in characters.
const MAX_LABEL: usize = 48;

/// What the Model branch shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The whole containment tree.
    #[default]
    Model,
    /// Only what the canvas selection touches.
    Context,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Model => "model",
            Mode::Context => "context",
        }
    }
}

/// An element waiting for the delete-from-model confirmation.
#[derive(Clone, Debug)]
struct Confirm {
    id: String,
    name: String,
    pages: usize,
}

/// What the pane remembers between frames. Transient: never saved with the preferences.
#[derive(Clone, Debug, Default)]
pub struct TreeState {
    /// Model or Context.
    pub mode: Mode,
    /// Keys opened explicitly.
    open: BTreeSet<String>,
    /// Keys closed explicitly.
    closed: BTreeSet<String>,
    /// Inline rename in progress: element or shape id and the text so far.
    rename: Option<(String, String)>,
    /// Delete-from-model confirmation showing.
    confirm: Option<Confirm>,
    /// Row to bring into view on the next frame.
    scroll_to: Option<String>,
    /// Row `ui.tree reveal` last asked for, drawn highlighted.
    revealed: Option<String>,
}

const BRANCH_MODEL: &str = "branch:model";
const BRANCH_DIAGRAMS: &str = "branch:diagrams";

fn is_container_key(key: &str) -> bool {
    key.starts_with("branch:") || key.starts_with("group:") || key.starts_with("page:")
}

impl TreeState {
    /// Whether the row `key` shows its children. Branches, groups and pages start open;
    /// elements start closed, except that Context mode opens them all.
    fn is_open(&self, key: &str) -> bool {
        if self.open.contains(key) {
            true
        } else if self.closed.contains(key) {
            false
        } else {
            is_container_key(key) || self.mode == Mode::Context
        }
    }

    fn set_open(&mut self, key: &str, on: bool) {
        let (add, remove) = if on { (&mut self.open, &mut self.closed) } else { (&mut self.closed, &mut self.open) };
        remove.remove(key);
        add.insert(key.to_string());
    }

    fn toggle(&mut self, key: &str) {
        let now = self.is_open(key);
        self.set_open(key, !now);
    }
}

/// Which row family a key names, for the `branch` report.
fn branch_report(st: &TreeState) -> &'static str {
    match (st.is_open(BRANCH_MODEL), st.is_open(BRANCH_DIAGRAMS)) {
        (true, true) => "both",
        (true, false) => "model",
        (false, true) => "diagrams",
        (false, false) => "none",
    }
}

/// Last path segment of a type id (`flowchart.Process` is `Process`).
fn type_name(type_id: &str) -> &str {
    type_id.rsplit('.').next().unwrap_or(type_id)
}

fn clip(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default().trim();
    if line.chars().count() > MAX_LABEL {
        let mut out: String = line.chars().take(MAX_LABEL).collect();
        out.push_str("...");
        out
    } else {
        line.to_string()
    }
}

fn active_index(doc: &Document) -> usize {
    doc.pages.iter().position(|p| p.id == doc.active_page).unwrap_or(0)
}

/// Where each model item is drawn: id to (page index, shape or connector id), in page order.
fn drawn_map(doc: &Document) -> HashMap<&str, Vec<(usize, &str)>> {
    let mut map: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            if let Some(e) = s.element.as_deref() {
                map.entry(e).or_default().push((pi, s.id.as_str()));
            }
        }
        for c in &page.connectors {
            if let Some(r) = c.relationship.as_deref() {
                map.entry(r).or_default().push((pi, c.id.as_str()));
            }
        }
    }
    map
}

/// The model items the selection on the active page draws: elements of selected shapes, and for
/// a selected connector its relationship's two ends.
fn selected_elements<'a>(doc: &'a Document, sel: &[String]) -> HashSet<&'a str> {
    let mut out = HashSet::new();
    let Some(page) = doc.page() else { return out };
    for id in sel {
        if let Some(e) = page.shape(id).and_then(|s| s.element.as_deref()) {
            out.insert(e);
        } else if let Some(r) = page.connectors.iter().find(|c| &c.id == id).and_then(|c| c.relationship.as_deref())
            && let Some(rel) = doc.model.relationships.iter().find(|x| x.id == r)
        {
            out.insert(rel.source.as_str());
            out.insert(rel.target.as_str());
        }
    }
    out
}

/// Context filter: the selected elements, their neighbours over relationships, and every owner
/// of those, so the containment chain stays connected.
fn context_set<'a>(doc: &'a Document, selected: &HashSet<&'a str>) -> HashSet<&'a str> {
    let mut keep: HashSet<&str> = selected.clone();
    for r in &doc.model.relationships {
        if selected.contains(r.source.as_str()) {
            keep.insert(r.target.as_str());
        }
        if selected.contains(r.target.as_str()) {
            keep.insert(r.source.as_str());
        }
    }
    let owner: HashMap<&str, &str> = doc.model.elements.iter().filter_map(|e| Some((e.id.as_str(), e.owner.as_deref()?))).collect();
    let mut all = keep.clone();
    for id in keep {
        let mut cur = id;
        for _ in 0..MAX_DEPTH {
            match owner.get(cur) {
                Some(o) if all.insert(o) => cur = o,
                _ => break,
            }
        }
    }
    all
}

/// The fields of a model element the tree reads, borrowed from the document.
#[derive(Clone, Copy)]
struct El<'a> {
    id: &'a str,
    type_id: &'a str,
    name: &'a str,
    tag: &'a str,
}

#[derive(Clone, Copy)]
enum Kind<'a> {
    Branch { model: bool, count: usize },
    Group { ty: &'a str, count: usize },
    Element { e: El<'a>, kids: bool },
    Page { page: &'a Page, index: usize },
    Shape { page: usize, id: &'a str, text: &'a str, element: Option<&'a str> },
    Connector { page: usize, id: &'a str, text: &'a str, relationship: Option<&'a str> },
}

#[derive(Clone, Copy)]
struct Row<'a> {
    depth: usize,
    kind: Kind<'a>,
}

impl Row<'_> {
    /// The key `ui.tree` and the open sets use for this row.
    fn key(&self) -> String {
        match self.kind {
            Kind::Branch { model: true, .. } => BRANCH_MODEL.into(),
            Kind::Branch { model: false, .. } => BRANCH_DIAGRAMS.into(),
            Kind::Group { ty, .. } => format!("group:{ty}"),
            Kind::Element { e, .. } => e.id.to_string(),
            Kind::Page { page, .. } => format!("page:{}", page.id),
            Kind::Shape { id, .. } | Kind::Connector { id, .. } => id.to_string(),
        }
    }
}

/// The visible rows, top to bottom, given what is open.
fn flatten<'a>(doc: &'a Document, st: &TreeState, filter: Option<&HashSet<&'a str>>) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    let m = &doc.model;
    let has_model = !m.elements.is_empty();
    if has_model {
        let keep = |id: &str| filter.is_none_or(|f| f.contains(id));
        let mut kids: HashMap<Option<&str>, Vec<El<'_>>> = HashMap::new();
        for e in m.elements.iter().filter(|e| keep(&e.id)) {
            let el = El { id: &e.id, type_id: &e.type_id, name: &e.name, tag: &e.tag };
            kids.entry(e.owner.as_deref().filter(|o| keep(o))).or_default().push(el);
        }
        let roots = kids.get(&None).map(Vec::as_slice).unwrap_or_default();
        rows.push(Row { depth: 0, kind: Kind::Branch { model: true, count: m.elements.len() } });
        if st.is_open(BRANCH_MODEL) {
            let mut types: Vec<&str> = Vec::new();
            for e in roots {
                if !types.contains(&e.type_id) {
                    types.push(e.type_id);
                }
            }
            let grouped = types.len() > 1;
            let push_tree = |rows: &mut Vec<Row<'a>>, start: Vec<El<'a>>, depth0: usize| {
                let mut stack: Vec<(El<'_>, usize)> = start.into_iter().rev().map(|e| (e, depth0)).collect();
                while let Some((e, depth)) = stack.pop() {
                    let below = kids.get(&Some(e.id)).map(Vec::as_slice).unwrap_or_default();
                    rows.push(Row { depth, kind: Kind::Element { e, kids: !below.is_empty() } });
                    if depth < MAX_DEPTH && st.is_open(e.id) {
                        stack.extend(below.iter().rev().map(|k| (*k, depth + 1)));
                    }
                }
            };
            if grouped {
                for ty in types {
                    let members: Vec<El<'_>> = roots.iter().copied().filter(|e| e.type_id == ty).collect();
                    rows.push(Row { depth: 1, kind: Kind::Group { ty, count: members.len() } });
                    if st.is_open(&format!("group:{ty}")) {
                        push_tree(&mut rows, members, 2);
                    }
                }
            } else {
                push_tree(&mut rows, roots.to_vec(), 1);
            }
        }
    }
    rows.push(Row { depth: 0, kind: Kind::Branch { model: false, count: doc.pages.len() } });
    if st.is_open(BRANCH_DIAGRAMS) {
        for (index, page) in doc.pages.iter().enumerate() {
            rows.push(Row { depth: 1, kind: Kind::Page { page, index } });
            if st.is_open(&format!("page:{}", page.id)) {
                for s in &page.shapes {
                    rows.push(Row { depth: 2, kind: Kind::Shape { page: index, id: &s.id, text: &s.text, element: s.element.as_deref() } });
                }
                for c in &page.connectors {
                    rows.push(Row { depth: 2, kind: Kind::Connector { page: index, id: &c.id, text: &c.text, relationship: c.relationship.as_deref() } });
                }
            }
        }
    }
    if !has_model {
        // Free drawing: Diagrams only, so the empty Model branch is not drawn.
        rows.retain(|r| !matches!(r.kind, Kind::Branch { model: true, .. }));
    }
    rows
}

/// Engine commands a frame wants run, in order.
type Runs = Vec<(&'static str, Value)>;

/// The commands that bring the views of model item `id` into the selection: switch to a page
/// that draws it (the active one when it does), then select the items there.
fn select_runs(doc: &Document, drawn: &HashMap<&str, Vec<(usize, &str)>>, id: &str) -> Runs {
    let Some(entries) = drawn.get(id) else { return Vec::new() };
    let active = active_index(doc);
    let page = if entries.iter().any(|(p, _)| *p == active) { active } else { entries.first().map_or(active, |(p, _)| *p) };
    let items: Vec<&str> = entries.iter().filter(|(p, _)| *p == page).map(|(_, i)| *i).collect();
    select_items_runs(doc, page, &items)
}

fn select_items_runs(doc: &Document, page: usize, items: &[&str]) -> Runs {
    let mut runs: Runs = Vec::new();
    if page != active_index(doc)
        && let Some(p) = doc.pages.get(page)
    {
        runs.push(("page.setActive", json!({"page": p.id})));
    }
    if !items.is_empty() {
        runs.push(("edit.select", json!({"ids": items})));
    }
    runs
}

fn run_all(app: &mut CraftApp, runs: Runs) {
    for (id, params) in runs {
        if let Err(e) = app.run(id, params) {
            app.status(e);
            break;
        }
    }
}

fn model_tree(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut st = std::mem::take(&mut app.tree);
    let mut runs: Runs = Vec::new();

    ui.horizontal(|ui| {
        for (label, mode) in [("Model", Mode::Model), ("Context", Mode::Context)] {
            let on = st.mode == mode;
            let (rect, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
            if on {
                check_mark(ui, rect.center(), t.accent);
            }
            if ui.add(egui::Button::new(label).frame(false)).on_hover_text(match mode {
                Mode::Model => "Show the whole model",
                Mode::Context => "Follow the canvas selection: its elements, owners and neighbours",
            }).clicked() {
                st.mode = mode;
            }
        }
    });
    ui.separator();

    {
        let doc = &app.session.doc;
        let drawn = drawn_map(doc);
        let selected = selected_elements(doc, &app.session.sel.items);
        let filter = (st.mode == Mode::Context && !selected.is_empty()).then(|| context_set(doc, &selected));
        let rows = flatten(doc, &st, filter.as_ref());
        let active = active_index(doc);
        let sel_items: HashSet<&str> = app.session.sel.items.iter().map(String::as_str).collect();

        let mut area = egui::ScrollArea::vertical().id_salt("model-tree-rows").auto_shrink(false);
        if let Some(key) = st.scroll_to.take()
            && let Some(i) = rows.iter().position(|r| r.key() == key)
        {
            area = area.vertical_scroll_offset((i as f32 * ROW_H - ROW_H * 2.0).max(0.0));
        }
        if rows.is_empty() {
            ui.label(egui::RichText::new("No items").small().color(t.text_dim));
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for row in rows.get(range).unwrap_or_default() {
                draw_row(ui, &t, &mut st, doc, &drawn, &selected, &sel_items, active, row, &mut runs);
            }
        });
    }

    confirm_window(app, ui.ctx(), &mut st, &mut runs);
    app.tree = st;
    run_all(app, runs);
}

#[allow(clippy::too_many_arguments)]
fn draw_row(
    ui: &mut Ui,
    t: &Tokens,
    st: &mut TreeState,
    doc: &Document,
    drawn: &HashMap<&str, Vec<(usize, &str)>>,
    selected: &HashSet<&str>,
    sel_items: &HashSet<&str>,
    active: usize,
    row: &Row<'_>,
    runs: &mut Runs,
) {
    let key = row.key();
    let revealed = st.revealed.as_deref() == Some(key.as_str());
    let (expandable, is_open) = match row.kind {
        Kind::Branch { .. } | Kind::Group { .. } | Kind::Page { .. } => (true, st.is_open(&key)),
        Kind::Element { kids, .. } => (kids, st.is_open(&key)),
        _ => (false, false),
    };
    let highlighted = revealed
        || match row.kind {
            Kind::Element { e, .. } => selected.contains(e.id),
            Kind::Shape { page, id, .. } | Kind::Connector { page, id, .. } => page == active && sel_items.contains(id),
            Kind::Page { index, .. } => index == active,
            _ => false,
        };
    // A model item's element or relationship id when the row is bound to one.
    let item: Option<&str> = match row.kind {
        Kind::Element { e, .. } => Some(e.id),
        Kind::Shape { element, .. } => element,
        Kind::Connector { relationship, .. } => relationship,
        _ => None,
    };

    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, ROW_H), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let bg = ui.painter().add(egui::Shape::Noop);
        ui.add_space(row.depth as f32 * INDENT + 2.0);
        let (arrow, ar) = ui.allocate_exact_size(vec2(12.0, ROW_H), if expandable { Sense::click() } else { Sense::hover() });
        if expandable {
            let c = arrow.center();
            let pts = if is_open {
                vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 3.0)]
            } else {
                vec![c + vec2(-2.0, -3.5), c + vec2(3.0, 0.0), c + vec2(-2.0, 3.5)]
            };
            ui.painter().add(egui::Shape::convex_polygon(pts, t.text_dim, egui::Stroke::NONE));
            if ar.clicked() {
                st.toggle(&key);
            }
        }

        let renaming = st.rename.as_ref().is_some_and(|(id, _)| item.is_some_and(|i| i == id));
        let mut resp = None;
        if renaming {
            let commit = draw_rename(ui, st, runs);
            if commit {
                st.rename = None;
            }
        } else {
            let (text, strong) = match row.kind {
                Kind::Branch { model: true, count } => (format!("Model ({count})"), true),
                Kind::Branch { model: false, count } => (format!("Diagrams ({count})"), true),
                Kind::Group { ty, count } => (format!("{} ({count})", type_name(ty)), true),
                Kind::Element { e, .. } => (clip(if e.name.is_empty() { e.id } else { e.name }), false),
                Kind::Page { page, .. } => (clip(&page.name), true),
                Kind::Shape { id, text, .. } | Kind::Connector { id, text, .. } => (clip(if text.trim().is_empty() { id } else { text }), false),
            };
            let mut rt = egui::RichText::new(text).color(t.text);
            if strong {
                rt = rt.strong();
            }
            let r = ui.add(egui::Label::new(rt).sense(Sense::click_and_drag()).selectable(false));
            resp = Some(r);
            // The dim tail: tag and pages for an element, the kind for a diagram row.
            let tail = match row.kind {
                Kind::Element { e, .. } => {
                    let mut pages: Vec<usize> = drawn.get(e.id).map(|v| v.iter().map(|(p, _)| *p).collect()).unwrap_or_default();
                    pages.dedup();
                    let names: Vec<&str> = pages.iter().filter_map(|p| doc.pages.get(*p).map(|p| p.name.as_str())).collect();
                    let mut s = String::new();
                    if !e.tag.is_empty() {
                        s.push_str(e.tag);
                    }
                    if !names.is_empty() {
                        if !s.is_empty() {
                            s.push_str("  ");
                        }
                        s.push_str(&format!("on {}", clip(&names.join(", "))));
                    }
                    s
                }
                Kind::Shape { id, .. } => id.to_string(),
                Kind::Connector { id, .. } => format!("connector {id}"),
                _ => String::new(),
            };
            if !tail.is_empty() {
                ui.label(egui::RichText::new(tail).small().color(t.text_dim));
            }
        }
        if highlighted {
            let r = ui.min_rect();
            let full = egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), r.top()), egui::pos2(ui.max_rect().right(), r.top() + ROW_H));
            ui.painter().set(bg, egui::Shape::rect_filled(full, 0.0, if revealed { t.hover } else { t.selection }));
        }

        let Some(r) = resp else { return };
        let (drop_onto, no_owner) = match row.kind {
            Kind::Element { e, .. } => (Some(e.id.to_string()), false),
            Kind::Branch { model: true, .. } | Kind::Group { .. } => (None, true),
            _ => (None, false),
        };
        row_input(ui, t, st, doc, drawn, row, &r, item, drop_onto, no_owner, runs);
    });
}

/// Click, double-click, drag and drop, and the context menu of one row.
#[allow(clippy::too_many_arguments)]
fn row_input(
    ui: &mut Ui,
    t: &Tokens,
    st: &mut TreeState,
    doc: &Document,
    drawn: &HashMap<&str, Vec<(usize, &str)>>,
    row: &Row<'_>,
    r: &egui::Response,
    item: Option<&str>,
    drop_onto: Option<String>,
    no_owner: bool,
    runs: &mut Runs,
) {
    if let Some(id) = item
        && matches!(row.kind, Kind::Element { .. })
    {
        r.dnd_set_drag_payload(id.to_string());
    }
    if (drop_onto.is_some() || no_owner) && r.dnd_hover_payload::<String>().is_some() {
        ui.painter().rect_stroke(r.rect.expand(2.0), 2.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
    if (drop_onto.is_some() || no_owner)
        && let Some(p) = r.dnd_release_payload::<String>()
        && drop_onto.as_deref() != Some(p.as_str())
    {
        runs.push(("model.move", json!({"id": *p, "owner": drop_onto})));
    }

    if r.double_clicked() {
        if let Some(id) = item {
            let name = match row.kind {
                Kind::Element { e, .. } => e.name.to_string(),
                Kind::Shape { text, .. } => text.to_string(),
                _ => String::new(),
            };
            if matches!(row.kind, Kind::Element { .. } | Kind::Shape { .. }) {
                st.rename = Some((id.to_string(), name));
            }
        }
    } else if r.clicked() {
        match row.kind {
            Kind::Branch { .. } | Kind::Group { .. } => st.toggle(&row.key()),
            Kind::Page { page, .. } => runs.push(("page.setActive", json!({"page": page.id}))),
            Kind::Element { e, .. } => runs.extend(select_runs(doc, drawn, e.id)),
            Kind::Shape { page, id, .. } | Kind::Connector { page, id, .. } => runs.extend(select_items_runs(doc, page, &[id])),
        }
    }

    let Some(id) = item else { return };
    r.context_menu(|ui| {
        let views = drawn.get(id).map_or(0, Vec::len);
        if ui.add_enabled(views > 0, egui::Button::new("Show in All Views")).clicked() {
            runs.push(("model.views", json!({"id": id})));
            runs.extend(select_runs(doc, drawn, id));
            ui.close();
        }
        if ui.add_enabled(views > 0, egui::Button::new("Delete from View")).clicked() {
            runs.extend(select_runs(doc, drawn, id));
            let ids: Vec<&str> = drawn.get(id).map(|v| v.iter().map(|(_, i)| *i).collect()).unwrap_or_default();
            let active = active_index(doc);
            let on_page: Vec<&str> = drawn.get(id).map(|v| v.iter().filter(|(p, _)| *p == active).map(|(_, i)| *i).collect()).unwrap_or_default();
            runs.push(("element.deleteFromView", json!({"ids": if on_page.is_empty() { ids.into_iter().take(1).collect() } else { on_page }})));
            ui.close();
        }
        ui.separator();
        if ui.button("Delete from Model").clicked() {
            let (name, pages) = match row.kind {
                Kind::Element { e, .. } => (e.name.to_string(), views),
                _ => (id.to_string(), views),
            };
            st.confirm = Some(Confirm { id: id.to_string(), name, pages });
            ui.close();
        }
    });
}

/// The inline editor of a rename. Returns true when the edit ended (Enter, Escape or focus lost).
fn draw_rename(ui: &mut Ui, st: &mut TreeState, runs: &mut Runs) -> bool {
    let Some((id, text)) = st.rename.as_mut() else { return true };
    let edit = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 8.0));
    if !edit.has_focus() && !edit.lost_focus() {
        edit.request_focus();
        // Start with the whole name selected, so typing replaces it.
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit.id) {
            let end = egui::text::CCursor::new(text.chars().count());
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
            state.store(ui.ctx(), edit.id);
        }
    }
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if edit.lost_focus() && !escaped {
        let name = text.trim().to_string();
        if !name.is_empty() {
            runs.push(("element.rename", json!({"id": id, "name": name})));
        }
        return true;
    }
    escaped
}

/// The delete-from-model confirmation: it removes every view of the element, so it asks first.
fn confirm_window(_app: &mut CraftApp, ctx: &egui::Context, st: &mut TreeState, runs: &mut Runs) {
    let Some(c) = st.confirm.clone() else { return };
    let mut done = false;
    egui::Window::new("Delete from Model").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
        let name = if c.name.is_empty() { c.id.clone() } else { c.name.clone() };
        ui.label(format!("Delete {} from the model?", clip(&name)));
        ui.label(format!("It is removed from {} view(s) and every element it owns is deleted too.", c.pages));
        ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
                runs.push(("model.delete", json!({"ids": [c.id]})));
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        st.confirm = None;
    }
}

fn id_list(p: &Value, key: &str) -> Vec<String> {
    p.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|s| s.len() <= MAX_ID).take(MAX_IDS).map(str::to_string).collect())
        .unwrap_or_default()
}

/// The open-set key `id` names: a branch word, a page id, or an element id.
fn key_of(doc: &Document, id: &str) -> String {
    match id {
        "model" => BRANCH_MODEL.into(),
        "diagrams" => BRANCH_DIAGRAMS.into(),
        _ if doc.pages.iter().any(|p| p.id == id) => format!("page:{id}"),
        _ => id.to_string(),
    }
}

/// `ui.tree {mode?, branch?, expand?, collapse?, reveal?}`: drive the pane. `branch` opens that
/// branch and closes the other. `reveal` opens every row above an element, shape, connector or
/// page, scrolls to it and highlights it. The reply is the state ([`inspect`]).
pub fn ui_tree(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let mut st = std::mem::take(&mut app.tree);
    let r = apply(&app.session.doc, &mut st, p);
    app.tree = st;
    r?;
    Ok(inspect(app))
}

fn apply(doc: &Document, st: &mut TreeState, p: &Value) -> Result<(), String> {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let mode = match s("mode") {
        None => None,
        Some("model") => Some(Mode::Model),
        Some("context") => Some(Mode::Context),
        Some(other) => return Err(format!("unknown mode {other:?} (model or context)")),
    };
    let branch = match s("branch") {
        None => None,
        Some(b @ ("model" | "diagrams")) => Some(b),
        Some(other) => return Err(format!("unknown branch {other:?} (model or diagrams)")),
    };
    let reveal = match s("reveal") {
        None => None,
        Some(id) if id.len() <= MAX_ID => Some(reveal_keys(doc, id).ok_or_else(|| format!("`{id}` is not a row of the tree"))?),
        Some(_) => return Err("`reveal` is too long".into()),
    };
    if let Some(m) = mode {
        st.mode = m;
    }
    if let Some(b) = branch {
        st.set_open(BRANCH_MODEL, b == "model");
        st.set_open(BRANCH_DIAGRAMS, b == "diagrams");
    }
    for id in id_list(p, "expand") {
        st.set_open(&key_of(doc, &id), true);
    }
    for id in id_list(p, "collapse") {
        st.set_open(&key_of(doc, &id), false);
    }
    if let Some((open, row)) = reveal {
        for k in open {
            st.set_open(&k, true);
        }
        st.scroll_to = Some(row.clone());
        st.revealed = Some(row);
    }
    Ok(())
}

/// The keys to open to show `id`, and the row key to scroll to; `None` when `id` is no row.
fn reveal_keys(doc: &Document, id: &str) -> Option<(Vec<String>, String)> {
    if let Some(page) = doc.pages.iter().find(|p| p.id == id) {
        return Some((vec![BRANCH_DIAGRAMS.into()], format!("page:{}", page.id)));
    }
    if doc.model.element(id).is_some() {
        let mut open = vec![BRANCH_MODEL.to_string()];
        let mut cur = doc.model.element(id)?;
        open.push(format!("group:{}", cur.type_id));
        for _ in 0..doc.model.elements.len() {
            let Some(owner) = cur.owner.as_deref().and_then(|o| doc.model.element(o)) else { break };
            open.push(owner.id.clone());
            cur = owner;
        }
        // The root's group is the one that holds the whole chain.
        open.push(format!("group:{}", cur.type_id));
        return Some((open, id.to_string()));
    }
    let page = doc.pages.iter().find(|p| p.contains_id(id))?;
    Some((vec![BRANCH_DIAGRAMS.into(), format!("page:{}", page.id)], id.to_string()))
}

/// The `tree` field of `ui.inspect`: `{mode, branch, expanded, collapsed, selected}`.
pub fn inspect(app: &CraftApp) -> Value {
    let st = &app.tree;
    let mut selected: Vec<&str> = selected_elements(&app.session.doc, &app.session.sel.items).into_iter().collect();
    selected.sort_unstable();
    json!({
        "mode": st.mode.name(),
        "branch": branch_report(st),
        "expanded": st.open,
        "collapsed": st.closed,
        "selected": selected,
    })
}

