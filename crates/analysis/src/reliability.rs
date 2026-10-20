//! Reliability analyses over the `reliability` pack's model: the minimal cut sets of a fault
//! tree ([`cut_sets`]), the availability of a reliability block diagram ([`availability`]) and
//! the outcome frequencies of an event tree ([`outcomes`]).
//!
//! Every walk is iterative with its own stack, and every list is capped; a cap that is hit sets
//! `truncated` (or a note) instead of failing. The exact availability solve is the only bounded
//! recursion: at most [`MAX_PIVOT`] levels deep.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_model::{Element, Model};

use crate::mech::{Scope, flag, fmt, in_scope, name_of, number, scope_param, text};
use crate::{Analysis, AnalysisError, AnalysisResult, Context, Group, Mark, ResultTable, TableRow, Types};

/// Element and relationship types of the `reliability` pack.
pub mod types {
    /// The fault tree's top event.
    pub const TOP: &str = "reliability.TopEvent";
    /// An event below the top, defined by the gate under it.
    pub const INTERMEDIATE: &str = "reliability.IntermediateEvent";
    /// A basic event (a leaf with a probability).
    pub const BASIC: &str = "reliability.BasicEvent";
    /// A house event: on or off.
    pub const HOUSE: &str = "reliability.HouseEvent";
    /// An event not developed further.
    pub const UNDEVELOPED: &str = "reliability.UndevelopedEvent";
    /// The condition of an inhibit gate.
    pub const CONDITIONING: &str = "reliability.ConditioningEvent";
    /// AND gate.
    pub const AND: &str = "reliability.And";
    /// OR gate.
    pub const OR: &str = "reliability.Or";
    /// Exclusive OR gate.
    pub const XOR: &str = "reliability.Xor";
    /// Priority AND gate.
    pub const PAND: &str = "reliability.PriorityAnd";
    /// Inhibit gate.
    pub const INHIBIT: &str = "reliability.Inhibit";
    /// k-out-of-n voting gate.
    pub const VOTING_GATE: &str = "reliability.VotingGate";
    /// Transfer in: the tree continues where the transfer out with the same tag is.
    pub const TRANSFER_IN: &str = "reliability.TransferIn";
    /// Transfer out.
    pub const TRANSFER_OUT: &str = "reliability.TransferOut";
    /// A fault tree input (child to parent).
    pub const INPUT: &str = "reliability.Input";
    /// RBD start node.
    pub const START: &str = "reliability.Start";
    /// RBD end node.
    pub const END: &str = "reliability.End";
    /// RBD block.
    pub const BLOCK: &str = "reliability.Block";
    /// RBD k-out-of-n node.
    pub const VOTING: &str = "reliability.Voting";
    /// RBD standby node.
    pub const STANDBY: &str = "reliability.Standby";
    /// RBD path.
    pub const PATH: &str = "reliability.Path";
    /// Event tree initiating event.
    pub const INITIATOR: &str = "reliability.Initiator";
    /// Event tree safety function (a column header).
    pub const SAFETY_FUNCTION: &str = "reliability.SafetyFunction";
    /// Event tree branch point.
    pub const BRANCH_POINT: &str = "reliability.BranchPoint";
    /// Event tree outcome.
    pub const OUTCOME: &str = "reliability.Outcome";
    /// Event tree success branch.
    pub const SUCCESS: &str = "reliability.Success";
    /// Event tree failure branch.
    pub const FAILURE: &str = "reliability.Failure";
    /// An FMEA failure mode row.
    pub const FAILURE_MODE: &str = "reliability.FailureMode";
}

use types::*;

/// Default largest cut set order kept.
pub const DEFAULT_MAX_ORDER: usize = 8;

/// Default and largest number of cut sets kept.
pub const DEFAULT_LIMIT: usize = 10_000;

/// Most blocks the exact availability solve pivots on.
pub const MAX_PIVOT: usize = 20;

/// Most event tree paths walked.
const MAX_TREE_PATHS: usize = 100_000;

fn params_err(t: impl Into<String>) -> AnalysisError {
    AnalysisError::Params(t.into())
}

fn is(ty: &dyn Types, e: &Element, want: &str) -> bool {
    ty.is_a(&e.type_id, want)
}

fn opt_id<'v>(v: &'v Value, k: &str) -> Result<Option<&'v str>, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if !t.is_empty() && t.len() <= vizcraft_model::MAX_KEY => Ok(Some(t)),
        Some(_) => Err(params_err(format!("`{k}` must be an element id"))),
    }
}

fn opt_count(v: &Value, k: &str, default: usize, max: usize) -> Result<usize, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(default),
        Some(x) => match x.as_u64() {
            Some(n) if n >= 1 && n <= max as u64 => Ok(usize::try_from(n).unwrap_or(default)),
            _ => Err(params_err(format!("`{k}` must be a whole number from 1 to {max}"))),
        },
    }
}

// ---------------------------------------------------------------- fault tree

/// A family of cut sets: each set a sorted list of event indices. `[[]]` is always true, `[]`
/// never.
type Family = Vec<Vec<u32>>;

/// Drop repeats and every set that holds another set.
fn minimize(mut f: Family) -> Family {
    for s in &mut f {
        s.sort_unstable();
        s.dedup();
    }
    f.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    f.dedup();
    let mut out: Family = Vec::with_capacity(f.len());
    for s in f {
        if !out.iter().any(|k| is_subset(k, &s)) {
            out.push(s);
        }
    }
    out
}

/// Whether sorted `a` is a subset of sorted `b`.
fn is_subset(a: &[u32], b: &[u32]) -> bool {
    let mut it = b.iter();
    a.iter().all(|x| it.any(|y| y == x))
}

