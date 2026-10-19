//! Architecture diagrams (`architecture-beta`): groups, services and junctions placed with `in`,
//! and edges between the sides of services.
//!
//! Only the statements the public mermaid.js.org architecture page documents are read:
//!
//! - `group <id>(<icon>)[<title>] in <parent>` (icon, title and `in` optional);
//! - `service <id>(<icon>)[<title>] in <parent>`;
//! - `junction <id> in <parent>`;
//! - edges `<id>:<side> -- <side>:<id>`, sides `L`, `R`, `T`, `B`, with `-->`, `<--` or `<-->`
//!   for arrows, and `{group}` after an id to attach the edge to the group the service is in.
//!
//! Anything else is skipped with a warning. An `in` naming nothing is a warning; a cycle of `in`
//! or nesting deeper than [`crate::limits::MAX_DEPTH`] is an error.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Direction;
use crate::common::{Ctx, Meta, ParseError, Prepared, Stmt, StmtError, StmtResult, check_depth, check_edges, check_nodes, settle};
use crate::graph::{ArrowKind, DiagramKind, Graph, GraphEdge, GraphGroup, GraphNode, GroupKind, Shape};

/// A side of a service an edge leaves or meets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// `L`.
    Left,
    /// `R`.
    Right,
    /// `T`.
    Top,
    /// `B`.
    Bottom,
}

impl Side {
    /// The side for its letter.
    pub fn from_letter(s: &str) -> Option<Side> {
        match s {
            "L" => Some(Side::Left),
            "R" => Some(Side::Right),
            "T" => Some(Side::Top),
            "B" => Some(Side::Bottom),
            _ => None,
        }
    }
    /// The letter the text writes.
    pub fn letter(self) -> &'static str {
        match self {
            Side::Left => "L",
            Side::Right => "R",
            Side::Top => "T",
            Side::Bottom => "B",
        }
    }
}

/// A group, service or junction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Its id.
    pub id: String,
    /// The icon name in parentheses, if any (`database`, or an icon pack name such as `logos:x`).
    pub icon: Option<String>,
    /// The title in brackets, if any.
    pub title: Option<String>,
    /// The group it is `in`.
    pub parent: Option<String>,
    /// The line it was declared on.
    pub line: usize,
}

/// One end of an edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeEnd {
    /// The service or junction id.
    pub id: String,
    /// The side.
    pub side: Side,
    /// Written `{group}`: the edge attaches to the group around the service.
    pub group: bool,
    /// An arrowhead at this end.
    pub arrow: bool,
}

/// An edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Where it starts.
    pub from: EdgeEnd,
    /// Where it ends.
    pub to: EdgeEnd,
}

/// An `architecture-beta` diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArchitectureDiagram {
    /// Title and accessibility text.
    pub meta: Meta,
    /// Groups, in declaration order.
    pub groups: Vec<Item>,
    /// Services, in declaration order.
    pub services: Vec<Item>,
    /// Junctions, in declaration order.
    pub junctions: Vec<Item>,
    /// Edges, in order.
    pub edges: Vec<Edge>,
}

/// Whether `s` reads as an id: letters, digits, `_` and `-`.
fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `<id>(<icon>)[<title>] in <parent>` after the keyword.
fn item(rest: &str, line: usize, junction: bool) -> StmtResult<Item> {
    let rest = rest.trim();
    let end = rest.find(|c: char| c == '(' || c == '[' || c.is_whitespace()).unwrap_or(rest.len());
    let id = rest.get(..end).unwrap_or_default();
    if !is_id(id) {
        return Err(StmtError::Malformed(0, format!("`{}` is not an id", crate::common::snippet(id))));
    }
    let mut at = rest.get(end..).unwrap_or_default();
    let mut icon = None;
    let mut title = None;
    if !junction && let Some(r) = at.strip_prefix('(') {
        let close = r.find(')').ok_or_else(|| StmtError::Malformed(end, "an icon `(` is never closed".into()))?;
        icon = Some(r.get(..close).unwrap_or_default().trim().to_string());
        at = r.get(close + 1..).unwrap_or_default();
    }
    if !junction && let Some(r) = at.strip_prefix('[') {
        let close = r.find(']').ok_or_else(|| StmtError::Malformed(end, "a title `[` is never closed".into()))?;
        title = Some(crate::common::unquote(r.get(..close).unwrap_or_default().trim()).to_string());
        at = r.get(close + 1..).unwrap_or_default();
    }
    let at = at.trim();
    let parent = if at.is_empty() {
        None
    } else if let Some(p) = at.strip_prefix("in ").map(str::trim) {
        if !is_id(p) {
            return Err(StmtError::Malformed(0, format!("`{}` is not a group id", crate::common::snippet(p))));
        }
        Some(p.to_string())
    } else {
        return Err(StmtError::Unknown(0, format!("unexpected `{}`", crate::common::snippet(at))));
    };
    Ok(Item { id: id.to_string(), icon, title, parent, line })
}

