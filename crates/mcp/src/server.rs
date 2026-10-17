//! JSON-RPC 2.0 framing and the MCP lifecycle, tools and resources methods.

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Value, json};

use crate::MAX_MESSAGE_BYTES;
use crate::backend::Backend;
use crate::diagram;
use crate::line::{Line, read_bounded};
use crate::resources;
use crate::tools::{call_tool, is_tool, tool_definitions};

/// The MCP revision answered when the client asks for one we do not know.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// The newest revision we speak; it adds cache hints to list and read results.
const MODERN_VERSION: &str = "2026-07-28";
const SUPPORTED_VERSIONS: &[&str] = &[MODERN_VERSION, "2025-11-25", PROTOCOL_VERSION, "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;
pub(crate) const INTERNAL_ERROR: i64 = -32603;

/// Most messages answered from one JSON-RPC batch array (older clients only).
const MAX_RPC_BATCH: usize = 64;

const INSTRUCTIONS: &str = "VizCraft is a diagram editor. Every action in it is a command with an id \
(`shape.add`, `connector.add`, `edit.undo`, `file.saveAs`): find commands and their params with \
command_list, run one with command_run or several in order with command_batch. Check the result with \
doc_inspect (JSON) or screenshot (PNG of the window). Coordinates are pixels at 100% zoom with y \
pointing down. For whole \
diagrams, open_xml, open_mermaid and open_csv fill a page from draw.io XML, Mermaid or CSV (with \
optional automatic layout and shape-avoiding routing); list_pages, get_page and set_page read and \
replace pages as draw.io XML; search_shapes finds stencil masters and their style strings; \
get_reference explains the accepted XML, styles, layouts, Mermaid and CSV. Resources (JSON, read-only): \
vizcraft://commands is the same catalog as command_list, vizcraft://stencils every stencil set and master, \
vizcraft://page/{id} one page with its shapes and connectors, and vizcraft://shape/{id} one shape with its \
geometry, text, style, shape data and the connectors glued to it; resources/list names every page. Model-backed diagrams \
(element.create, relationship.create) follow notation packs: read vizcraft://notation and \
vizcraft://notation/{pack} for their schemas, and look at `issues` in command_run and command_batch \
results for the rule problems a call raised (get_reference topic `model`).";

/// An MCP server bound to one backend.
pub struct Server {
    backend: Box<dyn Backend>,
    initialized: bool,
    modern: bool,
    /// Whether this server's client asked for log records (`logging/setLevel`). The queue is
    /// process-wide, so a server whose client did not ask leaves it to the one that did.
    logging: bool,
}

fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

/// The text of a caught panic, for the error reply.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_string())
}

