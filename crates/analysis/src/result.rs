//! What an analysis returns: ids with marks, groups and an optional table, all plain data with
//! serde so the engine can store it, the UI can draw it and agents can read it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::MAX_RESULT_IDS;

/// How one id in a result is highlighted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mark {
    /// The element the analysis started from.
    Focus,
    /// Reached against the edges.
    Upstream,
    /// Reached along the edges.
    Downstream,
    /// On a cycle.
    Cycle,
    /// No root reaches it.
    Unreachable,
    /// A cut point or a bridge.
    Cut,
    /// On the path found.
    Path,
    /// Only in the current model.
    Added,
    /// Only in the model compared against.
    Removed,
    /// In both, with different fields.
    Changed,
}

/// A named set of elements and relationships: one cycle, the removed items of a compare.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Group {
    /// What the group is (`Cycle 1`).
    pub label: String,
    /// Element ids, in model order.
    pub elements: Vec<String>,
    /// Relationship ids, in model order.
    pub relationships: Vec<String>,
}

/// One row of a result table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TableRow {
    /// The element or relationship the row is about.
    pub id: String,
    /// One value per column.
    pub cells: Vec<Value>,
}

/// A table a result carries for the dock (metrics).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResultTable {
    /// Column names.
    pub columns: Vec<String>,
    /// Rows, each as long as `columns`.
    pub rows: Vec<TableRow>,
}

/// The result of one analysis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AnalysisResult {
    /// The analysis id (`dependencies`, `cycles`, `diff`).
    pub kind: String,
    /// One sentence saying what was found.
    pub summary: String,
    /// The element ids found, in model order.
    pub elements: Vec<String>,
    /// The relationship ids found, in model order.
    pub relationships: Vec<String>,
    /// How each id is highlighted.
    pub marks: BTreeMap<String, Mark>,
    /// Named sets (cycles, removed items).
    pub groups: Vec<Group>,
    /// A table for the dock, when the analysis has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<ResultTable>,
    /// A list hit [`MAX_RESULT_IDS`] and stops there.
    pub truncated: bool,
    /// The document revision the result was computed at (the engine stamps it); a result whose
    /// `rev` is not the session's is stale.
    pub rev: u64,
    /// What only this kind returns (`levels`, `roots`, `totals`, `length`), merged into the
    /// command result.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub detail: Value,
}

impl AnalysisResult {
    /// An empty result of `kind`.
    pub fn new(kind: &str) -> Self {
        AnalysisResult { kind: kind.to_string(), ..AnalysisResult::default() }
    }

    /// Set the element ids, cut at [`MAX_RESULT_IDS`], marking each with `mark` unless it has one.
    pub fn set_elements(&mut self, ids: Vec<String>, mark: Option<Mark>) {
        self.elements = self.capped(ids);
        if let Some(m) = mark {
            for id in &self.elements {
                self.marks.entry(id.clone()).or_insert(m);
            }
        }
    }

    /// Set the relationship ids, cut at [`MAX_RESULT_IDS`], marking each with `mark` unless it has
    /// one.
    pub fn set_relationships(&mut self, ids: Vec<String>, mark: Option<Mark>) {
        self.relationships = self.capped(ids);
        if let Some(m) = mark {
            for id in &self.relationships {
                self.marks.entry(id.clone()).or_insert(m);
            }
        }
    }

    /// Add a group, cutting its lists so the groups hold at most [`MAX_RESULT_IDS`] ids in all.
    pub fn push_group(&mut self, mut g: Group) {
        let used: usize = self.groups.iter().map(|g| g.elements.len() + g.relationships.len()).sum();
        let room = MAX_RESULT_IDS.saturating_sub(used);
        if g.elements.len() + g.relationships.len() > room {
            self.truncated = true;
            g.elements.truncate(room);
            g.relationships.truncate(room.saturating_sub(g.elements.len()));
        }
        if !g.elements.is_empty() || !g.relationships.is_empty() {
            self.groups.push(g);
        }
    }

    fn capped(&mut self, mut ids: Vec<String>) -> Vec<String> {
        if ids.len() > MAX_RESULT_IDS {
            ids.truncate(MAX_RESULT_IDS);
            self.truncated = true;
        }
        ids
    }

    /// Every id the result names (elements, relationships, marks, group members and table rows),
    /// each once, in that order.
    pub fn ids(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let all = self
            .elements
            .iter()
            .chain(&self.relationships)
            .chain(self.marks.keys())
            .chain(self.groups.iter().flat_map(|g| g.elements.iter().chain(&g.relationships)))
            .chain(self.table.iter().flat_map(|t| t.rows.iter().map(|r| &r.id)));
        all.filter(|id| seen.insert(id.as_str())).cloned().collect()
    }
}

/// Why an analysis did not run.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    /// No element with this id in the graph analysed.
    #[error("no element `{0}` in the graph analysed")]
    Unknown(String),
    /// The params are wrong; the text says how.
    #[error("{0}")]
    Params(String),
    /// The input is larger than an analysis takes.
    #[error("the model is too large for this analysis")]
    TooLarge,
}