/// One end written `<id>{group}:<side>` (left) or `<side>:<id>{group}` (right).
fn end(text: &str, left: bool, arrow: bool) -> StmtResult<EdgeEnd> {
    let bad = || StmtError::Malformed(0, format!("`{}` is not an edge end such as `db:R`", crate::common::snippet(text)));
    let (who, side) = if left { text.rsplit_once(':').ok_or_else(bad)? } else { text.split_once(':').map(|(s, w)| (w, s)).ok_or_else(bad)? };
    let side = Side::from_letter(side.trim()).ok_or_else(bad)?;
    let who = who.trim();
    let (id, group) = match who.strip_suffix("{group}") {
        Some(id) => (id.trim(), true),
        None => (who, false),
    };
    if !is_id(id) {
        return Err(bad());
    }
    Ok(EdgeEnd { id: id.to_string(), side, group, arrow })
}

fn edge(t: &str) -> StmtResult<Edge> {
    let (l, r) = t.split_once("--").ok_or_else(|| StmtError::Unknown(0, "not a statement of an architecture diagram".into()))?;
    let l = l.trim_end();
    let (l, from_arrow) = match l.strip_suffix('<') {
        Some(x) => (x, true),
        None => (l, false),
    };
    let r = r.trim_start();
    let (r, to_arrow) = match r.strip_prefix('>') {
        Some(x) => (x, true),
        None => (r, false),
    };
    Ok(Edge { from: end(l.trim(), true, from_arrow)?, to: end(r.trim(), false, to_arrow)? })
}

