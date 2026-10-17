//! Protocol tests against an in-memory fake app, and one test of [`Remote`] against a tiny TCP
//! listener. No real app is needed.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::{Backend, COMMANDS_URI, MAX_BATCH, MAX_MESSAGE_BYTES, Remote, Server};

/// The smallest valid PNG header the screenshot path accepts: signature, IHDR length and type,
/// then width 3 and height 2.
fn tiny_png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    png.extend_from_slice(&3u32.to_be_bytes());
    png.extend_from_slice(&2u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png
}

/// A fake app: a few commands, a log of what ran, and canned answers for the `ui.*` methods.
#[derive(Default, Clone)]
struct Fake {
    executed: Arc<Mutex<Vec<String>>>,
}

impl Backend for Fake {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "engine.commands" => Ok(json!([
                {"id": "shape.add", "label": "Add Shape", "enabled": true, "params": "{stencil, x, y}"},
                {"id": "shape.delete", "label": "Delete", "enabled": true, "params": "{ids?}"},
                {"id": "edit.undo", "label": "Undo", "enabled": false, "reason": "nothing to undo", "params": ""},
            ])),
            "engine.execute" => {
                let id = params.get("command").and_then(Value::as_str).unwrap_or_default().to_string();
                if id.starts_with("fail.") {
                    return Err(format!("{id} refused"));
                }
                if id == "boom.panic" {
                    #[allow(clippy::panic)]
                    {
                        panic!("fake backend bug");
                    }
                }
                if let Ok(mut log) = self.executed.lock() {
                    log.push(id.clone());
                }
                Ok(json!({"ran": id, "params": params.get("params").cloned().unwrap_or(Value::Null)}))
            }
            "ui.inspect" => Ok(json!({"title": "Untitled", "dirty": false, "rev": 3})),
            "ui.screenshot" => {
                let path = params.get("path").and_then(Value::as_str).ok_or("no path")?;
                std::fs::write(path, tiny_png()).map_err(|e| e.to_string())?;
                Ok(json!({"path": path, "width": 3, "height": 2}))
            }
            other => Err(format!("unknown method {other}")),
        }
    }
    fn has_ui(&self) -> bool {
        true
    }
    fn describe(&self) -> String {
        "fake".into()
    }
}

fn server() -> (Server, Fake) {
    let fake = Fake::default();
    (Server::new(Box::new(fake.clone())), fake)
}

fn rpc(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    let reply: Value = serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap();
    assert_eq!(reply["jsonrpc"], "2.0", "{reply}");
    assert_eq!(reply["id"], id, "{reply}");
    reply
}

fn result(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let v = rpc(s, id, method, params);
    assert!(v.get("error").is_none(), "{method}: {v}");
    v["result"].clone()
}

fn err_code(s: &mut Server, id: u64, method: &str, params: Value) -> i64 {
    let v = rpc(s, id, method, params);
    v["error"]["code"].as_i64().unwrap_or_else(|| panic!("{method} should have failed: {v}"))
}

/// Call a tool; returns (isError, the first text block parsed as JSON or as a string).
fn tool(s: &mut Server, id: u64, name: &str, args: Value) -> (bool, Value) {
    let r = result(s, id, "tools/call", json!({"name": name, "arguments": args}));
    let text = r["content"].as_array().unwrap().iter().find(|c| c["type"] == "text").unwrap()["text"].as_str().unwrap().to_string();
    let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (r["isError"].as_bool().unwrap(), body)
}

#[test]
fn initialize_negotiates_the_version() {
    let (mut s, _) = server();
    let r = result(&mut s, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}}));
    assert_eq!(r["protocolVersion"], "2025-03-26");
    assert_eq!(r["serverInfo"]["name"], "vizcraft");
    assert!(r["capabilities"]["tools"].is_object() && r["capabilities"]["resources"].is_object(), "{r}");
    assert!(r["instructions"].as_str().unwrap().contains("command_list"), "{r}");
    // A version we do not know is answered with ours, and the client decides.
    let r = result(&mut s, 2, "initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(r["protocolVersion"], crate::PROTOCOL_VERSION);
    assert!(!s.is_initialized());
    assert_eq!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#), None);
    assert!(s.is_initialized());
    assert_eq!(result(&mut s, 3, "ping", json!({})), json!({}));
}

#[test]
fn tools_list_names_the_five_tools_with_schemas() {
    let (mut s, _) = server();
    let tools = result(&mut s, 1, "tools/list", json!({}))["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["command_list", "command_run", "command_batch", "doc_inspect", "screenshot"]);
    for t in &tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(t["description"].as_str().unwrap().len() > 40, "{t}");
    }
}

