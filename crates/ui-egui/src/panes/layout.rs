//! Where each pane is: [`PaneLayout`] (open panes per side, widths, heights, collapsed panes)
//! and the size limits the manager keeps it within.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Side, panes, spec};

/// Which panes are open on each side, how wide the sides are, which panes are collapsed to
/// their header and how tall the others are. Pane keys: see [`panes`](super::panes).
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
        // Layers and Shape Data start closed: most pages have no layers and most shapes no
        // data; Layer Properties and the Data tab open them. The Document Stencil opens from
        // Developer > Show/Hide.
        for p in panes().into_iter().filter(|p| !matches!(p.key, "layers" | "shapeData" | "documentStencil")) {
            l.side_mut(p.default_side).push(p.key.to_string());
        }
        // Navigation has nothing to list until pages arrive; it starts folded to its header so
        // the Shapes pane above it gets the height.
        if l.is_open("shapes") && l.is_open("navigation") {
            l.collapsed.push("navigation".into());
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
