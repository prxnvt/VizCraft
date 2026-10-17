//! Notation schema discovery as MCP resources.
//!
//! `vizcraft://notation` lists the notation packs, `vizcraft://notation/{pack}` describes one
//! pack and `vizcraft://notation/{pack}/{type}` one element or relationship type. Each read is
//! one engine command (`notation.list` or `notation.describe`) sent through `engine.execute`, so
//! the answer always matches the app on the other end. An app without those commands (one built
//! before the model core) lists only the catalog entry and reports reads as unavailable.

use serde_json::{Value, json};

use crate::NOTATION_URI;
use crate::backend::Backend;

/// Longest resource URI accepted; real ones are a few dozen characters.
const MAX_URI_CHARS: usize = 300;

/// Longest pack or type id accepted in a URI.
const MAX_ID_CHARS: usize = 100;

/// Most per-pack entries `resources/list` shows, so a misbehaving app cannot make the list huge.
const MAX_PACKS: usize = 200;

/// A notation resource a URI names.
#[derive(Debug, PartialEq, Eq)]
enum Notation {
    /// Every pack: `vizcraft://notation`.
    Catalog,
    /// One pack: `vizcraft://notation/{pack}`.
    Pack(String),
    /// One type in a pack: `vizcraft://notation/{pack}/{type}`.
    Type(String, String),
}

/// Whether `id` is a pack or type id we pass on: letters, digits, `_` and `.`, not too long, with
/// a dot only between two names (`flowchart.process`), so `..` and the like are refused. Anything
/// else (slashes, percent escapes, spaces) cannot name a pack or type.
fn valid_id(id: &str) -> bool {
    id.chars().count() <= MAX_ID_CHARS && id.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The notation resource `uri` names, or `None` when it names none.
fn parse(uri: &str) -> Option<Notation> {
    if uri.chars().count() > MAX_URI_CHARS {
        return None;
    }
    if uri == NOTATION_URI {
        return Some(Notation::Catalog);
    }
    let rest = uri.strip_prefix(NOTATION_URI)?.strip_prefix('/')?;
    let mut parts = rest.split('/');
    let pack = parts.next().filter(|p| valid_id(p))?;
    let found = match parts.next() {
        None => Notation::Pack(pack.to_string()),
        Some(ty) if valid_id(ty) => Notation::Type(pack.to_string(), ty.to_string()),
        Some(_) => return None,
    };
    // Nothing may follow the type.
    parts.next().is_none().then_some(found)
}

/// Run one notation command and return its result.
fn run(backend: &mut dyn Backend, command: &str, params: Value) -> Result<Value, String> {
    backend.call("engine.execute", json!({"command": command, "params": params}))
}

/// The `resources/list` entries: the catalog, then one per pack the app reports. When the app
/// cannot list packs, only the catalog entry is shown.
pub(crate) fn resources(backend: &mut dyn Backend) -> Vec<Value> {
    let mut out = vec![json!({
        "uri": NOTATION_URI,
        "name": "notation",
        "title": "Notation packs",
        "description": "The notation packs the app knows (id, version, name, audience, description and type ids); read vizcraft://notation/{pack} for one pack's schema.",
        "mimeType": "application/json",
    })];
    let Ok(list) = run(backend, "notation.list", json!({})) else { return out };
    let Some(packs) = list.get("packs").and_then(Value::as_array) else { return out };
    for pack in packs.iter().take(MAX_PACKS) {
        let Some(id) = pack.get("id").and_then(Value::as_str).filter(|id| valid_id(id)) else { continue };
        let title = pack.get("name").and_then(Value::as_str).unwrap_or(id);
        let about = pack.get("description").and_then(Value::as_str).map_or_else(String::new, |d| format!("{d} "));
        out.push(json!({
            "uri": format!("{NOTATION_URI}/{id}"),
            "name": format!("notation-{id}"),
            "title": format!("{title} notation"),
            "description": format!(
                "{about}Schema of the {id} pack: element and relationship types with their properties, ports, legal connections and rules."
            ),
            "mimeType": "application/json",
        }));
    }
    out
}

/// The `resources/templates/list` entries.
pub(crate) fn templates() -> Vec<Value> {
    vec![
        json!({
            "uriTemplate": format!("{NOTATION_URI}/{{pack}}"),
            "name": "notation-pack",
            "title": "Notation pack schema",
            "description": "One notation pack's schema: element and relationship types, properties, ports, legal connections and rules. Pack ids come from vizcraft://notation.",
            "mimeType": "application/json",
        }),
        json!({
            "uriTemplate": format!("{NOTATION_URI}/{{pack}}/{{type}}"),
            "name": "notation-type",
            "title": "Notation type",
            "description": "One element or relationship type of a pack, with its properties, ports and legal relationships. Type ids are pack-qualified, such as flowchart.process.",
            "mimeType": "application/json",
        }),
    ]
}

/// Read one notation resource: the command's result as pretty JSON text. `Err` carries why the
/// app could not answer (no such pack or type, or an app without the notation commands).
fn read(backend: &mut dyn Backend, which: &Notation) -> Result<String, String> {
    let (command, params) = match which {
        Notation::Catalog => ("notation.list", json!({})),
        Notation::Pack(pack) => ("notation.describe", json!({"pack": pack})),
        Notation::Type(pack, ty) => ("notation.describe", json!({"pack": pack, "type": ty})),
    };
    let result = run(backend, command, params).map_err(|e| format!("{command} failed: {e}"))?;
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

/// At most the first 200 characters of `uri`, so an error never echoes a huge URI whole.
fn clip(uri: &str) -> String {
    uri.chars().take(200).collect()
}

/// The `resources/read` result for `uri` when it is a notation URI, or `None` when it is not
/// one (the caller tries its other resources). `Err` carries the message for a resource that is
/// not there: no such pack or type, or an app without the notation commands.
pub(crate) fn read_uri(backend: &mut dyn Backend, uri: &str) -> Option<Result<Value, String>> {
    if !uri.starts_with(NOTATION_URI) {
        return None;
    }
    let Some(which) = parse(uri) else { return Some(Err(format!("resource not found: {}", clip(uri)))) };
    Some(
        read(backend, &which)
            .map(|text| json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": text}]}))
            .map_err(|e| format!("resource not available: {uri}: {e}")),
    )
}
