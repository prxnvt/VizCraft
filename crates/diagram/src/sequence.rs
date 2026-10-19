//! UML sequence diagrams as a substrate: a Sequence page is an ordinary page with an ordered list
//! of events ([`Page::sequence`]), and after every command [`settle_doc`] redraws the page from
//! that list.
//!
//! - Lifelines are shapes of kind [`ShapeKind::Lifeline`] or [`ShapeKind::ActorLifeline`] with a
//!   [`Part::Lifeline`]: the outline is the head, the kind's detail is the dashed stem.
//! - Messages are straight connectors with [`Connector::message`], glued by connection point to
//!   the lifelines; settle owns those points (four frame midpoints, then one per message end in
//!   event order) and the message rows.
//! - Activation bars and destruction marks are generated shapes ([`Part::Activation`],
//!   [`Part::Destruction`]), locked, reused per lifeline and owned by settle.
//! - Combined fragments are locked, transparent containers with a [`Part::Fragment`]; the
//!   operator tab, operand separators and guards are drawn by [`fragment_decor`], which
//!   [`crate::container::decor`] calls.
//! - Notes are any shapes with a [`Part::Note`].
//!
//! The geometry comes from `vizcraft_layout::sequence`. [`check_order`] and [`check_replies`]
//! are the logic behind the notation pack's order rules.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use vizcraft_layout::sequence::{self as lay, MessageSort, NoteSide, SeqEvent, SeqInput, SeqLifeline, SeqOptions};

use crate::autolayout::PrimitiveOutcome;
use crate::container::{Container, Decor, HeadingStyle};
use crate::geom::{Path, PathEl, Point, Rect};
use crate::model::{MAX_ID, truncate};
use crate::serde_util::{lenient, lenient_vec, named_enum, num};
use crate::shape_fields::{Freeform, Locks, TextBlock};
use crate::{Color, Connector, Document, End, Page, Route, Shape, ShapeKind};

/// Height of a lifeline's head box.
pub const HEAD: f64 = 40.0;
/// Height of an actor lifeline's head: the stick figure and the name band under it.
pub const ACTOR_HEAD: f64 = 64.0;
/// Most events one interaction keeps.
pub const MAX_EVENTS: usize = lay::MAX_EVENTS;
/// Most operands one fragment keeps.
pub const MAX_OPERANDS: usize = 32;
/// Longest guard kept, in bytes.
pub const MAX_GUARD: usize = 512;
/// Most lifelines a fragment or note names.
pub const MAX_COVERS: usize = 256;
/// Most dash segments a stem draws; beyond that the period stretches.
pub const MAX_DASHES: usize = 2000;
/// Width of an activation bar.
pub const ACTIVATION_W: f64 = 10.0;
/// Size of a destruction mark.
pub const CROSS: f64 = 16.0;
/// Narrowest lifeline head.
const MIN_HEAD_W: f64 = 80.0;
/// Narrowest actor head.
const MIN_ACTOR_W: f64 = 60.0;
/// Height of a fragment's operator tab.
const TAB_H: f64 = lay::TAB_H;
/// Margin of a page laid out from scratch.
const MARGIN: f64 = 40.0;
/// Prefix of ids handed to generated shapes before the document numbers them.
const PENDING: &str = "\u{1}seq";

// ---------------------------------------------------------------- model

/// Whether activation bars follow calls and replies or only explicit events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activations {
    /// A synchronous call opens a bar on its receiver; its reply closes it.
    #[default]
    Auto,
    /// Only `activate` and `deactivate` events draw bars.
    Explicit,
}

named_enum!(Activations, Activations::Auto, {
    Activations::Auto => "auto",
    Activations::Explicit => "explicit",
});

/// One event of an interaction, naming a page item. On disk a one-key object:
/// `{"message": "c1"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// A message connector.
    Message(String),
    /// A fragment shape starts.
    Open(String),
    /// The fragment's next operand starts.
    Else(String),
    /// The fragment ends.
    Close(String),
    /// A note shape.
    Note(String),
    /// An explicit activation starts on a lifeline shape.
    Activate(String),
    /// The lifeline's innermost activation ends.
    Deactivate(String),
}

impl Event {
    /// The item the event names.
    pub fn id(&self) -> &str {
        match self {
            Event::Message(i) | Event::Open(i) | Event::Else(i) | Event::Close(i) | Event::Note(i) | Event::Activate(i) | Event::Deactivate(i) => i,
        }
    }
    fn id_mut(&mut self) -> &mut String {
        match self {
            Event::Message(i) | Event::Open(i) | Event::Else(i) | Event::Close(i) | Event::Note(i) | Event::Activate(i) | Event::Deactivate(i) => i,
        }
    }
    /// The kind as it appears on disk (`message`, `open`, ...).
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Message(_) => "message",
            Event::Open(_) => "open",
            Event::Else(_) => "else",
            Event::Close(_) => "close",
            Event::Note(_) => "note",
            Event::Activate(_) => "activate",
            Event::Deactivate(_) => "deactivate",
        }
    }
}

/// A page's interaction: the single source of time order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Interaction {
    /// The events, top to bottom, at most [`MAX_EVENTS`]; unknown ones are dropped on load.
    #[serde(deserialize_with = "lenient_vec")]
    pub events: Vec<Event>,
    /// How activation bars are drawn.
    #[serde(deserialize_with = "lenient")]
    pub activations: Activations,
    /// Messages are numbered (kept for round trips through text formats).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_false")]
    pub autonumber: bool,
    /// Height of one message row.
    #[serde(deserialize_with = "num")]
    pub row: f64,
    /// Gap between neighbouring lifeline heads.
    #[serde(deserialize_with = "num")]
    pub column_gap: f64,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for Interaction {
    fn default() -> Self {
        Interaction { events: Vec::new(), activations: Activations::Auto, autonumber: false, row: 36.0, column_gap: 60.0 }
    }
}

impl Interaction {
    /// Cap the events and ids and clamp the spacings.
    pub fn sanitize(&mut self) {
        self.events.truncate(MAX_EVENTS);
        self.events.iter_mut().for_each(|e| truncate(e.id_mut(), MAX_ID));
        self.row = if self.row.is_finite() && self.row >= 16.0 { self.row.min(400.0) } else { 36.0 };
        self.column_gap = if self.column_gap.is_finite() && self.column_gap >= 0.0 { self.column_gap.min(2000.0) } else { 60.0 };
    }
}

/// A combined fragment's interaction operator (UML 2.5.1, 17.12.15 InteractionOperatorKind).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Operator {
    /// Alternatives: one operand runs. Unknown names load as this.
    #[default]
    Alt,
    /// Option: the one operand runs or not.
    Opt,
    /// Loop.
    Loop,
    /// Parallel operands.
    Par,
    /// Break: the operand runs instead of the rest of the enclosing fragment.
    Break,
    /// Critical region.
    Critical,
    /// Interaction use: a reference to another interaction.
    Ref,
    /// Negative: traces that must not happen.
    Neg,
    /// Strict sequencing.
    Strict,
    /// Weak sequencing.
    Seq,
    /// A highlighted region with no operator (Mermaid's `rect`).
    Region,
}

named_enum!(Operator, Operator::Alt, {
    Operator::Alt => "alt",
    Operator::Opt => "opt",
    Operator::Loop => "loop",
    Operator::Par => "par",
    Operator::Break => "break",
    Operator::Critical => "critical",
    Operator::Ref => "ref",
    Operator::Neg => "neg",
    Operator::Strict => "strict",
    Operator::Seq => "seq",
    Operator::Region => "region",
});

impl Operator {
    /// Every operator, in catalog order.
    pub const ALL: [Operator; 11] = [
        Operator::Alt,
        Operator::Opt,
        Operator::Loop,
        Operator::Par,
        Operator::Break,
        Operator::Critical,
        Operator::Ref,
        Operator::Neg,
        Operator::Strict,
        Operator::Seq,
        Operator::Region,
    ];
    /// Whether its operands are alternative or parallel paths, each starting from the state at
    /// the fragment's start.
    pub fn forks(self) -> bool {
        matches!(self, Operator::Alt | Operator::Opt | Operator::Break | Operator::Critical | Operator::Par)
    }
    /// Whether it may have more than one operand.
    pub fn allows_else(self) -> bool {
        !matches!(self, Operator::Opt | Operator::Loop | Operator::Break | Operator::Ref)
    }
}

/// Which side of a lifeline a note sits on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Over the lifeline, or across the two it names. Unknown names load as this.
    #[default]
    Over,
    /// Left of it.
    Left,
    /// Right of it.
    Right,
}

named_enum!(Side, Side::Over, {
    Side::Over => "over",
    Side::Left => "left",
    Side::Right => "right",
});

/// One operand of a fragment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Operand {
    /// The guard, without brackets; at most [`MAX_GUARD`] bytes.
    #[serde(deserialize_with = "lenient")]
    pub guard: String,
    /// Distance of the operand's top from the fragment's top (0 for the first).
    #[serde(deserialize_with = "num")]
    pub at: f64,
}

/// What a shape is in an interaction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "part", rename_all = "camelCase")]
pub enum Part {
    /// A lifeline.
    Lifeline {
        /// Its stem ends at its last event, with a destruction mark.
        #[serde(default, deserialize_with = "lenient")]
        destroyed: bool,
    },
    /// A generated activation bar.
    Activation {
        /// The lifeline shape.
        #[serde(default, deserialize_with = "lenient")]
        lifeline: String,
        /// Nesting level, 1 for a bar on the stem.
        #[serde(default, deserialize_with = "lenient")]
        depth: usize,
    },
    /// A generated destruction mark.
    Destruction {
        /// The lifeline shape.
        #[serde(default, deserialize_with = "lenient")]
        lifeline: String,
    },
    /// A combined fragment.
    Fragment {
        /// The operator.
        #[serde(default, deserialize_with = "lenient")]
        operator: Operator,
        /// The operands, at most [`MAX_OPERANDS`].
        #[serde(default, deserialize_with = "lenient_vec")]
        operands: Vec<Operand>,
        /// Lifeline shapes it spans; empty spans the lifelines its content touches.
        #[serde(default, deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
        covers: Vec<String>,
    },
    /// A note.
    Note {
        /// One lifeline shape, or two for a note across both.
        #[serde(default, deserialize_with = "lenient_vec")]
        over: Vec<String>,
        /// Where it sits.
        #[serde(default, deserialize_with = "lenient")]
        side: Side,
    },
}

