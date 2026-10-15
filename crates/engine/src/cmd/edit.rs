//! Edit menu: undo, redo and the clipboard, on the selected range of the current note; plus the
//! text commands the canvas and agents edit through.

use serde_json::{Value, json};

use super::sel_result;
use crate::{CmdResult, CommandSpec, EngineError, Selection, Session, p};

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
        CommandSpec::new("edit.cut", "Cut", "Home > Clipboard", cut).key("Mod+X").when(has_selection),
        CommandSpec::new("edit.copy", "Copy", "Home > Clipboard", copy).key("Mod+C").when(has_selection),
        CommandSpec::new("edit.paste", "Paste", "Home > Clipboard", paste).key("Mod+V").params(r#"{"text"?: string}"#),
        CommandSpec::new("edit.selectAll", "Select All", "Home > Editing", |s, _| {
            let len = s.current_body().chars().count();
            s.sel = Selection { note: s.sel.note, start: 0, end: len };
            sel_result(s)
        })
        .key("Mod+A"),
        CommandSpec::new("edit.select", "Select", "Agents", select).params(r#"{"note"?: n, "start"?: n, "end"?: n}"#),
        CommandSpec::new("edit.setText", "Typing", "Agents", set_text).params(r#"{"text": string, "note"?: n}"#),
        CommandSpec::new("edit.setNoteTitle", "Rename Note", "Properties", set_title).params(r#"{"title": string, "note"?: n}"#),
        CommandSpec::new("edit.setTitle", "Document Title", "Properties", |s, v| {
            s.doc.title = p::req_str(v, "title")?.to_string();
            Ok(json!({"title": s.doc.title}))
        })
        .params(r#"{"title": string}"#),
        CommandSpec::new("edit.deleteNote", "Delete Note", "Home > Notes", delete_note)
            .params(r#"{"note"?: n}"#)
            .when(|s| if s.doc.notes.len() > 1 { None } else { Some("the last note stays") }),
    ]
}

fn has_selection(s: &Session) -> Option<&'static str> {
    if s.sel.is_collapsed() { Some("nothing is selected") } else { None }
}

/// Replace characters `a..b` of the current note with `text`; the caret lands after it.
fn splice(s: &mut Session, a: usize, b: usize, text: &str) -> Result<(), EngineError> {
    let note = s.sel.note;
    let Some(n) = s.doc.notes.get_mut(note) else { return Err(EngineError::Failed("no current note".into())) };
    let chars: Vec<char> = n.body.chars().collect();
    let a = a.min(chars.len());
    let b = b.clamp(a, chars.len());
    let mut out: String = chars[..a].iter().collect();
    out.push_str(text);
    out.extend(chars[b..].iter());
    n.body = out;
    s.sel = Selection::caret(note, a + text.chars().count());
    Ok(())
}

fn cut(s: &mut Session, _: &Value) -> CmdResult {
    let text = s.selected_text();
    let (a, b) = s.sel.ordered();
    splice(s, a, b, "")?;
    s.clipboard = text.clone();
    Ok(json!({"text": text}))
}

fn copy(s: &mut Session, _: &Value) -> CmdResult {
    let text = s.selected_text();
    s.clipboard = text.clone();
    Ok(json!({"text": text}))
}

fn paste(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::str(v, "text").map(str::to_string).unwrap_or_else(|| s.clipboard.clone());
    let (a, b) = s.sel.ordered();
    splice(s, a, b, &text)?;
    sel_result(s)
}

fn select(s: &mut Session, v: &Value) -> CmdResult {
    let note = p::usize(v, "note").unwrap_or(s.sel.note);
    if note >= s.doc.notes.len() {
        return Err(EngineError::Params(format!("note {note} does not exist")));
    }
    let start = p::usize(v, "start").unwrap_or(0);
    s.sel = Selection { note, start, end: p::usize(v, "end").unwrap_or(start) };
    s.clamp_selection();
    sel_result(s)
}

fn set_text(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "text")?;
    let note = p::usize(v, "note").unwrap_or(s.sel.note);
    let Some(n) = s.doc.notes.get_mut(note) else { return Err(EngineError::Params(format!("note {note} does not exist"))) };
    n.body = text.to_string();
    s.sel = Selection::caret(note, text.chars().count());
    sel_result(s)
}

fn set_title(s: &mut Session, v: &Value) -> CmdResult {
    let title = p::req_str(v, "title")?;
    let note = p::usize(v, "note").unwrap_or(s.sel.note);
    let Some(n) = s.doc.notes.get_mut(note) else { return Err(EngineError::Params(format!("note {note} does not exist"))) };
    n.title = title.to_string();
    Ok(json!({"note": note, "title": title}))
}

fn delete_note(s: &mut Session, v: &Value) -> CmdResult {
    let note = p::usize(v, "note").unwrap_or(s.sel.note);
    if note >= s.doc.notes.len() {
        return Err(EngineError::Params(format!("note {note} does not exist")));
    }
    s.doc.notes.remove(note);
    s.sel = Selection::caret(note.min(s.doc.notes.len().saturating_sub(1)), 0);
    sel_result(s)
}
