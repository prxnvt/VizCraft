//! The Page Setup dialog: board (infinite or fixed), paper size (a preset or custom), orientation, units, drawing scale and
//! auto size for the active page. The size, orientation and auto size fields wait, greyed, while
//! the board is infinite. `page.setup` with no params asks the UI to open it; OK runs
//! `page.setup` again with the settings, so agents reach the same result without the dialog.
//!
//! Sizes are shown in the page's units; the page itself stores world units (100 per inch).

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::{Value, json};
use vizcraft_diagram::{Board, Orientation, Units};
use vizcraft_engine::cmd::page::{UNITS, parse_units, units_name};

use crate::CraftApp;
use crate::theme::Tokens;

/// Paper sizes offered by the dialog and the ribbon's Size menu: `(name, width, height)` in world
/// units, portrait.
pub const PRESETS: &[(&str, f64, f64)] = &[
    ("Letter", 850.0, 1100.0),
    ("Legal", 850.0, 1400.0),
    ("Tabloid", 1100.0, 1700.0),
    ("A3", 1169.3, 1653.5),
    ("A4", 826.8, 1169.3),
    ("A5", 583.5, 826.8),
];

/// World units in one unit of `u`.
pub fn units_scale(u: Units) -> f64 {
    match u {
        Units::Inches => 100.0,
        Units::Feet => 1200.0,
        Units::Millimeters => 100.0 / 25.4,
        Units::Centimeters => 1000.0 / 25.4,
        Units::Meters => 100_000.0 / 25.4,
        Units::Points => 100.0 / 72.0,
        Units::Pixels => 1.0,
    }
}

/// The `page.setup` params that give a page preset `(w, h)` (portrait) in `orientation`.
pub fn preset_params(w: f64, h: f64, orientation: Orientation) -> Value {
    let (w, h) = if orientation == Orientation::Landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
    json!({"width": w, "height": h})
}

/// The name of the preset a page of `w` by `h` world units matches in either orientation.
pub fn preset_of(w: f64, h: f64) -> Option<&'static str> {
    let close = |a: f64, b: f64| (a - b).abs() < 1.0;
    PRESETS.iter().find(|(_, pw, ph)| (close(w, *pw) && close(h, *ph)) || (close(w, *ph) && close(h, *pw))).map(|(n, ..)| *n)
}

/// The dialog's editable state, seeded from the active page.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageSetup {
    /// The page being set up.
    pub page: String,
    /// Infinite canvas or fixed sheet.
    pub board: Board,
    /// Width in world units.
    pub width: f64,
    /// Height in world units.
    pub height: f64,
    /// Portrait (else landscape).
    pub portrait: bool,
    /// Units for the size fields and the page.
    pub units: String,
    /// Page side of the drawing scale.
    pub scale_page: f64,
    /// Drawing side of the drawing scale.
    pub scale_drawing: f64,
    /// Units of the drawing side.
    pub scale_units: String,
    /// Grow the page to fit the drawing.
    pub auto_size: bool,
    /// The drawing sheet as `<standard>.<size>` (`iso.a3`, `asme.b`); empty for none. OK runs
    /// `sheet.set` with it in the chosen orientation, after `page.setup`.
    pub sheet: String,
    /// The page had a sheet when the dialog opened (choosing none then removes it).
    pub had_sheet: bool,
}

impl PageSetup {
    /// The setup of the active page.
    pub fn new(app: &CraftApp) -> PageSetup {
        let Some(p) = app.session.doc.page() else {
            return PageSetup {
                page: String::new(),
                board: Board::Infinite,
                width: 1100.0,
                height: 850.0,
                portrait: false,
                units: "in".into(),
                scale_page: 1.0,
                scale_drawing: 1.0,
                scale_units: "in".into(),
                auto_size: false,
                sheet: String::new(),
                had_sheet: false,
            };
        };
        PageSetup {
            page: p.id.clone(),
            board: p.board,
            width: p.width,
            height: p.height,
            portrait: p.height > p.width || (p.height == p.width && p.orientation == Orientation::Portrait),
            units: units_name(p.units),
            scale_page: p.drawing_scale.page,
            scale_drawing: p.drawing_scale.drawing,
            scale_units: units_name(p.drawing_scale.units),
            auto_size: p.auto_size,
            sheet: p.sheet.map(|s| format!("{}.{}", s.standard.short(), s.size.name())).unwrap_or_default(),
            had_sheet: p.sheet.is_some(),
        }
    }

    /// The `sheet.set` params OK sends after `page.setup`, if any: the chosen sheet in the
    /// chosen orientation, or `off` when the page loses its sheet.
    pub fn sheet_params(&self) -> Option<Value> {
        let orientation = if self.portrait { "portrait" } else { "landscape" };
        if !self.sheet.is_empty() {
            Some(json!({"page": self.page, "sheet": format!("{}.{orientation}", self.sheet)}))
        } else if self.had_sheet {
            Some(json!({"page": self.page, "off": true}))
        } else {
            None
        }
    }

