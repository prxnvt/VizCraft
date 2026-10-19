//! Read-only descriptions of the document at three levels of detail, so an agent can learn what
//! is connected to what without paying for colours.
//!
//! `structure` is ids, stencils, texts, model element and relationship ids, glue with connection
//! point indexes, containment and shape data rows; `layout` adds integer positions, sizes,
//! rotation, route style and page size; `full` adds everything else the document stores (style,
//! `ext`, waypoints, connection points, ...). The result is JSON, or an indented text outline that
//! costs a fraction of it. Every command here only reads, so the document never changes.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};
use vizcraft_diagram::{Connector, End, Page, Shape};

use super::{MAX_IDS, opt_str, short};
use crate::{CmdResult, CommandSpec, Document, EngineError, Session, p};

/// Return the description commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("doc.describe", "Describe Document", "Document", doc_describe).params(
            r#"{"detail"?: "structure"|"layout"|"full" (default structure), "format"?: "json"|"text" (default json), "pages"?: [id|name|index] (default: all)} -> {"detail", "format", "pages": [...]} or {"detail", "format", "text"}; structure is ids, stencils, texts, glue, containment and data rows without coordinates"#,
        ),
        CommandSpec::new("page.describe", "Describe Page", "Page", page_describe).params(
            r#"{"page"?: id|name|index (default: active), "detail"?, "format"?} -> {"detail", "format", "page": {...}} or {"detail", "format", "text"}"#,
        ),
        CommandSpec::new("shape.describe", "Describe Shape", "Shape", shape_describe).params(
            r#"{"id": string, "page"?: id|name|index (default: any page), "detail"?, "format"?} -> {"detail", "format", "page", "shape": {...}, "connections": [...], "children"?: [id]} or {"detail", "format", "page", "text"}"#,
        ),
    ]
}

/// Most pages one `doc.describe` may name.
const MAX_PAGES: usize = MAX_IDS / 10;
/// Most shapes and connectors described in one call, over all pages; the rest are counted, not shown.
const MAX_ITEMS: usize = 20_000;
/// Longest text or name quoted on one line of the text outline, in characters.
const MAX_LINE_TEXT: usize = 200;
/// Most children listed for one shape.
const MAX_CHILDREN: usize = 1000;
/// Deepest indentation of the text outline; deeper shapes sit at this depth.
const MAX_DEPTH: usize = 32;

/// How much of the document a description carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Detail {
    Structure,
    Layout,
    Full,
}

impl Detail {
    fn name(self) -> &'static str {
        match self {
            Detail::Structure => "structure",
            Detail::Layout => "layout",
            Detail::Full => "full",
        }
    }

    fn parse(v: &Value) -> Result<Self, EngineError> {
        match opt_str(v, "detail")? {
            None => Ok(Detail::Structure),
            Some("structure") => Ok(Detail::Structure),
            Some("layout") => Ok(Detail::Layout),
            Some("full") => Ok(Detail::Full),
            Some(other) => Err(EngineError::Params(format!("`detail` must be structure, layout or full, not `{}`", short(other)))),
        }
    }
}

/// The result format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Json,
    Text,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Text => "text",
        }
    }

    fn parse(v: &Value) -> Result<Self, EngineError> {
        match opt_str(v, "format")? {
            None | Some("json") => Ok(Format::Json),
            Some("text") => Ok(Format::Text),
            Some(other) => Err(EngineError::Params(format!("`format` must be json or text, not `{}`", short(other)))),
        }
    }
}

/// What a description needs from the model: element types, and relationship types and ports,
/// looked up by id.
struct Lookup<'a> {
    elements: HashMap<&'a str, &'a str>,
    relationships: HashMap<&'a str, Rel<'a>>,
}

struct Rel<'a> {
    type_id: &'a str,
    source_port: Option<&'a str>,
    target_port: Option<&'a str>,
}

impl<'a> Lookup<'a> {
    fn new(doc: &'a Document) -> Self {
        Lookup {
            elements: doc.model.elements.iter().map(|e| (e.id.as_str(), e.type_id.as_str())).collect(),
            relationships: doc
                .model
                .relationships
                .iter()
                .map(|r| (r.id.as_str(), Rel { type_id: &r.type_id, source_port: r.source_port.as_deref(), target_port: r.target_port.as_deref() }))
                .collect(),
        }
    }
}

/// A whole number as an integer, so layout positions cost few bytes; anything not finite is 0.
fn int(v: f64) -> Value {
    if v.is_finite() { json!(v.round() as i64) } else { json!(0) }
}

/// A rotation to one decimal place, written as an integer when it is whole.
fn degrees(v: f64) -> Value {
    if !v.is_finite() {
        return json!(0);
    }
    let tenth = (v * 10.0).round() / 10.0;
    if tenth.fract() == 0.0 { json!(tenth as i64) } else { json!(tenth) }
}

/// A finite number as it is, anything else as 0.
fn exact(v: f64) -> Value {
    if v.is_finite() { json!(v) } else { json!(0) }
}

