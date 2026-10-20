//! Instrument loops (`loop.*`, in `loops.rs` because `loop` is a keyword): create a loop and put
//! instruments and valves in it, trace a loop from its measuring element to its final element,
//! list the loops, and draw a loop's ISA-5.4 loop diagram on a new sheet.
//!
//! A loop is a `process.InstrumentationLoopFunction` element that owns its instruments and
//! actuated valves, so its number completes their tags (`LIC-101`); the process sync pass
//! recomposes those tags whenever the loop is renumbered or a member's letters change. Every
//! command that changes the document is one undo step; `loop.trace` and `loop.list` change
//! nothing.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_model::{Element, PropValue};
use vizcraft_notation::infer::Facts;
use vizcraft_notation::process_rules::{COMPONENT, CONTINUES, INSTRUMENT, IO_CHANNEL, LOOP, PRIMARY, SIGNAL, TERMINAL};

use super::model::{NewElement, cat, check_len, create_element, element_of};
use super::{MAX_IDS, opt_str, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The unit element type.
pub const UNIT: &str = "process.Unit";
/// The loop types `loopType` takes.
pub const LOOP_TYPES: &[&str] = &["control", "indication", "safety", "monitoring"];
/// Most elements one trace walks.
const MAX_TRACE: usize = 10_000;

/// Return the loop commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("loop.create", "New Loop", "Notation > Loops", create).params(
            r#"{"variable"?: one letter (default: the first letter of the first member's letters), "members"?: [element or shape id] (default: the selected instruments and valves), "loopType"?: "control"|"indication"|"safety"|"monitoring", "service"?: string, "suffix"?: string, "unit"?: unit element id (default: the unit the first member is in), "id"?: string} -> {"id", "tag", "members"}; a loop numbered in its scope (parallel numbering counts each variable apart), the members moved into it so their tags read its number; one undo step"#,
        ),
        CommandSpec::new("loop.assign", "Assign to Loop", "Notation > Loops", assign).params(
            r#"{"loop": loop element id or null, "elements": [element or shape id]} -> {"loop", "elements"}; puts instruments and valves in the loop (null takes them out of any loop); their tags follow at once"#,
        ),
        CommandSpec::new("loop.trace", "Trace Loop", "Notation > Loops", trace).params(
            r#"{"loop"?: loop id, "element"?: member element or shape id} (default: the selection's first element) -> {"loop", "tag", "chain": [{"element", "tag", "type", "role": "primary"|"transmitter"|"controller"|"final"|"other"}], "signals": [relationship id], "missing": [role], "complete", "cables": [tag], "terminals": [id], "channel"?: tag, "shapes": [shape id on the active page]}; follows signal lines and off-page pairs from the measuring element; read-only"#,
        ),
        CommandSpec::new("loop.list", "Loops", "Notation > Loops", list).params(
            r#"{} -> {"loops": [{"id", "tag", "variable", "loopType", "service", "members": [{"id", "tag", "type"}], "complete"}]}; read-only"#,
        ),
        CommandSpec::new("loop.diagram", "Loop Diagram", "Notation > Loops", super::plant::loop_diagram).params(
            r#"{"loop": loop id or member id, "sheet"?: sheet preset id, "name"?: page name} -> {"page", "shapes": [id], "cables": [id], "terminals": [id], "channels": [id]}; a new Loop page (ISA-5.4) with Field, Junction box, Marshalling, Control room and Panel front lanes, the loop's instruments, a junction box and a marshalling cabinet with terminal strips, the cable pairs as wire nets and an I/O channel per field signal; one undo step"#,
        )
        .when(has_loops),
    ]
}

/// Enabled when the model holds a loop.
fn has_loops(s: &Session) -> Option<&'static str> {
    if s.doc.model.elements.iter().any(|e| e.type_id == LOOP) { None } else { Some("there is no loop yet: make one with New Loop") }
}

/// The element ids a list param names (element or shape ids), checked.
pub(crate) fn element_list(s: &Session, v: &Value, k: &str) -> Result<Option<Vec<String>>, EngineError> {
    let Some(raw) = v.get(k).filter(|x| !x.is_null()) else { return Ok(None) };
    let arr =
        raw.as_array().filter(|a| a.len() <= MAX_IDS).ok_or_else(|| EngineError::Params(format!("`{k}` must be a list of at most {MAX_IDS} ids")))?;
    let mut out = Vec::with_capacity(arr.len());
    for x in arr {
        let id = x.as_str().ok_or_else(|| EngineError::Params(format!("`{k}` must hold ids")))?;
        let e = element_of(&s.doc, check_len(k, id)?)
            .ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element or a shape of one", short(id))))?;
        if !out.contains(&e) {
            out.push(e);
        }
    }
    Ok(Some(out))
}