impl Server {
    /// A server answering through `backend`.
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self { backend, initialized: false, modern: false, logging: false }
    }

    /// The backend, for callers that want to reach it directly.
    pub fn backend(&mut self) -> &mut dyn Backend {
        self.backend.as_mut()
    }

    /// Whether the client has sent `notifications/initialized`.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// The `notifications/message` lines queued since the last call.
    ///
    /// Log records belong to no particular request, so these go out after the reply to whatever
    /// line produced them. [`Server::serve`] drains this itself; a caller driving
    /// [`Server::handle_line`] by hand does the same.
    pub fn take_notifications(&mut self) -> Vec<String> {
        if !self.logging {
            return vec![];
        }
        crate::logging::drain()
            .into_iter()
            .map(|record| json!({"jsonrpc": "2.0", "method": "notifications/message", "params": record}).to_string())
            .collect()
    }

    /// Serve newline-delimited JSON-RPC until `input` closes. Only protocol messages are written
    /// to `output`; a bad line gets an error reply and the loop carries on.
    pub fn serve(&mut self, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        while let Some(line) = read_bounded(&mut input, MAX_MESSAGE_BYTES)? {
            let reply = match line {
                Line::Text(text) => self.handle_line(&text),
                Line::TooLong => {
                    Some(error(Value::Null, INVALID_REQUEST, format!("message longer than {MAX_MESSAGE_BYTES} bytes was skipped")).to_string())
                }
                Line::NotUtf8 => Some(error(Value::Null, PARSE_ERROR, "parse error: message is not UTF-8").to_string()),
            };
            let notes = self.take_notifications();
            for line in reply.iter().chain(notes.iter()) {
                output.write_all(line.as_bytes())?;
                output.write_all(b"\n")?;
            }
            output.flush()?;
        }
        Ok(())
    }

    /// Handle one line; returns the reply line (`None` for notifications and blank lines).
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if line.len() > MAX_MESSAGE_BYTES {
            return Some(error(Value::Null, INVALID_REQUEST, format!("message longer than {MAX_MESSAGE_BYTES} bytes was skipped")).to_string());
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) => {
                // JSON-RPC batches left MCP in 2025-06-18; older clients still get answers.
                if batch.is_empty() {
                    Some(error(Value::Null, INVALID_REQUEST, "empty batch"))
                } else if batch.len() > MAX_RPC_BATCH {
                    Some(error(Value::Null, INVALID_REQUEST, format!("batch of {} messages; at most {MAX_RPC_BATCH} are allowed", batch.len())))
                } else {
                    let replies: Vec<Value> = batch.into_iter().filter_map(|m| self.handle(m)).collect();
                    (!replies.is_empty()).then_some(Value::Array(replies))
                }
            }
            Ok(msg) => self.handle(msg),
            Err(e) => Some(error(Value::Null, PARSE_ERROR, format!("parse error: {e}"))),
        };
        reply.map(|r| r.to_string())
    }

    /// Handle one JSON-RPC message; `None` for notifications and responses.
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        let Value::Object(o) = &msg else { return Some(error(Value::Null, INVALID_REQUEST, "message must be an object")) };
        let id = o.get("id").cloned();
        let Some(method) = o.get("method").and_then(Value::as_str) else {
            // A response to a server-to-client request (we send none) is ignored; anything else
            // without a method is invalid.
            if o.contains_key("result") || o.contains_key("error") {
                return None;
            }
            return Some(error(id.unwrap_or(Value::Null), INVALID_REQUEST, "missing `method`"));
        };
        let params = o.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            self.notification(method);
            return None;
        };
        if !(id.is_string() || id.is_number()) {
            return Some(error(Value::Null, INVALID_REQUEST, "`id` must be a string or number"));
        }
        // A bug that panics fails this request only; the server keeps serving.
        let r = catch_unwind(AssertUnwindSafe(|| self.request(method, &params)))
            .unwrap_or_else(|p| Err((INTERNAL_ERROR, format!("internal error in `{method}`: {} (please report this bug)", panic_text(p.as_ref())))));
        Some(match r {
            Ok(mut r) => {
                let modern = params
                    .get("_meta")
                    .and_then(|m| m.get("io.modelcontextprotocol/protocolVersion"))
                    .and_then(Value::as_str)
                    .map_or(self.modern, |v| v == MODERN_VERSION);
                if modern && let Some(result) = r.as_object_mut() {
                    let ttl = match method {
                        "tools/list" | "resources/list" | "resources/templates/list" => Some(600_000),
                        "resources/read" => Some(0),
                        _ => None,
                    };
                    if let Some(ttl) = ttl {
                        result.insert("resultType".into(), json!("complete"));
                        result.insert("ttlMs".into(), json!(ttl));
                        result.insert("cacheScope".into(), json!("private"));
                    }
                }
                response(id, r)
            }
            Err((code, m)) => error(id, code, m),
        })
    }

    fn notification(&mut self, method: &str) {
        match method {
            "notifications/initialized" => self.initialized = true,
            "notifications/cancelled" | "notifications/progress" | "notifications/roots/list_changed" => {}
            other => log::debug!("ignoring notification {other}"),
        }
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION);
                let version = if SUPPORTED_VERSIONS.contains(&asked) { asked } else { PROTOCOL_VERSION };
                self.modern = version == MODERN_VERSION;
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false}, "logging": {}},
                    "serverInfo": {"name": "vizcraft", "title": "VizCraft", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": format!("{INSTRUCTIONS} Backend: {}.", self.backend.describe()),
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tool_definitions()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).ok_or((INVALID_PARAMS, "missing tool `name`".to_string()))?;
                if !is_tool(name) {
                    return Err((INVALID_PARAMS, format!("unknown tool `{name}`")));
                }
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                if let Some(arguments) = args.as_object()
                    && let Some(def) = tool_definitions().into_iter().find(|t| t.get("name").and_then(Value::as_str) == Some(name))
                    && let Some(properties) = def.get("inputSchema").and_then(|s| s.get("properties")).and_then(Value::as_object)
                    && let Some(key) = arguments.keys().find(|k| !properties.contains_key(*k))
                {
                    return Err((INVALID_PARAMS, format!("unknown argument `{key}` for `{name}`")));
                }
                // The diagram tools check every argument against their schema up front, so a
                // wrong type or value is a protocol error rather than a half-run import.
                diagram::validate(name, &args).map_err(|e| (INVALID_PARAMS, format!("`{name}`: {e}")))?;
                Ok(call_tool(self.backend.as_mut(), name, &args).to_value())
            }
            "resources/list" => Ok(resources::list(self.backend.as_mut())),
            "resources/templates/list" => Ok(resources::templates()),
            "resources/read" => resources::read(self.backend.as_mut(), params),
            "logging/setLevel" => {
                // `level` is required. A null level is our own way of turning logging back off,
                // which the spec has no method for and a long session needs.
                let Some(asked) = params.get("level") else {
                    return Err((INVALID_PARAMS, "missing `level`".into()));
                };
                let level = if asked.is_null() { None } else { asked.as_str() };
                crate::logging::set_level(level).map_err(|_| match level {
                    Some(given) => (INVALID_PARAMS, format!("unknown level `{given}`")),
                    None => (INVALID_PARAMS, "`level` must be a severity name or null".to_string()),
                })?;
                self.logging = level.is_some();
                log::debug!("client set logging to {}", level.unwrap_or("off"));
                Ok(json!({}))
            }
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }
}
