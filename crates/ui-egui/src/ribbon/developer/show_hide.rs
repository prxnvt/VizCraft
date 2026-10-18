//! Developer > Show/Hide: the Document Stencil pane and the Drawing Explorer (the Navigation
//! pane opened on the document's tree of diagrams).

use egui::Ui;
use serde_json::json;

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::Group;
use crate::widgets;

/// The Drawing Explorer button: opens the Navigation pane, then points its tree at the
/// diagrams branch. `ui.tree` may not exist in this build yet; then only the pane opens.
fn drawing_explorer(app: &mut CraftApp, ui: &mut Ui) {
    let r = widgets::big(ui, app, "drawingExplorer", &widgets::big_label("Drawing Explorer"), "ui.pane.navigation", json!({"value": true}));
    if r.clicked() {
        let _ = app.run("ui.tree", json!({"branch": "diagrams"}));
    }
}

/// The Developer tab's Show/Hide group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show/Hide",
        vec![
            Stack(vec![Toggle { id: "ui.pane.documentStencil", icon: "stencilPane", checked: |a| a.ui.panes.is_open("documentStencil") }]),
            Custom(drawing_explorer),
        ],
    )
}