/// Insert the keys of the serialized item that are not already described, under their own names.
fn add_rest(o: &mut Map<String, Value>, item: &Value, skip: &[&str]) {
    if let Some(all) = item.as_object() {
        for (k, v) in all {
            if !skip.contains(&k.as_str()) && !o.contains_key(k) {
                o.insert(k.clone(), v.clone());
            }
        }
    }
}

fn shape_json(sh: &Shape, d: Detail, lk: &Lookup) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(sh.id));
    if d == Detail::Full || !sh.stencil.is_empty() {
        o.insert("stencil".into(), json!(sh.stencil));
    }
    o.insert("kind".into(), serde_json::to_value(sh.kind).unwrap_or(Value::Null));
    if d == Detail::Full || !sh.text.is_empty() {
        o.insert("text".into(), json!(sh.text));
    }
    // A name is set on purpose (`shape.rename`), so every detail level shows it.
    if !sh.name.is_empty() {
        o.insert("name".into(), json!(sh.name));
    }
    if let Some(e) = &sh.element {
        o.insert("element".into(), json!(e));
        if let Some(t) = lk.elements.get(e.as_str()) {
            o.insert("elementType".into(), json!(t));
        }
    }
    if let Some(parent) = &sh.parent {
        o.insert("parent".into(), json!(parent));
    }
    if let Some(c) = &sh.container {
        o.insert("role".into(), serde_json::to_value(c.role).unwrap_or(Value::Null));
    }
    if !sh.compartments.is_empty() {
        // Sections are structure: an agent needs the rows of a class box or a tag, not its bands.
        let bands: Vec<Value> = sh
            .compartments
            .iter()
            .map(|c| {
                let mut b = Map::new();
                b.insert("id".into(), json!(c.id));
                b.insert("kind".into(), serde_json::to_value(c.kind).unwrap_or(Value::Null));
                if let Some(t) = &c.title {
                    b.insert("title".into(), json!(t));
                }
                if !c.text.is_empty() {
                    b.insert("text".into(), json!(c.text));
                }
                if !c.rows.is_empty() {
                    b.insert("rows".into(), json!(c.rows));
                }
                Value::Object(b)
            })
            .collect();
        o.insert("compartments".into(), Value::Array(bands));
    }
    if !sh.data.rows.is_empty() {
        let rows: Vec<Value> = if d == Detail::Full {
            sh.data.rows.iter().map(|r| serde_json::to_value(r).unwrap_or(Value::Null)).collect()
        } else {
            sh.data.rows.iter().map(|r| json!({"key": r.key, "value": r.value})).collect()
        };
        o.insert("data".into(), Value::Array(rows));
    }
    if d >= Detail::Layout {
        let num = |v: f64| if d == Detail::Full { exact(v) } else { int(v) };
        o.insert("x".into(), num(sh.x));
        o.insert("y".into(), num(sh.y));
        o.insert("w".into(), num(sh.w));
        o.insert("h".into(), num(sh.h));
        if d == Detail::Full || sh.angle_deg != 0.0 {
            o.insert("rotation".into(), degrees(sh.angle_deg));
        }
    }
    if d == Detail::Full {
        o.insert("connectionPoints".into(), serde_json::to_value(sh.connection_points()).unwrap_or(Value::Null));
        if sh.data.page_keys {
            o.insert("dataPageKeys".into(), json!(true));
        }
        let all = serde_json::to_value(sh).unwrap_or(Value::Null);
        add_rest(&mut o, &all, &["angleDeg", "data", "compartments"]);
    }
    Value::Object(o)
}

fn end_json(e: &End, port: Option<&str>, d: Detail) -> Value {
    let mut o = Map::new();
    match e {
        End::Glued { shape, point, port: glue_port } => {
            o.insert("shape".into(), json!(shape));
            if let Some(i) = point {
                o.insert("point".into(), json!(i));
            }
            // A compartment port on the glue names the band; the relationship's port is the
            // model's name for the same thing when there is no glue port.
            if let Some(port) = glue_port.as_deref().or(port) {
                o.insert("port".into(), json!(port));
            }
        }
        End::Free { x, y } => {
            o.insert("free".into(), json!(true));
            if d >= Detail::Layout {
                o.insert("x".into(), if d == Detail::Full { exact(*x) } else { int(*x) });
                o.insert("y".into(), if d == Detail::Full { exact(*y) } else { int(*y) });
            }
        }
    }
    Value::Object(o)
}

fn connector_json(c: &Connector, d: Detail, lk: &Lookup) -> Value {
    let rel = c.relationship.as_deref().and_then(|id| lk.relationships.get(id));
    let mut o = Map::new();
    o.insert("id".into(), json!(c.id));
    o.insert("from".into(), end_json(&c.start, rel.and_then(|r| r.source_port), d));
    o.insert("to".into(), end_json(&c.end, rel.and_then(|r| r.target_port), d));
    if d == Detail::Full || !c.text.is_empty() {
        o.insert("text".into(), json!(c.text));
    }
    if d == Detail::Full || !c.kind.is_empty() {
        o.insert("kind".into(), json!(c.kind));
    }
    if let Some(id) = &c.relationship {
        o.insert("relationship".into(), json!(id));
        if let Some(r) = rel {
            o.insert("relationshipType".into(), json!(r.type_id));
        }
    }
    if d >= Detail::Layout {
        o.insert("route".into(), serde_json::to_value(c.route).unwrap_or(Value::Null));
    }
    if d == Detail::Full {
        let all = serde_json::to_value(c).unwrap_or(Value::Null);
        add_rest(&mut o, &all, &["start", "end"]);
    }
    Value::Object(o)
}

