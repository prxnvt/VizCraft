//! Keyboard shortcuts, from the registry's `shortcut` strings and [`crate::UI_COMMANDS`].
//!
//! A shortcut is written portably: `Mod+S`, `Mod+Shift+Z`, `Alt+F4`, `F1`. `Mod` is Cmd on Apple
//! macOS and Ctrl elsewhere (egui's `command` modifier). Alternatives are separated by ` / `.
//!
//! Precedence, from the top:
//! 1. An open dialog owns the keyboard: Enter and Escape are its, and no shortcut fires until it
//!    closes (the Quit prompt must not be undone from under itself).
//! 2. An open in-window menu owns the keyboard the same way (arrows, Enter, Escape walk it).
//! 3. A focused text field keeps the editing keys egui implements for it: Mod+X, Mod+C, Mod+V,
//!    Mod+A, Mod+Z and Mod+Shift+Z act on the field, not the document.
//! 4. Everything else is looked up in the engine registry first, then in `UI_COMMANDS`. A key
//!    bound to a disabled command is swallowed and does nothing, so the field under it does not
//!    see a key the menu says belongs to a command.
//!
//! Only modifier combinations and function keys are shortcuts; plain keys and Shift+letter are
//! typing. Bare Delete and Backspace are the exception: they delete the canvas selection, but
//! only while no text field has the keyboard (in a field they delete characters). Canvas keys
//! (the engineering letters R, X, Y, W and P, as in electrical CAD, and Tab, Enter and `/`) are shortcuts only while the
//! canvas has the keyboard ([`bare_letters_ok`]): no widget is focused, nothing is being edited
//! in place, and no dialog, menu, File page, insert picker or presentation is up. On macOS with the native menu bar the host feeds menu key equivalents back through
//! egui so this rule applies there too.

use egui::{Key, Modifiers};

use crate::{CraftApp, UI_COMMANDS, menus};

/// The registry's name for a key, or `None` for keys no shortcut uses.
pub fn key_name(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::Minus => "-",
        Key::Equals | Key::Plus => "=",
        Key::Comma => ",",
        Key::Period => ".",
        Key::Slash | Key::Questionmark => "/",
        Key::Semicolon => ";",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backslash => "\\",
        Key::Backtick => "`",
        Key::Quote => "'",
        // Letters, digits, arrows, F-keys and editing keys are named as egui names them.
        other => other.name(),
    })
}

/// The portable shortcut string for this key and modifiers.
pub fn combo(m: Modifiers, key: &str) -> String {
    let mut s = String::new();
    if m.command {
        s.push_str("Mod+");
    }
    if m.ctrl && !m.command || m.mac_cmd && m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    s.push_str(key);
    s
}

