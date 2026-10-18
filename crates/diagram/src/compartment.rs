//! Compartments: sections stacked inside a shape below its main text, the groundwork for UML
//! class boxes (name, attributes, operations), equipment tags with fields and planning cards.
//!
//! A shape keeps its own text as the main text block; each [`Compartment`] in
//! `Shape::compartments` is a band below it, separated by the outline's line:
//!
//! - **Paragraph**: one block of text.
//! - **List**: one row per entry (an attribute, a field, a checklist item).
//! - **Slot**: a region that owns child shapes. A member names the shape as its `parent` (the
//!   container contract: it moves with the shape through [`crate::arrange`] and is selected on its
//!   own) and is listed in the slot's `members`; after every move or resize it is clamped into the
//!   slot ([`refit`], run by `arrange::refit_groups`).
//!
//! One layout function, [`layout`], gives every band's rectangle in the shape's unrotated frame;
//! the canvas, hit testing, SVG (and so PNG and PDF) export, the connection points and the slot
//! clamp all read it, so they agree. Heights are estimated from the font size (there is no font
//! metrics source here): a line is [`LINE_HEIGHT`] times the font size, the way SVG export
//! spaces lines.
//!
//! Each compartment adds two connection points after the shape's own (the left and right
//! midpoints of its band), which a connector end can name as a port, `"<compartment id>.left"` or
//! `"<compartment id>.right"`, so the glue follows the band when compartments are added,
//! reordered or resized ([`End::resolve_port`]).
//!
//! Nothing here panics: lists are capped, numbers clamped and ids that name nothing skipped.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geom::{MAX_COORD, Point, Rect, finite};
use crate::model::{Color, End, MAX_ID, Page, Shape, Style, truncate};
use crate::serde_util::{lenient, lenient_vec};
use crate::style_fields::HAlign;
use crate::{MAX_EXTENT, MAX_TEXT, arrange};

/// Most compartments one shape keeps.
pub const MAX_COMPARTMENTS: usize = 32;
/// Most rows one list compartment keeps.
pub const MAX_ROWS: usize = 256;
/// Longest row kept, in bytes.
pub const MAX_ROW: usize = 4096;
/// Longest title kept, in bytes.
pub const MAX_TITLE: usize = 1024;
/// Most members one slot lists.
pub const MAX_MEMBERS: usize = 256;
/// Line height as a multiple of the font size (the same as SVG export).
pub const LINE_HEIGHT: f64 = 1.2;
/// Space above and below a band's content, in world units.
pub const PAD: f64 = 4.0;
/// Space left and right of a band's text, in world units.
pub const PAD_X: f64 = 6.0;
/// Height of an empty slot region with automatic height.
pub const SLOT_MIN: f64 = 40.0;
/// Space kept between a slot's edge and its members.
pub const SLOT_MARGIN: f64 = 4.0;

/// What a compartment holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CompartmentKind {
    /// One block of text. Unknown names load as this.
    #[default]
    Paragraph,
    /// One row per entry.
    List,
    /// A region that owns child shapes.
    Slot,
}

crate::serde_util::named_enum!(CompartmentKind, CompartmentKind::Paragraph, {
    CompartmentKind::Paragraph => "paragraph",
    CompartmentKind::List => "list",
    CompartmentKind::Slot => "slot",
});

/// How tall a band is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Height {
    /// As tall as its content (estimated from the font size).
    #[default]
    Auto,
    /// This many world units.
    Fixed(f64),
}

impl Height {
    /// A fixed height clamped into the model's range; a non-finite one is automatic.
    pub fn fixed(v: f64) -> Height {
        if v.is_finite() { Height::Fixed(v.clamp(0.0, MAX_EXTENT)) } else { Height::Auto }
    }
}

impl Serialize for Height {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Height::Auto => s.serialize_str("auto"),
            Height::Fixed(v) => s.serialize_f64(*v),
        }
    }
}

impl<'de> Deserialize<'de> for Height {
    /// A number is a fixed height; anything else (`"auto"`, junk) is automatic.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: serde_json::Value = lenient(d)?;
        Ok(v.as_f64().map_or(Height::Auto, Height::fixed))
    }
}

/// A colour that is `None` unless the value is a `#rrggbb` string, so junk does not paint black.
fn opt_color<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Color>, D::Error> {
    let v: serde_json::Value = lenient(d)?;
    Ok(v.as_str().and_then(Color::parse))
}

