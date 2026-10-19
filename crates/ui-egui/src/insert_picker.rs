//! The insert picker: a type-to-filter list at a canvas point. A double-click on empty canvas
//! opens it there, `/` opens it at the pointer and `ui.insertPicker` opens it by id; arrows pick,
//! Enter runs the row, Escape closes.
//!
//! Rows, in order: with one shape selected the legal next steps from it ("Composition to Class",
//! `insert.next`); with two shapes selected the relationships that may join them (`insert.connect`;
//! illegal choices are dimmed and still run, because rules lint and never block); the page's
//! palette types and the Drawing primitives (inserted at the point, as a Shapes pane drop does);
//! and the patterns (`pattern.insert`).

use egui::{Align2, Sense, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::Point;

use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::draw;
use crate::theme::{Tokens, regular};

/// Most rows the picker lists.
const MAX_ROWS: usize = 30;
/// Longest query kept.
const MAX_QUERY: usize = 200;

/// The open picker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picker {
    /// Where the shape goes, in world coordinates (its centre).
    pub at: Point,
    /// The filter text.
    pub query: String,
    /// The highlighted row.
    pub selected: usize,
    /// Frames left in which a typed `/` is the key that opened the picker, not part of the query.
    pub skip_slash: u8,
}

/// What a row does when chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// Insert a shape drawn by `master` at the picker's point.
    Shape {
        /// The master inserted.
        master: String,
    },
    /// A legal next step from shape `from`: `insert.next`, then naming.
    Next {
        /// The shape the step follows.
        from: String,
        /// The element type (or stencil) of the new shape.
        type_id: String,
        /// The relationship type joining them, `None` between free shapes.
        relationship: Option<String>,
    },
    /// A relationship between two selected shapes: `insert.connect`.
    Connect {
        /// Source shape.
        from: String,
        /// Target shape.
        to: String,
        /// The relationship type, empty for a plain connector between free shapes.
        relationship: String,
        /// Whether the pack's connection table allows it.
        legal: bool,
    },
    /// A pattern: `pattern.insert` with default arguments.
    Pattern {
        /// The pattern id.
        id: String,
    },
}

impl ItemKind {
    /// The name `ui.insertPicker` reports for the kind.
    pub fn name(&self) -> &'static str {
        match self {
            ItemKind::Shape { .. } => "shape",
            ItemKind::Next { .. } => "next",
            ItemKind::Connect { .. } => "connect",
            ItemKind::Pattern { .. } => "pattern",
        }
    }
}

/// One row: an element type, a stencil, a relationship or a pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Element type id, stencil id, relationship type id or pattern id.
    pub id: String,
    /// What choosing the row does.
    pub kind: ItemKind,
    /// Label shown.
    pub label: String,
    /// Group shown dimmer next to the label.
    pub group: String,
}

/// The shapes selected on the active page, in selection order.
fn selected_shapes(app: &CraftApp) -> Vec<String> {
    let Some(page) = app.session.doc.page() else { return Vec::new() };
    app.session.sel.items.iter().filter(|id| page.shape(id).is_some()).cloned().collect()
}

/// Text to name a shape in a group heading: its text, else its type's label, cut short.
fn shape_name(app: &CraftApp, id: &str) -> String {
    let doc = &app.session.doc;
    let Some(s) = doc.page().and_then(|p| p.shape(id)) else { return String::new() };
    let text = s.text.lines().next().unwrap_or("").trim().to_string();
    let name = if !text.is_empty() {
        text
    } else {
        vizcraft_engine::suggest::element_type_of(doc, s)
            .and_then(|t| vizcraft_notation::catalog().element_type(t).map(|(_, et)| et.label.clone()))
            .unwrap_or_else(|| vizcraft_engine::suggest::stencil_of(s))
    };
    name.chars().take(24).collect()
}

