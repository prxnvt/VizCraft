//! Layered (Sugiyama-style) layout, always computed top to bottom; `crate::directed` maps it to
//! the other directions.
//!
//! The phases follow the literature:
//!
//! 1. Cycle breaking with the greedy feedback arc set heuristic of Eades, Lin and Smyth (1993).
//! 2. Layer assignment by longest path, refined by the network simplex method of Gansner,
//!    Koutsofios, North and Vo (1993) on components small enough for it, or by pulling sources
//!    toward their successors on larger ones.
//! 3. Long edges split into chains of dummy nodes (within a budget).
//! 4. Crossing reduction by alternating barycenter sweeps plus adjacent transposition, keeping
//!    the best ordering seen (bilayer crossings counted with a Fenwick tree, after Barth, Juenger
//!    and Mutzel).
//! 5. Horizontal coordinates from the four alignments of Brandes and Koepf (2001), with
//!    size-aware separation, balanced by the average of the two median candidates.
//! 6. Edge polylines through the dummy positions, or orthogonal polylines that bend in the
//!    channels between layers.
//!
//! Indices used for `[i]` access in this module are local node, dummy or edge indices created
//! here or checked by the caller (`crate::Orchestrator::layout_piece`), so they are in range by
//! construction.

use crate::geom::simplify;
use crate::{Notes, Options, Piece, Point, Sub};
use std::collections::{BTreeSet, VecDeque};

/// Components up to this many nodes get network simplex ranking.
const NS_NODE_LIMIT: usize = 2_000;
/// Upper bound on pivots times (nodes + edges) for network simplex.
const NS_WORK_BUDGET: usize = 40_000_000;
/// Most dummy nodes created for long edges; longer edges beyond it are drawn straight.
const DUMMY_BUDGET: usize = 200_000;
/// Upper bound on sweeps times (vertices + edges) for crossing reduction.
const CROSSING_WORK_BUDGET: usize = 6_000_000;
/// Most barycenter sweeps.
const MAX_SWEEPS: usize = 24;

/// An edge after cycle breaking: always points from a lower rank to a higher one.
#[derive(Debug, Clone, Copy)]
struct DagEdge {
    tail: usize,
    head: usize,
    weight: f64,
    min_len: i64,
}

/// Lays out a connected sub graph top to bottom.
pub(crate) fn layout(sub: &Sub, opt: &Options, notes: &mut Notes) -> Piece {
    let n = sub.sizes.len();
    let pairs: Vec<(usize, usize)> = sub.edges.iter().map(|e| (e.s, e.t)).collect();
    let order = greedy_order(n, &pairs);
    let reversed: Vec<bool> = pairs.iter().map(|&(s, t)| order[s] > order[t]).collect();
    let dag: Vec<DagEdge> = sub
        .edges
        .iter()
        .zip(&reversed)
        .map(|(e, &rev)| {
            let (tail, head) = if rev { (e.t, e.s) } else { (e.s, e.t) };
            DagEdge { tail, head, weight: e.weight, min_len: i64::from(e.min_len.max(1)) }
        })
        .collect();

    let mut rank = longest_path(n, &dag);
    if n <= NS_NODE_LIMIT {
        if !network_simplex(n, &dag, &mut rank) {
            notes.push("layered: layer assignment stopped at its iteration cap; layers may be longer than necessary");
        }
    } else {
        pull_toward_successors(n, &dag, &mut rank);
    }
    let min_rank = rank.iter().copied().min().unwrap_or(0);
    for r in rank.iter_mut() {
        *r -= min_rank;
    }

    let proper = Proper::build(&sub.sizes, &dag, &rank, notes);
    let layers = order_layers(&proper, sub, opt);
    let x = coordinates(&proper, &layers, opt);

    // Layer bands: each as tall as its tallest real node, separated by the layer spacing.
    let layer_count = layers.len();
    let mut layer_h = vec![0.0f64; layer_count];
    for v in 0..n {
        let l = proper.layer[v];
        layer_h[l] = layer_h[l].max(proper.height[v]);
    }
    let mut layer_top = vec![0.0f64; layer_count];
    let mut y = 0.0;
    for l in 0..layer_count {
        layer_top[l] = y;
        y += layer_h[l] + opt.layer_spacing;
    }
    let top_of = |v: usize| {
        let l = proper.layer[v];
        layer_top[l] + (layer_h[l] - proper.height[v]) / 2.0
    };
    let pos: Vec<Point> = (0..n).map(|v| Point::new(x[v] - proper.width[v] / 2.0, top_of(v))).collect();

    let geometry = Geometry { proper: &proper, x: &x, layer_top: &layer_top, layer_h: &layer_h };
    let mut routes = geometry.routes(opt);
    for (route, &rev) in routes.iter_mut().zip(&reversed) {
        if rev {
            route.reverse();
        }
    }
    Piece { pos, routes }
}

