//! BaseCraft desktop app.
//!
//! Usage: `basecraft [--control <port>] [--sample] [--version] [files...]`
//!
//! `--control <port>` (or `BASECRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}`.
//! See `docs/control-protocol.md`.
//!
//! One window for now. Cascade 2 adds `windows.rs` (several document windows in one process) and
//! `native_menu.rs` (the macOS menu bar); [`App`] is shaped so they slot in next to `root`.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod control_server;

use basecraft_engine::Session;
use basecraft_ui_egui::{CraftApp, Services, UiState, window_geometry::WindowGeometry};

/// The app: the first window's document, and the restored window geometry until the first frame
/// has checked it against the monitors.
struct App {
    root: CraftApp,
    restored: Option<WindowGeometry>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Documents Finder asked us to open (double click, Open With, the Dock icon).
        for path in basecraft_macos_open::take_pending() {
            if let Err(e) = self.root.run("file.open", serde_json::json!({"path": path})) {
                self.root.status(format!("Couldn't open {path}: {e}"));
            }
        }
        self.root.logic(ctx);
        let prev = self.root.ui.window;
        self.root.ui.window = ctx.input(|i| WindowGeometry::track(prev, i.viewport(), i.viewport_rect().size()));
        if let Some(pos) = self.restored.take().and_then(|g| ctx.input(|i| g.rescue_position(i.viewport()))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
        // Single window: a new window request opens the document here, and quit-all is a quit.
        if let Some(template) = self.root.new_window_request.take() {
            let _ = self.root.run("file.new", serde_json::json!({"template": template}));
        }
        if std::mem::take(&mut self.root.quit_all_request) {
            let _ = self.root.run("app.quit", serde_json::json!({}));
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.root.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.root.ui(ui);
    }
    fn on_exit(&mut self) {
        save_prefs(&self.root);
    }
}

/// `ui.json` in the platform's per-user configuration directory.
fn prefs_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support/BaseCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("BaseCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join("basecraft"))
    };
    base.map(|b| b.join("ui.json"))
}

fn no_prefs() -> bool {
    std::env::var_os("BASECRAFT_NO_PREFS").is_some()
}

fn load_prefs(app: &mut CraftApp) {
    if no_prefs() {
        return;
    }
    if let Some(p) = prefs_path()
        && let Ok(bytes) = std::fs::read(&p)
        && let Ok(ui) = serde_json::from_slice::<UiState>(&bytes)
    {
        app.apply_prefs(ui);
    }
}

/// The saved window geometry, read before the window opens (`load_prefs` runs after).
fn saved_window() -> Option<WindowGeometry> {
    if no_prefs() {
        return None;
    }
    let bytes = std::fs::read(prefs_path()?).ok()?;
    serde_json::from_slice::<UiState>(&bytes).ok()?.window?.sanitized()
}

fn save_prefs(app: &CraftApp) {
    if no_prefs() {
        return;
    }
    if let Some(p) = prefs_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&app.prefs()) {
            let _ = std::fs::write(&p, bytes);
        }
    }
}

/// Native file pickers.
fn services() -> Services {
    Services {
        pick_open: Some(Box::new(|_purpose: &str| {
            rfd::FileDialog::new()
                .add_filter("BaseCraft documents", &["json"])
                .add_filter("All files", &["*"])
                .pick_file()
                .map(|p| p.to_string_lossy().to_string())
        })),
        pick_save: Some(Box::new(|name: &str| rfd::FileDialog::new().set_file_name(name).save_file().map(|p| p.to_string_lossy().to_string()))),
        ..Default::default()
    }
}

/// A document window: no title bar of its own (the app draws one), files can be dropped on it.
fn window_builder() -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title("BaseCraft")
        .with_app_id("com.storytold.basecraft")
        .with_inner_size([1440.0, 920.0])
        .with_min_inner_size([basecraft_ui_egui::window_geometry::MIN_SIZE.x, basecraft_ui_egui::window_geometry::MIN_SIZE.y])
        .with_drag_and_drop(true)
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    // On macOS an empty icon stops eframe replacing the bundle's Dock icon with its default logo.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon(egui::IconData::default());
    builder
}

fn main() -> eframe::Result {
    let mut control_port: Option<u16> = std::env::var("BASECRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut sample = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--version" => {
                println!("basecraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("basecraft [--control <port>] [--sample] [--version] [files...]");
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    let mut options = eframe::NativeOptions { viewport: window_builder(), ..Default::default() };
    let restored = saved_window();
    if let Some(window) = restored {
        options.viewport = window.apply(options.viewport);
    }
    basecraft_macos_open::install();
    eframe::run_native(
        "BaseCraft",
        options,
        Box::new(move |cc| {
            let doc = if sample { basecraft_engine::sample::sample_document() } else { basecraft_engine::Document::new() };
            {
                let ctx = cc.egui_ctx.clone();
                basecraft_macos_open::set_waker(move || ctx.request_repaint());
            }
            let mut app = CraftApp::new(Session::new(doc), services());
            load_prefs(&mut app);
            app.ui.window = restored;
            app.integrated_titlebar = cfg!(target_os = "macos");
            if let Some(port) = control_port {
                let rx = control_server::start(port, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            for f in files {
                if let Err(e) = app.run("file.open", serde_json::json!({"path": f})) {
                    eprintln!("basecraft: {f}: {e}");
                }
            }
            Ok(Box::new(App { root: app, restored }))
        }),
    )
}
