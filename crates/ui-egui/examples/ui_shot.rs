//! Headless window screenshots: the whole VizCraft window rendered offscreen with
//! `egui_kittest` and wgpu, driven by a script of control requests. No window opens, nothing
//! steals focus, and it works with the screen locked, which is what makes it usable from an
//! agent running in the background or in CI on a machine with a GPU.
//!
//! ```text
//! cargo run -p vizcraft-ui-egui --example ui_shot -- [--size 1440x920] [--scale 2] [--dark] [--empty] script.jsonl
//! ```
//!
//! The script is JSON lines. Each line is a control request, the same JSON the socket takes
//! minus the `id` (`docs/control-protocol.md`):
//!
//! ```text
//! {"method": "engine.execute", "params": {"command": "view.zoomIn", "params": {}}}
//! {"method": "ui.set", "params": {"path": "ribbonCollapsed", "value": true}}
//! {"method": "ui.screenshot", "params": {"path": "/tmp/ribbon-collapsed.png"}}
//! ```
//!
//! Every method goes through the app's own control handler, so `ui.inspect`, `ui.click`,
//! `ui.key`, `ui.wait` and the rest behave as they do in the running app. `ui.screenshot` is the
//! one exception: the harness renders the frame itself and writes the PNG (the viewport
//! screenshot command needs a real window). Its reply has the same shape, `{"path", "width",
//! "height"}`, with the size in physical pixels (`--size` times `--scale`).
//!
//! One reply line is printed per request, as the socket would send it plus a `method` field.
//! Options: `--size WxH` in points (default 1440x920), `--scale N` pixels per point (default 2),
//! `--dark` starts in dark mode, `--empty` starts with a blank document instead of the sample.
//! Exit code 1 when a request failed or a render did not happen, 2 for a usage error.
//! Lines starting with `#` and blank lines are skipped.

use std::process::ExitCode;

use egui_kittest::Harness;
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{ControlRequest, CraftApp, Services};

const USAGE: &str = "usage: ui_shot [--size WxH] [--scale N] [--dark] [--empty] script.jsonl";

struct Options {
    size: egui::Vec2,
    scale: f32,
    dark: bool,
    empty: bool,
    script: String,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options { size: egui::vec2(1440.0, 920.0), scale: 2.0, dark: false, empty: false, script: String::new() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--size" => {
                let v = args.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size wants WxH, e.g. 1440x920")?;
                let w: f32 = w.trim().parse().map_err(|_| "--size: bad width")?;
                let h: f32 = h.trim().parse().map_err(|_| "--size: bad height")?;
                if !(200.0..=8192.0).contains(&w) || !(200.0..=8192.0).contains(&h) {
                    return Err("--size: each side must be between 200 and 8192 points".into());
                }
                o.size = egui::vec2(w, h);
            }
            "--scale" => {
                let s: f32 = args.next().ok_or("--scale needs a number")?.trim().parse().map_err(|_| "--scale: bad number")?;
                if !(0.5..=4.0).contains(&s) {
                    return Err("--scale must be between 0.5 and 4".into());
                }
                o.scale = s;
            }
            "--dark" => o.dark = true,
            "--empty" => o.empty = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other if other.starts_with("--") => return Err(format!("unknown option {other}\n{USAGE}")),
            other => o.script = other.to_string(),
        }
    }
    if o.script.is_empty() {
        return Err(USAGE.into());
    }
    Ok(o)
}

/// One frame: feed the app's synthetic input (what eframe's `raw_input_hook` does), then step.
fn step(harness: &mut Harness<'_, CraftApp>) {
    let mut raw = std::mem::take(harness.input_mut());
    harness.state_mut().raw_input_hook(&mut raw);
    *harness.input_mut() = raw;
    harness.step();
}

fn step_n(harness: &mut Harness<'_, CraftApp>, n: usize) {
    for _ in 0..n {
        step(harness);
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let script = match std::fs::read_to_string(&opts.script) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", opts.script);
            return ExitCode::from(2);
        }
    };
    let mut lines = Vec::new();
    for (n, l) in script.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<Value>(l) {
            Ok(v) if v.get("method").and_then(Value::as_str).is_some() => lines.push(v),
            Ok(_) => {
                eprintln!("{}:{}: a request needs a `method`", opts.script, n + 1);
                return ExitCode::from(2);
            }
            Err(e) => {
                eprintln!("{}:{}: bad JSON: {e}", opts.script, n + 1);
                return ExitCode::from(2);
            }
        }
    }

    let doc = if opts.empty { vizcraft_engine::Document::new() } else { vizcraft_engine::sample::sample_document() };
    let (tx, rx) = std::sync::mpsc::channel::<ControlRequest>();
    let mut app = CraftApp::new(Session::new(doc), Services::default()).with_control(rx);
    app.integrated_titlebar = cfg!(target_os = "macos");
    app.session.view.dark_mode = opts.dark;
    let mut harness = Harness::builder().with_size(opts.size).with_pixels_per_point(opts.scale).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    harness.input_mut().max_texture_side = Some(8192);
    // Fonts install on the first frame and the app skips drawing for two more; settle first.
    step_n(&mut harness, 6);

    let mut failed = false;
    for req in lines {
        let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        let params = req.get("params").cloned().unwrap_or(json!({}));
        let mut reply = if method == "ui.screenshot" {
            // Give whatever the previous requests changed a few frames to be drawn, as the app does.
            step_n(&mut harness, 4);
            let path = params.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()).map(str::to_string);
            let path = path.unwrap_or_else(vizcraft_ui_egui::control::default_screenshot_path);
            match harness.render() {
                Ok(img) => match img.save(&path) {
                    Ok(()) => json!({"ok": true, "result": {"path": path, "width": img.width(), "height": img.height()}}),
                    Err(e) => json!({"ok": false, "error": format!("{path}: {e}")}),
                },
                Err(e) => json!({"ok": false, "error": format!("render failed: {e}")}),
            }
        } else {
            let (creq, rrx) = ControlRequest::new(method.clone(), params.clone());
            if tx.send(creq).is_err() {
                json!({"ok": false, "error": "the app dropped the control channel"})
            } else {
                // `ui.wait` answers after that many frames; everything else within a frame or two.
                let frames = params.get("frames").and_then(Value::as_u64).unwrap_or(0).min(100_000) as usize;
                let mut got = None;
                for _ in 0..(frames + 60) {
                    step(&mut harness);
                    if let Ok(r) = rrx.try_recv() {
                        got = Some(r);
                        break;
                    }
                }
                got.unwrap_or_else(|| json!({"ok": false, "error": "no reply"}))
            }
        };
        if reply.get("ok") != Some(&Value::Bool(true)) {
            failed = true;
        }
        if let Some(o) = reply.as_object_mut() {
            o.insert("method".into(), Value::String(method));
        }
        println!("{reply}");
        // Let queued input land before the next request.
        step_n(&mut harness, 2);
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
