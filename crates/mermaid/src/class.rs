//! Class diagrams: `classDiagram`.
//!
//! Classes (`class Animal`, `class Square~Shape~`, `class A["Label"]`, with a `{ ... }` member
//! body or `Animal : +int age` member lines), annotations (`<<interface>>`), relationships with
//! optional cardinalities and labels (`A "1" *-- "many" B : contains`), namespaces, notes,
//! `direction`, styling (`classDef`, `cssClass`, `style`, `:::`) and `link` / `click` /
//! `callback` interaction hints.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::common::{
    ClassDef, Click, Ctx, Cursor, Direction, Meta, ParseError, Prepared, Stmt, StmtError, StmtResult, StyleProp, add_class_def, check_depth,
    check_edges, check_nodes, clean_label, first_word, is_word_char, parse_click, parse_style_props, push_unique, settle, tokens,
};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, LineStyle, Shape};

/// A parsed class diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ClassDiagram {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Direction from a `direction` statement (`TB` by default).
    pub direction: Direction,
    /// Classes in order of first mention.
    pub classes: Vec<Class>,
    /// Relationships in source order.
    pub relations: Vec<Relation>,
    /// Namespaces in the order they were opened.
    pub namespaces: Vec<Namespace>,
    /// Notes.
    pub notes: Vec<ClassNote>,
    /// `classDef` statements.
    pub class_defs: Vec<ClassDef>,
    /// `link`, `click` and `callback` statements.
    pub clicks: Vec<Click>,
}

/// A class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Class {
    /// Class name as used in relationships (without generics).
    pub id: String,
    /// Display label from `class A["Label"]`.
    pub label: Option<String>,
    /// Generic parameter text from `~T~`, with nested `~` turned into angle brackets.
    pub generic: Option<String>,
    /// Annotations such as `interface`, without angle brackets.
    pub annotations: Vec<String>,
    /// Attributes and methods in source order.
    pub members: Vec<Member>,
    /// Enclosing namespace.
    pub namespace: Option<String>,
    /// Style classes from `cssClass` and `:::`.
    pub classes: Vec<String>,
    /// Properties from `style`.
    pub style: Vec<StyleProp>,
}

impl Class {
    /// The name as displayed: the label, or the id with its generic in angle brackets.
    pub fn display_name(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        match &self.generic {
            Some(g) => format!("{}<{}>", self.id, g),
            None => self.id.clone(),
        }
    }
}

/// Member visibility marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// `+`
    Public,
    /// `-`
    Private,
    /// `#`
    Protected,
    /// `~`
    Package,
}

/// Whether a member is an attribute or a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberKind {
    /// A field.
    Attribute,
    /// An operation; has parentheses.
    Method,
}

/// Static or abstract marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Classifier {
    /// `$`: shown underlined.
    Static,
    /// `*`: shown in italics.
    Abstract,
}

/// A class member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Display text with generics as angle brackets and markers removed, for example
    /// `+getMessages(List<string> messages) : List<string>`.
    pub text: String,
    /// Visibility marker.
    pub visibility: Option<Visibility>,
    /// Attribute or method.
    pub kind: MemberKind,
    /// Member name.
    pub name: String,
    /// Parameter text for methods.
    pub parameters: Option<String>,
    /// Attribute type or method return type.
    pub ty: Option<String>,
    /// Static or abstract marker.
    pub classifier: Option<Classifier>,
}

/// The marker at one end of a class relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RelationEnd {
    /// No marker.
    #[default]
    None,
    /// `<|` or `|>`: hollow triangle.
    Inheritance,
    /// `*`: filled diamond.
    Composition,
    /// `o`: hollow diamond.
    Aggregation,
    /// `<` or `>`: arrowhead.
    Arrow,
    /// `()`: lollipop interface.
    Lollipop,
}

