//! The Insert Symbol dialog: a grid of common symbols, Greek letters, arrows and maths signs,
//! plus the ones used recently. It opens from `text.insertSymbol` with no symbol (Insert > Text >
//! Symbol). A click inserts the symbol at the in-place editor's caret when text is being edited;
//! otherwise it runs `text.insertSymbol` on the selection (at the caret where the last edit of
//! the selected item stopped, else at the end), so an agent gets the same result without it.

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::json;

use crate::CraftApp;
use crate::theme::{Tokens, regular};

/// The symbol sets, in tab order (Recent comes last and is filled as symbols are used).
pub const SETS: &[(&str, &str)] = &[
    (
        "Common",
        "\u{a9}\u{ae}\u{2122}\u{b0}\u{b1}\u{d7}\u{f7}\u{b5}\u{b6}\u{a7}\u{2026}\u{2022}\u{2013}\u{2014}\u{20ac}\u{a3}\u{a5}\u{a2}\u{bd}\u{bc}\u{be}\u{b9}\u{b2}\u{b3}\u{2020}\u{2021}\u{2030}\u{a1}\u{bf}\u{ab}\u{bb}\u{2018}\u{2019}\u{201c}\u{201d}\u{2713}\u{2717}\u{2605}\u{2606}\u{25cf}\u{25cb}\u{25a0}\u{25a1}\u{25b2}\u{25bc}",
    ),
    (
        "Greek",
        "\u{391}\u{392}\u{393}\u{394}\u{395}\u{396}\u{397}\u{398}\u{399}\u{39a}\u{39b}\u{39c}\u{39d}\u{39e}\u{39f}\u{3a0}\u{3a1}\u{3a3}\u{3a4}\u{3a5}\u{3a6}\u{3a7}\u{3a8}\u{3a9}\u{3b1}\u{3b2}\u{3b3}\u{3b4}\u{3b5}\u{3b6}\u{3b7}\u{3b8}\u{3b9}\u{3ba}\u{3bb}\u{3bc}\u{3bd}\u{3be}\u{3bf}\u{3c0}\u{3c1}\u{3c3}\u{3c4}\u{3c5}\u{3c6}\u{3c7}\u{3c8}\u{3c9}",
    ),
    (
        "Arrows",
        "\u{2190}\u{2191}\u{2192}\u{2193}\u{2194}\u{2195}\u{2196}\u{2197}\u{2198}\u{2199}\u{21d0}\u{21d1}\u{21d2}\u{21d3}\u{21d4}\u{21d5}\u{21a9}\u{21aa}\u{21ba}\u{21bb}\u{21c4}\u{21c6}\u{27f5}\u{27f6}\u{27f7}\u{21b5}\u{21b6}\u{21b7}\u{2934}\u{2935}\u{2794}\u{25b2}\u{25bc}\u{25c0}\u{25b6}\u{25ba}\u{25c4}",
    ),
    (
        "Maths",
        "\u{2264}\u{2265}\u{2260}\u{2248}\u{2261}\u{221e}\u{221a}\u{2211}\u{220f}\u{222b}\u{2202}\u{2206}\u{2207}\u{2208}\u{2209}\u{2229}\u{222a}\u{2227}\u{2228}\u{ac}\u{2200}\u{2203}\u{2205}\u{2282}\u{2283}\u{2286}\u{2287}\u{b7}\u{2032}\u{2033}\u{221d}\u{2234}\u{2235}\u{2220}\u{22a5}\u{2295}\u{2297}\u{2213}\u{b1}\u{d7}\u{f7}\u{3c0}\u{b5}\u{2030}\u{bd}\u{bc}\u{be}\u{2044}\u{207a}\u{207b}\u{2080}\u{2081}\u{2082}",
    ),
];

/// Most recent symbols kept.
pub const MAX_RECENT: usize = 24;

/// The dialog's state: the tab shown.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Index into [`SETS`]; `SETS.len()` is Recent.
    pub tab: usize,
}

/// Recently inserted symbols, newest first, kept in the egui context for the session.
fn recent(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp::<Vec<String>>(egui::Id::new("vizcraft-recent-symbols"))).unwrap_or_default()
}

fn remember(ctx: &egui::Context, symbol: &str) {
    let mut r = recent(ctx);
    r.retain(|s| s != symbol);
    r.insert(0, symbol.to_string());
    r.truncate(MAX_RECENT);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("vizcraft-recent-symbols"), r));
}

