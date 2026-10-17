//! Sequence diagrams: a row of participants, a lifeline under each, and the messages in order
//! as horizontal connectors down the page. Notes become small note shapes beside or over the
//! lifelines. Blocks (`loop`, `alt` and the rest) and activation bars are not drawn yet; their
//! messages are kept and a warning says what was dropped.

use std::collections::HashMap;

use vizcraft_diagram::{Arrow, Color, Connector, End, Page, Point, Route, Shape, ShapeKind};
use vizcraft_mermaid::sequence::{MessageArrow, NotePlacement, ParticipantKind, SeqItem, SequenceDiagram};

use super::common::{self, Ids, LINE_H, MARGIN, Warnings};

/// Smallest participant box.
const PARTICIPANT_MIN: (f64, f64) = (120.0, 50.0);
/// Smallest gap between neighbouring participant boxes.
const COLUMN_GAP: f64 = 50.0;
/// Width of a lifeline shape.
const LIFELINE_W: f64 = 2.0;
/// Vertical step of a message with a one-line label.
const MESSAGE_STEP: f64 = 44.0;
/// Width and height of a self-message loop.
const SELF_LOOP: (f64, f64) = (40.0, 24.0);
/// Gap between a note and the lifeline it sits beside.
const NOTE_GAP: f64 = 12.0;
/// Space above the participants for a box title.
const BOX_BAND: f64 = 30.0;
/// Space between a box and the participants inside it.
const BOX_PAD: f64 = 14.0;

/// One event placed on the page.
enum Placed {
    Message { from: usize, to: usize, y: f64, index: usize },
    Note { index: usize, x: f64, y: f64, w: f64, h: f64 },
}