impl RelationEnd {
    /// The matching arrow kind.
    pub fn arrow(self) -> ArrowKind {
        match self {
            RelationEnd::None => ArrowKind::None,
            RelationEnd::Inheritance => ArrowKind::Triangle,
            RelationEnd::Composition => ArrowKind::DiamondFilled,
            RelationEnd::Aggregation => ArrowKind::DiamondHollow,
            RelationEnd::Arrow => ArrowKind::Arrow,
            RelationEnd::Lollipop => ArrowKind::Lollipop,
        }
    }
}

/// The OMG UML meaning of a relationship, derived from its ends and line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationKind {
    /// Solid line with a triangle.
    Inheritance,
    /// Dashed line with a triangle.
    Realization,
    /// Filled diamond.
    Composition,
    /// Hollow diamond.
    Aggregation,
    /// Solid line with an arrowhead.
    Association,
    /// Dashed line with an arrowhead.
    Dependency,
    /// Lollipop interface.
    Lollipop,
    /// Solid line, no markers.
    Link,
    /// Dashed line, no markers.
    DashedLink,
}

/// A relationship between two classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    /// First class as written.
    pub from: String,
    /// Second class as written.
    pub to: String,
    /// Marker at the first class.
    pub from_end: RelationEnd,
    /// Marker at the second class.
    pub to_end: RelationEnd,
    /// Dashed (`..`) rather than solid (`--`).
    pub dashed: bool,
    /// Cardinality text at the first class, such as `1`.
    pub from_cardinality: Option<String>,
    /// Cardinality text at the second class, such as `many`.
    pub to_cardinality: Option<String>,
    /// Relationship label.
    pub label: Option<String>,
}

impl Relation {
    /// The UML meaning of this relationship.
    pub fn kind(&self) -> RelationKind {
        let ends = [self.from_end, self.to_end];
        let has = |e: RelationEnd| ends.contains(&e);
        if has(RelationEnd::Inheritance) {
            return if self.dashed { RelationKind::Realization } else { RelationKind::Inheritance };
        }
        if has(RelationEnd::Composition) {
            return RelationKind::Composition;
        }
        if has(RelationEnd::Aggregation) {
            return RelationKind::Aggregation;
        }
        if has(RelationEnd::Lollipop) {
            return RelationKind::Lollipop;
        }
        if has(RelationEnd::Arrow) {
            return if self.dashed { RelationKind::Dependency } else { RelationKind::Association };
        }
        if self.dashed { RelationKind::DashedLink } else { RelationKind::Link }
    }
}

/// A namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Namespace {
    /// Namespace name.
    pub id: String,
    /// Enclosing namespace.
    pub parent: Option<String>,
}

/// A note, free-standing or attached to a class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassNote {
    /// The class it is attached to (`note for X`).
    pub target: Option<String>,
    /// Note text.
    pub text: String,
}

/// An open block.
enum Block {
    Namespace(usize),
    Body(usize),
}

struct Builder {
    d: ClassDiagram,
    index: HashMap<String, usize>,
    ns_index: HashMap<String, usize>,
    stack: Vec<(Block, usize)>,
}

/// Parse a class diagram from preprocessed input.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<ClassDiagram, ParseError> {
    let mut b = Builder {
        d: ClassDiagram { meta: prep.meta.clone(), ..ClassDiagram::default() },
        index: HashMap::new(),
        ns_index: HashMap::new(),
        stack: Vec::new(),
    };
    for l in prep.all_lines() {
        let stmt = Stmt::from_line(&l);
        ctx.statement(stmt.line)?;
        let r = b.statement(&stmt, ctx);
        settle(ctx, &stmt, r)?;
    }
    if let Some((block, line)) = b.stack.last() {
        let what = match block {
            Block::Namespace(i) => format!("namespace `{}`", b.d.namespaces.get(*i).map(|n| n.id.as_str()).unwrap_or("")),
            Block::Body(i) => format!("body of class `{}`", b.d.classes.get(*i).map(|c| c.id.as_str()).unwrap_or("")),
        };
        return Err(ParseError::new(*line, 1, format!("{what} is never closed with `}}`")));
    }
    Ok(b.d)
}

