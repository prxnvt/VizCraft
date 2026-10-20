//! Fluid power commands (`fluid.*`): place a component, write ISO 1219-2 component codes, set a
//! valve's port labels, plug ports, read the circuit and trace the pressure through it, the
//! displacement-step chart ([`steps`]) and the hydraulic power unit ([`power_unit`]).
//!
//! The circuit model and the checks live in `vizcraft_notation::fluid`; these commands read it
//! and change the document through element properties (which win over a symbol's own
//! parameters), so every change redraws the symbols and is one undo step. `fluid.circuit`,
//! `fluid.steps` and `fluid.trace` change nothing (the trace keeps its result in
//! `Session.analysis`, so the canvas halos and the Analysis dock show it).

mod power_unit;
mod steps;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use vizcraft_analysis::{AnalysisResult, Mark};
use vizcraft_diagram::glyph_state::GlyphState;
use vizcraft_model::PropValue;
use vizcraft_notation::fluid::{self as fl, Circuit, Positions, Role, Walk, paths, rules as frules};

use super::model::{NewElement, cat, check_len, create_element, element_of, page_param, place_shape};
use super::{num, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the fluid power commands.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = vec![
        CommandSpec::new("fluid.insert", "Insert Fluid Power Component", "Circuit > Symbols", insert).params(
            r#"{"type": fluid power element type, "params"?: {key: value}, "x"?: n, "y"?: n, "page"?: page id, name or index} -> {"element", "shape"}; makes the element and draws it at x, y (top left; default: the page centre), with params as the symbol's own parameters (see glyph.builders); one undo step"#,
        ),
        CommandSpec::new("fluid.renumber", "Renumber Components", "Circuit > Codes", renumber).params(
            r#"{"page"?: page id, name or index, "codes"?: "letters"|"numeric"} -> {"page", "codes", "changed": [{"element", "from", "to"}]}; writes ISO 1219-2 codes on the components drawn on the page: letters ({installation}{circuit}{letter}{n}, 1V1) or numeric ({installation}{circuit}.{n}), numbered per circuit (and letter) left to right, then bottom to top; the default form is the page variant's; step chart rows follow the new codes; one undo step"#,
        ),
        CommandSpec::new("fluid.setPortSet", "Port Labels", "Circuit > Codes", set_port_set).params(
            r#"{"page"?: page id, name or index, "ids"?: [element or shape id], "set": "iso9461"|"iso5599"} -> {"ids": [element]}; labels the directional valves' ports with ISO 9461 letters (P, A, B, T) or ISO 5599 numbers (1, 2, 4, 3, 5); default: every directional valve on the page; one undo step"#,
        ),
        CommandSpec::new("fluid.plug", "Plug Ports", "Circuit > Codes", plug).params(
            r#"{"id": element or shape id, "ports": [port id], "plugged"?: bool (default true)} -> {"id", "plugged": [port id]}; marks ports closed with a plug (the open port check skips them), or opens them again; one undo step"#,
        ),
        CommandSpec::new("fluid.circuit", "Circuit", "Circuit > Analyze", circuit).params(
            r#"{"page"?: page id, name or index} -> {"page", "components": [{"element", "shape", "type", "code", "role", "medium", "ports": [{"id", "point", "net", "open"}], "paths": [[port, port, oneWay]], "positions": [[[port, port]]], "rest"}], "nets": [{"id", "ports": [{"element", "port"}], "segments": [connector]}], "sources": [element], "tanks": [element]}; the circuit the checks read; changes nothing"#,
        ),
        CommandSpec::new("fluid.trace", "Trace Pressure", "Circuit > Analyze", trace).params(
            r#"{"from"?: element or shape id | {"shape", "point"}, "positions"?: "rest"|"any"} -> the stored analysis result ({"kind": "fluidTrace", "summary", "elements", "relationships", "marks", "ports", "nets"}); from a pump or source its outlets, from another component all its ports, from a pin that port (default: the selected shape); through every valve in its rest position or in any position, one-way paths their way, never through a reservoir; kept in the session for the canvas and the Analysis dock; no undo step"#,
        ),
    ];
    v.extend(steps::specs());
    v.extend(power_unit::specs());
    v
}

/// Most ports one `fluid.plug` names.
const MAX_PLUG: usize = 64;

/// The page's module, else 10 units: the column width renumbering groups by.
fn grid(page: &vizcraft_diagram::Page) -> f64 {
    vizcraft_diagram::snap::module_step(page).filter(|m| *m > 0.0).unwrap_or(10.0)
}

