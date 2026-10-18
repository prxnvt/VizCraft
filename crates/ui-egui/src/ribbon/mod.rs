//! The ribbon: a data-driven, Office-style tab strip and ribbon body.
//!
//! The app declares its ribbon as data ([`Tab`], [`Group`], [`Control`]) and [`show`] renders it;
//! no egui code is needed for the common controls. Every control runs a command by id; its
//! label, shortcut, location and enablement come from the registry (or `UI_COMMANDS`), so the
//! ribbon never has behaviour of its own. [`tabs`] returns the tabs in Visio's order: Home,
//! Insert, Design, Data, Process, Review, View, Developer, Help, then the contextual Connector,
//! Container and Callout tabs.
//!
//! Rendering: the tab strip (active underline, hover, double-click collapses, `File` opens the
//! File page through `ui.tab`), the body with groups separated by rules and labelled underneath,
//! horizontal scrolling with edge arrows when the window is narrow, and a collapsed mode where
//! clicking a tab shows the ribbon as a popup over the document that closes on a click outside,
//! on Escape, or once a command has run.
//!
//! Layout: this module holds the shared types, the control drawing dispatch and the helpers the
//! tabs share; [`draw`] draws the strip and the body; each tab is a directory (`home/`,
//! `insert/`, ...) whose `mod.rs` lists its groups, one file per group. Adding a group means
//! adding its file and one line in that tab's `groups!` list. A tab with no groups is hidden.

mod controls;
mod draw;
mod kinds;

/// Declares a tab's group modules and its `groups()` list, left to right. Each tab's `mod.rs`
/// invokes it with one group per line, so adding a group is one file plus one line there.
macro_rules! groups {
    ($($group:ident),* $(,)?) => {
        $(mod $group;)*

        /// The tab's groups, left to right.
        fn groups() -> Vec<crate::ribbon::Group> {
            vec![$($group::group()),*]
        }
    };
}

// One directory per tab; their order in the strip is set by `tabs`.
mod callout;
mod connector;
mod container;
mod data;
mod design;
mod developer;
mod help;
mod home;
mod insert;
mod model;
mod notation;
mod process;
mod review;
mod view;

use egui::{Color32, Ui, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::{Color, Connector, Shape, Style};

use crate::CraftApp;
use crate::widgets::{self, GAP_W, big, big_label, combo, menu_button, menu_item, small, split, stack, toggle};
pub use controls::PALETTE;
use controls::{ColorsSpec, caption, colors, gallery};
pub use draw::{Overflow, popup_open, show, show_tabs};

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

/// One entry of a dropdown menu: a command run by id with params, and optionally how to tell
/// that it is the menu's current choice.
pub struct MenuEntry {
    /// The row's text.
    pub label: &'static str,
    /// The command the row runs.
    pub id: &'static str,
    /// The command's params.
    pub params: Value,
    /// Whether this row is the current choice, read from the app each time the menu is drawn. A
    /// row with `Some` reserves the check column at the right and paints the mark while the
    /// function returns true; `None` is a row with no current value (a plain action).
    pub checked: Option<fn(&CraftApp) -> bool>,
}

impl MenuEntry {
    /// An entry with no current-value state.
    pub const fn new(label: &'static str, id: &'static str, params: Value) -> Self {
        MenuEntry { label, id, params, checked: None }
    }

    /// The same entry, drawn with a check mark while `f` holds.
    pub fn checked(mut self, f: fn(&CraftApp) -> bool) -> Self {
        self.checked = Some(f);
        self
    }
}

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
    /// A tab with no groups never shows.
    pub fn visible(&self, app: &CraftApp) -> bool {
        !self.groups.is_empty() && self.contextual.is_none_or(|f| f(app))
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
                out.extend(menu.iter().map(|e| e.id));
            }
            Control::Menu { menu, .. } => out.extend(menu.iter().map(|e| e.id)),
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
    for e in menu {
        let checked = e.checked.map(|f| f(app));
        menu_item(ui, app, e.label, e.id, e.params.clone(), checked);
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

fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// A number for a combo field: no trailing `.0`.
fn short_num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}

/// Zoom steps offered by the View tab's zoom field.
pub const ZOOM_STEPS: &[u32] = &[50, 75, 100, 125, 150, 200, 300];

/// Font sizes offered by the Home tab's size field.
pub const FONT_SIZES: &[u32] = &[8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];

/// Line weights offered by the Home tab's weight field, in world units (pixels at 100%).
pub const LINE_WEIGHTS: &[f32] = &[0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

/// The tabs, in Visio's order: Home, Insert, Design, Data, Process, Review, View, Developer,
/// Help, then the contextual Connector, Container and Callout tabs (shown while a connector, a
/// container or lane, or a callout is selected). Tabs with no
/// groups yet are left out, so they stay hidden until a group lands.
pub fn tabs(app: &CraftApp) -> Vec<Tab> {
    [
        home::tab(),
        insert::tab(),
        design::tab(),
        data::tab(),
        process::tab(),
        model::tab(),
        review::tab(),
        view::tab(),
        developer::tab(),
        help::tab(),
        connector::tab(),
        container::tab(),
        callout::tab(),
        notation::tab(app),
    ]
    .into_iter()
    .filter(|t| !t.groups.is_empty())
    .collect()
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