/// A page described at `d`. `budget` counts the items still allowed in this call; what does not
/// fit is left out and named in `truncated`.
fn page_json(page: &Page, d: Detail, lk: &Lookup, budget: &mut usize) -> Value {
    let mut o = Map::new();
    if d == Detail::Full {
        if let Some(all) = serde_json::to_value(page).ok().and_then(|v| v.as_object().cloned()) {
            for (k, v) in all {
                if k != "shapes" && k != "connectors" {
                    o.insert(k, v);
                }
            }
        }
    } else if d == Detail::Layout {
        // The frame, not the stored sheet: on an infinite board the sheet means nothing.
        let f = page.frame();
        o.insert("board".into(), json!(page.board));
        o.insert("frame".into(), json!({"x": int(f.x0), "y": int(f.y0), "w": int(f.width()), "h": int(f.height())}));
    }
    o.insert("id".into(), json!(page.id));
    o.insert("name".into(), json!(page.name));
    let take_s = page.shapes.len().min(*budget);
    *budget = budget.saturating_sub(take_s);
    let take_c = page.connectors.len().min(*budget);
    *budget = budget.saturating_sub(take_c);
    o.insert("shapes".into(), Value::Array(page.shapes.iter().take(take_s).map(|s| shape_json(s, d, lk)).collect()));
    o.insert("connectors".into(), Value::Array(page.connectors.iter().take(take_c).map(|c| connector_json(c, d, lk)).collect()));
    let (more_s, more_c) = (page.shapes.len().saturating_sub(take_s), page.connectors.len().saturating_sub(take_c));
    if more_s > 0 || more_c > 0 {
        o.insert("truncated".into(), json!({"shapes": more_s, "connectors": more_c}));
    }
    Value::Object(o)
}

// ---- the text outline ----

/// `t` as a quoted, escaped, shortened string for one line.
fn quoted(t: &str) -> String {
    let mut s: String = t.chars().take(MAX_LINE_TEXT).collect();
    if s.len() < t.len() {
        s.push_str("...");
    }
    serde_json::to_string(&s).unwrap_or_default()
}

/// `t` bare when it is a plain word, else quoted.
fn bare_or_quoted(t: &str) -> String {
    if !t.is_empty() && t.chars().count() <= 60 && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/' | '%' | '#')) {
        t.to_string()
    } else {
        quoted(t)
    }
}

fn text_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

/// The keys the outline writes itself; any other key of a `full` item is appended as JSON.
const WRITTEN: &[&str] = &[
    "id",
    "stencil",
    "kind",
    "text",
    "name",
    "element",
    "elementType",
    "parent",
    "role",
    "compartments",
    "data",
    "x",
    "y",
    "w",
    "h",
    "rotation",
    "from",
    "to",
    "route",
    "relationship",
    "relationshipType",
];
// `kind` on a connector is written by `connector_line`; on a shape it is the outline kind.

/// ` +{...}`: the keys of a `full` item that the outline does not write, as compact JSON.
fn extras(item: &Value, skip: &[&str]) -> String {
    let rest: Map<String, Value> = item
        .as_object()
        .map(|o| {
            o.iter().filter(|(k, _)| !WRITTEN.contains(&k.as_str()) && !skip.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
        })
        .unwrap_or_default();
    if rest.is_empty() { String::new() } else { format!(" +{}", Value::Object(rest)) }
}

fn num_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => n.to_string(),
        _ => "?".into(),
    }
}

