//! External data commands (`datalink.*`): link shape data to a CSV or XLSX table, refresh it,
//! bind a shape to a row and unlink.
//!
//! A link names the table's key column and the shape data key that holds the same value on
//! shapes. A refresh rereads the file and, for each row, writes every linked column into the
//! shape data of every shape (on any page) whose key value equals the row's, defining missing
//! page keys with the column's inferred type; it is one command, so one undo step. The file is
//! read with a size cap, and only a regular file is read (not a directory or a device).

use std::collections::{HashMap, HashSet};
use std::io::Read as _;

use serde_json::{Value, json};
use vizcraft_diagram::data::{self, DataKey, DataRow, DataType, MAX_ROWS};
use vizcraft_diagram::data_graphic::{DataLink, MAX_COLUMNS, MAX_LINKS, MAX_NAME, MAX_PATH, TableFormat};
use vizcraft_diagram::{Document, Shape};
use vizcraft_formats::table;
pub use vizcraft_formats::table::{Column, Table};

use super::{now_iso, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Data > External Data";

/// The external data commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("datalink.link", "Link Data to Shapes", LOCATION, link).params(
            r#"{"path": file, "format"?: "csv|xlsx" (default: from the extension), "sheet"?: string (default: the first), "keyColumn": column name, "dataKey"?: shape data key (default: the key column's name), "columns"?: [column name] (default: all)} -> {id, columns: [{name, type}], rows, linked, unmatched, warnings}; links and refreshes"#,
        ),
        CommandSpec::new("datalink.list", "External Data Links", LOCATION, list).params(r#"{} -> {links: [{id, path, format, sheet, keyColumn, dataKey, columns, lastRefresh}]}"#),
        CommandSpec::new("datalink.refresh", "Refresh All", LOCATION, refresh).params(
            r#"{"id"?: link id (default: every link)} -> {links: [{id, linked, unmatched, error?}], linked, unmatched}; rereads the files and updates shape data in one undo step"#,
        ),
        CommandSpec::new("datalink.bind", "Link to Selection", LOCATION, bind).params(
            r#"{"id": link id, "row": key value (string) or row index (number), "shape": shape id} -> {shape, key, linked, unmatched}; sets the shape's key and refreshes the link"#,
        ),
        CommandSpec::new("datalink.unlink", "Unlink", LOCATION, unlink).params(r#"{"id": link id} -> {id}; shape data stays as it is"#),
    ]
}

fn params(msg: impl Into<String>) -> EngineError {
    EngineError::Params(msg.into())
}

/// Read a table file: a regular file of at most `table::MAX_BYTES`, read through a capped reader
/// so a file that grows while it is read cannot exhaust memory.
pub fn read_table(path: &str, format: TableFormat, sheet: &str) -> Result<Table, EngineError> {
    if path.trim().is_empty() || path.len() > MAX_PATH || path.contains('\0') {
        return Err(params("`path` must name a file"));
    }
    let fail = |e: String| EngineError::Failed(format!("{}: {e}", short(path)));
    let meta = std::fs::metadata(path).map_err(|e| fail(e.to_string()))?;
    if !meta.is_file() {
        return Err(fail("not a regular file".into()));
    }
    let max = table::MAX_BYTES as u64;
    if meta.len() > max {
        return Err(fail(format!("larger than {} MiB", max / (1024 * 1024))));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|f| f.take(max + 1).read_to_end(&mut bytes)).map_err(|e| fail(e.to_string()))?;
    table::read(&bytes, format, sheet).map_err(|e| fail(e.to_string()))
}

/// The table a link reads.
pub fn link_table(l: &DataLink) -> Result<Table, EngineError> {
    read_table(&l.path, l.format, &l.sheet)
}

/// The shape data key a column writes: its name with characters a key cannot hold replaced by
/// `_`, cut to the longest key; `None` when nothing usable is left.
pub fn column_key(name: &str) -> Option<String> {
    let mut k: String = name.trim().chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ') { c } else { '_' }).collect();
    let mut cut = k.len().min(data::MAX_KEY);
    while cut > 0 && !k.is_char_boundary(cut) {
        cut -= 1;
    }
    k.truncate(cut);
    let k = k.trim().to_string();
    data::is_key(&k).then_some(k)
}