    /// The `page.setup` params OK sends.
    pub fn params(&self) -> Value {
        json!({
            "page": self.page,
            "board": self.board,
            "width": self.width,
            "height": self.height,
            "orientation": if self.portrait { "portrait" } else { "landscape" },
            "units": self.units,
            "drawingScale": {"page": self.scale_page, "drawing": self.scale_drawing, "units": self.scale_units},
            "autoSize": self.auto_size,
        })
    }

    fn set_portrait(&mut self, portrait: bool) {
        if portrait != self.portrait && self.width != self.height {
            std::mem::swap(&mut self.width, &mut self.height);
        }
        self.portrait = portrait;
    }
}

fn units_combo(ui: &mut Ui, id: &str, value: &mut String) {
    egui::ComboBox::from_id_salt(id).selected_text(value.clone()).width(70.0).show_ui(ui, |ui| {
        for u in UNITS {
            let name = units_name(u);
            ui.selectable_value(value, name.clone(), name);
        }
    });
}

/// The dialog's fields.
pub fn fields(ui: &mut Ui, d: &mut PageSetup) {
    let t = Tokens::get(ui.ctx());
    let dim = |s: &str| egui::RichText::new(s).color(t.text_dim);
    let units = parse_units(&d.units).unwrap_or_default();
    let k = units_scale(units);
    // The sheet fields mean nothing on an infinite board; they keep their values for a switch back.
    let sheet = d.board == Board::Fixed;
    egui::Grid::new("page_setup").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
        ui.label(dim("Board"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut d.board, Board::Infinite, "Infinite");
            ui.radio_value(&mut d.board, Board::Fixed, "Fixed");
        });
        ui.end_row();
        ui.label(dim("Size"));
        let current = preset_of(d.width, d.height).unwrap_or("Custom");
        ui.add_enabled_ui(sheet, |ui| {
            egui::ComboBox::from_id_salt("page_setup_size").selected_text(current).width(160.0).show_ui(ui, |ui| {
                for (name, w, h) in PRESETS {
                    if ui.selectable_label(current == *name, *name).clicked() {
                        let (w, h) = if d.portrait { (*w, *h) } else { (*h, *w) };
                        d.width = w;
                        d.height = h;
                    }
                }
                let _ = ui.selectable_label(current == "Custom", "Custom");
            });
        });
        ui.end_row();
        ui.label(dim("Width"));
        let mut w = d.width / k;
        let drag = egui::DragValue::new(&mut w).speed(0.1).range(0.01..=1.0e6).max_decimals(2).suffix(format!(" {}", d.units));
        if ui.add_enabled(sheet, drag).changed() {
            d.width = (w * k).clamp(1.0, vizcraft_diagram::MAX_EXTENT);
            d.portrait = d.height > d.width;
        }
        ui.end_row();
        ui.label(dim("Height"));
        let mut h = d.height / k;
        let drag = egui::DragValue::new(&mut h).speed(0.1).range(0.01..=1.0e6).max_decimals(2).suffix(format!(" {}", d.units));
        if ui.add_enabled(sheet, drag).changed() {
            d.height = (h * k).clamp(1.0, vizcraft_diagram::MAX_EXTENT);
            d.portrait = d.height > d.width;
        }
        ui.end_row();
        ui.label(dim("Orientation"));
        ui.add_enabled_ui(sheet, |ui| {
            ui.horizontal(|ui| {
                let mut portrait = d.portrait;
                ui.radio_value(&mut portrait, true, "Portrait");
                ui.radio_value(&mut portrait, false, "Landscape");
                d.set_portrait(portrait);
            });
        });
        ui.end_row();
        ui.label(dim("Units"));
        units_combo(ui, "page_setup_units", &mut d.units);
        ui.end_row();
        sheet_rows(ui, &dim, d);
        ui.label(dim("Drawing scale"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut d.scale_page).speed(0.1).range(0.000_001..=1.0e6).max_decimals(3));
            ui.label(egui::RichText::new(format!("{} =", d.units)).color(t.text));
            ui.add(egui::DragValue::new(&mut d.scale_drawing).speed(0.1).range(0.000_001..=1.0e6).max_decimals(3));
            units_combo(ui, "page_setup_scale_units", &mut d.scale_units);
        });
        ui.end_row();
        ui.label(dim("Auto size"));
        ui.add_enabled(sheet, egui::Checkbox::new(&mut d.auto_size, "Grow the page to fit the drawing"));
        ui.end_row();
    });
}