/// The elements of the selected shapes on the active page.
pub(crate) fn selected_elements(s: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for id in &s.sel.items {
        if let Some(e) = element_of(&s.doc, id)
            && !out.contains(&e)
        {
            out.push(e);
        }
    }
    out
}

/// Whether an element of `type_id` may sit in a loop.
fn loopable(type_id: &str) -> bool {
    vizcraft_notation::nesting::allows(cat(), vizcraft_notation::nesting::allowed_parents(cat(), type_id), Some(LOOP))
}

/// The loop element `id` names: a loop, or a member of one.
fn loop_of(doc: &Document, id: &str) -> Option<String> {
    let e = doc.model.element(id)?;
    if e.type_id == LOOP {
        return Some(e.id.clone());
    }
    doc.model.ancestors(id).into_iter().find(|a| doc.model.element(a).is_some_and(|x| x.type_id == LOOP))
}

/// The unit an element sits in, through its owners.
fn unit_of(doc: &Document, id: &str) -> Option<String> {
    doc.model.ancestors(id).into_iter().find(|a| doc.model.element(a).is_some_and(|x| x.type_id == UNIT))
}

fn text(e: &Element, k: &str) -> String {
    e.prop(k).map(PropValue::display).unwrap_or_default().trim().to_string()
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let members = match element_list(s, v, "members")? {
        Some(m) => m,
        None => selected_elements(s).into_iter().filter(|e| s.doc.model.element(e).is_some_and(|x| loopable(&x.type_id))).collect(),
    };
    for m in &members {
        let t = s.doc.model.element(m).map(|e| e.type_id.clone()).unwrap_or_default();
        if !loopable(&t) {
            return Err(EngineError::Params(format!("`{}` is a {} and cannot sit in a loop", short(m), short(&t))));
        }
    }
    let variable = match opt_str(v, "variable")? {
        Some(x) => x.trim().to_string(),
        None => members
            .iter()
            .filter_map(|m| s.doc.model.element(m))
            .map(|e| text(e, "letters"))
            .find(|l| !l.is_empty())
            .map(|l| l.chars().take(1).collect())
            .unwrap_or_default(),
    };
    if variable.chars().count() != 1 || !variable.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(EngineError::Params("`variable` must be one capital letter (F, L, P, T); with no members to read it from, give it".into()));
    }
    let loop_type = opt_str(v, "loopType")?.unwrap_or("control");
    if !LOOP_TYPES.contains(&loop_type) {
        return Err(EngineError::Params(format!("`loopType` must be one of {}", LOOP_TYPES.join(", "))));
    }
    let unit = match opt_str(v, "unit")? {
        Some(u) => {
            let u = element_of(&s.doc, check_len("unit", u)?).filter(|e| s.doc.model.element(e).is_some_and(|x| x.type_id == UNIT));
            Some(u.ok_or_else(|| EngineError::Params("`unit` must be a plant unit element".into()))?)
        }
        None => members.first().and_then(|m| unit_of(&s.doc, m)),
    };
    let mut props = std::collections::BTreeMap::new();
    props.insert("variable".to_string(), PropValue::Text(variable));
    props.insert("loopType".to_string(), PropValue::Text(loop_type.to_string()));
    for k in ["service", "suffix"] {
        if let Some(x) = opt_str(v, k)? {
            props.insert(k.to_string(), PropValue::Text(super::model::cut(x, vizcraft_model::MAX_TEXT)));
        }
    }
    let id = super::new_id(s, v, "e")?;
    let (id, tag) =
        create_element(&mut s.doc, NewElement { type_id: LOOP.into(), owner: unit, properties: props, id: Some(id), ..NewElement::default() })?;
    for m in &members {
        s.doc.model.set_owner(m, Some(&id)).map_err(super::model::model_err)?;
    }
    with_sel(s, json!({"id": id, "tag": tag, "members": members}))
}

