//! The Problems tab: one row per issue, from the live rules and the last check.
//!
//! The rows are `Session.issues` (always current) plus the batch findings of the last
//! `validate.run`, merged by issue key. The last run is kept with the revision it was taken at and
//! taken again when the document changes, so a fixed or suppressed issue leaves the list at once.

use egui::{Color32, Pos2, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::Conformance;
use vizcraft_diagram::vizcraft_model::{Issue, Severity};

use super::DockTab;
use crate::CraftApp;
use crate::theme::Tokens;
use crate::widgets::{menu_button, menu_item};

/// The conformance profile in effect for the document (what `validate.setProfile` with no params
/// returns). Worked out once per document revision and kept on the app: the Live Checking toggle
/// and the dock ask every frame and the answer walks the model.
pub(crate) fn profile(app: &CraftApp) -> Conformance {
    let catalog = vizcraft_notation::catalog();
    let key = (app.session.rev(), std::ptr::from_ref(catalog) as usize);
    if let Some((rev, cat, p)) = app.profile_cache.borrow().as_ref()
        && (*rev, *cat) == key
    {
        return p.clone();
    }
    let p = vizcraft_notation::rules::profile(&app.session.doc, catalog);
    *app.profile_cache.borrow_mut() = Some((key.0, key.1, p.clone()));
    p
}

/// Whether checking is on for the document.
pub(crate) fn live_on(app: &CraftApp) -> bool {
    profile(app).enabled
}

/// The Rules menu: Off, then one row per ruleset of the catalog (built-in and loaded packs), the
/// ones the document is held to checked. Drawn by hand because the rows are the catalog's, which
/// loading a pack changes. Shared by the Process tab and the dock toolbar.
pub(crate) fn rules_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "rules", "Rules", "Rules", false, |ui, app| {
        let p = profile(app);
        menu_item(ui, app, "Off", "validate.setProfile", json!({"enabled": false}), Some(!p.enabled));
        ui.separator();
        for (_, r) in vizcraft_notation::catalog().rulesets() {
            let on = p.enabled && p.rulesets.contains(&r.id);
            menu_item(ui, app, &r.label, "validate.setProfile", json!({"rulesets": [r.id]}), Some(on));
        }
    });
}