/// The element a param names (element or shape id), checked to be a fluid power type.
fn fluid_element(s: &Session, v: &Value, k: &str) -> Result<String, EngineError> {
    let id = check_len(k, crate::p::req_str(v, k)?)?;
    let e = element_of(&s.doc, id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element or a shape drawing one", short(id))))?;
    let t = s.doc.model.element(&e).map(|x| x.type_id.clone()).unwrap_or_default();
    if vizcraft_model::pack_of(&t) != fl::PACK {
        return Err(EngineError::Params(format!("`{}` is not a fluid power element", short(id))));
    }
    Ok(e)
}

/// Place one fluid power component (see the `fluid.insert` params doc).
fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let type_id = check_len("type", crate::p::req_str(v, "type")?)?.to_string();
    let (pack, t) = cat().element_type(&type_id).ok_or_else(|| EngineError::Params(format!("unknown element type `{}`", short(&type_id))))?;
    if pack.id != fl::PACK {
        return Err(EngineError::Params(format!("`{}` is not a fluid power type", short(&type_id))));
    }
    let glyph = cat()
        .glyph(&t.glyph)
        .map(|(_, g)| g)
        .ok_or_else(|| EngineError::Params(format!("`{}` is an abstract type: pick one of its kinds", short(&type_id))))?;
    let spec = vizcraft_stencils::builders::spec(&glyph.builder);
    // The symbol's own parameters, each checked against the builder before anything changes.
    let mut own: BTreeMap<String, PropValue> = BTreeMap::new();
    match v.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) if m.len() <= 64 => {
            let spec = spec.ok_or_else(|| EngineError::Params(format!("`{}` takes no symbol parameters", short(&type_id))))?;
            for (k, x) in m {
                let p = spec.param(k).ok_or_else(|| EngineError::Params(format!("`{}` is not a parameter of `{}`", short(k), spec.name)))?;
                let value = match x {
                    Value::Bool(b) => PropValue::Bool(*b),
                    Value::Number(n) => PropValue::Number(
                        n.as_f64().filter(|f| f.is_finite()).ok_or_else(|| EngineError::Params(format!("`{}` must be finite", short(k))))?,
                    ),
                    Value::String(t) if t.len() <= 1024 => PropValue::Text(t.clone()),
                    _ => return Err(EngineError::Params(format!("`{}` must be text of at most 1024 bytes, a number or true or false", short(k)))),
                };
                if let (_, Some(why)) = p.check(&value) {
                    return Err(EngineError::Params(why));
                }
                own.insert(k.clone(), value);
            }
        }
        Some(_) => return Err(EngineError::Params("`params` must be an object of at most 64 entries".into())),
    }
    let page = page_param(&s.doc, v)?;
    let pg = s.doc.pages.get(page).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let centre = pg.frame().center();
    let x = super::shape::coord(num(v, "x")?.unwrap_or(centre.x - glyph.w / 2.0));
    let y = super::shape::coord(num(v, "y")?.unwrap_or(centre.y - glyph.h / 2.0));
    let (id, _) = create_element(&mut s.doc, NewElement { type_id, ..NewElement::default() })?;
    let shape = place_shape(&mut s.doc, &id, page, x, y, None, None)?;
    if let (Some(spec), false) = (spec, own.is_empty())
        && let Some(sh) = s.doc.pages.get_mut(page).and_then(|p| p.shape_mut(&shape))
    {
        let g = sh.glyph.get_or_insert_with(|| GlyphState { builder: spec.name.to_string(), ..GlyphState::default() });
        g.params.extend(own);
        sh.sanitize();
    }
    if page == s.doc.active_index() {
        select(s, vec![shape.clone()]);
    }
    with_sel(s, json!({"element": id, "shape": shape}))
}

