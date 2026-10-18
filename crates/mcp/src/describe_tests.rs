//! Headless tests of the detail ladder: the `describe` tool, `doc_inspect {detail}`,
//! `command_list {brief}` and the page and shape resources at each level, all against the real
//! engine and its sample document.

use serde_json::{Value, json};
use vizcraft_engine::{Session, sample};

use crate::{Headless, Server};

const COLOUR_KEYS: &[&str] = &["fill", "line", "font", "style", "shadow", "gradient", "glow", "dash", "fontSize", "fontColor", "lineWidth"];

fn sample_server() -> Server {
    Server::new(Box::new(Headless::with_session(Session::new(sample::sample_document()))))
}

fn rpc(s: &mut Server, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap()
}

/// The first text block of a successful tool call, and whether it was flagged as an error.
fn tool_text(s: &mut Server, name: &str, args: Value) -> (bool, String) {
    let reply = rpc(s, "tools/call", json!({"name": name, "arguments": args}));
    assert!(reply.get("error").is_none(), "{name}: {reply}");
    let r = &reply["result"];
    let text = r["content"].as_array().unwrap().iter().find(|c| c["type"] == "text").unwrap()["text"].as_str().unwrap().to_string();
    (r["isError"].as_bool().unwrap(), text)
}

fn tool_json(s: &mut Server, name: &str, args: Value) -> Value {
    let (err, text) = tool_text(s, name, args);
    assert!(!err, "{name}: {text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}: {text}"))
}

fn code(s: &mut Server, name: &str, args: Value) -> i64 {
    rpc(s, "tools/call", json!({"name": name, "arguments": args}))["error"]["code"].as_i64().unwrap_or_else(|| panic!("{name} {args} should fail"))
}

fn read(s: &mut Server, uri: &str) -> Result<(usize, Value), i64> {
    let r = rpc(s, "resources/read", json!({"uri": uri}));
    if let Some(e) = r.get("error") {
        return Err(e["code"].as_i64().unwrap());
    }
    let text = r["result"]["contents"][0]["text"].as_str().unwrap();
    Ok((text.len(), serde_json::from_str(text).unwrap()))
}

fn keys(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                out.push(k.clone());
                keys(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| keys(x, out)),
        _ => {}
    }
}

fn has_key(v: &Value, key: &str) -> bool {
    let mut found = Vec::new();
    keys(v, &mut found);
    found.iter().any(|k| k == key)
}

#[test]
fn page_and_shape_resources_climb_the_ladder() {
    let mut s = sample_server();
    let (bytes, page) = read(&mut s, "vizcraft://page/p1").unwrap();
    assert!(bytes < 4096, "{bytes} bytes of structure");
    for k in COLOUR_KEYS.iter().chain(&["x", "waypoints", "connectionPoints", "ext"]) {
        assert!(!has_key(&page, k), "structure has {k}");
    }
    assert_eq!(page["shapes"].as_array().unwrap().len(), 6);
    assert_eq!(page["connectors"][2]["text"], "Yes");
    assert_eq!(read(&mut s, "vizcraft://page/p1?detail=structure").unwrap().1, page);

    let (_, layout) = read(&mut s, "vizcraft://page/p1?detail=layout").unwrap();
    assert_eq!(layout["shapes"][0]["x"], 380);
    assert_eq!(layout["width"], 1100);
    assert!(!has_key(&layout, "style"));

    let (_, full) = read(&mut s, "vizcraft://page/p1?detail=full").unwrap();
    assert!(has_key(&full, "style") && has_key(&full, "connectionPoints") && has_key(&full, "waypoints") | has_key(&full, "endArrow"));

    let (_, shape) = read(&mut s, "vizcraft://shape/s4").unwrap();
    assert_eq!(shape["detail"], "structure");
    assert_eq!(shape["page"], "p1");
    assert_eq!(shape["shape"]["stencil"], "flowchart.decision");
    assert_eq!(shape["connections"].as_array().unwrap().len(), 3);
    assert!(!has_key(&shape, "style") && !has_key(&shape, "w"));
    let (_, shape) = read(&mut s, "vizcraft://shape/s4?detail=layout").unwrap();
    assert_eq!((shape["shape"]["w"].as_i64(), shape["shape"]["h"].as_i64()), (Some(140), Some(90)));
    assert!(!has_key(&shape, "style"));
    let (_, shape) = read(&mut s, "vizcraft://shape/s4?detail=full").unwrap();
    assert!(has_key(&shape["shape"], "style") && has_key(&shape["shape"], "connectionPoints"));

    for uri in ["vizcraft://page/p1?detail=pretty", "vizcraft://shape/s4?detail=", "vizcraft://shape/s4?color=red"] {
        assert_eq!(read(&mut s, uri).unwrap_err(), -32602, "{uri}");
    }
    assert_eq!(read(&mut s, "vizcraft://shape/nope?detail=full").unwrap_err(), -32002);
    assert_eq!(read(&mut s, "vizcraft://page/zz?detail=full").unwrap_err(), -32002);
}

