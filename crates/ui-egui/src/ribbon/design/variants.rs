//! Design > Variants: the four colour variants of the active theme.

use egui::{Ui, vec2};
use serde_json::json;
use vizcraft_diagram::theme::{self, DocTheme, VARIANTS};

use super::themes::{cell, preview};
use crate::CraftApp;
use crate::ribbon::{Control, Group};

fn gallery(app: &mut CraftApp, ui: &mut Ui) {
    let current = app.session.doc.theme.clone();
    // With no theme the cells preview the first theme's variants, disabled.
    let shown = current.theme().or_else(|| theme::themes().first()).map(|t| t.id).unwrap_or_default();
    let mut chosen = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for row in [0..2, 2..VARIANTS] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for i in row {
                    let t = DocTheme::new(shown, i);
                    let name = t.colors().map(|v| v.name).unwrap_or_default();
                    let selected = !current.id.is_empty() && usize::from(current.variant) == i;
                    if cell(ui, app, "design.setVariant", name, selected, |p, r| preview(p, r, &t)) {
                        chosen = Some(i);
                    }
                }
            });
        }
    });
    if let Some(i) = chosen {
        let _ = app.run("design.setVariant", json!({"variant": i}));
    }
}

/// The Design tab's Variants group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new("Variants", vec![Control::Custom(gallery)])
}
