//! Sequence diagrams: `sequenceDiagram`.
//!
//! Participants and actors (with aliases, `create` and `destroy`), boxes, messages of every
//! arrow kind with `+` / `-` activation shorthands, `activate` / `deactivate`, notes, and the
//! `loop`, `alt` / `else`, `opt`, `par` / `and`, `critical` / `option`, `break` and `rect`
//! blocks. The order of events matters here, so the IR keeps one flat, ordered list of items;
//! block starts, branches and ends are items too.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::common::{
    Ctx, Direction, Meta, ParseError, Prepared, StmtError, StmtResult, check_depth, check_edges, check_nodes, clean_label, first_word, settle,
    split_statements,
};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, LineStyle, Shape};

/// A parsed sequence diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SequenceDiagram {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Participants in order of first mention (left to right).
    pub participants: Vec<Participant>,
    /// Boxes grouping participants.
    pub boxes: Vec<SeqBox>,
    /// Events in source order.
    pub items: Vec<SeqItem>,
    /// Whether `autonumber` was given.
    pub autonumber: bool,
}

/// How a participant is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticipantKind {
    /// A box.
    #[default]
    Participant,
    /// A stick figure.
    Actor,
}

/// A participant or actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    /// Id used in messages.
    pub id: String,
    /// Display text (the alias, or the id).
    pub label: String,
    /// Box or figure.
    pub kind: ParticipantKind,
    /// Index into [`SequenceDiagram::boxes`].
    pub in_box: Option<usize>,
    /// Introduced with `create`.
    pub created: bool,
    /// Ended with `destroy`.
    pub destroyed: bool,
}

/// A `box` around participants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqBox {
    /// Box title.
    pub label: String,
    /// Box colour as written, such as `rgb(33,66,99)` or `Aqua`.
    pub color: Option<String>,
}

/// The arrow of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageArrow {
    /// `->`: solid line, no head.
    Solid,
    /// `-->`: dotted line, no head.
    Dotted,
    /// `->>`: solid line with an arrowhead.
    SolidArrow,
    /// `-->>`: dotted line with an arrowhead.
    DottedArrow,
    /// `-x`: solid line with a cross.
    SolidCross,
    /// `--x`: dotted line with a cross.
    DottedCross,
    /// `-)`: solid line with an open (async) arrowhead.
    SolidAsync,
    /// `--)`: dotted line with an open (async) arrowhead.
    DottedAsync,
    /// `<<->>`: solid line with arrowheads at both ends.
    BidirectionalSolid,
    /// `<<-->>`: dotted line with arrowheads at both ends.
    BidirectionalDotted,
}

impl MessageArrow {
    /// Whether the line is dotted.
    pub fn dotted(self) -> bool {
        matches!(
            self,
            MessageArrow::Dotted
                | MessageArrow::DottedArrow
                | MessageArrow::DottedCross
                | MessageArrow::DottedAsync
                | MessageArrow::BidirectionalDotted
        )
    }
}

/// Arrow tokens, longest first so `-->>` wins over `-->` and `->`.
const ARROWS: &[(&str, MessageArrow)] = &[
    ("<<-->>", MessageArrow::BidirectionalDotted),
    ("<<->>", MessageArrow::BidirectionalSolid),
    ("-->>", MessageArrow::DottedArrow),
    ("--x", MessageArrow::DottedCross),
    ("--)", MessageArrow::DottedAsync),
    ("-->", MessageArrow::Dotted),
    ("->>", MessageArrow::SolidArrow),
    ("-x", MessageArrow::SolidCross),
    ("-)", MessageArrow::SolidAsync),
    ("->", MessageArrow::Solid),
];

/// A message between two participants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Sender id.
    pub from: String,
    /// Receiver id.
    pub to: String,
    /// Arrow kind.
    pub arrow: MessageArrow,
    /// Message text.
    pub text: String,
    /// `+` before the receiver: activate it.
    pub activate: bool,
    /// `-` before the receiver: deactivate the sender.
    pub deactivate: bool,
}