fn shape_line(s: &Value, d: Detail, indent: usize) -> String {
    let mut line = format!("{}{}", "  ".repeat(indent), text_of(s, "id"));
    let stencil = text_of(s, "stencil");
    line.push(' ');
    line.push_str(if stencil.is_empty() { text_of(s, "kind") } else { stencil });
    let text = text_of(s, "text");
    if !text.is_empty() {
        line.push(' ');
        line.push_str(&quoted(text));
    }
    let name = text_of(s, "name");
    if !name.is_empty() {
        line.push_str(&format!(" name {}", quoted(name)));
    }
    if let Some(e) = s.get("element").and_then(Value::as_str) {
        line.push_str(&format!(" element {e}"));
        if let Some(t) = s.get("elementType").and_then(Value::as_str) {
            line.push_str(&format!(" ({t})"));
        }
    }
    if let Some(r) = s.get("role").and_then(Value::as_str) {
        line.push_str(&format!(" role {r}"));
    }
    if let Some(rows) = s.get("data").and_then(Value::as_array) {
        let cells: Vec<String> =
            rows.iter().map(|r| format!("{}={}", bare_or_quoted(text_of(r, "key")), bare_or_quoted(text_of(r, "value")))).collect();
        line.push_str(&format!(" {{{}}}", cells.join(", ")));
    }
    if let Some(bands) = s.get("compartments").and_then(Value::as_array) {
        // One `id: text` or `id: [row, row]` per section, in band order.
        let parts: Vec<String> = bands
            .iter()
            .map(|b| {
                let body = match b.get("rows").and_then(Value::as_array) {
                    Some(rows) => format!("[{}]", rows.iter().filter_map(Value::as_str).map(quoted).collect::<Vec<_>>().join(", ")),
                    None => quoted(text_of(b, "text")),
                };
                format!("{}: {body}", text_of(b, "id"))
            })
            .collect();
        line.push_str(&format!(" sections {{{}}}", parts.join("; ")));
    }
    if d >= Detail::Layout {
        line.push_str(&format!(" @ {},{} {}x{}", num_text(s.get("x")), num_text(s.get("y")), num_text(s.get("w")), num_text(s.get("h"))));
        if let Some(r) = s.get("rotation").filter(|r| r.as_f64().is_some_and(|f| f != 0.0)) {
            line.push_str(&format!(" rot {r}"));
        }
    }
    if d == Detail::Full {
        line.push_str(&extras(s, &["parent"]));
    }
    line
}

fn end_text(e: Option<&Value>) -> String {
    let Some(e) = e else { return "?".into() };
    match e.get("shape").and_then(Value::as_str) {
        Some(shape) => {
            let mut t = shape.to_string();
            if let Some(port) = e.get("port").and_then(Value::as_str) {
                t.push_str(&format!(":{port}"));
            }
            if let Some(i) = e.get("point").and_then(Value::as_u64) {
                t.push_str(&format!("[{i}]"));
            }
            t
        }
        None => match (e.get("x"), e.get("y")) {
            (Some(x), Some(y)) => format!("({x},{y})"),
            _ => "free".into(),
        },
    }
}

fn connector_line(c: &Value, d: Detail, indent: usize) -> String {
    let mut line = format!("{}{} {} -> {}", "  ".repeat(indent), text_of(c, "id"), end_text(c.get("from")), end_text(c.get("to")));
    if let Some(t) = c.get("relationshipType").and_then(Value::as_str) {
        line.push_str(&format!(" ({t})"));
    } else if let Some(r) = c.get("relationship").and_then(Value::as_str) {
        line.push_str(&format!(" ({r})"));
    }
    let text = text_of(c, "text");
    if !text.is_empty() {
        line.push(' ');
        line.push_str(&quoted(text));
    }
    let kind = text_of(c, "kind");
    if !kind.is_empty() {
        line.push_str(&format!(" kind {kind}"));
    }
    if d >= Detail::Layout {
        line.push_str(&format!(" [{}]", text_of(c, "route")));
    }
    if d == Detail::Full {
        line.push_str(&extras(c, &[]));
    }
    line
}

/// The shapes as outline lines: a shape's members sit one level deeper, directly under it. A
/// `parent` that names no shape on the page, or a loop of parents, cannot hide a shape.
fn shape_lines(shapes: &[Value], d: Detail, base: usize, out: &mut Vec<String>) {
    let ids: HashSet<&str> = shapes.iter().map(|s| text_of(s, "id")).collect();
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, s) in shapes.iter().enumerate() {
        match s.get("parent").and_then(Value::as_str) {
            Some(p) if ids.contains(p) && p != text_of(s, "id") => children.entry(p).or_default().push(i),
            _ => roots.push(i),
        }
    }
    let mut seen = vec![false; shapes.len()];
    // Roots first; whatever is left is in a parent loop and starts its own tree.
    for start in roots.into_iter().chain(0..shapes.len()) {
        if seen.get(start).copied().unwrap_or(true) {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        while let Some((i, depth)) = stack.pop() {
            let Some(done) = seen.get_mut(i) else { continue };
            if *done {
                continue;
            }
            *done = true;
            let Some(s) = shapes.get(i) else { continue };
            out.push(shape_line(s, d, base + depth.min(MAX_DEPTH)));
            if let Some(kids) = children.get(text_of(s, "id")) {
                stack.extend(kids.iter().rev().map(|k| (*k, depth + 1)));
            }
        }
    }
}