impl Part {
    /// Cap the lists and strings.
    pub fn sanitize(&mut self) {
        match self {
            Part::Lifeline { .. } => {}
            Part::Activation { lifeline, depth } => {
                truncate(lifeline, MAX_ID);
                *depth = (*depth).clamp(1, 64);
            }
            Part::Destruction { lifeline } => truncate(lifeline, MAX_ID),
            Part::Fragment { operands, covers, .. } => {
                operands.truncate(MAX_OPERANDS);
                if operands.is_empty() {
                    operands.push(Operand::default());
                }
                for o in operands.iter_mut() {
                    truncate(&mut o.guard, MAX_GUARD);
                    o.at = if o.at.is_finite() { o.at.clamp(0.0, crate::MAX_EXTENT) } else { 0.0 };
                }
                covers.truncate(MAX_COVERS);
                covers.iter_mut().for_each(|c| truncate(c, MAX_ID));
            }
            Part::Note { over, .. } => {
                over.truncate(2);
                over.iter_mut().for_each(|c| truncate(c, MAX_ID));
            }
        }
    }
    /// Whether the shape stands for something of the model (a lifeline or a note) rather than
    /// being drawn by settle (bars, marks, fragments): a merging import skips the others.
    pub fn is_element_view(&self) -> bool {
        matches!(self, Part::Lifeline { .. } | Part::Note { .. })
    }
}

/// The sort of a message (UML 2.5.1, 17.12.22 MessageSort, plus lost and found from 17.12.20 MessageKind).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    /// A synchronous call. Unknown names load as this.
    #[default]
    Sync,
    /// An asynchronous call or signal.
    Async,
    /// A reply.
    Reply,
    /// Creates its receiver.
    Create,
    /// Destroys its receiver.
    Destroy,
    /// The receiver is not shown.
    Lost,
    /// The sender is not shown.
    Found,
}

named_enum!(Sort, Sort::Sync, {
    Sort::Sync => "sync",
    Sort::Async => "async",
    Sort::Reply => "reply",
    Sort::Create => "create",
    Sort::Destroy => "destroy",
    Sort::Lost => "lost",
    Sort::Found => "found",
});

impl Sort {
    /// Every sort, in catalog order.
    pub const ALL: [Sort; 7] = [Sort::Sync, Sort::Async, Sort::Reply, Sort::Create, Sort::Destroy, Sort::Lost, Sort::Found];
    /// The `uml.Message` property value (`synchCall`, `reply`, ...).
    pub fn uml(self) -> &'static str {
        match self {
            Sort::Sync => "synchCall",
            Sort::Async | Sort::Lost | Sort::Found => "asynchCall",
            Sort::Reply => "reply",
            Sort::Create => "createMessage",
            Sort::Destroy => "deleteMessage",
        }
    }
    /// The `uml.*Message` connector type that draws it.
    pub fn connector_type(self) -> &'static str {
        match self {
            Sort::Sync => "uml.syncMessage",
            Sort::Async | Sort::Lost | Sort::Found => "uml.asyncMessage",
            Sort::Reply => "uml.replyMessage",
            Sort::Create => "uml.createMessage",
            Sort::Destroy => "uml.destroyMessage",
        }
    }
    /// The sort a `uml.*Message` connector type draws; lost and found share the async type, so
    /// that type reads as async.
    pub fn from_connector_type(kind: &str) -> Option<Sort> {
        [Sort::Sync, Sort::Async, Sort::Reply, Sort::Create, Sort::Destroy].into_iter().find(|s| s.connector_type() == kind)
    }
    fn layout(self) -> MessageSort {
        match self {
            Sort::Sync => MessageSort::Sync,
            Sort::Async => MessageSort::Async,
            Sort::Reply => MessageSort::Reply,
            Sort::Create => MessageSort::Create,
            Sort::Destroy => MessageSort::Destroy,
            Sort::Lost => MessageSort::Lost,
            Sort::Found => MessageSort::Found,
        }
    }
}

/// What makes a connector a message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    /// Its sort.
    #[serde(deserialize_with = "lenient")]
    pub sort: Sort,
}

// ---------------------------------------------------------------- drawing

/// The outline of a lifeline kind: its head. A lifeline's head is a box [`HEAD`] tall; an
/// actor's is the stick figure's head circle. A last lone point at the stem's foot keeps the
/// bounds over the whole stem without drawing anything.
pub fn head_outline(kind: ShapeKind, w: f64, h: f64) -> Path {
    let (w, h) = (size(w), size(h));
    let p = Point::new;
    let mut path = if kind == ShapeKind::ActorLifeline {
        let r = (w.min(h) / 2.0).min(7.0);
        crate::kind::ellipse(Rect::new(w / 2.0 - r, 1.0f64.min(h), w / 2.0 + r, (1.0 + 2.0 * r).min(h)))
    } else {
        let hh = HEAD.min(h);
        Path::polygon(&[p(0.0, 0.0), p(w, 0.0), p(w, hh), p(0.0, hh)])
    };
    path.els.push(PathEl::MoveTo(p(w / 2.0, h)));
    path
}

/// The detail of a lifeline kind: the dashed stem under the head (6 on, 4 off, at most
/// [`MAX_DASHES`] dashes, the period stretched beyond that), and an actor's body lines.
pub fn stem_detail(kind: ShapeKind, w: f64, h: f64) -> Option<Path> {
    let (w, h) = (size(w), size(h));
    let p = Point::new;
    let cx = w / 2.0;
    let top = if kind == ShapeKind::ActorLifeline { ACTOR_HEAD } else { HEAD }.min(h);
    let mut path = Path::new();
    if kind == ShapeKind::ActorLifeline {
        let y = |v: f64| v.min(h);
        path.move_to(p(cx, y(15.0))).line_to(p(cx, y(30.0)));
        path.move_to(p(cx - 10.0, y(20.0))).line_to(p(cx + 10.0, y(20.0)));
        path.move_to(p(cx, y(30.0))).line_to(p(cx - 8.0, y(42.0)));
        path.move_to(p(cx, y(30.0))).line_to(p(cx + 8.0, y(42.0)));
    }
    let len = h - top;
    if len > 0.0 {
        let count = (len / 10.0).ceil().clamp(1.0, MAX_DASHES as f64);
        let period = len / count;
        let on = period * 0.6;
        let mut k = 0.0;
        while k < count {
            let y0 = top + k * period;
            path.move_to(p(cx, y0)).line_to(p(cx, (y0 + on).min(h)));
            k += 1.0;
        }
    }
    Some(path)
}

/// The connection points of a lifeline kind: the frame's midpoints, top, right, bottom, left.
pub fn frame_points() -> Vec<Point> {
    vec![Point::new(0.5, 0.0), Point::new(1.0, 0.5), Point::new(0.5, 1.0), Point::new(0.0, 0.5)]
}

fn size(v: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, crate::MAX_EXTENT) } else { 0.0 }
}

/// Whether `p` is on lifeline `s`: in its head or within `tol` of its stem.
pub fn stem_hit(s: &Shape, p: Point, tol: f64) -> bool {
    if !matches!(s.kind, ShapeKind::Lifeline | ShapeKind::ActorLifeline) {
        return false;
    }
    let tol = if tol.is_finite() { tol.clamp(0.0, 1000.0) } else { 0.0 };
    let head = if s.kind == ShapeKind::ActorLifeline { ACTOR_HEAD } else { HEAD }.min(s.h);
    let (x, y) = (p.x - s.x, p.y - s.y);
    let in_head = (-tol..=s.w + tol).contains(&x) && (-tol..=head).contains(&y);
    let on_stem = (x - s.w / 2.0).abs() <= tol && y >= head - tol && y <= s.h + tol;
    in_head || on_stem
}

/// The operator tab, operand separators and guards of a fragment shape; empty for others.
pub fn fragment_decor(s: &Shape) -> Vec<Decor> {
    let Some(Part::Fragment { operator, operands, .. }) = &s.sequence else { return Vec::new() };
    let t = s.transform();
    let world = |pts: &[(f64, f64)]| pts.iter().map(|(x, y)| t.apply(Point::new(*x, *y))).collect::<Vec<_>>();
    let line = {
        let l = s.style.effective_line();
        if l.a == 0 { Color::rgb(0x2b, 0x3a, 0x4a) } else { l }
    };
    let font = Color::rgb(0x1f, 0x2a, 0x36);
    let size = decor_font(s);
    let (w, h) = (size_of(s.w), size_of(s.h));
    let mut out = Vec::new();
    let tw = tab_w(*operator, size).min(w);
    // Settle keeps the guards' x (clear of the first covered stem and its bars) as the margin.
    let guard_x = match &s.container {
        Some(c) if c.margin > 0.0 && c.margin < w => c.margin.max(tw + 8.0),
        _ => tw + 8.0,
    };
    if *operator != Operator::Region {
        let name = operator.name();
        let th = TAB_H.min(h);
        let cut = 6.0f64.min(tw / 2.0).min(th / 2.0);
        out.push(Decor::Line {
            points: world(&[(0.0, 0.0), (tw, 0.0), (tw, th - cut), (tw - cut, th), (0.0, th), (0.0, 0.0)]),
            color: line,
            width: 1.0,
            dashed: false,
        });
        out.push(Decor::Text {
            center: t.apply(Point::new(tw / 2.0, th / 2.0)),
            room: tw,
            angle: s.angle_deg,
            text: name.into(),
            size,
            color: font,
            bold: true,
        });
    }
    for (k, o) in operands.iter().enumerate() {
        let at = o.at.clamp(0.0, h);
        if k > 0 {
            out.push(Decor::Line { points: world(&[(0.0, at), (w, at)]), color: line, width: 1.0, dashed: true });
        }
        if o.guard.trim().is_empty() {
            continue;
        }
        let text = if *operator == Operator::Ref { o.guard.clone() } else { guard_text(&o.guard) };
        let gw = text_w(&text, size);
        let (gx, gy) = (guard_x, if k == 0 { TAB_H / 2.0 } else { at + TAB_H / 2.0 });
        if *operator == Operator::Ref {
            out.push(Decor::Text {
                center: t.apply(Point::new(w / 2.0, h / 2.0)),
                room: w,
                angle: s.angle_deg,
                text,
                size,
                color: font,
                bold: false,
            });
        } else {
            let room = (w - gx).max(0.0);
            out.push(Decor::Text {
                center: t.apply(Point::new(gx + gw.min(room) / 2.0, gy.min(h))),
                room,
                angle: s.angle_deg,
                text,
                size,
                color: font,
                bold: false,
            });
        }
    }
    out
}