/// The relationship rows: next steps from one selected shape, or the choices joining two.
fn relationship_items(app: &CraftApp) -> Vec<Item> {
    let cat = vizcraft_notation::catalog();
    let doc = &app.session.doc;
    let sel = selected_shapes(app);
    let mut out = Vec::new();
    match sel.as_slice() {
        [one] => {
            let group = format!("From {}", shape_name(app, one));
            for sg in vizcraft_engine::suggest::next(doc, one) {
                let rel_label = sg.relationship.as_deref().and_then(|r| cat.relationship_type(r)).map(|(_, rt)| rt.label.clone());
                let label = match rel_label {
                    Some(r) => format!("{r} to {}", sg.label),
                    None => format!("Next {}", sg.label),
                };
                out.push(Item {
                    id: sg.type_id.clone(),
                    kind: ItemKind::Next { from: one.clone(), type_id: sg.type_id, relationship: sg.relationship },
                    label,
                    group: group.clone(),
                });
            }
        }
        [a, b] => {
            for ch in vizcraft_engine::suggest::connect(doc, a, b) {
                let label = if ch.id.is_empty() { "Connector".to_string() } else { ch.label.clone() };
                out.push(Item {
                    id: ch.id.clone(),
                    kind: ItemKind::Connect { from: a.clone(), to: b.clone(), relationship: ch.id, legal: ch.legal },
                    label,
                    group: "Connect".into(),
                });
            }
        }
        _ => {}
    }
    out
}

/// Every row before filtering: relationship rows for the selection, the palette types of the
/// page's notation (its viewpoint's pack, else the packs in use, else every pack), the Drawing
/// primitives of the Basic pack, then the patterns (the page's pack first).
pub fn items(app: &CraftApp) -> Vec<Item> {
    let cat = vizcraft_notation::catalog();
    let doc = &app.session.doc;
    let viewpoint_pack =
        doc.page().and_then(|p| p.view.as_ref()).and_then(|v| v.viewpoint.split_once('.').map(|(p, _)| p.to_string())).filter(|p| !p.is_empty());
    let types: Vec<String> = match viewpoint_pack.as_deref().and_then(|p| cat.pack(p)) {
        Some(pack) => pack.palette.iter().flat_map(|g| g.types.iter().cloned()).collect(),
        None => vizcraft_engine::suggest::types_in_use(doc),
    };
    let mut out: Vec<Item> = relationship_items(app);
    for t in types {
        let (Some((pack, et)), Some(master)) = (cat.element_type(&t), cat.master_for_type(&t)) else { continue };
        if !out.iter().any(|i| i.id == t && matches!(i.kind, ItemKind::Shape { .. })) {
            out.push(Item { label: et.label.clone(), group: pack.name.clone(), kind: ItemKind::Shape { master: master.to_string() }, id: t });
        }
    }
    let primitives = cat.pack("basic").map(|p| p.palette.iter().flat_map(|g| g.glyphs.iter().cloned()).collect::<Vec<_>>()).unwrap_or_default();
    for g in primitives {
        if let Some(m) = vizcraft_stencils::master(&g)
            && !out.iter().any(|i| i.id == g && matches!(i.kind, ItemKind::Shape { .. }))
        {
            out.push(Item { id: g.clone(), kind: ItemKind::Shape { master: g }, label: m.label.to_string(), group: "Drawing".into() });
        }
    }
    for p in crate::panes::all_patterns(app) {
        out.push(Item { id: p.id.clone(), kind: ItemKind::Pattern { id: p.id.clone() }, label: p.name.clone(), group: "Pattern".into() });
    }
    out
}

/// The rows matching `query` (case-insensitive, label or id), label prefixes first.
pub fn filtered(app: &CraftApp, query: &str) -> Vec<Item> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<(u8, Item)> = items(app)
        .into_iter()
        .filter_map(|i| {
            let l = i.label.to_lowercase();
            let rank = if q.is_empty() || l.starts_with(&q) {
                0
            } else if l.contains(&q) || i.id.to_lowercase().contains(&q) || i.group.to_lowercase().contains(&q) {
                1
            } else {
                return None;
            };
            Some((rank, i))
        })
        .collect();
    hits.sort_by_key(|(r, _)| *r);
    hits.into_iter().map(|(_, i)| i).take(MAX_ROWS).collect()
}

/// Open the picker at world point `at` with `query` typed.
pub fn open(app: &mut CraftApp, at: Point, query: &str) {
    let query: String = query.chars().take(MAX_QUERY).collect();
    app.insertion.popover = None;
    app.insertion.picker = Some(Picker { at, query, selected: 0, skip_slash: 0 });
}

