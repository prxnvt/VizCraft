//! Insert menu: a new note after the current one.

use serde_json::Value;

use super::sel_result;
use crate::doc::Note;
use crate::{CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("insert.note", "Note", "Insert > Notes", note).key("Mod+Shift+N").params(r#"{"title"?: string, "body"?: string}"#)]
}

fn note(s: &mut Session, v: &Value) -> CmdResult {
    let n = s.doc.notes.len();
    let title = p::str(v, "title").map(str::to_string).unwrap_or_else(|| format!("Note {}", n + 1));
    let body = p::str(v, "body").unwrap_or("");
    let at = (s.sel.note + 1).min(n);
    s.doc.notes.insert(at, Note::new(title, body));
    s.sel = Selection::caret(at, body.chars().count());
    sel_result(s)
}
