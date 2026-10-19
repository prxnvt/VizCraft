//! Design > Layout: re-layout the page and route connectors around shapes.
//!
//! Re-Layout is a split button. Its menu starts with the layouts the page's notation uses (the
//! distinct layouts of its viewpoint's templates, under a heading with the viewpoint's name), then
//! the generic list. The main click runs the first notation layout, else layered top to bottom.
//! Insert Direction chooses which way Tab, Enter and the picker grow a chain.

use egui::{RichText, Ui};
use serde_json::json;

use crate::CraftApp;
use crate::notation_layout::{self, auto_direction};
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry};
use crate::widgets::{menu_button, menu_item, split};

/// The generic Re-Layout list: each algorithm in each direction it honours (radial and force have
/// none). No entry declares a check: `layout.full` is a one-shot arrangement that records nothing
/// on the page (`Page.view.layout` is written by no command), so there is no current choice to show.
fn generic_menu() -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::new();
    for (algorithm, names) in [
        ("layered", ["Layered, Top to Bottom", "Layered, Left to Right", "Layered, Bottom to Top", "Layered, Right to Left"]),
        ("tree", ["Tree, Top to Bottom", "Tree, Left to Right", "Tree, Bottom to Top", "Tree, Right to Left"]),
    ] {
        for (name, direction) in names.into_iter().zip(["TB", "LR", "BT", "RL"]) {
            out.push(MenuEntry::new(name, "layout.full", json!({"algorithm": algorithm, "direction": direction})));
        }
    }
    out.push(MenuEntry::new("Radial", "layout.full", json!({"algorithm": "radial"})));
    out.push(MenuEntry::new("Force-Directed", "layout.full", json!({"algorithm": "force"})));
    out
}

/// The Re-Layout split button.
fn relayout(app: &mut CraftApp, ui: &mut Ui) {
    let choices = notation_layout::layout_choices(app);
    let (id, params) = match choices.first() {
        Some(c) => (c.command, c.params.clone()),
        None => ("layout.full", json!({"algorithm": "layered", "direction": "TB"})),
    };
    let heading = notation_layout::viewpoint_label(app);
    split(ui, app, "layout", "Re-Layout", id, params, true, move |ui, app| {
        if let (Some(h), false) = (&heading, choices.is_empty()) {
            ui.label(RichText::new(h).strong());
            for c in &choices {
                menu_item(ui, app, &c.label, c.command, c.params.clone(), None);
            }
            ui.separator();
        }
        for e in generic_menu() {
            menu_item(ui, app, e.label, e.id, e.params.clone(), None);
        }
    });
}

/// The Insert Direction menu: Auto (naming what it resolves to here), Right, Down, Left, Up.
fn insert_direction(app: &mut CraftApp, ui: &mut Ui) {
    let auto = format!("Auto ({})", auto_direction(app).name());
    menu_button(ui, app, "arrowEnd", "Insert Direction", "Insert Direction", false, move |ui, app| {
        let now = app.ui.insert_direction.clone();
        let current = if now.is_empty() { "auto" } else { now.as_str() };
        let rows = [("auto", auto.as_str()), ("right", "Right"), ("down", "Down"), ("left", "Left"), ("up", "Up")];
        for (value, label) in rows {
            menu_item(ui, app, label, "ui.insertDirection", json!({"direction": value}), Some(current == value));
        }
    });
}

/// The Design tab's Layout group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Layout", vec![Custom(relayout), Big { id: "layout.route", icon: "routeAround" }, Stack(vec![Custom(insert_direction)])])
}