fn page_lines(page: &Page, described: &Value, d: Detail, out: &mut Vec<String>) {
    let mut head = format!("page {} {} ({} shapes, {} connectors)", page.id, quoted(&page.name), page.shapes.len(), page.connectors.len());
    if d >= Detail::Layout {
        let frame = described.get("frame");
        head.push_str(&format!(
            " {} {}x{}",
            described.get("board").and_then(Value::as_str).unwrap_or("fixed"),
            num_text(frame.and_then(|f| f.get("w"))),
            num_text(frame.and_then(|f| f.get("h")))
        ));
    }
    if d == Detail::Full {
        let skip = ["shapes", "connectors", "id", "name", "width", "height", "board", "frame", "truncated"];
        let rest: Map<String, Value> = described
            .as_object()
            .map(|o| o.iter().filter(|(k, _)| !skip.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        if !rest.is_empty() {
            head.push_str(&format!(" +{}", Value::Object(rest)));
        }
    }
    out.push(head);
    let items = |k: &str| described.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    shape_lines(&items("shapes"), d, 1, out);
    for c in items("connectors") {
        out.push(connector_line(&c, d, 1));
    }
    if let Some(t) = described.get("truncated") {
        let n = |k: &str| t.get(k).and_then(Value::as_u64).unwrap_or(0);
        out.push(format!("  ... {} more shapes and {} more connectors not shown", n("shapes"), n("connectors")));
    }
}

// ---- the commands ----

/// A page param as text: a string, or a number meaning an index.
fn ref_text(v: &Value) -> Result<String, EngineError> {
    match v {
        Value::String(t) => Ok(t.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(EngineError::Params("a page is an id, name or index".into())),
    }
}

fn find_page(doc: &Document, r: &str) -> Result<usize, EngineError> {
    doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r))))
}

fn doc_describe(s: &mut Session, v: &Value) -> CmdResult {
    let (d, f) = (Detail::parse(v)?, Format::parse(v)?);
    let doc = &s.doc;
    let indexes: Vec<usize> = match v.get("pages") {
        None | Some(Value::Null) => (0..doc.pages.len()).collect(),
        Some(Value::Array(refs)) if refs.len() > MAX_PAGES => {
            return Err(EngineError::Params(format!("`pages` has more than {MAX_PAGES} entries")));
        }
        Some(Value::Array(refs)) if refs.is_empty() => (0..doc.pages.len()).collect(),
        Some(Value::Array(refs)) => {
            let mut out: Vec<usize> = Vec::new();
            for r in refs {
                let i = find_page(doc, &ref_text(r)?)?;
                if !out.contains(&i) {
                    out.push(i);
                }
            }
            out
        }
        Some(_) => return Err(EngineError::Params("`pages` must be an array of page ids, names or indexes".into())),
    };
    let lk = Lookup::new(doc);
    let mut budget = MAX_ITEMS;
    let described: Vec<(&Page, Value)> =
        indexes.iter().filter_map(|i| doc.pages.get(*i)).map(|pg| (pg, page_json(pg, d, &lk, &mut budget))).collect();
    if f == Format::Text {
        let mut lines = Vec::new();
        for (pg, j) in &described {
            page_lines(pg, j, d, &mut lines);
        }
        return Ok(json!({"detail": d.name(), "format": f.name(), "text": lines.join("\n")}));
    }
    let pages: Vec<Value> = described.into_iter().map(|(_, j)| j).collect();
    Ok(json!({"detail": d.name(), "format": f.name(), "pages": pages}))
}

fn page_describe(s: &mut Session, v: &Value) -> CmdResult {
    let (d, f) = (Detail::parse(v)?, Format::parse(v)?);
    let doc = &s.doc;
    let index = match v.get("page") {
        None | Some(Value::Null) => doc.active_index(),
        Some(r) => find_page(doc, &ref_text(r)?)?,
    };
    let page = doc.pages.get(index).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let lk = Lookup::new(doc);
    let mut budget = MAX_ITEMS;
    let described = page_json(page, d, &lk, &mut budget);
    if f == Format::Text {
        let mut lines = Vec::new();
        page_lines(page, &described, d, &mut lines);
        return Ok(json!({"detail": d.name(), "format": f.name(), "page": page.id, "text": lines.join("\n")}));
    }
    Ok(json!({"detail": d.name(), "format": f.name(), "page": described}))
}

