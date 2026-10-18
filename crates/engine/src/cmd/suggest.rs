//! Suggestion and insertion commands over [`crate::suggest`]: `suggest.next`, `suggest.connect`,
//! `suggest.search`, and `insert.next` and `insert.connect`, which build a step of a diagram in
//! one undo step.
//!
//! `insert.next` is what the canvas's hover handles, the magic connector and Alt+Shift+Arrow
//! run; an agent gets the same result by id. Each insertion creates the element (for a shape
//! drawing one), its shape, the relationship and a connector glued to both ends, drawn as the
//! pack's connector kind for that relationship type, and reroutes it before returning the ids.

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, End, Page};
use vizcraft_model::{MAX_TEXT, Relationship};
use vizcraft_notation::legal;

use super::model::{NewElement, cat, create_element, cut, model_err, place_shape};
use super::{has_shapes, num, opt_str, page_mut, select, short, with_sel};
use crate::suggest::{self, CommandRow, Direction, HitKind, Suggestion};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return the suggestion and insertion commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("suggest.next", "Suggest Next Step", "Insert > Quick Insert", next)
            .params(r#"{"id": shape id, "direction"?: "right|left|up|down"} -> {"id", "suggestions": [{"type", "label", "relationship", "kind"}]}; legal next types from the pack's connection table, most used pair on the page first, then palette order; a free shape suggests its own stencil with the Basic Arrow kind"#)
            .when(has_shapes),
        CommandSpec::new("suggest.connect", "Suggest Relationship", "Insert > Quick Insert", connect_choices)
            .params(r#"{"from": shape id, "to": shape id} -> {"choices": [{"id", "label", "legal"}]}; legal relationship types first, illegal ones after and marked; free shapes give one plain choice with an empty id"#)
            .when(has_shapes),
        CommandSpec::new("suggest.search", "Search Everything", "Insert > Quick Insert", search).params(
            r#"{"query": string, "kinds"?: ["command"|"type"|"element"], "limit"?: n (default 50, at most 200)} -> {"hits": [{"kind", "id", "label", "detail", "rank", "page"?, "shape"?}]}; commands of the engine registry, element types of the packs in use, elements by name or tag"#,
        ),
        CommandSpec::new("insert.next", "Insert Next Step", "Insert > Quick Insert", insert_next)
            .params(r#"{"id": shape id, "direction": "right|left|up|down", "type"?: element type or stencil, "relationship"?: relationship type, "text"?: string} -> {"shape", "element"?, "relationship"?, "connector", "legal"}; one step: a new shape one step away clear of overlaps, glued to the source by the pack's connector kind, rerouted and selected; no type: the first suggestion of suggest.next"#)
            .when(has_shapes),
        CommandSpec::new("insert.connect", "Connect Shapes", "Insert > Quick Insert", insert_connect)
            .params(r#"{"from": shape id, "to": shape id, "relationship"?: relationship type} -> {"connector", "relationship"?, "legal"}; glued both ends; no relationship: the first choice of suggest.connect (an illegal pair still connects and the rule reports it)"#)
            .when(has_shapes),
    ]
}

/// Results returned when `limit` is not given.
const DEFAULT_LIMIT: usize = 50;

/// A shape id param that must name a shape on the active page.
fn shape_param<'a>(s: &Session, v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
    let id = p::req_str(v, k)?;
    let on_page = s.doc.page().is_some_and(|pg| pg.shape(id).is_some());
    if !on_page {
        return Err(EngineError::Params(format!("`{k}`: `{}` is not a shape on this page", short(id))));
    }
    Ok(id)
}

/// The `direction` param; `required` makes a missing one an error.
fn direction_param(v: &Value, required: bool) -> Result<Option<Direction>, EngineError> {
    match opt_str(v, "direction")? {
        None if required => Err(EngineError::Params("`direction` (right, left, up or down) is required".into())),
        None => Ok(None),
        Some(d) => Direction::parse(d)
            .map(Some)
            .ok_or_else(|| EngineError::Params(format!("`direction` must be right, left, up or down, not `{}`", short(d)))),
    }
}

/// An optional relationship type param that a loaded pack defines.
fn relationship_param(v: &Value) -> Result<Option<&str>, EngineError> {
    match opt_str(v, "relationship")? {
        None => Ok(None),
        Some(r) if cat().relationship_type(r).is_some() => Ok(Some(r)),
        Some(r) => Err(EngineError::Params(format!("unknown relationship type `{}`", short(r)))),
    }
}

fn next(s: &mut Session, v: &Value) -> CmdResult {
    let id = shape_param(s, v, "id")?;
    direction_param(v, false)?;
    let list = suggest::next(&s.doc, id);
    Ok(json!({"id": id, "suggestions": list}))
}

fn connect_choices(s: &mut Session, v: &Value) -> CmdResult {
    let from = shape_param(s, v, "from")?;
    let to = shape_param(s, v, "to")?;
    Ok(json!({"choices": suggest::connect(&s.doc, from, to)}))
}

fn search(s: &mut Session, v: &Value) -> CmdResult {
    let query = p::req_str(v, "query")?;
    let kinds: Vec<HitKind> = match v.get("kinds") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .take(HitKind::ALL.len().saturating_mul(4))
            .map(|k| k.as_str().and_then(HitKind::parse).ok_or_else(|| EngineError::Params("`kinds` holds command, type or element".into())))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(EngineError::Params("`kinds` must be an array of strings".into())),
    };
    let limit = match num(v, "limit")? {
        None => DEFAULT_LIMIT,
        // Clamped as a float first so a huge or negative number cannot overflow the cast.
        Some(n) => n.clamp(1.0, suggest::MAX_HITS as f64) as usize,
    };
    let rows: Vec<CommandRow<'_>> =
        s.registry.all().iter().map(|c| CommandRow { id: c.id, label: c.label, location: c.location, shortcut: c.shortcut }).collect();
    Ok(json!({"hits": suggest::search(&s.doc, &rows, query, &kinds, limit)}))
}

