//! Review > Reporting: the Shape Reports menu. Pick a report, the scope and the format, then Run:
//! a table is drawn on the page; a CSV or HTML file is saved where the file picker says (or
//! downloaded on the web). The choices are kept in egui memory, so the menu reopens as it was.

use egui::{Id, Ui};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets::{menu_button, primary_button};

/// The built-in reports: `(id, label)`.
const REPORTS: [(&str, &str); 3] = [("inventory", "Shape Inventory"), ("data", "Shape Data"), ("connections", "Connections")];

/// What the menu has chosen.
#[derive(Clone, PartialEq)]
struct Choice {
    report: &'static str,
    document: bool,
    format: &'static str,
}

impl Default for Choice {
    fn default() -> Self {
        Choice { report: "inventory", document: false, format: "table" }
    }
}

fn choice_id() -> Id {
    Id::new("review_report_choice")
}

/// The Review tab's Reporting group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Reporting", vec![Custom(reports_menu)])
}

fn reports_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "report", "Shape\nReports", "Shape Reports", true, |ui, app| {
        let mut c = ui.data(|d| d.get_temp::<Choice>(choice_id())).unwrap_or_default();
        ui.label(egui::RichText::new("Report").small().weak());
        for (id, label) in REPORTS {
            ui.radio_value(&mut c.report, id, label);
        }
        ui.separator();
        ui.label(egui::RichText::new("Scope").small().weak());
        ui.radio_value(&mut c.document, false, "This page");
        ui.radio_value(&mut c.document, true, "Whole document");
        ui.separator();
        ui.label(egui::RichText::new("Format").small().weak());
        ui.radio_value(&mut c.format, "table", "Table on the page");
        ui.radio_value(&mut c.format, "csv", "CSV file");
        ui.radio_value(&mut c.format, "html", "HTML file");
        ui.add_space(4.0);
        let run = primary_button(ui, "Run Report").clicked();
        ui.data_mut(|d| d.insert_temp(choice_id(), c.clone()));
        if run {
            run_report(app, c.report, c.document, c.format);
            ui.close();
        }
    });
}

/// Run a report as the menu chose: through `report.run`, with a path from the file picker for a
/// file format and a download where the web has no picker.
fn run_report(app: &mut CraftApp, report: &str, document: bool, format: &str) {
    let mut params = json!({"report": report, "scope": if document { "document" } else { "page" }, "format": format});
    if format != "table" {
        let label = REPORTS.iter().find(|(id, _)| *id == report).map_or(report, |(_, l)| l);
        let name = format!("{} - {label}.{format}", app.title_stem());
        if let Some(pick) = app.services.pick_save.as_ref() {
            let Some(path) = pick(&name) else { return };
            params["path"] = json!(path);
        } else if app.services.download.is_none() {
            app.status("This build has no file picker; run report.run with a path instead");
            return;
        }
    }
    match app.run("report.run", params) {
        Ok(r) => {
            if let (Some(content), Some(download)) = (r.get("content").and_then(Value::as_str), app.services.download.as_ref()) {
                let label = REPORTS.iter().find(|(id, _)| *id == report).map_or(report, |(_, l)| l);
                download(&format!("{} - {label}.{format}", app.title_stem()), content.as_bytes());
            }
            let rows = r.get("rows").and_then(Value::as_array).map_or(0, Vec::len);
            app.status(format!("Report finished: {rows} row(s)"));
        }
        Err(e) => app.status(e),
    }
}
