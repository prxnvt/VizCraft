//! Canvas keys: Escape and the arrow-key nudge.

use egui::{Key, Modifiers};
use serde_json::json;

use super::run;
use crate::{CraftApp, menus};

/// Canvas keys: Escape cancels a drag or clears the selection; arrows nudge the selection by one
/// unit (ten with Shift) through `shape.move`. Nothing fires while a dialog, a menu or a text
/// field has the keyboard.
pub fn keys(app: &mut CraftApp, ctx: &egui::Context) {
    if app.dialog.is_some() || menus::is_open(ctx) || ctx.egui_wants_keyboard_input() || app.canvas.editing.is_some() || app.backstage_open() {
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
        if m.command || m.alt || m.ctrl {
            continue;
        }
        let step = if m.shift { 10.0 } else { 1.0 };
        match key {
            Key::Escape => {
                ctx.input_mut(|i| i.consume_key(m, key));
                // Escape first disarms the format painter (kept armed by a double-click).
                if let crate::Tool::FormatPainter { .. } = app.tool {
                    app.tool = crate::Tool::Pointer;
                    app.canvas.painter_from = None;
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
