//! The Analysis tab: the stored result of the last analysis (`Session.analysis`) as rows, with
//! Go to, Select, Re-run and Clear, plus the options the Analyze ribbon group runs with.
//!
//! The rows are read from the stored result every frame, so the tab never holds a copy that can
//! drift. The tab keeps only the user's options (depth, relationship types, containment, Dim,
//! Live, the compare filter), the command that produced the result (for Re-run) and the selected
//! row. Every `analyze.*` command and `model.diff` comes through [`on_run`] from every origin, so
//! a result an agent asked for shows here as well. Nothing here changes the document or makes an
//! undo step: analyses are session state, Go to only switches page, selects and pans.

use std::collections::HashSet;

use egui::{RichText, Ui};
use serde_json::{Map, Value, json};
use vizcraft_analysis::{AnalysisResult, MAX_DEPTH_PARAM, MAX_RESULT_IDS, MAX_TYPES, Mark};
use vizcraft_diagram::Page;
use vizcraft_engine::{Origin, Session};

use super::DockTab;
use crate::CraftApp;
use crate::canvas::draw::highlights::{self, all_ghosts, legend_name, mark_color, mark_name};
use crate::theme::Tokens;

/// The commands whose results this tab shows; [`super::ui_command`] sends each to [`on_run`].
pub const RESULT_COMMANDS: &[&str] = &[
    "analyze.dependencies",
    "analyze.cycles",
    "analyze.unreachable",
    "analyze.spof",
    "analyze.metrics",
    "analyze.path",
    "analyze.clear",
    "model.diff",
    // The mechanical and reliability analyses (b6e-mechanical).
    "mech.mobility",
    "mech.ratio",
    "mech.mason",
    "rel.cutsets",
    "rel.availability",
    "rel.outcomes",
];

/// Longest relationship type id the Follow menu keeps.
const MAX_TYPE_LEN: usize = 256;
/// Longest row id `ui.analysis` accepts.
const MAX_ROW_LEN: usize = 1024;

/// Which compare rows show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffFilter {
    /// Added, removed and changed.
    #[default]
    All,
    /// Only items the current model added.
    Added,
    /// Only items the compared file had and the model lost.
    Removed,
    /// Only items whose fields differ.
    Changed,
}

impl DiffFilter {
    /// Every filter, in chip order.
    pub const ALL: [DiffFilter; 4] = [DiffFilter::All, DiffFilter::Added, DiffFilter::Removed, DiffFilter::Changed];

    /// The name in params and `ui.inspect`.
    pub fn name(self) -> &'static str {
        match self {
            DiffFilter::All => "all",
            DiffFilter::Added => "added",
            DiffFilter::Removed => "removed",
            DiffFilter::Changed => "changed",
        }
    }

    /// The filter a name means (any case).
    pub fn parse(s: &str) -> Option<DiffFilter> {
        let s = s.trim().to_ascii_lowercase();
        DiffFilter::ALL.into_iter().find(|f| f.name() == s)
    }

    fn title(self) -> &'static str {
        match self {
            DiffFilter::All => "All",
            DiffFilter::Added => "Added",
            DiffFilter::Removed => "Removed",
            DiffFilter::Changed => "Changed",
        }
    }

    fn shows(self, section: DiffFilter) -> bool {
        self == DiffFilter::All || self == section
    }
}

/// The Analysis tab's state: the user's options and what Re-run repeats. Not saved.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisState {
    /// How many steps Dependencies walks; `None` is all of them.
    pub depth: Option<u32>,
    /// The relationship types followed; empty follows every link.
    pub types: Vec<String>,
    /// Containment (owner to owned) counts as a link.
    pub containment: bool,
    /// Wash out what the result does not mark.
    pub dim: bool,
    /// Re-run a graph analysis after every edit.
    pub live: bool,
    /// Which compare rows show.
    pub diff_filter: DiffFilter,
    /// The command and params of the last successful run, for Re-run.
    pub last: Option<(String, Value)>,
    /// The selected row's id.
    pub selected: Option<String>,
    /// The metrics column sorted by (its index in the table) and whether descending.
    pub sort: Option<(usize, bool)>,
    /// The revision a live re-run last tried, so a failing one is not retried every frame.
    tried: Option<u64>,
}

impl Default for AnalysisState {
    fn default() -> Self {
        AnalysisState {
            depth: None,
            types: Vec::new(),
            containment: false,
            dim: true,
            live: false,
            diff_filter: DiffFilter::All,
            last: None,
            selected: None,
            sort: None,
            tried: None,
        }
    }
}