#[test]
fn describe_tool_is_doc_describe() {
    let mut s = sample_server();
    let json_form = tool_json(&mut s, "describe", json!({}));
    assert_eq!((json_form["detail"].as_str(), json_form["format"].as_str()), (Some("structure"), Some("json")));
    assert_eq!(json_form["pages"][0]["shapes"].as_array().unwrap().len(), 6);

    // The same as the command, through the generic tool.
    let via_command = tool_json(&mut s, "command_run", json!({"id": "doc.describe", "params": {"detail": "layout"}}));
    let via_tool = tool_json(&mut s, "describe", json!({"detail": "layout"}));
    assert_eq!(via_command["pages"], via_tool["pages"]);

    let (err, text) = tool_text(&mut s, "describe", json!({"format": "text"}));
    assert!(!err);
    assert!(text.starts_with("page p1 \"Page-1\" (6 shapes, 6 connectors)\n  s2 flowchart.terminator \"Start\"\n"), "{text}");
    assert!(text.contains("\n  c10 s4[2] -> s5[0] \"Yes\""), "{text}");
    assert!(text.len() < 500, "{} bytes", text.len());
    let (_, with_points) = tool_text(&mut s, "describe", json!({"format": "text", "detail": "layout"}));
    assert!(with_points.contains(" @ 380,40 140x50"), "{with_points}");

    s.handle_line(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "command_run", "arguments": {"id": "page.add", "params": {"name": "Second"}}}}).to_string());
    let only = tool_json(&mut s, "describe", json!({"pages": ["Second"]}));
    assert_eq!(only["pages"].as_array().unwrap().len(), 1);
    assert_eq!(only["pages"][0]["name"], "Second");
    let (err, msg) = tool_text(&mut s, "describe", json!({"pages": ["nope"]}));
    assert!(err && msg.contains("doc.describe failed") && msg.contains("no page `nope`"), "{msg}");
}

#[test]
fn describe_checks_its_arguments() {
    let mut s = sample_server();
    for bad in [
        json!({"detail": "colours"}),
        json!({"detail": 1}),
        json!({"format": "xml"}),
        json!({"pages": "Main"}),
        json!({"pages": [true]}),
        json!({"pages": [""]}),
        json!({"pages": vec!["a"; 1001]}),
        json!({"pages": [-1]}),
        json!({"bogus": 1}),
    ] {
        assert_eq!(code(&mut s, "describe", bad.clone()), -32602, "{bad}");
    }
    // Null counts as not given.
    assert!(tool_json(&mut s, "describe", json!({"detail": null, "format": null, "pages": null}))["pages"].is_array());
}

#[test]
fn doc_inspect_adds_pages_at_the_asked_detail() {
    let mut s = sample_server();
    let plain = tool_json(&mut s, "doc_inspect", json!({}));
    assert!(plain["pages"].is_number(), "without detail the report is unchanged: {plain}");
    let r = tool_json(&mut s, "doc_inspect", json!({"detail": "structure"}));
    assert_eq!(r["pages"][0]["connectors"].as_array().unwrap().len(), 6);
    assert!(!has_key(&r["pages"], "style"));
    assert_eq!(r["headless"], true, "the rest of the report is still there");
    let r = tool_json(&mut s, "doc_inspect", json!({"detail": "full", "page": 0}));
    assert!(has_key(&r["pages"], "style"));
    assert_eq!(r["page"]["id"], "p1");
    assert_eq!(r["pages"].as_array().unwrap().len(), 1);
    let (err, msg) = tool_text(&mut s, "doc_inspect", json!({"detail": "colours"}));
    assert!(err && msg.contains("structure, layout, full"), "{msg}");
    let (err, _) = tool_text(&mut s, "doc_inspect", json!({"detail": "layout", "page": "nope"}));
    assert!(err);
}

