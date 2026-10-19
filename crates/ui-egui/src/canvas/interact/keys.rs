//! Canvas keys: Escape, the arrow-key nudge and Alt+Shift+Arrow, which types a chain of steps.
//! The engineering bare-letter keys (R, X, Y, W, P) are shortcuts of `ui.*` commands and go
//! through `crate::keys`.

use egui::{Key, Modifiers};
use serde_json::json;

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
                } else if let crate::Tool::Wire { .. } = app.tool {
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
