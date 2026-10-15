//! ProtoCraft desktop app.
//!
//! Usage: `protocraft [--control <port>] [--sample] [--in-window-menus] [--window-pos x,y] [--version] [files...]`
//!
//! `--control <port>` (or `PROTOCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}`.
//! See `docs/control-protocol.md`.
//!
//! `--in-window-menus` (or `PROTOCRAFT_IN_WINDOW_MENUS=1`) starts with the menu bar drawn inside
//! the window instead of the native macOS one, as on other platforms; it is the same switch as
//! View > "Menu Bar in Window", which takes effect at once and is saved with the preferences.
//!
//! `--window-pos x,y` (or `PROTOCRAFT_WINDOW_POS="x,y"`) puts the first window's outer top-left
//! corner at that screen position, overriding the saved geometry, so test runs can be placed on a
//! spare monitor (`--window-pos -2500,100`) without moving them afterwards.
//!
//! Several document windows live in one process (`windows.rs`); on macOS the menu bar is native
//! (`native_menu.rs`) and Finder's open requests arrive through `protocraft-macos-open`.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod control_server;
#[cfg(target_os = "macos")]
mod native_menu;
mod windows;

use protocraft_engine::Session;
use protocraft_ui_egui::{CraftApp, Services, UiState, window_geometry::WindowGeometry};

/// The app: the first window's document, the restored window geometry until the first frame has
/// checked it against the monitors, the other document windows, and (macOS) the native menu bar
/// once installed.
struct App {
    root: CraftApp,
    restored: Option<WindowGeometry>,
    windows: windows::Windows,
    #[cfg(target_os = "macos")]
    menu: Option<native_menu::NativeMenu>,
    /// The menu bar is drawn inside the window (the native one is not installed). Follows the
    /// front window's `ui.in_window_menus` each frame.
    #[cfg(target_os = "macos")]
    in_window_menus: bool,
}

impl App {
    /// A new document window (File > New Window, Cmd+N), with the preferences of the window in front.
    fn open_window(&mut self, ctx: &egui::Context, template: &str) {
        let prefs = self.windows.front(&mut self.root).prefs();
        let mut app = CraftApp::new(Session::new(protocraft_engine::Document::new()), services());
        app.apply_prefs(prefs);
        app.ui.window = None;
        app.integrated_titlebar = cfg!(target_os = "macos");
        if template != "blank"
            && let Err(e) = app.run("file.new", serde_json::json!({"template": template}))
        {
            eprintln!("protocraft: new {template}: {e}");
        }
        self.windows.open(ctx, app, window_builder());
    }

    /// A document Finder asked us to open: in the window in front if that holds an untouched new
    /// document, otherwise in a new window (so nothing open is replaced).
    fn open_document(&mut self, ctx: &egui::Context, path: &str) {
        let front = self.windows.front(&mut self.root);
        if front.session.dirty || front.session.path.is_some() {
            self.open_window(ctx, "blank");
        }
        // A new window is the one in front now.
        let app = self.windows.front(&mut self.root);
        match app.run("file.open", serde_json::json!({"path": path})) {
            Ok(_) => {
                let _ = app.run("ui.backstage", serde_json::json!({"value": false}));
            }
            Err(e) => app.status(format!("Couldn't open {path}: {e}")),
        }
    }

    /// Keep the native bar in step with the "Menu Bar in Window" preference: the front window's
    /// flag is the truth, is copied to every window, and installs or removes the native bar the
    /// frame it changes. Then run the items chosen since the last frame in the front window;
    /// `quit` is set for `app.quit` (the host asks every window) and Cmd+W closes the front
    /// window the way its close button does.
    #[cfg(target_os = "macos")]
    fn menu_events(&mut self, ctx: &egui::Context, quit: &mut bool) {
        let in_window = self.windows.front(&mut self.root).ui.in_window_menus;
        if in_window != self.in_window_menus {
            self.in_window_menus = in_window;
            self.root.ui.in_window_menus = in_window;
            for app in self.windows.apps_mut() {
                app.ui.in_window_menus = in_window;
            }
        }
        if in_window {
            if let Some(menu) = self.menu.take() {
                menu.remove();
            }
            return;
        }
        let menu = self.menu.get_or_insert_with(|| native_menu::NativeMenu::install(ctx, &self.root));
        for (id, params) in menu.take_events() {
            match id.as_str() {
                "app.quit" => *quit = true,
                "file.close" => self.windows.close_front(ctx),
                // Everything else, File > New included, behaves as the in-window bar's item does.
                _ => protocraft_ui_egui::menus::run_item(self.windows.front(&mut self.root), ctx, &id, params),
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut quit = false;
        #[cfg(target_os = "macos")]
        self.menu_events(ctx, &mut quit);
        // Requests the other windows made (last frame's layout, or a menu item just now).
        let (mut new_windows, quit_all) = self.windows.take_requests();
        quit |= quit_all;
        // Documents Finder asked us to open (double click, Open With, the Dock icon).
        for path in protocraft_macos_open::take_pending() {
            self.open_document(ctx, &path);
        }
        self.root.logic(ctx);
        let prev = self.root.ui.window;
        self.root.ui.window = ctx.input(|i| WindowGeometry::track(prev, i.viewport(), i.viewport_rect().size()));
        if let Some(pos) = self.restored.take().and_then(|g| ctx.input(|i| g.rescue_position(i.viewport()))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }
        // Requests made by the menu run above or the root window this frame.
        new_windows.extend(self.root.new_window_request.take());
        quit |= std::mem::take(&mut self.root.quit_all_request);
        for template in new_windows {
            self.open_window(ctx, &template);
        }
        if quit {
            self.windows.quit();
        }
        self.windows.root_logic(ctx, &mut self.root);
        // The menu reflects the front window after this frame's commands have run.
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.menu {
            menu.refresh(self.windows.front(&mut self.root));
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.root.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.root.ui(ui);
        let ctx = ui.ctx().clone();
        self.windows.show(&ctx);
    }
    fn on_exit(&mut self) {
        save_prefs(&self.root);
    }
}

/// `ui.json` in the platform's per-user configuration directory.
fn prefs_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support/ProtoCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("ProtoCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join("protocraft"))
    };
    base.map(|b| b.join("ui.json"))
}

fn no_prefs() -> bool {
    std::env::var_os("PROTOCRAFT_NO_PREFS").is_some()
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
                .add_filter("ProtoCraft documents", &["json"])
                .add_filter("All files", &["*"])
                .pick_file()
                .map(|p| p.to_string_lossy().to_string())
        })),
        pick_save: Some(Box::new(|name: &str| rfd::FileDialog::new().set_file_name(name).save_file().map(|p| p.to_string_lossy().to_string()))),
        ..Default::default()
    }
}