fn is_auto(h: &Height) -> bool {
    *h == Height::Auto
}

/// One section inside a shape (see the module doc).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Compartment {
    /// Unique within the shape (`attrs`); ports name it.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// What it holds.
    #[serde(deserialize_with = "lenient")]
    pub kind: CompartmentKind,
    /// A heading drawn bold and centred at the top of the band.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A paragraph's text.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// A list's rows.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<String>,
    /// Horizontal alignment of the text or rows.
    #[serde(deserialize_with = "lenient")]
    pub align: HAlign,
    /// Automatic or fixed height.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_auto")]
    pub height: Height,
    /// Font size override; `None` is the shape's.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Bold override; `None` is the shape's.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Band fill; `None` shows the shape's fill.
    #[serde(deserialize_with = "opt_color", skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// Text colour override; `None` is the shape's.
    #[serde(deserialize_with = "opt_color", skip_serializing_if = "Option::is_none")]
    pub font_color: Option<Color>,
    /// A slot's member shapes (each has the shape as its `parent`).
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

impl Default for Compartment {
    fn default() -> Self {
        Compartment {
            id: String::new(),
            kind: CompartmentKind::Paragraph,
            title: None,
            text: String::new(),
            rows: Vec::new(),
            align: HAlign::Left,
            height: Height::Auto,
            font_size: None,
            bold: None,
            fill: None,
            font_color: None,
            members: Vec::new(),
        }
    }
}

impl Compartment {
    /// An empty compartment of `kind`.
    pub fn new(id: impl Into<String>, kind: CompartmentKind) -> Self {
        Compartment { id: id.into(), kind, ..Compartment::default() }
    }
    /// Clamp lengths, counts and numbers into the model's ranges.
    pub fn sanitize(&mut self) {
        truncate(&mut self.id, MAX_ID);
        if let Some(t) = &mut self.title {
            truncate(t, MAX_TITLE);
        }
        truncate(&mut self.text, MAX_TEXT);
        self.rows.truncate(MAX_ROWS);
        for r in &mut self.rows {
            // A row is one line: a newline would draw over the next row.
            if r.contains(['\n', '\r']) {
                *r = r.replace(['\n', '\r'], " ");
            }
            truncate(r, MAX_ROW);
        }
        if let Height::Fixed(v) = self.height {
            self.height = Height::fixed(v);
        }
        self.font_size = self.font_size.map(|v| finite(v, 12.0).clamp(1.0, Style::MAX_FONT_SIZE));
        self.members.truncate(MAX_MEMBERS);
        self.members.iter_mut().for_each(|m| truncate(m, MAX_ID));
        let mut seen = HashSet::new();
        self.members.retain(|m| !m.is_empty() && seen.insert(m.clone()));
        if self.kind != CompartmentKind::Slot {
            self.members.clear();
        }
    }
    /// The editable body: a paragraph's text, a list's rows one per line, nothing for a slot.
    pub fn body_text(&self) -> String {
        match self.kind {
            CompartmentKind::Paragraph => self.text.clone(),
            CompartmentKind::List => self.rows.join("\n"),
            CompartmentKind::Slot => String::new(),
        }
    }
    /// Set the body from text: a paragraph takes it as it is, a list one row per line (an
    /// empty text is no rows), a slot ignores it.
    pub fn set_body_text(&mut self, text: &str) {
        match self.kind {
            CompartmentKind::Paragraph => self.text = text.to_string(),
            CompartmentKind::List => self.rows = if text.is_empty() { Vec::new() } else { text.lines().map(str::to_string).collect() },
            CompartmentKind::Slot => {}
        }
        self.sanitize();
    }
    /// The shape's style with this compartment's overrides applied: what its text draws in.
    pub fn text_style(&self, base: &Style) -> Style {
        let mut st = base.clone();
        if let Some(v) = self.font_size {
            st.font_size = v;
        }
        if let Some(v) = self.bold {
            st.bold = v;
        }
        if let Some(c) = self.font_color {
            st.font_color = c;
        }
        st.h_align = self.align;
        // Bullets and case belong to the main text; rows draw as typed.
        st.bullets = crate::style_fields::Bullets::None;
        st.text_case = crate::style_fields::TextCase::Normal;
        st
    }
}

