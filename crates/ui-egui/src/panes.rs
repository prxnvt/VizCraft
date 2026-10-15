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

use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::{Tokens, regular, semibold};

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

/// One pane the manager can show.
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

/// Every pane, in the order the Window menu lists them.
pub fn panes() -> Vec<PaneSpec> {
    vec![
        PaneSpec { key: "navigation", title: "Navigation", default_side: Side::Left, default_width: 240.0, show: navigation },
        PaneSpec { key: "properties", title: "Properties", default_side: Side::Right, default_width: 260.0, show: properties },
        PaneSpec { key: "history", title: "History", default_side: Side::Right, default_width: 260.0, show: history },
    ]
}

/// The spec for `key`, if there is such a pane.
pub fn spec(key: &str) -> Option<PaneSpec> {
    panes().into_iter().find(|p| p.key == key)
}

/// Which panes are open on each side, how wide the sides are, which panes are collapsed to
/// their header and how tall the others are. Pane keys: see [`panes`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PaneLayout {
    /// Open panes on the left, top to bottom.
    pub left: Vec<String>,
    /// Open panes on the right, top to bottom.
    pub right: Vec<String>,
    pub left_width: f32,
    pub right_width: f32,
    /// The left side is folded to its strip of titles.
    pub left_collapsed: bool,
    /// The right side is folded to its strip of titles.
    pub right_collapsed: bool,
    /// Panes showing only their header.
    pub collapsed: Vec<String>,
    /// Heights the user dragged panes to (points, header included). Absent = default.
    pub heights: BTreeMap<String, f32>,
    /// The side a closed pane was last on, so reopening puts it back there.
    pub last_side: BTreeMap<String, Side>,
}

impl Default for PaneLayout {
    fn default() -> Self {
        let mut l = PaneLayout {
            left: Vec::new(),
            right: Vec::new(),
            left_width: 240.0,
            right_width: 260.0,
            left_collapsed: false,
            right_collapsed: false,
            collapsed: Vec::new(),
            heights: BTreeMap::new(),
            last_side: BTreeMap::new(),
        };
        for p in panes() {
            l.side_mut(p.default_side).push(p.key.to_string());
        }
        l
    }
}

/// Narrowest and widest a side can be.
pub const MIN_WIDTH: f32 = 140.0;
pub const MAX_WIDTH: f32 = 600.0;
/// Height of a pane header (also a collapsed pane's whole height).
pub const HEADER_H: f32 = 26.0;
/// Shortest an expanded pane can be dragged to.
pub const MIN_HEIGHT: f32 = HEADER_H + 48.0;
/// Tallest a stored height can be (guards values from `ui.set`).
pub const MAX_HEIGHT: f32 = 4000.0;
/// Height a pane gets before the user resizes it.
pub const DEFAULT_HEIGHT: f32 = 220.0;
/// Gap between stacked panes; also the splitter's grab area.
pub const GAP: f32 = 6.0;
/// Width of a folded side.
pub const STRIP_W: f32 = 26.0;

impl PaneLayout {
    /// Clamp widths and heights and drop keys that are not panes (a corrupt or older preferences
    /// file must not hide the canvas or crash the manager).
    pub fn sanitize(&mut self) {
        for w in [&mut self.left_width, &mut self.right_width] {
            if !w.is_finite() {
                *w = 240.0;
            }
            *w = w.clamp(MIN_WIDTH, MAX_WIDTH);
        }
        let known: Vec<&'static str> = panes().iter().map(|p| p.key).collect();
        let mut seen: Vec<String> = Vec::new();
        for list in [&mut self.left, &mut self.right] {
            list.retain(|k| {
                let keep = known.contains(&k.as_str()) && !seen.contains(k);
                if keep {
                    seen.push(k.clone());
                }
                keep
            });
        }
        let mut seen: Vec<String> = Vec::new();
        self.collapsed.retain(|k| {
            let keep = known.contains(&k.as_str()) && !seen.contains(k);
            if keep {
                seen.push(k.clone());
            }
            keep
        });
        self.heights.retain(|k, h| known.contains(&k.as_str()) && h.is_finite());
        for h in self.heights.values_mut() {
            *h = h.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
        self.last_side.retain(|k, _| known.contains(&k.as_str()));
    }

    /// Which side (`left`/`right`) an open pane is on.
    pub fn side_of(&self, pane: &str) -> Option<&'static str> {
        self.side(pane).map(Side::as_str)
    }

