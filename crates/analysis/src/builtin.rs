//! The built-in analyses: dependencies, cycles, unreachable, single points of failure, metrics
//! and path. Each reads its params, builds the graph through [`filter`], runs one algorithm and
//! turns indices back into ids.

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_model::MAX_KEY;
use vizcraft_model::query::{Selector, label_of};

use crate::graph::{Direction, EdgeFilter, Graph};
use crate::{Analysis, AnalysisError, AnalysisResult, Context, MAX_DEPTH_PARAM, MAX_RESULT_IDS, MAX_TYPES, Mark, cut, metrics, paths, reach, scc};

/// `t` cut to 64 characters for an error message.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

fn params_err(t: impl Into<String>) -> AnalysisError {
    AnalysisError::Params(t.into())
}

/// The list of type ids in param `k`: absent or null is `None`; at most [`MAX_TYPES`] strings of
/// 1 to `MAX_KEY` bytes.
fn type_list(v: &Value, k: &str) -> Result<Option<Vec<String>>, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            if a.len() > MAX_TYPES {
                return Err(params_err(format!("`{k}` names more than {MAX_TYPES} types")));
            }
            a.iter()
                .map(|x| match x.as_str() {
                    Some(t) if !t.is_empty() && t.len() <= MAX_KEY => Ok(t.to_string()),
                    _ => Err(params_err(format!("`{k}` must be type ids of 1 to {MAX_KEY} bytes"))),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        }
        Some(_) => Err(params_err(format!("`{k}` must be an array of type ids"))),
    }
}

/// `named` and every type in `in_use` that is a subtype of one of them, each once.
fn expand(cx: &Context, named: &[String], in_use: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in named {
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    for u in in_use {
        if !out.iter().any(|x| x == u) && named.iter().any(|n| cx.types.is_a(u, n)) {
            out.push((*u).to_string());
        }
    }
    out
}

fn distinct<'a>(it: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    it.filter(|t| seen.insert(*t)).collect()
}

/// The edge filter the params ask for: `types` (relationship types, subtypes included, inverses
/// followed backwards), `elements` (element types, subtypes included) and `containment`.
pub fn filter(cx: &Context, v: &Value) -> Result<EdgeFilter, AnalysisError> {
    let mut f = EdgeFilter::default();
    if let Some(named) = type_list(v, "types")? {
        let rels = distinct(cx.model.relationships.iter().map(|r| r.type_id.as_str()));
        let forward = expand(cx, &named, &rels);
        let inverses: Vec<String> = named.iter().flat_map(|n| cx.types.inverses(n)).take(MAX_TYPES).collect();
        f.backward = expand(cx, &inverses, &rels).into_iter().filter(|t| !forward.contains(t)).collect();
        f.forward = Selector::Types(forward);
    }
    if let Some(named) = type_list(v, "elements")? {
        let types = distinct(cx.model.elements.iter().map(|e| e.type_id.as_str()));
        f.elements = Selector::Types(expand(cx, &named, &types));
    }
    f.containment = match v.get("containment") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(params_err("`containment` must be true or false")),
    };
    Ok(f)
}

/// The graph of the model under the params' filter.
fn graph(cx: &Context, v: &Value) -> Result<Graph, AnalysisError> {
    Ok(Graph::from_model(cx.model, &filter(cx, v)?))
}

/// The node `id` names in `g`.
fn node(g: &Graph, id: &str) -> Result<usize, AnalysisError> {
    g.node(id).ok_or_else(|| AnalysisError::Unknown(short(id)))
}

/// The node of the required string param `k`.
fn req_node(g: &Graph, v: &Value, k: &str) -> Result<usize, AnalysisError> {
    match v.get(k) {
        Some(Value::String(t)) => node(g, t),
        None | Some(Value::Null) => Err(params_err(format!("`{k}` (element id) is required"))),
        Some(_) => Err(params_err(format!("`{k}` must be an element id"))),
    }
}

/// The nodes of the optional param `k`: one id or an array of at most [`MAX_RESULT_IDS`] ids.
fn opt_nodes(g: &Graph, v: &Value, k: &str) -> Result<Vec<usize>, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(t)) => Ok(vec![node(g, t)?]),
        Some(Value::Array(a)) => {
            if a.len() > MAX_RESULT_IDS {
                return Err(params_err(format!("`{k}` has more than {MAX_RESULT_IDS} ids")));
            }
            a.iter().map(|x| x.as_str().ok_or_else(|| params_err(format!("`{k}` must be element ids"))).and_then(|t| node(g, t))).collect()
        }
        Some(_) => Err(params_err(format!("`{k}` must be an element id or an array of them"))),
    }
}

