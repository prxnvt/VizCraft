//! The tool list and dispatch. The Cascade 1 tools here give generic access to the command
//! registry, the document summary and a screenshot; every new engine command is reachable
//! through `command_run` with no change here. The diagram tools live in `diagram.rs`.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};

use crate::backend::Backend;
use crate::{MAX_BATCH, base64, issues};

/// Largest screenshot the server will read back and inline, in bytes. A full window on a large
/// Retina display is a few megabytes; anything far beyond that is not a screenshot we asked for.
const MAX_PNG_BYTES: u64 = 48 * 1024 * 1024;

/// The eight bytes every PNG file starts with.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Longest command id accepted; real ids are a few dozen characters.
const MAX_ID_CHARS: usize = 200;

/// What a tool call returns: MCP content blocks and whether the call failed.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// Content blocks (`{"type":"text",...}` or `{"type":"image",...}`).
    pub content: Vec<Value>,
    /// True when the tool ran but failed; the content says why.
    pub is_error: bool,
}

impl ToolResult {
    /// A successful plain-text result.
    pub fn text(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: false }
    }

    /// A successful result carrying JSON, pretty-printed as text so any client can show it.
    pub fn json(v: &Value) -> Self {
        Self::text(serde_json::to_string_pretty(v).unwrap_or_default())
    }

    /// A failed result with a message for the agent.
    pub fn error(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: true }
    }

    /// A failed result whose details are JSON (used by `command_batch`).
    fn error_json(v: &Value) -> Self {
        Self { is_error: true, ..Self::json(v) }
    }

    /// The `tools/call` result object.
    pub fn to_value(&self) -> Value {
        json!({"content": self.content, "isError": self.is_error})
    }
}

pub(crate) fn tool(name: &str, title: &str, description: &str, input_schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {"title": title, "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false},
    })
}

pub(crate) fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}

/// The tool list for `tools/list`: the generic tools, then the diagram tools.
pub fn tool_definitions() -> Vec<Value> {
    let mut defs = generic_definitions();
    defs.extend(crate::diagram::definitions());
    defs
}

fn generic_definitions() -> Vec<Value> {
    let command_item = object(
        json!({
            "id": {"type": "string", "description": "Command id, such as `shape.add` or `edit.undo`"},
            "params": {"type": "object", "description": "The command's params; see its `params` text in command_list"},
        }),
        &["id"],
    );
    vec![
        tool(
            "command_list",
            "List commands",
            "Every VizCraft action is a command with an id. This returns the catalog: id, label, menu \
             location, shortcut, a short description of the params it takes, and whether it can run \
             right now (with the reason when it cannot). Give `prefix` (for example `shape.` or \
             `connector.`) to narrow the list.",
            object(json!({"prefix": {"type": "string", "description": "Only commands whose id starts with this"}}), &[]),
            true,
        ),
        tool(
            "command_run",
            "Run a command",
            "Run one command by id with optional params, exactly as the menu or ribbon would, and \
             return its result (commands that create something return the new id). Each run is one \
             undo step. Commands never open dialogs when called this way; ones that need a path take \
             it as a param. When the app checks a notation model, the result also carries `issues` \
             (the rule problems this run raised) and `openIssues` (how many are open in all).",
            object(
                json!({
                    "id": {"type": "string", "description": "Command id from command_list"},
                    "params": {"type": "object", "description": "The command's params (default {})"},
                }),
                &["id"],
            ),
            false,
        ),
        tool(
            "command_batch",
            "Run several commands",
            "Run a list of commands in order, each its own undo step. Stops at the first one that \
             fails and reports its index, id and error together with the results of the ones that \
             ran before it; the earlier commands stay applied. At most 256 commands per call. When the \
             app checks a notation model, the result also carries `issues` (the rule problems the \
             commands that ran raised) and `openIssues` (how many are open in all).",
            object(json!({"commands": {"type": "array", "items": command_item, "minItems": 1, "maxItems": MAX_BATCH}}), &["commands"]),
            false,
        ),
        tool(
            "doc_inspect",
            "Inspect the document",
            "Return the app's state as JSON: document title and path, unsaved changes, revision, \
             selection, undo and redo labels, zoom, open panes and dialogs. Use it to check the \
             effect of a command without a screenshot.",
            object(json!({}), &[]),
            true,
        ),
        tool(
            "screenshot",
            "Take a screenshot",
            "Capture the VizCraft window as a PNG image, after any queued input has been processed, \
             and return it inline with its size in pixels.",
            object(json!({}), &[]),
            true,
        ),
    ]
}

