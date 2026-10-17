//! Entity-relationship diagrams: `erDiagram`.
//!
//! Entities (`CUSTOMER`, `p[Person]`, optionally with an attribute block in braces),
//! relationships with a cardinality at each end in crow's foot symbols or words
//! (`CUSTOMER ||--o{ ORDER : places`, `A one or more to zero or many B : has`), identifying
//! (`--`, `to`) and non-identifying (`..`, `optionally to`) lines, `direction`, and
//! `classDef` / `class` / `style` / `:::` styling.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::common::{
    ClassDef, Ctx, Cursor, Direction, Meta, ParseError, Prepared, Stmt, StmtError, StmtResult, StyleProp, Token, add_class_def, check_edges,
    check_nodes, clean_label, first_word, parse_style_props, push_unique, settle, tokens,
};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphNode, LineStyle, Shape};

/// A parsed ER diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ErDiagram {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Direction from a `direction` statement (`TB` by default).
    pub direction: Direction,
    /// Entities in order of first mention.
    pub entities: Vec<Entity>,
    /// Relationships in source order.
    pub relationships: Vec<Relationship>,
    /// `classDef` statements.
    pub class_defs: Vec<ClassDef>,
}

/// An entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Entity name as used in relationships.
    pub id: String,
    /// Display alias from `id[Alias]`, when given.
    pub alias: Option<String>,
    /// Attributes in order.
    pub attributes: Vec<Attribute>,
    /// Style classes from `class` and `:::`.
    pub classes: Vec<String>,
    /// Properties from `style`.
    pub style: Vec<StyleProp>,
}

/// An attribute key marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyKind {
    /// `PK`, primary key.
    Primary,
    /// `FK`, foreign key.
    Foreign,
    /// `UK`, unique key.
    Unique,
}

/// An entity attribute: `type name [PK, FK] ["comment"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    /// Type as written, for example `varchar(255)` or `string[]`.
    pub ty: String,
    /// Attribute name.
    pub name: String,
    /// Key markers in order.
    pub keys: Vec<KeyKind>,
    /// Comment text.
    pub comment: Option<String>,
}

/// How many instances one end of a relationship allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    /// `|o` / `o|`: zero or one.
    ZeroOrOne,
    /// `||`: exactly one.
    ExactlyOne,
    /// `}o` / `o{`: zero or more.
    ZeroOrMore,
    /// `}|` / `|{`: one or more.
    OneOrMore,
}

impl Cardinality {
    /// The matching crow's foot marker.
    pub fn arrow(self) -> ArrowKind {
        match self {
            Cardinality::ZeroOrOne => ArrowKind::ZeroOrOne,
            Cardinality::ExactlyOne => ArrowKind::ExactlyOne,
            Cardinality::ZeroOrMore => ArrowKind::ZeroOrMore,
            Cardinality::OneOrMore => ArrowKind::OneOrMore,
        }
    }
}

/// A relationship between two entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// First entity.
    pub left: String,
    /// Second entity.
    pub right: String,
    /// Cardinality at the first entity's end.
    pub left_cardinality: Cardinality,
    /// Cardinality at the second entity's end.
    pub right_cardinality: Cardinality,
    /// Identifying (solid line) or non-identifying (dashed).
    pub identifying: bool,
    /// Relationship label.
    pub label: Option<String>,
}

struct Builder {
    d: ErDiagram,
    index: HashMap<String, usize>,
}

/// Parse an ER diagram from preprocessed input.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<ErDiagram, ParseError> {
    let mut b = Builder { d: ErDiagram { meta: prep.meta.clone(), ..ErDiagram::default() }, index: HashMap::new() };
    let lines = prep.all_lines();
    let mut i = 0usize;
    while let Some(l) = lines.get(i) {
        i = i.saturating_add(1);
        let stmt = Stmt::from_line(l);
        ctx.statement(stmt.line)?;
        let r = b.statement(&stmt, &lines, &mut i, ctx);
        settle(ctx, &stmt, r)?;
    }
    Ok(b.d)
}

