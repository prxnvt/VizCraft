//! Connector commands: add, glue, route, arrowheads and text.
//!
//! An end is `{"shape": id, "point"?: index}` (glued; no point is dynamic glue, which picks the
//! connection point nearest the other end each time it routes) or `{"x": n, "y": n}` (free), the
//! same form the document stores. Glued ends are resolved from shape geometry on every draw, so
//! nothing here has to follow shapes around.

use serde_json::{Value, json};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{Arrow, Connector, End, Page, Route};

use super::{Kinds, has_connectors, num, opt_str, page_mut, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all connector commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector.add", "Add Connector", "Insert > Connector", add).params(
            r#"{"from": end, "to": end, "route"?: "straight"|"orthogonal"|"curved", "text"?: string, "startArrow"?: arrow, "endArrow"?: arrow} -> {"id"}; end = {"shape": id, "point"?: index} or {"x": n, "y": n}"#,
        ),
        CommandSpec::new("connector.glue", "Glue Connector", "Canvas", glue)
            .params(r#"{"id": string, "end": "start"|"end", "shape"?: id, "point"?: index, "x"?: n, "y"?: n}"#)
            .when(has_connectors),
        CommandSpec::new("connector.setRoute", "Connector Route", "Connector > Route", set_route)
            .params(r#"{"ids"?: [string], "route": "straight"|"orthogonal"|"curved"}"#)
            .when(has_connectors),
        CommandSpec::new("connector.setArrows", "Arrowheads", "Connector > Arrows", set_arrows)
            .params(r#"{"ids"?: [string], "start"?: arrow, "end"?: arrow}; arrow = "none"|"triangle"|"open"|"diamond"|"circle""#)
            .when(has_connectors),
        CommandSpec::new("connector.setText", "Edit Text", "Home > Font", set_text).params(r#"{"id": string, "text": string}"#).when(has_connectors),
    ]
}

/// Parse an end from `v`: `shape` (with optional `point`) makes a glued end, else `x` and `y` a
/// free one. A glued end must name a shape on `page` and, with a point, a valid point index.
fn parse_end(page: &Page, v: &Value, what: &str) -> Result<End, EngineError> {
    if !v.is_object() {
        return Err(EngineError::Params(format!("`{what}` must be {{\"shape\": id, \"point\"?: n}} or {{\"x\": n, \"y\": n}}")));
    }
    if let Some(shape) = opt_str(v, "shape")? {
        let Some(sh) = page.shape(shape) else {
            return Err(EngineError::Params(format!("{what}: `{}` is not a shape on this page", short(shape))));
        };
        let point = match v.get("point") {
            None | Some(Value::Null) => None,
            Some(x) => {
                let n = x.as_u64().ok_or_else(|| EngineError::Params(format!("{what}: `point` must be a connection point index")))?;
                let count = sh.connection_points().len();
                match usize::try_from(n) {
                    Ok(i) if i < count => Some(i),
                    _ => return Err(EngineError::Params(format!("{what}: `{}` has connection points 0 to {}", sh.id, count.saturating_sub(1)))),
                }
            }
        };
        return Ok(End::Glued { shape: sh.id.clone(), point });
    }
    match (num(v, "x")?, num(v, "y")?) {
        (Some(x), Some(y)) => Ok(End::Free { x: x.clamp(-MAX_COORD, MAX_COORD), y: y.clamp(-MAX_COORD, MAX_COORD) }),
        _ => Err(EngineError::Params(format!("{what}: give `shape` (and `point`) or `x` and `y`"))),
    }
}

fn route_param(v: &Value, k: &str) -> Result<Option<Route>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => Route::from_name(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}` must be straight, orthogonal or curved"))),
    }
}

fn arrow_param(v: &Value, k: &str) -> Result<Option<Arrow>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => Arrow::from_name(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}` must be none, triangle, open, diamond or circle"))),
    }
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let from = parse_end(page, v.get("from").unwrap_or(&Value::Null), "from")?;
    let to = parse_end(page, v.get("to").unwrap_or(&Value::Null), "to")?;
    let route = route_param(v, "route")?;
    let start_arrow = arrow_param(v, "startArrow")?;
    let end_arrow = arrow_param(v, "endArrow")?;
    let text = opt_str(v, "text")?;
    let id = s.doc.next_id("c");
    let mut c = Connector::new(id.clone(), from, to);
    if let Some(r) = route {
        c.route = r;
    }
    if let Some(a) = start_arrow {
        c.start_arrow = a;
    }
    if let Some(a) = end_arrow {
        c.end_arrow = a;
    }
    if let Some(t) = text {
        c.text = t.to_string();
    }
    c.sanitize();
    page_mut(s)?.connectors.push(c);
    select(s, vec![id.clone()]);
    with_sel(s, json!({"id": id}))
}

fn glue(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let which = p::req_str(v, "end")?;
    if !matches!(which, "start" | "end") {
        return Err(EngineError::Params("`end` must be \"start\" or \"end\"".into()));
    }
    let page = page_mut(s)?;
    if page.connector(id).is_none() {
        return Err(not_a_connector(id));
    }
    let end = parse_end(page, v, "glue")?;
    let c = page.connector_mut(id).ok_or_else(|| not_a_connector(id))?;
    if which == "start" {
        c.start = end;
    } else {
        c.end = end;
    }
    c.sanitize();
    let out = json!({"id": c.id, "start": c.start, "end": c.end});
    with_sel(s, out)
}

fn set_route(s: &mut Session, v: &Value) -> CmdResult {
    let route = route_param(v, "route")?.ok_or_else(|| EngineError::Params("`route` (string) is required".into()))?;
    let ids = targets(s, v, Kinds::Connectors)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            c.route = route;
        }
    }
    with_sel(s, json!({"ids": ids, "route": route.name()}))
}

fn set_arrows(s: &mut Session, v: &Value) -> CmdResult {
    let start = arrow_param(v, "start")?;
    let end = arrow_param(v, "end")?;
    if start.is_none() && end.is_none() {
        return Err(EngineError::Params("give `start`, `end` or both".into()));
    }
    let ids = targets(s, v, Kinds::Connectors)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            if let Some(a) = start {
                c.start_arrow = a;
            }
            if let Some(a) = end {
                c.end_arrow = a;
            }
        }
    }
    with_sel(s, json!({"ids": ids}))
}

fn set_text(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let text = p::req_str(v, "text")?;
    let c = page_mut(s)?.connector_mut(id).ok_or_else(|| not_a_connector(id))?;
    c.text = text.to_string();
    c.sanitize();
    let id = id.to_string();
    with_sel(s, json!({"id": id}))
}

fn not_a_connector(id: &str) -> EngineError {
    EngineError::Params(format!("`{}` is not a connector on this page", short(id)))
}
