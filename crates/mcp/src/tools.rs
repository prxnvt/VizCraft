//! The tool list and dispatch. The Cascade 1 tools here give generic access to the command
//! registry, the document summary and a screenshot (of the window, or of one page rendered by
//! the engine's PNG export); every new engine command is reachable through `command_run` with no
//! change here. The diagram tools live in `diagram.rs`.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};

use crate::backend::Backend;
use crate::{MAX_BATCH, base64, issues};

/// Largest screenshot the server will read back and inline, in bytes. A full window on a large
/// Apple Retina display is a few megabytes; anything far beyond that is not a screenshot we asked
/// for.
const MAX_PNG_BYTES: u64 = 48 * 1024 * 1024;

/// The eight bytes every PNG file starts with.
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Longest command id accepted; real ids are a few dozen characters.
const MAX_ID_CHARS: usize = 200;

/// Longest page name accepted by `screenshot` and `doc_inspect`.
const MAX_PAGE_CHARS: usize = 256;
/// Highest zero-based page index accepted; documents have far fewer pages.
const MAX_PAGE_INDEX: u64 = 100_000;
/// Smallest and largest page render scale `screenshot` accepts, the range the renderer uses.
const MIN_SCALE: f64 = 0.25;
const MAX_SCALE: f64 = 4.0;

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
            "List commands as `{id, label, location, shortcut, params, enabled}`. `prefix` keeps ids that start with it; `brief` returns only `{id, label}`.",
            object(
                json!({
                    "prefix": {"type": "string", "description": "Only commands whose id starts with this, such as `shape.`"},
                    "brief": {"type": "boolean", "description": "true: return only `{id, label}` per command"},
                }),
                &[],
            ),
            true,
        ),
        tool(
            "command_run",
            "Run a command",
            "Run one command by id with optional params, as one undo step; it never opens a dialog. With a notation model the result also carries `issues` and `openIssues`.",
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
            "Run up to 256 commands in order as one undo step; a failure rolls the whole batch back and names its index. Batches with ui.*, app.* or undo and redo entries run step by step instead.",
            object(json!({"commands": {"type": "array", "items": command_item, "minItems": 1, "maxItems": MAX_BATCH}}), &["commands"]),
            false,
        ),
        tool(
            "doc_inspect",
            "Inspect the document",
            "Return the app state: title, path, revision, selection, undo labels, zoom. `page` adds that page's entry; `detail` adds `pages` with the structure, layout or full description of the pages (see describe).",
            object(
                json!({
                    "page": page_prop("A page to report on, by id, name or zero-based index"),
                    "detail": detail_prop(),
                }),
                &[],
            ),
            true,
        ),
        tool(
            "screenshot",
            "Take a screenshot",
            "Return a PNG with its size in pixels: the app window, or with `page`, `scale`, `window: false` or on the headless backend, one page rendered offscreen.",
            object(
                json!({
                    "page": page_prop("The page to render, by id, name or zero-based index (default: the first page)"),
                    "window": {"type": "boolean", "description": "true: capture the app window (needs the running app); false: render a page"},
                    "scale": {
                        "type": "number",
                        "minimum": MIN_SCALE,
                        "maximum": MAX_SCALE,
                        "description": "Pixels per page unit when rendering a page (default 1)",
                    },
                }),
                &[],
            ),
            true,
        ),
    ]
}

/// The `detail` schema shared by `doc_inspect` and `describe`.
pub(crate) fn detail_prop() -> Value {
    json!({
        "type": "string",
        "enum": crate::resources::DETAILS,
        "description": "`structure` (default): ids, stencils, texts, connections, containment, data rows. `layout` adds positions and sizes. `full` adds style and everything else",
    })
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
        "doc_inspect" => doc_inspect(backend, args),
        "screenshot" => screenshot(backend, args),
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
    let brief = match args.get("brief") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("`brief` must be true or false".into()),
    };
    let list: Vec<Value> =
        commands(backend)?.into_iter().filter(|c| c.get("id").and_then(Value::as_str).is_some_and(|id| id.starts_with(prefix))).collect();
    let list: Vec<Value> = if brief {
        list.iter()
            .map(|c| json!({"id": c.get("id").cloned().unwrap_or(Value::Null), "label": c.get("label").cloned().unwrap_or(Value::Null)}))
            .collect()
    } else {
        list
    };
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
    if steps.iter().all(|s| atomic(&s.id))
        && let Some(r) = command_batch_atomic(backend, &steps, before.as_deref())
    {
        return r;
    }
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

