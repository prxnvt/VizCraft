//! The in-process backend: an engine [`Session`] with no window.
//!
//! It answers the document half of the control protocol (`engine.execute`, `engine.commands`,
//! `ui.inspect`) straight from the engine, so an MCP client can build diagrams and get page
//! images without the desktop app running. Methods that need a window (`ui.click`,
//! `ui.screenshot`, `app.quit`, ...) are refused with a message that says so; page images come
//! from the `file.export` PNG renderer instead (see the `screenshot` tool).

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};

use crate::backend::Backend;

/// Why a window method cannot run here, in the words an agent sees.
const NO_WINDOW: &str = "the headless backend has no window";

/// An engine session owned by the MCP server, starting from a blank document.
pub struct Headless {
    session: Session,
}

impl Default for Headless {
    fn default() -> Self {
        Self::new()
    }
}

impl Headless {
    /// A backend over a fresh session with a blank document.
    pub fn new() -> Self {
        Self::with_session(Session::new(Document::new()))
    }

    /// A backend over an existing session (for example one with a file already open).
    pub fn with_session(session: Session) -> Self {
        Self { session }
    }

    /// The session, for callers that want to look at the document directly.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// `engine.execute {command, params?}`: run one engine command. A command that asks the UI
    /// for something (a file picker, a dialog) has nobody to ask here; the request is dropped and
    /// named in the result's `warnings`, as `vizcraft-cli run` does on stderr.
    fn execute(&mut self, params: &Value) -> Result<Value, String> {
        let id = params.get("command").or_else(|| params.get("id")).and_then(Value::as_str).ok_or("missing `command`")?;
        if id.starts_with("ui.") || id.starts_with("app.") {
            return Err(format!("`{}` needs the app window; {NO_WINDOW}", vizcraft_engine::cmd::short(id)));
        }
        let args = params.get("params").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| json!({}));
        let result = self.session.run(id, &args);
        let dropped: Vec<String> =
            std::mem::take(&mut self.session.ui_requests).into_iter().map(|r| format!("UI request ignored (no window): {r}")).collect();
        let mut result = result.map_err(|e| e.to_string())?;
        if !dropped.is_empty() {
            match result.as_object_mut() {
                Some(obj) => {
                    let mut warnings = match obj.remove("warnings") {
                        Some(Value::Array(list)) => list,
                        Some(Value::Null) | None => vec![],
                        Some(other) => vec![other],
                    };
                    warnings.extend(dropped.into_iter().map(Value::String));
                    obj.insert("warnings".into(), Value::Array(warnings));
                }
                None => result = json!({"result": result, "warnings": dropped}),
            }
        }
        Ok(result)
    }

    /// `ui.inspect`: the document part of the app's report, marked `headless`.
    fn inspect(&self) -> Value {
        let s = &self.session;
        let title = match &s.path {
            Some(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                vizcraft_engine::io::strip_extension(&name).to_string()
            }
            None => s.doc.title.clone(),
        };
        let page = s.doc.page();
        json!({
            "headless": true,
            "title": if title.is_empty() { "Untitled".to_string() } else { title },
            "path": s.path,
            "dirty": s.dirty,
            "rev": s.rev(),
            "selection": &s.sel,
            "canUndo": s.can_undo(),
            "canRedo": s.can_redo(),
            "undoLabel": s.undo_label(),
            "redoLabel": s.redo_label(),
            "view": &s.view,
            "status": if s.status.is_empty() { None } else { Some(s.status.as_str()) },
            "pages": s.doc.pages.len(),
            "diagram": {
                "page": page.map(|p| json!({"id": p.id, "name": p.name, "width": p.width, "height": p.height})),
                "shapes": page.map_or(0, |p| p.shapes.len()),
                "connectors": page.map_or(0, |p| p.connectors.len()),
                "selected": &s.sel.items,
            },
        })
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "engine.execute" | "command" => self.execute(&params),
            "engine.commands" => serde_json::to_value(self.session.commands()).map_err(|e| format!("cannot list the commands: {e}")),
            "ui.inspect" => Ok(self.inspect()),
            m if m.starts_with("ui.") || m == "app.quit" => Err(format!("`{}` needs the app window; {NO_WINDOW}", vizcraft_engine::cmd::short(m))),
            other => Err(format!("unknown method `{}`", vizcraft_engine::cmd::short(other))),
        }
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "headless engine in this process (no window; screenshot renders pages)".into()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::Headless;
    use crate::{Backend, Server};

    const FLOW: &str = "flowchart LR\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Ship]\n  B -->|no| D[Fix]\n  D --> B";

    fn result(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        let reply: Value = serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap();
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }

    fn call(s: &mut Server, id: u64, name: &str, args: Value) -> Value {
        result(s, id, "tools/call", json!({"name": name, "arguments": args}))
    }

    fn text_json(r: &Value) -> Value {
        let text = r["content"].as_array().unwrap().iter().find(|c| c["type"] == "text").unwrap()["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap_or(Value::String(text.to_string()))
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        let val = |c: u8| match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("bad base64 {c}"),
        };
        let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
        let mut out = Vec::new();
        for chunk in bytes.chunks(4) {
            let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &c)| acc | (u32::from(val(c)) << (18 - 6 * i)));
            out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
        }
        out
    }

    fn headless() -> Server {
        Server::new(Box::new(Headless::new()))
    }

    #[test]
    fn builds_a_diagram_and_screenshots_it_without_a_window() {
        let mut s = headless();
        let init = result(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
        assert!(init["instructions"].as_str().unwrap().contains("headless"), "{init}");

        let r = call(&mut s, 2, "open_mermaid", json!({"source": FLOW, "routing": "orthogonal"}));
        assert_eq!(r["isError"], false, "{r}");
        let ids = text_json(&r)["ids"].as_array().unwrap().len();
        assert!(ids >= 8, "four nodes and four links: {r}");

        let pages = text_json(&call(&mut s, 3, "list_pages", json!({})));
        assert_eq!(pages["pages"][0]["shapes"], 4, "{pages}");
        assert_eq!(pages["pages"][0]["connectors"], 4, "{pages}");

        let r = call(&mut s, 4, "screenshot", json!({}));
        assert_eq!(r["isError"], false, "{r}");
        let image = r["content"].as_array().unwrap().iter().find(|c| c["type"] == "image").unwrap().clone();
        assert_eq!(image["mimeType"], "image/png");
        let png = base64_decode(image["data"].as_str().unwrap());
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        let (w, h) = (be(16), be(20));
        assert!(w > 0 && h > 0, "{w} x {h}");
        let size = text_json(&r);
        assert_eq!((size["width"].as_u64(), size["height"].as_u64()), (Some(u64::from(w)), Some(u64::from(h))));
        assert_eq!(size["bytes"].as_u64(), Some(png.len() as u64));

        // A page by index and a scale; twice the scale is about twice the width.
        let r2 = text_json(&call(&mut s, 5, "screenshot", json!({"page": 0, "scale": 2})));
        assert!(r2["width"].as_u64().unwrap().abs_diff(u64::from(w) * 2) <= 2, "{r2}");
        // The temporary files are gone.
        let leftovers = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("vizcraft-mcp-{}-", std::process::id())))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn screenshot_refuses_the_window_and_bad_pages() {
        let mut s = headless();
        let r = call(&mut s, 1, "screenshot", json!({"window": true}));
        assert_eq!(r["isError"], true);
        assert!(text_json(&r).as_str().unwrap().contains("no window"), "{r}");
        let r = call(&mut s, 2, "screenshot", json!({"page": "nope"}));
        assert_eq!(r["isError"], true, "{r}");
        for bad in [json!({"page": true}), json!({"scale": "big"}), json!({"scale": 9}), json!({"window": 1}), json!({"page": ""})] {
            let r = call(&mut s, 3, "screenshot", bad.clone());
            assert_eq!(r["isError"], true, "{bad}: {r}");
        }
    }

    #[test]
    fn doc_inspect_reports_the_document_and_a_page() {
        let mut s = headless();
        call(&mut s, 1, "open_mermaid", json!({"source": FLOW}));
        let r = text_json(&call(&mut s, 2, "doc_inspect", json!({})));
        assert_eq!(r["headless"], true, "{r}");
        assert_eq!(r["dirty"], true);
        assert_eq!(r["canUndo"], true);
        assert_eq!(r["diagram"]["shapes"], 4);
        assert!(r.get("page").is_none());

        let r = text_json(&call(&mut s, 3, "doc_inspect", json!({"page": 0})));
        assert_eq!(r["page"]["index"], 0, "{r}");
        assert_eq!(r["page"]["shapes"], 4, "{r}");
        let name = r["page"]["name"].as_str().unwrap().to_string();
        let id = r["page"]["id"].as_str().unwrap().to_string();
        assert_eq!(text_json(&call(&mut s, 4, "doc_inspect", json!({"page": name})))["page"]["id"], json!(id));
        assert_eq!(text_json(&call(&mut s, 5, "doc_inspect", json!({"page": id.clone()})))["page"]["id"], json!(id));
        let r = call(&mut s, 6, "doc_inspect", json!({"page": "missing"}));
        assert_eq!(r["isError"], true, "{r}");
        let r = call(&mut s, 7, "doc_inspect", json!({"page": [1]}));
        assert_eq!(r["isError"], true, "{r}");
    }

    #[test]
    fn window_methods_are_refused_and_ui_requests_become_warnings() {
        let mut h = Headless::new();
        assert!(!h.has_ui());
        for method in ["ui.click", "ui.screenshot", "ui.set", "app.quit"] {
            let e = h.call(method, json!({"x": 1, "y": 2})).unwrap_err();
            assert!(e.contains("no window"), "{method}: {e}");
        }
        assert!(h.call("engine.execute", json!({"command": "ui.shapesSearch", "params": {}})).unwrap_err().contains("no window"));
        assert!(h.call("nonsense", json!({})).unwrap_err().contains("unknown method"));
        assert!(h.call("engine.execute", json!({})).is_err());
        assert!(h.call("engine.execute", json!({"command": "no.such"})).is_err());

        // file.open without a path asks the UI for a picker; headless says it was dropped.
        let r = h.call("engine.execute", json!({"command": "file.open"})).unwrap();
        let warnings = r["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("no window")), "{r}");
        assert!(h.session().ui_requests.is_empty());

        let list = h.call("engine.commands", json!({})).unwrap();
        assert!(list.as_array().unwrap().iter().any(|c| c["id"] == "file.export"), "{list}");
        let r = h.call("engine.execute", json!({"command": "shape.add", "params": {"stencil": "basic.rectangle", "x": 10, "y": 10}})).unwrap();
        assert!(r.is_object(), "{r}");
        assert_eq!(h.call("ui.inspect", json!({})).unwrap()["diagram"]["shapes"], 1);
    }

    #[test]
    fn hostile_calls_never_panic() {
        let mut h = Headless::new();
        let junk = [json!(null), json!(1), json!("x"), json!([1, 2]), json!({"command": 5}), json!({"command": "file.export", "params": "x"})];
        for method in ["engine.execute", "engine.commands", "ui.inspect", "ui.click", "", "\u{0}"] {
            for p in &junk {
                let _ = h.call(method, p.clone());
            }
        }
    }
}
