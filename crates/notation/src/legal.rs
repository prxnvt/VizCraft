//! Connection-table queries: which relationships may join which element types.

use std::collections::HashSet;

use crate::{Catalog, Pack};

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
    let Some((pack, _)) = cat.element_type(source_type) else { return Vec::new() };
    // Targets per relationship type, in first-appearance order of the relationship type.
    let mut merged: Vec<(&str, Vec<&str>)> = Vec::new();
    for row in pack.connections.iter().filter(|r| pack.lists(&r.source, source_type)) {
        let i = match merged.iter().position(|(r, _)| *r == row.relationship) {
            Some(i) => i,
            None => {
                merged.push((row.relationship.as_str(), Vec::new()));
                merged.len().saturating_sub(1)
            }
        };
        if let Some((_, targets)) = merged.get_mut(i) {
            for t in &row.target {
                if t == "*" {
                    targets.extend(pack.element_types.iter().map(|e| e.id.as_str()));
                } else {
                    targets.push(t.as_str());
                }
            }
        }
    }
    merged.into_iter().map(|(relationship, targets)| Legal { relationship: relationship.to_string(), targets: pack_order(pack, targets) }).collect()
}

/// `targets` without repeats: this pack's types in the order the pack declares them, then types
/// of other packs in the order they were listed.
fn pack_order(pack: &Pack, targets: Vec<&str>) -> Vec<String> {
    let wanted: HashSet<&str> = targets.iter().copied().collect();
    let mut out: Vec<String> = pack.element_types.iter().filter(|e| wanted.contains(e.id.as_str())).map(|e| e.id.clone()).collect();
    let mut seen: HashSet<&str> = pack.element_types.iter().map(|e| e.id.as_str()).collect();
    for t in targets {
        if seen.insert(t) {
            out.push(t.to_string());
        }
    }
    out
}

/// Whether some connection-table row of the relationship type's pack allows `relationship` from
/// `source_type` to `target_type`.
pub fn is_legal(cat: &Catalog, source_type: &str, relationship: &str, target_type: &str) -> bool {
    let Some(pack) = cat.pack(vizcraft_model::pack_of(relationship)) else { return false };
    pack.connections.iter().any(|r| r.relationship == relationship && pack.lists(&r.source, source_type) && pack.lists(&r.target, target_type))
}

/// The relationship type a connector drawn from `source_type` to `target_type` becomes, and
/// whether it is legal: the first legal type in the source pack's table, else the first
/// relationship type of the source's pack (illegal, so the rules engine reports it), else `None`
/// (the source pack has no relationship types, or the type is unknown).
pub fn relationship_for(cat: &Catalog, source_type: &str, target_type: &str) -> Option<(String, bool)> {
    let (pack, _) = cat.element_type(source_type)?;
    if let Some(l) = legal_from(cat, source_type).into_iter().find(|l| l.targets.iter().any(|t| t == target_type)) {
        return Some((l.relationship, true));
    }
    pack.relationship_types.first().map(|r| (r.id.clone(), false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    const F: &str = "flowchart.Flow";

    #[test]
    fn flowchart_connection_table() {
        let c = catalog();
        assert!(!is_legal(c, "flowchart.Database", F, "flowchart.Database"));
        assert!(!is_legal(c, "flowchart.Data", F, "flowchart.Document"));
        assert!(is_legal(c, "flowchart.Process", F, "flowchart.Decision"));
        assert!(is_legal(c, "flowchart.Terminator", F, "flowchart.Data"));
        assert!(is_legal(c, "flowchart.Database", F, "flowchart.Process"));
        assert!(!is_legal(c, "flowchart.Process", "flowchart.Nope", "flowchart.Decision"));
        assert!(!is_legal(c, "flowchart.Process", "nopack.Flow", "flowchart.Decision"));
        assert_eq!(relationship_for(c, "flowchart.Database", "flowchart.Database"), Some((F.to_string(), false)));
        assert_eq!(relationship_for(c, "flowchart.Process", "flowchart.Decision"), Some((F.to_string(), true)));
        assert_eq!(relationship_for(c, "flowchart.Ghost", "flowchart.Decision"), None);
        assert_eq!(relationship_for(c, "basic.Box", "basic.Box"), None);
    }

    #[test]
    fn legal_from_merges_rows_and_keeps_pack_order() {
        let c = catalog();
        let l = legal_from(c, "flowchart.Terminator");
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].relationship, F);
        // Steps and references from row two, data symbols from row three, in pack order.
        let pack = c.pack("flowchart").unwrap();
        let every: Vec<String> = pack.element_types.iter().map(|e| e.id.clone()).collect();
        assert_eq!(l[0].targets, every);
        let l = legal_from(c, "flowchart.Database");
        assert!(!l[0].targets.iter().any(|t| t == "flowchart.Database" || t == "flowchart.Data"));
        assert!(l[0].targets.iter().any(|t| t == "flowchart.Process"));
        assert!(legal_from(c, "flowchart.Ghost").is_empty());
        assert!(legal_from(c, "").is_empty());
    }

    #[test]
    fn star_expands_to_every_type_of_the_rows_pack() {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "id": "t", "version": "1.0.0",
            "elementTypes": [{"id": "t.A"}, {"id": "t.B"}],
            "relationshipTypes": [{"id": "t.L"}],
            "connections": [{"source": ["*"], "relationship": "t.L", "target": ["t.B", "*", "other.X"]}]
        }))
        .unwrap();
        let c = Catalog::new(vec![pack]);
        let l = legal_from(&c, "t.B");
        assert_eq!(l, vec![Legal { relationship: "t.L".into(), targets: vec!["t.A".into(), "t.B".into(), "other.X".into()] }]);
        assert!(is_legal(&c, "t.A", "t.L", "t.A"));
        assert!(!is_legal(&c, "other.X", "t.L", "t.A"), "`*` covers this pack's types only");
    }
}
