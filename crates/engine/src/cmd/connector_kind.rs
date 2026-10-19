//! Connector kinds: `connector.setKind` draws connectors as a named kind from the pack catalog
//! (`vizcraft-stencils`), and `connector.kinds` lists and searches the catalog.
//!
//! A kind is a recipe, not a lock: applying it copies the route, ends, dash, width, line colour
//! and jumps onto the connector and records the id in `Connector.kind`; glue, text, bends and the
//! rest of the style stay, and later hand styling wins until the kind is applied again.
//! [`apply_kind`] is the one place that does it, shared with `connector.add {kind}`.

use serde_json::{Value, json};
use vizcraft_diagram::Connector;
use vizcraft_stencils::ConnectorKind;

use super::{Kinds, has_connectors, num, opt_str, page_mut, short, targets, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return the connector kind commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector.setKind", "Connector Kind", "Connector > Kind", set_kind)
            .params(r#"{"ids"?: [string], "kind": string} -> {"ids", "kind"}; applies the kind's route, ends, dash, width, line colour and jumps and keeps glue, text and bends; "" only forgets the kind name; default: the selected connectors; kinds are listed by connector.kinds"#)
            .when(has_connectors),
        CommandSpec::new("connector.kinds", "Connector Kinds", "Connector > Kind", kinds).params(
            r#"{"query"?: string, "limit"?: n (default 50, at most 200)} -> {"kinds": [{"id", "label", "category", "route", "startArrow", "endArrow", "dash", "lineWidth", "line", "jumps", "relationship"?}]}; relationship is the relationship type a connector of the kind becomes between bound shapes"#,
        ),
    ]
}

/// Results returned when `limit` is not given.
const DEFAULT_LIMIT: usize = 50;
/// Most results one listing returns, whatever `limit` asks for.
const MAX_LIMIT: usize = 200;
/// Longest query looked at; the rest is ignored.
const MAX_QUERY_CHARS: usize = 1000;

/// Draw `c` as the connector kind `kind`: route, ends, dash, width, line colour and jumps come
/// from the kind and `Connector.kind` records its id. An empty `kind` only forgets the name and
/// leaves the look. Fails, leaving `c` as it was, for a kind the catalog does not know.
pub fn apply_kind(c: &mut Connector, kind: &str) -> Result<(), EngineError> {
    if kind.is_empty() {
        c.kind.clear();
        return Ok(());
    }
    let k = vizcraft_stencils::connector_kind(kind).ok_or_else(|| EngineError::Params(format!("unknown connector kind `{}`", short(kind))))?;
    k.apply(c);
    Ok(())
}

fn set_kind(s: &mut Session, v: &Value) -> CmdResult {
    let kind = p::req_str(v, "kind")?;
    // Checked before anything changes, so an unknown kind leaves every connector alone.
    if !kind.is_empty() && vizcraft_stencils::connector_kind(kind).is_none() {
        return Err(EngineError::Params(format!("unknown connector kind `{}`", short(kind))));
    }
    let ids = targets(s, v, Kinds::Connectors)?;
    let page = page_mut(s)?;
    for id in &ids {
        if let Some(c) = page.connector_mut(id) {
            apply_kind(c, kind)?;
        }
    }
    with_sel(s, json!({"ids": ids, "kind": kind}))
}

/// A kind as the listing reports it.
fn kind_json(k: &ConnectorKind) -> Value {
    let mut out = json!({
        "id": k.id,
        "label": k.label,
        "category": k.category,
        "route": k.route.name(),
        "startArrow": k.start_arrow.name(),
        "endArrow": k.end_arrow.name(),
        "dash": k.dash.name(),
        "lineWidth": k.line_width,
        "line": k.line.to_hex(),
        "jumps": k.jumps,
    });
    if let (Some(rel), Some(map)) = (k.relationship_type, out.as_object_mut()) {
        map.insert("relationship".into(), json!(rel));
    }
    out
}

