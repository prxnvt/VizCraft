//! MCP resources: what `resources/list`, `resources/templates/list` and `resources/read` answer.
//!
//! The design is two tables. [`STATIC`] holds resources with a fixed URI; [`TEMPLATES`] holds URI
//! templates of the form `<prefix>{id}`, each with a read function that gets the decoded id and,
//! optionally, a list function that names the concrete URIs that exist right now. Adding a kind
//! of resource is one entry in one table; `server.rs` only delegates the three methods here.
//!
//! Every read goes through the backend as engine commands (`page.describe`, `shape.describe`,
//! `shape.catalog`) or `engine.commands`, so the content is the same over the control channel
//! and in a headless session. Page and shape URIs take a `?detail=structure|layout|full` suffix;
//! the default is `structure`, which carries no colours or coordinates.

use serde_json::{Value, json};

use crate::COMMANDS_URI;
use crate::backend::Backend;
use crate::notation;
use crate::server::{INTERNAL_ERROR, INVALID_PARAMS};
use crate::tools;

/// JSON-RPC error code for a resource that does not exist (MCP's choice).
pub(crate) const RESOURCE_NOT_FOUND: i64 = -32002;

/// The stencil catalog: every set, its categories and masters.
pub(crate) const STENCILS_URI: &str = "vizcraft://stencils";
/// Prefix of one page's URI; the page id follows, percent-encoded.
pub(crate) const PAGE_URI_PREFIX: &str = "vizcraft://page/";
/// Prefix of one shape's URI; the shape id follows, percent-encoded.
pub(crate) const SHAPE_URI_PREFIX: &str = "vizcraft://shape/";

/// Longest `{id}` after percent-decoding, in bytes. Real ids are at most 64 characters; page
/// names (which `page.get` also accepts) are short too.
pub(crate) const MAX_ID_BYTES: usize = 256;
/// Longest URI looked at: the longest prefix plus a fully percent-encoded id. Anything longer
/// cannot name a resource, so it is refused before any decoding work.
const MAX_URI_BYTES: usize = 32 + MAX_ID_BYTES * 3;
/// Most pages named in `resources/list`; a document with more still reads every page by URI.
const MAX_LISTED_PAGES: usize = 1000;
/// Characters of a URI quoted back in an error message.
const MAX_QUOTED_CHARS: usize = 80;

const JSON: &str = "application/json";

/// The detail levels of `doc.describe`, `page.describe` and `shape.describe`, cheapest first.
pub(crate) const DETAILS: [&str; 3] = ["structure", "layout", "full"];

/// An error for the JSON-RPC reply: code and message.
pub(crate) type RpcError = (i64, String);

/// A resource with a fixed URI.
struct Static {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    read: fn(&mut dyn Backend) -> Result<String, RpcError>,
}

/// A family of resources addressed as `<prefix>{id}`.
struct Template {
    prefix: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// Reads the resource for a decoded id and a detail level (one of [`DETAILS`]).
    read: fn(&mut dyn Backend, &str, &str) -> Result<String, RpcError>,
    /// The concrete resources of this kind that exist now, for `resources/list`. An `Err` (no
    /// app running, say) leaves them out and the static entries are listed alone.
    list: Option<fn(&mut dyn Backend) -> Result<Vec<Value>, String>>,
}

const STATIC: &[Static] = &[
    Static {
        uri: COMMANDS_URI,
        name: "commands",
        title: "Command catalog",
        description: "Every VizCraft command with its label, params and whether it can run now; the same list as command_list.",
        read: read_commands,
    },
    Static {
        uri: STENCILS_URI,
        name: "stencils",
        title: "Stencil catalog",
        description: "Every stencil set with its categories and masters (id, label, keywords, default size and the draw.io style that opens as it).",
        read: read_stencils,
    },
];

const TEMPLATES: &[Template] = &[
    Template {
        prefix: PAGE_URI_PREFIX,
        name: "page",
        title: "Page",
        description: "One page as JSON: shapes (stencil, text, parent, data rows) and connectors (glue with connection points, text). {id} is a page id from list_pages. Add `?detail=layout` for positions and sizes, `?detail=full` for style and everything else; the default is `structure`.",
        read: read_page,
        list: Some(list_pages),
    },
    Template {
        prefix: SHAPE_URI_PREFIX,
        name: "shape",
        title: "Shape",
        description: "One shape as JSON, with the connectors glued to it: stencil, text, parent, data rows. Add `?detail=layout` for geometry, `?detail=full` for style, connection points and everything else; the default is `structure`.",
        read: read_shape,
        list: None,
    },
];

