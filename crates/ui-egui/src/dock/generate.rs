//! The Generate tab: pick a generator, preview what it would write, generate and check.
//!
//! A spec row (spec file with Browse, target menu, output folder) sits above Preview, Generate and
//! Check. Below, the files of the last plan with an action badge each, the issues and a run log on
//! the left, and the selected file's unified diff in JetBrains Mono on the right, added and
//! removed lines coloured from `Tokens`. Every button runs an engine command
//! (`generate.preview`, `generate.run`, `generate.check`) with the fields as params; the same
//! commands from the ribbon or palette, run by the person with no params, use these fields and
//! land here too. `ui.generate {spec?, target?, output?, run?}` sets the fields from outside.

use egui::{Color32, RichText, Ui};
use serde_json::{Value, json};
use vizcraft_engine::Origin;

use super::DockTab;
use crate::CraftApp;
use crate::theme::Tokens;

/// Longest spec, target or output text kept, in characters.
const MAX_FIELD: usize = 4096;
/// Most lines kept in the run log.
const MAX_LOG: usize = 100;
/// Most diff lines drawn for one file; a longer diff ends with a note.
const MAX_DIFF_LINES: usize = 4000;

/// One file of the last plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileRow {
    /// Path relative to the output folder.
    pub path: String,
    /// `create`, `unchanged`, `update`, `merge`, `conflict` or `delete`.
    pub action: String,
    /// Unified diff of the change.
    pub diff: String,
}

/// What the Generate tab shows. Transient: kept in the dock state, never saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerateState {
    /// Path of a generator spec file; when set it wins over target and output.
    pub spec: String,
    /// Target id for an inline generator.
    pub target: String,
    /// Output folder for an inline generator.
    pub output: String,
    /// Files of the last preview (refreshed after a run).
    pub files: Vec<FileRow>,
    /// The selected file, by index into `files`.
    pub selected: Option<usize>,
    /// Issues of the last plan as (severity, message).
    pub issues: Vec<(String, String)>,
    /// Renames the last preview listed, as `id: from -> to`.
    pub renames: Vec<String>,
    /// What each button did, oldest first.
    pub log: Vec<String>,
    /// The targets `generate.targets` lists, as (id, label); read once.
    pub targets: Vec<(String, String)>,
}

/// The targets, read from the engine the first time they are needed.
pub(crate) fn targets(app: &mut CraftApp) -> Vec<(String, String)> {
    if app.ui.dock.state.generate.targets.is_empty()
        && let Ok(r) = app.session.run("generate.targets", &json!({}))
    {
        app.ui.dock.state.generate.targets = r
            .get("targets")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|t| Some((t.get("id")?.as_str()?.to_string(), t.get("label")?.as_str().unwrap_or_default().to_string())))
                    .collect()
            })
            .unwrap_or_default();
    }
    app.ui.dock.state.generate.targets.clone()
}

/// The target in effect: the field, else the first target.
pub(crate) fn current_target(app: &mut CraftApp) -> String {
    let t = app.ui.dock.state.generate.target.clone();
    if !t.is_empty() {
        return t;
    }
    targets(app).first().map(|(id, _)| id.clone()).unwrap_or_default()
}

/// The command params the fields make: the spec, or the target and output.
fn params(app: &mut CraftApp) -> Value {
    let g = &app.ui.dock.state.generate;
    if !g.spec.trim().is_empty() {
        return json!({"spec": g.spec.trim()});
    }
    let output = g.output.trim().to_string();
    let target = current_target(app);
    json!({"target": target, "output": output})
}

fn log(app: &mut CraftApp, line: String) {
    let log = &mut app.ui.dock.state.generate.log;
    log.push(line);
    if log.len() > MAX_LOG {
        let extra = log.len() - MAX_LOG;
        log.drain(..extra);
    }
}

fn strings(r: &Value, k: &str) -> Vec<String> {
    r.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn issues_of(r: &Value) -> Vec<(String, String)> {
    r.get("issues")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|i| {
                    let f = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                    (f("severity"), f("message"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Keep a preview's files, issues and renames for the tab.
fn keep_preview(app: &mut CraftApp, r: &Value) {
    let files: Vec<FileRow> = r
        .get("files")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|f| {
                    let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                    FileRow { path: s("path"), action: s("action"), diff: s("diff") }
                })
                .collect()
        })
        .unwrap_or_default();
    let renames = r
        .get("renames")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|x| {
                    let s = |k: &str| x.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                    format!("{}: {} -> {}", s("id"), s("from"), s("to"))
                })
                .collect()
        })
        .unwrap_or_default();
    let g = &mut app.ui.dock.state.generate;
    let keep = g.selected.and_then(|i| g.files.get(i)).map(|f| f.path.clone());
    g.selected = keep
        .and_then(|p| files.iter().position(|f| f.path == p))
        .or_else(|| files.iter().position(|f| !f.diff.is_empty()))
        .or(if files.is_empty() { None } else { Some(0) });
    g.files = files;
    g.issues = issues_of(r);
    g.renames = renames;
}