/// The result kind a command produces: `analyze.cycles` gives `cycles`, `model.diff` gives `diff`.
fn kind_of(cmd: &str) -> &str {
    if cmd == "model.diff" {
        "diff"
    } else {
        // `mech.mobility` and `rel.cutsets` give the analysis of their own local name.
        cmd.strip_prefix("analyze.").or_else(|| cmd.strip_prefix("mech.")).or_else(|| cmd.strip_prefix("rel.")).unwrap_or(cmd)
    }
}

/// `params` with the tab's options in place: `depth` for Dependencies, `types` and
/// `containment` for every graph analysis, each left out at its default. A compare takes none.
fn apply_options(st: &AnalysisState, cmd: &str, params: Value) -> Value {
    let mut o = match params {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    if cmd == "model.diff" || cmd == "analyze.clear" {
        return Value::Object(o);
    }
    o.remove("depth");
    if cmd == "analyze.dependencies"
        && let Some(d) = st.depth
    {
        o.insert("depth".into(), json!(d));
    }
    o.remove("types");
    if !st.types.is_empty() {
        o.insert("types".into(), json!(st.types));
    }
    o.remove("containment");
    if st.containment {
        o.insert("containment".into(), json!(true));
    }
    Value::Object(o)
}

/// `base` with the tab's options, for the Analyze ribbon buttons.
pub(crate) fn with_options(app: &CraftApp, cmd: &str, base: Value) -> Value {
    apply_options(&app.ui.dock.state.analysis, cmd, base)
}

/// An `analyze.*` command or `model.diff`, from any origin: run it, keep it for Re-run, and when
/// the person asked for it (a UI-origin run) open the dock on Analysis. `None` for other ids.
pub(crate) fn on_run(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if !RESULT_COMMANDS.contains(&id) {
        return None;
    }
    let r = app.session.run(id, p).map_err(|e| e.to_string());
    app.after_command();
    match &r {
        Err(e) => app.status(e.clone()),
        Ok(v) => {
            let st = &mut app.ui.dock.state.analysis;
            st.selected = None;
            st.tried = None;
            if id == "analyze.clear" {
                st.last = None;
            } else {
                let mut params = match p {
                    Value::Object(o) => o.clone(),
                    _ => Map::new(),
                };
                // Dependencies run on the selection: keep the element it started from, so a
                // Re-run after the selection moves on repeats the same question.
                if id == "analyze.dependencies"
                    && params.get("id").is_none_or(Value::is_null)
                    && let Some(first) = v.pointer("/levels/0/0").filter(|x| x.is_string())
                {
                    params.insert("id".into(), first.clone());
                }
                st.last = Some((id.to_string(), Value::Object(params)));
                if app.session.origin() == Origin::Ui {
                    super::open_on(app, DockTab::Analysis);
                }
            }
        }
    }
    Some(r)
}

/// Whether Re-run can repeat the stored result: the command that made it is known.
fn can_rerun(app: &CraftApp) -> bool {
    let st = &app.ui.dock.state.analysis;
    match (&st.last, &app.session.analysis) {
        (Some((cmd, _)), Some(r)) => kind_of(cmd) == r.kind,
        _ => false,
    }
}

/// Run the last command again, with the tab's options when `options` is set, keeping the
/// selected row when the new result still lists it.
fn rerun(app: &mut CraftApp, options: bool) -> Option<Result<Value, String>> {
    let (cmd, params) = app.ui.dock.state.analysis.last.clone()?;
    let params = if options { apply_options(&app.ui.dock.state.analysis, &cmd, params) } else { params };
    let keep = app.ui.dock.state.analysis.selected.clone();
    let r = on_run(app, &cmd, &params);
    if let Some(id) = keep
        && app.session.analysis.as_ref().is_some_and(|a| a.ids().contains(&id))
    {
        app.ui.dock.state.analysis.selected = Some(id);
    }
    r
}

/// Every frame, before the dock is drawn (open or not): forget the last command once the result
/// is gone (Clear, a new document), and with Live on re-run a stale graph analysis. A compare is
/// never re-run live: it reads a file.
pub(crate) fn refresh(app: &mut CraftApp) {
    if app.session.analysis.is_none() {
        let st = &mut app.ui.dock.state.analysis;
        st.last = None;
        st.selected = None;
        return;
    }
    let st = &app.ui.dock.state.analysis;
    let rev = app.session.rev();
    if !st.live || !highlights::stale(&app.session) || st.tried == Some(rev) || !can_rerun(app) {
        return;
    }
    if st.last.as_ref().is_some_and(|(cmd, _)| cmd == "model.diff") {
        return;
    }
    let origin = app.session.set_origin(Origin::Script);
    let _ = rerun(app, false);
    app.session.set_origin(origin);
    app.ui.dock.state.analysis.tried = Some(rev);
}

/// One row of the tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The element or relationship id.
    pub id: String,
    /// Its name, else its id.
    pub name: String,
    /// Its type's label.
    pub ty: String,
    /// Why it is listed.
    pub why: String,
    /// How the canvas marks it.
    pub mark: Option<Mark>,
    /// Table cells (metrics), one per column.
    pub cells: Vec<Value>,
    /// The current model no longer has it (a compare's removed row).
    pub removed: bool,
}