impl Builder {
    fn ensure(&mut self, id: &str, line: usize) -> Result<usize, ParseError> {
        if let Some(&i) = self.index.get(id) {
            return Ok(i);
        }
        check_nodes(self.d.entities.len(), line)?;
        let i = self.d.entities.len();
        self.d.entities.push(Entity { id: id.to_string(), alias: None, attributes: Vec::new(), classes: Vec::new(), style: Vec::new() });
        self.index.insert(id.to_string(), i);
        Ok(i)
    }

    fn statement(&mut self, stmt: &Stmt, lines: &[crate::common::SrcLine<'_>], i: &mut usize, ctx: &mut Ctx) -> StmtResult<()> {
        let t = stmt.text.as_str();
        let line = stmt.line;
        let (kw, rest) = first_word(t);
        match kw {
            "direction" => {
                let Some(d) = Direction::from_keyword(rest) else {
                    return Err(StmtError::Unknown(0, format!("`{rest}` is not a direction")));
                };
                self.d.direction = d;
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
                    return Err(StmtError::Unknown(0, "`class` needs entity names and a class name".into()));
                }
                for id in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let e = self.ensure(id, line)?;
                    if let Some(ent) = self.d.entities.get_mut(e) {
                        push_unique(&mut ent.classes, class);
                    }
                }
                return Ok(());
            }
            "style" => {
                let (id, props) = first_word(rest);
                if id.is_empty() {
                    return Err(StmtError::Unknown(0, "`style` needs an entity name".into()));
                }
                let e = self.ensure(id, line)?;
                if let Some(ent) = self.d.entities.get_mut(e) {
                    ent.style.extend(parse_style_props(props));
                }
                return Ok(());
            }
            _ => {}
        }
        if t == "}" {
            return Err(StmtError::Unknown(0, "`}` without an open entity block".into()));
        }
        if let Some(rel) = parse_symbol_relationship(t)? {
            return self.add_relationship(rel, line);
        }
        if let Some(rel) = parse_word_relationship(t) {
            return self.add_relationship(rel, line);
        }
        self.entity_statement(stmt, lines, i, ctx)
    }

    fn add_relationship(&mut self, rel: Relationship, line: usize) -> StmtResult<()> {
        check_edges(self.d.relationships.len(), line)?;
        self.ensure(&rel.left, line)?;
        self.ensure(&rel.right, line)?;
        self.d.relationships.push(rel);
        Ok(())
    }

    /// `NAME`, `NAME[Alias]`, `NAME:::class`, optionally opening `{` an attribute block.
    fn entity_statement(&mut self, stmt: &Stmt, lines: &[crate::common::SrcLine<'_>], i: &mut usize, ctx: &mut Ctx) -> StmtResult<()> {
        let line = stmt.line;
        let mut cur = Cursor::new(&stmt.text);
        let Some(id) = entity_name(&mut cur) else {
            return Err(StmtError::Unknown(0, "not an ER diagram statement".into()));
        };
        let mut alias = None;
        if cur.peek() == Some('[') {
            let at = cur.pos();
            cur.bump();
            let rest = cur.rest();
            let Some(end) = rest.find(']') else {
                return Err(StmtError::Malformed(at, "entity alias `[` is never closed".into()));
            };
            alias = Some(clean_label(rest.get(..end).unwrap_or("")));
            cur.advance(end.saturating_add(1));
        }
        let mut classes = Vec::new();
        while cur.eat(":::") {
            push_unique(&mut classes, cur.word(true, false));
        }
        cur.skip_ws();
        let mut attributes = Vec::new();
        if cur.eat("{") {
            let inline = cur.rest().trim();
            if let Some(body) = inline.strip_suffix('}') {
                // A one-line block: `A { string name }`.
                if !body.trim().is_empty() {
                    attributes.push(parse_attribute(body.trim()).ok_or_else(|| StmtError::Unknown(0, "attribute needs a type and a name".into()))?);
                }
            } else {
                if !inline.is_empty() {
                    return Err(StmtError::Unknown(cur.pos(), "attributes go on the lines after `{`".into()));
                }
                let mut closed = false;
                while let Some(l) = lines.get(*i) {
                    *i = i.saturating_add(1);
                    ctx.statement(l.no)?;
                    if l.text == "}" {
                        closed = true;
                        break;
                    }
                    match parse_attribute(l.text) {
                        Some(a) => attributes.push(a),
                        None => ctx.warn(l.no, format!("attribute needs a type and a name; skipped `{}`", crate::common::snippet(l.text))),
                    }
                }
                if !closed {
                    return Err(StmtError::Malformed(0, format!("attribute block of `{id}` is never closed with `}}`")));
                }
            }
        } else if !cur.is_eof() {
            return Err(StmtError::Unknown(cur.pos(), "not an ER diagram statement".into()));
        }
        let e = self.ensure(&id, line)?;
        if let Some(ent) = self.d.entities.get_mut(e) {
            if alias.is_some() {
                ent.alias = alias;
            }
            for c in &classes {
                push_unique(&mut ent.classes, c);
            }
            ent.attributes.extend(attributes);
        }
        Ok(())
    }
}