/// Run one of the generate commands with `p` and record the result in the tab.
fn act(app: &mut CraftApp, id: &str, p: &Value) -> Result<Value, String> {
    let r = app.session.run(id, p).map_err(|e| e.to_string());
    match (id, &r) {
        (_, Err(e)) => log(app, format!("{}: {e}", verb(id))),
        ("generate.preview", Ok(v)) => {
            keep_preview(app, v);
            let n = app.ui.dock.state.generate.files.iter().filter(|f| f.action != "unchanged").count();
            let stopped = if v.get("stopped").and_then(Value::as_bool) == Some(true) { "; a conflict would stop the run" } else { "" };
            log(app, format!("Preview: {n} of {} files would change{stopped}", app.ui.dock.state.generate.files.len()));
        }
        ("generate.run", Ok(v)) => {
            let line = if v.get("stopped").and_then(Value::as_bool) == Some(true) {
                "Generate: stopped, hand edits conflict with the new output; nothing was written".to_string()
            } else {
                format!(
                    "Generate: wrote {}, merged {}, deleted {}, unchanged {}",
                    strings(v, "written").len(),
                    strings(v, "merged").len(),
                    strings(v, "deleted").len(),
                    strings(v, "unchanged").len()
                )
            };
            log(app, line);
            app.ui.dock.state.generate.issues = issues_of(v);
            // Show the folder as it is now.
            if let Ok(pv) = app.session.run("generate.preview", p) {
                keep_preview(app, &pv);
            }
        }
        ("generate.check", Ok(v)) => {
            let line = if v.get("ok").and_then(Value::as_bool) == Some(true) {
                "Check: the output is current".to_string()
            } else {
                let mut parts = Vec::new();
                for k in ["stale", "missing"] {
                    let l = strings(v, k);
                    if !l.is_empty() {
                        parts.push(format!("{k} {}", l.join(", ")));
                    }
                }
                format!("Check: out of date ({})", parts.join("; "))
            };
            log(app, line);
            app.ui.dock.state.generate.issues = issues_of(v);
        }
        _ => {}
    }
    r
}

fn verb(id: &str) -> &'static str {
    match id {
        "generate.run" => "Generate",
        "generate.check" => "Check",
        _ => "Preview",
    }
}

/// The person's `generate.preview`, `generate.run` or `generate.check` with no params (ribbon,
/// palette): run it with the tab's fields, keep the result and open the dock on Generate. Calls
/// from agents, and calls that bring their own params, go to the engine untouched.
pub(crate) fn on_run(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let empty = p.is_null() || p.as_object().is_some_and(|o| o.is_empty());
    if app.session.origin() != Origin::Ui || !empty {
        return None;
    }
    super::open_on(app, DockTab::Generate);
    let params = params(app);
    Some(act(app, id, &params))
}

fn field(p: &Value, k: &str) -> Result<Option<String>, String> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.chars().take(MAX_FIELD).collect())),
        Some(_) => Err(format!("`{k}` must be a string")),
    }
}

/// `ui.generate {spec?, target?, output?, run?: "preview"|"generate"|"check"}`: set the fields,
/// open the dock on Generate and optionally press a button. The reply is the tab's state, with
/// the command's result as `result` when one ran.
pub(crate) fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let (spec, target, output) = (field(p, "spec")?, field(p, "target")?, field(p, "output")?);
    let run = match p.get("run") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match s.as_str() {
            "preview" => Some("generate.preview"),
            "generate" | "run" => Some("generate.run"),
            "check" => Some("generate.check"),
            other => return Err(format!("unknown run {other:?} (preview, generate or check)")),
        },
        Some(_) => return Err("`run` must be a string".into()),
    };
    if let Some(t) = &target
        && !t.is_empty()
        && !targets(app).iter().any(|(id, _)| id == t)
    {
        return Err(format!("unknown target {t:?}"));
    }
    let g = &mut app.ui.dock.state.generate;
    if let Some(s) = spec {
        g.spec = s;
    }
    if let Some(t) = target {
        g.target = t;
        // Choosing a target means an inline generator.
        if p.get("spec").is_none() {
            g.spec.clear();
        }
    }
    if let Some(o) = output {
        g.output = o;
    }
    super::open_on(app, DockTab::Generate);
    let mut reply = inspect(app);
    if let Some(id) = run {
        let params = params(app);
        let r = act(app, id, &params);
        reply = inspect(app);
        if let Some(o) = reply.as_object_mut() {
            o.insert("result".into(), r.unwrap_or_else(|e| json!({"error": e})));
        }
    }
    Ok(reply)
}

/// The tab as `ui.inspect` reports it: `{spec, target, output, files: [{path, action}]}`.
pub fn inspect(app: &CraftApp) -> Value {
    let g = &app.ui.dock.state.generate;
    let files: Vec<Value> = g.files.iter().map(|f| json!({"path": f.path, "action": f.action})).collect();
    json!({"spec": g.spec, "target": g.target, "output": g.output, "files": files, "issues": g.issues.len(), "log": g.log.last()})
}