/// The Sheet standard and Sheet size rows: every standard offered (ISO 5457 first, ASME
/// Y14.1, or none), and the sizes of the chosen one.
fn sheet_rows(ui: &mut Ui, dim: &dyn Fn(&str) -> egui::RichText, d: &mut PageSetup) {
    use vizcraft_diagram::sheet::{PAPER_SIZES, SheetStandard};
    let standard = d.sheet.split_once('.').and_then(|(s, _)| SheetStandard::parse(s));
    ui.label(dim("Sheet standard"));
    let shown = standard.map_or("None", |s| s.label());
    egui::ComboBox::from_id_salt("page_setup_sheet_standard").selected_text(shown).width(160.0).show_ui(ui, |ui| {
        if ui.selectable_label(standard.is_none(), "None").clicked() {
            d.sheet.clear();
        }
        for st in [SheetStandard::Iso5457, SheetStandard::AsmeY14_1] {
            if ui.selectable_label(standard == Some(st), st.label()).clicked() && standard != Some(st) {
                let first = PAPER_SIZES.iter().filter(|p| p.standard == st).map(|p| p.size.name()).collect::<Vec<_>>();
                // ISO A3 and ANSI B: the usual engineering sheet of each standard.
                let size = if st == SheetStandard::Iso5457 { "a3" } else { "b" };
                let size = if first.contains(&size) { size } else { first.first().copied().unwrap_or(size) };
                d.sheet = format!("{}.{size}", st.short());
                d.board = Board::Fixed;
            }
        }
    });
    ui.end_row();
    ui.label(dim("Sheet size"));
    ui.add_enabled_ui(standard.is_some(), |ui| {
        let current = d.sheet.split_once('.').map(|(_, z)| z.to_string()).unwrap_or_default();
        let name = PAPER_SIZES.iter().find(|p| Some(p.standard) == standard && p.size.name() == current).map_or("-", |p| p.name);
        egui::ComboBox::from_id_salt("page_setup_sheet_size").selected_text(name).width(160.0).show_ui(ui, |ui| {
            for p in PAPER_SIZES.iter().filter(|p| Some(p.standard) == standard) {
                if ui.selectable_label(p.size.name() == current, p.name).clicked() {
                    d.sheet = format!("{}.{}", p.standard.short(), p.size.name());
                }
            }
        });
    });
    ui.end_row();
}

/// OK: run `page.setup` with the dialog's settings, then `sheet.set` for the drawing sheet.
pub fn apply(app: &mut CraftApp, d: &PageSetup) {
    if let Err(e) = app.run("page.setup", d.params()) {
        app.status(e);
        return;
    }
    if let Some(p) = d.sheet_params()
        && let Err(e) = app.run("sheet.set", p)
    {
        app.status(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_follow_the_orientation_and_are_recognised_both_ways() {
        assert_eq!(preset_params(850.0, 1100.0, Orientation::Landscape), json!({"width": 1100.0, "height": 850.0}));
        assert_eq!(preset_params(850.0, 1100.0, Orientation::Portrait), json!({"width": 850.0, "height": 1100.0}));
        assert_eq!(preset_of(1100.0, 850.0), Some("Letter"));
        assert_eq!(preset_of(826.8, 1169.3), Some("A4"));
        assert_eq!(preset_of(123.0, 456.0), None);
        assert!((units_scale(Units::Millimeters) * 25.4 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn ok_runs_page_setup_with_the_settings() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        let mut d = PageSetup::new(&app);
        assert_eq!(d.board, Board::Infinite, "a new document is on an infinite board");
        assert!(!d.portrait);
        d.board = Board::Fixed;
        d.set_portrait(true);
        d.units = "mm".into();
        d.auto_size = true;
        apply(&mut app, &d);
        let p = app.session.doc.page().unwrap();
        assert_eq!((p.width, p.height, p.orientation, p.units, p.auto_size), (850.0, 1100.0, Orientation::Portrait, Units::Millimeters, true));
        assert_eq!(p.board, Board::Fixed);
        assert!(app.dialog.is_none(), "applying never reopens the dialog");
    }

    #[test]
    fn a_sheet_chosen_in_the_dialog_is_set_and_none_removes_it() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        let mut d = PageSetup::new(&app);
        assert!(d.sheet_params().is_none(), "no sheet, none chosen: nothing to set");
        d.board = Board::Fixed;
        d.sheet = "asme.b".into();
        apply(&mut app, &d);
        let sh = app.session.doc.page().unwrap().sheet.unwrap();
        assert_eq!(sh.preset_id(), "asme.b.landscape");
        let mut d = PageSetup::new(&app);
        assert_eq!((d.sheet.as_str(), d.had_sheet), ("asme.b", true));
        d.sheet.clear();
        apply(&mut app, &d);
        assert!(app.session.doc.page().unwrap().sheet.is_none());
    }
}