/// Run `item` at `at` and close the picker.
fn insert(app: &mut CraftApp, item: &Item, at: Point) {
    app.insertion.picker = None;
    match &item.kind {
        ItemKind::Shape { master } => crate::canvas::insert_master(app, master, at),
        ItemKind::Next { from, type_id, relationship } => {
            let mut params = json!({"id": from, "direction": crate::notation_layout::flow_direction(app).name(), "type": type_id});
            if let (Some(o), Some(r)) = (params.as_object_mut(), relationship) {
                o.insert("relationship".into(), json!(r));
            }
            if let Ok(r) = app.run("insert.next", params)
                && let Some(shape) = r.get("shape").and_then(Value::as_str)
            {
                crate::canvas::interact::start_editing(app, vizcraft_diagram::Hit::Shape(shape.to_string()));
            }
        }
        ItemKind::Connect { from, to, relationship, .. } => {
            let mut params = json!({"from": from, "to": to});
            if let (Some(o), false) = (params.as_object_mut(), relationship.is_empty()) {
                o.insert("relationship".into(), json!(relationship));
            }
            let _ = app.run("insert.connect", params);
        }
        ItemKind::Pattern { id } => {
            let _ = app.run("pattern.insert", json!({"pattern": id, "x": at.x.round(), "y": at.y.round()}));
        }
    }
}

/// Paint a short line ending in the head of relationship type `relationship`'s connector kind
/// (a plain arrow when it has none) inside `r`.
fn paint_relationship(painter: &egui::Painter, r: egui::Rect, relationship: &str, colour: egui::Color32) {
    let kind = vizcraft_engine::suggest::kind_for((!relationship.is_empty()).then_some(relationship));
    let arrow = vizcraft_stencils::connector_kind(&kind).map_or(vizcraft_diagram::Arrow::Triangle, |k| k.end_arrow);
    let y = r.center().y;
    let (x0, x1) = (r.left(), r.right());
    painter.line_segment([egui::pos2(x0, y), egui::pos2(x1 - 6.0, y)], egui::Stroke::new(1.2, colour));
    // The head is drawn in a unit space with the tip at the origin pointing right, scaled down.
    let Some(h) = vizcraft_diagram::arrows::arrowhead(arrow, Point::new(0.0, 0.0), vizcraft_diagram::geom::Vec2::new(1.0, 0.0), 1.0) else { return };
    let scale = 0.55_f32;
    for pl in h.path.flatten(0.25) {
        let pts: Vec<egui::Pos2> = pl.points.iter().map(|p| egui::pos2(x1 + p.x as f32 * scale, y + p.y as f32 * scale)).collect();
        let stroke = egui::Stroke::new(1.0, colour);
        if h.filled && pl.closed {
            painter.add(egui::epaint::PathShape::convex_polygon(pts, colour, stroke));
        } else if pl.closed {
            painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
        } else {
            painter.add(egui::epaint::PathShape::line(pts, stroke));
        }
    }
}

/// `ui.insertPicker {x?, y?, query?}`; `None` for any other id.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "ui.insertPicker" {
        return None;
    }
    let num = |k: &str| {
        p.get(k)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(-vizcraft_diagram::geom::MAX_COORD, vizcraft_diagram::geom::MAX_COORD))
    };
    // Without a point it opens at the pointer when that is over the canvas, else at the view.
    let at = match (num("x"), num("y")) {
        (Some(x), Some(y)) => Point::new(x, y),
        _ => app.canvas.pointer_world.unwrap_or_else(|| crate::canvas::insert_point(app)),
    };
    let query = p.get("query").and_then(Value::as_str).unwrap_or("");
    open(app, at, query);
    let rows = filtered(app, query);
    let ids: Vec<&str> = rows.iter().map(|i| i.id.as_str()).collect();
    let detail: Vec<Value> = rows.iter().map(|i| json!({"id": i.id, "kind": i.kind.name(), "label": i.label, "group": i.group})).collect();
    Some(Ok(json!({"open": true, "at": [at.x, at.y], "query": app.insertion.picker.as_ref().map(|p| p.query.clone()), "items": ids, "rows": detail})))
}

