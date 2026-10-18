//! The bottom dock: Problems, Search, Activity and the Tables and Generate slots.
//!
//! [`DockLayout`] (open, height, tab) is persisted in `UiState.dock`; [`show`] draws the dock as a
//! bottom panel after the side panes and before the canvas, so it sits between them. The panel's
//! top edge is the splitter. Everything in it works through commands: Check runs `validate.run`,
//! a fix runs its command as one `edit.batch`, Suppress runs `validate.suppress`, Activity undoes
//! with `edit.undo`. The UI-only commands are `ui.dock` and `ui.search` ([`ui_command`]).

pub(crate) mod activity;
mod generate;
pub(crate) mod problems;
pub(crate) mod search;
mod tables;

use egui::{Id, Ui};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::Tokens;

/// Smallest dock height, in points.
pub const MIN_HEIGHT: f32 = 90.0;
/// Largest dock height, in points.
pub const MAX_HEIGHT: f32 = 900.0;
/// Height the dock opens with.
pub const DEFAULT_HEIGHT: f32 = 230.0;

/// The tabs of the dock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DockTab {
    /// Issues from the live rules and the last check.
    #[default]
    Problems,
    /// Find results over text and shape data.
    Search,
    /// The transaction log, newest first.
    Activity,
    /// Derived tables (filled by a later block).
    Tables,
    /// Generator preview and run log (filled by a later block).
    Generate,
}

impl DockTab {
    /// Every tab in strip order.
    pub const ALL: [DockTab; 5] = [DockTab::Problems, DockTab::Search, DockTab::Activity, DockTab::Tables, DockTab::Generate];

    /// The name used in params and `ui.inspect`.
    pub fn name(self) -> &'static str {
        match self {
            DockTab::Problems => "problems",
            DockTab::Search => "search",
            DockTab::Activity => "activity",
            DockTab::Tables => "tables",
            DockTab::Generate => "generate",
        }
    }

    /// The tab for a name (any case); `None` for anything else.
    pub fn parse(s: &str) -> Option<DockTab> {
        let s = s.trim().to_ascii_lowercase();
        DockTab::ALL.into_iter().find(|t| t.name() == s)
    }

    /// The label on the tab.
    pub fn title(self) -> &'static str {
        match self {
            DockTab::Problems => "Problems",
            DockTab::Search => "Search",
            DockTab::Activity => "Activity",
            DockTab::Tables => "Tables",
            DockTab::Generate => "Generate",
        }
    }
}

// A saved layout naming a tab this build does not have opens on Problems instead of failing the
// whole preferences file.
impl<'de> Deserialize<'de> for DockTab {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(v.as_str().and_then(DockTab::parse).unwrap_or_default())
    }
}

/// Where the dock is and what it shows. Persisted; the transient state next to it is not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DockLayout {
    /// The dock is showing.
    pub open: bool,
    /// Its height in points.
    pub height: f32,
    /// The tab showing.
    pub tab: DockTab,
    /// Filters, the last check and the search box. Not saved with the preferences.
    #[serde(skip)]
    pub state: DockState,
}

impl Default for DockLayout {
    fn default() -> Self {
        DockLayout { open: false, height: DEFAULT_HEIGHT, tab: DockTab::Problems, state: DockState::default() }
    }
}

impl DockLayout {
    /// Keep the height something the window can show: junk from `ui.set` or a hand-edited
    /// preferences file falls back to the default.
    pub fn sanitize(&mut self) {
        if !self.height.is_finite() {
            self.height = DEFAULT_HEIGHT;
        }
        self.height = self.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    }
}

/// What the dock remembers between frames but not between runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockState {
    /// Problems: the last `validate.run` result and the revision it was taken at.
    pub checked: Option<(u64, Vec<vizcraft_diagram::vizcraft_model::Issue>)>,
    /// Problems: hide errors.
    pub hide_errors: bool,
    /// Problems: hide warnings.
    pub hide_warnings: bool,
    /// Problems: hide infos.
    pub hide_infos: bool,
    /// Problems: only rows whose message, rule or page contains this text.
    pub filter: String,
    /// Problems: the issue key being suppressed and the reason typed so far.
    pub suppressing: Option<(String, String)>,
    /// Search: the text in the field.
    pub query: String,
    /// Search: look in every page instead of the active one.
    pub document: bool,
    /// Search: the hits of the last search and the (query, scope, revision) they were found for.
    pub hits: Vec<search::Hit>,
    pub(crate) searched: Option<(String, bool, u64)>,
    /// Activity: the undo and redo depths at a revision (counting them costs a pass over the
    /// history, so it is done once per revision).
    pub(crate) depths: Option<(u64, usize, usize)>,
}

