//! Programmatic control of the running app (agents, tests, the CLI's `control` subcommand).
//!
//! The host (desktop app, `ui_shot`, a test) turns each line of its transport into a
//! [`ControlRequest`] and sends it to the app; [`CraftApp::logic`] drains them once per frame and
//! answers through [`handle`]. Methods, params and replies are the contract in
//! `docs/control-protocol.md`; this file and that document change together.
//!
//! - `engine.execute {command, params?, origin?}`: run any command (engine, `ui.*` or `app.*`);
//!   `origin: "mcp"` logs it as made by the MCP server, anything else as the control channel
//! - `engine.commands`: every command with its enablement, UI commands included
//! - `ui.inspect`: UI state, view, selection, dialog, window size, title, dirty, rev, perf, tool,
//!   a diagram summary (page, counts, selection, hover, editing, drag) and the open ribbon
//!   dropdown (`menu`: its title and rows with `label`, `id`, `enabled` and `checked`)
//! - `ui.set {path, value}`: set one field of the UI state by dotted path or JSON pointer
//! - `ui.menus`: the menu tree with labels, ids, shortcuts, enablement and check marks
//! - `ui.click`, `ui.move`, `ui.drag`, `ui.key`, `ui.text`: synthetic input, queued for the next frame
//! - `ui.wait {frames?}`: reply after that many frames have run
//! - `ui.screenshot {path?}`: PNG of the window, a few frames later
//! - `ui.resize {width, height}`, `ui.focus`: viewport commands
//! - `app.quit`: quit without asking about unsaved changes
//!
//! A method name that is a command id (`edit.undo`) is the short form of `engine.execute`; any
//! other name is `unknown method`. Params are hostile: every reader tolerates junk and no path
//! panics (see the tests at the bottom and `tests/control.rs`).

use std::sync::mpsc::Sender;

use serde_json::{Value, json};
use vizcraft_engine::Origin;

use crate::{CraftApp, UiState, ViewportCmdExt, menus};

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

/// What handling a request produced: a reply now, or something the app replies to later (a
/// screenshot once the frame arrives, a wait once the frames have run).
pub enum Outcome {
    Done(Value),
    Screenshot { path: Option<String> },
    Wait { frames: u32 },
}

/// Most frames `ui.wait` accepts; more than this is a stuck script, not a wait.
pub const MAX_WAIT_FRAMES: u32 = 10_000;

/// Where `ui.screenshot` writes when the request names no path.
pub fn default_screenshot_path() -> String {
    std::env::temp_dir().join("vizcraft-screenshot.png").to_string_lossy().to_string()
}

fn ok(v: Value) -> Outcome {
    Outcome::Done(json!({"ok": true, "result": v}))
}
fn err(e: impl std::fmt::Display) -> Outcome {
    Outcome::Done(json!({"ok": false, "error": e.to_string()}))
}

/// The `ui.inspect` report: everything a script needs to check its work without a screenshot.
pub fn inspect(app: &CraftApp, ctx: &egui::Context) -> Value {
    let r = ctx.content_rect();
    json!({
        "ui": app.prefs(),
        "view": &app.session.view,
        "selection": &app.session.sel,
        "dialog": app.dialog.as_ref().and_then(|d| serde_json::to_value(d).ok()),
        "backstage": app.backstage_open(),
        "window": [r.width(), r.height()],
        "title": app.title_stem(),
        "path": app.session.path,
        "dirty": app.session.dirty,
        "rev": app.session.rev(),
        "canUndo": app.session.can_undo(),
        "canRedo": app.session.can_redo(),
        "undoLabel": app.session.undo_label(),
        "redoLabel": app.session.redo_label(),
        "status": app.status_msg.as_ref().map(|(s, _)| s.as_str()),
        "perf": {"frameMs": app.frame_ms},
        "tool": app.tool.name(),
        "inspector": {
            "open": app.ui.panes.is_open("shapeData"), "mode": app.ui.inspector.mode, "tab": app.ui.inspector.tab,
            "fields": crate::panes::inspector_fields(app),
        },
        "palette": crate::palette::for_app(app).report(app),
        "diagram": crate::canvas::summary(app),
        "tree": crate::panes::tree_inspect(app),
        "dock": crate::dock::inspect(app),
        "tables": crate::dock::tables::inspect(app),
        "nets": crate::dock::nets::inspect(app),
        "generate": crate::dock::generate::inspect(app),
        "patterns": crate::panes::patterns_inspect(app),
        // The ribbon dropdown that is open, row by row, with each row's check state.
        "menu": crate::widgets::open_menu(ctx),
    })
}