fn size_of(v: f64) -> f64 {
    size(v)
}

/// Font size of a fragment's tab and guards.
fn decor_font(s: &Shape) -> f64 {
    s.style.font_size.clamp(6.0, 48.0)
}

/// Width of an operator's tab at font `size`; none for a region.
fn tab_w(operator: Operator, size: f64) -> f64 {
    if operator == Operator::Region { 0.0 } else { text_w(operator.name(), size) + 16.0 }
}

/// A guard as drawn: in brackets.
fn guard_text(guard: &str) -> String {
    format!("[{}]", guard.trim())
}

/// Width of an operand's guard as drawn (none for an empty guard or a `ref`, whose text is
/// centred in the frame).
fn guard_w(operator: Operator, guard: &str, size: f64) -> f64 {
    if operator == Operator::Ref || guard.trim().is_empty() { 0.0 } else { text_w(&guard_text(guard), size) }
}

/// Estimated width of one line of text at `font` size.
fn text_w(text: &str, font: f64) -> f64 {
    text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f64 * font * 0.6
}

/// Space between a message label's box and its line.
pub const LABEL_GAP: f64 = 3.0;

/// The box a connector label of `text` at `font` size takes, as SVG export draws it: width and
/// height.
pub fn label_box_size(text: &str, font: f64) -> (f64, f64) {
    let font = if font.is_finite() { font.clamp(1.0, 400.0) } else { 12.0 };
    let lines: Vec<&str> = text.lines().collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    (longest * font * 0.6 + font * 0.6, lines.len().max(1) as f64 * font * 1.2 + font * 0.3)
}

/// The centre of a message's label: above a straight message (lost and found ones above their
/// stub), its box's bottom [`LABEL_GAP`] over the line's midpoint; right of a self message's loop,
/// left-aligned 4 past it at the loop's middle. `None` for a route with no points. Labels never
/// sit on the line, so a stem or arrowhead never runs through one.
pub fn label_anchor(polyline: &[Point], text: &str, style: &crate::Style) -> Option<Point> {
    let (first, last) = (*polyline.first()?, *polyline.last()?);
    let (bw, bh) = label_box_size(text, style.font_size);
    if polyline.len() >= 4 {
        let x = polyline.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        return Some(Point::new(x + 4.0 + bw / 2.0, (first.y + last.y) / 2.0));
    }
    let mid = crate::geom::point_along(polyline, 0.5)?;
    Some(Point::new(mid.x, mid.y - bh / 2.0 - LABEL_GAP))
}

/// [`label_anchor`] on `page`: a straight message that passes other lifelines keeps its label
/// clear of their stems and activation bars, centred in the widest clear stretch of the line when
/// the middle is taken (the layout keeps the stretch by the sender wide enough for the label).
pub fn label_anchor_on(page: &Page, polyline: &[Point], text: &str, style: &crate::Style) -> Option<Point> {
    let p = label_anchor(polyline, text, style)?;
    let [first, last] = polyline else { return Some(p) };
    let (bw, bh) = label_box_size(text, style.font_size);
    let (y0, y1) = (p.y - bh / 2.0, p.y + bh / 2.0);
    let (a, b) = (first.x.min(last.x), first.x.max(last.x));
    let mut blocks: Vec<(f64, f64)> = page
        .shapes
        .iter()
        .filter_map(|s| match &s.sequence {
            Some(Part::Lifeline { .. }) => {
                let head = if s.kind == ShapeKind::ActorLifeline { ACTOR_HEAD } else { HEAD }.min(s.h);
                let x = s.x + s.w / 2.0;
                (s.y + head < y1 && s.y + s.h > y0).then_some((x, x))
            }
            Some(Part::Activation { .. }) => (s.y < y1 && s.y + s.h > y0).then_some((s.x, s.x + s.w)),
            _ => None,
        })
        .filter(|(l, r)| *r > a && *l < b)
        .collect();
    let clear = |cx: f64| blocks.iter().all(|(l, r)| *r <= cx - bw / 2.0 || *l >= cx + bw / 2.0);
    if blocks.is_empty() || clear(p.x) {
        return Some(p);
    }
    blocks.sort_by(|u, v| u.0.total_cmp(&v.0));
    let mut gaps: Vec<(f64, f64)> = Vec::new();
    let mut cur = a;
    for (l, r) in &blocks {
        if *l > cur {
            gaps.push((cur, *l));
        }
        cur = cur.max(*r);
    }
    if b > cur {
        gaps.push((cur, b));
    }
    let best = gaps.into_iter().max_by(|u, v| (u.1 - u.0).total_cmp(&(v.1 - v.0)))?;
    Some(Point::new((best.0 + best.1) / 2.0, p.y))
}

/// The box a message's label takes on the page, from its centre: what must stay clear of stems,
/// bars, tabs and guards.
pub fn label_rect(c: &Connector, centre: Point) -> Option<Rect> {
    if c.text.trim().is_empty() {
        return None;
    }
    let (bw, bh) = label_box_size(&c.text, c.style.font_size);
    Some(Rect::new(centre.x - bw / 2.0, centre.y - bh / 2.0, centre.x + bw / 2.0, centre.y + bh / 2.0))
}

/// Estimated size of a label.
fn label_size(text: &str, font: f64) -> (f64, f64) {
    if text.trim().is_empty() {
        return (0.0, 0.0);
    }
    (text_w(text, font) + 8.0, text.split('\n').count() as f64 * font * 1.3)
}

// ---------------------------------------------------------------- settle

/// The lifeline a connector end is glued to, if it is one.
fn lifeline_of<'a>(page: &'a Page, e: &End) -> Option<&'a Shape> {
    let s = page.shape(e.shape()?)?;
    matches!(s.sequence, Some(Part::Lifeline { .. })).then_some(s)
}

fn is_lifeline(page: &Page, id: &str) -> bool {
    page.shape(id).is_some_and(|s| matches!(s.sequence, Some(Part::Lifeline { .. })))
}

fn is_fragment(page: &Page, id: &str) -> bool {
    page.shape(id).is_some_and(|s| matches!(s.sequence, Some(Part::Fragment { .. })))
}

fn is_note(page: &Page, id: &str) -> bool {
    page.shape(id).is_some_and(|s| matches!(s.sequence, Some(Part::Note { .. })))
}

/// Whether connector `c` can be placed as a message: glued to a lifeline at both ends, or at
/// the shown end of a lost or found message.
fn placeable(page: &Page, c: &Connector) -> bool {
    let Some(m) = &c.message else { return false };
    let (a, b) = (lifeline_of(page, &c.start).is_some(), lifeline_of(page, &c.end).is_some());
    match m.sort {
        Sort::Lost => a,
        Sort::Found => b,
        _ => a && b,
    }
}

/// Make the event list consistent with the page: drop events naming missing items (or messages
/// that cannot be placed), repeats, and fragments whose events do not balance; adopt message
/// connectors and notes no event names; clear fragment parts no event names. Returns whether
/// anything changed.
///
/// Shapes of a lifeline kind with no part join as lifelines (a `shape.add` of the lifeline master,
/// from the Insert menu or an agent), and connectors of one of the five `uml.*Message` types glued
/// to two lifelines join as messages of that sort. Other connectors stay plain drawing.
pub fn reconcile(page: &mut Page) -> bool {
    if page.sequence.is_none() {
        return false;
    }
    let adopted = adopt(page);
    let Some(seq) = page.sequence.as_ref() else { return adopted };
    let before = seq.events.clone();
    let mut events: Vec<Event> = Vec::with_capacity(before.len());
    let mut seen: HashSet<(&'static str, String)> = HashSet::new();
    for e in &before {
        let ok = match e {
            Event::Message(id) => page.connector(id).is_some_and(|c| placeable(page, c)),
            Event::Open(id) | Event::Else(id) | Event::Close(id) => is_fragment(page, id),
            Event::Note(id) => is_note(page, id),
            Event::Activate(id) | Event::Deactivate(id) => is_lifeline(page, id),
        };
        let once = !matches!(e, Event::Else(_) | Event::Activate(_) | Event::Deactivate(_));
        if ok && (!once || seen.insert((e.kind(), e.id().to_string()))) {
            events.push(e.clone());
        }
    }
    // Fragments must open, take their separators while innermost, and close, properly nested.
    let mut stack: Vec<String> = Vec::new();
    let mut broken: HashSet<String> = HashSet::new();
    let mut closed: HashSet<String> = HashSet::new();
    for e in &events {
        match e {
            Event::Open(f) => stack.push(f.clone()),
            Event::Else(f) if stack.last() != Some(f) => {
                broken.insert(f.clone());
            }
            Event::Close(f) => {
                if stack.last() == Some(f) {
                    stack.pop();
                    closed.insert(f.clone());
                } else {
                    broken.insert(f.clone());
                    if let Some(i) = stack.iter().rposition(|x| x == f) {
                        broken.extend(stack.drain(i..));
                    }
                }
            }
            _ => {}
        }
    }
    broken.extend(stack);
    for e in &events {
        if let Event::Open(f) | Event::Else(f) | Event::Close(f) = e
            && !closed.contains(f)
        {
            broken.insert(f.clone());
        }
    }
    events.retain(|e| !matches!(e, Event::Open(f) | Event::Else(f) | Event::Close(f) if broken.contains(f)));
    // Adopt messages and notes in z order.
    let named: HashSet<String> = events.iter().map(|e| e.id().to_string()).collect();
    for c in &page.connectors {
        if events.len() < MAX_EVENTS && !named.contains(&c.id) && placeable(page, c) {
            events.push(Event::Message(c.id.clone()));
        }
    }
    for s in &page.shapes {
        if events.len() < MAX_EVENTS && !named.contains(&s.id) && matches!(s.sequence, Some(Part::Note { .. })) {
            events.push(Event::Note(s.id.clone()));
        }
    }
    let fragments: HashSet<String> = events.iter().filter_map(|e| if let Event::Open(f) = e { Some(f.clone()) } else { None }).collect();
    let mut changed = adopted || events != before;
    for s in &mut page.shapes {
        if matches!(s.sequence, Some(Part::Fragment { .. })) && !fragments.contains(&s.id) {
            s.sequence = None;
            changed = true;
        }
    }
    if let Some(seq) = page.sequence.as_mut() {
        seq.events = events;
    }
    changed
}

/// Give lifeline-kind shapes with no part a lifeline part, and message-type connectors glued to
/// two lifelines a message; returns whether anything changed.
fn adopt(page: &mut Page) -> bool {
    let mut changed = false;
    for s in &mut page.shapes {
        if s.sequence.is_none() && matches!(s.kind, ShapeKind::Lifeline | ShapeKind::ActorLifeline) {
            s.sequence = Some(Part::Lifeline { destroyed: false });
            changed = true;
        }
    }
    let lifelines: HashSet<String> = page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. }))).map(|s| s.id.clone()).collect();
    let on = |e: &End| e.shape().is_some_and(|id| lifelines.contains(id));
    for c in &mut page.connectors {
        if c.message.is_some() || !on(&c.start) || !on(&c.end) {
            continue;
        }
        if let Some(sort) = Sort::from_connector_type(&c.kind) {
            c.message = Some(Message { sort });
            changed = true;
        }
    }
    changed
}