/// An entity name: a quoted string or letters, digits, `_` and inner `-`.
fn entity_name(cur: &mut Cursor<'_>) -> Option<String> {
    cur.skip_ws();
    if let Some(q) = cur.quoted() {
        return Some(q.to_string());
    }
    let w = cur.word(true, false);
    if w.is_empty() { None } else { Some(w.to_string()) }
}

/// Parse `type name [PK, FK] ["comment"]`.
fn parse_attribute(text: &str) -> Option<Attribute> {
    let toks: Vec<Token> = tokens(text);
    let mut it = toks.into_iter();
    let ty = it.next().filter(|t| !t.quoted)?.text;
    let name = it.next().filter(|t| !t.quoted)?.text;
    let mut attr = Attribute { ty, name, keys: Vec::new(), comment: None };
    for t in it {
        if t.quoted {
            attr.comment = Some(t.text);
            continue;
        }
        for k in t.text.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            let key = match k.to_ascii_uppercase().as_str() {
                "PK" => KeyKind::Primary,
                "FK" => KeyKind::Foreign,
                "UK" => KeyKind::Unique,
                _ => return None,
            };
            if !attr.keys.contains(&key) {
                attr.keys.push(key);
            }
        }
    }
    Some(attr)
}

/// The left-end symbol of a crow's foot relationship.
fn left_card(s: &str) -> Option<Cardinality> {
    match s {
        "|o" => Some(Cardinality::ZeroOrOne),
        "||" => Some(Cardinality::ExactlyOne),
        "}o" => Some(Cardinality::ZeroOrMore),
        "}|" => Some(Cardinality::OneOrMore),
        _ => None,
    }
}

/// The right-end symbol of a crow's foot relationship.
fn right_card(s: &str) -> Option<Cardinality> {
    match s {
        "o|" => Some(Cardinality::ZeroOrOne),
        "||" => Some(Cardinality::ExactlyOne),
        "o{" => Some(Cardinality::ZeroOrMore),
        "|{" => Some(Cardinality::OneOrMore),
        _ => None,
    }
}

/// The optional `: label` after a relationship.
fn rel_label(cur: &mut Cursor<'_>) -> StmtResult<Option<String>> {
    cur.skip_ws();
    if cur.is_eof() {
        return Ok(None);
    }
    if !cur.eat(":") {
        return Err(StmtError::Unknown(cur.pos(), "expected `: label` after the relationship".into()));
    }
    let l = clean_label(cur.rest());
    Ok((!l.is_empty()).then_some(l))
}

/// `A ||--o{ B : label`. `Ok(None)` when the statement is not in this form.
fn parse_symbol_relationship(t: &str) -> StmtResult<Option<Relationship>> {
    let mut cur = Cursor::new(t);
    let Some(left) = entity_name(&mut cur) else {
        return Ok(None);
    };
    cur.skip_ws();
    let op_start = cur.pos();
    let op: String = cur.rest().chars().take(6).collect();
    if op.chars().count() < 6 {
        return Ok(None);
    }
    let (l, line, r) = (op.get(0..2).unwrap_or(""), op.get(2..4).unwrap_or(""), op.get(4..6).unwrap_or(""));
    let identifying = match line {
        "--" => true,
        ".." => false,
        _ => return Ok(None),
    };
    let (Some(lc), Some(rc)) = (left_card(l), right_card(r)) else {
        if left_card(l).is_some() || right_card(r).is_some() {
            return Err(StmtError::Malformed(op_start, format!("`{op}` is not a valid relationship cardinality")));
        }
        return Ok(None);
    };
    cur.advance(6);
    let at = cur.pos();
    let Some(right) = entity_name(&mut cur) else {
        return Err(StmtError::Malformed(at, "relationship has no second entity".into()));
    };
    let label = rel_label(&mut cur)?;
    Ok(Some(Relationship { left, right, left_cardinality: lc, right_cardinality: rc, identifying, label }))
}