/// `ui.set` paths: a JSON pointer (`/panes/leftWidth`) is used as is; a dotted path
/// (`panes.leftWidth`, `panes.left_width`) becomes one, with snake_case segments turned into the
/// camelCase the state serializes as.
fn pointer_of(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return None;
    }
    if path.starts_with('/') {
        return Some(path.to_string());
    }
    let mut out = String::new();
    for seg in path.split('.') {
        out.push('/');
        let mut upper = false;
        for c in seg.chars() {
            if c == '_' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
    }
    Some(out)
}

/// `ui.set`: replace the field at `path` in the UI state and re-apply it. The whole state is
/// validated through serde, so a wrong type is rejected and the state left alone; the pane layout
/// and window geometry are sanitized afterwards so a bad width or size cannot hide the canvas.
pub fn set(app: &mut CraftApp, path: &str, value: Value) -> Result<Value, String> {
    let pointer = pointer_of(path).ok_or_else(|| "`path` must name a field of the UI state, e.g. `dark` or `panes.leftWidth`".to_string())?;
    let mut state = serde_json::to_value(app.prefs()).map_err(|e| e.to_string())?;
    let slot = state.pointer_mut(&pointer).ok_or_else(|| format!("no UI state field at {pointer}"))?;
    *slot = value;
    let ui: UiState = serde_json::from_value(state).map_err(|e| format!("{pointer}: {e}"))?;
    let backstage = app.backstage;
    app.apply_prefs(ui);
    app.backstage = backstage;
    app.ui.window = app.ui.window.and_then(|w| w.sanitized());
    serde_json::to_value(app.prefs()).map_err(|e| e.to_string())
}

