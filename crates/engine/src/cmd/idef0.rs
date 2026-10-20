//! IDEF0 commands (`idef0.*`): decompose a box into a child diagram whose boundary arrow ends
//! carry the parent's ICOM codes, number boxes in staircase order, recompute the codes, and list
//! the node index.
//!
//! A diagram is a page. The context page (A-0) has the viewpoint `idef0.Context`; each child
//! page has `idef0.Decomposition`, its node number in `Page.ext["idef0.node"]` and the box it
//! details in `Page.ext["idef0.parent"]`.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vizcraft_diagram::{Hyperlink, LinkKind};
use vizcraft_model::PropValue;
use vizcraft_notation::idef0_rules::{
    ACTIVITY, ARROW, BOUNDARY, CONTEXT, DECOMPOSITION, NODE_KEY, PARENT_KEY, arrows_by_side, box_number, port_letter, tunnelled,
};
use vizcraft_notation::infer::Facts;

use super::model::{NewElement, cat, create_element, element_of, has_elements, page_param, place_shape, rename_element};
use super::short;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the IDEF0 commands live in the UI.
const LOCATION: &str = "IDEF0 > Model";

/// The IDEF0 commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("idef0.decompose", "Decompose", LOCATION, decompose)
            .params(r#"{"id"?: activity element or shape id (default: the selected box)} -> {"page", "node", "boundaries": [{"code", "id", "shape"}], "shapes": {code: shape id}}; adds the child page A<n> with a Decomposition view, a page link from the box and one boundary arrow end per arrow of the box coded I1, C1, O1, M1 in side order, in one undo step; the child page becomes active"#)
            .when(has_elements),
        CommandSpec::new("idef0.renumber", "Renumber Boxes", LOCATION, renumber)
            .params(r#"{"page"?} -> {"numbered": [{"id", "number"}]}; numbers the boxes on the page 1 to n in staircase order (left to right, top to bottom), the context box 0, in one undo step"#)
            .when(has_elements),
        CommandSpec::new("idef0.icom", "ICOM Codes", LOCATION, icom)
            .params(r#"{"page"?} -> {"codes": [{"id", "code"}]}; recomputes the ICOM code of every boundary arrow end on the page from the box side its arrow meets, in order along the side (tunnelled ones are left alone), in one undo step"#)
            .when(has_elements),
        CommandSpec::new("idef0.nodes", "Node Index", LOCATION, nodes).params(
            r#"{} -> {"nodes": [{"page", "pageName", "node", "parent"?, "boxes": [{"id", "name", "number"}]}]}; the diagrams of the model in page order; changes nothing"#,
        ),
    ]
}

fn facts(s: &Session) -> Facts<'_> {
    Facts::new(&s.doc.model, cat())
}

/// The node number of page `pi`: its stored one, `A-0` on a context page, else empty.
fn node_of(s: &Session, pi: usize) -> String {
    let Some(p) = s.doc.pages.get(pi) else { return String::new() };
    if let Some(n) = p.ext.get(NODE_KEY) {
        return n.clone();
    }
    if p.view.as_ref().is_some_and(|v| v.viewpoint == CONTEXT) { "A-0".into() } else { String::new() }
}

/// The child node of box `number` on a diagram with node `parent`: A-0's box gives A0, A0's box
/// 2 gives A2, A2's box 3 gives A23.
pub fn child_node(parent: &str, number: u32) -> String {
    match parent {
        "A-0" | "" => "A0".into(),
        "A0" => format!("A{number}"),
        p => format!("{p}{number}"),
    }
}

/// Run a registered command's function on the session directly (inside this command's undo step).
fn call(s: &mut Session, id: &str, v: Value) -> CmdResult {
    let run = s.registry.get(id).map(|c| c.run).ok_or_else(|| EngineError::Failed(format!("`{id}` is not registered")))?;
    run(s, &v)
}

