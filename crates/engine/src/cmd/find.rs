//! Find and Replace over shape text, connector text and shape data values, on the active page or
//! the whole document.
//!
//! A hit is one field of one item: `{"page", "id", "field"}` where `field` is `text` or
//! `data:<key>`. Every searchable field comes from [`fields`], so widening the search (typed
//! shape data, say) means changing that one function and [`set_field`]. Matching works on chars,
//! not bytes, so case folding and whole-word checks never slice a string inside a character.

use serde_json::{Value, json};
use vizcraft_diagram::{Connector, Document, Shape, layer};

use super::{opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Longest search text accepted, in chars.
pub const MAX_NEEDLE: usize = 1024;

/// Most hits one search reports; a search past this stops, so a hostile document cannot make
/// one command build an unbounded list.
pub const MAX_HITS: usize = 10_000;

/// Return the find commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.find", "Find", "Home > Editing", find).params(
            r#"{"text": string, "matchCase"?: bool, "wholeWord"?: bool, "scope"?: "page"|"document", "inData"?: bool} -> {"hits": [{"page", "id", "field", "value"}]}; field is "text" or "data:<key>""#,
        ),
        CommandSpec::new("edit.replace", "Replace", "Home > Editing", replace).params(
            r#"{"text": string, "with": string, "all"?: bool, "at"?: {"page", "id", "field"}, "matchCase"?, "wholeWord"?, "scope"?, "inData"?} -> {"replaced": n, "changed": [hit], "skipped": [id], "hits": [hit]}; without "all" replaces in the hit "at" (default: the first)"#,
        ),
    ]
}

/// What to look for and how.
#[derive(Clone, Debug)]
pub struct Query {
    needle: Vec<char>,
    match_case: bool,
    whole_word: bool,
    /// The whole document instead of the active page.
    pub document: bool,
    /// Shape data values as well as text.
    pub in_data: bool,
}

impl Query {
    /// The query from command params; the search text must be non-empty and at most
    /// [`MAX_NEEDLE`] chars.
    pub fn from_params(v: &Value) -> Result<Query, EngineError> {
        let text = p::req_str(v, "text")?;
        let needle: Vec<char> = text.chars().collect();
        if needle.is_empty() {
            return Err(EngineError::Params("`text` must not be empty".into()));
        }
        if needle.len() > MAX_NEEDLE {
            return Err(EngineError::Params(format!("`text` is longer than {MAX_NEEDLE} characters")));
        }
        let document = match opt_str(v, "scope")? {
            None | Some("page") => false,
            Some("document") => true,
            Some(o) => return Err(EngineError::Params(format!("`scope` must be page or document, not `{}`", short(o)))),
        };
        Ok(Query {
            needle,
            match_case: opt_bool(v, "matchCase")?.unwrap_or(false),
            whole_word: opt_bool(v, "wholeWord")?.unwrap_or(false),
            document,
            in_data: opt_bool(v, "inData")?.unwrap_or(false),
        })
    }

    fn same(&self, a: char, b: char) -> bool {
        a == b || (!self.match_case && a.to_lowercase().eq(b.to_lowercase()))
    }

    /// Start indexes (in chars) of the non-overlapping matches in `hay`.
    pub fn matches(&self, hay: &[char]) -> Vec<usize> {
        let n = self.needle.len();
        let mut out = Vec::new();
        let mut i = 0;
        while i + n <= hay.len() {
            let hit = hay.get(i..i + n).is_some_and(|w| w.iter().zip(&self.needle).all(|(a, b)| self.same(*a, *b)));
            let bounded = !self.whole_word
                || (i.checked_sub(1).and_then(|j| hay.get(j)).is_none_or(|c| !word_char(*c)) && hay.get(i + n).is_none_or(|c| !word_char(*c)));
            if hit && bounded {
                out.push(i);
                i += n;
            } else {
                i += 1;
            }
        }
        out
    }

    /// Whether `hay` holds a match.
    pub fn found(&self, hay: &str) -> bool {
        !self.matches(&hay.chars().collect::<Vec<_>>()).is_empty()
    }

    /// `hay` with every match replaced by `with`, and how many there were.
    pub fn replace(&self, hay: &str, with: &str) -> (String, usize) {
        let chars: Vec<char> = hay.chars().collect();
        let starts = self.matches(&chars);
        if starts.is_empty() {
            return (hay.to_string(), 0);
        }
        let mut out = String::with_capacity(hay.len());
        let mut at = 0;
        for &st in &starts {
            out.extend(chars.get(at..st).unwrap_or_default());
            out.push_str(with);
            at = st + self.needle.len();
        }
        out.extend(chars.get(at..).unwrap_or_default());
        (out, starts.len())
    }
}

