//! The ribbon: a data-driven, Office-style tab strip and ribbon body.
//!
//! The app declares its ribbon as data ([`Tab`], [`Group`], [`Control`]) and [`show`] renders it;
//! no egui code is needed for the common controls. Every control runs a command by id; its
//! label, shortcut, location and enablement come from the registry (or `UI_COMMANDS`), so the
//! ribbon never has behaviour of its own. [`tabs`] returns the tabs: it
//! declares Home, Insert, View, Help and the contextual Connector tab.
//!
//! Rendering: the tab strip (active underline, hover, double-click collapses, `File` opens the
//! File page through `ui.tab`), the body with groups separated by rules and labelled underneath,
//! horizontal scrolling with edge arrows when the window is narrow, and a collapsed mode where
//! clicking a tab shows the ribbon as a popup over the document that closes on a click outside,
//! on Escape, or once a command has run.

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{Color, Connector, Shape, Style};

use crate::theme::{Tokens, medium, regular, semibold};
use crate::widgets::{
    self, ARROW_GLYPH, ARROW_W, BODY_H, BUTTON_RADIUS, CONTENT_H, GAP_W, LABEL_H, SMALL_FONT, SMALL_H, SMALL_ICON, SMALL_ICON_W, SMALL_PAD_X, big,
    big_label, combo, group, menu_button, menu_item, small, split, stack, toggle,
};
use crate::{CraftApp, Tool, icons};

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
    /// The groups, left to right.
    pub groups: Vec<Group>,
    /// A contextual tab shows only while this returns true (Office's "Table Design"); `None`
    /// is an ordinary tab.
    pub contextual: Option<fn(&CraftApp) -> bool>,
}

/// A labelled group of controls, separated from its neighbours by a rule.
pub struct Group {
    /// Shown under the group.
    pub label: &'static str,
    /// The controls, left to right.
    pub controls: Vec<Control>,
}

/// One entry of a dropdown menu: `(label, command id, params)`.
pub type MenuEntry = (&'static str, &'static str, Value);

/// Command params worked out from the app when the control is drawn (the selection, the tool).
pub type ParamsFn = fn(&CraftApp) -> Value;

/// How a [`Control::Button`] is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 32 px icon over the label.
    Big,
    /// 16 px icon beside the label.
    Small,
    /// 16 px icon alone; the label shows as the tooltip.
    Icon,
}

