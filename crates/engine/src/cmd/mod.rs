//! Command implementations, one module per menu: each exposes `specs()`.

pub mod addon;
pub mod analyze;
pub mod arrange;
pub mod batch;
pub mod callout;
pub mod comment;
pub mod compartment;
pub mod connector;
pub mod connector_kind;
pub mod container;
pub mod data;
pub mod datagraphic;
pub mod datalink;
pub mod describe;
pub mod design;
pub mod document;
pub mod edit;
pub mod effects;
pub mod element;
pub mod file;
pub mod find;
pub mod generate;
pub mod help;
pub mod ink;
pub mod insert;
pub mod inspector;
pub mod lane;
pub mod layer;
pub mod layout;
pub mod link;
pub mod r#macro;
pub mod master;
pub mod model;
pub mod net;
pub mod notation;
pub mod page;
pub mod pattern;
pub mod picture;
pub mod print;
pub mod process;
pub mod proof;
pub mod protect;
pub mod relationship;
pub mod report;
pub mod routing;
pub mod script;
pub mod shape;
pub mod shapeop;
pub mod sheet;
pub mod suggest;
pub mod table;
pub mod text;
pub mod validate;
pub mod variant;
pub mod view;
pub mod wire;
pub mod xref;

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::Page;

use crate::{CmdResult, EngineError, Registry, Session};

/// The full registry, in menu order.
pub fn registry() -> Registry {
    let mut v = Vec::new();
    v.extend(file::specs());
    v.extend(edit::specs());
    v.extend(find::specs());
    v.extend(batch::specs());
    v.extend(view::specs());
    v.extend(shape::specs());
    v.extend(text::specs());
    v.extend(arrange::specs());
    v.extend(container::specs());
    v.extend(lane::specs());
    v.extend(callout::specs());
    v.extend(compartment::specs());
    v.extend(connector::specs());
    v.extend(connector_kind::specs());
    v.extend(page::specs());
    v.extend(routing::specs());
    v.extend(print::specs());
    v.extend(layer::specs());
    v.extend(data::specs());
    v.extend(datagraphic::specs());
    v.extend(datalink::specs());
    v.extend(describe::specs());
    v.extend(design::specs());
    v.extend(sheet::specs());
    v.extend(xref::specs());
    v.extend(layout::specs());
    v.extend(model::specs());
    v.extend(element::specs());
    v.extend(inspector::specs());
    v.extend(relationship::specs());
    v.extend(validate::specs());
    v.extend(proof::specs());
    v.extend(report::specs());
    v.extend(table::specs());
    v.extend(analyze::specs());
    v.extend(pattern::specs());
    v.extend(document::specs());
    v.extend(notation::specs());
    v.extend(variant::specs());
    v.extend(insert::specs());
    v.extend(suggest::specs());
    v.extend(protect::specs());
    v.extend(shapeop::specs());
    v.extend(master::specs());
    v.extend(script::specs());
    v.extend(link::specs());
    v.extend(comment::specs());
    v.extend(process::specs());
    v.extend(addon::specs());
    v.extend(r#macro::specs());
    v.extend(picture::specs());
    v.extend(ink::specs());
    v.extend(help::specs());
    v.extend(generate::specs());
    v.extend(net::specs());
    v.extend(wire::specs());
    Registry::new(v)
}

/// The standard result: the selection after the command.
pub fn sel_result(s: &Session) -> CmdResult {
    Ok(json!({
        "primary": s.sel.primary,
        "items": s.sel.items,
        "range": s.sel.range,
    }))
}

/// `extra` (an object) with the selection after the command merged in as `primary` and `items`,
/// so a command can report the ids it touched and the selection in one result.
pub fn with_sel(s: &Session, mut extra: Value) -> CmdResult {
    if let Some(o) = extra.as_object_mut() {
        o.insert("primary".into(), json!(s.sel.primary));
        o.insert("items".into(), json!(s.sel.items));
    }
    Ok(extra)
}

/// Most ids one command accepts: a list longer than this is refused rather than walked, so a
/// hostile caller cannot make one command allocate or loop without bound.
pub const MAX_IDS: usize = 10_000;

/// Largest move or offset in one command: far beyond any page, small enough that adding it to a
/// clamped coordinate stays finite (an infinite sum would reset the coordinate to zero).
pub const MAX_DELTA: f64 = 2.0e7;

/// Smallest shape width or height a command leaves, so a shape never collapses to nothing that
/// cannot be clicked or resized back.
pub const MIN_SIZE: f64 = 1.0;

/// Drop selected ids that no longer name a shape or connector (after a delete, a cut or a
/// document change), keeping the primary in the list. Called at the end of every command that
/// changes shapes; `Session::clamp_selection` stays an empty hook.
pub fn prune_selection(s: &mut Session) {
    let doc = &s.doc;
    s.sel.items.retain(|id| doc.page().is_some_and(|p| p.contains_id(id)));
    if s.sel.primary.as_ref().is_some_and(|p| !s.sel.items.contains(p)) {
        s.sel.primary = s.sel.items.first().cloned();
    }
}

/// Select exactly `ids` (the first is the primary).
pub fn select(s: &mut Session, ids: Vec<String>) {
    s.sel.primary = ids.first().cloned();
    s.sel.items = ids;
    s.sel.range = None;
}

/// The `ids` param: `None` when absent or null, else a list of strings of at most [`MAX_IDS`].
/// Anything else (a string, a number, an array with a non-string in it) is a params error, so a
/// typo is reported instead of silently acting on the selection.
pub fn ids_param(v: &Value) -> Result<Option<Vec<String>>, EngineError> {
    match v.get("ids") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            if a.len() > MAX_IDS {
                return Err(EngineError::Params(format!("`ids` has more than {MAX_IDS} entries")));
            }
            a.iter()
                .map(|x| x.as_str().map(str::to_string).ok_or_else(|| EngineError::Params("`ids` must be an array of strings".into())))
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        }
        Some(_) => Err(EngineError::Params("`ids` must be an array of strings".into())),
    }
}

