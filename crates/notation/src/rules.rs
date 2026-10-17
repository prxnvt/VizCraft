//! The rules engine: checks declared by packs, run over a document.
//!
//! Live rules (`RuleMode::Live`) run after every command that changes the document and must stay
//! cheap (linear in the model and the views). Batch rules run on request over a scope. Each rule
//! declared by a pack the model uses runs once; its findings become [`Issue`]s carrying the rule's
//! id and severity, the element and relationship ids involved and the view items that draw them.
//! Results are sorted by severity, then rule id, then first subject, so equal documents give equal
//! lists.

use vizcraft_diagram::Document;
use vizcraft_model::Issue;

use crate::Catalog;

/// What a batch run looks at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// The whole project.
    #[default]
    All,
    /// The page at this index: elements drawn on it and its connectors.
    Page(usize),
    /// These elements, relationships, shapes or connectors (shapes and connectors stand for the
    /// model items they draw).
    Items(Vec<String>),
}

/// Live issues for `doc`: every live rule of every pack the model uses.
pub fn live(doc: &Document, cat: &Catalog) -> Vec<Issue> {
    // Contract stub; the c4a-rules piece implements every check.
    let _ = (doc, cat);
    Vec::new()
}

/// Batch issues for `doc` within `scope`: every batch rule of every pack the model uses, with
/// findings outside the scope left out.
pub fn batch(doc: &Document, cat: &Catalog, scope: &Scope) -> Vec<Issue> {
    // Contract stub; the c4a-rules piece implements every check.
    let _ = (doc, cat, scope);
    Vec::new()
}
