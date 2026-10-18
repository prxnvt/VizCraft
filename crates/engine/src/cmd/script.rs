//! Command scripts: record the commands run in a session as `[{command, params}]` and replay a
//! script as one transaction. Recording skips view, UI, app, help, file and script commands
//! (`session::recordable`).

use serde_json::{Value, json};

use super::batch::{label_of, results_of, steps_of};
use crate::session::Origin;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Largest script text `script.run` parses when the script comes as a string.
pub const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024;

/// Return all script commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("script.recordStart", "Record Script", "Developer > Scripts", |s, _| {
            s.record_start();
            Ok(json!({"recording": true}))
        }),
        CommandSpec::new("script.recordStop", "Stop Recording", "Developer > Scripts", |s, _| {
            let script = s.record_stop().ok_or_else(|| EngineError::Params("no script is being recorded".into()))?;
            Ok(json!({"count": script.len(), "script": script}))
        })
        .params("{} -> {count, script: [{command, params}]}")
        .when(|s| if s.is_recording() { None } else { Some("no script is being recorded") }),
        CommandSpec::new("script.run", "Run Script", "Developer > Scripts", run)
            .params(r#"{"script": [{"command": string, "params"?: object}] | string (that list as JSON), "label"?: string} -> {label, completed, results: [{command, result}]}"#),
    ]
}

/// Replay a script as one transaction made on behalf of [`Origin::Script`]: one undo step, and
/// nothing changed when a command fails.
fn run(s: &mut Session, v: &Value) -> CmdResult {
    let parsed;
    let list = match v.get("script") {
        Some(Value::String(text)) => {
            if text.len() > MAX_SCRIPT_BYTES {
                return Err(EngineError::Params("the script is too large".into()));
            }
            parsed = serde_json::from_str::<Value>(text).map_err(|e| EngineError::Params(format!("the script is not JSON: {e}")))?;
            &parsed
        }
        Some(list) => list,
        None => return Err(EngineError::Params("`script` is required".into())),
    };
    let steps = steps_of(list, "script")?;
    if steps.is_empty() {
        return Err(EngineError::Params("the script is empty".into()));
    }
    let label = label_of(v, "Run Script")?;
    let before = std::mem::replace(&mut s.origin, Origin::Script);
    let results = s.transaction(&label, &steps);
    s.origin = before;
    let results = results?;
    Ok(json!({"label": label, "completed": results.len(), "results": results_of(&steps, results)}))
}