/// A titled list of rows. A section with `group` set is one cycle: its header carries Go to.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// The heading.
    pub title: String,
    /// The ids Go to selects for the whole section (a cycle's members); empty for none.
    pub group: Vec<String>,
    /// The rows.
    pub rows: Vec<Row>,
}

/// The name of item `id`: the model's, else the compared file's (for a removed item), else the id.
fn name_of(session: &Session, id: &str) -> String {
    let m = &session.doc.model;
    let n = m.element(id).map(|e| e.name.clone()).or_else(|| m.relationship(id).map(|r| r.name.clone())).filter(|n| !n.is_empty());
    n.or_else(|| all_ghosts(session).into_iter().find(|g| g.id == id).map(|g| g.name)).unwrap_or_else(|| id.to_string())
}

/// The label of item `id`'s type, from the catalog.
fn type_of(session: &Session, id: &str) -> String {
    let cat = vizcraft_notation::catalog();
    let m = &session.doc.model;
    let tail = |t: &str| t.rsplit('.').next().unwrap_or(t).to_string();
    if let Some(e) = m.element(id) {
        return cat.element_type(&e.type_id).map(|(_, t)| t.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| tail(&e.type_id));
    }
    if let Some(r) = m.relationship(id) {
        return cat
            .relationship_type(&r.type_id)
            .map(|(_, t)| t.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| if r.type_id.is_empty() { "Link".into() } else { tail(&r.type_id) });
    }
    String::new()
}

fn row(session: &Session, r: &AnalysisResult, id: &str, why: String) -> Row {
    Row {
        id: id.to_string(),
        name: name_of(session, id),
        ty: type_of(session, id),
        why,
        mark: r.marks.get(id).copied(),
        cells: Vec::new(),
        removed: false,
    }
}

fn strs(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).take(MAX_RESULT_IDS).collect()
}

