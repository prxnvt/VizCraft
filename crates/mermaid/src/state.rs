//! State diagrams: `stateDiagram` and `stateDiagram-v2`.
//!
//! One statement per line: states (`s1`, `state "Long name" as s1`, `s1 : description`),
//! transitions (`s1 --> s2 : label`, with `[*]` as start or end), composite states
//! (`state X { ... }`) with `--` concurrency separators, `<<choice>>`, `<<fork>>` and
//! `<<join>>` pseudo states, notes, `direction`, and `classDef` / `class` / `:::` styling.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::common::{
    ClassDef, Ctx, Cursor, Direction, Meta, ParseError, Prepared, Stmt, StmtError, StmtResult, StyleProp, add_class_def, check_depth, check_edges,
    check_nodes, clean_label, first_word, parse_style_props, push_unique, settle,
};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, LineStyle, Shape};

/// A parsed state diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StateDiagram {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Overall direction.
    pub direction: Direction,
    /// States in order of first mention, including generated start and end states.
    pub states: Vec<State>,
    /// Transitions in source order.
    pub transitions: Vec<Transition>,
    /// Notes attached to states.
    pub notes: Vec<StateNote>,
    /// `classDef` statements.
    pub class_defs: Vec<ClassDef>,
}

/// What kind of state a [`State`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StateKind {
    /// An ordinary state.
    #[default]
    Simple,
    /// A state with nested states (`state X { ... }`).
    Composite,
    /// A `[*]` used as a transition source.
    Start,
    /// A `[*]` used as a transition target.
    End,
    /// `<<choice>>`.
    Choice,
    /// `<<fork>>`.
    Fork,
    /// `<<join>>`.
    Join,
}

/// A state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// State id. Start and end states get generated ids ending in `[*]start` and `[*]end`,
    /// which no Mermaid id can collide with.
    pub id: String,
    /// Display name from `state "name" as id`.
    pub label: Option<String>,
    /// Description lines from `id : text`, in order.
    pub descriptions: Vec<String>,
    /// Kind of state.
    pub kind: StateKind,
    /// Enclosing composite state.
    pub parent: Option<String>,
    /// Zero-based concurrent region inside the parent.
    pub region: usize,
    /// Number of concurrent regions, for composite states (1 when there is no `--`).
    pub regions: usize,
    /// Direction requested inside a composite state.
    pub direction: Option<Direction>,
    /// Style classes from `class` and `:::`.
    pub classes: Vec<String>,
    /// Properties from `style`.
    pub style: Vec<StyleProp>,
}

/// A transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    /// Source state id.
    pub from: String,
    /// Target state id.
    pub to: String,
    /// Transition label.
    pub label: Option<String>,
}

/// Which side of its state a note sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteSide {
    /// `note left of`.
    Left,
    /// `note right of`.
    Right,
}

/// A note attached to a state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateNote {
    /// The state it belongs to.
    pub target: String,
    /// Side of the state.
    pub side: NoteSide,
    /// Note text; lines joined with `\n`.
    pub text: String,
}

/// One end of a transition before it is resolved.
#[derive(Debug)]
enum End {
    Pseudo,
    Id(String, Vec<String>),
}

struct Frame {
    idx: usize,
    region: usize,
    line: usize,
}

struct Builder {
    d: StateDiagram,
    index: HashMap<String, usize>,
    stack: Vec<Frame>,
}

/// Parse a state diagram from preprocessed input.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<StateDiagram, ParseError> {
    let mut b = Builder { d: StateDiagram { meta: prep.meta.clone(), ..StateDiagram::default() }, index: HashMap::new(), stack: Vec::new() };
    let lines = prep.all_lines();
    let mut i = 0usize;
    while let Some(l) = lines.get(i) {
        i = i.saturating_add(1);
        let stmt = Stmt::from_line(l);
        ctx.statement(stmt.line)?;
        if first_word(&stmt.text).0 == "note" {
            let r = b.note(&stmt, &lines, &mut i);
            settle(ctx, &stmt, r)?;
            continue;
        }
        let r = b.statement(&stmt.text, stmt.line);
        settle(ctx, &stmt, r)?;
    }
    if let Some(f) = b.stack.last() {
        let id = b.d.states.get(f.idx).map(|s| s.id.clone()).unwrap_or_default();
        return Err(ParseError::new(f.line, 1, format!("composite state `{id}` is never closed with `}}`")));
    }
    Ok(b.d)
}

