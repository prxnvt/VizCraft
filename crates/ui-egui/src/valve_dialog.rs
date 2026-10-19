//! The Valve dialog: an ISO 1219 directional valve built from its parameters, with a live
//! preview. The form is read from `glyph.builders {builder: "valve"}` (positions, ports, the
//! path table, the operators, the return, the rest position, port labels and medium) and the
//! preview from `glyph.build`, which changes no document. OK updates the selected valve through
//! `glyph.setParams`, or places a new one through `glyph.insert {builder: "valve", params}`.
//! `ui.valve` opens it.

use std::collections::BTreeMap;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, vec2};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::CraftApp;
use crate::theme::Tokens;

/// The builder this dialog edits.
pub const BUILDER: &str = "valve";

/// Most positions a valve has (the builder's `positions` maximum).
const MAX_POSITIONS: usize = 3;

/// The dialog's state.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct State {
    /// The form has been read from the builder (the first frame does it).
    #[serde(skip)]
    pub loaded: bool,
    /// The selected valve being edited; empty places a new one.
    pub target: String,
    /// The parameter specs from `glyph.builders`.
    #[serde(skip)]
    pub specs: Vec<Value>,
    /// The values being edited, by key.
    pub params: BTreeMap<String, Value>,
    /// The values the target was drawn with when the dialog opened (empty for a new valve).
    #[serde(skip)]
    pub start: BTreeMap<String, Value>,
    /// The flow paths of each position, one cell per position (the `paths` parameter split at `;`).
    pub paths: Vec<String>,
    /// The last preview: the `glyph.build` result for `params`, or why it failed.
    #[serde(skip)]
    preview: Option<(BTreeMap<String, Value>, Result<Value, String>)>,
}

impl State {
    /// The params `glyph.build` and OK send: every edited value, `paths` joined from the table.
    pub fn build_params(&self) -> Map<String, Value> {
        let mut m: Map<String, Value> = self.params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let positions = self.positions();
        let cells: Vec<&str> = self.paths.iter().take(positions).map(|s| s.trim()).collect();
        let joined = if cells.iter().all(|c| c.is_empty()) { String::new() } else { cells.join("; ") };
        m.insert("paths".into(), json!(joined));
        m
    }

    /// The number of positions chosen.
    pub fn positions(&self) -> usize {
        self.params.get("positions").and_then(Value::as_u64).map_or(2, |n| n as usize).clamp(1, MAX_POSITIONS)
    }
}

/// The dialog seeded from the selected valve (its drawn values) or the builder's defaults.
pub fn open(app: &mut CraftApp) -> State {
    let specs = app
        .session
        .run("glyph.builders", &json!({"builder": BUILDER}))
        .ok()
        .and_then(|r| r["builders"][0]["params"].as_array().cloned())
        .unwrap_or_default();
    let mut s = State { specs, loaded: true, ..State::default() };
    for p in &s.specs {
        if let Some(k) = p["key"].as_str() {
            s.params.insert(k.to_string(), p["default"].clone());
        }
    }
    let selected = match app.session.sel.items.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    };
    if let Some(id) = selected
        && let Ok(r) = app.session.run("glyph.params", &json!({"id": id}))
        && r["builder"] == BUILDER
    {
        for p in r["params"].as_array().map(Vec::as_slice).unwrap_or_default() {
            if let Some(k) = p["key"].as_str() {
                s.params.insert(k.to_string(), p["value"].clone());
            }
        }
        s.start = s.params.clone();
        s.target = id;
    }
    let paths = s.params.get("paths").and_then(Value::as_str).unwrap_or_default().to_string();
    s.paths = paths.split(';').map(|p| p.trim().to_string()).collect();
    s.paths.resize(MAX_POSITIONS, String::new());
    s
}

/// One command path of the preview: points in page units, and whether it closes.
type Sub = (Vec<Pos2>, bool);