/// A ribbon control. Labels and tooltips come from the command registry by `id`.
pub enum Control {
    /// A big button (32 px icon over the label) running `id`.
    Big { id: &'static str, icon: &'static str },
    /// A small button (16 px icon beside the label) running `id`.
    Small { id: &'static str, icon: &'static str },
    /// A small button drawn checked while `checked(app)` holds; runs `id` (a toggle command).
    Toggle { id: &'static str, icon: &'static str, checked: fn(&CraftApp) -> bool },
    /// A button with its own label and params, for one command offered several ways (`ui.tool`
    /// as Pointer, Connector and Text; `shape.setStyle` as Bold and Italic). Drawn checked while
    /// `checked` holds (ignored for [`Size::Big`]).
    Button { id: &'static str, icon: &'static str, label: &'static str, size: Size, params: ParamsFn, checked: Option<fn(&CraftApp) -> bool> },
    /// A split button: the main part runs `id` with `{}`, the arrow opens `menu`.
    Split { id: &'static str, icon: &'static str, big: bool, menu: Vec<MenuEntry> },
    /// A button that only opens `menu`; `label` names it since it runs no command of its own.
    Menu { label: &'static str, icon: &'static str, big: bool, menu: Vec<MenuEntry> },
    /// An editable field with a dropdown list. Choosing or typing a value runs `id` with
    /// `{param: value}`; a value that parses as a number (a trailing `%`, `px` or `pt` is
    /// dropped) is sent as one. `name` labels the field for tooltips and accessibility.
    Combo {
        id: &'static str,
        param: &'static str,
        name: &'static str,
        width: f32,
        items: fn(&CraftApp) -> Vec<String>,
        current: fn(&CraftApp) -> String,
    },
    /// A colour button: the icon's bar shows `current`, the arrow opens the palette, and a
    /// choice runs `id` with `{param: "#rrggbb"}`. `none` adds a "no colour" entry with that
    /// label (sent as fully transparent).
    Colors {
        id: &'static str,
        param: &'static str,
        icon: &'static str,
        name: &'static str,
        show_label: bool,
        none: Option<&'static str>,
        current: fn(&CraftApp) -> Option<Color32>,
    },
    /// One small button per master of stencil set `set`, `rows` high, each drawing the master's
    /// outline; a click runs `shape.add` at the centre of the visible page area.
    Gallery { set: &'static str, rows: usize },
    /// A caption: a small icon and optional text, not clickable (names the field beside it).
    Label { icon: &'static str, text: &'static str },
    /// A vertical stack of small controls.
    Stack(Vec<Control>),
    /// A horizontal row of small controls (inside a stack).
    Row(Vec<Control>),
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
    /// A group of `controls` labelled `label`.
    pub fn new(label: &'static str, controls: Vec<Control>) -> Self {
        Group { label, controls }
    }
}

impl Control {
    /// Collect every command id this control can run.
    pub fn ids(&self, out: &mut Vec<&'static str>) {
        match self {
            Control::Big { id, .. }
            | Control::Small { id, .. }
            | Control::Toggle { id, .. }
            | Control::Button { id, .. }
            | Control::Combo { id, .. }
            | Control::Colors { id, .. } => out.push(id),
            Control::Gallery { .. } => out.push("shape.add"),
            Control::Split { id, menu, .. } => {
                out.push(id);
                out.extend(menu.iter().map(|(_, id, _)| *id));
            }
            Control::Menu { menu, .. } => out.extend(menu.iter().map(|(_, id, _)| *id)),
            Control::Stack(items) | Control::Row(items) => items.iter().for_each(|c| c.ids(out)),
            Control::Label { .. } | Control::Gap | Control::Custom(_) => {}
        }
    }

    /// Collect every icon name this control draws by name, for tests.
    pub fn icons(&self, out: &mut Vec<&'static str>) {
        match self {
            Control::Big { icon, .. }
            | Control::Small { icon, .. }
            | Control::Toggle { icon, .. }
            | Control::Button { icon, .. }
            | Control::Split { icon, .. }
            | Control::Menu { icon, .. }
            | Control::Colors { icon, .. }
            | Control::Label { icon, .. } => out.push(icon),
            Control::Stack(items) | Control::Row(items) => items.iter().for_each(|c| c.icons(out)),
            Control::Combo { .. } | Control::Gallery { .. } | Control::Gap | Control::Custom(_) => {}
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
            Control::Button { id, icon, label, size, params, checked } => {
                let p = params(app);
                let on = checked.is_some_and(|f| f(app));
                match size {
                    Size::Big => {
                        big(ui, app, icon, &big_label(label), id, p);
                    }
                    Size::Small => {
                        toggle(ui, app, icon, Some(label), label, id, p, on);
                    }
                    Size::Icon => {
                        toggle(ui, app, icon, None, label, id, p, on);
                    }
                }
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
            Control::Combo { id, param, name, width, items, current } => {
                let items = items(app);
                let cur = current(app);
                if let Some(v) = combo(ui, name, *width, &cur, &items) {
                    let _ = app.run(id, json!({ *param: combo_value(&v) }));
                }
            }
            Control::Colors { id, param, icon, name, show_label, none, current } => {
                colors(ui, app, ColorsSpec { id, param, icon, name, show_label: *show_label, none: *none, current: current(app) });
            }
            Control::Gallery { set, rows } => gallery(ui, app, set, *rows),
            Control::Label { icon, text } => caption(ui, icon, text),
            Control::Stack(items) => stack(ui, |ui| {
                for c in items {
                    c.show(app, ui);
                }
            }),
            Control::Row(items) => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
                    for c in items {
                        c.show(app, ui);
                    }
                });
            }
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

/// A combo value as a command parameter: a number when it reads as one (a unit suffix of `%`,
/// `px` or `pt` is dropped), else the text.
pub fn combo_value(text: &str) -> Value {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_suffix('%').or_else(|| trimmed.strip_suffix("px")).or_else(|| trimmed.strip_suffix("pt")).unwrap_or(trimmed).trim();
    match trimmed.parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ => json!(text.trim()),
    }
}

/// The selected ids, primary first.
fn selected_ids(app: &CraftApp) -> impl Iterator<Item = &String> {
    app.session.sel.primary.iter().chain(app.session.sel.items.iter())
}

/// The first selected shape (the primary one when it is a shape).
pub fn selected_shape(app: &CraftApp) -> Option<&Shape> {
    let page = app.session.doc.page()?;
    selected_ids(app).find_map(|id| page.shape(id))
}

/// The first selected connector (the primary one when it is a connector).
pub fn selected_connector(app: &CraftApp) -> Option<&Connector> {
    let page = app.session.doc.page()?;
    selected_ids(app).find_map(|id| page.connector(id))
}

/// The style the Font and Shape Styles controls show: the primary selected item's.
fn selected_style(app: &CraftApp) -> Option<&Style> {
    let page = app.session.doc.page()?;
    selected_ids(app).find_map(|id| page.shape(id).map(|s| &s.style).or_else(|| page.connector(id).map(|c| &c.style)))
}

fn has_connector_selected(app: &CraftApp) -> bool {
    selected_connector(app).is_some()
}

fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// A number for a combo field: no trailing `.0`.
fn short_num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}

/// The new angle for a quarter turn of the selected shape; with nothing selected the engine
/// answers "select a shape first", which the status bar shows.
fn quarter_turn(app: &CraftApp, by: f64) -> Value {
    let angle = selected_shape(app).map(|s| s.angle_deg + by).unwrap_or(0.0);
    json!({ "angle": angle.rem_euclid(360.0) })
}

/// The selected style's flag flipped, so the toggle turns it off when every selected shape
/// shows it and on otherwise. With nothing selected the engine answers "select a shape first".
fn flip(app: &CraftApp, key: &str, get: fn(&Style) -> bool) -> Value {
    json!({ key: !selected_style(app).is_some_and(get) })
}

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

struct ColorsSpec<'a> {
    id: &'a str,
    param: &'a str,
    icon: &'a str,
    name: &'a str,
    show_label: bool,
    none: Option<&'a str>,
    current: Option<Color32>,
}

