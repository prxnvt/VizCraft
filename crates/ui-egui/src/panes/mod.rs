//! Side panes and the pane manager.
//!
//! A pane is a [`PaneSpec`] in [`panes`]: a key, a title, the side it starts on, a default width
//! and a body function. The manager ([`show`]) draws the open panes of each side stacked
//! vertically with draggable splitters (fixed heights, the last expanded pane fills the rest),
//! gives each a header (title, collapse, move to the other side, close, drag to reorder), and
//! can fold a whole side to a thin strip of titles that reopen on click.
//!
//! Where a pane is lives in [`PaneLayout`], persisted in `UiState.panes`, and is driven by the
//! `ui.pane*` commands ([`ui_command`]).
//!
//! Source of truth for pane visibility: `PaneLayout`. The engine's `view.navigationPane` and
//! `view.propertiesPane` flags (`ViewState.nav_pane` / `props_pane`), which the ribbon's View tab
//! and the View menu toggle, are mirrors: every pane command writes the layout and then copies
//! the open state into those flags ([`mirror_view_flags`]); every frame [`show`] checks whether
//! an engine command flipped a flag since the last mirror and applies that to the layout. A
//! loaded layout therefore wins over the flags' defaults at startup, and either path (`ui.pane`
//! or `view.navigationPane`) ends in the same state.
//!
//! Layout: this module holds [`PaneSpec`], the pane list and the `ui.pane*` commands; [`layout`]
//! holds [`PaneLayout`]; `manager` draws the sides; each pane is one file exposing its `SPEC`.
//! Adding a pane means adding its file, one line in the `default_panes!` list and its
//! `ui.pane.<key>` row in `UI_COMMANDS`.

mod layout;
mod manager;

/// Declares the default panes' modules and [`panes`]'s default list, in Window menu order. Each
/// pane module has a `SPEC` constant.
macro_rules! default_panes {
    ($($pane:ident),* $(,)?) => {
        $(mod $pane;)*

        /// The built-in panes, in Window menu order.
        fn default_panes() -> Vec<PaneSpec> {
            vec![$($pane::SPEC),*]
        }
    };
}

default_panes! {
    shapes,
    model_tree,
    history,
    layers,
    inspector,
    comments,
    pan_zoom,
    document_stencil,
    external_data,
    patterns,
}

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CraftApp;
pub use inspector::{InspectorMode, InspectorState, InspectorTab};
pub use layout::{DEFAULT_HEIGHT, GAP, HEADER_H, MAX_HEIGHT, MAX_WIDTH, MIN_HEIGHT, MIN_WIDTH, PaneLayout, STRIP_W};
pub use manager::show;
pub use model_tree::{TreeState, inspect as tree_inspect};
pub use patterns::{PatternsState, inspect as patterns_inspect};
pub use shapes::MasterDrag;
use shapes::{palette_command, shapes_search};

/// Which side of the window a pane sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// `left` or `right`.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
    /// The opposite side.
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
    /// Parse `left` / `right` (any case); anything else is `None`.
    pub fn parse(s: &str) -> Option<Side> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Some(Side::Left),
            "right" | "r" => Some(Side::Right),
            _ => None,
        }
    }
}

/// Panes registered with [`set_panes`]; `None` means the default set.
#[cfg(not(test))]
static CUSTOM_PANES: std::sync::RwLock<Option<Vec<PaneSpec>>> = std::sync::RwLock::new(None);

// Unit tests run in parallel threads of one process; a process-wide registry would let the test
// that registers custom panes leak them into every other test running at the same moment.
#[cfg(test)]
thread_local! {
    static CUSTOM_PANES: std::cell::RefCell<Option<Vec<PaneSpec>>> = const { std::cell::RefCell::new(None) };
}

fn store_custom_panes(specs: Option<Vec<PaneSpec>>) {
    #[cfg(not(test))]
    if let Ok(mut lock) = CUSTOM_PANES.write() {
        *lock = specs;
    }
    #[cfg(test)]
    CUSTOM_PANES.with(|c| *c.borrow_mut() = specs);
}

fn custom_panes() -> Option<Vec<PaneSpec>> {
    #[cfg(not(test))]
    return CUSTOM_PANES.read().ok().and_then(|lock| lock.clone());
    #[cfg(test)]
    return CUSTOM_PANES.with(|c| c.borrow().clone());
}

/// One pane the manager can show.
#[derive(Clone, Copy, Debug)]
pub struct PaneSpec {
    /// Stable key used in `PaneLayout`, commands and `ui.set` (`navigation`).
    pub key: &'static str,
    /// Header title.
    pub title: &'static str,
    /// Side the pane opens on when the layout does not remember one.
    pub default_side: Side,
    /// Width its side gets when the pane is the first one opened there.
    pub default_width: f32,
    /// Draws the body (the manager draws the header).
    pub show: fn(&mut CraftApp, &mut Ui),
}

impl PartialEq for PaneSpec {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.title == other.title && self.default_side == other.default_side && self.default_width == other.default_width
    }
}

