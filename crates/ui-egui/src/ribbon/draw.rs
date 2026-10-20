//! Drawing the ribbon: the tab strip, the body of the active tab (as a panel, or as a popup
//! over the document while collapsed) and the scroll arrows of a narrow window.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use super::{FILE_TAB, SCROLL_ARROW_W, TAB_STRIP_H, Tab, tabs};
use crate::theme::{Tokens, medium, semibold};
use crate::widgets::{self, BODY_H, CONTENT_H, LABEL_H, group};
use crate::{CraftApp, icons};

/// Last frame's body geometry, for the scroll arrows and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Overflow {
    /// Width of all the tab's groups.
    pub content: f32,
    /// Width of the visible body.
    pub visible: f32,
    /// How far the groups are scrolled.
    pub offset: f32,
}

impl Overflow {
    fn id() -> egui::Id {
        egui::Id::new("ribbon_overflow")
    }
    /// The geometry recorded by the last frame.
    pub fn get(ctx: &egui::Context) -> Overflow {
        ctx.data(|d| d.get_temp(Self::id())).unwrap_or_default()
    }
    /// The groups are wider than the body.
    pub fn overflows(&self) -> bool {
        self.content > self.visible + 0.5
    }
}

fn popup_id() -> egui::Id {
    egui::Id::new("ribbon_popup_open")
}

/// Whether the collapsed ribbon is showing as a popup over the document.
pub fn popup_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(popup_id())).unwrap_or(false)
}

fn set_popup(ctx: &egui::Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(popup_id(), open));
}

/// The tab to draw: the chosen one if it is visible, else the first.
fn active_index(app: &CraftApp, tabs: &[Tab]) -> Option<usize> {
    tabs.iter().position(|t| t.name == app.ui.tab && t.visible(app)).or_else(|| tabs.iter().position(|t| t.visible(app)))
}

/// Tab strip plus the ribbon of the current tab (as a panel, or as a popup when collapsed).
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let tabs = tabs(app);
    show_tabs(app, ui, &tabs);
}

/// [`show`] with an explicit tab list (several ribbons, or a test).
pub fn show_tabs(app: &mut CraftApp, ui: &mut Ui, tabs: &[Tab]) {
    let t = Tokens::get(ui.ctx());
    let strip = egui::Panel::top("tabs")
        .exact_size(TAB_STRIP_H)
        .frame(egui::Frame::NONE.fill(t.tab_strip).inner_margin(egui::Margin { left: 8, right: 10, top: 0, bottom: 0 }))
        .show(ui, |ui| tab_strip(app, ui, tabs, &t));
    let strip_rect = strip.response.rect;
    let Some(active) = active_index(app, tabs) else { return };
    let Some(tab) = tabs.get(active) else { return };
    if !app.ui.ribbon_collapsed {
        set_popup(ui.ctx(), false);
        egui::Panel::top("ribbon")
            .exact_size(BODY_H)
            .frame(
                egui::Frame::NONE
                    .fill(t.ribbon)
                    .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                    .stroke(Stroke::new(1.0, t.border)),
            )
            .show(ui, |ui| body(app, ui, tab, &t));
        return;
    }
    if !popup_open(ui.ctx()) {
        return;
    }
    let ctx = ui.ctx().clone();
    let before = signature(app);
    let area = egui::Area::new(egui::Id::new("ribbon_popup")).order(egui::Order::Foreground).fade_in(false).fixed_pos(strip_rect.left_bottom()).show(
        &ctx,
        |ui| {
            ui.set_width(strip_rect.width());
            egui::Frame::NONE
                .fill(t.ribbon)
                .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                .stroke(Stroke::new(1.0, t.border_strong))
                .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: t.shadow })
                .show(ui, |ui| {
                    ui.set_min_height(CONTENT_H + LABEL_H);
                    ui.set_width(strip_rect.width() - 16.0);
                    body(app, ui, tab, &t);
                });
        },
    );
    // Microsoft Office closes the popup once a command has run, on a click elsewhere, and on
    // Escape.
    let ran = signature(app) != before;
    let menu_open = egui::Popup::is_any_open(&ctx);
    let clicked_outside = ctx
        .input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !strip_rect.contains(p)));
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if (ran || clicked_outside || escape) && !menu_open {
        set_popup(&ctx, false);
    }
}

/// What a command visibly changes, to notice one ran inside the popup.
fn signature(app: &CraftApp) -> (u64, String, bool, bool, Option<String>) {
    (
        app.session.rev(),
        serde_json::to_string(&app.session.view).unwrap_or_default(),
        app.ui.ribbon_collapsed,
        app.backstage_open(),
        app.dialog.as_ref().map(|d| d.name().to_string()),
    )
}

