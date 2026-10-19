//! Edit menu commands: undo, redo, selection, delete, the clipboard and the Home tab's editing
//! tools (duplicate, paste special, format painter, change shape, select by type).
//!
//! The clipboard holds a [`Fragment`] in its JSON form (see [`crate::fragment`]). `edit.copy` and
//! `edit.cut` store it in `Session::clipboard` and return it as `{"text"}` for the system
//! clipboard; `edit.paste` takes `{"text"}` from the host or falls back to the internal copy.
//! Find and Replace live in [`super::find`].

use serde_json::{Value, json};
use vizcraft_diagram::{End, Point, Shape, Style, arrange, layer};

use super::{
    Kinds, delete_items, has_selection, ids_param, num, opt_bool, opt_str, page_mut, prune_selection, sel_result, select, short, targets, with_sel,
};
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
        CommandSpec::new("edit.duplicate", "Duplicate", "Home > Clipboard", duplicate)
            .key("Mod+D")
            .params(r#"{"ids"?: [string], "dx"?: n, "dy"?: n} -> {"duplicated": [id]} (default: the selection, 20 right and down); the copies are selected"#)
            .when(has_selection_or_ids),
        CommandSpec::new("edit.pasteSpecial", "Paste Special...", "Home > Clipboard", paste_special).params(
            r#"{"as"?: "shapes"|"text"|"style", "text"?: string} (default: the internal clipboard); "text" sets the selection's text, "style" its whole style; without "as" the UI asks"#,
        ),
        CommandSpec::new("edit.formatPainter", "Format Painter", "Home > Clipboard", format_painter)
            .params(r#"{"from"?: id, "to"?: [id]} -> {"painted": [id]} (default: from the primary selection to the rest); copies fill, line and effects, not text or geometry"#)
            .when(has_selection_or_ids),
        CommandSpec::new("edit.changeShape", "Change Shape", "Home > Editing", change_shape).params(
            r#"{"stencil": string, "ids"?: [id]} -> {"changed": [id]}; keeps frame, text, data, layers, parent, glue (nearest connection point) and hand formatting"#,
        ),
        CommandSpec::new("edit.selectByType", "Select by Type", "Home > Editing", select_by_type)
            .params(r#"{"kind": "shapes"|"connectors"|"groups"|"text"|"layer", "layer"?: id|name}"#),
        CommandSpec::new("edit.setTitle", "Document Title", "Properties", |s, v| {
            super::document::write_title(&mut s.doc, p::req_str(v, "title")?);
            Ok(json!({"title": s.doc.title}))
        })
        .params(r#"{"title": string} (the same title as doc.setProperty {key: "title"}: the title block, exports and the window show it)"#),
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

/// Enabled with a selection; a caller passing `ids` (or `from`) does not need one, so the check
/// is left to the command.
fn has_selection_or_ids(_: &Session) -> Option<&'static str> {
    None
}

/// Copy items as a [`Fragment`] at an offset: internal glue and group membership come along,
/// and the copies are selected.
fn duplicate(s: &mut Session, v: &Value) -> CmdResult {
    let ids = targets(s, v, Kinds::Both)?;
    let dx = num(v, "dx")?.unwrap_or(PASTE_OFFSET);
    let dy = num(v, "dy")?.unwrap_or(PASTE_OFFSET);
    let frag = Fragment::of_items(&s.doc, &ids);
    if frag.is_empty() {
        return Err(EngineError::Params("nothing to duplicate".into()));
    }
    let new = frag.insert(&mut s.doc, dx, dy)?;
    // Select the top level of the copies, so a duplicated group is selected as the group.
    let top = s
        .doc
        .page()
        .map(|p| {
            let shapes = arrange::top_level(p, &new);
            new.iter().filter(|id| shapes.contains(id) || p.connector(id).is_some()).cloned().collect()
        })
        .unwrap_or_else(|| new.clone());
    select(s, top);
    with_sel(s, json!({"duplicated": new}))
}

/// Paste only part of the clipboard: its shapes (as `edit.paste`), its text into the selected
/// items, or its first item's style onto them. Without `as` this asks the UI for the Paste
/// Special dialog.
fn paste_special(s: &mut Session, v: &Value) -> CmdResult {
    let Some(how) = opt_str(v, "as")? else {
        s.ui_requests.push(json!({"open": "pasteSpecial"}));
        return Ok(json!({"dialog": "pasteSpecial"}));
    };
    if !matches!(how, "shapes" | "text" | "style") {
        return Err(EngineError::Params(format!("`as` must be shapes, text or style, not `{}`", short(how))));
    }
    if how == "shapes" {
        return paste(s, v);
    }
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
    // Plain text from another program pastes as text; it has no style.
    let frag = serde_json::from_str::<Value>(&text).ok().and_then(|c| Fragment::from_json(&c).ok());
    let ids = targets(s, v, Kinds::Both)?;
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    if how == "text" {
        let t = match &frag {
            Some(f) => {
                let parts: Vec<&str> =
                    f.shapes.iter().map(|x| x.text.as_str()).chain(f.connectors.iter().map(|c| c.text.as_str())).filter(|t| !t.is_empty()).collect();
                parts.join("\n")
            }
            None => text,
        };
        for id in &ids {
            if let Some(sh) = page.shape_mut(id) {
                sh.text = t.clone();
                sh.sanitize();
            } else if let Some(c) = page.connector_mut(id) {
                c.text = t.clone();
                c.sanitize();
            }
        }
    } else {
        let style = frag
            .as_ref()
            .and_then(|f| f.shapes.first().map(|x| x.style.clone()).or_else(|| f.connectors.first().map(|c| c.style.clone())))
            .ok_or_else(|| EngineError::Params("the clipboard has no style to paste".into()))?;
        for id in &ids {
            if let Some(sh) = page.shape_mut(id) {
                sh.style = style.clone();
                sh.sanitize();
            } else if let Some(c) = page.connector_mut(id) {
                c.style = style.clone();
                c.sanitize();
            }
        }
    }
    with_sel(s, json!({"pasted": how, "ids": ids}))
}

/// Copy the fill, line and effect fields of `src` onto `dst`, leaving text formatting alone. A
/// connector has no fill, so fill fields move only between shapes.
pub fn paint_style(dst: &mut Style, src: &Style, fills: bool) {
    if fills {
        dst.fill = src.fill;
        dst.gradient = src.gradient.clone();
        dst.fill_transparency = src.fill_transparency;
        dst.soft_edges = src.soft_edges;
    }
    dst.line = src.line;
    dst.line_width = src.line_width;
    dst.dash = src.dash;
    dst.line_cap = src.line_cap;
    dst.line_transparency = src.line_transparency;
    dst.corner_radius = src.corner_radius;
    dst.shadow = src.shadow;
    dst.glow = src.glow;
}

/// Format painter: the fill, line and effects of one shape or connector onto others (arrowheads
/// too between connectors). Text, text formatting and geometry stay as they are.
fn format_painter(s: &mut Session, v: &Value) -> CmdResult {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let from = match opt_str(v, "from")? {
        Some(f) => f.to_string(),
        None => s
            .sel
            .primary
            .clone()
            .or_else(|| s.sel.items.first().cloned())
            .ok_or_else(|| EngineError::Params("select the shape to copy the format from".into()))?,
    };
    let (src, src_arrows) = if let Some(sh) = page.shape(&from) {
        (sh.style.clone(), None)
    } else if let Some(c) = page.connector(&from) {
        (c.style.clone(), Some((c.start_arrow, c.end_arrow)))
    } else {
        return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(&from))));
    };
    let to: Vec<String> = match v.get("to") {
        None | Some(Value::Null) => s.sel.items.iter().filter(|i| **i != from).cloned().collect(),
        Some(_) => {
            let ids = ids_param(&json!({"ids": v.get("to")}))?.unwrap_or_default();
            if let Some(bad) = ids.iter().find(|id| !page.contains_id(id)) {
                return Err(EngineError::Params(format!("`{}` is not a shape or connector on this page", short(bad))));
            }
            ids
        }
    };
    let to: Vec<String> = to.into_iter().filter(|i| *i != from).collect();
    if to.is_empty() {
        return Err(EngineError::Params("choose the items to copy the format to (`to`)".into()));
    }
    super::layer::ensure_unlocked(s, &to)?;
    let src_is_shape = src_arrows.is_none();
    let page = page_mut(s)?;
    for id in &to {
        if let Some(sh) = page.shape_mut(id) {
            paint_style(&mut sh.style, &src, src_is_shape);
            sh.style.sanitize();
        } else if let Some(c) = page.connector_mut(id) {
            paint_style(&mut c.style, &src, false);
            if let Some((a, b)) = src_arrows {
                c.start_arrow = a;
                c.end_arrow = b;
            }
            c.style.sanitize();
        }
    }
    with_sel(s, json!({"painted": to}))
}