/// Limits on a cut set computation.
struct Caps {
    max_order: usize,
    limit: usize,
    truncated: bool,
}

fn or(fams: &[&Family], caps: &mut Caps) -> Family {
    let mut out: Family = Vec::new();
    for f in fams {
        out.extend(f.iter().cloned());
        if out.len() > caps.limit.saturating_mul(4) {
            caps.truncated = true;
            break;
        }
    }
    let mut m = minimize(out);
    if m.len() > caps.limit {
        caps.truncated = true;
        m.truncate(caps.limit);
    }
    m
}

fn and(fams: &[&Family], caps: &mut Caps) -> Family {
    let mut acc: Family = vec![Vec::new()];
    for f in fams {
        let mut next: Family = Vec::new();
        'outer: for a in &acc {
            for b in f.iter() {
                let mut u: Vec<u32> = a.iter().chain(b.iter()).copied().collect();
                u.sort_unstable();
                u.dedup();
                if u.len() > caps.max_order {
                    caps.truncated = true;
                    continue;
                }
                next.push(u);
                if next.len() > caps.limit.saturating_mul(4) {
                    caps.truncated = true;
                    break 'outer;
                }
            }
        }
        acc = minimize(next);
        if acc.len() > caps.limit {
            caps.truncated = true;
            acc.truncate(caps.limit);
        }
    }
    acc
}

