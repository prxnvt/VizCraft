//! The pane manager's drawing: each side's stacked panes with headers and splitters, the
//! folded strip of titles, and header drags that reorder panes.

use std::f32::consts::FRAC_PI_2;

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use crate::CraftApp;
use crate::theme::{Tokens, regular, semibold};

use super::{GAP, HEADER_H, MAX_WIDTH, MIN_WIDTH, STRIP_W, Side, mirror_view_flags, spec, sync_view_flags};

/// What the headers asked for this frame; applied after the layout pass.
enum Action {
    ToggleCollapse(String),
    Close(String),
    Move(String, Side),
    Reorder(String, Option<String>),
    FoldSide(Side, bool),
}

/// Draw the side panels. Call after the top and bottom bars, before the central panel.
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    sync_view_flags(app, ui.ctx());
    let mut actions = Vec::new();
    for side in [Side::Left, Side::Right] {
        show_side(app, ui, side, &mut actions);
    }
    for a in actions {
        match a {
            Action::ToggleCollapse(k) => {
                let on = !app.ui.panes.is_collapsed(&k);
                app.ui.panes.set_collapsed(&k, on);
            }
            Action::Close(k) => {
                app.ui.panes.close(&k);
            }
            Action::Move(k, side) => {
                app.ui.panes.move_to(&k, side);
            }
            Action::Reorder(k, before) => app.ui.panes.reorder(&k, before.as_deref()),
            Action::FoldSide(side, on) => app.ui.panes.set_side_collapsed(side, on),
        }
    }
    mirror_view_flags(app);
}

fn show_side(app: &mut CraftApp, ui: &mut Ui, side: Side, actions: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let keys: Vec<String> = app.ui.panes.on(side).iter().filter(|k| spec(k).is_some()).cloned().collect();
    if keys.is_empty() {
        return;
    }
    // The strip is its own panel: egui remembers a panel's size by id, and the strip's width
    // must not come back as the side's width when it unfolds.
    let folded = app.ui.panes.side_collapsed(side);
    let id = match (side, folded) {
        (Side::Left, false) => "left_panes",
        (Side::Left, true) => "left_panes_strip",
        (Side::Right, false) => "right_panes",
        (Side::Right, true) => "right_panes_strip",
    };
    let panel = match side {
        Side::Left => egui::Panel::left(id),
        Side::Right => egui::Panel::right(id),
    };
    if folded {
        let frame = egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border));
        panel.exact_size(STRIP_W).resizable(false).frame(frame).show(ui, |ui| strip(app, ui, side, &keys, actions));
        return;
    }
    let frame = egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(0, 0)).stroke(Stroke::new(1.0, t.border));
    let want = app.ui.panes.width(side);
    // A width changed by `ui.set` or a reset (not by dragging the panel edge) is pushed into
    // the panel; otherwise the panel keeps the size the user dragged it to.
    let width_id = egui::Id::new(("pane-side-width", side.as_str()));
    let last: Option<f32> = ui.ctx().data(|d| d.get_temp(width_id));
    let mut panel = panel.default_size(want).size_range(MIN_WIDTH..=MAX_WIDTH).resizable(true).frame(frame);
    if last.is_some_and(|l| (l - want).abs() > 0.5) {
        panel = panel.exact_size(want);
    }
    let r = panel.show(ui, |ui| column(app, ui, side, &keys, actions));
    let w = r.response.rect.width().clamp(MIN_WIDTH, MAX_WIDTH);
    app.ui.panes.set_width(side, w);
    ui.ctx().data_mut(|d| d.insert_temp(width_id, w));
}

/// A folded side: an unfold button and each pane's title drawn upright, reopening on click.
fn strip(app: &mut CraftApp, ui: &mut Ui, side: Side, keys: &[String], actions: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let area = ui.available_rect_before_wrap();
    let btn = Rect::from_min_size(area.min, vec2(STRIP_W, HEADER_H));
    let resp = ui.interact(btn, ui.id().with(("unfold", side.as_str())), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(btn.shrink(3.0), CornerRadius::same(4), t.hover);
    }
    // The chevron points into the window: the side will grow that way.
    chevron(ui, btn.center(), side.other(), t.icon);
    if resp.on_hover_text("Expand panes").clicked() {
        actions.push(Action::FoldSide(side, false));
    }
    let mut y = btn.bottom() + 6.0;
    for k in keys {
        let Some(sp) = spec(k) else { continue };
        let galley = ui.painter().layout_no_wrap(sp.title.to_string(), regular(12.0), t.text);
        let h = galley.size().x + 16.0;
        let r = Rect::from_min_size(pos2(area.left(), y), vec2(STRIP_W, h));
        let resp = ui.interact(r, ui.id().with(("strip", k.as_str())), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(3.0, 0.0)), CornerRadius::same(4), t.hover);
        }
        // Rotated a quarter turn counter-clockwise, reading bottom to top.
        let pos = pos2(r.center().x - galley.size().y / 2.0, r.bottom() - 8.0);
        ui.painter().add(egui::epaint::TextShape::new(pos, galley, t.text).with_angle(-FRAC_PI_2));
        if resp.on_hover_text(format!("Show {}", sp.title)).clicked() {
            actions.push(Action::FoldSide(side, false));
            app.ui.panes.set_collapsed(k, false);
        }
        y += h + 4.0;
    }
    ui.advance_cursor_after_rect(area);
}

