//! Keyboard shortcuts, from the registry's `shortcut` strings and [`crate::UI_COMMANDS`].
//!
//! A shortcut is written portably: `Mod+S`, `Mod+Shift+Z`, `Alt+F4`, `F1`. `Mod` is Cmd on macOS
//! and Ctrl elsewhere (egui's `command` modifier). Alternatives are separated by ` / `.

use egui::{Key, Modifiers};
use serde_json::json;

use crate::{CraftApp, UI_COMMANDS};

/// The registry's name for a key, or `None` for keys no shortcut uses.
pub fn key_name(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::Minus => "-",
        Key::Equals | Key::Plus => "=",
        Key::Comma => ",",
        Key::Period => ".",
        Key::Slash => "/",
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

/// The command bound to a shortcut string: engine first, then UI commands.
pub fn command_for(app: &CraftApp, combo: &str) -> Option<&'static str> {
    if let Some(spec) = app.session.registry.by_shortcut(combo) {
        return Some(spec.id);
    }
    UI_COMMANDS.iter().find(|(_, _, sc, _)| sc.split(" / ").any(|k| k.eq_ignore_ascii_case(combo))).map(|(id, ..)| *id)
}

/// Shortcuts egui's text fields handle themselves; while one is focused they stay with it.
fn text_field_owns(id: &str) -> bool {
    matches!(id, "edit.cut" | "edit.copy" | "edit.paste" | "edit.selectAll")
}

/// Run the commands bound to this frame's key presses and consume those keys, so a text field
/// does not also act on them. Called from `logic`, before layout.
pub fn shortcuts(app: &mut CraftApp, ctx: &egui::Context) {
    let text_focused = ctx.egui_wants_keyboard_input();
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
        // Plain keys and Shift+letter are typing; only modifier combos and function keys are shortcuts.
        let is_fkey =
            matches!(key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12);
        if !(m.command || m.ctrl || m.alt || is_fkey) {
            continue;
        }
        let Some(name) = key_name(key) else { continue };
        let Some(id) = command_for(app, &combo(m, name)) else { continue };
        if text_focused && text_field_owns(id) {
            continue;
        }
        ctx.input_mut(|i| i.consume_key(m, key));
        if id == "file.new" {
            // Cmd+N opens a fresh window instead of swapping the document in front of you.
            app.new_window("blank");
            continue;
        }
        match app.run(id, json!({})) {
            // Copy and cut reach the system clipboard through egui.
            Ok(v) if matches!(id, "edit.copy" | "edit.cut") => {
                if let Some(t) = v.get("text").and_then(serde_json::Value::as_str)
                    && !t.is_empty()
                {
                    ctx.copy_text(t.to_string());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_match_registry_spelling() {
        assert_eq!(combo(Modifiers::COMMAND, "S"), "Mod+S");
        assert_eq!(combo(Modifiers::COMMAND | Modifiers::SHIFT, "Z"), "Mod+Shift+Z");
        assert_eq!(combo(Modifiers::ALT, "F4"), "Alt+F4");
        assert_eq!(combo(Modifiers::NONE, "F1"), "F1");
        assert_eq!(key_name(Key::Equals), Some("="));
        assert_eq!(key_name(Key::ArrowLeft), Some("Left"));
        assert_eq!(key_name(Key::Num0), Some("0"));
    }

    #[test]
    fn every_registered_shortcut_is_reachable_from_a_key() {
        let app = CraftApp::new(basecraft_engine::Session::new(basecraft_engine::Document::new()), crate::Services::default());
        let all: Vec<(&str, &str)> = app
            .session
            .registry
            .all()
            .iter()
            .map(|c| (c.id, c.shortcut))
            .chain(UI_COMMANDS.iter().map(|(id, _, sc, _)| (*id, *sc)))
            .filter(|(_, sc)| !sc.is_empty())
            .collect();
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
                    Key::Num0,
                    Key::F1,
                    Key::A,
                    Key::C,
                    Key::N,
                    Key::O,
                    Key::P,
                    Key::Q,
                    Key::S,
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
}
