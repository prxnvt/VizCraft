//! The Activity tab: the transaction log, newest first, with an "Undo to before this" per entry.

use egui::Ui;
use serde_json::json;
use vizcraft_engine::Origin;

use crate::CraftApp;
use crate::theme::Tokens;

fn origin_name(o: Origin) -> &'static str {
    match o {
        Origin::Ui => "ui",
        Origin::Control => "control",
        Origin::Mcp => "mcp",
        Origin::Script => "script",
    }
}

/// How many steps are on the undo stack and on the redo stack, counted once per revision.
fn depths(app: &mut CraftApp) -> (usize, usize) {
    let rev = app.session.rev();
    if let Some((r, u, d)) = app.ui.dock.state.depths
        && r == rev
    {
        return (u, d);
    }
    let stats = app.session.memory_stats();
    let n = |k: &str| stats.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
    let (u, d) = (n("undoSteps"), n("redoSteps"));
    app.ui.dock.state.depths = Some((rev, u, d));
    (u, d)
}

/// The Activity tab.
pub(super) fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (undo, redo) = depths(app);
    let log: Vec<_> = app.session.transactions().iter().cloned().collect();
    if log.is_empty() {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("No changes yet").color(t.text_dim));
        return;
    }
    let len = log.len();
    // The log keeps undone steps too (the last `redo` entries); the entries on the undo stack are
    // the `undo` before them. An entry can be undone back to only while it is one of those.
    let top = len.saturating_sub(redo);
    let floor = top.saturating_sub(undo);
    let mut undo_n = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, rec) in log.iter().enumerate().rev() {
            let undone = i >= top;
            let reachable = i < top && i >= floor;
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let label = egui::RichText::new(&rec.label);
                ui.label(if undone { label.color(t.text_disabled).strikethrough() } else { label.strong() });
                ui.label(egui::RichText::new(origin_name(rec.origin)).small().monospace().color(t.accent_text));
                let n = rec.commands.len();
                ui.label(egui::RichText::new(format!("{n} {}", if n == 1 { "command" } else { "commands" })).small().color(t.text_dim));
                if undone {
                    ui.label(egui::RichText::new("undone").small().color(t.text_dim));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if ui.add_enabled(reachable, egui::Button::new("Undo to before this")).clicked() {
                        undo_n = Some(top - i);
                    }
                });
            });
            if !rec.commands.is_empty() {
                egui::CollapsingHeader::new(egui::RichText::new("Commands").small()).id_salt(("activity", i)).show(ui, |ui| {
                    for c in &rec.commands {
                        ui.label(egui::RichText::new(c).monospace().small());
                    }
                });
            }
            ui.separator();
        }
    });
    if let Some(n) = undo_n {
        for _ in 0..n {
            if app.run("edit.undo", json!({})).is_err() {
                break;
            }
        }
    }
}
