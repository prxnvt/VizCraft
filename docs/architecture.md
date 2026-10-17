# VizCraft architecture (the contract)

VizCraft is a clean-room, open-source desktop diagramming app written in Rust and egui, aiming
for the feature range of Visio, with an MCP server shaped like draw.io's so agents can create and
edit diagrams. Today the window chrome, menus, ribbon, panes, multiple windows, preferences,
control channel and command registry are finished, around a placeholder document model; the
diagram model is planned (see [Planned (not yet built)](#planned-not-yet-built) and
[ROADMAP.md](../ROADMAP.md)).

This file is the contract agents build against in parallel. The sections up to Conventions
describe the code as it is. Change it only together with
the code that depends on it, and say so in the commit message.

## Layers (enforced by `cargo xtask layers`)

| Layer | Crate | Job |
|---|---|---|
| L4 | `crates/engine` (`vizcraft-engine`) | `Session`, undo, command registry, placeholder document model, I/O |
| L6 | `crates/ui-egui` (`vizcraft-ui-egui`) | egui front end: theme, chrome, menus, ribbon, panes, dialogs, keys, control |
| platform | `crates/macos-open` | Finder Open Documents (odoc) Apple Event, safe API |
| apps | `apps/vizcraft`, `apps/vizcraft-cli`, `apps/vizcraft-web` | desktop app, headless CLI, wasm shell (see [docs/web.md](web.md)) |
| tooling | `xtask` | `cargo xtask ci`, `layers`, `assets`, `wasm`, `ico`, `version` |

Nothing in `engine` depends on egui/eframe/winit/rfd/muda. `ui-egui` depends on `egui` only (not
eframe). Only `apps/*` touch eframe, rfd, muda. `unsafe_code = "forbid"` everywhere except
`macos-open` (`deny`, with one audited `imp` module).

## Everything is a command

User-visible behaviour is a `CommandSpec` in the engine registry:

```rust
pub struct CommandSpec {
    pub id: &'static str,          // "file.save", "edit.undo", "view.zoomIn"
    pub label: &'static str,       // "Save"
    pub location: &'static str,    // "Home > Clipboard" (ribbon / menu location, for tooltips and parity)
    pub shortcut: &'static str,    // "Mod+S" ("Mod" = Cmd on macOS, Ctrl elsewhere; " / " separates alternatives)
    pub params: &'static str,      // one-line params doc: "{path?: string}"
    pub enabled: fn(&Session) -> Option<&'static str>, // None = enabled, Some(reason) = disabled
    pub run: fn(&mut Session, &Value) -> Result<Value, EngineError>,
}
```

UI-only commands are `ui.*` and live in `ui-egui` (`CraftApp::ui_command`); they are listed in
`ui_egui::UI_COMMANDS: &[(&str /*id*/, &str /*label*/, &str /*shortcut*/, &str /*params*/)]` so
the palette, menus and control channel see them. The menus, ribbon, shortcuts, command palette,
CLI, control channel and (later) MCP all dispatch by id. Programmatic calls never open dialogs:
a command that needs one pushes `{"open": "<dialog>"}` to `session.ui_requests` and the UI
handles it.

Command id prefixes: `file.*`, `edit.*`, `view.*`, `insert.*`, `format.*`, `window.*`, `help.*`,
`transport.*` (playback, to be removed in Cascade 1), `ui.*` (UI layer), `app.*` (app-level:
`app.quit`, `app.preferences`, `app.about`). `insert.*` and `format.*` are reserved; no engine
command uses them yet.

## Engine (`vizcraft-engine`) public API

```rust
pub struct Session {
    pub doc: Document,                 // placeholder model: title and metadata (crates/engine/src/doc.rs)
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub author: String,
    pub sel: Selection,                // generalized selection (primary, items: Vec<String>, range: Option<(u64, u64)>)
    pub transport: TransportState,     // playback state (playing, time_ms, tempo, looping); removed in Cascade 1
    pub view: ViewState,               // zoom (f32), pan ([f32; 2]), dark_mode, pane visibility flags
    pub registry: Registry,            // Registry::get(id) -> Option<&CommandSpec>, Registry::all() -> &[CommandSpec]
    pub ui_requests: Vec<Value>,       // requests to the UI, drained by the UI after each command
    pub status: String,                // one-shot status message, taken by the UI
    pub clipboard: String,             // in-process clipboard for edit.cut / edit.copy / edit.paste
    // private: undo/redo stacks
}
impl Session {
    pub fn new(doc: Document) -> Self;
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, EngineError>; // panic-guarded; unknown id is an error
    pub fn commands(&self) -> Vec<CommandInfo>;   // id, label, shortcut, location, enabled: bool, reason
    pub fn can_undo(&self) -> bool; pub fn can_redo(&self) -> bool; pub fn undo_label(&self) -> Option<&str>;
    pub fn rev(&self) -> u64;                     // bumps on every document change
}
pub mod io { pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError>; pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError>; }
pub mod sample { pub fn sample_document() -> Document; }
```

The placeholder document format is JSON (`.craft.json`, `io::EXTENSION`); Cascade 1 replaces it
with `.vizcraft` (see [docs/formats.md](formats.md)). `file.new`, `file.open`, `file.save`,
`file.saveAs`, `file.close`, `file.export` and `file.import` exist and are what the chrome
exercises; `file.export` and `file.import` are slots that the diagram formats will fill. Engine crate root carries
`#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.
A test `hostile_params_never_panic` runs every command with junk params.

## UI (`vizcraft-ui-egui`) public API

Modules: `theme`, `icons`, `widgets`, `chrome` (title bar + status bar), `menus` (menu tree +
in-window menu bar), `ribbon`, `panes` (side panes, `PaneSpec`, `set_panes`), `canvas` (`Viewport2D`
screen/world projections, pan and zoom: middle-drag or Cmd+left-drag, Ctrl off macOS), `dialogs`, `keys`, `control`, `window_geometry`,
`backstage` (optional File page).

```rust
pub struct Services {        // host-injected platform services; all optional
    pub pick_open: Option<Box<dyn Fn(&str) -> Option<String>>>,   // purpose: "document" | "picture"
    pub pick_save: Option<Box<dyn Fn(&str) -> Option<String>>>,   // suggested file name
    pub open_async: Option<Box<dyn Fn(&str)>>, pub inbox: Option<Inbox>, pub download: Option<Box<dyn Fn(&str, &[u8])>>, // web
}
#[derive(Clone, Serialize, Deserialize)] #[serde(default, rename_all = "camelCase")]
pub struct UiState {         // persisted between runs as ui.json; agents read/set it via ui.inspect
    pub tab: String, pub ribbon_collapsed: bool, pub dark: bool, pub recent: Vec<String>,
    pub author: String, pub panes: panes::PaneLayout, pub window: Option<window_geometry::WindowGeometry>,
    pub in_window_menus: bool,
}
pub struct CraftApp {
    pub session: Session, pub ui: UiState, pub services: Services,
    pub dialog: Option<dialogs::Dialog>, pub status_msg: Option<(String, f64)>,
    pub integrated_titlebar: bool,       // macOS: window has no title bar, leave room for traffic lights
    pub multi_window: bool,              // host runs several windows in one process
    pub new_window_request: Option<String>, pub quit_all_request: bool,
    pub quit_requested: bool, pub quit_confirmed: bool,
    pub frame_ms: f64,
    // private: control_rx, screenshot queues, synthetic events, style flags
}
impl CraftApp {
    pub fn new(session: Session, services: Services) -> Self;
    pub fn with_control(self, rx: Receiver<ControlRequest>) -> Self; pub fn take_control(&mut self) -> Option<Receiver<ControlRequest>>;
    pub fn prefs(&self) -> UiState; pub fn apply_prefs(&mut self, ui: UiState);
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String>;   // ui.* here, else engine
    pub fn logic(&mut self, ctx: &egui::Context);     // per frame, before layout: control, shortcuts, close requests, drops
    pub fn ui(&mut self, ui: &mut egui::Ui);           // lay out the window
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput);
    pub fn new_window(&mut self, template: &str); pub fn has_unsaved(&self) -> bool;
    pub fn ask_before_closing(&mut self); pub fn asking_before_closing(&self) -> bool;
    pub fn status(&mut self, s: impl Into<String>); pub fn title_stem(&self) -> String;
}
pub struct ControlRequest { pub method: String, pub params: Value, pub reply: Sender<Value> }
```

Window layout order in `CraftApp::ui`: `chrome::title_bar` (top), `menus::menu_bar` (top, only
when `menus::in_window_menus(app)`: not macOS, or `ui.in_window_menus`), `ribbon::show` (tab strip
+ ribbon), `chrome::status_bar` (bottom), `panes::show` (left/right side panels), then the
central canvas. Colours only from `theme::Tokens` (light and dark), fonts Inter and JetBrains Mono
from `assets/fonts`.

### Menu tree (shared by the in-window bar and the native macOS bar)

```rust
pub enum Item { Cmd(&'static str /*label*/, &'static str /*id*/, Value), Sub(&'static str, Vec<Item>), Sep, Header(&'static str), Check(&'static str, &'static str, Value) }
pub struct Menu { pub title: &'static str, pub role: MenuRole /*Normal|App|Window|Help*/, pub items: Vec<Item> }
pub fn menu_tree(app: &CraftApp) -> Vec<Menu>;             // File, Edit, View, Window, Help (+ recent files)
pub fn entry(app: &CraftApp, id: &str) -> Entry;           // label, shortcut text, enabled, checked (for display)
pub fn menu_bar(app: &mut CraftApp, ui: &mut egui::Ui);     // in-window bar (egui menus)
pub fn in_window_menus(app: &CraftApp) -> bool;
```

The desktop app's `native_menu.rs` (muda, macOS only) builds from `menu_tree`, adds the app menu
(About, Settings, Services, Hide, Hide Others, Show All, Quit as our own `app.quit` so unsaved
changes are asked about) and the Window menu, and delivers item ids back to the host each frame.

### Multi-window (`apps/vizcraft/src/windows.rs`)

The first window is eframe's root viewport, further windows are
immediate viewports each with its own `CraftApp`. Cmd+N opens a window in the same process (one
Dock icon). Closing the root while others exist hands the root to the next window. Quit asks
each window with unsaved changes in turn; Cancel stops quitting. Cmd+W closes the front window
asking to save. The control channel stays with the app when the first window closes.

### Control channel (`--control <port>`, `VIZCRAFT_CONTROL_PORT`)

JSON lines on 127.0.0.1: `{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}`
-> `{"id":1,"ok":true,"result":...}`. Methods: `engine.execute`, `engine.commands`, `ui.inspect`,
`ui.set {path, value}` (UiState), `ui.click {x,y,button?,count?,shift?,cmd?}`, `ui.move`,
`ui.drag`, `ui.key {key, shift?, alt?, cmd?}`, `ui.text {text}`, `ui.screenshot {path?}`,
`app.quit`. See [docs/control-protocol.md](control-protocol.md).

### Preferences

`~/Library/Application Support/VizCraft/ui.json` (macOS), `%APPDATA%\VizCraft\ui.json`,
`$XDG_CONFIG_HOME/vizcraft/ui.json`. `VIZCRAFT_NO_PREFS=1` disables. Window geometry is saved
in it and restored (sanitized) at launch.

## Conventions

- Rust 2024, `rustfmt.toml` (max_width 150), clippy `-D warnings`, no `unwrap`/`expect`/panics in
  non-test code, tests may.
- Comments say why, in plain words; doc comments on every public item.
- Every asset file has a row in `ATTRIBUTION.md`; icons are drawn in code (`icons.rs`). No
  Microsoft/Adobe/Avid/Autodesk iconography, ever.
- Commit after every arc that builds; `cargo xtask ci` before every commit.
- No emoji anywhere (code, comments, docs, UI text). Use words.

## Planned (not yet built)

This section is plan, not contract. Nothing below exists in the code yet. When a piece from
[ROADMAP.md](../ROADMAP.md) lands, move its part of this section up into the contract above, in
the same commit as the code, and update [docs/parity.md](parity.md).

### Planned crates and layering

| Layer | Crate | Job |
|---|---|---|
| model | `crates/diagram` (`vizcraft-diagram`) | Pages, layers, shapes, connectors, groups and containers, text, styles and themes, shape data; geometry, hit testing, glue, connector routing, auto-layout. No egui, no command plumbing |
| model | `crates/stencils` (`vizcraft-stencils`) | Built-in stencil library: original shapes defined in code, categories, keyword search index |
| I/O | `crates/formats` (`vizcraft-formats`) | `.vizcraft` native JSON, .drawio read and write, Mermaid and CSV import, SVG, PNG and PDF export, .vsdx import |
| L4 | `crates/engine` | Gains `cmd/` modules for the new command prefixes; `Session.doc` becomes the diagram `Document` |
| L6 | `crates/ui-egui` | Diagram canvas (handles, connector drag, snap, rulers, guides), page tabs, new panes and ribbon tabs |
| tooling | `crates/mcp` (`vizcraft-mcp`) | Stdio JSON-RPC MCP server, launched by `vizcraft-cli mcp` |

`diagram` and `stencils` sit below `engine` and depend on neither egui nor the engine. `formats`
sits beside `engine`: it depends on `diagram` (and `stencils` for import mapping), and the engine
calls it from `file.*` commands. `mcp` is tooling: it either talks to a running app over the
control channel (Remote backend) or hosts a headless `Session` in process (Headless backend); it
never depends on `ui-egui`. `xtask/src/layers.rs` gains each crate when it lands.

### Diagram model sketch

| Type | Holds |
|---|---|
| `Document` | Metadata (the current `CoreProps`), ordered pages, themes and named styles, stencil references used |
| `Page` | Id, name, size and orientation, units, optional background page reference, layers, shapes, connectors |
| `Shape` | Id, stencil or master reference, geometry (x, y, w, h, rotation), text, style, shape data (key and value pairs), connection points, parent container or group, layer membership |
| `Connector` | Id, source and target glue (shape plus connection point, or a free point), route style (straight, orthogonal, curved), waypoints, arrowheads at each end, label text, style |
| `Theme`, `Style` | Theme colours, fonts and effects; styles for fill, line, shadow, gradient and text, referenced by name from shapes and connectors |
| `Layer` | Id, name, flags: visible, locked, printable, colour |

Glue is dynamic when it names only a shape (the connector picks the nearest connection point as
shapes move) and point glue when it names a connection point. Ids are strings and are what
`Selection.items` holds.

### Planned command prefixes

New prefixes join the existing `file.*`, `edit.*`, `view.*`, `insert.*`, `format.*`, `window.*`,
`help.*`, `ui.*` and `app.*`:

| Prefix | Covers | Module |
|---|---|---|
| `shape.*` | Add, delete, move, resize, rotate, text, style of shapes | `crates/engine/src/cmd/shape.rs` |
| `connector.*` | Add, glue, unglue, route style, waypoints, arrowheads, label | `crates/engine/src/cmd/connector.rs` |
| `page.*` | Add, delete, rename, reorder, page setup, background page | `crates/engine/src/cmd/page.rs` |
| `layer.*` | Add, delete, rename, flags, assign shapes | `crates/engine/src/cmd/layer.rs` |
| `arrange.*` | Align, distribute, order, group, ungroup, flip, rotate, size and position | `crates/engine/src/cmd/arrange.rs` |
| `design.*` | Themes, quick styles, page background, format painter | `crates/engine/src/cmd/design.rs` |
| `data.*` | Shape data, data graphics, data import | `crates/engine/src/cmd/data.rs` |
| `layout.*` | Auto-layout (layered, tree, radial, force) of a page or selection | `crates/engine/src/cmd/layout.rs` |

Every new command follows [Everything is a command](#everything-is-a-command): undoable when it
changes the document, covered by `hostile_params_never_panic`, never opening a dialog when run
by id.

### MCP server mapping

The MCP server adds no behaviour of its own. Generic tools (`command_list`, `command_run`,
`command_batch`) expose the registry, so every command above is reachable the day it lands;
`doc_inspect` and `screenshot` map to `ui.inspect` and `ui.screenshot`. The draw.io-style tools
(`open_xml`, `open_mermaid`, `open_csv`, `search_shapes`, `get_reference`, `list_pages`,
`get_page`, `set_page`) are thin wrappers that call `formats` for parsing and writing and run
commands for every change, so undo and the control channel see them. Tool list, resources and
launch: [docs/mcp.md](mcp.md). File formats and their public sources: [docs/formats.md](formats.md).