/// Parse the `M`, `L`, `C` and `Z` path data `glyph.build` writes (numbers separated by spaces,
/// each command letter on its first number). A curve keeps its end point only, which is enough
/// for a preview; anything unreadable ends the parse.
pub fn parse_path(d: &str) -> Vec<Sub> {
    let mut out: Vec<Sub> = Vec::new();
    let mut cmd = ' ';
    let mut nums: Vec<f32> = Vec::new();
    let flush = |cmd: char, nums: &mut Vec<f32>, out: &mut Vec<Sub>| {
        match cmd {
            'M' => {
                if let [x, y, ..] = nums.as_slice() {
                    out.push((vec![Pos2::new(*x, *y)], false));
                }
            }
            'L' => {
                if let ([x, y, ..], Some(sub)) = (nums.as_slice(), out.last_mut()) {
                    sub.0.push(Pos2::new(*x, *y));
                }
            }
            'C' => {
                if let ([_, _, _, _, x, y, ..], Some(sub)) = (nums.as_slice(), out.last_mut()) {
                    sub.0.push(Pos2::new(*x, *y));
                }
            }
            'Z' => {
                if let Some(sub) = out.last_mut() {
                    sub.1 = true;
                }
            }
            _ => {}
        }
        nums.clear();
    };
    for tok in d.split_whitespace().take(100_000) {
        let mut rest = tok;
        if let Some(c) = tok.chars().next().filter(char::is_ascii_alphabetic) {
            flush(cmd, &mut nums, &mut out);
            cmd = c.to_ascii_uppercase();
            rest = &tok[c.len_utf8()..];
        }
        if !rest.is_empty() {
            match rest.parse::<f32>() {
                Ok(n) if n.is_finite() => nums.push(n),
                _ => break,
            }
        }
    }
    flush(cmd, &mut nums, &mut out);
    out
}

/// Draw the built valve scaled into `rect`: filled outlines (arrow heads, pilot triangles) and
/// stroked lines, with its ports marked.
fn draw_preview(ui: &mut Ui, t: &Tokens, built: &Value, rect: Rect) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.page);
    let (w, h) = (built["w"].as_f64().unwrap_or(1.0).max(1e-3) as f32, built["h"].as_f64().unwrap_or(1.0).max(1e-3) as f32);
    let k = ((rect.width() - 24.0) / w).min((rect.height() - 24.0) / h).max(0.01);
    let origin = rect.center() - vec2(w * k / 2.0, h * k / 2.0);
    let map = |q: Pos2| origin + vec2(q.x * k, q.y * k);
    let ink = Color32::from_rgb(0x1e, 0x26, 0x30);
    let stroke = Stroke::new(1.4, ink);
    // The outline is filled even-odd in the exports (a box is an outer and an inner contour);
    // the preview fills only small closed contours (arrow heads, pilot triangles) and traces the
    // rest, which draws a box's two contours as its line.
    let small = 0.2 * w.min(h);
    for (pts, closed) in parse_path(built["d"].as_str().unwrap_or_default()) {
        let r = Rect::from_points(&pts);
        let mut pts: Vec<Pos2> = pts.into_iter().map(map).collect();
        if closed && pts.len() >= 3 && r.width() <= small && r.height() <= small {
            p.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
        } else {
            if closed && let Some(first) = pts.first().copied() {
                pts.push(first);
            }
            p.add(egui::Shape::line(pts, Stroke::new(1.0, ink)));
        }
    }
    for (pts, closed) in parse_path(built["lines"].as_str().unwrap_or_default()) {
        let mut pts: Vec<Pos2> = pts.into_iter().map(map).collect();
        if closed && let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        p.add(egui::Shape::line(pts, stroke));
    }
    for port in built["ports"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let at = map(Pos2::new(port["x"].as_f64().unwrap_or(0.0) as f32, port["y"].as_f64().unwrap_or(0.0) as f32));
        p.circle_filled(at, 2.5, t.accent);
        let label = port["label"].as_str().unwrap_or_default();
        p.text(at + vec2(4.0, 2.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(10.0), t.text_dim);
    }
}

/// The dialog body: the parameter form at the left, the preview at the right.
pub fn body(app: &mut CraftApp, ui: &mut Ui, s: &mut State) {
    let t = Tokens::get(ui.ctx());
    if s.specs.is_empty() {
        ui.label("The valve builder is not available.");
        return;
    }
    let what = if s.target.is_empty() { "A new valve, placed in the middle of the view".to_string() } else { format!("Valve {}", s.target) };
    ui.label(egui::RichText::new(what).small().color(t.text_dim));
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(250.0);
            form(ui, &t, s);
        });
        let params = s.build_params();
        let key: BTreeMap<String, Value> = params.clone().into_iter().collect();
        if s.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            let r = app.session.run("glyph.build", &json!({"builder": BUILDER, "params": params})).map_err(|e| e.to_string());
            s.preview = Some((key, r));
        }
        let (rect, _) = ui.allocate_exact_size(vec2(230.0, 200.0), Sense::hover());
        match s.preview.as_ref().map(|(_, r)| r) {
            Some(Ok(built)) => {
                draw_preview(ui, &t, built, rect);
                let warnings: Vec<&str> = built["warnings"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                if !warnings.is_empty() {
                    ui.label(egui::RichText::new(warnings.join("\n")).small().color(t.red));
                }
            }
            Some(Err(e)) => {
                ui.painter_at(rect).text(rect.center(), egui::Align2::CENTER_CENTER, e, egui::FontId::proportional(11.0), t.red);
            }
            None => {}
        }
    });
}