fn assign(s: &mut Session, v: &Value) -> CmdResult {
    let target = match v.get("loop") {
        None => return Err(EngineError::Params("`loop` (a loop id, or null) is required".into())),
        Some(Value::Null) => None,
        Some(Value::String(l)) => {
            let l = element_of(&s.doc, check_len("loop", l)?).filter(|e| s.doc.model.element(e).is_some_and(|x| x.type_id == LOOP));
            Some(l.ok_or_else(|| EngineError::Params("`loop` must be a loop element".into()))?)
        }
        Some(_) => return Err(EngineError::Params("`loop` must be a loop id or null".into())),
    };
    let elements = element_list(s, v, "elements")?.ok_or_else(|| EngineError::Params("`elements` (a list of ids) is required".into()))?;
    if elements.is_empty() {
        return Err(EngineError::Params("`elements` is empty".into()));
    }
    for e in &elements {
        let t = s.doc.model.element(e).map(|x| x.type_id.clone()).unwrap_or_default();
        if !loopable(&t) {
            return Err(EngineError::Params(format!("`{}` is a {} and cannot sit in a loop", short(e), short(&t))));
        }
    }
    for e in &elements {
        // Out of a loop, a member goes back to the loop's unit, if any.
        let owner = match &target {
            Some(l) => Some(l.clone()),
            None => loop_of(&s.doc, e).and_then(|l| s.doc.model.element(&l).and_then(|x| x.owner.clone())),
        };
        s.doc.model.set_owner(e, owner.as_deref()).map_err(super::model::model_err)?;
    }
    Ok(json!({"loop": target, "elements": elements}))
}