#[test]
fn command_list_brief_is_id_and_label_only() {
    let mut s = sample_server();
    let full = tool_text(&mut s, "command_list", json!({"prefix": "shape."}));
    let brief = tool_text(&mut s, "command_list", json!({"prefix": "shape.", "brief": true}));
    assert!(brief.1.len() * 3 < full.1.len(), "brief {} bytes, full {}", brief.1.len(), full.1.len());
    let brief: Value = serde_json::from_str(&brief.1).unwrap();
    let list = brief["commands"].as_array().unwrap();
    assert!(list.iter().any(|c| c["id"] == "shape.describe" && c["label"] == "Describe Shape"));
    for c in list {
        assert_eq!(c.as_object().unwrap().len(), 2, "{c}");
    }
    assert_eq!(brief["count"], list.len());
    let (err, _) = tool_text(&mut s, "command_list", json!({"brief": "yes"}));
    assert!(err);
    let all = tool_json(&mut s, "command_list", json!({"brief": false}));
    assert!(all["commands"][0].get("location").is_some());
}

#[test]
fn tool_text_is_short_and_points_at_describe() {
    let mut s = sample_server();
    let tools = rpc(&mut s, "tools/list", json!({}))["result"]["tools"].as_array().unwrap().clone();
    assert!(tools.iter().any(|t| t["name"] == "describe" && t["annotations"]["readOnlyHint"] == true));
    for t in &tools {
        let d = t["description"].as_str().unwrap();
        assert!(d.len() < 340, "{}: {} characters", t["name"], d.len());
        assert!(d.matches(". ").count() <= 2, "{}: {d}", t["name"]);
    }
    for name in ["describe", "doc_inspect"] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["inputSchema"]["properties"]["detail"]["enum"], json!(["structure", "layout", "full"]), "{name}");
    }
    let get_page = tools.iter().find(|t| t["name"] == "get_page").unwrap();
    assert!(get_page["description"].as_str().unwrap().contains("page.describe"));
    assert_eq!(get_page["inputSchema"]["properties"]["format"]["enum"][0], "drawio", "get_page keeps its draw.io default");

    let init = rpc(&mut s, "initialize", json!({"protocolVersion": "2025-06-18"}));
    let text = init["result"]["instructions"].as_str().unwrap();
    assert!(text.find("describe").unwrap() < text.find("command_list").unwrap(), "describe comes first: {text}");
    assert!(text.contains("detail: full") && text.contains("?detail="));
    let templates = rpc(&mut s, "resources/templates/list", json!({}))["result"]["resourceTemplates"].as_array().unwrap().clone();
    assert!(
        templates.iter().filter(|t| t["name"] == "page" || t["name"] == "shape").all(|t| t["description"].as_str().unwrap().contains("?detail="))
    );
}

#[test]
fn full_still_returns_what_page_get_gives() {
    let mut s = sample_server();
    let described = tool_json(&mut s, "describe", json!({"detail": "full"}));
    let native = tool_json(&mut s, "get_page", json!({"page": "p1", "format": "vizcraft"}));
    let native: Value = serde_json::from_str(native["content"].as_str().unwrap()).unwrap();
    // Every key the stored page has for a shape and a connector is in the full description.
    for (kind, item) in [("shapes", 0), ("connectors", 0)] {
        let stored = native[kind][item].as_object().unwrap();
        let shown = described["pages"][0][kind][item].as_object().unwrap();
        for k in stored.keys() {
            assert!(shown.contains_key(k) || k == "angleDeg" || k == "start" || k == "end", "full {kind} misses {k}");
        }
    }
    assert_eq!(described["pages"][0]["shapes"][0]["style"], native["shapes"][0]["style"]);
}