fn hover_bg(ui: &Ui, r: Rect, resp: &egui::Response, on: bool, t: &Tokens) {
    if on && (resp.hovered() || resp.is_pointer_button_down_on()) {
        let fill = if resp.is_pointer_button_down_on() { t.pressed } else { t.hover };
        ui.painter().rect_filled(r, BUTTON_RADIUS as f32, fill);
    }
}

/// A small colour button: icon with the current colour as its bar, optional label, an arrow, and
/// a palette popup.
fn colors(ui: &mut Ui, app: &mut CraftApp, spec: ColorsSpec<'_>) {
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
fn gallery(ui: &mut Ui, app: &mut CraftApp, set: &str, rows: usize) {
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
fn caption(ui: &mut Ui, icon: &str, text: &str) {
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

/// Zoom steps offered by the View tab's zoom field.
pub const ZOOM_STEPS: &[u32] = &[50, 75, 100, 125, 150, 200, 300];

/// Font sizes offered by the Home tab's size field.
pub const FONT_SIZES: &[u32] = &[8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];

/// Line weights offered by the Home tab's weight field, in world units (pixels at 100%).
pub const LINE_WEIGHTS: &[f32] = &[0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

/// Arrowhead names offered by the Connector tab, as `connector.setArrows` takes them.
pub const ARROWS: &[&str] = &["none", "triangle", "open", "diamond", "circle"];

/// The tabs: Home (Clipboard, Font, Tools, Shape Styles, Arrange, Editing, Undo), Insert
/// (Basic Shapes, Flowchart, Diagram Parts), View (Zoom, Show, Appearance), Help (Help, Settings),
/// plus a contextual Connector tab while a connector is selected.
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
                        Stack(vec![Small { id: "edit.cut", icon: "cut" }, Small { id: "edit.copy", icon: "copy" }]),
                    ],
                ),
                Group::new(
                    "Font",
                    vec![Stack(vec![
                        Combo {
                            id: "shape.setStyle",
                            param: "fontSize",
                            name: "Font Size",
                            width: 72.0,
                            items: |_| FONT_SIZES.iter().map(|s| format!("{s} pt")).collect(),
                            current: |a| selected_style(a).map(|s| format!("{} pt", short_num(s.font_size))).unwrap_or_default(),
                        },
                        Row(vec![
                            Button {
                                id: "shape.setStyle",
                                icon: "bold",
                                label: "Bold",
                                size: Size::Icon,
                                params: |a| flip(a, "bold", |s| s.bold),
                                checked: Some(|a| selected_style(a).is_some_and(|s| s.bold)),
                            },
                            Button {
                                id: "shape.setStyle",
                                icon: "italic",
                                label: "Italic",
                                size: Size::Icon,
                                params: |a| flip(a, "italic", |s| s.italic),
                                checked: Some(|a| selected_style(a).is_some_and(|s| s.italic)),
                            },
                            Colors {
                                id: "shape.setStyle",
                                param: "fontColor",
                                icon: "fontColor",
                                name: "Font Color",
                                show_label: false,
                                none: None,
                                current: |a| selected_style(a).map(|s| color32(s.font_color)),
                            },
                        ]),
                    ])],
                ),
                Group::new(
                    "Tools",
                    vec![Stack(vec![
                        Button {
                            id: "ui.tool",
                            icon: "pointer",
                            label: "Pointer",
                            size: Size::Small,
                            params: |_| json!({"tool": "pointer"}),
                            checked: Some(|a| a.tool == Tool::Pointer),
                        },
                        Button {
                            id: "ui.tool",
                            icon: "connector",
                            label: "Connector",
                            size: Size::Small,
                            params: |_| json!({"tool": "connector"}),
                            checked: Some(|a| a.tool == Tool::Connector),
                        },
                        Button {
                            id: "ui.tool",
                            icon: "text",
                            label: "Text",
                            size: Size::Small,
                            params: |_| json!({"tool": "text"}),
                            checked: Some(|a| a.tool == Tool::Text),
                        },
                    ])],
                ),
                Group::new(
                    "Shape Styles",
                    vec![Stack(vec![
                        Colors {
                            id: "shape.setStyle",
                            param: "fill",
                            icon: "fill",
                            name: "Fill",
                            show_label: true,
                            none: Some("No Fill"),
                            current: |a| selected_shape(a).map(|s| color32(s.style.fill)),
                        },
                        Colors {
                            id: "shape.setStyle",
                            param: "line",
                            icon: "lineColor",
                            name: "Line",
                            show_label: true,
                            none: None,
                            current: |a| selected_style(a).map(|s| color32(s.line)),
                        },
                        Row(vec![
                            Label { icon: "lineWeight", text: "" },
                            Combo {
                                id: "shape.setStyle",
                                param: "lineWidth",
                                name: "Line Weight",
                                width: 66.0,
                                items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                                current: |a| selected_style(a).map(|s| format!("{} px", short_num(s.line_width))).unwrap_or_default(),
                            },
                        ]),
                    ])],
                ),
                Group::new(
                    "Arrange",
                    vec![Stack(vec![
                        Button {
                            id: "shape.rotate",
                            icon: "rotateLeft",
                            label: "Rotate Left",
                            size: Size::Small,
                            params: |a| quarter_turn(a, -90.0),
                            checked: None,
                        },
                        Button {
                            id: "shape.rotate",
                            icon: "rotateRight",
                            label: "Rotate Right",
                            size: Size::Small,
                            params: |a| quarter_turn(a, 90.0),
                            checked: None,
                        },
                        Small { id: "edit.delete", icon: "delete" },
                    ])],
                ),
                Group::new("Editing", vec![Big { id: "edit.selectAll", icon: "selectAll" }]),
                Group::new("Undo", vec![Big { id: "edit.undo", icon: "undo" }, Big { id: "edit.redo", icon: "redo" }]),
            ],
        ),
        Tab::new(
            "Insert",
            vec![
                Group::new("Basic Shapes", vec![Gallery { set: "basic", rows: 3 }]),
                Group::new("Flowchart", vec![Gallery { set: "flowchart", rows: 3 }]),
                Group::new(
                    "Diagram Parts",
                    vec![
                        Button {
                            id: "ui.tool",
                            icon: "connector",
                            label: "Connector",
                            size: Size::Big,
                            params: |_| json!({"tool": "connector"}),
                            checked: None,
                        },
                        Button { id: "ui.tool", icon: "text", label: "Text", size: Size::Big, params: |_| json!({"tool": "text"}), checked: None },
                    ],
                ),
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
                                name: "Zoom Level",
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
            "Connector",
            has_connector_selected,
            vec![
                Group::new(
                    "Route",
                    vec![Stack(vec![
                        Button {
                            id: "connector.setRoute",
                            icon: "routeStraight",
                            label: "Straight",
                            size: Size::Small,
                            params: |_| json!({"route": "straight"}),
                            checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == vizcraft_diagram::Route::Straight)),
                        },
                        Button {
                            id: "connector.setRoute",
                            icon: "routeOrthogonal",
                            label: "Orthogonal",
                            size: Size::Small,
                            params: |_| json!({"route": "orthogonal"}),
                            checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == vizcraft_diagram::Route::Orthogonal)),
                        },
                        Button {
                            id: "connector.setRoute",
                            icon: "routeCurved",
                            label: "Curved",
                            size: Size::Small,
                            params: |_| json!({"route": "curved"}),
                            checked: Some(|a| selected_connector(a).is_some_and(|c| c.route == vizcraft_diagram::Route::Curved)),
                        },
                    ])],
                ),
                Group::new(
                    "Arrows",
                    vec![Stack(vec![
                        Row(vec![
                            Label { icon: "arrowStart", text: "Start" },
                            Combo {
                                id: "connector.setArrows",
                                param: "start",
                                name: "Start Arrow",
                                width: 92.0,
                                items: |_| ARROWS.iter().map(|s| s.to_string()).collect(),
                                current: |a| selected_connector(a).map(|c| c.start_arrow.name().to_string()).unwrap_or_default(),
                            },
                        ]),
                        Row(vec![
                            Label { icon: "arrowEnd", text: "End" },
                            Combo {
                                id: "connector.setArrows",
                                param: "end",
                                name: "End Arrow",
                                width: 92.0,
                                items: |_| ARROWS.iter().map(|s| s.to_string()).collect(),
                                current: |a| selected_connector(a).map(|c| c.end_arrow.name().to_string()).unwrap_or_default(),
                            },
                        ]),
                    ])],
                ),
                Group::new(
                    "Style",
                    vec![Stack(vec![
                        Colors {
                            id: "shape.setStyle",
                            param: "line",
                            icon: "lineColor",
                            name: "Line",
                            show_label: true,
                            none: None,
                            current: |a| selected_connector(a).map(|c| color32(c.style.line)),
                        },
                        Row(vec![
                            Label { icon: "lineWeight", text: "" },
                            Combo {
                                id: "shape.setStyle",
                                param: "lineWidth",
                                name: "Line Weight",
                                width: 66.0,
                                items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                                current: |a| selected_connector(a).map(|c| format!("{} px", short_num(c.style.line_width))).unwrap_or_default(),
                            },
                        ]),
                    ])],
                ),
            ],
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
        assert_eq!(combo_value("1.5 px"), json!(1.5));
        assert_eq!(combo_value("12 pt"), json!(12.0));
        assert_eq!(combo_value("triangle"), json!("triangle"));
    }
}
