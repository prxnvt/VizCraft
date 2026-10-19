//! Net commands (`net.*`): list, inspect, name, number, merge, split and assign the nets that
//! wire segments belong to.
//!
//! A net is a model item (`vizcraft_model::Net`) that segments name in `Connector.net`; the net
//! pass of the sync step keeps them consistent after every command (see
//! `vizcraft_notation::bind_nets`), so these commands only change names, numbers and membership
//! and let the pass fold, split and tidy. Every command that changes something is one undo step.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::net::{NetIndex, components};
use vizcraft_diagram::{Document, End};
use vizcraft_model::{Issue, Net, NetKind, NetLabel, Severity, issue::Fix};

use super::model::{check_len, page_param, prop_value};
use super::{Kinds, MAX_IDS, opt_bool, opt_str, short, targets};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return the net commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("net.list", "Nets", "Nets pane", list).params(
            r#"{"page"?: page id, name or index, "kind"?: "wire"|"potential"|"pipe"|"fluid"|"signal"} -> {"nets": [{"id", "kind", "name", "number", "label", "segments": [connector id], "pins": [{"shape", "point"}], "junctions": [id], "pages": [page id], "properties"}]}; with page, only the nets drawn on that page"#,
        ),
        CommandSpec::new("net.of", "Net Of", "Nets pane", of).params(
            r#"{"connector": id} or {"shape": id, "point": n} or {"junction": id} -> {"net": the net as net.list gives it, or null}"#,
        ),
        CommandSpec::new("net.members", "Net Members", "Nets pane", members)
            .params(r#"{"net": id} -> {"net", "segments": [connector id], "pins": [{"shape", "point"}], "junctions": [id]}"#),
        CommandSpec::new("net.set", "Net Properties", "Nets pane", set).params(
            r#"{"net": id, "name"?: string, "number"?: string, "kind"?: net kind, "label"?: "once"|"each"|"off", "properties"?: {key: value or null}} -> {"net"}; a name another net holds is refused; a net with a name or number is kept when it loses its wires and is never merged with another named net"#,
        ),
        CommandSpec::new("net.merge", "Merge Nets", "Nets pane", merge).params(
            r#"{"nets": [id, id, ...], "into"?: id, "force"?: bool} -> {"net", "merged": [id]}; every segment of the nets joins `into` (default: the first named, else the first), properties are unioned (into's values win); two or more named nets are refused unless force"#,
        ),
        CommandSpec::new("net.split", "Split Net", "Nets pane", split).params(
            r#"{"net": id, "at": connector id} -> {"net": new id, "segments": [id]}; the wiring joined to that connector gets a new unnamed net of the same kind and properties"#,
        ),
        CommandSpec::new("net.renumber", "Renumber Nets", "Nets pane", renumber).params(
            r#"{"nets"?: [id] | "page"?: page, "scheme"?: "sequential"|"page" (default sequential), "start"?: n (default 1), "step"?: n (default 1), "prefix"?: string, "band"?: page units (default 100), "force"?: bool} -> {"numbered": [{"net", "number"}]}; sequential numbers in model order; page gives page number times 1000 plus the line, the line counted from the top in bands of `band` (2003: page 2, line 3), the next free number on a clash; nets with a name are skipped unless force"#,
        ),
        CommandSpec::new("net.assign", "Assign to Net", "Nets pane", assign).params(
            r#"{"ids"?: [connector id] (default: selection), "net"?: id or null, "kind"?: net kind} -> {"net", "segments": [id]}; makes the connectors segments of `net`, or of a new net of `kind` (default wire); "net": null releases them. A segment keeps its drawn bends: it is no longer rerouted around shapes unless its own avoid flag is set"#,
        ),
    ]
}

/// A net as the commands report it.
pub(crate) fn net_json(n: &Net, ix: &NetIndex) -> Value {
    let entry = ix.nets.get(&n.id).cloned().unwrap_or_default();
    json!({
        "id": n.id,
        "kind": n.kind.name(),
        "name": n.name,
        "number": n.number,
        "label": n.label.name(),
        "segments": entry.segments,
        "pins": entry.pins.iter().map(|(s, p)| json!({"shape": s, "point": p})).collect::<Vec<_>>(),
        "junctions": entry.junctions,
        "pages": entry.pages,
        "properties": n.properties,
    })
}