impl Builder {
    fn current_namespace(&self) -> Option<String> {
        self.stack.iter().rev().find_map(|(b, _)| match b {
            Block::Namespace(i) => self.d.namespaces.get(*i).map(|n| n.id.clone()),
            Block::Body(_) => None,
        })
    }

    fn ensure(&mut self, id: &str, line: usize) -> Result<usize, ParseError> {
        if let Some(&i) = self.index.get(id) {
            return Ok(i);
        }
        check_nodes(self.d.classes.len().saturating_add(self.d.notes.len()), line)?;
        let i = self.d.classes.len();
        let namespace = self.current_namespace();
        self.d.classes.push(Class {
            id: id.to_string(),
            label: None,
            generic: None,
            annotations: Vec::new(),
            members: Vec::new(),
            namespace,
            classes: Vec::new(),
            style: Vec::new(),
        });
        self.index.insert(id.to_string(), i);
        Ok(i)
    }

    fn statement(&mut self, stmt: &Stmt, ctx: &mut Ctx) -> StmtResult<()> {
        let t = stmt.text.as_str();
        let line = stmt.line;
        // Inside a class body every line is a member, an annotation or the closing brace.
        if let Some(&(Block::Body(ci), _)) = self.stack.last() {
            if t == "}" {
                self.stack.pop();
                return Ok(());
            }
            if let Some(a) = t.strip_prefix("<<").and_then(|r| r.strip_suffix(">>")) {
                if let Some(c) = self.d.classes.get_mut(ci) {
                    push_unique(&mut c.annotations, a.trim());
                }
                return Ok(());
            }
            if let Some(c) = self.d.classes.get_mut(ci) {
                c.members.push(parse_member(t));
            }
            return Ok(());
        }
        if t == "}" {
            if self.stack.pop().is_none() {
                return Err(StmtError::Unknown(0, "`}` without an open block".into()));
            }
            return Ok(());
        }
        let (kw, rest) = first_word(t);
        let rest_off = t.len().saturating_sub(rest.len());
        match kw {
            "class" => return self.class_decl(rest, rest_off, line),
            "namespace" => {
                let name = rest.trim_end_matches('{').trim();
                if name.is_empty() || !rest.ends_with('{') {
                    return Err(StmtError::Unknown(0, "namespaces are written `namespace Name {`".into()));
                }
                check_depth(self.stack.len(), line, 1)?;
                let parent = self.current_namespace();
                let i = match self.ns_index.get(name) {
                    Some(&i) => i,
                    None => {
                        let i = self.d.namespaces.len();
                        self.d.namespaces.push(Namespace { id: name.to_string(), parent });
                        self.ns_index.insert(name.to_string(), i);
                        i
                    }
                };
                self.stack.push((Block::Namespace(i), line));
                return Ok(());
            }
            "direction" => {
                let Some(d) = Direction::from_keyword(rest) else {
                    return Err(StmtError::Unknown(0, format!("`{rest}` is not a direction")));
                };
                self.d.direction = d;
                return Ok(());
            }
            "note" => return self.note(rest, line),
            "classDef" => {
                let (names, props) = first_word(rest);
                if names.is_empty() {
                    return Err(StmtError::Unknown(0, "`classDef` needs a class name".into()));
                }
                add_class_def(&mut self.d.class_defs, names, props);
                return Ok(());
            }
            "cssClass" => {
                let toks = tokens(rest);
                let (Some(ids), Some(class)) = (toks.first(), toks.get(1)) else {
                    return Err(StmtError::Unknown(0, "`cssClass` needs class names and a style class".into()));
                };
                for id in ids.text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let i = self.ensure(id, line)?;
                    if let Some(c) = self.d.classes.get_mut(i) {
                        push_unique(&mut c.classes, &class.text);
                    }
                }
                return Ok(());
            }
            "style" => {
                let (id, props) = first_word(rest);
                if id.is_empty() {
                    return Err(StmtError::Unknown(0, "`style` needs a class name".into()));
                }
                let i = self.ensure(id, line)?;
                if let Some(c) = self.d.classes.get_mut(i) {
                    c.style.extend(parse_style_props(props));
                }
                return Ok(());
            }
            "link" | "click" => {
                let Some(click) = parse_click(rest) else {
                    return Err(StmtError::Unknown(0, format!("`{kw}` needs a class name and a target")));
                };
                if click.callback.is_some() {
                    ctx.warn(line, format!("click callback on `{}` kept as a hint only; callbacks never run", click.node));
                }
                self.d.clicks.push(click);
                return Ok(());
            }
            "callback" => {
                let toks = tokens(rest);
                let (Some(node), Some(name)) = (toks.first(), toks.get(1)) else {
                    return Err(StmtError::Unknown(0, "`callback` needs a class name and a function".into()));
                };
                ctx.warn(line, format!("callback on `{}` kept as a hint only; callbacks never run", node.text));
                self.d.clicks.push(Click {
                    node: node.text.clone(),
                    url: None,
                    target: None,
                    tooltip: toks.get(2).map(|t| t.text.clone()),
                    callback: Some(name.text.clone()),
                });
                return Ok(());
            }
            _ => {}
        }
        if let Some(a) = t.strip_prefix("<<") {
            let Some(end) = a.find(">>") else {
                return Err(StmtError::Malformed(0, "`<<` is never closed with `>>`".into()));
            };
            let annotation = a.get(..end).unwrap_or("").trim();
            let mut cur = Cursor::new(a.get(end.saturating_add(2)..).unwrap_or(""));
            cur.skip_ws();
            let Some((id, _)) = class_name(&mut cur) else {
                return Err(StmtError::Unknown(0, "annotation needs a class name after `>>`".into()));
            };
            let i = self.ensure(&id, line)?;
            if let Some(c) = self.d.classes.get_mut(i) {
                push_unique(&mut c.annotations, annotation);
            }
            return Ok(());
        }
        if let Some(rel) = parse_relation(t)? {
            check_edges(self.d.relations.len(), line)?;
            self.ensure(&rel.from, line)?;
            self.ensure(&rel.to, line)?;
            self.d.relations.push(rel);
            return Ok(());
        }
        // `Name : member`, `Name`, `Name:::class`.
        let mut cur = Cursor::new(t);
        let Some((id, generic)) = class_name(&mut cur) else {
            return Err(StmtError::Unknown(0, "not a class diagram statement".into()));
        };
        let mut classes = Vec::new();
        while cur.eat(":::") {
            push_unique(&mut classes, cur.word(true, false));
        }
        cur.skip_ws();
        let member = if cur.eat(":") {
            let m = cur.rest().trim();
            if m.is_empty() {
                return Err(StmtError::Unknown(cur.pos(), "member line has no member after `:`".into()));
            }
            Some(parse_member(m))
        } else if cur.is_eof() {
            None
        } else {
            return Err(StmtError::Unknown(cur.pos(), "not a class diagram statement".into()));
        };
        let i = self.ensure(&id, line)?;
        if let Some(c) = self.d.classes.get_mut(i) {
            if generic.is_some() {
                c.generic = generic;
            }
            for cl in &classes {
                push_unique(&mut c.classes, cl);
            }
            if let Some(m) = member {
                c.members.push(m);
            }
        }
        Ok(())
    }

    /// `class Name~T~["Label"]:::style {`, with an optional one-line body.
    fn class_decl(&mut self, rest: &str, off: usize, line: usize) -> StmtResult<()> {
        let mut cur = Cursor::new(rest);
        let Some((id, generic)) = class_name(&mut cur) else {
            return Err(StmtError::Unknown(off, "`class` needs a class name".into()));
        };
        let mut label = None;
        if cur.peek() == Some('[') {
            let at = off.saturating_add(cur.pos());
            cur.bump();
            let r = cur.rest();
            let Some(end) = r.find(']') else {
                return Err(StmtError::Malformed(at, "class label `[` is never closed".into()));
            };
            label = Some(clean_label(r.get(..end).unwrap_or("")));
            cur.advance(end.saturating_add(1));
        }
        let mut classes = Vec::new();
        while cur.eat(":::") {
            push_unique(&mut classes, cur.word(true, false));
        }
        cur.skip_ws();
        let mut open = false;
        let mut inline_members = Vec::new();
        if cur.eat("{") {
            let inner = cur.rest().trim();
            match inner.strip_suffix('}') {
                Some(body) => {
                    if !body.trim().is_empty() {
                        inline_members.push(parse_member(body.trim()));
                    }
                }
                None => {
                    if !inner.is_empty() {
                        inline_members.push(parse_member(inner));
                    }
                    open = true;
                }
            }
        } else if !cur.is_eof() {
            return Err(StmtError::Unknown(off.saturating_add(cur.pos()), "unexpected text in a class declaration".into()));
        }
        if open {
            check_depth(self.stack.len(), line, 1)?;
        }
        let ns = self.current_namespace();
        let i = self.ensure(&id, line)?;
        if let Some(c) = self.d.classes.get_mut(i) {
            if label.is_some() {
                c.label = label;
            }
            if generic.is_some() {
                c.generic = generic;
            }
            if c.namespace.is_none() {
                c.namespace = ns;
            }
            for cl in &classes {
                push_unique(&mut c.classes, cl);
            }
            c.members.extend(inline_members);
        }
        if open {
            self.stack.push((Block::Body(i), line));
        }
        Ok(())
    }

    /// `note "text"` or `note for Class "text"`.
    fn note(&mut self, rest: &str, line: usize) -> StmtResult<()> {
        let toks = tokens(rest);
        let (target, text) = match toks.as_slice() {
            [t] if t.quoted => (None, t.text.clone()),
            [f, target, t] if !f.quoted && f.text == "for" && t.quoted => (Some(target.text.clone()), t.text.clone()),
            _ => return Err(StmtError::Unknown(0, "notes are written `note \"text\"` or `note for Class \"text\"`".into())),
        };
        check_nodes(self.d.classes.len().saturating_add(self.d.notes.len()), line)?;
        if let Some(t) = &target {
            self.ensure(t, line)?;
        }
        self.d.notes.push(ClassNote { target, text: clean_label(&text.replace("\\n", "\n")) });
        Ok(())
    }
}