/// An expanded side: the stacked panes with headers and splitters, and a fold button on top.
fn column(app: &mut CraftApp, ui: &mut Ui, side: Side, keys: &[String], actions: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let full = ui.available_rect_before_wrap();
    // A thin bar above the panes carries the fold button at the window edge.
    let bar = Rect::from_min_size(full.min, vec2(full.width(), 18.0));
    let fold = match side {
        Side::Left => Rect::from_min_size(pos2(bar.left() + 2.0, bar.top()), vec2(22.0, 18.0)),
        Side::Right => Rect::from_min_size(pos2(bar.right() - 24.0, bar.top()), vec2(22.0, 18.0)),
    };
    let resp = ui.interact(fold, ui.id().with(("fold", side.as_str())), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(fold.shrink(1.0), CornerRadius::same(4), t.hover);
    }
    chevron(ui, fold.center(), side, if resp.hovered() { t.text } else { t.text_dim });
    if resp.on_hover_text("Collapse panes").clicked() {
        actions.push(Action::FoldSide(side, true));
    }
    let area = Rect::from_min_max(pos2(full.left() + 6.0, bar.bottom()), pos2(full.right() - 6.0, full.bottom() - 6.0));
    let heights = app.ui.panes.heights_for(keys, area.height());
    let rects = rects_after_layout(&heights, area);
    let mut dragging: Option<String> = None;
    for (i, (k, rect)) in keys.iter().zip(&rects).enumerate() {
        let Some(sp) = spec(k) else { continue };
        let collapsed = app.ui.panes.is_collapsed(k);
        let mut child = ui.new_child(UiBuilder::new().id_salt(("pane", k.as_str())).max_rect(*rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        // Card.
        child.painter().rect(*rect, CornerRadius::same(6), t.page, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        let hresp = header(&mut child, app, k, sp.title, head, collapsed, side, actions);
        if hresp.dragged() {
            dragging = Some(k.clone());
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if hresp.drag_stopped()
            && let Some(p) = ui.ctx().pointer_interact_pos()
        {
            actions.push(Action::Reorder(k.clone(), drop_before(keys, &rects, k, p.y)));
        }
        if !collapsed {
            let body = Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max).shrink2(vec2(8.0, 0.0));
            let body = Rect::from_min_max(pos2(body.left(), body.top() + 6.0), pos2(body.right(), body.bottom() - 6.0));
            let mut inner = child.new_child(UiBuilder::new().id_salt("body").max_rect(body));
            inner.set_clip_rect(body.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical().id_salt(("pane-scroll", k.as_str())).max_height(body.height()).auto_shrink([false, false]).show(
                &mut inner,
                |ui| {
                    ui.set_min_width(body.width());
                    (sp.show)(app, ui);
                },
            );
        }
        // Splitter in the gap below this pane: resizes it against the next expanded pane.
        let later_expanded = keys.iter().skip(i + 1).any(|n| !app.ui.panes.is_collapsed(n));
        if !collapsed && later_expanded {
            let gap = Rect::from_min_size(pos2(rect.left(), rect.bottom()), vec2(rect.width(), GAP)).expand2(vec2(0.0, 2.0));
            let sresp = ui.interact(gap, ui.id().with(("pane-splitter", k.as_str())), Sense::drag());
            if sresp.hovered() || sresp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                ui.painter().line_segment([gap.left_center(), gap.right_center()], Stroke::new(2.0, t.accent.gamma_multiply(0.7)));
            }
            if sresp.dragged() {
                app.ui.panes.resize(keys, &heights, i, sresp.drag_delta().y);
            }
        }
    }
    // Drop indicator while a pane is dragged by its header.
    if let (Some(k), Some(p)) = (dragging, ui.ctx().pointer_interact_pos()) {
        let before = drop_before(keys, &rects, &k, p.y);
        let line_y = match before.as_ref().and_then(|b| keys.iter().position(|x| x == b)).and_then(|i| rects.get(i)) {
            Some(r) => r.top() - GAP / 2.0,
            None => rects.last().map_or(area.top(), |r| r.bottom() + GAP / 2.0),
        };
        ui.painter().line_segment([pos2(area.left(), line_y), pos2(area.right(), line_y)], Stroke::new(3.0, t.accent));
    }
    ui.advance_cursor_after_rect(full);
}

/// The pane header: title, collapse toggle, move to the other side, close. Dragging it reorders;
/// a double click collapses. Returns the header's response.
#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut Ui,
    app: &mut CraftApp,
    key: &str,
    title: &str,
    head: Rect,
    collapsed: bool,
    side: Side,
    actions: &mut Vec<Action>,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(head, ui.id().with(("pane-head", key)), Sense::click_and_drag());
    ui.painter().rect_filled(head, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, if resp.hovered() { t.hover } else { t.ribbon });
    if !collapsed {
        ui.painter().line_segment([pos2(head.left() + 1.0, head.bottom()), pos2(head.right() - 1.0, head.bottom())], Stroke::new(1.0, t.border));
    }
    // The title is clipped before the buttons on a narrow side.
    let title_rect = Rect::from_min_max(head.min, pos2(head.right() - 72.0, head.bottom()));
    ui.painter().with_clip_rect(title_rect).text(pos2(head.left() + 10.0, head.center().y), Align2::LEFT_CENTER, title, semibold(12.5), t.text);
    if resp.double_clicked() {
        actions.push(Action::ToggleCollapse(key.to_string()));
    }
    // Buttons from the right edge: close, move, collapse.
    let mut x = head.right() - 4.0;
    let mut button = |ui: &mut Ui, name: &str, tip: &str| -> bool {
        let r = Rect::from_min_size(pos2(x - 20.0, head.center().y - 10.0), vec2(20.0, 20.0));
        x -= 22.0;
        let br = ui.interact(r, ui.id().with(("pane-btn", key, name)), Sense::click());
        if br.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(4), t.pressed);
        }
        let c = if br.hovered() { t.text } else { t.text_dim };
        match name {
            "close" => {
                let s = 3.5;
                let cc = r.center();
                ui.painter().line_segment([cc + vec2(-s, -s), cc + vec2(s, s)], Stroke::new(1.3, c));
                ui.painter().line_segment([cc + vec2(-s, s), cc + vec2(s, -s)], Stroke::new(1.3, c));
            }
            "move" => chevron(ui, r.center(), side.other(), c),
            _ => chevron_v(ui, r.center(), collapsed, c),
        }
        br.on_hover_text(tip).clicked()
    };
    if button(ui, "close", "Close pane") {
        actions.push(Action::Close(key.to_string()));
    }
    let other = match side {
        Side::Left => "Move to the right side",
        Side::Right => "Move to the left side",
    };
    if button(ui, "move", other) {
        actions.push(Action::Move(key.to_string(), side.other()));
    }
    if button(ui, "collapse", if collapsed { "Expand pane" } else { "Collapse pane" }) {
        actions.push(Action::ToggleCollapse(key.to_string()));
    }
    let _ = app;
    resp
}