#[test]
fn command_list_filters_by_prefix() {
    let (mut s, _) = server();
    let (err, all) = tool(&mut s, 1, "command_list", json!({}));
    assert!(!err);
    assert_eq!(all["count"], 3);
    let (_, shapes) = tool(&mut s, 2, "command_list", json!({"prefix": "shape."}));
    assert_eq!(shapes["count"], 2);
    assert!(shapes["commands"].as_array().unwrap().iter().all(|c| c["id"].as_str().unwrap().starts_with("shape.")));
    let (err, msg) = tool(&mut s, 3, "command_list", json!({"prefix": 7}));
    assert!(err, "{msg}");
}

#[test]
fn command_run_forwards_and_reports_failure() {
    let (mut s, fake) = server();
    let (err, r) = tool(&mut s, 1, "command_run", json!({"id": "shape.add", "params": {"stencil": "basic.rectangle", "x": 1, "y": 2}}));
    assert!(!err, "{r}");
    assert_eq!(r["ran"], "shape.add");
    assert_eq!(r["params"]["x"], 1);
    let (err, r) = tool(&mut s, 2, "command_run", json!({"id": "edit.undo"}));
    assert!(!err);
    assert_eq!(r["params"], json!({}), "params default to an empty object");
    let (err, msg) = tool(&mut s, 3, "command_run", json!({"id": "fail.this"}));
    assert!(err);
    assert!(msg.as_str().unwrap().contains("fail.this refused"), "{msg}");
    for bad in [json!({}), json!({"id": ""}), json!({"id": 3}), json!({"id": "x", "params": [1]})] {
        let (err, msg) = tool(&mut s, 4, "command_run", bad.clone());
        assert!(err, "{bad}: {msg}");
    }
    assert_eq!(*fake.executed.lock().unwrap(), ["shape.add", "edit.undo"]);
}

#[test]
fn command_batch_stops_at_the_first_failure() {
    let (mut s, fake) = server();
    let (err, r) = tool(
        &mut s,
        1,
        "command_batch",
        json!({"commands": [{"id": "shape.add"}, {"id": "shape.delete", "params": {"ids": ["s1"]}}, {"id": "fail.now"}, {"id": "edit.undo"}]}),
    );
    assert!(err, "{r}");
    assert_eq!(r["failedIndex"], 2);
    assert_eq!(r["failedId"], "fail.now");
    assert_eq!(r["completed"], 2);
    assert_eq!(r["results"].as_array().unwrap().len(), 2);
    assert!(r["error"].as_str().unwrap().contains("refused"));
    assert_eq!(*fake.executed.lock().unwrap(), ["shape.add", "shape.delete"], "nothing after the failure ran");

    let (err, r) = tool(&mut s, 2, "command_batch", json!({"commands": [{"id": "edit.undo"}, {"id": "edit.undo"}]}));
    assert!(!err, "{r}");
    assert_eq!(r["completed"], 2);
}

#[test]
fn command_batch_validates_before_running_and_is_bounded() {
    let (mut s, fake) = server();
    let (err, msg) = tool(&mut s, 1, "command_batch", json!({"commands": [{"id": "shape.add"}, {"params": {}}]}));
    assert!(err);
    assert!(msg.as_str().unwrap().contains("commands[1]"), "{msg}");
    let too_many: Vec<Value> = (0..=MAX_BATCH).map(|_| json!({"id": "edit.undo"})).collect();
    let (err, _) = tool(&mut s, 2, "command_batch", json!({"commands": too_many}));
    assert!(err);
    for bad in [json!({}), json!({"commands": []}), json!({"commands": "x"}), json!({"commands": [1]})] {
        let (err, msg) = tool(&mut s, 3, "command_batch", bad.clone());
        assert!(err, "{bad}: {msg}");
    }
    assert!(fake.executed.lock().unwrap().is_empty(), "a rejected batch runs nothing");
}

#[test]
fn doc_inspect_and_screenshot() {
    let (mut s, _) = server();
    let (err, r) = tool(&mut s, 1, "doc_inspect", json!({}));
    assert!(!err);
    assert_eq!(r["rev"], 3);

    let r = result(&mut s, 2, "tools/call", json!({"name": "screenshot", "arguments": {}}));
    assert_eq!(r["isError"], false, "{r}");
    let image = r["content"].as_array().unwrap().iter().find(|c| c["type"] == "image").unwrap().clone();
    assert_eq!(image["mimeType"], "image/png");
    assert_eq!(image["data"], crate::base64_encode(&tiny_png()));
    let (_, size) = tool(&mut s, 3, "screenshot", json!({}));
    assert_eq!((size["width"].as_u64(), size["height"].as_u64()), (Some(3), Some(2)));
}

