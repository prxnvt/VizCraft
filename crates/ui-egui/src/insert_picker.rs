//! The insert picker: a type-to-filter list at a canvas point of the page's palette types and the
//! Drawing primitives. A double-click on empty canvas opens it there, `ui.insertPicker` opens it
//! by id; arrows pick, Enter inserts at the point (through `shape.add`, as a Shapes pane drop
//! does), Escape closes.

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
}

/// One row: an element type or a stencil, and the master that draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Element type id or stencil id.
    pub id: String,
    /// The master inserted.
    pub master: String,
    /// Label shown.
    pub label: String,
    /// Group shown dimmer next to the label.
    pub group: String,
}

/// Every row before filtering: the palette types of the page's notation (its viewpoint's pack,
/// else the packs in use, else every pack), then the Drawing primitives of the Basic pack.
pub fn items(app: &CraftApp) -> Vec<Item> {
    let cat = vizcraft_notation::catalog();
    let doc = &app.session.doc;
    let viewpoint_pack =
        doc.page().and_then(|p| p.view.as_ref()).and_then(|v| v.viewpoint.split_once('.').map(|(p, _)| p.to_string())).filter(|p| !p.is_empty());
    let types: Vec<String> = match viewpoint_pack.as_deref().and_then(|p| cat.pack(p)) {
        Some(pack) => pack.palette.iter().flat_map(|g| g.types.iter().cloned()).collect(),
        None => vizcraft_engine::suggest::types_in_use(doc),
    };
    let mut out: Vec<Item> = Vec::new();
    for t in types {
        let (Some((pack, et)), Some(master)) = (cat.element_type(&t), cat.master_for_type(&t)) else { continue };
        if !out.iter().any(|i| i.id == t) {
            out.push(Item { label: et.label.clone(), group: pack.name.clone(), master: master.to_string(), id: t });
        }
    }
    let primitives = cat.pack("basic").map(|p| p.palette.iter().flat_map(|g| g.glyphs.iter().cloned()).collect::<Vec<_>>()).unwrap_or_default();
    for g in primitives {
        if let Some(m) = vizcraft_stencils::master(&g)
            && !out.iter().any(|i| i.id == g)
        {
            out.push(Item { id: g.clone(), master: g, label: m.label.to_string(), group: "Drawing".into() });
        }
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
    app.insertion.picker = Some(Picker { at, query, selected: 0 });
}

/// Insert `item` centred on `at` and close the picker.
fn insert(app: &mut CraftApp, item: &Item, at: Point) {
    app.insertion.picker = None;
    crate::canvas::insert_master(app, &item.master, at);
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
    let at = match (num("x"), num("y")) {
        (Some(x), Some(y)) => Point::new(x, y),
        _ => crate::canvas::insert_point(app),
    };
    let query = p.get("query").and_then(Value::as_str).unwrap_or("");
    open(app, at, query);
    let ids: Vec<String> = filtered(app, query).into_iter().map(|i| i.id).collect();
    Some(Ok(json!({"open": true, "at": [at.x, at.y], "query": app.insertion.picker.as_ref().map(|p| p.query.clone()), "items": ids})))
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
            let r = ui.add(egui::TextEdit::singleline(&mut pk.query).hint_text("Insert a shape").desired_width(f32::INFINITY).font(regular(13.0)));
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
                    if let Some(m) = vizcraft_stencils::master(&it.master) {
                        crate::icons::paint_kind(ui.painter(), icon, m.kind, m.w as f32, m.h as f32, t.icon, t.accent);
                    }
                    let g = ui.painter().text(rect.left_center() + vec2(30.0, 0.0), Align2::LEFT_CENTER, &it.label, regular(12.5), t.text);
                    ui.painter().text(g.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, &it.group, regular(11.0), t.text_dim);
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