/// Rows the tab lists, for the dock's `rows`.
pub(crate) fn rows(app: &CraftApp) -> usize {
    app.ui.dock.state.generate.files.len()
}

/// The badge colour of an action.
fn badge_color(action: &str, t: &Tokens) -> Color32 {
    match action {
        "create" | "update" => t.green,
        "merge" => t.accent,
        "conflict" | "delete" => t.red,
        _ => t.text_dim,
    }
}

/// The tab's body.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let targets = targets(app);
    let current = current_target(app);
    let mut pressed: Option<&str> = None;
    ui.horizontal_wrapped(|ui| {
        ui.add_space(6.0);
        // The buttons come first so they stay in view when the dock is narrow; the fields wrap.
        if ui.button("Preview").clicked() {
            pressed = Some("generate.preview");
        }
        if ui.button("Generate").clicked() {
            pressed = Some("generate.run");
        }
        if ui.button("Check").clicked() {
            pressed = Some("generate.check");
        }
        ui.separator();
        ui.label("Spec");
        let g = &mut app.ui.dock.state.generate;
        ui.add(egui::TextEdit::singleline(&mut g.spec).hint_text(".vizcraft-gen.toml").desired_width(180.0));
        if ui.button("Browse...").clicked()
            && let Some(path) = app.services.pick_open.as_ref().and_then(|f| f("spec"))
        {
            app.ui.dock.state.generate.spec = path;
        }
        ui.label("Target");
        let shown = targets.iter().find(|(id, _)| *id == current).map(|(_, l)| l.clone()).unwrap_or_else(|| current.clone());
        egui::ComboBox::from_id_salt("generate-target").selected_text(shown).width(160.0).show_ui(ui, |ui| {
            for (id, label) in &targets {
                if ui.selectable_label(*id == current, label).clicked() {
                    app.ui.dock.state.generate.target = id.clone();
                    app.ui.dock.state.generate.spec.clear();
                }
            }
        });
        ui.label("Output");
        let g = &mut app.ui.dock.state.generate;
        ui.add(egui::TextEdit::singleline(&mut g.output).hint_text("folder, relative to the document").desired_width(200.0));
    });
    if let Some(id) = pressed {
        let p = params(app);
        let _ = act(app, id, &p);
    }
    ui.add_space(2.0);
    let g = app.ui.dock.state.generate.clone();
    if g.files.is_empty() && g.log.is_empty() {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Choose a spec file, or a target and an output folder, then Preview to see what would be written.").color(t.text_dim),
            );
        });
        return;
    }
    let mut pick = None;
    ui.horizontal_top(|ui| {
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.set_width(300.0);
            egui::ScrollArea::vertical().id_salt("generate-files").auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new("Files").small().color(t.text_dim));
                for (i, f) in g.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:<9}", f.action)).monospace().small().color(badge_color(&f.action, &t)));
                        if ui.selectable_label(g.selected == Some(i), &f.path).clicked() {
                            pick = Some(i);
                        }
                    });
                }
                if !g.issues.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Issues").small().color(t.text_dim));
                    for (sev, msg) in &g.issues {
                        let c = if sev == "error" { t.red } else { t.text };
                        ui.label(RichText::new(msg).color(c));
                    }
                }
                if !g.renames.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Renames").small().color(t.text_dim));
                    for r in &g.renames {
                        ui.label(r);
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Run log").small().color(t.text_dim));
                for line in g.log.iter().rev() {
                    ui.label(RichText::new(line).color(t.text_dim));
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            let file = g.selected.and_then(|i| g.files.get(i));
            egui::ScrollArea::both().id_salt("generate-diff").auto_shrink([false, false]).show(ui, |ui| match file {
                None => {
                    ui.label(RichText::new("Select a file to see its diff").color(t.text_dim));
                }
                Some(f) if f.diff.is_empty() => {
                    ui.label(RichText::new(format!("{}: no change", f.path)).color(t.text_dim));
                }
                Some(f) => diff_view(ui, &f.diff, &t),
            });
        });
    });
    if let Some(i) = pick {
        app.ui.dock.state.generate.selected = Some(i);
    }
}

/// A unified diff, one monospace label per line, added lines green and removed lines red.
fn diff_view(ui: &mut Ui, diff: &str, t: &Tokens) {
    ui.spacing_mut().item_spacing.y = 0.0;
    // Code keeps its lines: long ones scroll sideways instead of wrapping.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    let mut lines = diff.lines();
    for line in lines.by_ref().take(MAX_DIFF_LINES) {
        let color = if line.starts_with("+++") || line.starts_with("---") {
            t.text_dim
        } else if line.starts_with('+') {
            t.green
        } else if line.starts_with('-') {
            t.red
        } else if line.starts_with("@@") {
            t.accent_text
        } else {
            t.text
        };
        ui.label(RichText::new(line).monospace().color(color));
    }
    if lines.next().is_some() {
        ui.label(RichText::new(format!("... the diff goes on past {MAX_DIFF_LINES} lines")).color(t.text_dim));
    }
}