/// Eades-Lin-Smyth greedy ordering: sinks go to the end, sources to the front, otherwise the node
/// with the largest out-degree minus in-degree goes to the front. Edges pointing backward in the
/// returned order are the ones to reverse.
///
/// Returns each node's position in the order.
fn greedy_order(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, t) in edges {
        if s != t && s < n && t < n {
            out_adj[s].push(t);
            in_adj[t].push(s);
        }
    }
    let mut outdeg: Vec<i64> = out_adj.iter().map(|a| a.len() as i64).collect();
    let mut indeg: Vec<i64> = in_adj.iter().map(|a| a.len() as i64).collect();
    // Keyed by in - out so the first entry has the largest out - in, ties to the lowest index.
    let mut by_delta: BTreeSet<(i64, usize)> = (0..n).map(|v| (indeg[v] - outdeg[v], v)).collect();
    let mut sinks: Vec<usize> = (0..n).rev().filter(|&v| outdeg[v] == 0).collect();
    let mut sources: Vec<usize> = (0..n).rev().filter(|&v| indeg[v] == 0).collect();
    let mut removed = vec![false; n];
    let mut front: Vec<usize> = Vec::with_capacity(n);
    let mut back: Vec<usize> = Vec::new();
    let mut remaining = n;
    while remaining > 0 {
        let v = if let Some(v) = pop_live(&mut sinks, &removed) {
            back.push(v);
            v
        } else if let Some(v) = pop_live(&mut sources, &removed) {
            front.push(v);
            v
        } else if let Some(&(_, v)) = by_delta.first() {
            front.push(v);
            v
        } else {
            break;
        };
        removed[v] = true;
        remaining -= 1;
        by_delta.remove(&(indeg[v] - outdeg[v], v));
        for &w in &out_adj[v] {
            if !removed[w] {
                by_delta.remove(&(indeg[w] - outdeg[w], w));
                indeg[w] -= 1;
                by_delta.insert((indeg[w] - outdeg[w], w));
                if indeg[w] == 0 {
                    sources.push(w);
                }
            }
        }
        for &u in &in_adj[v] {
            if !removed[u] {
                by_delta.remove(&(indeg[u] - outdeg[u], u));
                outdeg[u] -= 1;
                by_delta.insert((indeg[u] - outdeg[u], u));
                if outdeg[u] == 0 {
                    sinks.push(u);
                }
            }
        }
    }
    let mut position = vec![0usize; n];
    for (i, v) in front.into_iter().chain(back.into_iter().rev()).enumerate() {
        position[v] = i;
    }
    position
}

fn pop_live(stack: &mut Vec<usize>, removed: &[bool]) -> Option<usize> {
    while let Some(v) = stack.pop() {
        if !removed[v] {
            return Some(v);
        }
    }
    None
}

/// Kahn's topological order, lowest index first among ready nodes. Nodes left over by a cycle
/// (impossible after cycle breaking, handled anyway) are appended.
fn topo_order(n: usize, dag: &[DagEdge]) -> Vec<usize> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for e in dag {
        out[e.tail].push(e.head);
        indeg[e.head] += 1;
    }
    let mut queue: VecDeque<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    let mut order = Vec::with_capacity(n);
    let mut done = vec![false; n];
    while let Some(v) = queue.pop_front() {
        order.push(v);
        done[v] = true;
        for &w in &out[v] {
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    if order.len() < n {
        order.extend((0..n).filter(|&v| !done[v]));
    }
    order
}

fn longest_path(n: usize, dag: &[DagEdge]) -> Vec<i64> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in dag.iter().enumerate() {
        out[e.tail].push(i);
    }
    let mut rank = vec![0i64; n];
    for v in topo_order(n, dag) {
        for &i in &out[v] {
            let e = dag[i];
            rank[e.head] = rank[e.head].max(rank[v] + e.min_len);
        }
    }
    rank
}

/// Cheap compaction for components too large for network simplex: in reverse topological order,
/// a node whose outgoing weight exceeds its incoming weight moves down next to its successors.
fn pull_toward_successors(n: usize, dag: &[DagEdge], rank: &mut [i64]) {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_w = vec![0.0f64; n];
    let mut out_w = vec![0.0f64; n];
    for (i, e) in dag.iter().enumerate() {
        out[e.tail].push(i);
        out_w[e.tail] += e.weight;
        in_w[e.head] += e.weight;
    }
    for v in topo_order(n, dag).into_iter().rev() {
        if out[v].is_empty() || out_w[v] <= in_w[v] {
            continue;
        }
        let limit = out[v].iter().map(|&i| rank[dag[i].head] - dag[i].min_len).min().unwrap_or(rank[v]);
        if limit > rank[v] {
            rank[v] = limit;
        }
    }
}

fn slack(e: &DagEdge, rank: &[i64]) -> i64 {
    rank[e.head] - rank[e.tail] - e.min_len
}