/// `ui.menus`: the menu tree as JSON, so a script can find out what is on the menus and how each
/// entry looks right now.
pub fn menus_json(app: &CraftApp) -> Value {
    fn items(app: &CraftApp, list: &[menus::Item]) -> Vec<Value> {
        list.iter()
            .map(|item| match item {
                menus::Item::Cmd(label, id, params) | menus::Item::Check(label, id, params) => {
                    let e = menus::entry(app, id);
                    json!({
                        "label": label,
                        "id": id,
                        "params": params,
                        "shortcut": e.shortcut,
                        "enabled": e.enabled,
                        "checked": e.checked,
                        "check": matches!(item, menus::Item::Check(..)),
                    })
                }
                menus::Item::Sub(label, sub) => json!({"label": label, "items": items(app, sub)}),
                menus::Item::Sep => json!({"sep": true}),
                menus::Item::Header(label) => json!({"header": label}),
                menus::Item::Recent(path) => json!({"recent": path, "id": "file.open", "params": {"path": path}}),
            })
            .collect()
    }
    let role = |r: menus::MenuRole| match r {
        menus::MenuRole::Normal => "normal",
        menus::MenuRole::App => "app",
        menus::MenuRole::Window => "window",
        menus::MenuRole::Help => "help",
    };
    Value::Array(menus::menu_tree(app).iter().map(|m| json!({"title": m.title, "role": role(m.role), "items": items(app, &m.items)})).collect())
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

/// A non-negative integer param; floats are truncated, junk is `None`.
fn uint(p: &Value, k: &str) -> Option<u64> {
    let v = p.get(k)?;
    v.as_u64().or_else(|| v.as_f64().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f.min(u64::MAX as f64) as u64))
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

/// Whether `id` is something [`CraftApp::run`] knows: an engine command or a UI command.
fn is_command(app: &CraftApp, id: &str) -> bool {
    app.session.registry.get(id).is_some() || crate::UI_COMMANDS.iter().any(|(cid, ..)| *cid == id)
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
            // The MCP server says so with `origin: "mcp"`, so the transaction log tells its steps from a script's.
            let origin = if s("origin") == Some("mcp") { Origin::Mcp } else { Origin::Control };
            match app.run_as(origin, id, params) {
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
        "ui.menus" => ok(menus_json(app)),
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
            let count = uint(p, "count").unwrap_or(1);
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
            let steps = uint(p, "steps").unwrap_or(8).clamp(1, 100);
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
            let Some(name) = s("key") else { return err("missing `key`") };
            let Some(k) = key_from(name) else { return err(format!("unknown key `{name}`")) };
            let m = mods(p);
            app.synthetic.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
            app.synthetic.push(egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
            ok(json!({"queued": true}))
        }
        "ui.text" => {
            let Some(t) = s("text") else { return err("missing `text`") };
            app.synthetic.push(egui::Event::Text(t.to_string()));
            ok(json!({"queued": true}))
        }
        "ui.wait" => {
            let frames = uint(p, "frames").unwrap_or(1).clamp(1, u64::from(MAX_WAIT_FRAMES)) as u32;
            Outcome::Wait { frames }
        }
        "ui.screenshot" => Outcome::Screenshot { path: s("path").filter(|p| !p.trim().is_empty()).map(str::to_string) },
        "ui.resize" => {
            let (Some(w), Some(h)) = (f("width"), f("height")) else { return err("missing width/height") };
            let size = egui::vec2(w, h).clamp(crate::window_geometry::MIN_SIZE, egui::vec2(16384.0, 16384.0));
            ctx.viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ok(json!({"width": size.x, "height": size.y}))
        }
        "ui.focus" => {
            ctx.viewport_cmd(egui::ViewportCommand::Focus);
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
        "" => err("missing `method`"),
        other if is_command(app, other) => match app.run_as(Origin::Control, other, p.clone()) {
            Ok(v) => ok(v),
            Err(e) => err(e),
        },
        other => err(format!("unknown method `{other}` (see docs/control-protocol.md)")),
    }
}

/// Write a captured frame as PNG; the reply for `ui.screenshot`.
pub fn save_screenshot(image: &egui::ColorImage, path: Option<&str>) -> Value {
    let path = path.map(str::to_string).unwrap_or_else(default_screenshot_path);
    let [w, h] = image.size;
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) else {
        return json!({"ok": false, "error": "screenshot buffer has the wrong size"});
    };
    if let Err(e) = img.save_with_format(&path, image::ImageFormat::Png) {
        return json!({"ok": false, "error": format!("{path}: {e}")});
    }
    json!({"ok": true, "result": {"path": path, "width": w, "height": h}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default())
    }

    fn call(app: &mut CraftApp, method: &str, params: Value) -> Value {
        let ctx = egui::Context::default();
        let (req, _rx) = ControlRequest::new(method, params);
        match handle(app, &ctx, &req) {
            Outcome::Done(v) => v,
            Outcome::Screenshot { path } => json!({"deferred": "screenshot", "path": path}),
            Outcome::Wait { frames } => json!({"deferred": "wait", "frames": frames}),
        }
    }

    #[test]
    fn ui_set_changes_one_field_and_rejects_bad_ones() {
        let mut app = app();
        set(&mut app, "/tab", json!("View")).unwrap();
        assert_eq!(app.ui.tab, "View");
        set(&mut app, "panes/leftWidth", json!(333)).unwrap();
        assert_eq!(app.ui.panes.left_width, 333.0);
        set(&mut app, "panes.rightWidth", json!(300)).unwrap();
        assert_eq!(app.ui.panes.right_width, 300.0);
        set(&mut app, "panes.right_width", json!(310)).unwrap();
        assert_eq!(app.ui.panes.right_width, 310.0, "snake_case segments are accepted");
        set(&mut app, "/dark", json!(true)).unwrap();
        assert!(app.session.view.dark_mode);
        assert!(set(&mut app, "/nothing", json!(1)).is_err());
        assert!(set(&mut app, "", json!(1)).is_err());
        assert!(set(&mut app, "/", json!({})).is_err());
        assert!(set(&mut app, "/tab", json!(["not", "a", "string"])).is_err());
        assert_eq!(app.ui.tab, "View", "a failed set leaves the state alone");
    }

    #[test]
    fn ui_set_sanitizes_panes_and_window() {
        let mut app = app();
        set(&mut app, "panes.leftWidth", json!(5)).unwrap();
        assert_eq!(app.ui.panes.left_width, crate::panes::MIN_WIDTH);
        set(&mut app, "panes.leftWidth", json!(1e9)).unwrap();
        assert_eq!(app.ui.panes.left_width, crate::panes::MAX_WIDTH);
        set(&mut app, "window", json!({"width": 10, "height": 10, "position": [1e9, 0]})).unwrap();
        let w = app.ui.window.unwrap();
        assert_eq!((w.width, w.height), (crate::window_geometry::MIN_SIZE.x, crate::window_geometry::MIN_SIZE.y));
        assert_eq!(w.position, None, "an impossible position is dropped");
        set(&mut app, "window", json!({"width": -1, "height": 0})).unwrap();
        assert_eq!(app.ui.window, None, "an impossible size means no saved geometry");
        set(&mut app, "window", json!(null)).unwrap();
        assert_eq!(app.ui.window, None);
    }

    #[test]
    fn ui_set_keeps_the_file_page_open() {
        let mut app = app();
        app.run("ui.backstage", json!({"value": true})).unwrap();
        set(&mut app, "dark", json!(true)).unwrap();
        assert!(app.backstage_open());
    }

    #[test]
    fn execute_logs_the_origin_it_is_told() {
        let mut app = app();
        let r = call(&mut app, "engine.execute", json!({"command": "view.zoomIn", "origin": "mcp"}));
        assert_eq!(r["ok"], true, "{r}");
        call(&mut app, "engine.execute", json!({"command": "edit.setTitle", "params": {"title": "a"}}));
        call(&mut app, "engine.execute", json!({"command": "edit.setTitle", "params": {"title": "b"}, "origin": "mcp"}));
        call(&mut app, "engine.execute", json!({"command": "edit.setTitle", "params": {"title": "c"}, "origin": 5}));
        let origins: Vec<Origin> = app.session.transactions().iter().rev().take(3).map(|t| t.origin).collect();
        assert_eq!(origins, [Origin::Control, Origin::Mcp, Origin::Control], "newest first: junk falls back to the control channel");
    }

    #[test]
    fn execute_and_short_form_dispatch_commands() {
        let mut app = app();
        let r = call(&mut app, "engine.execute", json!({"command": "view.zoomIn"}));
        assert_eq!(r["ok"], true, "{r}");
        let r = call(&mut app, "view.zoomOut", json!({}));
        assert_eq!(r["ok"], true, "{r}");
        let r = call(&mut app, "engine.execute", json!({"command": "nope.nothing"}));
        assert_eq!(r["ok"], false);
        let r = call(&mut app, "engine.execute", json!({}));
        assert_eq!(r["ok"], false);
        let r = call(&mut app, "no.such.method", json!({}));
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().starts_with("unknown method"), "{r}");
        let r = call(&mut app, "", json!({}));
        assert_eq!(r["ok"], false);
    }

    #[test]
    fn commands_include_ui_commands() {
        let mut app = app();
        let r = call(&mut app, "engine.commands", json!({}));
        let ids: Vec<&str> = r["result"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"file.save"));
        assert!(ids.contains(&"ui.collapseRibbon"));
        assert!(ids.contains(&"app.quit"));
        for c in r["result"].as_array().unwrap() {
            for k in ["id", "label", "location", "shortcut", "params", "enabled", "reason"] {
                assert!(c.get(k).is_some(), "{k} missing in {c}");
            }
        }
    }

    #[test]
    fn inspect_has_the_documented_fields() {
        let mut app = app();
        let r = call(&mut app, "ui.inspect", json!({}));
        let v = &r["result"];
        for k in [
            "ui",
            "view",
            "selection",
            "dialog",
            "backstage",
            "window",
            "title",
            "path",
            "dirty",
            "rev",
            "undoLabel",
            "status",
            "perf",
            "tool",
            "diagram",
            "menu",
        ] {
            assert!(v.get(k).is_some(), "{k} missing in {v}");
        }
        assert_eq!(v["dialog"], Value::Null);
        assert_eq!(v["menu"], Value::Null, "no dropdown is open");
        assert_eq!(v["dirty"], false);
        assert_eq!(v["tool"], "pointer");
        assert_eq!(v["diagram"]["shapes"], 6);
        assert_eq!(v["diagram"]["connectors"], 6);
        assert_eq!(v["diagram"]["page"]["board"], "infinite", "the sample is on an infinite board");
        let frame = app.session.doc.page().unwrap().frame();
        assert_eq!(v["diagram"]["page"]["width"], frame.width(), "width and height report the frame");
        assert_eq!(v["diagram"]["selected"], json!([]));
        app.run("edit.setTitle", json!({"title": "x"})).unwrap();
        app.run("ui.dialog", json!({"name": "zoom"})).unwrap();
        let v = call(&mut app, "ui.inspect", json!({}))["result"].clone();
        assert_eq!(v["dirty"], true);
        assert_eq!(v["undoLabel"], "Document Title");
        assert_eq!(v["dialog"]["dialog"], "zoom");
    }

    #[test]
    fn menus_list_every_entry_with_its_state() {
        let mut app = app();
        app.ui.recent.push("/tmp/Plans.craft.json".into());
        let r = call(&mut app, "ui.menus", json!({}));
        let menus = r["result"].as_array().unwrap();
        assert!(menus.iter().any(|m| m["title"] == "File"));
        let view = menus.iter().find(|m| m["title"] == "View").unwrap();
        let dark = view["items"].as_array().unwrap().iter().find(|i| i["id"] == "view.dark").unwrap();
        assert_eq!(dark["checked"], false);
        assert_eq!(dark["check"], true);
        app.run("view.dark", json!({"value": true})).unwrap();
        let r = call(&mut app, "ui.menus", json!({}));
        let view = r["result"].as_array().unwrap().iter().find(|m| m["title"] == "View").unwrap().clone();
        let dark = view["items"].as_array().unwrap().iter().find(|i| i["id"] == "view.dark").unwrap().clone();
        assert_eq!(dark["checked"], true);
        let edit = r["result"].as_array().unwrap().iter().find(|m| m["title"] == "Edit").unwrap();
        let undo = edit["items"].as_array().unwrap().iter().find(|i| i["id"] == "edit.undo").unwrap();
        assert_eq!(undo["enabled"], false, "nothing to undo yet");
        let file = r["result"].as_array().unwrap().iter().find(|m| m["title"] == "File").unwrap();
        let recent = file["items"].as_array().unwrap().iter().find(|i| i["label"] == "Open Recent").unwrap();
        assert_eq!(recent["items"][0]["recent"], "/tmp/Plans.craft.json");
    }

    #[test]
    fn control_channel_steps_are_logged_as_control() {
        let mut app = app();
        let r = call(&mut app, "engine.execute", json!({"command": "edit.setTitle", "params": {"title": "From control"}}));
        assert_eq!(r["ok"], true, "{r}");
        let last = app.session.transactions().back().map(|t| t.origin);
        assert_eq!(last, Some(Origin::Control));
        assert_eq!(app.session.origin(), Origin::Ui, "the origin is restored after the call");
    }

    #[test]
    fn input_methods_queue_events() {
        let mut app = app();
        for (m, p) in [
            ("ui.click", json!({"x": 10, "y": 20, "count": 2, "button": "right", "shift": true})),
            ("ui.move", json!({"x": 1, "y": 2})),
            ("ui.drag", json!({"x": 1, "y": 2, "toX": 50, "toY": 60, "steps": 3})),
            ("ui.key", json!({"key": "S", "cmd": true})),
            ("ui.key", json!({"key": "Escape"})),
            ("ui.text", json!({"text": "hi"})),
        ] {
            let r = call(&mut app, m, p);
            assert_eq!(r["result"]["queued"], true, "{m}: {r}");
        }
        assert!(!app.synthetic.is_empty());
        assert_eq!(call(&mut app, "ui.key", json!({"key": "NoSuchKey"}))["ok"], false);
        assert_eq!(call(&mut app, "ui.click", json!({"x": "left"}))["ok"], false);
    }

    #[test]
    fn deferred_methods_are_deferred() {
        let mut app = app();
        let r = call(&mut app, "ui.wait", json!({"frames": 3}));
        assert_eq!(r["frames"], 3);
        let r = call(&mut app, "ui.wait", json!({"frames": 1e12}));
        assert_eq!(r["frames"], MAX_WAIT_FRAMES);
        let r = call(&mut app, "ui.wait", json!({}));
        assert_eq!(r["frames"], 1);
        let r = call(&mut app, "ui.screenshot", json!({"path": "/tmp/x.png"}));
        assert_eq!(r["path"], "/tmp/x.png");
        let r = call(&mut app, "ui.screenshot", json!({"path": ""}));
        assert_eq!(r["path"], Value::Null, "an empty path means the default");
    }

    #[test]
    fn hostile_params_never_panic() {
        let methods = [
            "engine.execute",
            "engine.commands",
            "ui.inspect",
            "ui.menus",
            "ui.set",
            "ui.click",
            "ui.move",
            "ui.drag",
            "ui.key",
            "ui.text",
            "ui.wait",
            "ui.screenshot",
            "ui.resize",
            "ui.focus",
            "view.zoomIn",
            "edit.setTitle",
            "ui.tab",
            "nonsense",
            "",
        ];
        let junk = [
            json!(null),
            json!({}),
            json!([]),
            json!("text"),
            json!(5),
            json!({"command": 5, "params": "x", "path": [], "value": {}, "x": "a", "y": null, "key": 3, "text": 1, "frames": -1}),
            json!({"command": "", "path": "", "x": 1e300, "y": -1e300, "toX": f64::NAN, "toY": 0, "steps": 1e30, "count": 1e30, "frames": 1e30, "width": -1, "height": 1e30}),
            json!({"command": "edit.setTitle", "params": {"title": 5}, "path": "/panes/left", "value": "not a list", "key": "", "text": "\u{0}\u{ffff}"}),
            json!({"path": "/window", "value": {"width": "x"}, "x": 0, "y": 0, "toX": 0, "toY": 0, "button": "none", "frames": 0}),
            json!({"path": "a.b.c.d.e.f", "value": 1, "command": "ui.dialog", "params": {"name": "nothing"}}),
        ];
        for m in methods {
            for j in &junk {
                let mut app = app();
                let r = call(&mut app, m, j.clone());
                assert!(r.is_object(), "{m}: {r}");
            }
        }
    }
}
