//! A page's connector routing policy: whether every connector on the page avoids shapes, and what
//! a straight connector does when its straight line would cross one.
//!
//! The policy is page data (`Page.routing`), so it is saved with the file and undone like any
//! other change. Files without it load with the default, which routes every connector around
//! shapes; see [`crate::autoroute`] for what routing does with it.
//!
//! The clearance (how far routes keep from shapes, and how long the straight stub is that leaves
//! a shape before the first bend) is the page's too: unset, it follows the page's symbol grid
//! module when one is set, else [`DEFAULT_CLEARANCE`].

use serde::{Deserialize, Serialize};

use crate::model::{Connector, Page};
use crate::serde_util::lenient;

/// Which connectors on a page route around shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Avoid {
    /// Every connector avoids shapes. Unknown names load as this.
    #[default]
    Always,
    /// Only connectors with their own `avoid` flag set do (the behaviour before page policies).
    Never,
}

/// What a straight connector draws when no straight line between its connection points is clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StraightFallback {
    /// Draw the orthogonal detour around the shapes. Unknown names load as this.
    #[default]
    Detour,
    /// Keep the straight line (a circuit page wants hard straight wires); the crossing is reported
    /// by [`crate::autoroute::crossings`] instead.
    Keep,
}

crate::serde_util::named_enum!(Avoid, Avoid::Always, {
    Avoid::Always => "always",
    Avoid::Never => "never",
});

crate::serde_util::named_enum!(StraightFallback, StraightFallback::Detour, {
    StraightFallback::Detour => "detour",
    StraightFallback::Keep => "keep",
});

/// The clearance routes keep from shapes on a page with neither its own clearance nor a module.
pub const DEFAULT_CLEARANCE: f64 = 10.0;

/// Largest clearance a page may ask for; a larger one leaves no room between ordinary shapes.
pub const MAX_CLEARANCE: f64 = 100.0;

/// How connectors on a page are routed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RoutingPolicy {
    /// Which connectors avoid shapes.
    #[serde(deserialize_with = "lenient")]
    pub avoid: Avoid,
    /// What a straight connector does when its line would cross a shape.
    #[serde(deserialize_with = "lenient")]
    pub straight_fallback: StraightFallback,
    /// How far routes keep from shapes that are not their ends, and the least length of the stub
    /// that leaves an end shape, in page units; `None` (the default) follows the page's symbol
    /// grid module when one is set, else [`DEFAULT_CLEARANCE`].
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
}

impl RoutingPolicy {
    /// Drop a clearance that is not a finite number and clamp one out of `0..=MAX_CLEARANCE`.
    pub fn sanitize(&mut self) {
        self.clearance = self.clearance.filter(|c| c.is_finite()).map(|c| c.clamp(0.0, MAX_CLEARANCE));
    }

    /// Whether a connector whose own flag is `avoid` routes around shapes under this policy.
    pub fn routes(&self, avoid: bool) -> bool {
        self.avoid == Avoid::Always || avoid
    }
}

/// The clearance routes keep on `page`: its own, else its symbol grid module, else
/// [`DEFAULT_CLEARANCE`]; always finite and within `0..=MAX_CLEARANCE`.
pub fn clearance(page: &Page) -> f64 {
    let c = page.routing.clearance.or_else(|| crate::snap::module_step(page)).unwrap_or(DEFAULT_CLEARANCE);
    if c.is_finite() { c.clamp(0.0, MAX_CLEARANCE) } else { DEFAULT_CLEARANCE }
}

/// Whether `c` routes around shapes on `page`: the page policy says always, or the connector's
/// own flag is set. A net segment (a connector with `net` set) follows its own flag only, so a
/// wire drawn with bends keeps them even on a page whose policy is to always avoid; `wire.draw`
/// sets the flag on a wire drawn without bends on such a page, so it routes like any connector.
/// The sample lines of a generated key ([`crate::key`]) never route.
pub fn routes(page: &Page, c: &Connector) -> bool {
    // A generated key's sample lines are drawn where the key puts them, never routed.
    if crate::key::is_key_connector(c) {
        return false;
    }
    if c.net.is_some() || c.message.is_some() {
        return c.avoid;
    }
    page.routing.routes(c.avoid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_with_avoidance_on_and_junk_is_the_default() {
        let p: RoutingPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(p, RoutingPolicy { avoid: Avoid::Always, straight_fallback: StraightFallback::Detour, clearance: None });
        let p: RoutingPolicy = serde_json::from_str(r#"{"avoid": "never", "straightFallback": "keep"}"#).unwrap();
        assert_eq!((p.avoid, p.straight_fallback), (Avoid::Never, StraightFallback::Keep));
        let p: RoutingPolicy = serde_json::from_str(r#"{"avoid": 7, "straightFallback": "sideways"}"#).unwrap();
        assert_eq!(p, RoutingPolicy::default());
        assert_eq!(serde_json::to_value(p).unwrap(), serde_json::json!({"avoid": "always", "straightFallback": "detour"}));
        assert!(
            p.routes(false) && RoutingPolicy { avoid: Avoid::Never, ..p }.routes(true) && !RoutingPolicy { avoid: Avoid::Never, ..p }.routes(false)
        );
    }

    #[test]
    fn the_clearance_round_trips_is_sanitised_and_falls_back_to_the_module_then_ten() {
        let p: RoutingPolicy = serde_json::from_str(r#"{"clearance": 16}"#).unwrap();
        assert_eq!(p.clearance, Some(16.0));
        assert_eq!(serde_json::to_value(p).unwrap()["clearance"], 16.0);
        let p: RoutingPolicy = serde_json::from_str(r#"{"clearance": "wide"}"#).unwrap();
        assert_eq!(p.clearance, None);
        let mut p = RoutingPolicy { clearance: Some(1e9), ..RoutingPolicy::default() };
        p.sanitize();
        assert_eq!(p.clearance, Some(MAX_CLEARANCE));
        p.clearance = Some(f64::NAN);
        p.sanitize();
        assert_eq!(p.clearance, None);
        let mut page = Page::new("p", "P");
        assert_eq!(clearance(&page), DEFAULT_CLEARANCE);
        page.view = Some(vizcraft_model::View { module: vizcraft_model::Module::Custom(5.0), ..Default::default() });
        assert_eq!(clearance(&page), 5.0, "the module when the page has one");
        page.routing.clearance = Some(12.0);
        assert_eq!(clearance(&page), 12.0, "the page's own clearance wins");
    }

    #[test]
    fn net_segments_keep_their_bends_on_an_always_avoid_page() {
        let page = Page::new("p", "P");
        let mut c = Connector::default();
        assert!(routes(&page, &c), "the page policy routes plain connectors");
        c.net = Some("n1".into());
        assert!(!routes(&page, &c), "a segment keeps its drawn bends");
        c.avoid = true;
        assert!(routes(&page, &c), "unless it asks to avoid shapes");
    }
}