/// Parse the body of an `architecture-beta` diagram.
pub(crate) fn parse(prep: Prepared<'_>, ctx: &mut Ctx) -> Result<ArchitectureDiagram, ParseError> {
    let mut d = ArchitectureDiagram { meta: prep.meta.clone(), ..ArchitectureDiagram::default() };
    let mut ids: HashMap<String, usize> = HashMap::new();
    for l in prep.all_lines() {
        let stmt = Stmt::from_line(&l);
        ctx.statement(stmt.line)?;
        let t = stmt.text.trim();
        let (kw, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
        let r: StmtResult<()> = match kw {
            "group" | "service" | "junction" => item(rest, stmt.line, kw == "junction").and_then(|it| {
                check_nodes(d.groups.len() + d.services.len() + d.junctions.len(), stmt.line)?;
                if ids.insert(it.id.clone(), stmt.line).is_some() {
                    ctx.warn(stmt.line, format!("`{}` is declared again; the later one is kept too", crate::common::snippet(&it.id)));
                }
                match kw {
                    "group" => d.groups.push(it),
                    "service" => d.services.push(it),
                    _ => d.junctions.push(it),
                }
                Ok(())
            }),
            _ if t.contains("--") => edge(t).and_then(|e| {
                check_edges(d.edges.len(), stmt.line)?;
                d.edges.push(e);
                Ok(())
            }),
            _ => Err(StmtError::Unknown(0, "not a statement of an architecture diagram".into())),
        };
        settle(ctx, &stmt, r)?;
    }
    check_parents(&mut d, ctx)?;
    for e in &d.edges {
        for end in [&e.from, &e.to] {
            if !d.services.iter().chain(&d.junctions).any(|s| s.id == end.id) {
                ctx.warn(1, format!("an edge names `{}`, which is no service or junction", crate::common::snippet(&end.id)));
            }
        }
    }
    d.edges.retain(|e| [&e.from, &e.to].iter().all(|end| d.services.iter().chain(&d.junctions).any(|s| s.id == end.id)));
    Ok(d)
}

/// Every `in` names a group; a cycle or a chain deeper than the cap is an error, an unknown
/// group a warning (the item goes to the top level).
fn check_parents(d: &mut ArchitectureDiagram, ctx: &mut Ctx) -> Result<(), ParseError> {
    let parent_of: HashMap<String, Option<String>> = d.groups.iter().map(|g| (g.id.clone(), g.parent.clone())).collect();
    for it in d.groups.iter_mut().chain(d.services.iter_mut()).chain(d.junctions.iter_mut()) {
        let Some(p) = it.parent.clone() else { continue };
        if !parent_of.contains_key(&p) {
            ctx.warn(it.line, format!("`{}` is `in` `{}`, which is no group", crate::common::snippet(&it.id), crate::common::snippet(&p)));
            it.parent = None;
        }
    }
    for g in &d.groups {
        let mut at = g.parent.as_deref();
        let mut depth = 0usize;
        while let Some(p) = at {
            if p == g.id {
                return Err(ParseError::new(g.line, 1, format!("group `{}` is inside itself", crate::common::snippet(&g.id))));
            }
            check_depth(depth, g.line, 1)?;
            depth = depth.saturating_add(1);
            at = parent_of.get(p).and_then(|x| x.as_deref());
        }
    }
    Ok(())
}

impl ArchitectureDiagram {
    /// Services and junctions as nodes, groups as groups, edges with their arrows. Sides are
    /// left to the formats mapper, which reads the typed IR.
    pub fn to_graph(&self) -> Graph {
        let mut g = Graph::new(DiagramKind::Architecture, Direction::LeftToRight, self.meta.title.clone());
        for s in &self.services {
            let mut n = GraphNode::new(s.id.clone(), s.title.clone().unwrap_or_else(|| s.id.clone()), Shape::Rectangle);
            n.group.clone_from(&s.parent);
            n.annotations.extend(s.icon.clone());
            g.nodes.push(n);
        }
        for j in &self.junctions {
            let mut n = GraphNode::new(j.id.clone(), "", Shape::SmallCircle);
            n.group.clone_from(&j.parent);
            g.nodes.push(n);
        }
        for gr in &self.groups {
            let mut out = GraphGroup::new(gr.id.clone(), gr.title.clone().unwrap_or_else(|| gr.id.clone()), GroupKind::Subgraph);
            out.parent.clone_from(&gr.parent);
            out.members =
                self.services.iter().chain(&self.junctions).filter(|s| s.parent.as_deref() == Some(gr.id.as_str())).map(|s| s.id.clone()).collect();
            g.groups.push(out);
        }
        for e in &self.edges {
            let mut edge = GraphEdge::new(e.from.id.clone(), e.to.id.clone());
            edge.start = if e.from.arrow { ArrowKind::Arrow } else { ArrowKind::None };
            edge.end = if e.to.arrow { ArrowKind::Arrow } else { ArrowKind::None };
            g.edges.push(edge);
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use crate::{Diagram, parse};

    fn arch(src: &str) -> super::ArchitectureDiagram {
        match parse(src).unwrap().diagram {
            Diagram::Architecture(a) => a,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reads_the_documented_statements() {
        let src = "architecture-beta\n  group api(cloud)[API]\n  group inner[Inner] in api\n  service db(database)[Database] in api\n  service server(server)[Server] in inner\n  junction j1 in api\n  service web(internet)[The Web]\n  db:L -- R:server\n  server:B --> T:j1\n  web{group}:R <-- L:db{group}\n  j1:L <--> R:web\n";
        let a = arch(src);
        assert_eq!((a.groups.len(), a.services.len(), a.junctions.len(), a.edges.len()), (2, 3, 1, 4));
        assert_eq!(a.groups[1].parent.as_deref(), Some("api"));
        assert_eq!((a.services[0].icon.as_deref(), a.services[0].title.as_deref()), (Some("database"), Some("Database")));
        let e = &a.edges[2];
        assert!(e.from.group && e.from.arrow && !e.to.arrow && e.to.group);
        assert_eq!((e.from.side, e.to.side), (super::Side::Right, super::Side::Left));
        assert!(a.edges[3].from.arrow && a.edges[3].to.arrow);
        let g = parse(src).unwrap().diagram.to_graph();
        assert_eq!(g.groups[0].members, ["db", "j1"]);
    }

    #[test]
    fn unknown_statements_warn_and_bad_input_is_an_error() {
        let p = parse("architecture-beta\n  service a(server)[A] in ghost\n  frobnicate a\n").unwrap();
        assert_eq!(p.warnings.len(), 2, "{:?}", p.warnings);
        for bad in [
            "architecture-beta\n  service a(server[A]\n",
            "architecture-beta\n  service a\n  service b\n  a:Q -- L:b\n",
            "architecture-beta\n  group a in b\n  group b in a\n",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let deep: String = (0..100).map(|i| format!("  group g{i} in g{}\n", i + 1)).collect();
        assert!(parse(&format!("architecture-beta\n{deep}  group g100\n")).is_err());
        let many: String = (0..100_000).map(|i| format!("group g{i}\n")).collect();
        assert!(parse(&format!("architecture-beta\n{many}")).is_err());
    }
}