/// `"x,y"` as a window position; whitespace around the numbers is fine, anything else is `None`.
fn parse_window_pos(s: &str) -> Option<[f32; 2]> {
    let (x, y) = s.split_once(',')?;
    let (x, y) = (x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?);
    (x.is_finite() && y.is_finite()).then_some([x, y])
}

/// A document window: no title bar of its own (the app draws one), files can be dropped on it.
fn window_builder() -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title("ProtoCraft")
        .with_app_id("com.storytold.protocraft")
        .with_inner_size([1440.0, 920.0])
        .with_min_inner_size([protocraft_ui_egui::window_geometry::MIN_SIZE.x, protocraft_ui_egui::window_geometry::MIN_SIZE.y])
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
    let mut control_port: Option<u16> = std::env::var("PROTOCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut sample = false;
    let mut in_window_menus = std::env::var_os("PROTOCRAFT_IN_WINDOW_MENUS").is_some_and(|v| v != "0" && !v.is_empty());
    let mut window_pos = std::env::var("PROTOCRAFT_WINDOW_POS").ok().and_then(|v| parse_window_pos(&v));
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--in-window-menus" => in_window_menus = true,
            "--window-pos" => window_pos = args.next().and_then(|v| parse_window_pos(&v)),
            "--version" => {
                println!("protocraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("protocraft [--control <port>] [--sample] [--in-window-menus] [--window-pos x,y] [--version] [files...]");
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    let mut options = eframe::NativeOptions { viewport: window_builder(), ..Default::default() };
    let mut restored = saved_window();
    if let Some(window) = restored {
        options.viewport = window.apply(options.viewport);
    }
    if let Some(pos) = window_pos {
        // An explicit position wins over the saved one and is not second-guessed on the first frame.
        options.viewport = options.viewport.with_position(pos);
        restored = None;
    }
    protocraft_macos_open::install();
    eframe::run_native(
        "ProtoCraft",
        options,
        Box::new(move |cc| {
            let doc = if sample { protocraft_engine::sample::sample_document() } else { protocraft_engine::Document::new() };
            {
                let ctx = cc.egui_ctx.clone();
                protocraft_macos_open::set_waker(move || ctx.request_repaint());
            }
            let mut app = CraftApp::new(Session::new(doc), services());
            load_prefs(&mut app);
            app.ui.window = restored;
            app.integrated_titlebar = cfg!(target_os = "macos");
            app.multi_window = true;
            if in_window_menus {
                app.ui.in_window_menus = true;
            }
            // The host's copy starts from what the window asks for (flag, or the saved preference).
            #[cfg(target_os = "macos")]
            let in_window_menus = app.ui.in_window_menus;
            if let Some(port) = control_port {
                let rx = control_server::start(port, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            for f in files {
                if let Err(e) = app.run("file.open", serde_json::json!({"path": f})) {
                    eprintln!("protocraft: {f}: {e}");
                }
            }
            Ok(Box::new(App {
                root: app,
                restored,
                windows: windows::Windows::default(),
                #[cfg(target_os = "macos")]
                menu: None,
                #[cfg(target_os = "macos")]
                in_window_menus,
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::parse_window_pos;

    #[test]
    fn window_pos_parses_two_numbers() {
        assert_eq!(parse_window_pos("-2500,100"), Some([-2500.0, 100.0]));
        assert_eq!(parse_window_pos(" 10 , 20.5 "), Some([10.0, 20.5]));
        assert_eq!(parse_window_pos("10"), None);
        assert_eq!(parse_window_pos("a,b"), None);
        assert_eq!(parse_window_pos("inf,0"), None);
    }
}
