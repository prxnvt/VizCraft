//! Sequence diagrams: Mermaid `sequenceDiagram` read into the sequence substrate (see
//! `vizcraft_diagram::sequence`). Participants and actors become lifelines; messages become
//! message connectors glued to them, their arrows mapped to sorts; blocks become combined
//! fragments with their operands; `activate`, `deactivate` and the `+` / `-` shorthands become
//! explicit activation events (and switch the interaction to explicit bars); notes become note
//! shapes; boxes stay background rectangles. The page is then laid out by the substrate's own
//! settle pass, so an import draws exactly what editing the page draws.
//!
//! What the substrate does not model is kept for round trips: an arrow token that is not the
//! canonical one for its sort in `ext["mermaid.arrow"]`, a box's colour in `ext["mermaid.box"]`,
//! a `rect` colour in `ext["mermaid.rect"]`.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::sequence::{self as seq, Activations, Event, Interaction, Message, Operand, Operator, Part, Side, Sort};
use vizcraft_diagram::{Color, Connector, End, Page, Shape, ShapeKind};
use vizcraft_mermaid::sequence::{BlockKind, MessageArrow, NotePlacement, ParticipantKind, SeqItem, SequenceDiagram};

use super::common::{self, Ids, MARGIN, Warnings};

/// Space above the lifelines for a box title.
const BOX_BAND: f64 = 30.0;
/// Space between a box and the lifelines inside it.
const BOX_PAD: f64 = 14.0;
/// The `ext` key holding a message's arrow token when it is not its sort's canonical one.
pub const ARROW_KEY: &str = "mermaid.arrow";
/// The `ext` key holding a box's colour.
pub const BOX_KEY: &str = "mermaid.box";
/// The `ext` key holding a `rect` fragment's colour.
pub const RECT_KEY: &str = "mermaid.rect";

/// The canonical arrow token of a sort, the one the writer uses.
pub fn canonical_arrow(sort: Sort) -> &'static str {
    match sort {
        Sort::Reply => "-->>",
        Sort::Async | Sort::Lost | Sort::Found => "-)",
        Sort::Sync | Sort::Create | Sort::Destroy => "->>",
    }
}

/// The arrow token as written in Mermaid.
pub fn arrow_token(a: MessageArrow) -> &'static str {
    match a {
        MessageArrow::Solid => "->",
        MessageArrow::Dotted => "-->",
        MessageArrow::SolidArrow => "->>",
        MessageArrow::DottedArrow => "-->>",
        MessageArrow::SolidCross => "-x",
        MessageArrow::DottedCross => "--x",
        MessageArrow::SolidAsync => "-)",
        MessageArrow::DottedAsync => "--)",
        MessageArrow::BidirectionalSolid => "<<->>",
        MessageArrow::BidirectionalDotted => "<<-->>",
    }
}

/// The sort an arrow token reads as.
pub fn sort_of(a: MessageArrow) -> Sort {
    match a {
        MessageArrow::SolidArrow | MessageArrow::Solid | MessageArrow::BidirectionalSolid | MessageArrow::BidirectionalDotted => Sort::Sync,
        MessageArrow::DottedArrow | MessageArrow::Dotted => Sort::Reply,
        MessageArrow::SolidAsync | MessageArrow::DottedAsync | MessageArrow::SolidCross | MessageArrow::DottedCross => Sort::Async,
    }
}

fn operator_of(k: BlockKind) -> Operator {
    match k {
        BlockKind::Loop => Operator::Loop,
        BlockKind::Alt => Operator::Alt,
        BlockKind::Opt => Operator::Opt,
        BlockKind::Par => Operator::Par,
        BlockKind::Critical => Operator::Critical,
        BlockKind::Break => Operator::Break,
        BlockKind::Rect => Operator::Region,
    }
}