/// The role an element plays in its loop.
pub(crate) fn role(facts: &Facts<'_>, e: &Element) -> &'static str {
    let letters = text(e, "letters");
    let functions: String = letters.chars().skip(1).collect();
    if facts.is_a(&e.type_id, PRIMARY) || (facts.is_a(&e.type_id, INSTRUMENT) && functions.contains('E')) {
        "primary"
    } else if facts.is_a(&e.type_id, COMPONENT) || functions.contains('V') {
        "final"
    } else if functions.contains('T') {
        "transmitter"
    } else if functions.contains('C') || functions.contains('K') {
        "controller"
    } else {
        "other"
    }
}

/// What [`walk`] finds for one loop.
pub(crate) struct Trace {
    pub loop_id: String,
    pub chain: Vec<String>,
    pub signals: Vec<String>,
    pub missing: Vec<&'static str>,
    pub complete: bool,
}

/// Walk loop `lp` from its measuring elements along signal lines and off-page pairs.
pub(crate) fn walk(doc: &Document, lp: &str) -> Trace {
    let facts = Facts::new(&doc.model, cat());
    let members: Vec<String> = doc.model.descendants(lp);
    let elements: HashMap<&str, &Element> = doc.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut graph: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for r in doc.model.relationships.iter().filter(|r| r.type_id == SIGNAL || r.type_id == CONTINUES) {
        graph.entry(r.source.as_str()).or_default().push((r.target.as_str(), r.id.as_str()));
        graph.entry(r.target.as_str()).or_default().push((r.source.as_str(), r.id.as_str()));
    }
    let role_of = |id: &str| elements.get(id).map_or("other", |e| role(&facts, e));
    let starts: Vec<&str> = members.iter().map(String::as_str).filter(|m| matches!(role_of(m), "primary" | "transmitter")).collect();
    let mut seen: HashSet<&str> = starts.iter().copied().collect();
    let mut order: Vec<&str> = starts.clone();
    let mut signals: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(n) = order.get(i).copied() {
        i += 1;
        if order.len() >= MAX_TRACE {
            break;
        }
        for (next, rel) in graph.get(n).map(Vec::as_slice).unwrap_or_default() {
            if !signals.iter().any(|s| s == rel) {
                signals.push((*rel).to_string());
            }
            if seen.insert(next) {
                order.push(next);
            }
        }
    }
    // Members the walk did not reach still belong to the chain, after it.
    let mut chain: Vec<String> = order.iter().map(|s| s.to_string()).collect();
    for m in &members {
        if !chain.contains(m) && elements.get(m.as_str()).is_some_and(|e| !facts.is_a(&e.type_id, LOOP)) {
            chain.push(m.clone());
        }
    }
    let reached_final = order.iter().skip(starts.len()).any(|n| role_of(n) == "final");
    let kind = elements.get(lp).map(|e| text(e, "loopType")).filter(|t| !t.is_empty()).unwrap_or_else(|| "control".into());
    let mut missing = Vec::new();
    if kind == "control" {
        if starts.is_empty() {
            missing.push("measuring");
        }
        if !members.iter().any(|m| role_of(m) == "controller") {
            missing.push("controller");
        }
        if !reached_final {
            missing.push("final");
        }
    }
    let complete = kind != "control" || (!starts.is_empty() && reached_final);
    Trace { loop_id: lp.to_string(), chain, signals, missing, complete }
}

fn trace(s: &mut Session, v: &Value) -> CmdResult {
    let from = match (opt_str(v, "loop")?, opt_str(v, "element")?) {
        (Some(l), _) => element_of(&s.doc, check_len("loop", l)?),
        (None, Some(e)) => element_of(&s.doc, check_len("element", e)?),
        (None, None) => selected_elements(s).into_iter().next(),
    };
    let from = from.ok_or_else(|| EngineError::Params("name a `loop` or a member `element`, or select one".into()))?;
    let lp = loop_of(&s.doc, &from).ok_or_else(|| EngineError::Params(format!("`{}` is not in a loop", short(&from))))?;
    let t = walk(&s.doc, &lp);
    let facts = Facts::new(&s.doc.model, cat());
    let chain: Vec<Value> = t
        .chain
        .iter()
        .filter_map(|id| s.doc.model.element(id))
        .map(|e| json!({"element": e.id, "tag": e.tag, "type": e.type_id, "role": role(&facts, e)}))
        .collect();
    let members: HashSet<String> = t.chain.iter().cloned().collect();
    let cables: Vec<String> = s
        .doc
        .model
        .elements
        .iter()
        .filter(|e| e.type_id == "process.Cable" && e.owner.as_deref() == Some(lp.as_str()))
        .map(|e| e.tag.clone())
        .collect();
    // Terminals and channels wired to the loop's elements.
    let wired: Vec<&str> = s
        .doc
        .model
        .relationships
        .iter()
        .filter(|r| r.type_id == "process.Wire")
        .flat_map(|r| [(&r.source, &r.target), (&r.target, &r.source)])
        .filter(|(a, _)| members.contains(a.as_str()))
        .map(|(_, b)| b.as_str())
        .collect();
    let mut terminals: Vec<String> = Vec::new();
    let mut channel: Option<String> = None;
    let mut frontier: Vec<&str> = wired;
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(n) = frontier.pop() {
        if !seen.insert(n) || seen.len() > MAX_TRACE {
            continue;
        }
        let Some(e) = s.doc.model.element(n) else { continue };
        if facts.is_a(&e.type_id, TERMINAL) {
            terminals.push(e.id.clone());
            for r in s.doc.model.relationships.iter().filter(|r| r.type_id == "process.Wire") {
                if r.source == n {
                    frontier.push(r.target.as_str());
                } else if r.target == n {
                    frontier.push(r.source.as_str());
                }
            }
        } else if facts.is_a(&e.type_id, IO_CHANNEL) && channel.is_none() {
            channel = Some(e.tag.clone());
        }
    }
    let page = s.doc.page();
    let shapes: Vec<String> = page
        .map(|p| p.shapes.iter().filter(|sh| sh.element.as_deref().is_some_and(|e| members.contains(e))).map(|sh| sh.id.clone()).collect())
        .unwrap_or_default();
    let tag = s.doc.model.element(&lp).map(|e| e.tag.clone()).unwrap_or_default();
    let mut out = json!({
        "loop": t.loop_id, "tag": tag, "chain": chain, "signals": t.signals, "missing": t.missing, "complete": t.complete,
        "cables": cables, "terminals": terminals, "shapes": shapes,
    });
    if let (Some(c), Some(o)) = (channel, out.as_object_mut()) {
        o.insert("channel".into(), json!(c));
    }
    Ok(out)
}

fn list(s: &mut Session, _v: &Value) -> CmdResult {
    let loops: Vec<Value> = s
        .doc
        .model
        .elements
        .iter()
        .filter(|e| e.type_id == LOOP)
        .map(|lp| {
            let members: Vec<Value> = s
                .doc
                .model
                .descendants(&lp.id)
                .iter()
                .filter_map(|m| s.doc.model.element(m))
                .map(|m| json!({"id": m.id, "tag": m.tag, "type": m.type_id}))
                .collect();
            json!({
                "id": lp.id, "tag": lp.tag, "variable": text(lp, "variable"), "loopType": text(lp, "loopType"), "service": text(lp, "service"),
                "members": members, "complete": walk(&s.doc, &lp.id).complete,
            })
        })
        .collect();
    Ok(json!({"loops": loops}))
}