/// A class name: backtick-quoted, or a word (inner `-` and `.` allowed), with an optional
/// `~generic~` suffix. Returns the id and the generic text.
fn class_name(cur: &mut Cursor<'_>) -> Option<(String, Option<String>)> {
    cur.skip_ws();
    let id = if cur.peek() == Some('`') {
        let rest = cur.rest();
        let inner = rest.get(1..)?;
        let end = inner.find('`')?;
        let name = inner.get(..end)?.to_string();
        cur.advance(end.saturating_add(2));
        name
    } else {
        let w = cur.word(true, true);
        if w.is_empty() {
            return None;
        }
        w.to_string()
    };
    let mut generic = None;
    if cur.peek() == Some('~') {
        let start = cur.pos();
        let tok = cur.rest();
        let tok_len = tok.find(|c: char| c.is_whitespace() || "{[:\"".contains(c)).unwrap_or(tok.len());
        let tok = tok.get(..tok_len).unwrap_or("");
        if let Some(last) = tok.rfind('~')
            && last > 0
        {
            generic = Some(tilde_to_angle(tok.get(1..last).unwrap_or("")));
            cur.set_pos(start);
            cur.advance(last.saturating_add(1));
        }
    }
    Some((id, generic))
}

/// Turn Mermaid's `~` generic markers into angle brackets: `List~List~int~~` becomes
/// `List<List<int>>`. A `~` closes when one is open and no word character follows it.
pub(crate) fn tilde_to_angle(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '~' {
            out.push(c);
            continue;
        }
        let next_is_word = chars.peek().is_some_and(|n| is_word_char(*n));
        if depth > 0 && !next_is_word {
            depth -= 1;
            out.push('>');
        } else {
            depth = depth.saturating_add(1);
            out.push('<');
        }
    }
    out
}