/// Map a sequence diagram to one page.
pub(super) fn build(d: &SequenceDiagram, warnings: &mut Warnings) -> Page {
    let mut page = Page::new("p1", d.meta.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Mermaid".into()));
    let mut ids = Ids::default();
    let index: HashMap<&str, usize> = d.participants.iter().enumerate().rev().map(|(i, p)| (p.id.as_str(), i)).collect();
    let n = d.participants.len();
    let sizes: Vec<(f64, f64)> = d.participants.iter().map(|p| common::fit(ShapeKind::Rectangle, &p.label, PARTICIPANT_MIN)).collect();
    let head_h = sizes.iter().map(|s| s.1).fold(PARTICIPANT_MIN.1, f64::max);
    let width = |i: usize| sizes.get(i).map_or(PARTICIPANT_MIN.0, |s| s.0);

    // Message texts with autonumbering, and note sizes, before placing columns.
    let mut number: usize = 0;
    let mut texts: Vec<String> = Vec::new();
    let mut note_sizes: Vec<(f64, f64)> = Vec::new();
    for item in &d.items {
        match item {
            SeqItem::Message(m) => {
                number = number.saturating_add(1);
                texts.push(if d.autonumber { format!("{number}. {}", m.text).trim_end().to_string() } else { m.text.clone() });
            }
            SeqItem::Note(note) => note_sizes.push(common::fit(ShapeKind::Rectangle, &note.text, (80.0, 36.0))),
            _ => {}
        }
    }

    // Column centres: neighbours never closer than their boxes allow, then widened so every
    // label and side note fits. Widening moves whole columns right, so it never undoes an
    // earlier fix.
    let mut center = vec![0.0f64; n];
    let mut left_room: f64 = 0.0;
    for i in 1..n {
        let prev = center.get(i - 1).copied().unwrap_or(0.0);
        if let Some(c) = center.get_mut(i) {
            *c = prev + width(i - 1) / 2.0 + COLUMN_GAP + width(i) / 2.0;
        }
    }
    let widen = |center: &mut Vec<f64>, lo: usize, hi: usize, need: f64| {
        let (Some(&a), Some(&b)) = (center.get(lo), center.get(hi)) else { return };
        let deficit = need - (b - a);
        if deficit > 0.0 {
            for c in center.iter_mut().skip(hi) {
                *c += deficit;
            }
        }
    };
    let (mut msg_i, mut note_i) = (0usize, 0usize);
    for item in &d.items {
        match item {
            SeqItem::Message(m) => {
                let (Some(&a), Some(&b)) = (index.get(m.from.as_str()), index.get(m.to.as_str())) else {
                    msg_i = msg_i.saturating_add(1);
                    continue;
                };
                let label_w = texts.get(msg_i).map_or(0.0, |t| if t.is_empty() { 0.0 } else { common::text_size(t).0 });
                if a == b {
                    widen(&mut center, a, a.saturating_add(1), SELF_LOOP.0 + label_w + width(a.saturating_add(1)) / 2.0 + 10.0);
                } else {
                    widen(&mut center, a.min(b), a.max(b), label_w + 20.0);
                }
                msg_i = msg_i.saturating_add(1);
            }
            SeqItem::Note(note) => {
                let w = note_sizes.get(note_i).map_or(0.0, |s| s.0);
                note_i = note_i.saturating_add(1);
                let Some(&p) = note.participants.first().and_then(|id| index.get(id.as_str())) else { continue };
                match note.placement {
                    NotePlacement::RightOf => widen(&mut center, p, p.saturating_add(1), w + 2.0 * NOTE_GAP + width(p.saturating_add(1)) / 2.0),
                    NotePlacement::LeftOf if p == 0 => left_room = left_room.max(w + NOTE_GAP - width(0) / 2.0),
                    NotePlacement::LeftOf => widen(&mut center, p - 1, p, w + 2.0 * NOTE_GAP + width(p - 1) / 2.0),
                    NotePlacement::Over => {}
                }
            }
            _ => {}
        }
    }
    let x0 = MARGIN + width(0) / 2.0 + left_room.max(0.0);
    let center: Vec<f64> = center.into_iter().map(|c| c + x0).collect();
    let cx = |i: usize| center.get(i).copied().unwrap_or(x0);

    // Events down the page.
    let top = if d.boxes.is_empty() { MARGIN } else { MARGIN + BOX_BAND };
    let mut y = top + head_h + 24.0;
    let mut placed: Vec<Placed> = Vec::new();
    let (mut msg_i, mut note_i) = (0usize, 0usize);
    let (mut saw_block, mut saw_activation, mut saw_lifecycle) = (false, false, false);
    for item in &d.items {
        match item {
            SeqItem::Message(m) => {
                saw_activation |= m.activate || m.deactivate;
                let lines = texts.get(msg_i).map_or(0, |t| if t.is_empty() { 0 } else { t.split('\n').count() });
                let step = MESSAGE_STEP.max(lines as f64 * LINE_H + 28.0);
                if let (Some(&a), Some(&b)) = (index.get(m.from.as_str()), index.get(m.to.as_str())) {
                    let extra = if a == b { SELF_LOOP.1 } else { 0.0 };
                    placed.push(Placed::Message { from: a, to: b, y: y + step / 2.0, index: msg_i });
                    y += step + extra;
                } else {
                    warnings.add("a message names a participant that does not exist and was skipped");
                }
                msg_i = msg_i.saturating_add(1);
            }
            SeqItem::Note(note) => {
                let (w, h) = note_sizes.get(note_i).copied().unwrap_or((80.0, 36.0));
                let cols: Vec<usize> = note.participants.iter().filter_map(|id| index.get(id.as_str()).copied()).collect();
                if let Some(&p) = cols.first() {
                    let x = match note.placement {
                        NotePlacement::LeftOf => cx(p) - NOTE_GAP - w,
                        NotePlacement::RightOf => cx(p) + NOTE_GAP,
                        NotePlacement::Over => {
                            let q = cols.get(1).copied().unwrap_or(p);
                            let (l, r) = (cx(p).min(cx(q)), cx(p).max(cx(q)));
                            // A note over two participants spans both lifelines.
                            let w = if q == p { w } else { w.max(r - l + 40.0) };
                            placed.push(Placed::Note { index: note_i, x: (l + r) / 2.0 - w / 2.0, y: y + 6.0, w, h });
                            y += h + 18.0;
                            note_i = note_i.saturating_add(1);
                            continue;
                        }
                    };
                    placed.push(Placed::Note { index: note_i, x, y: y + 6.0, w, h });
                    y += h + 18.0;
                } else {
                    warnings.add("a note names a participant that does not exist and was skipped");
                }
                note_i = note_i.saturating_add(1);
            }
            SeqItem::Activate(_) | SeqItem::Deactivate(_) => saw_activation = true,
            SeqItem::Destroy(_) => saw_lifecycle = true,
            SeqItem::BlockStart { .. } | SeqItem::Branch { .. } | SeqItem::BlockEnd => saw_block = true,
        }
    }
    let bottom = y + 16.0;
    if saw_block {
        warnings.add("sequence blocks (loop, alt, opt, par, critical, break, rect) are not drawn yet; their messages are kept");
    }
    if saw_activation {
        warnings.add("sequence activation bars are not drawn yet");
    }
    if saw_lifecycle || d.participants.iter().any(|p| p.created || p.destroyed) {
        warnings.add("participant create and destroy are not drawn yet; those participants span the whole diagram");
    }

    // Participants claim their Mermaid ids before anything generated can take them.
    let participant_ids: Vec<String> = d.participants.iter().map(|p| ids.claim(&p.id)).collect();
    // Boxes behind everything, then participants and lifelines, then notes.
    for (bi, b) in d.boxes.iter().enumerate() {
        let members: Vec<usize> = d.participants.iter().enumerate().filter(|(_, p)| p.in_box == Some(bi)).map(|(i, _)| i).collect();
        let (Some(l), Some(r)) =
            (members.iter().map(|&i| cx(i) - width(i) / 2.0).reduce(f64::min), members.iter().map(|&i| cx(i) + width(i) / 2.0).reduce(f64::max))
        else {
            continue;
        };
        let id = ids.claim(&format!("box{}", bi.saturating_add(1)));
        let mut s = common::from_master("basic.rectangle", &id);
        (s.x, s.y, s.w, s.h) = (l - BOX_PAD, MARGIN, r - l + 2.0 * BOX_PAD, bottom - MARGIN + BOX_PAD);
        s.text = b.label.clone();
        s.style.fill = b.color.as_deref().and_then(common::color).unwrap_or(Color::rgb(0xf4, 0xf6, 0xf9));
        s.style.line = Color::rgb(0x8a, 0x97, 0xa6);
        s.style.line_width = 1.0;
        s.style.bold = true;
        s.ext.insert("mermaid.group".into(), "box".into());
        s.sanitize();
        page.shapes.push(s);
    }
    for ((i, p), id) in d.participants.iter().enumerate().zip(&participant_ids) {
        let master = if p.kind == ParticipantKind::Actor { "basic.roundedRectangle" } else { "basic.rectangle" };
        let mut s = common::from_master(master, id);
        let w = width(i);
        (s.x, s.y, s.w, s.h) = (cx(i) - w / 2.0, top, w, head_h);
        s.text = p.label.clone();
        s.ext.insert("mermaid.shape".into(), if p.kind == ParticipantKind::Actor { "actor" } else { "participant" }.into());
        s.sanitize();
        page.shapes.push(s);
    }
    for (i, pid) in participant_ids.iter().enumerate() {
        let id = ids.claim(&format!("{pid}_lifeline"));
        let mut s = Shape::new(id, ShapeKind::Rectangle, cx(i) - LIFELINE_W / 2.0, top + head_h, LIFELINE_W, (bottom - top - head_h).max(1.0));
        s.stencil = "basic.rectangle".into();
        s.style.fill = Color::rgb(0x9a, 0xa5, 0xb1);
        s.style.line = Color::TRANSPARENT;
        s.style.line_width = 0.0;
        s.ext.insert("mermaid.shape".into(), "lifeline".into());
        s.sanitize();
        page.shapes.push(s);
    }
    let notes: Vec<&str> = d
        .items
        .iter()
        .filter_map(|i| match i {
            SeqItem::Note(n) => Some(n.text.as_str()),
            _ => None,
        })
        .collect();
    let messages: Vec<&vizcraft_mermaid::sequence::Message> = d
        .items
        .iter()
        .filter_map(|i| match i {
            SeqItem::Message(m) => Some(m),
            _ => None,
        })
        .collect();
    for p in &placed {
        match *p {
            Placed::Note { index, x, y, w, h } => {
                let id = ids.claim(&format!("note{}", index.saturating_add(1)));
                let mut s = common::from_master("basic.rectangle", &id);
                (s.x, s.y, s.w, s.h) = (x, y, w, h);
                s.text = notes.get(index).map(|t| t.to_string()).unwrap_or_default();
                s.style.fill = Color::rgb(0xfd, 0xf6, 0xc8);
                s.style.line = Color::rgb(0xb5, 0x9f, 0x3b);
                s.ext.insert("mermaid.shape".into(), "note".into());
                s.sanitize();
                page.shapes.push(s);
            }
            Placed::Message { from, to, y, index } => {
                let Some(m) = messages.get(index) else { continue };
                let id = ids.claim(&format!("m{}", index.saturating_add(1)));
                let (xa, xb) = (cx(from), cx(to));
                let mut c = if from == to {
                    // A message to oneself: a small loop out to the right and back.
                    let mut c = Connector::new(id, End::free(Point::new(xa, y)), End::free(Point::new(xa, y + SELF_LOOP.1)));
                    c.waypoints = vec![Point::new(xa + SELF_LOOP.0, y), Point::new(xa + SELF_LOOP.0, y + SELF_LOOP.1)];
                    c.route = Route::Orthogonal;
                    c
                } else {
                    let mut c = Connector::new(id, End::free(Point::new(xa, y)), End::free(Point::new(xb, y)));
                    c.route = Route::Straight;
                    c
                };
                c.text = texts.get(index).cloned().unwrap_or_default();
                let (start, end, marker) = match m.arrow {
                    MessageArrow::Solid | MessageArrow::Dotted => (Arrow::None, Arrow::None, None),
                    MessageArrow::SolidArrow | MessageArrow::DottedArrow => (Arrow::None, Arrow::Triangle, None),
                    MessageArrow::SolidCross | MessageArrow::DottedCross => (Arrow::None, Arrow::Open, Some("cross")),
                    MessageArrow::SolidAsync | MessageArrow::DottedAsync => (Arrow::None, Arrow::Open, Some("openArrow")),
                    MessageArrow::BidirectionalSolid | MessageArrow::BidirectionalDotted => (Arrow::Triangle, Arrow::Triangle, None),
                };
                (c.start_arrow, c.end_arrow) = (start, end);
                if let Some(mk) = marker {
                    c.ext.insert("mermaid.endMarker".into(), mk.into());
                }
                if m.arrow.dotted() {
                    c.ext.insert("mermaid.line".into(), "dotted".into());
                }
                c.sanitize();
                page.connectors.push(c);
            }
        }
    }
    common::fit_page(&mut page);
    page
}
