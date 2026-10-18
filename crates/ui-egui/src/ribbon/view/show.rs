//! View > Show: rulers, page breaks, grid, guides, connection points, the side panes and the
//! ribbon.

use crate::ribbon::Control::*;
use crate::CraftApp;
use crate::ribbon::Group;
use crate::widgets::{menu_button, menu_item};

/// The View tab's Show group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Show",
        vec![
            Stack(vec![
                Toggle { id: "view.rulers", icon: "ruler", checked: |a| a.session.view.aids.rulers },
                Toggle { id: "view.pageBreaks", icon: "pageBreaks", checked: |a| a.session.view.aids.page_breaks },
                Toggle { id: "view.connectionPoints", icon: "connectionPoints", checked: |a| a.session.view.aids.connection_points },
            ]),
            Stack(vec![
                Toggle { id: "view.grid", icon: "grid", checked: |a| a.session.view.aids.grid },
                Toggle { id: "view.guides", icon: "guides", checked: |a| a.session.view.aids.guides },
                Custom(task_panes),
            ]),
            Stack(vec![
                Toggle { id: "view.navigationPane", icon: "panelLeft", checked: |a| a.session.view.nav_pane },
                Toggle { id: "view.propertiesPane", icon: "panelRight", checked: |a| a.session.view.props_pane },
                Toggle { id: "ui.collapseRibbon", icon: "ribbon", checked: |a| a.ui.ribbon_collapsed },
                Toggle { id: "ui.dock", icon: "dock", checked: |a| a.ui.dock.open },
            ]),
        ],
    )
}

/// Task Panes: every pane with a check mark while it is open (`panes::window_menu_items`).
fn task_panes(app: &mut CraftApp, ui: &mut egui::Ui) {
    menu_button(ui, app, "taskPanes", "Task Panes", "Task Panes", false, |ui, app| {
        for (title, id, open) in crate::panes::window_menu_items(app) {
            menu_item(ui, app, title, id, serde_json::json!({}), Some(open));
        }
    });
}