/// At least `k` of `fams` at once: the OR over every choice of `k` of them, ANDed.
fn voting(fams: &[&Family], k: usize, caps: &mut Caps, notes: &mut Vec<String>) -> Family {
    let n = fams.len();
    if k == 0 {
        return vec![Vec::new()];
    }
    if k > n {
        return Vec::new();
    }
    let mut out: Family = Vec::new();
    let mut idx: Vec<usize> = (0..k).collect();
    let mut combos = 0usize;
    loop {
        combos += 1;
        if combos > caps.limit {
            caps.truncated = true;
            notes.push(format!("a {k}-out-of-{n} gate has more combinations than the limit; the rest are left out"));
            break;
        }
        let chosen: Vec<&Family> = idx.iter().filter_map(|i| fams.get(*i).copied()).collect();
        out.extend(and(&chosen, caps));
        // Next combination in lexicographic order.
        let mut i = k;
        let mut moved = false;
        while i > 0 {
            i -= 1;
            if idx.get(i).copied().unwrap_or(n) < n - k + i {
                if let Some(x) = idx.get_mut(i) {
                    *x += 1;
                }
                for j in i + 1..k {
                    let prev = idx.get(j - 1).copied().unwrap_or(0);
                    if let Some(x) = idx.get_mut(j) {
                        *x = prev + 1;
                    }
                }
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
    or(&[&out], caps)
}

/// One minimal cut set.
#[derive(Clone, Debug, PartialEq)]
pub struct CutSet {
    /// The events, in model order.
    pub events: Vec<String>,
    /// Their joint probability (independent events).
    pub probability: f64,
}

/// What [`cut_sets`] found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutSets {
    /// The top event analysed.
    pub top: String,
    /// The minimal cut sets, smallest first.
    pub sets: Vec<CutSet>,
    /// Top probability by the rare-event approximation (the sum of the sets).
    pub rare_event: f64,
    /// Top probability by the min-cut upper bound, 1 - prod(1 - P(set)).
    pub upper_bound: f64,
    /// Fussell-Vesely importance of each event in a set, in model order.
    pub importance: Vec<(String, f64)>,
    /// Approximations and gaps, in plain words.
    pub notes: Vec<String>,
    /// A cap was hit.
    pub truncated: bool,
}

/// The minimal cut sets of the fault tree under `top` (the first top event when not given),
/// bottom up in topological order. XOR and priority AND are taken as OR and AND; a house event
/// that is on drops out of its sets and one that is off prunes its branch; transfers are followed
/// by tag. Sets above `max_order` events are dropped and at most `limit` sets are kept.
pub fn cut_sets(model: &Model, ty: &dyn Types, scope: &Scope, top: Option<&str>, max_order: usize, limit: usize) -> Result<CutSets, AnalysisError> {
    let els: Vec<&Element> = model.elements.iter().filter(|e| in_scope(scope, &e.id)).collect();
    let index: HashMap<&str, usize> = els.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut inputs: Vec<Vec<usize>> = vec![Vec::new(); els.len()];
    for r in model.relationships.iter().filter(|r| ty.is_a(&r.type_id, INPUT)) {
        if let (Some(&s), Some(&t)) = (index.get(r.source.as_str()), index.get(r.target.as_str()))
            && let Some(list) = inputs.get_mut(t)
        {
            list.push(s);
        }
    }
    let tag = |e: &Element| if e.tag.trim().is_empty() { e.name.trim().to_string() } else { e.tag.trim().to_string() };
    let outs: HashMap<String, usize> =
        els.iter().enumerate().filter(|(_, e)| is(ty, e, TRANSFER_OUT)).map(|(i, e)| (tag(e), i)).filter(|(t, _)| !t.is_empty()).collect();
    let start = match top {
        Some(id) => *index.get(id).ok_or_else(|| AnalysisError::Unknown(id.chars().take(64).collect()))?,
        None => els.iter().position(|e| is(ty, e, TOP)).ok_or_else(|| params_err("the fault tree has no top event; give `top`"))?,
    };
    // The children a node's value is made of: its inputs, or for a transfer in, its partner.
    let children = |i: usize| -> Vec<usize> {
        match els.get(i) {
            Some(e) if is(ty, e, TRANSFER_IN) => outs.get(&tag(e)).map(|o| vec![*o]).unwrap_or_default(),
            _ => inputs.get(i).cloned().unwrap_or_default(),
        }
    };
    let mut caps = Caps { max_order, limit, truncated: false };
    let mut notes: Vec<String> = Vec::new();
    let mut events: Vec<usize> = Vec::new();
    let mut event_of: HashMap<usize, u32> = HashMap::new();
    let mut value: HashMap<usize, Family> = HashMap::new();
    // Iterative post-order: 0 = not seen, 1 = on the stack, 2 = done.
    let mut state = vec![0u8; els.len()];
    let mut stack: Vec<(usize, bool)> = vec![(start, false)];
    let mut cycle_noted = false;
    while let Some((i, expanded)) = stack.pop() {
        if !expanded {
            if state.get(i).copied() != Some(0) {
                continue;
            }
            if let Some(s) = state.get_mut(i) {
                *s = 1;
            }
            stack.push((i, true));
            for c in children(i) {
                match state.get(c).copied() {
                    Some(0) => stack.push((c, false)),
                    Some(1) if !cycle_noted => {
                        cycle_noted = true;
                        notes.push("the tree has a cycle; the input that closes it is left out".into());
                    }
                    _ => {}
                }
            }
            continue;
        }
        let Some(e) = els.get(i) else { continue };
        let kids: Vec<usize> = children(i).into_iter().filter(|c| state.get(*c) == Some(&2)).collect();
        let fams: Vec<&Family> = kids.iter().filter_map(|c| value.get(c)).collect();
        let leaf = |events: &mut Vec<usize>, event_of: &mut HashMap<usize, u32>| -> Family {
            let n = *event_of.entry(i).or_insert_with(|| {
                events.push(i);
                u32::try_from(events.len() - 1).unwrap_or(u32::MAX)
            });
            vec![vec![n]]
        };
        let f = if is(ty, e, HOUSE) {
            if flag(&e.properties, "state").unwrap_or(false) { vec![Vec::new()] } else { Vec::new() }
        } else if is(ty, e, BASIC) || is(ty, e, UNDEVELOPED) || is(ty, e, CONDITIONING) {
            leaf(&mut events, &mut event_of)
        } else if is(ty, e, AND) || is(ty, e, PAND) || is(ty, e, INHIBIT) {
            if is(ty, e, PAND) && !notes.iter().any(|n| n.starts_with("priority AND")) {
                notes.push("priority AND gates are taken as AND (the order of failures is not counted)".into());
            }
            and(&fams, &mut caps)
        } else if is(ty, e, OR) || is(ty, e, XOR) {
            if is(ty, e, XOR) && !notes.iter().any(|n| n.starts_with("exclusive OR")) {
                notes.push("exclusive OR gates are taken as OR".into());
            }
            or(&fams, &mut caps)
        } else if is(ty, e, VOTING_GATE) {
            let k = number(&e.properties, "k").map_or(2, |k| k.clamp(0.0, 1e6) as usize);
            voting(&fams, k, &mut caps, &mut notes)
        } else if fams.is_empty() && !is(ty, e, TRANSFER_IN) {
            // An event with nothing under it is a leaf of its own.
            leaf(&mut events, &mut event_of)
        } else {
            if is(ty, e, TRANSFER_IN) && kids.is_empty() {
                notes.push(format!("transfer {} has no matching transfer out", tag(e)));
            }
            or(&fams, &mut caps)
        };
        value.insert(i, f);
        if let Some(s) = state.get_mut(i) {
            *s = 2;
        }
    }
    let family = value.remove(&start).unwrap_or_default();
    let p_of = |n: u32| -> f64 {
        let Some(e) = events.get(n as usize).and_then(|i| els.get(*i)) else { return 0.0 };
        number(&e.properties, "probability").map_or(0.0, |p| p.clamp(0.0, 1.0))
    };
    for i in &events {
        if let Some(e) = els.get(*i)
            && number(&e.properties, "probability").is_none()
        {
            notes.push(format!("{} has no probability; 0 is used", name_of(e)));
        }
    }
    let mut keyed: Vec<(Vec<usize>, CutSet)> = Vec::with_capacity(family.len());
    let mut per_event: BTreeMap<u32, f64> = BTreeMap::new();
    for s in &family {
        let p: f64 = s.iter().map(|n| p_of(*n)).product();
        for n in s {
            *per_event.entry(*n).or_default() += p;
        }
        let mut ids: Vec<(usize, String)> =
            s.iter().filter_map(|n| events.get(*n as usize).and_then(|i| els.get(*i).map(|e| (*i, e.id.clone())))).collect();
        ids.sort();
        let key: Vec<usize> = ids.iter().map(|(i, _)| *i).collect();
        keyed.push((key, CutSet { events: ids.into_iter().map(|(_, id)| id).collect(), probability: if s.is_empty() { 1.0 } else { p } }));
    }
    // Smallest sets first, then in model order, whatever order the walk numbered the events in.
    keyed.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    let sets: Vec<CutSet> = keyed.into_iter().map(|(_, c)| c).collect();
    let rare: f64 = sets.iter().map(|c| c.probability).sum();
    let upper = 1.0 - sets.iter().map(|c| 1.0 - c.probability).product::<f64>();
    let mut importance: Vec<(usize, String, f64)> = per_event
        .into_iter()
        .filter_map(|(n, p)| events.get(n as usize).and_then(|i| els.get(*i).map(|e| (*i, e.id.clone(), if rare > 0.0 { p / rare } else { 0.0 }))))
        .collect();
    importance.sort_by_key(|(i, _, _)| *i);
    if family.iter().any(Vec::is_empty) {
        notes.push("the top event occurs whatever happens (a house event that is on, or an empty AND)".into());
    }
    Ok(CutSets {
        top: els.get(start).map(|e| e.id.clone()).unwrap_or_default(),
        sets,
        rare_event: rare.min(1.0),
        upper_bound: upper.clamp(0.0, 1.0),
        importance: importance.into_iter().map(|(_, id, f)| (id, f)).collect(),
        notes,
        truncated: caps.truncated,
    })
}

/// `cutsets`: the minimal cut sets of a fault tree.
pub struct CutSetsAnalysis;

impl Analysis for CutSetsAnalysis {
    fn id(&self) -> &'static str {
        "cutsets"
    }
    fn label(&self) -> &'static str {
        "Cut Sets"
    }
    fn params(&self) -> &'static str {
        r#"{"top"?: element id (default: the top event), "maxOrder"?: 1..64 (default 8), "limit"?: 1..10000 (default 10000), "scope"?: [element id]} -> {"sets": [{"events", "order", "probability"}], "rareEvent", "upperBound", "importance": [{"id", "fussellVesely"}], "notes"}; XOR and priority AND are taken as OR and AND"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let scope = scope_param(v)?;
        let max_order = opt_count(v, "maxOrder", DEFAULT_MAX_ORDER, 64)?;
        let limit = opt_count(v, "limit", DEFAULT_LIMIT, DEFAULT_LIMIT)?;
        let c = cut_sets(cx.model, cx.types, &scope, opt_id(v, "top")?, max_order, limit)?;
        let names: HashMap<&str, &Element> = cx.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let label = |id: &str| names.get(id).map_or_else(|| id.to_string(), |e| name_of(e));
        let mut r = AnalysisResult::new(self.id());
        let mut els: Vec<String> = c.importance.iter().map(|(id, _)| id.clone()).collect();
        els.insert(0, c.top.clone());
        r.set_elements(els, Some(Mark::Cut));
        r.marks.insert(c.top.clone(), Mark::Focus);
        for (i, s) in c.sets.iter().enumerate() {
            r.push_group(Group { label: format!("Cut set {}", i + 1), elements: s.events.clone(), relationships: Vec::new() });
        }
        r.table = Some(ResultTable {
            columns: vec!["Cut set".into(), "Order".into(), "Probability".into()],
            rows: c
                .sets
                .iter()
                .map(|s| TableRow {
                    id: s.events.first().cloned().unwrap_or_default(),
                    cells: vec![json!(s.events.iter().map(|e| label(e)).collect::<Vec<_>>().join(", ")), json!(s.events.len()), json!(s.probability)],
                })
                .collect(),
        });
        r.truncated |= c.truncated;
        let singles = c.sets.iter().filter(|s| s.events.len() == 1).count();
        r.summary = format!(
            "{} minimal cut set(s) of {} ({} single); top probability {} (rare event), {} (upper bound)",
            c.sets.len(),
            label(&c.top),
            singles,
            sci(c.rare_event),
            sci(c.upper_bound)
        );
        r.detail = json!({
            "top": c.top,
            "sets": c.sets.iter().map(|s| json!({"events": s.events, "order": s.events.len(), "probability": s.probability})).collect::<Vec<_>>(),
            "rareEvent": c.rare_event,
            "upperBound": c.upper_bound,
            "importance": c.importance.iter().map(|(id, f)| json!({"id": id, "fussellVesely": f})).collect::<Vec<_>>(),
            "notes": c.notes,
        });
        Ok(r)
    }
}