/// The pages a `page` param names: that page.
fn renumber(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let page = s.doc.pages.get(index).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let variant = fl::page_variant(page).to_string();
    let codes = match opt_str(v, "codes")? {
        None => frules::default_codes(cat(), &variant),
        Some("letters") => "letters",
        Some("numeric") => "numeric",
        Some(x) => return Err(EngineError::Params(format!("`codes` is letters or numeric, not `{}`", short(x)))),
    };
    let (letters, numeric) = frules::formats(cat(), &variant);
    let format = if codes == "letters" { letters } else { numeric };
    let step = grid(page);
    let c = Circuit::build(&s.doc, cat(), Some(index));
    // One view per element: the first on the page.
    let mut seen = BTreeSet::new();
    let mut groups: BTreeMap<(String, i64, String), Vec<(i64, f64, String)>> = BTreeMap::new();
    for p in &c.parts {
        if !seen.insert(p.element.clone()) {
            continue;
        }
        let Some(e) = s.doc.model.element(&p.element) else { continue };
        let letter = if codes == "letters" { fl::letter_of(cat(), e) } else { String::new() };
        let key = (fl::installation_of(cat(), e), fl::circuit_of(cat(), e), letter);
        let column = (p.at.0 / step).round().clamp(-1e9, 1e9) as i64;
        groups.entry(key).or_default().push((column, -p.at.1, p.element.clone()));
    }
    let mut plan: Vec<(String, String, String)> = Vec::new();
    for list in groups.values_mut() {
        list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for (n, (_, _, id)) in list.iter().enumerate() {
            let Some(e) = s.doc.model.element(id) else { continue };
            let to = frules::compose(cat(), e, &format, Some(n + 1));
            if to != e.tag {
                plan.push((id.clone(), e.tag.clone(), to));
            }
        }
    }
    let renamed: BTreeMap<String, String> = plan.iter().filter(|(_, from, _)| !from.is_empty()).map(|(_, f, t)| (f.clone(), t.clone())).collect();
    for (id, _, to) in &plan {
        if let Some(e) = s.doc.model.element_mut(id) {
            e.tag = to.clone();
        }
    }
    if !renamed.is_empty() {
        steps::rename_codes(s, &renamed);
    }
    let changed: Vec<Value> = plan.iter().map(|(e, f, t)| json!({"element": e, "from": f, "to": t})).collect();
    let page_id = s.doc.pages.get(index).map(|p| p.id.clone());
    Ok(json!({"page": page_id, "codes": codes, "changed": changed}))
}

fn set_port_set(s: &mut Session, v: &Value) -> CmdResult {
    let set = crate::p::req_str(v, "set")?;
    if !matches!(set, "iso9461" | "iso5599") {
        return Err(EngineError::Params(format!("`set` is iso9461 or iso5599, not `{}`", short(set))));
    }
    let index = page_param(&s.doc, v)?;
    let c = Circuit::build(&s.doc, cat(), Some(index));
    let valves: BTreeSet<String> = match super::ids_param(v)? {
        Some(ids) => {
            let mut out = BTreeSet::new();
            for id in ids.iter().take(super::MAX_IDS) {
                let e = element_of(&s.doc, check_len("ids", id)?)
                    .ok_or_else(|| EngineError::Params(format!("`{}` is not an element or its shape", short(id))))?;
                let t = s.doc.model.element(&e).map(|x| x.type_id.as_str()).unwrap_or_default();
                if !fl::is_a(cat(), t, fl::DIRECTIONAL) && !c.parts.iter().any(|p| p.element == e && p.role == Role::Valve) {
                    return Err(EngineError::Params(format!("`{}` is not a directional valve", short(id))));
                }
                out.insert(e);
            }
            out
        }
        None => c.parts.iter().filter(|p| p.role == Role::Valve).map(|p| p.element.clone()).collect(),
    };
    for id in &valves {
        if let Some(e) = s.doc.model.element_mut(id) {
            e.properties.insert("portLabels".into(), PropValue::Text(set.to_string()));
        }
    }
    Ok(json!({"ids": valves, "set": set}))
}

fn plug(s: &mut Session, v: &Value) -> CmdResult {
    let id = fluid_element(s, v, "id")?;
    let ports: Vec<String> = match v.get("ports") {
        Some(Value::Array(a)) if a.len() <= MAX_PLUG => a
            .iter()
            .map(|x| x.as_str().filter(|t| !t.is_empty() && t.len() <= 16 && !t.contains([' ', ','])).map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| EngineError::Params("`ports` must be port ids (1 to 16 characters, no spaces or commas)".into()))?,
        _ => return Err(EngineError::Params(format!("`ports` (a list of at most {MAX_PLUG} port ids) is required"))),
    };
    let plugged = crate::p::bool(v, "plugged").unwrap_or(true);
    // The ports the element's views have.
    let mut known: BTreeSet<String> = BTreeSet::new();
    let type_id = s.doc.model.element(&id).map(|e| e.type_id.clone()).unwrap_or_default();
    for sh in s.doc.pages.iter().flat_map(|p| p.shapes.iter()).filter(|sh| sh.element.as_deref() == Some(id.as_str())) {
        known.extend(fl::port_ids(cat(), sh, &type_id));
    }
    if let Some(bad) = ports.iter().find(|p| !known.is_empty() && !known.contains(*p)) {
        return Err(EngineError::Params(format!("`{}` has no port `{}`", short(&id), short(bad))));
    }
    let e = s.doc.model.element_mut(&id).ok_or_else(|| EngineError::Params("no such element".into()))?;
    let mut now: BTreeSet<String> = fl::plugged_of(e).into_iter().collect();
    for p in ports {
        if plugged {
            now.insert(p);
        } else {
            now.remove(&p);
        }
    }
    if now.is_empty() {
        e.properties.remove("plugged");
    } else {
        e.properties.insert("plugged".into(), PropValue::Text(now.iter().cloned().collect::<Vec<_>>().join(" ")));
    }
    Ok(json!({"id": id, "plugged": now}))
}

