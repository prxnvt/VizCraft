//! Home > Paragraph: horizontal and vertical alignment, bullets, indent and text direction of the
//! selected shapes' text, all through `text.setParagraph`.

use serde_json::{Value, json};
use vizcraft_diagram::{Bullets, HAlign, TextDirection, VAlign};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Control, Group, MenuEntry, Size, selected_style};

/// Indent added or taken away by one indent step, in world units (a quarter inch).
pub const INDENT_STEP: f64 = 25.0;

fn align(icon: &'static str, label: &'static str, params: fn(&CraftApp) -> Value, checked: fn(&CraftApp) -> bool) -> Control {
    Button { id: "text.setParagraph", icon, label, size: Size::Icon, params, checked: Some(checked) }
}

fn h_is(a: &CraftApp, h: HAlign) -> bool {
    selected_style(a).is_some_and(|s| s.h_align == h)
}

fn v_is(a: &CraftApp, v: VAlign) -> bool {
    selected_style(a).is_some_and(|s| s.v_align == v)
}

/// The indent after one step in or out from the selected style's.
fn indent(a: &CraftApp, by: f64) -> Value {
    let now = selected_style(a).map_or(0.0, |s| s.indent);
    json!({"indent": (now + by).max(0.0)})
}

/// An icon-only dropdown of `text.setParagraph` entries.
fn icon_menu(app: &mut CraftApp, ui: &mut egui::Ui, icon: &str, tip: &str, menu: &[MenuEntry]) {
    crate::widgets::menu_button(ui, app, icon, "", tip, false, |ui, app| {
        for e in menu {
            let checked = e.checked.map(|f| f(app));
            crate::widgets::menu_item(ui, app, e.label, e.id, e.params.clone(), checked);
        }
    });
}

/// A `text.setParagraph` entry, checked while the selected style passes `is`.
fn entry(label: &'static str, params: Value, is: fn(&CraftApp) -> bool) -> MenuEntry {
    MenuEntry::new(label, "text.setParagraph", params).checked(is)
}

fn bullets_are(a: &CraftApp, b: Bullets) -> bool {
    selected_style(a).is_some_and(|s| s.bullets == b)
}

fn direction_is(a: &CraftApp, d: TextDirection) -> bool {
    selected_style(a).is_some_and(|s| s.text_direction == d)
}

fn bullets_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    let entries = [
        entry("None", json!({"bullets": "none"}), |a| bullets_are(a, Bullets::None)),
        entry("Bullets", json!({"bullets": "disc"}), |a| bullets_are(a, Bullets::Disc)),
        entry("Dashes", json!({"bullets": "dash"}), |a| bullets_are(a, Bullets::Dash)),
        entry("Numbering", json!({"bullets": "number"}), |a| bullets_are(a, Bullets::Number)),
    ];
    icon_menu(app, ui, "bullets", "Bullets", &entries);
}

fn direction_menu(app: &mut CraftApp, ui: &mut egui::Ui) {
    let entries = [
        entry("Left to Right", json!({"direction": "ltr"}), |a| direction_is(a, TextDirection::LeftToRight)),
        entry("Right to Left", json!({"direction": "rtl"}), |a| direction_is(a, TextDirection::RightToLeft)),
        entry("Vertical", json!({"direction": "vertical"}), |a| direction_is(a, TextDirection::Vertical)),
    ];
    icon_menu(app, ui, "textDirection", "Text Direction", &entries);
}

/// The Home tab's Paragraph group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Paragraph",
        vec![Stack(vec![
            Row(vec![
                align("alignLeft", "Align Left", |_| json!({"hAlign": "left"}), |a| h_is(a, HAlign::Left)),
                align("alignCenter", "Center", |_| json!({"hAlign": "center"}), |a| h_is(a, HAlign::Center)),
                align("alignRight", "Align Right", |_| json!({"hAlign": "right"}), |a| h_is(a, HAlign::Right)),
                align("justify", "Justify", |_| json!({"hAlign": "justify"}), |a| h_is(a, HAlign::Justify)),
            ]),
            Row(vec![
                align("alignTop", "Align Top", |_| json!({"vAlign": "top"}), |a| v_is(a, VAlign::Top)),
                align("alignMiddle", "Align Middle", |_| json!({"vAlign": "middle"}), |a| v_is(a, VAlign::Middle)),
                align("alignBottom", "Align Bottom", |_| json!({"vAlign": "bottom"}), |a| v_is(a, VAlign::Bottom)),
                Custom(bullets_menu),
            ]),
            Row(vec![
                Button { id: "text.setParagraph", icon: "indentLess", label: "Decrease Indent", size: Size::Icon, params: |a| indent(a, -INDENT_STEP), checked: None },
                Button { id: "text.setParagraph", icon: "indentMore", label: "Increase Indent", size: Size::Icon, params: |a| indent(a, INDENT_STEP), checked: None },
                Custom(direction_menu),
            ]),
        ])],
    )
}
