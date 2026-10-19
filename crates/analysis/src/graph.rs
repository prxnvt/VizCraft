//! The graph every analysis walks: element ids as indices, relationships as directed edges.
//!
//! Indices keep the walks linear and allocation-light; ids are only looked up at the edges of an
//! analysis (its params and its result). Nodes are in model order, so every output sorted by index
//! is in model order too.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use vizcraft_model::Model;
use vizcraft_model::query::Selector;

/// The relationship type an owner-to-child edge carries when containment is followed.
pub const CONTAINMENT: &str = "containment";

/// One directed edge of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The relationship id; empty for a containment edge, which is no relationship.
    pub id: String,
    /// Index of the node the edge leaves.
    pub from: usize,
    /// Index of the node the edge enters.
    pub to: usize,
    /// The relationship type, or [`CONTAINMENT`].
    pub rel: String,
}

/// One link handed to [`Graph::from_edges`], by node id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// The link's id (a relationship, a connector, a net); empty when it has none.
    pub id: String,
    /// The node it leaves.
    pub from: String,
    /// The node it enters.
    pub to: String,
    /// Its type.
    pub rel: String,
}

/// Which relationships and elements a graph built from a model takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeFilter {
    /// Relationship types followed source to target.
    pub forward: Selector,
    /// Relationship types followed target to source (the inverses of the types asked for). A type
    /// listed here is never also followed forwards.
    pub backward: Vec<String>,
    /// Element types that become nodes; a relationship to an element left out is skipped.
    pub elements: Selector,
    /// Owner-to-child links become edges too.
    pub containment: bool,
}

/// Which way a walk follows edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Source to target.
    #[default]
    Downstream,
    /// Target to source.
    Upstream,
    /// Either way.
    Both,
}

impl Direction {
    /// The direction named `t` (`downstream`, `upstream`, `both`).
    pub fn parse(t: &str) -> Option<Direction> {
        match t {
            "downstream" => Some(Direction::Downstream),
            "upstream" => Some(Direction::Upstream),
            "both" => Some(Direction::Both),
            _ => None,
        }
    }
    /// The word for this direction.
    pub fn name(self) -> &'static str {
        match self {
            Direction::Downstream => "downstream",
            Direction::Upstream => "upstream",
            Direction::Both => "both",
        }
    }
}

/// A directed multigraph over element ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    /// Node ids, in model order.
    pub ids: Vec<String>,
    /// Node index by id.
    pub index: HashMap<String, usize>,
    /// The edges, in model order.
    pub edges: Vec<Edge>,
    /// Per node: `(target node, edge index)` of each edge leaving it.
    pub out: Vec<Vec<(usize, usize)>>,
    /// Per node: `(source node, edge index)` of each edge entering it.
    pub inc: Vec<Vec<(usize, usize)>>,
}

impl Graph {
    /// An empty graph over `nodes`; a repeated id keeps the first.
    fn with_nodes(nodes: impl IntoIterator<Item = String>) -> Graph {
        let mut g = Graph::default();
        for id in nodes {
            if g.index.contains_key(&id) {
                continue;
            }
            g.index.insert(id.clone(), g.ids.len());
            g.ids.push(id);
        }
        g.out = vec![Vec::new(); g.ids.len()];
        g.inc = vec![Vec::new(); g.ids.len()];
        g
    }

    /// Add an edge between two known nodes.
    fn push(&mut self, id: String, from: usize, to: usize, rel: String) {
        let e = self.edges.len();
        if let Some(o) = self.out.get_mut(from) {
            o.push((to, e));
        }
        if let Some(i) = self.inc.get_mut(to) {
            i.push((from, e));
        }
        self.edges.push(Edge { id, from, to, rel });
    }

    /// The graph of `model` under `filter`: the elements it takes as nodes, the relationships it
    /// takes between them as edges (backward types reversed), and owner-to-child edges when
    /// containment is on. Relationships whose ends are missing or left out are skipped.
    pub fn from_model(model: &Model, filter: &EdgeFilter) -> Graph {
        let mut g = Graph::with_nodes(model.elements.iter().filter(|e| filter.elements.matches(&e.type_id)).map(|e| e.id.clone()));
        for r in &model.relationships {
            let backward = filter.backward.contains(&r.type_id);
            if !backward && !filter.forward.matches(&r.type_id) {
                continue;
            }
            let (Some(&s), Some(&t)) = (g.index.get(&r.source), g.index.get(&r.target)) else { continue };
            let (from, to) = if backward { (t, s) } else { (s, t) };
            g.push(r.id.clone(), from, to, r.type_id.clone());
        }
        if filter.containment {
            // Only the first element with an id is a node, so a repeated id must not add the
            // second one's owner link.
            let mut seen = vec![false; g.ids.len()];
            for e in &model.elements {
                let Some(&child) = g.index.get(&e.id) else { continue };
                if std::mem::replace(seen.get_mut(child).unwrap_or(&mut true), true) {
                    continue;
                }
                let Some(&owner) = e.owner.as_ref().and_then(|o| g.index.get(o)) else { continue };
                g.push(String::new(), owner, child, CONTAINMENT.to_string());
            }
        }
        g
    }

    /// The graph of `nodes` and `links` (nets, pins, anything not in the model): links whose ends
    /// are not among the nodes are skipped.
    pub fn from_edges(nodes: impl IntoIterator<Item = String>, links: impl IntoIterator<Item = Link>) -> Graph {
        let mut g = Graph::with_nodes(nodes);
        for l in links {
            let (Some(&from), Some(&to)) = (g.index.get(&l.from), g.index.get(&l.to)) else { continue };
            g.push(l.id, from, to, l.rel);
        }
        g
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True when the graph has no nodes.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The node index of `id`.
    pub fn node(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// The id of node `i` (empty when out of range).
    pub fn id(&self, i: usize) -> &str {
        self.ids.get(i).map_or("", String::as_str)
    }

    /// The neighbours of `n` one step in `dir`, as `(node, edge)`.
    pub fn step(&self, n: usize, dir: Direction) -> impl Iterator<Item = (usize, usize)> + '_ {
        let empty: &[(usize, usize)] = &[];
        let out = if dir != Direction::Upstream { self.out.get(n).map_or(empty, Vec::as_slice) } else { empty };
        let inc = if dir != Direction::Downstream { self.inc.get(n).map_or(empty, Vec::as_slice) } else { empty };
        out.iter().chain(inc.iter()).copied()
    }

    /// The relationship ids of `edges`, skipping containment edges, in the order given.
    pub fn relationship_ids(&self, edges: impl IntoIterator<Item = usize>) -> Vec<String> {
        edges.into_iter().filter_map(|e| self.edges.get(e)).filter(|e| !e.id.is_empty()).map(|e| e.id.clone()).collect()
    }

    /// The ids of `nodes`, in the order given.
    pub fn node_ids(&self, nodes: impl IntoIterator<Item = usize>) -> Vec<String> {
        nodes.into_iter().filter_map(|n| self.ids.get(n)).cloned().collect()
    }
}
