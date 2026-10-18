//! VizCraft desktop app.
//!
//! Usage: `vizcraft [--control <port>] [--sample] [--in-window-menus] [--window-pos x,y] [--trace] [--version] [files...]`
//!
//! `--control <port>` (or `VIZCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}`.
//! See `docs/control-protocol.md`.
//!
//! `--in-window-menus` (or `VIZCRAFT_IN_WINDOW_MENUS=1`) starts with the menu bar drawn inside
//! the window instead of the native macOS one, as on other platforms; it is the same switch as
//! View > "Menu Bar in Window", which takes effect at once and is saved with the preferences.
//!
//! `--window-pos x,y` puts the first window's outer top-left
//! corner at that screen position, overriding the saved geometry, so test runs can be placed on a
//! spare monitor (`--window-pos -2500,100`) without moving them afterwards.
//!
//! `--trace` (or `VIZCRAFT_TRACE=1`) records a performance trace (frame times, every command and
//! its model sync, routing and undo work, UI sections, repaint causes, input by kind, stalls
//! between frames with native stack captures, memory) into `traces/` at the top of the source
//! tree, or `VIZCRAFT_TRACE_DIR`. Frames slower than `VIZCRAFT_TRACE_DETAIL_MS` (default 8) keep
//! their nested spans; a pending repaint that no frame answers within `VIZCRAFT_TRACE_STALL_MS`
//! (default 100) is a stall, sampled with the system `sample` tool unless
//! `VIZCRAFT_TRACE_NO_SAMPLE` is set. Cmd+Shift+M (`ui.traceMark`) marks the moment. See
//! `vizcraft_engine::trace` and `docs/testing.md`.
//!
//! Several document windows live in one process (`windows.rs`); on macOS the menu bar is native
//! (`native_menu.rs`) and Finder's open requests arrive through `vizcraft-macos-open`.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod control_server;
#[cfg(target_os = "macos")]
mod native_menu;
mod print;
mod windows;

use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services, UiState, ViewportCmdExt, window_geometry::WindowGeometry};

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
    /// The previous frame's CPU time as eframe measured it (layout, tessellation and paint).
    prev_cpu: Option<f32>,
    /// When memory was last sampled for the trace.
    mem_sampled: Option<std::time::Instant>,
    /// Frames seen by the trace (display facts are recorded once they have settled).
    trace_frames: u64,
}

/// This frame's input events counted by kind for the trace (`pointerMoved`, `key`, ...), without
/// the zero counts. More pointer moves than a display refresh can carry means the frame was late.
fn input_kinds(events: &[egui::Event]) -> serde_json::Value {
    let mut counts: std::collections::BTreeMap<&'static str, u64> = std::collections::BTreeMap::new();
    for e in events {
        let kind = match e {
            egui::Event::PointerMoved(_) => "pointerMoved",
            egui::Event::MouseMoved(_) => "mouseMoved",
            egui::Event::PointerButton { .. } => "pointerButton",
            egui::Event::PointerGone => "pointerGone",
            egui::Event::MouseWheel { .. } => "scroll",
            egui::Event::Zoom(_) | egui::Event::Rotate(_) => "zoom",
            egui::Event::Key { .. } => "key",
            egui::Event::Text(_) => "text",
            egui::Event::ModifiersChanged(_) => "modifiers",
            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => "clipboard",
            egui::Event::WindowFocused(_) => "focus",
            egui::Event::Ime(_) => "ime",
            egui::Event::Touch { .. } => "touch",
            egui::Event::Screenshot { .. } => "screenshot",
            _ => "other",
        };
        *counts.entry(kind).or_default() += 1;
    }
    serde_json::json!(counts)
}