fn tab_strip(app: &mut CraftApp, ui: &mut Ui, tabs: &[Tab], t: &Tokens) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
        let active = active_index(app, tabs);
        let names: Vec<(&str, bool, bool)> = std::iter::once((FILE_TAB, false, false))
            .chain(
                tabs.iter().enumerate().filter(|(_, tab)| tab.visible(app)).map(|(i, tab)| (tab.name, active == Some(i), tab.contextual.is_some())),
            )
            .collect();
        for (name, is_active, contextual) in names {
            let is_file = name == FILE_TAB;
            let w = ui.ctx().fonts_mut(|f| f.layout_no_wrap(name.to_string(), medium(12.5), t.text).size().x) + 18.0;
            let (r, resp) = ui.allocate_exact_size(vec2(w, TAB_STRIP_H), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, is_active, format!("{name} tab")));
            if resp.hovered() && !is_active {
                ui.painter().rect_filled(r.shrink2(vec2(0.0, 4.0)), 4.0, t.hover);
            }
            let color = if is_active || is_file || contextual { t.accent_text } else { t.text };
            ui.painter().text(r.center(), Align2::CENTER_CENTER, name, if is_active { semibold(12.5) } else { medium(12.5) }, color);
            if is_active {
                let u = Rect::from_center_size(pos2(r.center().x, r.max.y - 2.0), vec2(w - 16.0, 3.0));
                ui.painter().rect_filled(u, 2.0, t.accent);
            }
            if resp.double_clicked() && !is_file {
                let _ = app.run("ui.collapseRibbon", json!({}));
                set_popup(ui.ctx(), false);
            } else if resp.clicked() {
                let _ = app.run("ui.tab", json!({"tab": name}));
                if !is_file && app.ui.ribbon_collapsed {
                    // A collapsed ribbon shows the tab as a popup; clicking the open tab hides it.
                    set_popup(ui.ctx(), !(popup_open(ui.ctx()) && is_active));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tip = if app.ui.ribbon_collapsed { "Show the ribbon" } else { "Collapse the ribbon" };
            let icon = if app.ui.ribbon_collapsed { "chevronDown" } else { "chevronUp" };
            if widgets::icon_button(ui, icon, tip, 14.0, true) {
                let _ = app.run("ui.collapseRibbon", json!({}));
                set_popup(ui.ctx(), false);
            }
        });
    });
}

/// The groups of one tab in a horizontal scroll area, with edge arrows when they overflow.
fn body(app: &mut CraftApp, ui: &mut Ui, tab: &Tab, t: &Tokens) {
    let scroll = egui::ScrollArea::horizontal()
        .id_salt("ribbon_groups")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                let n = tab.groups.len();
                for (i, g) in tab.groups.iter().enumerate() {
                    group(ui, g.label, i + 1 < n, app, |ui, app| {
                        for c in &g.controls {
                            c.show(app, ui);
                        }
                    });
                }
            });
        });
    let inner = scroll.inner_rect;
    let max = (scroll.content_size.x - inner.width()).max(0.0);
    let overflow = Overflow { content: scroll.content_size.x, visible: inner.width(), offset: scroll.state.offset.x };
    ui.ctx().data_mut(|d| d.insert_temp(Overflow::id(), overflow));
    if !overflow.overflows() {
        return;
    }
    let step = (inner.width() * 0.6).max(60.0);
    let mut target = None;
    for (left, shown) in [(true, overflow.offset > 0.5), (false, overflow.offset < max - 0.5)] {
        if !shown {
            continue;
        }
        let r = if left {
            Rect::from_min_size(inner.left_top(), vec2(SCROLL_ARROW_W, inner.height()))
        } else {
            Rect::from_min_size(pos2(inner.right() - SCROLL_ARROW_W, inner.top()), vec2(SCROLL_ARROW_W, inner.height()))
        };
        let resp = ui.interact(r, ui.id().with(("ribbon_scroll", left)), Sense::click());
        let name = if left { "Scroll ribbon left" } else { "Scroll ribbon right" };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        ui.painter().rect_filled(r, 4.0, if resp.hovered() { t.hover } else { t.ribbon });
        ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        icons::paint(
            ui.painter(),
            Rect::from_center_size(r.center(), vec2(14.0, 14.0)),
            if left { "chevronLeft" } else { "chevronRight" },
            t.icon,
            t.accent,
        );
        if resp.on_hover_text(name).clicked() {
            target = Some(if left { overflow.offset - step } else { overflow.offset + step });
        }
    }
    if let Some(x) = target {
        let mut state = scroll.state;
        state.offset.x = x.clamp(0.0, max);
        state.store(ui.ctx(), scroll.id);
        ui.ctx().request_repaint();
    }
}
