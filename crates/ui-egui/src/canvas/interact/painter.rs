//! The format painter tool: armed from Home > Clipboard (or `ui.tool {"tool": "formatPainter"}`)
//! with a source item, it runs `edit.formatPainter` on the item clicked next. One click disarms
//! it unless it was armed to stay (a double-click on the button); Escape always does.

use serde_json::json;
use vizcraft_diagram::Hit;

use crate::{CraftApp, Tool};

/// Arm the painter with `from` (default: the primary selection), staying armed when `keep`.
/// Returns the tool to set, or why it cannot be armed.
pub fn arm(app: &mut CraftApp, from: Option<&str>, keep: bool) -> Result<Tool, String> {
    let from = match from {
        Some(f) => f.to_string(),
        None => {
            let sel = &app.session.sel;
            sel.primary.clone().or_else(|| sel.items.first().cloned()).ok_or("select the shape to copy the format from")?
        }
    };
    if !app.session.doc.page().is_some_and(|p| p.contains_id(&from)) {
        return Err(format!("`{}` is not a shape or connector on this page", vizcraft_engine::cmd::short(&from)));
    }
    app.canvas.painter_from = Some(from);
    Ok(Tool::FormatPainter { keep })
}

/// A click with the painter armed: paint the item under the pointer, then disarm unless `keep`
/// (a click on empty space paints nothing).
pub(super) fn apply(app: &mut CraftApp, hit: Option<Hit>, keep: bool) {
    let target = match hit {
        Some(Hit::Shape(id) | Hit::Connector(id)) => Some(id),
        None => None,
    };
    if let (Some(from), Some(to)) = (app.canvas.painter_from.clone(), target)
        && from != to
    {
        super::run(app, "edit.formatPainter", json!({"from": from, "to": [to]}));
    }
    if !keep {
        app.tool = Tool::Pointer;
        app.canvas.painter_from = None;
    }
}