/// The suggestion `insert.next` builds: the one matching `type` (and `relationship`) when given,
/// else the first. A `type` the suggestions do not list is still allowed when it can be drawn:
/// between elements its relationship is the given one or the pack's choice for the pair (an
/// illegal pair is reported by the rules, as `relationship.create` does).
fn pick(s: &Session, source: &str, v: &Value) -> Result<Suggestion, EngineError> {
    let ty = opt_str(v, "type")?;
    let rel = relationship_param(v)?;
    let list = suggest::next(&s.doc, source);
    let matches = |x: &&Suggestion| ty.is_none_or(|t| x.type_id == t) && rel.is_none_or(|r| x.relationship.as_deref() == Some(r));
    if let Some(found) = list.iter().find(matches) {
        return Ok(found.clone());
    }
    let Some(t) = ty else {
        return Err(EngineError::Params("nothing can follow this shape (no legal next type); give `type`".into()));
    };
    let shape = s.doc.page().and_then(|pg| pg.shape(source));
    let source_type = shape.and_then(|sh| suggest::element_type_of(&s.doc, sh)).map(str::to_string);
    match source_type {
        Some(st) => {
            let (_, et) = cat().element_type(t).ok_or_else(|| EngineError::Params(format!("unknown element type `{}`", short(t))))?;
            if cat().master_for_type(t).is_none() {
                return Err(EngineError::Params(format!("type `{}` has no glyph to draw it with", short(t))));
            }
            let relationship = match rel {
                Some(r) => r.to_string(),
                None => legal::relationship_for(cat(), &st, t)
                    .map(|(r, _)| r)
                    .ok_or_else(|| EngineError::Params(format!("no relationship type joins a {} to a {}", short(&st), short(t))))?,
            };
            Ok(Suggestion {
                type_id: t.to_string(),
                label: et.label.clone(),
                kind: suggest::kind_for(Some(&relationship)),
                relationship: Some(relationship),
            })
        }
        None => {
            let m = vizcraft_stencils::master(t).ok_or_else(|| EngineError::Params(format!("unknown stencil `{}`", short(t))))?;
            Ok(Suggestion { type_id: t.to_string(), label: m.label.to_string(), relationship: rel.map(str::to_string), kind: suggest::kind_for(rel) })
        }
    }
}

