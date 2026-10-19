//! UML sequence diagram layout: lifeline columns left to right, one row per event down the page,
//! activation bars derived from calls and replies, and combined fragments that span the
//! lifelines they cover.
//!
//! The input is plain geometry and an ordered event list (lifelines by index); the output is
//! positions for everything the diagram draws. Pure and deterministic: identical input gives
//! identical output, every loop is bounded, and bad input is a [`LayoutError`], never a panic.
//!
//! Columns are packed left to right with `column_gap` between heads, then pushed apart where a
//! message label, a self message or a side note needs the room; with `keep_columns` the given
//! centres stay and are only pushed apart. Rows: a message takes one row (taller for a tall
//! label), a self message two. A fragment's top, each operand separator and the fragment's
//! bottom sit half a row below the row before them, and the first row of an operand one row below
//! its top or separator, so rows stay evenly spaced and nothing touches a separator; activate
//! and deactivate take none. Activation bars in `auto` mode:
//! a synchronous call pushes a bar on its receiver one level deeper than the receiver's open
//! bars, and a reply pops the innermost bar opened by a call from the replying lifeline's
//! caller. The operands of forking operators start from the bars open at the fragment's start,
//! so each operand can answer the same call.

use serde::{Deserialize, Serialize};

use crate::{LayoutError, MAX_NODE_SIZE, Point, Rect};

/// Most events one layout takes.
pub const MAX_EVENTS: usize = 4096;
/// Most lifelines one layout takes.
pub const MAX_LIFELINES: usize = 1024;
/// Deepest fragment nesting.
pub const MAX_NESTING: usize = 64;
/// Width of a self message's loop to the right of its bar.
pub const SELF_W: f64 = 30.0;
/// Length of a lost or found message's free end.
pub const STUB: f64 = 60.0;
/// Space between a fragment's frame and the lifelines it covers, at the innermost level.
pub const FRAGMENT_PAD: f64 = 24.0;
/// Extra space per nesting level inside a fragment.
pub const NEST_PAD: f64 = 8.0;
/// Gap between a note and its lifeline.
pub const NOTE_GAP: f64 = 12.0;
/// Height of a fragment's operator tab and of the guard band under a separator: what follows a
/// fragment's top or a separator keeps clear of it.
pub const TAB_H: f64 = 20.0;
/// Clear space between a fragment's tab or guard and the nearest stem or activation bar.
pub const GUARD_GAP: f64 = 6.0;

/// What a message is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageSort {
    /// A synchronous call: the receiver gets an activation.
    #[default]
    Sync,
    /// An asynchronous signal.
    Async,
    /// The reply to a synchronous call.
    Reply,
    /// Creates the receiver: its head sits on the message row.
    Create,
    /// Destroys the receiver: its stem ends on the message row.
    Destroy,
    /// A message whose receiver is not shown (drawn to a free point on the right).
    Lost,
    /// A message whose sender is not shown (drawn from a free point on the left).
    Found,
}

/// Which side of its lifeline a note sits on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteSide {
    /// Over the lifeline (or centred between the two it names).
    #[default]
    Over,
    /// Left of the lifeline.
    Left,
    /// Right of the lifeline.
    Right,
}

/// A lifeline column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SeqLifeline {
    /// Head width.
    pub width: f64,
    /// Head height.
    pub head: f64,
    /// Drawn as an actor figure (informational; the head size already says how tall).
    #[serde(default)]
    pub actor: bool,
    /// The stem ends at the lifeline's last event even without a destroy message.
    #[serde(default)]
    pub destroyed: bool,
}

/// One event, lifelines by index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeqEvent {
    /// A message (for lost and found ones `from` and `to` are the one lifeline shown).
    Message {
        /// Sender.
        from: usize,
        /// Receiver.
        to: usize,
        /// What it is.
        sort: MessageSort,
        /// Label size (0 for none).
        label: (f64, f64),
    },
    /// A fragment starts.
    Open {
        /// Its operands start from the bars open here (alt, opt, break, critical, par).
        forks: bool,
        /// Lifelines it spans; empty spans the lifelines its content touches.
        covers: Vec<usize>,
        /// Narrowest frame, less 40.
        label: f64,
        /// Width of the operator tab (0 for none); the tab sits left of the first covered stem.
        #[serde(default)]
        tab: f64,
        /// Width of the first operand's guard text; guards sit right of the first covered stem.
        #[serde(default)]
        guard: f64,
    },
    /// The innermost fragment's next operand starts.
    Else {
        /// Width of the guard text.
        label: f64,
    },
    /// The innermost fragment ends.
    Close,
    /// A note.
    Note {
        /// One lifeline, or two for a note over both.
        over: Vec<usize>,
        /// Where it sits.
        side: NoteSide,
        /// Size.
        size: (f64, f64),
    },
    /// An explicit activation starts on the lifeline.
    Activate(usize),
    /// The lifeline's innermost activation ends.
    Deactivate(usize),
}