/// Keep a shape's compartments bounded: at most [`MAX_COMPARTMENTS`], each sanitised, with
/// unique non-empty ids (an empty or repeated id gets the next free `cN`).
pub fn sanitize_all(list: &mut Vec<Compartment>) {
    list.truncate(MAX_COMPARTMENTS);
    list.iter_mut().for_each(Compartment::sanitize);
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..list.len() {
        let taken = list.get(i).is_none_or(|c| c.id.is_empty() || seen.contains(&c.id));
        if taken {
            let fresh = free_id(list, "c");
            if let Some(c) = list.get_mut(i) {
                c.id = fresh;
            }
        }
        if let Some(c) = list.get(i) {
            seen.insert(c.id.clone());
        }
    }
}

/// The first `<prefix>N` (N from 1) no compartment in `list` uses.
pub fn free_id(list: &[Compartment], prefix: &str) -> String {
    (1..=MAX_COMPARTMENTS * 4 + 1)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !list.iter().any(|c| &c.id == id))
        .unwrap_or_else(|| format!("{prefix}x"))
}

/// Whether `id` may name a compartment: the plain-id rule other ids follow.
pub fn is_valid_id(id: &str) -> bool {
    crate::model::is_plain_id(id)
}

/// One band of a shape's layout, in the shape's unrotated frame (`(0, 0)` to `(w, h)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// Index into `Shape::compartments`; `None` is the main text block.
    pub compartment: Option<usize>,
    /// The whole band.
    pub rect: Rect,
    /// The title line, when the compartment has a title.
    pub title: Option<Rect>,
    /// The content area under the title: the text, the rows or the slot region.
    pub body: Rect,
    /// A list's rows, one rectangle per row, top to bottom (rows past the band's bottom are left
    /// out).
    pub rows: Vec<Rect>,
}

impl Band {
    /// A slot's member area: the body inset by [`SLOT_MARGIN`].
    pub fn slot_area(&self) -> Rect {
        inset(self.body, SLOT_MARGIN, SLOT_MARGIN)
    }
}

/// `r` shrunk by `dx` left and right and `dy` top and bottom, never inside out.
fn inset(r: Rect, dx: f64, dy: f64) -> Rect {
    let (cx, cy) = (r.center().x, r.center().y);
    let (hw, hh) = ((r.width() / 2.0 - dx).max(0.0), (r.height() / 2.0 - dy).max(0.0));
    Rect::new(cx - hw, cy - hh, cx + hw, cy + hh)
}

fn line_height(font_size: f64) -> f64 {
    finite(font_size, 12.0).clamp(1.0, Style::MAX_FONT_SIZE) * LINE_HEIGHT
}

fn count_lines(text: &str) -> usize {
    text.lines().count().max(1)
}

/// The height a compartment's content wants (its fixed height, or the estimate).
pub fn natural_height(s: &Shape, c: &Compartment) -> f64 {
    if let Height::Fixed(v) = c.height {
        return v;
    }
    let lh = line_height(c.font_size.unwrap_or(s.style.font_size));
    let title = if c.title.is_some() { lh } else { 0.0 };
    match c.kind {
        CompartmentKind::Paragraph => 2.0 * PAD + title + count_lines(&c.text) as f64 * lh,
        CompartmentKind::List => 2.0 * PAD + title + c.rows.len().max(1) as f64 * lh,
        CompartmentKind::Slot => title + SLOT_MIN,
    }
}

/// The height the main text wants: its lines, or none when it is empty.
fn main_natural(s: &Shape) -> f64 {
    if s.text.trim().is_empty() { 0.0 } else { 2.0 * PAD + count_lines(&s.text) as f64 * line_height(s.style.font_size) }
}

/// The frame height that fits the main text and every compartment at their natural heights.
pub fn fit_height(s: &Shape) -> f64 {
    let bands: f64 = s.compartments.iter().map(|c| natural_height(s, c)).sum();
    (main_natural(s) + bands).min(MAX_EXTENT)
}

