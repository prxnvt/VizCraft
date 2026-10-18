//! VizCraft's egui front end (L6): an Office-style window (title bar with quick access
//! toolbar, optional in-window menu bar, ribbon, side panes, canvas, status bar, dialogs) over
//! `vizcraft-engine`.
//!
//! The UI is thin: it reads `Session` state and acts through [`CraftApp::run`] (command ids), so
//! menus, ribbon, shortcuts, the palette and the control channel all reach the same behaviour.
//! This crate depends on `egui` only; the host (eframe, web) drives [`CraftApp::logic`] and
//! [`CraftApp::ui`] each frame.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod backstage;
pub mod canvas;
pub mod chrome;
pub mod control;
pub mod dialogs;
pub mod icons;
pub mod keys;
pub mod menus;
pub mod page_setup;
pub mod page_tabs;
pub mod panes;
pub mod ribbon;
pub mod theme;
pub mod widgets;
pub mod window_geometry;

use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vizcraft_engine::Session;

pub use control::ControlRequest;

/// Commands that live in the UI layer: `(id, label, shortcut, params)`. Listed so the palette,
/// menus and control channel see them next to the engine's.
pub const UI_COMMANDS: &[(&str, &str, &str, &str)] = &[
    ("ui.tab", "Switch Ribbon Tab", "", r#"{"tab": "File|Home|Insert|Design|View|Help|Connector"}"#),
    ("ui.collapseRibbon", "Collapse the Ribbon", "Mod+F1", r#"{"value"?: bool}"#),
    ("ui.commands", "Search Commands", "Mod+Shift+P", "{}"),
    (
        "ui.dialog",
        "Open Dialog",
        "",
        r#"{"name": "commands|zoom|about|preferences|quit|shortcuts|message|pageSetup", "title"?: string, "text"?: string}"#,
    ),
    ("ui.closeDialog", "Close Dialog", "", "{}"),
    ("ui.backstage", "File Page", "", r#"{"value"?: bool}"#),
    ("ui.openFileDialog", "Open...", "", "{}"),
    ("ui.saveAsDialog", "Save As...", "", "{}"),
    ("ui.inWindowMenus", "Menu Bar in Window", "", r#"{"value"?: bool}"#),
    ("ui.newWindow", "New Window", "", r#"{"template"?: "blank|sample"}"#),
    ("ui.pane", "Toggle Pane", "", r#"{"name": "shapes|navigation|properties|history", "value"?: bool, "side"?: "left|right"}"#),
    ("ui.pane.shapes", "Shapes Pane", "", r#"{"value"?: bool}"#),
    ("ui.pane.navigation", "Navigation Pane", "", r#"{"value"?: bool}"#),
    ("ui.pane.properties", "Properties Pane", "", r#"{"value"?: bool}"#),
    ("ui.pane.history", "History Pane", "", r#"{"value"?: bool}"#),
    ("ui.paneSide", "Move Pane to Side", "", r#"{"name": "shapes|navigation|properties|history", "side": "left|right"}"#),
    ("ui.collapsePanes", "Collapse Side Panes", "", r#"{"side": "left|right", "value"?: bool}"#),
    ("ui.panesReset", "Reset Panes", "", "{}"),
    ("ui.shapesSearch", "Search Shapes Pane", "", r#"{"query": string} -> {"query", "results": [master id]}"#),
    ("ui.clearRecent", "Clear Recent Files", "", "{}"),
    ("ui.shortcuts", "Keyboard Shortcuts", "Mod+/", "{}"),
    ("ui.print", "Print", "Mod+P", "{}"),
    ("ui.tool", "Tool", "", r#"{"tool": "pointer|connector|text"}"#),
    ("ui.find", "Find", "Mod+F", "{}"),
    ("window.minimize", "Minimize", "", "{}"),
    ("window.zoom", "Zoom Window", "", "{}"),
    ("app.about", "About VizCraft", "", "{}"),
    ("app.preferences", "Preferences", "Mod+,", "{}"),
    ("app.quit", "Quit VizCraft", "Mod+Q", "{}"),
];

/// The canvas tool: what a left drag on an empty part of the page does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Select, move, resize and rotate (the default).
    #[default]
    Pointer,
    /// Drag from a shape or empty space to draw a connector.
    Connector,
    /// Click to add a text block, or edit the text of the shape under the pointer.
    Text,
}

impl Tool {
    /// The id used by `ui.tool` and `ui.inspect`.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pointer => "pointer",
            Tool::Connector => "connector",
            Tool::Text => "text",
        }
    }
    /// The tool for a `ui.tool` name.
    pub fn parse(s: &str) -> Option<Tool> {
        match s {
            "pointer" => Some(Tool::Pointer),
            "connector" => Some(Tool::Connector),
            "text" => Some(Tool::Text),
            _ => None,
        }
    }
}

/// Platform services injected by the host (file dialogs, file I/O). All optional: without a
/// picker the matching commands simply do nothing.
#[derive(Default)]
pub struct Services {
    /// Pick a file to open; the argument says what for (`document`, `picture`).
    pub pick_open: Option<Box<dyn Fn(&str) -> Option<String>>>,
    /// Pick a path to save to, given a suggested name.
    pub pick_save: Option<Box<dyn Fn(&str) -> Option<String>>>,
    /// Web: open a file picker; the file arrives later through `inbox`.
    pub open_async: Option<Box<dyn Fn(&str)>>,
    /// Web: files (name, bytes) delivered asynchronously (picker, drag and drop).
    pub inbox: Option<Inbox>,
    /// Web: hand bytes to the browser as a download.
    pub download: Option<Box<dyn Fn(&str, &[u8])>>,
}

/// Files delivered asynchronously.
pub type Inbox = std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

/// UI state that persists between runs (`ui.json`) and that agents read and set (`ui.inspect`,
/// `ui.set`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    /// The ribbon tab showing.
    pub tab: String,
    pub ribbon_collapsed: bool,
    pub dark: bool,
    /// Recently opened files, newest first.
    pub recent: Vec<String>,
    /// User name (Preferences); empty keeps the default.
    pub author: String,
    pub panes: panes::PaneLayout,
    /// Desktop: the main window's size and position, restored at the next launch.
    pub window: Option<window_geometry::WindowGeometry>,
    /// Draw the menu bar inside the window even where the platform has a native one.
    pub in_window_menus: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            tab: "Home".into(),
            ribbon_collapsed: false,
            dark: false,
            recent: Vec::new(),
            author: String::new(),
            panes: panes::PaneLayout::default(),
            window: None,
            in_window_menus: false,
        }
    }
}

/// The application: one document window.
pub struct CraftApp {
    pub session: Session,
    pub ui: UiState,
    pub services: Services,
    pub dialog: Option<dialogs::Dialog>,
    /// Status bar message and when it was set (it fades after a few seconds).
    pub status_msg: Option<(String, f64)>,
    /// macOS: the window has no title bar; leave room for the traffic lights.
    pub integrated_titlebar: bool,
    /// The host shows several document windows in one process: `new_window` and Cmd+Q become
    /// requests to the host instead of acting on this window alone.
    pub multi_window: bool,
    /// New window wanted, with this template (taken by the host when `multi_window`).
    pub new_window_request: Option<String>,
    /// Quit the whole app, asking about each window's unsaved changes (taken by the host).
    pub quit_all_request: bool,
    /// This window wants to close (the host closes it).
    pub quit_requested: bool,
    /// Quit was confirmed (changes saved or discarded, or an automation quit): let the window close.
    pub quit_confirmed: bool,
    /// Time the last frame's layout took.
    pub frame_ms: f64,
    /// What a left drag on the canvas does; set by `ui.tool` from the ribbon, keys or an agent.
    pub tool: Tool,
    /// Canvas interaction state (drag preview, text editing, hover).
    pub canvas: canvas::CanvasState,
    /// Text in the Shapes pane's search field (`ui.shapesSearch`). Transient: not in [`UiState`],
    /// so it is never saved with the preferences.
    pub shapes_query: String,
    /// The File page is showing instead of the document.
    backstage: bool,
    control_rx: Option<Receiver<ControlRequest>>,
    shot_token: u64,
    /// Screenshots waiting to be issued: (token, not before, frames waited).
    queued_shots: Vec<(u64, f64, u32)>,
    /// Screenshots issued and waiting for the frame: (token, path, reply, deadline).
    pending_shots: Vec<(u64, Option<String>, std::sync::mpsc::Sender<Value>, f64)>,
    /// `ui.wait` requests: (frames still to run, reply).
    pending_waits: Vec<(u32, std::sync::mpsc::Sender<Value>)>,
    /// Input events injected by the control channel.
    pub(crate) synthetic: Vec<egui::Event>,
    styled: bool,
    fonts_frames: u32,
    applied_dark: Option<bool>,
}

impl CraftApp {
    pub fn new(session: Session, services: Services) -> Self {
        CraftApp {
            session,
            ui: UiState::default(),
            services,
            dialog: None,
            status_msg: None,
            integrated_titlebar: false,
            multi_window: false,
            new_window_request: None,
            quit_all_request: false,
            quit_requested: false,
            quit_confirmed: false,
            frame_ms: 0.0,
            tool: Tool::Pointer,
            canvas: canvas::CanvasState::default(),
            shapes_query: String::new(),
            backstage: false,
            control_rx: None,
            shot_token: 0,
            queued_shots: Vec::new(),
            pending_shots: Vec::new(),
            pending_waits: Vec::new(),
            synthetic: Vec::new(),
            styled: false,
            fonts_frames: 0,
            applied_dark: None,
        }
    }

    /// Attach the control channel (requests arrive from the host's transport thread).
    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        self
    }

    /// Hand over the control channel (a multi-window host moves it to the window that takes the
    /// first window's place).
    pub fn take_control(&mut self) -> Option<Receiver<ControlRequest>> {
        self.control_rx.take()
    }

    /// Preferences to save between runs: the UI state plus the session's name and dark mode.
    pub fn prefs(&self) -> UiState {
        let mut ui = self.ui.clone();
        ui.author = self.session.author.clone();
        ui.dark = self.session.view.dark_mode;
        ui
    }

    /// Restore preferences saved by [`CraftApp::prefs`].
    pub fn apply_prefs(&mut self, ui: UiState) {
        self.ui = ui;
        self.backstage = false;
        // The session owns the name and dark mode from here on; `prefs` copies them back.
        let author = std::mem::take(&mut self.ui.author);
        if !author.trim().is_empty() {
            self.session.author = author;
        }
        self.session.view.dark_mode = self.ui.dark;
        self.ui.panes.sanitize();
    }

    /// Run a command; UI-level commands (`ui.*`, `app.*`) are handled here, the rest by the engine.
    /// [`CraftApp::run`] on behalf of `origin`, so the engine's transaction log records who made
    /// each step (the control channel passes `Origin::Control`).
    pub fn run_as(&mut self, origin: vizcraft_engine::Origin, id: &str, params: Value) -> Result<Value, String> {
        let before = self.session.set_origin(origin);
        let r = self.run(id, params);
        self.session.set_origin(before);
        r
    }

    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        if let Some(r) = self.ui_command(id, &params) {
            return r;
        }
        // Web: saving becomes a download.
        if self.services.download.is_some() && matches!(id, "file.save" | "file.saveAs") {
            let name = params
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}.{}", self.title_stem(), vizcraft_engine::io::EXTENSION));
            let bytes = vizcraft_engine::io::save_bytes(&name, &self.session.doc).map_err(|e| e.to_string())?;
            if let Some(d) = &self.services.download {
                d(&name, &bytes);
            }
            self.session.mark_saved();
            return Ok(json!({"downloaded": name, "bytes": bytes.len()}));
        }
        let rev = self.session.rev();
        let r = self.session.run(id, &params).map_err(|e| e.to_string());
        self.after_command();
        if let Err(e) = &r {
            self.status(e.clone());
        }
        if r.is_ok() && self.session.rev() != rev && fits_after(id, &params) {
            self.fit_if_cut_off();
        }
        r
    }

    /// Fit the page into the canvas when the canvas cuts it off at the current zoom, so a diagram
    /// that was just opened or imported is seen whole; a page that already fits keeps its zoom.
    /// Before the canvas has been laid out (a file opened at launch) the check waits for it.
    fn fit_if_cut_off(&mut self) {
        let Some([w, h]) = self.session.view.canvas else {
            self.canvas.fit_pending = true;
            return;
        };
        let Some(page) = self.session.doc.page() else { return };
        let cut = |extent: f64, view: f32| extent as f32 * self.session.view.zoom > view - 2.0 * vizcraft_engine::cmd::view::FIT_MARGIN;
        if cut(page.width, w) || cut(page.height, h) {
            let _ = self.run("view.zoomFit", json!({}));
        }
    }

    /// Bookkeeping after an engine command: UI requests, recent files, status.
    fn after_command(&mut self) {
        for req in std::mem::take(&mut self.session.ui_requests) {
            self.handle_request(&req);
        }
        if let Some(p) = self.session.path.as_ref().map(|p| p.to_string_lossy().to_string())
            && self.ui.recent.first() != Some(&p)
        {
            self.ui.recent.retain(|r| *r != p);
            self.ui.recent.insert(0, p);
            self.ui.recent.truncate(12);
        }
        if !self.session.status.is_empty() {
            let s = std::mem::take(&mut self.session.status);
            self.status(s);
        }
    }

    /// Requests commands make of the UI (open a dialog, close the window).
    fn handle_request(&mut self, req: &Value) {
        if let Some(what) = req.get("open").and_then(Value::as_str) {
            match what {
                "openFile" => self.open_dialog(),
                "saveAs" => self.save_as_dialog(),
                "export" => self.export_dialog(),
                "import" => self.import_dialog(),
                other => self.dialog = dialogs::Dialog::open(other, self),
            }
        }
        if req.get("close").is_some() {
            self.close_window();
        }
    }

    /// Close this window, asking about unsaved changes first.
    fn close_window(&mut self) {
        if self.has_unsaved() {
            self.ask_before_closing();
        } else {
            self.quit_requested = true;
        }
    }

    /// Commands that live in the UI layer. `None` when `id` is not one of them.
    fn ui_command(&mut self, id: &str, p: &Value) -> Option<Result<Value, String>> {
        if let Some(r) = panes::ui_command(self, id, p) {
            return Some(r);
        }
        let s = |k: &str| p.get(k).and_then(Value::as_str);
        let b = |k: &str| p.get(k).and_then(Value::as_bool);
        Some(Ok(match id {
            "ui.tool" => {
                // Unknown or missing names fall back to the pointer, so junk never strands the canvas.
                self.tool = s("tool").and_then(Tool::parse).unwrap_or(Tool::Pointer);
                json!({"tool": self.tool.name()})
            }
            "ui.tab" => {
                let tab = s("tab").unwrap_or("Home");
                if tab == "File" {
                    self.backstage = true;
                } else {
                    self.ui.tab = tab.to_string();
                    self.backstage = false;
                }
                json!({"tab": self.ui.tab, "backstage": self.backstage})
            }
            "ui.collapseRibbon" => {
                self.ui.ribbon_collapsed = b("value").unwrap_or(!self.ui.ribbon_collapsed);
                json!({"collapsed": self.ui.ribbon_collapsed})
            }
            "ui.commands" => {
                self.dialog = dialogs::Dialog::open("commands", self);
                json!({"dialog": "commands"})
            }
            "ui.dialog" => {
                self.dialog = match s("name").unwrap_or("") {
                    "message" => Some(dialogs::Dialog::Message {
                        title: s("title").unwrap_or("VizCraft").to_string(),
                        text: s("text").unwrap_or("").to_string(),
                    }),
                    name => dialogs::Dialog::open(name, self),
                };
                json!({"dialog": self.dialog.as_ref().map(|d| d.name())})
            }
            "ui.closeDialog" => {
                self.dialog = None;
                json!({})
            }
            "ui.backstage" => {
                self.backstage = b("value").unwrap_or(!self.backstage);
                json!({"backstage": self.backstage})
            }
            "ui.openFileDialog" => {
                self.open_dialog();
                json!({})
            }
            "ui.saveAsDialog" => {
                self.save_as_dialog();
                json!({})
            }
            "ui.inWindowMenus" => {
                self.ui.in_window_menus = b("value").unwrap_or(!self.ui.in_window_menus);
                json!({"inWindowMenus": self.ui.in_window_menus})
            }
            "ui.newWindow" => {
                self.new_window(s("template").unwrap_or("blank"));
                json!({})
            }
            "ui.clearRecent" => {
                self.ui.recent.clear();
                json!({"recent": []})
            }
            "ui.shortcuts" => {
                self.dialog = dialogs::Dialog::open("shortcuts", self);
                json!({"dialog": "shortcuts"})
            }
            // Placeholders (`menus::PLACEHOLDERS`): listed so every front sees them, shown
            // disabled in menus; running one by id only reports that it is not there yet.
            "ui.print" | "ui.find" | "window.minimize" | "window.zoom" => {
                self.status(format!("{} is not available in this craft yet", widgets::label_of(self, id)));
                json!({"placeholder": true})
            }
            "app.about" => {
                self.dialog = dialogs::Dialog::open("about", self);
                json!({"dialog": "about"})
            }
            "app.preferences" => {
                self.dialog = dialogs::Dialog::open("preferences", self);
                json!({"dialog": "preferences"})
            }
            "app.quit" => {
                // Quit goes through the close path, so unsaved changes are asked about; with
                // several windows the host asks each in turn.
                if self.multi_window {
                    self.quit_all_request = true;
                } else {
                    self.close_window();
                }
                json!({})
            }
            _ => return None,
        }))
    }

    /// Show a status bar message.
    pub fn status(&mut self, s: impl Into<String>) {
        self.status_msg = Some((s.into(), now_ms()));
    }

    /// The File page is showing.
    pub fn backstage_open(&self) -> bool {
        self.backstage
    }

    fn open_dialog(&mut self) {
        if let Some(f) = &self.services.open_async {
            f("document");
            return;
        }
        let picked = self.services.pick_open.as_ref().and_then(|f| f("document"));
        if let Some(path) = picked {
            let _ = self.run("file.open", json!({"path": path}));
            self.backstage = false;
        }
    }

    /// Ask for a draw.io, Mermaid or CSV file and import it onto the page (`file.import` without
    /// a path). On the web the picked file arrives through the inbox, which imports anything
    /// that is not a document.
    pub fn import_dialog(&mut self) {
        if let Some(f) = &self.services.open_async {
            f("import");
            return;
        }
        let picked = self.services.pick_open.as_ref().and_then(|f| f("import"));
        if let Some(path) = picked {
            let _ = self.run("file.import", json!({"path": path}));
            self.backstage = false;
        }
    }

    /// Ask for a path and save there.
    pub fn save_as_dialog(&mut self) {
        let name = format!("{}.{}", self.title_stem(), vizcraft_engine::io::EXTENSION);
        let picked = self.services.pick_save.as_ref().and_then(|f| f(&name));
        if let Some(path) = picked {
            let _ = self.run("file.save", json!({"path": path}));
        }
    }

    /// Ask for an SVG path and export the page there (`file.export` without a path). On the web
    /// the SVG becomes a download instead.
    pub fn export_dialog(&mut self) {
        let name = format!("{}.svg", self.title_stem());
        if let Some(d) = &self.services.download {
            match vizcraft_engine::io::export_bytes(&name, Some("svg"), &self.session.doc) {
                Ok(bytes) => d(&name, &bytes),
                Err(e) => self.status(e.to_string()),
            }
            return;
        }
        let picked = self.services.pick_save.as_ref().and_then(|f| f(&name));
        if let Some(path) = picked {
            let _ = self.run("file.export", json!({"path": path}));
        }
    }

    /// Document title for the title bar: the file's stem, else the document title, else "Untitled".
    pub fn title_stem(&self) -> String {
        match &self.session.path {
            Some(p) => {
                let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let stem = vizcraft_engine::io::strip_extension(&name);
                if stem.is_empty() { "Untitled".into() } else { stem.to_string() }
            }
            None if self.session.doc.title.is_empty() => "Untitled".into(),
            None => self.session.doc.title.clone(),
        }
    }

    /// Per-frame logic before layout: style, control requests, screenshots, shortcuts, close
    /// requests, file drops.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.styled {
            theme::install_fonts(ctx);
            // Mod with -, = and 0 are our zoom commands; egui's keyboard zoom would also scale the
            // whole window on them.
            ctx.options_mut(|o| o.zoom_with_keyboard = false);
            self.styled = true;
        }
        let dark = self.session.view.dark_mode;
        if self.applied_dark != Some(dark) {
            theme::apply(ctx, &if dark { theme::Tokens::dark() } else { theme::Tokens::light() });
            self.applied_dark = Some(dark);
        }
        self.tick_waits(ctx);
        self.drain_control(ctx);
        self.drain_inbox();
        self.collect_screenshots(ctx);
        self.issue_screenshots(ctx);
        keys::shortcuts(self, ctx);
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && self.has_unsaved() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.asking_before_closing() {
                self.ask_before_closing();
            }
        }
        if !self.multi_window && std::mem::take(&mut self.quit_requested) {
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.synthetic.is_empty() {
            ctx.request_repaint();
        }
        #[cfg(not(target_arch = "wasm32"))]
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            let _ = self.run("file.open", json!({"path": f.path().to_string_lossy()}));
        }
    }

    /// Open a new document in a new window of this app (the host opens it). Without a
    /// multi-window host (web) the new document replaces the current one.
    pub fn new_window(&mut self, template: &str) {
        if self.multi_window {
            self.new_window_request = Some(template.to_string());
            return;
        }
        let _ = self.run("file.new", json!({"template": template}));
    }

    /// Unsaved changes a close would lose (and the user has not yet said to discard them).
    pub fn has_unsaved(&self) -> bool {
        self.session.dirty && !self.quit_confirmed
    }

    /// Show the Save / Don't Save / Cancel prompt for closing this window.
    pub fn ask_before_closing(&mut self) {
        self.dialog = Some(dialogs::Dialog::Quit);
    }

    /// The Save / Don't Save / Cancel prompt is showing.
    pub fn asking_before_closing(&self) -> bool {
        matches!(self.dialog, Some(dialogs::Dialog::Quit))
    }

    /// Inject synthetic input events from the control channel (one pointer event per frame, so
    /// hover and click land on the widget egui has laid out).
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
        if self.synthetic.is_empty() {
            return;
        }
        let n = match self.synthetic.first() {
            Some(egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }) => 1,
            _ => self.synthetic.iter().position(|e| matches!(e, egui::Event::Key { pressed: false, .. })).map_or(self.synthetic.len(), |i| i + 1),
        };
        if let Some(egui::Event::PointerMoved(p) | egui::Event::PointerButton { pos: p, .. }) = self.synthetic.first() {
            raw.events.push(egui::Event::PointerMoved(*p));
        }
        let n = n.min(self.synthetic.len());
        raw.events.extend(self.synthetic.drain(..n));
    }

    /// Lay out the window: title bar, menu bar, ribbon, status bar, panes, canvas, then dialogs.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let t0 = now_ms();
        let ctx = ui.ctx().clone();
        // Fonts installed by `logic` take effect on the next frame; drawing with the old atlas
        // for two frames shows a flash of the default font.
        if self.fonts_frames < 2 {
            self.fonts_frames += 1;
            ctx.request_repaint();
            return;
        }
        let t = theme::Tokens::get(&ctx);
        if self.backstage {
            backstage::show(self, ui);
        } else {
            chrome::title_bar(self, ui);
            if menus::in_window_menus(self) {
                menus::menu_bar(self, ui);
            }
            ribbon::show(self, ui);
            chrome::status_bar(self, ui);
            panes::show(self, ui);
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
                canvas::show(self, ui);
            });
        }
        dialogs::show(self, &ctx);
        let title = format!("{}{} - VizCraft", self.title_stem(), if self.session.dirty { " (edited)" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.frame_ms = now_ms() - t0;
    }

    /// Files that arrived asynchronously (web picker, drops).
    fn drain_inbox(&mut self) {
        let Some(inbox) = self.services.inbox.clone() else { return };
        let files = std::mem::take(&mut *inbox.lock().unwrap_or_else(|e| e.into_inner()));
        for (name, bytes) in files {
            // Mermaid, CSV and bare XML files are imported onto the page; documents open.
            if !vizcraft_engine::io::is_document(&name)
                && let Some(format) = vizcraft_engine::io::import_format_for_name(&name)
            {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let _ = self.run("file.import", json!({"text": text, "format": format}));
                self.backstage = false;
                continue;
            }
            match vizcraft_engine::io::load_bytes(&name, &bytes) {
                Ok(doc) => {
                    self.session.set_document(doc);
                    self.session.path = None;
                    self.session.doc.title = name;
                    self.backstage = false;
                }
                Err(e) => self.status(e.to_string()),
            }
        }
    }

    fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control_rx.take() else { return };
        while let Ok(req) = rx.try_recv() {
            let reply = req.reply.clone();
            match control::handle(self, ctx, &req) {
                control::Outcome::Done(v) => {
                    let _ = reply.send(v);
                }
                control::Outcome::Screenshot { path } => {
                    self.shot_token += 1;
                    let token = self.shot_token;
                    // Wait a little so whatever the request changed has been drawn.
                    self.queued_shots.push((token, now_ms() + 120.0, 0));
                    self.pending_shots.push((token, path, reply, now_ms() + 8000.0));
                }
                control::Outcome::Wait { frames } => {
                    self.pending_waits.push((frames, reply));
                    ctx.request_repaint();
                }
            }
        }
        self.control_rx = Some(rx);
    }

    /// Count down `ui.wait` requests, one frame each; reply when they reach zero.
    fn tick_waits(&mut self, ctx: &egui::Context) {
        if self.pending_waits.is_empty() {
            return;
        }
        self.pending_waits.retain_mut(|(frames, reply)| {
            *frames = frames.saturating_sub(1);
            if *frames == 0 {
                let _ = reply.send(json!({"ok": true, "result": {"waited": true}}));
                false
            } else {
                true
            }
        });
        if !self.pending_waits.is_empty() {
            ctx.request_repaint();
        }
    }

    fn issue_screenshots(&mut self, ctx: &egui::Context) {
        let now = now_ms();
        self.queued_shots.retain_mut(|(token, at, frames)| {
            *frames += 1;
            if now >= *at && *frames >= 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(*token)));
                false
            } else {
                true
            }
        });
        if !self.queued_shots.is_empty() || !self.pending_shots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn collect_screenshots(&mut self, ctx: &egui::Context) {
        if self.pending_shots.is_empty() {
            return;
        }
        let events: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { user_data, image, .. } => {
                        let token = user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).copied()?;
                        Some((token, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        for (token, image) in events {
            if let Some(i) = self.pending_shots.iter().position(|(t, ..)| *t == token) {
                let (_, path, reply, _) = self.pending_shots.remove(i);
                let _ = reply.send(control::save_screenshot(&image, path.as_deref()));
            }
        }
        let now = now_ms();
        self.pending_shots.retain(|(_, _, reply, deadline)| {
            if now < *deadline {
                return true;
            }
            let _ = reply.send(json!({"ok": false, "error": "no frame was presented (window hidden?)"}));
            false
        });
    }
}

/// Whether a successful `id` brings in a new diagram that should be fitted to the canvas: opening
/// a file, or an import that replaces a page. An import that adds to a page leaves the view where
/// the person put it. A caller that sets the view itself passes `"fit": false`.
fn fits_after(id: &str, params: &Value) -> bool {
    let replaces = match id {
        "file.open" => true,
        "file.import" => params.get("replace").and_then(Value::as_bool) == Some(true),
        _ => false,
    };
    replaces && params.get("fit").and_then(Value::as_bool) != Some(false)
}

/// Wall-clock milliseconds, for status fades and screenshot timing (on the web through
/// `web_time`, since `std`'s clock is unavailable there).
pub fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(target_arch = "wasm32")]
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default())
    }

    #[test]
    fn prefs_round_trip() {
        let mut first = app();
        first.run("file.setAuthor", json!({"name": "Ada Lovelace"})).unwrap();
        first.run("view.dark", json!({"value": true})).unwrap();
        first.run("ui.tab", json!({"tab": "View"})).unwrap();
        first.ui.panes.left_width = 300.0;
        let saved = serde_json::to_vec(&first.prefs()).unwrap();

        let mut second = app();
        second.apply_prefs(serde_json::from_slice(&saved).unwrap());
        assert_eq!(second.session.author, "Ada Lovelace");
        assert!(second.session.view.dark_mode);
        assert_eq!(second.ui.tab, "View");
        assert_eq!(second.ui.panes.left_width, 300.0);
        assert_eq!(second.prefs(), first.prefs());

        // A later rename is what gets saved next, not the name loaded at startup.
        second.run("file.setAuthor", json!({"name": "Grace Hopper"})).unwrap();
        assert_eq!(second.prefs().author, "Grace Hopper");

        // Prefs from an older build (missing fields) and an empty name keep the defaults.
        let mut a = app();
        let default = a.session.author.clone();
        a.apply_prefs(serde_json::from_str(r#"{"tab": "View", "author": "  "}"#).unwrap());
        assert_eq!(a.session.author, default);
        assert_eq!(a.ui.tab, "View");
        assert_eq!(a.ui.panes, panes::PaneLayout::default());
    }

    #[test]
    fn ui_commands_all_run() {
        let mut a = app();
        let junk = [json!({}), json!(null), json!({"tab": 5, "value": "x", "name": 1, "template": []})];
        for (id, ..) in UI_COMMANDS {
            for j in &junk {
                a.run(id, j.clone()).unwrap_or_else(|e| panic!("{id}: {e}"));
            }
        }
        // Every UI shortcut is unique and does not collide with an engine shortcut.
        let mut seen = std::collections::HashSet::new();
        for (id, _, sc, _) in UI_COMMANDS {
            if sc.is_empty() {
                continue;
            }
            assert!(seen.insert(*sc), "{id}: shortcut {sc} used twice");
            assert!(a.session.registry.by_shortcut(sc).is_none(), "{id}: shortcut {sc} is also an engine shortcut");
        }
        // Unknown ids are errors, not panics.
        assert!(a.run("ui.nothing", json!({})).is_err());
    }

    #[test]
    fn quit_asks_about_unsaved_changes() {
        let mut a = app();
        a.run("app.quit", json!({})).unwrap();
        assert!(a.quit_requested, "a clean document closes at once");
        let mut a = app();
        a.run("edit.setTitle", json!({"title": "changed"})).unwrap();
        a.run("app.quit", json!({})).unwrap();
        assert!(a.asking_before_closing());
        assert!(!a.quit_requested);
        a.multi_window = true;
        a.dialog = None;
        a.run("app.quit", json!({})).unwrap();
        assert!(a.quit_all_request, "with several windows the host runs the quit");
    }

    #[test]
    fn title_stem_prefers_the_file_name() {
        let mut a = app();
        assert_eq!(a.title_stem(), "Sample flowchart");
        a.session.doc.title.clear();
        assert_eq!(a.title_stem(), "Untitled");
        a.session.doc.title = "My Project".into();
        assert_eq!(a.title_stem(), "My Project");
        a.session.path = Some("/tmp/Plans.craft.json".into());
        assert_eq!(a.title_stem(), "Plans");
        a.session.path = None;
        a.session.doc.title.clear();
        assert_eq!(a.title_stem(), "Untitled");
    }

    #[test]
    fn export_without_a_path_asks_for_an_svg_name() {
        let dir = std::env::temp_dir().join(format!("vizcraft-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("chart.svg");
        let asked = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen = asked.clone();
        let target = out.to_string_lossy().to_string();
        let services = Services {
            pick_save: Some(Box::new(move |name| {
                *seen.lock().unwrap() = name.to_string();
                Some(target.clone())
            })),
            ..Services::default()
        };
        let mut a = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), services);
        a.run("file.export", json!({})).unwrap();
        assert_eq!(*asked.lock().unwrap(), "Sample flowchart.svg");
        let svg = std::fs::read_to_string(&out).unwrap();
        assert!(svg.contains("<svg"), "{svg}");
        let _ = std::fs::remove_dir_all(&dir);

        // The web build downloads the bytes instead.
        let got = std::sync::Arc::new(std::sync::Mutex::new((String::new(), 0usize)));
        let sink = got.clone();
        let services =
            Services { download: Some(Box::new(move |name, bytes| *sink.lock().unwrap() = (name.to_string(), bytes.len()))), ..Services::default() };
        let mut a = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), services);
        a.run("file.export", json!({})).unwrap();
        let (name, len) = got.lock().unwrap().clone();
        assert_eq!(name, "Sample flowchart.svg");
        assert!(len > 100);
    }
}
