//! Tables over nets: `table.list {subject: "nets" | "connections"}` and the same lists for
//! `table.export`, and `table.set` on a net (its number, name or a property).
//!
//! Rows show numbers and device names, not ids; each row's id is in the list's `ids`. The nets
//! table has one row per net. The connections table (the wire list) has one row per
//! wire segment, its two ends named as `device:pin`; an end on a junction names a pin of the same
//! net instead, taken in the order `net.members` lists them, skipping the pin at the segment's
//! other end (a convention of ours: junctions are not devices).

use serde_json::{Value, json};
use vizcraft_diagram::net::{NetIndex, junction_of, pin_of};
use vizcraft_diagram::{Document, End};
use vizcraft_model::{Net, NetKind, PropValue};

use super::model::cat;
use super::{opt_str, short};
use crate::{CmdResult, EngineError, Session};

/// Most rows one list returns.
const MAX_ROWS: usize = 10_000;

/// The subjects this module answers for.
pub const SUBJECTS: &[&str] = &["nets", "connections"];

/// A table of nets or connections: columns, row ids, rows, and whether it was cut.
pub struct NetTable {
    /// Column names.
    pub columns: Vec<String>,
    /// The net or connector id of each row.
    pub ids: Vec<String>,
    /// Cell text.
    pub rows: Vec<Vec<String>>,
    /// More rows than [`MAX_ROWS`].
    pub truncated: bool,
}

/// The net kinds `kind` asks for: all when absent.
fn kind_param(v: &Value) -> Result<Option<NetKind>, EngineError> {
    match opt_str(v, "kind")? {
        None => Ok(None),
        Some(k) => NetKind::from_name(k).map(Some).ok_or_else(|| EngineError::Params(format!("unknown net kind `{}`", short(k)))),
    }
}

fn label(n: &Net) -> String {
    if !n.number.is_empty() {
        n.number.clone()
    } else if !n.name.is_empty() {
        n.name.clone()
    } else {
        n.id.clone()
    }
}

fn prop(n: &Net, k: &str) -> String {
    n.properties.get(k).map(PropValue::display).unwrap_or_default()
}

/// `device:pin` for pin `(shape, point)` on page `page`.
fn pin_name(doc: &Document, page: usize, shape: &str, point: usize) -> String {
    let Some(sh) = doc.pages.get(page).and_then(|p| p.shape(shape)) else { return String::new() };
    let Some(e) = sh.element.as_deref().and_then(|e| doc.model.element(e)) else {
        return format!("{shape}:{point}");
    };
    let port = vizcraft_notation::electrical_rules::port_id(cat(), sh, e, point).unwrap_or_else(|| point.to_string());
    let device = if e.tag.is_empty() { e.id.clone() } else { e.tag.clone() };
    format!("{device}:{port}")
}

/// The table `subject` (`nets` or `connections`) of `doc`.
pub fn build(doc: &Document, subject: &str, v: &Value) -> Result<NetTable, EngineError> {
    let kind = kind_param(v)?;
    let ix = NetIndex::build(doc);
    let filter = opt_str(v, "filter")?.unwrap_or_default().to_lowercase();
    let mut t = match subject {
        "nets" => nets(doc, &ix, kind),
        "connections" => connections(doc, &ix, kind),
        other => return Err(EngineError::Params(format!("`subject` must be nets or connections, not `{}`", short(other)))),
    };
    if !filter.is_empty() {
        let keep: Vec<bool> = t.rows.iter().map(|r| r.iter().any(|c| c.to_lowercase().contains(&filter))).collect();
        let mut k = keep.iter();
        t.rows.retain(|_| k.next().copied().unwrap_or(false));
        let mut k = keep.iter();
        t.ids.retain(|_| k.next().copied().unwrap_or(false));
    }
    Ok(t)
}

