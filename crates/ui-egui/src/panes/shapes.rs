//! The Shapes pane: a search field and the stencil sets as grids of master tiles that insert on
//! click or drag onto the canvas; also `ui.shapesSearch`.

use egui::{CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::{Tokens, regular, semibold};

use super::{PaneSpec, Side};

/// The Shapes pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("shapes", "Shapes", Side::Left, 240.0, shapes);

/// Drag-and-drop payload of a master dragged from the Shapes pane onto the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterDrag(pub String);

/// Size of one master tile in the Shapes pane.
const TILE: egui::Vec2 = egui::vec2(66.0, 66.0);

/// A small drawing of a master's outline, fitted into `r` (document colours: it previews what
/// the master inserts).
fn master_preview(painter: &egui::Painter, m: &vizcraft_stencils::Master, r: Rect) {
    use crate::canvas::draw::{color, pos};
    let (w, h) = (m.w.max(1.0), m.h.max(1.0));
    let k = (f64::from(r.width()) / w).min(f64::from(r.height()) / h);
    let (w, h) = (w * k, h * k);
    let origin = r.center() - vec2(w as f32, h as f32) / 2.0;
    let place = |p: vizcraft_diagram::Point| origin + pos(p).to_vec2();
    let stroke = Stroke::new(1.0, color(m.style.line));
    for pl in m.kind.outline(w, h).flatten(0.3) {
        let pts: Vec<egui::Pos2> = pl.points.iter().map(|p| place(*p)).collect();
        if pl.closed {
            let tris = crate::canvas::tess::triangulate(&pts);
            let mut mesh = egui::epaint::Mesh::default();
            for p in &pts {
                mesh.colored_vertex(*p, color(m.style.fill));
            }
            for [a, b, c] in tris {
                mesh.add_triangle(a, b, c);
            }
            painter.add(egui::Shape::mesh(mesh));
            painter.add(egui::Shape::closed_line(pts, stroke));
        } else {
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
    if let Some(d) = m.kind.detail(w, h) {
        for pl in d.flatten(0.3) {
            let pts: Vec<egui::Pos2> = pl.points.iter().map(|p| place(*p)).collect();
            painter.add(if pl.closed { egui::Shape::closed_line(pts, stroke) } else { egui::Shape::line(pts, stroke) });
        }
    }
}

/// One master tile: preview and label. Click inserts at the view centre; dragging carries a
/// [`MasterDrag`] to the canvas.
fn master_tile(app: &mut CraftApp, ui: &mut Ui, m: &'static vizcraft_stencils::Master) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(TILE, Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, m.label));
    let resp = resp.on_hover_text(m.label);
    if resp.hovered() || resp.dragged() {
        ui.painter().rect_filled(r, CornerRadius::same(4), t.hover);
    }
    let art = Rect::from_min_size(r.min + vec2(14.0, 6.0), vec2(r.width() - 28.0, 30.0));
    master_preview(ui.painter(), m, art);
    // One line when it fits; two-word labels wrap; a single long word shrinks to fit instead of
    // breaking mid-word.
    let room = r.width() - 4.0;
    let mut galley = ui.painter().layout_no_wrap(m.label.to_string(), regular(10.5), t.text);
    if galley.size().x > room {
        galley = if m.label.contains(' ') {
            ui.painter().layout(m.label.to_string(), regular(10.5), t.text, room)
        } else {
            let size = (10.5 * room / galley.size().x).max(7.0);
            ui.painter().layout_no_wrap(m.label.to_string(), regular(size), t.text)
        };
    }
    let at = pos2(r.center().x - galley.size().x / 2.0, art.bottom() + 4.0);
    ui.painter().with_clip_rect(r).galley(at, galley, t.text);
    if resp.dragged() {
        egui::DragAndDrop::set_payload(ui.ctx(), MasterDrag(m.id.to_string()));
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        // A ghost of the master follows the pointer.
        if let Some(p) = ui.ctx().pointer_interact_pos() {
            let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("master-drag-ghost"));
            let ghost = Rect::from_center_size(p, vec2(48.0, 32.0));
            master_preview(&ui.ctx().layer_painter(layer), m, ghost);
        }
    }
    if resp.clicked() {
        let at = crate::canvas::insert_point(app);
        crate::canvas::insert_master(app, m.id, at);
    }
}

/// Tiles in rows of as many as fit the pane's width.
fn tile_grid(app: &mut CraftApp, ui: &mut Ui, masters: &[&'static vizcraft_stencils::Master]) {
    let cols = ((ui.available_width() / TILE.x).floor() as usize).max(1);
    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
    for row in masters.chunks(cols) {
        ui.horizontal(|ui| {
            for m in row {
                master_tile(app, ui, m);
            }
        });
    }
}

/// Height of the Shapes pane's search field.
const SEARCH_H: f32 = 24.0;

/// The Shapes pane's search field: a magnifier, the text and, with text, a clear button. Escape
/// clears it. Edits `app.shapes_query`, which is not saved between runs.
fn search_field(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), SEARCH_H), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(4), t.input, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    let icon = Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(14.0, 14.0));
    crate::icons::paint(ui.painter(), icon, "search", t.text_dim, t.text_dim);
    let has_text = !app.shapes_query.is_empty();
    let clear = Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(18.0, 18.0));
    let text_r = Rect::from_min_max(
        pos2(icon.right() + 5.0, r.top() + 2.0),
        pos2(if has_text { clear.left() - 2.0 } else { r.right() - 6.0 }, r.bottom() - 1.0),
    );
    let edit = egui::TextEdit::singleline(&mut app.shapes_query)
        .id_salt("shapes-search")
        .hint_text(egui::RichText::new("Search shapes").color(t.text_dim))
        .font(regular(12.0))
        .text_color(t.text)
        .frame(egui::Frame::NONE)
        .margin(vec2(0.0, 3.5))
        .desired_width(text_r.width());
    let resp = ui.put(text_r, edit);
    // egui gives up a text field's focus on Escape, so the press is seen as focus lost.
    if (resp.has_focus() || resp.lost_focus()) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.shapes_query.clear();
    }
    if has_text {
        let cresp = ui.interact(clear, ui.id().with("shapes-search-clear"), Sense::click());
        cresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Clear search"));
        if cresp.hovered() {
            ui.painter().rect_filled(clear, CornerRadius::same(3), t.hover);
        }
        crate::icons::paint(ui.painter(), clear.shrink(3.0), "close", if cresp.hovered() { t.text } else { t.text_dim }, t.text_dim);
        if cresp.on_hover_text("Clear search").clicked() {
            app.shapes_query.clear();
        }
    }
}