    /// The side an open pane is on.
    pub fn side(&self, pane: &str) -> Option<Side> {
        if self.left.iter().any(|p| p == pane) {
            Some(Side::Left)
        } else if self.right.iter().any(|p| p == pane) {
            Some(Side::Right)
        } else {
            None
        }
    }

    /// The pane is open (on either side).
    pub fn is_open(&self, pane: &str) -> bool {
        self.side(pane).is_some()
    }

    /// Open panes on `side`, top to bottom.
    pub fn on(&self, side: Side) -> &[String] {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn side_mut(&mut self, side: Side) -> &mut Vec<String> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    /// Width of `side`.
    pub fn width(&self, side: Side) -> f32 {
        match side {
            Side::Left => self.left_width,
            Side::Right => self.right_width,
        }
    }

    /// Set the width of `side` (clamped).
    pub fn set_width(&mut self, side: Side, w: f32) {
        let w = if w.is_finite() { w.clamp(MIN_WIDTH, MAX_WIDTH) } else { 240.0 };
        match side {
            Side::Left => self.left_width = w,
            Side::Right => self.right_width = w,
        }
    }

    /// `side` is folded to its strip.
    pub fn side_collapsed(&self, side: Side) -> bool {
        match side {
            Side::Left => self.left_collapsed,
            Side::Right => self.right_collapsed,
        }
    }

    /// Fold or unfold `side`.
    pub fn set_side_collapsed(&mut self, side: Side, on: bool) {
        match side {
            Side::Left => self.left_collapsed = on,
            Side::Right => self.right_collapsed = on,
        }
    }

    /// Open `pane` (no-op for unknown keys) on `side`, else where it last was, else its default
    /// side. A pane already open stays where it is unless `side` says otherwise. Returns false
    /// for an unknown key.
    pub fn open(&mut self, pane: &str, side: Option<Side>) -> bool {
        let Some(sp) = spec(pane) else { return false };
        let target = side.or_else(|| self.side(pane)).or_else(|| self.last_side.get(pane).copied()).unwrap_or(sp.default_side);
        if self.side(pane) == Some(target) {
            self.set_side_collapsed(target, false);
            return true;
        }
        self.close(pane);
        if self.on(target).is_empty() {
            self.set_width(target, sp.default_width);
        }
        self.side_mut(target).push(pane.to_string());
        self.set_side_collapsed(target, false);
        true
    }

    /// Close `pane`, remembering its side. Returns false for an unknown key.
    pub fn close(&mut self, pane: &str) -> bool {
        if spec(pane).is_none() {
            return false;
        }
        if let Some(side) = self.side(pane) {
            self.last_side.insert(pane.to_string(), side);
        }
        self.left.retain(|p| p != pane);
        self.right.retain(|p| p != pane);
        true
    }

    /// Open a closed pane or close an open one. Returns false for an unknown key.
    pub fn toggle(&mut self, pane: &str) -> bool {
        if self.is_open(pane) { self.close(pane) } else { self.open(pane, None) }
    }

    /// Put `pane` on `side` (opening it if closed), last on that side.
    pub fn move_to(&mut self, pane: &str, side: Side) -> bool {
        if spec(pane).is_none() {
            return false;
        }
        self.left.retain(|p| p != pane);
        self.right.retain(|p| p != pane);
        self.open(pane, Some(side))
    }

    /// Move `pane` within its side so it sits just before `before` (`None` = last).
    pub fn reorder(&mut self, pane: &str, before: Option<&str>) {
        let Some(side) = self.side(pane) else { return };
        if before == Some(pane) {
            return;
        }
        let list = self.side_mut(side);
        list.retain(|p| p != pane);
        let at = before.and_then(|b| list.iter().position(|p| p == b)).unwrap_or(list.len());
        list.insert(at.min(list.len()), pane.to_string());
    }

    /// `pane` shows only its header.
    pub fn is_collapsed(&self, pane: &str) -> bool {
        self.collapsed.iter().any(|p| p == pane)
    }

    /// Collapse `pane` to its header or expand it.
    pub fn set_collapsed(&mut self, pane: &str, on: bool) {
        self.collapsed.retain(|p| p != pane);
        if on && spec(pane).is_some() {
            self.collapsed.push(pane.to_string());
        }
    }

    /// The stored (or default) height of an expanded pane, sanitised.
    pub fn height(&self, pane: &str) -> f32 {
        match self.heights.get(pane) {
            Some(h) if h.is_finite() => h.clamp(MIN_HEIGHT, MAX_HEIGHT),
            _ => DEFAULT_HEIGHT,
        }
    }

    /// Lay out `keys` (one side, top to bottom) in a column `avail` points tall: collapsed panes
    /// get their header, the last expanded pane fills what is left, the others their stored
    /// height. When the column is too short the expanded panes nearest the filler give way
    /// first, down to [`MIN_HEIGHT`].
    pub fn heights_for(&self, keys: &[String], avail: f32) -> Vec<f32> {
        let avail = if avail.is_finite() { avail.max(0.0) } else { 0.0 };
        let filler = keys.iter().rposition(|k| !self.is_collapsed(k));
        let mut hs: Vec<f32> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                if self.is_collapsed(k) {
                    HEADER_H
                } else if Some(i) == filler {
                    0.0
                } else {
                    self.height(k)
                }
            })
            .collect();
        let Some(f) = filler else { return hs };
        let gaps = GAP * keys.len().saturating_sub(1) as f32;
        let used: f32 = hs.iter().sum::<f32>() + gaps;
        let mut deficit = (used + MIN_HEIGHT - avail).max(0.0);
        for i in (0..f).rev() {
            if deficit <= 0.0 {
                break;
            }
            if keys.get(i).is_some_and(|k| self.is_collapsed(k)) {
                continue;
            }
            if let Some(h) = hs.get_mut(i) {
                let give = (*h - MIN_HEIGHT).max(0.0).min(deficit);
                *h -= give;
                deficit -= give;
            }
        }
        let rest = avail - hs.iter().sum::<f32>() - gaps;
        if let Some(h) = hs.get_mut(f) {
            *h = rest.max(MIN_HEIGHT);
        }
        hs
    }

    /// Splitter below pane `i` of `keys` dragged by `dy`: pane `i` grows or shrinks against the
    /// next expanded pane (the filler absorbs the difference).
    pub fn resize(&mut self, keys: &[String], heights: &[f32], i: usize, dy: f32) {
        if !dy.is_finite() || dy == 0.0 {
            return;
        }
        let (Some(k), Some(&h)) = (keys.get(i), heights.get(i)) else { return };
        let filler = keys.iter().rposition(|k| !self.is_collapsed(k));
        let Some(j) = (i + 1..keys.len()).find(|&j| keys.get(j).is_some_and(|n| !self.is_collapsed(n))) else { return };
        let (Some(n), Some(&nh)) = (keys.get(j), heights.get(j)) else { return };
        // The first drag pins the other panes at the heights they show, so panes still at their
        // defaults do not shift while this one is resized.
        for (idx, (o, oh)) in keys.iter().zip(heights).enumerate() {
            if Some(idx) != filler && !self.is_collapsed(o) {
                self.heights.entry(o.clone()).or_insert(oh.clamp(MIN_HEIGHT, MAX_HEIGHT));
            }
        }
        let new_h = (h + dy).clamp(MIN_HEIGHT, (h + nh - MIN_HEIGHT).max(MIN_HEIGHT));
        self.heights.insert(k.clone(), new_h);
        if Some(j) != filler {
            self.heights.insert(n.clone(), (nh - (new_h - h)).max(MIN_HEIGHT));
        }
    }
}

