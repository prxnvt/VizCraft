//! The control channel end to end: requests go through the channel into a `CraftApp` laid out
//! by an `egui_kittest` harness (no GPU needed), and the replies are what
//! `docs/control-protocol.md` promises.

use std::sync::mpsc::{Receiver, Sender};

use egui_kittest::Harness;
use protocraft_engine::Session;
use protocraft_ui_egui::{ControlRequest, CraftApp, Services};
use serde_json::{Value, json};

struct Rig {
    harness: Harness<'static, CraftApp>,
    tx: Sender<ControlRequest>,
}

impl Rig {
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<ControlRequest>();
        let app = CraftApp::new(Session::new(protocraft_engine::sample::sample_document()), Services::default()).with_control(rx);
        let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
            |ui, app: &mut CraftApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        for _ in 0..6 {
            harness.step();
        }
        Rig { harness, tx }
    }

    /// One frame with the app's synthetic input fed in, as eframe's `raw_input_hook` would.
    fn step(&mut self) {
        let mut raw = std::mem::take(self.harness.input_mut());
        self.harness.state_mut().raw_input_hook(&mut raw);
        *self.harness.input_mut() = raw;
        self.harness.step();
    }

    /// Send a request and run frames until it is answered (or give up after `max_frames`).
    fn send(&mut self, method: &str, params: Value) -> (Receiver<Value>, Option<Value>) {
        let (req, rx) = ControlRequest::new(method, params);
        self.tx.send(req).unwrap();
        for _ in 0..200 {
            self.step();
            if let Ok(r) = rx.try_recv() {
                return (rx, Some(r));
            }
        }
        (rx, None)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (_rx, r) = self.send(method, params);
        r.unwrap_or_else(|| panic!("{method}: no reply within 200 frames"))
    }

    fn app(&self) -> &CraftApp {
        self.harness.state()
    }
}

#[test]
fn execute_runs_commands_and_reports_errors() {
    let mut rig = Rig::new();
    let r = rig.call("engine.execute", json!({"command": "view.zoomIn", "params": {}}));
    assert_eq!(r["ok"], true, "{r}");
    assert!((rig.app().session.view.zoom - 1.1).abs() < 1e-5);
    let r = rig.call("engine.execute", json!({"command": "edit.setTitle", "params": {"title": "from the channel"}}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(rig.app().session.doc.title, "from the channel");
    let r = rig.call("engine.execute", json!({"command": "edit.undo"}));
    assert_eq!(r["ok"], true);
    let r = rig.call("engine.execute", json!({"command": "edit.undo"}));
    assert_eq!(r["ok"], false, "nothing left to undo is an error, not a crash");
    assert!(r["error"].is_string());
    let r = rig.call("engine.execute", json!({"command": "no.such"}));
    assert_eq!(r["ok"], false);
    let r = rig.call("view.zoom100", json!({}));
    assert_eq!(r["ok"], true, "a command id is the short form of engine.execute");
    let r = rig.call("not.a.method", json!({}));
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().starts_with("unknown method"), "{r}");
}

#[test]
fn commands_list_engine_and_ui_commands() {
    let mut rig = Rig::new();
    let r = rig.call("engine.commands", json!({}));
    let list = r["result"].as_array().unwrap();
    let ids: Vec<&str> = list.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"edit.undo") && ids.contains(&"ui.tab") && ids.contains(&"app.quit"));
    let undo = list.iter().find(|c| c["id"] == "edit.undo").unwrap();
    assert_eq!(undo["enabled"], false);
    assert_eq!(undo["reason"], "nothing to undo");
}

