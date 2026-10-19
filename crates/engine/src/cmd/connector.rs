//! Connector commands: add, glue, route, arrowheads and text.
//!
//! An end is `{"shape": id, "point"?: index}` (glued; no point is dynamic glue, which picks the
//! connection point nearest the other end each time it routes) or `{"x": n, "y": n}` (free), the
//! same form the document stores. Glued ends are resolved from shape geometry on every draw, so
//! nothing here has to follow shapes around.

use serde_json::{Value, json};
use vizcraft_diagram::geom::MAX_COORD;
use vizcraft_diagram::{Arrow, Connector, End, Page, Route};

use super::{Kinds, has_connectors, ids_param, num, opt_bool, opt_str, page_mut, select, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all connector commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector.add", "Add Connector", "Insert > Connector", add).params(
            r#"{"from": end, "to": end, "route"?: "straight"|"orthogonal"|"curved", "text"?: string, "startArrow"?: arrow, "endArrow"?: arrow, "kind"?: string (a connector kind id, see connector.kinds), "id"?: string} -> {"id"}; end = {"shape": id, "point"?: index, "port"?: "<compartment>.left"|"<compartment>.right"} or {"x": n, "y": n}"#,
        ),
        CommandSpec::new("connector.glue", "Glue Connector", "Canvas", glue)
            .params(r#"{"id": string, "end": "start"|"end", "shape"?: id, "point"?: index, "x"?: n, "y"?: n}"#)
            .when(has_connectors),
        CommandSpec::new("connector.setRoute", "Connector Route", "Connector > Route", set_route)
            .params(r#"{"ids"?: [string], "route": "straight"|"orthogonal"|"curved"}"#)
            .when(has_connectors),
        CommandSpec::new("connector.setArrows", "Arrowheads", "Connector > Arrows", set_arrows)
            .params(r#"{"ids"?: [string], "start"?: arrow, "end"?: arrow}; arrow = "none"|"triangle"|"open"|"diamond"|"circle"|"hollowTriangle"|"hollowDiamond"|"bar"|"openCircle"|"crowsFoot"|"crowsFootBar"|"crowsFootCircle"|"barCircle"|"slash""#)
            .when(has_connectors),
        CommandSpec::new("connector.reroute", "Avoid Shapes", "Design > Connectors", reroute)
            .params(r#"{"ids"?: [string], "avoid"?: bool (default true)} -> {"ids", "avoid"}; true routes orthogonally around shapes and keeps the route clear after every change, false clears the stored bends; default: the selected connectors, else all"#)
            .when(has_connectors),
        CommandSpec::new("connector.setJumps", "Line Jumps", "Design > Connectors", set_jumps)
            .params(r#"{"ids"?: [string], "on": bool} -> {"ids", "on"}; draw a hop where the connector crosses one drawn before it; default: the selected connectors, else all"#)
            .when(has_connectors),
        CommandSpec::new("connector.setEndLabels", "End Labels", "Connector > End Labels", set_end_labels)
            .params(r#"{"ids"?: [string], "start"?: string or number, "end"?: string or number} -> {"ids"}; the text beside each end (a cardinality such as "1" or "N", a multiplicity such as "0..*"), drawn a little outside the line; "" or null clears one; default: the selected connectors"#)
            .when(has_connectors),
        CommandSpec::new("connector.setText", "Edit Text", "Home > Font", set_text).params(r#"{"id": string, "text": string}; on a connector of a relationship, renames the relationship and every view of it"#).when(has_connectors),
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
        super::layer::ensure_gluable(page, sh, what)?;
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
        // A compartment's point is glued by its port, so the end follows the band.
        let port = match opt_str(v, "port")? {
            Some(p) => match vizcraft_diagram::compartment::port_index(sh, p) {
                Some(_) => Some(p.to_string()),
                None => return Err(EngineError::Params(format!("{what}: `{}` is not a port of `{}`", short(p), sh.id))),
            },
            None => point.and_then(|i| vizcraft_diagram::compartment::port_for_index(sh, i)),
        };
        let point = if port.is_some() { None } else { point };
        return Ok(End::Glued { shape: sh.id.clone(), point, port });
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
    let kind = opt_str(v, "kind")?;
    let id = super::new_id(s, v, "c")?;
    let mut c = Connector::new(id.clone(), from, to);
    // A kind supplies the defaults; explicit route and arrow params below still win over it.
    if let Some(k) = kind {
        super::connector_kind::apply_kind(&mut c, k)?;
    }
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
    super::protect::check(s, super::opt_str(v, "shape")?, super::protect::Lock::Glue)?;
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

/// An end label param: absent is `None` (leave it), text or a number is the new label, null is
/// empty (clear it).
fn end_label_param(v: &Value, k: &str) -> Result<Option<String>, EngineError> {
    match v.get(k) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(String::new())),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        // A ribbon field sends "1" as the number 1.0; it reads back as written.
        Some(Value::Number(n)) => Ok(Some(match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        })),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be text, a number or null"))),
    }
}

fn set_end_labels(s: &mut Session, v: &Value) -> CmdResult {
    let start = end_label_param(v, "start")?;
    let end = end_label_param(v, "end")?;
    if start.is_none() && end.is_none() {
        return Err(EngineError::Params("give `start`, `end` or both".into()));
    }
    let ids = targets(s, v, Kinds::Connectors)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            if let Some(t) = &start {
                c.start_label = t.trim().to_string();
            }
            if let Some(t) = &end {
                c.end_label = t.trim().to_string();
            }
            c.sanitize();
        }
    }
    with_sel(s, json!({"ids": ids}))
}

fn set_text(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let text = p::req_str(v, "text")?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    page.connector(id).ok_or_else(|| not_a_connector(id))?;
    // The shared setter renames the relationship when the connector draws one, so every view follows.
    super::text::put_text(s, id, text)?;
    let id = id.to_string();
    with_sel(s, json!({"id": id}))
}

/// The connectors a whole-connector command acts on: `ids` when given (each must be a connector),
/// else the selected connectors, else every connector on the page, so the Design tab's toggles
/// work with nothing selected.
pub fn connector_targets(s: &Session, v: &Value) -> Result<Vec<String>, EngineError> {
    if ids_param(v)?.is_some() {
        return targets(s, v, Kinds::Connectors);
    }
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let mut out: Vec<String> = Vec::new();
    for id in &s.sel.items {
        if page.connector(id).is_some() && !out.contains(id) {
            out.push(id.clone());
        }
    }
    if out.is_empty() {
        out = page.connectors.iter().map(|c| c.id.clone()).collect();
    }
    if out.is_empty() {
        return Err(EngineError::Params("add a connector first".into()));
    }
    Ok(out)
}

fn reroute(s: &mut Session, v: &Value) -> CmdResult {
    let avoid = opt_bool(v, "avoid")?.unwrap_or(true);
    let ids = connector_targets(s, v)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            c.avoid = avoid;
            if avoid {
                c.route = Route::Orthogonal;
            } else {
                c.waypoints.clear();
            }
        }
    }
    if avoid {
        vizcraft_diagram::autoroute::reroute(page, Some(&ids));
    }
    with_sel(s, json!({"ids": ids, "avoid": avoid}))
}

