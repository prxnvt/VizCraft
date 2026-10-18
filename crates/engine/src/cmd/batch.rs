//! `edit.batch`: several commands as one undo step, all or nothing.

use serde_json::{Value, json};

use super::{MAX_IDS, opt_str};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Longest label a batch or script step keeps; longer ones are cut at a character boundary.
pub const MAX_LABEL_CHARS: usize = 80;

/// Return the batch command.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.batch", "Batch", "Developer > Batch", batch).params(
            r#"{"label"?: string, "commands": [{"command": string, "params"?: object}]} -> {label, completed, results: [{command, result}]}"#,
        ),
    ]
}

/// Read a list of `{command, params?}` (`id` is accepted for `command`, as the control channel
/// accepts it): at most [`MAX_IDS`] entries, each with a non-empty command id and an object or no
/// params. `what` names the param in errors.
pub fn steps_of(list: &Value, what: &str) -> Result<Vec<(String, Value)>, EngineError> {
    let Some(list) = list.as_array() else { return Err(EngineError::Params(format!("`{what}` must be an array of {{command, params?}}"))) };
    if list.len() > MAX_IDS {
        return Err(EngineError::Params(format!("`{what}` has more than {MAX_IDS} entries")));
    }
    list.iter()
        .enumerate()
        .map(|(i, item)| {
            let bad = |why: &str| EngineError::Params(format!("{what}[{i}]: {why}"));
            let id = item.get("command").or_else(|| item.get("id")).and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty());
            let id = id.ok_or_else(|| bad("`command` (string) is required"))?;
            let params = match item.get("params") {
                None | Some(Value::Null) => json!({}),
                Some(p @ Value::Object(_)) => p.clone(),
                Some(_) => return Err(bad("`params` must be an object")),
            };
            Ok((id.to_string(), params))
        })
        .collect()
}

/// The `label` param, trimmed and cut to [`MAX_LABEL_CHARS`], or `default` when absent or blank.
pub fn label_of(v: &Value, default: &str) -> Result<String, EngineError> {
    let label: String = opt_str(v, "label")?.unwrap_or_default().trim().chars().take(MAX_LABEL_CHARS).collect();
    Ok(if label.is_empty() { default.to_string() } else { label })
}

/// Each step's result beside its command id.
pub fn results_of(steps: &[(String, Value)], results: Vec<Value>) -> Value {
    Value::Array(steps.iter().zip(results).map(|((id, _), r)| json!({"command": id, "result": r})).collect())
}

fn batch(s: &mut Session, v: &Value) -> CmdResult {
    let steps = steps_of(v.get("commands").unwrap_or(&Value::Null), "commands")?;
    if steps.is_empty() {
        return Err(EngineError::Params("`commands` is empty".into()));
    }
    let label = label_of(v, "Batch")?;
    let results = s.transaction(&label, &steps)?;
    Ok(json!({"label": label, "completed": results.len(), "results": results_of(&steps, results)}))
}