impl PaneSpec {
    /// Create a new pane specification.
    pub const fn new(key: &'static str, title: &'static str, default_side: Side, default_width: f32, show: fn(&mut CraftApp, &mut Ui)) -> Self {
        Self { key, title, default_side, default_width, show }
    }
}

/// Register custom pane specifications for the application.
pub fn set_panes(specs: Vec<PaneSpec>) {
    store_custom_panes(Some(specs));
}

/// Reset pane specifications to the default set.
pub fn reset_panes() {
    store_custom_panes(None);
}

/// Every pane, in the order the Window menu lists them.
pub fn panes() -> Vec<PaneSpec> {
    if let Some(custom) = custom_panes() {
        return custom;
    }
    default_panes()
}

/// The spec for `key`, if there is such a pane.
pub fn spec(key: &str) -> Option<PaneSpec> {
    panes().into_iter().find(|p| p.key == key)
}

/// Copy the layout's open state into the engine's `view.navigationPane` / `view.propertiesPane`
/// flags (see the module docs: the layout is the truth, the flags mirror it).
pub fn mirror_view_flags(app: &mut CraftApp) {
    app.session.view.nav_pane = app.ui.panes.is_open("navigation");
    // The Properties pane is the Inspector's Document section now; the flag follows the Inspector.
    app.session.view.props_pane = app.ui.panes.is_open("shapeData");
}

fn mirror_id() -> egui::Id {
    egui::Id::new("panes-view-flags-mirror")
}

/// Per frame: an engine command (ribbon, menu, control channel) that flipped a view flag since
/// the last mirror is applied to the layout; then the flags are set from the layout again.
fn sync_view_flags(app: &mut CraftApp, ctx: &egui::Context) {
    let view = (app.session.view.nav_pane, app.session.view.props_pane);
    let last: Option<(bool, bool)> = ctx.data(|d| d.get_temp(mirror_id()));
    if let Some(last) = last
        && last != view
    {
        for (key, was, now) in [("navigation", last.0, view.0), ("shapeData", last.1, view.1)] {
            if was != now {
                if now {
                    app.ui.panes.open(key, None);
                } else {
                    app.ui.panes.close(key);
                }
            }
        }
    }
    mirror_view_flags(app);
    let now = (app.session.view.nav_pane, app.session.view.props_pane);
    ctx.data_mut(|d| d.insert_temp(mirror_id(), now));
}

/// The toggle command of a pane (`ui.pane.<key>`), as spelled in [`crate::UI_COMMANDS`]; a pane
/// without a row there has no menu item, so adding a pane means adding the row too (the
/// `registry_has_the_three_panes_and_commands` test checks).
pub fn command_id(key: &str) -> Option<&'static str> {
    crate::UI_COMMANDS.iter().map(|(id, ..)| *id).find(|id| id.strip_prefix("ui.pane.") == Some(key))
}

/// Window menu entries: one per pane, `(title, command id, checked)`. The id is the pane's
/// toggle command (`ui.pane.<key>`), so a menu item needs no params; the strings are static so
/// the menu tree can hold them as [`crate::menus::Item::Check`].
pub fn window_menu_items(app: &CraftApp) -> Vec<(&'static str, &'static str, bool)> {
    panes().iter().filter_map(|p| Some((p.title, command_id(p.key)?, app.ui.panes.is_open(p.key)))).collect()
}

/// `ui.*` commands of the pane manager; `None` when `id` is not one of them.
///
/// - `ui.pane {name, value?: bool, side?}`: toggle (no `value`), open or close a pane; `side`
///   says where to open it
/// - `ui.pane.<key> {value?}`: the same for one pane, for menus
/// - `ui.paneSide {name, side}`: move a pane to a side (opening it there)
/// - `ui.collapsePanes {side, value?: bool}`: fold a side to its strip, or unfold it
/// - `ui.panesReset {}`: the default layout
///
/// Every reply is the layout as it is afterwards. A call without its `name` or `side` changes
/// nothing and just reports the layout; an unknown pane or side is an error.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let b = |k: &str| p.get(k).and_then(Value::as_bool);
    let side = || match s("side") {
        None => Ok(None),
        Some(x) => Side::parse(x).map(Some).ok_or_else(|| format!("unknown side {x:?} (left or right)")),
    };
    if id == "ui.tree" {
        return Some(model_tree::ui_tree(app, p));
    }
    if id == "ui.inspector" {
        return Some(inspector::ui_command(app, p));
    }
    if id == "ui.shapesSearch" {
        return Some(Ok(shapes_search(app, p)));
    }
    if id == "ui.patterns" {
        return Some(patterns::command(app, p));
    }
    if id == "ui.palette" {
        return Some(Ok(palette_command(app, p)));
    }
    let r = match id {
        "ui.pane" => match s("name") {
            Some(name) => toggle_pane(app, name, b("value"), side()),
            None => Ok(()),
        },
        "ui.paneSide" => match (s("name"), side()) {
            (_, Err(e)) => Err(e),
            (None, _) | (_, Ok(None)) => Ok(()),
            (Some(name), Ok(Some(sd))) => {
                if app.ui.panes.move_to(name, sd) {
                    Ok(())
                } else {
                    Err(format!("unknown pane {name:?}"))
                }
            }
        },
        "ui.collapsePanes" => match side() {
            Err(e) => Err(e),
            Ok(None) => Ok(()),
            Ok(Some(sd)) => {
                let on = b("value").unwrap_or(!app.ui.panes.side_collapsed(sd));
                app.ui.panes.set_side_collapsed(sd, on);
                Ok(())
            }
        },
        "ui.panesReset" => {
            app.ui.panes = PaneLayout::default();
            Ok(())
        }
        other => toggle_pane(app, other.strip_prefix("ui.pane.")?, b("value"), side()),
    };
    mirror_view_flags(app);
    Some(r.map(|()| serde_json::to_value(&app.ui.panes).unwrap_or_default()))
}