/// A message connector between two lifelines.
fn message(id: String, from: &str, to: &str, sort: Sort, text: &str) -> Connector {
    let mut c = Connector::new(id, End::dynamic(from), End::dynamic(to));
    match vizcraft_stencils::connector_kind(sort.connector_type()) {
        Some(k) => k.apply(&mut c),
        None => c.end_arrow = vizcraft_diagram::Arrow::Open,
    }
    c.route = vizcraft_diagram::Route::Straight;
    c.text = text.to_string();
    c.message = Some(Message { sort });
    c
}

/// Map a sequence diagram to one page.
pub(super) fn build(d: &SequenceDiagram, warnings: &mut Warnings) -> Page {
    let mut page = Page::new("p1", d.meta.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "Mermaid".into()));
    let mut ids = Ids::default();
    // Participants claim their Mermaid ids before anything generated can take them.
    let pid: HashMap<&str, String> = d.participants.iter().map(|p| (p.id.as_str(), ids.claim(&p.id))).collect();
    for (i, p) in d.participants.iter().enumerate() {
        let id = pid.get(p.id.as_str()).cloned().unwrap_or_default();
        let actor = p.kind == ParticipantKind::Actor;
        let mut s = common::from_master(if actor { "uml.actorLifeline" } else { "uml.lifeline" }, &id);
        s.kind = if actor { ShapeKind::ActorLifeline } else { ShapeKind::Lifeline };
        // Columns in order of first mention; settle keeps them in this order.
        (s.x, s.y, s.w, s.h) = (MARGIN + i as f64 * 200.0, MARGIN, 100.0, 300.0);
        s.text = p.label.clone();
        s.sequence = Some(Part::Lifeline { destroyed: false });
        s.connection_points = Vec::new();
        s.sanitize();
        page.shapes.push(s);
    }
    let lifeline = |name: &str| pid.get(name).cloned();
    let mut events: Vec<Event> = Vec::new();
    let mut explicit = false;
    let mut pending_destroy: HashSet<String> = HashSet::new();
    let mut created: HashSet<String> = d.participants.iter().filter(|p| p.created).filter_map(|p| lifeline(&p.id)).collect();
    // Open fragments: shape id and operand count.
    let mut open: Vec<String> = Vec::new();
    let mut n_msg = 0usize;
    let mut n_note = 0usize;
    let mut n_frag = 0usize;
    for item in &d.items {
        if events.len() >= seq::MAX_EVENTS {
            warnings.add(format!("only the first {} events of the sequence were read", seq::MAX_EVENTS));
            break;
        }
        match item {
            SeqItem::Message(m) => {
                let (Some(a), Some(b)) = (lifeline(&m.from), lifeline(&m.to)) else {
                    warnings.add("a message names a participant that does not exist and was skipped");
                    continue;
                };
                let mut sort = sort_of(m.arrow);
                if created.remove(&b) && a != b {
                    sort = Sort::Create;
                } else if pending_destroy.remove(&b) && a != b {
                    sort = Sort::Destroy;
                }
                n_msg += 1;
                let id = ids.claim(&format!("m{n_msg}"));
                let mut c = message(id.clone(), &a, &b, sort, &m.text);
                let token = arrow_token(m.arrow);
                if token != canonical_arrow(sort) {
                    c.ext.insert(ARROW_KEY.into(), token.into());
                }
                c.sanitize();
                page.connectors.push(c);
                events.push(Event::Message(id));
                if m.activate {
                    explicit = true;
                    events.push(Event::Activate(b.clone()));
                }
                if m.deactivate {
                    explicit = true;
                    events.push(Event::Deactivate(a.clone()));
                }
                // `destroy X` then a message from X: X ends after it.
                if pending_destroy.remove(&a)
                    && let Some(s) = page.shape_mut(&a)
                {
                    s.sequence = Some(Part::Lifeline { destroyed: true });
                }
            }
            SeqItem::Note(n) => {
                let over: Vec<String> = n.participants.iter().filter_map(|p| lifeline(p)).take(2).collect();
                if over.is_empty() {
                    warnings.add("a note names a participant that does not exist and was skipped");
                    continue;
                }
                n_note += 1;
                let id = ids.claim(&format!("note{n_note}"));
                let mut s = common::from_master("uml.note", &id);
                let (w, h) = common::fit(ShapeKind::Rectangle, &n.text, (80.0, 36.0));
                (s.w, s.h) = (w, h);
                s.text = n.text.clone();
                let side = match n.placement {
                    NotePlacement::LeftOf => Side::Left,
                    NotePlacement::RightOf => Side::Right,
                    NotePlacement::Over => Side::Over,
                };
                s.sequence = Some(Part::Note { over, side });
                s.sanitize();
                page.shapes.push(s);
                events.push(Event::Note(id));
            }
            SeqItem::Activate(p) | SeqItem::Deactivate(p) => {
                let Some(l) = lifeline(p) else { continue };
                explicit = true;
                events.push(if matches!(item, SeqItem::Activate(_)) { Event::Activate(l) } else { Event::Deactivate(l) });
            }
            SeqItem::Destroy(p) => {
                if let Some(l) = lifeline(p) {
                    pending_destroy.insert(l);
                }
            }
            SeqItem::BlockStart { kind, label } => {
                n_frag += 1;
                let id = ids.claim(&format!("f{n_frag}"));
                let mut s = common::from_master("uml.fragment", &id);
                let operator = operator_of(*kind);
                let guard = if operator == Operator::Region {
                    if !label.trim().is_empty() {
                        s.ext.insert(RECT_KEY.into(), label.trim().to_string());
                        if let Some(c) = common::color(label.trim()) {
                            s.style.fill = Color { a: 60, ..c };
                        }
                    }
                    String::new()
                } else {
                    label.clone()
                };
                s.sequence = Some(Part::Fragment { operator, operands: vec![Operand { guard, at: 0.0 }], covers: Vec::new() });
                seq::fragment_look(&mut s);
                if operator == Operator::Region && s.ext.contains_key(RECT_KEY) {
                    // A rect's colour is its point; fragment_look made it transparent.
                    if let Some(c) = common::color(s.ext.get(RECT_KEY).map(String::as_str).unwrap_or_default()) {
                        s.style.fill = Color { a: 60, ..c };
                    }
                }
                s.sanitize();
                page.shapes.push(s);
                open.push(id.clone());
                events.push(Event::Open(id));
            }
            SeqItem::Branch { label, .. } => {
                let Some(f) = open.last().cloned() else { continue };
                if let Some(Part::Fragment { operands, .. }) = page.shape_mut(&f).and_then(|s| s.sequence.as_mut())
                    && operands.len() < seq::MAX_OPERANDS
                {
                    operands.push(Operand { guard: label.clone(), at: 0.0 });
                }
                events.push(Event::Else(f));
            }
            SeqItem::BlockEnd => {
                if let Some(f) = open.pop() {
                    events.push(Event::Close(f));
                }
            }
        }
    }
    for f in open.into_iter().rev() {
        events.push(Event::Close(f));
    }
    page.sequence = Some(Interaction {
        events,
        activations: if explicit { Activations::Explicit } else { Activations::Auto },
        autonumber: d.autonumber,
        ..Interaction::default()
    });
    if seq::settle_page(&mut page, false).is_none() {
        warnings.add("the sequence could not be laid out; its items are kept unplaced");
    }
    // Generated bars and marks get readable ids.
    for s in &mut page.shapes {
        if s.id.starts_with('\u{1}') {
            let base = match s.sequence {
                Some(Part::Destruction { .. }) => "destroy",
                _ => "activation",
            };
            s.id = ids.claim(base);
        }
    }
    add_boxes(d, &mut page, &pid, &mut ids);
    common::fit_page(&mut page);
    page
}