/// Copy the layout's open state into the engine's `view.navigationPane` / `view.propertiesPane`
/// flags (see the module docs: the layout is the truth, the flags mirror it).
pub fn mirror_view_flags(app: &mut CraftApp) {
    app.session.view.nav_pane = app.ui.panes.is_open("navigation");
    app.session.view.props_pane = app.ui.panes.is_open("properties");
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
        for (key, was, now) in [("navigation", last.0, view.0), ("properties", last.1, view.1)] {
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
/// without a row there has no menu item, so a craft adding a pane adds the row too (the
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
    if spec(name).is_none() {
        return Err(format!("unknown pane {name:?}"));
    }
    let open = value.unwrap_or(!app.ui.panes.is_open(name));
    if open {
        app.ui.panes.open(name, side);
    } else {
        app.ui.panes.close(name);
    }
    Ok(())
}

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

fn navigation(_app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(egui::RichText::new("No items").small().color(t.text_dim));
}

fn properties(app: &mut CraftApp, ui: &mut Ui) {
    ui.label(egui::RichText::new("Document title").small().weak());
    let mut doc_title = app.session.doc.title.clone();
    if ui.add(egui::TextEdit::singleline(&mut doc_title).desired_width(f32::INFINITY)).changed() {
        let _ = app.run("edit.setTitle", json!({"title": doc_title}));
    }
    ui.add_space(10.0);
    egui::Grid::new("doc_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        let core = &app.session.doc.core;
        ui.label(egui::RichText::new("Revision").weak());
        ui.label(core.revision.to_string());
        ui.end_row();
        if !app.session.author.is_empty() {
            ui.label(egui::RichText::new("Author").weak());
            ui.label(&app.session.author);
            ui.end_row();
        }
        if !core.modified.is_empty() {
            ui.label(egui::RichText::new("Saved").weak());
            ui.label(&core.modified);
            ui.end_row();
            ui.label(egui::RichText::new("By").weak());
            ui.label(&core.last_modified_by);
            ui.end_row();
        }
    });
}

