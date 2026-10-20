//! Sequence diagram commands (`sequence.*`): edit a Sequence page's event order without a
//! mouse. The page is an ordinary page with an interaction (`Page.sequence`); after every
//! command the session's settle pass (`vizcraft_diagram::sequence::settle_doc`) redraws it from
//! the order, so these commands only add items and edit the order, never place anything.
//!
//! Every command but `sequence.new` needs a page with an interaction; refusals name the bad id.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vizcraft_diagram::sequence::{self as seq, Event, Interaction, Operand, Operator, Part, Position, Side, Sort};
use vizcraft_diagram::{Connector, End, Page, Point, Route, Shape};
use vizcraft_model::{MAX_TEXT, PropValue, Relationship, View};

use super::model::{NewElement, cat, check_len, create_element, cut, model_err, page_param, place_shape};
use super::{drop_undrawn_relationships, ids_param, new_id, opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The viewpoint a new interaction's page shows, when the pack for OMG UML is loaded.
pub const VIEWPOINT: &str = "uml.Sequence";

/// Return the sequence commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sequence.new", "New Sequence Diagram", "Insert > Diagram", new)
            .params(r#"{"page"?: page} -> {"page"}; gives the page an interaction (the ordered events every sequence.* command edits) and the uml.Sequence viewpoint; a page that has one keeps it"#),
        CommandSpec::new("sequence.addLifeline", "Add Lifeline", "Sequence > Lifelines", add_lifeline)
            .params(r#"{"name": string, "id"?: shape id, "actor"?: bool, "at"?: column index (default: rightmost), "represents"?: string, "page"?: page} -> {"id", "element"}; a uml.Lifeline (uml.ActorLifeline with actor) element and its lifeline shape"#)
            .when(has_interaction),
        CommandSpec::new("sequence.addMessage", "Add Message", "Sequence > Messages", add_message)
            .params(r#"{"from": lifeline id, "to"?: lifeline id, "label"?: string, "sort"?: "sync"|"async"|"reply"|"create"|"destroy"|"lost"|"found", "id"?: connector id, "before"?|"after"?: message, note or fragment id, "in"?: {"fragment", "operand": n}, "page"?: page} -> {"id", "relationship", "order"}; a lost message needs only from, a found one only to; at most one of before, after and in (default: the end); the uml.Message relationship carries the sort"#)
            .when(has_interaction),
        CommandSpec::new("sequence.reply", "Reply", "Sequence > Messages", reply)
            .params(r#"{"call": message id, "label"?: string, "id"?: connector id, "page"?: page} -> {"id", "relationship", "order"}; a reply from the call's receiver to its sender, placed after the events nested in the call"#)
            .when(has_interaction),
        CommandSpec::new("sequence.move", "Move in Order", "Sequence > Order", move_item)
            .params(r#"{"id": message, note or fragment id, "before"?|"after"?: id, "in"?: {"fragment", "operand": n}, "page"?: page} -> {"id", "order"}; exactly one of before, after and in; a fragment moves with its content and never into itself"#)
            .when(has_interaction),
        CommandSpec::new("sequence.setSort", "Message Sort", "Sequence > Messages", set_sort)
            .params(r#"{"id": message id, "sort": "sync"|"async"|"reply"|"create"|"destroy"|"lost"|"found", "page"?: page} -> {"id", "sort"}; the arrows, dash, connector type and the relationship's sort follow; lost frees the end, found the start"#)
            .when(has_interaction),
        CommandSpec::new("sequence.addNote", "Add Note", "Sequence > Notes", add_note)
            .params(r#"{"text": string, "over": lifeline id or [one or two ids], "side"?: "over"|"left"|"right", "id"?: shape id, "before"?|"after"?: id, "page"?: page} -> {"id", "element", "order"}"#)
            .when(has_interaction),
        CommandSpec::new("sequence.wrap", "Wrap in Fragment", "Sequence > Fragments", wrap)
            .params(r#"{"ids": [message, note or fragment id], "operator": "alt"|"opt"|"loop"|"par"|"break"|"critical"|"ref"|"neg"|"strict"|"seq"|"region", "guard"?: string, "covers"?: [lifeline id], "id"?: shape id, "page"?: page} -> {"id", "order"}; the ids must be contiguous at one nesting level"#)
            .when(has_interaction),
        CommandSpec::new("sequence.unwrap", "Unwrap Fragment", "Sequence > Fragments", unwrap)
            .params(r#"{"id": fragment id, "page"?: page} -> {"id"}; removes the fragment and keeps its content in place"#)
            .when(has_interaction),
        CommandSpec::new("sequence.addOperand", "Add Operand", "Sequence > Fragments", add_operand)
            .params(r#"{"id": fragment id, "before": id directly inside it, "guard"?: string, "page"?: page} -> {"id", "operand"}; a new operand starts at that event"#)
            .when(has_interaction),
        CommandSpec::new("sequence.removeOperand", "Remove Operand", "Sequence > Fragments", remove_operand)
            .params(r#"{"id": fragment id, "operand": n, "page"?: page} -> {"id", "operands"}; its content joins the operand before it (operand 0 joins the next one)"#)
            .when(has_interaction),
        CommandSpec::new("sequence.setFragment", "Fragment Settings", "Sequence > Fragments", set_fragment)
            .params(r#"{"id": fragment id, "operator"?: operator, "guards"?: [string], "covers"?: [lifeline id], "page"?: page} -> {"id", "operator", "guards", "covers"}"#)
            .when(has_interaction),
        CommandSpec::new("sequence.layout", "Lay Out Sequence", "Sequence > Layout", layout)
            .params(r#"{"page"?: page, "row"?: 16..400, "columnGap"?: 0..2000} -> {"ids", "bounds"}; packs the lifelines from the left margin and redraws every row"#)
            .when(has_interaction),
        CommandSpec::new("sequence.order", "Sequence Order", "Sequence > Order", order)
            .params(r#"{"page"?: page} -> {"events": [{"kind", "id", "order", "from", "to", "sort", "label", "fragment", "operand", "depth"}], "findings": [string]}; read only"#)
            .when(has_interaction),
        CommandSpec::new("sequence.write", "Copy Sequence as Text", "Sequence > Code", write)
            .params(r#"{"page"?: page, "grammar": "mermaid"|"plantuml"} -> {"text", "warnings"}; read only"#)
            .when(has_interaction),
        CommandSpec::new("sequence.moveUp", "Move Up", "Sequence > Order", move_up)
            .key("Alt+Up")
            .params(r#"{"id"?: message, note or fragment id (default: the selection), "page"?: page} -> {"id", "order"}; one step earlier: past the block before it, into the operand before at an operand's start, out of the fragment at its first operand's start"#)
            .when(has_interaction),
        CommandSpec::new("sequence.moveDown", "Move Down", "Sequence > Order", move_down)
            .key("Alt+Down")
            .params(r#"{"id"?: message, note or fragment id (default: the selection), "page"?: page} -> {"id", "order"}; one step later: past the block after it, into the next operand at an operand's end, out of the fragment at its last operand's end"#)
            .when(has_interaction),
        CommandSpec::new("sequence.moveLeft", "Move Left", "Sequence > Order", move_left)
            .key("Alt+Left")
            .params(r#"{"id"?: lifeline id (default: the selection), "page"?: page} -> {"id", "column"}; swaps the lifeline's column with its left neighbour's"#)
            .when(has_interaction),
        CommandSpec::new("sequence.moveRight", "Move Right", "Sequence > Order", move_right)
            .key("Alt+Right")
            .params(r#"{"id"?: lifeline id (default: the selection), "page"?: page} -> {"id", "column"}; swaps the lifeline's column with its right neighbour's"#)
            .when(has_interaction),
        CommandSpec::new("sequence.setActivations", "Activations", "Sequence > Activations", set_activations)
            .params(r#"{"mode": "auto"|"explicit", "page"?: page} -> {"mode"}; auto: a synchronous call opens a bar on its receiver and its reply closes it; explicit: only activate and deactivate events draw bars"#)
            .when(has_interaction),
        CommandSpec::new("sequence.setActivation", "Activate or Deactivate", "Sequence > Activations", set_activation)
            .params(r#"{"id"?: message id (default: the selection), "activate"?: bool, "deactivate"?: bool, "page"?: page} -> {"id", "activate", "deactivate"}; activate starts a bar on the receiver directly after the message, deactivate ends the sender's (Mermaid's + and -); false removes it"#)
            .when(has_interaction),
    ]
}

/// Enabled when the active page has an interaction.
fn has_interaction(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| p.sequence.is_some()) { None } else { Some("the page has no interaction (sequence.new adds one)") }
}

fn err(msg: impl Into<String>) -> EngineError {
    EngineError::Params(msg.into())
}

/// The page index of the `page` param, which must have an interaction.
fn seq_page(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let i = page_param(&s.doc, v)?;
    if s.doc.pages.get(i).is_some_and(|p| p.sequence.is_some()) { Ok(i) } else { Err(err("the page has no interaction (sequence.new adds one)")) }
}

fn page_at(s: &mut Session, i: usize) -> Result<&mut Page, EngineError> {
    s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Failed("the page went missing".into()))
}

fn events_mut(page: &mut Page) -> Result<&mut Vec<Event>, EngineError> {
    page.sequence.as_mut().map(|q| &mut q.events).ok_or_else(|| err("the page has no interaction"))
}

fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
    let t = opt_str(v, k)?.ok_or_else(|| err(format!("`{k}` is required")))?;
    check_len(k, t)
}

fn sort_param(v: &Value) -> Result<Option<Sort>, EngineError> {
    match opt_str(v, "sort")? {
        None => Ok(None),
        Some(t) => Sort::from_name(t)
            .map(Some)
            .ok_or_else(|| err(format!("`sort` must be one of sync, async, reply, create, destroy, lost, found (not `{}`)", short(t)))),
    }
}

fn operator_param(v: &Value) -> Result<Option<Operator>, EngineError> {
    match opt_str(v, "operator")? {
        None => Ok(None),
        Some(t) => Operator::from_name(t)
            .map(Some)
            .ok_or_else(|| err(format!("`operator` must be one of {} (not `{}`)", Operator::ALL.map(|o| o.name()).join(", "), short(t)))),
    }
}

/// The position params: at most one of `before`, `after` and `in`.
fn position_param(v: &Value) -> Result<Option<Position>, EngineError> {
    let mut found = Vec::new();
    if let Some(b) = opt_str(v, "before")? {
        found.push(Position::Before(check_len("before", b)?.to_string()));
    }
    if let Some(a) = opt_str(v, "after")? {
        found.push(Position::After(check_len("after", a)?.to_string()));
    }
    match v.get("in") {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            let fragment = o.get("fragment").and_then(Value::as_str).ok_or_else(|| err("`in.fragment` (fragment id) is required"))?;
            let operand = match o.get("operand") {
                None | Some(Value::Null) => 0,
                Some(x) => x.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("`in.operand` must be a whole number"))?,
            };
            found.push(Position::In { fragment: check_len("in.fragment", fragment)?.to_string(), operand });
        }
        Some(_) => return Err(err("`in` must be {\"fragment\", \"operand\"}")),
    }
    if found.len() > 1 {
        return Err(err("give at most one of `before`, `after` and `in`"));
    }
    Ok(found.pop())
}

/// The lifeline shape on `page` named by `id`: a lifeline shape id, or the id of the element
/// one draws.
fn lifeline(page: &Page, k: &str, id: &str) -> Result<String, EngineError> {
    let is = |s: &Shape| matches!(s.sequence, Some(Part::Lifeline { .. }));
    page.shapes
        .iter()
        .find(|s| s.id == id && is(s))
        .or_else(|| page.shapes.iter().find(|s| s.element.as_deref() == Some(id) && is(s)))
        .map(|s| s.id.clone())
        .ok_or_else(|| err(format!("`{k}`: `{}` is not a lifeline on this page", short(id))))
}

/// Insert `e` at `pos` (the end when `None`); returns its index.
fn insert(events: &mut Vec<Event>, e: Event, pos: Option<&Position>) -> Result<usize, EngineError> {
    if events.len() >= seq::MAX_EVENTS {
        return Err(err(format!("the interaction holds at most {} events", seq::MAX_EVENTS)));
    }
    let at = match pos {
        None => events.len(),
        Some(p) => seq::index_for(events, p).map_err(err)?,
    };
    events.insert(at.min(events.len()), e);
    Ok(at)
}

/// The index of `id`'s block in the order.
fn order_of(page: &Page, id: &str) -> Option<usize> {
    page.sequence.as_ref().and_then(|q| seq::block_of(&q.events, id)).map(|r| r.start)
}

fn new(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let known = cat().pack(vizcraft_model::pack_of(VIEWPOINT)).and_then(|p| p.viewpoint(VIEWPOINT)).is_some();
    let page = page_at(s, i)?;
    if page.sequence.is_none() {
        page.sequence = Some(Interaction::default());
    }
    if known {
        let view = page.view.get_or_insert_with(View::default);
        if view.viewpoint.is_empty() {
            view.viewpoint = VIEWPOINT.into();
        }
    }
    Ok(json!({"page": page.id}))
}

fn add_lifeline(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let name = cut(req_str(v, "name")?, MAX_TEXT);
    let actor = opt_bool(v, "actor")?.unwrap_or(false);
    let type_id = if actor { "uml.ActorLifeline" } else { "uml.Lifeline" };
    if cat().element_type(type_id).is_none() {
        return Err(EngineError::Failed(format!("the UML pack with `{type_id}` is not loaded")));
    }
    let at = match v.get("at") {
        None | Some(Value::Null) => None,
        Some(x) => Some(x.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("`at` must be a column index"))?),
    };
    let mut properties = BTreeMap::new();
    if let Some(r) = opt_str(v, "represents")? {
        properties.insert("represents".to_string(), PropValue::Text(cut(r, MAX_TEXT)));
    }
    let sid = new_id(s, v, "s")?;
    // Where the new column goes: before column `at`, else right of the last.
    let (x, y) = {
        let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
        let mut cols: Vec<&Shape> = page.shapes.iter().filter(|x| matches!(x.sequence, Some(Part::Lifeline { .. }))).collect();
        cols.sort_by(|a, b| (a.x + a.w / 2.0).total_cmp(&(b.x + b.w / 2.0)));
        let y = cols.iter().map(|c| c.y).reduce(f64::min).unwrap_or(40.0);
        let x = match (at.and_then(|k| cols.get(k)), cols.last()) {
            (Some(c), _) => c.x + c.w / 2.0 - 2.0 - 50.0,
            (None, Some(c)) => c.x + c.w + 60.0,
            (None, None) => 40.0,
        };
        (x, y)
    };
    let ne = NewElement { type_id: type_id.into(), name, properties, ..NewElement::default() };
    let (eid, _) = create_element(&mut s.doc, ne)?;
    let placed = place_shape(&mut s.doc, &eid, i, x, y, Some(100.0), Some(300.0))?;
    let page = page_at(s, i)?;
    let shape = page.shape_mut(&placed).ok_or_else(|| EngineError::Failed("the lifeline went missing".into()))?;
    shape.id = sid.clone();
    shape.sequence = Some(Part::Lifeline { destroyed: false });
    Ok(json!({"id": sid, "element": eid}))
}

/// Draw `c` as the connector type of `sort` when the pack has it, else set the arrows and dash.
fn look(c: &mut Connector, sort: Sort) {
    match vizcraft_stencils::connector_kind(sort.connector_type()) {
        Some(k) => k.apply(c),
        None => {
            c.end_arrow = if sort == Sort::Sync { vizcraft_diagram::Arrow::Triangle } else { vizcraft_diagram::Arrow::Open };
            c.style.dash = if matches!(sort, Sort::Reply | Sort::Create) { vizcraft_diagram::Dash::Dashed } else { vizcraft_diagram::Dash::Solid };
        }
    }
    c.route = Route::Straight;
    c.avoid = false;
    c.message = Some(seq::Message { sort });
}

/// Add a message between lifeline shapes `from` and `to` (one may be `None` for lost and found)
/// at `pos`, with its relationship when both ends draw elements.
#[allow(clippy::too_many_arguments)]
fn put_message(
    s: &mut Session,
    i: usize,
    id: String,
    from: Option<String>,
    to: Option<String>,
    sort: Sort,
    label: String,
    pos: Option<&Position>,
) -> CmdResult {
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let element = |sid: &Option<String>| sid.as_deref().and_then(|x| page.shape(x)).and_then(|x| x.element.clone());
    let free = |sid: &Option<String>, dx: f64| {
        sid.as_deref().and_then(|x| page.shape(x)).map_or(Point::new(0.0, 0.0), |x| Point::new(x.x + x.w / 2.0 + dx, x.y + x.h / 2.0))
    };
    let (ea, eb) = (element(&from), element(&to));
    let start = match &from {
        Some(f) => End::dynamic(f.clone()),
        None => End::free(free(&to, -60.0)),
    };
    let end = match &to {
        Some(t) => End::dynamic(t.clone()),
        None => End::free(free(&from, 60.0)),
    };
    let mut rid = None;
    if let (Some(a), Some(b)) = (ea, eb)
        && cat().relationship_type("uml.Message").is_some()
    {
        let r_id = s.doc.next_id("r");
        let mut r = Relationship::new(r_id.clone(), "uml.Message", a, b);
        r.name = label.clone();
        r.properties.insert("sort".into(), PropValue::Text(sort.uml().into()));
        let pack = cat().pack_ref(r.pack());
        s.doc.model.add_relationship(r).map_err(model_err)?;
        if let Some(p) = pack {
            s.doc.model.use_pack(p);
        }
        rid = Some(r_id);
    }
    let mut c = Connector::new(id.clone(), start, end);
    look(&mut c, sort);
    c.text = label;
    c.relationship = rid.clone();
    c.sanitize();
    let page = page_at(s, i)?;
    page.connectors.push(c);
    let order = insert(events_mut(page)?, Event::Message(id.clone()), pos)?;
    Ok(json!({"id": id, "relationship": rid, "order": order}))
}

fn add_message(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let sort = sort_param(v)?.unwrap_or_default();
    let label = opt_str(v, "label")?.map(|t| cut(t, MAX_TEXT)).unwrap_or_default();
    let pos = position_param(v)?;
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let from = match opt_str(v, "from")? {
        Some(f) => Some(lifeline(page, "from", check_len("from", f)?)?),
        None => None,
    };
    let to = match opt_str(v, "to")? {
        Some(t) => Some(lifeline(page, "to", check_len("to", t)?)?),
        None => None,
    };
    let (from, to) = match sort {
        Sort::Lost => (Some(from.ok_or_else(|| err("a lost message needs `from`"))?), None),
        Sort::Found => (None, Some(to.or(from).ok_or_else(|| err("a found message needs `to`"))?)),
        _ => (Some(from.ok_or_else(|| err("`from` (lifeline id) is required"))?), Some(to.ok_or_else(|| err("`to` (lifeline id) is required"))?)),
    };
    if let Some(p) = &pos {
        let events = &page.sequence.as_ref().map(|q| q.events.clone()).unwrap_or_default();
        seq::index_for(events, p).map_err(err)?;
    }
    let id = new_id(s, v, "c")?;
    put_message(s, i, id, from, to, sort, label, pos.as_ref())
}

fn message_param(page: &Page, v: &Value, k: &str) -> Result<String, EngineError> {
    let id = req_str(v, k)?;
    if page.connector(id).is_some_and(|c| c.message.is_some()) {
        Ok(id.to_string())
    } else {
        Err(err(format!("`{k}`: `{}` is not a message on this page", short(id))))
    }
}

fn reply(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let call = message_param(page, v, "call")?;
    let c = page.connector(&call).ok_or_else(|| err("the call went missing"))?;
    let (Some(caller), Some(callee)) = (c.start.shape().map(str::to_string), c.end.shape().map(str::to_string)) else {
        return Err(err(format!("`{}` has a free end; a reply needs a call between two lifelines", short(&call))));
    };
    let events = page.sequence.as_ref().map(|q| q.events.clone()).unwrap_or_default();
    let start = events
        .iter()
        .position(|e| matches!(e, Event::Message(m) if *m == call))
        .ok_or_else(|| err(format!("`{}` is not in the order", short(&call))))?;
    // The events nested in the call: what follows while the callee is involved, at the call's
    // nesting level or deeper, up to its own reply.
    let mut last = start;
    let mut depth: i64 = 0;
    for (k, e) in events.iter().enumerate().skip(start + 1) {
        match e {
            Event::Open(_) => depth += 1,
            Event::Else(_) if depth == 0 => break,
            Event::Close(_) => {
                depth -= 1;
                if depth < 0 {
                    break;
                }
                last = k;
                continue;
            }
            Event::Message(m) => {
                let Some(mc) = page.connector(m) else { continue };
                let (a, b) = (mc.start.shape(), mc.end.shape());
                let back = a == Some(callee.as_str()) && b == Some(caller.as_str());
                let involved = a == Some(callee.as_str()) || b == Some(callee.as_str());
                if depth == 0 && (back || !involved) {
                    break;
                }
            }
            _ => {}
        }
        if depth >= 0 {
            last = k;
        }
    }
    let after = events.get(last).map(|e| e.id().to_string()).unwrap_or(call.clone());
    let label = opt_str(v, "label")?.map(|t| cut(t, MAX_TEXT)).unwrap_or_default();
    let id = new_id(s, v, "c")?;
    // After a fragment's close the anchor is the fragment; after anything else its own block.
    put_message(s, i, id, Some(callee), Some(caller), Sort::Reply, label, Some(&Position::After(after)))
}

fn move_item(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let id = req_str(v, "id")?.to_string();
    let pos = position_param(v)?.ok_or_else(|| err("give one of `before`, `after` and `in`"))?;
    let page = page_at(s, i)?;
    seq::move_block(events_mut(page)?, &id, &pos).map_err(err)?;
    let order = order_of(page, &id);
    Ok(json!({"id": id, "order": order}))
}

fn set_sort(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let sort = sort_param(v)?.ok_or_else(|| err("`sort` is required"))?;
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let id = message_param(page, v, "id")?;
    let c = page.connector(&id).cloned().ok_or_else(|| err("the message went missing"))?;
    let glued = c.start.shape().is_some() && c.end.shape().is_some();
    if !matches!(sort, Sort::Lost | Sort::Found) && !glued {
        return Err(err(format!("`{}` has a free end; glue both ends before making it a {}", short(&id), sort.name())));
    }
    let mut c = c;
    let mut dropped = Vec::new();
    match sort {
        Sort::Lost => c.end = End::free(Point::new(0.0, 0.0)),
        Sort::Found => c.start = End::free(Point::new(0.0, 0.0)),
        _ => {}
    }
    if matches!(sort, Sort::Lost | Sort::Found) {
        dropped.extend(c.relationship.take());
    }
    look(&mut c, sort);
    if let Some(r) = c.relationship.as_deref().and_then(|r| s.doc.model.relationship_mut(r)) {
        r.properties.insert("sort".into(), PropValue::Text(sort.uml().into()));
    }
    let page = page_at(s, i)?;
    if let Some(slot) = page.connector_mut(&id) {
        *slot = c;
    }
    drop_undrawn_relationships(&mut s.doc, &dropped);
    Ok(json!({"id": id, "sort": sort.name()}))
}

/// One or two lifeline ids from a string or an array.
fn lifelines_param(page: &Page, v: &Value, k: &str, max: usize) -> Result<Vec<String>, EngineError> {
    let list: Vec<&str> = match v.get(k) {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(a)) => {
            if a.len() > max {
                return Err(err(format!("`{k}` names at most {max} lifelines")));
            }
            a.iter().map(|x| x.as_str().ok_or_else(|| err(format!("`{k}` must hold lifeline ids")))).collect::<Result<_, _>>()?
        }
        None | Some(Value::Null) => Vec::new(),
        Some(_) => return Err(err(format!("`{k}` must be a lifeline id or a list of them"))),
    };
    list.into_iter().map(|id| lifeline(page, k, check_len(k, id)?)).collect()
}

fn add_note(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let text = cut(req_str(v, "text")?, MAX_TEXT);
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let over = lifelines_param(page, v, "over", 2)?;
    if over.is_empty() {
        return Err(err("`over` (one or two lifeline ids) is required"));
    }
    let side = match opt_str(v, "side")? {
        None => Side::Over,
        Some(t) => Side::from_name(t).ok_or_else(|| err(format!("`side` must be over, left or right (not `{}`)", short(t))))?,
    };
    let pos = position_param(v)?;
    if let Some(p) = &pos {
        let events = page.sequence.as_ref().map(|q| q.events.clone()).unwrap_or_default();
        seq::index_for(&events, p).map_err(err)?;
    }
    let sid = new_id(s, v, "s")?;
    let ne = NewElement { type_id: "uml.Note".into(), name: text, ..NewElement::default() };
    if cat().element_type("uml.Note").is_none() {
        return Err(EngineError::Failed("the UML pack with `uml.Note` is not loaded".into()));
    }
    let (eid, _) = create_element(&mut s.doc, ne)?;
    let placed = place_shape(&mut s.doc, &eid, i, 0.0, 0.0, Some(120.0), Some(40.0))?;
    let page = page_at(s, i)?;
    let shape = page.shape_mut(&placed).ok_or_else(|| EngineError::Failed("the note went missing".into()))?;
    shape.id = sid.clone();
    shape.sequence = Some(Part::Note { over, side });
    let order = insert(events_mut(page)?, Event::Note(sid.clone()), pos.as_ref())?;
    Ok(json!({"id": sid, "element": eid, "order": order}))
}

fn fragment_param(page: &Page, v: &Value) -> Result<String, EngineError> {
    let id = req_str(v, "id")?;
    if page.shape(id).is_some_and(|x| matches!(x.sequence, Some(Part::Fragment { .. }))) {
        Ok(id.to_string())
    } else {
        Err(err(format!("`id`: `{}` is not a fragment on this page", short(id))))
    }
}

fn guard(t: &str) -> String {
    cut(t, seq::MAX_GUARD)
}

fn wrap(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let ids = ids_param(v)?.ok_or_else(|| err("`ids` (the messages, notes and fragments to wrap) is required"))?;
    let operator = operator_param(v)?.ok_or_else(|| err("`operator` is required"))?;
    let g = opt_str(v, "guard")?.map(guard).unwrap_or_default();
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let covers = lifelines_param(page, v, "covers", seq::MAX_COVERS)?;
    let fid = new_id(s, v, "s")?;
    let page = page_at(s, i)?;
    let mut events = events_mut(page)?.clone();
    seq::wrap(&mut events, &ids, &fid).map_err(err)?;
    if events.len() > seq::MAX_EVENTS {
        return Err(err(format!("the interaction holds at most {} events", seq::MAX_EVENTS)));
    }
    let mut shape = vizcraft_stencils::new_shape("uml.fragment", fid.clone(), 0.0, 0.0, Some(240.0), Some(120.0))
        .unwrap_or_else(|| Shape::new(fid.clone(), vizcraft_diagram::ShapeKind::Rectangle, 0.0, 0.0, 240.0, 120.0));
    shape.sequence = Some(Part::Fragment { operator, operands: vec![Operand { guard: g, at: 0.0 }], covers });
    seq::fragment_look(&mut shape);
    shape.sanitize();
    page.shapes.insert(0, shape);
    *events_mut(page)? = events;
    let order = order_of(page, &fid);
    Ok(json!({"id": fid, "order": order}))
}

fn unwrap(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let page = page_at(s, i)?;
    let id = fragment_param(page, v)?;
    seq::unwrap(events_mut(page)?, &id);
    page.remove_shape(&id);
    Ok(json!({"id": id}))
}

fn operands_mut<'a>(page: &'a mut Page, id: &str) -> Result<&'a mut Vec<Operand>, EngineError> {
    match page.shape_mut(id).and_then(|x| x.sequence.as_mut()) {
        Some(Part::Fragment { operands, .. }) => Ok(operands),
        _ => Err(err(format!("`{}` is not a fragment on this page", short(id)))),
    }
}

fn add_operand(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let before = req_str(v, "before")?.to_string();
    let g = opt_str(v, "guard")?.map(guard).unwrap_or_default();
    let page = page_at(s, i)?;
    let id = fragment_param(page, v)?;
    if operands_mut(page, &id)?.len() >= seq::MAX_OPERANDS {
        return Err(err(format!("a fragment has at most {} operands", seq::MAX_OPERANDS)));
    }
    let k = seq::add_operand(events_mut(page)?, &id, &before).map_err(err)?;
    let ops = operands_mut(page, &id)?;
    let at = k.min(ops.len());
    ops.insert(at, Operand { guard: g, at: 0.0 });
    Ok(json!({"id": id, "operand": k}))
}

fn remove_operand(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let operand = match v.get("operand") {
        Some(x) => x.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("`operand` must be a whole number"))?,
        None => return Err(err("`operand` (index) is required")),
    };
    let page = page_at(s, i)?;
    let id = fragment_param(page, v)?;
    seq::remove_operand(events_mut(page)?, &id, operand).map_err(err)?;
    let ops = operands_mut(page, &id)?;
    if ops.len() > 1 {
        let k = if operand == 0 { 0 } else { operand.min(ops.len() - 1) };
        ops.remove(k);
    }
    let n = ops.len();
    Ok(json!({"id": id, "operands": n}))
}

fn set_fragment(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let operator = operator_param(v)?;
    let guards: Option<Vec<String>> = match v.get("guards") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) if a.len() <= seq::MAX_OPERANDS => {
            Some(a.iter().map(|x| x.as_str().map(guard).ok_or_else(|| err("`guards` must hold strings"))).collect::<Result<_, _>>()?)
        }
        Some(_) => return Err(err(format!("`guards` must be a list of at most {} strings", seq::MAX_OPERANDS))),
    };
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let covers = if v.get("covers").is_some_and(|x| !x.is_null()) { Some(lifelines_param(page, v, "covers", seq::MAX_COVERS)?) } else { None };
    let page = page_at(s, i)?;
    let id = fragment_param(page, v)?;
    let Some(Part::Fragment { operator: op, operands, covers: cov }) = page.shape_mut(&id).and_then(|x| x.sequence.as_mut()) else {
        return Err(err(format!("`{}` is not a fragment on this page", short(&id))));
    };
    if let Some(o) = operator {
        *op = o;
    }
    if let Some(g) = guards {
        for (o, g) in operands.iter_mut().zip(g) {
            o.guard = g;
        }
    }
    if let Some(c) = covers {
        *cov = c;
    }
    let out = json!({"id": id, "operator": op.name(), "guards": operands.iter().map(|o| o.guard.clone()).collect::<Vec<_>>(), "covers": cov.clone()});
    Ok(out)
}

fn layout(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let row = super::num(v, "row")?;
    let gap = super::num(v, "columnGap")?;
    if row.is_some_and(|r| !(16.0..=400.0).contains(&r)) {
        return Err(err("`row` must be 16 to 400"));
    }
    if gap.is_some_and(|g| !(0.0..=2000.0).contains(&g)) {
        return Err(err("`columnGap` must be 0 to 2000"));
    }
    let page = page_at(s, i)?;
    if let Some(q) = page.sequence.as_mut() {
        if let Some(r) = row {
            q.row = r;
        }
        if let Some(g) = gap {
            q.column_gap = g;
        }
    }
    let out = seq::relayout(page).map_err(EngineError::Failed)?;
    seq::number_pending(&mut s.doc);
    let bounds = out.bounds.map(|b| json!({"x": b.x0, "y": b.y0, "width": b.width(), "height": b.height()}));
    Ok(json!({"ids": out.ids, "bounds": bounds}))
}

fn order(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
    let events = page.sequence.as_ref().map(|q| q.events.clone()).unwrap_or_default();
    let mut out = Vec::with_capacity(events.len());
    // Innermost fragment and operand per event.
    let mut stack: Vec<(String, usize)> = Vec::new();
    for (k, e) in events.iter().enumerate() {
        if let Event::Else(f) = e
            && let Some(top) = stack.last_mut().filter(|t| &t.0 == f)
        {
            top.1 += 1;
        }
        if let Event::Close(_) = e {
            stack.pop();
        }
        let (fragment, operand) = stack.last().map_or((Value::Null, Value::Null), |(f, o)| (json!(f), json!(o)));
        let mut row = json!({"kind": e.kind(), "id": e.id(), "order": k, "fragment": fragment, "operand": operand, "depth": stack.len()});
        if let Event::Message(m) = e
            && let Some(c) = page.connector(m)
        {
            row["from"] = json!(c.start.shape());
            row["to"] = json!(c.end.shape());
            row["sort"] = json!(c.message.as_ref().map(|x| x.sort.name()));
            row["label"] = json!(c.text);
        }
        if let Event::Activate(l) | Event::Deactivate(l) = e {
            row["to"] = json!(l);
        }
        if let Event::Open(f) = e {
            stack.push((f.clone(), 0));
        }
        out.push(row);
    }
    let findings: Vec<String> = seq::check_order(page).into_iter().chain(seq::check_replies(page, false)).map(|f| f.what).collect();
    Ok(json!({"events": out, "findings": findings}))
}

/// The `id` param, else the selection's primary item.
fn id_or_selection(s: &Session, v: &Value) -> Result<String, EngineError> {
    match opt_str(v, "id")? {
        Some(id) => Ok(check_len("id", id)?.to_string()),
        None => s.sel.primary.clone().ok_or_else(|| err("`id` is required (nothing is selected)")),
    }
}

fn move_step(s: &mut Session, v: &Value, down: bool) -> CmdResult {
    let i = seq_page(s, v)?;
    let id = id_or_selection(s, v)?;
    let page = page_at(s, i)?;
    let events = events_mut(page)?;
    let pos = seq::step(events, &id, down).map_err(err)?;
    seq::move_block(events, &id, &pos).map_err(err)?;
    let order = order_of(page, &id);
    Ok(json!({"id": id, "order": order}))
}

fn move_up(s: &mut Session, v: &Value) -> CmdResult {
    move_step(s, v, false)
}

fn move_down(s: &mut Session, v: &Value) -> CmdResult {
    move_step(s, v, true)
}

/// Swap lifeline `id`'s column with its neighbour's: their centres trade places, and settle
/// pushes the columns apart where they need room.
fn move_column(s: &mut Session, v: &Value, right: bool) -> CmdResult {
    let i = seq_page(s, v)?;
    let id = id_or_selection(s, v)?;
    let page = page_at(s, i)?;
    let id = lifeline(page, "id", &id)?;
    let mut cols: Vec<(String, f64, f64)> =
        page.shapes.iter().filter(|x| matches!(x.sequence, Some(Part::Lifeline { .. }))).map(|x| (x.id.clone(), x.x + x.w / 2.0, x.w)).collect();
    cols.sort_by(|a, b| a.1.total_cmp(&b.1));
    let k = cols.iter().position(|c| c.0 == id).ok_or_else(|| err("the lifeline went missing"))?;
    let n = if right { k + 1 } else { k.checked_sub(1).ok_or_else(|| err(format!("`{}` is already the leftmost lifeline", short(&id))))? };
    let (Some(a), Some(b)) = (cols.get(k).cloned(), cols.get(n).cloned()) else {
        return Err(err(format!("`{}` is already the rightmost lifeline", short(&id))));
    };
    if let Some(x) = page.shape_mut(&a.0) {
        x.x = b.1 - a.2 / 2.0;
    }
    if let Some(x) = page.shape_mut(&b.0) {
        x.x = a.1 - b.2 / 2.0;
    }
    Ok(json!({"id": id, "column": n}))
}

fn move_left(s: &mut Session, v: &Value) -> CmdResult {
    move_column(s, v, false)
}

fn move_right(s: &mut Session, v: &Value) -> CmdResult {
    move_column(s, v, true)
}

fn set_activations(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let mode = req_str(v, "mode")?;
    let m = seq::Activations::from_name(mode).ok_or_else(|| err(format!("`mode` must be auto or explicit (not `{}`)", short(mode))))?;
    let page = page_at(s, i)?;
    if let Some(q) = page.sequence.as_mut() {
        q.activations = m;
    }
    Ok(json!({"mode": m.name()}))
}

fn set_activation(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let id = id_or_selection(s, v)?;
    let activate = opt_bool(v, "activate")?;
    let deactivate = opt_bool(v, "deactivate")?;
    if activate.is_none() && deactivate.is_none() {
        return Err(err("give `activate` or `deactivate` (true adds it, false removes it)"));
    }
    let page = page_at(s, i)?;
    let c =
        page.connector(&id).filter(|c| c.message.is_some()).ok_or_else(|| err(format!("`id`: `{}` is not a message on this page", short(&id))))?;
    let receiver = c.end.shape().filter(|x| page.shape(x).is_some_and(|x| matches!(x.sequence, Some(Part::Lifeline { .. })))).map(str::to_string);
    let sender = c.start.shape().filter(|x| page.shape(x).is_some_and(|x| matches!(x.sequence, Some(Part::Lifeline { .. })))).map(str::to_string);
    let events = events_mut(page)?;
    let at =
        events.iter().position(|e| matches!(e, Event::Message(m) if *m == id)).ok_or_else(|| err(format!("`{}` is not in the order", short(&id))))?;
    // The run of activation events directly after the message.
    let run_end =
        events.iter().skip(at + 1).position(|e| !matches!(e, Event::Activate(_) | Event::Deactivate(_))).map_or(events.len(), |k| at + 1 + k);
    let mut run: Vec<Event> = events.drain(at + 1..run_end).collect();
    let mut apply = |want: Option<bool>, who: &Option<String>, what: &str, make: fn(String) -> Event| -> Result<(), EngineError> {
        let Some(want) = want else { return Ok(()) };
        let Some(l) = who else { return Err(err(format!("`{}` has no {what} lifeline", short(&id)))) };
        let ev = make(l.clone());
        let has = run.contains(&ev);
        if want && !has {
            run.push(ev);
        } else if !want && has {
            run.retain(|e| *e != ev);
        }
        Ok(())
    };
    let res = apply(activate, &receiver, "receiving", Event::Activate).and_then(|_| apply(deactivate, &sender, "sending", Event::Deactivate));
    if events.len() + run.len() > seq::MAX_EVENTS {
        run.truncate(seq::MAX_EVENTS.saturating_sub(events.len()));
    }
    let a = receiver.as_ref().is_some_and(|l| run.contains(&Event::Activate(l.clone())));
    let d = sender.as_ref().is_some_and(|l| run.contains(&Event::Deactivate(l.clone())));
    let tail = events.split_off(at + 1);
    events.extend(run);
    events.extend(tail);
    res?;
    Ok(json!({"id": id, "activate": a, "deactivate": d}))
}

fn write(s: &mut Session, v: &Value) -> CmdResult {
    let i = seq_page(s, v)?;
    let grammar = req_str(v, "grammar")?;
    let (text, warnings) = match grammar {
        "mermaid" => {
            let page = s.doc.pages.get(i).ok_or_else(|| err("no such page"))?;
            vizcraft_formats::mermaid::sequence_write::write(page)
        }
        "plantuml" => vizcraft_formats::plantuml_sequence::write(&s.doc, i).map_err(|e| EngineError::Failed(e.to_string()))?,
        other => return Err(err(format!("`grammar` must be mermaid or plantuml (not `{}`)", short(other)))),
    };
    Ok(json!({"text": text, "warnings": warnings}))
}

#[cfg(test)]
#[path = "sequence_tests.rs"]
mod tests;
