//! The ribbon: a data-driven, Office-style tab strip and ribbon body.
//!
//! The app declares its ribbon as data ([`Tab`], [`Group`], [`Control`]) and [`show`] renders it;
//! no egui code is needed for the common controls. Every control runs a command by id; its
//! label, shortcut, location and enablement come from the registry (or `UI_COMMANDS`), so the
//! ribbon never has behaviour of its own. [`tabs`] returns the default tabs; the diagram tabs
//! will replace the function body (or filter and extend its result).
//!
//! Rendering: the tab strip (active underline, hover, double-click collapses, `File` opens the
//! File page through `ui.tab`), the body with groups separated by rules and labelled underneath,
//! horizontal scrolling with edge arrows when the window is narrow, and a collapsed mode where
//! clicking a tab shows the ribbon as a popup over the document that closes on a click outside,
//! on Escape, or once a command has run.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::theme::{Tokens, medium, semibold};
use crate::widgets::{self, BODY_H, CONTENT_H, GAP_W, LABEL_H, big, big_label, combo, group, menu_button, menu_item, small, split, stack, toggle};
use crate::{CraftApp, icons};

/// The tab that opens the File page instead of a ribbon. Always first in the strip.
pub const FILE_TAB: &str = "File";

/// Height of the tab strip.
pub const TAB_STRIP_H: f32 = 30.0;

/// Width of the scroll arrows shown at the body's edges when the groups do not fit.
pub const SCROLL_ARROW_W: f32 = 18.0;

/// One ribbon tab.
pub struct Tab {
    /// Shown in the strip and matched against `UiState::tab` (`ui.tab {"tab": name}`).
    pub name: &'static str,
    pub groups: Vec<Group>,
    /// A contextual tab shows only while this returns true (Office's "Table Design"); `None`
    /// is an ordinary tab.
    pub contextual: Option<fn(&CraftApp) -> bool>,
}

/// A labelled group of controls, separated from its neighbours by a rule.
pub struct Group {
    pub label: &'static str,
    pub controls: Vec<Control>,
}

/// One entry of a dropdown menu: `(label, command id, params)`.
pub type MenuEntry = (&'static str, &'static str, Value);

/// A ribbon control. Labels and tooltips come from the command registry by `id`.
pub enum Control {
    /// A big button (32 px icon over the label) running `id`.
    Big { id: &'static str, icon: &'static str },
    /// A small button (16 px icon beside the label) running `id`.
    Small { id: &'static str, icon: &'static str },
    /// A small button drawn checked while `checked(app)` holds; runs `id` (a toggle command).
    Toggle { id: &'static str, icon: &'static str, checked: fn(&CraftApp) -> bool },
    /// A split button: the main part runs `id` with `{}`, the arrow opens `menu`.
    Split { id: &'static str, icon: &'static str, big: bool, menu: Vec<MenuEntry> },
    /// A button that only opens `menu`; `label` names it since it runs no command of its own.
    Menu { label: &'static str, icon: &'static str, big: bool, menu: Vec<MenuEntry> },
    /// An editable field with a dropdown list. Choosing or typing a value runs `id` with
    /// `{param: value}`; a value that parses as a number (a trailing `%` is dropped) is sent as
    /// one.
    Combo { id: &'static str, param: &'static str, width: f32, items: fn(&CraftApp) -> Vec<String>, current: fn(&CraftApp) -> String },
    /// A vertical stack of small controls.
    Stack(Vec<Control>),
    /// A little horizontal space.
    Gap,
    /// Anything else, drawn by the app.
    Custom(fn(&mut CraftApp, &mut Ui)),
}

impl Tab {
    /// An ordinary tab.
    pub fn new(name: &'static str, groups: Vec<Group>) -> Self {
        Tab { name, groups, contextual: None }
    }
    /// A contextual tab, shown while `when` holds.
    pub fn contextual(name: &'static str, when: fn(&CraftApp) -> bool, groups: Vec<Group>) -> Self {
        Tab { name, groups, contextual: Some(when) }
    }
    /// Whether the tab is in the strip right now.
    pub fn visible(&self, app: &CraftApp) -> bool {
        self.contextual.is_none_or(|f| f(app))
    }
    /// Every command id the tab's controls run (menu entries included), for tests and parity.
    pub fn ids(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for g in &self.groups {
            for c in &g.controls {
                c.ids(&mut out);
            }
        }
        out
    }
}

impl Group {
    pub fn new(label: &'static str, controls: Vec<Control>) -> Self {
        Group { label, controls }
    }
}

impl Control {
    /// Collect every command id this control can run.
    pub fn ids(&self, out: &mut Vec<&'static str>) {
        match self {
            Control::Big { id, .. } | Control::Small { id, .. } | Control::Toggle { id, .. } | Control::Combo { id, .. } => out.push(id),
            Control::Split { id, menu, .. } => {
                out.push(id);
                out.extend(menu.iter().map(|(_, id, _)| *id));
            }
            Control::Menu { menu, .. } => out.extend(menu.iter().map(|(_, id, _)| *id)),
            Control::Stack(items) => items.iter().for_each(|c| c.ids(out)),
            Control::Gap | Control::Custom(_) => {}
        }
    }