/// Add a connector from `from` to `to` on the active page drawn as `kind` and bound to
/// `relationship`, avoiding shapes, and route it now. Returns its id.
fn add_connector(s: &mut Session, from: &str, to: &str, kind: &str, relationship: Option<String>) -> Result<String, EngineError> {
    let cid = s.doc.next_id("c");
    let mut c = Connector::new(cid.clone(), End::dynamic(from), End::dynamic(to));
    super::connector_kind::apply_kind(&mut c, kind)?;
    c.avoid = true;
    if let Some(r) = &relationship {
        c.text = s.doc.model.relationship(r).map(|x| x.name.clone()).unwrap_or_default();
    }
    c.relationship = relationship;
    c.sanitize();
    let page = page_mut(s)?;
    page.connectors.push(c);
    vizcraft_diagram::autoroute::reroute(page, Some(std::slice::from_ref(&cid)));
    Ok(cid)
}

/// Add a relationship of `type_id` between two elements; returns its id and whether it is legal.
fn add_relationship(s: &mut Session, type_id: &str, source: &str, target: &str) -> Result<(String, bool), EngineError> {
    let ty = |e: &str| s.doc.model.element(e).map(|x| x.type_id.clone()).unwrap_or_default();
    let ok = legal::is_legal(cat(), &ty(source), type_id, &ty(target));
    let id = s.doc.next_id("r");
    let mut r = Relationship::new(id.clone(), type_id, source, target);
    if let Some((_, rt)) = cat().relationship_type(type_id) {
        super::model::fill_defaults(&mut r.properties, &rt.properties);
    }
    let pack = cat().pack_ref(r.pack());
    s.doc.model.add_relationship(r).map_err(model_err)?;
    if let Some(p) = pack {
        s.doc.model.use_pack(p);
    }
    Ok((id, ok))
}

/// Make `shape` a member of the container holding `source`, when `source` sits in one, so a step
/// inside a container stays inside it (the container refits around it).
fn join_container(page: &mut Page, source: &str, shape: &str) {
    let parent = page.shape(source).and_then(|sh| sh.parent.clone());
    if let Some(p) = parent
        && page.shape(&p).is_some_and(vizcraft_diagram::container::is_container)
    {
        // A locked or full container keeps the step outside it; that is not an error.
        let _ = vizcraft_diagram::container::add_members(page, &p, &[shape.to_string()]);
    }
}

fn insert_next(s: &mut Session, v: &Value) -> CmdResult {
    let source = shape_param(s, v, "id")?.to_string();
    let dir = direction_param(v, true)?.unwrap_or(Direction::Right);
    let text = opt_str(v, "text")?.map(|t| cut(t, MAX_TEXT)).unwrap_or_default();
    let choice = pick(s, &source, v)?;
    let (w, h) = suggest::size_of(&choice.type_id)
        .ok_or_else(|| EngineError::Params(format!("`{}` has no master to draw it with", short(&choice.type_id))))?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let (x, y) = suggest::place(page, &source, w, h, dir).ok_or_else(|| EngineError::Params("the source shape is not on this page".into()))?;
    let source_element = page.shape(&source).and_then(|sh| sh.element.clone()).filter(|e| s.doc.model.element(e).is_some());
    let bound = source_element.is_some() && cat().element_type(&choice.type_id).is_some();

    let (shape, element) = if bound {
        let ne = NewElement { type_id: choice.type_id.clone(), name: text, ..Default::default() };
        let (eid, _) = create_element(&mut s.doc, ne)?;
        let active = s.doc.active_index();
        (place_shape(&mut s.doc, &eid, active, x, y, None, None)?, Some(eid))
    } else {
        let sid = s.doc.next_id("s");
        let mut sh = vizcraft_stencils::new_shape(&choice.type_id, sid.clone(), x, y, None, None)
            .ok_or_else(|| EngineError::Params(format!("unknown stencil `{}`", short(&choice.type_id))))?;
        sh.text = text;
        sh.sanitize();
        page_mut(s)?.shapes.push(sh);
        (sid, None)
    };
    join_container(page_mut(s)?, &source, &shape);

    let (relationship, is_legal) = match (&source_element, &element, &choice.relationship) {
        (Some(a), Some(b), Some(r)) => {
            let (rid, ok) = add_relationship(s, r, a, b)?;
            (Some(rid), ok)
        }
        _ => (None, true),
    };
    let connector = add_connector(s, &source, &shape, &choice.kind, relationship.clone())?;
    select(s, vec![shape.clone()]);
    let mut out = json!({"shape": shape, "connector": connector, "legal": is_legal, "type": choice.type_id, "direction": dir.name()});
    if let Some(o) = out.as_object_mut() {
        if let Some(e) = element {
            o.insert("element".into(), json!(e));
        }
        if let Some(r) = relationship {
            o.insert("relationship".into(), json!(r));
        }
    }
    with_sel(s, out)
}

