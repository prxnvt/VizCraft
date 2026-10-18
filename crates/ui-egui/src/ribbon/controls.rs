//! The ribbon's own drawn controls: colour buttons with a palette, stencil galleries and
//! captions. The plain buttons, splits, menus and combos live in [`crate::widgets`].

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::json;

use crate::theme::{Tokens, regular};
use crate::widgets::{self, ARROW_GLYPH, ARROW_W, BUTTON_RADIUS, SMALL_FONT, SMALL_H, SMALL_ICON, SMALL_ICON_W, SMALL_PAD_X};
use crate::{CraftApp, icons};

/// Our own palette for the colour buttons: neutrals, strong colours, and their light tints.
pub const PALETTE: [[u32; 8]; 3] = [
    [0xffffff, 0xeef0f3, 0xd5dae0, 0xa9b1bb, 0x76808c, 0x4a5562, 0x2b333d, 0x111418],
    [0x2f7d6d, 0x2f62a3, 0x5a4b8c, 0xa8436b, 0xc4452f, 0xd27a1f, 0xc9a227, 0x4f8a3a],
    [0xd9ece7, 0xdde8f6, 0xe7e2f3, 0xf4dde6, 0xf8ded9, 0xfbe8d3, 0xf8f0d2, 0xe1efdb],
];

/// Size of one palette swatch.
const SWATCH: f32 = 18.0;
/// Narrowest caption text, so stacked fields line up.
const CAPTION_MIN_W: f32 = 30.0;
/// Size of one gallery cell.
const CELL: egui::Vec2 = egui::Vec2::new(26.0, 22.0);

pub(super) struct ColorsSpec<'a> {
    pub(super) id: &'a str,
    pub(super) param: &'a str,
    pub(super) icon: &'a str,
    pub(super) name: &'a str,
    pub(super) show_label: bool,
    pub(super) none: Option<&'a str>,
    pub(super) current: Option<Color32>,
}

fn hover_bg(ui: &Ui, r: Rect, resp: &egui::Response, on: bool, t: &Tokens) {
    if on && (resp.hovered() || resp.is_pointer_button_down_on()) {
        let fill = if resp.is_pointer_button_down_on() { t.pressed } else { t.hover };
        ui.painter().rect_filled(r, BUTTON_RADIUS as f32, fill);
    }
}

/// A small colour button: icon with the current colour as its bar, optional label, an arrow, and
/// a palette popup.
pub(super) fn colors(ui: &mut Ui, app: &mut CraftApp, spec: ColorsSpec<'_>) {
    let t = Tokens::get(ui.ctx());
    let on = widgets::enabled(app, spec.id);
    let label_w = if spec.show_label {
        ui.ctx().fonts_mut(|f| f.layout_no_wrap(spec.name.to_string(), regular(SMALL_FONT), t.text).size().x) + SMALL_PAD_X
    } else {
        0.0
    };
    let (r, resp) = ui.allocate_exact_size(vec2(SMALL_ICON_W + label_w + ARROW_W, SMALL_H), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, spec.name));
    hover_bg(ui, r, &resp, on, &t);
    let (ic, tc) = if on { (t.icon, t.text) } else { (t.text_disabled, t.text_disabled) };
    // With nothing selected the bar shows the accent, as Office shows its last-used colour.
    let bar = if on { spec.current.unwrap_or(t.accent) } else { t.text_disabled };
    let icon_r = Rect::from_center_size(pos2(r.min.x + SMALL_ICON_W / 2.0, r.center().y), vec2(SMALL_ICON, SMALL_ICON));
    icons::paint(ui.painter(), icon_r, spec.icon, ic, bar);
    // The colour icons draw their bar at (3, 15.5)..(17, 18.5) on the 20 unit grid. A thin rim
    // keeps a white fill visible on the light ribbon and a dark line visible on the dark one.
    let u = SMALL_ICON / icons::GRID;
    let bar_r = Rect::from_min_max(icon_r.min + vec2(3.0 * u, 15.5 * u), icon_r.min + vec2(17.0 * u, 18.5 * u));
    ui.painter().rect_stroke(bar_r, 0.5, Stroke::new(0.5, t.border_strong), egui::StrokeKind::Outside);
    if spec.show_label {
        ui.painter().text(pos2(r.min.x + SMALL_ICON_W, r.center().y), Align2::LEFT_CENTER, spec.name, regular(SMALL_FONT), tc);
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(r.max.x - ARROW_W / 2.0, r.center().y), vec2(ARROW_GLYPH, ARROW_GLYPH)),
        "dropdown",
        ic,
        ic,
    );
    let resp = widgets::tooltip(app, resp, spec.name, spec.id);
    if !on {
        return;
    }
    egui::Popup::menu(&resp).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        let mut chosen = None;
        for row in PALETTE {
            ui.horizontal(|ui| {
                for rgb in row {
                    let hex = format!("#{rgb:06x}");
                    let c = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                    let (sr, sresp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
                    let label = format!("{} {hex}", spec.name);
                    sresp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));
                    ui.painter().rect_filled(sr, 2.0, c);
                    let border = if sresp.hovered() { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border_strong) };
                    ui.painter().rect_stroke(sr, 2.0, border, egui::StrokeKind::Inside);
                    if sresp.on_hover_text(&hex).clicked() {
                        chosen = Some(hex);
                    }
                }
            });
        }
        if let Some(none) = spec.none
            && ui.button(none).clicked()
        {
            chosen = Some("#00000000".to_string());
        }
        if let Some(hex) = chosen {
            let _ = app.run(spec.id, json!({ spec.param: hex }));
            ui.close();
        }
    });
}

