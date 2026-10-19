//! The Patterns pane (key `patterns`, left side, closed by default): the patterns of the active
//! page's pack first, then the others under their pack names, filtered by audience and a search
//! field. A click opens the pattern form (`crate::pattern_dialog`), whose Insert runs
//! `pattern.insert`. When the selection belongs to an instance the pane shows that instance's
//! form with its current arguments and an Apply button running `pattern.reparameterize`; an
//! instance of a table-driven pattern also gets a Sync button, which runs `pattern.sync
//! {direction: toTable}` and puts the rows read back from the diagram into the form.
//!
//! `ui.patterns {query?, pattern?}` sets the filter or opens the form; `ui.inspect` reports
//! `patterns: {query, audience, instance?, args?, sync, open}`.

use egui::Ui;
use serde_json::{Value, json};
use vizcraft_engine::cmd::pattern::patterns::{self as vizcraft_patterns, Pattern};
use vizcraft_engine::cmd::pattern::sync::has_table;
use vizcraft_engine::cmd::pattern::{instance_of_selection, instance_param};

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::pattern_dialog::{self, State};
use crate::theme::Tokens;

/// The Patterns pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("patterns", "Patterns", Side::Left, 260.0, show);

/// Longest search text kept.
const MAX_QUERY: usize = 256;

/// What the pane shows: the search text, the audience filter and the selected instance's form.
/// Transient: never saved with the preferences.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternsState {
    /// The search field.
    pub query: String,
    /// Keep patterns serving this audience; empty keeps every pattern.
    pub audience: String,
    /// The form of the instance the selection belongs to, while it does.
    pub form: Option<State>,
}

/// The pack the active page draws with: its viewpoint's pack, else the first pack the model uses.
fn active_pack(app: &CraftApp) -> String {
    let page = app.session.doc.page();
    let vp = page.and_then(|p| p.view.as_ref()).map(|v| vizcraft_diagram::vizcraft_model::pack_of(&v.viewpoint).to_string()).unwrap_or_default();
    if !vp.is_empty() {
        return vp;
    }
    app.session.doc.model.packs.first().map(|p| p.id.clone()).unwrap_or_default()
}

/// Whether `p` passes the search text and the audience filter.
fn matches(p: &Pattern, query: &str, audience: &str) -> bool {
    let q = query.trim().to_lowercase();
    let text = q.is_empty() || [&p.name, &p.id, &p.description].iter().any(|f| f.to_lowercase().contains(&q));
    text && (audience.is_empty() || p.audience.iter().any(|a| a == audience))
}

/// Every available pattern regardless of the pane's search, the active pack's first (the insert
/// picker lists these).
pub fn all(app: &CraftApp) -> Vec<&'static Pattern> {
    let first = active_pack(app);
    let mut list = vizcraft_patterns::list(vizcraft_notation::catalog(), None, None);
    list.sort_by_key(|p| p.pack != first);
    list
}

/// The patterns to list, grouped by pack: the active pack first, then the others in catalog
/// order. Each group is (pack name, patterns).
pub fn groups(app: &CraftApp) -> Vec<(String, Vec<&'static Pattern>)> {
    let cat = vizcraft_notation::catalog();
    let st = &app.patterns;
    let first = active_pack(app);
    let mut packs: Vec<&str> = cat.packs().iter().map(|p| p.id.as_str()).collect();
    packs.sort_by_key(|p| *p != first.as_str());
    packs
        .into_iter()
        .filter_map(|pack| {
            let list: Vec<&'static Pattern> =
                vizcraft_patterns::list(cat, Some(pack), None).into_iter().filter(|p| matches(p, &st.query, &st.audience)).collect();
            (!list.is_empty()).then(|| (cat.pack(pack).map(|k| k.name.clone()).unwrap_or_else(|| pack.to_string()), list))
        })
        .collect()
}

/// Every audience some pattern serves, sorted.
fn audiences() -> Vec<String> {
    let mut a: Vec<String> = vizcraft_patterns::available(vizcraft_notation::catalog()).iter().flat_map(|p| p.audience.iter().cloned()).collect();
    a.sort();
    a.dedup();
    a
}

/// Keep `app.patterns.form` on the instance the selection belongs to.
fn sync_form(app: &mut CraftApp) {
    let Some(iid) = instance_of_selection(&app.session) else {
        app.patterns.form = None;
        return;
    };
    if app.patterns.form.as_ref().is_some_and(|f| f.instance == iid) {
        return;
    }
    app.patterns.form = app.session.doc.model.pattern(&iid).and_then(|p| State::for_instance(&p.pattern, &p.id, &p.args));
}

fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    sync_form(app);
    let mut query = app.patterns.query.clone();
    let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search patterns").desired_width(f32::INFINITY).id_salt("patterns_search"));
    if r.changed() {
        app.patterns.query = query.chars().take(MAX_QUERY).collect();
    }
    let all = audiences();
    let shown = if app.patterns.audience.is_empty() { "Every audience".to_string() } else { app.patterns.audience.clone() };
    let mut pick = app.patterns.audience.clone();
    egui::ComboBox::from_id_salt("patterns_audience").selected_text(shown).width(180.0).show_ui(ui, |ui| {
        ui.selectable_value(&mut pick, String::new(), "Every audience");
        for a in &all {
            ui.selectable_value(&mut pick, a.clone(), a);
        }
    });
    app.patterns.audience = pick;
    ui.add_space(6.0);

    if let Some(mut form) = app.patterns.form.take() {
        let name = vizcraft_patterns::find(&form.pattern).map(|p| p.name.clone()).unwrap_or_default();
        ui.label(egui::RichText::new(format!("Selected: {name}")).strong());
        if let Some(p) = vizcraft_patterns::find(&form.pattern) {
            pattern_dialog::fields(ui, p, &mut form);
        }
        if !form.error.is_empty() {
            ui.label(egui::RichText::new(&form.error).color(t.red));
        }
        let table = has_table(&app.session, &form.instance);
        let (apply, sync) = ui
            .horizontal(|ui| {
                let apply = ui.button("Apply").on_hover_text("Regenerate the instance with these values").clicked();
                let sync = table && ui.button("Sync").on_hover_text("Read the table back from the diagram into these rows").clicked();
                (apply, sync)
            })
            .inner;
        if apply {
            match pattern_dialog::submit(app, &form) {
                Ok(_) => form.error.clear(),
                Err(e) => form.error = e,
            }
        }
        if sync {
            sync_rows(app, &mut form);
        }
        // The instance may have changed group id or gone; keep the form only while it is selected.
        if instance_of_selection(&app.session).is_some_and(|i| i == form.instance) {
            app.patterns.form = Some(form);
        }
        ui.separator();
    }

    let groups = groups(app);
    if groups.is_empty() {
        ui.label(egui::RichText::new("No patterns match").color(t.text_dim));
        return;
    }
    let mut open = None;
    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        for (pack, list) in &groups {
            ui.label(egui::RichText::new(pack).small().color(t.group_label));
            for p in list {
                if ui.add(egui::Button::new(&p.name).frame(false)).on_hover_text(&p.description).clicked() {
                    open = Some(p.id.clone());
                }
            }
            ui.add_space(4.0);
        }
    });
    if let Some(id) = open {
        let _ = app.run("ui.patterns", json!({"pattern": id}));
    }
}

/// Run `pattern.sync {direction: toTable}` for the form's instance and put the rows it reads
/// back into the form's table field; a refusal shows its reason under the form.
fn sync_rows(app: &mut CraftApp, form: &mut State) {
    let key = vizcraft_patterns::find(&form.pattern).and_then(|p| vizcraft_patterns::tables::table_param(p).map(str::to_string));
    let Some(key) = key else { return };
    match app.run("pattern.sync", json!({"instance": form.instance, "direction": "toTable"})) {
        Ok(v) => {
            form.values.insert(key, v["table"]["rows"].clone());
            form.error.clear();
        }
        Err(e) => form.error = e,
    }
}

/// `ui.patterns {query?, pattern?}`: set the search text (opening the pane), and open the form
/// for `pattern`. Returns what [`inspect`] reports.
pub fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    if let Some(q) = p.get("query") {
        let q = q.as_str().ok_or("`query` must be a string")?;
        app.patterns.query = q.chars().take(MAX_QUERY).collect();
        if !app.ui.panes.is_open("patterns") {
            app.ui.panes.open("patterns", None);
        }
        app.ui.panes.set_collapsed("patterns", false);
    }
    if let Some(id) = p.get("pattern") {
        let id = id.as_str().ok_or("`pattern` must be a pattern id")?;
        let short: String = id.chars().take(64).collect();
        let state = State::new(id).ok_or_else(|| format!("no pattern `{short}`"))?;
        app.dialog = Some(crate::dialogs::Dialog::Pattern(state));
    }
    Ok(inspect(app))
}

/// The pane's state for `ui.inspect`: `{query, audience, instance?, open}`.
pub fn inspect(app: &CraftApp) -> Value {
    let instance = instance_of_selection(&app.session);
    let args = instance.as_ref().and_then(|i| instance_param(&app.session, &json!({"instance": i})).ok()).and_then(|i| app.session.doc.model.pattern(&i)).map(|p| p.args.clone());
    json!({
        "query": app.patterns.query,
        "audience": app.patterns.audience,
        "instance": instance,
        "args": args,
        "sync": instance.as_ref().is_some_and(|i| has_table(&app.session, i)),
        "open": app.ui.panes.is_open("patterns"),
    })
}