/// A probability for a summary: four significant places, in exponent form when small.
fn sci(p: f64) -> String {
    if p == 0.0 || !p.is_finite() {
        "0".into()
    } else if p.abs() < 1e-3 {
        format!("{p:.3e}")
    } else {
        fmt((p * 10_000.0).round() / 10_000.0)
    }
}

// ---------------------------------------------------------------- reliability block diagram

/// The steady-state availability of an RBD block: `availability`, else MTBF / (MTBF + MTTR),
/// else repair rate / (failure rate + repair rate).
pub fn block_availability(e: &Element) -> Option<f64> {
    let p = &e.properties;
    if let Some(a) = number(p, "availability").filter(|a| (0.0..=1.0).contains(a)) {
        return Some(a);
    }
    if let (Some(f), Some(r)) = (number(p, "mtbf"), number(p, "mttr"))
        && f > 0.0
        && r >= 0.0
    {
        return Some(f / (f + r));
    }
    if let (Some(l), Some(m)) = (number(p, "failureRate"), number(p, "repairRate"))
        && l >= 0.0
        && m > 0.0
    {
        return Some(m / (l + m));
    }
    None
}

/// The reliability of an RBD block at `t`: e^(-λt) from its failure rate (or 1 / MTBF).
pub fn block_reliability(e: &Element, t: f64) -> Option<f64> {
    let p = &e.properties;
    let lambda = number(p, "failureRate").filter(|l| *l >= 0.0).or_else(|| number(p, "mtbf").filter(|m| *m > 0.0).map(|m| 1.0 / m))?;
    Some((-lambda * t).exp())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Node {
    Start,
    End,
    Block,
    /// At least k of the predecessors.
    Voting(usize),
    /// Any predecessor (standby with perfect switching, or a junction).
    Any,
}

/// The RBD as a graph: kinds, probabilities and both adjacency lists, with dead nodes.
#[derive(Clone)]
struct Net {
    kind: Vec<Node>,
    p: Vec<f64>,
    preds: Vec<Vec<usize>>,
    succs: Vec<Vec<usize>>,
    alive: Vec<bool>,
}

impl Net {
    fn is_block(&self, i: usize) -> bool {
        self.alive.get(i).copied().unwrap_or(false) && self.kind.get(i) == Some(&Node::Block)
    }
    fn preds(&self, i: usize) -> &[usize] {
        self.preds.get(i).map_or(&[], Vec::as_slice)
    }
    fn succs(&self, i: usize) -> &[usize] {
        self.succs.get(i).map_or(&[], Vec::as_slice)
    }
    fn replace(list: &mut Vec<usize>, old: usize, new: usize) {
        for x in list.iter_mut() {
            if *x == old {
                *x = new;
            }
        }
        list.sort_unstable();
        list.dedup();
    }
    fn remove(list: &mut Vec<usize>, x: usize) {
        list.retain(|y| *y != x);
    }

    /// Series, parallel and k-out-of-n reductions until none applies.
    fn reduce(&mut self) {
        let n = self.kind.len();
        let mut changed = true;
        // Each round kills at least one node, so there are at most n rounds.
        let mut rounds = 0;
        while changed && rounds <= n {
            changed = false;
            rounds += 1;
            for b in 0..n {
                // Series: b's one successor is a block whose one predecessor is b.
                while self.is_block(b) {
                    let (s, ps) = match self.succs(b) {
                        [c] if self.is_block(*c) && self.preds(*c) == [b] && *c != b => (*c, self.succs(*c).to_vec()),
                        _ => break,
                    };
                    let pc = self.p.get(s).copied().unwrap_or(1.0);
                    if let Some(pb) = self.p.get_mut(b) {
                        *pb *= pc;
                    }
                    for x in &ps {
                        if let Some(l) = self.preds.get_mut(*x) {
                            Net::replace(l, s, b);
                        }
                    }
                    if let Some(l) = self.succs.get_mut(b) {
                        *l = ps;
                    }
                    if let Some(a) = self.alive.get_mut(s) {
                        *a = false;
                    }
                    changed = true;
                }
            }
            for x in 0..n {
                if !self.alive.get(x).copied().unwrap_or(false) {
                    continue;
                }
                // Parallel: blocks between x and one successor y that is not a voting node.
                let mut by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                for &b in self.succs(x) {
                    if self.is_block(b)
                        && self.preds(b) == [x]
                        && let [y] = self.succs(b)
                        && !matches!(self.kind.get(*y), Some(Node::Voting(_)))
                    {
                        by_target.entry(*y).or_default().push(b);
                    }
                }
                for (y, group) in by_target {
                    let Some((&keep, rest)) = group.split_first() else { continue };
                    if rest.is_empty() {
                        continue;
                    }
                    let q: f64 = group.iter().map(|b| 1.0 - self.p.get(*b).copied().unwrap_or(1.0)).product();
                    if let Some(pk) = self.p.get_mut(keep) {
                        *pk = 1.0 - q;
                    }
                    for &b in rest {
                        if let Some(a) = self.alive.get_mut(b) {
                            *a = false;
                        }
                        if let Some(l) = self.succs.get_mut(x) {
                            Net::remove(l, b);
                        }
                        if let Some(l) = self.preds.get_mut(y) {
                            Net::remove(l, b);
                        }
                    }
                    changed = true;
                }
            }
            for v in 0..n {
                // k out of n: every input of a voting node is a block between one node x and it.
                let Some(Node::Voting(k)) = self.kind.get(v).copied() else { continue };
                if !self.alive.get(v).copied().unwrap_or(false) {
                    continue;
                }
                let ins = self.preds(v).to_vec();
                let Some(&first) = ins.first() else { continue };
                let x = match self.preds(first) {
                    [x] => *x,
                    _ => continue,
                };
                if !ins.iter().all(|b| self.is_block(*b) && self.preds(*b) == [x] && self.succs(*b) == [v]) {
                    continue;
                }
                let ps: Vec<f64> = ins.iter().map(|b| self.p.get(*b).copied().unwrap_or(1.0)).collect();
                if let Some(kd) = self.kind.get_mut(v) {
                    *kd = Node::Block;
                }
                if let Some(pv) = self.p.get_mut(v) {
                    *pv = at_least(k, &ps);
                }
                for &b in &ins {
                    if let Some(a) = self.alive.get_mut(b) {
                        *a = false;
                    }
                    if let Some(l) = self.succs.get_mut(x) {
                        Net::remove(l, b);
                    }
                }
                if let Some(l) = self.succs.get_mut(x) {
                    l.push(v);
                    l.sort_unstable();
                    l.dedup();
                }
                if let Some(l) = self.preds.get_mut(v) {
                    *l = vec![x];
                }
                changed = true;
            }
        }
    }

    /// Nodes in topological order, or `None` when there is a cycle.
    fn order(&self) -> Option<Vec<usize>> {
        let n = self.kind.len();
        let mut indeg: Vec<usize> = (0..n).map(|i| self.preds(i).iter().filter(|p| self.alive.get(**p) == Some(&true)).count()).collect();
        let mut queue: Vec<usize> = (0..n).filter(|i| self.alive.get(*i) == Some(&true) && indeg.get(*i) == Some(&0)).collect();
        queue.reverse();
        let mut out = Vec::new();
        while let Some(i) = queue.pop() {
            out.push(i);
            for &s in self.succs(i) {
                if self.alive.get(s) != Some(&true) {
                    continue;
                }
                if let Some(d) = indeg.get_mut(s) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push(s);
                    }
                }
            }
        }
        (out.len() == self.alive.iter().filter(|a| **a).count()).then_some(out)
    }

    /// Whether `end` is reached from `start` with blocks up as `up` says.
    fn works(&self, order: &[usize], start: usize, end: usize, up: &dyn Fn(usize) -> bool) -> bool {
        let mut reached = vec![false; self.kind.len()];
        for &i in order {
            let preds_up = self.preds(i).iter().filter(|p| reached.get(**p) == Some(&true)).count();
            let r = match self.kind.get(i) {
                Some(Node::Start) => i == start,
                Some(Node::Block) => preds_up > 0 && up(i),
                Some(Node::Voting(k)) => preds_up >= (*k).max(1),
                Some(Node::Any | Node::End) => preds_up > 0,
                None => false,
            };
            if let Some(x) = reached.get_mut(i) {
                *x = r;
            }
        }
        reached.get(end).copied().unwrap_or(false)
    }
}

