//! The net highlight: hovering a wire highlights its whole net on the page (through
//! `Session.nets`), and a click on a wire holds the highlight until a click elsewhere or Escape.
//! Drawn by `draw::nets::highlight`; the status bar names the net and counts its pins
//! ([`status`]).

use vizcraft_diagram::Hit;

use crate::CraftApp;

/// The net of the wire under the pointer, if any.
fn under_pointer(app: &CraftApp) -> Option<String> {
    let Some(Hit::Connector(id)) = &app.canvas.hover else { return None };
    app.session.nets.net_of_connector(id).map(str::to_string)
}

/// Update the hovered net for this frame; a held net that no longer exists is let go.
pub fn hover(app: &mut CraftApp) {
    app.canvas.net_hover = under_pointer(app);
    if app.canvas.net_held.as_ref().is_some_and(|n| !app.session.nets.nets.contains_key(n)) {
        app.canvas.net_held = None;
    }
}

/// A click on the canvas: on a wire it holds that wire's net, anywhere else it lets go.
pub fn click(app: &mut CraftApp) {
    app.canvas.net_held = under_pointer(app);
}

/// The status bar text for the highlighted net: its number (else name, else id) and its pin
/// count, as `Net 2003, 3 pins`. `None` when no net is highlighted.
pub fn status(app: &CraftApp) -> Option<String> {
    let id = app.canvas.net_held.as_deref().or(app.canvas.net_hover.as_deref())?;
    let entry = app.session.nets.nets.get(id)?;
    let name = app.session.doc.model.nets.iter().find(|n| n.id == id).map(|n| n.label_text().to_string()).filter(|t| !t.is_empty());
    let pins = entry.pins.len();
    Some(format!("Net {}, {pins} pin{}", name.as_deref().unwrap_or(id), if pins == 1 { "" } else { "s" }))
}
