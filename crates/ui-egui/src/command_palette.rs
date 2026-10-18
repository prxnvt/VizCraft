//! The command palette (`ui.commands`, Mod+Shift+P or Mod+K): one search box over the engine's
//! `suggest::search`, which covers every command (the registry and [`UI_COMMANDS`]), the element
//! types of the packs in use and the model's elements by name or tag. Enter or a click acts on
//! the hit: a command runs, a type is inserted at the centre of the view, an element is
//! selected and revealed (switching page when it is drawn on another one).
//!
//! The modal frame is `dialogs::show`; this module is the body of `Dialog::Commands`.

use egui::{Align2, Sense, Ui, vec2};
use serde_json::json;
use vizcraft_engine::suggest::{self, CommandRow, HitKind, SearchHit};

use crate::theme::{Tokens, regular};
use crate::{CraftApp, UI_COMMANDS, keys, menus, widgets};

/// Most hits the palette lists.
pub const MAX_HITS: usize = 40;

/// The keys the dialog read for this frame before any field consumed them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keys {
    /// Enter: act on the highlighted hit.
    pub enter: bool,
    /// Arrow up: highlight the previous hit.
    pub up: bool,
    /// Arrow down: highlight the next hit.
    pub down: bool,
}

/// One palette row.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// What the row is and what acting on it does.
    pub kind: HitKind,
    /// Command id, element type id or element id.
    pub id: String,
    /// Label.
    pub label: String,
    /// Location (commands) or the search's detail line.
    pub location: String,
    /// Platform shortcut text (commands only).
    pub shortcut: String,
    /// Whether acting on it does anything now.
    pub enabled: bool,
    /// For an element: the page index and shape that draw it.
    pub view: Option<(usize, String)>,
}

/// The rows matching `query`, best first (see `suggest::search`), at most [`MAX_HITS`].
pub fn hits(app: &CraftApp, query: &str) -> Vec<Hit> {
    let ui_rows = UI_COMMANDS.iter().map(|(id, label, sc, _)| CommandRow { id, label, location: "UI", shortcut: sc });
    let rows: Vec<CommandRow<'_>> = app
        .session
        .registry
        .all()
        .iter()
        .map(|c| CommandRow { id: c.id, label: c.label, location: c.location, shortcut: c.shortcut })
        .chain(ui_rows)
        .collect();
    suggest::search(&app.session.doc, &rows, query, &[], MAX_HITS).into_iter().map(|h| row(app, &rows, h)).collect()
}

