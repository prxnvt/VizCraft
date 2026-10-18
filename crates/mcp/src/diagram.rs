//! The Cascade 3 diagram tools: open draw.io XML, Mermaid or CSV, list, read and replace pages,
//! search the stencil library, and a reference for agents that write diagrams; plus `describe`,
//! the cheap structure read (`doc.describe`).
//!
//! Each tool except `get_reference` is one engine command sent through `engine.execute`, so undo,
//! the control channel and the CLI see exactly what an agent did. The tools only check and
//! rename arguments; the behaviour lives in the engine.

use serde_json::{Map, Value, json};

use crate::backend::Backend;
use crate::reference;
use crate::tools::{ToolResult, object, tool};

/// Longest file path accepted; real paths are far shorter.
const MAX_PATH_CHARS: usize = 4096;
/// Longest page name accepted.
const MAX_PAGE_CHARS: usize = 256;
/// Longest search query accepted.
const MAX_QUERY_CHARS: usize = 1000;
/// Most search results one call may ask for.
const MAX_SEARCH_LIMIT: u64 = 200;
/// Most pages one `describe` may name.
const MAX_DESCRIBE_PAGES: u64 = 1000;
/// Highest zero-based page index accepted; documents have far fewer pages.
const MAX_PAGE_INDEX: u64 = 100_000;

/// The names of the diagram tools, in `tools/list` order.
pub(crate) const NAMES: &[&str] =
    &["open_xml", "open_mermaid", "open_csv", "list_pages", "get_page", "set_page", "search_shapes", "get_reference", "describe"];

fn path_prop(what: &str) -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": MAX_PATH_CHARS, "description": what})
}

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

fn layout_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["layered", "tree", "radial", "force"],
        "description": "Place the shapes automatically: `layered` for flows, `tree` for hierarchies, `radial` for a tree on rings, `force` for networks",
    })
}

fn direction_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["TB", "BT", "LR", "RL"],
        "description": "Flow direction for the layered and tree layouts: top to bottom, bottom to top, left to right or right to left",
    })
}

fn routing_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["orthogonal"],
        "description": "`orthogonal`: route every new connector with right angles around the shapes in its way, and keep it so when shapes move",
    })
}

/// The properties shared by the tools that bring content into a page.
fn import_props(text_key: &str, text_what: &str) -> Map<String, Value> {
    let mut p = Map::new();
    p.insert(text_key.into(), json!({"type": "string", "minLength": 1, "description": text_what}));
    p.insert(
        "page".into(),
        page_prop("Page to fill, by id, name or zero-based index (default: the first page). A name no page has adds a new page with it"),
    );
    p.insert("layout".into(), layout_prop());
    p.insert("direction".into(), direction_prop());
    p.insert("routing".into(), routing_prop());
    p
}

const IMPORT_RESULT: &str = "Returns `{page, ids, warnings}`.";

