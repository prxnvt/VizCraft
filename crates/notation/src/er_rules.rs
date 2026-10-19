//! The checks of the entity-relationship pack: a table needs a primary key, and a foreign key
//! must name a column that exists, of the same data type. [`check`] is the rules engine's one
//! dispatch arm for them.
//!
//! Columns are elements owned by their table (`er.Attribute` owned by `er.Entity`), with the
//! bool property `pk` and the text properties `references` (`<table>.<column>`) and `dataType`.

use std::collections::HashMap;

use vizcraft_diagram::Document;
use vizcraft_model::{Element, Issue, PropValue, Severity};

use crate::Catalog;
use crate::infer::Facts;
use crate::rules::{cut, message};
use crate::schema::{Check, Pack, RuleDecl};

/// Longest name quoted in a message, in bytes.
const MAX_LABEL: usize = 60;

/// Run one of the checks of this module; other checks are ignored.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let views = views(doc);
    match &rule.check {
        Check::PrimaryKey { types, viewpoints } => primary_keys(doc, pack, &facts, &views, rule, types, viewpoints, out),
        Check::ForeignKey { types } => foreign_keys(doc, pack, &facts, &views, rule, types, out),
        _ => {}
    }
}

/// Shape views by element id: (page index, shape id).
fn views(doc: &Document) -> HashMap<&str, Vec<(usize, &str)>> {
    let mut m: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            if let Some(e) = s.element.as_deref() {
                m.entry(e).or_default().push((pi, s.id.as_str()));
            }
        }
    }
    m
}

fn covers(facts: &Facts<'_>, pack: &Pack, types: &[String], type_id: &str) -> bool {
    if types.is_empty() { facts.of_pack(type_id, pack) } else { facts.lists(pack, types, type_id) }
}

fn quoted(e: &Element) -> String {
    let n = e.name.trim();
    if n.is_empty() { e.id.clone() } else { format!("\"{}\"", cut(&n.replace(['\n', '\r'], " "), MAX_LABEL)) }
}

fn issue(doc: &Document, rule: &RuleDecl, what: &str, subject: &str, items: Vec<(usize, &str)>) -> Issue {
    let page = match items.first() {
        Some((p, _)) if items.iter().all(|(q, _)| q == p) => doc.pages.get(*p).map(|pg| pg.id.clone()),
        _ => None,
    };
    let mut i = Issue::new(rule.id.clone(), rule.severity, message(&rule.description, what))
        .with_subjects(vec![subject.to_string()])
        .with_items(items.into_iter().map(|(_, id)| id.to_string()).collect());
    i.page = page;
    i
}

fn is_true(v: Option<&PropValue>) -> bool {
    matches!(v, Some(PropValue::Bool(true))) || v.and_then(PropValue::as_str).is_some_and(|t| t.trim().eq_ignore_ascii_case("true"))
}

/// Tables drawn on a checked page that own no primary-key column.
#[allow(clippy::too_many_arguments)]
fn primary_keys(
    doc: &Document,
    pack: &Pack,
    facts: &Facts<'_>,
    views: &HashMap<&str, Vec<(usize, &str)>>,
    rule: &RuleDecl,
    types: &[String],
    viewpoints: &[String],
    out: &mut Vec<Issue>,
) {
    let mut keyed: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in &doc.model.elements {
        if let Some(o) = e.owner.as_deref()
            && is_true(e.prop("pk"))
        {
            keyed.insert(o);
        }
    }
    let checked_page = |pi: usize| {
        viewpoints.is_empty() || doc.pages.get(pi).and_then(|p| p.view.as_ref()).is_some_and(|v| viewpoints.contains(&v.viewpoint))
    };
    for e in doc.model.elements.iter().filter(|e| covers(facts, pack, types, &e.type_id)) {
        if keyed.contains(e.id.as_str()) {
            continue;
        }
        let items: Vec<(usize, &str)> =
            views.get(e.id.as_str()).map(|v| v.iter().copied().filter(|(pi, _)| checked_page(*pi)).collect()).unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        out.push(issue(doc, rule, &format!("{} has no primary key column.", quoted(e)), &e.id, items));
    }
}

/// The text of a property, trimmed; empty when unset or not text.
fn text<'a>(e: &'a Element, key: &str) -> &'a str {
    e.prop(key).and_then(PropValue::as_str).map(str::trim).unwrap_or_default()
}

/// Columns whose `references` names no column, or one of another data type.
fn foreign_keys(
    doc: &Document,
    pack: &Pack,
    facts: &Facts<'_>,
    views: &HashMap<&str, Vec<(usize, &str)>>,
    rule: &RuleDecl,
    types: &[String],
    out: &mut Vec<Issue>,
) {
    // Columns by (table name, column name); the first of each pair wins.
    let names: HashMap<&str, &str> = doc.model.elements.iter().map(|e| (e.id.as_str(), e.name.trim())).collect();
    let mut columns: HashMap<(&str, &str), &Element> = HashMap::new();
    for e in &doc.model.elements {
        let Some(table) = e.owner.as_deref().and_then(|o| names.get(o)) else { continue };
        columns.entry((table, e.name.trim())).or_insert(e);
    }
    for e in doc.model.elements.iter().filter(|e| covers(facts, pack, types, &e.type_id)) {
        let target = text(e, "references");
        if target.is_empty() {
            continue;
        }
        let items = || {
            let mut v = views.get(e.id.as_str()).cloned().unwrap_or_default();
            if let Some(o) = e.owner.as_deref() {
                v.extend(views.get(o).cloned().unwrap_or_default());
            }
            v
        };
        let owner = e.owner.as_deref().and_then(|o| names.get(o)).copied().unwrap_or_default();
        let label = format!("{}.{}", cut(owner, MAX_LABEL), cut(e.name.trim(), MAX_LABEL));
        let found = target.rsplit_once('.').and_then(|(t, c)| columns.get(&(t.trim(), c.trim())));
        match found {
            None => {
                let what = format!("Column {label} references \"{}\", which is not a column.", cut(target, MAX_LABEL * 2));
                out.push(issue(doc, rule, &what, &e.id, items()));
            }
            Some(col) => {
                let (mine, theirs) = (text(e, "dataType"), text(col, "dataType"));
                if !mine.is_empty() && !theirs.is_empty() && !mine.eq_ignore_ascii_case(theirs) {
                    let what = format!(
                        "Column {label} is {} but references {}, which is {}.",
                        cut(mine, MAX_LABEL),
                        cut(target, MAX_LABEL * 2),
                        cut(theirs, MAX_LABEL)
                    );
                    let mut i = issue(doc, rule, &what, &e.id, items());
                    // A type mismatch may be deliberate (a widening); it is worth a look, not an error.
                    i.severity = Severity::Warning;
                    out.push(i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_model::Severity;

    fn case(name: &str) -> crate::cases::Outcome {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/tests/er").join(name);
        let text = std::fs::read_to_string(&path).unwrap();
        crate::cases::run_text(name, &text, crate::catalog())
    }

    #[test]
    fn a_foreign_key_of_another_type_is_a_warning_and_a_missing_column_an_error() {
        let mismatch = case("fk_type_mismatch.json");
        assert!(mismatch.passed(), "{:?}", mismatch.problems);
        assert_eq!(mismatch.issues.iter().map(|i| i.severity).collect::<Vec<_>>(), [Severity::Warning]);
        let missing = case("fk_to_missing_column.json");
        assert!(missing.passed(), "{:?}", missing.problems);
        assert_eq!(missing.issues.iter().map(|i| i.severity).collect::<Vec<_>>(), [Severity::Error]);
    }
}
