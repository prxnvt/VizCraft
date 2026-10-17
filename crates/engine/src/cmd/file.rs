//! File menu: new, open, save, save as, export, close, user name.
//!
//! Without a `path`, `file.open`, `file.saveAs` and `file.export` ask the UI for a picker
//! through `ui_requests`; with one they work headless (CLI, control channel, tests). Formats are
//! chosen by `crate::io` from the file name.

use serde_json::{Value, json};

use super::sel_result;
use crate::doc::Document;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("file.new", "New", "File", new).key("Mod+N").params(r#"{"template"?: "blank|sample"}"#),
        CommandSpec::new("file.open", "Open", "File", open).key("Mod+O").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.save", "Save", "File", save).key("Mod+S").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.saveAs", "Save As", "File", save_as).key("Mod+Shift+S").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.close", "Close", "File", |s, _| {
            s.ui_requests.push(json!({"close": true}));
            sel_result(s)
        })
        .key("Mod+W"),
        CommandSpec::new("file.export", "Export...", "File", export)
            .key("Mod+Shift+E")
            .params(r#"{"path"?: string, "format"?: "svg"} (first page; format from the extension when omitted)"#),
        CommandSpec::new("file.import", "Import...", "File", import).key("Mod+Shift+I").params(r#"{"path"?: string}"#),
        CommandSpec::new("file.setAuthor", "User Name", "File > Options", |s, v| {
            s.author = p::req_str(v, "name")?.trim().to_string();
            Ok(json!({"author": s.author}))
        })
        .params(r#"{"name": string}"#),
    ]
}

fn new(s: &mut Session, v: &Value) -> CmdResult {
    let doc = match p::str(v, "template").unwrap_or("blank") {
        "sample" => crate::sample::sample_document(),
        _ => Document::new(),
    };
    s.set_document(doc);
    s.path = None;
    sel_result(s)
}

fn open(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "openFile"}));
        return sel_result(s);
    };
    let bytes = std::fs::read(path).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    let doc = crate::io::load_bytes(path, &bytes)?;
    s.set_document(doc);
    s.path = Some(path.into());
    Ok(json!({"path": path}))
}

fn save(s: &mut Session, v: &Value) -> CmdResult {
    let path = match p::str(v, "path").map(std::path::PathBuf::from).or_else(|| s.path.clone()) {
        Some(p) => p,
        None => {
            s.ui_requests.push(json!({"open": "saveAs"}));
            return Ok(json!({"saved": false}));
        }
    };
    // Metadata is stamped before the bytes are made, so the file records this save.
    s.doc.core.modified = super::now_iso();
    if s.doc.core.created.is_empty() {
        s.doc.core.created = s.doc.core.modified.clone();
    }
    if s.doc.core.author.is_empty() {
        s.doc.core.author = s.author.clone();
    }
    s.doc.core.last_modified_by = s.author.clone();
    s.doc.core.revision = s.doc.core.revision.saturating_add(1);
    let name = path.to_string_lossy().to_string();
    let bytes = crate::io::save_bytes(&name, &s.doc)?;
    std::fs::write(&path, bytes).map_err(|e| EngineError::Failed(format!("{name}: {e}")))?;
    s.path = Some(path);
    s.mark_saved();
    s.status = format!("Saved {name}");
    Ok(json!({"saved": true, "path": name}))
}

fn save_as(s: &mut Session, v: &Value) -> CmdResult {
    if p::str(v, "path").is_none() {
        s.ui_requests.push(json!({"open": "saveAs"}));
        return Ok(json!({"saved": false}));
    }
    save(s, v)
}

fn export(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "export"}));
        return Ok(json!({"exported": false}));
    };
    let format = crate::io::export_format(path, p::str(v, "format"))?;
    let bytes = crate::io::export_bytes(path, Some(format), &s.doc)?;
    std::fs::write(path, bytes).map_err(|e| EngineError::Failed(format!("{path}: {e}")))?;
    s.status = format!("Exported {path}");
    Ok(json!({"exported": true, "path": path, "format": format}))
}

fn import(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.ui_requests.push(json!({"open": "import"}));
        return Ok(json!({"imported": false}));
    };
    s.status = format!("Imported {path}");
    Ok(json!({"imported": true, "path": path}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    /// A fresh directory under the system temp dir, unique to this process and test.
    fn temp_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-file-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_open_restores_the_sample() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("sample.vizcraft");
        let mut s = Session::new(crate::sample::sample_document());
        let r = s.run("file.save", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["saved"], true);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 2"));

        let mut t = Session::new(Document::new());
        t.run("file.open", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(t.doc, s.doc);
        assert!(!t.doc.page().unwrap().connectors.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_writes_svg() {
        let dir = temp_dir("export");
        let mut s = Session::new(crate::sample::sample_document());
        let path = dir.join("page.svg");
        let r = s.run("file.export", &json!({"path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["exported"], true);
        assert_eq!(r["format"], "svg");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg"));
        assert!(s.status.starts_with("Exported"));
        assert!(!s.dirty, "exporting does not change the document");

        // An explicit format wins over the extension; an unknown one writes nothing.
        let other = dir.join("page.out");
        s.run("file.export", &json!({"path": other.to_string_lossy(), "format": "svg"})).unwrap();
        assert!(std::fs::read(&other).unwrap().starts_with(b"<?xml"));
        let bad = dir.join("page.png");
        assert!(s.run("file.export", &json!({"path": bad.to_string_lossy()})).is_err());
        assert!(!bad.exists());

        // Without a path the UI is asked for a picker.
        assert_eq!(s.run("file.export", &json!({})).unwrap()["exported"], false);
        assert_eq!(s.ui_requests.pop(), Some(json!({"open": "export"})));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
