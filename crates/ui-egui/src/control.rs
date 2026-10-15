//! Programmatic control of the running app (agents, tests, the CLI's `--connect` later).
//!
//! Methods (JSON lines over the host's transport, see `docs/control-protocol.md`):
//! - `engine.execute {command, params}`: run any command (engine, `ui.*` or `app.*`)
//! - `engine.commands`: every command with its enablement
//! - `ui.inspect`: UI state, view, dialog, window size, perf
//! - `ui.set {path, value}`: set one field of the UI state by JSON pointer (`/tab`, `/panes/leftWidth`)
//! - `ui.click {x, y, button?, count?, shift?, cmd?}`, `ui.move {x, y}`, `ui.drag {x, y, toX, toY}`:
//!   real pointer input in screen points (reaches every widget)
//! - `ui.key {key, shift?, alt?, cmd?}`, `ui.text {text}`: keyboard input through egui
//! - `ui.screenshot {path?}`: PNG of the window
//! - `ui.resize {width, height}`, `ui.focus`
//! - `app.quit`: quit without asking about unsaved changes
//!
//! Anything else is taken as a command id.

use std::sync::mpsc::Sender;

use serde_json::{Value, json};

use crate::{CraftApp, UiState};

/// One request from the transport thread; the reply goes back through `reply`.
pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Value>,
}

impl ControlRequest {
    /// A request and the receiver its reply arrives on.
    pub fn new(method: impl Into<String>, params: Value) -> (Self, std::sync::mpsc::Receiver<Value>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { method: method.into(), params, reply: tx }, rx)
    }
}

/// What handling a request produced: a reply now, or a screenshot the app replies to once the
/// frame arrives.
pub enum Outcome {
    Done(Value),
    Screenshot { path: Option<String> },
}

fn ok(v: Value) -> Outcome {
    Outcome::Done(json!({"ok": true, "result": v}))
}
fn err(e: impl std::fmt::Display) -> Outcome {
    Outcome::Done(json!({"ok": false, "error": e.to_string()}))
}

/// The `ui.inspect` report.
pub fn inspect(app: &CraftApp, ctx: &egui::Context) -> Value {
    let r = ctx.content_rect();
    json!({
        "ui": app.prefs(),
        "view": app.session.view,
        "selection": app.session.sel,
        "dialog": app.dialog.as_ref().map(|d| serde_json::to_value(d).unwrap_or_default()),
        "backstage": app.backstage_open(),
        "window": [r.width(), r.height()],
        "title": app.title_stem(),
        "path": app.session.path,
        "dirty": app.session.dirty,
        "notes": app.session.doc.notes.len(),
        "canUndo": app.session.can_undo(),
        "canRedo": app.session.can_redo(),
        "perf": {"frameMs": app.frame_ms},
    })
}

/// `ui.set`: replace the field at `pointer` in the UI state and re-apply it.
pub fn set(app: &mut CraftApp, pointer: &str, value: Value) -> Result<Value, String> {
    let pointer = if pointer.starts_with('/') { pointer.to_string() } else { format!("/{pointer}") };
    let mut state = serde_json::to_value(app.prefs()).map_err(|e| e.to_string())?;
    let slot = state.pointer_mut(&pointer).ok_or_else(|| format!("no UI state field at {pointer}"))?;
    *slot = value;
    let ui: UiState = serde_json::from_value(state).map_err(|e| format!("{pointer}: {e}"))?;
    app.apply_prefs(ui);
    serde_json::to_value(app.prefs()).map_err(|e| e.to_string())
}

fn key_from(name: &str) -> Option<egui::Key> {
    egui::Key::from_name(name).or(match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(egui::Key::Enter),
        "esc" | "escape" => Some(egui::Key::Escape),
        "delete" => Some(egui::Key::Delete),
        "backspace" => Some(egui::Key::Backspace),
        "left" => Some(egui::Key::ArrowLeft),
        "right" => Some(egui::Key::ArrowRight),
        "up" => Some(egui::Key::ArrowUp),
        "down" => Some(egui::Key::ArrowDown),
        "space" => Some(egui::Key::Space),
        "tab" => Some(egui::Key::Tab),
        "home" => Some(egui::Key::Home),
        "end" => Some(egui::Key::End),
        "-" | "minus" => Some(egui::Key::Minus),
        "=" | "equals" | "plus" => Some(egui::Key::Equals),
        "," | "comma" => Some(egui::Key::Comma),
        _ => None,
    })
}

fn mods(p: &Value) -> egui::Modifiers {
    let b = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    let cmd = b("cmd") || b("command") || b("ctrl") || b("mod");
    egui::Modifiers {
        alt: b("alt"),
        ctrl: cmd && !cfg!(target_os = "macos"),
        shift: b("shift"),
        mac_cmd: cmd && cfg!(target_os = "macos"),
        command: cmd,
    }
}

fn click_events(app: &mut CraftApp, pos: egui::Pos2, button: egui::PointerButton, count: u64, m: egui::Modifiers) {
    app.synthetic.push(egui::Event::PointerMoved(pos));
    for _ in 0..count.clamp(1, 3) {
        app.synthetic.push(egui::Event::PointerButton { pos, button, pressed: true, modifiers: m });
        app.synthetic.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers: m });
    }
}