/// What [`layout`] reads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeqInput {
    /// The columns, left to right.
    pub lifelines: Vec<SeqLifeline>,
    /// The events, top to bottom.
    pub events: Vec<SeqEvent>,
}

/// How [`layout`] spaces things.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeqOptions {
    /// Height of one message row (16 to 400).
    pub row: f64,
    /// Gap between neighbouring heads (0 to 2000).
    pub column_gap: f64,
    /// Width of an activation bar (2 to 100).
    pub activation_width: f64,
    /// Derive bars from calls and replies (explicit activate events count either way).
    pub auto_activations: bool,
    /// Column centres to keep, one per lifeline; they are only pushed apart.
    pub keep_columns: Option<Vec<f64>>,
}

impl Default for SeqOptions {
    fn default() -> Self {
        SeqOptions { row: 36.0, column_gap: 60.0, activation_width: 10.0, auto_activations: true, keep_columns: None }
    }
}

/// An activation bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeqActivation {
    /// Its lifeline.
    pub lifeline: usize,
    /// Nesting level, 1 for a bar directly on the stem.
    pub depth: usize,
    /// Top.
    pub y0: f64,
    /// Bottom.
    pub y1: f64,
}

/// Where everything goes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeqLayout {
    /// Column centre x per lifeline.
    pub columns: Vec<f64>,
    /// Head rectangle per lifeline.
    pub heads: Vec<Rect>,
    /// Stem top and bottom y per lifeline.
    pub stems: Vec<(f64, f64)>,
    /// The y of each event (a message's line, a fragment's top, a separator, a fragment's
    /// bottom, a note's top; activate and deactivate take the current y).
    pub rows: Vec<f64>,
    /// Per event, a message's points from sender to receiver; empty for other events.
    pub routes: Vec<Vec<Point>>,
    /// The activation bars.
    pub activations: Vec<SeqActivation>,
    /// Per `Open` event in order, the fragment's frame.
    pub fragments: Vec<Rect>,
    /// Per `Open` event in order, the y of each operand separator.
    pub separators: Vec<Vec<f64>>,
    /// Per `Open` event in order, the x its guards start at: clear of the first covered stem and
    /// its activation bars, so no stem crosses a guard.
    pub guards: Vec<f64>,
    /// Per `Note` event in order, the note's frame.
    pub notes: Vec<Rect>,
    /// Everything drawn.
    pub bounds: Rect,
}

fn bad(reason: impl Into<String>) -> LayoutError {
    LayoutError::InvalidRoles { primitive: "sequence", reason: reason.into() }
}

