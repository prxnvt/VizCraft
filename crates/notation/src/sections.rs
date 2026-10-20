//! Pack sections: a large pack (UML) groups its lists by diagram kind, and another branch can add
//! a whole section without touching the others. [`flatten`] runs first when a pack loads and
//! appends each section's lists to the pack's own, so the rest of the crate only sees flat lists.

use crate::schema::Pack;

/// Most sections one pack may have.
pub const MAX_SECTIONS: usize = 32;

/// Longest section id, in bytes.
pub const MAX_SECTION_ID: usize = 64;

/// Append every section's lists to the pack's, in section order, and clear the sections. A
/// section ruleset whose id is already declared extends that ruleset's rules (each rule once)
/// instead of being added. Returns the problems: duplicate or malformed section ids, and more
/// than [`MAX_SECTIONS`] sections (the extra ones are dropped).
pub fn flatten(pack: &mut Pack) -> Vec<String> {
    let mut problems = Vec::new();
    let mut sections = std::mem::take(&mut pack.sections);
    if sections.len() > MAX_SECTIONS {
        problems.push(format!("at most {MAX_SECTIONS} sections"));
        sections.truncate(MAX_SECTIONS);
    }
    let mut seen = std::collections::HashSet::new();
    for sec in sections {
        let shown: String = sec.id.chars().take(64).collect();
        if sec.id.is_empty() || sec.id.len() > MAX_SECTION_ID || !sec.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            problems.push(format!("section `{shown}`: the id must be lowercase letters, digits and `_`, at most {MAX_SECTION_ID} bytes"));
        }
        if !seen.insert(sec.id.clone()) {
            problems.push(format!("section `{shown}`: declared twice"));
        }
        pack.glyphs.extend(sec.glyphs);
        pack.connector_types.extend(sec.connector_types);
        pack.element_types.extend(sec.element_types);
        pack.relationship_types.extend(sec.relationship_types);
        pack.connections.extend(sec.connections);
        pack.conditional_styles.extend(sec.conditional_styles);
        pack.palette.extend(sec.palette);
        pack.viewpoints.extend(sec.viewpoints);
        pack.rules.extend(sec.rules);
        pack.templates.extend(sec.templates);
        pack.examples.extend(sec.examples);
        for rs in sec.rulesets {
            match pack.rulesets.iter_mut().find(|r| r.id == rs.id) {
                Some(existing) => {
                    for r in rs.rules {
                        if !existing.rules.contains(&r) {
                            existing.rules.push(r);
                        }
                    }
                }
                None => pack.rulesets.push(rs),
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::loader::{NotationError, load_json};

    fn pack() -> serde_json::Value {
        json!({
            "id": "sec", "version": "1.0.0", "name": "Sections",
            "elementTypes": [{"id": "sec.A"}],
            "relationshipTypes": [{"id": "sec.Link"}],
            "rules": [{"id": "sec.illegal", "check": {"kind": "illegalRelationship"}}],
            "rulesets": [{"id": "sec.core", "label": "Core", "rules": ["sec.illegal"]}],
            "sections": [
                {"id": "more", "elementTypes": [{"id": "sec.B"}],
                 "connections": [{"source": ["sec.A"], "relationship": "sec.Link", "target": ["sec.B"]}],
                 "rules": [{"id": "sec.mult", "check": {"kind": "multiplicity"}}],
                 "rulesets": [{"id": "sec.core", "rules": ["sec.mult", "sec.illegal"]}, {"id": "sec.extra", "label": "Extra", "rules": ["sec.mult"]}]}
            ]
        })
    }

    #[test]
    fn a_section_appends_its_lists_and_extends_a_matching_ruleset() {
        let p = load_json(&pack().to_string()).unwrap();
        assert!(p.sections.is_empty(), "flattened");
        assert_eq!(p.element_types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["sec.A", "sec.B"]);
        assert_eq!(p.connections.len(), 1);
        assert_eq!(p.ruleset("sec.core").unwrap().rules, ["sec.illegal", "sec.mult"]);
        assert!(p.ruleset("sec.extra").is_some());
        assert_eq!(p.rulesets.first().map(|r| r.id.as_str()), Some("sec.core"), "the top-level ruleset stays the default");
        let text = serde_json::to_value(&p).unwrap();
        assert!(text.get("sections").is_none(), "a loaded pack writes flat lists");
    }

    #[test]
    fn duplicate_or_bad_section_ids_are_problems() {
        let mut v = pack();
        let first = v["sections"][0].clone();
        let mut second = first.clone();
        second["elementTypes"] = json!([]);
        second["connections"] = json!([]);
        second["rules"] = json!([]);
        second["rulesets"] = json!([]);
        v["sections"] = json!([first, second]);
        match load_json(&v.to_string()) {
            Err(NotationError::Invalid(p)) => assert!(p.join(";").contains("section `more`: declared twice"), "{p:?}"),
            other => panic!("{other:?}"),
        }
        v["sections"] = json!([{"id": "Bad"}]);
        assert!(matches!(load_json(&v.to_string()), Err(NotationError::Invalid(p)) if p.join(";").contains("lowercase")));
        let many: Vec<_> = (0..33).map(|i| json!({"id": format!("s{i}")})).collect();
        v["sections"] = json!(many);
        assert!(matches!(load_json(&v.to_string()), Err(NotationError::Invalid(p)) if p.join(";").contains("at most 32 sections")));
    }
}
