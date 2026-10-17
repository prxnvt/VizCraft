//! Flowcharts: `flowchart` and `graph`.
//!
//! Statements are separated by line breaks or `;`. A statement is a keyword statement
//! (`subgraph`, `end`, `direction`, `classDef`, `class`, `style`, `linkStyle`, `click`) or a
//! chain of node groups joined by links: `A[Start] --> B & C -- yes --> D:::warn`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::common::{
    ClassDef, Click, Ctx, Cursor, Direction, Meta, ParseError, Prepared, StmtError, StmtResult, StyleProp, add_class_def, check_depth, check_edges,
    check_nodes, clean_label, first_word, parse_click, parse_style_props, push_unique, settle, split_statements,
};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, LineStyle, Shape};

/// A parsed flowchart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Flowchart {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Direction from the header (`TB` when none was given).
    pub direction: Direction,
    /// Nodes in order of first mention.
    pub nodes: Vec<FlowNode>,
    /// Links in source order; `linkStyle` indices count into this list.
    pub edges: Vec<FlowEdge>,
    /// Subgraphs in the order they were opened.
    pub subgraphs: Vec<Subgraph>,
    /// `classDef` statements.
    pub class_defs: Vec<ClassDef>,
    /// `linkStyle` statements as written (already applied to the edges' `style`).
    pub link_styles: Vec<LinkStyle>,
    /// `click` statements.
    pub clicks: Vec<Click>,
}

/// A flowchart node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    /// Node id.
    pub id: String,
    /// Display text (the id when no label was given).
    pub label: String,
    /// Node shape.
    pub shape: Shape,
    /// Style classes from `class` and `:::`.
    pub classes: Vec<String>,
    /// Properties from `style`.
    pub style: Vec<StyleProp>,
    /// Innermost subgraph holding the node.
    pub subgraph: Option<String>,
}

/// A flowchart link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    /// Edge id from the `id@-->` form.
    pub id: Option<String>,
    /// Source node or subgraph id.
    pub from: String,
    /// Target node or subgraph id.
    pub to: String,
    /// Link text.
    pub label: Option<String>,
    /// Line style.
    pub line: LineStyle,
    /// Marker at the source end (`<`, `o`, `x`).
    pub start: ArrowKind,
    /// Marker at the target end (`>`, `o`, `x`).
    pub end: ArrowKind,
    /// Length in ranks: 1 for `-->`, 2 for `--->` and so on.
    pub length: u8,
    /// Properties from `linkStyle`.
    pub style: Vec<StyleProp>,
}

/// A subgraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subgraph {
    /// Subgraph id.
    pub id: String,
    /// Title text.
    pub title: String,
    /// Direction from a `direction` statement inside it.
    pub direction: Option<Direction>,
    /// Enclosing subgraph.
    pub parent: Option<String>,
    /// Ids of the nodes directly inside.
    pub nodes: Vec<String>,
    /// Style classes from `class` and `:::`.
    pub classes: Vec<String>,
    /// Properties from `style`.
    pub style: Vec<StyleProp>,
}

/// Which links a `linkStyle` statement targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkTarget {
    /// `linkStyle default`: every link.
    Default,
    /// Zero-based link indices in definition order.
    Indices(Vec<usize>),
}

/// A `linkStyle` statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkStyle {
    /// Targeted links.
    pub targets: LinkTarget,
    /// Properties; an `interpolate` curve name is kept as the property `interpolate`.
    pub props: Vec<StyleProp>,
    /// 1-based source line of the statement.
    pub line: usize,
}

/// One node reference inside a chain, before it is applied.
#[derive(Debug)]
struct NodeRef {
    id: String,
    shape: Option<Shape>,
    label: Option<String>,
    classes: Vec<String>,
    ignored_attrs: Vec<String>,
    unknown_shape: Option<String>,
    at_form: bool,
}

/// One link inside a chain, before it is applied.
#[derive(Debug)]
struct LinkTok {
    id: Option<String>,
    line: LineStyle,
    start: ArrowKind,
    end: ArrowKind,
    length: u8,
    label: Option<String>,
}

/// Node groups joined by links: `groups.len() == links.len() + 1`.
#[derive(Debug)]
struct Chain {
    groups: Vec<Vec<NodeRef>>,
    links: Vec<LinkTok>,
}

struct Builder {
    fc: Flowchart,
    node_index: HashMap<String, usize>,
    sub_index: HashMap<String, usize>,
    edge_ids: HashSet<String>,
    explicit: HashSet<String>,
    /// Open subgraphs (indices into `fc.subgraphs`) with the line that opened each.
    stack: Vec<(usize, usize)>,
    auto_sub: usize,
}