/// The probability that at least `k` of independent events with probabilities `ps` occur.
pub fn at_least(k: usize, ps: &[f64]) -> f64 {
    // dist[j]: probability that exactly j of the events so far occur.
    let mut dist = vec![0.0; ps.len() + 1];
    if let Some(d) = dist.first_mut() {
        *d = 1.0;
    }
    for (i, p) in ps.iter().enumerate() {
        for j in (0..=i + 1).rev() {
            let stay = dist.get(j).copied().unwrap_or(0.0) * (1.0 - p);
            let come = if j > 0 { dist.get(j - 1).copied().unwrap_or(0.0) * p } else { 0.0 };
            if let Some(d) = dist.get_mut(j) {
                *d = stay + come;
            }
        }
    }
    dist.iter().skip(k).sum()
}

/// Pivot on the undecided blocks: P(works) = p P(works | up) + (1 - p) P(works | down). The
/// depth is at most the number of blocks, which the caller keeps to [`MAX_PIVOT`].
fn pivot(net: &Net, order: &[usize], start: usize, end: usize, blocks: &[usize], states: &mut Vec<Option<bool>>, depth: usize) -> f64 {
    let state_of = |states: &Vec<Option<bool>>, i: usize, unknown: bool| -> bool {
        blocks.iter().position(|b| *b == i).and_then(|k| states.get(k).copied().flatten()).unwrap_or(unknown)
    };
    if !net.works(order, start, end, &|i| state_of(states, i, true)) {
        return 0.0;
    }
    if net.works(order, start, end, &|i| state_of(states, i, false)) {
        return 1.0;
    }
    let Some(k) = states.iter().position(Option::is_none) else { return 0.0 };
    if depth > MAX_PIVOT {
        return 0.0;
    }
    let p = blocks.get(k).and_then(|b| net.p.get(*b)).copied().unwrap_or(1.0);
    if let Some(s) = states.get_mut(k) {
        *s = Some(true);
    }
    let a = pivot(net, order, start, end, blocks, states, depth + 1);
    if let Some(s) = states.get_mut(k) {
        *s = Some(false);
    }
    let b = pivot(net, order, start, end, blocks, states, depth + 1);
    if let Some(s) = states.get_mut(k) {
        *s = None;
    }
    p * a + (1.0 - p) * b
}

