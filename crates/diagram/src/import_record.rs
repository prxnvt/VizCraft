//! Import provenance: an [`ImportRecord`] in `Document.imports` remembers what one merging import
//! (`file.import {merge: true}`) brought in from a source, so a re-import of the same source can
//! match elements by the ids they were given and flag those the source no longer has.
//!
//! Records are capped and sanitised like the rest of the document: [`sanitize`] bounds their
//! count and sizes, and [`prune`] drops ids the model no longer holds. [`removed_issues`] turns
//! the flagged elements into `import.removed` issues whose fix deletes the element.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;
use vizcraft_model::issue::Fix;
use vizcraft_model::{Issue, Model, Severity};

use crate::Document;
use crate::serde_util::{lenient, lenient_vec};

/// Most import records a document keeps; the oldest goes first.
pub const MAX_IMPORTS: usize = 256;

/// Longest source (a path, or the name a caller gave), in bytes.
pub const MAX_SOURCE: usize = 4096;

/// Longest format id, in bytes.
pub const MAX_FORMAT: usize = 32;

/// Most ids one record keeps (elements, relationships and flagged elements each).
pub const MAX_RECORD_IDS: usize = 100_000;

/// The rule id of the issue raised for an element the source no longer has.
pub const REMOVED_RULE: &str = "import.removed";

/// One source imported into the model with `merge`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImportRecord {
    /// Where it came from: a file path, or the name the caller gave a text import (empty for an
    /// unnamed text).
    #[serde(deserialize_with = "lenient")]
    pub source: String,
    /// The import format (`mermaid`, `csv`, `drawio`).
    #[serde(deserialize_with = "lenient")]
    pub format: String,
    /// When it was last imported, in seconds since 1970 (0 where there is no clock).
    #[serde(deserialize_with = "lenient")]
    pub time: u64,
    /// The id of the page its shapes went to.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub page: String,
    /// Id in the source to the id of the model element it became.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "BTreeMap::is_empty")]
    pub elements: BTreeMap<String, String>,
    /// Ids of the model relationships it made.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<String>,
    /// Model elements it made that the source no longer has: kept, and flagged until deleted or
    /// imported again.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