fn kinds(_s: &mut Session, v: &Value) -> CmdResult {
    let query: String = opt_str(v, "query")?.unwrap_or("").chars().take(MAX_QUERY_CHARS).collect();
    let limit = match num(v, "limit")? {
        None => DEFAULT_LIMIT,
        // Clamped as a float first so a huge or negative number cannot overflow the cast.
        Some(n) => n.clamp(1.0, MAX_LIMIT as f64) as usize,
    };
    let kinds: Vec<Value> = vizcraft_stencils::search_connector_kinds(&query, limit).into_iter().map(kind_json).collect();
    Ok(json!({"kinds": kinds}))
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::{Arrow, Dash, End, Point, Route};

    use super::*;

    fn session() -> Session {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        for (id, x) in [("s1", 0.0), ("s2", 300.0)] {
            s.run("shape.add", &json!({"stencil": "flowchart.process", "x": x, "y": 0, "id": id})).unwrap();
        }
        s.run("connector.add", &json!({"from": {"shape": "s1"}, "to": {"shape": "s2"}, "text": "go", "id": "c1"})).unwrap();
        s
    }

    fn connector(s: &Session) -> Connector {
        s.doc.page().unwrap().connector("c1").unwrap().clone()
    }

    #[test]
    fn set_kind_applies_the_look_and_keeps_glue_text_and_bends() {
        let mut s = session();
        s.doc.page_mut().unwrap().connector_mut("c1").unwrap().waypoints = vec![Point::new(150.0, 90.0)];
        s.run("connector.setKind", &json!({"ids": ["c1"], "kind": "basic.dependency"})).unwrap();
        let c = connector(&s);
        assert_eq!(c.kind, "basic.dependency");
        assert_eq!((c.route, c.start_arrow, c.end_arrow), (Route::Orthogonal, Arrow::None, Arrow::Open));
        assert_eq!(c.style.dash, Dash::Dashed);
        assert_eq!(c.text, "go");
        assert_eq!(c.waypoints, vec![Point::new(150.0, 90.0)]);
        assert_eq!((c.start.shape(), c.end.shape()), (Some("s1"), Some("s2")));
        assert!(matches!(c.end, End::Glued { .. }));
        // Heavy and light kinds change the width; the selection is the default target.
        s.run("connector.setKind", &json!({"kind": "basic.bus"})).unwrap();
        assert_eq!(connector(&s).style.line_width, 3.0);
        assert_eq!(connector(&s).style.dash, Dash::Solid);
        // Undo restores the connector as it was.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(connector(&s).kind, "basic.dependency");
    }

    #[test]
    fn set_kind_refuses_unknown_kinds_without_changing_anything() {
        let mut s = session();
        let before = connector(&s);
        assert!(s.run("connector.setKind", &json!({"ids": ["c1"], "kind": "vendor.nothing"})).is_err());
        assert!(s.run("connector.setKind", &json!({"ids": ["c1"]})).is_err());
        assert!(s.run("connector.setKind", &json!({"ids": ["s1"], "kind": "basic.arrow"})).is_err());
        assert_eq!(connector(&s), before);
        // An empty kind only forgets the name.
        s.run("connector.setKind", &json!({"ids": ["c1"], "kind": "basic.bus"})).unwrap();
        s.run("connector.setKind", &json!({"ids": ["c1"], "kind": ""})).unwrap();
        let c = connector(&s);
        assert_eq!((c.kind.as_str(), c.style.line_width), ("", 3.0));
    }

    #[test]
    fn connector_add_takes_a_kind_and_explicit_params_win_over_it() {
        let mut s = session();
        s.run("connector.add", &json!({"from": {"shape": "s1"}, "to": {"shape": "s2"}, "kind": "basic.dependency", "id": "c2"})).unwrap();
        let c = s.doc.page().unwrap().connector("c2").unwrap().clone();
        assert_eq!((c.kind.as_str(), c.end_arrow, c.style.dash), ("basic.dependency", Arrow::Open, Dash::Dashed));
        s.run("connector.add", &json!({"from": {"shape": "s1"}, "to": {"shape": "s2"}, "kind": "basic.dependency", "endArrow": "triangle", "route": "straight", "id": "c3"}))
            .unwrap();
        let c = s.doc.page().unwrap().connector("c3").unwrap().clone();
        assert_eq!((c.kind.as_str(), c.end_arrow, c.route), ("basic.dependency", Arrow::Triangle, Route::Straight));
        assert!(s.run("connector.add", &json!({"from": {"shape": "s1"}, "to": {"shape": "s2"}, "kind": "vendor.nothing"})).is_err());
        assert!(s.doc.page().unwrap().connector("c4").is_none());
    }

    #[test]
    fn apply_kind_is_what_connector_add_needs() {
        let mut c = Connector::new("c", End::free(Point::ZERO), End::free(Point::new(5.0, 5.0)));
        apply_kind(&mut c, "basic.signal").unwrap();
        assert_eq!((c.kind.as_str(), c.start_arrow, c.end_arrow), ("basic.signal", Arrow::Circle, Arrow::Open));
        let kept = c.clone();
        assert!(apply_kind(&mut c, "nope").is_err());
        assert_eq!(c, kept);
    }

    #[test]
    fn kinds_lists_and_searches_the_catalog() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        let all = s.run("connector.kinds", &json!({})).unwrap();
        let all = all["kinds"].as_array().unwrap();
        assert_eq!(all.iter().filter(|k| k["id"].as_str().is_some_and(|i| i.starts_with("basic.") || i.starts_with("flowchart."))).count(), 7);
        assert_eq!(all[0]["id"], "basic.line");
        let flow = all.iter().find(|k| k["id"] == "flowchart.flow").unwrap();
        assert_eq!((flow["endArrow"].as_str(), flow["relationship"].as_str()), (Some("triangle"), Some("flowchart.Flow")));
        assert!(all[0].get("relationship").is_none());
        let bus = s.run("connector.kinds", &json!({"query": "bus", "limit": 5})).unwrap();
        assert_eq!(bus["kinds"][0]["id"], "basic.bus");
        assert_eq!(bus["kinds"][0]["lineWidth"], 3.0);
        assert_eq!(s.run("connector.kinds", &json!({"limit": 2})).unwrap()["kinds"].as_array().unwrap().len(), 2);
        assert_eq!(s.run("connector.kinds", &json!({"limit": -4})).unwrap()["kinds"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn the_flow_kind_between_flowchart_shapes_creates_a_flow_relationship() {
        let mut s = Session::new(vizcraft_diagram::Document::new());
        for (id, x) in [("s1", 0.0), ("s2", 300.0)] {
            s.run("shape.add", &json!({"stencil": "flowchart.process", "x": x, "y": 0, "id": id})).unwrap();
        }
        s.run("connector.add", &json!({"from": {"shape": "s1"}, "to": {"shape": "s2"}, "id": "c1"})).unwrap();
        s.run("connector.setKind", &json!({"ids": ["c1"], "kind": "flowchart.flow"})).unwrap();
        let c = connector(&s);
        let rid = c.relationship.expect("the flow kind binds a relationship");
        assert_eq!(s.doc.model.relationship(&rid).unwrap().type_id, "flowchart.Flow");
    }
}
