//! Sequence > Fragments: Wrap (the selected messages, notes and fragments in a fragment of the
//! chosen operator, `sequence.wrap`), Add Operand (a new operand of the enclosing fragment starting
//! at the selected block, `sequence.addOperand`) and Unwrap (`sequence.unwrap` on the selected
//! fragment, else the one around the selection). The engine's refusal (a gap between the blocks,
//! another nesting level) goes to the status bar.

use egui::Ui;
use serde_json::json;
use vizcraft_diagram::sequence::{self as seq, Operator, Part};

use super::{events, selected_blocks};
use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};
use crate::widgets::{menu_button, menu_item};

/// The fragment Unwrap removes: the selected fragment, else the one enclosing the selection.
fn unwrap_target(app: &CraftApp) -> Option<String> {
    let page = app.session.doc.page()?;
    let blocks = selected_blocks(app);
    blocks
        .iter()
        .find(|id| page.shape(id).is_some_and(|s| matches!(s.sequence, Some(Part::Fragment { .. }))))
        .cloned()
        .or_else(|| blocks.first().and_then(|b| seq::enclosing(&events(app), b)).map(|(f, _)| f))
}

/// The Wrap menu: one row per operator in catalog order, each wrapping the selection.
fn wrap_menu(app: &mut CraftApp, ui: &mut Ui) {
    menu_button(ui, app, "fragment", "Wrap", "Wrap in Fragment", true, |ui, app| {
        let ids = selected_blocks(app);
        for op in Operator::ALL {
            menu_item(ui, app, op.name(), "sequence.wrap", json!({"ids": ids, "operator": op.name()}), None);
        }
    });
}

/// The Sequence tab's Fragments group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Fragments",
        vec![
            Custom(wrap_menu),
            Stack(vec![
                Button {
                    id: "sequence.addOperand",
                    icon: "operand",
                    label: "Add Operand",
                    size: Size::Small,
                    params: |a| {
                        let b = selected_blocks(a).into_iter().next();
                        let f = b.as_deref().and_then(|b| seq::enclosing(&events(a), b)).map(|(f, _)| f);
                        json!({"id": f, "before": b})
                    },
                    checked: None,
                },
                Button { id: "sequence.unwrap", icon: "disband", label: "Unwrap", size: Size::Small, params: |a| json!({"id": unwrap_target(a)}), checked: None },
            ]),
        ],
    )
}