/// A field value as the Why column shows it.
fn shown(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "(none)".into(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// The metrics column's heading.
pub fn column_title(c: &str) -> String {
    match c {
        "fanIn" => "Fan-in".into(),
        "fanOut" => "Fan-out".into(),
        "layer" => "Layer".into(),
        "containmentDepth" => "Containment depth".into(),
        other => other.to_string(),
    }
}

fn cell_key(v: &Value) -> (f64, String) {
    (v.as_f64().unwrap_or(f64::NEG_INFINITY), v.as_str().unwrap_or_default().to_string())
}

/// The table rows (metrics, or a later pack's table), sorted by the tab's column when one is set.
fn table_rows(session: &Session, r: &AnalysisResult, sort: Option<(usize, bool)>) -> Vec<Row> {
    let Some(t) = &r.table else { return Vec::new() };
    let mut rows: Vec<Row> =
        t.rows.iter().take(MAX_RESULT_IDS).map(|x| Row { cells: x.cells.clone(), ..row(session, r, &x.id, String::new()) }).collect();
    if let Some((col, desc)) = sort {
        // A stable sort, so equal values keep the result's (model) order.
        rows.sort_by(|a, b| {
            let (ka, kb) = (a.cells.get(col).map(cell_key), b.cells.get(col).map(cell_key));
            let o = ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal);
            if desc { o.reverse() } else { o }
        });
    }
    rows
}

/// The compare rows of one kind of item (`elements` or `relationships` in the detail).
fn diff_rows(session: &Session, r: &AnalysisResult, key: &str, which: DiffFilter, what: &str) -> Vec<Row> {
    let d = r.detail.get(key);
    match which {
        DiffFilter::Added => strs(d.and_then(|d| d.get("added"))).iter().map(|id| row(session, r, id, format!("{what} added"))).collect(),
        DiffFilter::Removed => strs(d.and_then(|d| d.get("removed")))
            .iter()
            .map(|id| Row { ty: what.to_string(), mark: Some(Mark::Removed), removed: true, ..row(session, r, id, format!("{what} removed")) })
            .collect(),
        _ => d
            .and_then(|d| d.get("changed"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(MAX_RESULT_IDS)
            .filter_map(|c| {
                let id = c.get("id")?.as_str()?;
                let fields: Vec<String> = c
                    .get("fields")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .take(32)
                    .map(|f| format!("{}: {} \u{2192} {}", shown(f.get("field")), shown(f.get("before")), shown(f.get("after"))))
                    .collect();
                Some(row(session, r, id, fields.join("; ")))
            })
            .collect(),
    }
}

/// The rows of the stored result, by section, as the tab lists them.
pub fn sections(session: &Session, st: &AnalysisState) -> Vec<Section> {
    let Some(r) = &session.analysis else { return Vec::new() };
    let sec = |title: &str, rows: Vec<Row>| Section { title: title.to_string(), group: Vec::new(), rows };
    let els = |why: &dyn Fn(&str) -> String| r.elements.iter().map(|id| row(session, r, id, why(id))).collect::<Vec<_>>();
    let rels = |why: &str| r.relationships.iter().map(|id| row(session, r, id, why.to_string())).collect::<Vec<_>>();
    let mut out = match r.kind.as_str() {
        "dependencies" => {
            let levels: Vec<Vec<String>> = r.detail.get("levels").and_then(Value::as_array).into_iter().flatten().map(|l| strs(Some(l))).collect();
            let level = |id: &str| levels.iter().position(|l| l.iter().any(|x| x == id));
            let why = |id: &str| match (r.marks.get(id), level(id)) {
                (Some(Mark::Focus), _) => "Start".to_string(),
                (Some(Mark::Upstream), Some(n)) => format!("Level {n} upstream"),
                (Some(_), Some(n)) => format!("Level {n} downstream"),
                _ => String::new(),
            };
            vec![sec("Elements", els(&why)), sec("Relationships", rels("Link followed"))]
        }
        "cycles" => r
            .groups
            .iter()
            .map(|g| {
                let n = g.elements.len();
                let mut rows: Vec<Row> = g.elements.iter().map(|id| row(session, r, id, "On the cycle".into())).collect();
                rows.extend(g.relationships.iter().map(|id| row(session, r, id, "Closes the cycle".into())));
                Section {
                    title: format!("{}, {n} {}", g.label, if n == 1 { "element" } else { "elements" }),
                    group: g.elements.iter().chain(&g.relationships).cloned().collect(),
                    rows,
                }
            })
            .collect(),
        "unreachable" => vec![sec("Elements", els(&|_| "No root reaches it".into()))],
        "spof" => vec![sec("Cut points", els(&|_| "Removing it splits the graph".into())), sec("Bridges", rels("Removing it splits the graph"))],
        "path" => {
            let mut steps = Vec::new();
            for (i, e) in r.elements.iter().enumerate() {
                steps.push(row(session, r, e, format!("Step {i}")));
                if let Some(rel) = r.relationships.get(i) {
                    steps.push(row(session, r, rel, "Link".into()));
                }
            }
            vec![sec("Steps", steps)]
        }
        "metrics" => vec![sec("Elements", table_rows(session, r, st.sort))],
        "diff" => {
            let mut v = Vec::new();
            for which in [DiffFilter::Added, DiffFilter::Removed, DiffFilter::Changed] {
                if st.diff_filter.shows(which) {
                    let mut rows = diff_rows(session, r, "elements", which, "Element");
                    rows.extend(diff_rows(session, r, "relationships", which, "Relationship"));
                    v.push(sec(which.title(), rows));
                }
            }
            v
        }
        _ => {
            let why = |id: &str| r.marks.get(id).map(|m| mark_name(*m).to_string()).unwrap_or_default();
            let mut v =
                vec![sec("Elements", els(&why)), sec("Relationships", r.relationships.iter().map(|id| row(session, r, id, why(id))).collect())];
            if r.table.is_some() {
                v.push(sec("Table", table_rows(session, r, st.sort)));
            }
            v
        }
    };
    out.retain(|s| !s.rows.is_empty() || r.kind == "diff");
    out
}

/// How many rows the tab lists.
pub fn count(session: &Session, st: &AnalysisState) -> usize {
    sections(session, st).iter().map(|s| s.rows.len()).sum()
}

/// The label of a result kind: the analysis's own, "Compare" for a model compare.
fn kind_label(kind: &str) -> String {
    if kind == "diff" {
        return "Compare".into();
    }
    vizcraft_analysis::analysis(kind).map(|a| a.label().to_string()).unwrap_or_else(|| kind.to_string())
}

/// The items of `page` that draw any of `ids`.
fn drawn_on(page: &Page, ids: &HashSet<&str>) -> Vec<String> {
    let hit = |x: Option<&str>| x.is_some_and(|x| ids.contains(x));
    page.shapes
        .iter()
        .filter(|s| hit(s.element.as_deref()))
        .map(|s| s.id.clone())
        .chain(page.connectors.iter().filter(|c| hit(c.relationship.as_deref())).map(|c| c.id.clone()))
        .take(MAX_RESULT_IDS)
        .collect()
}

/// The page to show `ids` on (the active page when it draws any of them, else the first page
/// that does) and the items there that draw them.
pub fn goto_target(session: &Session, ids: &[String]) -> Option<(String, Vec<String>)> {
    let want: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let active = session.doc.page().map(|p| (p.id.clone(), drawn_on(p, &want))).filter(|(_, items)| !items.is_empty());
    active.or_else(|| session.doc.pages.iter().map(|p| (p.id.clone(), drawn_on(p, &want))).find(|(_, items)| !items.is_empty()))
}

/// Centre the view on world point `at` when it is off screen.
fn reveal(app: &mut CraftApp, at: vizcraft_diagram::Point) {
    let Some(vp) = app.canvas.viewport(&app.session.view) else { return };
    if !vp.visible_world_rect().contains(at) {
        let _ = app.run("view.pan", json!({"x": at.x, "y": at.y}));
    }
}

/// Go to `ids`: switch to the page drawing them, select their shapes and connectors there and
/// bring them into view.
pub(crate) fn goto_ids(app: &mut CraftApp, ids: &[String]) {
    let Some((page, items)) = goto_target(&app.session, ids) else {
        let what = ids.first().map(|id| name_of(&app.session, id)).unwrap_or_default();
        app.status(format!("{what} is not drawn on any page"));
        return;
    };
    if app.session.doc.page().is_none_or(|p| p.id != page) {
        let _ = app.run("page.setActive", json!({"page": page}));
    }
    let _ = app.run("edit.select", json!({"ids": items}));
    let Some(p) = app.session.doc.page() else { return };
    let mut bounds: Option<vizcraft_diagram::Rect> = None;
    for id in &items {
        let b = p
            .shape(id)
            .map(|s| s.bounds())
            .or_else(|| p.connector(id).and_then(|c| vizcraft_diagram::Rect::bounding(vizcraft_diagram::route_polyline(p, c))));
        if let Some(b) = b {
            bounds = Some(bounds.map_or(b, |u| u.union(&b)));
        }
    }
    if let Some(b) = bounds {
        reveal(app, b.center());
    }
}

/// Go to row `id`. A removed row goes to its ghost, when this document has the page the compared
/// file drew it on.
pub(crate) fn goto(app: &mut CraftApp, id: &str) {
    let removed = app.session.analysis.as_ref().is_some_and(|r| r.kind == "diff") && !app.session.doc.model.contains_id(id);
    if !removed {
        goto_ids(app, &[id.to_string()]);
        return;
    }
    let ghosts: Vec<highlights::Ghost> = all_ghosts(&app.session).into_iter().filter(|g| g.id == id).collect();
    let active = app.session.doc.page().map(|p| p.id.clone()).unwrap_or_default();
    let here = ghosts.iter().find(|g| g.page == active).or_else(|| ghosts.iter().find(|g| app.session.doc.pages.iter().any(|p| p.id == g.page)));
    let Some(g) = here.cloned() else {
        let msg = match ghosts.first() {
            Some(g) => format!("The compared file drew {} on page {}, which this document does not have", g.name, g.page_name),
            None => format!("The compared file did not draw {}", name_of(&app.session, id)),
        };
        app.status(msg);
        return;
    };
    if g.page != active {
        let _ = app.run("page.setActive", json!({"page": g.page}));
    }
    let _ = app.run("edit.selectNone", json!({}));
    if let Some(c) = g.centre() {
        reveal(app, c);
    }
}

/// The tab's state as `ui.analysis` replies with it; `kind` is null with no result.
pub fn state(app: &CraftApp) -> Value {
    let s = &app.session;
    let st = &app.ui.dock.state.analysis;
    let r = s.analysis.as_ref();
    let page = s.doc.page();
    let highlighted = page.map_or(0, |p| highlights::targets(s, p).len());
    let ghosts = page.map_or(0, |p| highlights::ghosts(s, p).len());
    let secs: Vec<Value> = sections(s, st).iter().map(|x| json!({"title": x.title, "rows": x.rows.len()})).collect();
    json!({
        "kind": r.map(|r| r.kind.clone()),
        "summary": r.map(|r| r.summary.clone()),
        "count": count(s, st),
        "sections": secs,
        "rev": r.map(|r| r.rev),
        "stale": highlights::stale(s),
        "truncated": r.is_some_and(|r| r.truncated),
        "highlighted": highlighted,
        "ghosts": ghosts,
        "selected": st.selected,
        "options": {
            "depth": st.depth,
            "types": st.types,
            "containment": st.containment,
            "dim": st.dim,
            "live": st.live,
            "filter": st.diff_filter.name(),
            "sort": st.sort.map(|(c, d)| json!({"column": c, "descending": d})),
        },
        "last": st.last.as_ref().map(|(c, p)| json!({"command": c, "params": p})),
    })
}

/// The `analysis` entry of `ui.inspect`: [`state`] while a result is stored, else null.
pub fn inspect(app: &CraftApp) -> Value {
    if app.session.analysis.is_none() { Value::Null } else { state(app) }
}

/// `ui.analysis {depth?: n|"all", types?: [id], containment?, dim?, live?, filter?, row?, goto?}`:
/// set the options (junk is ignored field by field, depth is clamped to 1..1024), re-run the
/// last graph analysis when depth, types or containment changed, select row `row` (and with
/// `goto` go to it), and open the dock on Analysis. The reply is [`state`].
pub fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let st = &mut app.ui.dock.state.analysis;
    let before = (st.depth, st.types.clone(), st.containment);
    match p.get("depth") {
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("all") => st.depth = None,
        Some(v) => {
            let n = v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())).filter(|n| n.is_finite());
            if let Some(n) = n {
                st.depth = Some(n.round().clamp(1.0, f64::from(MAX_DEPTH_PARAM)) as u32);
            }
        }
        None => {}
    }
    if let Some(a) = p.get("types").and_then(Value::as_array) {
        let mut types: Vec<String> = Vec::new();
        for t in a.iter().filter_map(Value::as_str).filter(|t| !t.is_empty() && t.len() <= MAX_TYPE_LEN) {
            if types.len() < MAX_TYPES && !types.iter().any(|x| x == t) {
                types.push(t.to_string());
            }
        }
        st.types = types;
    }
    let flag = |k: &str| p.get(k).and_then(Value::as_bool);
    if let Some(b) = flag("containment") {
        st.containment = b;
    }
    if let Some(b) = flag("dim") {
        st.dim = b;
    }
    if let Some(b) = flag("live") {
        st.live = b;
        st.tried = None;
    }
    if let Some(f) = p.get("filter").and_then(Value::as_str).and_then(DiffFilter::parse) {
        st.diff_filter = f;
    }
    let row = p.get("row").and_then(Value::as_str).filter(|r| !r.is_empty() && r.len() <= MAX_ROW_LEN).map(str::to_string);
    let changed = before != (st.depth, st.types.clone(), st.containment);
    let graph = st.last.as_ref().is_some_and(|(cmd, _)| cmd != "model.diff");
    if changed && graph && can_rerun(app) {
        let _ = rerun(app, true);
    }
    if let Some(r) = row {
        app.ui.dock.state.analysis.selected = Some(r.clone());
        if flag("goto") == Some(true) {
            goto(app, &r);
        }
    }
    super::open_on(app, DockTab::Analysis);
    Ok(state(app))
}