    /// Draw the control, running commands through `app`.
    pub fn show(&self, app: &mut CraftApp, ui: &mut Ui) {
        match self {
            Control::Big { id, icon } => {
                big(ui, app, icon, &big_label(widgets::label_of(app, id)), id, json!({}));
            }
            Control::Small { id, icon } => {
                let label = widgets::label_of(app, id);
                small(ui, app, icon, Some(label), label, id, json!({}));
            }
            Control::Toggle { id, icon, checked } => {
                let label = widgets::label_of(app, id);
                let on = checked(app);
                toggle(ui, app, icon, Some(label), label, id, json!({}), on);
            }
            Control::Split { id, icon, big: big_btn, menu } => {
                let label = widgets::label_of(app, id);
                let label = if *big_btn { big_label(label) } else { label.to_string() };
                split(ui, app, icon, &label, id, json!({}), *big_btn, |ui, app| menu_entries(ui, app, menu));
            }
            Control::Menu { label, icon, big: big_btn, menu } => {
                let text = if *big_btn { big_label(label) } else { label.to_string() };
                menu_button(ui, app, icon, &text, label, *big_btn, |ui, app| menu_entries(ui, app, menu));
            }
            Control::Combo { id, param, width, items, current } => {
                let items = items(app);
                let cur = current(app);
                if let Some(v) = combo(ui, id, *width, &cur, &items) {
                    let _ = app.run(id, json!({ *param: combo_value(&v) }));
                }
            }
            Control::Stack(items) => stack(ui, |ui| {
                for c in items {
                    c.show(app, ui);
                }
            }),
            Control::Gap => ui.add_space(GAP_W),
            Control::Custom(f) => f(app, ui),
        }
    }
}

fn menu_entries(ui: &mut Ui, app: &mut CraftApp, menu: &[MenuEntry]) {
    for (label, id, params) in menu {
        menu_item(ui, app, label, id, params.clone());
    }
}

/// A combo value as a command parameter: a number when it reads as one, else the text.
pub fn combo_value(text: &str) -> Value {
    let trimmed = text.trim().trim_end_matches('%').trim();
    match trimmed.parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ => json!(text.trim()),
    }
}

fn has_selection(app: &CraftApp) -> bool {
    !app.session.sel.is_empty()
}

/// Zoom steps offered by the View tab's zoom field.
pub const ZOOM_STEPS: &[u32] = &[50, 75, 100, 125, 150, 200, 300];

/// The default tabs: Home (Clipboard, Undo), View (Zoom, Show, Appearance), Help (Help,
/// Settings), plus a contextual Selection tab while text or elements are selected.
/// A craft edits this function to declare its own ribbon.
pub fn tabs(_app: &CraftApp) -> Vec<Tab> {
    use Control::*;
    let mut appearance = vec![Toggle { id: "view.dark", icon: "dark", checked: |a| a.session.view.dark_mode }];
    if cfg!(target_os = "macos") {
        appearance.push(Toggle { id: "ui.inWindowMenus", icon: "menuBar", checked: |a| a.ui.in_window_menus });
    }
    vec![
        Tab::new(
            "Home",
            vec![
                Group::new(
                    "Clipboard",
                    vec![
                        Big { id: "edit.paste", icon: "paste" },
                        Stack(vec![
                            Small { id: "edit.cut", icon: "cut" },
                            Small { id: "edit.copy", icon: "copy" },
                            Small { id: "edit.selectAll", icon: "selectAll" },
                        ]),
                    ],
                ),
                Group::new("Undo", vec![Big { id: "edit.undo", icon: "undo" }, Big { id: "edit.redo", icon: "redo" }]),
            ],
        ),
        Tab::new(
            "View",
            vec![
                Group::new(
                    "Zoom",
                    vec![
                        Big { id: "view.zoom", icon: "zoom" },
                        Big { id: "view.zoomIn", icon: "zoomIn" },
                        Big { id: "view.zoomOut", icon: "zoomOut" },
                        Stack(vec![
                            Combo {
                                id: "view.zoom",
                                param: "value",
                                width: 76.0,
                                items: |_| ZOOM_STEPS.iter().map(|z| format!("{z}%")).collect(),
                                current: |a| format!("{}%", (a.session.view.zoom * 100.0).round()),
                            },
                            Small { id: "view.zoom100", icon: "zoomFit" },
                        ]),
                    ],
                ),
                Group::new(
                    "Show",
                    vec![Stack(vec![
                        Toggle { id: "view.navigationPane", icon: "panelLeft", checked: |a| a.session.view.nav_pane },
                        Toggle { id: "view.propertiesPane", icon: "panelRight", checked: |a| a.session.view.props_pane },
                        Toggle { id: "ui.collapseRibbon", icon: "ribbon", checked: |a| a.ui.ribbon_collapsed },
                    ])],
                ),
                Group::new("Appearance", vec![Stack(appearance)]),
            ],
        ),
        Tab::new(
            "Help",
            vec![
                Group::new("Help", vec![Big { id: "ui.commands", icon: "commands" }, Big { id: "help.about", icon: "info" }]),
                Group::new("Settings", vec![Big { id: "app.preferences", icon: "settings" }]),
            ],
        ),
        Tab::contextual(
            "Selection",
            has_selection,
            vec![Group::new(
                "Selection",
                vec![Big { id: "edit.cut", icon: "cut" }, Big { id: "edit.copy", icon: "copy" }, Big { id: "edit.selectAll", icon: "selectAll" }],
            )],
        ),
    ]
}

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
    // Office closes the popup once a command has run, on a click elsewhere, and on Escape.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_values_parse_numbers_and_keep_text() {
        assert_eq!(combo_value("150%"), json!(150.0));
        assert_eq!(combo_value(" 75 "), json!(75.0));
        assert_eq!(combo_value("fit"), json!("fit"));
        assert_eq!(combo_value("NaN"), json!("NaN"));
    }
}