/// The History pane: what Undo and Redo would do, with buttons, and the document revision.
fn history(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let undo = app.session.undo_label().map(str::to_string);
    let redo = app.session.redo_label().map(str::to_string);
    ui.horizontal(|ui| {
        if ui.add_enabled(undo.is_some(), egui::Button::new("Undo")).clicked() {
            let _ = app.run("edit.undo", json!({}));
        }
        if ui.add_enabled(redo.is_some(), egui::Button::new("Redo")).clicked() {
            let _ = app.run("edit.redo", json!({}));
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Undo").small().weak());
    match &undo {
        Some(l) => ui.label(l.as_str()),
        None => ui.label(egui::RichText::new("Nothing to undo").color(t.text_dim)),
    };
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Redo").small().weak());
    match &redo {
        Some(l) => ui.label(l.as_str()),
        None => ui.label(egui::RichText::new("Nothing to redo").color(t.text_dim)),
    };
    ui.add_space(10.0);
    egui::Grid::new("history_props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Revision").weak());
        ui.label(app.session.rev().to_string());
        ui.end_row();
        ui.label(egui::RichText::new("Unsaved").weak());
        ui.label(if app.session.dirty { "yes" } else { "no" });
        ui.end_row();
    });
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
        let back: PaneLayout = serde_json::from_str(r#"{"left": ["properties"], "right": []}"#).unwrap();
        assert_eq!(back.side_of("properties"), Some("left"));
        assert_eq!(back.side_of("navigation"), None);
        assert_eq!(back.left_width, 240.0);
        assert!(back.heights.is_empty());
    }

    #[test]
    fn heights_fill_and_squeeze() {
        let l = PaneLayout::default();
        let keys = vec!["properties".to_string(), "history".to_string()];
        let hs = l.heights_for(&keys, 600.0);
        assert_eq!(hs[0], DEFAULT_HEIGHT);
        assert_eq!(hs[1], 600.0 - DEFAULT_HEIGHT - GAP);
        // Too short: the upper pane gives way to its minimum, the filler keeps its minimum.
        let hs = l.heights_for(&keys, 100.0);
        assert_eq!(hs, vec![MIN_HEIGHT, MIN_HEIGHT]);
        // Collapsed panes are a header tall.
        let mut c = l.clone();
        c.set_collapsed("properties", true);
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
        let keys = vec!["properties".to_string(), "history".to_string()];
        let hs = l.heights_for(&keys, 600.0);
        l.resize(&keys, &hs, 0, 50.0);
        assert_eq!(l.height("properties"), DEFAULT_HEIGHT + 50.0);
        l.resize(&keys, &hs, 0, -10_000.0);
        assert_eq!(l.height("properties"), MIN_HEIGHT);
        l.resize(&keys, &hs, 0, 10_000.0);
        assert!(l.height("properties") <= 600.0 - MIN_HEIGHT);
        l.resize(&keys, &hs, 0, f32::NAN);
        l.resize(&keys, &hs, 7, 10.0);
        l.resize(&keys, &[], 0, 10.0);
    }
}