/// The page's lifelines left to right (by head centre, then z order).
fn columns(page: &Page) -> Vec<usize> {
    let mut cols: Vec<usize> =
        page.shapes.iter().enumerate().filter(|(_, s)| matches!(s.sequence, Some(Part::Lifeline { .. }))).map(|(i, _)| i).collect();
    cols.sort_by(|&a, &b| {
        let cx = |i: usize| page.shapes.get(i).map_or(0.0, |s| s.x + s.w / 2.0);
        cx(a).total_cmp(&cx(b)).then(a.cmp(&b))
    });
    cols
}

/// The layout input of a page: lifeline columns (shape indices), the events mapped to them,
/// and per layout event the index of its page event.
struct Mapped {
    cols: Vec<usize>,
    input: SeqInput,
    options: SeqOptions,
    /// Per layout event, the index of the page event it stands for.
    which: Vec<usize>,
}

fn head_size(s: &Shape) -> (f64, f64) {
    let font = s.style.font_size.clamp(6.0, 72.0);
    let tw = text_w(&s.text, font);
    if s.kind == ShapeKind::ActorLifeline { ((tw + 16.0).max(MIN_ACTOR_W), ACTOR_HEAD) } else { ((tw + 24.0).max(MIN_HEAD_W), HEAD) }
}

fn map(page: &Page, keep: bool) -> Mapped {
    let seq = page.sequence.clone().unwrap_or_default();
    let cols = columns(page);
    let col_of: HashMap<&str, usize> = cols.iter().enumerate().filter_map(|(c, &i)| page.shapes.get(i).map(|s| (s.id.as_str(), c))).collect();
    let lifelines: Vec<SeqLifeline> = cols
        .iter()
        .filter_map(|&i| page.shapes.get(i))
        .map(|s| {
            let (w, head) = head_size(s);
            SeqLifeline {
                width: w,
                head,
                actor: s.kind == ShapeKind::ActorLifeline,
                destroyed: matches!(s.sequence, Some(Part::Lifeline { destroyed: true })),
            }
        })
        .collect();
    let col = |id: Option<&str>| id.and_then(|i| col_of.get(i).copied());
    let mut events = Vec::with_capacity(seq.events.len());
    let mut which = Vec::with_capacity(seq.events.len());
    let mut elses: HashMap<&str, usize> = HashMap::new();
    for (ei, e) in seq.events.iter().enumerate() {
        let ev = match e {
            Event::Message(id) => {
                let Some(c) = page.connector(id) else { continue };
                let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
                let (a, b) = (col(c.start.shape()), col(c.end.shape()));
                let (from, to) = match (sort, a, b) {
                    (Sort::Lost, Some(a), _) => (a, a),
                    (Sort::Found, _, Some(b)) => (b, b),
                    (_, Some(a), Some(b)) => (a, b),
                    _ => continue,
                };
                SeqEvent::Message { from, to, sort: sort.layout(), label: label_size(&c.text, c.style.font_size.clamp(6.0, 72.0)) }
            }
            Event::Open(id) => {
                let Some(s @ Shape { sequence: Some(Part::Fragment { operator, operands, covers }), .. }) = page.shape(id) else { continue };
                let guard = operands.first().map(|o| o.guard.as_str()).unwrap_or_default();
                let font = decor_font(s);
                // A ref's text is centred in the frame, which must hold it.
                let label = if *operator == Operator::Ref { text_w(guard, font) } else { 0.0 };
                SeqEvent::Open {
                    forks: operator.forks(),
                    covers: covers.iter().filter_map(|c| col(Some(c))).collect(),
                    label,
                    tab: tab_w(*operator, font),
                    guard: guard_w(*operator, guard, font),
                }
            }
            Event::Else(id) => {
                let n = elses.entry(id.as_str()).or_insert(0);
                *n += 1;
                let n = *n;
                let label = match page.shape(id) {
                    Some(s @ Shape { sequence: Some(Part::Fragment { operator, operands, .. }), .. }) => {
                        operands.get(n).map_or(0.0, |o| guard_w(*operator, &o.guard, decor_font(s)))
                    }
                    _ => 0.0,
                };
                SeqEvent::Else { label }
            }
            Event::Close(_) => SeqEvent::Close,
            Event::Note(id) => {
                let Some(s) = page.shape(id) else { continue };
                let Some(Part::Note { over, side }) = &s.sequence else { continue };
                let over: Vec<usize> = over.iter().filter_map(|o| col(Some(o))).collect();
                if over.is_empty() {
                    continue;
                }
                let side = match side {
                    Side::Over => NoteSide::Over,
                    Side::Left => NoteSide::Left,
                    Side::Right => NoteSide::Right,
                };
                SeqEvent::Note { over, side, size: (s.w.max(20.0), s.h.max(16.0)) }
            }
            Event::Activate(id) => match col(Some(id)) {
                Some(c) => SeqEvent::Activate(c),
                None => continue,
            },
            Event::Deactivate(id) => match col(Some(id)) {
                Some(c) => SeqEvent::Deactivate(c),
                None => continue,
            },
        };
        events.push(ev);
        which.push(ei);
    }
    let keep_columns = keep.then(|| cols.iter().filter_map(|&i| page.shapes.get(i)).map(|s| s.x + s.w / 2.0).collect());
    let options = SeqOptions {
        row: seq.row,
        column_gap: seq.column_gap,
        activation_width: ACTIVATION_W,
        auto_activations: seq.activations == Activations::Auto,
        keep_columns,
    };
    Mapped { cols, input: SeqInput { lifelines, events }, options, which }
}

/// The page y the layout's 0 maps to: the top of the lifelines that are not created by a
/// message (their heads are on the top row).
fn top_of(page: &Page, cols: &[usize]) -> f64 {
    let created: HashSet<&str> =
        page.connectors.iter().filter(|c| c.message.as_ref().is_some_and(|m| m.sort == Sort::Create)).filter_map(|c| c.end.shape()).collect();
    let tops = || cols.iter().filter_map(|&i| page.shapes.get(i));
    tops()
        .filter(|s| !created.contains(s.id.as_str()))
        .map(|s| s.y)
        .reduce(f64::min)
        .or_else(|| tops().map(|s| s.y).reduce(f64::min))
        .unwrap_or(MARGIN)
}

/// Settle every page with an interaction whose interaction or items differ from `before`, then
/// number the generated shapes it added.
pub fn settle_doc(doc: &mut Document, before: &Document) {
    let mut any = false;
    for page in &mut doc.pages {
        if page.sequence.is_none() {
            continue;
        }
        let same = before
            .pages
            .iter()
            .find(|p| p.id == page.id)
            .is_some_and(|p| p.sequence == page.sequence && p.shapes == page.shapes && p.connectors == page.connectors);
        if !same {
            settle_page(page, true);
            any = true;
        }
    }
    if any {
        number_pending(doc);
    }
}

/// Give generated shapes that still carry a pending id a real one.
pub fn number_pending(doc: &mut Document) {
    let pending: Vec<(usize, usize)> = doc
        .pages
        .iter()
        .enumerate()
        .flat_map(|(pi, p)| p.shapes.iter().enumerate().filter(|(_, s)| s.id.starts_with(PENDING)).map(move |(si, _)| (pi, si)))
        .collect();
    for (pi, si) in pending {
        let id = doc.next_id("s");
        if let Some(s) = doc.pages.get_mut(pi).and_then(|p| p.shapes.get_mut(si)) {
            s.id = id;
        }
    }
}

