//! Connection-table queries: which relationships may join which element types.

use crate::Catalog;

/// A relationship type allowed from some source type, and the target types it may reach.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Legal {
    /// Qualified relationship type.
    pub relationship: String,
    /// Qualified target element types, in pack order, without repeats (`*` expanded).
    pub targets: Vec<String>,
}

/// Every relationship type that may start at an element of `source_type`, with its targets, in
/// connection-table order and merged per relationship type. Empty for an unknown type.
pub fn legal_from(cat: &Catalog, source_type: &str) -> Vec<Legal> {
    // Contract stub; the c4a-notation piece implements the table walk.
    let _ = (cat, source_type);
    Vec::new()
}

/// Whether some connection-table row of the relationship type's pack allows `relationship` from
/// `source_type` to `target_type`.
pub fn is_legal(cat: &Catalog, source_type: &str, relationship: &str, target_type: &str) -> bool {
    // Contract stub; the c4a-notation piece implements the row match.
    let _ = (cat, source_type, relationship, target_type);
    false
}

/// The relationship type a connector drawn from `source_type` to `target_type` becomes, and
/// whether it is legal: the first legal type in the source pack's table, else the first
/// relationship type of the source's pack (illegal, so the rules engine reports it), else `None`
/// (the source pack has no relationship types, or the type is unknown).
pub fn relationship_for(cat: &Catalog, source_type: &str, target_type: &str) -> Option<(String, bool)> {
    // Contract stub; the c4a-notation piece implements the choice.
    let _ = (cat, source_type, target_type);
    None
}