/// A search hit as the palette shows it.
fn row(app: &CraftApp, rows: &[CommandRow<'_>], h: SearchHit) -> Hit {
    match h.kind {
        HitKind::Command => {
            let r = rows.iter().find(|r| r.id == h.id);
            let enabled = !menus::PLACEHOLDERS.contains(&h.id.as_str()) && widgets::enabled(app, &h.id);
            Hit {
                kind: h.kind,
                location: r.map(|r| r.location.to_string()).unwrap_or_default(),
                shortcut: keys::pretty(r.map(|r| r.shortcut).unwrap_or("")),
                enabled,
                view: None,
                id: h.id,
                label: h.label,
            }
        }
        HitKind::Type => Hit { kind: h.kind, id: h.id, label: h.label, location: h.detail, shortcut: "Insert".into(), enabled: true, view: None },
        HitKind::Element => {
            let view = h.page.zip(h.shape);
            Hit { kind: h.kind, enabled: view.is_some(), id: h.id, label: h.label, location: h.detail, shortcut: "Element".into(), view }
        }
    }
}

/// Act on a hit: run the command, insert the type at the centre of the view, or select and
/// reveal the element's shape.
pub fn act(app: &mut CraftApp, ctx: &egui::Context, h: &Hit) {
    match h.kind {
        HitKind::Command => menus::run_item(app, ctx, &h.id, json!({})),
        HitKind::Type => {
            if let Some(master) = vizcraft_notation::catalog().master_for_type(&h.id) {
                let at = crate::canvas::insert_point(app);
                crate::canvas::insert_master(app, master, at);
            }
        }
        HitKind::Element => {
            let Some((page, shape)) = &h.view else { return };
            if app.session.doc.active_index() != *page {
                let _ = app.run("page.setActive", json!({"page": page}));
            }
            let _ = app.run("edit.select", json!({"ids": [shape]}));
            // Reveal: centre the view on it when it is out of sight.
            let centre = app.session.doc.page().and_then(|p| p.shape(shape)).map(|s| s.bounds());
            let visible = app.canvas.viewport(&app.session.view).map(|vp| vp.visible_world_rect());
            if let Some(b) = centre
                && visible.is_none_or(|v| !v.contains_rect(&b))
            {
                let c = b.center();
                let _ = app.run("view.pan", json!({"x": c.x, "y": c.y}));
            }
        }
    }
}

/// The palette body. Returns true to close.
pub fn show(app: &mut CraftApp, ui: &mut Ui, query: &mut String, selected: &mut usize, keys: Keys) -> bool {
    let t = Tokens::get(ui.ctx());
    let r = ui.add(
        egui::TextEdit::singleline(query)
            .hint_text("Type a command, a shape type or an element, e.g. \"zoom\"")
            .desired_width(f32::INFINITY)
            .font(regular(14.0)),
    );
    if r.changed() {
        *selected = 0;
    }
    r.request_focus();
    let hits = hits(app, query);
    if hits.is_empty() {
        *selected = 0;
    } else {
        if keys.down {
            *selected = (*selected + 1).min(hits.len() - 1);
        }
        if keys.up {
            *selected = selected.saturating_sub(1);
        }
        *selected = (*selected).min(hits.len() - 1);
    }
    let mut chosen: Option<usize> = None;
    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
        for (k, h) in hits.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, h.enabled, k == *selected, h.label.as_str()));
            if k == *selected {
                ui.painter().rect_filled(rect, 4.0, if h.enabled { t.checked } else { t.hover });
                if keys.down || keys.up {
                    ui.scroll_to_rect(rect, None);
                }
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 4.0, t.hover);
            }
            let color = if h.enabled { t.text } else { t.text_disabled };
            let g = ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &h.label, regular(12.5), color);
            ui.painter().text(g.right_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &h.location, regular(11.0), t.text_dim);
            if !h.shortcut.is_empty() {
                ui.painter().text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, &h.shortcut, regular(12.0), t.text_dim);
            }
            let resp = resp.on_hover_text(&h.id);
            if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                *selected = k;
            }
            if resp.clicked() && h.enabled {
                chosen = Some(k);
            }
        }
        if hits.is_empty() {
            ui.label(egui::RichText::new("Nothing matches.").color(t.text_dim));
        }
    });
    if keys.enter && hits.get(*selected).is_some_and(|h| h.enabled) {
        chosen = Some(*selected);
    }
    let Some(h) = chosen.and_then(|k| hits.get(k)) else { return false };
    let ctx = ui.ctx().clone();
    act(app, &ctx, h);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default())
    }

    #[test]
    fn palette_ranks_prefix_matches_first_and_marks_placeholders() {
        let a = app();
        let hits = hits(&a, "zoom");
        assert!(hits.first().is_some_and(|h| h.label.to_lowercase().starts_with("zoom")), "{:?}", hits.iter().map(|h| &h.label).collect::<Vec<_>>());
        let print = super::hits(&a, "print");
        assert!(print.iter().any(|h| h.id == "ui.print" && h.enabled));
        let find = super::hits(&a, "find");
        assert!(find.iter().any(|h| h.id == "ui.find" && h.enabled), "Find opens the Find dialog");
        let minimize = super::hits(&a, "minimize");
        assert!(minimize.iter().any(|h| h.id == "window.minimize" && !h.enabled), "placeholders show disabled");
        assert!(super::hits(&a, "").len() <= MAX_HITS);
    }

    #[test]
    fn types_and_elements_are_listed_next_to_commands() {
        let mut a = app();
        a.run("element.create", json!({"type": "flowchart.Decision", "name": "Approved?", "tag": "Q-1", "x": 0, "y": 0})).unwrap();
        let h = hits(&a, "q-1");
        assert!(h.iter().any(|h| h.kind == HitKind::Element && h.label == "Approved?" && h.view.is_some()));
        let h = hits(&a, "terminator");
        assert!(h.iter().any(|h| h.kind == HitKind::Type && h.id == "flowchart.Terminator"));
    }
}