/// A small chevron pointing left or right.
fn chevron(ui: &Ui, c: egui::Pos2, dir: Side, color: egui::Color32) {
    let dx = match dir {
        Side::Left => -2.5,
        Side::Right => 2.5,
    };
    let pts = [c + vec2(-dx, -4.0), c + vec2(dx, 0.0), c + vec2(-dx, 4.0)];
    ui.painter().add(egui::Shape::line(pts.to_vec(), Stroke::new(1.4, color)));
}

/// A small chevron pointing down (expanded) or up (collapsed, "expand me").
fn chevron_v(ui: &Ui, c: egui::Pos2, collapsed: bool, color: egui::Color32) {
    let dy = if collapsed { -2.5 } else { 2.5 };
    let pts = [c + vec2(-4.0, -dy), c + vec2(0.0, dy), c + vec2(4.0, -dy)];
    ui.painter().add(egui::Shape::line(pts.to_vec(), Stroke::new(1.4, color)));
}

fn rects_after_layout(heights: &[f32], area: Rect) -> Vec<Rect> {
    let mut y = area.top();
    heights
        .iter()
        .map(|h| {
            let r = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), h.max(0.0)));
            y = r.bottom() + GAP;
            r
        })
        .collect()
}

/// The pane the dragged one lands before when released at `y` (`None` = last).
fn drop_before(keys: &[String], rects: &[Rect], dragged: &str, y: f32) -> Option<String> {
    let hit = rects.iter().position(|r| y < r.center().y).and_then(|i| keys.get(i));
    match hit {
        Some(k) if k == dragged => Some(dragged.to_string()),
        Some(k) => {
            let after_self = keys.iter().position(|x| x == dragged).zip(keys.iter().position(|x| x == k)).is_some_and(|(a, b)| b == a + 1);
            if after_self { Some(dragged.to_string()) } else { Some(k.clone()) }
        }
        None => None,
    }
}