/// Network simplex ranking (Gansner et al. 1993): minimises the weighted total edge length
/// subject to `rank[head] - rank[tail] >= min_len`, starting from the feasible `rank`.
///
/// Returns false when it stopped at the pivot cap (the ranking is still feasible) or the graph is
/// not connected (ranking left as it was made feasible).
fn network_simplex(n: usize, dag: &[DagEdge], rank: &mut [i64]) -> bool {
    let m = dag.len();
    if n <= 1 || m == 0 {
        return true;
    }
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in dag.iter().enumerate() {
        incident[e.tail].push(i);
        incident[e.head].push(i);
    }

    // Feasible spanning tree of tight edges, grown from node 0.
    let mut in_tree = vec![false; n];
    let mut tree_edge = vec![false; m];
    let mut tree_nodes: Vec<usize> = vec![0];
    in_tree[0] = true;
    let mut stack: Vec<usize> = vec![0];
    grow_tight(dag, &incident, rank, &mut in_tree, &mut tree_edge, &mut tree_nodes, &mut stack);
    while tree_nodes.len() < n {
        let mut best: Option<(i64, usize)> = None;
        for (i, e) in dag.iter().enumerate() {
            if in_tree[e.tail] != in_tree[e.head] {
                let s = slack(e, rank);
                if best.is_none_or(|(bs, _)| s < bs) {
                    best = Some((s, i));
                }
            }
        }
        let Some((s, i)) = best else { return false };
        let e = dag[i];
        let delta = if in_tree[e.tail] { s } else { -s };
        for &v in &tree_nodes {
            rank[v] += delta;
        }
        let w = if in_tree[e.tail] { e.head } else { e.tail };
        in_tree[w] = true;
        tree_edge[i] = true;
        tree_nodes.push(w);
        stack.push(w);
        grow_tight(dag, &incident, rank, &mut in_tree, &mut tree_edge, &mut tree_nodes, &mut stack);
    }

    let mut netflow = vec![0.0f64; n];
    for e in dag {
        netflow[e.tail] += e.weight;
        netflow[e.head] -= e.weight;
    }
    let max_pivots = (4 * n + 100).min(NS_WORK_BUDGET / (n + m)).max(1);
    let mut search_start = 0usize;
    let mut parent_edge = vec![usize::MAX; n];
    let mut lim = vec![0usize; n];
    let mut low = vec![usize::MAX; n];
    for _ in 0..max_pivots {
        let tadj = tree_adjacency(n, dag, &tree_edge);
        // Postorder numbering (lim) and subtree minimum (low) let us test subtree membership in O(1).
        parent_edge.fill(usize::MAX);
        low.fill(usize::MAX);
        let mut visited = vec![false; n];
        let mut post: Vec<usize> = Vec::with_capacity(n);
        let mut dfs: Vec<(usize, usize)> = vec![(0, 0)];
        visited[0] = true;
        while let Some(top) = dfs.last_mut() {
            let (v, idx) = *top;
            if let Some(&i) = tadj[v].get(idx) {
                top.1 += 1;
                let e = dag[i];
                let w = if e.tail == v { e.head } else { e.tail };
                if !visited[w] {
                    visited[w] = true;
                    parent_edge[w] = i;
                    dfs.push((w, 0));
                }
            } else {
                dfs.pop();
                post.push(v);
                lim[v] = post.len();
                low[v] = low[v].min(lim[v]);
                if parent_edge[v] != usize::MAX {
                    let e = dag[parent_edge[v]];
                    let p = if e.tail == v { e.head } else { e.tail };
                    low[p] = low[p].min(low[v]);
                }
            }
        }
        if post.len() < n {
            return false;
        }
        // Cut value of a tree edge = net weight leaving the child's subtree, signed by the edge
        // direction; the subtree sum of per-node net outflow gives it because internal edges cancel.
        let mut sum = netflow.clone();
        for &v in &post {
            if parent_edge[v] != usize::MAX {
                let e = dag[parent_edge[v]];
                let p = if e.tail == v { e.head } else { e.tail };
                sum[p] += sum[v];
            }
        }
        let mut leave: Option<usize> = None;
        for k in 0..n {
            let c = (search_start + k) % n;
            let pe = parent_edge[c];
            if pe == usize::MAX {
                continue;
            }
            let cut = if dag[pe].tail == c { sum[c] } else { -sum[c] };
            if cut < -1e-9 {
                leave = Some(c);
                search_start = (c + 1) % n;
                break;
            }
        }
        let Some(c) = leave else { return true };
        let leaving = parent_edge[c];
        let in_sub = |v: usize| low[c] <= lim[v] && lim[v] <= lim[c];
        let tail_in_sub = dag[leaving].tail == c;
        let mut best: Option<(i64, usize)> = None;
        for (i, f) in dag.iter().enumerate() {
            if tree_edge[i] {
                continue;
            }
            let (a, b) = (in_sub(f.tail), in_sub(f.head));
            // Entering edges run from the head component to the tail component.
            let crosses = if tail_in_sub { !a && b } else { a && !b };
            if crosses {
                let s = slack(f, rank);
                if best.is_none_or(|(bs, _)| s < bs) {
                    best = Some((s, i));
                }
            }
        }
        let Some((_, entering)) = best else { return true };
        tree_edge[leaving] = false;
        tree_edge[entering] = true;
        rerank(n, dag, &tree_edge, rank);
    }
    false
}