fn decompose(s: &mut Session, v: &Value) -> CmdResult {
    let id = match super::opt_str(v, "id")? {
        Some(x) => element_of(&s.doc, super::model::check_len("id", x)?).unwrap_or_else(|| x.to_string()),
        None => super::mech::selected_elements(s)
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::Params("select a box to decompose (or give `id`)".into()))?,
    };
    let f = facts(s);
    let e = s
        .doc
        .model
        .element(&id)
        .filter(|e| f.is_a(&e.type_id, ACTIVITY))
        .ok_or_else(|| EngineError::Params(format!("`{}` is not an IDEF0 box", short(&id))))?;
    let number = box_number(e).ok_or_else(|| EngineError::Params("the box has no number yet; run Renumber first".into()))?;
    let sides: BTreeMap<char, usize> = arrows_by_side(&s.doc.model.relationships, &f, &id).into_iter().map(|(k, v)| (k, v.len())).collect();
    drop(f);
    // The parent diagram: the active page when it draws the box, else the first that does.
    let active = s.doc.active_index();
    let draws = |pi: usize| s.doc.pages.get(pi).is_some_and(|p| p.shapes.iter().any(|sh| sh.element.as_deref() == Some(id.as_str())));
    let parent_pi = if draws(active) {
        active
    } else {
        (0..s.doc.pages.len()).find(|pi| draws(*pi)).ok_or_else(|| EngineError::Params("the box is drawn on no page".into()))?
    };
    let parent_node = node_of(s, parent_pi);
    let node = child_node(&parent_node, number);
    if s.doc.pages.iter().any(|p| p.ext.get(NODE_KEY) == Some(&node)) {
        return Err(EngineError::Params(format!("diagram {node} already exists; the box is decomposed")));
    }
    let box_shape =
        s.doc.pages.get(parent_pi).and_then(|p| p.shapes.iter().find(|sh| sh.element.as_deref() == Some(id.as_str()))).map(|sh| sh.id.clone());
    // The child page takes the parent's size, sheet and setup.
    s.doc.set_active_index(parent_pi);
    if let Some(p) = s.doc.pages.get_mut(parent_pi) {
        p.ext.entry(NODE_KEY.into()).or_insert(parent_node.clone());
    }
    let added = call(s, "page.add", json!({"name": node}))?;
    let page_id = added.get("page").and_then(Value::as_str).unwrap_or_default().to_string();
    call(s, "model.setViewpoint", json!({"viewpoint": DECOMPOSITION}))?;
    // A child diagram is drawn on the same sheet as its parent.
    let parent_sheet = s.doc.pages.get(parent_pi).and_then(|p| p.sheet.as_ref()).map(|sh| sh.preset_id());
    if let Some(preset) = parent_sheet
        && s.registry.get("sheet.set").is_some()
    {
        call(s, "sheet.set", json!({"sheet": preset}))?;
    }
    let pi = s.doc.active_index();
    let frame = {
        let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the new page is missing".into()))?;
        page.ext.insert(NODE_KEY.into(), node.clone());
        page.ext.insert(PARENT_KEY.into(), id.clone());
        page.frame()
    };
    let (w, h) = (frame.x1 - frame.x0, frame.y1 - frame.y0);
    let mut boundaries = Vec::new();
    let mut shapes = serde_json::Map::new();
    for (letter, count) in sides {
        for k in 0..count {
            let t = (k as f64 + 1.0) / (count as f64 + 1.0);
            let (x, y) = match letter {
                'I' => (frame.x0 + w * 0.07, frame.y0 + h * (0.12 + 0.6 * t)),
                'C' => (frame.x0 + w * (0.15 + 0.6 * t), frame.y0 + h * 0.07),
                'O' => (frame.x0 + w * 0.88, frame.y0 + h * (0.12 + 0.6 * t)),
                _ => (frame.x0 + w * (0.15 + 0.6 * t), frame.y0 + h * 0.74),
            };
            let code = format!("{letter}{}", k + 1);
            let ne = NewElement {
                type_id: BOUNDARY.into(),
                name: code.clone(),
                tag: Some(code.clone()),
                owner: None,
                properties: BTreeMap::new(),
                id: None,
            };
            let (eid, _) = create_element(&mut s.doc, ne)?;
            let sid = place_shape(&mut s.doc, &eid, pi, x, y, None, None)?;
            shapes.insert(code.clone(), json!(sid));
            boundaries.push(json!({"code": code, "id": eid, "shape": sid}));
        }
    }
    if let Some(sid) = box_shape
        && let Some(sh) = s.doc.pages.get_mut(parent_pi).and_then(|p| p.shapes.iter_mut().find(|x| x.id == sid))
    {
        sh.links.push(Hyperlink { kind: LinkKind::Page, target: page_id.clone(), label: format!("Diagram {node}") });
    }
    s.status = format!("Diagram {node} added with {} boundary arrows", boundaries.len());
    Ok(json!({"page": page_id, "node": node, "boundaries": boundaries, "shapes": shapes}))
}