/// Parse one member line.
pub(crate) fn parse_member(raw: &str) -> Member {
    let mut s = raw.trim();
    let mut classifier = None;
    if let Some(x) = s.strip_suffix('$') {
        classifier = Some(Classifier::Static);
        s = x.trim_end();
    } else if let Some(x) = s.strip_suffix('*') {
        classifier = Some(Classifier::Abstract);
        s = x.trim_end();
    }
    let (visibility, vis_char) = match s.chars().next() {
        Some('+') => (Some(Visibility::Public), "+"),
        Some('-') => (Some(Visibility::Private), "-"),
        Some('#') => (Some(Visibility::Protected), "#"),
        Some('~') => (Some(Visibility::Package), "~"),
        _ => (None, ""),
    };
    if visibility.is_some() {
        s = s.get(1..).unwrap_or("").trim_start();
    }
    if let Some(open) = s.find('(')
        && let Some(close) = s.rfind(')')
        && close > open
    {
        let name = s.get(..open).unwrap_or("").trim().to_string();
        let params = tilde_to_angle(s.get(open.saturating_add(1)..close).unwrap_or("").trim());
        let mut after = s.get(close.saturating_add(1)..).unwrap_or("").trim();
        if let Some(x) = after.strip_prefix('$') {
            classifier = Some(Classifier::Static);
            after = x.trim_start();
        } else if let Some(x) = after.strip_prefix('*') {
            classifier = Some(Classifier::Abstract);
            after = x.trim_start();
        }
        let after = after.strip_prefix(':').unwrap_or(after).trim();
        let ty = (!after.is_empty()).then(|| tilde_to_angle(after));
        let mut text = format!("{vis_char}{name}({params})");
        if let Some(t) = &ty {
            text.push_str(" : ");
            text.push_str(t);
        }
        return Member { text, visibility, kind: MemberKind::Method, name, parameters: Some(params), ty, classifier };
    }
    let (ty, name) = match s.rsplit_once(char::is_whitespace) {
        Some((t, n)) => (Some(tilde_to_angle(t.trim())), n.trim().to_string()),
        None => (None, s.to_string()),
    };
    Member { text: format!("{vis_char}{}", tilde_to_angle(s)), visibility, kind: MemberKind::Attribute, name, parameters: None, ty, classifier }
}