/// The style a shape gets from `old` when it changes from a master with style `was` to one
/// with style `now`: each field the user changed by hand (it differs from `was`) is kept, every
/// other field takes the new master's value. Compared field by field through the serialized
/// form, so a field added to `Style` later is handled without touching this.
pub fn restyle_for_master(old: &Style, was: &Style, now: &Style) -> Style {
    let (Ok(Value::Object(o)), Ok(Value::Object(w)), Ok(Value::Object(n))) =
        (serde_json::to_value(old), serde_json::to_value(was), serde_json::to_value(now))
    else {
        return now.clone();
    };
    let mut out = serde_json::Map::new();
    let keys: std::collections::BTreeSet<&String> = o.keys().chain(w.keys()).chain(n.keys()).collect();
    for k in keys {
        let pick = if o.get(k) != w.get(k) { o.get(k) } else { n.get(k) };
        if let Some(x) = pick {
            out.insert(k.clone(), x.clone());
        }
    }
    let mut st: Style = serde_json::from_value(Value::Object(out)).unwrap_or_else(|_| now.clone());
    st.sanitize();
    st
}

/// The index in `new` nearest to `p` (frame-normalized coordinates); `None` when `new` is empty.
fn nearest_point(new: &[Point], p: Point) -> Option<usize> {
    new.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = (a.x - p.x).powi(2) + (a.y - p.y).powi(2);
            let db = (b.x - p.x).powi(2) + (b.y - p.y).powi(2);
            da.total_cmp(&db)
        })
        .map(|(i, _)| i)
}

