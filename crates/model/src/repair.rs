//! Repairs and indexes behind [`Model::sanitize`] and the containment walks.
//!
//! Models arrive from files and agents, so nothing here trusts them: every walk is bounded and
//! remembers where it has been, and lookups go through hash maps so a model at the element cap
//! is repaired in linear time rather than with a scan per element.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::{MAX_DEPTH, MAX_ELEMENTS, MAX_ID, MAX_KEY, MAX_PROPERTIES, MAX_RELATIONSHIPS, MAX_TAG, MAX_TEXT, Model, PropValue, SCHEMA_VERSION};

/// Cut `s` to at most `max` bytes at a char boundary, so no multi-byte character is split.
pub(crate) fn truncate(s: &mut String, max: usize) {
    if s.len() > max {
        let mut cut = max;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
}

/// Element index by id; with duplicate ids the first wins, as in [`Model::element`].
pub(crate) fn by_id(model: &Model) -> HashMap<&str, usize> {
    let mut map = HashMap::with_capacity(model.elements.len());
    for (i, e) in model.elements.iter().enumerate() {
        map.entry(e.id.as_str()).or_insert(i);
    }
    map
}

/// Element indexes by owner id, each list in model order.
pub(crate) struct ChildIndex<'a> {
    by_id: HashMap<&'a str, usize>,
    children: HashMap<&'a str, Vec<usize>>,
}

impl<'a> ChildIndex<'a> {
    /// Index `model`.
    pub(crate) fn new(model: &'a Model) -> Self {
        let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, e) in model.elements.iter().enumerate() {
            if let Some(o) = e.owner.as_deref() {
                children.entry(o).or_default().push(i);
            }
        }
        ChildIndex { by_id: by_id(model), children }
    }

    /// Indexes of each root element (unknown ids skipped) followed by everything it owns,
    /// breadth first so parents come before their children, each element once. `max_depth`
    /// bounds how far below a root the walk goes; `None` walks the whole subtree, which still
    /// ends because no element is visited twice.
    pub(crate) fn subtree(&self, model: &Model, roots: &[&str], max_depth: Option<usize>) -> Vec<usize> {
        let mut out = Vec::new();
        let mut seen = vec![false; model.elements.len()];
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        for root in roots {
            let Some(&r) = self.by_id.get(root) else { continue };
            match seen.get_mut(r) {
                Some(s) if !*s => *s = true,
                _ => continue,
            }
            queue.push_back((r, 0));
            while let Some((i, depth)) = queue.pop_front() {
                out.push(i);
                if max_depth.is_some_and(|m| depth >= m) {
                    continue;
                }
                let Some(e) = model.elements.get(i) else { continue };
                // Only the first element with an id owns its children, matching `by_id`; a later
                // duplicate would otherwise adopt them a second time.
                if self.by_id.get(e.id.as_str()) != Some(&i) {
                    continue;
                }
                for &c in self.children.get(e.id.as_str()).map(Vec::as_slice).unwrap_or_default() {
                    if let Some(s) = seen.get_mut(c)
                        && !*s
                    {
                        *s = true;
                        queue.push_back((c, depth.saturating_add(1)));
                    }
                }
            }
        }
        out
    }
}

/// Cap key lengths, drop empty keys, non-finite numbers and entries past the count cap, and cut
/// text values. Keys that collide after cutting keep the first in key order.
fn sanitize_props(props: &mut BTreeMap<String, PropValue>) {
    let needs_work = props.len() > MAX_PROPERTIES
        || props.iter().any(|(k, v)| {
            k.is_empty()
                || k.len() > MAX_KEY
                || match v {
                    PropValue::Number(n) => !n.is_finite(),
                    PropValue::Text(t) => t.len() > MAX_TEXT,
                    PropValue::Bool(_) => false,
                }
        });
    if !needs_work {
        return;
    }
    let mut out = BTreeMap::new();
    for (mut k, mut v) in std::mem::take(props) {
        if out.len() >= MAX_PROPERTIES {
            break;
        }
        truncate(&mut k, MAX_KEY);
        if k.is_empty() || out.contains_key(&k) {
            continue;
        }
        match &mut v {
            PropValue::Number(n) if !n.is_finite() => continue,
            PropValue::Text(t) => truncate(t, MAX_TEXT),
            _ => {}
        }
        out.insert(k, v);
    }
    *props = out;
}

/// Cut an optional id and turn an empty one into `None`.
fn sanitize_opt(s: &mut Option<String>, max: usize) {
    if let Some(v) = s {
        truncate(v, max);
        if v.is_empty() {
            *s = None;
        }
    }
}