fn shape_describe(s: &mut Session, v: &Value) -> CmdResult {
    let (d, f) = (Detail::parse(v)?, Format::parse(v)?);
    let id = p::req_str(v, "id")?;
    let doc = &s.doc;
    let pages: Vec<&Page> = match v.get("page") {
        None | Some(Value::Null) => doc.pages.iter().collect(),
        Some(r) => doc.pages.get(find_page(doc, &ref_text(r)?)?).into_iter().collect(),
    };
    // The wording matches `shape.get`, which the MCP resources look for.
    let (page, sh) =
        pages.into_iter().find_map(|pg| pg.shape(id).map(|sh| (pg, sh))).ok_or_else(|| EngineError::Params(format!("no shape `{}`", short(id))))?;
    let lk = Lookup::new(doc);
    let shape = shape_json(sh, d, &lk);
    let mut connections = Vec::new();
    for c in page.connectors.iter().filter(|c| c.start.shape() == Some(id) || c.end.shape() == Some(id)).take(MAX_ITEMS) {
        let end = match (c.start.shape() == Some(id), c.end.shape() == Some(id)) {
            (true, true) => "both",
            (true, false) => "start",
            _ => "end",
        };
        let mut j = connector_json(c, d, &lk);
        if let Some(o) = j.as_object_mut() {
            o.insert("end".into(), json!(end));
        }
        connections.push(j);
    }
    let children: Vec<&str> = page.shapes.iter().filter(|c| c.parent.as_deref() == Some(id)).map(|c| c.id.as_str()).take(MAX_CHILDREN).collect();
    if f == Format::Text {
        let mut lines = vec![shape_line(&shape, d, 0)];
        if !children.is_empty() {
            lines.push(format!("  children {}", children.join(" ")));
        }
        lines.extend(connections.iter().map(|c| connector_line(c, d, 1)));
        return Ok(json!({"detail": d.name(), "format": f.name(), "page": page.id, "text": lines.join("\n")}));
    }
    let mut out = json!({"detail": d.name(), "format": f.name(), "page": page.id, "shape": shape, "connections": connections});
    if let (Some(o), false) = (out.as_object_mut(), children.is_empty()) {
        o.insert("children".into(), json!(children));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_model::Element;

    use crate::{Session, sample};

    fn session() -> Session {
        Session::new(sample::sample_document())
    }

    /// Every object key anywhere in `v`.
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

    const COLOUR_KEYS: &[&str] = &["fill", "line", "font", "style", "shadow", "gradient", "glow", "dash", "fontSize", "fontColor", "lineWidth"];

    #[test]
    fn structure_of_the_sample_is_small_and_has_no_style() {
        let mut s = session();
        let r = s.run("doc.describe", &json!({})).unwrap();
        assert_eq!((r["detail"].as_str(), r["format"].as_str()), (Some("structure"), Some("json")));
        let compact = serde_json::to_string(&r).unwrap();
        assert!(compact.len() < 4096, "{} bytes", compact.len());
        assert!(serde_json::to_string_pretty(&r).unwrap().len() < 4096, "pretty form is over budget too");
        let mut found = Vec::new();
        keys(&r, &mut found);
        for k in COLOUR_KEYS {
            assert!(!found.iter().any(|f| f == k), "structure has the key {k}");
        }
        for k in ["x", "y", "w", "h", "waypoints", "connectionPoints", "ext"] {
            assert!(!found.iter().any(|f| f == k), "structure has the key {k}");
        }
        let page = &r["pages"][0];
        assert_eq!((page["shapes"].as_array().map(Vec::len), page["connectors"].as_array().map(Vec::len)), (Some(6), Some(6)));
        let c = &page["connectors"][2];
        assert_eq!(c["text"], "Yes");
        assert_eq!(c["from"]["point"], 2);
        assert_eq!(page["shapes"][2]["stencil"], "flowchart.decision");
        assert_eq!(page["shapes"][2]["id"], "s4");
        // The description names the same ids the document has.
        let ids: Vec<&str> = page["shapes"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        let doc_ids: Vec<&str> = s.doc.page().unwrap().shapes.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, doc_ids);
    }

    #[test]
    fn a_shape_name_shows_at_structure_detail() {
        let mut s = session();
        s.run("shape.rename", &json!({"id": "s4", "name": "Gate"})).unwrap();
        let r = s.run("doc.describe", &json!({})).unwrap();
        assert_eq!(r["pages"][0]["shapes"][2]["name"], "Gate");
        assert!(r["pages"][0]["shapes"][0].get("name").is_none(), "an unnamed shape has no name key");
        let t = s.run("doc.describe", &json!({"format": "text"})).unwrap();
        assert!(t["text"].as_str().unwrap().contains("name \"Gate\""), "{}", t["text"]);
    }

    #[test]
    fn structure_lists_compartments_connector_kinds_and_ports() {
        let mut s = session();
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 600, "y": 40, "text": "Tank", "id": "tank"})).unwrap();
        s.run(
            "compartment.add",
            &json!({"id": "tank", "kind": "list", "title": "Fields", "rows": ["tag: T-101", "volume: 40 m3"], "compartmentId": "fields"}),
        )
        .unwrap();
        let first = s.doc.page().unwrap().shapes[0].id.clone();
        s.run("connector.add", &json!({"from": {"shape": first}, "to": {"shape": "tank", "port": "fields.left"}, "kind": "basic.bus", "id": "feed"}))
            .unwrap();
        let v = s.run("page.describe", &json!({})).unwrap();
        let tank = v["page"]["shapes"].as_array().unwrap().iter().find(|sh| sh["id"] == "tank").unwrap().clone();
        assert_eq!(tank["compartments"][0]["id"], "fields");
        assert_eq!(tank["compartments"][0]["rows"][1], "volume: 40 m3");
        assert!(tank["compartments"][0].get("style").is_none(), "no style at structure: {tank}");
        let feed = v["page"]["connectors"].as_array().unwrap().iter().find(|c| c["id"] == "feed").unwrap().clone();
        assert_eq!((feed["kind"].as_str(), feed["to"]["port"].as_str()), (Some("basic.bus"), Some("fields.left")));
        let text = s.run("page.describe", &json!({"format": "text"})).unwrap();
        let text = text["text"].as_str().unwrap();
        assert!(text.contains("sections {fields: [\"tag: T-101\", \"volume: 40 m3\"]}"), "{text}");
        assert!(text.contains("kind basic.bus"), "{text}");
    }

    #[test]
    fn the_ladder_adds_geometry_then_everything() {
        let mut s = session();
        let layout = s.run("page.describe", &json!({"detail": "layout"})).unwrap();
        let sh = &layout["page"]["shapes"][0];
        for k in ["x", "y", "w", "h"] {
            assert!(sh[k].is_i64() || sh[k].is_u64(), "{k} is a whole number: {sh}");
        }
        assert!(sh.get("rotation").is_none(), "no rotation when it is zero");
        assert!(sh.get("style").is_none());
        assert_eq!(layout["page"]["board"], "infinite", "a new document is an infinite board");
        assert!(
            layout["page"]["frame"]["w"].is_number() && layout["page"].get("width").is_none(),
            "layout reports the frame, not the sheet: {}",
            layout["page"]
        );
        assert_eq!(layout["page"]["connectors"][0]["route"], "orthogonal");

        let full = s.run("page.describe", &json!({"detail": "full"})).unwrap();
        let sh = &full["page"]["shapes"][0];
        assert!(sh["style"].is_object() && sh["connectionPoints"].is_array(), "{sh}");
        assert!(sh.get("rotation").is_some());
        let c = &full["page"]["connectors"][0];
        assert!(c["style"].is_object() && c.get("endArrow").is_some(), "{c}");
        assert_eq!(full["page"]["width"], 1100.0);
        // Everything the document stores for the shape is there.
        let stored = serde_json::to_value(s.doc.page().unwrap().shapes.first().unwrap()).unwrap();
        for k in stored.as_object().unwrap().keys() {
            let described_as = if k == "angleDeg" { "rotation" } else { k };
            assert!(sh.get(described_as).is_some(), "full is missing {k}");
        }
        let mut found = Vec::new();
        keys(&full, &mut found);
        assert!(found.iter().any(|f| f == "style"));
    }

    #[test]
    fn rotation_shows_in_layout_when_not_zero() {
        let mut s = session();
        s.run("shape.rotate", &json!({"ids": ["s2"], "angle": 30.0})).unwrap();
        let r = s.run("shape.describe", &json!({"id": "s2", "detail": "layout"})).unwrap();
        assert_eq!(r["shape"]["rotation"], 30);
        assert_eq!(r["page"], s.doc.page().unwrap().id.as_str());
    }

    #[test]
    fn text_outline_of_the_sample() {
        let mut s = session();
        let r = s.run("doc.describe", &json!({"format": "text"})).unwrap();
        assert_eq!(r["format"], "text");
        let text = r["text"].as_str().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let page_id = s.doc.page().unwrap().id.clone();
        assert_eq!(lines[0], format!("page {page_id} \"Page-1\" (6 shapes, 6 connectors)"));
        assert_eq!(lines[1], "  s2 flowchart.terminator \"Start\"");
        assert!(lines.contains(&"  s4 flowchart.decision \"Valid?\""), "{text}");
        assert!(lines.contains(&"  c10 s4[2] -> s5[0] \"Yes\""), "{text}");
        assert_eq!(lines.len(), 13);
        let json_len = serde_json::to_string(&s.run("doc.describe", &json!({})).unwrap()).unwrap().len();
        assert!(text.len() * 2 < json_len, "text {} bytes, json {json_len} bytes", text.len());

        let layout = s.run("doc.describe", &json!({"format": "text", "detail": "layout"})).unwrap();
        let layout = layout["text"].as_str().unwrap();
        assert!(layout.lines().nth(1).unwrap().contains(" @ "), "{layout}");
        let head = layout.lines().next().unwrap();
        let size = head.rsplit(' ').next().unwrap();
        assert!(head.contains(" infinite ") || head.contains(" fixed "), "the header names the board: {head}");
        assert!(
            size.split_once('x').is_some_and(|(w, h)| w.parse::<i64>().is_ok() && h.parse::<i64>().is_ok()),
            "the header ends with the frame size: {head}"
        );
        assert!(layout.contains("[orthogonal]"), "{layout}");

        let full = s.run("doc.describe", &json!({"format": "text", "detail": "full"})).unwrap();
        assert!(full["text"].as_str().unwrap().contains("\"style\""));
    }

    #[test]
    fn containment_elements_and_data_show_in_the_outline() {
        let mut s = session();
        {
            let page = s.doc.page_mut().unwrap();
            // s6 holds s3 and s7; the shapes s4 and s5 name each other as parents; s2 names itself;
            // s6 itself names a parent that is not on the page.
            page.shapes[1].parent = Some("s6".into());
            page.shapes[5].parent = Some("s6".into());
            page.shapes[4].parent = Some("gone".into());
            page.shapes[2].element = Some("e1".into());
            page.shapes[2].data.rows.push(vizcraft_diagram::data::DataRow { key: "Owner".into(), value: "Ops team".into(), ..Default::default() });
            page.shapes[0].parent = Some("s2".into());
            page.shapes[2].parent = Some("s5".into());
            page.shapes[3].parent = Some("s4".into());
        }
        s.doc.model.elements.push(Element::new("e1", "flowchart.Decision"));
        let r = s.run("page.describe", &json!({"format": "text"})).unwrap();
        let text = r["text"].as_str().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.contains(&"  s6 flowchart.terminator \"End\""), "{text}");
        assert!(lines.contains(&"    s3 flowchart.process \"Gather input\""), "{text}");
        assert!(lines.contains(&"    s7 flowchart.process \"Fix input\""), "{text}");
        assert!(text.contains("element e1 (flowchart.Decision) {Owner=\"Ops team\"}"), "{text}");
        for id in ["s2 ", "s3 ", "s4 ", "s5 ", "s6 ", "s7 "] {
            assert_eq!(lines.iter().filter(|l| l.trim_start().starts_with(id)).count(), 1, "{id}: {text}");
        }
        let j = s.run("page.describe", &json!({})).unwrap();
        let shape = &j["page"]["shapes"][2];
        assert_eq!(
            (shape["element"].as_str(), shape["elementType"].as_str(), shape["parent"].as_str()),
            (Some("e1"), Some("flowchart.Decision"), Some("s5"))
        );
        assert_eq!(shape["data"], json!([{"key": "Owner", "value": "Ops team"}]));
        let one = s.run("shape.describe", &json!({"id": "s6"})).unwrap();
        assert_eq!(one["children"], json!(["s3", "s7"]));
    }

    #[test]
    fn shape_describe_lists_glued_connectors_like_shape_get() {
        let mut s = session();
        let r = s.run("shape.describe", &json!({"id": "s4"})).unwrap();
        assert_eq!(r["shape"]["id"], "s4");
        let got = s.run("shape.get", &json!({"id": "s4"})).unwrap();
        let described: Vec<(&str, &str)> =
            r["connections"].as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap(), c["end"].as_str().unwrap())).collect();
        let expected: Vec<(&str, &str)> =
            got["connections"].as_array().unwrap().iter().map(|c| (c["connector"].as_str().unwrap(), c["end"].as_str().unwrap())).collect();
        assert_eq!(described, expected);
        let t = s.run("shape.describe", &json!({"id": "s4", "format": "text"})).unwrap();
        let t = t["text"].as_str().unwrap();
        assert!(t.starts_with("s4 flowchart.decision \"Valid?\"\n  c9 s3[2] -> s4[0]"), "{t}");
    }

    #[test]
    fn pages_param_picks_pages_and_junk_is_refused() {
        let mut s = session();
        s.run("page.add", &json!({"name": "Second"})).unwrap();
        let both = s.run("doc.describe", &json!({})).unwrap();
        assert_eq!(both["pages"].as_array().unwrap().len(), 2);
        let one = s.run("doc.describe", &json!({"pages": ["Second", 1, "Second"]})).unwrap();
        assert_eq!(one["pages"].as_array().unwrap().len(), 1);
        assert_eq!(one["pages"][0]["name"], "Second");
        assert_eq!(s.run("doc.describe", &json!({"pages": []})).unwrap()["pages"].as_array().unwrap().len(), 2);
        for bad in [
            json!({"detail": "colourful"}),
            json!({"detail": 3}),
            json!({"format": "xml"}),
            json!({"pages": "Second"}),
            json!({"pages": [true]}),
            json!({"pages": ["nope"]}),
            json!({"pages": vec![0; 2000]}),
        ] {
            assert!(s.run("doc.describe", &bad).is_err(), "{bad}");
        }
        assert!(s.run("page.describe", &json!({"page": "nope"})).is_err());
        assert!(s.run("page.describe", &json!({"page": {}})).is_err());
        assert!(s.run("shape.describe", &json!({})).is_err());
        assert!(s.run("shape.describe", &json!({"id": "nope"})).unwrap_err().to_string().contains("no shape `nope`"));
        assert!(s.run("shape.describe", &json!({"id": "s2", "page": "Second"})).is_err());
    }

    #[test]
    fn descriptions_do_not_change_the_document() {
        let mut s = session();
        let before = serde_json::to_string(&s.doc).unwrap();
        let rev = s.rev();
        for (id, p) in
            [("doc.describe", json!({"detail": "full"})), ("page.describe", json!({"format": "text"})), ("shape.describe", json!({"id": "s3"}))]
        {
            s.run(id, &p).unwrap();
        }
        assert_eq!(serde_json::to_string(&s.doc).unwrap(), before);
        assert_eq!(s.rev(), rev);
    }

    #[test]
    fn long_text_is_clipped_in_the_outline_only() {
        let mut s = session();
        s.doc.page_mut().unwrap().shapes[0].text = "x\ny".repeat(500);
        let t = s.run("page.describe", &json!({"format": "text"})).unwrap();
        let line = t["text"].as_str().unwrap().lines().nth(1).unwrap().to_string();
        assert!(line.len() < 300 && line.ends_with("...\""), "{line}");
        let j = s.run("page.describe", &json!({})).unwrap();
        assert_eq!(j["page"]["shapes"][0]["text"].as_str().unwrap().len(), 1500);
    }
}
