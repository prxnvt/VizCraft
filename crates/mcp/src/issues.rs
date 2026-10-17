//! The structured issues a `command_run` or `command_batch` call raised.
//!
//! The server reads the app's live issues with the engine command `validate.list` once before a
//! call and once after it, and reports the issues that are new (after minus before) together
//! with how many are open in all. An app that does not know `validate.list` (one built before
//! the model core) simply gets no issue report; the call itself is never failed for it.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use crate::backend::Backend;

/// The engine command that lists the live issues.
const LIST_COMMAND: &str = "validate.list";

/// The app's live issues right now, or `None` when the app cannot say (an older app, a broken
/// connection or a reply without an `issues` array).
pub(crate) fn live(backend: &mut dyn Backend) -> Option<Vec<Value>> {
    let reply = backend.call("engine.execute", json!({"command": LIST_COMMAND, "params": {}})).ok()?;
    match reply {
        Value::Object(mut o) => match o.remove("issues") {
            Some(Value::Array(list)) => Some(list),
            _ => None,
        },
        _ => None,
    }
}

/// The identity of one issue: rule, subjects, items and page, not the message, the same rule
/// the engine uses (`Issue::key`), so a reworded message is still the same issue.
fn key(issue: &Value) -> String {
    let Value::Object(o) = issue else {
        // Not an issue object; keep it distinct from every real key.
        return format!("?{issue}");
    };
    let text = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) => String::new(),
        Some(other) => other.to_string(),
    };
    let list = |v: Option<&Value>| match v {
        Some(Value::Array(items)) => items.iter().map(|i| text(Some(i))).collect::<Vec<_>>().join(","),
        other => text(other),
    };
    format!("{}|{}|{}|{}", text(o.get("rule")), list(o.get("subjects")), list(o.get("items")), text(o.get("page")))
}

/// The issues in `after` whose key is not in `before`, in `after`'s order.
pub(crate) fn raised(before: &[Value], after: &[Value]) -> Vec<Value> {
    let old: HashSet<String> = before.iter().map(key).collect();
    after.iter().filter(|i| !old.contains(&key(i))).cloned().collect()
}

/// The `issues` and `openIssues` keys for a call that started with `before` live issues, read
/// from the app now. `None` when there was no reading before or the app cannot answer now.
pub(crate) fn report(backend: &mut dyn Backend, before: Option<&[Value]>) -> Option<Map<String, Value>> {
    let before = before?;
    let after = live(backend)?;
    let mut keys = Map::new();
    keys.insert("issues".into(), Value::Array(raised(before, &after)));
    keys.insert("openIssues".into(), json!(after.len()));
    Some(keys)
}