fn direction(v: &Value) -> Result<Direction, AnalysisError> {
    match v.get("direction") {
        None | Some(Value::Null) => Ok(Direction::Downstream),
        Some(Value::String(t)) => {
            Direction::parse(t).ok_or_else(|| params_err(format!("`direction` must be downstream, upstream or both, not `{}`", short(t))))
        }
        Some(_) => Err(params_err("`direction` must be downstream, upstream or both")),
    }
}

/// The optional whole-number param `k` in `1..=max`.
fn count(v: &Value, k: &str, max: u64) -> Result<Option<u64>, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => match x.as_u64() {
            Some(n) if (1..=max).contains(&n) => Ok(Some(n)),
            _ => Err(params_err(format!("`{k}` must be a whole number from 1 to {max}"))),
        },
    }
}

/// What a result calls an element: its name, else its id.
fn name(cx: &Context, id: &str) -> String {
    label_of(cx.model, id)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `dependencies`: everything reachable from one element.
pub struct Dependencies;

impl Analysis for Dependencies {
    fn id(&self) -> &'static str {
        "dependencies"
    }
    fn label(&self) -> &'static str {
        "Dependencies"
    }
    fn params(&self) -> &'static str {
        r#"{"id": element, "direction"?: "downstream"|"upstream"|"both", "depth"?: 1..1024, "types"?: [relationship type], "elements"?: [element type], "containment"?: bool} -> {"elements", "relationships", "levels": [[ids]]}; downstream follows source to target; levels[0] is the element itself"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let start = req_node(&g, v, "id")?;
        let dir = direction(v)?;
        let max = count(v, "depth", u64::from(MAX_DEPTH_PARAM))?.and_then(|d| u32::try_from(d).ok());
        // Both directions are two walks, so every reached element can say which side it is on.
        let walks: Vec<Direction> = match dir {
            Direction::Both => vec![Direction::Downstream, Direction::Upstream],
            d => vec![d],
        };
        let mut depth: Vec<Option<u32>> = vec![None; g.len()];
        let mut side: Vec<Option<Mark>> = vec![None; g.len()];
        let mut edges: Vec<usize> = Vec::new();
        for w in walks {
            let d = reach::depths(&g, &[start], w, max);
            edges.extend(reach::walked_edges(&g, &d, w, max));
            let mark = if w == Direction::Upstream { Mark::Upstream } else { Mark::Downstream };
            for (i, x) in d.iter().enumerate() {
                let Some(x) = x else { continue };
                if let (Some(slot), Some(s)) = (depth.get_mut(i), side.get_mut(i)) {
                    if slot.is_none_or(|old| *x < old) {
                        *slot = Some(*x);
                    }
                    s.get_or_insert(mark);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        let mut r = AnalysisResult::new(self.id());
        let start_id = g.id(start).to_string();
        r.marks.insert(start_id.clone(), Mark::Focus);
        let reached: Vec<usize> = (0..g.len()).filter(|&i| depth.get(i).copied().flatten().is_some()).collect();
        for &i in &reached {
            if let Some(Some(m)) = side.get(i)
                && i != start
            {
                r.marks.insert(g.id(i).to_string(), *m);
            }
        }
        let deepest = reached.iter().filter_map(|&i| depth.get(i).copied().flatten()).max().unwrap_or(0);
        let mut levels: Vec<Vec<String>> = vec![Vec::new(); usize::try_from(deepest).unwrap_or(0).saturating_add(1)];
        let mut listed = 0usize;
        for &i in &reached {
            let Some(d) = depth.get(i).copied().flatten() else { continue };
            if listed < MAX_RESULT_IDS
                && let Some(l) = levels.get_mut(usize::try_from(d).unwrap_or(usize::MAX))
            {
                l.push(g.id(i).to_string());
                listed += 1;
            }
        }
        let others = reached.len().saturating_sub(1);
        r.set_elements(g.node_ids(reached), None);
        r.set_relationships(g.relationship_ids(edges), Some(Mark::Path));
        // The marks map must not outgrow the lists it marks.
        if r.truncated {
            let keep: HashSet<String> = r.elements.iter().chain(&r.relationships).cloned().collect();
            r.marks.retain(|k, _| keep.contains(k));
        }
        let within = max.map(|m| format!(" within {m} steps")).unwrap_or_default();
        r.summary = format!("{} {} of {}{within}", plural(others, "element", "elements"), dir.name(), name(cx, &start_id));
        r.detail = json!({"levels": levels, "direction": dir.name()});
        Ok(r)
    }
}

/// `cycles`: every cycle of the graph.
pub struct Cycles;

impl Analysis for Cycles {
    fn id(&self) -> &'static str {
        "cycles"
    }
    fn label(&self) -> &'static str {
        "Cycles"
    }
    fn params(&self) -> &'static str {
        r#"{"types"?: [relationship type], "elements"?: [element type], "containment"?: bool} -> {"groups": [{"label", "elements", "relationships"}], "elements", "relationships"}; a group is a set of elements that all reach each other, or one element with a link to itself"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let groups = scc::cycles(&g);
        let mut r = AnalysisResult::new(self.id());
        let mut els: Vec<String> = Vec::new();
        let mut rels: Vec<String> = Vec::new();
        for gr in &groups {
            els.extend(gr.elements.iter().cloned());
            rels.extend(gr.relationships.iter().cloned());
        }
        // Model order across groups.
        let order = |ids: &mut Vec<String>, rank: &dyn Fn(&str) -> usize| ids.sort_by_key(|x| rank(x));
        order(&mut els, &|x| g.node(x).unwrap_or(usize::MAX));
        let rel_rank: std::collections::HashMap<&str, usize> = g.edges.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
        order(&mut rels, &|x| rel_rank.get(x).copied().unwrap_or(usize::MAX));
        let n = groups.len();
        for gr in groups {
            r.push_group(gr);
        }
        r.set_elements(els, Some(Mark::Cycle));
        r.set_relationships(rels, Some(Mark::Cycle));
        r.summary = if n == 0 {
            "No cycles".into()
        } else {
            format!("{} through {}", plural(n, "cycle", "cycles"), plural(r.elements.len(), "element", "elements"))
        };
        Ok(r)
    }
}