/// `ui.compareFile {}`: pick a VizCraft document, then `model.diff {path}` against it. With no
/// file picker (the web build, tests) or a cancelled pick it replies `{picked: false}` and
/// changes nothing.
pub fn compare_file(app: &mut CraftApp) -> Result<Value, String> {
    let Some(pick) = app.services.pick_open.as_ref() else {
        app.status("This front end cannot pick a file to compare with");
        return Ok(json!({"picked": false}));
    };
    let Some(path) = pick("document") else { return Ok(json!({"picked": false})) };
    let mut r = app.run("model.diff", json!({"path": path}))?;
    if let Some(o) = r.as_object_mut() {
        o.insert("picked".into(), Value::Bool(true));
    }
    Ok(r)
}

/// What a click in the tab asked for, applied after drawing (drawing borrows the rows).
enum Act {
    Run(&'static str, Value),
    Goto(String),
    GotoIds(Vec<String>),
    Rerun,
    Sort(usize),
}

/// The Analysis tab.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(r) = app.session.analysis.clone() else {
        ui.add_space(10.0);
        ui.label(RichText::new("Run an analysis from the Model tab.").color(t.text_dim));
        return;
    };
    let mut acts: Vec<Act> = Vec::new();
    header(app, ui, &r, &t, &mut acts);
    ui.separator();
    let secs = sections(&app.session, &app.ui.dock.state.analysis);
    let selected = app.ui.dock.state.analysis.selected.clone();
    egui::ScrollArea::vertical().id_salt("analysis-rows").auto_shrink([false, false]).show(ui, |ui| {
        if secs.iter().all(|s| s.rows.is_empty()) {
            ui.label(RichText::new("Nothing found.").color(t.text_dim));
        }
        for (i, s) in secs.iter().enumerate() {
            if !s.group.is_empty() {
                egui::CollapsingHeader::new(&s.title).id_salt(("analysis-group", i)).default_open(true).show(ui, |ui| {
                    if ui.small_button("Go to").on_hover_text("Select every item of this cycle").clicked() {
                        acts.push(Act::GotoIds(s.group.clone()));
                    }
                    rows_grid(ui, &r, s, i, selected.as_deref(), &t, &mut acts);
                });
            } else {
                ui.label(RichText::new(format!("{} ({})", s.title, s.rows.len())).small().strong().color(t.text_dim));
                if !s.rows.is_empty() {
                    rows_grid(ui, &r, s, i, selected.as_deref(), &t, &mut acts);
                }
            }
            ui.add_space(4.0);
        }
        if r.kind == "metrics" {
            metrics_footer(ui, &app.session, &r, &t);
        }
    });
    for a in acts {
        match a {
            Act::Run(id, p) => {
                let _ = app.run(id, p);
            }
            Act::Goto(id) => {
                app.ui.dock.state.analysis.selected = Some(id.clone());
                goto(app, &id);
            }
            Act::GotoIds(ids) => goto_ids(app, &ids),
            Act::Rerun => {
                let _ = rerun(app, false);
            }
            Act::Sort(col) => {
                let st = &mut app.ui.dock.state.analysis;
                st.sort = match st.sort {
                    Some((c, true)) if c == col => Some((col, false)),
                    _ => Some((col, true)),
                };
            }
        }
    }
}