fn nets(doc: &Document, ix: &NetIndex, kind: Option<NetKind>) -> NetTable {
    let columns = ["number", "name", "kind", "pins", "segments", "colour", "crossSection", "cable", "core"].map(String::from).to_vec();
    let mut t = NetTable { columns, ids: Vec::new(), rows: Vec::new(), truncated: false };
    for n in doc.model.nets.iter().filter(|n| kind.is_none_or(|k| n.kind == k)) {
        if t.rows.len() >= MAX_ROWS {
            t.truncated = true;
            break;
        }
        let entry = ix.nets.get(&n.id);
        t.ids.push(n.id.clone());
        t.rows.push(vec![
            n.number.clone(),
            n.name.clone(),
            n.kind.name().to_string(),
            entry.map(|e| e.pins.len()).unwrap_or(0).to_string(),
            entry.map(|e| e.segments.len()).unwrap_or(0).to_string(),
            prop(n, "colour"),
            prop(n, "crossSection"),
            prop(n, "cable"),
            prop(n, "core"),
        ]);
    }
    t
}

fn connections(doc: &Document, ix: &NetIndex, kind: Option<NetKind>) -> NetTable {
    let columns = ["wire", "from", "to", "colour", "crossSection", "page"].map(String::from).to_vec();
    let mut t = NetTable { columns, ids: Vec::new(), rows: Vec::new(), truncated: false };
    for (pi, page) in doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
        for c in &page.connectors {
            let Some(net) = ix.net_of_connector(&c.id).and_then(|n| doc.model.net(n)) else { continue };
            if kind.is_some_and(|k| net.kind != k) {
                continue;
            }
            if t.rows.len() >= MAX_ROWS {
                t.truncated = true;
                return t;
            }
            let pins = ix.nets.get(&net.id).map(|e| e.pins.as_slice()).unwrap_or_default();
            let own = |e: &End| pin_of(page, e);
            let (a, b) = (own(&c.start), own(&c.end));
            // A junction end names the first member pin that is not this segment's other end.
            let end_text = |e: &End, other: &Option<(String, usize)>| -> String {
                if let Some((s, p)) = pin_of(page, e) {
                    return pin_name(doc, pi, &s, p);
                }
                if junction_of(page, e).is_some()
                    && let Some((s, p)) = pins.iter().find(|p| Some(*p) != other.as_ref())
                {
                    let on = doc.pages.iter().position(|pg| pg.shape(s).is_some()).unwrap_or(pi);
                    return pin_name(doc, on, s, *p);
                }
                String::new()
            };
            t.ids.push(c.id.clone());
            t.rows.push(vec![
                label(net),
                end_text(&c.start, &b),
                end_text(&c.end, &a),
                prop(net, "colour"),
                prop(net, "crossSection"),
                page.name.clone(),
            ]);
        }
    }
    t
}

/// `table.list` for a net subject.
pub fn list(s: &Session, subject: &str, v: &Value) -> CmdResult {
    let t = build(&s.doc, subject, v)?;
    Ok(json!({"subject": subject, "columns": t.columns, "ids": t.ids, "count": t.rows.len(), "rows": t.rows, "truncated": t.truncated}))
}

/// `table.set` on net `id`: `number`, `name` or a property; null or empty text clears it.
pub fn set(s: &mut Session, id: &str, key: &str, value: &Value) -> CmdResult {
    let text = match value {
        Value::Null => String::new(),
        Value::String(t) => super::model::cut(t.trim(), 256),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return Err(EngineError::Params("`value` must be text, a number, a flag or null".into())),
    };
    if key == "name" && !text.is_empty() && s.doc.model.nets.iter().any(|n| n.id != id && n.name == text) {
        return Err(EngineError::Params(format!("another net is already named `{}`", short(&text))));
    }
    let n = s.doc.model.net_mut(id).ok_or_else(|| EngineError::Params(format!("no net `{}`", short(id))))?;
    match key {
        "number" => n.number = text.clone(),
        "name" => n.name = text.clone(),
        "id" | "kind" | "pins" | "segments" | "from" | "to" | "page" => {
            return Err(EngineError::Params(format!("`{key}` is not edited in a table; draw the wire again or use net.set")));
        }
        k => {
            super::model::check_key(k)?;
            if text.is_empty() {
                n.properties.remove(k);
            } else if n.properties.contains_key(k) || n.properties.len() < vizcraft_model::MAX_PROPERTIES {
                n.properties.insert(k.to_string(), PropValue::Text(text.clone()));
            }
        }
    }
    Ok(json!({"id": id, "key": key, "value": text}))
}