/// Run `validate.run` in the session and keep an all-document result for the rows. The reply is
/// the command's own.
fn run_check(app: &mut CraftApp, params: &Value) -> Result<Value, String> {
    let r = app.session.run("validate.run", params).map_err(|e| e.to_string())?;
    let issues: Vec<Issue> = serde_json::from_value(r.get("issues").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
    app.ui.dock.state.checked = Some((app.session.rev(), issues));
    Ok(r)
}

/// `validate.run` for the whole document, from any origin (ribbon, palette, dock, control
/// channel, MCP, scripts): run it, keep the result for the rows, and, when the person asked for
/// it (a UI-origin run), open the dock on Problems. A scope other than the whole document goes to
/// the engine untouched.
pub(crate) fn on_run(app: &mut CraftApp, params: &Value) -> Option<Result<Value, String>> {
    let whole = params.get("scope").and_then(Value::as_str).is_none_or(|s| s == "all") && params.get("page").is_none() && params.get("ids").is_none();
    if !whole {
        return None;
    }
    let r = run_check(app, &json!({}));
    if app.session.origin() == vizcraft_engine::Origin::Ui {
        super::open_on(app, DockTab::Problems);
    }
    Some(r)
}

/// Take the last check again when the document changed since, so rows never show a stale finding.
pub(crate) fn refresh(app: &mut CraftApp) {
    let stale = app.ui.dock.state.checked.as_ref().is_some_and(|(rev, _)| *rev != app.session.rev());
    if stale && run_check(app, &json!({})).is_err() {
        app.ui.dock.state.checked = None;
    }
}

/// Every issue to list, errors first: the live issues, then the batch findings of the last check
/// that are not already live.
pub fn rows(app: &CraftApp) -> Vec<Issue> {
    let mut out: Vec<Issue> = app.session.issues.clone();
    if let Some((rev, issues)) = &app.ui.dock.state.checked
        && *rev == app.session.rev()
    {
        let have: std::collections::HashSet<String> = out.iter().map(Issue::key).collect();
        out.extend(issues.iter().filter(|i| !have.contains(&i.key())).cloned());
    }
    vizcraft_notation::rules::sort(&mut out);
    out
}

/// The rows the filters let through.
fn visible(app: &CraftApp) -> Vec<Issue> {
    let s = &app.ui.dock.state;
    let needle = s.filter.trim().to_lowercase();
    rows(app)
        .into_iter()
        .filter(|i| match i.severity {
            Severity::Error => !s.hide_errors,
            Severity::Warning => !s.hide_warnings,
            Severity::Info => !s.hide_infos,
        })
        .filter(|i| needle.is_empty() || i.message.to_lowercase().contains(&needle) || i.rule.to_lowercase().contains(&needle))
        .collect()
}

/// The page id an issue is on: the one it names, else the first page drawing one of its items.
fn page_of(app: &CraftApp, issue: &Issue) -> Option<String> {
    if let Some(p) = &issue.page
        && app.session.doc.pages.iter().any(|pg| &pg.id == p)
    {
        return Some(p.clone());
    }
    app.session.doc.pages.iter().find(|pg| issue.items.iter().any(|it| pg.contains_id(it))).map(|pg| pg.id.clone())
}

fn page_name(app: &CraftApp, issue: &Issue) -> String {
    page_of(app, issue).and_then(|id| app.session.doc.pages.iter().find(|p| p.id == id).map(|p| p.name.clone())).unwrap_or_default()
}

/// Go to the issue's page and select its items there.
fn select_issue(app: &mut CraftApp, issue: &Issue) {
    let Some(page) = page_of(app, issue) else { return };
    let items: Vec<String> = app
        .session
        .doc
        .pages
        .iter()
        .find(|p| p.id == page)
        .map(|p| issue.items.iter().filter(|i| p.contains_id(i)).cloned().collect())
        .unwrap_or_default();
    if items.is_empty() {
        return;
    }
    let _ = app.run("page.setActive", json!({"page": page}));
    let _ = app.run("edit.select", json!({"ids": items}));
}

/// A little severity mark: filled circle for an error, triangle for a warning, ring for an info.
fn severity_icon(ui: &mut Ui, sev: Severity, t: &Tokens) {
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, sev.name()));
    let c = r.center();
    let p = ui.painter();
    match sev {
        Severity::Error => {
            p.circle_filled(c, 6.0, t.red);
        }
        Severity::Warning => {
            let pts = vec![Pos2::new(c.x, c.y - 6.5), Pos2::new(c.x + 6.5, c.y + 5.5), Pos2::new(c.x - 6.5, c.y + 5.5)];
            p.add(egui::Shape::convex_polygon(pts, Color32::TRANSPARENT, Stroke::new(1.5, t.red)));
        }
        Severity::Info => {
            p.circle_stroke(c, 5.5, Stroke::new(1.5, t.accent));
        }
    }
    resp.on_hover_text(sev.name());
}

/// Width the message column wraps at, in points.
const MESSAGE_W: f32 = 360.0;

/// What a row asked for, applied after the table is drawn (drawing borrows the rows).
enum Act {
    Select(Issue),
    Suppress(String),
    Fix(String, String, Value),
}