fn grow_tight(
    dag: &[DagEdge],
    incident: &[Vec<usize>],
    rank: &[i64],
    in_tree: &mut [bool],
    tree_edge: &mut [bool],
    tree_nodes: &mut Vec<usize>,
    stack: &mut Vec<usize>,
) {
    while let Some(v) = stack.pop() {
        for &i in &incident[v] {
            let e = dag[i];
            let w = if e.tail == v { e.head } else { e.tail };
            if !in_tree[w] && slack(&e, rank) == 0 {
                in_tree[w] = true;
                tree_edge[i] = true;
                tree_nodes.push(w);
                stack.push(w);
            }
        }
    }
}

fn tree_adjacency(n: usize, dag: &[DagEdge], tree_edge: &[bool]) -> Vec<Vec<usize>> {
    let mut tadj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in dag.iter().enumerate() {
        if tree_edge[i] {
            tadj[e.tail].push(i);
            tadj[e.head].push(i);
        }
    }
    tadj
}

/// Recomputes ranks so every tree edge is tight, keeping node 0 fixed.
fn rerank(n: usize, dag: &[DagEdge], tree_edge: &[bool], rank: &mut [i64]) {
    let tadj = tree_adjacency(n, dag, tree_edge);
    let mut seen = vec![false; n];
    seen[0] = true;
    let mut stack = vec![0usize];
    while let Some(v) = stack.pop() {
        for &i in &tadj[v] {
            let e = dag[i];
            let (w, r) = if e.tail == v { (e.head, rank[v] + e.min_len) } else { (e.tail, rank[v] - e.min_len) };
            if !seen[w] {
                seen[w] = true;
                rank[w] = r;
                stack.push(w);
            }
        }
    }
}

/// The proper layered graph: real nodes first (same indices as the sub graph), then dummies.
struct Proper {
    width: Vec<f64>,
    height: Vec<f64>,
    dummy: Vec<bool>,
    layer: Vec<usize>,
    /// Neighbours in the layer above.
    up: Vec<Vec<usize>>,
    /// Neighbours in the layer below.
    down: Vec<Vec<usize>>,
    /// Per edge, the vertices from its top end to its bottom end.
    chains: Vec<Vec<usize>>,
    layer_count: usize,
    real: usize,
}

impl Proper {
    fn build(sizes: &[(f64, f64)], dag: &[DagEdge], rank: &[i64], notes: &mut Notes) -> Proper {
        let n = sizes.len();
        let span = |e: &DagEdge| (rank[e.head] - rank[e.tail]).max(1);
        // Short long edges get dummies first so the budget covers as many edges as possible.
        let mut long: Vec<(i64, usize)> = dag.iter().enumerate().filter(|(_, e)| span(e) > 1).map(|(i, e)| (span(e), i)).collect();
        long.sort_unstable();
        let mut accepted = vec![false; dag.len()];
        let mut used = 0usize;
        for &(s, i) in &long {
            let need = usize::try_from(s - 1).unwrap_or(usize::MAX);
            if used.saturating_add(need) > DUMMY_BUDGET {
                notes.push("layered: too many long edges for dummy nodes; some long edges are drawn straight");
                break;
            }
            used += need;
            accepted[i] = true;
        }
        // Only ranks that hold a vertex become layers, so huge min_len values cannot create
        // millions of empty layers.
        let mut ranks: Vec<i64> = rank.to_vec();
        for (i, e) in dag.iter().enumerate() {
            if accepted[i] {
                ranks.extend(rank[e.tail] + 1..rank[e.head]);
            }
        }
        ranks.sort_unstable();
        ranks.dedup();
        let layer_of = |r: i64| ranks.binary_search(&r).unwrap_or_else(|i| i);

        let mut p = Proper {
            width: sizes.iter().map(|s| s.0).collect(),
            height: sizes.iter().map(|s| s.1).collect(),
            dummy: vec![false; n],
            layer: rank.iter().map(|&r| layer_of(r)).collect(),
            up: vec![Vec::new(); n],
            down: vec![Vec::new(); n],
            chains: Vec::with_capacity(dag.len()),
            layer_count: ranks.len().max(1),
            real: n,
        };
        for (i, e) in dag.iter().enumerate() {
            let mut chain = vec![e.tail];
            let linked = accepted[i] || span(e) == 1;
            if accepted[i] {
                for r in rank[e.tail] + 1..rank[e.head] {
                    let d = p.width.len();
                    p.width.push(0.0);
                    p.height.push(0.0);
                    p.dummy.push(true);
                    p.layer.push(layer_of(r));
                    p.up.push(Vec::new());
                    p.down.push(Vec::new());
                    chain.push(d);
                }
            }
            chain.push(e.head);
            if linked {
                for pair in chain.windows(2) {
                    if let [a, b] = pair {
                        p.down[*a].push(*b);
                        p.up[*b].push(*a);
                    }
                }
            }
            p.chains.push(chain);
        }
        p
    }
}