/// The `resources/list` result: the static resources, the notation packs, then one entry per page.
pub(crate) fn list(backend: &mut dyn Backend) -> Value {
    let mut out: Vec<Value> =
        STATIC.iter().map(|r| json!({"uri": r.uri, "name": r.name, "title": r.title, "description": r.description, "mimeType": JSON})).collect();
    out.extend(notation::resources(backend));
    for t in TEMPLATES {
        let Some(list) = t.list else { continue };
        match list(backend) {
            Ok(entries) => out.extend(entries),
            Err(e) => log::debug!("resources/list without {} entries: {e}", t.name),
        }
    }
    json!({"resources": out})
}

/// The `resources/templates/list` result.
pub(crate) fn templates() -> Value {
    let mut list: Vec<Value> = TEMPLATES
        .iter()
        .map(|t| {
            json!({
                "uriTemplate": format!("{}{{id}}", t.prefix),
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "mimeType": JSON,
            })
        })
        .collect();
    list.extend(notation::templates());
    json!({"resourceTemplates": list})
}

/// The `resources/read` result for `params.uri`.
pub(crate) fn read(backend: &mut dyn Backend, params: &Value) -> Result<Value, RpcError> {
    let uri = params.get("uri").and_then(Value::as_str).ok_or((INVALID_PARAMS, "missing `uri`".to_string()))?;
    // Notation URIs carry their own parser and clip what they echo back.
    if let Some(read) = notation::read_uri(backend, uri) {
        return read.map_err(|e| (RESOURCE_NOT_FOUND, e));
    }
    if uri.len() > MAX_URI_BYTES {
        return Err(not_found(uri));
    }
    let text = if let Some(r) = STATIC.iter().find(|r| r.uri == uri) {
        (r.read)(backend)?
    } else if let Some((t, rest)) = TEMPLATES.iter().find_map(|t| uri.strip_prefix(t.prefix).map(|rest| (t, rest))) {
        let (raw_id, detail) = split_detail(rest)?;
        let id = decode_id(raw_id).ok_or_else(|| not_found(uri))?;
        (t.read)(backend, &id, detail).map_err(|(code, m)| if code == RESOURCE_NOT_FOUND { not_found(uri) } else { (code, m) })?
    } else {
        return Err(not_found(uri));
    };
    Ok(json!({"contents": [{"uri": uri, "mimeType": JSON, "text": text}]}))
}

/// Split `<id>[?detail=<level>]`. Only `detail` is understood after the `?` (an id never holds a
/// raw `?`, because listed URIs encode it); any other query, or a level that is not one of
/// [`DETAILS`], is invalid params.
fn split_detail(rest: &str) -> Result<(&str, &str), RpcError> {
    let Some((id, query)) = rest.split_once('?') else { return Ok((rest, DETAILS[0])) };
    let mut detail = DETAILS[0];
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        match pair.split_once('=') {
            Some(("detail", level)) => {
                detail = DETAILS
                    .iter()
                    .copied()
                    .find(|d| *d == level)
                    .ok_or_else(|| (INVALID_PARAMS, format!("`detail` must be one of {}, not `{}`", DETAILS.join(", "), clip(level))))?;
            }
            _ => return Err((INVALID_PARAMS, format!("unsupported query `{}`; only `?detail={}` is understood", clip(pair), DETAILS.join("|")))),
        }
    }
    Ok((id, detail))
}

/// `t` cut to a length that can be quoted in an error.
fn clip(t: &str) -> String {
    let mut shown: String = t.chars().take(MAX_QUOTED_CHARS).collect();
    if shown.len() < t.len() {
        shown.push_str("...");
    }
    shown
}

fn not_found(uri: &str) -> RpcError {
    let mut shown: String = uri.chars().take(MAX_QUOTED_CHARS).collect();
    if shown.len() < uri.len() {
        shown.push_str("...");
    }
    (RESOURCE_NOT_FOUND, format!("resource not found: {shown}"))
}

