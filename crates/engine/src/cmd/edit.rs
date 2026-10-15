//! Edit menu commands: undo, redo, clipboard, and selection operations.

use serde_json::{Value, json};

use super::sel_result;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all edit commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.undo", "Undo", "Home > Undo", |s, _| {
            s.undo();
            sel_result(s)
        })
        .key("Mod+Z")
        .when(|s| if s.can_undo() { None } else { Some("nothing to undo") }),
        CommandSpec::new("edit.redo", "Redo", "Home > Undo", |s, _| {
            s.redo();
            sel_result(s)
        })
        .key("Mod+Shift+Z / Mod+Y")
        .when(|s| if s.can_redo() { None } else { Some("nothing to redo") }),
        CommandSpec::new("edit.cut", "Cut", "Home > Clipboard", cut).key("Mod+X"),
        CommandSpec::new("edit.copy", "Copy", "Home > Clipboard", copy).key("Mod+C"),
        CommandSpec::new("edit.paste", "Paste", "Home > Clipboard", paste).key("Mod+V").params(r#"{"text"?: string}"#),
        CommandSpec::new("edit.selectAll", "Select All", "Home > Editing", select_all).key("Mod+A"),
        CommandSpec::new("edit.setTitle", "Document Title", "Properties", |s, v| {
            s.doc.title = p::req_str(v, "title")?.to_string();
            Ok(json!({"title": s.doc.title}))
        })
        .params(r#"{"title": string}"#),
    ]
}

fn cut(_: &mut Session, _: &Value) -> CmdResult {
    Err(EngineError::Failed("Nothing to cut in template".into()))
}

fn copy(_: &mut Session, _: &Value) -> CmdResult {
    Err(EngineError::Failed("Nothing to copy in template".into()))
}

fn paste(_: &mut Session, _: &Value) -> CmdResult {
    Err(EngineError::Failed("Nothing to paste in template".into()))
}

fn select_all(_: &mut Session, _: &Value) -> CmdResult {
    Err(EngineError::Failed("Nothing to select in template".into()))
}