fn insert_connect(s: &mut Session, v: &Value) -> CmdResult {
    let from = shape_param(s, v, "from")?.to_string();
    let to = shape_param(s, v, "to")?.to_string();
    if from == to {
        return Err(EngineError::Params("`from` and `to` are the same shape".into()));
    }
    let rel = relationship_param(v)?;
    let choices = suggest::connect(&s.doc, &from, &to);
    let elements = {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let el = |id: &str| page.shape(id).and_then(|sh| sh.element.clone()).filter(|e| s.doc.model.element(e).is_some());
        el(&from).zip(el(&to))
    };
    let (relationship, is_legal) = match elements {
        Some((a, b)) => {
            let ty = match rel {
                Some(r) => r.to_string(),
                None => choices
                    .iter()
                    .find(|c| !c.id.is_empty())
                    .map(|c| c.id.clone())
                    .ok_or_else(|| EngineError::Params("no relationship type joins these shapes; give `relationship`".into()))?,
            };
            let (rid, ok) = add_relationship(s, &ty, &a, &b)?;
            (Some(rid), ok)
        }
        None => (None, true),
    };
    let kind = match &relationship {
        Some(r) => suggest::kind_for(s.doc.model.relationship(r).map(|x| x.type_id.as_str())),
        None => suggest::kind_for(rel),
    };
    let connector = add_connector(s, &from, &to, &kind, relationship.clone())?;
    select(s, vec![connector.clone()]);
    let mut out = json!({"connector": connector, "legal": is_legal});
    if let (Some(r), Some(o)) = (relationship, out.as_object_mut()) {
        o.insert("relationship".into(), json!(r));
    }
    with_sel(s, out)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::{End, autoroute};

    use super::super::model::fixture::{blank, refused, run};
    use crate::Session;

    /// A blank session with one Terminator drawn at the origin; returns it and the shape id.
    fn start() -> (Session, String) {
        let mut s = blank();
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Terminator", "name": "Start", "id": "t0", "x": 0, "y": 0}));
        let shape = r["shape"].as_str().unwrap().to_string();
        (s, shape)
    }

    fn str_of(v: &Value, k: &str) -> String {
        v[k].as_str().unwrap_or_else(|| panic!("{k} in {v}")).to_string()
    }

    fn no_overlaps(s: &Session) {
        let page = s.doc.page().unwrap();
        for (i, a) in page.shapes.iter().enumerate() {
            for b in page.shapes.iter().skip(i + 1) {
                let (ra, rb) = (a.bounds(), b.bounds());
                let overlap = ra.x0 < rb.x1 && rb.x0 < ra.x1 && ra.y0 < rb.y1 && rb.y0 < ra.y1;
                assert!(!overlap, "{} {ra:?} overlaps {} {rb:?}", a.id, b.id);
            }
        }
    }

    #[test]
    fn next_on_a_flowchart_process_lists_only_flowchart_types_with_flow() {
        let mut s = blank();
        let r = run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "p", "x": 0, "y": 0}));
        let shape = str_of(&r, "shape");
        let out = run(&mut s, "suggest.next", json!({"id": shape}));
        let list = out["suggestions"].as_array().unwrap();
        assert!(!list.is_empty());
        for x in list {
            assert!(x["type"].as_str().unwrap().starts_with("flowchart."), "{x}");
            assert_eq!(x["relationship"], "flowchart.Flow", "{x}");
            assert_eq!(x["kind"], "flowchart.flow", "{x}");
        }
        // Palette order: the Steps group first, Terminator leading it.
        assert_eq!(list[0]["type"], "flowchart.Terminator");
        assert!(list.iter().any(|x| x["type"] == "flowchart.Database"), "a process may write to a store");
        // A pair already on the page moves to the front.
        run(&mut s, "insert.next", json!({"id": shape, "direction": "down", "type": "flowchart.Decision"}));
        let again = run(&mut s, "suggest.next", json!({"id": shape}));
        assert_eq!(again["suggestions"][0]["type"], "flowchart.Decision");
    }

    #[test]
    fn a_free_shape_suggests_its_own_stencil_with_the_basic_arrow() {
        let mut s = blank();
        run(&mut s, "shape.add", json!({"stencil": "basic.ellipse", "x": 0, "y": 0, "id": "a"}));
        let out = run(&mut s, "suggest.next", json!({"id": "a"}));
        assert_eq!(out["suggestions"], json!([{"type": "basic.ellipse", "label": "Ellipse", "relationship": null, "kind": "basic.arrow"}]));
        let r = run(&mut s, "insert.next", json!({"id": "a", "direction": "right"}));
        let page = s.doc.page().unwrap();
        let new = page.shape(&str_of(&r, "shape")).unwrap();
        assert_eq!((new.stencil.as_str(), new.element.is_none()), ("basic.ellipse", true));
        assert!(new.x > 0.0, "to the right");
        let c = page.connector(&str_of(&r, "connector")).unwrap();
        assert_eq!((c.kind.as_str(), c.relationship.is_none()), ("basic.arrow", true));
    }

    #[test]
    fn insert_next_builds_a_glued_bound_clear_step_in_one_undo_step() {
        let (mut s, t0) = start();
        let undo_before = s.can_undo();
        let r = run(&mut s, "insert.next", json!({"id": t0, "direction": "down", "type": "flowchart.Process", "text": "Do it"}));
        assert!(undo_before);
        let (shape, element, rel, cid) = (str_of(&r, "shape"), str_of(&r, "element"), str_of(&r, "relationship"), str_of(&r, "connector"));
        assert_eq!(r["legal"], true);
        assert_eq!(s.sel.items, vec![shape.clone()], "the new shape is selected");
        let page = s.doc.page().unwrap();
        let (src, new) = (page.shape(&t0).unwrap(), page.shape(&shape).unwrap());
        assert_eq!(new.element.as_deref(), Some(element.as_str()));
        assert_eq!(new.text, "Do it");
        assert!(new.y >= src.y + src.h, "below the source");
        let c = page.connector(&cid).unwrap();
        assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic(t0.clone()), End::dynamic(shape.clone())));
        assert_eq!((c.relationship.as_deref(), c.kind.as_str()), (Some(rel.as_str()), "flowchart.flow"));
        let m = s.doc.model.relationship(&rel).unwrap();
        assert_eq!((m.type_id.as_str(), m.source.as_str(), m.target.as_str()), ("flowchart.Flow", "t0", element.as_str()));
        assert!(autoroute::crossings(page).is_empty());
        // Every direction from the same source, crowding it: nothing overlaps.
        for d in ["down", "right", "left", "up", "down", "right"] {
            run(&mut s, "insert.next", json!({"id": t0, "direction": d, "type": "flowchart.Data"}));
        }
        no_overlaps(&s);
        assert!(autoroute::crossings(s.doc.page().unwrap()).is_empty(), "{:?}", autoroute::crossings(s.doc.page().unwrap()));
        // One insertion is one undo step.
        let shapes = s.doc.page().unwrap().shapes.len();
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.page().unwrap().shapes.len(), shapes - 1);
    }

    #[test]
    fn a_chain_of_five_steps_from_a_terminator_validates_clean() {
        let (mut s, t0) = start();
        let mut at = t0;
        let steps = ["flowchart.Process", "flowchart.Process", "flowchart.Process", "flowchart.Process", "flowchart.Terminator"];
        for (i, ty) in steps.iter().enumerate() {
            let r = run(&mut s, "insert.next", json!({"id": at, "direction": "down", "type": ty, "text": format!("Step {i}")}));
            at = str_of(&r, "shape");
        }
        let page = s.doc.page().unwrap();
        assert_eq!((page.shapes.len(), page.connectors.len()), (6, 5));
        no_overlaps(&s);
        assert!(autoroute::crossings(page).is_empty());
        let v = run(&mut s, "validate.run", json!({}));
        assert_eq!(v["issues"], json!([]), "{v}");
    }

    #[test]
    fn insert_connect_between_shapes_with_no_legal_pair_still_connects_and_the_rule_reports_it() {
        let mut s = blank();
        let a = run(&mut s, "element.create", json!({"type": "flowchart.Database", "id": "d1", "x": 0, "y": 0}));
        let b = run(&mut s, "element.create", json!({"type": "flowchart.Database", "id": "d2", "x": 300, "y": 0}));
        let (a, b) = (str_of(&a, "shape"), str_of(&b, "shape"));
        let choices = run(&mut s, "suggest.connect", json!({"from": a, "to": b}));
        assert_eq!(choices["choices"], json!([{"id": "flowchart.Flow", "label": "Flow", "legal": false}]));
        let r = run(&mut s, "insert.connect", json!({"from": a, "to": b}));
        assert_eq!(r["legal"], false);
        let c = s.doc.page().unwrap().connector(&str_of(&r, "connector")).unwrap();
        assert_eq!((c.start.clone(), c.end.clone()), (End::dynamic(a.clone()), End::dynamic(b.clone())));
        assert!(s.issues.iter().any(|i| i.rule == "flowchart.illegalFlow"), "{:?}", s.issues.iter().map(|i| &i.rule).collect::<Vec<_>>());
        // A legal pair is listed first and applied.
        let p = run(&mut s, "element.create", json!({"type": "flowchart.Process", "id": "proc", "x": 0, "y": 300}));
        let p = str_of(&p, "shape");
        let r = run(&mut s, "insert.connect", json!({"from": p, "to": a}));
        assert_eq!(r["legal"], true);
    }

    #[test]
    fn search_finds_commands_types_and_an_element_by_tag() {
        let mut s = blank();
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Approve order", "tag": "AP-7", "id": "e1", "x": 0, "y": 0}));
        let hits = run(&mut s, "suggest.search", json!({"query": "ap-7"}));
        let first = &hits["hits"][0];
        assert_eq!((first["kind"].as_str(), first["id"].as_str(), first["label"].as_str()), (Some("element"), Some("e1"), Some("Approve order")));
        assert_eq!(first["page"], 0);
        assert!(first["shape"].is_string());
        let types = run(&mut s, "suggest.search", json!({"query": "decision", "kinds": ["type"]}));
        assert_eq!(types["hits"][0]["id"], "flowchart.Decision");
        assert!(types["hits"].as_array().unwrap().iter().all(|h| h["kind"] == "type"));
        let cmds = run(&mut s, "suggest.search", json!({"query": "zoom", "kinds": ["command"], "limit": 3}));
        assert!(cmds["hits"].as_array().unwrap().len() <= 3);
        assert!(cmds["hits"][0]["label"].as_str().unwrap().to_lowercase().starts_with("zoom"));
    }

    #[test]
    fn a_step_inside_a_container_joins_it_and_never_enters_another() {
        let mut s = blank();
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0, "id": "a"}));
        run(&mut s, "container.add", json!({"ids": ["a"], "id": "box"})).as_object().unwrap();
        let frame = s.doc.page().unwrap().shape("box").unwrap().bounds();
        // Another container right where the next step would go.
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": frame.x1 + 60.0, "y": frame.y0 + 10.0, "id": "b"}));
        run(&mut s, "container.add", json!({"ids": ["b"], "id": "other"}));
        let r = run(&mut s, "insert.next", json!({"id": "a", "direction": "right"}));
        let page = s.doc.page().unwrap();
        let new = page.shape(&str_of(&r, "shape")).unwrap();
        assert_eq!(new.parent.as_deref(), Some("box"), "joins the source's container");
        let other = page.shape("other").unwrap().bounds();
        let nb = new.bounds();
        assert!(!(nb.x0 < other.x1 && other.x0 < nb.x1 && nb.y0 < other.y1 && other.y0 < nb.y1), "{nb:?} entered {other:?}");
    }

    #[test]
    fn bad_params_are_refused_without_changes() {
        let (mut s, t0) = start();
        refused(&mut s, "insert.next", json!({"id": t0}));
        refused(&mut s, "insert.next", json!({"id": t0, "direction": "sideways"}));
        refused(&mut s, "insert.next", json!({"id": "nope", "direction": "down"}));
        refused(&mut s, "insert.next", json!({"id": t0, "direction": "down", "type": "nope.Nope"}));
        refused(&mut s, "insert.next", json!({"id": t0, "direction": "down", "relationship": "nope.Nope"}));
        refused(&mut s, "insert.connect", json!({"from": t0, "to": t0}));
        refused(&mut s, "suggest.search", json!({"query": "x", "kinds": ["nope"]}));
        refused(&mut s, "suggest.next", json!({"id": t0, "direction": 5}));
    }
}
