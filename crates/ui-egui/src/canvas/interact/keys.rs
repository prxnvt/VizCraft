//! Canvas keys: Escape, the arrow-key nudge and Alt+Shift+Arrow, which types a chain of steps.
//! The engineering bare-letter keys (R, X, Y, W, P) are shortcuts of `ui.*` commands and go
//! through `crate::keys`.

use egui::{Key, Modifiers};
use serde_json::{Value, json};

use vizcraft_diagram as dg;
use vizcraft_engine::suggest::Direction;

use super::run;
use crate::{CraftApp, menus};

/// Canvas keys: Escape returns the Wire tool to the pointer, lets go of a held net highlight, or
/// cancels a drag or clears the selection; arrows nudge the selection by one
/// unit (ten with Shift) through `shape.move`; Alt+Shift+Arrow with one shape selected runs
/// `insert.next` that way with the first suggestion, which selects the new shape, so the next
/// press carries the chain on. Nothing fires while a dialog, a menu or a text field has the
/// keyboard.
pub fn keys(app: &mut CraftApp, ctx: &egui::Context) {
    // The insert picker and the insertion popover own the keys while open.
    let insertion_open = app.insertion.picker.is_some() || app.insertion.popover.is_some();
    if app.dialog.is_some()
        || menus::is_open(ctx)
        || ctx.egui_wants_keyboard_input()
        || app.canvas.editing.is_some()
        || app.backstage_open()
        || insertion_open
    {
        return;
    }
    let presses: Vec<(Key, Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    let (mut dx, mut dy) = (0.0, 0.0);
    for (key, m) in presses {
        if m.alt
            && m.shift
            && !m.command
            && !m.ctrl
            && let Some(dir) = arrow_direction(key)
        {
            ctx.input_mut(|i| i.consume_key(m, key));
            chain(app, dir);
            continue;
        }
        if m.command || m.alt || m.ctrl {
            continue;
        }
        let step = if m.shift { 10.0 } else { 1.0 };
        match key {
            Key::Escape => {
                ctx.input_mut(|i| i.consume_key(m, key));
                // Escape first disarms the format painter (kept armed by a double-click) and
                // ends a wire drag; the Wire tool itself goes back to the pointer.
                if let crate::Tool::FormatPainter { .. } = app.tool {
                    app.tool = crate::Tool::Pointer;
                    app.canvas.painter_from = None;
                } else if app.tool == crate::Tool::Steps {
                    app.tool = crate::Tool::Pointer;
                } else if let crate::Tool::Wire { .. } | crate::Tool::Ladder = app.tool {
                    app.canvas.drag = None;
                    app.tool = crate::Tool::Pointer;
                } else if app.canvas.net_held.take().is_some() {
                    // A held net highlight lets go before the selection does.
                } else if app.canvas.drag.take().is_none() && !app.session.sel.items.is_empty() {
                    run(app, "edit.selectNone", json!({}));
                }
            }
            Key::ArrowLeft => dx -= step,
            Key::ArrowRight => dx += step,
            Key::ArrowUp => dy -= step,
            Key::ArrowDown => dy += step,
            _ => continue,
        }
        if matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown) {
            ctx.input_mut(|i| i.consume_key(m, key));
        }
    }
    if (dx != 0.0 || dy != 0.0) && !app.session.sel.items.is_empty() {
        run(app, "shape.move", json!({"dx": dx, "dy": dy}));
    }
}

/// The insertion direction an arrow key names.
fn arrow_direction(key: Key) -> Option<Direction> {
    match key {
        Key::ArrowLeft => Some(Direction::Left),
        Key::ArrowRight => Some(Direction::Right),
        Key::ArrowUp => Some(Direction::Up),
        Key::ArrowDown => Some(Direction::Down),
        _ => None,
    }
}

/// One step of a typed chain from the single selected shape.
fn chain(app: &mut CraftApp, dir: Direction) {
    let Some(page) = app.session.doc.page() else { return };
    let shapes: Vec<&String> = app.session.sel.items.iter().filter(|id| page.shape(id).is_some()).collect();
    let [one] = shapes.as_slice() else {
        app.status("Select one shape to continue from".to_string());
        return;
    };
    let id = (*one).clone();
    run(app, "insert.next", json!({"id": id, "direction": dir.name()}));
}

/// The insertion direction named by a `ui.insertNext` or `ui.insertDirection` param: `auto` is the
/// notation's flow ([`crate::notation_layout::flow_direction`] resolves it), the rest are fixed.
fn direction_param(app: &CraftApp, p: &Value) -> Result<Direction, String> {
    match p.get("direction").and_then(Value::as_str) {
        None | Some("auto") => Ok(crate::notation_layout::flow_direction(app)),
        Some(d) => Direction::parse(d).ok_or_else(|| "`direction` must be auto, right, down, left or up".to_string()),
    }
}

/// The one selected shape, else a status line saying what to select.
fn single_selected_shape(app: &mut CraftApp) -> Option<String> {
    let page = app.session.doc.page()?;
    let shapes: Vec<&String> = app.session.sel.items.iter().filter(|id| page.shape(id).is_some()).collect();
    if let [one] = shapes.as_slice() {
        return Some((*one).clone());
    }
    app.status("Select one shape to continue from".to_string());
    None
}