/// Tool definitions for `tools/list`.
pub(crate) fn definitions() -> Vec<Value> {
    let mut defs = vec![
        tool(
            "open_xml",
            "Open draw.io XML",
            &format!(
                "Replace a page's content with a draw.io XML diagram (`mxfile` or `mxGraphModel`); coordinates are kept unless `layout` is given. One undo step; see get_reference `xml`. {IMPORT_RESULT}"
            ),
            object(Value::Object(import_props("xml", "The diagram as draw.io XML")), &["xml"]),
            false,
        ),
        tool(
            "open_mermaid",
            "Open Mermaid",
            &format!(
                "Replace a page's content with a Mermaid flowchart, sequence, class, state or ER diagram, laid out automatically. One undo step. {IMPORT_RESULT}"
            ),
            object(
                Value::Object(import_props("source", "The Mermaid text, starting with its diagram kind (`flowchart LR`, `sequenceDiagram`, ...)")),
                &["source"],
            ),
            false,
        ),
        tool(
            "open_csv",
            "Open CSV",
            &format!(
                "Replace a page's content with a diagram built from a CSV table with `#` header lines; see get_reference `csv`. One undo step. {IMPORT_RESULT}"
            ),
            object(Value::Object(import_props("csv", "The header lines followed by the CSV table")), &["csv"]),
            false,
        ),
        tool(
            "list_pages",
            "List pages",
            "List pages as `{index, id, name, width, height, shapes, connectors}`, of the open document or of the .drawio or .vizcraft file at `path`.",
            object(json!({"path": path_prop("A .drawio or .vizcraft file to read instead of the open document")}), &[]),
            true,
        ),
        tool(
            "get_page",
            "Get a page",
            "Return one page as draw.io XML (default) or VizCraft JSON, with every style: `{page, format, content}`. To learn what connects to what, use describe or page.describe, which cost a fraction.",
            object(
                json!({
                    "page": page_prop("The page, by id, name or zero-based index"),
                    "path": path_prop("A .drawio or .vizcraft file to read instead of the open document"),
                    "format": {"type": "string", "enum": ["drawio", "vizcraft"], "description": "`drawio` for mxGraphModel XML (default), `vizcraft` for the native JSON"},
                }),
                &["page"],
            ),
            true,
        ),
        tool(
            "set_page",
            "Set a page",
            &format!(
                "Replace one page with draw.io XML (a new name adds a page); with `path` it edits that file on disk and leaves the open document alone. {IMPORT_RESULT}"
            ),
            object(
                json!({
                    "page": page_prop("The page to replace, by id, name or zero-based index; a new name adds a page"),
                    "xml": {"type": "string", "minLength": 1, "description": "The page as draw.io XML (a bare mxGraphModel or an mxfile whose first diagram is used)"},
                    "path": path_prop("A .drawio or .vizcraft file to edit on disk instead of the open document"),
                    "layout": layout_prop(),
                    "direction": direction_prop(),
                    "routing": routing_prop(),
                }),
                &["page", "xml"],
            ),
            false,
        ),
        tool(
            "search_shapes",
            "Search shapes",
            "Find stencil masters whose id, name or keywords contain every word of `query` (empty lists all): `{masters: [{id, label, category, keywords, w, h, style}]}`.",
            object(
                json!({
                    "query": {"type": "string", "maxLength": MAX_QUERY_CHARS, "description": "Words to look for, such as `decision` or `database`"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_SEARCH_LIMIT, "description": "Most results to return (default 20)"},
                }),
                &["query"],
            ),
            true,
        ),
        tool(
            "get_reference",
            "Diagram reference",
            "Notes on the draw.io XML, styles, masters, layouts, Mermaid, CSV and notation models VizCraft accepts; give a `topic` or get every section. Needs no open document.",
            object(json!({"topic": {"type": "string", "enum": reference::topic_names(), "description": "One section of the reference"}}), &[]),
            true,
        ),
        tool(
            "describe",
            "Describe the document",
            "Read what is in the document as ids, stencils, texts, connections and containment, with no colours or coordinates; start here. `detail: layout` adds positions, `full` adds style. `format: text` is a compact outline (far fewer tokens) returned as plain text.",
            object(
                json!({
                    "detail": crate::tools::detail_prop(),
                    "format": {"type": "string", "enum": ["json", "text"], "description": "`json` (default) or `text`, an indented outline: one line per shape and connector"},
                    "pages": {
                        "type": "array",
                        "items": page_prop("A page, by id, name or zero-based index"),
                        "maxItems": MAX_DESCRIBE_PAGES,
                        "description": "Only these pages (default: all)",
                    },
                }),
                &[],
            ),
            true,
        ),
    ];
    // Importing replaces a page's content and set_page can rewrite a file; say so to clients
    // that ask before running such tools.
    for def in &mut defs {
        let name = def.get("name").and_then(Value::as_str).unwrap_or_default();
        if matches!(name, "open_xml" | "open_mermaid" | "open_csv" | "set_page")
            && let Some(a) = def.get_mut("annotations").and_then(Value::as_object_mut)
        {
            a.insert("destructiveHint".into(), json!(true));
        }
    }
    defs
}

/// Whether `name` is a diagram tool.
pub(crate) fn is_diagram_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Check `args` against the input schema of diagram tool `name`: an object, every required key
/// present, no unknown key, and each value of the declared type, enum, length and range. Null
/// counts as absent. Other tools are not checked here.
pub(crate) fn validate(name: &str, args: &Value) -> Result<(), String> {
    if !is_diagram_tool(name) {
        return Ok(());
    }
    let Some(def) = definitions().into_iter().find(|t| t.get("name").and_then(Value::as_str) == Some(name)) else {
        return Ok(());
    };
    let empty = Map::new();
    let args = match args {
        Value::Object(o) => o,
        Value::Null => &empty,
        _ => return Err("`arguments` must be an object".into()),
    };
    let schema = def.get("inputSchema").cloned().unwrap_or(Value::Null);
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(key) = args.keys().find(|k| !props.contains_key(*k)) {
        let known: Vec<&str> = props.keys().map(String::as_str).collect();
        return Err(format!("unknown argument `{}` (expected {})", clip(key), list(&known)));
    }
    for key in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if args.get(key).is_none_or(Value::is_null) {
            return Err(format!("missing `{key}`"));
        }
    }
    for (key, value) in args {
        if value.is_null() {
            continue;
        }
        if let Some(prop) = props.get(key) {
            check_value(key, value, prop)?;
        }
    }
    Ok(())
}