/// The Shapes pane: a search field, then each stencil set as a collapsible section of master
/// tiles, or one flat "Results" grid while the field has text.
fn shapes(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    search_field(app, ui);
    ui.add_space(6.0);
    let query = app.shapes_query.trim().to_string();
    if !query.is_empty() {
        let hits = vizcraft_stencils::search(&query);
        if hits.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("No shapes match").font(regular(12.0)).color(t.text_dim));
        } else {
            ui.label(egui::RichText::new("Results").font(semibold(12.0)).color(t.text));
            ui.add_space(2.0);
            tile_grid(app, ui, &hits);
        }
        return;
    }
    for set in vizcraft_stencils::sets() {
        egui::CollapsingHeader::new(egui::RichText::new(set.name).font(semibold(12.0)))
            .id_salt(("stencil-set", set.id))
            .default_open(true)
            .show_unindented(ui, |ui| {
                let masters: Vec<&'static vizcraft_stencils::Master> = set.masters.iter().collect();
                tile_grid(app, ui, &masters);
            });
    }
}

/// `ui.shapesSearch {query}`: set the Shapes pane's search text (a missing or non-string query
/// clears it). Text opens the pane if it is closed. The reply is the query and the ids of the
/// matching masters, best first.
pub(super) fn shapes_search(app: &mut CraftApp, p: &Value) -> Value {
    // Same cap as a pasted query in the field would meet in the index; keeps the reply small.
    app.shapes_query = p.get("query").and_then(Value::as_str).unwrap_or("").chars().take(256).collect();
    let query = app.shapes_query.trim();
    if !query.is_empty() && !app.ui.panes.is_open("shapes") {
        app.ui.panes.open("shapes", None);
    }
    if !query.is_empty() && app.ui.panes.is_collapsed("shapes") {
        app.ui.panes.set_collapsed("shapes", false);
    }
    let results: Vec<&str> = if query.is_empty() { Vec::new() } else { vizcraft_stencils::search(query).iter().map(|m| m.id).collect() };
    json!({"query": app.shapes_query, "results": results})
}