/// Where a note sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotePlacement {
    /// `left of`.
    LeftOf,
    /// `right of`.
    RightOf,
    /// `over`, spanning one or two participants.
    Over,
}

/// A note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqNote {
    /// Placement.
    pub placement: NotePlacement,
    /// One participant, or two for a note over both.
    pub participants: Vec<String>,
    /// Note text.
    pub text: String,
}

/// A block kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    /// `loop`.
    Loop,
    /// `alt`.
    Alt,
    /// `opt`.
    Opt,
    /// `par`.
    Par,
    /// `critical`.
    Critical,
    /// `break`.
    Break,
    /// `rect`: a background highlight; the label holds the colour.
    Rect,
}

/// A branch inside a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BranchKind {
    /// `else` inside `alt`.
    Else,
    /// `and` inside `par`.
    And,
    /// `option` inside `critical`.
    Option,
}

/// One event of a sequence diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeqItem {
    /// A message.
    Message(Message),
    /// A note.
    Note(SeqNote),
    /// `activate X`.
    Activate(String),
    /// `deactivate X`.
    Deactivate(String),
    /// `destroy X`.
    Destroy(String),
    /// Start of a block.
    BlockStart {
        /// Block kind.
        kind: BlockKind,
        /// Condition or title text.
        label: String,
    },
    /// A branch of the innermost block.
    Branch {
        /// Branch kind.
        kind: BranchKind,
        /// Condition text.
        label: String,
    },
    /// End of the innermost block.
    BlockEnd,
}

/// Something `end` can close.
enum Open {
    Box,
    Block(BlockKind),
}

struct Builder {
    d: SequenceDiagram,
    index: HashMap<String, usize>,
    stack: Vec<(Open, usize)>,
    current_box: Option<usize>,
    edges: usize,
}

/// Parse a sequence diagram from preprocessed input.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<SequenceDiagram, ParseError> {
    let mut b = Builder {
        d: SequenceDiagram { meta: prep.meta.clone(), ..SequenceDiagram::default() },
        index: HashMap::new(),
        stack: Vec::new(),
        current_box: None,
        edges: 0,
    };
    let lines = prep.all_lines();
    for stmt in split_statements(&lines, true) {
        ctx.statement(stmt.line)?;
        let r = b.statement(&stmt.text, stmt.line, ctx);
        settle(ctx, &stmt, r)?;
    }
    if let Some((open, line)) = b.stack.last() {
        let what = match open {
            Open::Box => "box",
            Open::Block(_) => "block",
        };
        return Err(ParseError::new(*line, 1, format!("{what} is never closed with `end`")));
    }
    Ok(b.d)
}

/// Colour names accepted as the first word of a `box` line.
const BOX_COLORS: &[&str] = &[
    "transparent",
    "aqua",
    "black",
    "blue",
    "fuchsia",
    "gray",
    "grey",
    "green",
    "lime",
    "maroon",
    "navy",
    "olive",
    "orange",
    "purple",
    "red",
    "silver",
    "teal",
    "white",
    "yellow",
    "lightblue",
    "lightgreen",
    "lightgrey",
    "lightgray",
    "lightyellow",
    "pink",
];

impl Builder {
    fn ensure(&mut self, id: &str, line: usize) -> Result<usize, ParseError> {
        if let Some(&i) = self.index.get(id) {
            return Ok(i);
        }
        check_nodes(self.d.participants.len(), line)?;
        let i = self.d.participants.len();
        self.d.participants.push(Participant {
            id: id.to_string(),
            label: id.to_string(),
            kind: ParticipantKind::Participant,
            in_box: self.current_box,
            created: false,
            destroyed: false,
        });
        self.index.insert(id.to_string(), i);
        Ok(i)
    }

    fn push_item(&mut self, item: SeqItem, line: usize) -> Result<(), ParseError> {
        check_edges(self.edges, line)?;
        self.edges = self.edges.saturating_add(1);
        self.d.items.push(item);
        Ok(())
    }