#[test]
fn commands_resource() {
    let (mut s, _) = server();
    let list = result(&mut s, 1, "resources/list", json!({}));
    assert_eq!(list["resources"][0]["uri"], COMMANDS_URI);
    let read = result(&mut s, 2, "resources/read", json!({"uri": COMMANDS_URI}));
    let body: Value = serde_json::from_str(read["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 3);
    assert_eq!(err_code(&mut s, 3, "resources/read", json!({"uri": "vizcraft://nope"})), -32002);
    assert_eq!(err_code(&mut s, 4, "resources/read", json!({})), -32602);
}

#[test]
fn unknown_methods_tools_and_arguments_are_errors() {
    let (mut s, _) = server();
    assert_eq!(err_code(&mut s, 1, "nope/nothing", json!({})), -32601);
    assert_eq!(err_code(&mut s, 2, "tools/call", json!({"name": "draw_everything"})), -32602);
    assert_eq!(err_code(&mut s, 3, "tools/call", json!({"arguments": {}})), -32602);
    assert_eq!(err_code(&mut s, 4, "tools/call", json!({"name": "doc_inspect", "arguments": {"page": 1}})), -32602);
    let r = result(&mut s, 5, "tools/call", json!({"name": "command_run", "arguments": "id"}));
    assert_eq!(r["isError"], true, "{r}");
}

#[test]
fn a_panicking_backend_fails_one_request_only() {
    let (mut s, _) = server();
    let v = rpc(&mut s, 1, "tools/call", json!({"name": "command_run", "arguments": {"id": "boom.panic"}}));
    assert_eq!(v["error"]["code"], -32603, "{v}");
    assert_eq!(result(&mut s, 2, "ping", json!({})), json!({}));
}

#[test]
fn junk_lines_get_errors_and_the_server_keeps_reading() {
    let (mut s, _) = server();
    let long = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{{\"x\":\"{}\"}}}}", "a".repeat(MAX_MESSAGE_BYTES));
    let mut input = Vec::new();
    for line in [
        "this is not json",
        "{\"jsonrpc\":\"2.0\",\"id\":",
        "[]",
        "42",
        "{\"jsonrpc\":\"2.0\",\"id\":{\"x\":1},\"method\":\"ping\"}",
        "{\"jsonrpc\":\"2.0\",\"id\":5}",
        "",
        &long,
    ] {
        input.extend_from_slice(line.as_bytes());
        input.push(b'\n');
    }
    input.extend_from_slice(b"\xff\xfe\n");
    input.extend_from_slice(b"[{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"},{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}]\n");
    // The last line has no newline: it still counts.
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}");
    let mut out = Vec::new();
    s.serve(input.as_slice(), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let codes: Vec<i64> = lines.iter().filter_map(|l| l["error"]["code"].as_i64()).collect();
    assert_eq!(codes, [-32700, -32700, -32600, -32600, -32600, -32600, -32600, -32700], "{lines:?}");
    assert_eq!(lines[lines.len() - 2], json!([{"jsonrpc": "2.0", "id": 7, "result": {}}]));
    assert_eq!(lines[lines.len() - 1]["id"], 9);
    assert!(s.is_initialized());
}

#[test]
fn oversized_rpc_batches_are_refused() {
    let (mut s, _) = server();
    let batch: Vec<Value> = (0..65).map(|i| json!({"jsonrpc": "2.0", "id": i, "method": "ping"})).collect();
    let reply: Value = serde_json::from_str(&s.handle_line(&Value::Array(batch).to_string()).unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], -32600);
}

#[test]
fn remote_talks_json_lines_to_a_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen_in_thread = Arc::clone(&seen);
    let app = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let req: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let reply = match req["method"].as_str() {
                Some("engine.commands") => json!({"id": req["id"], "ok": true, "result": [{"id": "shape.add"}, {"id": "edit.undo"}]}),
                _ => json!({"id": req["id"], "ok": false, "error": "unknown method"}),
            };
            seen_in_thread.lock().unwrap().push(req);
            writeln!(writer, "{reply}").unwrap();
        }
    });

    let remote = Remote::connect(&addr.to_string()).unwrap();
    let mut s = Server::new(Box::new(remote));
    let (err, r) = tool(&mut s, 1, "command_list", json!({"prefix": "shape"}));
    assert!(!err, "{r}");
    assert_eq!(r["commands"], json!([{"id": "shape.add"}]));
    let (err, msg) = tool(&mut s, 2, "doc_inspect", json!({}));
    assert!(err);
    assert_eq!(msg, "unknown method");
    drop(s);
    app.join().unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0]["method"], "engine.commands");
    assert_eq!(seen[1]["method"], "ui.inspect");
    assert_ne!(seen[0]["id"], seen[1]["id"], "each request carries its own id");
}

#[test]
fn remote_says_when_no_app_is_listening_and_refuses_other_hosts() {
    // Bind and drop to find a port that is very likely closed.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut remote = Remote::new(&format!("127.0.0.1:{port}")).unwrap();
    let e = remote.call("ui.inspect", json!({})).unwrap_err();
    assert!(e.contains("no VizCraft app is listening"), "{e}");
    assert!(e.contains(&format!("--control {port}")), "{e}");
    assert!(Remote::new("192.0.2.1:7990").unwrap_err().contains("loopback"));
    assert!(Remote::new("not an address").is_err());
}
