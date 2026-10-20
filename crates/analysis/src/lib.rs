//! VizCraft graph analysis over the semantic model (model layer, below the engine).
//!
//! An analysis reads a [`Model`] through a [`Context`] and returns an [`AnalysisResult`]: the
//! element and relationship ids it found, a [`Mark`] for each so a view can highlight them,
//! named groups and an optional table. Nothing here changes the model.
//!
//! - [`graph`]: the indexed [`Graph`] every analysis walks, built from a model under an
//!   [`EdgeFilter`] or from plain links ([`Graph::from_edges`]) for nets and pins.
//! - [`scc`], [`reach`], [`cut`], [`paths`], [`metrics`]: the algorithms. Each is iterative and
//!   linear in the graph, so a 100,000-element chain cannot overflow the stack, and each returns
//!   its output in model order.
//! - [`diff`]: a symmetric, id-keyed compare of two models.
//! - [`builtin`]: the six built-in analyses behind the [`Analysis`] trait.
//! - [`mech`] and [`reliability`]: the mechanical (mobility, ratio, Mason) and reliability (cut
//!   sets, availability, outcomes) analyses of the engineering packs, which the notation rules
//!   call too.
//!
//! Pack knowledge (subtypes, inverse relationship types) comes in through the [`Types`] trait, so
//! this crate does not depend on the notation catalog; the engine implements it over the catalog.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod builtin;
pub mod cut;
pub mod diff;
pub mod graph;
pub mod mech;
pub mod metrics;
pub mod paths;
pub mod reach;
pub mod reliability;
mod result;
pub mod scc;
#[cfg(test)]
mod tests;

use serde_json::Value;
pub use vizcraft_model::Model;

pub use diff::{ModelDiff, diff};
pub use graph::{Direction, EdgeFilter, Graph, Link};
pub use result::{AnalysisError, AnalysisResult, Group, Mark, ResultTable, TableRow};

/// Deepest walk a `depth` param asks for.
pub const MAX_DEPTH_PARAM: u32 = 1024;

/// Most ids one result list holds; past it the result says `truncated`.
pub const MAX_RESULT_IDS: usize = 10_000;

/// Most type ids one `types` or `elements` param names.
pub const MAX_TYPES: usize = 64;

/// What an analysis knows about types beyond their ids: the notation catalog, for the engine.
pub trait Types {
    /// Whether `ty` is `ancestor` or one of its subtypes.
    fn is_a(&self, ty: &str, ancestor: &str) -> bool;
    /// The relationship types that count as `rel` followed backwards.
    fn inverses(&self, rel: &str) -> Vec<String>;
}

/// No pack knowledge: a type is only itself and nothing is an inverse.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTypes;

impl Types for NoTypes {
    fn is_a(&self, ty: &str, ancestor: &str) -> bool {
        ty == ancestor
    }
    fn inverses(&self, _rel: &str) -> Vec<String> {
        Vec::new()
    }
}

/// What an analysis reads. Passed by reference so later fields (the net index) can be added
/// without changing the trait.
pub struct Context<'a> {
    /// The model analysed.
    pub model: &'a Model,
    /// Subtypes and inverses.
    pub types: &'a dyn Types,
}

/// One analysis: an id, a label, a params description and a run over a context. Later packs add
/// analyses by appending one line to [`analyses`].
pub trait Analysis {
    /// The id (`cycles`); the engine's command is `analyze.<id>`.
    fn id(&self) -> &'static str;
    /// The label a menu shows.
    fn label(&self) -> &'static str;
    /// The params, in the command registry's params style.
    fn params(&self) -> &'static str;
    /// Run over `cx` with `params` (a JSON object; element ids already resolved from shape ids).
    fn run(&self, cx: &Context, params: &Value) -> Result<AnalysisResult, AnalysisError>;
}

/// Every analysis, built-ins first.
pub fn analyses() -> &'static [&'static dyn Analysis] {
    &[
        &builtin::Dependencies,
        &builtin::Cycles,
        &builtin::Unreachable,
        &builtin::Spof,
        &builtin::Metrics,
        &builtin::PathAnalysis,
        // Mechanical and reliability packs (b6e-mechanical).
        &mech::MobilityAnalysis,
        &mech::RatioAnalysis,
        &mech::MasonAnalysis,
        &reliability::CutSetsAnalysis,
        &reliability::AvailabilityAnalysis,
        &reliability::OutcomesAnalysis,
    ]
}

/// The analysis with this id.
pub fn analysis(id: &str) -> Option<&'static dyn Analysis> {
    analyses().iter().find(|a| a.id() == id).copied()
}

/// A model compare as a result of kind `diff`: added and changed ids marked on the current model,
/// removed ids kept in a `Removed` group (they are not in the current model to highlight).
pub fn diff_result(d: &ModelDiff) -> AnalysisResult {
    let mut r = AnalysisResult::new("diff");
    let changed = |c: &diff::Changes| c.changed.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
    let mut els = d.elements.added.clone();
    els.extend(changed(&d.elements));
    let mut rels = d.relationships.added.clone();
    rels.extend(changed(&d.relationships));
    for (ids, mark) in [(&d.elements.added, Mark::Added), (&d.relationships.added, Mark::Added)] {
        for id in ids {
            r.marks.insert(id.clone(), mark);
        }
    }
    for c in d.elements.changed.iter().chain(&d.relationships.changed) {
        r.marks.insert(c.id.clone(), Mark::Changed);
    }
    r.set_elements(els, None);
    r.set_relationships(rels, None);
    r.push_group(Group { label: "Removed".into(), elements: d.elements.removed.clone(), relationships: d.relationships.removed.clone() });
    r.truncated |= d.truncated;
    let n = |c: &diff::Changes| (c.added.len(), c.removed.len(), c.changed.len());
    let (ea, er, ec) = n(&d.elements);
    let (ra, rr, rc) = n(&d.relationships);
    r.summary = if d.is_empty() {
        "No differences".to_string()
    } else {
        format!("Elements: {ea} added, {er} removed, {ec} changed; relationships: {ra} added, {rr} removed, {rc} changed")
    };
    r.detail = serde_json::to_value(d).unwrap_or(Value::Null);
    r
}
