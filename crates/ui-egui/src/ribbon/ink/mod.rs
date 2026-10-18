//! The contextual Ink tab, shown while an ink tool is active or a stroke is selected: the pens,
//! their colour and thickness, conversion to shapes and leaving the ink tools. Review > Ink offers
//! the same controls ([`tools`], [`format`]).

use serde_json::json;

use super::Tab;
use crate::canvas::interact::ink::{WIDTHS, settings};
use crate::ribbon::Control::{self, *};
use crate::ribbon::{MenuEntry, Size, selected_shape};
use crate::{CraftApp, Tool};

groups! {
    write,
    pens,
    convert,
}

fn ink_active(app: &CraftApp) -> bool {
    matches!(app.tool, Tool::Pen | Tool::Highlighter | Tool::Eraser) || selected_shape(app).is_some_and(vizcraft_diagram::ink::is_stroke)
}

/// The Ink tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Ink", ink_active, groups())
}

/// A `ui.tool` button for `tool`, checked while it is the tool; clicking the checked one goes
/// back to the pointer.
macro_rules! tool_button {
    ($tool:ident, $name:literal, $icon:literal, $label:literal) => {
        Button {
            id: "ui.tool",
            icon: $icon,
            label: $label,
            size: Size::Small,
            params: |a| json!({"tool": if a.tool == Tool::$tool { "pointer" } else { $name }}),
            checked: Some(|a| a.tool == Tool::$tool),
        }
    };
}

/// Pen, Highlighter, Eraser and Lasso Select.
pub(in crate::ribbon) fn tools() -> Vec<Control> {
    vec![
        Stack(vec![tool_button!(Pen, "pen", "pen", "Pen"), tool_button!(Highlighter, "highlighter", "highlighter", "Highlighter")]),
        Stack(vec![tool_button!(Eraser, "eraser", "eraser", "Eraser"), tool_button!(Lasso, "lasso", "lasso", "Lasso Select")]),
    ]
}

/// The colour the Colour menu shows as current: the highlighter's while it is the tool.
fn current_color(app: &CraftApp) -> String {
    let s = settings();
    if app.tool == Tool::Highlighter { s.highlighter.to_hex() } else { s.color.to_hex() }
}

/// A `ui.inkPen` colour, checked while it is the current one.
macro_rules! ink_color {
    ($label:literal, $hex:literal) => {
        MenuEntry::new($label, "ui.inkPen", json!({ "color": $hex })).checked(|a| current_color(a) == $hex)
    };
}

/// A `ui.inkPen` thickness, checked while it is the pen's.
macro_rules! thickness {
    ($label:literal, $i:literal) => {
        MenuEntry::new($label, "ui.inkPen", json!({ "width": WIDTHS[$i] })).checked(|_| settings().width == WIDTHS[$i])
    };
}

/// The Colour and Thickness menus.
pub(in crate::ribbon) fn format() -> Vec<Control> {
    vec![Stack(vec![
        Menu {
            label: "Colour",
            icon: "inkColor",
            big: false,
            menu: vec![
                ink_color!("Ink Blue", "#1f2a44"),
                ink_color!("Black", "#000000"),
                ink_color!("Red", "#d0312d"),
                ink_color!("Blue", "#2f6fd0"),
                ink_color!("Green", "#2e8b57"),
                ink_color!("Yellow", "#ffd82e"),
                ink_color!("Pink", "#ff7ab8"),
            ],
        },
        Menu {
            label: "Thickness",
            icon: "inkThickness",
            big: false,
            menu: vec![thickness!("1 px", 0), thickness!("2 px", 1), thickness!("3.5 px", 2), thickness!("5 px", 3), thickness!("8 px", 4)],
        },
    ])]
}