/// Redraw one page from its interaction: reconcile the events, lay them out (keeping the
/// columns when `keep`), and apply the result. New generated shapes get pending ids that
/// [`number_pending`] replaces. Returns the bounds of what was drawn.
pub fn settle_page(page: &mut Page, keep: bool) -> Option<Rect> {
    page.sequence.as_ref()?;
    reconcile(page);
    // Generated shapes whose lifeline went away go with it.
    let lifelines: HashSet<String> = page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. }))).map(|s| s.id.clone()).collect();
    page.shapes.retain(|s| match &s.sequence {
        Some(Part::Activation { lifeline, .. } | Part::Destruction { lifeline }) => lifelines.contains(lifeline),
        _ => true,
    });
    let m = map(page, keep);
    let l = lay::layout(&m.input, &m.options).ok()?;
    let (dx, dy) = if keep { (0.0, top_of(page, &m.cols)) } else { (MARGIN, MARGIN) };
    apply(page, &m, &l, dx, dy);
    let b = l.bounds;
    Some(Rect::new(b.x + dx, b.y + dy, b.x + b.width + dx, b.y + b.height + dy))
}

fn apply(page: &mut Page, m: &Mapped, l: &lay::SeqLayout, dx: f64, dy: f64) {
    let pt = |p: &vizcraft_layout::Point| Point::new(p.x + dx, p.y + dy);
    let seq = page.sequence.clone().unwrap_or_default();
    // Lifeline frames and head text.
    let mut frames: HashMap<String, Rect> = HashMap::new();
    for (c, &si) in m.cols.iter().enumerate() {
        let (Some(head), Some(stem)) = (l.heads.get(c), l.stems.get(c)) else { continue };
        let Some(s) = page.shapes.get_mut(si) else { continue };
        let (x, y, w, h) = (head.x + dx, head.y + dy, head.width, (stem.1 - head.y).max(head.height));
        (s.x, s.y, s.w, s.h) = (x, y, w, h);
        let band = if s.kind == ShapeKind::ActorLifeline { (44.0, 20.0) } else { (0.0, HEAD) };
        s.text_block = Some(TextBlock { dx: 0.0, dy: band.0 + band.1 / 2.0 - h / 2.0, w: w.max(MIN_ACTOR_W), h: band.1, angle_deg: 0.0 });
        s.connection_points = frame_points();
        s.angle_deg = 0.0;
        frames.insert(s.id.clone(), Rect::new(x, y, x + w, y + h));
    }
    // Message ends: one connection point per end, in event order.
    let col_ids: Vec<String> = m.cols.iter().filter_map(|&i| page.shapes.get(i)).map(|s| s.id.clone()).collect();
    let mut fi = 0usize;
    let mut ni = 0usize;
    let mut operand_at: HashMap<String, Vec<f64>> = HashMap::new();
    let mut frag_rects: HashMap<String, Rect> = HashMap::new();
    let mut guard_xs: HashMap<String, f64> = HashMap::new();
    for (k, &ei) in m.which.iter().enumerate() {
        let Some(e) = seq.events.get(ei) else { continue };
        let route = l.routes.get(k).cloned().unwrap_or_default();
        match e {
            Event::Message(id) => {
                let Some(c) = page.connector(id).cloned() else { continue };
                let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
                let (Some(first), Some(last)) = (route.first().map(pt), route.last().map(pt)) else { continue };
                let start = match c.start.shape() {
                    _ if sort == Sort::Found => End::free(first),
                    Some(sid) => add_point(page, &frames, sid, first),
                    None => c.start.clone(),
                };
                let end = match c.end.shape() {
                    _ if sort == Sort::Lost => End::free(last),
                    Some(sid) => add_point(page, &frames, sid, last),
                    None => c.end.clone(),
                };
                if let Some(cm) = page.connector_mut(id) {
                    cm.start = start;
                    cm.end = end;
                    cm.route = Route::Straight;
                    cm.avoid = false;
                    cm.waypoints = route.iter().skip(1).take(route.len().saturating_sub(2)).map(pt).collect();
                }
            }
            Event::Open(id) => {
                let r = l.fragments.get(fi).copied();
                let seps = l.separators.get(fi).cloned().unwrap_or_default();
                let gx = l.guards.get(fi).copied();
                fi += 1;
                if let Some(r) = r {
                    frag_rects.insert(id.clone(), Rect::new(r.x + dx, r.y + dy, r.x + r.width + dx, r.y + r.height + dy));
                    if let Some(gx) = gx {
                        guard_xs.insert(id.clone(), gx - r.x);
                    }
                    operand_at.insert(id.clone(), seps.iter().map(|s| s - r.y).collect());
                }
            }
            Event::Note(id) => {
                if let Some(r) = l.notes.get(ni).copied()
                    && let Some(s) = page.shape_mut(id)
                {
                    (s.x, s.y, s.w, s.h) = (r.x + dx, r.y + dy, r.width, r.height);
                }
                ni += 1;
            }
            _ => {}
        }
    }
    // Fragment frames and operands.
    for (id, r) in &frag_rects {
        let Some(s) = page.shape_mut(id) else { continue };
        (s.x, s.y, s.w, s.h) = (r.x0, r.y0, r.width(), r.height());
        s.angle_deg = 0.0;
        if let Some(Part::Fragment { operands, .. }) = &mut s.sequence {
            let ats = operand_at.get(id).cloned().unwrap_or_default();
            while operands.len() < ats.len() + 1 && operands.len() < MAX_OPERANDS {
                operands.push(Operand::default());
            }
            if let Some(first) = operands.first_mut() {
                first.at = 0.0;
            }
            for (o, at) in operands.iter_mut().skip(1).zip(ats.iter().chain(std::iter::repeat(&0.0))) {
                o.at = *at;
            }
        }
        fragment_look(s);
        if let (Some(c), Some(gx)) = (s.container.as_mut(), guard_xs.get(id)) {
            c.margin = gx.clamp(0.0, Container::MAX_BAND);
        }
    }
    // Generated shapes, reused per lifeline in order.
    let mut old_bars: HashMap<String, Vec<Shape>> = HashMap::new();
    let mut old_marks: HashMap<String, Shape> = HashMap::new();
    let mut rest: Vec<Shape> = Vec::with_capacity(page.shapes.len());
    for s in std::mem::take(&mut page.shapes) {
        match &s.sequence {
            Some(Part::Activation { lifeline, .. }) => old_bars.entry(lifeline.clone()).or_default().push(s),
            Some(Part::Destruction { lifeline }) => {
                old_marks.insert(lifeline.clone(), s);
            }
            _ => rest.push(s),
        }
    }
    let mut pending = 0usize;
    let mut fresh = |base: Option<Shape>| -> Shape {
        base.unwrap_or_else(|| {
            pending += 1;
            Shape { id: format!("{PENDING}{pending}"), ..Shape::default() }
        })
    };
    let mut bars: Vec<Shape> = Vec::new();
    for a in &l.activations {
        let Some(lid) = col_ids.get(a.lifeline).cloned() else { continue };
        let Some(&cx) = l.columns.get(a.lifeline) else { continue };
        let base = old_bars.get_mut(&lid).and_then(|v| if v.is_empty() { None } else { Some(v.remove(0)) });
        let mut s = fresh(base);
        let x = cx + dx - ACTIVATION_W / 2.0 + (a.depth.saturating_sub(1)) as f64 * ACTIVATION_W / 2.0;
        bar_look(&mut s, &lid, a.depth, x, a.y0 + dy, ACTIVATION_W, (a.y1 - a.y0).max(1.0));
        bars.push(s);
    }
    let mut marks: Vec<Shape> = Vec::new();
    let destroyed_by_message: HashSet<String> = page
        .connectors
        .iter()
        .filter(|c| c.message.as_ref().is_some_and(|m| m.sort == Sort::Destroy))
        .filter_map(|c| c.end.shape().map(str::to_string))
        .collect();
    for (c, lid) in col_ids.iter().enumerate() {
        let Some(s) = rest.iter().find(|s| &s.id == lid) else { continue };
        let destroyed = matches!(s.sequence, Some(Part::Lifeline { destroyed: true })) || destroyed_by_message.contains(&s.id);
        if !destroyed {
            continue;
        }
        let (Some(&cx), Some(stem)) = (l.columns.get(c), l.stems.get(c)) else { continue };
        let lid = s.id.clone();
        let mut mk = fresh(old_marks.remove(&lid));
        mark_look(&mut mk, &lid, cx + dx - CROSS / 2.0, stem.1 + dy - CROSS / 2.0);
        marks.push(mk);
    }
    // Z order: other shapes, fragments, lifelines, bars, marks, notes.
    let rank = |s: &Shape| match &s.sequence {
        None => 0,
        Some(Part::Fragment { .. }) => 1,
        Some(Part::Lifeline { .. }) => 2,
        Some(Part::Activation { .. }) => 3,
        Some(Part::Destruction { .. }) => 4,
        Some(Part::Note { .. }) => 5,
    };
    let mut all = rest;
    all.extend(bars);
    all.extend(marks);
    all.sort_by_key(|s| rank(s));
    for s in &mut all {
        s.sanitize();
    }
    page.shapes = all;
}

/// Glue point for a message end at world point `p` on lifeline `sid`: a new stored connection
/// point, returned as point glue.
fn add_point(page: &mut Page, frames: &HashMap<String, Rect>, sid: &str, p: Point) -> End {
    let Some(f) = frames.get(sid) else { return End::free(p) };
    let Some(s) = page.shape_mut(sid) else { return End::free(p) };
    let w = f.width().max(1e-9);
    let h = f.height().max(1e-9);
    s.connection_points.push(Point::new(((p.x - f.x0) / w).clamp(0.0, 1.0), ((p.y - f.y0) / h).clamp(0.0, 1.0)));
    End::point(sid, s.connection_points.len().saturating_sub(1))
}

fn locked() -> Locks {
    Locks { width: true, height: true, position: true, delete: true, rotation: true, ..Locks::default() }
}

fn bar_look(s: &mut Shape, lifeline: &str, depth: usize, x: f64, y: f64, w: f64, h: f64) {
    s.kind = ShapeKind::Rectangle;
    s.stencil = "uml.activation".into();
    (s.x, s.y, s.w, s.h, s.angle_deg) = (x, y, w, h, 0.0);
    s.text.clear();
    s.style.fill = Color::rgb(0xff, 0xff, 0xff);
    s.style.line = Color::rgb(0x2b, 0x3a, 0x4a);
    s.style.line_width = 1.0;
    s.locks = locked();
    s.connection_points = Vec::new();
    s.sequence = Some(Part::Activation { lifeline: lifeline.into(), depth });
}