/// The Problems tab.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    toolbar(app, ui);
    if !live_on(app) {
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Checking is off").strong());
            ui.label(egui::RichText::new("Turn it on to see rule findings as you draw.").color(t.text_dim));
            if ui.button("Turn On").clicked() {
                let _ = app.run("validate.setProfile", json!({"enabled": true}));
            }
        });
        return;
    }
    suppress_strip(app, ui);
    let rows = visible(app);
    if rows.is_empty() {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("No problems").color(t.text_dim));
        return;
    }
    let mut act = None;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("problems_grid").num_columns(5).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for h in ["", "Message", "Rule", "Page", "Items"] {
                ui.label(egui::RichText::new(h).small().color(t.text_dim));
            }
            ui.end_row();
            for (n, issue) in rows.iter().enumerate() {
                severity_icon(ui, issue.severity, &t);
                // The message wraps inside a fixed width, with the row's buttons under it, so the
                // table fits the dock between the side panes.
                ui.vertical(|ui| {
                    ui.set_max_width(MESSAGE_W);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    if ui.selectable_label(false, &issue.message).clicked() {
                        act = Some(Act::Select(issue.clone()));
                    }
                    ui.horizontal_wrapped(|ui| {
                        explain_button(app, ui, issue, n);
                        if ui.button("Suppress").clicked() {
                            act = Some(Act::Suppress(issue.key()));
                        }
                        for fix in &issue.fixes {
                            if ui.button(&fix.label).clicked() {
                                act = Some(Act::Fix(fix.label.clone(), fix.command.clone(), fix.params.clone()));
                            }
                        }
                    });
                });
                ui.scope(|ui| {
                    ui.set_max_width(150.0);
                    ui.add(egui::Label::new(egui::RichText::new(&issue.rule).monospace().small()).truncate());
                });
                ui.label(page_name(app, issue));
                ui.scope(|ui| {
                    ui.set_max_width(100.0);
                    ui.add(egui::Label::new(issue.items.join(", ")).truncate());
                });
                ui.end_row();
            }
        });
    });
    match act {
        Some(Act::Select(i)) => select_issue(app, &i),
        Some(Act::Suppress(key)) => app.ui.dock.state.suppressing = Some((key, String::new())),
        Some(Act::Fix(label, command, params)) => {
            let _ = app.run("edit.batch", json!({"label": label, "commands": [{"command": command, "params": params}]}));
        }
        None => {}
    }
}

/// Explain: a popover with the rule, its reference and the issue's message.
fn explain_button(app: &mut CraftApp, ui: &mut Ui, issue: &Issue, n: usize) {
    let resp = ui.push_id(("explain", n), |ui| ui.button("Explain")).inner;
    egui::Popup::from_toggle_button_response(&resp).show(|ui| {
        ui.set_min_width(300.0);
        let r = app.run("validate.explain", json!({"rule": issue.rule}));
        let get = |k: &str| r.as_ref().ok().and_then(|v| v.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
        ui.label(egui::RichText::new(&issue.rule).monospace().strong());
        ui.label(&issue.message);
        let (desc, reference) = (get("description"), get("reference"));
        if !desc.is_empty() {
            ui.label(desc);
        }
        if !reference.is_empty() {
            ui.label(egui::RichText::new(format!("Reference: {reference}")).small());
        }
        if let Err(e) = &r {
            ui.label(e.as_str());
        }
    });
}

/// The one-line reason field that appears after Suppress was pressed on a row.
fn suppress_strip(app: &mut CraftApp, ui: &mut Ui) {
    let Some((key, mut reason)) = app.ui.dock.state.suppressing.clone() else { return };
    let mut done = false;
    ui.horizontal(|ui| {
        ui.label("Reason");
        ui.add(egui::TextEdit::singleline(&mut reason).hint_text("Why this is fine").desired_width(280.0));
        if ui.button("Confirm").clicked() {
            let _ = app.run("validate.suppress", json!({"key": key, "reason": reason.trim()}));
            done = true;
        }
        if ui.button("Cancel").clicked() {
            done = true;
        }
    });
    app.ui.dock.state.suppressing = if done { None } else { Some((key, reason)) };
}

/// Check, Rules, Live Checking, Previous and Next, the severity and text filters, the count.
fn toolbar(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if ui.button("Check").clicked() {
            let _ = app.run("validate.run", json!({}));
        }
        rules_menu(app, ui);
        let on = live_on(app);
        crate::widgets::toggle(ui, app, "liveChecking", Some("Live Checking"), "Live Checking", "validate.setProfile", json!({"enabled": !on}), on);
        crate::widgets::small(ui, app, "prevIssue", Some("Previous"), "Previous issue", "validate.previous", json!({}));
        crate::widgets::small(ui, app, "nextIssue", Some("Next"), "Next issue", "validate.next", json!({}));
        ui.separator();
        let n = rows(app).len();
        ui.label(egui::RichText::new(format!("{n} {}", if n == 1 { "problem" } else { "problems" })).color(t.text_dim));
        let s = &mut app.ui.dock.state;
        for (label, hidden) in [("Errors", &mut s.hide_errors), ("Warnings", &mut s.hide_warnings), ("Info", &mut s.hide_infos)] {
            let mut shown = !*hidden;
            if ui.toggle_value(&mut shown, label).changed() {
                *hidden = !shown;
            }
        }
        ui.add(egui::TextEdit::singleline(&mut s.filter).hint_text("Filter").desired_width(90.0));
    });
}