impl App {
    /// End the trace frame: input count, eframe's CPU time, repaint causes, and once a second the
    /// memory counters (process, documents, undo snapshots, egui textures and text caches).
    fn trace_frame_end(&mut self, ctx: &egui::Context) {
        use vizcraft_engine::trace;
        if !trace::enabled() {
            return;
        }
        if ctx.has_requested_repaint() {
            // Registry paths are shortened to the crate: `egui-0.36.2/src/context.rs:4024`.
            let short = |f: &str| f.rsplit_once("/registry/src/").and_then(|(_, r)| r.split_once('/')).map_or(f.to_string(), |(_, r)| r.to_string());
            trace::repaint_causes(
                ctx.repaint_causes().into_iter().map(|c| format!("{}:{} {}", short(c.file), c.line, c.reason).trim_end().to_string()),
            );
        }
        if let Some(cpu) = self.prev_cpu {
            // Layout, tessellation and paint of the previous frame, which the spans do not see.
            trace::sample_ms("eframe.previousFrameCpu", f64::from(cpu) * 1000.0);
        }
        let (input, kinds) = ctx.input(|i| (i.raw.events.len(), input_kinds(&i.raw.events)));
        let windows = 1 + self.windows.apps_mut().count();
        self.trace_frames += 1;
        if self.trace_frames == 5 {
            // Display facts, once egui's frame-interval prediction has settled.
            let (dt, inner, monitor) = ctx.input(|i| (i.predicted_dt, i.viewport_rect().size(), i.viewport().monitor_size));
            trace::session(serde_json::json!({
                "pixelsPerPoint": ctx.pixels_per_point(),
                "predictedDtMs": (f64::from(dt) * 1e5).round() / 100.0,
                "innerSize": [inner.x, inner.y],
                "monitorSize": monitor.map(|m| [m.x, m.y]),
                "windows": windows,
            }));
        }
        trace::frame_end(serde_json::json!({
            "input": input,
            "inputKinds": kinds,
            "synthetic": self.root.synthetic_injected(),
            // The UI asked to be drawn again: the next gap is a wait on eframe and the platform,
            // not on the user.
            "repaintPending": ctx.has_requested_repaint(),
            "prevCpuMs": self.prev_cpu.map(|s| (f64::from(s) * 1e5).round() / 100.0),
            "windows": windows,
        }));
        let now = std::time::Instant::now();
        if self.mem_sampled.is_some_and(|t| now.saturating_duration_since(t) < std::time::Duration::from_secs(1)) {
            return;
        }
        self.mem_sampled = Some(now);
        let mut v = self.root.session.memory_stats();
        let mb = |b: usize| (b as f64 / 1e4).round() / 100.0;
        if let Some(o) = v.as_object_mut() {
            if let Some(m) = memory_stats::memory_stats() {
                o.insert("rssMb".into(), serde_json::json!(mb(m.physical_mem)));
            }
            let textures: usize = ctx.tex_manager().read().allocated().map(|(_, m)| m.bytes_used()).sum();
            o.insert("eguiTexturesMb".into(), serde_json::json!(mb(textures)));
            o.insert("eguiGalleys".into(), serde_json::json!(ctx.fonts(|f| f.num_galleys_in_cache())));
            o.insert("fillCache".into(), serde_json::json!(self.root.canvas.fill_cache_len()));
            let others: usize = self.windows.apps_mut().map(|a| a.session.memory_stats()["undoEstMb"].as_f64().unwrap_or(0.0) as usize).sum();
            o.insert("otherWindowsUndoEstMb".into(), serde_json::json!(others));
        }
        trace::counter("memory", v);
    }
}