/// Whether a batch entry can run inside the app's `edit.batch` transaction: engine commands other
/// than undo and redo. A batch with a `ui.*` or `app.*` entry, or with undo or redo, runs entry by
/// entry instead.
fn atomic(id: &str) -> bool {
    !(id.starts_with("ui.") || id.starts_with("app.") || matches!(id, "edit.undo" | "edit.redo"))
}

/// Run the whole batch as one `edit.batch` call, so it is one undo step and a failure leaves the
/// document unchanged. `None` when the app predates `edit.batch`; the caller then runs the entries
/// one by one.
fn command_batch_atomic(backend: &mut dyn Backend, steps: &[Step], before: Option<&[Value]>) -> Option<ToolResult> {
    let commands: Vec<Value> = steps.iter().map(|s| json!({"command": s.id, "params": s.params})).collect();
    match backend.call("engine.execute", json!({"command": BATCH_COMMAND, "params": {"label": BATCH_LABEL, "commands": commands}})) {
        Ok(reply) => {
            let listed = reply.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
            let results: Vec<Value> = steps
                .iter()
                .zip(listed.into_iter().map(|r| r.get("result").cloned().unwrap_or(Value::Null)).chain(std::iter::repeat(Value::Null)))
                .map(|(s, r)| json!({"id": s.id, "result": r}))
                .collect();
            let mut body = json!({"completed": results.len(), "results": results});
            add_issues(&mut body, issues::report(backend, before));
            Some(ToolResult::json(&body))
        }
        Err(e) if e.contains(&format!("unknown command `{BATCH_COMMAND}`")) => None,
        Err(e) => {
            // `edit.batch` names the failing entry as `commands[i] (id) failed: ...`.
            let index =
                e.strip_prefix("commands[").and_then(|t| t.split(']').next()).and_then(|n| n.parse::<usize>().ok()).filter(|i| *i < steps.len());
            let mut body = json!({
                "completed": 0,
                "failedIndex": index,
                "failedId": index.and_then(|i| steps.get(i)).map(|s| s.id.clone()),
                "error": e,
                "results": [],
                "rolledBack": true,
            });
            add_issues(&mut body, issues::report(backend, before));
            Some(ToolResult::error_json(&body))
        }
    }
}

/// The engine command that runs a list of commands as one transaction.
const BATCH_COMMAND: &str = "edit.batch";

/// The undo label of an agent's batch.
const BATCH_LABEL: &str = "Agent Batch";

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

/// The `page` schema shared by `screenshot` and `doc_inspect`: an id or name, or an index.
fn page_prop(what: &str) -> Value {
    json!({
        "type": ["string", "integer"],
        "minLength": 1,
        "maxLength": MAX_PAGE_CHARS,
        "minimum": 0,
        "maximum": MAX_PAGE_INDEX,
        "description": what,
    })
}

/// The optional `page` argument checked against [`page_prop`]: a non-empty string or an index.
fn page_arg(args: &Map<String, Value>) -> Result<Option<Value>, String> {
    match args.get("page") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if t.is_empty() => Err("`page` is empty".into()),
        Some(Value::String(t)) if t.chars().count() > MAX_PAGE_CHARS => Err(format!("`page` is longer than {MAX_PAGE_CHARS} characters")),
        Some(p @ Value::String(_)) => Ok(Some(p.clone())),
        Some(Value::Number(n)) => match n.as_u64() {
            Some(i) if i <= MAX_PAGE_INDEX => Ok(Some(json!(i))),
            _ => Err(format!("`page` must be a name, an id or an index from 0 to {MAX_PAGE_INDEX}")),
        },
        Some(_) => Err("`page` must be a string or an integer".into()),
    }
}

