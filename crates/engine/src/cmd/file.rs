//! File menu: new, open, save, save as, close, user name.
//!
//! Without a `path`, `file.open` and `file.saveAs` ask the UI for a picker through
//! `ui_requests`; with one they work headless (CLI, control channel, tests).

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
