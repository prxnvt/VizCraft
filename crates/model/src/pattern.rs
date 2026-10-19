//! Pattern provenance: a [`PatternInstance`] records which pattern made a set of elements and
//! relationships, with which arguments, and the role each item plays, so the engine can
//! regenerate the instance with other arguments and keep what a person edited.
//!
//! The record lives in `Model.patterns`. It names model items by id and diagram items (the
//! group the shapes were gathered in, the regions drawn as containers) by shape id; ids that no
//! longer exist are dropped by [`sanitize`], so a record never points at nothing.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repair::truncate;
use crate::serde_util::{lenient, lenient_map};
use crate::{MAX_ID, MAX_KEY, MAX_TEXT, Model};

/// Most pattern instances one model records; a file with more keeps the first ones.
pub const MAX_INSTANCES: usize = 10_000;

/// Most roles one instance records (elements, relationships and regions together).
pub const MAX_MEMBERS: usize = 4096;

/// Largest argument object, in bytes of JSON; a larger one is replaced by an empty object.
pub const MAX_ARGS_BYTES: usize = 64 * 1024;

/// One inserted pattern.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PatternInstance {
    /// Instance id, unique among the model's instances.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// The pattern id (`workflow.parallelSplit`).
    #[serde(deserialize_with = "lenient")]
    pub pattern: String,
    /// The arguments it was last generated with, as checked by the pattern's schema.
    #[serde(deserialize_with = "lenient")]
    pub args: Value,
    /// Role (`branch.2`) to the id of the element or relationship playing it.
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub members: BTreeMap<String, String>,
    /// Role to the name generated for it when it was last generated; a member whose name differs
    /// was renamed by hand, and regenerating keeps that name.
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub generated: BTreeMap<String, String>,
    /// The id of the page the instance was drawn on.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub page: String,
    /// The group shape gathering the instance's shapes on that page, empty when there is none.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// Region role to the container shape drawing it on that page.
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub frames: BTreeMap<String, String>,
}

impl PatternInstance {
    /// The role an element or relationship plays in this instance.
    pub fn role_of(&self, id: &str) -> Option<&str> {
        self.members.iter().find(|(_, m)| m.as_str() == id).map(|(r, _)| r.as_str())
    }
}

impl Model {
    /// The pattern instance with this id.
    pub fn pattern(&self, id: &str) -> Option<&PatternInstance> {
        self.patterns.iter().find(|p| p.id == id)
    }

    /// The pattern instance with this id, mutably.
    pub fn pattern_mut(&mut self, id: &str) -> Option<&mut PatternInstance> {
        self.patterns.iter_mut().find(|p| p.id == id)
    }

    /// The instance an element or relationship belongs to.
    pub fn pattern_of(&self, item: &str) -> Option<&PatternInstance> {
        self.patterns.iter().find(|p| p.role_of(item).is_some())
    }
}

fn cut_map(m: &mut BTreeMap<String, String>, value_max: usize) {
    let mut out = BTreeMap::new();
    for (mut k, mut v) in std::mem::take(m) {
        if out.len() >= MAX_MEMBERS {
            break;
        }
        truncate(&mut k, MAX_KEY);
        truncate(&mut v, value_max);
        if !k.is_empty() {
            out.insert(k, v);
        }
    }
    *m = out;
}

/// Make `model.patterns` safe: cap the count and lengths, drop instances with an empty or
/// repeated id, drop members naming no element or relationship and generated names for roles
/// that are not members, and replace an argument value that is not an object or is too large
/// by an empty object. Diagram ids (`page`, `group`, `frames`) are only cut to length: the
/// model cannot see the pages, and the engine treats a missing shape as absent. Idempotent.
pub fn sanitize(model: &mut Model) {
    model.patterns.truncate(MAX_INSTANCES);
    let known: HashSet<String> = model.elements.iter().map(|e| e.id.clone()).chain(model.relationships.iter().map(|r| r.id.clone())).collect();
    let mut seen: HashSet<String> = HashSet::new();
    model.patterns.retain_mut(|p| {
        truncate(&mut p.id, MAX_ID);
        truncate(&mut p.pattern, MAX_KEY);
        truncate(&mut p.page, MAX_ID);
        truncate(&mut p.group, MAX_ID);
        cut_map(&mut p.members, MAX_ID);
        p.members.retain(|_, id| known.contains(id));
        cut_map(&mut p.generated, MAX_TEXT);
        let members = &p.members;
        p.generated.retain(|role, _| members.contains_key(role));
        cut_map(&mut p.frames, MAX_ID);
        let fits = p.args.is_object() && serde_json::to_vec(&p.args).map(|b| b.len() <= MAX_ARGS_BYTES).unwrap_or(false);
        if !fits {
            p.args = Value::Object(serde_json::Map::new());
        }
        !p.id.is_empty() && seen.insert(p.id.clone())
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn instances_round_trip_and_sanitise() {
        let text = json!({
            "elements": [{"id": "e1", "type": "flowchart.Process"}],
            "patterns": [
                {"id": "p1", "pattern": "workflow.parallelSplit", "args": {"branches": 4}, "members": {"split": "e1", "join": "gone"},
                 "generated": {"split": "Split", "join": "Join"}, "page": "page1", "group": "g1", "frames": {"region": "s9"}},
                {"id": "p1", "pattern": "dup"},
                {"id": "", "pattern": "empty"},
                {"id": "p2", "args": [1, 2], "members": 7},
                "junk"
            ]
        });
        let mut m: Model = serde_json::from_value(text).unwrap();
        m.sanitize();
        assert_eq!(m.patterns.len(), 2);
        let p = m.pattern("p1").unwrap();
        assert_eq!(p.members.len(), 1, "the member naming no element is dropped");
        assert_eq!(p.generated.keys().collect::<Vec<_>>(), vec!["split"]);
        assert_eq!(m.pattern_of("e1").map(|p| p.id.as_str()), Some("p1"));
        assert_eq!(m.pattern("p2").unwrap().args, json!({}));
        let again: Model = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(again, m);
        let mut twice = m.clone();
        twice.sanitize();
        assert_eq!(twice, m);
    }
}