/// Parse a flowchart from preprocessed input.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<Flowchart, ParseError> {
    let mut b = Builder {
        fc: Flowchart { meta: prep.meta.clone(), ..Flowchart::default() },
        node_index: HashMap::new(),
        sub_index: HashMap::new(),
        edge_ids: HashSet::new(),
        explicit: HashSet::new(),
        stack: Vec::new(),
        auto_sub: 0,
    };
    let mut header = prep.header;
    let dir_end = header.text.find(|c: char| c.is_whitespace() || c == ';').unwrap_or(header.text.len());
    let dir_word = header.text.get(..dir_end).unwrap_or("");
    if let Some(d) = Direction::from_keyword(dir_word) {
        b.fc.direction = d;
        let rest = header.text.get(dir_end..).unwrap_or("");
        let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ';');
        header.col = header.col.saturating_add(header.text.len().saturating_sub(trimmed.len()));
        header.text = trimmed;
    } else if !dir_word.is_empty() {
        ctx.warn(header.no, format!("`{dir_word}` is not a direction; using TB"));
    }
    let mut lines = Vec::with_capacity(prep.body.len().saturating_add(1));
    if !header.text.is_empty() {
        lines.push(header);
    }
    lines.extend(prep.body.iter().copied());

    for stmt in split_statements(&lines, true) {
        ctx.statement(stmt.line)?;
        let r = b.statement(&stmt.text, stmt.line, ctx);
        settle(ctx, &stmt, r)?;
    }
    if let Some(&(idx, line)) = b.stack.last() {
        let id = b.fc.subgraphs.get(idx).map(|s| s.id.clone()).unwrap_or_default();
        return Err(ParseError::new(line, 1, format!("subgraph `{id}` is never closed with `end`")));
    }
    b.finish(ctx)
}