/// Show the open picker at its point.
pub fn show(app: &mut CraftApp, ctx: &egui::Context, vp: &Viewport2D) {
    let Some(mut pk) = app.insertion.picker.take() else { return };
    let t = Tokens::get(ctx);
    let (enter, escape, up, down) = ctx.input(|i| {
        (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown))
    });
    ctx.input_mut(|i| {
        for k in [egui::Key::Enter, egui::Key::Escape, egui::Key::ArrowUp, egui::Key::ArrowDown] {
            i.consume_key(egui::Modifiers::NONE, k);
        }
    });
    if escape {
        return;
    }
    if pk.skip_slash > 0 {
        pk.skip_slash -= 1;
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t == "/")));
    }
    let at = vp.world_to_screen(draw::pos(pk.at));
    let hits = filtered(app, &pk.query);
    if hits.is_empty() {
        pk.selected = 0;
    } else {
        if down {
            pk.selected = (pk.selected + 1).min(hits.len() - 1);
        }
        if up {
            pk.selected = pk.selected.saturating_sub(1);
        }
        pk.selected = pk.selected.min(hits.len() - 1);
    }
    let frame = egui::Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(6.0).inner_margin(egui::Margin::same(6));
    let mut chosen: Option<usize> = None;
    let area = egui::Area::new(egui::Id::new("insert_picker")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_width(240.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut pk.query)
                    .hint_text("Insert a shape, relationship or pattern")
                    .desired_width(f32::INFINITY)
                    .font(regular(13.0)),
            );
            if r.changed() {
                pk.selected = 0;
                pk.query = pk.query.chars().take(MAX_QUERY).collect();
            }
            r.request_focus();
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                for (k, it) in hits.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, k == pk.selected, it.label.as_str()));
                    if k == pk.selected {
                        ui.painter().rect_filled(rect, 4.0, t.checked);
                        if up || down {
                            ui.scroll_to_rect(rect, None);
                        }
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 4.0, t.hover);
                    }
                    let icon = egui::Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), vec2(18.0, 14.0));
                    let legal = !matches!(it.kind, ItemKind::Connect { legal: false, .. });
                    let ink = if legal { t.text } else { t.text_dim };
                    match &it.kind {
                        ItemKind::Shape { master } => {
                            if let Some(m) = vizcraft_stencils::master(master) {
                                crate::icons::paint_master(ui.painter(), icon, m, t.icon, t.accent);
                            }
                        }
                        ItemKind::Next { type_id, relationship, .. } => match relationship {
                            Some(r) => paint_relationship(ui.painter(), icon, r, t.icon),
                            None => {
                                if let Some(m) = vizcraft_stencils::master(type_id) {
                                    crate::icons::paint_master(ui.painter(), icon, m, t.icon, t.accent);
                                }
                            }
                        },
                        ItemKind::Connect { relationship, .. } => {
                            paint_relationship(ui.painter(), icon, relationship, if legal { t.icon } else { t.text_dim })
                        }
                        ItemKind::Pattern { .. } => {
                            ui.painter().rect_stroke(icon.shrink2(vec2(3.0, 1.0)), 2.0, egui::Stroke::new(1.0, t.icon), egui::StrokeKind::Inside);
                        }
                    }
                    let g = ui.painter().text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, &it.label, regular(12.5), ink);
                    let group = if legal { it.group.clone() } else { format!("{}, not allowed", it.group) };
                    ui.painter().text(g.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, group, regular(11.0), t.text_dim);
                    if resp.clicked() {
                        chosen = Some(k);
                    }
                }
                if hits.is_empty() {
                    ui.label(egui::RichText::new("Nothing matches.").color(t.text_dim));
                }
            });
        });
    });
    if enter && chosen.is_none() && !hits.is_empty() {
        chosen = Some(pk.selected);
    }
    if let Some(item) = chosen.and_then(|k| hits.get(k)) {
        let at = pk.at;
        insert(app, item, at);
        return;
    }
    // A press anywhere else closes it, as a menu closes.
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
    if !outside {
        app.insertion.picker = Some(pk);
    }
}