/// Background rectangles around the lifelines of each `box`, behind everything.
fn add_boxes(d: &SequenceDiagram, page: &mut Page, pid: &HashMap<&str, String>, ids: &mut Ids) {
    if d.boxes.is_empty() {
        return;
    }
    // Make room above the heads for the box titles.
    for s in &mut page.shapes {
        s.y += BOX_BAND;
    }
    for c in &mut page.connectors {
        for p in &mut c.waypoints {
            p.y += BOX_BAND;
        }
        for e in [&mut c.start, &mut c.end] {
            if let End::Free { y, .. } = e {
                *y += BOX_BAND;
            }
        }
    }
    let bottom = page.shapes.iter().map(|s| s.y + s.h).fold(0.0, f64::max);
    let mut boxes = Vec::new();
    for (bi, b) in d.boxes.iter().enumerate() {
        let members: Vec<&Shape> =
            d.participants.iter().filter(|p| p.in_box == Some(bi)).filter_map(|p| pid.get(p.id.as_str())).filter_map(|id| page.shape(id)).collect();
        let (Some(l), Some(r)) = (members.iter().map(|s| s.x).reduce(f64::min), members.iter().map(|s| s.x + s.w).reduce(f64::max)) else { continue };
        let id = ids.claim(&format!("box{}", bi.saturating_add(1)));
        let mut s = common::from_master("basic.rectangle", &id);
        (s.x, s.y, s.w, s.h) = (l - BOX_PAD, MARGIN, r - l + 2.0 * BOX_PAD, bottom - MARGIN + BOX_PAD);
        s.text = b.label.clone();
        s.style.fill = b.color.as_deref().and_then(common::color).unwrap_or(Color::rgb(0xf4, 0xf6, 0xf9));
        s.style.line = Color::rgb(0x8a, 0x97, 0xa6);
        s.style.line_width = 1.0;
        s.style.bold = true;
        s.style.v_align = vizcraft_diagram::VAlign::Top;
        s.ext.insert(BOX_KEY.into(), b.color.clone().unwrap_or_default());
        s.ext.insert("mermaid.group".into(), "box".into());
        s.sanitize();
        boxes.push(s);
    }
    let rest = std::mem::take(&mut page.shapes);
    page.shapes = boxes;
    page.shapes.extend(rest);
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::sequence::{Event, Part, Sort, rows};
    use vizcraft_diagram::{Color, End};

    use super::super::import;
    use super::*;

    fn page(src: &str) -> Page {
        import(src).unwrap().pages.remove(0)
    }

    fn sorts(p: &Page) -> Vec<(String, Sort)> {
        p.connectors.iter().map(|c| (c.text.clone(), c.message.as_ref().map(|m| m.sort).unwrap_or_default())).collect()
    }

    #[test]
    fn participants_messages_blocks_and_notes_build_the_substrate() {
        let src = "sequenceDiagram\n  participant A as Alice\n  actor B as Bob\n  A->>B: Hello\n  B-->>A: Hi back\n  Note right of B: thinking\n  B-)A: async\n  A->>A: self\n  loop Every minute\n    A-xB: ping\n  end\n  alt ok\n    A->>B: yes\n  else not ok\n    A->>B: no\n  end\n";
        let p = page(src);
        let a = p.shape("A").unwrap();
        let b = p.shape("B").unwrap();
        assert_eq!((a.text.as_str(), a.kind, b.kind), ("Alice", ShapeKind::Lifeline, ShapeKind::ActorLifeline));
        assert!(b.x > a.x + a.w, "columns in order of mention");
        let seq = p.sequence.as_ref().unwrap();
        let kinds: Vec<&str> = seq.events.iter().map(Event::kind).collect();
        assert_eq!(
            kinds,
            ["message", "message", "note", "message", "message", "open", "message", "close", "open", "message", "else", "message", "close"]
        );
        assert_eq!(
            sorts(&p).iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            [Sort::Sync, Sort::Reply, Sort::Async, Sort::Sync, Sort::Async, Sort::Sync, Sort::Sync]
        );
        let ping = p.connectors.iter().find(|c| c.text == "ping").unwrap();
        assert_eq!(ping.ext.get(ARROW_KEY).map(String::as_str), Some("-x"), "a non-canonical arrow is kept");
        assert!(p.connectors.iter().all(|c| matches!(c.start, End::Glued { point: Some(_), .. })));
        let alt = p.shapes.iter().find(|s| matches!(&s.sequence, Some(Part::Fragment { operator: Operator::Alt, .. }))).unwrap();
        let Some(Part::Fragment { operands, .. }) = &alt.sequence else { panic!() };
        assert_eq!(operands.iter().map(|o| o.guard.as_str()).collect::<Vec<_>>(), ["ok", "not ok"]);
        assert!(operands[1].at > 0.0);
        let note = p.shapes.iter().find(|s| matches!(s.sequence, Some(Part::Note { .. }))).unwrap();
        assert_eq!(note.text, "thinking");
        assert!(note.x > b.x + b.w / 2.0);
        let ys: Vec<f64> = rows(&p).into_iter().map(|(_, y)| y).collect();
        assert!(ys.windows(2).all(|w| w[1] > w[0]), "{ys:?}");
        // Bars: Hello opens one on B, Hi back closes it; the self call nests on A.
        assert!(p.shapes.iter().any(|s| matches!(&s.sequence, Some(Part::Activation { lifeline, .. }) if lifeline == "B")));
    }

    #[test]
    fn activation_shorthands_create_destroy_autonumber_and_boxes() {
        let src = "sequenceDiagram\n  autonumber\n  box Aqua Team\n    participant A\n    participant B\n  end\n  participant C\n  A->>+B: one\n  create participant D\n  B->>D: make\n  destroy D\n  B-xD: kill\n  B-->>-A: two\n  rect rgb(200, 220, 255)\n    A->>C: three\n  end\n";
        let p = page(src);
        let seq = p.sequence.as_ref().unwrap();
        assert!(seq.autonumber);
        assert_eq!(seq.activations, Activations::Explicit);
        assert!(seq.events.contains(&Event::Activate("B".into())) && seq.events.contains(&Event::Deactivate("B".into())));
        let s = sorts(&p);
        assert_eq!(s.iter().find(|(t, _)| t == "make").unwrap().1, Sort::Create);
        assert_eq!(s.iter().find(|(t, _)| t == "kill").unwrap().1, Sort::Destroy);
        assert_eq!(s.iter().find(|(t, _)| t == "one").unwrap().0, "one", "labels are not numbered; autonumber is kept for the writer");
        assert!(p.shapes.iter().any(|x| matches!(&x.sequence, Some(Part::Destruction { lifeline }) if lifeline == "D")));
        let d = p.shape("D").unwrap();
        assert!(d.y > p.shape("A").unwrap().y, "a created lifeline starts on its row");
        let bx = p.shapes.iter().find(|x| x.ext.contains_key(BOX_KEY)).unwrap();
        assert_eq!(bx.style.fill, Color::rgb(0, 255, 255));
        assert_eq!(p.shapes.first().map(|x| x.id.as_str()), Some(bx.id.as_str()), "boxes are behind");
        let (a, c) = (p.shape("A").unwrap(), p.shape("C").unwrap());
        assert!(bx.x <= a.x && bx.x + bx.w >= a.x + a.w && bx.x + bx.w < c.x);
        let region = p.shapes.iter().find(|x| matches!(&x.sequence, Some(Part::Fragment { operator: Operator::Region, .. }))).unwrap();
        assert_eq!(region.ext.get(RECT_KEY).map(String::as_str), Some("rgb(200, 220, 255)"));
        assert!(p.shapes.iter().all(|x| vizcraft_diagram::is_plain_id(&x.id)));
    }
}