/// A size that is finite and in range; anything else is `default`.
fn clean(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

/// An open fragment while rows are placed.
struct OpenFrag {
    /// Index into the output's fragments.
    index: usize,
    forks: bool,
    covers: Vec<usize>,
    /// Lifelines touched by its content.
    touched: Vec<usize>,
    /// Leftmost and rightmost x of content that is not a column (self loops, stubs, notes).
    extra: (f64, f64),
    /// Nesting levels inside it.
    levels: usize,
    top: f64,
    /// The bars at the start, for forking operands.
    snapshot: Vec<Vec<usize>>,
    /// Narrowest frame.
    min_w: f64,
    /// Width of the operator tab.
    tab: f64,
    /// Widest guard text.
    guard_w: f64,
    /// Leftmost and rightmost x of the fragments nested in it.
    children: (f64, f64),
}

/// Lay out a sequence diagram. Errors: more than [`MAX_EVENTS`] events or
/// [`MAX_LIFELINES`] lifelines, an event naming a lifeline that does not exist, and unbalanced
/// fragments (an `Else` or `Close` with no open fragment, a fragment left open, nesting deeper
/// than [`MAX_NESTING`]).
pub fn layout(input: &SeqInput, options: &SeqOptions) -> Result<SeqLayout, LayoutError> {
    let n = input.lifelines.len();
    if input.events.len() > MAX_EVENTS {
        return Err(bad(format!("{} events (at most {MAX_EVENTS})", input.events.len())));
    }
    if n > MAX_LIFELINES {
        return Err(bad(format!("{n} lifelines (at most {MAX_LIFELINES})")));
    }
    let row = clean(options.row, 16.0, 400.0, 36.0);
    let gap = clean(options.column_gap, 0.0, 2000.0, 60.0);
    let aw = clean(options.activation_width, 2.0, 100.0, 10.0);
    let widths: Vec<f64> = input.lifelines.iter().map(|l| clean(l.width, 1.0, MAX_NODE_SIZE, 100.0)).collect();
    let heads: Vec<f64> = input.lifelines.iter().map(|l| clean(l.head, 1.0, MAX_NODE_SIZE, 40.0)).collect();
    let w = |i: usize| widths.get(i).copied().unwrap_or(0.0);
    let lab = |v: f64| clean(v, 0.0, MAX_NODE_SIZE, 0.0);
    // Check indices and balance before placing anything.
    let mut depth = 0usize;
    for (k, e) in input.events.iter().enumerate() {
        let check = |i: usize| if i < n { Ok(()) } else { Err(bad(format!("event {k} names lifeline {i}, which does not exist"))) };
        match e {
            SeqEvent::Message { from, to, .. } => {
                check(*from)?;
                check(*to)?;
            }
            SeqEvent::Open { covers, .. } => {
                for &c in covers {
                    check(c)?;
                }
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(bad(format!("event {k}: fragments nest deeper than {MAX_NESTING}")));
                }
            }
            SeqEvent::Else { .. } if depth == 0 => return Err(bad(format!("event {k}: an operand separator outside any fragment"))),
            SeqEvent::Else { .. } => {}
            SeqEvent::Close => {
                depth = depth.checked_sub(1).ok_or_else(|| bad(format!("event {k}: a fragment end with no fragment open")))?;
            }
            SeqEvent::Note { over, .. } => {
                if over.is_empty() {
                    return Err(bad(format!("event {k}: a note over no lifeline")));
                }
                for &c in over.iter().take(2) {
                    check(c)?;
                }
            }
            SeqEvent::Activate(l) | SeqEvent::Deactivate(l) => check(*l)?,
        }
    }
    if depth != 0 {
        return Err(bad(format!("{depth} fragment(s) left open")));
    }

    // Columns.
    let mut col: Vec<f64> = Vec::with_capacity(n);
    let keep = options.keep_columns.as_ref().filter(|k| k.len() == n && k.iter().all(|x| x.is_finite() && x.abs() < crate::MAX_COORD));
    for i in 0..n {
        let packed = match i.checked_sub(1).and_then(|p| col.get(p).map(|c| (p, *c))) {
            Some((p, c)) => c + w(p) / 2.0 + gap + w(i) / 2.0,
            None => w(0) / 2.0,
        };
        let x = match keep.and_then(|k| k.get(i)) {
            Some(&kx) if i == 0 => kx,
            Some(&kx) => kx.max(packed),
            None => packed,
        };
        col.push(x);
    }
    let widen = |col: &mut Vec<f64>, lo: usize, hi: usize, need: f64| {
        let (Some(&a), Some(&b)) = (col.get(lo), col.get(hi)) else { return };
        let deficit = need - (b - a);
        if deficit > 0.0 && need.is_finite() {
            for c in col.iter_mut().skip(hi) {
                *c += deficit;
            }
        }
    };
    // Per open fragment while columns are widened: its first covered column and widest guard.
    let mut guard_room: Vec<(Option<usize>, f64)> = Vec::new();
    let first_of = |stack: &mut Vec<(Option<usize>, f64)>, ls: &[usize]| {
        for f in stack.iter_mut() {
            for &l in ls {
                f.0 = Some(f.0.map_or(l, |c| c.min(l)));
            }
        }
    };
    for e in &input.events {
        match e {
            SeqEvent::Open { covers, guard, .. } => guard_room.push((covers.iter().copied().min(), lab(*guard))),
            SeqEvent::Else { label } => {
                if let Some(f) = guard_room.last_mut() {
                    f.1 = f.1.max(lab(*label));
                }
            }
            SeqEvent::Close => {
                // A guard sits right of the first covered stem and its bars; the next stem stays
                // clear of it.
                if let Some((Some(first), gw)) = guard_room.pop()
                    && gw > 0.0
                {
                    widen(&mut col, first, first.saturating_add(1), gw + 2.0 * GUARD_GAP + 2.0 * aw);
                }
            }
            _ => {}
        }
        match e {
            SeqEvent::Message { from, to, .. } => first_of(&mut guard_room, &[*from, *to]),
            SeqEvent::Note { over, .. } => first_of(&mut guard_room, over.get(..over.len().min(2)).unwrap_or_default()),
            _ => {}
        }
        match e {
            SeqEvent::Message { from, to, sort, label } => {
                let lw = lab(label.0);
                if from == to || matches!(sort, MessageSort::Lost | MessageSort::Found) {
                    let next = from.saturating_add(1);
                    widen(&mut col, *from, next, SELF_W + aw * 2.0 + lw + w(next) / 2.0 + 10.0);
                } else {
                    widen(&mut col, (*from).min(*to), (*from).max(*to), lw + 20.0 + aw * 2.0);
                }
            }
            SeqEvent::Note { over, side, size } => {
                let Some(&p) = over.first() else { continue };
                let nw = lab(size.0);
                match side {
                    NoteSide::Right => widen(&mut col, p, p.saturating_add(1), nw + 2.0 * NOTE_GAP + w(p.saturating_add(1)) / 2.0),
                    NoteSide::Left if p > 0 => widen(&mut col, p - 1, p, nw + 2.0 * NOTE_GAP + w(p - 1) / 2.0),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let cx = |i: usize| col.get(i).copied().unwrap_or(0.0);

    // Rows.
    let top_h = heads.iter().copied().fold(0.0, f64::max);
    let mut y = top_h + row / 2.0;
    let mut out = SeqLayout {
        columns: col.clone(),
        rows: Vec::with_capacity(input.events.len()),
        routes: Vec::with_capacity(input.events.len()),
        ..SeqLayout::default()
    };
    let mut head_y: Vec<f64> = vec![0.0; n];
    let mut stem_end: Vec<Option<f64>> = vec![None; n];
    let mut last_y: Vec<f64> = vec![0.0; n];
    // Bars: indices into `out.activations`; the caller lifeline of each bar (None for explicit).
    let mut callers: Vec<Option<usize>> = Vec::new();
    let mut stacks: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut frags: Vec<OpenFrag> = Vec::new();
    // The last row was a fragment's top or a separator: what follows keeps clear of its tab and
    // guard band.
    let mut after_guard = false;
    let half = |after: bool| if after { (row / 2.0).max(TAB_H + GUARD_GAP) } else { row / 2.0 };
    let edge = |stacks: &Vec<Vec<usize>>, acts: &Vec<SeqActivation>, l: usize, right: bool| -> f64 {
        let c = cx(l);
        match stacks.get(l).and_then(|s| s.last()).and_then(|&b| acts.get(b)) {
            Some(a) => {
                let shift = (a.depth.saturating_sub(1)) as f64 * aw / 2.0;
                if right { c + aw / 2.0 + shift } else { c - aw / 2.0 + shift }
            }
            None => c,
        }
    };
    let touch = |frags: &mut Vec<OpenFrag>, ls: &[usize], xs: (f64, f64)| {
        for f in frags.iter_mut() {
            for &l in ls {
                if !f.touched.contains(&l) {
                    f.touched.push(l);
                }
            }
            f.extra = (f.extra.0.min(xs.0), f.extra.1.max(xs.1));
        }
    };
    for e in &input.events {
        let mut route = Vec::new();
        let at;
        let guard_before = after_guard;
        if !matches!(e, SeqEvent::Activate(_) | SeqEvent::Deactivate(_)) {
            after_guard = false;
        }
        match e {
            SeqEvent::Message { from, to, sort, label } => {
                let (a, b) = (*from, *to);
                let step = row.max(lab(label.1) + 20.0);
                y += step;
                at = y;
                let self_msg = a == b && !matches!(sort, MessageSort::Lost | MessageSort::Found);
                match sort {
                    MessageSort::Lost => {
                        let s = edge(&stacks, &out.activations, a, true);
                        route = vec![Point::new(s, y), Point::new(s + STUB, y)];
                    }
                    MessageSort::Found => {
                        let e = edge(&stacks, &out.activations, b, false);
                        route = vec![Point::new(e - STUB, y), Point::new(e, y)];
                    }
                    _ if self_msg => {
                        let s = edge(&stacks, &out.activations, a, true);
                        let y2 = y + row / 2.0;
                        if *sort == MessageSort::Sync && options.auto_activations {
                            push_bar(&mut out.activations, &mut stacks, &mut callers, b, Some(a), y2);
                        } else if *sort == MessageSort::Reply && options.auto_activations {
                            pop_reply(&mut out.activations, &mut stacks, &callers, a, b, y2);
                        }
                        let e = edge(&stacks, &out.activations, b, true);
                        route = vec![Point::new(s, y), Point::new(s.max(e) + SELF_W, y), Point::new(s.max(e) + SELF_W, y2), Point::new(e, y2)];
                        y = y2;
                    }
                    _ => {
                        let right = cx(b) >= cx(a);
                        let s = edge(&stacks, &out.activations, a, right);
                        match sort {
                            MessageSort::Sync if options.auto_activations => {
                                push_bar(&mut out.activations, &mut stacks, &mut callers, b, Some(a), y);
                            }
                            MessageSort::Create => {
                                if let Some(h) = head_y.get_mut(b) {
                                    *h = y - heads.get(b).copied().unwrap_or(0.0) / 2.0;
                                }
                            }
                            _ => {}
                        }
                        let e = if *sort == MessageSort::Create {
                            if right { cx(b) - w(b) / 2.0 } else { cx(b) + w(b) / 2.0 }
                        } else {
                            edge(&stacks, &out.activations, b, !right)
                        };
                        route = vec![Point::new(s, y), Point::new(e, y)];
                        if *sort == MessageSort::Reply && options.auto_activations {
                            pop_reply(&mut out.activations, &mut stacks, &callers, a, b, y);
                        }
                        if *sort == MessageSort::Destroy {
                            if let Some(st) = stacks.get_mut(b) {
                                for bar in st.drain(..) {
                                    if let Some(act) = out.activations.get_mut(bar) {
                                        act.y1 = act.y1.max(y);
                                    }
                                }
                            }
                            if let Some(s) = stem_end.get_mut(b) {
                                *s = Some(y);
                            }
                        }
                    }
                }
                for l in [a, b] {
                    if let Some(v) = last_y.get_mut(l) {
                        *v = y;
                    }
                }
                let (lo, hi) = route.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
                let lw = lab(label.0);
                let hi = if self_msg { hi + lw } else { hi };
                touch(&mut frags, &[a, b], (lo, hi));
            }
            SeqEvent::Open { forks, covers, label, tab, guard } => {
                // Half a row below the last row; the first row inside comes one row below the top.
                let top = y + half(guard_before);
                at = top;
                y = top;
                after_guard = true;
                let index = out.fragments.len();
                out.fragments.push(Rect::new(0.0, top, 0.0, 0.0));
                out.separators.push(Vec::new());
                out.guards.push(0.0);
                let snapshot = if *forks { stacks.clone() } else { Vec::new() };
                let lw = lab(*label);
                frags.push(OpenFrag {
                    index,
                    forks: *forks,
                    covers: covers.clone(),
                    touched: Vec::new(),
                    extra: (f64::INFINITY, f64::NEG_INFINITY),
                    levels: 0,
                    top,
                    snapshot,
                    min_w: lw + 40.0,
                    tab: lab(*tab),
                    guard_w: lab(*guard),
                    children: (f64::INFINITY, f64::NEG_INFINITY),
                });
            }
            SeqEvent::Else { label } => {
                // Half a row below the operand's last row; the next operand's first row comes one
                // row below it.
                let sep = y + half(guard_before);
                at = sep;
                y = sep;
                after_guard = true;
                if let Some(f) = frags.last_mut() {
                    f.guard_w = f.guard_w.max(lab(*label));
                }
                if let Some(f) = frags.last() {
                    if let Some(s) = out.separators.get_mut(f.index) {
                        s.push(sep);
                    }
                    if f.forks {
                        // The next operand starts from the bars open at the fragment's start;
                        // bars the snapshot does not hold (opened and left open in the operand)
                        // end here.
                        let snap = f.snapshot.clone();
                        for (l, st) in stacks.iter_mut().enumerate() {
                            let keep = snap.get(l).cloned().unwrap_or_default();
                            for bar in st.iter().filter(|b| !keep.contains(b)) {
                                if let Some(act) = out.activations.get_mut(*bar) {
                                    act.y1 = act.y1.max(sep);
                                }
                            }
                            *st = keep;
                        }
                    }
                }
            }
            SeqEvent::Close => {
                // Half a row below the last operand's last row.
                let bottom = y + half(guard_before);
                at = bottom;
                y = bottom;
                if let Some(f) = frags.pop() {
                    let mut ls = f.touched.clone();
                    for &c in &f.covers {
                        if !ls.contains(&c) {
                            ls.push(c);
                        }
                    }
                    let r = frame(&f, &ls, bottom, &FrameCtx { cx: &cx, acts: &out.activations, stacks: &stacks, aw, gap });
                    if let Some(slot) = out.fragments.get_mut(f.index) {
                        *slot = r.rect;
                    }
                    if let Some(slot) = out.guards.get_mut(f.index) {
                        *slot = r.guard_x;
                    }
                    if let Some(parent) = frags.last_mut() {
                        parent.levels = parent.levels.max(f.levels + 1);
                        for l in ls {
                            if !parent.touched.contains(&l) {
                                parent.touched.push(l);
                            }
                        }
                        parent.extra = (parent.extra.0.min(f.extra.0), parent.extra.1.max(f.extra.1));
                        parent.children = (parent.children.0.min(r.rect.x), parent.children.1.max(r.rect.right()));
                    }
                }
            }
            SeqEvent::Note { over, side, size } => {
                let (nw, nh) = (lab(size.0).max(1.0), lab(size.1).max(1.0));
                let top = y + if guard_before { TAB_H + GUARD_GAP } else { 8.0 };
                at = top;
                let a = over.first().copied().unwrap_or(0);
                let b = over.get(1).copied().unwrap_or(a);
                let x = match side {
                    NoteSide::Right => edge(&stacks, &out.activations, a, true) + NOTE_GAP,
                    NoteSide::Left => edge(&stacks, &out.activations, a, false) - NOTE_GAP - nw,
                    NoteSide::Over => {
                        let (l, r) = (cx(a).min(cx(b)), cx(a).max(cx(b)));
                        let nw2 = if a == b { nw } else { nw.max(r - l + 40.0) };
                        let x = (l + r) / 2.0 - nw2 / 2.0;
                        out.notes.push(Rect::new(x, top, nw2, nh));
                        y = top + nh;
                        touch(&mut frags, &[a, b], (x, x + nw2));
                        out.rows.push(at);
                        out.routes.push(route);
                        continue;
                    }
                };
                out.notes.push(Rect::new(x, top, nw, nh));
                y = top + nh;
                touch(&mut frags, &[a], (x, x + nw));
            }
            SeqEvent::Activate(l) => {
                at = y;
                push_bar(&mut out.activations, &mut stacks, &mut callers, *l, None, y);
            }
            SeqEvent::Deactivate(l) => {
                at = y;
                if let Some(bar) = stacks.get_mut(*l).and_then(Vec::pop)
                    && let Some(act) = out.activations.get_mut(bar)
                {
                    act.y1 = act.y1.max(y);
                }
            }
        }
        out.rows.push(at);
        out.routes.push(route);
    }
    let bottom = y + row / 2.0;
    // Calls never answered end after the receiver's last message.
    for (l, st) in stacks.iter().enumerate() {
        for &bar in st {
            if let Some(act) = out.activations.get_mut(bar) {
                let last = last_y.get(l).copied().unwrap_or(act.y0);
                act.y1 = act.y1.max(last.max(act.y0) + row / 4.0);
            }
        }
    }
    for a in &mut out.activations {
        a.y1 = a.y1.max(a.y0 + row / 4.0);
    }

    // Heads and stems.
    for i in 0..n {
        let hy = head_y.get(i).copied().unwrap_or(0.0);
        let hh = heads.get(i).copied().unwrap_or(0.0);
        out.heads.push(Rect::new(cx(i) - w(i) / 2.0, hy, w(i), hh));
        let destroyed = input.lifelines.get(i).is_some_and(|l| l.destroyed);
        let end = match stem_end.get(i).copied().flatten() {
            Some(e) => e,
            None if destroyed => last_y.get(i).copied().unwrap_or(0.0).max(hy + hh + row / 2.0),
            None => bottom,
        };
        out.stems.push((hy + hh, end.max(hy + hh)));
    }

    // Bounds; a packed layout starts at x = 0.
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = 0.0f64;
    let mut add = |r: &Rect| {
        x0 = x0.min(r.x);
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    };
    for r in out.heads.iter().chain(&out.fragments).chain(&out.notes) {
        add(r);
    }
    for p in out.routes.iter().flatten() {
        add(&Rect::new(p.x, p.y, 0.0, 0.0));
    }
    for (i, s) in out.stems.iter().enumerate() {
        add(&Rect::new(cx(i), s.0, 0.0, s.1 - s.0));
    }
    if !x0.is_finite() {
        x0 = 0.0;
        x1 = 0.0;
    }
    if keep.is_none() && x0 != 0.0 {
        let dx = -x0;
        out.columns.iter_mut().for_each(|c| *c += dx);
        for r in out.heads.iter_mut().chain(out.fragments.iter_mut()).chain(out.notes.iter_mut()) {
            r.x += dx;
        }
        for p in out.routes.iter_mut().flatten() {
            p.x += dx;
        }
        out.guards.iter_mut().for_each(|g| *g += dx);
        x1 += dx;
        x0 = 0.0;
    }
    out.bounds = Rect::new(x0, 0.0, (x1 - x0).max(0.0), y1.max(bottom));
    Ok(out)
}

/// What [`frame`] reads besides the fragment.
struct FrameCtx<'a, F: Fn(usize) -> f64> {
    cx: &'a F,
    acts: &'a [SeqActivation],
    stacks: &'a [Vec<usize>],
    aw: f64,
    gap: f64,
}

/// A closed fragment's frame and where its guards start.
struct Frame {
    rect: Rect,
    guard_x: f64,
}

/// The frame of fragment `f` over lifelines `ls`, from its top to `bottom`: at least half a column
/// gap past its outermost stems (more per level of nesting inside it), clear of their activation
/// bars and of the fragments nested in it, with the operator tab left of the first covered stem
/// and the guards right of that stem and its bars, so no stem or bar crosses either.
fn frame<F: Fn(usize) -> f64>(f: &OpenFrag, ls: &[usize], bottom: f64, c: &FrameCtx<'_, F>) -> Frame {
    let cx = c.cx;
    let pad = FRAGMENT_PAD.max(c.gap / 2.0) + NEST_PAD * f.levels as f64;
    // Bars on the covered lifelines that overlap the frame (still open ones count from their top).
    let bar_x = |l: usize, a: &SeqActivation| {
        let shift = (a.depth.saturating_sub(1)) as f64 * c.aw / 2.0;
        (cx(l) - c.aw / 2.0 + shift, cx(l) + c.aw / 2.0 + shift)
    };
    let mut bars = (f64::INFINITY, f64::NEG_INFINITY);
    let first = ls.iter().copied().min_by(|a, b| cx(*a).total_cmp(&cx(*b)));
    let mut first_bars = (f64::INFINITY, f64::NEG_INFINITY);
    for (bi, a) in c.acts.iter().enumerate() {
        if !ls.contains(&a.lifeline) {
            continue;
        }
        let open = c.stacks.get(a.lifeline).is_some_and(|s| s.contains(&bi));
        if a.y0 > bottom || (a.y1 < f.top && !open) {
            continue;
        }
        let (l, r) = bar_x(a.lifeline, a);
        bars = (bars.0.min(l), bars.1.max(r));
        if Some(a.lifeline) == first {
            first_bars = (first_bars.0.min(l), first_bars.1.max(r));
        }
    }
    let stems = ls.iter().map(|&l| cx(l));
    let (mut lo, mut hi) = (stems.clone().fold(f.extra.0, f64::min), stems.fold(f.extra.1, f64::max));
    if !lo.is_finite() || !hi.is_finite() {
        lo = cx(0);
        hi = cx(0);
    }
    let first_x = first.map_or(lo, cx);
    let mut x0 = lo - pad;
    x0 = x0.min(bars.0 - NEST_PAD).min(f.children.0 - NEST_PAD);
    // The tab ends left of the first stem and its bars.
    if f.tab > 0.0 {
        x0 = x0.min(first_x.min(first_bars.0) - GUARD_GAP - f.tab);
    }
    let guard_x = first_x.max(first_bars.1) + GUARD_GAP;
    let mut x1 = (hi + pad).max(bars.1 + NEST_PAD).max(f.children.1 + NEST_PAD).max(x0 + f.min_w).max(x0 + f.tab + 8.0);
    if f.guard_w > 0.0 {
        x1 = x1.max(guard_x + f.guard_w + 8.0);
    }
    Frame { rect: Rect::new(x0, f.top, (x1 - x0).max(0.0), (bottom - f.top).max(0.0)), guard_x }
}

/// Open a bar on `l`, one level deeper than its open bars.
fn push_bar(acts: &mut Vec<SeqActivation>, stacks: &mut [Vec<usize>], callers: &mut Vec<Option<usize>>, l: usize, caller: Option<usize>, y: f64) {
    let Some(st) = stacks.get_mut(l) else { return };
    acts.push(SeqActivation { lifeline: l, depth: st.len() + 1, y0: y, y1: y });
    callers.push(caller);
    st.push(acts.len() - 1);
}

/// A reply from `from` to `to` closes the innermost bar on `from` opened by a call from `to`,
/// and any bars above it.
fn pop_reply(acts: &mut [SeqActivation], stacks: &mut [Vec<usize>], callers: &[Option<usize>], from: usize, to: usize, y: f64) {
    let Some(st) = stacks.get_mut(from) else { return };
    let Some(pos) = st.iter().rposition(|&b| callers.get(b).copied().flatten() == Some(to)) else { return };
    for bar in st.drain(pos..) {
        if let Some(a) = acts.get_mut(bar) {
            a.y1 = a.y1.max(y);
        }
    }
}

/// The columns a graph's nodes take as lifelines, its edges in index order as synchronous
/// calls: each node's top-left (heads at the top, left to right in `order` or node order).
pub(crate) fn graph_columns(graph: &crate::Graph, order: Option<&[usize]>, options: &crate::Options) -> Result<Vec<Point>, LayoutError> {
    let n = graph.nodes.len();
    let order: Vec<usize> = match order {
        Some(o) => {
            let mut seen = vec![false; n];
            for &v in o {
                match seen.get_mut(v) {
                    None => return Err(bad(format!("lifelines names node {v}, which does not exist"))),
                    Some(true) => return Err(bad(format!("lifelines names node {v} twice"))),
                    Some(s) => *s = true,
                }
            }
            // Nodes the order leaves out follow in node order.
            o.iter().copied().chain((0..n).filter(|v| !seen.get(*v).copied().unwrap_or(true))).collect()
        }
        None => (0..n).collect(),
    };
    let mut col_of = vec![0usize; n];
    for (c, &v) in order.iter().enumerate() {
        if let Some(slot) = col_of.get_mut(v) {
            *slot = c;
        }
    }
    let lifelines: Vec<SeqLifeline> = order
        .iter()
        .map(|&v| {
            graph.nodes.get(v).map_or(SeqLifeline::default(), |nd| SeqLifeline { width: nd.width, head: nd.height, actor: false, destroyed: false })
        })
        .collect();
    let events: Vec<SeqEvent> = graph
        .edges
        .iter()
        .take(MAX_EVENTS)
        .map(|e| SeqEvent::Message {
            from: col_of.get(e.source).copied().unwrap_or(0),
            to: col_of.get(e.target).copied().unwrap_or(0),
            sort: MessageSort::Sync,
            label: (0.0, 0.0),
        })
        .collect();
    let opts = SeqOptions { column_gap: options.node_spacing, auto_activations: false, ..SeqOptions::default() };
    let l = layout(&SeqInput { lifelines, events }, &opts)?;
    let mut pos = vec![Point::default(); n];
    for (c, &v) in order.iter().enumerate() {
        if let (Some(slot), Some(h)) = (pos.get_mut(v), l.heads.get(c)) {
            *slot = Point::new(h.x, h.y);
        }
    }
    Ok(pos)
}
