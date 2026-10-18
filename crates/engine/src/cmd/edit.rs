//! Edit menu commands: undo, redo, selection, delete and the clipboard.
//!
//! The clipboard holds a [`Fragment`] in its JSON form (see [`crate::fragment`]). `edit.copy` and
//! `edit.cut` store it in `Session::clipboard` and return it as `{"text"}` for the system
//! clipboard; `edit.paste` takes `{"text"}` from the host or falls back to the internal copy.

use serde_json::{Value, json};

use super::{delete_items, has_selection, ids_param, opt_bool, opt_str, prune_selection, sel_result, select, short, with_sel};
use crate::fragment::Fragment;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// How far a paste lands from the copied items, so the copy is visible beside the original.
pub const PASTE_OFFSET: f64 = 20.0;

/// Largest clipboard text `edit.paste` reads; beyond this it refuses instead of parsing.
pub const MAX_CLIP_BYTES: usize = 64 * 1024 * 1024;

/// Return all edit commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.undo", "Undo", "Home > Undo", |s, _| {
            s.undo();
            prune_selection(s);
            sel_result(s)
        })
        .key("Mod+Z")
        .when(|s| if s.can_undo() { None } else { Some("nothing to undo") }),
        CommandSpec::new("edit.redo", "Redo", "Home > Undo", |s, _| {
            s.redo();
            prune_selection(s);
            sel_result(s)
        })
        .key("Mod+Shift+Z / Mod+Y")
        .when(|s| if s.can_redo() { None } else { Some("nothing to redo") }),
        CommandSpec::new("edit.cut", "Cut", "Home > Clipboard", cut).key("Mod+X").when(has_selection),
        CommandSpec::new("edit.copy", "Copy", "Home > Clipboard", copy).key("Mod+C").when(has_selection),
        CommandSpec::new("edit.paste", "Paste", "Home > Clipboard", paste)
            .key("Mod+V")
            .params(r#"{"text"?: string} (default: the internal clipboard)"#),
        CommandSpec::new("edit.delete", "Delete", "Home > Editing", |s, _| {
            let ids = s.sel.items.clone();
            delete_items(s, &ids)
        })
        // Bare keys: the UI key map treats Delete and Backspace as shortcuts only while no text
        // field has the keyboard, so typing in a field still deletes characters.
        .key("Delete / Backspace")
        .when(has_selection),
        CommandSpec::new("edit.select", "Select", "Home > Editing", select_ids).params(r#"{"ids": [string], "add"?: bool}"#),
        CommandSpec::new("edit.selectAll", "Select All", "Home > Editing", select_all)
            .key("Mod+A")
            .when(|s| if s.doc.page().is_some_and(|p| !p.shapes.is_empty() || !p.connectors.is_empty()) { None } else { Some("the page is empty") }),
        CommandSpec::new("edit.selectNone", "Select None", "Home > Editing", |s, _| {
            s.sel.clear();
            sel_result(s)
        }),
        CommandSpec::new("edit.setTitle", "Document Title", "Properties", |s, v| {
            s.doc.title = p::req_str(v, "title")?.to_string();
            Ok(json!({"title": s.doc.title}))
        })
        .params(r#"{"title": string}"#),
    ]
}

/// Select exactly `ids`, or add them to the selection with `add: true`. Every id must name a
/// shape or connector on the page; an unknown id is an error and the selection stays as it was,
/// so an agent learns about a typo instead of acting on a partial selection.
fn select_ids(s: &mut Session, v: &Value) -> CmdResult {
    let ids = ids_param(v)?.ok_or_else(|| EngineError::Params("`ids` (array of strings) is required".into()))?;
    let add = opt_bool(v, "add")?.unwrap_or(false);
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(bad) = ids.iter().find(|id| !page.contains_id(id)) {
        return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(bad))));
    }
    super::layer::ensure_selectable(s, &ids)?;
    let mut out = if add { s.sel.items.clone() } else { Vec::new() };
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    let primary = if add { s.sel.primary.clone().filter(|p| out.contains(p)) } else { None };
    select(s, out);
    if primary.is_some() {
        s.sel.primary = primary;
    }
    sel_result(s)
}

/// Select every shape and connector on the page, shapes first.
fn select_all(s: &mut Session, _: &Value) -> CmdResult {
    let ids: Vec<String> =
        s.doc.page().map(|p| p.shapes.iter().map(|x| x.id.clone()).chain(p.connectors.iter().map(|c| c.id.clone())).collect()).unwrap_or_default();
    let ids = super::layer::selectable_only(s, ids);
    select(s, ids);
    sel_result(s)
}

fn copy(s: &mut Session, _: &Value) -> CmdResult {
    if let Some(why) = has_selection(s) {
        return Err(EngineError::Params(why.into()));
    }
    let frag = Fragment::of_selection(s);
    let text = serde_json::to_string(&frag.to_json()).map_err(|e| EngineError::Failed(format!("copy: {e}")))?;
    s.clipboard = text.clone();
    Ok(json!({"text": text, "shapes": frag.shapes.len(), "connectors": frag.connectors.len()}))
}

fn cut(s: &mut Session, v: &Value) -> CmdResult {
    let copied = copy(s, v)?;
    let ids = s.sel.items.clone();
    let mut out = delete_items(s, &ids)?;
    if let (Some(o), Some(text)) = (out.as_object_mut(), copied.get("text")) {
        o.insert("text".into(), text.clone());
    }
    Ok(out)
}

/// Paste a clip as a [`Fragment`]: fresh ids, glue between pasted items remapped to the new ids,
/// everything offset by [`PASTE_OFFSET`], and the pasted items selected.
fn paste(s: &mut Session, v: &Value) -> CmdResult {
    let text = match opt_str(v, "text")? {
        Some(t) => t.to_string(),
        None => s.clipboard.clone(),
    };
    if text.trim().is_empty() {
        return Err(EngineError::Params("the clipboard is empty".into()));
    }
    if text.len() > MAX_CLIP_BYTES {
        return Err(EngineError::Params("the clipboard text is too large to paste".into()));
    }
    let clip: Value = serde_json::from_str(&text).map_err(|_| EngineError::Params("the clipboard does not hold VizCraft shapes".into()))?;
    let frag = Fragment::from_json(&clip)?;
    let ids = frag.insert(&mut s.doc, PASTE_OFFSET, PASTE_OFFSET)?;
    if ids.is_empty() {
        return Err(EngineError::Params("the clipboard has nothing to paste".into()));
    }
    select(s, ids.clone());
    with_sel(s, json!({"pasted": ids}))
}