impl Builder {
    fn statement(&mut self, text: &str, line: usize, ctx: &mut Ctx) -> StmtResult<()> {
        let (kw, rest) = first_word(text);
        match kw {
            "subgraph" => return self.open_subgraph(rest, line),
            "end" if rest.is_empty() => {
                if self.stack.pop().is_none() {
                    return Err(StmtError::Unknown(0, "`end` without an open subgraph".into()));
                }
                return Ok(());
            }
            "direction" => {
                let Some(d) = Direction::from_keyword(rest) else {
                    return Err(StmtError::Unknown(0, format!("`{rest}` is not a direction")));
                };
                match self.stack.last().and_then(|&(i, _)| self.fc.subgraphs.get_mut(i)) {
                    Some(s) => s.direction = Some(d),
                    None => self.fc.direction = d,
                }
                return Ok(());
            }
            "classDef" => {
                let (names, props) = first_word(rest);
                if names.is_empty() {
                    return Err(StmtError::Unknown(0, "`classDef` needs a class name".into()));
                }
                add_class_def(&mut self.fc.class_defs, names, props);
                return Ok(());
            }
            "class" => {
                let (ids, rest2) = first_word(rest);
                let (classes, _) = first_word(rest2);
                if ids.is_empty() || classes.is_empty() {
                    return Err(StmtError::Unknown(0, "`class` needs node ids and a class name".into()));
                }
                for id in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    for class in classes.split(',').map(str::trim) {
                        self.add_class(id, class, line)?;
                    }
                }
                return Ok(());
            }
            "style" => {
                let (id, props) = first_word(rest);
                if id.is_empty() {
                    return Err(StmtError::Unknown(0, "`style` needs a node id".into()));
                }
                let props = parse_style_props(props);
                if let Some(&i) = self.sub_index.get(id) {
                    if let Some(s) = self.fc.subgraphs.get_mut(i) {
                        s.style.extend(props);
                    }
                } else {
                    let i = self.ensure_node(id, line)?;
                    if let Some(n) = self.fc.nodes.get_mut(i) {
                        n.style.extend(props);
                    }
                }
                return Ok(());
            }
            "linkStyle" => return self.link_style(rest, line),
            "click" => {
                let Some(click) = parse_click(rest) else {
                    return Err(StmtError::Unknown(0, "`click` needs a node id and an action".into()));
                };
                if click.callback.is_some() {
                    ctx.warn(line, format!("click callback on `{}` kept as a hint only; callbacks never run", click.node));
                }
                self.fc.clicks.push(click);
                return Ok(());
            }
            _ => {}
        }
        let chain = parse_chain(text)?;
        self.apply_chain(chain, line, ctx)
    }

    fn open_subgraph(&mut self, rest: &str, line: usize) -> StmtResult<()> {
        check_depth(self.stack.len(), line, 1)?;
        let (id, title) = subgraph_header(rest, &mut self.auto_sub)?;
        let parent = self.stack.last().and_then(|&(i, _)| self.fc.subgraphs.get(i)).map(|s| s.id.clone());
        let idx = match self.sub_index.get(&id) {
            Some(&i) => i,
            None => {
                check_nodes(self.fc.subgraphs.len().saturating_add(self.fc.nodes.len()), line)?;
                let i = self.fc.subgraphs.len();
                self.fc.subgraphs.push(Subgraph {
                    id: id.clone(),
                    title,
                    direction: None,
                    parent,
                    nodes: Vec::new(),
                    classes: Vec::new(),
                    style: Vec::new(),
                });
                self.sub_index.insert(id, i);
                i
            }
        };
        self.stack.push((idx, line));
        Ok(())
    }

    fn link_style(&mut self, rest: &str, line: usize) -> StmtResult<()> {
        let (targets_word, mut props_text) = first_word(rest);
        let targets = if targets_word == "default" {
            LinkTarget::Default
        } else {
            let mut idx = Vec::new();
            for part in targets_word.split(',').filter(|p| !p.is_empty()) {
                match part.trim().parse::<usize>() {
                    Ok(i) => idx.push(i),
                    Err(_) => return Err(StmtError::Unknown(0, format!("`{part}` is not a link index"))),
                }
            }
            if idx.is_empty() {
                return Err(StmtError::Unknown(0, "`linkStyle` needs link indices or `default`".into()));
            }
            LinkTarget::Indices(idx)
        };
        let mut props = Vec::new();
        let (w, after) = first_word(props_text);
        if w == "interpolate" {
            let (curve, after2) = first_word(after);
            props.push(StyleProp::new("interpolate", curve));
            props_text = after2;
        }
        props.extend(parse_style_props(props_text));
        self.fc.link_styles.push(LinkStyle { targets, props, line });
        Ok(())
    }

    fn add_class(&mut self, id: &str, class: &str, line: usize) -> Result<(), ParseError> {
        if let Some(&i) = self.sub_index.get(id) {
            if let Some(s) = self.fc.subgraphs.get_mut(i) {
                push_unique(&mut s.classes, class);
            }
            return Ok(());
        }
        let i = self.ensure_node(id, line)?;
        if let Some(n) = self.fc.nodes.get_mut(i) {
            push_unique(&mut n.classes, class);
        }
        Ok(())
    }

    /// The index of a node, creating it with default text and shape when new.
    fn ensure_node(&mut self, id: &str, line: usize) -> Result<usize, ParseError> {
        if let Some(&i) = self.node_index.get(id) {
            return Ok(i);
        }
        check_nodes(self.fc.nodes.len(), line)?;
        let i = self.fc.nodes.len();
        self.fc.nodes.push(FlowNode {
            id: id.to_string(),
            label: id.to_string(),
            shape: Shape::Rectangle,
            classes: Vec::new(),
            style: Vec::new(),
            subgraph: None,
        });
        self.node_index.insert(id.to_string(), i);
        Ok(i)
    }

    fn apply_node(&mut self, nr: &NodeRef, line: usize, ctx: &mut Ctx) -> Result<(), ParseError> {
        for key in &nr.ignored_attrs {
            ctx.warn(line, format!("node attribute `{key}` on `{}` is not supported and was ignored", nr.id));
        }
        if let Some(name) = &nr.unknown_shape {
            ctx.warn(line, format!("unknown shape `{name}` on `{}`; kept by name", nr.id));
        }
        let i = self.ensure_node(&nr.id, line)?;
        let in_sub = self.stack.last().and_then(|&(s, _)| self.fc.subgraphs.get(s)).map(|s| s.id.clone());
        let is_sub_id = self.sub_index.contains_key(&nr.id);
        if let Some(n) = self.fc.nodes.get_mut(i) {
            if let Some(shape) = &nr.shape {
                n.shape = shape.clone();
                self.explicit.insert(nr.id.clone());
            }
            if let Some(label) = &nr.label {
                n.label.clone_from(label);
                self.explicit.insert(nr.id.clone());
            }
            for c in &nr.classes {
                push_unique(&mut n.classes, c);
            }
            // A node belongs to the first subgraph that mentions it; mentions at the top level
            // do not claim it, so a node defined before its subgraph still moves inside.
            if n.subgraph.is_none()
                && !is_sub_id
                && let Some(sub_id) = in_sub
            {
                n.subgraph = Some(sub_id.clone());
                if let Some(&si) = self.sub_index.get(&sub_id)
                    && let Some(s) = self.fc.subgraphs.get_mut(si)
                {
                    s.nodes.push(nr.id.clone());
                }
            }
        }
        Ok(())
    }

    fn apply_chain(&mut self, chain: Chain, line: usize, ctx: &mut Ctx) -> StmtResult<()> {
        // `e1@{ animate: true }` sets properties of a known edge; none are supported.
        if chain.links.is_empty()
            && let Some([only]) = chain.groups.first().map(Vec::as_slice)
            && only.at_form
            && self.edge_ids.contains(&only.id)
        {
            ctx.warn(line, format!("edge properties on `{}` are not supported and were ignored", only.id));
            return Ok(());
        }
        for group in &chain.groups {
            for nr in group {
                // A subgraph id used in a chain names the subgraph, not a new node, unless the
                // statement gives it a shape or label.
                if self.sub_index.contains_key(&nr.id) && nr.shape.is_none() && nr.label.is_none() {
                    if let Some(&si) = self.sub_index.get(&nr.id)
                        && let Some(s) = self.fc.subgraphs.get_mut(si)
                    {
                        for c in &nr.classes {
                            push_unique(&mut s.classes, c);
                        }
                    }
                    continue;
                }
                self.apply_node(nr, line, ctx)?;
            }
        }
        for (i, link) in chain.links.iter().enumerate() {
            let (Some(from), Some(to)) = (chain.groups.get(i), chain.groups.get(i.saturating_add(1))) else {
                continue;
            };
            let mut first = true;
            for a in from {
                for b in to {
                    check_edges(self.fc.edges.len(), line)?;
                    let id = if first { link.id.clone() } else { None };
                    first = false;
                    if let Some(id) = &id {
                        self.edge_ids.insert(id.clone());
                    }
                    self.fc.edges.push(FlowEdge {
                        id,
                        from: a.id.clone(),
                        to: b.id.clone(),
                        label: link.label.clone(),
                        line: link.line,
                        start: link.start,
                        end: link.end,
                        length: link.length,
                        style: Vec::new(),
                    });
                }
            }
        }
        Ok(())
    }

    fn finish(mut self, ctx: &mut Ctx) -> Result<Flowchart, ParseError> {
        // Drop placeholder nodes created for subgraph ids before the subgraph was opened.
        let drop: HashSet<String> =
            self.fc.nodes.iter().filter(|n| self.sub_index.contains_key(&n.id) && !self.explicit.contains(&n.id)).map(|n| n.id.clone()).collect();
        if !drop.is_empty() {
            self.fc.nodes.retain(|n| !drop.contains(&n.id));
            for s in &mut self.fc.subgraphs {
                s.nodes.retain(|n| !drop.contains(n));
            }
        }
        let count = self.fc.edges.len();
        let styles = std::mem::take(&mut self.fc.link_styles);
        for ls in &styles {
            if ls.targets == LinkTarget::Default {
                for e in &mut self.fc.edges {
                    e.style.extend(ls.props.iter().cloned());
                }
            }
        }
        for ls in &styles {
            if let LinkTarget::Indices(idx) = &ls.targets {
                for &i in idx {
                    match self.fc.edges.get_mut(i) {
                        Some(e) => e.style.extend(ls.props.iter().cloned()),
                        None => ctx.warn(ls.line, format!("linkStyle index {i} is out of range; the flowchart has {count} links")),
                    }
                }
            }
        }
        self.fc.link_styles = styles;
        Ok(self.fc)
    }
}