/// Crossing reduction: an initial breadth-first order (sorted by model order when the nodes have
/// one), then alternating barycenter sweeps with transposition, keeping the best ordering found.
/// Sweeps and transpositions only move a vertex on a strict improvement, so ties keep the
/// initial order.
fn order_layers(p: &Proper, sub: &Sub, opt: &Options) -> Vec<Vec<usize>> {
    let count = p.width.len();
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); p.layer_count];
    let mut visited = vec![false; count];
    let mut starts: Vec<usize> = (0..count).collect();
    starts.sort_by_key(|&v| (p.layer[v], v));
    let mut queue = VecDeque::new();
    for s in starts {
        if visited[s] {
            continue;
        }
        visited[s] = true;
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            layers[p.layer[v]].push(v);
            for &w in &p.down[v] {
                if !visited[w] {
                    visited[w] = true;
                    queue.push_back(w);
                }
            }
        }
    }
    if sub.has_order() {
        // A dummy takes the key of the vertex above it (dummies are numbered after their tail),
        // so a long edge stays beside the node it leaves from.
        let mut key = vec![u64::MAX; count];
        for v in 0..count {
            key[v] = if v < p.real { sub.order_key(v) } else { p.up[v].first().map_or(u64::MAX, |&u| if u < v { key[u] } else { u64::MAX }) };
        }
        for layer in layers.iter_mut() {
            layer.sort_by_key(|&v| key[v]);
        }
    }
    let mut pos = vec![0usize; count];
    set_positions(&layers, &mut pos);

    let work = count + p.down.iter().map(Vec::len).sum::<usize>();
    let passes = MAX_SWEEPS.min(opt.max_iterations as usize).min((CROSSING_WORK_BUDGET / work.max(1)).max(2));
    let rounds = if work > 200_000 { 1 } else { 4 };
    transpose(&mut layers, &mut pos, p, rounds);
    let mut best = layers.clone();
    let mut best_c = count_crossings(&layers, &pos, &p.down);
    let mut stale = 0;
    for pass in 0..passes {
        if best_c == 0 {
            break;
        }
        if pass % 2 == 0 {
            for layer in layers.iter_mut().skip(1) {
                reorder_layer(layer, &mut pos, &p.up);
            }
        } else {
            for layer in layers.iter_mut().rev().skip(1) {
                reorder_layer(layer, &mut pos, &p.down);
            }
        }
        transpose(&mut layers, &mut pos, p, rounds);
        let c = count_crossings(&layers, &pos, &p.down);
        if c < best_c {
            best = layers.clone();
            best_c = c;
            stale = 0;
        } else {
            stale += 1;
            if stale >= 4 {
                break;
            }
        }
    }
    best
}

fn set_positions(layers: &[Vec<usize>], pos: &mut [usize]) {
    for layer in layers {
        for (i, &v) in layer.iter().enumerate() {
            pos[v] = i;
        }
    }
}

/// Sorts a layer by the barycenter of each vertex's neighbours in the fixed layer; vertices
/// without neighbours there keep their slots.
fn reorder_layer(layer: &mut Vec<usize>, pos: &mut [usize], adj: &[Vec<usize>]) {
    let mut movable: Vec<(f64, usize, usize)> = Vec::new();
    let mut fixed = vec![false; layer.len()];
    for (i, &v) in layer.iter().enumerate() {
        let nb = &adj[v];
        if nb.is_empty() {
            fixed[i] = true;
        } else {
            let sum: usize = nb.iter().map(|&u| pos[u]).sum();
            movable.push((sum as f64 / nb.len() as f64, i, v));
        }
    }
    movable.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut it = movable.into_iter();
    let new: Vec<usize> = layer.iter().zip(&fixed).map(|(&v, &is_fixed)| if is_fixed { v } else { it.next().map_or(v, |m| m.2) }).collect();
    *layer = new;
    for (i, &v) in layer.iter().enumerate() {
        pos[v] = i;
    }
}