impl Builder {
    fn scope(&self) -> Option<(String, usize)> {
        let f = self.stack.last()?;
        let s = self.d.states.get(f.idx)?;
        Some((s.id.clone(), f.region))
    }

    fn ensure(&mut self, id: &str, line: usize) -> Result<usize, ParseError> {
        if let Some(&i) = self.index.get(id) {
            return Ok(i);
        }
        check_nodes(self.d.states.len().saturating_add(self.d.notes.len()), line)?;
        let (parent, region) = match self.scope() {
            Some((p, r)) => (Some(p), r),
            None => (None, 0),
        };
        let i = self.d.states.len();
        self.d.states.push(State {
            id: id.to_string(),
            label: None,
            descriptions: Vec::new(),
            kind: StateKind::Simple,
            parent,
            region,
            regions: 0,
            direction: None,
            classes: Vec::new(),
            style: Vec::new(),
        });
        self.index.insert(id.to_string(), i);
        Ok(i)
    }

    /// The generated start or end state of the current scope.
    fn pseudo(&mut self, start: bool, line: usize) -> Result<String, ParseError> {
        let suffix = if start { "[*]start" } else { "[*]end" };
        let id = match self.scope() {
            None => suffix.to_string(),
            Some((p, 0)) => format!("{p}/{suffix}"),
            Some((p, r)) => format!("{p}/region{r}/{suffix}"),
        };
        let i = self.ensure(&id, line)?;
        if let Some(s) = self.d.states.get_mut(i) {
            s.kind = if start { StateKind::Start } else { StateKind::End };
        }
        Ok(id)
    }

