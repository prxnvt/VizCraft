//! Home > Clipboard: paste (with Paste Special), cut, copy, the format painter and delete (the
//! last two share a row of icons).

use serde_json::json;

use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};
use crate::widgets::toggle;
use crate::{CraftApp, Tool};

/// The Home tab's Clipboard group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Clipboard",
        vec![
            Split {
                id: "edit.paste",
                icon: "paste",
                big: true,
                menu: vec![MenuEntry::new("Paste", "edit.paste", json!({})), MenuEntry::new("Paste Special...", "edit.pasteSpecial", json!({}))],
            },
            Stack(vec![
                Small { id: "edit.cut", icon: "cut" }, Small { id: "edit.copy", icon: "copy" },
                Row(vec![
                    Custom(format_painter),
                    Button { id: "edit.delete", icon: "delete", label: "Delete", size: Size::Icon, params: |_| json!({}), checked: None },
                ]),
            ]),
        ],
    )
}

/// The Format Painter button: a click arms the painter for one click on the canvas, a
/// double-click keeps it armed until Escape. It copies from the primary selection, so it is
/// disabled with nothing selected; drawn checked while armed.
fn format_painter(app: &mut CraftApp, ui: &mut egui::Ui) {
    let armed = matches!(app.tool, Tool::FormatPainter { .. });
    let can = armed || !app.session.sel.items.is_empty();
    ui.add_enabled_ui(can, |ui| {
        let params = if armed { json!({"tool": "pointer"}) } else { json!({"tool": "formatPainter"}) };
        let resp = toggle(ui, app, "fill", None, "Format Painter", "ui.tool", params, armed);
        if resp.double_clicked() {
            // The first click of the double-click armed it for one use (or disarmed it again).
            let _ = app.run("ui.tool", json!({"tool": "formatPainter", "keep": true}));
        }
    });
}