fn toggle_pane(app: &mut CraftApp, name: &str, value: Option<bool>, side: Result<Option<Side>, String>) -> Result<(), String> {
    let side = side?;
    // `properties` was a pane of its own; it is the Inspector's Document section now.
    let (name, document) = if name == "properties" { ("shapeData", true) } else { (name, false) };
    if spec(name).is_none() {
        return Err(format!("unknown pane {name:?}"));
    }
    let open = value.unwrap_or(!app.ui.panes.is_open(name));
    if open {
        app.ui.panes.open(name, side);
        app.ui.inspector.show_document |= document;
    } else {
        app.ui.panes.close(name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_survives_prefs_and_sanitizes_junk() {
        let mut l = PaneLayout { left_width: f32::NAN, right_width: 1.0e9, ..Default::default() };
        l.sanitize();
        assert_eq!((l.left_width, l.right_width), (240.0, MAX_WIDTH));
        let json = serde_json::to_string(&PaneLayout::default()).unwrap();
        assert!(json.contains("leftWidth"));
        // The cascade 1 shape (two lists and two widths) still loads.
        let back: PaneLayout = serde_json::from_str(r#"{"left": ["shapeData"], "right": []}"#).unwrap();
        assert_eq!(back.side_of("shapeData"), Some("left"));
        assert_eq!(back.side_of("navigation"), None);
        assert_eq!(back.left_width, 240.0);
        assert!(back.heights.is_empty());
    }

    #[test]
    fn heights_fill_and_squeeze() {
        let l = PaneLayout::default();
        let keys = vec!["shapeData".to_string(), "history".to_string()];
        let hs = l.heights_for(&keys, 600.0);
        assert_eq!(hs[0], DEFAULT_HEIGHT);
        assert_eq!(hs[1], 600.0 - DEFAULT_HEIGHT - GAP);
        // Too short: the upper pane gives way to its minimum, the filler keeps its minimum.
        let hs = l.heights_for(&keys, 100.0);
        assert_eq!(hs, vec![MIN_HEIGHT, MIN_HEIGHT]);
        // Collapsed panes are a header tall.
        let mut c = l.clone();
        c.set_collapsed("shapeData", true);
        let hs = c.heights_for(&keys, 600.0);
        assert_eq!(hs[0], HEADER_H);
        assert_eq!(hs[1], 600.0 - HEADER_H - GAP);
        // Nothing expanded: just headers, no panic.
        c.set_collapsed("history", true);
        assert_eq!(c.heights_for(&keys, 600.0), vec![HEADER_H, HEADER_H]);
        assert!(l.heights_for(&keys, f32::NAN).iter().all(|h| h.is_finite()));
    }

    #[test]
    fn resize_moves_the_splitter_within_limits() {
        let mut l = PaneLayout::default();
        let keys = vec!["shapeData".to_string(), "history".to_string()];
        let hs = l.heights_for(&keys, 600.0);
        l.resize(&keys, &hs, 0, 50.0);
        assert_eq!(l.height("shapeData"), DEFAULT_HEIGHT + 50.0);
        l.resize(&keys, &hs, 0, -10_000.0);
        assert_eq!(l.height("shapeData"), MIN_HEIGHT);
        l.resize(&keys, &hs, 0, 10_000.0);
        assert!(l.height("shapeData") <= 600.0 - MIN_HEIGHT);
        l.resize(&keys, &hs, 0, f32::NAN);
        l.resize(&keys, &hs, 7, 10.0);
        l.resize(&keys, &[], 0, 10.0);
    }

    #[test]
    fn custom_pane_registration_and_reset() {
        assert_eq!(panes().len(), default_panes().len());
        set_panes(vec![PaneSpec::new("custom", "Custom Pane", Side::Left, 200.0, |_app, _ui| {})]);
        assert_eq!(panes().len(), 1);
        assert_eq!(spec("custom").unwrap().title, "Custom Pane");
        assert!(spec("navigation").is_none());
        reset_panes();
        assert_eq!(panes().len(), default_panes().len());
        assert!(spec("navigation").is_some());
    }
}