    fn statement(&mut self, t: &str, line: usize) -> StmtResult<()> {
        if t == "}" {
            if self.stack.pop().is_none() {
                return Err(StmtError::Unknown(0, "`}` without an open composite state".into()));
            }
            return Ok(());
        }
        if t == "--" {
            let Some(f) = self.stack.last_mut() else {
                return Err(StmtError::Unknown(0, "`--` concurrency separator outside a composite state".into()));
            };
            f.region = f.region.saturating_add(1);
            let (idx, region) = (f.idx, f.region);
            if let Some(s) = self.d.states.get_mut(idx) {
                s.regions = s.regions.max(region.saturating_add(1));
            }
            return Ok(());
        }
        let (kw, rest) = first_word(t);
        match kw {
            "direction" => {
                let Some(d) = Direction::from_keyword(rest) else {
                    return Err(StmtError::Unknown(0, format!("`{rest}` is not a direction")));
                };
                match self.stack.last().map(|f| f.idx) {
                    Some(i) => {
                        if let Some(s) = self.d.states.get_mut(i) {
                            s.direction = Some(d);
                        }
                    }
                    None => self.d.direction = d,
                }
                return Ok(());
            }
            "classDef" => {
                let (names, props) = first_word(rest);
                if names.is_empty() {
                    return Err(StmtError::Unknown(0, "`classDef` needs a class name".into()));
                }
                add_class_def(&mut self.d.class_defs, names, props);
                return Ok(());
            }
            "class" => {
                let (ids, rest2) = first_word(rest);
                let (class, _) = first_word(rest2);
                if ids.is_empty() || class.is_empty() {
                    return Err(StmtError::Unknown(0, "`class` needs state ids and a class name".into()));
                }
                for id in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let i = self.ensure(id, line)?;
                    if let Some(s) = self.d.states.get_mut(i) {
                        push_unique(&mut s.classes, class);
                    }
                }
                return Ok(());
            }
            "style" => {
                let (id, props) = first_word(rest);
                if id.is_empty() {
                    return Err(StmtError::Unknown(0, "`style` needs a state id".into()));
                }
                let i = self.ensure(id, line)?;
                if let Some(s) = self.d.states.get_mut(i) {
                    s.style.extend(parse_style_props(props));
                }
                return Ok(());
            }
            // Rendering options with no effect on the diagram's content.
            "hide" | "scale" => return Ok(()),
            "state" => return self.state_decl(t, rest, line),
            _ => {}
        }
        if let Some(arrow) = t.find("-->") {
            return self.transition(t, arrow, line);
        }
        self.id_statement(t, line)
    }

    /// `state "Name" as id`, `state id <<fork>>`, `state id {`, `state id : desc`.
    fn state_decl(&mut self, t: &str, rest: &str, line: usize) -> StmtResult<()> {
        let offset = t.len().saturating_sub(rest.len());
        let mut cur = Cursor::new(rest);
        let mut label = None;
        let id;
        if cur.peek() == Some('"') {
            let Some(q) = cur.quoted() else {
                return Err(StmtError::Malformed(offset, "state name is missing its closing quote".into()));
            };
            label = Some(clean_label(q));
            cur.skip_ws();
            if !cur.eat("as") {
                return Err(StmtError::Unknown(offset, "expected `as <id>` after the state name".into()));
            }
            cur.skip_ws();
            id = cur.word(true, true).to_string();
        } else {
            id = cur.word(true, true).to_string();
            // `state id as "Name"` is accepted too.
            let save = cur.pos();
            cur.skip_ws();
            if cur.eat("as ") {
                cur.skip_ws();
                match cur.quoted() {
                    Some(q) => label = Some(clean_label(q)),
                    None => cur.set_pos(save),
                }
            } else {
                cur.set_pos(save);
            }
        }
        if id.is_empty() {
            return Err(StmtError::Unknown(offset, "`state` needs an id".into()));
        }
        let mut classes = Vec::new();
        let mut kind = None;
        let mut open = false;
        let mut description = None;
        loop {
            cur.skip_ws();
            if cur.is_eof() {
                break;
            }
            if cur.eat(":::") {
                let c = cur.word(true, false);
                push_unique(&mut classes, c);
                continue;
            }
            if cur.eat("<<") {
                let r = cur.rest();
                let Some(end) = r.find(">>") else {
                    return Err(StmtError::Malformed(offset.saturating_add(cur.pos()), "`<<` is never closed with `>>`".into()));
                };
                let name = r.get(..end).unwrap_or("").trim().to_ascii_lowercase();
                cur.advance(end.saturating_add(2));
                kind = Some(match name.as_str() {
                    "choice" => StateKind::Choice,
                    "fork" => StateKind::Fork,
                    "join" => StateKind::Join,
                    other => return Err(StmtError::Unknown(offset, format!("`<<{other}>>` is not a supported state type"))),
                });
                continue;
            }
            if cur.eat("{") {
                open = true;
                cur.skip_ws();
                if !cur.is_eof() {
                    return Err(StmtError::Unknown(offset.saturating_add(cur.pos()), "text after `{` must go on its own line".into()));
                }
                break;
            }
            if cur.eat(":") {
                description = Some(clean_label(cur.rest()));
                break;
            }
            return Err(StmtError::Unknown(offset.saturating_add(cur.pos()), "unexpected text in a state declaration".into()));
        }
        if open {
            check_depth(self.stack.len(), line, 1)?;
        }
        let i = self.ensure(&id, line)?;
        if let Some(s) = self.d.states.get_mut(i) {
            if label.is_some() {
                s.label = label;
            }
            if let Some(k) = kind {
                s.kind = k;
            }
            if let Some(d) = description {
                s.descriptions.push(d);
            }
            for c in &classes {
                push_unique(&mut s.classes, c);
            }
            if open {
                s.kind = StateKind::Composite;
                s.regions = s.regions.max(1);
            }
        }
        if open {
            self.stack.push(Frame { idx: i, region: 0, line });
        }
        Ok(())
    }

    fn transition(&mut self, t: &str, arrow: usize, line: usize) -> StmtResult<()> {
        let left_text = t.get(..arrow).unwrap_or("");
        let right_off = arrow.saturating_add(3);
        let right_text = t.get(right_off..).unwrap_or("");
        let mut lc = Cursor::new(left_text.trim());
        let Some(from) = parse_end(&mut lc) else {
            return Err(StmtError::Malformed(0, "transition has no source state".into()));
        };
        lc.skip_ws();
        if !lc.is_eof() {
            return Err(StmtError::Unknown(0, "unexpected text before `-->`".into()));
        }
        let mut rc = Cursor::new(right_text);
        rc.skip_ws();
        let Some(to) = parse_end(&mut rc) else {
            return Err(StmtError::Malformed(right_off, "transition has no target state".into()));
        };
        rc.skip_ws();
        let label = if rc.eat(":") {
            let l = clean_label(rc.rest());
            (!l.is_empty()).then_some(l)
        } else if rc.is_eof() {
            None
        } else {
            return Err(StmtError::Unknown(right_off.saturating_add(rc.pos()), "unexpected text after the target state".into()));
        };
        check_edges(self.d.transitions.len(), line)?;
        let from = self.resolve(from, true, line)?;
        let to = self.resolve(to, false, line)?;
        self.d.transitions.push(Transition { from, to, label });
        Ok(())
    }

    fn resolve(&mut self, end: End, start: bool, line: usize) -> Result<String, ParseError> {
        match end {
            End::Pseudo => self.pseudo(start, line),
            End::Id(id, classes) => {
                let i = self.ensure(&id, line)?;
                if let Some(s) = self.d.states.get_mut(i) {
                    for c in &classes {
                        push_unique(&mut s.classes, c);
                    }
                }
                Ok(id)
            }
        }
    }

    /// `id`, `id:::class` or `id : description`.
    fn id_statement(&mut self, t: &str, line: usize) -> StmtResult<()> {
        let mut cur = Cursor::new(t);
        let id = cur.word(true, true);
        if id.is_empty() {
            return Err(StmtError::Unknown(0, "not a state diagram statement".into()));
        }
        let mut classes = Vec::new();
        while cur.eat(":::") {
            push_unique(&mut classes, cur.word(true, false));
        }
        cur.skip_ws();
        let description = if cur.eat(":") {
            Some(clean_label(cur.rest()))
        } else if cur.is_eof() {
            None
        } else {
            return Err(StmtError::Unknown(cur.pos(), "not a state diagram statement".into()));
        };
        let i = self.ensure(id, line)?;
        if let Some(s) = self.d.states.get_mut(i) {
            for c in &classes {
                push_unique(&mut s.classes, c);
            }
            if let Some(d) = description.filter(|d| !d.is_empty()) {
                s.descriptions.push(d);
            }
        }
        Ok(())
    }

    /// `note left of X : text`, or `note right of X` followed by lines up to `end note`.
    fn note(&mut self, stmt: &Stmt, lines: &[crate::common::SrcLine<'_>], i: &mut usize) -> StmtResult<()> {
        let rest = first_word(&stmt.text).1;
        let (head, inline) = match rest.split_once(':') {
            Some((h, text)) => (h, Some(text)),
            None => (rest, None),
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        let (side, target) = match words.as_slice() {
            ["left", "of", target] => (NoteSide::Left, *target),
            ["right", "of", target] => (NoteSide::Right, *target),
            _ => return Err(StmtError::Unknown(0, "notes must be `note left of <state>` or `note right of <state>`".into())),
        };
        let text = match inline {
            Some(t) => clean_label(t),
            None => {
                let mut body = Vec::new();
                let mut closed = false;
                while let Some(l) = lines.get(*i) {
                    *i = i.saturating_add(1);
                    if l.text == "end note" {
                        closed = true;
                        break;
                    }
                    body.push(l.text);
                }
                if !closed {
                    return Err(StmtError::Malformed(0, "note is never closed with `end note`".into()));
                }
                clean_label(&body.join("\n"))
            }
        };
        check_nodes(self.d.states.len().saturating_add(self.d.notes.len()), stmt.line)?;
        self.ensure(target, stmt.line)?;
        self.d.notes.push(StateNote { target: target.to_string(), side, text });
        Ok(())
    }
}

/// `[*]`, or a state id with optional `:::class`es.
fn parse_end(cur: &mut Cursor<'_>) -> Option<End> {
    if cur.eat("[*]") {
        return Some(End::Pseudo);
    }
    let id = cur.word(true, true);
    if id.is_empty() {
        return None;
    }
    let mut classes = Vec::new();
    while cur.eat(":::") {
        push_unique(&mut classes, cur.word(true, false));
    }
    Some(End::Id(id.to_string(), classes))
}

impl StateDiagram {
    /// The group a child of `parent` in `region` belongs to: the composite itself, or one of
    /// its region groups when it has concurrent regions.
    fn group_of(&self, parent: Option<&String>, region: usize) -> Option<String> {
        let p = parent?;
        let regions = self.states.iter().find(|s| &s.id == p).map_or(1, |s| s.regions);
        if regions > 1 { Some(format!("{p}/region{region}")) } else { Some(p.clone()) }
    }

    /// The uniform graph view. Composite states become groups (with one child group per
    /// concurrent region), every other state a node, notes become note nodes joined to their
    /// state by a dotted edge.
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::State, self.direction, self.meta.title.clone());
        for s in &self.states {
            let group = self.group_of(s.parent.as_ref(), s.region);
            let display = s.label.clone().unwrap_or_else(|| if s.descriptions.is_empty() { s.id.clone() } else { s.descriptions.join("\n") });
            if s.kind == StateKind::Composite {
                let mut grp = GraphGroup::new(s.id.clone(), display, GroupKind::CompositeState);
                grp.parent = group;
                grp.direction = s.direction;
                grp.classes.clone_from(&s.classes);
                grp.style.clone_from(&s.style);
                g.groups.push(grp);
                if s.regions > 1 {
                    for r in 0..s.regions {
                        let mut region = GraphGroup::new(format!("{}/region{r}", s.id), String::new(), GroupKind::Region);
                        region.parent = Some(s.id.clone());
                        g.groups.push(region);
                    }
                }
                continue;
            }
            let (shape, label, annotation) = match s.kind {
                StateKind::Start => (Shape::Start, String::new(), None),
                StateKind::End => (Shape::End, String::new(), None),
                StateKind::Choice => (Shape::Choice, String::new(), Some("choice")),
                StateKind::Fork => (Shape::ForkBar, String::new(), Some("fork")),
                StateKind::Join => (Shape::ForkBar, String::new(), Some("join")),
                StateKind::Simple | StateKind::Composite => (Shape::Rounded, display, None),
            };
            let mut node = GraphNode::new(s.id.clone(), label, shape);
            if let Some(a) = annotation {
                node.annotations.push(a.to_string());
            }
            if s.label.is_some() {
                node.details.clone_from(&s.descriptions);
            }
            node.classes.clone_from(&s.classes);
            node.style.clone_from(&s.style);
            node.group = group;
            g.nodes.push(node);
        }
        // Group membership in node order.
        let memberships: Vec<(String, String)> = g.nodes.iter().filter_map(|n| Some((n.group.clone()?, n.id.clone()))).collect();
        for (gid, nid) in memberships {
            if let Some(grp) = g.groups.iter_mut().find(|x| x.id == gid) {
                grp.members.push(nid);
            }
        }
        for t in &self.transitions {
            let mut e = GraphEdge::new(t.from.clone(), t.to.clone());
            e.label.clone_from(&t.label);
            e.end = ArrowKind::Arrow;
            g.edges.push(e);
        }
        for (i, n) in self.notes.iter().enumerate() {
            let id = format!("note{i}");
            let mut node = GraphNode::new(id.clone(), n.text.clone(), Shape::Note);
            let target_group = self.states.iter().find(|s| s.id == n.target).and_then(|s| self.group_of(s.parent.as_ref(), s.region));
            node.group.clone_from(&target_group);
            if let Some(gid) = &target_group
                && let Some(grp) = g.groups.iter_mut().find(|x| &x.id == gid)
            {
                grp.members.push(id.clone());
            }
            g.nodes.push(node);
            let mut e = GraphEdge::new(id, n.target.clone());
            e.line = LineStyle::Dotted;
            g.edges.push(e);
        }
        g.class_defs.clone_from(&self.class_defs);
        g
    }
}
