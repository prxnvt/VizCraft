//! The page routing policy command (`page.setRouting`): whether every connector on a page routes
//! around shapes, and what a straight connector does when its line would cross one. The policy is
//! page data (`vizcraft_diagram::RoutingPolicy`); the session's reroute after the command applies
//! it to every connector on the page in the same undo step.

use serde_json::{Value, json};
use vizcraft_diagram::{Avoid, Page, RoutingPolicy, StraightFallback};

use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The routing commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("page.setRouting", "Connector Routing", "Design > Connectors", set_routing).params(concat!(
        r#"{"page"?: id|name|index (default the active page), "avoid"?: "always"|"never", "straightFallback"?: "detour"|"keep"} -> {page, avoid, straightFallback}"#,
        r#"; always (the default) routes every connector on the page around shapes, never leaves it to each connector's own Avoid Shapes; a straight connector whose line would cross a shape draws the orthogonal detour, or with keep stays straight and is reported"#
    ))]
}

/// Apply the `avoid` and `straightFallback` keys of `v` (both optional) to `page` and return the
/// policy it ends with; `page.setup {routing}` shares it. Turning avoidance off for the page drops
/// the stored routes of connectors that do not avoid shapes on their own, since those routes were
/// the page's, not the user's.
pub fn apply_routing(page: &mut Page, v: &Value) -> Result<RoutingPolicy, EngineError> {
    let mut policy = page.routing;
    if let Some(t) = opt_str(v, "avoid")? {
        policy.avoid =
            Avoid::from_name(t).ok_or_else(|| EngineError::Params(format!("`avoid` must be \"always\" or \"never\", not `{}`", short(t))))?;
    }
    if let Some(t) = opt_str(v, "straightFallback")? {
        policy.straight_fallback = StraightFallback::from_name(t)
            .ok_or_else(|| EngineError::Params(format!("`straightFallback` must be \"detour\" or \"keep\", not `{}`", short(t))))?;
    }
    if page.routing.avoid == Avoid::Always && policy.avoid == Avoid::Never {
        for c in page.connectors.iter_mut().filter(|c| !c.avoid) {
            c.waypoints.clear();
        }
    }
    page.routing = policy;
    Ok(policy)
}

/// The page the `page` param names (id, name or index), else the active page.
fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let r = match v.get("page") {
        None | Some(Value::Null) => {
            return if s.doc.pages.is_empty() { Err(EngineError::Failed("the document has no page".into())) } else { Ok(s.doc.active_index()) };
        }
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(EngineError::Params("`page` must be a page id, name or index".into())),
    };
    s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))
}

fn set_routing(s: &mut Session, v: &Value) -> CmdResult {
    let index = page_index(s, v)?;
    let page = s.doc.pages.get_mut(index).ok_or_else(|| EngineError::Failed("no such page".into()))?;
    let policy = apply_routing(page, v)?;
    Ok(json!({"page": page.id, "avoid": policy.avoid.name(), "straightFallback": policy.straight_fallback.name()}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::autoroute::crossings;
    use vizcraft_diagram::{Avoid, Connector, Document, End, Page, Route, Shape, ShapeKind, StraightFallback};

    use crate::Session;

    /// Two shapes with a wall between them and a straight connector across the wall.
    fn walled() -> Session {
        let mut page = Page::new("p1", "Page-1");
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 0.0, 100.0, 60.0, 40.0));
        page.shapes.push(Shape::new("wall", ShapeKind::Rectangle, 150.0, 0.0, 40.0, 260.0));
        page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 100.0, 60.0, 40.0));
        let mut c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        c.route = Route::Straight;
        page.connectors.push(c);
        Session::new(Document { pages: vec![page], ..Document::default() })
    }

    #[test]
    fn set_routing_switches_the_page_policy_in_one_undo_step() {
        let mut s = walled();
        // Any change reroutes: the straight line across the wall becomes a detour.
        s.run("page.setRouting", &json!({"straightFallback": "keep"})).unwrap();
        assert_eq!(s.doc.page().unwrap().routing.straight_fallback, StraightFallback::Keep);
        assert!(!crossings(s.doc.page().unwrap()).is_empty(), "a kept straight line is reported");
        let out = s.run("page.setRouting", &json!({"page": "Page-1", "straightFallback": "detour"})).unwrap();
        assert_eq!(out, json!({"page": "p1", "avoid": "always", "straightFallback": "detour"}));
        assert!(crossings(s.doc.page().unwrap()).is_empty(), "the detour goes around the wall");
        assert!(!s.doc.page().unwrap().connector("c").unwrap().waypoints.is_empty());
        let out = s.run("page.setRouting", &json!({"page": 0, "avoid": "never"})).unwrap();
        assert_eq!(out["avoid"], "never");
        let page = s.doc.page().unwrap();
        assert_eq!(page.routing.avoid, Avoid::Never);
        assert!(page.connector("c").unwrap().waypoints.is_empty(), "the page's route is dropped");
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.page().unwrap().routing.avoid, Avoid::Always);
        assert!(crossings(s.doc.page().unwrap()).is_empty());
    }

    #[test]
    fn bad_params_are_refused() {
        let mut s = walled();
        for bad in [json!({"avoid": "sometimes"}), json!({"straightFallback": 3}), json!({"page": "nope"}), json!({"page": []})] {
            assert!(s.run("page.setRouting", &bad).is_err(), "{bad}");
        }
        assert_eq!(s.doc.page().unwrap().routing, vizcraft_diagram::RoutingPolicy::default());
    }
}