fn renumber(s: &mut Session, v: &Value) -> CmdResult {
    let pi = page_param(&s.doc, v)?;
    let f = facts(s);
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let context = page.view.as_ref().is_some_and(|x| x.viewpoint == CONTEXT);
    let mut boxes: Vec<(String, f64, f64)> = page
        .shapes
        .iter()
        .filter_map(|sh| {
            let e = sh.element.as_deref().and_then(|e| f.element(e)).filter(|e| f.is_a(&e.type_id, ACTIVITY))?;
            let r = sh.bounds();
            Some((e.id.clone(), (r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0))
        })
        .collect();
    drop(f);
    if boxes.is_empty() {
        return Err(EngineError::Params("the page draws no IDEF0 boxes".into()));
    }
    // Staircase: left to right, then top to bottom for boxes in one column.
    boxes.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
    let mut out = Vec::new();
    for (i, (id, _, _)) in boxes.iter().enumerate() {
        let n = if context { 0 } else { i + 1 };
        if let Some(e) = s.doc.model.element_mut(id) {
            e.properties.insert("number".into(), PropValue::Text(n.to_string()));
        }
        out.push(json!({"id": id, "number": n}));
    }
    s.status = format!("{} boxes numbered", out.len());
    Ok(json!({"numbered": out}))
}

fn icom(s: &mut Session, v: &Value) -> CmdResult {
    let pi = page_param(&s.doc, v)?;
    let f = facts(s);
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let centre = |eid: &str| {
        page.shapes.iter().find(|sh| sh.element.as_deref() == Some(eid)).map(|sh| {
            let r = sh.bounds();
            ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
        })
    };
    // Per letter: (position along the side, boundary id).
    let mut sides: BTreeMap<char, Vec<(f64, String)>> = BTreeMap::new();
    for sh in &page.shapes {
        let Some(b) = sh.element.as_deref().and_then(|e| f.element(e)).filter(|e| f.is_a(&e.type_id, BOUNDARY)) else { continue };
        let arrows: Vec<&vizcraft_model::Relationship> =
            s.doc.model.relationships.iter().filter(|r| f.is_a(&r.type_id, ARROW) && (r.source == b.id || r.target == b.id)).collect();
        if arrows.iter().any(|r| tunnelled(r)) {
            continue;
        }
        let letter = arrows.iter().find_map(|r| {
            if r.source == b.id {
                port_letter(r.target_port.as_deref()).filter(|l| *l != 'O')
            } else {
                port_letter(r.source_port.as_deref()).filter(|l| *l == 'O')
            }
        });
        let (Some(l), Some((x, y))) = (letter, centre(&b.id)) else { continue };
        sides.entry(l).or_default().push((if matches!(l, 'I' | 'O') { y } else { x }, b.id.clone()));
    }
    drop(f);
    let mut out = Vec::new();
    for (l, mut list) in sides {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (k, (_, id)) in list.iter().enumerate() {
            let code = format!("{l}{}", k + 1);
            rename_element(&mut s.doc, id, &code)?;
            if let Some(e) = s.doc.model.element_mut(id) {
                e.tag = code.clone();
            }
            out.push(json!({"id": id, "code": code}));
        }
    }
    s.status = format!("{} ICOM codes set", out.len());
    Ok(json!({"codes": out}))
}

fn nodes(s: &mut Session, _: &Value) -> CmdResult {
    let f = facts(s);
    let mut out = Vec::new();
    for (pi, p) in s.doc.pages.iter().enumerate() {
        let node = node_of(s, pi);
        if node.is_empty() {
            continue;
        }
        let boxes: Vec<Value> = p
            .shapes
            .iter()
            .filter_map(|sh| sh.element.as_deref().and_then(|e| f.element(e)))
            .filter(|e| f.is_a(&e.type_id, ACTIVITY))
            .map(|e| json!({"id": e.id, "name": e.name, "number": box_number(e)}))
            .collect();
        let mut row = json!({"page": p.id, "pageName": p.name, "node": node, "boxes": boxes});
        if let (Some(parent), Some(o)) = (p.ext.get(PARENT_KEY), row.as_object_mut()) {
            o.insert("parent".into(), json!(parent));
        }
        out.push(row);
    }
    Ok(json!({"nodes": out}))
}

#[cfg(test)]
#[path = "idef0_tests.rs"]
mod tests;
