//! Page commands (`page.*`): list the pages, read one as draw.io XML or VizCraft JSON, replace
//! one from draw.io XML, in the open document or in a file on disk.
//!
//! A page is named by id, name or zero-based index (`Document::find_page`). With `path`, the
//! commands work on that file (`.drawio` or `.vizcraft`) and leave the session alone; without it
//! they work on the open document. `page.set` without a path is `file.import` with
//! `format: "drawio"` and `replace: true`, and shares its code.

use serde_json::{Value, json};

use super::file::{import_into_session, import_opts, place};
use super::{opt_str, short};
use crate::doc::Document;
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The page commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("page.list", "List Pages", "Page", list)
            .params(r#"{"path"?: string} -> {pages: [{index, id, name, width, height, shapes, connectors}]}"#),
        CommandSpec::new("page.get", "Get Page", "Page", get)
            .params(r#"{"page"?: id|name|index (default the first), "path"?: string, "format"?: "drawio"|"vizcraft"} -> {page, format, content}"#),
        CommandSpec::new("page.set", "Set Page", "Page", set).params(
            r#"{"page": id|name|index, "xml": string, "path"?: string, "layout"?: "layered"|"tree"|"radial"|"force", "direction"?: "TB"|"BT"|"LR"|"RL", "routing"?: "orthogonal"} -> {page, ids, warnings} (replaces the page from draw.io XML; a name that matches nothing adds a page; with path the file is edited and saved, the open document is untouched)"#,
        ),
    ]
}

/// The document a command reads: the file at `path` when given, else the open one.
fn source<'a>(s: &'a Session, path: Option<&str>) -> Result<std::borrow::Cow<'a, Document>, EngineError> {
    match path {
        Some(path) => Ok(std::borrow::Cow::Owned(load_file(path)?)),
        None => Ok(std::borrow::Cow::Borrowed(&s.doc)),
    }
}

fn load_file(path: &str) -> Result<Document, EngineError> {
    if !crate::io::is_document(path) {
        return Err(EngineError::Params(format!("{}: page commands read .drawio and .vizcraft files", short(path))));
    }
    let bytes = crate::io::read_limited(path)?;
    crate::io::load_bytes(path, &bytes)
}

/// The `page` param as text: a string, or a number meaning an index.
fn page_ref(v: &Value) -> Result<Option<String>, EngineError> {
    match v.get("page") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(EngineError::Params("`page` must be a page id, name or index".into())),
    }
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let doc = source(s, opt_str(v, "path")?)?;
    let pages: Vec<Value> = doc
        .pages
        .iter()
        .enumerate()
        .map(|(index, p)| {
            json!({
                "index": index,
                "id": p.id,
                "name": p.name,
                "width": p.width,
                "height": p.height,
                "shapes": p.shapes.len(),
                "connectors": p.connectors.len(),
            })
        })
        .collect();
    Ok(json!({"pages": pages}))
}