/// The symbols of `set` the app's fonts can draw (the rest would show as empty boxes).
pub fn drawable(ctx: &egui::Context, font: &egui::FontId, set: &str) -> Vec<String> {
    ctx.fonts_mut(|f| set.chars().filter(|c| f.has_glyph(font, *c)).map(String::from).collect())
}

/// Insert `symbol`: at the editor's caret while editing, else with `text.insertSymbol` on the
/// selection.
pub fn insert(app: &mut CraftApp, symbol: &str) {
    if crate::canvas::interact::insert_at_caret(app, symbol) {
        return;
    }
    let single = match app.session.sel.items.as_slice() {
        [id] => Some(id.clone()),
        _ => None,
    };
    let at = match (&app.canvas.last_caret, &single) {
        (Some((id, c)), Some(sel)) if id == sel => Some(*c),
        _ => None,
    };
    let params = match at {
        Some(c) => json!({"symbol": symbol, "at": c}),
        None => json!({"symbol": symbol}),
    };
    if app.run("text.insertSymbol", params).is_ok()
        && let (Some(c), Some(id)) = (at, single)
    {
        // The next symbol follows this one.
        app.canvas.last_caret = Some((id, c + symbol.chars().count()));
    }
}

/// The tabs, the grid and the Close button. Returns whether to close.
pub fn body(app: &mut CraftApp, ui: &mut Ui, s: &mut State) -> bool {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        for (i, (name, _)) in SETS.iter().enumerate() {
            ui.selectable_value(&mut s.tab, i, *name);
        }
        ui.selectable_value(&mut s.tab, SETS.len(), "Recent");
    });
    let font = regular(18.0);
    let symbols: Vec<String> = match SETS.get(s.tab) {
        Some((_, set)) => drawable(ui.ctx(), &font, set),
        None => recent(ui.ctx()),
    };
    if symbols.is_empty() {
        ui.label(egui::RichText::new("Symbols you insert show here.").color(t.text_dim));
    }
    let mut chosen = None;
    egui::Grid::new("symbol-grid").spacing(vec2(4.0, 4.0)).show(ui, |ui| {
        for (i, sym) in symbols.iter().enumerate() {
            let b = egui::Button::new(egui::RichText::new(sym).font(font.clone()).color(t.text)).min_size(vec2(32.0, 32.0));
            if ui.add(b).on_hover_text(format!("U+{:04X}", sym.chars().next().map_or(0, u32::from))).clicked() {
                chosen = Some(sym.clone());
            }
            if i % 10 == 9 {
                ui.end_row();
            }
        }
    });
    if let Some(sym) = chosen {
        remember(ui.ctx(), &sym);
        insert(app, &sym);
    }
    ui.add_space(8.0);
    let mut close = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        close = crate::widgets::primary_button(ui, "Close").clicked();
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    #[test]
    fn the_command_opens_the_dialog() {
        let mut app = app();
        app.run("text.insertSymbol", json!({})).unwrap();
        assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some("symbol"));
    }

    #[test]
    fn without_an_editor_symbols_go_to_the_selection_at_the_last_caret() {
        let mut app = app();
        let id = app.session.doc.page().unwrap().shapes[0].id.clone();
        app.run("shape.setText", json!({"id": id, "text": "ab"})).unwrap();
        app.run("edit.select", json!({"ids": [id]})).unwrap();
        app.canvas.last_caret = Some((id.clone(), 1));
        insert(&mut app, "\u{3b1}");
        insert(&mut app, "\u{3b2}");
        assert_eq!(app.session.doc.page().unwrap().shape(&id).unwrap().text, "a\u{3b1}\u{3b2}b");
        app.canvas.last_caret = None;
        insert(&mut app, "!");
        assert_eq!(app.session.doc.page().unwrap().shape(&id).unwrap().text, "a\u{3b1}\u{3b2}b!");
    }

    #[test]
    fn every_set_has_drawable_symbols() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            for (name, set) in SETS {
                let n = drawable(ui.ctx(), &regular(18.0), set).len();
                assert!(n >= 20, "{name}: {n} drawable of {}", set.chars().count());
            }
        });
        out.textures_delta.clear();
    }
}