/// Swap the master of shapes: the outline and its connection points come from `stencil`; the
/// frame, text, data, layers, parent, element, links and hand formatting stay; connectors glued
/// to a connection point move to the nearest point of the new outline.
fn change_shape(s: &mut Session, v: &Value) -> CmdResult {
    let stencil = p::req_str(v, "stencil")?;
    let master = vizcraft_stencils::master(stencil).ok_or_else(|| EngineError::Params(format!("unknown stencil `{}`", short(stencil))))?;
    let ids = targets(s, v, Kinds::Shapes)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    if let Some(g) = ids.iter().find(|id| page.shape(id).is_some_and(arrange::is_group)) {
        return Err(EngineError::Params(format!("`{}` is a group; change the shapes inside it", short(g))));
    }
    super::layer::ensure_unlocked(s, &ids)?;
    let page = page_mut(s)?;
    let mut remap: Vec<(String, Vec<Point>, Vec<Point>)> = Vec::new();
    for id in &ids {
        let Some(sh) = page.shape_mut(id) else { continue };
        let old_points = sh.connection_points();
        let was = vizcraft_stencils::master(&sh.stencil).map(|m| m.style.clone()).unwrap_or(Style::DEFAULT);
        sh.style = restyle_for_master(&sh.style, &was, &master.style);
        sh.stencil = master.id.to_string();
        sh.kind = master.kind;
        sh.connection_points = master.kind.connection_points();
        sh.sanitize();
        remap.push((id.clone(), old_points, sh.connection_points()));
    }
    for c in &mut page.connectors {
        for end in [&mut c.start, &mut c.end] {
            if let End::Glued { shape, point, .. } = end
                && let Some(i) = *point
                && let Some((_, old, new)) = remap.iter().find(|(id, ..)| id == shape)
            {
                // A glued point with no equivalent (an outline without points) becomes dynamic glue.
                *point = old.get(i).and_then(|p| nearest_point(new, *p));
            }
        }
    }
    with_sel(s, json!({"changed": ids}))
}

/// Select every item of one kind on the page: `shapes` (not groups, nor members of a group),
/// `connectors`, `groups` (outermost), `text` (shapes and connectors with text) or `layer`
/// (shapes on the layer named by `layer`, by id or name). Hidden and locked shapes are left out.
fn select_by_type(s: &mut Session, v: &Value) -> CmdResult {
    let kind = p::req_str(v, "kind")?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let in_group = |sh: &Shape| sh.parent.as_deref().and_then(|p| page.shape(p)).is_some_and(arrange::is_group);
    let ids: Vec<String> = match kind {
        "shapes" => page.shapes.iter().filter(|x| !arrange::is_group(x) && !in_group(x)).map(|x| x.id.clone()).collect(),
        "connectors" => page.connectors.iter().map(|c| c.id.clone()).collect(),
        "groups" => page.shapes.iter().filter(|x| arrange::is_group(x) && !in_group(x)).map(|x| x.id.clone()).collect(),
        "text" => page
            .shapes
            .iter()
            .filter(|x| !x.text.trim().is_empty())
            .map(|x| x.id.clone())
            .chain(page.connectors.iter().filter(|c| !c.text.trim().is_empty()).map(|c| c.id.clone()))
            .collect(),
        "layer" => {
            let want = p::req_str(v, "layer")?;
            let l = page
                .layers
                .iter()
                .find(|l| l.id == want || l.name == want)
                .ok_or_else(|| EngineError::Params(format!("no layer `{}` on this page", short(want))))?;
            page.shapes.iter().filter(|x| x.layers.contains(&l.id)).map(|x| x.id.clone()).collect()
        }
        other => return Err(EngineError::Params(format!("`kind` must be shapes, connectors, groups, text or layer, not `{}`", short(other)))),
    };
    let ids: Vec<String> = ids.into_iter().filter(|id| page.shape(id).is_none_or(|sh| layer::selectable(page, sh))).collect();
    select(s, ids);
    sel_result(s)
}