/// A role as `fluid.circuit` names it.
fn role_name(r: Role) -> &'static str {
    match r {
        Role::Valve => "valve",
        Role::Pump => "pump",
        Role::Rotary => "rotary",
        Role::Cylinder => "cylinder",
        Role::PressureValve => "pressureValve",
        Role::PassThrough => "passThrough",
        Role::Tank => "tank",
        Role::End => "end",
        Role::Other => "other",
    }
}

fn circuit(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_param(&s.doc, v)?;
    let c = Circuit::build(&s.doc, cat(), Some(index));
    let port_id = |i: usize| c.ports.get(i).map(|p| p.id.clone()).unwrap_or_default();
    let plugged: BTreeMap<&str, Vec<String>> =
        c.parts.iter().filter_map(|p| s.doc.model.element(&p.element).map(|e| (p.element.as_str(), fl::plugged_of(e)))).collect();
    let components: Vec<Value> = c
        .parts
        .iter()
        .map(|p| {
            let ports: Vec<Value> = p
                .ports
                .iter()
                .filter_map(|&i| c.ports.get(i))
                .map(|x| {
                    let open = paths::is_fluid(&x.id)
                        && !x.glued
                        && x.net.is_none()
                        && !x.exhaust
                        && !plugged.get(p.element.as_str()).is_some_and(|l| l.contains(&x.id));
                    json!({"id": x.id, "point": x.point, "net": x.net, "open": open})
                })
                .collect();
            let paths: Vec<Value> = p.edges.iter().map(|e| json!([port_id(e.a), port_id(e.b), e.directed])).collect();
            let positions: Vec<Value> = p.positions.iter().map(|pos| json!(pos.iter().map(|e| json!([port_id(e.a), port_id(e.b)])).collect::<Vec<_>>())).collect();
            let mut o = json!({"element": p.element, "shape": p.shape, "type": p.type_id, "code": p.tag, "role": role_name(p.role), "medium": p.medium, "ports": ports, "paths": paths});
            if p.role == Role::Valve {
                o["positions"] = json!(positions);
                o["rest"] = json!(p.rest);
            }
            o
        })
        .collect();
    let nets: Vec<Value> = c
        .nets
        .iter()
        .map(|(n, ports)| {
            let list: Vec<Value> = ports
                .iter()
                .filter_map(|&i| c.ports.get(i))
                .filter_map(|x| c.parts.get(x.part).map(|p| json!({"element": p.element, "port": x.id})))
                .collect();
            json!({"id": n, "ports": list, "segments": c.segments.get(n).cloned().unwrap_or_default()})
        })
        .collect();
    let sources: BTreeSet<&str> =
        (0..c.parts.len()).filter(|&i| !c.outlets(cat(), i).is_empty()).filter_map(|i| c.parts.get(i).map(|p| p.element.as_str())).collect();
    let tanks: BTreeSet<&str> = c.parts.iter().filter(|p| p.role == Role::Tank).map(|p| p.element.as_str()).collect();
    let page_id = s.doc.pages.get(index).map(|p| p.id.clone());
    Ok(json!({"page": page_id, "components": components, "nets": nets, "sources": sources, "tanks": tanks}))
}