/// The key values of `l`'s data key held by any shape of `doc`, for the External Data pane's
/// linked mark.
pub fn linked_keys(doc: &Document, l: &DataLink) -> HashSet<String> {
    doc.pages
        .iter()
        .flat_map(|p| &p.shapes)
        .filter_map(|s| s.data.row(&l.data_key))
        .map(|r| r.value.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// What a refresh did.
#[derive(Debug, Default, PartialEq)]
struct Outcome {
    /// Shapes whose key matched a row.
    linked: usize,
    /// Rows whose key matched no shape.
    unmatched: usize,
}

/// Set row `key` of `sh` to `text`: an existing row keeps its type when the value fits it; a new
/// row takes the page's definition, else the column's type. A value that fits neither is kept as
/// text. Returns false when the shape has no room for another row.
fn set_value(sh: &mut Shape, defs: &[DataKey], key: &str, label: &str, kind: DataType, text: &str) -> bool {
    if let Some(r) = sh.data.row_mut(key) {
        match data::normalize_text(r.kind, &r.values, &r.unit, text) {
            Ok(v) => r.value = v,
            Err(_) => {
                r.kind = DataType::String;
                r.value = text.to_string();
            }
        }
        return true;
    }
    if sh.data.rows.len() >= MAX_ROWS {
        return false;
    }
    let mut row = match defs.iter().find(|d| d.key == key) {
        Some(d) => d.row(),
        None => DataRow { key: key.to_string(), label: if label == key { String::new() } else { label.to_string() }, kind, ..DataRow::default() },
    };
    match data::normalize_text(row.kind, &row.values, &row.unit, text) {
        Ok(v) => row.value = v,
        Err(_) => {
            row.kind = DataType::String;
            row.value = text.to_string();
        }
    }
    row.sanitize();
    sh.data.rows.push(row);
    true
}

/// Write `t` into the shapes of `doc` as link `l` says.
fn apply_table(doc: &mut Document, l: &DataLink, t: &Table) -> Result<Outcome, EngineError> {
    let kc = t.column(&l.key_column).ok_or_else(|| params(format!("the table has no column `{}`", short(&l.key_column))))?;
    // The columns written, each with its data key.
    let mut cols: Vec<(usize, String)> = Vec::new();
    for (i, c) in t.columns.iter().enumerate() {
        if !l.columns.is_empty() && !l.columns.iter().any(|n| t.column(n) == Some(i)) {
            continue;
        }
        if let Some(k) = column_key(&c.name)
            && !cols.iter().any(|(_, o)| *o == k)
        {
            cols.push((i, k));
        }
    }
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (ri, _) in t.rows.iter().enumerate() {
        let k = t.cell(ri, kc).trim();
        if !k.is_empty() {
            by_key.entry(k.to_string()).or_insert(ri);
        }
    }
    let mut matched_rows: HashSet<usize> = HashSet::new();
    let mut out = Outcome::default();
    for page in &mut doc.pages {
        let mut defs = data::page_keys(page);
        let mut touched = false;
        for si in 0..page.shapes.len() {
            let Some(sh) = page.shapes.get(si) else { continue };
            let Some(&ri) = sh.data.row(&l.data_key).and_then(|r| by_key.get(r.value.trim())) else { continue };
            if vizcraft_diagram::layer::locked(page, sh) {
                continue;
            }
            matched_rows.insert(ri);
            out.linked += 1;
            touched = true;
            let Some(sh) = page.shapes.get_mut(si) else { continue };
            for (ci, key) in &cols {
                let (name, kind) = t.columns.get(*ci).map_or(("", DataType::String), |c| (c.name.as_str(), c.kind));
                set_value(sh, &defs, key, name, kind, t.cell(ri, *ci));
            }
        }
        if !touched {
            continue;
        }
        // Define the columns this page does not know yet, with their inferred types, and give
        // shapes that take page keys their rows.
        let before = defs.len();
        for (ci, key) in &cols {
            if defs.len() >= data::MAX_PAGE_KEYS {
                break;
            }
            if !defs.iter().any(|d| d.key == *key)
                && let Some(c) = t.columns.get(*ci)
            {
                let label = if c.name == *key { String::new() } else { c.name.clone() };
                defs.push(DataKey { key: key.clone(), label, kind: c.kind, ..DataKey::default() });
            }
        }
        if defs.len() != before {
            data::set_page_keys(page, &defs);
            for sh in page.shapes.iter_mut().filter(|s| s.data.page_keys) {
                data::add_page_rows(sh, &defs);
            }
        }
    }
    out.unmatched = by_key.values().filter(|ri| !matched_rows.contains(ri)).count();
    Ok(out)
}

/// Reread link `index` and apply it; stamps the refresh time.
fn refresh_one(s: &mut Session, index: usize) -> Result<Outcome, EngineError> {
    let l = s.doc.data_links.get(index).cloned().ok_or_else(|| EngineError::Failed("the link went away".into()))?;
    let t = link_table(&l)?;
    let out = apply_table(&mut s.doc, &l, &t)?;
    if let Some(l) = s.doc.data_links.get_mut(index) {
        l.last_refresh = now_iso();
    }
    Ok(out)
}

fn next_id(s: &Session) -> String {
    let used: HashSet<&str> = s.doc.data_links.iter().map(|l| l.id.as_str()).collect();
    (1..).map(|n| format!("dl{n}")).find(|id| !used.contains(id.as_str())).unwrap_or_default()
}

/// A list of at most [`MAX_COLUMNS`] column names.
fn columns_param(v: &Value) -> Result<Vec<String>, EngineError> {
    let bad = || params(format!("`columns` must be a list of at most {MAX_COLUMNS} column names"));
    match v.get("columns") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= MAX_COLUMNS => {
            a.iter().map(|c| c.as_str().filter(|c| c.len() <= MAX_NAME).map(|c| c.trim().to_string()).ok_or_else(bad)).collect()
        }
        Some(_) => Err(bad()),
    }
}

fn link(s: &mut Session, v: &Value) -> CmdResult {
    let path = opt_str(v, "path")?.ok_or_else(|| params("`path` (string) is required"))?.to_string();
    let format = match opt_str(v, "format")? {
        Some(f) => TableFormat::from_name(f.trim()).ok_or_else(|| params(format!("`format` must be csv or xlsx, not `{}`", short(f))))?,
        None => TableFormat::for_path(&path),
    };
    let sheet = opt_str(v, "sheet")?.unwrap_or_default().to_string();
    if sheet.len() > MAX_NAME {
        return Err(params(format!("`sheet` is at most {MAX_NAME} bytes")));
    }
    let key_column = opt_str(v, "keyColumn")?.ok_or_else(|| params("`keyColumn` (string) is required"))?.trim().to_string();
    let columns = columns_param(v)?;
    if s.doc.data_links.len() >= MAX_LINKS {
        return Err(params(format!("the document already has {MAX_LINKS} data links")));
    }
    let t = read_table(&path, format, &sheet)?;
    let names = || t.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ");
    let kc = t.column(&key_column).ok_or_else(|| params(format!("no column `{}` (columns: {})", short(&key_column), short(&names()))))?;
    for c in &columns {
        if t.column(c).is_none() {
            return Err(params(format!("no column `{}` (columns: {})", short(c), short(&names()))));
        }
    }
    let data_key = match opt_str(v, "dataKey")? {
        Some(k) if data::is_key(k) => k.to_string(),
        Some(k) => return Err(params(format!("`dataKey` must be a data key, not `{}`", short(k)))),
        None => t
            .columns
            .get(kc)
            .and_then(|c| column_key(&c.name))
            .ok_or_else(|| params("the key column's name is not a usable data key; pass `dataKey`"))?,
    };
    let id = next_id(s);
    let l = DataLink {
        id: id.clone(),
        path,
        format,
        sheet,
        key_column: t.columns.get(kc).map(|c| c.name.clone()).unwrap_or(key_column),
        data_key,
        columns,
        last_refresh: now_iso(),
    };
    let out = apply_table(&mut s.doc, &l, &t)?;
    s.doc.data_links.push(l);
    let cols: Vec<Value> = t.columns.iter().map(|c| json!({"name": c.name, "type": c.kind.name()})).collect();
    Ok(json!({"id": id, "columns": cols, "rows": t.rows.len(), "linked": out.linked, "unmatched": out.unmatched, "warnings": t.warnings}))
}

fn list(s: &mut Session, _: &Value) -> CmdResult {
    Ok(json!({"links": s.doc.data_links}))
}

fn link_index(s: &Session, id: &str) -> Result<usize, EngineError> {
    s.doc.data_links.iter().position(|l| l.id == id).ok_or_else(|| params(format!("no data link `{}`", short(id))))
}

fn refresh(s: &mut Session, v: &Value) -> CmdResult {
    let which: Vec<usize> = match opt_str(v, "id")? {
        Some(id) => vec![link_index(s, id)?],
        None => (0..s.doc.data_links.len()).collect(),
    };
    if which.is_empty() {
        return Err(params("the document has no data links"));
    }
    let single = which.len() == 1;
    let (mut linked, mut unmatched, mut ok) = (0usize, 0usize, 0usize);
    let mut links = Vec::new();
    for i in which {
        let id = s.doc.data_links.get(i).map(|l| l.id.clone()).unwrap_or_default();
        match refresh_one(s, i) {
            Ok(o) => {
                linked += o.linked;
                unmatched += o.unmatched;
                ok += 1;
                links.push(json!({"id": id, "linked": o.linked, "unmatched": o.unmatched}));
            }
            Err(e) if single => return Err(e),
            Err(e) => links.push(json!({"id": id, "error": e.to_string()})),
        }
    }
    if ok == 0 {
        return Err(EngineError::Failed("no data link could be read".into()));
    }
    Ok(json!({"links": links, "linked": linked, "unmatched": unmatched}))
}

fn bind(s: &mut Session, v: &Value) -> CmdResult {
    let id = opt_str(v, "id")?.ok_or_else(|| params("`id` (link id) is required"))?.to_string();
    let index = link_index(s, &id)?;
    let shape = opt_str(v, "shape")?.ok_or_else(|| params("`shape` (shape id) is required"))?.to_string();
    let pi = s.doc.pages.iter().position(|p| p.shape(&shape).is_some()).ok_or_else(|| params(format!("`{}` is not a shape", short(&shape))))?;
    let l = s.doc.data_links.get(index).cloned().ok_or_else(|| EngineError::Failed("the link went away".into()))?;
    let t = link_table(&l)?;
    let kc = t.column(&l.key_column).ok_or_else(|| params(format!("the table has no column `{}`", short(&l.key_column))))?;
    let key = match v.get("row") {
        Some(Value::String(k)) if !k.trim().is_empty() && k.len() <= data::MAX_VALUE => k.trim().to_string(),
        Some(Value::Number(n)) => {
            let ri = n
                .as_u64()
                .and_then(|r| usize::try_from(r).ok())
                .filter(|r| *r < t.rows.len())
                .ok_or_else(|| params(format!("`row` must be below {}", t.rows.len())))?;
            t.cell(ri, kc).trim().to_string()
        }
        _ => return Err(params("`row` (a key value or a row index) is required")),
    };
    if !(0..t.rows.len()).any(|ri| t.cell(ri, kc).trim() == key) {
        return Err(params(format!("no row has `{}` in `{}`", short(&key), short(&l.key_column))));
    }
    let page = s.doc.pages.get_mut(pi).ok_or_else(|| EngineError::Failed("the page went away".into()))?;
    if page.shape(&shape).is_some_and(|sh| vizcraft_diagram::layer::locked(page, sh)) {
        return Err(params(format!("`{}` is on a locked layer", short(&shape))));
    }
    let defs = data::page_keys(page);
    let kind = t.columns.get(kc).map_or(DataType::String, |c| c.kind);
    let sh = page.shape_mut(&shape).ok_or_else(|| EngineError::Failed("the shape went away".into()))?;
    if !set_value(sh, &defs, &l.data_key, &l.key_column, kind, &key) {
        return Err(params(format!("`{}` already has {MAX_ROWS} data rows", short(&shape))));
    }
    let out = apply_table(&mut s.doc, &l, &t)?;
    if let Some(l) = s.doc.data_links.get_mut(index) {
        l.last_refresh = now_iso();
    }
    Ok(json!({"shape": shape, "key": key, "linked": out.linked, "unmatched": out.unmatched}))
}

fn unlink(s: &mut Session, v: &Value) -> CmdResult {
    let id = opt_str(v, "id")?.ok_or_else(|| params("`id` (link id) is required"))?.to_string();
    let index = link_index(s, &id)?;
    s.doc.data_links.remove(index);
    Ok(json!({"id": id}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::Document;

    use crate::Session;

    /// A scratch directory for this test's files.
    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("vizcraft-datalink-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Shapes `a` (Tag A1), `b` (Tag B2) and `c` (no tag).
    fn session() -> Session {
        let mut s = Session::new(Document::new());
        for (id, x) in [("a", 0), ("b", 200), ("c", 400)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "id": id, "x": x, "y": 0, "w": 100, "h": 60})).unwrap();
        }
        s.run("data.set", &json!({"ids": ["a"], "key": "Tag", "value": "A1"})).unwrap();
        s.run("data.set", &json!({"ids": ["b"], "key": "Tag", "value": "B2"})).unwrap();
        s
    }

    fn value(s: &Session, id: &str, key: &str) -> String {
        s.doc.shape(id).unwrap().data.row(key).map(|r| r.value.clone()).unwrap_or_default()
    }

    #[test]
    fn link_sets_rows_on_matching_shapes_and_counts_the_unmatched() {
        let d = dir("link");
        let path = d.join("assets.csv");
        std::fs::write(&path, "Tag,Cost,Owner,Due\nA1,1200,Ana,2026-03-01\nB2,80.5,Bo,2026-04-02\nZ9,5,Zed,2026-05-03\n").unwrap();
        let mut s = session();
        let r = s.run("datalink.link", &json!({"path": path.to_string_lossy(), "keyColumn": "Tag"})).unwrap();
        assert_eq!((r["linked"].as_u64(), r["unmatched"].as_u64(), r["rows"].as_u64()), (Some(2), Some(1), Some(3)), "{r}");
        assert_eq!(value(&s, "a", "Cost"), "1200");
        assert_eq!(value(&s, "b", "Owner"), "Bo");
        assert_eq!(s.doc.shape("b").unwrap().data.row("Due").unwrap().kind, vizcraft_diagram::data::DataType::Date);
        assert!(s.doc.shape("c").unwrap().data.rows.is_empty(), "an untagged shape is left alone");
        let keys = vizcraft_diagram::data::page_keys(s.doc.page().unwrap());
        assert_eq!(
            keys.iter().find(|k| k.key == "Cost").map(|k| k.kind),
            Some(vizcraft_diagram::data::DataType::Number),
            "missing page keys are defined"
        );
        assert_eq!(s.run("datalink.list", &json!({})).unwrap()["links"][0]["keyColumn"], "Tag");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refresh_after_the_file_changes_is_one_undo_step() {
        let d = dir("refresh");
        let path = d.join("assets.csv");
        std::fs::write(&path, "Tag,Cost\nA1,10\nB2,20\n").unwrap();
        let mut s = session();
        s.run("datalink.link", &json!({"path": path.to_string_lossy(), "keyColumn": "Tag"})).unwrap();
        std::fs::write(&path, "Tag,Cost\nA1,11\nB2,22\n").unwrap();
        let r = s.run("datalink.refresh", &json!({})).unwrap();
        assert_eq!(r["linked"], 2);
        assert_eq!((value(&s, "a", "Cost"), value(&s, "b", "Cost")), ("11".to_string(), "22".to_string()));
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!((value(&s, "a", "Cost"), value(&s, "b", "Cost")), ("10".to_string(), "20".to_string()), "one undo takes back the whole refresh");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bind_sets_the_key_and_refreshes() {
        let d = dir("bind");
        let path = d.join("assets.csv");
        std::fs::write(&path, "Tag,Cost\nA1,10\nZ9,99\n").unwrap();
        let mut s = session();
        s.run("datalink.link", &json!({"path": path.to_string_lossy(), "keyColumn": "Tag"})).unwrap();
        let r = s.run("datalink.bind", &json!({"id": "dl1", "row": 1, "shape": "c"})).unwrap();
        assert_eq!(r["key"], "Z9");
        assert_eq!(value(&s, "c", "Cost"), "99");
        assert!(s.run("datalink.bind", &json!({"id": "dl1", "row": "nope", "shape": "c"})).is_err());
        s.run("datalink.unlink", &json!({"id": "dl1"})).unwrap();
        assert!(s.doc.data_links.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn junk_paths_and_params_never_panic() {
        let d = dir("junk");
        let long = "x".repeat(10_000);
        let dir_path = d.to_string_lossy().to_string();
        let paths = ["", "\0", "/", "/dev/zero", "/dev/null", "/nonexistent/none.csv", long.as_str(), dir_path.as_str(), "nul:", "C:\\nope\\x.xlsx"];
        let mut junk: Vec<Value> = paths.iter().map(|p| json!({"path": p, "keyColumn": "Tag", "id": "dl1", "row": 0, "shape": "a"})).collect();
        junk.extend([
            json!({"path": 5, "keyColumn": [], "columns": "x", "format": "pdf"}),
            json!({"path": "/dev/zero", "format": "xlsx", "sheet": long, "keyColumn": "Tag"}),
            json!({"id": 1e300, "row": -1, "shape": {}}),
            json!({"id": "dl1", "row": 1e300, "shape": "nope"}),
            json!({"graphic": 5, "items": [{"kind": "text", "key": long}], "name": long}),
            json!({"name": "g", "items": [{"kind": "bar", "key": "k", "min": 1e308, "max": -1e308}], "id": "../x"}),
            json!(null),
        ]);
        let ids: Vec<&str> =
            crate::cmd::registry().all().iter().map(|c| c.id).filter(|id| id.starts_with("datalink.") || id.starts_with("datagraphic.")).collect();
        assert_eq!(ids.len(), 10);
        for id in ids {
            for j in &junk {
                let mut s = session();
                let _ = s.run(id, j);
                assert!(s.doc.data_links.is_empty(), "{id}: {j}");
            }
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