/// `unreachable`: the elements no root reaches.
pub struct Unreachable;

impl Analysis for Unreachable {
    fn id(&self) -> &'static str {
        "unreachable"
    }
    fn label(&self) -> &'static str {
        "Unreachable Elements"
    }
    fn params(&self) -> &'static str {
        r#"{"from"?: [element], "types"?: [relationship type], "elements"?: [element type], "containment"?: bool} -> {"elements", "roots"}; elements nothing in from reaches downstream; with no from, the roots are the elements with no incoming link, so a cycle nothing feeds is unreachable"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let given = opt_nodes(&g, v, "from")?;
        let roots = reach::roots_of(&g, &given);
        let lost = reach::unreachable(&g, &roots);
        let mut r = AnalysisResult::new(self.id());
        let n = lost.len();
        r.set_elements(g.node_ids(lost), Some(Mark::Unreachable));
        let roots: Vec<String> = g.node_ids(roots.into_iter().take(MAX_RESULT_IDS));
        r.summary = if n == 0 {
            "Every element is reachable".into()
        } else {
            format!("{} not reachable from {}", plural(n, "element", "elements"), plural(roots.len(), "root", "roots"))
        };
        r.detail = json!({"roots": roots});
        Ok(r)
    }
}

/// `spof`: single points of failure, the cut points and bridges.
pub struct Spof;

impl Analysis for Spof {
    fn id(&self) -> &'static str {
        "spof"
    }
    fn label(&self) -> &'static str {
        "Single Points of Failure"
    }
    fn params(&self) -> &'static str {
        r#"{"types"?: [relationship type], "elements"?: [element type], "containment"?: bool} -> {"elements": cut points, "relationships": bridges}; link directions are ignored; a cut point's removal, or a bridge's, splits the elements it connects"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let c = cut::cuts(&g);
        let mut r = AnalysisResult::new(self.id());
        let (np, nb) = (c.points.len(), c.bridges.len());
        r.set_elements(g.node_ids(c.points), Some(Mark::Cut));
        r.set_relationships(g.relationship_ids(c.bridges), Some(Mark::Cut));
        r.summary = if np + nb == 0 {
            "No single points of failure".into()
        } else {
            format!("{} and {}", plural(np, "cut point", "cut points"), plural(nb, "bridge", "bridges"))
        };
        Ok(r)
    }
}

/// `metrics`: fan-in, fan-out, layer and containment depth per element, with totals and the
/// critical path.
pub struct Metrics;