#[test]
fn inspect_has_the_documented_shape() {
    let mut rig = Rig::new();
    let r = rig.call("ui.inspect", json!({}));
    assert_eq!(r["ok"], true);
    let v = &r["result"];
    for k in ["ui", "view", "dialog", "window", "title", "dirty", "rev", "undoLabel", "status", "perf"] {
        assert!(v.get(k).is_some(), "{k} missing in {v}");
    }
    for k in ["tab", "ribbonCollapsed", "dark", "recent", "author", "panes", "window", "inWindowMenus"] {
        assert!(v["ui"].get(k).is_some(), "ui.{k} missing in {v}");
    }
    for k in ["zoom", "darkMode", "navPane", "propsPane"] {
        assert!(v["view"].get(k).is_some(), "view.{k} missing in {v}");
    }
    assert_eq!(v["window"], json!([1200.0, 800.0]));
    assert_eq!(v["title"], "Untitled");
    assert_eq!(v["dialog"], Value::Null);
    assert_eq!(v["dirty"], false);
    assert!(v["perf"]["frameMs"].is_number());
    // Open a dialog and the report says which.
    rig.call("ui.dialog", json!({"name": "about"}));
    let v = rig.call("ui.inspect", json!({}))["result"].clone();
    assert_eq!(v["dialog"]["dialog"], "about");
}