/// Parse a subgraph header: `id`, `id [title]`, `id["title"]`, `"title"` or `title words`.
fn subgraph_header(rest: &str, auto: &mut usize) -> StmtResult<(String, String)> {
    let rest = rest.trim();
    if rest.is_empty() {
        *auto = auto.saturating_add(1);
        let id = format!("subGraph{}", *auto);
        return Ok((id, String::new()));
    }
    if rest.starts_with('"') {
        let title = clean_label(rest);
        return Ok((title.clone(), title));
    }
    if let Some(open) = rest.find('[') {
        let id = rest.get(..open).unwrap_or("").trim();
        let after = rest.get(open.saturating_add(1)..).unwrap_or("");
        let Some(close) = after.rfind(']') else {
            return Err(StmtError::Malformed(open, "subgraph title `[` is never closed".into()));
        };
        let title = clean_label(after.get(..close).unwrap_or(""));
        let id = if id.is_empty() { title.clone() } else { id.to_string() };
        return Ok((id, title));
    }
    Ok((rest.to_string(), clean_label(rest)))
}

/// Parse a node and link chain.
fn parse_chain(text: &str) -> StmtResult<Chain> {
    let mut cur = Cursor::new(text);
    let first = match parse_group(&mut cur, true)? {
        Some(g) => g,
        None => return Err(StmtError::Unknown(0, "not a flowchart statement".into())),
    };
    let mut chain = Chain { groups: vec![first], links: Vec::new() };
    loop {
        cur.skip_ws();
        if cur.is_eof() {
            break;
        }
        let save = cur.pos();
        let mut edge_id = None;
        let ident = cur.word(true, true);
        if !ident.is_empty() && cur.eat("@") && cur.peek() != Some('{') {
            edge_id = Some(ident.to_string());
        } else {
            cur.set_pos(save);
        }
        let Some(mut link) = parse_link(&mut cur)? else {
            cur.set_pos(save);
            return Err(StmtError::Unknown(save, "unexpected text after a node".into()));
        };
        link.id = edge_id;
        let at = cur.pos();
        let Some(group) = parse_group(&mut cur, false)? else {
            return Err(StmtError::Malformed(at, "link has no target node".into()));
        };
        chain.links.push(link);
        chain.groups.push(group);
    }
    Ok(chain)
}