/// What [`availability`] found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Availability {
    /// The start and end nodes.
    pub start: String,
    /// The end.
    pub end: String,
    /// The system availability.
    pub availability: f64,
    /// The system reliability at the time asked for, when every block has a failure rate.
    pub reliability: Option<f64>,
    /// Per block: id, availability, reliability at the time.
    pub blocks: Vec<(String, f64, Option<f64>)>,
    /// Approximations and gaps.
    pub notes: Vec<String>,
}

/// The availability of the RBD between `start` and `end` (the first of each when not given):
/// series and parallel groups and k-out-of-n groups are reduced first, then what is left is
/// solved exactly by pivoting on up to [`MAX_PIVOT`] blocks. A standby node counts as parallel
/// with perfect switching. With `time`, also the reliability at that time from failure rates.
pub fn availability(
    model: &Model,
    ty: &dyn Types,
    scope: &Scope,
    start: Option<&str>,
    end: Option<&str>,
    time: Option<f64>,
) -> Result<Availability, AnalysisError> {
    let els: Vec<&Element> =
        model.elements.iter().filter(|e| in_scope(scope, &e.id) && [START, END, BLOCK, VOTING, STANDBY].iter().any(|t| is(ty, e, t))).collect();
    let index: HashMap<&str, usize> = els.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut notes = Vec::new();
    let kind: Vec<Node> = els
        .iter()
        .map(|e| {
            if is(ty, e, START) {
                Node::Start
            } else if is(ty, e, END) {
                Node::End
            } else if is(ty, e, BLOCK) {
                Node::Block
            } else if is(ty, e, VOTING) {
                Node::Voting(number(&e.properties, "k").map_or(1, |k| k.clamp(1.0, 1e6) as usize))
            } else {
                Node::Any
            }
        })
        .collect();
    let mut net =
        Net { kind, p: vec![1.0; els.len()], preds: vec![Vec::new(); els.len()], succs: vec![Vec::new(); els.len()], alive: vec![true; els.len()] };
    for r in model.relationships.iter().filter(|r| ty.is_a(&r.type_id, PATH)) {
        if let (Some(&a), Some(&b)) = (index.get(r.source.as_str()), index.get(r.target.as_str()))
            && a != b
        {
            if let Some(l) = net.succs.get_mut(a) {
                l.push(b);
            }
            if let Some(l) = net.preds.get_mut(b) {
                l.push(a);
            }
        }
    }
    for l in net.succs.iter_mut().chain(net.preds.iter_mut()) {
        l.sort_unstable();
        l.dedup();
    }
    let pick = |want: Option<&str>, kind: &str, k: &str| -> Result<usize, AnalysisError> {
        match want {
            Some(id) => index.get(id).copied().ok_or_else(|| AnalysisError::Unknown(id.chars().take(64).collect())),
            None => els.iter().position(|e| is(ty, e, kind)).ok_or_else(|| params_err(format!("`{k}`: the diagram has no {} node", k))),
        }
    };
    let (s, t) = (pick(start, START, "start")?, pick(end, END, "end")?);
    if net.order().is_none() {
        return Err(params_err("the block diagram has a loop; paths must run from start to end"));
    }
    let mut blocks = Vec::new();
    let mut avail = vec![1.0; els.len()];
    let mut rel: Vec<Option<f64>> = vec![Some(1.0); els.len()];
    for (i, e) in els.iter().enumerate() {
        if net.kind.get(i) != Some(&Node::Block) {
            continue;
        }
        let a = block_availability(e);
        if a.is_none() {
            notes.push(format!("{} has no availability data; 1 is used", name_of(e)));
        }
        let r = time.and_then(|t| block_reliability(e, t));
        if let Some(x) = avail.get_mut(i) {
            *x = a.unwrap_or(1.0);
        }
        if let Some(x) = rel.get_mut(i) {
            *x = r;
        }
        blocks.push((e.id.clone(), a.unwrap_or(1.0), r));
    }
    if els.iter().any(|e| is(ty, e, STANDBY)) {
        notes.push("standby nodes are taken as parallel with perfect switching".into());
    }
    let solve = |probs: &[f64]| -> Result<f64, AnalysisError> {
        let mut n = net.clone();
        n.p = probs.to_vec();
        n.reduce();
        let order = n.order().ok_or_else(|| params_err("the block diagram has a loop"))?;
        let left: Vec<usize> = (0..n.kind.len()).filter(|i| n.is_block(*i)).collect();
        if left.len() > MAX_PIVOT {
            return Err(params_err(format!(
                "after series, parallel and k-out-of-n reduction {} blocks are left; at most {MAX_PIVOT} are solved exactly",
                left.len()
            )));
        }
        let mut states = vec![None; left.len()];
        Ok(pivot(&n, &order, s, t, &left, &mut states, 0))
    };
    let a = solve(&avail)?;
    let reliability = match time {
        Some(_) if rel.iter().zip(&net.kind).all(|(r, k)| *k != Node::Block || r.is_some()) => {
            Some(solve(&rel.iter().map(|r| r.unwrap_or(1.0)).collect::<Vec<_>>())?)
        }
        Some(_) => {
            notes.push("some blocks have no failure rate, so no reliability is given".into());
            None
        }
        None => None,
    };
    Ok(Availability {
        start: els.get(s).map(|e| e.id.clone()).unwrap_or_default(),
        end: els.get(t).map(|e| e.id.clone()).unwrap_or_default(),
        availability: a,
        reliability,
        blocks,
        notes,
    })
}