fn set_jumps(s: &mut Session, v: &Value) -> CmdResult {
    let on = opt_bool(v, "on")?.ok_or_else(|| EngineError::Params("`on` (true or false) is required".into()))?;
    let ids = connector_targets(s, v)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            c.jumps = on;
        }
    }
    with_sel(s, json!({"ids": ids, "on": on}))
}

fn not_a_connector(id: &str) -> EngineError {
    EngineError::Params(format!("`{}` is not a connector on this page", short(id)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Document, Shape, ShapeKind};

    use crate::Session;

    #[test]
    fn set_end_labels_sets_clears_and_refuses_junk() {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 60.0));
        let mut s = Session::new(doc);
        let c = s.run("connector.add", &json!({"from": {"shape": "a"}, "to": {"shape": "b"}})).unwrap()["id"].as_str().unwrap().to_string();
        s.run("connector.setEndLabels", &json!({"ids": [c], "start": "1", "end": "N"})).unwrap();
        let get = |s: &Session| {
            let k = s.doc.pages[0].connector(&c).unwrap();
            (k.start_label.clone(), k.end_label.clone())
        };
        assert_eq!(get(&s), ("1".to_string(), "N".to_string()));
        // A number from a ribbon field reads as written; null clears; an absent end stays.
        s.run("connector.setEndLabels", &json!({"ids": [c], "start": 1.0})).unwrap();
        s.run("connector.setEndLabels", &json!({"ids": [c], "end": null})).unwrap();
        assert_eq!(get(&s), ("1".to_string(), String::new()));
        s.run("connector.setEndLabels", &json!({"ids": [c], "end": "0..*".repeat(100_000)})).unwrap();
        assert!(get(&s).1.len() <= vizcraft_diagram::MAX_TEXT, "capped like the label");
        for bad in [json!({"ids": [c]}), json!({"ids": [c], "start": []}), json!({"ids": ["nope"], "start": "1"}), json!({"ids": [c], "end": {}})] {
            assert!(s.run("connector.setEndLabels", &bad).is_err(), "{bad}");
        }
    }
}