/// The live issues for named nets joined by wiring.
pub fn conflict_issues(doc: &Document, ix: &NetIndex) -> Vec<Issue> {
    ix.conflicts
        .iter()
        .map(|c| {
            let names: Vec<String> = c
                .nets
                .iter()
                .map(|id| doc.model.net(id).map(|n| if n.name.is_empty() { n.number.clone() } else { n.name.clone() }).unwrap_or_default())
                .collect();
            Issue::new("net.conflict", Severity::Warning, format!("named nets {} are joined by wiring", names.join(" and ")))
                .with_subjects(c.nets.clone())
                .with_items(c.segments.clone())
                .on_page(c.page.clone())
                .with_fix(Fix::new("Merge the nets", "net.merge", json!({"nets": c.nets, "force": true})))
        })
        .collect()
}

/// After the sync step: drop selected ids the net pass removed (a healed junction or wire).
pub(crate) fn prune_removed(s: &mut Session, before: &Document) {
    if s.sel.items.is_empty() || (s.doc.model.nets.is_empty() && before.model.nets.is_empty()) {
        return;
    }
    let (Some(now), Some(was)) = (s.doc.page(), before.page()) else { return };
    let gone: HashSet<&str> =
        was.shapes.iter().map(|x| x.id.as_str()).chain(was.connectors.iter().map(|c| c.id.as_str())).filter(|id| !now.contains_id(id)).collect();
    if gone.is_empty() {
        return;
    }
    let gone: HashSet<String> = gone.into_iter().map(str::to_string).collect();
    s.sel.items.retain(|id| !gone.contains(id));
    if s.sel.primary.as_ref().is_some_and(|p| !s.sel.items.contains(p)) {
        s.sel.primary = s.sel.items.first().cloned();
    }
}

/// The `net` param: an existing net id.
fn req_net(s: &Session, v: &Value) -> Result<String, EngineError> {
    let id = check_len("net", p::req_str(v, "net")?)?;
    s.doc.model.net(id).map(|n| n.id.clone()).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(id))))
}

/// An optional net kind param.
fn kind_param(v: &Value) -> Result<Option<NetKind>, EngineError> {
    match opt_str(v, "kind")? {
        None => Ok(None),
        Some(k) => NetKind::from_name(k).map(Some).ok_or_else(|| EngineError::Params(format!("unknown net kind `{}`", short(k)))),
    }
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let kind = kind_param(v)?;
    let ix = NetIndex::build(&s.doc);
    let page_id = page.and_then(|i| s.doc.pages.get(i)).map(|p| p.id.clone());
    let nets: Vec<Value> = s
        .doc
        .model
        .nets
        .iter()
        .filter(|n| kind.is_none_or(|k| n.kind == k))
        .filter(|n| page_id.as_ref().is_none_or(|pid| ix.nets.get(&n.id).is_some_and(|e| e.pages.contains(pid))))
        .map(|n| net_json(n, &ix))
        .collect();
    Ok(json!({"nets": nets}))
}

fn of(s: &mut Session, v: &Value) -> CmdResult {
    let ix = NetIndex::build(&s.doc);
    let net = if let Some(c) = opt_str(v, "connector")? {
        let c = check_len("connector", c)?;
        s.doc.connector(c).ok_or_else(|| EngineError::Params(format!("no connector `{}`", short(c))))?;
        ix.of_connector.get(c).cloned()
    } else if let Some(j) = opt_str(v, "junction")? {
        let j = check_len("junction", j)?;
        ix.of_junction.get(j).cloned()
    } else if let Some(sh) = opt_str(v, "shape")? {
        let sh = check_len("shape", sh)?;
        let point = p::usize(v, "point").ok_or_else(|| EngineError::Params("`point` (connection point index) is required with `shape`".into()))?;
        ix.of_pin.get(&(sh.to_string(), point)).cloned().or_else(|| ix.of_junction.get(sh).cloned())
    } else {
        return Err(EngineError::Params("give `connector`, `junction`, or `shape` and `point`".into()));
    };
    Ok(json!({"net": net.and_then(|id| s.doc.model.net(&id).map(|n| net_json(n, &ix)))}))
}