impl Analysis for Metrics {
    fn id(&self) -> &'static str {
        "metrics"
    }
    fn label(&self) -> &'static str {
        "Metrics"
    }
    fn params(&self) -> &'static str {
        r#"{"types"?: [relationship type], "elements"?: [element type], "containment"?: bool, "sort"?: "fanIn"|"fanOut"|"layer"|"depth", "limit"?: 1..10000} -> {"rows": [{"id", "fanIn", "fanOut", "layer", "containmentDepth"}], "totals": {"elements", "relationships", "components", "cycles", "longestPath"}, "critical": {"elements", "relationships"}, "cyclic"}; sort is descending, model order breaks ties"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let m = metrics::metrics(cx.model, &g);
        let mut rows = m.rows;
        match v.get("sort") {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) => match s.as_str() {
                "fanIn" => rows.sort_by_key(|r| std::cmp::Reverse(r.fan_in)),
                "fanOut" => rows.sort_by_key(|r| std::cmp::Reverse(r.fan_out)),
                "layer" => rows.sort_by_key(|r| std::cmp::Reverse(r.layer)),
                "depth" => rows.sort_by_key(|r| std::cmp::Reverse(r.containment_depth)),
                other => return Err(params_err(format!("`sort` must be fanIn, fanOut, layer or depth, not `{}`", short(other)))),
            },
            Some(_) => return Err(params_err("`sort` must be fanIn, fanOut, layer or depth")),
        }
        let limit = count(v, "limit", MAX_RESULT_IDS as u64)?.and_then(|n| usize::try_from(n).ok()).unwrap_or(MAX_RESULT_IDS);
        let mut r = AnalysisResult::new(self.id());
        if rows.len() > limit {
            r.truncated = limit == MAX_RESULT_IDS;
            rows.truncate(limit);
        }
        let critical_els = g.node_ids(m.critical.nodes.iter().copied());
        let critical_rels = g.relationship_ids(m.critical.edges.iter().copied());
        r.table = Some(crate::ResultTable {
            columns: vec!["fanIn".into(), "fanOut".into(), "layer".into(), "containmentDepth".into()],
            rows: rows
                .iter()
                .map(|x| crate::TableRow {
                    id: x.id.clone(),
                    cells: vec![json!(x.fan_in), json!(x.fan_out), json!(x.layer), json!(x.containment_depth)],
                })
                .collect(),
        });
        r.set_elements(critical_els.clone(), Some(Mark::Path));
        r.set_relationships(critical_rels.clone(), Some(Mark::Path));
        let t = &m.totals;
        r.summary = format!(
            "{}, {}, {}, {}; longest path {}",
            plural(t.elements, "element", "elements"),
            plural(t.relationships, "relationship", "relationships"),
            plural(t.components, "component", "components"),
            plural(t.cycles, "cycle", "cycles"),
            plural(t.longest_path, "step", "steps")
        );
        r.detail = json!({
            "rows": rows,
            "totals": t,
            "critical": {"elements": critical_els, "relationships": critical_rels},
            "cyclic": m.cyclic,
        });
        Ok(r)
    }
}

/// `path`: the shortest path between two elements.
pub struct PathAnalysis;

impl Analysis for PathAnalysis {
    fn id(&self) -> &'static str {
        "path"
    }
    fn label(&self) -> &'static str {
        "Path"
    }
    fn params(&self) -> &'static str {
        r#"{"from": element, "to": element, "direction"?: "downstream"|"upstream"|"both", "types"?: [relationship type], "elements"?: [element type], "containment"?: bool} -> {"elements", "relationships", "length"}; the path with the fewest links; an error when there is none"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let g = graph(cx, v)?;
        let from = req_node(&g, v, "from")?;
        let to = req_node(&g, v, "to")?;
        let dir = direction(v)?;
        let (a, b) = (g.id(from).to_string(), g.id(to).to_string());
        let p = paths::shortest_path(&g, from, to, dir)
            .ok_or_else(|| params_err(format!("no {} path from `{}` to `{}`", dir.name(), short(&a), short(&b))))?;
        let mut r = AnalysisResult::new(self.id());
        let len = p.edges.len();
        r.set_elements(g.node_ids(p.nodes), Some(Mark::Path));
        r.set_relationships(g.relationship_ids(p.edges), Some(Mark::Path));
        r.marks.insert(a.clone(), Mark::Focus);
        r.summary = format!("{} from {} to {}", plural(len, "step", "steps"), name(cx, &a), name(cx, &b));
        r.detail = json!({"length": len});
        Ok(r)
    }
}