/// The parameter form: one editor per spec, with the paths as a table of one row per position.
fn form(ui: &mut Ui, t: &Tokens, s: &mut State) {
    let positions = s.positions();
    egui::Grid::new("valve-form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        for spec in s.specs.clone() {
            let key = spec["key"].as_str().unwrap_or_default().to_string();
            if key == "paths" {
                continue;
            }
            let label = spec["label"].as_str().unwrap_or(&key).to_string();
            ui.label(egui::RichText::new(&label).color(t.text_dim)).on_hover_text(spec["description"].as_str().unwrap_or_default());
            let v = s.params.entry(key.clone()).or_insert(Value::Null);
            match spec["kind"].as_str().unwrap_or_default() {
                "choice" => {
                    let current = v.as_str().unwrap_or_default().to_string();
                    egui::ComboBox::from_id_salt(("valve", key.as_str())).selected_text(current.clone()).width(130.0).show_ui(ui, |ui| {
                        for c in spec["choices"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_str) {
                            if ui.selectable_label(current == c, c).clicked() {
                                *v = json!(c);
                            }
                        }
                    });
                }
                "int" | "number" => {
                    let int = spec["kind"] == "int";
                    let mut n = v.as_f64().unwrap_or(0.0);
                    let (lo, hi) = (spec["min"].as_f64().unwrap_or(0.0), spec["max"].as_f64().unwrap_or(1000.0));
                    let r = ui.add(egui::DragValue::new(&mut n).range(lo..=hi).speed(0.05).max_decimals(if int { 0 } else { 2 }));
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, label.clone()));
                    if r.changed() && n.is_finite() {
                        *v = if int { json!(n.round() as i64) } else { json!(n) };
                    }
                }
                "bool" => {
                    let mut on = v.as_bool().unwrap_or(false);
                    if ui.checkbox(&mut on, "").changed() {
                        *v = json!(on);
                    }
                }
                _ => {
                    let mut text = v.as_str().unwrap_or_default().to_string();
                    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(130.0)).changed() {
                        *v = json!(text);
                    }
                }
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Flow paths").color(t.text_dim))
        .on_hover_text("Per position: P-A flows from P to A, P| blocks P, X blocks every port. Leave all empty for the usual paths.");
    egui::Grid::new("valve-paths").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, cell) in s.paths.iter_mut().enumerate().take(positions) {
            ui.label(format!("Position {}", i + 1));
            ui.add(egui::TextEdit::singleline(cell).hint_text("usual").desired_width(130.0));
            ui.end_row();
        }
    });
}

/// After the buttons: on OK update the selected valve (`glyph.setParams` with the values that
/// changed) or place a new one (`glyph.insert`). Returns whether to close.
pub fn finish(app: &mut CraftApp, s: &State, ok: bool, cancel: bool) -> bool {
    if !ok {
        return cancel;
    }
    let params = s.build_params();
    let r = if s.target.is_empty() {
        app.run("glyph.insert", json!({"builder": BUILDER, "params": params}))
    } else {
        let start_paths = s.start.get("paths").cloned().unwrap_or(Value::Null);
        let changed: Map<String, Value> = params
            .into_iter()
            .filter(|(k, v)| if k == "paths" { *v != start_paths && !(v == "" && start_paths.is_null()) } else { s.start.get(k) != Some(v) })
            .collect();
        if changed.is_empty() {
            return true;
        }
        app.run("glyph.setParams", json!({"ids": [s.target], "params": changed}))
    };
    // A refusal shows in the status bar (`app.run` puts it there); the dialog stays open.
    r.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_parses_moves_lines_curves_and_closes() {
        let subs = parse_path("M0 0 L10 0 L10 5 Z M1 1 C2 2 3 3 4 4 L junk 7");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].0.len(), 3);
        assert!(subs[0].1);
        assert_eq!(subs[1].0, vec![Pos2::new(1.0, 1.0), Pos2::new(4.0, 4.0)]);
        assert!(parse_path("").is_empty());
        assert!(parse_path("L1 2 Z").is_empty(), "a line before a move draws nothing");
    }

    #[test]
    fn the_path_table_joins_into_the_paths_parameter() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default());
        let mut s = open(&mut app);
        assert!(s.target.is_empty());
        assert_eq!(s.params["positions"].as_f64(), Some(2.0));
        assert_eq!(s.build_params()["paths"], "", "all empty: the usual paths");
        s.params.insert("positions".into(), json!(3));
        s.paths = vec!["P-A B-T".into(), "X".into(), "P-B A-T".into()];
        assert_eq!(s.build_params()["paths"], "P-A B-T; X; P-B A-T");
        s.params.insert("positions".into(), json!(2));
        assert_eq!(s.build_params()["paths"], "P-A B-T; X", "only the positions the valve has");
    }
}