    fn open_block(&mut self, kind: BlockKind, label: &str, line: usize) -> StmtResult<()> {
        check_depth(self.stack.len(), line, 1)?;
        self.stack.push((Open::Block(kind), line));
        self.push_item(SeqItem::BlockStart { kind, label: clean_label(label) }, line)?;
        Ok(())
    }

    fn branch(&mut self, kind: BranchKind, parent: BlockKind, label: &str, line: usize) -> StmtResult<()> {
        let ok = matches!(self.stack.last(), Some((Open::Block(k), _)) if *k == parent);
        if !ok {
            return Err(StmtError::Unknown(0, "branch keyword outside its block".into()));
        }
        self.push_item(SeqItem::Branch { kind, label: clean_label(label) }, line)?;
        Ok(())
    }

    fn statement(&mut self, t: &str, line: usize, ctx: &mut Ctx) -> StmtResult<()> {
        // `title: Text` has no blank after the keyword, so check it before splitting words.
        if let Some(r) = t.strip_prefix("title")
            && (r.starts_with(':') || r.starts_with(char::is_whitespace))
        {
            let title = r.trim_start().strip_prefix(':').unwrap_or(r).trim();
            self.d.meta.title = Some(title.to_string());
            return Ok(());
        }
        let (kw, rest) = first_word(t);
        match kw {
            "participant" | "actor" => return self.participant(kw, rest, false, line, ctx),
            "create" => {
                let (kw2, rest2) = first_word(rest);
                if kw2 != "participant" && kw2 != "actor" {
                    return Err(StmtError::Unknown(0, "`create` must be followed by `participant` or `actor`".into()));
                }
                return self.participant(kw2, rest2, true, line, ctx);
            }
            "destroy" => {
                if rest.is_empty() {
                    return Err(StmtError::Unknown(0, "`destroy` needs a participant".into()));
                }
                let i = self.ensure(rest, line)?;
                if let Some(p) = self.d.participants.get_mut(i) {
                    p.destroyed = true;
                }
                self.push_item(SeqItem::Destroy(rest.to_string()), line)?;
                return Ok(());
            }
            "box" => {
                if self.current_box.is_some() {
                    return Err(StmtError::Unknown(0, "boxes cannot be nested".into()));
                }
                check_depth(self.stack.len(), line, 1)?;
                let (color, label) = box_header(rest);
                self.current_box = Some(self.d.boxes.len());
                self.d.boxes.push(SeqBox { label, color });
                self.stack.push((Open::Box, line));
                return Ok(());
            }
            "end" if rest.is_empty() => {
                match self.stack.pop() {
                    Some((Open::Box, _)) => self.current_box = None,
                    Some((Open::Block(_), _)) => self.push_item(SeqItem::BlockEnd, line)?,
                    None => return Err(StmtError::Unknown(0, "`end` without an open block".into())),
                }
                return Ok(());
            }
            "autonumber" => {
                self.d.autonumber = true;
                return Ok(());
            }
            "activate" | "deactivate" => {
                if rest.is_empty() {
                    return Err(StmtError::Unknown(0, format!("`{kw}` needs a participant")));
                }
                self.ensure(rest, line)?;
                let item = if kw == "activate" { SeqItem::Activate(rest.to_string()) } else { SeqItem::Deactivate(rest.to_string()) };
                self.push_item(item, line)?;
                return Ok(());
            }
            "loop" => return self.open_block(BlockKind::Loop, rest, line),
            "alt" => return self.open_block(BlockKind::Alt, rest, line),
            "opt" => return self.open_block(BlockKind::Opt, rest, line),
            "par" | "par_over" => return self.open_block(BlockKind::Par, rest, line),
            "critical" => return self.open_block(BlockKind::Critical, rest, line),
            "break" => return self.open_block(BlockKind::Break, rest, line),
            "rect" => return self.open_block(BlockKind::Rect, rest, line),
            "else" => return self.branch(BranchKind::Else, BlockKind::Alt, rest, line),
            "and" => return self.branch(BranchKind::And, BlockKind::Par, rest, line),
            "option" => return self.branch(BranchKind::Option, BlockKind::Critical, rest, line),
            "link" | "links" | "properties" | "details" => {
                return Err(StmtError::Unknown(0, format!("participant `{kw}` menus are not supported")));
            }
            _ => {}
        }
        if kw.eq_ignore_ascii_case("note") {
            return self.note(rest, line);
        }
        self.message(t, line)
    }