/// The bands of `s`: the main text block first (`compartment: None`), then one per compartment
/// in order, stacked top to bottom over the frame. Empty for a shape without compartments.
///
/// Each compartment takes its natural height and the main text takes the rest. When the
/// compartments do not fit under the main text's own lines, they are squeezed in proportion so
/// every band stays inside the frame.
pub fn layout(s: &Shape) -> Vec<Band> {
    if s.compartments.is_empty() {
        return Vec::new();
    }
    let (w, h) = (finite(s.w, 0.0).max(0.0), finite(s.h, 0.0).max(0.0));
    let heights: Vec<f64> = s.compartments.iter().map(|c| natural_height(s, c).max(0.0)).collect();
    let total: f64 = heights.iter().sum();
    let room = (h - main_natural(s).min(h)).max(0.0);
    let k = if total > room && total > 0.0 { room / total } else { 1.0 };
    let used: f64 = heights.iter().map(|v| v * k).sum();
    let main_h = (h - used).max(0.0);
    let mut out = Vec::with_capacity(heights.len() + 1);
    let main = Rect::new(0.0, 0.0, w, main_h);
    out.push(Band { compartment: None, rect: main, title: None, body: main, rows: Vec::new() });
    let mut y = main_h;
    for (i, (c, nat)) in s.compartments.iter().zip(&heights).enumerate() {
        let bh = nat * k;
        let rect = Rect::new(0.0, y, w, (y + bh).min(h.max(y)));
        let lh = line_height(c.font_size.unwrap_or(s.style.font_size));
        let pad = if c.kind == CompartmentKind::Slot { 0.0 } else { PAD.min(rect.height() / 2.0) };
        let mut top = rect.y0 + pad;
        let title = c.title.as_ref().map(|_| {
            let t = Rect::new(0.0, top, w, (top + lh).min(rect.y1));
            top = t.y1;
            t
        });
        let body = Rect::new(0.0, top.min(rect.y1), w, (rect.y1 - pad).max(top.min(rect.y1)));
        let rows = if c.kind == CompartmentKind::List {
            (0..c.rows.len())
                .map(|r| Rect::new(0.0, body.y0 + r as f64 * lh, w, body.y0 + (r + 1) as f64 * lh))
                .take_while(|r| r.y1 <= rect.y1 + 0.5)
                .collect()
        } else {
            Vec::new()
        };
        out.push(Band { compartment: Some(i), rect, title, body, rows });
        y = rect.y1;
    }
    out
}

/// The main text block when the shape has compartments: the top band. Without compartments it
/// is the whole frame.
pub fn main_rect(s: &Shape) -> Rect {
    layout(s).first().map_or_else(|| Rect::from_xywh(0.0, 0.0, s.w, s.h), |b| b.rect)
}

/// The two connection points each compartment adds (left and right midpoints of its band), in
/// normalized `0..1` frame coordinates, in compartment order.
pub fn points(s: &Shape) -> Vec<Point> {
    let h = s.h;
    layout(s)
        .iter()
        .filter(|b| b.compartment.is_some())
        .flat_map(|b| {
            let y = if h > 1e-9 { (b.rect.center().y / h).clamp(0.0, 1.0) } else { 0.5 };
            [Point::new(0.0, y), Point::new(1.0, y)]
        })
        .collect()
}

/// How many connection points the shape has before the compartments' ones.
fn base_points(s: &Shape) -> usize {
    if s.connection_points.is_empty() { s.kind.connection_points().len() } else { s.connection_points.len() }
}

/// The connection point index a port names on `s`: `"<compartment id>.left"` or `".right"`.
pub fn port_index(s: &Shape, port: &str) -> Option<usize> {
    let (id, side) = port.rsplit_once('.')?;
    let side = match side {
        "left" => 0,
        "right" => 1,
        _ => return None,
    };
    let i = s.compartments.iter().position(|c| c.id == id)?;
    base_points(s).checked_add(i.checked_mul(2)?)?.checked_add(side)
}

/// The port name of connection point `index` on `s`, when it is a compartment's point.
pub fn port_for_index(s: &Shape, index: usize) -> Option<String> {
    let rel = index.checked_sub(base_points(s))?;
    let c = s.compartments.get(rel / 2)?;
    Some(format!("{}.{}", c.id, if rel % 2 == 0 { "left" } else { "right" }))
}

/// Longest port name accepted: a compartment id and a side.
pub const MAX_PORT: usize = MAX_ID + 8;

impl End {
    /// The connection point index this end uses on `s`, the shape it is glued to: the point its
    /// `port` names when that resolves, else its `point` (`None` is dynamic glue). A free end has
    /// none.
    pub fn resolve_port(&self, s: &Shape) -> Option<usize> {
        match self {
            End::Glued { point, port, .. } => port.as_deref().and_then(|p| port_index(s, p)).or(*point),
            End::Free { .. } => None,
        }
    }
    /// Glue to port `port` of `shape`.
    pub fn port(shape: impl Into<String>, port: impl Into<String>) -> Self {
        End::Glued { shape: shape.into(), point: None, port: Some(port.into()) }
    }
}