/// `availability`: the availability of a reliability block diagram.
pub struct AvailabilityAnalysis;

impl Analysis for AvailabilityAnalysis {
    fn id(&self) -> &'static str {
        "availability"
    }
    fn label(&self) -> &'static str {
        "Availability"
    }
    fn params(&self) -> &'static str {
        r#"{"start"?: element id, "end"?: element id, "time"?: number (hours), "scope"?: [element id]} -> {"availability", "reliability"?, "blocks": [{"id", "availability", "reliability"?}], "notes"}; series, parallel and k-out-of-n first, then an exact solve on at most 20 blocks"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let scope = scope_param(v)?;
        let time = match v.get("time") {
            None | Some(Value::Null) => None,
            Some(x) => {
                Some(x.as_f64().filter(|t| t.is_finite() && *t >= 0.0 && *t <= 1e12).ok_or_else(|| params_err("`time` must be a number of hours"))?)
            }
        };
        let a = availability(cx.model, cx.types, &scope, opt_id(v, "start")?, opt_id(v, "end")?, time)?;
        let names: HashMap<&str, &Element> = cx.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let label = |id: &str| names.get(id).map_or_else(|| id.to_string(), |e| name_of(e));
        let mut r = AnalysisResult::new(self.id());
        r.set_elements(a.blocks.iter().map(|(id, _, _)| id.clone()).collect(), Some(Mark::Path));
        r.table = Some(ResultTable {
            columns: vec!["Block".into(), "Availability".into(), "Reliability".into()],
            rows: a.blocks.iter().map(|(id, av, re)| TableRow { id: id.clone(), cells: vec![json!(label(id)), json!(av), json!(re)] }).collect(),
        });
        r.summary = format!("System availability {} over {} block(s)", fmt((a.availability * 1e6).round() / 1e6), a.blocks.len());
        if let (Some(rt), Some(t)) = (a.reliability, time) {
            r.summary.push_str(&format!("; reliability at {} h is {}", fmt(t), fmt((rt * 1e6).round() / 1e6)));
        }
        r.detail = json!({
            "availability": a.availability,
            "reliability": a.reliability,
            "start": a.start,
            "end": a.end,
            "blocks": a.blocks.iter().map(|(id, av, re)| json!({"id": id, "availability": av, "reliability": re})).collect::<Vec<_>>(),
            "notes": a.notes,
        });
        Ok(r)
    }
}

// ---------------------------------------------------------------- event tree

/// The failure probability a branch point uses: its own `failureProbability`, else that of the
/// safety function its `function` names.
fn branch_q(e: &Element, functions: &HashMap<String, f64>) -> Option<f64> {
    number(&e.properties, "failureProbability").or_else(|| functions.get(text(&e.properties, "function")).copied()).map(|q| q.clamp(0.0, 1.0))
}

/// What [`outcomes`] found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcomes {
    /// The initiating event.
    pub initiator: String,
    /// Its frequency.
    pub frequency: f64,
    /// Each outcome reached, with its frequency (summed over the paths to it), in model order.
    pub outcomes: Vec<(String, f64)>,
    /// Notes.
    pub notes: Vec<String>,
    /// The path cap was hit.
    pub truncated: bool,
}