    fn participant(&mut self, kw: &str, rest: &str, created: bool, line: usize, ctx: &mut Ctx) -> StmtResult<()> {
        let (id, label) = match rest.split_once(" as ") {
            Some((id, alias)) => (id.trim(), Some(clean_label(alias))),
            None => (rest.trim(), None),
        };
        // The newer `participant A@{ "type": "database" }` form: keep the id, drop the type.
        let id = match id.split_once("@{") {
            Some((base, _)) => {
                ctx.warn(line, format!("participant type on `{}` is not supported; drawn as a participant", base.trim()));
                base.trim()
            }
            None => id,
        };
        if id.is_empty() {
            return Err(StmtError::Unknown(0, format!("`{kw}` needs an id")));
        }
        let i = self.ensure(id, line)?;
        if let Some(p) = self.d.participants.get_mut(i) {
            if let Some(l) = label {
                p.label = l;
            }
            p.kind = if kw == "actor" { ParticipantKind::Actor } else { ParticipantKind::Participant };
            p.created |= created;
            if p.in_box.is_none() {
                p.in_box = self.current_box;
            }
        }
        Ok(())
    }

    fn note(&mut self, rest: &str, line: usize) -> StmtResult<()> {
        let Some((head, text)) = rest.split_once(':') else {
            return Err(StmtError::Unknown(0, "notes are written `Note right of A: text`".into()));
        };
        let head = head.trim();
        let (placement, who) = if let Some(w) = head.strip_prefix("left of ") {
            (NotePlacement::LeftOf, w)
        } else if let Some(w) = head.strip_prefix("right of ") {
            (NotePlacement::RightOf, w)
        } else if let Some(w) = head.strip_prefix("over ") {
            (NotePlacement::Over, w)
        } else {
            return Err(StmtError::Unknown(0, "note placement must be `left of`, `right of` or `over`".into()));
        };
        let participants: Vec<String> = who.split(',').map(str::trim).filter(|s| !s.is_empty()).take(2).map(str::to_string).collect();
        if participants.is_empty() {
            return Err(StmtError::Unknown(0, "note needs a participant".into()));
        }
        for p in &participants {
            self.ensure(p, line)?;
        }
        self.push_item(SeqItem::Note(SeqNote { placement, participants, text: clean_label(text) }), line)?;
        Ok(())
    }

    fn message(&mut self, t: &str, line: usize) -> StmtResult<()> {
        let Some((at, token, arrow)) = find_arrow(t) else {
            return Err(StmtError::Unknown(0, "not a sequence diagram statement".into()));
        };
        let from = t.get(..at).unwrap_or("").trim();
        if from.is_empty() {
            return Err(StmtError::Malformed(at, "message has no sender".into()));
        }
        let after_off = at.saturating_add(token.len());
        let mut after = t.get(after_off..).unwrap_or("").trim_start();
        let mut activate = false;
        let mut deactivate = false;
        if let Some(a) = after.strip_prefix('+') {
            activate = true;
            after = a.trim_start();
        } else if let Some(a) = after.strip_prefix('-') {
            deactivate = true;
            after = a.trim_start();
        }
        let (to, text) = match after.split_once(':') {
            Some((to, text)) => (to.trim(), clean_label(text)),
            None => (after.trim(), String::new()),
        };
        if to.is_empty() {
            return Err(StmtError::Malformed(after_off, "message has no receiver".into()));
        }
        self.ensure(from, line)?;
        self.ensure(to, line)?;
        self.push_item(SeqItem::Message(Message { from: from.to_string(), to: to.to_string(), arrow, text, activate, deactivate }), line)?;
        Ok(())
    }
}