/// Draw the dock when it is open. Call after the side panes and before the central panel.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    app.ui.dock.sanitize();
    if !app.ui.dock.open {
        return;
    }
    problems::refresh(app);
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let want = app.ui.dock.height;
    // The panel remembers the height the splitter was dragged to; a height set by `ui.dock` or
    // `ui.set` is pushed in only when it differs from what the panel last reported.
    let height_id = Id::new("dock-height");
    let last: Option<f32> = ctx.data(|d| d.get_temp(height_id));
    let max = (ui.available_height() * 0.6).clamp(MIN_HEIGHT, MAX_HEIGHT);
    let frame = egui::Frame::NONE.fill(t.panel).stroke(egui::Stroke::new(1.0, t.border));
    let mut panel = egui::Panel::bottom("dock").resizable(true).size_range(MIN_HEIGHT..=max).default_size(want.min(max)).frame(frame);
    if last.is_some_and(|l| (l - want).abs() > 0.5) {
        panel = panel.exact_size(want.min(max));
    }
    let r = panel.show(ui, |ui| {
        strip(app, ui);
        ui.separator();
        match app.ui.dock.tab {
            DockTab::Problems => problems::show(app, ui),
            DockTab::Search => search::show(app, ui),
            DockTab::Activity => activity::show(app, ui),
            DockTab::Tables => tables::show(app, ui),
            DockTab::Generate => generate::show(app, ui),
        }
    });
    let h = r.response.rect.height().clamp(MIN_HEIGHT, MAX_HEIGHT);
    app.ui.dock.height = h;
    ctx.data_mut(|d| d.insert_temp(height_id, h));
}

/// The tab strip with the close button at the right.
fn strip(app: &mut CraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        for tab in DockTab::ALL {
            let on = app.ui.dock.tab == tab;
            if ui.selectable_label(on, tab.title()).clicked() {
                app.ui.dock.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            if crate::widgets::icon_button(ui, "close", "Close the dock", 14.0, true) {
                app.ui.dock.open = false;
            }
        });
    });
}

/// Open the dock on `tab` (the Check button and `ui.dock` both end here).
pub(crate) fn open_on(app: &mut CraftApp, tab: DockTab) {
    app.ui.dock.open = true;
    app.ui.dock.tab = tab;
}

/// The dock's state as `ui.inspect` reports it: `{open, height, tab, rows}`, where `rows` counts
/// what the showing tab lists (zero for the empty slots).
pub fn inspect(app: &CraftApp) -> Value {
    let d = &app.ui.dock;
    let rows = match d.tab {
        DockTab::Problems => problems::rows(app).len(),
        DockTab::Search => d.state.hits.len(),
        DockTab::Activity => app.session.transactions().len(),
        DockTab::Tables | DockTab::Generate => 0,
    };
    json!({"open": d.open, "height": d.height, "tab": d.tab.name(), "rows": rows})
}

/// `ui.dock` and `ui.search`, and the person's `validate.run` (Check opens the dock); `None` for any
/// other id.
///
/// - `ui.dock {open?: bool, tab?: "problems"|"search"|"activity"|"tables"|"generate", height?}`:
///   with `open` absent, naming a tab opens the dock on it and naming none toggles it
/// - `ui.search {query, scope?: "page"|"document"}`: fill the Search tab, run the search and show it;
///   the reply lists the hits
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    match id {
        "ui.dock" => Some(dock_command(app, p)),
        "ui.search" => Some(search::command(app, p)),
        "validate.run" => problems::on_run(app, p),
        _ => None,
    }
}

fn dock_command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let tab = match p.get("tab").and_then(Value::as_str) {
        None => None,
        Some(name) => {
            Some(DockTab::parse(name).ok_or_else(|| format!("unknown dock tab {name:?} (problems, search, activity, tables or generate)"))?)
        }
    };
    if let Some(h) = p.get("height").and_then(Value::as_f64) {
        app.ui.dock.height = h as f32;
    }
    let open = p.get("open").and_then(Value::as_bool).unwrap_or(if tab.is_some() { true } else { !app.ui.dock.open });
    app.ui.dock.open = open;
    if let Some(t) = tab {
        app.ui.dock.tab = t;
    }
    app.ui.dock.sanitize();
    Ok(inspect(app))
}