/// An optional finite number param: absent or null is `None`; a string, NaN-like text or any other
/// non-number is a params error.
pub fn num(v: &Value, k: &str) -> Result<Option<f64>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x.as_f64().filter(|f| f.is_finite()).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}` must be a number"))),
    }
}

/// A required finite number param.
pub fn req_num(v: &Value, k: &str) -> Result<f64, EngineError> {
    num(v, k)?.ok_or_else(|| EngineError::Params(format!("`{k}` (number) is required")))
}

/// An optional string param: absent or null is `None`, a non-string is a params error.
pub fn opt_str<'a>(v: &'a Value, k: &str) -> Result<Option<&'a str>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t)),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be a string"))),
    }
}

/// An optional boolean param: absent or null is `None`, a non-boolean is a params error.
pub fn opt_bool(v: &Value, k: &str) -> Result<Option<bool>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be true or false"))),
    }
}

/// The active page, mutably. A document always has one after `sanitize`; the error is a guard.
/// Longest id a caller may choose for a new shape or connector.
const MAX_CHOSEN_ID: usize = 64;

/// The id for a new item: the caller's `id` when given, else the next free one with `prefix`.
/// Agents name items so one `command_batch` can add shapes and connect them without reading ids
/// back; a chosen id must be short, plain (letters, digits, `_`, `-`, `.`) and not already used.
pub fn new_id(s: &mut Session, v: &Value, prefix: &str) -> Result<String, EngineError> {
    let Some(id) = opt_str(v, "id")? else { return Ok(s.doc.next_id(prefix)) };
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
    if id.is_empty() || id.len() > MAX_CHOSEN_ID || !id.chars().all(plain) {
        return Err(EngineError::Params(format!("`id` must be 1 to {MAX_CHOSEN_ID} letters, digits, `_`, `-` or `.`")));
    }
    if s.doc.contains_id(id) {
        return Err(EngineError::Params(format!("`{id}` is already used")));
    }
    Ok(id.to_string())
}

pub fn page_mut(s: &mut Session) -> Result<&mut Page, EngineError> {
    s.doc.page_mut().ok_or_else(|| EngineError::Failed("the document has no page".into()))
}

/// Enabled when the active page has at least one shape.
pub fn has_shapes(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| !p.shapes.is_empty()) { None } else { Some("add a shape first") }
}

/// Enabled when the active page has at least one connector.
pub fn has_connectors(s: &Session) -> Option<&'static str> {
    if s.doc.page().is_some_and(|p| !p.connectors.is_empty()) { None } else { Some("add a connector first") }
}

/// Enabled when something is selected.
pub fn has_selection(s: &Session) -> Option<&'static str> {
    if s.sel.items.is_empty() { Some("select a shape or connector first") } else { None }
}

/// Which items a command acts on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kinds {
    /// Shapes only.
    Shapes,
    /// Connectors only.
    Connectors,
    /// Shapes and connectors.
    Both,
}

impl Kinds {
    fn accepts(self, page: &Page, id: &str) -> bool {
        match self {
            Kinds::Shapes => page.shape(id).is_some(),
            Kinds::Connectors => page.connector(id).is_some(),
            Kinds::Both => page.contains_id(id),
        }
    }
    fn noun(self) -> &'static str {
        match self {
            Kinds::Shapes => "shape",
            Kinds::Connectors => "connector",
            Kinds::Both => "shape or connector",
        }
    }
}

/// The items a command with an `ids?` param acts on: the given ids (each must name an item of the
/// right kind on the page, or the command fails without changing anything), else the selected
/// items of that kind. Duplicates are dropped; an empty result is a params error that says what
/// to select.
pub fn targets(s: &Session, v: &Value, kinds: Kinds) -> Result<Vec<String>, EngineError> {
    let Some(page) = s.doc.page() else { return Err(EngineError::Failed("the document has no page".into())) };
    let mut out: Vec<String> = Vec::new();
    match ids_param(v)? {
        Some(ids) => {
            for id in ids {
                if !kinds.accepts(page, &id) {
                    return Err(EngineError::Params(format!("`{}` is not a {} on this page", short(&id), kinds.noun())));
                }
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        None => {
            for id in &s.sel.items {
                if kinds.accepts(page, id) && !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
    }
    if out.is_empty() {
        return Err(EngineError::Params(format!("select a {} first (or pass `ids`)", kinds.noun())));
    }
    Ok(out)
}

/// An id or name cut to a readable length for an error message (a hostile caller may send
/// megabytes).
pub fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// What [`delete_on_page`] took off one page.
#[derive(Default)]
pub struct Deleted {
    /// Shape and connector ids removed, connectors first.
    pub items: Vec<String>,
    /// Connectors left on the page with a glued end turned free.
    pub detached: Vec<String>,
    /// Relationships drawn by the removed connectors, each once.
    pub relationships: Vec<String>,
}

/// Delete shapes and connectors by id from `page` (unknown ids are skipped). Connectors are
/// removed first; then each connector still glued to a deleted shape is either removed too, when
/// both of its ends were glued to deleted shapes (it would be left hanging in empty space), or has
/// the glued end turned into a free end where it was drawn, so a single deleted step leaves its
/// arrows in place to re-glue.
pub fn delete_on_page(page: &mut Page, ids: &[String]) -> Deleted {
    // A deleted group takes its members with it.
    let ids = &vizcraft_diagram::arrange::with_descendants(page, ids);
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let shapes: Vec<String> = page.shapes.iter().filter(|x| wanted.contains(x.id.as_str())).map(|x| x.id.clone()).collect();
    let gone_shapes: HashSet<&str> = shapes.iter().map(String::as_str).collect();
    let gone = |id: Option<&str>| id.is_some_and(|i| gone_shapes.contains(i));
    let mut connectors: Vec<String> = Vec::new();
    let mut out = Deleted::default();
    for c in &page.connectors {
        if wanted.contains(c.id.as_str()) {
            connectors.push(c.id.clone());
            continue;
        }
        match (gone(c.start.shape()), gone(c.end.shape())) {
            (true, true) => connectors.push(c.id.clone()),
            (true, false) | (false, true) => out.detached.push(c.id.clone()),
            (false, false) => {}
        }
    }
    for id in &connectors {
        if let Some(c) = page.remove_connector(id) {
            if let Some(r) = c.relationship
                && !out.relationships.contains(&r)
            {
                out.relationships.push(r);
            }
            out.items.push(id.clone());
        }
    }
    for id in &shapes {
        page.detach_shape(id);
        if page.remove_shape(id).is_some() {
            out.items.push(id.clone());
        }
    }
    // A group left with no members goes too, and the others refit.
    out.items.extend(vizcraft_diagram::arrange::tidy_groups(page));
    out
}

/// Remove from the model each of `relationships` that no connector on any page draws any more:
/// deleting a relationship's last connector view deletes the relationship (it has no other
/// handle on the canvas). Relationships still drawn somewhere, and ids that are not
/// relationships, are left alone.
pub fn drop_undrawn_relationships(doc: &mut vizcraft_diagram::Document, relationships: &[String]) {
    if relationships.is_empty() {
        return;
    }
    let drawn: HashSet<&str> = doc.pages.iter().flat_map(|p| p.connectors.iter().filter_map(|c| c.relationship.as_deref())).collect();
    let doomed: Vec<String> = relationships.iter().filter(|r| !drawn.contains(r.as_str()) && doc.model.relationship(r).is_some()).cloned().collect();
    if !doomed.is_empty() {
        doc.model.remove(&doomed);
    }
}

/// Delete shapes and connectors by id from the active page (see [`delete_on_page`]); a
/// relationship whose last connector view went is removed from the model, while elements stay in
/// the model when their shapes go. A device with exactly one wire segment on each of two pins
/// heals its wire as it goes (see [`delete_items_heal`]). Returns `{"deleted": [ids],
/// "detached": [connector ids]}` (and `"healed": [connector ids]` when a wire was healed) and the
/// selection.
pub fn delete_items(s: &mut Session, ids: &[String]) -> CmdResult {
    delete_items_heal(s, ids, None)
}

/// [`delete_items`] with the healing choice: `Some(false)` never heals; otherwise each deleted
/// shape with exactly one wire segment on each of two pins (not two different named nets, and
/// neither segment itself deleted) has its two segments joined through where its pins were, so
/// taking a device out of a wire leaves the wire whole.
pub fn delete_items_heal(s: &mut Session, ids: &[String], heal: Option<bool>) -> CmdResult {
    layer::ensure_unlocked(s, ids)?;
    protect::check(s, ids, protect::Lock::Delete)?;
    let mut healed: Vec<String> = Vec::new();
    if heal != Some(false) && !s.doc.model.nets.is_empty() {
        let doomed: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let plans: Vec<(String, String, String)> = match s.doc.page() {
            Some(page) => ids
                .iter()
                .filter_map(|id| {
                    let (a, b) = net::heal_pair(&s.doc, page, id)?;
                    (!doomed.contains(a.as_str()) && !doomed.contains(b.as_str())).then(|| (id.clone(), a, b))
                })
                .collect(),
            None => Vec::new(),
        };
        let page = page_mut(s)?;
        for (shape, a, b) in plans {
            if vizcraft_diagram::net::join(page, &a, &b, &shape) {
                healed.push(a);
            }
        }
    }
    let page = page_mut(s)?;
    let page_id = page.id.clone();
    let d = delete_on_page(page, ids);
    // A comment thread outlives the shape it was pinned to: it moves to the page, in the same
    // undo step as the delete.
    for c in s.doc.comments.iter_mut().filter(|c| c.anchor.page == page_id) {
        if c.anchor.shape.as_ref().is_some_and(|sh| d.items.contains(sh)) {
            c.anchor.shape = None;
        }
    }
    drop_undrawn_relationships(&mut s.doc, &d.relationships);
    prune_selection(s);
    if healed.is_empty() {
        with_sel(s, json!({"deleted": d.items, "detached": d.detached}))
    } else {
        with_sel(s, json!({"deleted": d.items, "detached": d.detached, "healed": healed}))
    }
}

/// ISO-8601 timestamp (UTC, seconds), without a date-time dependency.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 to (year, month, day): Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