/// The kind and summary, the out-of-date badge with Re-run, the truncation note, Select, Clear,
/// Dim and Live, the legend, and for a compare the filter chips.
fn header(app: &CraftApp, ui: &mut Ui, r: &AnalysisResult, t: &Tokens, acts: &mut Vec<Act>) {
    let st = &app.ui.dock.state.analysis;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(kind_label(&r.kind)).strong());
        ui.label(RichText::new(&r.summary).color(t.text_dim));
        if highlights::stale(&app.session) {
            let badge = egui::Frame::NONE.fill(t.amber.gamma_multiply(0.2)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 1));
            badge.show(ui, |ui| ui.label(RichText::new("Out of date").small().color(t.text)));
            let b = ui.add_enabled(can_rerun(app), egui::Button::new("Re-run").small());
            if b.on_hover_text("Run the same analysis on the document as it is now").clicked() {
                acts.push(Act::Rerun);
            }
        }
        if r.truncated {
            ui.label(RichText::new(format!("Truncated at {}", fmt_thousands(MAX_RESULT_IDS))).small().color(t.text_dim));
        }
        ui.separator();
        if ui.small_button("Select").on_hover_text("Select every item of the result on this page").clicked() {
            acts.push(Act::Run("analyze.select", json!({})));
        }
        if ui.small_button("Clear").on_hover_text("Clear the highlights and this result").clicked() {
            acts.push(Act::Run("analyze.clear", json!({})));
        }
        let mut dim = st.dim;
        if r.kind != "diff" && ui.checkbox(&mut dim, "Dim").on_hover_text("Fade what the result does not include").changed() {
            acts.push(Act::Run("ui.analysis", json!({"dim": dim})));
        }
        let mut live = st.live;
        if r.kind != "diff" && ui.checkbox(&mut live, "Live").on_hover_text("Run the analysis again after every edit").changed() {
            acts.push(Act::Run("ui.analysis", json!({"live": live})));
        }
    });
    ui.horizontal_wrapped(|ui| {
        let mut present: Vec<Mark> = r.marks.values().copied().collect();
        if r.kind == "diff" && !r.groups.is_empty() {
            present.push(Mark::Removed);
        }
        present.sort();
        present.dedup();
        for m in present {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, mark_color(t, m));
            ui.label(RichText::new(legend_name(&r.kind, m)).small());
            ui.add_space(6.0);
        }
        if r.kind == "unreachable" {
            let roots = strs(r.detail.get("roots"));
            let names: Vec<String> = roots.iter().take(20).map(|id| name_of(&app.session, id)).collect();
            let more = if roots.len() > names.len() { format!(" and {} more", roots.len() - names.len()) } else { String::new() };
            ui.label(
                RichText::new(format!("Roots: {}{more}", if names.is_empty() { "none".into() } else { names.join(", ") })).small().color(t.text_dim),
            );
        }
        if r.kind == "diff" {
            ui.separator();
            for f in DiffFilter::ALL {
                if ui.selectable_label(st.diff_filter == f, f.title()).clicked() {
                    acts.push(Act::Run("ui.analysis", json!({"filter": f.name()})));
                }
            }
        }
    });
}