/// `s` cut to at most `max` bytes at a character boundary.
fn cut(s: &mut String, max: usize) {
    if s.len() > max {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

/// Whether `id` could name a model item.
fn usable(id: &str) -> bool {
    !id.is_empty() && id.len() <= vizcraft_model::MAX_ID
}

impl ImportRecord {
    /// Keep the record bounded: short source and format, at most [`MAX_RECORD_IDS`] usable ids in
    /// each list, no repeats.
    pub fn sanitize(&mut self) {
        cut(&mut self.source, MAX_SOURCE);
        cut(&mut self.format, MAX_FORMAT);
        if self.page.len() > vizcraft_model::MAX_ID {
            self.page.clear();
        }
        self.elements.retain(|k, v| usable(k) && usable(v));
        while self.elements.len() > MAX_RECORD_IDS {
            if self.elements.pop_last().is_none() {
                break;
            }
        }
        for list in [&mut self.relationships, &mut self.removed] {
            let mut seen = HashSet::new();
            list.retain(|id| usable(id) && seen.insert(id.clone()));
            list.truncate(MAX_RECORD_IDS);
        }
    }
}

/// Bound a document's records: at most [`MAX_IMPORTS`] (the newest kept), each sanitised, one
/// per source.
pub fn sanitize(records: &mut Vec<ImportRecord>) {
    if records.len() > MAX_IMPORTS {
        let extra = records.len() - MAX_IMPORTS;
        records.drain(..extra);
    }
    records.iter_mut().for_each(ImportRecord::sanitize);
    // A later record for the same source wins.
    let mut seen = HashSet::new();
    let mut keep: Vec<bool> = records.iter().rev().map(|r| seen.insert(r.source.clone())).collect();
    keep.reverse();
    let mut flags = keep.into_iter();
    records.retain(|_| flags.next().unwrap_or(true));
}

/// Drop ids the model no longer holds, so a record never points at nothing.
pub fn prune(records: &mut [ImportRecord], model: &Model) {
    for r in records {
        r.elements.retain(|_, id| model.element(id).is_some());
        r.relationships.retain(|id| model.relationship(id).is_some());
        r.removed.retain(|id| model.element(id).is_some());
    }
}

/// The index of the record for `source`.
pub fn find(records: &[ImportRecord], source: &str) -> Option<usize> {
    records.iter().position(|r| r.source == source)
}

/// Store `rec`, replacing the record of the same source (it moves to the end, as the newest),
/// dropping the oldest when the list is full.
pub fn store(records: &mut Vec<ImportRecord>, mut rec: ImportRecord) {
    rec.sanitize();
    records.retain(|r| r.source != rec.source);
    while records.len() >= MAX_IMPORTS {
        records.remove(0);
    }
    records.push(rec);
}

/// One `import.removed` warning per flagged element still in the model: it names the element and
/// the source, points at the shapes drawing it, and offers `model.delete` as its fix.
pub fn removed_issues(doc: &Document) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in &doc.imports {
        let from = if r.source.is_empty() { "the imported text".to_string() } else { short(&r.source) };
        for id in &r.removed {
            let Some(e) = doc.model.element(id) else { continue };
            if !seen.insert(id.as_str()) {
                continue;
            }
            let name = if e.name.trim().is_empty() { e.id.clone() } else { short(e.name.trim()) };
            let views = crate::views::views_of(doc, id);
            let mut issue =
                Issue::new(REMOVED_RULE, Severity::Warning, format!("{name} is no longer in {from}; delete it from the model or keep it"))
                    .with_subjects(vec![id.clone()])
                    .with_items(views.iter().flat_map(|v| v.items.iter().cloned()).collect())
                    .with_fix(Fix::new("Delete from model", "model.delete", json!({"ids": [id]})));
            if let Some(p) = views.first().map(|v| v.page_id.clone()) {
                issue = issue.on_page(p);
            }
            out.push(issue);
        }
    }
    out
}

/// `s` cut to a readable length for a message.
fn short(s: &str) -> String {
    const MAX: usize = 80;
    if s.chars().count() <= MAX { s.to_string() } else { format!("{}...", s.chars().take(MAX).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_bounded_one_per_source_and_pruned() {
        let mut rs: Vec<ImportRecord> = (0..MAX_IMPORTS + 5).map(|i| ImportRecord { source: format!("s{i}"), ..ImportRecord::default() }).collect();
        rs.push(ImportRecord { source: "s300".into(), format: "x".repeat(100), ..ImportRecord::default() });
        rs.push(ImportRecord {
            source: "é".repeat(MAX_SOURCE),
            elements: BTreeMap::from([(String::new(), "a".into()), ("k".into(), "e1".into())]),
            removed: vec!["e1".into(), "e1".into(), "x".repeat(100)],
            ..ImportRecord::default()
        });
        sanitize(&mut rs);
        assert!(rs.len() <= MAX_IMPORTS);
        let last = rs.last().unwrap();
        assert!(last.source.len() <= MAX_SOURCE);
        assert_eq!(last.elements.len(), 1);
        assert_eq!(last.removed, vec!["e1".to_string()]);
        assert_eq!(rs.iter().filter(|r| r.source == "s300").count(), 1);
        let mut m = Model::new();
        m.elements.push(vizcraft_model::Element::new("e1", "t.A"));
        let mut one =
            vec![ImportRecord { elements: BTreeMap::from([("a".into(), "e1".into()), ("b".into(), "gone".into())]), ..ImportRecord::default() }];
        prune(&mut one, &m);
        assert_eq!(one[0].elements.len(), 1);
    }
}