impl App {
    /// A new document window (File > New Window, Cmd+N), with the preferences of the window in front.
    fn open_window(&mut self, ctx: &egui::Context, template: &str) {
        let prefs = self.windows.front(&mut self.root).prefs();
        let mut app = CraftApp::new(Session::new(vizcraft_engine::Document::new()), services());
        app.apply_prefs(prefs);
        app.ui.window = None;
        app.integrated_titlebar = cfg!(target_os = "macos");
        if template != "blank"
            && let Err(e) = app.run("file.new", serde_json::json!({"template": template}))
        {
            eprintln!("vizcraft: new {template}: {e}");
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
                _ => vizcraft_ui_egui::menus::run_item(self.windows.front(&mut self.root), ctx, &id, params),
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Whether the last frame asked for an immediate repaint decides if the gap before this
        // one was a wait on the platform or on the user.
        vizcraft_engine::trace::frame_begin(Some(ctx.requested_repaint_last_pass()));
        let _t = vizcraft_engine::trace::span("app.logic");
        let mut quit = false;
        #[cfg(target_os = "macos")]
        self.menu_events(ctx, &mut quit);
        // Requests the other windows made (last frame's layout, or a menu item just now).
        let (mut new_windows, quit_all) = self.windows.take_requests();
        quit |= quit_all;
        // Documents Finder asked us to open (double click, Open With, the Dock icon).
        for path in vizcraft_macos_open::take_pending() {
            self.open_document(ctx, &path);
        }
        self.root.logic(ctx);
        let prev = self.root.ui.window;
        self.root.ui.window = ctx.input(|i| WindowGeometry::track(prev, i.viewport(), i.viewport_rect().size()));
        if let Some(pos) = self.restored.take().and_then(|g| ctx.input(|i| g.rescue_position(i.viewport()))) {
            ctx.viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.prev_cpu = frame.info().cpu_usage;
        self.root.ui(ui);
        let ctx = ui.ctx().clone();
        {
            let _t = vizcraft_engine::trace::span("app.otherWindows");
            self.windows.show(&ctx);
        }
        self.trace_frame_end(&ctx);
    }
    fn on_exit(&mut self) {
        save_prefs(&self.root);
        vizcraft_engine::trace::finish();
    }
}

/// `ui.json` in the platform's per-user configuration directory.
fn prefs_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support/VizCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("VizCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .map(|c| c.join("vizcraft"))
    };
    base.map(|b| b.join("ui.json"))
}

fn no_prefs() -> bool {
    std::env::var_os("VIZCRAFT_NO_PREFS").is_some()
}

/// Rebuild the notation catalog from the built-in packs plus the pack files in the `packs`
/// folder beside the preferences file, before anything reads the catalog (the stencil sets are
/// built from it once). Skipped with `VIZCRAFT_NO_PREFS`, so test runs see only the built-in
/// packs, and when `VIZCRAFT_PACKS_DIR` names a folder, which the catalog reads by itself. A
/// folder that fails to load keeps the built-in catalog; the reason goes to stderr.
fn load_packs() {
    if no_prefs() || std::env::var_os(vizcraft_notation::catalog::PACKS_DIR_ENV).is_some_and(|d| !d.is_empty()) {
        return;
    }
    let Some(dir) = prefs_path().and_then(|p| p.parent().map(|d| d.join("packs"))) else { return };
    match vizcraft_notation::catalog::use_packs_dir(&dir) {
        Ok(c) => {
            for e in c.errors() {
                eprintln!("vizcraft: pack in {}: {e}", dir.display());
            }
        }
        Err(e) => eprintln!("vizcraft: packs folder {}: {e}", dir.display()),
    }
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

/// Native file pickers and printing. Each blocks the UI thread while it runs, so the trace
/// records them as `blocking.*` spans: a long gap that is a file dialog is not a stall.
fn services() -> Services {
    Services {
        pick_open: Some(Box::new(|purpose: &str| {
            let _t = vizcraft_engine::trace::span("blocking.pickOpen");
            let dialog = if purpose == "import" {
                rfd::FileDialog::new().add_filter("Diagrams to import", &["drawio", "xml", "mmd", "mermaid", "csv"])
            } else {
                rfd::FileDialog::new().add_filter("VizCraft documents", &["vizcraft", "json"]).add_filter("draw.io diagrams", &["drawio"])
            };
            dialog.add_filter("All files", &["*"]).pick_file().map(|p| p.to_string_lossy().to_string())
        })),
        pick_save: Some(Box::new(|name: &str| {
            let _t = vizcraft_engine::trace::span("blocking.pickSave");
            let mut dialog = rfd::FileDialog::new().set_file_name(name);
            // Save As offers PDF next to the document formats; other saves (export) keep the
            // name they suggest.
            if name.ends_with(&format!(".{}", vizcraft_engine::io::EXTENSION)) {
                dialog = dialog
                    .add_filter("VizCraft documents", &[vizcraft_engine::io::EXTENSION])
                    .add_filter("draw.io diagrams", &["drawio"])
                    .add_filter("PDF", &["pdf"]);
            }
            dialog.save_file().map(|p| p.to_string_lossy().to_string())
        })),
        print: Some(Box::new(|path: &str| {
            let _t = vizcraft_engine::trace::span("blocking.print");
            print::send(path)
        })),
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
        .with_title("VizCraft")
        .with_app_id("com.storytold.vizcraft")
        .with_inner_size([1440.0, 920.0])
        .with_min_inner_size([vizcraft_ui_egui::window_geometry::MIN_SIZE.x, vizcraft_ui_egui::window_geometry::MIN_SIZE.y])
        .with_drag_and_drop(true)
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    // On macOS an empty icon stops eframe replacing the bundle's Dock icon with its default logo.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon(egui::IconData::default());
    builder
}

/// Start the performance trace: `VIZCRAFT_TRACE_DIR`, else `traces/` at the top of the source
/// tree this binary was built from, else `traces/` in the working directory.
fn start_trace() {
    let dir = std::env::var_os("VIZCRAFT_TRACE_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| {
        let tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if tree.join("Cargo.toml").is_file() { tree.join("traces") } else { std::path::PathBuf::from("traces") }
    });
    let env_ms = |name: &str, default: f64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let options = vizcraft_engine::trace::Options {
        detail_ms: env_ms("VIZCRAFT_TRACE_DETAIL_MS", 8.0),
        stall_ms: env_ms("VIZCRAFT_TRACE_STALL_MS", 100.0),
        sample_stalls: std::env::var_os("VIZCRAFT_TRACE_NO_SAMPLE").is_none(),
        ..Default::default()
    };
    match vizcraft_engine::trace::start(&dir, options) {
        Ok(path) => eprintln!("vizcraft: tracing to {}", path.display()),
        Err(e) => eprintln!("vizcraft: could not start the trace: {e}"),
    }
}

fn main() -> eframe::Result {
    load_packs();
    let mut control_port: Option<u16> = std::env::var("VIZCRAFT_CONTROL_PORT").ok().and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut sample = false;
    let mut in_window_menus = std::env::var_os("VIZCRAFT_IN_WINDOW_MENUS").is_some_and(|v| v != "0" && !v.is_empty());
    let mut window_pos = None;
    let mut trace = std::env::var_os("VIZCRAFT_TRACE").is_some_and(|v| v != "0" && !v.is_empty());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--in-window-menus" => in_window_menus = true,
            "--window-pos" => window_pos = args.next().and_then(|v| parse_window_pos(&v)),
            "--trace" => trace = true,
            "--version" => {
                println!("vizcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("vizcraft [--control <port>] [--sample] [--in-window-menus] [--window-pos x,y] [--trace] [--version] [files...]");
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
    if trace {
        start_trace();
    }
    let present_mode = format!("{:?}", options.wgpu_options.surface.present_mode);
    let frame_latency = options.wgpu_options.surface.desired_maximum_frame_latency;
    let n_files = files.len();
    vizcraft_macos_open::install();
    eframe::run_native(
        "VizCraft",
        options,
        Box::new(move |cc| {
            if vizcraft_engine::trace::enabled() {
                // What this run is: build, GPU and surface, flags. The display facts follow once
                // frames run (`trace_frame_end`).
                let adapter = cc.wgpu_render_state.as_ref().map(|rs| {
                    let info = rs.adapter.get_info();
                    serde_json::json!({
                        "name": info.name,
                        "backend": format!("{:?}", info.backend),
                        "deviceType": format!("{:?}", info.device_type),
                        "driver": info.driver,
                        "driverInfo": info.driver_info,
                    })
                });
                vizcraft_engine::trace::session(serde_json::json!({
                    "app": env!("CARGO_PKG_VERSION"),
                    "commit": env!("VIZCRAFT_GIT_COMMIT"),
                    "debugAssertions": cfg!(debug_assertions),
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                    "adapter": adapter,
                    "presentMode": present_mode,
                    "desiredMaximumFrameLatency": frame_latency,
                    "controlPort": control_port,
                    "inWindowMenus": in_window_menus,
                    "sampleDocument": sample,
                    "filesOnCommandLine": n_files,
                }));
            }
            let doc = if sample { vizcraft_engine::sample::sample_document() } else { vizcraft_engine::Document::new() };
            {
                let ctx = cc.egui_ctx.clone();
                vizcraft_macos_open::set_waker(move || {
                    vizcraft_engine::trace::note("open: request");
                    ctx.request_repaint();
                });
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
                    eprintln!("vizcraft: {f}: {e}");
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
                prev_cpu: None,
                mem_sampled: None,
                trace_frames: 0,
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