fn mark_look(s: &mut Shape, lifeline: &str, x: f64, y: f64) {
    s.kind = ShapeKind::Freeform;
    s.stencil = "uml.destruction".into();
    (s.x, s.y, s.w, s.h, s.angle_deg) = (x, y, CROSS, CROSS, 0.0);
    s.text.clear();
    let mut p = Path::new();
    p.move_to(Point::new(0.0, 0.0)).line_to(Point::new(CROSS, CROSS));
    p.move_to(Point::new(CROSS, 0.0)).line_to(Point::new(0.0, CROSS));
    s.path = Some(Freeform::new(&p, false));
    s.style.line = Color::rgb(0x2b, 0x3a, 0x4a);
    s.style.line_width = 1.5;
    s.locks = locked();
    s.sequence = Some(Part::Destruction { lifeline: lifeline.into() });
}

/// The container look of a fragment: locked, transparent, no heading. Settle keeps the x its
/// guards start at (right of the first covered stem and its bars) in the container's margin.
pub fn fragment_look(s: &mut Shape) {
    let c = s.container.get_or_insert_with(Container::default);
    c.locked = true;
    c.heading_size = 0.0;
    c.heading_style = HeadingStyle::Plain;
    c.auto_fit = false;
    c.band = Color::TRANSPARENT;
    s.style.fill = Color::TRANSPARENT;
    if s.stencil.is_empty() {
        s.stencil = "uml.fragment".into();
    }
}

/// Lay the page's interaction out from scratch (columns packed from the left margin), as
/// `layout.primitive {primitive: "sequence"}` does on a Sequence page. New generated shapes
/// carry pending ids; the caller numbers them with [`number_pending`] (the engine's settle does).
pub fn relayout(page: &mut Page) -> Result<PrimitiveOutcome, String> {
    if page.sequence.is_none() {
        return Err("the page has no interaction (sequence.new adds one)".into());
    }
    reconcile(page);
    let m = map(page, false);
    lay::layout(&m.input, &m.options).map_err(|e| e.to_string())?;
    let bounds = settle_page(page, false);
    let ids = columns(page).iter().filter_map(|&i| page.shapes.get(i)).map(|s| s.id.clone()).collect();
    Ok(PrimitiveOutcome { ids, notes: Vec::new(), bounds })
}

/// The y of every event as the page is drawn, with its item id, top to bottom: what a drag
/// in the canvas compares against to find a drop position.
pub fn rows(page: &Page) -> Vec<(String, f64)> {
    let Some(seq) = &page.sequence else { return Vec::new() };
    let m = map(page, true);
    let Ok(l) = lay::layout(&m.input, &m.options) else { return Vec::new() };
    let dy = top_of(page, &m.cols);
    m.which.iter().zip(&l.rows).filter_map(|(&ei, y)| seq.events.get(ei).map(|e| (e.id().to_string(), y + dy))).collect()
}

/// Per page event, the y it is drawn at (`None` for one the layout skips).
fn event_rows(page: &Page) -> Vec<Option<f64>> {
    let Some(seq) = &page.sequence else { return Vec::new() };
    let mut out = vec![None; seq.events.len()];
    let m = map(page, true);
    let Ok(l) = lay::layout(&m.input, &m.options) else { return out };
    let dy = top_of(page, &m.cols);
    for (&ei, y) in m.which.iter().zip(&l.rows) {
        if let Some(slot) = out.get_mut(ei) {
            *slot = Some(y + dy);
        }
    }
    out
}

fn is_activation(e: &Event) -> bool {
    matches!(e, Event::Activate(_) | Event::Deactivate(_))
}

/// The operand of fragment `f` that event index `i` lies in: the separators of `f` before it.
fn operand_at(events: &[Event], f: &str, i: usize) -> usize {
    let start = block_of(events, f).map_or(0, |r| r.start);
    events.iter().take(i.saturating_add(1)).skip(start).filter(|e| matches!(e, Event::Else(x) if x == f)).count()
}

/// Where a block dropped at page `y` goes, and the y of the insertion line to draw. The events of
/// `skip`'s block (the one being dragged) are passed over; activation events are stepped over. A
/// drop above a separator or a fragment's bottom ends that operand; a drop below everything goes
/// after the last top-level block. `None` when the page has no interaction.
pub fn drop_position(page: &Page, y: f64, skip: Option<&str>) -> Option<(Position, f64)> {
    let seq = page.sequence.as_ref()?;
    let events = &seq.events;
    let rows = event_rows(page);
    let skipped = skip.and_then(|id| block_of(events, id)).unwrap_or(0..0);
    let row = seq.row;
    let mut prev: Option<f64> = None;
    for (i, e) in events.iter().enumerate() {
        if skipped.contains(&i) || is_activation(e) {
            continue;
        }
        let Some(ey) = rows.get(i).copied().flatten() else { continue };
        if ey > y {
            let line = prev.map_or(ey - row / 2.0, |p| (p + ey) / 2.0);
            let pos = match e {
                Event::Message(id) | Event::Note(id) | Event::Open(id) => Position::Before(id.clone()),
                Event::Else(f) => Position::In { fragment: f.clone(), operand: operand_at(events, f, i).saturating_sub(1) },
                Event::Close(f) => Position::In { fragment: f.clone(), operand: operand_at(events, f, i) },
                Event::Activate(_) | Event::Deactivate(_) => continue,
            };
            return Some((pos, line));
        }
        prev = Some(ey);
    }
    // Below everything: after the last top-level block that is not the one dragged.
    let mut depth = 0usize;
    let mut last: Option<&str> = None;
    for (i, e) in events.iter().enumerate() {
        match e {
            Event::Open(f) => {
                if depth == 0 && !skipped.contains(&i) {
                    last = Some(f);
                }
                depth += 1;
            }
            Event::Close(_) => depth = depth.saturating_sub(1),
            Event::Message(id) | Event::Note(id) if depth == 0 && !skipped.contains(&i) => last = Some(id),
            _ => {}
        }
    }
    let line = prev.map_or(y, |p| p + row / 2.0);
    Some((last.map_or(Position::End, |l| Position::After(l.to_string())), line))
}

/// Where the block of `id` goes one step up or down: past the neighbouring sibling block (a
/// fragment is stepped over whole); at an operand's edge, into the neighbouring operand; at the
/// first or last operand, out of the fragment. Activation events are stepped over.
pub fn step(events: &[Event], id: &str, down: bool) -> Result<Position, String> {
    let r = block_of(events, id).ok_or_else(|| format!("`{id}` is not a message, note or fragment of the interaction"))?;
    if down {
        let Some((j, e)) = events.iter().enumerate().skip(r.end).find(|(_, e)| !is_activation(e)) else {
            return Err(format!("`{id}` is already last"));
        };
        Ok(match e {
            Event::Message(x) | Event::Note(x) | Event::Open(x) => Position::After(x.clone()),
            Event::Close(f) => Position::After(f.clone()),
            Event::Else(f) => {
                let k = operand_at(events, f, j);
                match events.iter().skip(j + 1).find(|e| !is_activation(e)) {
                    Some(Event::Message(x) | Event::Note(x) | Event::Open(x)) => Position::Before(x.clone()),
                    _ => Position::In { fragment: f.clone(), operand: k },
                }
            }
            Event::Activate(_) | Event::Deactivate(_) => return Err(format!("`{id}` is already last")),
        })
    } else {
        let Some((j, e)) = events.iter().enumerate().take(r.start).rev().find(|(_, e)| !is_activation(e)) else {
            return Err(format!("`{id}` is already first"));
        };
        Ok(match e {
            Event::Message(x) | Event::Note(x) => Position::Before(x.clone()),
            Event::Close(f) | Event::Open(f) => Position::Before(f.clone()),
            Event::Else(f) => Position::In { fragment: f.clone(), operand: operand_at(events, f, j).saturating_sub(1) },
            Event::Activate(_) | Event::Deactivate(_) => return Err(format!("`{id}` is already first")),
        })
    }
}