fn members(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_net(s, v)?;
    let ix = NetIndex::build(&s.doc);
    let e = ix.nets.get(&id).cloned().unwrap_or_default();
    Ok(json!({
        "net": id,
        "segments": e.segments,
        "pins": e.pins.iter().map(|(s, p)| json!({"shape": s, "point": p})).collect::<Vec<_>>(),
        "junctions": e.junctions,
    }))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_net(s, v)?;
    let name = opt_str(v, "name")?.map(|t| check_len("name", t)).transpose()?.map(str::to_string);
    let number = opt_str(v, "number")?.map(|t| check_len("number", t)).transpose()?.map(str::to_string);
    let kind = kind_param(v)?;
    let label = match opt_str(v, "label")? {
        None => None,
        Some(l) => Some(NetLabel::from_name(l).ok_or_else(|| EngineError::Params(format!("unknown label mode `{}`", short(l))))?),
    };
    let mut props: Vec<(String, Option<vizcraft_model::PropValue>)> = Vec::new();
    match v.get("properties") {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            for (k, x) in o.iter().take(vizcraft_model::MAX_PROPERTIES) {
                super::model::check_key(k)?;
                props.push((k.clone(), if x.is_null() { None } else { Some(prop_value(x, k)?) }));
            }
        }
        Some(_) => return Err(EngineError::Params("`properties` must be an object of key to value or null".into())),
    }
    if let Some(n) = name.as_deref().filter(|n| !n.is_empty())
        && let Some(other) = s.doc.model.nets.iter().find(|x| x.id != id && x.name == n)
    {
        return Err(EngineError::Params(format!("net `{}` is already called `{}`", short(&other.id), short(n))));
    }
    let net = s.doc.model.net_mut(&id).ok_or_else(|| EngineError::Params("no such net".into()))?;
    if let Some(n) = name {
        net.name = n;
    }
    if let Some(n) = number {
        net.number = n;
    }
    if let Some(k) = kind {
        net.kind = k;
    }
    if let Some(l) = label {
        net.label = l;
    }
    for (k, x) in props {
        super::model::set_prop(&mut net.properties, k, x)?;
    }
    s.doc.model.sanitize();
    Ok(json!({"net": id}))
}

/// A list of net ids param: each must exist; duplicates dropped.
fn nets_param(s: &Session, v: &Value, k: &str) -> Result<Vec<String>, EngineError> {
    let Some(Value::Array(a)) = v.get(k) else { return Err(EngineError::Params(format!("`{k}` (array of net ids) is required"))) };
    if a.len() > MAX_IDS {
        return Err(EngineError::Params(format!("`{k}` has more than {MAX_IDS} entries")));
    }
    let mut out: Vec<String> = Vec::new();
    for x in a {
        let id = x.as_str().ok_or_else(|| EngineError::Params(format!("`{k}` must be an array of net ids")))?;
        let n = s.doc.model.net(id).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(id))))?;
        if !out.contains(&n.id) {
            out.push(n.id.clone());
        }
    }
    Ok(out)
}

fn merge(s: &mut Session, v: &Value) -> CmdResult {
    let nets = nets_param(s, v, "nets")?;
    if nets.len() < 2 {
        return Err(EngineError::Params("`nets` must name at least two nets".into()));
    }
    let force = opt_bool(v, "force")?.unwrap_or(false);
    let named: Vec<&String> = nets.iter().filter(|n| s.doc.model.net(n).is_some_and(Net::named)).collect();
    if named.len() >= 2 && !force {
        return Err(EngineError::Params(format!(
            "nets {} are named; merging them loses a name, so pass \"force\": true",
            named.iter().map(|n| short(n)).collect::<Vec<_>>().join(" and ")
        )));
    }
    let into = match opt_str(v, "into")? {
        Some(i) => {
            if !nets.iter().any(|n| n == i) {
                return Err(EngineError::Params("`into` must be one of `nets`".into()));
            }
            i.to_string()
        }
        None => named.first().map(|n| (*n).clone()).or_else(|| nets.first().cloned()).unwrap_or_default(),
    };
    let others: Vec<String> = nets.iter().filter(|n| **n != into).cloned().collect();
    let mut props: BTreeMap<String, vizcraft_model::PropValue> = BTreeMap::new();
    for o in &others {
        if let Some(n) = s.doc.model.net(o) {
            for (k, x) in &n.properties {
                props.entry(k.clone()).or_insert_with(|| x.clone());
            }
        }
    }
    if let Some(t) = s.doc.model.net_mut(&into) {
        for (k, x) in props {
            if t.properties.len() >= vizcraft_model::MAX_PROPERTIES {
                break;
            }
            t.properties.entry(k).or_insert(x);
        }
    }
    let gone: HashSet<&str> = others.iter().map(String::as_str).collect();
    for page in &mut s.doc.pages {
        for c in &mut page.connectors {
            if c.net.as_deref().is_some_and(|n| gone.contains(n)) {
                c.net = Some(into.clone());
            }
        }
    }
    s.doc.model.nets.retain(|n| !gone.contains(n.id.as_str()));
    Ok(json!({"net": into, "merged": others}))
}