fn get(s: &mut Session, v: &Value) -> CmdResult {
    let format = match opt_str(v, "format")?.map(|f| f.trim().to_ascii_lowercase()) {
        None => "drawio",
        Some(f) if f == "drawio" || f == "xml" => "drawio",
        Some(f) if f == "vizcraft" || f == "json" => "vizcraft",
        Some(f) => return Err(EngineError::Params(format!("`format` must be drawio or vizcraft, not `{}`", short(&f)))),
    };
    let r = page_ref(v)?;
    let doc = source(s, opt_str(v, "path")?)?;
    let index = match &r {
        None => 0,
        Some(r) => doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r))))?,
    };
    let page = doc.pages.get(index).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let content = match format {
        "drawio" => vizcraft_formats::drawio::page_xml(page).map_err(|e| EngineError::Failed(e.to_string()))?,
        _ => serde_json::to_string_pretty(page).map_err(|e| EngineError::Failed(e.to_string()))?,
    };
    Ok(json!({"page": page.id, "format": format, "content": content}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let Some(r) = page_ref(v)? else { return Err(EngineError::Params("`page` (id, name or index) is required".into())) };
    let xml = opt_str(v, "xml")?.ok_or_else(|| EngineError::Params("`xml` (string) is required".into()))?;
    if xml.len() as u64 > crate::io::MAX_IMPORT_BYTES {
        return Err(EngineError::Params(format!("`xml` is longer than {} MiB", crate::io::MAX_IMPORT_BYTES / (1024 * 1024))));
    }
    let mut opts = import_opts(v)?;
    opts.page = Some(r);
    opts.replace = true;
    let imported = crate::io::import_bytes("drawio", xml.as_bytes())?;
    let Some(path) = opt_str(v, "path")? else {
        return import_into_session(s, imported, &opts, None);
    };
    let mut doc = load_file(path)?;
    let placed = place(&mut doc, imported, &opts)?;
    let bytes = crate::io::save_bytes(path, &doc)?;
    std::fs::write(path, bytes).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    let page = doc.pages.get(placed.index).map(|p| p.id.clone()).unwrap_or_default();
    Ok(json!({"page": page, "ids": placed.ids, "warnings": placed.warnings, "path": path}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    const DOCS_EXAMPLE: &str = include_str!("../../../formats/testdata/drawio-docs-example.drawio");

    fn temp_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-page-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_and_get_the_open_document() {
        let mut s = Session::new(crate::sample::sample_document());
        let r = s.run("page.list", &json!({})).unwrap();
        let first = &r["pages"][0];
        assert_eq!(first["index"], 0);
        assert_eq!(first["shapes"], s.doc.page().unwrap().shapes.len());
        let xml = s.run("page.get", &json!({})).unwrap();
        assert_eq!(xml["format"], "drawio");
        assert!(xml["content"].as_str().unwrap().starts_with("<mxGraphModel"));
        let page_id = s.doc.page().unwrap().id.clone();
        let js = s.run("page.get", &json!({"page": page_id, "format": "vizcraft"})).unwrap();
        let back: vizcraft_diagram::Page = serde_json::from_str(js["content"].as_str().unwrap()).unwrap();
        assert_eq!(&back, s.doc.page().unwrap());
        assert!(s.run("page.get", &json!({"page": "nope"})).is_err());
        assert!(s.run("page.get", &json!({"page": 3})).is_err());
        assert!(s.run("page.get", &json!({"format": "png"})).is_err());
        assert!(!s.can_undo(), "reading changes nothing");
    }

    #[test]
    fn set_without_a_path_replaces_a_page_in_one_undo_step() {
        let mut s = Session::new(crate::sample::sample_document());
        let before = s.doc.clone();
        let r = s.run("page.set", &json!({"page": 0, "xml": DOCS_EXAMPLE, "routing": "orthogonal"})).unwrap();
        assert_eq!(r["ids"].as_array().unwrap().len(), 3);
        let page = s.doc.page().unwrap();
        assert_eq!(page.shapes.len(), 2);
        assert!(page.connectors.iter().all(|c| c.avoid));
        assert!(s.run("page.set", &json!({"xml": DOCS_EXAMPLE})).is_err());
        assert!(s.run("page.set", &json!({"page": 0})).is_err());
        assert!(s.run("page.set", &json!({"page": 0, "xml": "<mxGraphModel><root>"})).is_err());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc, before);
    }

    #[test]
    fn path_commands_edit_the_file_and_leave_the_session_alone() {
        let dir = temp_dir("path");
        for ext in ["drawio", "vizcraft"] {
            let path = dir.join(format!("doc.{ext}"));
            let p = path.to_string_lossy().to_string();
            let mut s = Session::new(crate::sample::sample_document());
            s.run("file.save", &json!({"path": p})).unwrap();
            let mut t = Session::new(Document::new());
            let before = t.doc.clone();
            let r = t.run("page.set", &json!({"path": p, "page": "Added", "xml": DOCS_EXAMPLE, "layout": "layered"})).unwrap();
            assert_eq!(r["ids"].as_array().unwrap().len(), 3, "{ext}");
            assert_eq!(t.doc, before, "the open document is untouched");
            assert!(!t.can_undo());
            let list = t.run("page.list", &json!({"path": p})).unwrap();
            let pages = list["pages"].as_array().unwrap();
            assert_eq!(pages.len(), 2, "{ext}");
            assert_eq!(pages[1]["name"], "Added");
            assert_eq!(pages[1]["shapes"], 2);
            let got = t.run("page.get", &json!({"path": p, "page": "Added"})).unwrap();
            assert!(got["content"].as_str().unwrap().contains("Hello,"));
            // Replacing the page again keeps two pages.
            t.run("page.set", &json!({"path": p, "page": "Added", "xml": DOCS_EXAMPLE})).unwrap();
            assert_eq!(t.run("page.list", &json!({"path": p})).unwrap()["pages"].as_array().unwrap().len(), 2);
        }
        let bad = dir.join("notes.txt");
        std::fs::write(&bad, "x").unwrap();
        let mut t = Session::new(Document::new());
        assert!(t.run("page.list", &json!({"path": bad.to_string_lossy()})).is_err());
        assert!(t.run("page.set", &json!({"path": bad.to_string_lossy(), "page": 0, "xml": DOCS_EXAMPLE})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