/// The ports a trace starts from (see the `fluid.trace` params doc).
fn trace_start(s: &Session, c: &Circuit, v: &Value) -> Result<(Vec<usize>, Option<String>), EngineError> {
    let from = v.get("from").cloned().unwrap_or(Value::Null);
    let part_ports = |i: usize| {
        let out = c.outlets(cat(), i);
        if out.is_empty() { c.parts.get(i).map(|p| p.ports.iter().copied().filter(|&q| c.is_fluid(q)).collect()).unwrap_or_default() } else { out }
    };
    match &from {
        Value::Object(o) => {
            let shape = o.get("shape").and_then(Value::as_str).ok_or_else(|| EngineError::Params("`from.shape` (a shape id) is required".into()))?;
            let point = o
                .get("point")
                .and_then(Value::as_u64)
                .ok_or_else(|| EngineError::Params("`from.point` (a connection point index) is required".into()))?;
            let port = usize::try_from(point).ok().and_then(|p| c.port_at(check_len("shape", shape).unwrap_or_default(), p));
            let port = port.ok_or_else(|| {
                EngineError::Params(format!("`{}` point {point} is not a port of a fluid power component on this page", short(shape)))
            })?;
            let element = c.ports.get(port).and_then(|p| c.parts.get(p.part)).map(|p| p.element.clone());
            Ok((vec![port], element))
        }
        Value::String(id) => {
            let id = check_len("from", id)?;
            let e = element_of(&s.doc, id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element or a shape", short(id))))?;
            let parts: Vec<usize> = c.parts_of_element(&e).collect();
            if parts.is_empty() {
                return Err(EngineError::Params(format!("`{}` is not a fluid power component on this page", short(id))));
            }
            Ok((parts.into_iter().flat_map(part_ports).collect(), Some(e)))
        }
        Value::Null => {
            let sel = s.sel.primary.clone().ok_or_else(|| EngineError::Params("`from` is required when no component is selected".into()))?;
            let i = c.part_of_shape(&sel).ok_or_else(|| EngineError::Params("the selected shape is not a fluid power component".into()))?;
            Ok((part_ports(i), c.parts.get(i).map(|p| p.element.clone())))
        }
        _ => Err(EngineError::Params("`from` is an element or shape id, or {shape, point}".into())),
    }
}

fn trace(s: &mut Session, v: &Value) -> CmdResult {
    let positions = match opt_str(v, "positions")? {
        None | Some("any") => Positions::Any,
        Some("rest") => Positions::Rest,
        Some(x) => return Err(EngineError::Params(format!("`positions` is rest or any, not `{}`", short(x)))),
    };
    let index = s.doc.active_index();
    let c = Circuit::build(&s.doc, cat(), Some(index));
    let (start, focus) = trace_start(s, &c, v)?;
    let reached = c.trace(&start, &Walk::pressure(positions));
    let mut elements: Vec<String> = Vec::new();
    for &i in &reached.parts {
        if let Some(p) = c.parts.get(i)
            && !elements.contains(&p.element)
        {
            elements.push(p.element.clone());
        }
    }
    let page = s.doc.pages.get(index);
    let mut relationships: Vec<String> = Vec::new();
    let mut connectors: Vec<String> = Vec::new();
    for n in &reached.nets {
        for cid in c.segments.get(n).into_iter().flatten() {
            connectors.push(cid.clone());
            if let Some(r) = page.and_then(|p| p.connector(cid)).and_then(|x| x.relationship.clone())
                && !relationships.contains(&r)
            {
                relationships.push(r);
            }
        }
    }
    let mut r = AnalysisResult::new("fluidTrace");
    if let Some(f) = &focus {
        r.marks.insert(f.clone(), Mark::Focus);
    }
    let from_name = focus
        .as_deref()
        .and_then(|f| s.doc.model.element(f))
        .map(|e| frules::label(cat(), Some(e), &e.type_id))
        .unwrap_or_else(|| "the start".into());
    r.summary = format!(
        "From {from_name} the pressure reaches {} component{} over {} line{}{}.",
        elements.len().saturating_sub(usize::from(focus.is_some())),
        if elements.len().saturating_sub(usize::from(focus.is_some())) == 1 { "" } else { "s" },
        reached.nets.len(),
        if reached.nets.len() == 1 { "" } else { "s" },
        if positions == Positions::Rest { " with every valve at rest" } else { " in any valve position" }
    );
    r.set_elements(elements, Some(Mark::Downstream));
    r.set_relationships(relationships, Some(Mark::Downstream));
    let ports: Vec<Value> = reached
        .ports
        .iter()
        .filter_map(|&i| c.ports.get(i))
        .filter_map(|x| c.parts.get(x.part).map(|p| json!({"element": p.element, "shape": p.shape, "port": x.id})))
        .collect();
    r.detail = json!({"ports": ports, "nets": reached.nets, "connectors": connectors, "positions": if positions == Positions::Rest { "rest" } else { "any" }});
    r.rev = s.rev();
    let mut out = serde_json::to_value(&r).map_err(|e| EngineError::Failed(e.to_string()))?;
    if let (Some(o), Value::Object(d)) = (out.as_object_mut(), r.detail.clone()) {
        o.remove("detail");
        let extra: Map<String, Value> = d;
        for (k, x) in extra {
            o.entry(k).or_insert(x);
        }
        o.insert("stale".into(), Value::Bool(false));
    }
    s.analysis = Some(r);
    Ok(out)
}