/// Swaps adjacent vertices whenever that lowers the crossings with both neighbouring layers.
fn transpose(layers: &mut [Vec<usize>], pos: &mut [usize], p: &Proper, rounds: usize) {
    let mut a = Vec::new();
    let mut b = Vec::new();
    for _ in 0..rounds {
        let mut improved = false;
        for layer in layers.iter_mut() {
            for i in 0..layer.len().saturating_sub(1) {
                let (v, w) = (layer[i], layer[i + 1]);
                let (u1, u2) = pair_crossings(v, w, &p.up, pos, &mut a, &mut b);
                let (d1, d2) = pair_crossings(v, w, &p.down, pos, &mut a, &mut b);
                if u2 + d2 < u1 + d1 {
                    layer.swap(i, i + 1);
                    pos[v] = i + 1;
                    pos[w] = i;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
}

/// Crossings between the edges of `v` and `w` into one neighbouring layer: (with v before w, with w before v).
fn pair_crossings(v: usize, w: usize, adj: &[Vec<usize>], pos: &[usize], a: &mut Vec<usize>, b: &mut Vec<usize>) -> (u64, u64) {
    a.clear();
    a.extend(adj[v].iter().map(|&x| pos[x]));
    b.clear();
    b.extend(adj[w].iter().map(|&x| pos[x]));
    b.sort_unstable();
    let (mut vw, mut wv) = (0u64, 0u64);
    for &x in a.iter() {
        let lt = b.partition_point(|&y| y < x);
        let le = b.partition_point(|&y| y <= x);
        vw += lt as u64;
        wv += (b.len() - le) as u64;
    }
    (vw, wv)
}

/// Total crossings of the layering, one Fenwick-tree inversion count per pair of layers.
fn count_crossings(layers: &[Vec<usize>], pos: &[usize], down: &[Vec<usize>]) -> u64 {
    let mut total = 0u64;
    let mut targets: Vec<usize> = Vec::new();
    for li in 0..layers.len().saturating_sub(1) {
        let lower_len = layers[li + 1].len();
        let mut tree = vec![0u64; lower_len + 1];
        let mut inserted = 0u64;
        for &v in &layers[li] {
            targets.clear();
            targets.extend(down[v].iter().map(|&w| pos[w]));
            targets.sort_unstable();
            for &t in &targets {
                // Count earlier edges ending to the right of t.
                let mut i = t + 1;
                let mut not_greater = 0u64;
                while i > 0 {
                    not_greater += tree.get(i).copied().unwrap_or(0);
                    i &= i - 1;
                }
                total = total.saturating_add(inserted - not_greater);
                let mut i = t + 1;
                while i <= lower_len {
                    if let Some(slot) = tree.get_mut(i) {
                        *slot += 1;
                    }
                    i += 1 << i.trailing_zeros();
                }
                inserted += 1;
            }
        }
    }
    total
}

/// Brandes-Koepf horizontal coordinates (centres) for every vertex of the proper graph.
fn coordinates(p: &Proper, layers: &[Vec<usize>], opt: &Options) -> Vec<f64> {
    let count = p.width.len();
    let sep = |a: usize, b: usize| {
        let gap = match (p.dummy[a], p.dummy[b]) {
            (false, false) => opt.node_spacing,
            (true, true) => opt.edge_spacing,
            _ => (opt.node_spacing + opt.edge_spacing) / 2.0,
        };
        (p.width[a] + p.width[b]) / 2.0 + gap
    };
    let mut results: Vec<(Vec<f64>, bool)> = Vec::with_capacity(4);
    for downward in [true, false] {
        for leftward in [true, false] {
            // The other three alignments are the canonical one on a mirrored layering.
            let mut frame: Vec<Vec<usize>> = if downward { layers.to_vec() } else { layers.iter().rev().cloned().collect() };
            if !leftward {
                for l in frame.iter_mut() {
                    l.reverse();
                }
            }
            let preds = if downward { &p.up } else { &p.down };
            let mut xs = align_and_compact(&frame, preds, &p.dummy, &sep, count);
            if !leftward {
                for x in xs.iter_mut() {
                    *x = -*x;
                }
            }
            results.push((xs, leftward));
        }
    }
    let extent = |xs: &[f64]| {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (v, &x) in xs.iter().enumerate() {
            lo = lo.min(x - p.width[v] / 2.0);
            hi = hi.max(x + p.width[v] / 2.0);
        }
        (lo, hi)
    };
    let ext: Vec<(f64, f64)> = results.iter().map(|(xs, _)| extent(xs)).collect();
    let mut smallest = 0;
    for (k, e) in ext.iter().enumerate() {
        if e.1 - e.0 < ext[smallest].1 - ext[smallest].0 {
            smallest = k;
        }
    }
    let (smin, smax) = ext[smallest];
    for (k, (xs, leftward)) in results.iter_mut().enumerate() {
        let shift = if *leftward { smin - ext[k].0 } else { smax - ext[k].1 };
        if shift.is_finite() {
            for x in xs.iter_mut() {
                *x += shift;
            }
        }
    }
    (0..count)
        .map(|v| {
            let mut vals = [0.0f64; 4];
            for (slot, (xs, _)) in vals.iter_mut().zip(&results) {
                *slot = xs[v];
            }
            vals.sort_by(f64::total_cmp);
            (vals[1] + vals[2]) / 2.0
        })
        .collect()
}

/// One Brandes-Koepf alignment (top-down, left-biased) on `frame`, where `preds` gives each
/// vertex's neighbours in the previous layer of the frame, followed by block compaction.
fn align_and_compact(frame: &[Vec<usize>], preds: &[Vec<usize>], dummy: &[bool], sep: &dyn Fn(usize, usize) -> f64, count: usize) -> Vec<f64> {
    let mut pos = vec![0usize; count];
    set_positions(frame, &mut pos);

    // Type 1 conflicts: non-inner segments crossing an inner (dummy to dummy) segment are
    // excluded from alignment so long edges stay straight.
    let mut marked: BTreeSet<(usize, usize)> = BTreeSet::new();
    for i in 0..frame.len().saturating_sub(1) {
        let (upper, lower) = (&frame[i], &frame[i + 1]);
        if upper.is_empty() {
            continue;
        }
        let mut k0 = 0usize;
        let mut l = 0usize;
        for (l1, &v) in lower.iter().enumerate() {
            let inner = if dummy[v] { preds[v].iter().copied().find(|&u| dummy[u]) } else { None };
            if l1 + 1 == lower.len() || inner.is_some() {
                let k1 = inner.map_or(upper.len() - 1, |u| pos[u]);
                while l <= l1 {
                    let w = lower[l];
                    for &u in &preds[w] {
                        let k = pos[u];
                        if (k < k0 || k > k1) && !(dummy[u] && dummy[w]) {
                            marked.insert((u, w));
                        }
                    }
                    l += 1;
                }
                k0 = k1;
            }
        }
    }

    // Vertical alignment with the median predecessors.
    let mut root: Vec<usize> = (0..count).collect();
    let mut align: Vec<usize> = (0..count).collect();
    let mut ups: Vec<usize> = Vec::new();
    for layer in frame.iter().skip(1) {
        let mut r: i64 = -1;
        for &v in layer {
            ups.clear();
            ups.extend(preds[v].iter().copied());
            ups.sort_by_key(|&u| pos[u]);
            let d = ups.len();
            if d == 0 {
                continue;
            }
            let medians = [(d - 1) / 2, d / 2];
            let distinct = if medians[0] == medians[1] { 1 } else { 2 };
            for &mi in &medians[..distinct] {
                if align[v] != v {
                    continue;
                }
                let u = ups[mi];
                let pu = pos[u] as i64;
                if !marked.contains(&(u, v)) && r < pu {
                    align[u] = v;
                    root[v] = root[u];
                    align[v] = root[v];
                    r = pu;
                }
            }
        }
    }

    // Compaction: blocks form a DAG ordered by the left-to-right constraints; place each block
    // as far left as its predecessors allow, then pull blocks right toward their successors.
    let mut succ: Vec<Vec<(usize, f64)>> = vec![Vec::new(); count];
    let mut indeg = vec![0usize; count];
    for layer in frame {
        for pair in layer.windows(2) {
            if let [a, b] = pair {
                let (ra, rb) = (root[*a], root[*b]);
                succ[ra].push((rb, sep(*a, *b)));
                indeg[rb] += 1;
            }
        }
    }
    let block_count = (0..count).filter(|&v| root[v] == v).count();
    let mut queue: VecDeque<usize> = (0..count).filter(|&v| root[v] == v && indeg[v] == 0).collect();
    let mut topo = Vec::with_capacity(block_count);
    while let Some(b) = queue.pop_front() {
        topo.push(b);
        for &(c, _) in &succ[b] {
            indeg[c] -= 1;
            if indeg[c] == 0 {
                queue.push_back(c);
            }
        }
    }
    let mut x = vec![0.0f64; count];
    if topo.len() != block_count {
        // Cannot happen for a valid alignment; pack each layer left to right instead.
        for layer in frame {
            let mut cur = 0.0;
            for (i, &v) in layer.iter().enumerate() {
                if i > 0 {
                    cur += sep(layer[i - 1], v);
                }
                x[v] = cur;
            }
        }
        return x;
    }
    let mut xb = vec![0.0f64; count];
    for &b in &topo {
        for &(c, d) in &succ[b] {
            if xb[b] + d > xb[c] {
                xb[c] = xb[b] + d;
            }
        }
    }
    for &b in topo.iter().rev() {
        if succ[b].is_empty() {
            continue;
        }
        let limit = succ[b].iter().map(|&(c, d)| xb[c] - d).fold(f64::INFINITY, f64::min);
        if limit > xb[b] {
            xb[b] = limit;
        }
    }
    for v in 0..count {
        x[v] = xb[root[v]];
    }
    x
}

/// Placed proper graph, used to build the edge polylines.
struct Geometry<'a> {
    proper: &'a Proper,
    x: &'a [f64],
    layer_top: &'a [f64],
    layer_h: &'a [f64],
}

impl Geometry<'_> {
    fn node_top(&self, v: usize) -> f64 {
        let l = self.proper.layer[v];
        self.layer_top[l] + (self.layer_h[l] - self.proper.height[v]) / 2.0
    }

    fn center_y(&self, v: usize) -> f64 {
        let l = self.proper.layer[v];
        self.layer_top[l] + self.layer_h[l] / 2.0
    }

    /// One polyline per edge, top to bottom.
    fn routes(&self, opt: &Options) -> Vec<Vec<Point>> {
        let p = self.proper;
        let x = self.x;
        let m = p.chains.len();
        // Ports: edges leaving a node's bottom (entering its top) are spread along that side in
        // the order of where they go next, so parallel edges do not overlap.
        let mut bottom_port = vec![0.0f64; m];
        let mut top_port = vec![0.0f64; m];
        let mut outs: Vec<Vec<(f64, usize)>> = vec![Vec::new(); p.real];
        let mut ins: Vec<Vec<(f64, usize)>> = vec![Vec::new(); p.real];
        for (i, chain) in p.chains.iter().enumerate() {
            let (Some(&a), Some(&b)) = (chain.first(), chain.last()) else { continue };
            let next = chain.get(1).copied().unwrap_or(b);
            let prev = chain.len().checked_sub(2).and_then(|k| chain.get(k)).copied().unwrap_or(a);
            outs[a].push((x[next], i));
            ins[b].push((x[prev], i));
        }
        let spread = |lists: &mut Vec<Vec<(f64, usize)>>, port: &mut Vec<f64>| {
            for (v, list) in lists.iter_mut().enumerate() {
                list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let k = list.len();
                let step = if k > 1 { opt.edge_spacing.min(p.width[v] / (k as f64 + 1.0)) } else { 0.0 };
                for (j, &(_, i)) in list.iter().enumerate() {
                    port[i] = x[v] + (j as f64 - (k as f64 - 1.0) / 2.0) * step;
                }
            }
        };
        spread(&mut outs, &mut bottom_port);
        spread(&mut ins, &mut top_port);

        if !opt.orthogonal_edges {
            return p
                .chains
                .iter()
                .enumerate()
                .map(|(i, chain)| {
                    let (Some(&a), Some(&b)) = (chain.first(), chain.last()) else { return Vec::new() };
                    let mut pts = vec![Point::new(bottom_port[i], self.node_top(a) + p.height[a])];
                    for &d in chain.iter().skip(1).take(chain.len().saturating_sub(2)) {
                        pts.push(Point::new(x[d], self.center_y(d)));
                    }
                    pts.push(Point::new(top_port[i], self.node_top(b)));
                    pts
                })
                .collect();
        }

        // Orthogonal: each chain step that changes x gets a horizontal run on its own track in
        // the channel below the step's upper layer.
        let seg_x = |i: usize, chain: &[usize], j: usize| {
            let from = if j == 0 { bottom_port[i] } else { x[chain[j]] };
            let to = if j + 2 == chain.len() { top_port[i] } else { x[chain[j + 1]] };
            (from, to)
        };
        let mut requests: Vec<Vec<(f64, f64, usize, usize)>> = vec![Vec::new(); p.layer_count];
        for (i, chain) in p.chains.iter().enumerate() {
            for j in 0..chain.len().saturating_sub(1) {
                let (from, to) = seg_x(i, chain, j);
                if (from - to).abs() > 1e-9 {
                    requests[p.layer[chain[j]]].push((from.min(to), from.max(to), i, j));
                }
            }
        }
        let mut track: Vec<Vec<Option<f64>>> = p.chains.iter().map(|c| vec![None; c.len().saturating_sub(1)]).collect();
        for (g, list) in requests.iter_mut().enumerate() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
            let gap_top = self.layer_top[g] + self.layer_h[g];
            let c = list.len() as f64;
            for (t, &(_, _, i, j)) in list.iter().enumerate() {
                track[i][j] = Some(gap_top + opt.layer_spacing * (t as f64 + 1.0) / (c + 1.0));
            }
        }
        p.chains
            .iter()
            .enumerate()
            .map(|(i, chain)| {
                let (Some(&a), Some(&b)) = (chain.first(), chain.last()) else { return Vec::new() };
                let mut pts = vec![Point::new(bottom_port[i], self.node_top(a) + p.height[a])];
                for (j, slot) in track[i].iter().enumerate() {
                    if let Some(y) = *slot {
                        let (from, to) = seg_x(i, chain, j);
                        pts.push(Point::new(from, y));
                        pts.push(Point::new(to, y));
                    }
                }
                pts.push(Point::new(top_port[i], self.node_top(b)));
                simplify(pts)
            })
            .collect()
    }
}