/// Whether `name` is one of ours.
pub(crate) fn is_tool(name: &str) -> bool {
    tool_definitions().iter().any(|t| t.get("name").and_then(Value::as_str) == Some(name))
}

/// Run tool `name` with `args` against `backend`. Unknown names and bad arguments come back as
/// tool errors; this never panics on what a client sends.
pub fn call_tool(backend: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    if crate::diagram::is_diagram_tool(name) {
        return crate::diagram::call(backend, name, args);
    }
    let empty = Map::new();
    let args = match args {
        Value::Object(o) => o,
        Value::Null => &empty,
        _ => return ToolResult::error("`arguments` must be an object"),
    };
    let outcome = match name {
        "command_list" => command_list(backend, args),
        "command_run" => command_run(backend, args),
        "command_batch" => return command_batch(backend, args),
        "doc_inspect" => backend.call("ui.inspect", json!({})).map(|v| ToolResult::json(&v)),
        "screenshot" => screenshot(backend),
        other => Err(format!("unknown tool `{other}`")),
    };
    outcome.unwrap_or_else(ToolResult::error)
}

/// Every command the backend knows, as a JSON array.
pub(crate) fn commands(backend: &mut dyn Backend) -> Result<Vec<Value>, String> {
    match backend.call("engine.commands", json!({}))? {
        Value::Array(list) => Ok(list),
        other => Err(format!("engine.commands returned {} instead of a list", kind(&other))),
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn command_list(backend: &mut dyn Backend, args: &Map<String, Value>) -> Result<ToolResult, String> {
    let prefix = match args.get("prefix") {
        None | Some(Value::Null) => "",
        Some(Value::String(p)) => p.as_str(),
        Some(_) => return Err("`prefix` must be a string".into()),
    };
    let list: Vec<Value> =
        commands(backend)?.into_iter().filter(|c| c.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with(prefix))).collect();
    Ok(ToolResult::json(&json!({"count": list.len(), "commands": list})))
}

/// A validated `{id, params?}`.
struct Step {
    id: String,
    params: Value,
}

/// Check one `{id, params?}` object, as given to `command_run` or inside `command_batch`.
fn step(v: &Map<String, Value>) -> Result<Step, String> {
    if let Some(key) = v.keys().find(|k| !matches!(k.as_str(), "id" | "params")) {
        return Err(format!("unknown field `{key}` (expected `id` and `params`)"));
    }
    let id = match v.get("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => id.trim(),
        Some(Value::String(_)) => return Err("`id` is empty".into()),
        Some(_) => return Err("`id` must be a string".into()),
        None => return Err("missing `id`".into()),
    };
    if id.chars().count() > MAX_ID_CHARS {
        return Err(format!("`id` is longer than {MAX_ID_CHARS} characters"));
    }
    let params = match v.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return Err("`params` must be an object".into()),
    };
    Ok(Step { id: id.to_string(), params })
}

fn execute(backend: &mut dyn Backend, s: &Step) -> Result<Value, String> {
    backend.call("engine.execute", json!({"command": s.id, "params": s.params}))
}

fn command_run(backend: &mut dyn Backend, args: &Map<String, Value>) -> Result<ToolResult, String> {
    let s = step(args)?;
    let before = issues::live(backend);
    let result = execute(backend, &s).map_err(|e| format!("{} failed: {e}", s.id))?;
    let Some(report) = issues::report(backend, before.as_deref()) else {
        return Ok(ToolResult::json(&result));
    };
    match result {
        // The usual case: an object result gains the two keys.
        Value::Object(mut o) if !o.contains_key("issues") && !o.contains_key("openIssues") => {
            o.extend(report);
            Ok(ToolResult::json(&Value::Object(o)))
        }
        // A result that is not an object, or that has its own `issues` (such as `validate.run`),
        // stays exactly as the command returned it; the report follows as a second text block.
        other => {
            let mut r = ToolResult::json(&other);
            r.content.push(json!({"type": "text", "text": Value::Object(report).to_string()}));
            Ok(r)
        }
    }
}