/// Check one argument against its property schema (the subset our schemas use).
fn check_value(key: &str, value: &Value, prop: &Value) -> Result<(), String> {
    let types: Vec<&str> = match prop.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    let ok_type = types.iter().any(|t| match *t {
        "string" => value.is_string(),
        "integer" => value.is_u64() || value.is_i64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        _ => false,
    });
    if !types.is_empty() && !ok_type {
        let wanted: Vec<String> = types.iter().map(|t| format!("a {t}")).collect();
        return Err(format!("`{key}` must be {}", wanted.join(" or ")));
    }
    if let Some(items) = value.as_array() {
        if let Some(max) = prop.get("maxItems").and_then(Value::as_u64)
            && items.len() as u64 > max
        {
            return Err(format!("`{key}` has more than {max} entries"));
        }
        if let Some(item_schema) = prop.get("items") {
            for (i, item) in items.iter().enumerate() {
                check_value(&format!("{key}[{i}]"), item, item_schema)?;
            }
        }
    }
    if let Some(allowed) = prop.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        let names: Vec<&str> = allowed.iter().filter_map(Value::as_str).collect();
        return Err(format!("`{key}` must be one of {}", list(&names)));
    }
    if let Some(t) = value.as_str() {
        let n = t.chars().count() as u64;
        if let Some(min) = prop.get("minLength").and_then(Value::as_u64)
            && n < min
        {
            return Err(format!("`{key}` is empty"));
        }
        if let Some(max) = prop.get("maxLength").and_then(Value::as_u64)
            && n > max
        {
            return Err(format!("`{key}` is longer than {max} characters"));
        }
    }
    if let Some(n) = value.as_i64().map(|n| n as f64).or_else(|| value.as_u64().map(|n| n as f64)) {
        let min = prop.get("minimum").and_then(Value::as_f64);
        let max = prop.get("maximum").and_then(Value::as_f64);
        if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
            return Err(format!("`{key}` must be from {} to {}", min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX)));
        }
    }
    Ok(())
}

/// Names as a readable list: `a`, `b`, `c`.
fn list(names: &[&str]) -> String {
    names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

/// A key short enough to quote back in an error.
fn clip(t: &str) -> String {
    let short: String = t.chars().take(64).collect();
    if short.len() < t.len() { format!("{short}...") } else { short }
}

/// Run diagram tool `name`. Arguments are checked first; a bad one is a tool error here (the
/// server rejects it as invalid params before it gets this far).
pub(crate) fn call(backend: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    if let Err(e) = validate(name, args) {
        return ToolResult::error(e);
    }
    let empty = Map::new();
    let args = args.as_object().unwrap_or(&empty);
    let (command, params) = match name {
        "open_xml" => ("file.import", import_params(args, "xml", "drawio")),
        "open_mermaid" => ("file.import", import_params(args, "source", "mermaid")),
        "open_csv" => ("file.import", import_params(args, "csv", "csv")),
        "list_pages" => ("page.list", pick(args, &["path"])),
        "get_page" => ("page.get", pick(args, &["page", "path", "format"])),
        "set_page" => ("page.set", pick(args, &["page", "xml", "path", "layout", "direction", "routing"])),
        "search_shapes" => ("shape.search", pick(args, &["query", "limit"])),
        "get_reference" => return reference_text(args),
        "describe" => return describe(backend, args),
        other => return ToolResult::error(format!("unknown tool `{other}`")),
    };
    match backend.call("engine.execute", json!({"command": command, "params": params})) {
        Ok(result) => ToolResult::json(&result),
        Err(e) => ToolResult::error(format!("{command} failed: {e}")),
    }
}

/// The given, non-null arguments among `keys`, as a params object.
fn pick(args: &Map<String, Value>, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for key in keys {
        if let Some(v) = args.get(*key).filter(|v| !v.is_null()) {
            out.insert((*key).to_string(), v.clone());
        }
    }
    Value::Object(out)
}

/// `file.import` params for an open_* tool: the text under `text`, the fixed format, and
/// `replace: true`, because these tools set a page's content rather than add to it.
fn import_params(args: &Map<String, Value>, text_key: &str, format: &str) -> Value {
    let mut params = Map::new();
    if let Some(text) = args.get(text_key) {
        params.insert("text".into(), text.clone());
    }
    params.insert("format".into(), json!(format));
    params.insert("replace".into(), json!(true));
    if let Value::Object(rest) = pick(args, &["page", "layout", "direction", "routing"]) {
        params.extend(rest);
    }
    Value::Object(params)
}

/// `describe` is `doc.describe` with the same params. A `format: "text"` outline comes back as
/// plain text, because JSON would escape every quote and line break in it.
fn describe(backend: &mut dyn Backend, args: &Map<String, Value>) -> ToolResult {
    let params = pick(args, &["detail", "format", "pages"]);
    match backend.call("engine.execute", json!({"command": "doc.describe", "params": params})) {
        Ok(result) => match result.get("text").and_then(Value::as_str) {
            Some(text) => ToolResult::text(text),
            None => ToolResult::json(&result),
        },
        Err(e) => ToolResult::error(format!("doc.describe failed: {e}")),
    }
}

fn reference_text(args: &Map<String, Value>) -> ToolResult {
    match args.get("topic").and_then(Value::as_str) {
        None => ToolResult::text(reference::everything()),
        Some(t) => match reference::topic(t) {
            Some(body) => ToolResult::text(body),
            None => ToolResult::error(format!("unknown topic `{}`; the topics are {}", clip(t), list(&reference::topic_names()))),
        },
    }
}
