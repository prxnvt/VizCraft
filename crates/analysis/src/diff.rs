//! Model compare: what was added, removed and changed between two models, keyed by id.
//!
//! This is a symmetric compare of two states of one model. It is not `vizcraft_model::sync::diff`,
//! which matches imported items by their import record and only runs one way.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vizcraft_model::{Element, Model, PropValue, Relationship};

use crate::MAX_RESULT_IDS;

/// One field that differs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    /// `type`, `name`, `tag`, `owner`, `source`, `target`, `sourcePort`, `targetPort` or
    /// `properties.<key>`.
    pub field: String,
    /// The value before; `None` when it was unset.
    pub before: Option<Value>,
    /// The value after; `None` when it is unset.
    pub after: Option<Value>,
}

/// One item present on both sides whose fields differ.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemChange {
    /// The item id.
    pub id: String,
    /// The fields that differ, in field order.
    pub fields: Vec<FieldChange>,
}

/// What changed in one kind of item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// Ids only in the after model, in its order.
    pub added: Vec<String>,
    /// Ids only in the before model, in its order.
    pub removed: Vec<String>,
    /// Items in both whose fields differ, in the after model's order.
    pub changed: Vec<ItemChange>,
}

impl Changes {
    /// Nothing was added, removed or changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
    /// How many items were added, removed or changed.
    pub fn count(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

/// The difference between two models.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDiff {
    /// Element changes.
    pub elements: Changes,
    /// Relationship changes.
    pub relationships: Changes,
    /// More than `MAX_RESULT_IDS` items differ; the lists stop there.
    pub truncated: bool,
}

impl ModelDiff {
    /// The models are the same item for item.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.relationships.is_empty()
    }
}

/// Field name, value pairs an item is compared on.
type Fields = Vec<(String, Option<Value>)>;

fn text(t: &str) -> Option<Value> {
    (!t.is_empty()).then(|| Value::String(t.to_string()))
}

fn opt(t: Option<&String>) -> Option<Value> {
    t.map(|t| Value::String(t.clone()))
}

fn prop(v: &PropValue) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn props(out: &mut Fields, p: &BTreeMap<String, PropValue>) {
    out.extend(p.iter().map(|(k, v)| (format!("properties.{k}"), Some(prop(v)))));
}

fn element_fields(e: &Element) -> Fields {
    let mut f = vec![
        ("type".to_string(), text(&e.type_id)),
        ("name".to_string(), text(&e.name)),
        ("tag".to_string(), text(&e.tag)),
        ("owner".to_string(), opt(e.owner.as_ref())),
    ];
    props(&mut f, &e.properties);
    f
}

fn relationship_fields(r: &Relationship) -> Fields {
    let mut f = vec![
        ("type".to_string(), text(&r.type_id)),
        ("name".to_string(), text(&r.name)),
        ("source".to_string(), text(&r.source)),
        ("target".to_string(), text(&r.target)),
        ("sourcePort".to_string(), opt(r.source_port.as_ref())),
        ("targetPort".to_string(), opt(r.target_port.as_ref())),
    ];
    props(&mut f, &r.properties);
    f
}

/// The fields that differ between two field lists: the fixed fields first, then properties by key.
fn compare(before: Fields, after: Fields) -> Vec<FieldChange> {
    let b: BTreeMap<String, Option<Value>> = before.iter().cloned().collect();
    let a: BTreeMap<String, Option<Value>> = after.iter().cloned().collect();
    let mut order: Vec<&String> = before.iter().map(|(k, _)| k).collect();
    for (k, _) in &after {
        if !b.contains_key(k) {
            order.push(k);
        }
    }
    // Fixed fields stay in their order; properties sort by key so the result is stable.
    order.sort_by_key(|k| (k.starts_with("properties."), if k.starts_with("properties.") { (*k).clone() } else { String::new() }));
    order
        .into_iter()
        .filter_map(|k| {
            let (x, y) = (b.get(k).cloned().flatten(), a.get(k).cloned().flatten());
            (x != y).then(|| FieldChange { field: k.clone(), before: x, after: y })
        })
        .collect()
}

/// Compare two lists of items keyed by id (the first of a repeated id counts).
fn changes<T>(before: &[T], after: &[T], id: impl Fn(&T) -> &str, fields: impl Fn(&T) -> Fields, room: &mut usize, truncated: &mut bool) -> Changes {
    let mut out = Changes::default();
    let mut b: HashMap<&str, &T> = HashMap::with_capacity(before.len());
    for x in before {
        b.entry(id(x)).or_insert(x);
    }
    let mut a: HashMap<&str, &T> = HashMap::with_capacity(after.len());
    for x in after {
        a.entry(id(x)).or_insert(x);
    }
    let mut take = || -> bool {
        if *room == 0 {
            *truncated = true;
            return false;
        }
        *room -= 1;
        true
    };
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for x in after {
        let key = id(x);
        if !seen.insert(key) {
            continue;
        }
        match b.get(key) {
            None => {
                if take() {
                    out.added.push(key.to_string());
                }
            }
            Some(old) => {
                let f = compare(fields(old), fields(x));
                if !f.is_empty() && take() {
                    out.changed.push(ItemChange { id: key.to_string(), fields: f });
                }
            }
        }
    }
    seen.clear();
    for x in before {
        let key = id(x);
        if seen.insert(key) && !a.contains_key(key) && take() {
            out.removed.push(key.to_string());
        }
    }
    out
}

/// What changed from `before` to `after`, by element and relationship id: added, removed, and
/// changed in type, name, tag, owner, ends, ports or any property. At most `MAX_RESULT_IDS` items
/// in all; past that `truncated` is set.
pub fn diff(before: &Model, after: &Model) -> ModelDiff {
    let mut room = MAX_RESULT_IDS;
    let mut truncated = false;
    let elements = changes(&before.elements, &after.elements, |e| e.id.as_str(), element_fields, &mut room, &mut truncated);
    let relationships = changes(&before.relationships, &after.relationships, |r| r.id.as_str(), relationship_fields, &mut room, &mut truncated);
    ModelDiff { elements, relationships, truncated }
}
