//! The Palette pane (key `shapes`): a search field and the scoped palette of the active page
//! (see [`crate::palette`]): a compact list per group, the connector kinds as rows that arm the
//! connector tool, and the Drawing group as a tile grid. Masters insert on click or drag onto the
//! canvas. Also `ui.palette` and `ui.shapesSearch`.

use egui::{CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::palette::{self, Group, Item, What};
use crate::theme::{Tokens, regular, semibold};

use super::{PaneSpec, Side};

/// The Palette pane.
pub(super) const SPEC: PaneSpec = PaneSpec::new("shapes", "Palette", Side::Left, 240.0, shapes);

/// Drag-and-drop payload of a master dragged from the Shapes pane onto the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterDrag(pub String);

/// Size of one master tile in the Shapes pane.
const TILE: egui::Vec2 = egui::vec2(66.0, 66.0);

/// A small drawing of a master's outline, fitted into `r` (document colours: it previews what
/// the master inserts). A path glyph previews from its path: closed rings fill even-odd, open
/// ones are lines.
pub(crate) fn master_preview(painter: &egui::Painter, m: &vizcraft_stencils::Master, r: Rect) {
    use crate::canvas::draw::{color, pos};
    // The fork and join bars are one drawing; the palette shows their flows instead.
    if let Some(join) = crate::icons::fork_join_master(m.id) {
        return crate::icons::paint_fork_join(painter, r.expand2(vec2(-6.0, 0.0)), join, color(m.style.line));
    }
    let (w, h) = (m.w.max(1.0), m.h.max(1.0));
    let k = (f64::from(r.width()) / w).min(f64::from(r.height()) / h);
    let (w, h) = (w * k, h * k);
    let origin = r.center() - vec2(w as f32, h as f32) / 2.0;
    let place = |p: vizcraft_diagram::Point| origin + pos(p).to_vec2();
    let stroke = Stroke::new(1.0, color(m.style.line));
    let outline = match &m.path {
        Some(p) => p.transformed(&vizcraft_diagram::Transform::scale(w, h)),
        None => m.kind.outline(w, h),
    };
    let rings: Vec<(Vec<egui::Pos2>, bool)> = outline.flatten(0.3).into_iter().map(|pl| (pl.points.iter().map(|p| place(*p)).collect(), pl.closed)).collect();
    let closed: Vec<Vec<egui::Pos2>> = rings.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
    for (pts, tris) in crate::canvas::tess::even_odd(&closed) {
        let mut mesh = egui::epaint::Mesh::default();
        for p in &pts {
            mesh.colored_vertex(*p, color(m.style.fill));
        }
        for [a, b, c] in tris {
            mesh.add_triangle(a, b, c);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    for (pts, closed) in rings {
        painter.add(if closed { egui::Shape::closed_line(pts, stroke) } else { egui::Shape::line(pts, stroke) });
    }
    if let Some(d) = m.kind.detail(w, h) {
        for pl in d.flatten(0.3) {
            let pts: Vec<egui::Pos2> = pl.points.iter().map(|p| place(*p)).collect();
            painter.add(if pl.closed { egui::Shape::closed_line(pts, stroke) } else { egui::Shape::line(pts, stroke) });
        }
    }
}

/// A small drawing of a glyph builder's default symbol fitted into `r`, in the default shape
/// colours (it previews what `glyph.insert` places): closed rings filled even-odd, open ones
/// stroked.
fn builder_preview(painter: &egui::Painter, name: &str, r: Rect) {
    use crate::canvas::draw::{color, pos};
    let Some(b) = palette::builder_preview(name) else { return };
    let st = vizcraft_diagram::Style::DEFAULT;
    let k = (f64::from(r.width()) / b.w.max(1.0)).min(f64::from(r.height()) / b.h.max(1.0));
    let origin = r.center() - vec2((b.w * k) as f32, (b.h * k) as f32) / 2.0;
    let place = |p: vizcraft_diagram::Point| origin + pos(p).to_vec2();
    let outline = b.path.transformed(&vizcraft_diagram::Transform::scale(k, k));
    let rings: Vec<(Vec<egui::Pos2>, bool)> = outline.flatten(0.3).into_iter().map(|pl| (pl.points.iter().map(|p| place(*p)).collect(), pl.closed)).collect();
    let closed: Vec<Vec<egui::Pos2>> = rings.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
    for (pts, tris) in crate::canvas::tess::even_odd(&closed) {
        let mut mesh = egui::epaint::Mesh::default();
        for p in &pts {
            mesh.colored_vertex(*p, color(st.fill));
        }
        for [a, b, c] in tris {
            mesh.add_triangle(a, b, c);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    let stroke = Stroke::new(1.0, color(st.line));
    for (pts, closed) in rings {
        painter.add(if closed { egui::Shape::closed_line(pts, stroke) } else { egui::Shape::line(pts, stroke) });
    }
}

/// The master the active page draws `m`'s element type with: its rendering variant's glyph when
/// the variant maps the type, else `m`.
fn shown(app: &CraftApp, m: &'static vizcraft_stencils::Master) -> &'static vizcraft_stencils::Master {
    let variant = app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.variant.as_str()).unwrap_or_default();
    if variant.is_empty() {
        return m;
    }
    vizcraft_stencils::variant_master(m.id, variant).unwrap_or(m)
}

/// One master tile: preview and label. Click inserts at the view centre; dragging carries a
/// [`MasterDrag`] to the canvas.
fn master_tile(app: &mut CraftApp, ui: &mut Ui, m: &'static vizcraft_stencils::Master) {
    let m = shown(app, m);
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
    act(app, ui, &resp, What::Master(m));
}

/// What a click or a drag on a palette entry does: a master inserts at the visible centre (a drag
/// carries a [`MasterDrag`] to the canvas, with a ghost under the pointer); a connector kind arms
/// the connector tool with that kind.
fn act(app: &mut CraftApp, ui: &mut Ui, resp: &egui::Response, what: What) {
    match what {
        What::Master(m) => {
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
        What::Kind(k) => {
            if resp.clicked() {
                let _ = app.run("ui.tool", json!({"tool": "connector", "kind": k.id}));
            }
        }
        What::Builder(b) => {
            if resp.dragged() {
                egui::DragAndDrop::set_payload(ui.ctx(), MasterDrag(format!("{}{}", palette::BUILDER_DRAG, b.name)));
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                if let Some(p) = ui.ctx().pointer_interact_pos() {
                    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("master-drag-ghost"));
                    builder_preview(&ui.ctx().layer_painter(layer), b.name, Rect::from_center_size(p, vec2(48.0, 32.0)));
                }
            }
            if resp.clicked() {
                let at = crate::canvas::insert_point(app);
                crate::canvas::insert_builder(app, b.name, at);
            }
        }
    }
}

/// Height of one list row.
const ROW_H: f32 = 22.0;

/// A small drawing of a connector kind: its line (dashed as the kind says) and line ends, fitted
/// into `r`.
fn kind_preview(painter: &egui::Painter, k: &vizcraft_notation::ConnectorType, r: Rect, fallback: egui::Color32) {
    use vizcraft_diagram::{Arrow, Color, Dash};
    let col = Color::parse(&k.line).map_or(fallback, crate::canvas::draw::color);
    let stroke = Stroke::new((k.line_width as f32).clamp(1.0, 2.5), col);
    let (a, b) = (r.left_center(), r.right_center());
    let has = |name: &str| Arrow::from_name(name).is_some_and(|x| x != Arrow::None);
    let (a, b) = (a + vec2(if has(&k.start_arrow) { 4.0 } else { 0.0 }, 0.0), b - vec2(if has(&k.end_arrow) { 4.0 } else { 0.0 }, 0.0));
    match Dash::from_name(&k.dash).unwrap_or_default() {
        Dash::Solid => {
            painter.line_segment([a, b], stroke);
        }
        _ => {
            painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 3.0, 2.0));
        }
    }
    if has(&k.end_arrow) {
        painter.add(egui::Shape::convex_polygon(vec![b + vec2(4.0, 0.0), b + vec2(-1.0, -3.0), b + vec2(-1.0, 3.0)], col, Stroke::NONE));
    }
    if has(&k.start_arrow) {
        painter.add(egui::Shape::convex_polygon(vec![a - vec2(4.0, 0.0), a + vec2(1.0, -3.0), a + vec2(1.0, 3.0)], col, Stroke::NONE));
    }
}

/// One list row: a small preview and the label. The connector kind in use is drawn checked.
fn item_row(app: &mut CraftApp, ui: &mut Ui, item: &Item) {
    item_row_in(app, ui, item, false);
}

/// [`item_row`], with the pack's name dimmed at the right when `show_pack` is set. The accessible
/// label stays the row's own label.
fn item_row_in(app: &mut CraftApp, ui: &mut Ui, item: &Item, show_pack: bool) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    let label = item.label.as_str();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let in_use = matches!(item.what, What::Kind(k) if app.tool == crate::Tool::Connector && app.connector_kind == k.id);
    if in_use {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.checked);
    } else if resp.hovered() || resp.dragged() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    let art = Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(26.0, 16.0));
    let what = match item.what {
        What::Master(m) => What::Master(shown(app, m)),
        other => other,
    };
    match what {
        What::Master(m) => master_preview(ui.painter(), m, art),
        What::Kind(k) => kind_preview(ui.painter(), k, art, t.text),
        What::Builder(b) => builder_preview(ui.painter(), b.name, art),
    }
    ui.painter().with_clip_rect(r).text(pos2(r.left() + 40.0, r.center().y), egui::Align2::LEFT_CENTER, label, regular(12.0), t.text);
    if show_pack && !item.pack.is_empty() {
        ui.painter().with_clip_rect(r).text(pos2(r.right() - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, &item.pack, regular(11.0), t.text_dim);
    }
    let tip = match what {
        // The type's description when the pack has one (Fork: splits one flow into concurrent
        // flows), else the label.
        What::Master(m) => item
            .type_id
            .and_then(|t| vizcraft_notation::catalog().element_type(t))
            .map(|(_, et)| et.description.as_str())
            .filter(|d| !d.is_empty())
            .unwrap_or(m.label),
        What::Kind(_) => "Draw connectors of this kind",
        What::Builder(b) => b.description,
    };
    act(app, ui, &resp.on_hover_text(tip), what);
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

/// Draw one group: a collapsing heading (open as the group starts, unless forced otherwise) over
/// a list of rows, or a tile grid for the Drawing group.
fn group_body(app: &mut CraftApp, ui: &mut Ui, section: &str, g: &Group, focus: Option<&str>) {
    let forced = focus.is_some_and(|f| f == g.id || f.eq_ignore_ascii_case(&g.label)).then_some(true);
    egui::CollapsingHeader::new(egui::RichText::new(g.label.as_str()).font(semibold(12.0)))
        .id_salt(("palette-group", section, g.id.as_str()))
        .default_open(g.open)
        .open(forced)
        .show_unindented(ui, |ui| {
            if g.tiles {
                let masters: Vec<&'static vizcraft_stencils::Master> = g.items.iter().filter_map(|i| if let What::Master(m) = i.what { Some(m) } else { None }).collect();
                tile_grid(app, ui, &masters);
            } else {
                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                for item in &g.items {
                    item_row(app, ui, item);
                }
            }
        });
}

/// The Palette pane: a search field, then the scoped palette of the active page, or one flat
/// "Results" list while the field has text.
fn shapes(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    search_field(app, ui);
    ui.add_space(4.0);
    let pal = palette::for_app(app);
    if pal.viewpoint.is_some() {
        let mut on = app.palette_filter;
        if ui.checkbox(&mut on, egui::RichText::new("Only this viewpoint").font(regular(11.5)).color(t.text_dim)).changed() {
            let _ = app.run("ui.palette", json!({"viewpointFilter": on}));
        }
    }
    ui.add_space(2.0);
    let query = app.shapes_query.trim().to_string();
    if !query.is_empty() {
        let hits = pal.search(&query);
        if hits.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("No shapes match").font(regular(12.0)).color(t.text_dim));
        } else {
            ui.label(egui::RichText::new("Results").font(semibold(12.0)).color(t.text));
            ui.add_space(2.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
            // Two hits with the same label (Flowchart's and UML's Decision) are told apart by the
            // pack named, dimmed, at the right of the row.
            let shared: Vec<bool> = hits.iter().map(|a| hits.iter().filter(|b| b.label == a.label).count() > 1).collect();
            for (item, shared) in hits.into_iter().zip(shared) {
                item_row_in(app, ui, item, shared);
            }
        }
        return;
    }
    let focus = app.palette_focus.take();
    for sec in &pal.sections {
        match &sec.label {
            Some(name) => {
                let open = focus.as_deref().is_some_and(|f| f == sec.id || f.eq_ignore_ascii_case(name) || sec.groups.iter().any(|g| f == g.id)).then_some(true);
                egui::CollapsingHeader::new(egui::RichText::new(name.as_str()).font(semibold(12.5)))
                    .id_salt(("palette-pack", sec.id.as_str()))
                    .default_open(sec.open)
                    .open(open)
                    .show_unindented(ui, |ui| {
                        for g in &sec.groups {
                            group_body(app, ui, &sec.id, g, focus.as_deref());
                        }
                    });
            }
            None => {
                for g in &sec.groups {
                    group_body(app, ui, &sec.id, g, focus.as_deref());
                }
            }
        }
    }
}

/// `ui.palette {query?, group?, viewpointFilter?}`: set the search text, open the group (or pack)
/// named by id or label, and switch the viewpoint filter. A query opens the pane if it is
/// closed. The reply is the palette as `ui.inspect` reports it.
pub(super) fn palette_command(app: &mut CraftApp, p: &Value) -> Value {
    if let Some(q) = p.get("query").and_then(Value::as_str) {
        app.shapes_query = q.chars().take(256).collect();
    }
    if let Some(f) = p.get("viewpointFilter").and_then(Value::as_bool) {
        app.palette_filter = f;
    }
    if let Some(g) = p.get("group").and_then(Value::as_str) {
        app.palette_focus = Some(g.chars().take(128).collect());
    }
    if p.get("group").is_some() || !app.shapes_query.trim().is_empty() {
        if !app.ui.panes.is_open("shapes") {
            app.ui.panes.open("shapes", None);
        }
        if app.ui.panes.is_collapsed("shapes") {
            app.ui.panes.set_collapsed("shapes", false);
        }
    }
    palette::for_app(app).report(app)
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