/// A portable shortcut string (all alternatives) as the platform shows it: glyphs on macOS
/// (`Cmd Shift Z` as symbols), `Ctrl+Shift+Z` elsewhere; alternatives joined with " or ".
pub fn pretty(sc: &str) -> String {
    sc.split(" / ")
        .filter(|a| !a.is_empty())
        .map(|a| {
            if cfg!(target_os = "macos") {
                a.replace("Mod+", "\u{2318}").replace("Shift+", "\u{21e7}").replace("Alt+", "\u{2325}").replace("Ctrl+", "\u{2303}")
            } else {
                a.replace("Mod+", "Ctrl+")
            }
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

/// The command bound to a shortcut string: engine first, then UI commands.
pub fn command_for(app: &CraftApp, combo: &str) -> Option<&'static str> {
    if let Some(spec) = app.session.registry.by_shortcut(combo) {
        return Some(spec.id);
    }
    UI_COMMANDS.iter().find(|(_, _, sc, _)| sc.split(" / ").any(|k| k.eq_ignore_ascii_case(combo))).map(|(id, ..)| *id)
}

/// Shortcuts egui's text fields handle themselves; while one is focused they stay with it. The
/// sequence moves are Alt+arrows, which move the caret by word in a field.
fn text_field_owns(id: &str) -> bool {
    matches!(
        id,
        "edit.cut"
            | "edit.copy"
            | "edit.paste"
            | "edit.selectAll"
            | "edit.undo"
            | "edit.redo"
            | "sequence.moveUp"
            | "sequence.moveDown"
            | "sequence.moveLeft"
            | "sequence.moveRight"
    )
}

/// Whether `key` is a letter A to Z.
fn is_letter(key: Key) -> bool {
    key.name().len() == 1 && key.name().chars().all(|c| c.is_ascii_uppercase())
}

/// Whether canvas keys (bare letters, Tab, Enter and `/`) may fire this frame: the canvas has the keyboard. No widget is
/// focused (a text field, a pane's search, the command palette's field), no shape or label is
/// being edited in place, and no dialog, in-window menu, File page, insert picker or popover,
/// or presentation owns the keys.
pub fn bare_letters_ok(app: &CraftApp, ctx: &egui::Context) -> bool {
    !(ctx.egui_wants_keyboard_input()
        || ctx.memory(|m| m.focused().is_some())
        || app.dialog.is_some()
        || menus::is_open(ctx)
        || app.canvas.editing.is_some()
        || app.backstage_open()
        || app.presenting
        || app.insertion.picker.is_some()
        || app.insertion.popover.is_some())
}

/// Whether a key press counts as a shortcut at all: a modifier combination, a function key,
/// bare Delete / Backspace while no text field is focused, or a canvas key (a bare letter, Tab, Enter or `/`) while the canvas has
/// the keyboard (`canvas_keys`, see [`bare_letters_ok`]).
fn is_shortcut_key(key: Key, m: Modifiers, text_focused: bool, canvas_keys: bool) -> bool {
    if matches!(key, Key::Delete | Key::Backspace) && !text_focused {
        return true;
    }
    if canvas_keys
        && !text_focused
        && (is_letter(key) || matches!(key, Key::Tab | Key::Enter | Key::Slash))
        && !(m.shift || m.alt || m.ctrl || m.command || m.mac_cmd)
    {
        return true;
    }
    let fkey = matches!(
        key,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
            | Key::F13
            | Key::F14
            | Key::F15
            | Key::F16
            | Key::F17
            | Key::F18
            | Key::F19
            | Key::F20
    );
    // Bare Space is not a shortcut: it is held to pan the canvas and types a space in fields.
    m.command || m.ctrl || m.alt || fkey
}

/// Run the commands bound to this frame's key presses and consume those keys, so a text field
/// does not also act on them. Called from `logic`, before layout.
pub fn shortcuts(app: &mut CraftApp, ctx: &egui::Context) {
    // Rules 1 and 2: a dialog or an open menu has the keyboard.
    if app.dialog.is_some() || menus::is_open(ctx) {
        return;
    }
    let text_focused = ctx.egui_wants_keyboard_input();
    let canvas_keys = bare_letters_ok(app, ctx);
    let presses: Vec<(Key, Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    for (key, m) in presses {
        if !is_shortcut_key(key, m, text_focused, canvas_keys) {
            continue;
        }
        let Some(name) = key_name(key) else { continue };
        let Some(id) = command_for(app, &combo(m, name)) else { continue };
        // Rule 3: the field keeps its editing keys and normal typing.
        if text_focused && text_field_owns(id) {
            continue;
        }
        ctx.input_mut(|i| i.consume_key(m, key));
        // Rule 4: a disabled command swallows its key and does nothing.
        if !menus::entry(app, id).enabled {
            continue;
        }
        menus::run_item(app, ctx, id, serde_json::json!({}));
        // The `/` that opened the picker also arrives as typed text, in this frame or the next;
        // it must not land in the query.
        if key == Key::Slash
            && let Some(pk) = app.insertion.picker.as_mut()
        {
            pk.skip_slash = 2;
        }
    }
}

/// Every command with a shortcut, as `(label, keys)` in registry order then UI order, for the
/// Keyboard Shortcuts dialog. Keys are platform text ([`pretty`]).
pub fn shortcut_table(app: &CraftApp) -> Vec<(String, String)> {
    let engine = app.session.registry.all().iter().map(|c| (c.id, c.label, c.shortcut));
    let ui = UI_COMMANDS.iter().map(|(id, label, sc, _)| (*id, *label, *sc));
    engine
        .chain(ui)
        .filter(|(_, _, sc)| !sc.is_empty())
        .map(|(id, label, sc)| {
            // The menu's wording where the command has one (Open..., Save As...).
            let menu_label = menus::entry(app, id).label;
            let label = if menu_label.is_empty() { label.to_string() } else { menu_label };
            (label, pretty(sc))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default())
    }

    #[test]
    fn combos_match_registry_spelling() {
        assert_eq!(combo(Modifiers::COMMAND, "S"), "Mod+S");
        assert_eq!(combo(Modifiers::COMMAND | Modifiers::SHIFT, "Z"), "Mod+Shift+Z");
        assert_eq!(combo(Modifiers::ALT, "F4"), "Alt+F4");
        assert_eq!(combo(Modifiers::NONE, "F1"), "F1");
        assert_eq!(key_name(Key::Equals), Some("="));
        assert_eq!(key_name(Key::ArrowLeft), Some("Left"));
        assert_eq!(key_name(Key::Num0), Some("0"));
        assert_eq!(key_name(Key::Space), Some("Space"));
        assert!(is_shortcut_key(Key::F5, Modifiers::NONE, false, false));
        assert!(!is_shortcut_key(Key::Space, Modifiers::NONE, false, true), "bare Space pans the canvas");
        assert!(is_shortcut_key(Key::P, Modifiers::COMMAND | Modifiers::SHIFT, false, false));
        assert!(!is_shortcut_key(Key::A, Modifiers::SHIFT, false, true), "Shift+letter is typing");
        assert!(is_shortcut_key(Key::Delete, Modifiers::NONE, false, false), "Delete deletes the selection");
        assert!(is_shortcut_key(Key::Backspace, Modifiers::NONE, false, false));
        assert!(!is_shortcut_key(Key::Backspace, Modifiers::NONE, true, false), "in a text field Backspace edits the text");
        assert_eq!(command_for(&app(), "Delete"), Some("edit.delete"));
        // Bare letters: only while the canvas has the keyboard, never while a field is focused.
        assert!(is_shortcut_key(Key::R, Modifiers::NONE, false, true));
        assert!(!is_shortcut_key(Key::R, Modifiers::NONE, false, false), "typing elsewhere");
        assert!(!is_shortcut_key(Key::R, Modifiers::NONE, true, true), "a focused text field types the letter");
        assert!(!is_shortcut_key(Key::Num1, Modifiers::NONE, false, true), "only letters");
        assert!(!is_shortcut_key(Key::Escape, Modifiers::NONE, false, true));
        for k in [Key::Tab, Key::Enter, Key::Slash] {
            assert!(is_shortcut_key(k, Modifiers::NONE, false, true), "{k:?} is a canvas key");
            assert!(!is_shortcut_key(k, Modifiers::NONE, false, false), "{k:?} types elsewhere");
            assert!(!is_shortcut_key(k, Modifiers::NONE, true, true), "{k:?} stays with a focused field");
        }
        assert!(!is_shortcut_key(Key::Tab, Modifiers::SHIFT, false, true), "Shift+Tab walks focus");
        assert_eq!(key_name(Key::Tab), Some("Tab"));
        assert_eq!(key_name(Key::Enter), Some("Enter"));
        for (k, id) in [("R", "ui.rotateQuarter"), ("X", "ui.mirrorX"), ("Y", "ui.mirrorY"), ("W", "ui.wire"), ("P", "ui.pipe")] {
            assert_eq!(command_for(&app(), k), Some(id), "{k}");
        }
        for (k, id) in [("Tab", "ui.insertNext"), ("Enter", "ui.insertSibling"), ("/", "ui.insertPicker"), ("F2", "ui.editText")] {
            assert_eq!(command_for(&app(), k), Some(id), "{k}");
        }
    }

    #[test]
    fn pretty_uses_platform_glyphs() {
        let p = pretty("Mod+Shift+Z / Mod+Y");
        if cfg!(target_os = "macos") {
            assert_eq!(p, "\u{2318}\u{21e7}Z or \u{2318}Y");
        } else {
            assert_eq!(p, "Ctrl+Shift+Z or Ctrl+Y");
        }
        assert_eq!(pretty(""), "");
    }

    #[test]
    fn every_registered_shortcut_is_reachable_from_a_key() {
        let app = app();
        let all: Vec<(&str, &str)> = app
            .session
            .registry
            .all()
            .iter()
            .map(|c| (c.id, c.shortcut))
            .chain(UI_COMMANDS.iter().map(|(id, _, sc, _)| (*id, *sc)))
            .filter(|(_, sc)| !sc.is_empty())
            .collect();
        assert!(!all.is_empty());
        for (id, sc) in all {
            for alt in sc.split(" / ") {
                let key = alt.rsplit('+').next().unwrap();
                // Each key name must be one `key_name` can produce, otherwise the shortcut never fires.
                let found = [
                    Key::Minus,
                    Key::Equals,
                    Key::Comma,
                    Key::Period,
                    Key::Slash,
                    Key::Tab,
                    Key::Enter,
                    Key::Num0,
                    Key::F1,
                    Key::F2,
                    Key::F7,
                    Key::F12,
                    Key::Space,
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::Delete,
                    Key::Backspace,
                    Key::A,
                    Key::B,
                    Key::C,
                    Key::D,
                    Key::E,
                    Key::F,
                    Key::G,
                    Key::H,
                    Key::I,
                    Key::J,
                    Key::K,
                    Key::L,
                    Key::M,
                    Key::N,
                    Key::O,
                    Key::P,
                    Key::Q,
                    Key::R,
                    Key::S,
                    Key::T,
                    Key::U,
                    Key::V,
                    Key::W,
                    Key::X,
                    Key::Y,
                    Key::Z,
                ]
                .into_iter()
                .any(|k| key_name(k) == Some(key));
                assert!(found, "{id}: shortcut {alt} ends in a key no event produces");
                assert_eq!(command_for(&app, alt), Some(id), "{id}: {alt} resolves to the wrong command");
            }
        }
    }

    #[test]
    fn shortcut_table_lists_labels_and_keys() {
        let app = app();
        let table = shortcut_table(&app);
        assert!(table.iter().any(|(l, k)| l == "Save" && !k.is_empty()));
        assert!(table.iter().any(|(l, _)| l == "Open..."), "menu wording wins: {table:?}");
        assert!(table.iter().all(|(_, k)| !k.contains("Mod+")), "portable spelling leaks: {table:?}");
    }
}