/// The repairs [`Model::sanitize`] documents, in an order that makes a second pass a no-op:
/// lengths first (cutting can create duplicates), then ids, then the references between items.
pub(crate) fn sanitize(model: &mut Model) {
    if model.schema_version == 0 {
        model.schema_version = SCHEMA_VERSION;
    }

    model.packs.truncate(Model::MAX_PACKS);
    let mut pack_ids: HashSet<String> = HashSet::new();
    model.packs.retain_mut(|p| {
        truncate(&mut p.id, MAX_KEY);
        truncate(&mut p.version, MAX_KEY);
        !p.id.is_empty() && pack_ids.insert(p.id.clone())
    });

    // Counts are capped before anything else so a hostile file costs work in proportion to the
    // cap, not to its own size. "Keeps the first ones" refers to file order.
    model.elements.truncate(MAX_ELEMENTS);
    model.relationships.truncate(MAX_RELATIONSHIPS);

    let mut ids: HashSet<String> = HashSet::with_capacity(model.elements.len() + model.relationships.len());
    model.elements.retain_mut(|e| {
        truncate(&mut e.id, MAX_ID);
        truncate(&mut e.type_id, MAX_KEY);
        truncate(&mut e.name, MAX_TEXT);
        truncate(&mut e.tag, MAX_TAG);
        sanitize_opt(&mut e.owner, MAX_ID);
        sanitize_props(&mut e.properties);
        !e.id.is_empty() && ids.insert(e.id.clone())
    });
    let element_ids = ids.clone();
    // Elements were inserted first, so an id shared by an element and a relationship keeps the
    // element and drops the relationship.
    model.relationships.retain_mut(|r| {
        truncate(&mut r.id, MAX_ID);
        truncate(&mut r.type_id, MAX_KEY);
        truncate(&mut r.source, MAX_ID);
        truncate(&mut r.target, MAX_ID);
        sanitize_opt(&mut r.source_port, MAX_KEY);
        sanitize_opt(&mut r.target_port, MAX_KEY);
        truncate(&mut r.name, MAX_TEXT);
        sanitize_props(&mut r.properties);
        !r.id.is_empty() && element_ids.contains(&r.source) && element_ids.contains(&r.target) && ids.insert(r.id.clone())
    });

    for e in &mut model.elements {
        if e.owner.as_ref().is_some_and(|o| *o == e.id || !element_ids.contains(o)) {
            e.owner = None;
        }
    }
    repair_tree(model);
}

/// Where the tree repair stands for one element.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Not reached yet.
    New,
    /// On the chain being walked now; reaching it again means a cycle.
    OnPath,
    /// Settled at this depth (number of owners above it).
    Depth(usize),
}

/// Break ownership cycles and cut chains deeper than [`MAX_DEPTH`], moving the offending element
/// to the root. Each element is walked once, so the repair is linear. Ids are unique and owners
/// exist by the time this runs.
fn repair_tree(model: &mut Model) {
    let index: HashMap<String, usize> = model.elements.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
    let owner_of =
        |model: &Model, i: usize| -> Option<usize> { model.elements.get(i).and_then(|e| e.owner.as_ref()).and_then(|o| index.get(o).copied()) };
    let mut marks = vec![Mark::New; model.elements.len()];
    let mut path: Vec<usize> = Vec::new();
    for start in 0..model.elements.len() {
        if marks.get(start) != Some(&Mark::New) {
            continue;
        }
        // Climb until a settled element, the root or a cycle.
        path.clear();
        let mut cur = start;
        let mut base: Option<usize> = None;
        loop {
            if let Some(m) = marks.get_mut(cur) {
                *m = Mark::OnPath;
            }
            path.push(cur);
            match owner_of(model, cur) {
                None => break,
                Some(o) => match marks.get(o).copied() {
                    Some(Mark::Depth(d)) => {
                        base = Some(d);
                        break;
                    }
                    Some(Mark::OnPath) => {
                        // `cur` closes the loop: its owner is already on this chain. Moving `cur`
                        // to the root opens the cycle; the rest of the chain hangs below it.
                        if let Some(e) = model.elements.get_mut(cur) {
                            e.owner = None;
                        }
                        break;
                    }
                    _ => cur = o,
                },
            }
        }
        // Settle depths from the top of the chain down. `base` is the depth of the settled owner
        // above the chain, if any.
        let mut depth = base.map_or(0, |d| d.saturating_add(1));
        for &i in path.iter().rev() {
            if depth > MAX_DEPTH {
                if let Some(e) = model.elements.get_mut(i) {
                    e.owner = None;
                }
                depth = 0;
            }
            if let Some(m) = marks.get_mut(i) {
                *m = Mark::Depth(depth);
            }
            depth = depth.saturating_add(1);
        }
    }
}