/// Parse `node (& node)*`; `None` when no node id starts here.
fn parse_group(cur: &mut Cursor<'_>, first: bool) -> StmtResult<Option<Vec<NodeRef>>> {
    let Some(n) = parse_node_ref(cur)? else {
        return Ok(None);
    };
    let mut group = vec![n];
    loop {
        let save = cur.pos();
        cur.skip_ws();
        if !cur.eat("&") {
            cur.set_pos(save);
            break;
        }
        let at = cur.pos();
        match parse_node_ref(cur)? {
            Some(n) => group.push(n),
            None => {
                let msg = if first { "`&` is not followed by a node" } else { "`&` is not followed by a target node" };
                return Err(StmtError::Malformed(at, msg.into()));
            }
        }
    }
    Ok(Some(group))
}

/// Parse one node reference: an id, then a bracket shape or `@{ ... }`, then `:::class`es.
fn parse_node_ref(cur: &mut Cursor<'_>) -> StmtResult<Option<NodeRef>> {
    cur.skip_ws();
    let id = cur.word(true, true);
    if id.is_empty() {
        return Ok(None);
    }
    let mut nr =
        NodeRef { id: id.to_string(), shape: None, label: None, classes: Vec::new(), ignored_attrs: Vec::new(), unknown_shape: None, at_form: false };
    if cur.rest().starts_with("@{") {
        let at = cur.pos();
        cur.eat("@{");
        let Some(body) = take_until_brace(cur) else {
            return Err(StmtError::Malformed(at, "`@{` is never closed with `}`".into()));
        };
        nr.at_form = true;
        for (key, value) in parse_attr_map(body) {
            match key.as_str() {
                "shape" => match shape_from_name(&value) {
                    Some(s) => nr.shape = Some(s),
                    None => {
                        nr.unknown_shape = Some(value.clone());
                        nr.shape = Some(Shape::Other(value));
                    }
                },
                "label" => nr.label = Some(clean_label(&value)),
                _ => nr.ignored_attrs.push(key),
            }
        }
    } else {
        let save = cur.pos();
        cur.skip_ws();
        match parse_shape(cur)? {
            Some((shape, label)) => {
                nr.shape = Some(shape);
                nr.label = Some(label);
            }
            None => cur.set_pos(save),
        }
    }
    while cur.eat(":::") {
        let class = cur.word(true, false);
        if !class.is_empty() {
            push_unique(&mut nr.classes, class);
        }
    }
    Ok(Some(nr))
}

/// Consume up to and including the `}` that closes an `@{`, skipping quoted text.
fn take_until_brace<'a>(cur: &mut Cursor<'a>) -> Option<&'a str> {
    let start = cur.pos();
    let mut in_quote = false;
    while let Some(c) = cur.peek() {
        if c == '"' {
            in_quote = !in_quote;
        } else if c == '}' && !in_quote {
            let body = cur.slice(start, cur.pos());
            cur.bump();
            return Some(body);
        }
        cur.bump();
    }
    None
}

