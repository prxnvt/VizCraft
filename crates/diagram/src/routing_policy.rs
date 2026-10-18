//! A page's connector routing policy: whether every connector on the page avoids shapes, and what
//! a straight connector does when its straight line would cross one.
//!
//! The policy is page data (`Page.routing`), so it is saved with the file and undone like any
//! other change. Files without it load with the default, which routes every connector around
//! shapes; see [`crate::autoroute`] for what routing does with it.

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

/// How connectors on a page are routed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RoutingPolicy {
    /// Which connectors avoid shapes.
    #[serde(deserialize_with = "lenient")]
    pub avoid: Avoid,
    /// What a straight connector does when its line would cross a shape.
    #[serde(deserialize_with = "lenient")]
    pub straight_fallback: StraightFallback,
}

impl RoutingPolicy {
    /// Nothing to clamp today; kept so `Page::sanitize` treats the policy like every other field.
    pub fn sanitize(&mut self) {}

    /// Whether a connector whose own flag is `avoid` routes around shapes under this policy.
    pub fn routes(&self, avoid: bool) -> bool {
        self.avoid == Avoid::Always || avoid
    }
}

/// Whether `c` routes around shapes on `page`: the page policy says always, or the connector's
/// own flag is set.
pub fn routes(page: &Page, c: &Connector) -> bool {
    page.routing.routes(c.avoid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_with_avoidance_on_and_junk_is_the_default() {
        let p: RoutingPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(p, RoutingPolicy { avoid: Avoid::Always, straight_fallback: StraightFallback::Detour });
        let p: RoutingPolicy = serde_json::from_str(r#"{"avoid": "never", "straightFallback": "keep"}"#).unwrap();
        assert_eq!((p.avoid, p.straight_fallback), (Avoid::Never, StraightFallback::Keep));
        let p: RoutingPolicy = serde_json::from_str(r#"{"avoid": 7, "straightFallback": "sideways"}"#).unwrap();
        assert_eq!(p, RoutingPolicy::default());
        assert_eq!(serde_json::to_value(p).unwrap(), serde_json::json!({"avoid": "always", "straightFallback": "detour"}));
        assert!(
            p.routes(false) && RoutingPolicy { avoid: Avoid::Never, ..p }.routes(true) && !RoutingPolicy { avoid: Avoid::Never, ..p }.routes(false)
        );
    }
}