/// `n` with thousands separators (10,000).
fn fmt_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One section's rows: Name (a click or Enter goes to it), Type, then Why or the table cells.
fn rows_grid(ui: &mut Ui, r: &AnalysisResult, s: &Section, i: usize, selected: Option<&str>, t: &Tokens, acts: &mut Vec<Act>) {
    let columns: Vec<String> = if r.table.is_some() && s.rows.iter().any(|x| !x.cells.is_empty()) {
        r.table.as_ref().map(|t| t.columns.clone()).unwrap_or_default()
    } else {
        Vec::new()
    };
    let n = 3 + columns.len().saturating_sub(1).min(16);
    egui::Grid::new(("analysis-grid", i)).num_columns(n.max(3)).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
        ui.label(RichText::new("Name").small().color(t.text_dim));
        ui.label(RichText::new("Type").small().color(t.text_dim));
        if columns.is_empty() {
            ui.label(RichText::new("Why").small().color(t.text_dim));
        } else {
            for (c, name) in columns.iter().enumerate().take(16) {
                if ui.add(egui::Button::new(RichText::new(column_title(name)).small()).frame(false)).on_hover_text("Sort by this column").clicked() {
                    acts.push(Act::Sort(c));
                }
            }
        }
        ui.end_row();
        for x in &s.rows {
            let on = selected == Some(x.id.as_str());
            ui.horizontal(|ui| {
                if let Some(m) = x.mark {
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 3.5, mark_color(t, m));
                }
                let text = if x.removed { RichText::new(&x.name).strikethrough() } else { RichText::new(&x.name) };
                let resp = ui.selectable_label(on, text).on_hover_text(&x.id);
                let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if resp.clicked() || enter {
                    acts.push(Act::Goto(x.id.clone()));
                }
            });
            ui.label(RichText::new(&x.ty).color(t.text_dim));
            if columns.is_empty() {
                ui.label(&x.why);
            } else {
                for c in x.cells.iter().take(16) {
                    ui.label(match c {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    });
                }
            }
            ui.end_row();
        }
    });
}

/// The metrics totals and the critical path under the table.
fn metrics_footer(ui: &mut Ui, session: &Session, r: &AnalysisResult, t: &Tokens) {
    let tot = r.detail.get("totals");
    let n = |k: &str| tot.and_then(|x| x.get(k)).and_then(Value::as_u64).unwrap_or(0);
    ui.label(
        RichText::new(format!(
            "Totals: {} elements, {} relationships, {} components, {} cycles, longest path {}",
            n("elements"),
            n("relationships"),
            n("components"),
            n("cycles"),
            n("longestPath")
        ))
        .small()
        .color(t.text_dim),
    );
    let crit = strs(r.detail.get("critical").and_then(|c| c.get("elements")));
    if !crit.is_empty() {
        let names: Vec<String> = crit.iter().take(40).map(|id| name_of(session, id)).collect();
        ui.label(RichText::new(format!("Critical path: {}", names.join(" \u{2192} "))).small().color(t.text_dim));
    }
}