/// Percent-decode an `{id}`: `None` for an empty id, a bad escape, text that is not UTF-8 once
/// decoded, or one longer than [`MAX_ID_BYTES`].
pub(crate) fn decode_id(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len().min(MAX_ID_BYTES));
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hi = bytes.get(i.saturating_add(1)).and_then(|c| (*c as char).to_digit(16))?;
            let lo = bytes.get(i.saturating_add(2)).and_then(|c| (*c as char).to_digit(16))?;
            out.push(u8::try_from(hi * 16 + lo).ok()?);
            i = i.saturating_add(3);
        } else {
            out.push(b);
            i = i.saturating_add(1);
        }
        if out.len() > MAX_ID_BYTES {
            return None;
        }
    }
    let id = String::from_utf8(out).ok()?;
    (!id.is_empty()).then_some(id)
}

/// Percent-encode an id for a URI, keeping only the RFC 3986 unreserved characters as they are.
pub(crate) fn encode_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Run one engine command. A command that says the page or shape does not exist becomes
/// [`RESOURCE_NOT_FOUND`]; any other failure (no app, a timeout) is an internal error with the
/// reason, so the client can tell "wrong id" from "try again".
fn execute(backend: &mut dyn Backend, command: &str, params: Value) -> Result<Value, RpcError> {
    backend.call("engine.execute", json!({"command": command, "params": params})).map_err(|e| {
        // The engine words these as `no page `<ref>`` and `no shape `<id>`` (cmd/page.rs and
        // cmd/shape.rs); the id we send is always well formed, so no other params error arises.
        if e.contains("no page `") || e.contains("no shape `") { (RESOURCE_NOT_FOUND, e) } else { (INTERNAL_ERROR, format!("{command} failed: {e}")) }
    })
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn read_commands(backend: &mut dyn Backend) -> Result<String, RpcError> {
    let list = tools::commands(backend).map_err(|e| (INTERNAL_ERROR, e))?;
    Ok(pretty(&Value::Array(list)))
}

fn read_stencils(backend: &mut dyn Backend) -> Result<String, RpcError> {
    Ok(pretty(&execute(backend, "shape.catalog", json!({}))?))
}

fn read_page(backend: &mut dyn Backend, id: &str, detail: &str) -> Result<String, RpcError> {
    let r = execute(backend, "page.describe", json!({"page": id, "detail": detail}))?;
    match r.get("page") {
        Some(page) => Ok(pretty(page)),
        None => Err((INTERNAL_ERROR, "page.describe returned no `page`".into())),
    }
}

fn read_shape(backend: &mut dyn Backend, id: &str, detail: &str) -> Result<String, RpcError> {
    let mut r = execute(backend, "shape.describe", json!({"id": id, "detail": detail}))?;
    // The format is always JSON here; the level is worth keeping so the reader knows what is missing.
    if let Some(o) = r.as_object_mut() {
        o.remove("format");
    }
    Ok(pretty(&r))
}

/// One resource per page, from `page.list`.
fn list_pages(backend: &mut dyn Backend) -> Result<Vec<Value>, String> {
    let r = backend.call("engine.execute", json!({"command": "page.list", "params": {}}))?;
    let pages = r.get("pages").and_then(Value::as_array).ok_or("page.list returned no `pages`")?;
    Ok(pages
        .iter()
        .take(MAX_LISTED_PAGES)
        .filter_map(|p| {
            let id = p.get("id").and_then(Value::as_str).filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES)?;
            let name: String = p.get("name").and_then(Value::as_str).unwrap_or(id).chars().take(MAX_ID_BYTES).collect();
            let count = |k: &str| p.get(k).and_then(Value::as_u64).unwrap_or(0);
            Some(json!({
                "uri": format!("{PAGE_URI_PREFIX}{}", encode_id(id)),
                "name": format!("page {id}"),
                "title": name,
                "description": format!("Page `{name}`: {} shapes, {} connectors, as JSON (`?detail=layout|full` for more).", count("shapes"), count("connectors")),
                "mimeType": JSON,
            }))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{MAX_ID_BYTES, decode_id, encode_id};
    use crate::{Backend, COMMANDS_URI, Server};

    /// A fake app with two pages and three shapes. `down` makes every call fail the way an
    /// unreachable app does.
    #[derive(Default)]
    struct Fake {
        down: bool,
    }

    const PAGES: &[(&str, &str, &[&str])] = &[("p1", "Main", &["s1", "s2"]), ("p 2/\u{e9}", "Second", &["s\u{4e2d}"])];

    impl Fake {
        fn execute(&self, command: &str, params: &Value) -> Result<Value, String> {
            match command {
                "page.list" => Ok(json!({"pages": PAGES.iter().enumerate().map(|(i, (id, name, shapes))| json!({
                    "index": i, "id": id, "name": name, "width": 1100, "height": 850, "shapes": shapes.len(), "connectors": 1,
                })).collect::<Vec<_>>()})),
                "page.describe" => {
                    let r = params["page"].as_str().unwrap_or_default();
                    let (id, name, shapes) = PAGES.iter().find(|p| p.0 == r).ok_or(format!("bad parameters: no page `{r}`"))?;
                    let page = json!({"id": id, "name": name, "detail": params["detail"], "shapes": shapes.iter().map(|s| json!({"id": s})).collect::<Vec<_>>()});
                    Ok(json!({"detail": params["detail"], "format": "json", "page": page}))
                }
                "shape.describe" => {
                    let id = params["id"].as_str().unwrap_or_default();
                    let page = PAGES.iter().find(|p| p.2.contains(&id)).ok_or(format!("bad parameters: no shape `{id}`"))?;
                    Ok(json!({
                        "detail": params["detail"], "format": "json", "page": page.0, "shape": {"id": id, "stencil": "basic.rectangle"},
                        "connections": [{"id": "c1", "end": "start", "from": {"shape": id}, "to": {"shape": "s2"}}],
                    }))
                }
                "shape.catalog" => {
                    Ok(json!({"sets": [{"id": "basic", "name": "Basic Shapes", "categories": ["Shapes"], "masters": [{"id": "basic.rectangle"}]}]}))
                }
                other => Err(format!("unknown command `{other}`")),
            }
        }
    }

    impl Backend for Fake {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
            if self.down {
                return Err("no VizCraft app is listening at 127.0.0.1:7990 (connection refused)".into());
            }
            match method {
                "engine.commands" => Ok(json!([{"id": "shape.add"}, {"id": "edit.undo"}])),
                "engine.execute" => self.execute(params["command"].as_str().unwrap_or_default(), &params["params"]),
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

    fn rpc(s: &mut Server, method: &str, params: Value) -> Value {
        let line = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap()
    }

    fn read(s: &mut Server, uri: &str) -> Result<(String, Value), (i64, String)> {
        let r = rpc(s, "resources/read", json!({"uri": uri}));
        if let Some(e) = r.get("error") {
            return Err((e["code"].as_i64().unwrap(), e["message"].as_str().unwrap().to_string()));
        }
        let c = &r["result"]["contents"][0];
        assert_eq!(c["uri"], uri);
        assert_eq!(c["mimeType"], "application/json");
        let text = c["text"].as_str().unwrap();
        Ok((text.to_string(), serde_json::from_str(text).unwrap()))
    }

    fn up() -> Server {
        Server::new(Box::new(Fake::default()))
    }

    #[test]
    fn list_names_static_resources_and_every_page() {
        let mut s = up();
        let list = rpc(&mut s, "resources/list", json!({}))["result"]["resources"].as_array().unwrap().clone();
        // Notation pack entries are covered by the notation tests.
        let uris: Vec<&str> = list.iter().map(|r| r["uri"].as_str().unwrap()).filter(|u| !u.starts_with("vizcraft://notation")).collect();
        assert_eq!(uris, [COMMANDS_URI, "vizcraft://stencils", "vizcraft://page/p1", "vizcraft://page/p%202%2F%C3%A9"]);
        for r in &list {
            assert_eq!(r["mimeType"], "application/json", "{r}");
            assert!(r["name"].is_string() && r["description"].is_string(), "{r}");
        }
        assert_eq!(list.iter().find(|r| r["uri"] == "vizcraft://page/p1").unwrap()["title"], "Main");
        // Every listed URI reads back.
        for uri in uris {
            assert!(read(&mut s, uri).is_ok(), "{uri}");
        }
    }

    #[test]
    fn list_falls_back_to_static_entries_when_the_backend_fails() {
        let mut s = Server::new(Box::new(Fake { down: true }));
        let list = rpc(&mut s, "resources/list", json!({}))["result"]["resources"].as_array().unwrap().clone();
        // Notation pack entries are covered by the notation tests.
        let uris: Vec<&str> = list.iter().map(|r| r["uri"].as_str().unwrap()).filter(|u| !u.starts_with("vizcraft://notation")).collect();
        assert_eq!(uris, [COMMANDS_URI, "vizcraft://stencils"]);
        // Reads fail as internal errors with the reason, not as "not found".
        for uri in [COMMANDS_URI, "vizcraft://stencils", "vizcraft://page/p1", "vizcraft://shape/s1"] {
            let (code, message) = read(&mut s, uri).unwrap_err();
            assert_eq!(code, -32603, "{uri}");
            assert!(message.contains("no VizCraft app is listening"), "{message}");
        }
    }

    #[test]
    fn templates_name_pages_and_shapes() {
        let mut s = up();
        let t = rpc(&mut s, "resources/templates/list", json!({}))["result"]["resourceTemplates"].as_array().unwrap().clone();
        let uris: Vec<&str> = t.iter().map(|t| t["uriTemplate"].as_str().unwrap()).filter(|u| !u.starts_with("vizcraft://notation")).collect();
        assert_eq!(uris, ["vizcraft://page/{id}", "vizcraft://shape/{id}"]);
        for t in &t {
            assert_eq!(t["mimeType"], "application/json");
            assert!(t["name"].is_string() && t["title"].is_string() && t["description"].is_string(), "{t}");
        }
    }

    #[test]
    fn read_each_kind() {
        let mut s = up();
        let (_, commands) = read(&mut s, COMMANDS_URI).unwrap();
        assert_eq!(commands.as_array().unwrap().len(), 2);
        let (_, stencils) = read(&mut s, "vizcraft://stencils").unwrap();
        assert_eq!(stencils["sets"][0]["masters"][0]["id"], "basic.rectangle");
        let (text, page) = read(&mut s, "vizcraft://page/p1").unwrap();
        assert_eq!(page["name"], "Main");
        assert!(text.contains('\n'), "the page JSON is passed through as the engine wrote it");
        let (_, shape) = read(&mut s, "vizcraft://shape/s2").unwrap();
        assert_eq!((shape["page"].as_str(), shape["shape"]["id"].as_str()), (Some("p1"), Some("s2")));
        assert_eq!(shape["connections"][0]["id"], "c1");
        assert_eq!((page["detail"].as_str(), shape["detail"].as_str()), (Some("structure"), Some("structure")), "structure is the default");
        assert!(shape.get("format").is_none());
    }

    #[test]
    fn detail_suffix_picks_the_level_before_the_id_is_looked_up() {
        let mut s = up();
        for level in ["structure", "layout", "full"] {
            assert_eq!(read(&mut s, &format!("vizcraft://page/p1?detail={level}")).unwrap().1["detail"], level);
            assert_eq!(read(&mut s, &format!("vizcraft://shape/s2?detail={level}")).unwrap().1["detail"], level);
        }
        // The id is decoded without the suffix, so an encoded id with a suffix still resolves.
        assert_eq!(read(&mut s, "vizcraft://page/p%202%2F%C3%A9?detail=full").unwrap().1["detail"], "full");
        assert_eq!(read(&mut s, "vizcraft://shape/s%E4%B8%AD?detail=layout").unwrap().1["page"], "p 2/\u{e9}");
        assert_eq!(read(&mut s, "vizcraft://page/p1?").unwrap().1["detail"], "structure");
        assert_eq!(read(&mut s, "vizcraft://page/p1?detail=layout&detail=full").unwrap().1["detail"], "full", "the last one wins");
        // An unknown level or query is invalid params; an unknown id with a good suffix is not found.
        for uri in [
            "vizcraft://page/p1?detail=colourful",
            "vizcraft://page/p1?detail=",
            "vizcraft://page/p1?detail=FULL",
            "vizcraft://shape/s1?detail=Full",
            "vizcraft://page/p1?format=text",
            "vizcraft://page/p1?detail",
            "vizcraft://shape/s1?detail=full&x=1",
        ] {
            assert_eq!(read(&mut s, uri).unwrap_err().0, -32602, "{uri}");
        }
        assert_eq!(read(&mut s, "vizcraft://page/p9?detail=full").unwrap_err().0, -32002);
        assert_eq!(read(&mut s, "vizcraft://page/?detail=full").unwrap_err().0, -32002);
        let (_, message) = read(&mut s, &format!("vizcraft://page/p1?detail={}", "x".repeat(600))).unwrap_err();
        assert!(message.len() < 400, "{message}");
        // The descriptions say so.
        let t = rpc(&mut s, "resources/templates/list", json!({}))["result"]["resourceTemplates"].as_array().unwrap().clone();
        for t in t.iter().filter(|t| {
            t["uriTemplate"].as_str().unwrap().starts_with("vizcraft://page") || t["uriTemplate"].as_str().unwrap().starts_with("vizcraft://shape")
        }) {
            assert!(t["description"].as_str().unwrap().contains("?detail=full"), "{t}");
        }
    }

    #[test]
    fn ids_are_percent_decoded() {
        let mut s = up();
        assert_eq!(read(&mut s, "vizcraft://page/p%202%2F%C3%A9").unwrap().1["name"], "Second");
        assert_eq!(read(&mut s, "vizcraft://page/p%202%2f%c3%a9").unwrap().1["name"], "Second", "lower-case escapes");
        assert_eq!(read(&mut s, "vizcraft://shape/s%E4%B8%AD").unwrap().1["page"], "p 2/\u{e9}");
        // Unencoded unicode is taken as it is.
        assert_eq!(read(&mut s, "vizcraft://shape/s\u{4e2d}").unwrap().1["shape"]["id"], "s\u{4e2d}");
        assert_eq!(read(&mut s, "vizcraft://page/%70%31").unwrap().1["name"], "Main");
    }

    #[test]
    fn unknown_and_missing_resources_are_not_found() {
        let mut s = up();
        for uri in [
            "vizcraft://nope",
            "vizcraft://page/",
            "vizcraft://shape/",
            "vizcraft://page/p9",
            "vizcraft://shape/s9",
            "vizcraft://page/p1/extra",
            "vizcraft://page/p1/extra?detail=full",
            "vizcraft://commands/",
            "VIZCRAFT://COMMANDS",
            "file:///etc/passwd",
            "",
        ] {
            assert_eq!(read(&mut s, uri).unwrap_err().0, -32002, "{uri:?}");
        }
        assert_eq!(rpc(&mut s, "resources/read", json!({}))["error"]["code"], -32602);
        assert_eq!(rpc(&mut s, "resources/read", json!({"uri": 7}))["error"]["code"], -32602);
    }

    #[test]
    fn hostile_uris_are_refused_quickly_and_quoted_briefly() {
        let mut s = up();
        let long_id = "a".repeat(MAX_ID_BYTES + 1);
        for uri in [
            format!("vizcraft://page/{}", "x".repeat(3_000_000)),
            format!("vizcraft://shape/{long_id}"),
            format!("vizcraft://shape/{}", "%41".repeat(MAX_ID_BYTES + 1)),
            "vizcraft://shape/%".to_string(),
            "vizcraft://shape/%4".to_string(),
            "vizcraft://shape/%zz".to_string(),
            "vizcraft://shape/%FF%FE".to_string(),
            "vizcraft://shape/%C3".to_string(),
            "vizcraft://shape/a%".to_string(),
            "vizcraft://page/%00".to_string(),
            format!("vizcraft://page/{}", "\u{4e2d}".repeat(200)),
            "vizcraft://shape/%E2%80%AE".to_string(),
        ] {
            let (code, message) = read(&mut s, &uri).unwrap_err();
            assert_eq!(code, -32002, "{}", &uri[..uri.len().min(40)]);
            assert!(message.len() < 400, "{message}");
        }
    }

    #[test]
    fn decode_and_encode_round_trip() {
        for id in ["p1", "a b", "p 2/\u{e9}", "s\u{4e2d}", "%", "100%", "~._-"] {
            assert_eq!(decode_id(&encode_id(id)).as_deref(), Some(id), "{id}");
        }
        assert_eq!(encode_id("a b/c"), "a%20b%2Fc");
        assert_eq!(decode_id(""), None);
        assert_eq!(decode_id(&"a".repeat(MAX_ID_BYTES)).map(|s| s.len()), Some(MAX_ID_BYTES));
        assert_eq!(decode_id(&"a".repeat(MAX_ID_BYTES + 1)), None);
    }
}