/// A cardinality written in words (`one or more`, `zero or one`, `only one`, `1+`, ...).
fn word_card(words: &[String]) -> Option<Cardinality> {
    let joined = words.join(" ").to_ascii_lowercase();
    match joined.as_str() {
        "one or zero" | "zero or one" => Some(Cardinality::ZeroOrOne),
        "one or more" | "one or many" | "many(1)" | "1+" => Some(Cardinality::OneOrMore),
        "zero or more" | "zero or many" | "many(0)" | "0+" => Some(Cardinality::ZeroOrMore),
        "only one" | "1" => Some(Cardinality::ExactlyOne),
        _ => None,
    }
}

/// `A only one to zero or more B : label`; `optionally to` makes it non-identifying.
fn parse_word_relationship(t: &str) -> Option<Relationship> {
    let (head, label) = match t.split_once(':') {
        Some((h, l)) => (h, Some(clean_label(l)).filter(|l| !l.is_empty())),
        None => (t, None),
    };
    let toks = tokens(head);
    if toks.len() < 4 {
        return None;
    }
    let left = toks.first()?.text.clone();
    let right = toks.last()?.text.clone();
    let middle: Vec<String> = toks.get(1..toks.len().saturating_sub(1))?.iter().map(|t| t.text.clone()).collect();
    let to_pos = middle.iter().position(|w| w == "to")?;
    let (mut lw, rw) = (middle.get(..to_pos)?.to_vec(), middle.get(to_pos.saturating_add(1)..)?.to_vec());
    let identifying = if lw.last().is_some_and(|w| w == "optionally") {
        lw.pop();
        false
    } else {
        true
    };
    Some(Relationship { left, right, left_cardinality: word_card(&lw)?, right_cardinality: word_card(&rw)?, identifying, label })
}

/// Display text of one attribute row.
fn attribute_row(a: &Attribute) -> String {
    let mut row = format!("{} {}", a.ty, a.name);
    if !a.keys.is_empty() {
        let keys: Vec<&str> = a
            .keys
            .iter()
            .map(|k| match k {
                KeyKind::Primary => "PK",
                KeyKind::Foreign => "FK",
                KeyKind::Unique => "UK",
            })
            .collect();
        row.push(' ');
        row.push_str(&keys.join(", "));
    }
    if let Some(c) = &a.comment {
        row.push_str(&format!(" \"{c}\""));
    }
    row
}

impl ErDiagram {
    /// The uniform graph view: entities as entity nodes with one detail row per attribute,
    /// relationships as edges with crow's foot markers, solid when identifying.
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::Er, self.direction, self.meta.title.clone());
        for e in &self.entities {
            let mut node = GraphNode::new(e.id.clone(), e.alias.clone().unwrap_or_else(|| e.id.clone()), Shape::Entity);
            node.details = e.attributes.iter().map(attribute_row).collect();
            node.classes.clone_from(&e.classes);
            node.style.clone_from(&e.style);
            g.nodes.push(node);
        }
        for r in &self.relationships {
            let mut edge = GraphEdge::new(r.left.clone(), r.right.clone());
            edge.label.clone_from(&r.label);
            edge.line = if r.identifying { LineStyle::Solid } else { LineStyle::Dashed };
            edge.start = r.left_cardinality.arrow();
            edge.end = r.right_cardinality.arrow();
            g.edges.push(edge);
        }
        g.class_defs.clone_from(&self.class_defs);
        g
    }
}