/// The first message arrow in `t`: byte offset, token and kind.
fn find_arrow(t: &str) -> Option<(usize, &'static str, MessageArrow)> {
    for (i, c) in t.char_indices() {
        if c == ':' {
            // Message text starts here; an arrow after it is just text.
            return None;
        }
        if c != '-' && c != '<' {
            continue;
        }
        let rest = t.get(i..)?;
        for (tok, kind) in ARROWS {
            if rest.starts_with(tok) {
                return Some((i, tok, *kind));
            }
        }
    }
    None
}

/// Split a `box` line into an optional colour and the title.
fn box_header(rest: &str) -> (Option<String>, String) {
    let rest = rest.trim();
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("rgb")
        && let Some(close) = rest.find(')')
    {
        let color = rest.get(..close.saturating_add(1)).unwrap_or("").to_string();
        let label = clean_label(rest.get(close.saturating_add(1)..).unwrap_or(""));
        return (Some(color), label);
    }
    let (first, after) = first_word(rest);
    if BOX_COLORS.contains(&first.to_ascii_lowercase().as_str()) {
        return (Some(first.to_string()), clean_label(after));
    }
    (None, clean_label(rest))
}

impl SequenceDiagram {
    /// The uniform graph view: participants as nodes from left to right, boxes as groups,
    /// messages as edges in order, notes as note nodes. Block structure and activations are only
    /// in [`SequenceDiagram::items`].
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::Sequence, Direction::LeftToRight, self.meta.title.clone());
        for (i, b) in self.boxes.iter().enumerate() {
            let mut grp = GraphGroup::new(format!("box{i}"), b.label.clone(), GroupKind::Box);
            grp.members = self.participants.iter().filter(|p| p.in_box == Some(i)).map(|p| p.id.clone()).collect();
            if let Some(c) = &b.color {
                grp.style.push(crate::common::StyleProp::new("fill", c.clone()));
            }
            g.groups.push(grp);
        }
        for p in &self.participants {
            let shape = match p.kind {
                ParticipantKind::Participant => Shape::Participant,
                ParticipantKind::Actor => Shape::Actor,
            };
            let mut node = GraphNode::new(p.id.clone(), p.label.clone(), shape);
            node.group = p.in_box.map(|i| format!("box{i}"));
            g.nodes.push(node);
        }
        let mut notes = 0usize;
        for item in &self.items {
            match item {
                SeqItem::Message(m) => {
                    let mut e = GraphEdge::new(m.from.clone(), m.to.clone());
                    e.label = (!m.text.is_empty()).then(|| m.text.clone());
                    e.line = if m.arrow.dotted() { LineStyle::Dashed } else { LineStyle::Solid };
                    e.end = match m.arrow {
                        MessageArrow::Solid | MessageArrow::Dotted => ArrowKind::None,
                        MessageArrow::SolidArrow
                        | MessageArrow::DottedArrow
                        | MessageArrow::BidirectionalSolid
                        | MessageArrow::BidirectionalDotted => ArrowKind::Arrow,
                        MessageArrow::SolidCross | MessageArrow::DottedCross => ArrowKind::Cross,
                        MessageArrow::SolidAsync | MessageArrow::DottedAsync => ArrowKind::OpenArrow,
                    };
                    if matches!(m.arrow, MessageArrow::BidirectionalSolid | MessageArrow::BidirectionalDotted) {
                        e.start = ArrowKind::Arrow;
                    }
                    g.edges.push(e);
                }
                SeqItem::Note(n) => {
                    let id = format!("note{notes}");
                    notes = notes.saturating_add(1);
                    g.nodes.push(GraphNode::new(id.clone(), n.text.clone(), Shape::Note));
                    if let Some(target) = n.participants.first() {
                        let mut e = GraphEdge::new(id, target.clone());
                        e.line = LineStyle::Dotted;
                        g.edges.push(e);
                    }
                }
                _ => {}
            }
        }
        g
    }
}