/// Parse a relationship operator such as `<|--`, `*--`, `..>`, `()--` or `<|..|>`.
fn relation_op(cur: &mut Cursor<'_>) -> Option<(RelationEnd, bool, RelationEnd)> {
    let start = cur.pos();
    let line_char = |c: Option<char>| matches!(c, Some('-' | '.'));
    let left = if cur.eat("<|") {
        RelationEnd::Inheritance
    } else if cur.eat("()") {
        RelationEnd::Lollipop
    } else if cur.peek() == Some('*') && line_char(cur.peek_nth(1)) {
        cur.bump();
        RelationEnd::Composition
    } else if cur.peek() == Some('o') && line_char(cur.peek_nth(1)) {
        cur.bump();
        RelationEnd::Aggregation
    } else if cur.peek() == Some('<') && line_char(cur.peek_nth(1)) {
        cur.bump();
        RelationEnd::Arrow
    } else {
        RelationEnd::None
    };
    let dashed = if cur.eat("--") {
        false
    } else if cur.eat("..") {
        true
    } else {
        cur.set_pos(start);
        return None;
    };
    let right = if cur.eat("|>") {
        RelationEnd::Inheritance
    } else if cur.eat("()") {
        RelationEnd::Lollipop
    } else if cur.eat("*") {
        RelationEnd::Composition
    } else if cur.peek() == Some('o') && !cur.peek_nth(1).is_some_and(is_word_char) {
        cur.bump();
        RelationEnd::Aggregation
    } else if cur.eat(">") {
        RelationEnd::Arrow
    } else {
        RelationEnd::None
    };
    Some((left, dashed, right))
}

