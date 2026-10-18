//! The Print Preview dialog: which pages, which paper, fit to one sheet or tile, with a picture
//! of each page's sheets. `ui.print` (File > Print, Mod+P) opens it; Print runs `file.print` with
//! the same settings, so an agent gets the same PDF without the dialog. The tiles come from
//! `vizcraft_engine::cmd::print::plans`, the layout `file.print` itself uses.

use egui::{Align2, Sense, Stroke, Ui, pos2, vec2};
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_engine::cmd::print::Plan;

use crate::CraftApp;
use crate::theme::{Tokens, regular};

/// Paper choices: (label, `paper` param).
pub const PAPERS: &[(&str, &str)] =
    &[("Page size", "page"), ("Letter", "letter"), ("Legal", "legal"), ("Tabloid", "tabloid"), ("A4", "a4"), ("A3", "a3")];

/// The dialog's settings.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Print every foreground page rather than the active one.
    pub all_pages: bool,
    /// The `paper` param.
    pub paper: String,
    /// Shrink or grow each page to one sheet.
    pub fit: bool,
}

/// The dialog seeded with every page on paper the size of the page, tiled.
pub fn open(app: &CraftApp) -> State {
    State { all_pages: app.session.doc.pages.iter().filter(|p| !p.is_background).count() > 1, paper: "page".into(), fit: false }
}

/// The `file.print` params for these settings. The Print button is the one caller that sends to
/// the printer; `send` is opt-in so agents and scripts only write the PDF.
pub fn params(s: &State) -> Value {
    json!({"pages": if s.all_pages { "all" } else { "active" }, "paper": s.paper, "fit": s.fit, "send": true})
}

/// Height of the sheet picture.
const PREVIEW_H: f32 = 180.0;

/// The settings and the sheet picture above the Print and Cancel buttons.
pub fn body(app: &CraftApp, ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    egui::Grid::new("print_preview").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        ui.label(egui::RichText::new("Pages").color(t.text_dim));
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.all_pages, false, "Current page");
            ui.radio_value(&mut s.all_pages, true, "All pages");
        });
        ui.end_row();
        ui.label(egui::RichText::new("Paper").color(t.text_dim));
        let current = PAPERS.iter().find(|(_, v)| *v == s.paper).map_or("Page size", |(l, _)| *l);
        egui::ComboBox::from_id_salt("print_paper").selected_text(current).width(140.0).show_ui(ui, |ui| {
            for (label, value) in PAPERS {
                ui.selectable_value(&mut s.paper, (*value).to_string(), *label);
            }
        });
        ui.end_row();
        ui.label(egui::RichText::new("Size").color(t.text_dim));
        ui.checkbox(&mut s.fit, "Fit each page on one sheet");
        ui.end_row();
    });
    ui.add_space(6.0);
    match vizcraft_engine::cmd::print::plans(&app.session, &params(s)) {
        Ok(plans) => {
            let sheets: usize = plans.iter().map(|(_, _, p)| p.sheets()).sum();
            ui.label(egui::RichText::new(format!("{sheets} sheet(s) of paper")).color(t.text));
            egui::ScrollArea::horizontal().id_salt("print_tiles").show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (_, name, plan) in &plans {
                        sheet_picture(ui, name, plan);
                    }
                });
            });
        }
        Err(e) => {
            ui.label(egui::RichText::new(e.to_string()).color(t.red));
        }
    }
}

/// One page's grid of sheets with the drawing's extent inside, and the page name below.
fn sheet_picture(ui: &mut Ui, name: &str, p: &Plan) {
    let t = Tokens::get(ui.ctx());
    let grid_w = f64::from(p.cols) * p.paper_w;
    let grid_h = f64::from(p.rows) * p.paper_h;
    let k = (f64::from(PREVIEW_H) / grid_h).min(260.0 / grid_w.max(1.0));
    let size = vec2((grid_w * k) as f32, (grid_h * k) as f32);
    ui.vertical(|ui| {
        let (r, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(r.expand(1.0));
        let (dw, dh) = ((p.drawing_w * p.scale * k) as f32, (p.drawing_h * p.scale * k) as f32);
        let drawing = egui::Rect::from_center_size(r.center(), vec2(dw, dh));
        for row in 0..p.rows {
            for col in 0..p.cols {
                let x = r.min.x + (f64::from(col) * p.paper_w * k) as f32;
                let y = r.min.y + (f64::from(row) * p.paper_h * k) as f32;
                let sheet = egui::Rect::from_min_size(pos2(x, y), vec2((p.paper_w * k) as f32, (p.paper_h * k) as f32));
                painter.rect_filled(sheet, 0.0, t.page);
                painter.rect_stroke(sheet, 0.0, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
            }
        }
        // The drawing as a tinted box over the sheets, so where it is cut between sheets shows.
        painter.rect_filled(drawing, 0.0, t.accent.gamma_multiply(0.15));
        painter.rect_stroke(drawing, 0.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Middle);
        let label = if p.sheets() > 1 { format!("{name}: {} by {}", p.cols, p.rows) } else { name.to_string() };
        ui.painter().text(pos2(r.min.x, r.max.y + 4.0), Align2::LEFT_TOP, label, regular(11.0), t.text_dim);
        ui.add_space(18.0);
    });
    ui.add_space(12.0);
}

/// After the buttons: on Print run `file.print`. Returns whether to close.
/// On the web, where there is no printer to reach, the PDF is downloaded instead.
pub fn finish(app: &mut CraftApp, s: &State, print: bool, cancel: bool) -> bool {
    if print {
        if app.services.download.is_some() {
            let name = format!("{}.pdf", app.title_stem());
            match vizcraft_engine::cmd::print::print_pdf(&app.session, &params(s)) {
                Ok(printed) => {
                    if let Some(d) = &app.services.download {
                        d(&name, &printed.pdf);
                    }
                }
                Err(e) => app.status(e.to_string()),
            }
        } else {
            let _ = app.run("file.print", params(s));
        }
    }
    print || cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn ui_print_opens_the_preview_and_print_runs_the_command() {
        let mut app = app();
        app.run("ui.print", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("printPreview"));
        let s = State { all_pages: false, paper: "letter".into(), fit: true };
        let plans = vizcraft_engine::cmd::print::plans(&app.session, &params(&s)).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].2.sheets(), 1);
        assert!(!finish(&mut app, &s, false, false));
        // Print hands the PDF to the front end's print service; with none it is only written.
        let printed = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let seen = printed.clone();
        app.services.print = Some(Box::new(move |path: &str| {
            seen.borrow_mut().push(path.to_string());
            Ok("sent".into())
        }));
        assert!(finish(&mut app, &s, true, false));
        let got = printed.borrow().clone();
        assert_eq!(got.len(), 1);
        assert!(std::fs::read(&got[0]).unwrap().starts_with(b"%PDF-"));
        let _ = std::fs::remove_file(&got[0]);
    }
}