/// Parse the `key: value, key: "value"` body of `@{ ... }`.
fn parse_attr_map(body: &str) -> Vec<(String, String)> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in body.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            ',' | '\n' if !in_quote => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    parts
        .iter()
        .filter_map(|p| {
            let (k, v) = p.split_once(':')?;
            let k = k.trim();
            if k.is_empty() {
                return None;
            }
            Some((k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Bracket shapes: opening token and the closing tokens that may end it, longest first so
/// `((` wins over `(`. When an opening has no matching close, shorter openings are tried, so
/// `A[/path]` falls back to a rectangle labelled `/path`.
const SHAPES: &[(&str, &[(&str, Shape)])] = &[
    ("(((", &[(")))", Shape::DoubleCircle)]),
    ("((", &[("))", Shape::Circle)]),
    ("([", &[("])", Shape::Stadium)]),
    ("(", &[(")", Shape::Rounded)]),
    ("[[", &[("]]", Shape::Subroutine)]),
    ("[(", &[(")]", Shape::Cylinder)]),
    ("[/", &[("/]", Shape::Parallelogram), ("\\]", Shape::Trapezoid)]),
    ("[\\", &[("\\]", Shape::ParallelogramAlt), ("/]", Shape::TrapezoidAlt)]),
    ("[", &[("]", Shape::Rectangle)]),
    ("{{", &[("}}", Shape::Hexagon)]),
    ("{", &[("}", Shape::Diamond)]),
    (">", &[("]", Shape::Asymmetric)]),
];

/// Parse a bracket shape with its label at the cursor.
fn parse_shape(cur: &mut Cursor<'_>) -> StmtResult<Option<(Shape, String)>> {
    let start = cur.pos();
    let rest = cur.rest();
    let mut first_open: Option<&str> = None;
    for (open, closes) in SHAPES {
        let Some(after) = rest.strip_prefix(open) else {
            continue;
        };
        first_open.get_or_insert(open);
        let inner_start = open.len();
        let lead = after.len().saturating_sub(after.trim_start().len());
        let body = after.trim_start();
        // A quoted label: everything up to the closing quote, then the close token.
        if body.starts_with('"')
            && let Some(qend) = body.get(1..).and_then(|b| b.find('"'))
        {
            let label_raw = body.get(..qend.saturating_add(2)).unwrap_or("");
            let tail = body.get(qend.saturating_add(2)..).unwrap_or("");
            let tail_trim = tail.trim_start();
            for (close, shape) in *closes {
                if tail_trim.starts_with(close) {
                    let used = inner_start + lead + label_raw.len() + (tail.len() - tail_trim.len()) + close.len();
                    cur.set_pos(start);
                    cur.advance(used);
                    return Ok(Some((shape.clone(), clean_label(label_raw))));
                }
            }
            continue;
        }
        // Unquoted: the earliest close token ends the label.
        let mut best: Option<(usize, &str, &Shape)> = None;
        for (close, shape) in *closes {
            if let Some(p) = after.find(close)
                && best.is_none_or(|(bp, _, _)| p < bp)
            {
                best = Some((p, close, shape));
            }
        }
        if let Some((p, close, shape)) = best {
            let label = after.get(..p).unwrap_or("");
            cur.set_pos(start);
            cur.advance(inner_start + p + close.len());
            return Ok(Some((shape.clone(), clean_label(label))));
        }
    }
    match first_open {
        Some(open) => Err(StmtError::Malformed(start, format!("shape opened with `{open}` is never closed"))),
        None => Ok(None),
    }
}

/// Map a `@{ shape: ... }` name to a shape.
pub(crate) fn shape_from_name(name: &str) -> Option<Shape> {
    let s = match name.trim().to_ascii_lowercase().as_str() {
        "rect" | "rectangle" | "proc" | "process" => Shape::Rectangle,
        "rounded" | "event" => Shape::Rounded,
        "stadium" | "pill" | "terminal" => Shape::Stadium,
        "fr-rect" | "subprocess" | "subproc" | "framed-rectangle" | "subroutine" => Shape::Subroutine,
        "cyl" | "cylinder" | "database" | "db" => Shape::Cylinder,
        "circle" | "circ" => Shape::Circle,
        "dbl-circ" | "double-circle" => Shape::DoubleCircle,
        "odd" | "asymmetric" => Shape::Asymmetric,
        "diam" | "diamond" | "decision" | "question" | "rhombus" => Shape::Diamond,
        "hex" | "hexagon" | "prepare" => Shape::Hexagon,
        "lean-r" | "lean-right" | "in-out" => Shape::Parallelogram,
        "lean-l" | "lean-left" | "out-in" => Shape::ParallelogramAlt,
        "trap-b" | "trapezoid-bottom" | "priority" | "trapezoid" => Shape::Trapezoid,
        "trap-t" | "trapezoid-top" | "inv-trapezoid" | "manual" => Shape::TrapezoidAlt,
        "doc" | "document" => Shape::Document,
        "docs" | "documents" | "st-doc" | "stacked-document" => Shape::MultiDocument,
        "lin-doc" | "lined-document" => Shape::LinedDocument,
        "tag-doc" | "tagged-document" => Shape::TaggedDocument,
        "sm-circ" | "small-circle" | "start" => Shape::SmallCircle,
        "fr-circ" | "framed-circle" | "stop" => Shape::FramedCircle,
        "f-circ" | "filled-circle" | "junction" => Shape::FilledCircle,
        "cross-circ" | "crossed-circle" | "summary" => Shape::CrossCircle,
        "cloud" => Shape::Cloud,
        "tri" | "triangle" | "extract" => Shape::Triangle,
        "flip-tri" | "flipped-triangle" | "manual-file" => Shape::FlippedTriangle,
        "text" => Shape::Text,
        "notch-rect" | "notched-rectangle" | "card" => Shape::NotchedRectangle,
        "lin-rect" | "lined-rectangle" | "lin-proc" | "lined-process" | "shaded-process" => Shape::LinedRectangle,
        "st-rect" | "stacked-rectangle" | "procs" | "processes" => Shape::StackedRectangle,
        "tag-rect" | "tagged-rectangle" | "tag-proc" | "tagged-process" => Shape::TaggedRectangle,
        "div-rect" | "divided-rectangle" | "div-proc" | "divided-process" => Shape::DividedRectangle,
        "win-pane" | "window-pane" | "internal-storage" => Shape::InternalStorage,
        "sl-rect" | "sloped-rectangle" | "manual-input" => Shape::ManualInput,
        "delay" | "half-rounded-rectangle" => Shape::Delay,
        "h-cyl" | "das" | "horizontal-cylinder" => Shape::HorizontalCylinder,
        "lin-cyl" | "disk" | "lined-cylinder" => Shape::LinedCylinder,
        "curv-trap" | "curved-trapezoid" | "display" => Shape::Display,
        "bow-rect" | "bow-tie-rectangle" | "stored-data" => Shape::StoredData,
        "notch-pent" | "notched-pentagon" | "loop-limit" => Shape::LoopLimit,
        "hourglass" | "collate" => Shape::Hourglass,
        "flag" | "paper-tape" => Shape::Flag,
        "bolt" | "com-link" | "lightning-bolt" => Shape::Bolt,
        "brace" | "comment" | "brace-l" | "brace-r" | "braces" => Shape::Brace,
        "bang" => Shape::Bang,
        "fork" | "join" => Shape::ForkBar,
        _ => return None,
    };
    Some(s)
}

/// Parse a link at the cursor. `Ok(None)` (cursor unchanged) when no link starts here.
fn parse_link(cur: &mut Cursor<'_>) -> StmtResult<Option<LinkTok>> {
    let start = cur.pos();
    let mut start_head = ArrowKind::None;
    match (cur.peek(), cur.peek_nth(1), cur.peek_nth(2)) {
        (Some('<'), Some('-' | '='), _) => {
            cur.bump();
            start_head = ArrowKind::Arrow;
        }
        (Some('o'), Some('-' | '='), Some('-' | '=' | '.')) => {
            cur.bump();
            start_head = ArrowKind::Circle;
        }
        (Some('x'), Some('-' | '='), Some('-' | '=' | '.')) => {
            cur.bump();
            start_head = ArrowKind::Cross;
        }
        _ => {}
    }
    let body_start = cur.pos();
    let link = match cur.peek() {
        Some('-') if cur.peek_nth(1) == Some('.') => dotted_link(cur, body_start)?,
        Some('-') => solid_link(cur, '-', LineStyle::Solid, body_start)?,
        Some('=') => solid_link(cur, '=', LineStyle::Thick, body_start)?,
        Some('~') => {
            let n = cur.count_run('~');
            if n >= 3 {
                Some(LinkTok { id: None, line: LineStyle::Invisible, start: ArrowKind::None, end: ArrowKind::None, length: len(n, 2), label: None })
            } else {
                None
            }
        }
        _ => None,
    };
    let Some(mut link) = link else {
        cur.set_pos(start);
        return Ok(None);
    };
    link.start = start_head;
    // `-->|text|`: the pipe form of link text.
    let save = cur.pos();
    cur.skip_ws();
    if cur.peek() == Some('|') {
        let at = cur.pos();
        cur.bump();
        let rest = cur.rest();
        let Some(end) = rest.find('|') else {
            return Err(StmtError::Malformed(at, "link text `|` is never closed".into()));
        };
        let text = clean_label(rest.get(..end).unwrap_or(""));
        cur.advance(end.saturating_add(1));
        if !text.is_empty() {
            link.label = Some(text);
        }
    } else {
        cur.set_pos(save);
    }
    Ok(Some(link))
}

/// Convert a run length to a rank length, never below 1.
fn len(run: usize, base: usize) -> u8 {
    u8::try_from(run.saturating_sub(base).max(1)).unwrap_or(u8::MAX)
}

/// An end marker character.
fn head_of(c: Option<char>) -> Option<ArrowKind> {
    match c {
        Some('>') => Some(ArrowKind::Arrow),
        Some('o') => Some(ArrowKind::Circle),
        Some('x') => Some(ArrowKind::Cross),
        _ => None,
    }
}

/// `---`, `-->`, `--o`, `-- text -->` and their thick `=` forms.
fn solid_link(cur: &mut Cursor<'_>, ch: char, line: LineStyle, at: usize) -> StmtResult<Option<LinkTok>> {
    let n = cur.count_run(ch);
    if n < 2 {
        return Ok(None);
    }
    if let Some(end) = head_of(cur.peek()) {
        cur.bump();
        return Ok(Some(LinkTok { id: None, line, start: ArrowKind::None, end, length: len(n, 1), label: None }));
    }
    if n >= 3 {
        return Ok(Some(LinkTok { id: None, line, start: ArrowKind::None, end: ArrowKind::None, length: len(n, 2), label: None }));
    }
    // Exactly two: the text form, closed by the next run of two or more with a head, or three
    // or more without one.
    let rest = cur.rest();
    let Some((text_end, close_end, end, m)) = find_solid_close(rest, ch) else {
        return Err(StmtError::Malformed(at, "link text is never closed with an arrow".into()));
    };
    let label = clean_label(rest.get(..text_end).unwrap_or(""));
    cur.advance(close_end);
    let length = if end == ArrowKind::None { len(m, 2) } else { len(m, 1) };
    Ok(Some(LinkTok { id: None, line, start: ArrowKind::None, end, length, label: (!label.is_empty()).then_some(label) }))
}

/// Find the closing part of a text link: `(text end, close end, head, run length)` in bytes.
fn find_solid_close(rest: &str, ch: char) -> Option<(usize, usize, ArrowKind, usize)> {
    let mut iter = rest.char_indices().peekable();
    let mut prev: Option<char> = None;
    while let Some((i, c)) = iter.next() {
        if c == ch && prev != Some(ch) {
            let mut m = 1usize;
            let mut end_byte = i.saturating_add(c.len_utf8());
            while let Some(&(j, c2)) = iter.peek() {
                if c2 != ch {
                    break;
                }
                m = m.saturating_add(1);
                end_byte = j.saturating_add(c2.len_utf8());
                iter.next();
            }
            let next = rest.get(end_byte..).and_then(|r| r.chars().next());
            if m >= 2
                && let Some(head) = head_of(next)
            {
                return Some((i, end_byte.saturating_add(1), head, m));
            }
            if m >= 3 {
                return Some((i, end_byte, ArrowKind::None, m));
            }
            prev = Some(ch);
            continue;
        }
        prev = Some(c);
    }
    None
}

/// `-.-`, `-.->`, `-..->` and `-. text .->`.
fn dotted_link(cur: &mut Cursor<'_>, at: usize) -> StmtResult<Option<LinkTok>> {
    cur.bump();
    let k = cur.count_run('.');
    if cur.peek() == Some('-') {
        cur.bump();
        let end = match head_of(cur.peek()) {
            Some(h) => {
                cur.bump();
                h
            }
            None => ArrowKind::None,
        };
        return Ok(Some(LinkTok { id: None, line: LineStyle::Dotted, start: ArrowKind::None, end, length: len(k, 0), label: None }));
    }
    // The text form: closed by one or more dots, a dash and an optional head.
    let rest = cur.rest();
    let mut found = None;
    let mut iter = rest.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c != '.' {
            continue;
        }
        let mut dots = 1usize;
        let mut after = i.saturating_add(1);
        while let Some(&(j, '.')) = iter.peek() {
            dots = dots.saturating_add(1);
            after = j.saturating_add(1);
            iter.next();
        }
        if rest.get(after..).is_some_and(|r| r.starts_with('-')) {
            found = Some((i, after.saturating_add(1), dots));
            break;
        }
    }
    let Some((text_end, mut close_end, dots)) = found else {
        return Err(StmtError::Malformed(at, "dotted link text is never closed with `.->` or `.-`".into()));
    };
    let label = clean_label(rest.get(..text_end).unwrap_or(""));
    let end = match head_of(rest.get(close_end..).and_then(|r| r.chars().next())) {
        Some(h) => {
            close_end = close_end.saturating_add(1);
            h
        }
        None => ArrowKind::None,
    };
    cur.advance(close_end);
    Ok(Some(LinkTok {
        id: None,
        line: LineStyle::Dotted,
        start: ArrowKind::None,
        end,
        length: len(dots, 0),
        label: (!label.is_empty()).then_some(label),
    }))
}

impl Flowchart {
    /// The uniform graph view.
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::Flowchart, self.direction, self.meta.title.clone());
        for n in &self.nodes {
            let mut node = GraphNode::new(n.id.clone(), n.label.clone(), n.shape.clone());
            node.classes.clone_from(&n.classes);
            node.style.clone_from(&n.style);
            node.group.clone_from(&n.subgraph);
            if let Some(c) = self.clicks.iter().rev().find(|c| c.node == n.id) {
                node.link.clone_from(&c.url);
                node.tooltip.clone_from(&c.tooltip);
            }
            g.nodes.push(node);
        }
        for e in &self.edges {
            let mut edge = GraphEdge::new(e.from.clone(), e.to.clone());
            edge.id.clone_from(&e.id);
            edge.label.clone_from(&e.label);
            edge.line = e.line;
            edge.start = e.start;
            edge.end = e.end;
            edge.length = e.length;
            edge.style.clone_from(&e.style);
            g.edges.push(edge);
        }
        for s in &self.subgraphs {
            let mut group = GraphGroup::new(s.id.clone(), s.title.clone(), GroupKind::Subgraph);
            group.parent.clone_from(&s.parent);
            group.members.clone_from(&s.nodes);
            group.direction = s.direction;
            group.classes.clone_from(&s.classes);
            group.style.clone_from(&s.style);
            g.groups.push(group);
        }
        g.class_defs.clone_from(&self.class_defs);
        g
    }
}