/// The innermost fragment around the block of `id`, and the operand it lies in.
pub fn enclosing(events: &[Event], id: &str) -> Option<(String, usize)> {
    let r = block_of(events, id)?;
    let mut stack: Vec<(String, usize)> = Vec::new();
    for e in events.iter().take(r.start) {
        match e {
            Event::Open(f) => stack.push((f.clone(), 0)),
            Event::Else(f) => {
                if let Some(top) = stack.last_mut().filter(|t| &t.0 == f) {
                    top.1 += 1;
                }
            }
            Event::Close(_) => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack.pop()
}

/// The lifeline under `p`: its head or stem (within `tol`), or one of its activation bars or its
/// destruction mark.
pub fn lifeline_at(page: &Page, p: Point, tol: f64) -> Option<String> {
    let tol = if tol.is_finite() { tol.clamp(0.0, 1000.0) } else { 0.0 };
    page.shapes.iter().rev().find_map(|s| match &s.sequence {
        Some(Part::Lifeline { .. }) if stem_hit(s, p, tol) => Some(s.id.clone()),
        Some(Part::Activation { lifeline, .. } | Part::Destruction { lifeline })
            if s.bounds().inflate(tol).contains(p) && is_lifeline(page, lifeline) =>
        {
            Some(lifeline.clone())
        }
        _ => None,
    })
}

/// The shape a click on `id` picks: an activation bar or destruction mark picks its lifeline,
/// anything else itself.
pub fn owner(page: &Page, id: &str) -> String {
    match page.shape(id).and_then(|s| s.sequence.as_ref()) {
        Some(Part::Activation { lifeline, .. } | Part::Destruction { lifeline }) if is_lifeline(page, lifeline) => lifeline.clone(),
        _ => id.to_string(),
    }
}

/// The column a lifeline dropped at page `x` takes: how many lifelines have their centre left of it.
pub fn column_at(page: &Page, x: f64) -> usize {
    columns(page).iter().filter_map(|&i| page.shapes.get(i)).filter(|s| s.x + s.w / 2.0 < x).count()
}

/// The block that starts operand `operand` (1 or more) of `fragment` when its separator is dragged
/// to page `y`: the first block directly inside the fragment drawn below `y`, from the second
/// block of the operand before it to the last block of the operand itself (the neighbouring
/// separators bound it). `None` when there is no such block.
pub fn separator_target(page: &Page, fragment: &str, operand: usize, y: f64) -> Option<String> {
    let seq = page.sequence.as_ref()?;
    let events = &seq.events;
    let r = block_of(events, fragment).filter(|r| matches!(events.get(r.start), Some(Event::Open(_))))?;
    if operand == 0 {
        return None;
    }
    let rows = event_rows(page);
    let inner = depth_at(events, r.start) + 1;
    // Blocks directly inside, with their operand and row.
    let mut blocks: Vec<(String, usize, f64)> = Vec::new();
    let mut k = 0usize;
    for i in r.clone().skip(1) {
        match events.get(i) {
            Some(Event::Else(f)) if f == fragment => k += 1,
            Some(Event::Message(x) | Event::Note(x) | Event::Open(x)) if depth_at(events, i) == inner => {
                if let Some(ey) = rows.get(i).copied().flatten() {
                    blocks.push((x.clone(), k, ey));
                }
            }
            _ => {}
        }
    }
    let prev_first = blocks.iter().position(|b| b.1 == operand - 1);
    let candidates: Vec<&(String, usize, f64)> =
        blocks.iter().enumerate().filter(|(i, b)| (b.1 == operand - 1 && Some(*i) != prev_first) || b.1 == operand).map(|(_, b)| b).collect();
    candidates.iter().find(|b| b.2 > y).or(candidates.last()).map(|b| b.0.clone())
}

/// Rename item ids in a page's interaction and parts after a copy: `shapes` maps old shape ids
/// to new ones, `connectors` old connector ids to new ones.
pub fn rename_ids(page: &mut Page, shapes: &HashMap<String, String>, connectors: &HashMap<String, String>) {
    let sh = |id: &mut String| {
        if let Some(n) = shapes.get(id.as_str()) {
            *id = n.clone();
        }
    };
    if let Some(seq) = page.sequence.as_mut() {
        for e in &mut seq.events {
            let map = if matches!(e, Event::Message(_)) { connectors } else { shapes };
            let id = e.id_mut();
            if let Some(n) = map.get(id.as_str()) {
                *id = n.clone();
            }
        }
    }
    for s in &mut page.shapes {
        match &mut s.sequence {
            Some(Part::Activation { lifeline, .. } | Part::Destruction { lifeline }) => sh(lifeline),
            Some(Part::Fragment { covers, .. }) => covers.iter_mut().for_each(sh),
            Some(Part::Note { over, .. }) => over.iter_mut().for_each(sh),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- editing

/// Where an event block goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    /// Before the block of this item.
    Before(String),
    /// After the block of this item.
    After(String),
    /// At the end of operand `operand` (0 based) of fragment `fragment`.
    In {
        /// The fragment shape.
        fragment: String,
        /// The operand.
        operand: usize,
    },
    /// At the end of the interaction.
    End,
}

/// The events a block spans: a message's or note's one event, a fragment's events from its
/// open to its close.
pub fn block_of(events: &[Event], id: &str) -> Option<std::ops::Range<usize>> {
    let first = events.iter().position(|e| e.id() == id && !matches!(e, Event::Activate(_) | Event::Deactivate(_)))?;
    match events.get(first) {
        Some(Event::Open(_)) => {
            let close = events.iter().skip(first).position(|e| matches!(e, Event::Close(f) if f == id))?;
            Some(first..first + close + 1)
        }
        Some(Event::Message(_) | Event::Note(_)) => Some(first..first + 1),
        _ => None,
    }
}

/// The nesting depth at event `i`: how many fragments are open around it.
fn depth_at(events: &[Event], i: usize) -> usize {
    let mut d: usize = 0;
    for e in events.iter().take(i) {
        match e {
            Event::Open(_) => d += 1,
            Event::Close(_) => d = d.saturating_sub(1),
            _ => {}
        }
    }
    d
}

/// The index an insertion at `pos` lands at.
pub fn index_for(events: &[Event], pos: &Position) -> Result<usize, String> {
    match pos {
        Position::End => Ok(events.len()),
        Position::Before(id) => {
            block_of(events, id).map(|r| r.start).ok_or_else(|| format!("`{id}` is not a message, note or fragment of the interaction"))
        }
        Position::After(id) => {
            block_of(events, id).map(|r| r.end).ok_or_else(|| format!("`{id}` is not a message, note or fragment of the interaction"))
        }
        Position::In { fragment, operand } => {
            let r = block_of(events, fragment)
                .filter(|r| matches!(events.get(r.start), Some(Event::Open(_))))
                .ok_or_else(|| format!("`{fragment}` is not a fragment of the interaction"))?;
            // The operand ends at the next separator of this fragment, or at its close.
            let mut k = 0usize;
            for i in r.clone().skip(1) {
                match events.get(i) {
                    Some(Event::Else(f) | Event::Close(f)) if f == fragment => {
                        if k == *operand {
                            return Ok(i);
                        }
                        k += 1;
                    }
                    _ => {}
                }
            }
            Err(format!("fragment `{fragment}` has no operand {operand}"))
        }
    }
}

/// Move the block of `id` to `pos`. A block cannot move into itself.
pub fn move_block(events: &mut Vec<Event>, id: &str, pos: &Position) -> Result<(), String> {
    let r = block_of(events, id).ok_or_else(|| format!("`{id}` is not a message, note or fragment of the interaction"))?;
    let target = index_for(events, pos)?;
    let anchor = match pos {
        Position::Before(a) | Position::After(a) => Some(a.as_str()),
        Position::In { fragment, .. } => Some(fragment.as_str()),
        Position::End => None,
    };
    if anchor == Some(id) || (target > r.start && target < r.end) {
        return Err(format!("`{id}` cannot move into itself"));
    }
    let block: Vec<Event> = events.drain(r.clone()).collect();
    let at = if target >= r.end { target - block.len() } else { target };
    let tail = events.split_off(at.min(events.len()));
    events.extend(block);
    events.extend(tail);
    Ok(())
}

/// Wrap the blocks of `ids` in fragment `fragment`: they must be contiguous at one nesting
/// level. Returns where the fragment opens.
pub fn wrap(events: &mut Vec<Event>, ids: &[String], fragment: &str) -> Result<usize, String> {
    if ids.is_empty() {
        return Err("`ids` names nothing to wrap".into());
    }
    let mut ranges: Vec<(std::ops::Range<usize>, &str)> = Vec::new();
    for id in ids {
        let r = block_of(events, id).ok_or_else(|| format!("`{id}` is not a message, note or fragment of the interaction"))?;
        if !ranges.iter().any(|(q, _)| *q == r) {
            ranges.push((r, id));
        }
    }
    ranges.sort_by_key(|(r, _)| r.start);
    let depth = ranges.first().map_or(0, |(r, _)| depth_at(events, r.start));
    for (r, id) in &ranges {
        if depth_at(events, r.start) != depth {
            return Err(format!("`{id}` is at another nesting level"));
        }
    }
    for pair in ranges.windows(2) {
        let [(a, ai), (b, bi)] = pair else { continue };
        if b.start < a.end {
            return Err(format!("`{bi}` is inside `{ai}`"));
        }
        // Only activations may sit between two wrapped blocks.
        if let Some(gap) = (a.end..b.start).find(|&i| !matches!(events.get(i), Some(Event::Activate(_) | Event::Deactivate(_)))) {
            let what = events.get(gap).map(|e| e.id().to_string()).unwrap_or_default();
            return Err(format!("the ids are not contiguous: `{what}` lies between `{ai}` and `{bi}`"));
        }
    }
    let (Some((first, _)), Some((last, _))) = (ranges.first(), ranges.last()) else { return Err("nothing to wrap".into()) };
    let (start, end) = (first.start, last.end);
    events.insert(end, Event::Close(fragment.to_string()));
    events.insert(start, Event::Open(fragment.to_string()));
    Ok(start)
}

/// Remove fragment `fragment`'s open, separators and close, keeping its content.
pub fn unwrap(events: &mut Vec<Event>, fragment: &str) -> bool {
    let before = events.len();
    events.retain(|e| !matches!(e, Event::Open(f) | Event::Else(f) | Event::Close(f) if f == fragment));
    events.len() != before
}

/// Start a new operand of `fragment` at the block of `before`, which must be directly inside
/// it. Returns the new operand's index.
pub fn add_operand(events: &mut Vec<Event>, fragment: &str, before: &str) -> Result<usize, String> {
    let f = block_of(events, fragment)
        .filter(|r| matches!(events.get(r.start), Some(Event::Open(_))))
        .ok_or_else(|| format!("`{fragment}` is not a fragment of the interaction"))?;
    let b = block_of(events, before).ok_or_else(|| format!("`{before}` is not a message, note or fragment of the interaction"))?;
    if b.start <= f.start || b.end >= f.end || depth_at(events, b.start) != depth_at(events, f.start) + 1 {
        return Err(format!("`{before}` is not directly inside `{fragment}`"));
    }
    if matches!(b.start.checked_sub(1).and_then(|i| events.get(i)), Some(Event::Else(x) | Event::Open(x)) if x == fragment) {
        return Err(format!("`{before}` already starts an operand of `{fragment}`"));
    }
    let k = events.iter().take(b.start).skip(f.start).filter(|e| matches!(e, Event::Else(x) if x == fragment)).count() + 1;
    events.insert(b.start, Event::Else(fragment.to_string()));
    Ok(k)
}

/// Remove operand `operand` of `fragment` (its separator; operand 0 merges into the next one).
/// Returns the separator index removed among the fragment's separators.
pub fn remove_operand(events: &mut Vec<Event>, fragment: &str, operand: usize) -> Result<usize, String> {
    let seps: Vec<usize> = events.iter().enumerate().filter(|(_, e)| matches!(e, Event::Else(f) if f == fragment)).map(|(i, _)| i).collect();
    if seps.is_empty() {
        return Err(format!("fragment `{fragment}` has one operand"));
    }
    let k = operand.saturating_sub(1);
    let Some(&i) = seps.get(k) else { return Err(format!("fragment `{fragment}` has no operand {operand}")) };
    events.remove(i);
    Ok(k)
}

// ---------------------------------------------------------------- checks

/// A problem found by [`check_order`] or [`check_replies`]: what is wrong and the page items it
/// is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// What is wrong, in a few words.
    pub what: String,
    /// The connectors and shapes involved.
    pub items: Vec<String>,
}

fn label_of(page: &Page, id: &str) -> String {
    let t = page.connector(id).map(|c| c.text.as_str()).or_else(|| page.shape(id).map(|s| s.text.as_str())).unwrap_or_default();
    let t: String = t.split('\n').next().unwrap_or_default().chars().take(40).collect();
    if t.trim().is_empty() { id.to_string() } else { format!("\"{}\"", t.trim()) }
}

/// The world y a message starts at, as drawn.
fn message_y(page: &Page, c: &Connector) -> Option<f64> {
    match &c.start {
        End::Free { y, .. } => Some(*y),
        End::Glued { shape, point, .. } => {
            let s = page.shape(shape)?;
            match point {
                Some(i) => s.connection_point_world(*i).map(|p| p.y),
                None => Some(s.center().y),
            }
        }
    }
}

/// Order problems on a page with an interaction: a message drawn above an earlier one or on its
/// row; a message to a created lifeline before its create message, or any message after a
/// lifeline's destroy; an unbalanced fragment or an operand separator in a fragment whose
/// operator takes one operand; a message drawn outside the operand it is listed in.
pub fn check_order(page: &Page) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(seq) = &page.sequence else { return out };
    // Rows as drawn.
    let mut prev: Option<(f64, &str)> = None;
    for e in &seq.events {
        let Event::Message(id) = e else { continue };
        let Some(c) = page.connector(id) else { continue };
        let Some(y) = message_y(page, c) else { continue };
        if let Some((py, pid)) = prev {
            if y < py - 0.5 {
                out.push(Finding {
                    what: format!("message {} is drawn above {}, which comes before it", label_of(page, id), label_of(page, pid)),
                    items: vec![id.clone(), pid.to_string()],
                });
            } else if (y - py).abs() <= 0.5 {
                out.push(Finding {
                    what: format!("message {} is drawn on the row of {}", label_of(page, id), label_of(page, pid)),
                    items: vec![id.clone(), pid.to_string()],
                });
            }
        }
        prev = Some((y, id.as_str()));
    }
    // Creation and destruction.
    let created: HashSet<&str> = seq
        .events
        .iter()
        .filter_map(|e| if let Event::Message(id) = e { page.connector(id) } else { None })
        .filter(|c| c.message.as_ref().is_some_and(|m| m.sort == Sort::Create))
        .filter_map(|c| c.end.shape())
        .collect();
    let mut born: HashSet<&str> = HashSet::new();
    let mut dead: HashMap<&str, &str> = HashMap::new();
    for e in &seq.events {
        let Event::Message(id) = e else { continue };
        let Some(c) = page.connector(id) else { continue };
        let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
        for l in [c.start.shape(), c.end.shape()].into_iter().flatten() {
            if let Some(d) = dead.get(l) {
                out.push(Finding {
                    what: format!("message {} comes after {} destroyed its lifeline", label_of(page, id), label_of(page, d)),
                    items: vec![id.clone(), l.to_string()],
                });
            } else if created.contains(l) && !born.contains(l) && !(sort == Sort::Create && c.end.shape() == Some(l)) {
                out.push(Finding {
                    what: format!("message {} reaches a lifeline before its create message", label_of(page, id)),
                    items: vec![id.clone(), l.to_string()],
                });
            }
        }
        if let Some(t) = c.end.shape() {
            match sort {
                Sort::Create => {
                    born.insert(t);
                }
                Sort::Destroy => {
                    dead.insert(t, id.as_str());
                }
                _ => {}
            }
        }
    }
    // Fragment balance and operands.
    let mut stack: Vec<&str> = Vec::new();
    let operator_of = |f: &str| match page.shape(f).and_then(|s| s.sequence.as_ref()) {
        Some(Part::Fragment { operator, .. }) => Some(*operator),
        _ => None,
    };
    // Per message, the innermost operand it is listed in: (fragment, operand index).
    let mut operand: Vec<(String, usize)> = Vec::new();
    let mut counters: HashMap<&str, usize> = HashMap::new();
    let mut listed: Vec<(&str, Option<(String, usize)>)> = Vec::new();
    for e in &seq.events {
        match e {
            Event::Open(f) => {
                if stack.contains(&f.as_str()) {
                    out.push(Finding { what: format!("fragment {f} opens twice"), items: vec![f.clone()] });
                }
                stack.push(f);
                operand.push((f.clone(), 0));
                counters.insert(f, 0);
            }
            Event::Else(f) => {
                if stack.last() != Some(&f.as_str()) {
                    out.push(Finding { what: format!("an operand separator of fragment {f} is outside it"), items: vec![f.clone()] });
                    continue;
                }
                if let Some(op) = operator_of(f).filter(|o| !o.allows_else()) {
                    out.push(Finding { what: format!("a {} fragment takes one operand", op.name()), items: vec![f.clone()] });
                }
                let k = counters.entry(f).or_insert(0);
                *k += 1;
                if let Some(last) = operand.last_mut() {
                    last.1 = *k;
                }
            }
            Event::Close(f) => {
                if stack.last() == Some(&f.as_str()) {
                    stack.pop();
                    operand.pop();
                } else {
                    out.push(Finding { what: format!("fragment {f} closes out of turn"), items: vec![f.clone()] });
                }
            }
            Event::Message(id) => listed.push((id, operand.last().cloned())),
            _ => {}
        }
    }
    for f in stack {
        out.push(Finding { what: format!("fragment {f} never closes"), items: vec![f.to_string()] });
    }
    // Messages drawn outside their operand.
    for (id, op) in listed {
        let (Some(c), Some((f, k))) = (page.connector(id), op) else { continue };
        let Some(Shape { y, h, sequence: Some(Part::Fragment { operands, .. }), .. }) = page.shape(&f) else { continue };
        let Some(my) = message_y(page, c) else { continue };
        let top = y + operands.get(k).map_or(0.0, |o| o.at);
        let bottom = operands.get(k + 1).map_or(y + h, |o| y + o.at);
        if my < top - 0.5 || my > bottom + 0.5 {
            out.push(Finding {
                what: format!("message {} is drawn outside the operand it belongs to", label_of(page, id)),
                items: vec![id.to_string(), f],
            });
        }
    }
    out
}

/// Reply problems on a page with an interaction, operands taken as separate paths: a reply with
/// no open synchronous call from its receiver to its sender, a call answered twice on one path,
/// and with `require_replies` a call never answered.
pub fn check_replies(page: &Page, require_replies: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(seq) = &page.sequence else { return out };
    #[derive(Clone, Default)]
    struct State {
        /// Open calls: (caller, callee, message id).
        open: Vec<(String, String, String)>,
        /// Calls answered on this path: (caller, callee, message id).
        answered: Vec<(String, String, String)>,
    }
    let mut st = State::default();
    let mut snaps: Vec<Option<State>> = Vec::new();
    let mut unanswered: Vec<String> = Vec::new();
    let mut reported: HashSet<String> = HashSet::new();
    let flag_open = |s: &State, since: Option<&State>, unanswered: &mut Vec<String>| {
        for (_, _, m) in &s.open {
            if since.is_none_or(|b| !b.open.iter().any(|(_, _, x)| x == m)) && !unanswered.contains(m) {
                unanswered.push(m.clone());
            }
        }
    };
    for e in &seq.events {
        match e {
            Event::Open(f) => {
                let forks = matches!(page.shape(f).and_then(|s| s.sequence.as_ref()), Some(Part::Fragment { operator, .. }) if operator.forks());
                snaps.push(forks.then(|| st.clone()));
            }
            Event::Else(_) => {
                if let Some(Some(snap)) = snaps.last() {
                    flag_open(&st, Some(snap), &mut unanswered);
                    st = snap.clone();
                }
            }
            Event::Close(_) => {
                snaps.pop();
            }
            Event::Message(id) => {
                let Some(c) = page.connector(id) else { continue };
                let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
                match c.message.as_ref().map(|m| m.sort).unwrap_or_default() {
                    Sort::Sync => st.open.push((a.to_string(), b.to_string(), id.clone())),
                    Sort::Reply => {
                        if let Some(pos) = st.open.iter().rposition(|(caller, callee, _)| caller == b && callee == a) {
                            // Calls nested in the answered one that are still open stay unanswered.
                            let mut drained: Vec<(String, String, String)> = st.open.drain(pos..).collect();
                            for (_, _, m) in drained.iter().skip(1) {
                                if !unanswered.contains(m) {
                                    unanswered.push(m.clone());
                                }
                            }
                            drained.truncate(1);
                            st.answered.extend(drained);
                        } else if reported.insert(id.clone()) {
                            let twice = st.answered.iter().rev().find(|(caller, callee, _)| caller == b && callee == a);
                            match twice {
                                Some((_, _, call)) => out.push(Finding {
                                    what: format!("call {} is answered twice: {} answers it again", label_of(page, call), label_of(page, id)),
                                    items: vec![id.clone(), call.clone()],
                                }),
                                None => {
                                    out.push(Finding { what: format!("reply {} answers no open call", label_of(page, id)), items: vec![id.clone()] })
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if require_replies {
        flag_open(&st, None, &mut unanswered);
        for m in unanswered {
            out.push(Finding { what: format!("call {} is never answered", label_of(page, &m)), items: vec![m] });
        }
    }
    out
}

#[cfg(test)]
#[path = "sequence_tests.rs"]
mod tests;