/// The topmost band of `s` under world point `p`: `(compartment index or None for the main
/// text, row index for a list row)`. `None` when `p` is outside the frame or the shape has no
/// compartments.
pub fn band_at(s: &Shape, p: Point) -> Option<(Option<usize>, Option<usize>)> {
    if s.compartments.is_empty() || !p.is_finite() {
        return None;
    }
    let local = s.transform().inverse()?.apply(p);
    let bands = layout(s);
    let b = bands.iter().rev().find(|b| b.rect.contains(local))?;
    let row = b.rows.iter().position(|r| r.contains(local));
    Some((b.compartment, row))
}

/// Clamp every slot member on `page` into its slot after a move or resize: a member listed by a
/// slot of its parent is shifted (with everything inside it) so its bounds lie in the slot area,
/// top-left first when it is larger. Members whose parent is not the slot's shape are skipped.
/// Called by `arrange::refit_groups`, so every move, resize, rotate and flip ends here.
pub fn refit(page: &mut Page) {
    let mut moves: Vec<(String, f64, f64)> = Vec::new();
    for owner in page.shapes.iter().filter(|s| s.compartments.iter().any(|c| !c.members.is_empty())) {
        let Some(inv) = owner.transform().inverse() else { continue };
        let t = owner.transform();
        for band in layout(owner) {
            let Some(c) = band.compartment.and_then(|i| owner.compartments.get(i)) else { continue };
            if c.kind != CompartmentKind::Slot {
                continue;
            }
            let area = band.slot_area();
            for m in &c.members {
                let Some(ms) = page.shape(m).filter(|ms| ms.parent.as_deref() == Some(owner.id.as_str())) else { continue };
                // The member's bounds in the owner's frame (its corners, so rotation counts).
                let corners = ms
                    .outline_world()
                    .bounds()
                    .map(|r| [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)]);
                let Some(local) = corners.and_then(|c| Rect::bounding(c.iter().map(|p| inv.apply(*p)))) else { continue };
                let shift = |lo: f64, hi: f64, a0: f64, a1: f64| -> f64 {
                    // Too big to fit: its top left goes to the slot's top left.
                    if hi - lo >= a1 - a0 || lo < a0 {
                        a0 - lo
                    } else if hi > a1 {
                        a1 - hi
                    } else {
                        0.0
                    }
                };
                let dx = shift(local.x0, local.x1, area.x0, area.x1);
                let dy = shift(local.y0, local.y1, area.y0, area.y1);
                if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                    let v = t.apply_vector(Point::new(dx, dy));
                    if v.is_finite() {
                        moves.push((m.clone(), v.x, v.y));
                    }
                }
            }
        }
    }
    for (id, dx, dy) in moves {
        let all: HashSet<String> = arrange::with_descendants(page, std::slice::from_ref(&id)).into_iter().collect();
        for s in page.shapes.iter_mut().filter(|s| all.contains(&s.id)) {
            s.x = coord(s.x + dx);
            s.y = coord(s.y + dy);
            s.sanitize();
        }
    }
}