/// Letters, digits and `_` make up words for the whole-word check.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// One searchable item.
enum Item<'a> {
    Shape(&'a Shape),
    Connector(&'a Connector),
}

/// The searchable fields of an item: its text, then (with `in_data`) its data values as
/// `data:<key>`. The one place that says what Find searches.
fn fields(item: &Item, in_data: bool) -> Vec<(String, String)> {
    let (text, ext) = match item {
        Item::Shape(s) => (&s.text, &s.ext),
        Item::Connector(c) => (&c.text, &c.ext),
    };
    let mut out = vec![("text".to_string(), text.clone())];
    if let (true, Item::Shape(sh)) = (in_data, item) {
        // Typed shape data rows first, then format round-trip data under keys no row uses.
        out.extend(sh.data.rows.iter().filter(|r| !r.value.is_empty()).map(|r| (format!("data:{}", r.key), r.value.clone())));
        out.extend(ext.iter().filter(|(k, _)| !sh.data.rows.iter().any(|r| &r.key == *k)).map(|(k, v)| (format!("data:{k}"), v.clone())));
    }
    out
}

/// Every hit for `q`, in page order (the active page alone unless the scope is the document),
/// shapes before connectors, fields in [`fields`] order.
pub fn search(doc: &Document, q: &Query) -> Vec<Value> {
    let mut hits = Vec::new();
    let active = doc.active_index();
    for (i, page) in doc.pages.iter().enumerate() {
        if !q.document && i != active {
            continue;
        }
        let items = page.shapes.iter().map(Item::Shape).chain(page.connectors.iter().map(Item::Connector));
        for item in items {
            let id = match &item {
                Item::Shape(s) => &s.id,
                Item::Connector(c) => &c.id,
            };
            for (field, value) in fields(&item, q.in_data) {
                if q.found(&value) {
                    hits.push(json!({"page": page.id, "id": id, "field": field, "value": value}));
                    if hits.len() >= MAX_HITS {
                        return hits;
                    }
                }
            }
        }
    }
    hits
}

fn find(s: &mut Session, v: &Value) -> CmdResult {
    let q = Query::from_params(v)?;
    Ok(json!({"hits": search(&s.doc, &q)}))
}

/// Write `value` into one field of an item. Text on a shape or connector that draws a model item
/// renames that item, so every view of it follows (as `shape.setText` does).
fn set_field(doc: &mut Document, page_id: &str, id: &str, field: &str, value: &str) -> Result<(), EngineError> {
    let page = doc.pages.iter().find(|p| p.id == page_id).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(page_id))))?;
    let element = page.shape(id).and_then(|s| s.element.clone()).filter(|e| doc.model.element(e).is_some());
    let relationship = page.connector(id).and_then(|c| c.relationship.clone()).filter(|r| doc.model.relationship(r).is_some());
    if field == "text" {
        if let Some(e) = element {
            return super::model::rename_element(doc, &e, value);
        }
        if let Some(r) = relationship {
            return super::model::rename_relationship(doc, &r, value);
        }
    }
    let page = doc.pages.iter_mut().find(|p| p.id == page_id).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(page_id))))?;
    if let Some(sh) = page.shape_mut(id) {
        match field.strip_prefix("data:") {
            Some(k) if sh.data.rows.iter().any(|r| r.key == k) => {
                if let Some(row) = sh.data.rows.iter_mut().find(|r| r.key == k) {
                    // A replacement must still fit the row's type, unit and enum.
                    row.value = vizcraft_diagram::data::normalize_text(row.kind, &row.values, &row.unit, value)
                        .map_err(|e| EngineError::Params(format!("`{}` {}: {e}", short(id), short(field))))?;
                }
            }
            Some(k) if sh.ext.contains_key(k) => {
                sh.ext.insert(k.to_string(), value.to_string());
            }
            Some(_) => return Err(EngineError::Params(format!("`{}` has no field `{}`", short(id), short(field)))),
            None => sh.text = value.to_string(),
        }
        sh.sanitize();
    } else if let Some(c) = page.connector_mut(id) {
        if field != "text" {
            return Err(EngineError::Params(format!("`{}` has no field `{}`", short(id), short(field))));
        }
        c.text = value.to_string();
        c.sanitize();
    } else {
        return Err(EngineError::Params(format!("`{}` is not on page `{}`", short(id), short(page_id))));
    }
    Ok(())
}

/// Whether item `id` on page `page_id` is a shape on a locked layer.
fn locked(doc: &Document, page_id: &str, id: &str) -> bool {
    doc.pages.iter().find(|p| p.id == page_id).is_some_and(|p| p.shape(id).is_some_and(|s| layer::locked(p, s)))
}

fn hit_key(h: &Value) -> (&str, &str, &str) {
    let f = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or("");
    (f("page"), f("id"), f("field"))
}

fn replace(s: &mut Session, v: &Value) -> CmdResult {
    let q = Query::from_params(v)?;
    let with = p::req_str(v, "with")?;
    let all = opt_bool(v, "all")?.unwrap_or(false);
    let hits = search(&s.doc, &q);
    let chosen: Vec<Value> = if all {
        hits
    } else {
        match v.get("at") {
            None | Some(Value::Null) => hits.into_iter().take(1).collect(),
            Some(at) => {
                let want = hit_key(at);
                let h =
                    hits.into_iter().find(|h| hit_key(h) == want).ok_or_else(|| EngineError::Params("`at` is not a hit of this search".into()))?;
                vec![h]
            }
        }
    };
    let (mut replaced, mut changed, mut skipped) = (0usize, Vec::new(), Vec::<String>::new());
    for h in chosen {
        let (page, id, field) = hit_key(&h);
        let (page, id, field) = (page.to_string(), id.to_string(), field.to_string());
        if locked(&s.doc, &page, &id) {
            if !skipped.contains(&id) {
                skipped.push(id);
            }
            continue;
        }
        let old = h.get("value").and_then(Value::as_str).unwrap_or("");
        let (new, n) = q.replace(old, with);
        if n == 0 {
            continue;
        }
        set_field(&mut s.doc, &page, &id, &field, &new)?;
        replaced += n;
        changed.push(json!({"page": page, "id": id, "field": field}));
    }
    super::prune_selection(s);
    Ok(json!({"replaced": replaced, "changed": changed, "skipped": skipped, "hits": search(&s.doc, &q)}))
}