/// The frequency of every outcome of the event tree from `initiator` (the first one when not
/// given): along each path, the initiator's frequency times the success (1 - q) or failure (q)
/// probability of each branch.
pub fn outcomes(model: &Model, ty: &dyn Types, scope: &Scope, initiator: Option<&str>) -> Result<Outcomes, AnalysisError> {
    let els: HashMap<&str, &Element> = model.elements.iter().filter(|e| in_scope(scope, &e.id)).map(|e| (e.id.as_str(), e)).collect();
    let functions: HashMap<String, f64> = model
        .elements
        .iter()
        .filter(|e| in_scope(scope, &e.id) && is(ty, e, SAFETY_FUNCTION))
        .filter_map(|e| number(&e.properties, "failureProbability").map(|q| (e.name.trim().to_string(), q)))
        .collect();
    let start = match initiator {
        Some(id) => els.get(id).copied().ok_or_else(|| AnalysisError::Unknown(id.chars().take(64).collect()))?,
        None => model
            .elements
            .iter()
            .find(|e| in_scope(scope, &e.id) && is(ty, e, INITIATOR))
            .ok_or_else(|| params_err("the event tree has no initiating event"))?,
    };
    let mut notes = Vec::new();
    let freq = number(&start.properties, "frequency").unwrap_or_else(|| {
        notes.push(format!("{} has no frequency; 1 is used", name_of(start)));
        1.0
    });
    let mut out_by: HashMap<&str, Vec<(&str, bool)>> = HashMap::new();
    for r in &model.relationships {
        let failure = ty.is_a(&r.type_id, FAILURE);
        if !(failure || ty.is_a(&r.type_id, SUCCESS)) || !els.contains_key(r.source.as_str()) || !els.contains_key(r.target.as_str()) {
            continue;
        }
        out_by.entry(r.source.as_str()).or_default().push((r.target.as_str(), failure));
    }
    let mut totals: HashMap<&str, f64> = HashMap::new();
    let mut stack: Vec<(&str, f64, usize)> = vec![(start.id.as_str(), freq, 0)];
    let mut paths = 0usize;
    let mut truncated = false;
    let depth_cap = els.len() + 1;
    let mut missing: HashSet<&str> = HashSet::new();
    while let Some((at, f, depth)) = stack.pop() {
        if depth > depth_cap {
            continue;
        }
        let Some(e) = els.get(at) else { continue };
        if is(ty, e, OUTCOME) {
            *totals.entry(at).or_default() += f;
            paths += 1;
            if paths >= MAX_TREE_PATHS {
                truncated = true;
                break;
            }
            continue;
        }
        let q = if is(ty, e, BRANCH_POINT) {
            branch_q(e, &functions).unwrap_or_else(|| {
                if missing.insert(at) {
                    notes.push(format!("{} has no failure probability; 0 is used", name_of(e)));
                }
                0.0
            })
        } else {
            0.0
        };
        for &(next, failure) in out_by.get(at).map_or(&[][..], Vec::as_slice) {
            let share = if !is(ty, e, BRANCH_POINT) {
                1.0
            } else if failure {
                q
            } else {
                1.0 - q
            };
            stack.push((next, f * share, depth + 1));
        }
    }
    let outcomes = model.elements.iter().filter_map(|e| totals.get(e.id.as_str()).map(|f| (e.id.clone(), *f))).collect();
    Ok(Outcomes { initiator: start.id.clone(), frequency: freq, outcomes, notes, truncated })
}

/// `outcomes`: the outcome frequencies of an event tree.
pub struct OutcomesAnalysis;

impl Analysis for OutcomesAnalysis {
    fn id(&self) -> &'static str {
        "outcomes"
    }
    fn label(&self) -> &'static str {
        "Outcomes"
    }
    fn params(&self) -> &'static str {
        r#"{"initiator"?: element id (default: the first initiating event), "scope"?: [element id]} -> {"initiator", "frequency", "outcomes": [{"id", "frequency"}], "notes"}"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let scope = scope_param(v)?;
        let o = outcomes(cx.model, cx.types, &scope, opt_id(v, "initiator")?)?;
        let names: HashMap<&str, &Element> = cx.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let label = |id: &str| names.get(id).map_or_else(|| id.to_string(), |e| name_of(e));
        let consequence = |id: &str| names.get(id).map(|e| text(&e.properties, "consequence").to_string()).unwrap_or_default();
        let mut r = AnalysisResult::new(self.id());
        r.set_elements(o.outcomes.iter().map(|(id, _)| id.clone()).collect(), Some(Mark::Path));
        r.marks.insert(o.initiator.clone(), Mark::Focus);
        r.table = Some(ResultTable {
            columns: vec!["Outcome".into(), "Consequence".into(), "Frequency".into()],
            rows: o
                .outcomes
                .iter()
                .map(|(id, f)| TableRow { id: id.clone(), cells: vec![json!(label(id)), json!(consequence(id)), json!(f)] })
                .collect(),
        });
        r.truncated |= o.truncated;
        r.summary = format!("{} outcome(s) from {} at {} per year", o.outcomes.len(), label(&o.initiator), sci(o.frequency));
        r.detail = json!({
            "initiator": o.initiator,
            "frequency": o.frequency,
            "outcomes": o.outcomes.iter().map(|(id, f)| json!({"id": id, "frequency": f})).collect::<Vec<_>>(),
            "notes": o.notes,
        });
        Ok(r)
    }
}

#[cfg(test)]
#[path = "reliability_tests.rs"]
mod tests;