/// Handle one request.
pub fn handle(app: &mut CraftApp, ctx: &egui::Context, req: &ControlRequest) -> Outcome {
    let p = &req.params;
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let f = |k: &str| p.get(k).and_then(Value::as_f64).map(|v| v as f32).filter(|v| v.is_finite());
    match req.method.as_str() {
        "engine.execute" | "command" => {
            let Some(id) = s("command").or(s("id")) else { return err("missing `command`") };
            let params = p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
            match app.run(id, params) {
                Ok(v) => ok(v),
                Err(e) => err(e),
            }
        }
        "engine.commands" => {
            let mut list = serde_json::to_value(app.session.commands()).unwrap_or_default();
            if let Some(arr) = list.as_array_mut() {
                for (id, label, shortcut, params) in crate::UI_COMMANDS {
                    arr.push(
                        json!({"id": id, "label": label, "shortcut": shortcut, "location": "UI", "params": params, "enabled": true, "reason": ""}),
                    );
                }
            }
            ok(list)
        }
        "ui.inspect" => ok(inspect(app, ctx)),
        "ui.set" => {
            let Some(path) = s("path") else { return err("missing `path`") };
            match set(app, path, p.get("value").cloned().unwrap_or(Value::Null)) {
                Ok(v) => ok(v),
                Err(e) => err(e),
            }
        }
        "ui.click" => {
            let (Some(x), Some(y)) = (f("x"), f("y")) else { return err("missing x/y") };
            let button = match s("button") {
                Some("right") => egui::PointerButton::Secondary,
                Some("middle") => egui::PointerButton::Middle,
                _ => egui::PointerButton::Primary,
            };
            let count = p.get("count").and_then(Value::as_u64).unwrap_or(1);
            click_events(app, egui::pos2(x, y), button, count, mods(p));
            ok(json!({"queued": true}))
        }
        "ui.move" => {
            let (Some(x), Some(y)) = (f("x"), f("y")) else { return err("missing x/y") };
            app.synthetic.push(egui::Event::PointerMoved(egui::pos2(x, y)));
            ok(json!({"queued": true}))
        }
        "ui.drag" => {
            let (Some(x), Some(y), Some(tx), Some(ty)) = (f("x"), f("y"), f("toX"), f("toY")) else { return err("missing x/y/toX/toY") };
            let m = mods(p);
            let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 100);
            app.synthetic.push(egui::Event::PointerMoved(egui::pos2(x, y)));
            app.synthetic.push(egui::Event::PointerButton {
                pos: egui::pos2(x, y),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: m,
            });
            for i in 1..=steps {
                let k = i as f32 / steps as f32;
                app.synthetic.push(egui::Event::PointerMoved(egui::pos2(x + (tx - x) * k, y + (ty - y) * k)));
            }
            app.synthetic.push(egui::Event::PointerButton {
                pos: egui::pos2(tx, ty),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: m,
            });
            ok(json!({"queued": true}))
        }
        "ui.key" => {
            let Some(k) = s("key").and_then(key_from) else { return err("unknown key") };
            let m = mods(p);
            app.synthetic.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
            app.synthetic.push(egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
            ok(json!({"queued": true}))
        }
        "ui.text" => {
            let Some(t) = s("text") else { return err("missing text") };
            app.synthetic.push(egui::Event::Text(t.to_string()));
            ok(json!({"queued": true}))
        }
        "ui.screenshot" => Outcome::Screenshot { path: s("path").map(str::to_string) },
        "ui.resize" => {
            let (Some(w), Some(h)) = (f("width"), f("height")) else { return err("missing width/height") };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            ok(json!({}))
        }
        "ui.focus" => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ok(json!({}))
        }
        "app.quit" => {
            // Automation quits without asking about unsaved changes.
            app.quit_confirmed = true;
            app.quit_requested = true;
            if app.multi_window {
                app.quit_all_request = true;
            }
            ok(json!({}))
        }
        other => match app.run(other, p.clone()) {
            Ok(v) => ok(v),
            Err(e) => err(e),
        },
    }
}

/// Write a captured frame as PNG; the reply for `ui.screenshot`.
pub fn save_screenshot(image: &egui::ColorImage, path: Option<&str>) -> Value {
    let path = path.unwrap_or("protocraft-screenshot.png");
    let [w, h] = image.size;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) else {
        return json!({"ok": false, "error": "screenshot buffer has the wrong size"});
    };
    if let Err(e) = img.save_with_format(path, image::ImageFormat::Png) {
        return json!({"ok": false, "error": format!("{path}: {e}")});
    }
    json!({"ok": true, "result": {"path": path, "width": w, "height": h}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_set_changes_one_field_and_rejects_bad_ones() {
        let mut app = CraftApp::new(protocraft_engine::Session::new(protocraft_engine::Document::new()), crate::Services::default());
        set(&mut app, "/tab", json!("View")).unwrap();
        assert_eq!(app.ui.tab, "View");
        set(&mut app, "panes/leftWidth", json!(333)).unwrap();
        assert_eq!(app.ui.panes.left_width, 333.0);
        set(&mut app, "/dark", json!(true)).unwrap();
        assert!(app.session.view.dark_mode);
        assert!(set(&mut app, "/nothing", json!(1)).is_err());
        assert!(set(&mut app, "/tab", json!(["not", "a", "string"])).is_err());
        assert_eq!(app.ui.tab, "View", "a failed set leaves the state alone");
    }
}