/// `doc_inspect {page?, detail?}`: `ui.inspect`, plus the `page.list` entry for `page` when given,
/// plus the `doc.describe` pages at `detail` when given.
fn doc_inspect(backend: &mut dyn Backend, args: &Map<String, Value>) -> Result<ToolResult, String> {
    let page = page_arg(args)?;
    let detail = match args.get("detail") {
        None | Some(Value::Null) => None,
        Some(Value::String(d)) if crate::resources::DETAILS.contains(&d.as_str()) => Some(d.clone()),
        Some(_) => return Err(format!("`detail` must be one of {}", crate::resources::DETAILS.join(", "))),
    };
    let mut report = backend.call("ui.inspect", json!({}))?;
    if let Some(r) = page.clone() {
        let listed = backend.call("engine.execute", json!({"command": "page.list", "params": {}}))?;
        let pages = listed.get("pages").and_then(Value::as_array).cloned().unwrap_or_default();
        // The same order the engine resolves page references in: id, then name, then index.
        let field = |key: &str| pages.iter().find(|p| p.get(key).is_some_and(|v| *v == r)).cloned();
        let by_index = || r.as_str().and_then(|t| t.trim().parse::<u64>().ok()).map(|i| json!(i)).or_else(|| r.as_u64().map(|i| json!(i)));
        let entry =
            field("id").or_else(|| field("name")).or_else(|| by_index().and_then(|i| pages.iter().find(|p| p.get("index") == Some(&i)).cloned()));
        let Some(entry) = entry else {
            return Err(format!("no page {r} (list_pages shows the pages)"));
        };
        match report.as_object_mut() {
            Some(obj) => {
                obj.insert("page".into(), entry);
            }
            None => report = json!({"inspect": report, "page": entry}),
        }
    }
    if let Some(detail) = detail {
        let mut params = json!({"detail": detail});
        if let (Some(o), Some(p)) = (params.as_object_mut(), &page) {
            o.insert("pages".into(), json!([p]));
        }
        let described =
            backend.call("engine.execute", json!({"command": "doc.describe", "params": params})).map_err(|e| format!("doc.describe failed: {e}"))?;
        let pages = described.get("pages").cloned().unwrap_or(Value::Null);
        match report.as_object_mut() {
            Some(obj) => {
                obj.insert("pages".into(), pages);
            }
            None => report = json!({"inspect": report, "pages": pages}),
        }
    }
    Ok(ToolResult::json(&report))
}

/// `screenshot {page?, window?, scale?}`: the app window, or one page rendered to PNG.
fn screenshot(backend: &mut dyn Backend, args: &Map<String, Value>) -> Result<ToolResult, String> {
    let page = page_arg(args)?;
    let window = match args.get("window") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err("`window` must be true or false".into()),
    };
    let scale = match args.get("scale") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(f) if (MIN_SCALE..=MAX_SCALE).contains(&f) => Some(f),
            _ => return Err(format!("`scale` must be a number from {MIN_SCALE} to {MAX_SCALE}")),
        },
    };
    let render_page = page.is_some() || scale.is_some() || window == Some(false) || !backend.has_ui();
    if window == Some(true) {
        if !backend.has_ui() {
            return Err("screenshot `window: true` needs the running app; this backend has no window (leave `window` out to render a page)".into());
        }
        if render_page {
            return Err("`window: true` captures the whole window; it takes no `page` or `scale`".into());
        }
    }
    let path = screenshot_path();
    let path_text = path.to_string_lossy().to_string();
    let reply = if render_page {
        let mut params = Map::new();
        params.insert("path".into(), json!(path_text));
        params.insert("format".into(), json!("png"));
        if let Some(p) = &page {
            params.insert("page".into(), p.clone());
        }
        if let Some(f) = scale {
            params.insert("scale".into(), json!(f));
        }
        backend.call("engine.execute", json!({"command": "file.export", "params": params})).map_err(|e| format!("file.export failed: {e}"))
    } else {
        backend.call("ui.screenshot", json!({"path": path_text}))
    };
    // Read back only the file we named, whatever the reply says, and always remove it.
    let page_id = reply.as_ref().ok().and_then(|r| r.get("page")).filter(|p| render_page && !p.is_null()).cloned();
    let png = reply.and_then(|_| read_png(&path));
    std::fs::remove_file(&path).ok();
    let (png, width, height) = png?;
    let mut size = json!({"width": width, "height": height, "bytes": png.len()});
    if let (Some(obj), Some(id)) = (size.as_object_mut(), page_id) {
        obj.insert("page".into(), id);
    }
    Ok(ToolResult {
        content: vec![
            json!({"type": "image", "data": base64::encode(&png), "mimeType": "image/png"}),
            json!({"type": "text", "text": size.to_string()}),
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