/// A grid of small buttons, one per master of the stencil set, `rows` high.
pub(super) fn gallery(ui: &mut Ui, app: &mut CraftApp, set: &str, rows: usize) {
    let Some(set) = vizcraft_stencils::set(set) else { return };
    let t = Tokens::get(ui.ctx());
    let rows = rows.max(1);
    let cols = set.masters.len().div_ceil(rows).max(1);
    let on = widgets::enabled(app, "shape.add");
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for row in set.masters.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for m in row {
                    let (r, resp) = ui.allocate_exact_size(CELL, Sense::click());
                    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, m.label));
                    hover_bg(ui, r, &resp, on, &t);
                    let (c, a) = if on { (t.icon, t.accent) } else { (t.text_disabled, t.text_disabled) };
                    icons::paint_kind(
                        ui.painter(),
                        Rect::from_center_size(r.center(), vec2(SMALL_ICON, SMALL_ICON)),
                        m.kind,
                        m.w as f32,
                        m.h as f32,
                        c,
                        a,
                    );
                    let resp = widgets::tooltip(app, resp, m.label, "shape.add");
                    if resp.clicked() && on {
                        // Same placement as a Shapes pane click: the visible middle of the page.
                        let at = crate::canvas::insert_point(app);
                        crate::canvas::insert_master(app, m.id, at);
                    }
                }
            });
        }
    });
}

/// A small icon and caption that name the field next to them.
pub(super) fn caption(ui: &mut Ui, icon: &str, text: &str) {
    let t = Tokens::get(ui.ctx());
    // A minimum width lines up the fields of a stack whose captions differ in length (Start, End).
    let text_w = if text.is_empty() {
        0.0
    } else {
        ui.ctx().fonts_mut(|f| f.layout_no_wrap(text.to_string(), regular(SMALL_FONT), t.text).size().x).max(CAPTION_MIN_W) + SMALL_PAD_X
    };
    let (r, _) = ui.allocate_exact_size(vec2(SMALL_ICON_W + text_w, SMALL_H), Sense::hover());
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(r.min.x + SMALL_ICON_W / 2.0, r.center().y), vec2(SMALL_ICON, SMALL_ICON)),
        icon,
        t.icon,
        t.accent,
    );
    if !text.is_empty() {
        ui.painter().text(pos2(r.min.x + SMALL_ICON_W, r.center().y), Align2::LEFT_CENTER, text, regular(SMALL_FONT), t.text_dim);
    }
}