/// Start naming the shape an insertion returned: typing replaces its (empty) text and Enter
/// commits. Insert and name are two undo steps ("Insert Next Step", "Edit Text").
fn name_new_shape(app: &mut CraftApp, result: &Value) {
    if let Some(shape) = result.get("shape").and_then(Value::as_str) {
        super::edit_text::start_editing(app, dg::Hit::Shape(shape.to_string()));
    }
}

/// Tab: with one shape selected, insert the legal next element in the flow direction and start
/// naming it; Tab again continues the chain from the new shape. Two undo steps (the insert, then
/// the name). `direction` overrides the flow direction. Nothing happens on a sequence page.
pub fn insert_next(app: &mut CraftApp, direction: Option<Direction>) -> Result<Value, String> {
    if crate::notation_layout::sequence_page(app) {
        return Ok(json!({"done": false, "reason": "sequence page"}));
    }
    if crate::notation_layout::wiring_page(app) {
        return Ok(json!({"done": false, "reason": "electrical page"}));
    }
    let Some(id) = single_selected_shape(app) else { return Ok(json!({"done": false})) };
    let dir = direction.unwrap_or_else(|| crate::notation_layout::flow_direction(app));
    let r = app.run("insert.next", json!({"id": id, "direction": dir.name()}))?;
    name_new_shape(app, &r);
    Ok(r)
}

/// Enter: with one shape selected, add a sibling (a new step from the same predecessor, same
/// type and relationship) and start naming it; with no predecessor, edit the shape's text; with
/// one connector selected, edit its label. Nothing happens on a sequence page.
pub fn insert_sibling(app: &mut CraftApp) -> Result<Value, String> {
    if crate::notation_layout::sequence_page(app) {
        return Ok(json!({"done": false, "reason": "sequence page"}));
    }
    if crate::notation_layout::wiring_page(app) {
        return Ok(json!({"done": false, "reason": "electrical page"}));
    }
    let Some(page) = app.session.doc.page() else { return Ok(json!({"done": false})) };
    if let [one] = app.session.sel.items.as_slice()
        && page.connector(one).is_some()
    {
        let id = one.clone();
        super::edit_text::start_editing(app, dg::Hit::Connector(id));
        return Ok(json!({"done": true, "editing": true}));
    }
    let Some(id) = single_selected_shape(app) else { return Ok(json!({"done": false})) };
    let Some(page) = app.session.doc.page() else { return Ok(json!({"done": false})) };
    let incoming = page.connectors.iter().find(|c| c.end.shape() == Some(id.as_str()) && c.start.shape().is_some_and(|s| s != id));
    let Some(incoming) = incoming else {
        super::edit_text::start_editing(app, dg::Hit::Shape(id));
        return Ok(json!({"done": true, "editing": true}));
    };
    let source = incoming.start.shape().unwrap_or_default().to_string();
    let relationship = incoming.relationship.as_deref().and_then(|r| app.session.doc.model.relationship(r)).map(|r| r.type_id.clone());
    let shape = page.shape(&id);
    let ty = shape
        .and_then(|s| vizcraft_engine::suggest::element_type_of(&app.session.doc, s).map(str::to_string))
        .or_else(|| shape.map(vizcraft_engine::suggest::stencil_of));
    let dir = crate::notation_layout::flow_direction(app);
    let mut params = json!({"id": source, "direction": dir.name()});
    if let (Some(o), Some(ty)) = (params.as_object_mut(), ty) {
        o.insert("type".into(), json!(ty));
        if let Some(r) = relationship {
            o.insert("relationship".into(), json!(r));
        }
    }
    let r = app.run("insert.next", params)?;
    name_new_shape(app, &r);
    Ok(r)
}

/// `ui.insertNext`, `ui.insertSibling`, `ui.editText` and `ui.insertDirection`; `None` for any
/// other id.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    Some(match id {
        "ui.insertNext" => direction_param(app, p).and_then(|d| insert_next(app, Some(d))),
        "ui.insertSibling" => insert_sibling(app),
        "ui.editText" => edit_text_command(app, p),
        "ui.insertDirection" => {
            let d = p.get("direction").and_then(Value::as_str).unwrap_or("auto");
            if d != "auto" && Direction::parse(d).is_none() {
                return Some(Err("`direction` must be auto, right, down, left or up".to_string()));
            }
            app.ui.insert_direction = d.to_string();
            Ok(json!({"direction": d, "resolved": crate::notation_layout::flow_direction(app).name()}))
        }
        _ => return None,
    })
}

/// `ui.editText {id?}`: edit the text of shape or connector `id`, else of the selected one.
fn edit_text_command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let page = app.session.doc.page().ok_or("the document has no page")?;
    let id = match p.get("id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => match app.session.sel.items.as_slice() {
            [one] => one.clone(),
            _ => {
                app.status("Select one shape or connector to edit".to_string());
                return Ok(json!({"editing": false}));
            }
        },
    };
    let target = if page.shape(&id).is_some() {
        dg::Hit::Shape(id.clone())
    } else if page.connector(&id).is_some() {
        dg::Hit::Connector(id.clone())
    } else {
        return Ok(json!({"editing": false, "reason": "not a shape or connector on this page"}));
    };
    super::edit_text::start_editing(app, target);
    Ok(json!({"editing": app.canvas.editing.is_some(), "id": id}))
}