fn split(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_net(s, v)?;
    let at = check_len("at", p::req_str(v, "at")?)?;
    let (pi, page) = s
        .doc
        .pages
        .iter()
        .enumerate()
        .find(|(_, p)| p.connector(at).is_some())
        .ok_or_else(|| EngineError::Params(format!("no connector `{}`", short(at))))?;
    if page.connector(at).and_then(|c| c.net.as_deref()) != Some(id.as_str()) {
        return Err(EngineError::Params(format!("`{}` is not a segment of `{}`", short(at), short(&id))));
    }
    let comp = components(pi, page, |c| c.net.as_deref() == Some(id.as_str()))
        .into_iter()
        .find(|k| k.segments.iter().any(|&i| page.connectors.get(i).is_some_and(|c| c.id == at)))
        .unwrap_or_default();
    let template = s.doc.model.net(&id).cloned().unwrap_or_default();
    let fresh = s.doc.next_id("n");
    s.doc.model.nets.push(Net { id: fresh.clone(), name: String::new(), number: String::new(), ..template });
    let mut segs = Vec::new();
    if let Some(page) = s.doc.pages.get_mut(pi) {
        for &i in &comp.segments {
            if let Some(c) = page.connectors.get_mut(i) {
                c.net = Some(fresh.clone());
                segs.push(c.id.clone());
            }
        }
    }
    Ok(json!({"net": fresh, "segments": segs}))
}

fn renumber(s: &mut Session, v: &Value) -> CmdResult {
    let scheme = opt_str(v, "scheme")?.unwrap_or("sequential");
    if !matches!(scheme, "sequential" | "page") {
        return Err(EngineError::Params("`scheme` must be \"sequential\" or \"page\"".into()));
    }
    let start = super::num(v, "start")?.unwrap_or(1.0).clamp(0.0, 1e12).round() as u64;
    let step = super::num(v, "step")?.unwrap_or(1.0).clamp(1.0, 1e9).round() as u64;
    let band = super::num(v, "band")?.unwrap_or(100.0).clamp(1.0, 1e7);
    let prefix = check_len("prefix", opt_str(v, "prefix")?.unwrap_or_default())?.to_string();
    let force = opt_bool(v, "force")?.unwrap_or(false);
    let ix = NetIndex::build(&s.doc);
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(page_param(&s.doc, v)?),
    };
    let page_id = page.and_then(|i| s.doc.pages.get(i)).map(|p| p.id.clone());
    let chosen: Vec<String> = match v.get("nets") {
        None | Some(Value::Null) => s
            .doc
            .model
            .nets
            .iter()
            .filter(|n| page_id.as_ref().is_none_or(|pid| ix.nets.get(&n.id).is_some_and(|e| e.pages.first() == Some(pid))))
            .map(|n| n.id.clone())
            .collect(),
        Some(_) => nets_param(s, v, "nets")?,
    };
    let chosen: Vec<String> = chosen.into_iter().filter(|id| force || s.doc.model.net(id).is_some_and(|n| n.name.is_empty())).collect();
    let mut numbers: Vec<(String, String)> = Vec::new();
    match scheme {
        "sequential" => {
            let mut n = start;
            for id in &chosen {
                numbers.push((id.clone(), format!("{prefix}{n}")));
                n = n.saturating_add(step);
            }
        }
        _ => {
            // Page number times 1000 plus the line counted in bands from the top of the page.
            let mut taken: HashSet<u64> = HashSet::new();
            for id in &chosen {
                let Some(entry) = ix.nets.get(id) else { continue };
                let Some((pi, pg)) = entry.pages.first().and_then(|pid| s.doc.pages.iter().enumerate().find(|(_, p)| p.id == *pid)) else { continue };
                let top = entry
                    .segments
                    .iter()
                    .filter_map(|c| pg.connector(c))
                    .flat_map(|c| vizcraft_diagram::route_polyline(pg, c))
                    .map(|p| p.y)
                    .filter(|y| y.is_finite())
                    .fold(f64::INFINITY, f64::min);
                let y = if top.is_finite() { top - pg.rect().y0 } else { 0.0 };
                let line = ((y / band).floor().max(0.0) as u64).saturating_add(1).min(999);
                let mut n = (pi as u64).saturating_add(1).saturating_mul(1000).saturating_add(line);
                while !taken.insert(n) {
                    n = n.saturating_add(1);
                }
                numbers.push((id.clone(), format!("{prefix}{n}")));
            }
        }
    }
    for (id, number) in &numbers {
        if let Some(n) = s.doc.model.net_mut(id) {
            n.number = number.clone();
        }
    }
    s.doc.model.sanitize();
    let out: Vec<Value> = numbers.into_iter().map(|(n, num)| json!({"net": n, "number": num})).collect();
    Ok(json!({"numbered": out}))
}