/// A coordinate clamped to the model's range.
fn coord(v: f64) -> f64 {
    if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ShapeKind;
    use crate::model::Connector;

    /// A UML-style class: the name as the main text, then attributes and operations lists.
    pub(crate) fn class(id: &str) -> Shape {
        let mut s = Shape::new(id, ShapeKind::Rectangle, 100.0, 100.0, 160.0, 120.0);
        s.text = "Order".into();
        let mut attrs = Compartment::new("attrs", CompartmentKind::List);
        attrs.rows = vec!["+ id: int".into(), "+ total: money".into()];
        let mut ops = Compartment::new("ops", CompartmentKind::List);
        ops.rows = vec!["+ pay()".into()];
        s.compartments = vec![attrs, ops];
        s.sanitize();
        s
    }

    #[test]
    fn bands_stack_below_the_main_text_which_takes_the_rest() {
        let s = class("k");
        let bands = layout(&s);
        assert_eq!(bands.len(), 3);
        let lh = 12.0 * LINE_HEIGHT;
        let (main, attrs, ops) = (&bands[0], &bands[1], &bands[2]);
        assert_eq!(main.compartment, None);
        assert!((ops.rect.height() - (2.0 * PAD + lh)).abs() < 1e-9);
        assert!((attrs.rect.height() - (2.0 * PAD + 2.0 * lh)).abs() < 1e-9);
        assert!((ops.rect.y1 - 120.0).abs() < 1e-9 && (attrs.rect.y1 - ops.rect.y0).abs() < 1e-9 && (main.rect.y1 - attrs.rect.y0).abs() < 1e-9);
        assert_eq!(attrs.rows.len(), 2);
        assert!((attrs.rows[1].y0 - (attrs.rect.y0 + PAD + lh)).abs() < 1e-9);
        assert_eq!(s.text_rect().0, main.rect);
        // Too small a frame: the bands squeeze in proportion and stay inside it.
        let mut tiny = s.clone();
        tiny.h = 30.0;
        let b = layout(&tiny);
        assert!(b.iter().all(|b| b.rect.y0 >= -1e-9 && b.rect.y1 <= 30.0 + 1e-9), "{b:?}");
        assert!(fit_height(&s) <= s.h && fit_height(&tiny) > tiny.h);
    }

    #[test]
    fn each_compartment_adds_two_points_named_by_ports() {
        let s = class("k");
        let base = ShapeKind::Rectangle.connection_points().len();
        let pts = s.connection_points();
        assert_eq!(pts.len(), base + 4);
        let attrs = &layout(&s)[1];
        let y = attrs.rect.center().y / s.h;
        assert_eq!(pts[base], Point::new(0.0, y));
        assert_eq!(pts[base + 1], Point::new(1.0, y));
        assert_eq!(port_index(&s, "attrs.left"), Some(base));
        assert_eq!(port_index(&s, "ops.right"), Some(base + 3));
        assert_eq!(port_index(&s, "ops.middle"), None);
        assert_eq!(port_index(&s, "nope.left"), None);
        assert_eq!(port_for_index(&s, base + 3).as_deref(), Some("ops.right"));
        assert_eq!(port_for_index(&s, 0), None);
        assert_eq!(End::port("k", "ops.left").resolve_port(&s), Some(base + 2));
        // A port that names nothing falls back to the point.
        let e = End::Glued { shape: "k".into(), point: Some(1), port: Some("gone.left".into()) };
        assert_eq!(e.resolve_port(&s), Some(1));
    }

    #[test]
    fn a_port_end_follows_its_band_when_the_shape_moves_and_compartments_change() {
        let mut page = Page::new("p", "P");
        page.shapes.push(class("k"));
        page.shapes.push(Shape::new("o", ShapeKind::Rectangle, 500.0, 100.0, 60.0, 40.0));
        page.connectors.push(Connector::new("c", End::port("k", "attrs.right"), End::dynamic("o")));
        let at = |page: &Page| crate::route::resolve_ends(page, &page.connectors[0]).start.pos;
        let k = page.shape("k").cloned().unwrap();
        let band = &layout(&k)[1];
        assert_eq!(at(&page), Point::new(k.x + k.w, k.y + band.rect.center().y));
        crate::arrange::translate(&mut page, &["k".to_string()], 30.0, 40.0);
        let k = page.shape("k").cloned().unwrap();
        assert_eq!(at(&page), Point::new(k.x + k.w, k.y + layout(&k)[1].rect.center().y));
        // Moving the compartment down: the end goes with it.
        page.shape_mut("k").unwrap().compartments.reverse();
        let k = page.shape("k").cloned().unwrap();
        assert_eq!(at(&page), Point::new(k.x + k.w, k.y + layout(&k)[2].rect.center().y));
        // Round trip on disk: `{"shape": "k", "port": "attrs.right"}`.
        let json = serde_json::to_value(&page.connectors[0].start).unwrap();
        assert_eq!(json, serde_json::json!({"shape": "k", "port": "attrs.right"}));
        let back: End = serde_json::from_value(json).unwrap();
        assert_eq!(back, page.connectors[0].start);
        let junk: End = serde_json::from_value(serde_json::json!({"shape": "k", "port": "x".repeat(500)})).unwrap();
        assert_eq!(junk, End::dynamic("k"));
    }

    #[test]
    fn band_at_picks_the_band_and_the_row() {
        let mut s = class("k");
        let b = layout(&s);
        let world = |p: Point| s.transform().apply(p);
        let row1 = b[1].rows[1].center();
        assert_eq!(band_at(&s, world(row1)), Some((Some(0), Some(1))));
        assert_eq!(band_at(&s, world(b[2].rect.center())), Some((Some(1), Some(0))));
        assert_eq!(band_at(&s, world(Point::new(80.0, 5.0))), Some((None, None)));
        assert_eq!(band_at(&s, Point::new(-1000.0, 0.0)), None);
        // Rotation is honoured.
        s.angle_deg = 90.0;
        let b = layout(&s);
        assert_eq!(band_at(&s, s.transform().apply(b[2].rect.center())), Some((Some(1), Some(0))));
        assert_eq!(band_at(&Shape::default(), Point::ZERO), None);
    }

    #[test]
    fn a_slot_member_moves_with_its_parent_and_is_clamped_into_the_slot() {
        let mut page = Page::new("p", "P");
        let mut tag = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 200.0, 140.0);
        tag.text = "Pump".into();
        let mut slot = Compartment::new("pins", CompartmentKind::Slot);
        slot.height = Height::Fixed(80.0);
        slot.members = vec!["m".into(), "ghost".into()];
        tag.compartments = vec![slot];
        page.shapes.push(tag);
        let mut m = Shape::new("m", ShapeKind::Ellipse, 500.0, 500.0, 20.0, 20.0);
        m.parent = Some("t".into());
        page.shapes.push(m);
        crate::arrange::refit_groups(&mut page);
        let area = layout(page.shape("t").unwrap())[1].slot_area();
        let m = page.shape("m").unwrap();
        assert_eq!((m.x + m.w, m.y + m.h), (area.x1, area.y1), "clamped into the bottom right of the slot");
        // Moving the parent carries the member.
        crate::arrange::translate(&mut page, &["t".to_string()], 100.0, 10.0);
        let m = page.shape("m").unwrap();
        assert_eq!((m.x + m.w, m.y + m.h), (area.x1 + 100.0, area.y1 + 10.0));
        // Shrinking the parent keeps the member inside the smaller slot.
        crate::arrange::set_frame(&mut page, "t", 100.0, 10.0, 100.0, 120.0);
        let t = page.shape("t").unwrap();
        let area = layout(t)[1].slot_area();
        let m = page.shape("m").unwrap();
        assert!(m.x >= t.x + area.x0 - 1e-9 && m.x + m.w <= t.x + area.x1 + 1e-9, "{m:?}");
        // A member that left (another parent) is not pulled back.
        page.shape_mut("m").unwrap().parent = None;
        crate::arrange::translate(&mut page, &["m".to_string()], 900.0, 0.0);
        assert!(page.shape("m").unwrap().x > 900.0);
    }

    #[test]
    fn sanitizing_caps_counts_and_gives_unique_ids() {
        let mut s = Shape {
            compartments: (0..40).map(|_| Compartment { id: "x".into(), rows: vec!["a\nb".into(); 300], ..Compartment::default() }).collect(),
            ..Shape::default()
        };
        s.compartments[1].id = String::new();
        s.compartments[2].height = Height::Fixed(f64::INFINITY);
        s.compartments[3].font_size = Some(f64::NAN);
        s.compartments[4].members = vec!["m".into()];
        s.sanitize();
        assert_eq!(s.compartments.len(), MAX_COMPARTMENTS);
        let ids: HashSet<&str> = s.compartments.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), MAX_COMPARTMENTS);
        assert!(s.compartments.iter().all(|c| c.rows.len() == MAX_ROWS && c.rows[0] == "a b"));
        assert_eq!(s.compartments[2].height, Height::Auto);
        assert_eq!(s.compartments[3].font_size, Some(12.0));
        assert!(s.compartments[4].members.is_empty(), "only slots have members");
        // Junk on disk loads as defaults.
        let c: Compartment =
            serde_json::from_value(serde_json::json!({"id": 5, "kind": "zz", "rows": [1, "a"], "height": "tall", "fill": "red"})).unwrap();
        assert_eq!(
            (c.id.as_str(), c.kind, c.rows.clone(), c.height, c.fill),
            ("", CompartmentKind::Paragraph, vec!["a".to_string()], Height::Auto, None)
        );
        let c: Compartment = serde_json::from_value(serde_json::json!({"height": 30, "kind": "slot"})).unwrap();
        assert_eq!((c.kind, c.height), (CompartmentKind::Slot, Height::Fixed(30.0)));
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!({"id": "", "kind": "slot", "align": "left", "height": 30.0}));
    }
}