/// `A ["card"] OP ["card"] B [: label]`. `Ok(None)` when there is no relationship operator.
fn parse_relation(t: &str) -> StmtResult<Option<Relation>> {
    let mut cur = Cursor::new(t);
    let Some((from, _)) = class_name(&mut cur) else {
        return Ok(None);
    };
    cur.skip_ws();
    let from_cardinality = cur.quoted().map(str::to_string);
    cur.skip_ws();
    let Some((from_end, dashed, to_end)) = relation_op(&mut cur) else {
        if from_cardinality.is_some() {
            return Err(StmtError::Malformed(cur.pos(), "expected a relationship arrow after the cardinality".into()));
        }
        return Ok(None);
    };
    cur.skip_ws();
    let to_cardinality = cur.quoted().map(str::to_string);
    cur.skip_ws();
    let at = cur.pos();
    let Some((to, _)) = class_name(&mut cur) else {
        return Err(StmtError::Malformed(at, "relationship has no second class".into()));
    };
    cur.skip_ws();
    let label = if cur.eat(":") {
        let l = clean_label(cur.rest());
        (!l.is_empty()).then_some(l)
    } else if cur.is_eof() {
        None
    } else {
        return Err(StmtError::Unknown(cur.pos(), "unexpected text after the relationship".into()));
    };
    Ok(Some(Relation { from, to, from_end, to_end, dashed, from_cardinality, to_cardinality, label }))
}

impl ClassDiagram {
    /// The uniform graph view: classes as class nodes (annotations, then attribute rows, then
    /// method rows), namespaces as groups, relationships as edges with UML markers and
    /// cardinalities as end labels, notes as note nodes.
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::Class, self.direction, self.meta.title.clone());
        for ns in &self.namespaces {
            let mut grp = GraphGroup::new(ns.id.clone(), ns.id.clone(), GroupKind::Namespace);
            grp.parent.clone_from(&ns.parent);
            grp.members = self.classes.iter().filter(|c| c.namespace.as_ref() == Some(&ns.id)).map(|c| c.id.clone()).collect();
            g.groups.push(grp);
        }
        for c in &self.classes {
            let mut node = GraphNode::new(c.id.clone(), c.display_name(), Shape::Class);
            node.annotations.clone_from(&c.annotations);
            node.details = c
                .members
                .iter()
                .filter(|m| m.kind == MemberKind::Attribute)
                .chain(c.members.iter().filter(|m| m.kind == MemberKind::Method))
                .map(|m| m.text.clone())
                .collect();
            node.classes.clone_from(&c.classes);
            node.style.clone_from(&c.style);
            node.group.clone_from(&c.namespace);
            if let Some(click) = self.clicks.iter().rev().find(|k| k.node == c.id) {
                node.link.clone_from(&click.url);
                node.tooltip.clone_from(&click.tooltip);
            }
            g.nodes.push(node);
        }
        for r in &self.relations {
            let mut e = GraphEdge::new(r.from.clone(), r.to.clone());
            e.line = if r.dashed { LineStyle::Dashed } else { LineStyle::Solid };
            e.start = r.from_end.arrow();
            e.end = r.to_end.arrow();
            e.source_label.clone_from(&r.from_cardinality);
            e.target_label.clone_from(&r.to_cardinality);
            e.label.clone_from(&r.label);
            g.edges.push(e);
        }
        for (i, n) in self.notes.iter().enumerate() {
            let id = format!("note{i}");
            let mut node = GraphNode::new(id.clone(), n.text.clone(), Shape::Note);
            if let Some(t) = &n.target {
                node.group = self.classes.iter().find(|c| &c.id == t).and_then(|c| c.namespace.clone());
                let mut e = GraphEdge::new(id, t.clone());
                e.line = LineStyle::Dotted;
                g.edges.push(e);
            }
            g.nodes.push(node);
        }
        g.class_defs.clone_from(&self.class_defs);
        g
    }
}