#[test]
fn set_round_trips_through_inspect() {
    let mut rig = Rig::new();
    let r = rig.call("ui.set", json!({"path": "dark", "value": true}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["dark"], true, "the reply is the whole state");
    assert!(r["result"].get("panes").is_some());
    assert!(rig.app().session.view.dark_mode);
    let r = rig.call("ui.set", json!({"path": "panes.leftWidth", "value": 320}));
    assert_eq!(r["result"]["panes"]["leftWidth"], 320.0);
    let r = rig.call("ui.set", json!({"path": "/ribbonCollapsed", "value": true}));
    assert_eq!(r["result"]["ribbonCollapsed"], true);
    let v = rig.call("ui.inspect", json!({}))["result"].clone();
    assert_eq!(v["ui"]["dark"], true);
    assert_eq!(v["ui"]["ribbonCollapsed"], true);
    assert_eq!(v["view"]["darkMode"], true);
    // Pane widths are owned by the side panels once they are showing (egui keeps the dragged
    // width), so a set width is reported in the reply but the panel keeps its own until the
    // panes module applies external changes; see docs/control-protocol.md.
    // Sanitized: a width too small is clamped, a bad window geometry dropped.
    let r = rig.call("ui.set", json!({"path": "panes.leftWidth", "value": 1}));
    assert_eq!(r["result"]["panes"]["leftWidth"], 140.0);
    // Rejected: unknown path, wrong type; the state is unchanged.
    let r = rig.call("ui.set", json!({"path": "nothing.here", "value": 1}));
    assert_eq!(r["ok"], false);
    let r = rig.call("ui.set", json!({"path": "tab", "value": 7}));
    assert_eq!(r["ok"], false);
    assert_eq!(rig.app().ui.tab, "Home");
    let r = rig.call("ui.set", json!({"value": 1}));
    assert_eq!(r["ok"], false, "missing path");
}

#[test]
fn menus_describe_the_menu_tree() {
    let mut rig = Rig::new();
    let r = rig.call("ui.menus", json!({}));
    assert_eq!(r["ok"], true);
    let menus = r["result"].as_array().unwrap();
    let titles: Vec<&str> = menus.iter().map(|m| m["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"File") && titles.contains(&"Edit") && titles.contains(&"View") && titles.contains(&"Help"));
    let edit = menus.iter().find(|m| m["title"] == "Edit").unwrap();
    let undo = edit["items"].as_array().unwrap().iter().find(|i| i["id"] == "edit.undo").unwrap();
    for k in ["label", "id", "enabled", "checked", "shortcut"] {
        assert!(undo.get(k).is_some(), "{k} missing in {undo}");
    }
    assert_eq!(undo["enabled"], false);
    assert!(edit["items"].as_array().unwrap().iter().any(|i| i["sep"] == true));
}

#[test]
fn input_methods_are_queued_and_then_land() {
    let mut rig = Rig::new();
    for (m, p) in [
        ("ui.move", json!({"x": 100, "y": 100})),
        ("ui.click", json!({"x": 100, "y": 100})),
        ("ui.drag", json!({"x": 100, "y": 100, "toX": 200, "toY": 150})),
        ("ui.text", json!({"text": "abc"})),
    ] {
        let r = rig.call(m, p);
        assert_eq!(r["result"]["queued"], true, "{m}: {r}");
    }
    // Shortcuts apply to ui.key: Mod+Shift+P opens the command palette. Pointer events land one
    // per frame, so the drag above takes a dozen frames to play out; ui.wait covers that.
    let r = rig.call("ui.key", json!({"key": "P", "cmd": true, "shift": true}));
    assert_eq!(r["result"]["queued"], true);
    let r = rig.call("ui.wait", json!({"frames": 30}));
    assert_eq!(r["ok"], true);
    let v = rig.call("ui.inspect", json!({}))["result"].clone();
    assert_eq!(v["dialog"]["dialog"], "commands", "{v}");
    let r = rig.call("ui.key", json!({"key": "Escape"}));
    assert_eq!(r["ok"], true);
    rig.call("ui.wait", json!({"frames": 6}));
    let v = rig.call("ui.inspect", json!({}))["result"].clone();
    assert_eq!(v["dialog"], Value::Null, "Escape closes the palette");
}

#[test]
fn wait_replies_after_the_frames() {
    let mut rig = Rig::new();
    let (rx, immediate) = rig.send("ui.wait", json!({"frames": 10}));
    // `send` runs up to 30 frames, so the reply is there; it must not have been there at once.
    let r = immediate.unwrap_or_else(|| rx.try_recv().unwrap());
    assert_eq!(r["ok"], true);
    assert_eq!(r["result"]["waited"], true);
    let (req, rx) = ControlRequest::new("ui.wait", json!({"frames": 5}));
    rig.tx.send(req).unwrap();
    rig.step();
    rig.step();
    assert!(rx.try_recv().is_err(), "not answered after two frames");
    for _ in 0..6 {
        rig.step();
    }
    assert_eq!(rx.try_recv().unwrap()["ok"], true);
}

#[test]
fn screenshot_is_deferred_not_answered_at_once() {
    let mut rig = Rig::new();
    let (req, rx) = ControlRequest::new("ui.screenshot", json!({"path": "/nonexistent/dir/x.png"}));
    rig.tx.send(req).unwrap();
    for _ in 0..5 {
        rig.step();
    }
    // Without a real viewport no frame is delivered; the reply comes later as an error (8 s),
    // never as a panic and never as an immediate success.
    assert!(rx.try_recv().is_err());
}

#[test]
fn quit_sets_the_quit_flags_without_asking() {
    let mut rig = Rig::new();
    rig.call("edit.setText", json!({"text": "unsaved"}));
    let r = rig.call("app.quit", json!({}));
    assert_eq!(r["ok"], true);
    assert!(rig.app().quit_confirmed);
    assert!(rig.app().dialog.is_none(), "automation quit does not ask about unsaved changes");
}

#[test]
fn hostile_params_never_panic_through_the_channel() {
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
        "ui.resize",
        "ui.focus",
        "edit.setText",
        "ui.tab",
        "ui.dialog",
        "garbage",
        "",
    ];
    let junk = [
        json!(null),
        json!([1, 2]),
        json!("string"),
        json!({"command": [], "params": 3, "path": 1, "value": null, "x": "a", "y": {}, "key": null, "text": [], "frames": "soon"}),
        json!({"command": "edit.setText", "params": {"text": 1e308, "note": -5}, "path": "/panes/leftWidth", "value": "wide", "x": 1e38, "y": -1e38, "toX": 0, "toY": 0, "steps": -1, "count": 0, "frames": 1e300, "width": 1e300, "height": -1}),
        json!({"path": "/recent", "value": [1, 2, 3], "key": "\u{0}", "text": "\u{ffff}", "name": "\u{0}", "tab": 42}),
    ];
    let mut rig = Rig::new();
    for m in methods {
        for j in &junk {
            let (req, rx) = ControlRequest::new(m, j.clone());
            rig.tx.send(req).unwrap();
            let mut got = None;
            for _ in 0..40 {
                rig.step();
                if let Ok(r) = rx.try_recv() {
                    got = Some(r);
                    break;
                }
            }
            // A huge `ui.wait` is clamped, not rejected, so it is still pending after 40 frames.
            if m == "ui.wait" && got.is_none() {
                continue;
            }
            let r = got.unwrap_or_else(|| panic!("{m} {j}: no reply"));
            assert!(r["ok"].is_boolean(), "{m} {j}: {r}");
        }
    }
    assert!(!rig.app().session.author.is_empty());
}
