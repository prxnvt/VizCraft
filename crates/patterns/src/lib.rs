//! VizCraft patterns (model layer, below the engine): generators with typed parameters that emit
//! model elements, not bare shapes.
//!
//! - [`schema`]: [`Pattern`], its [`Param`]s and [`ParamKind`]s, and [`Args`] checked against them.
//! - [`emit`]: [`generate`], pure, from a pattern and its arguments to an [`Emission`] of elements
//!   and relationships keyed by role, containment, regions, a layout primitive and the generated
//!   names.
//! - [`workflow`]: the starter workflow patterns over the Flowchart pack.
//!
//! The engine's `pattern.*` commands turn an emission into elements, shapes and connectors in one
//! undo step and record the instance (`vizcraft_model::PatternInstance`) so it can be
//! regenerated with other arguments. A pack lists the patterns it offers in its `patterns` field;
//! [`available`] keeps the patterns whose pack is loaded and lists them.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod emit;
pub mod schema;
pub mod workflow;

use std::sync::OnceLock;

pub use emit::{Emission, EmittedElement, EmittedRelationship, Region, generate};
pub use schema::{Args, Param, ParamKind, Pattern};
use vizcraft_notation::Catalog;

/// Most elements, relationships and regions one generation emits.
pub const MAX_EMITTED: usize = 1024;

/// Why a pattern could not be generated.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PatternError {
    /// No pattern with this id.
    #[error("no pattern `{0}`")]
    Unknown(String),
    /// The arguments do not fit the schema; the text says which and why.
    #[error("{0}")]
    Args(String),
    /// The pattern needs a pack or type the catalog does not have.
    #[error("{0}")]
    Pack(String),
    /// The pattern's own generator produced something inconsistent (a bug in the pattern).
    #[error("{0}")]
    Pattern(String),
}

/// A built-in pattern and its generator.
#[derive(Clone, Debug)]
pub struct Builtin {
    /// The pattern and its schema.
    pub pattern: Pattern,
    /// Builds the emission from checked arguments; the caller validates it.
    pub generate: fn(&Args) -> Result<Emission, PatternError>,
}

/// Every built-in pattern, in listing order.
pub fn builtins() -> &'static [Builtin] {
    static ALL: OnceLock<Vec<Builtin>> = OnceLock::new();
    ALL.get_or_init(workflow::builtins)
}

/// The built-in pattern with this id.
pub fn builtin(id: &str) -> Option<&'static Builtin> {
    builtins().iter().find(|b| b.pattern.id == id)
}

/// The pattern with this id.
pub fn find(id: &str) -> Option<&'static Pattern> {
    builtin(id).map(|b| &b.pattern)
}

/// The patterns `cat` offers: each built-in whose pack is loaded and lists it in its `patterns`.
pub fn available(cat: &Catalog) -> Vec<&'static Pattern> {
    builtins().iter().map(|b| &b.pattern).filter(|p| cat.pack(&p.pack).is_some_and(|pk| pk.patterns.contains(&p.id))).collect()
}

/// [`available`] filtered: `pack` keeps one pack's patterns, `audience` those serving it.
pub fn list(cat: &Catalog, pack: Option<&str>, audience: Option<&str>) -> Vec<&'static Pattern> {
    available(cat)
        .into_iter()
        .filter(|p| pack.is_none_or(|k| p.pack == k))
        .filter(|p| audience.is_none_or(|a| p.audience.iter().any(|x| x == a)))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_starter_pattern_generates_valid_emissions_at_its_bounds() {
        let cat = vizcraft_notation::catalog();
        let listed = available(cat);
        assert_eq!(listed.len(), 5, "the Flowchart pack lists the five starter patterns");
        for p in listed {
            for args in [json!({}), p.defaults().to_value()] {
                let a = p.check(&args).unwrap();
                let e = generate(p, &a, cat).unwrap_or_else(|e| panic!("{}: {e}", p.id));
                assert!(!e.elements.is_empty(), "{}", p.id);
            }
            // Every count at its largest.
            let mut big = p.defaults();
            for q in &p.params {
                if let ParamKind::Count { max, .. } = q.kind {
                    big.0.insert(q.key.clone(), json!(max));
                }
            }
            let a = p.check(&big.to_value()).unwrap();
            generate(p, &a, cat).unwrap_or_else(|e| panic!("{} at its largest: {e}", p.id));
        }
    }

    #[test]
    fn roles_are_stable_across_counts() {
        let cat = vizcraft_notation::catalog();
        let p = find("workflow.parallelSplit").unwrap();
        let four = generate(p, &p.check(&json!({"branches": 4})).unwrap(), cat).unwrap();
        let six = generate(p, &p.check(&json!({"branches": 6})).unwrap(), cat).unwrap();
        for e in &four.elements {
            assert!(six.element_by_role(&e.role).is_some(), "{}", e.role);
        }
        assert_eq!(six.elements.len(), four.elements.len() + 2);
        assert_eq!(six.generated()["branch.5"], "Branch 5");
    }

    #[test]
    fn n_of_m_refuses_more_required_than_branches_and_names_the_join() {
        let cat = vizcraft_notation::catalog();
        let p = find("workflow.nOfMJoin").unwrap();
        let e = generate(p, &p.check(&json!({"branches": 3, "required": 2})).unwrap(), cat).unwrap();
        assert_eq!(e.element_by_role("join").unwrap().name, "2 of 3");
        assert!(generate(p, &p.check(&json!({"branches": 2, "required": 3})).unwrap(), cat).is_err());
    }

    #[test]
    fn filters_by_pack_and_audience() {
        let cat = vizcraft_notation::catalog();
        assert_eq!(list(cat, Some("flowchart"), Some("general")).len(), 5);
        assert!(list(cat, Some("basic"), None).is_empty());
        assert!(list(cat, None, Some("embedded")).is_empty());
    }
}
