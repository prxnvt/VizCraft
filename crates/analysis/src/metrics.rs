//! Per-element metrics (fan-in, fan-out, layer, containment depth) and graph totals.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use vizcraft_model::{MAX_DEPTH, Model};

use crate::graph::Graph;
use crate::paths::{self, Path};
use crate::scc;

/// The metrics of one element.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricRow {
    /// The element id.
    pub id: String,
    /// Edges entering it.
    pub fan_in: usize,
    /// Edges leaving it.
    pub fan_out: usize,
    /// Its longest-path layer (sources are 0; a cycle counts as one step).
    pub layer: u32,
    /// How many owners it sits inside (0 at the project root).
    pub containment_depth: u32,
}

/// Counts over the whole graph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Nodes.
    pub elements: usize,
    /// Edges that are relationships (containment edges are not counted).
    pub relationships: usize,
    /// Weakly connected components.
    pub components: usize,
    /// Cycles, as [`crate::scc::cycles`] counts them.
    pub cycles: usize,
    /// Edges on the longest path.
    pub longest_path: usize,
}

/// Every element's metrics, the totals and the critical (longest) path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// One row per node, in model order.
    pub rows: Vec<MetricRow>,
    /// Totals.
    pub totals: Totals,
    /// The longest path.
    pub critical: Path,
    /// The graph has cycles, so the critical path runs through the condensation.
    pub cyclic: bool,
}

/// The containment depth of every element of `model`, from one owner index with a memo, so the
/// whole model costs one pass rather than one walk per element. Chains are cut at `MAX_DEPTH` and
/// an owner cycle stops the walk where it repeats.
pub fn containment_depths(model: &Model) -> HashMap<&str, u32> {
    let mut owner: HashMap<&str, Option<&str>> = HashMap::with_capacity(model.elements.len());
    for e in &model.elements {
        owner.entry(e.id.as_str()).or_insert(e.owner.as_deref());
    }
    let mut depth: HashMap<&str, u32> = HashMap::with_capacity(owner.len());
    let mut chain: Vec<&str> = Vec::new();
    for e in &model.elements {
        if depth.contains_key(e.id.as_str()) {
            continue;
        }
        // Walk up until an element whose depth is known, the root, a missing owner or a repeat.
        chain.clear();
        let mut base = 0u32;
        let mut at = Some(e.id.as_str());
        while let Some(id) = at {
            if let Some(&d) = depth.get(id) {
                base = d.saturating_add(1);
                break;
            }
            if chain.contains(&id) || chain.len() > MAX_DEPTH {
                break;
            }
            chain.push(id);
            at = owner.get(id).copied().flatten().filter(|o| owner.contains_key(o));
        }
        // `chain` runs child to top; the top sits at `base`.
        let top = chain.len();
        for (i, id) in chain.iter().enumerate() {
            let d = base.saturating_add(u32::try_from(top - 1 - i).unwrap_or(u32::MAX));
            depth.insert(id, d.min(u32::try_from(MAX_DEPTH).unwrap_or(u32::MAX)));
        }
    }
    depth
}

/// Weakly connected components of `g` (union-find with path halving).
fn components(g: &Graph) -> usize {
    let mut parent: Vec<usize> = (0..g.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while let Some(&up) = p.get(x) {
            if up == x {
                break;
            }
            let grand = p.get(up).copied().unwrap_or(up);
            if let Some(slot) = p.get_mut(x) {
                *slot = grand;
            }
            x = grand;
        }
        x
    }
    let mut count = g.len();
    for e in &g.edges {
        let (a, b) = (find(&mut parent, e.from), find(&mut parent, e.to));
        if a != b {
            if let Some(slot) = parent.get_mut(a) {
                *slot = b;
            }
            count -= 1;
        }
    }
    count
}

/// The metrics of every node of `g`, whose containment depths come from `model` (a node that is
/// not an element of `model` is at depth 0).
pub fn metrics(model: &Model, g: &Graph) -> Metrics {
    let depths = containment_depths(model);
    let layers = paths::layers(g);
    let rows = (0..g.len())
        .map(|n| MetricRow {
            id: g.id(n).to_string(),
            fan_in: g.inc.get(n).map_or(0, Vec::len),
            fan_out: g.out.get(n).map_or(0, Vec::len),
            layer: layers.get(n).copied().unwrap_or(0),
            containment_depth: depths.get(g.id(n)).copied().unwrap_or(0),
        })
        .collect();
    let longest = paths::longest_path(g);
    let totals = Totals {
        elements: g.len(),
        relationships: g.edges.iter().filter(|e| !e.id.is_empty()).count(),
        components: components(g),
        cycles: scc::cycles(g).len(),
        longest_path: longest.path.edges.len(),
    };
    Metrics { rows, totals, critical: longest.path, cyclic: longest.cyclic }
}