fn command_batch(backend: &mut dyn Backend, args: &Map<String, Value>) -> ToolResult {
    let list = match args.get("commands") {
        Some(Value::Array(list)) => list,
        Some(_) => return ToolResult::error("`commands` must be an array of {id, params?}"),
        None => return ToolResult::error("missing `commands`"),
    };
    if list.is_empty() {
        return ToolResult::error("`commands` is empty");
    }
    if list.len() > MAX_BATCH {
        return ToolResult::error(format!("`commands` has {} entries; at most {MAX_BATCH} are allowed per call", list.len()));
    }
    // Check every entry before running any, so a typo at the end does not leave half a batch done.
    let mut steps = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let checked = match item {
            Value::Object(o) => step(o),
            _ => Err("each entry must be an object {id, params?}".into()),
        };
        match checked {
            Ok(s) => steps.push(s),
            Err(e) => return ToolResult::error(format!("commands[{i}]: {e}; nothing was run")),
        }
    }
    let before = issues::live(backend);
    let mut results = Vec::with_capacity(steps.len());
    for (i, s) in steps.iter().enumerate() {
        match execute(backend, s) {
            Ok(r) => results.push(json!({"id": s.id, "result": r})),
            Err(e) => {
                // The commands before the failure stay applied, so what they raised is reported too.
                let mut body = json!({
                    "completed": i,
                    "failedIndex": i,
                    "failedId": s.id,
                    "error": e,
                    "results": results,
                });
                add_issues(&mut body, issues::report(backend, before.as_deref()));
                return ToolResult::error_json(&body);
            }
        }
    }
    let mut body = json!({"completed": results.len(), "results": results});
    add_issues(&mut body, issues::report(backend, before.as_deref()));
    ToolResult::json(&body)
}

/// Put the `issues` and `openIssues` keys, when there are any, into a batch result object.
fn add_issues(body: &mut Value, report: Option<Map<String, Value>>) {
    if let (Value::Object(o), Some(report)) = (body, report) {
        o.extend(report);
    }
}

/// A fresh file name in the system temporary directory for one screenshot.
fn screenshot_path() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("vizcraft-mcp-{}-{n}.png", std::process::id()))
}

fn screenshot(backend: &mut dyn Backend) -> Result<ToolResult, String> {
    if !backend.has_ui() {
        return Err("screenshot needs the running app; this backend has no window".into());
    }
    let path = screenshot_path();
    let path_text = path.to_string_lossy().to_string();
    let reply = backend.call("ui.screenshot", json!({"path": path_text}));
    // Read back only the file we named, whatever the reply says, and always remove it.
    let png = reply.and_then(|_| read_png(&path));
    std::fs::remove_file(&path).ok();
    let (png, width, height) = png?;
    Ok(ToolResult {
        content: vec![
            json!({"type": "image", "data": base64::encode(&png), "mimeType": "image/png"}),
            json!({"type": "text", "text": json!({"width": width, "height": height, "bytes": png.len()}).to_string()}),
        ],
        is_error: false,
    })
}

/// Read a PNG the app wrote, with its size from the header.
fn read_png(path: &std::path::Path) -> Result<(Vec<u8>, u32, u32), String> {
    let shown = path.display();
    let len = std::fs::metadata(path).map_err(|e| format!("screenshot was not written to {shown}: {e}"))?.len();
    if len > MAX_PNG_BYTES {
        return Err(format!("screenshot is {len} bytes, more than the {MAX_PNG_BYTES} byte limit"));
    }
    let png = std::fs::read(path).map_err(|e| format!("cannot read screenshot {shown}: {e}"))?;
    if !png.starts_with(PNG_SIGNATURE) {
        return Err(format!("{shown} is not a PNG file"));
    }
    // IHDR is always the first chunk: width and height are big-endian at bytes 16..24.
    let be = |at: usize| png.get(at..at + 4).and_then(|b| <[u8; 4]>::try_from(b).ok()).map_or(0, u32::from_be_bytes);
    let (width, height) = (be(16), be(20));
    Ok((png, width, height))
}