fn assign(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Connectors)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let net: Option<String> = match v.get("net") {
        Some(Value::Null) => None,
        Some(_) => Some(req_net(s, v)?),
        None => {
            let kind = kind_param(v)?.unwrap_or_default();
            if s.doc.model.nets.len() >= vizcraft_model::net::MAX_NETS {
                return Err(EngineError::Failed("the model already holds the most nets it can".into()));
            }
            let id = s.doc.next_id("n");
            s.doc.model.nets.push(Net::new(id.clone(), kind));
            Some(id)
        }
    };
    let page = super::page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            c.net = net.clone();
        }
    }
    Ok(json!({"net": net, "segments": ids}))
}

/// The pins of a shape that wire segments are glued to, with the segments on each, for
/// `wire.heal` and the healing delete: `(connection point, [(connector id, true for its start)])`.
pub(crate) fn segmented_pins(page: &vizcraft_diagram::Page, shape: &str) -> Vec<(usize, Vec<String>)> {
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    for c in page.connectors.iter().filter(|c| c.net.is_some()) {
        for e in [&c.start, &c.end] {
            if e.shape() != Some(shape) {
                continue;
            }
            let Some((_, i)) = vizcraft_diagram::net::pin_of(page, e) else { continue };
            match out.iter_mut().find(|(p, _)| *p == i) {
                Some((_, list)) => list.push(c.id.clone()),
                None => out.push((i, vec![c.id.clone()])),
            }
        }
    }
    out
}

/// The two segments to join when `shape` is removed: exactly two pins with segments, one segment
/// each, distinct connectors, not on two different named nets, and nothing else glued to it
/// dynamically. `None` when the shape does not qualify.
pub(crate) fn heal_pair(doc: &Document, page: &vizcraft_diagram::Page, shape: &str) -> Option<(String, String)> {
    let s = page.shape(shape)?;
    if vizcraft_diagram::net::is_junction(s) {
        return None;
    }
    let pins = segmented_pins(page, shape);
    let [(_, a), (_, b)] = pins.as_slice() else { return None };
    let ([a], [b]) = (a.as_slice(), b.as_slice()) else { return None };
    if a == b {
        return None;
    }
    let net = |c: &str| page.connector(c).and_then(|c| c.net.clone());
    let (na, nb) = (net(a), net(b));
    if na != nb && [&na, &nb].iter().all(|n| n.as_deref().and_then(|id| doc.model.net(id)).is_some_and(Net::named)) {
        return None;
    }
    // Only point glue: a dynamic end on the shape is not a pin and would be left dangling.
    let dynamic = page
        .connectors
        .iter()
        .any(|c| [&c.start, &c.end].iter().any(|e| matches!(e, End::Glued { shape: sh, point: None, port: None } if sh == shape)));
    (!dynamic).then(|| (a.clone(), b.clone()))
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
