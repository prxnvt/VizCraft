# VizCraft architecture (the contract)

VizCraft is a clean-room, open-source desktop diagramming app written in Rust and egui, aiming
for the feature range of Visio, with an MCP server shaped like draw.io's so agents can create and
edit diagrams. Today the window chrome, menus, ribbon, panes, multiple windows, preferences,
control channel and command registry are finished, and the diagram model (`vizcraft-diagram`,
`vizcraft-stencils`) is the engine's document; the rest of the diagram editor is being built
cascade by cascade (see [Planned (not yet built)](#planned-not-yet-built) and
[ROADMAP.md](../ROADMAP.md)).

This file is the contract agents build against in parallel. The sections up to Conventions
describe the code as it is. Change it only together with
the code that depends on it, and say so in the commit message.

## Layers (enforced by `cargo xtask layers`)

| Layer | Crate | Job |
|---|---|---|
| model | `crates/diagram` (`vizcraft-diagram`) | Geometry (points, rects, transforms, paths), shape outlines and connection points, the document model, hit testing, glue and connector routing, auto-layout (`autolayout`), obstacle-avoiding routes (`autoroute`) and line jumps (`jumps`). Depends on serde, `layout` and `routing` |
| model | `crates/stencils` (`vizcraft-stencils`) | Built-in stencil catalog: Basic and Flowchart sets, masters with default size and style, keyword search. Depends on `diagram` only |
| model | `crates/formats` (`vizcraft-formats`) | File formats, bytes in and bytes out: `.vizcraft` native JSON (`native::save`, `native::load`), SVG export (`svg::export_page`), `.drawio` (`drawio::{load, save, import, page_xml, master_style}`), Mermaid (`mermaid::import`) and CSV (`csv::import`) import into `Imported { pages, warnings }`. Depends on `diagram`, `stencils`, `drawio`, `mermaid` and `layout` |
| model | `crates/mermaid` (`vizcraft-mermaid`) | Mermaid parser: flowchart, sequence, class, state and ER text to a typed IR with a common node, edge and group view (`Diagram::to_graph`); warnings for unsupported statements. No workspace dependencies |
| model | `crates/drawio` (`vizcraft-drawio`) | draw.io codec: `.drawio` files (inline or compressed pages), the mxGraphModel cell IR, style strings, HTML labels, page list/get/set, and draw.io-style CSV import. No workspace dependencies |
| model | `crates/layout` (`vizcraft-layout`) | Automatic layout over an index-based graph: layered (Sugiyama), tidy tree, radial and force-directed, all four directions, deterministic. No workspace dependencies |
| model | `crates/routing` (`vizcraft-routing`) | Connector routing over obstacle rectangles: orthogonal routes around shapes, nudging of shared segments, line-jump positions. No workspace dependencies |
| L4 | `crates/engine` (`vizcraft-engine`) | `Session`, undo, command registry, I/O; `engine::doc` re-exports the diagram `Document` |
| L6 | `crates/ui-egui` (`vizcraft-ui-egui`) | egui front end: theme, chrome, menus, ribbon, panes, dialogs, keys, control |
| platform | `crates/macos-open` | Finder Open Documents (odoc) Apple Event, safe API |
| mcp | `crates/mcp` (`vizcraft-mcp`) | Stdio JSON-RPC MCP server, launched by `vizcraft-cli mcp`; same rank as `ui-egui`, so neither depends on the other, and no UI toolkit (see [docs/mcp.md](mcp.md)) |
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
CLI, control channel and MCP server all dispatch by id. Programmatic calls never open dialogs:
a command that needs one pushes `{"open": "<dialog>"}` to `session.ui_requests` and the UI
handles it.

Command id prefixes: `file.*`, `edit.*`, `view.*`, `shape.*` (`crates/engine/src/cmd/shape.rs`),
`connector.*` (`cmd/connector.rs`), `page.*` (`cmd/page.rs`), `layout.*` (`cmd/layout.rs`), `insert.*`, `format.*`, `window.*`, `help.*`, `ui.*` (UI layer), `app.*` (app-level: `app.quit`,
`app.preferences`, `app.about`). `insert.*` and `format.*` are reserved; no engine
command uses them yet.

Diagram commands (Cascade 1). Without `ids` they act on the selection; an explicit `ids` list
must name existing items or nothing changes. Each document change is one undo step.

| Command | Params |
|---|---|
| `shape.add` | `{stencil, x, y, w?, h?, text?, id?}` (x, y = top-left of the frame; `id` lets a caller name it) -> `{id}` |
| `shape.delete` | `{ids?}`; connectors glued to a deleted shape keep a free end there, and go when both ends are deleted |
| `shape.move` | `{ids?, dx, dy}` or `{id, x, y}` |
| `shape.resize`, `shape.rotate` | `{id, x?, y?, w, h}`; `{ids?, angle}` (absolute degrees) |
| `shape.setText`, `shape.setStyle` | `{id, text}`; `{ids?, fill?, line?, lineWidth?, fontSize?, fontColor?, bold?, italic?}` |
| `connector.add` | `{from, to, route?, text?, startArrow?, endArrow?, id?}`, an end is `{shape, point?}` or `{x, y}` -> `{id}` |
| `connector.glue` | `{id, end: "start" \| "end", shape?, point?, x?, y?}` |
| `connector.setRoute`, `connector.setArrows`, `connector.setText` | `{ids?, route}`; `{ids?, start?, end?}`; `{id, text}` |
| `edit.select`, `edit.selectNone`, `edit.selectAll`, `edit.delete` | `{ids, add?}`; `{}` |
| `edit.cut`, `edit.copy`, `edit.paste` | copy returns `{text}` holding `{"vizcraft": "clip", "shapes", "connectors"}`; paste takes `{text?}` |
| `connector.reroute`, `connector.setJumps` | `{ids?, avoid?}` (true: route around shapes and keep clear; false: clear bends); `{ids?, on}` |
| `shape.search` | `{query, limit?}` -> `{masters: [{id, label, category, keywords, w, h, style}]}` (`style` is the draw.io style string) |

Interop and layout commands (Cascade 3). Each is one undo step; a page is named by id, name or
zero-based index (`Document::find_page`).

| Command | Params |
|---|---|
| `file.import` | `{path? \| text?, format?: "drawio" \| "mermaid" \| "csv", page?, replace?, layout?, direction?, routing?: "orthogonal"}` -> `{page, ids, warnings}`; imported ids are kept when plain and free (`Document::adopt`); with a `layout` on a page that has shapes, the existing shapes stay and new ones are placed incrementally |
| `file.export` | `{path?, format?: "svg" \| "drawio"}` |
| `page.list`, `page.get`, `page.set` | `{path?}`; `{page?, path?, format?: "drawio" \| "vizcraft"}`; `{page, xml, path?, layout?, direction?, routing?}`; with `path` they read or edit that file and leave the session alone |
| `layout.full` | `{algorithm?: "layered" \| "tree" \| "radial" \| "force", direction?, nodeSpacing?, layerSpacing?, ids?}`: the named shapes, else a selection of two or more, else the page; the page grows to fit |
| `layout.incremental` | `{ids, algorithm?, direction?, nodeSpacing?, layerSpacing?}`: places only `ids`, every other shape stays |
| `layout.route` | `{ids?}`: routes connectors around shapes, never moves a shape |

## Engine (`vizcraft-engine`) public API

```rust
pub struct Session {
    pub doc: Document,                 // vizcraft_diagram::Document, re-exported by crates/engine/src/doc.rs
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub author: String,
    pub sel: Selection,                // generalized selection (primary, items: Vec<String>, range: Option<(u64, u64)>)
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
pub mod io { pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError>; pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError>;
             pub fn export_bytes(name: &str, format: Option<&str>, doc: &Document) -> Result<Vec<u8>, EngineError>; }
pub mod sample { pub fn sample_document() -> Document; }
```

Documents are `.vizcraft` files (`io::EXTENSION`), versioned JSON read and written by
`vizcraft-formats` (a model-class crate beside `diagram`); `io` dispatches by file name and
still reads `.craft.json` and `.json`; `.drawio` and `.drawio.xml` names open and save as draw.io.
`file.export {path, format?}` writes the first page as SVG, or the document as draw.io, through
`io::export_bytes`; `file.import` brings in draw.io, Mermaid and CSV. After every command that
changes the document, `Session::run` calls `vizcraft_diagram::autoroute::refresh`, inside the same
undo step, so connectors that avoid shapes are rerouted when anything moves. See
[docs/formats.md](formats.md). Engine crate root carries
`#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.
A test `hostile_params_never_panic` runs every command with junk params.

## UI (`vizcraft-ui-egui`) public API

Modules: `theme`, `icons`, `widgets`, `chrome` (title bar + status bar), `menus` (menu tree +
in-window menu bar), `ribbon` (Home, Insert, Design, View, Help, and the contextual Connector tab),
`panes` (side panes, `PaneSpec`, `set_panes`; Shapes, Navigation, Properties, History),
`canvas` (the diagram canvas: `Viewport2D` screen/world projections; drawing of the page, shapes,
text and connectors; selection, marquee, move, resize and rotate handles, connector drag and
reglue, in-place text editing, arrow-key nudge; pan with middle-drag, Space+left-drag or
Cmd+left-drag, Ctrl off macOS; `insert_point` and `insert_master` place new shapes), `dialogs`,
`keys`, `control`, `window_geometry`, `backstage` (optional File page).

A canvas drag previews in UI state and commits one command on release, so every drag is one undo
step. The canvas tool (`CraftApp.tool`: pointer, connector, text) is set by `ui.tool`.

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
    pub tool: Tool,                      // canvas tool: Pointer | Connector | Text, set by ui.tool
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

## Diagram model (`vizcraft-diagram`, `vizcraft-stencils`)

Both crates sit below the engine (`xtask layers` class `model`): no egui, no engine, wasm safe.
Units are `f64` world units, pixels at 100% zoom, y down; angles are degrees clockwise about the
shape's frame centre.

| Type | Holds |
|---|---|
| `Document` | `title`, `core: CoreProps` (author, timestamps, revision), `pages: Vec<Page>` (always at least one), `id_seq`; `next_id(prefix)` hands out `s1`, `c2` and so on, never reused |
| `Page` | Id, name, width and height (1100 x 850 by default), `shapes` and `connectors` in z order, `ext` |
| `Shape` | Id, `stencil` master id (`basic.rectangle`), `kind: ShapeKind` (16 outlines), frame `x, y, w, h`, `angle_deg`, text, `Style`, optional custom connection points (normalized; kinds default to top, right, bottom, left), `ext` (data from other formats kept for round trips) |
| `Connector` | Id, `start` and `end` as `End::Glued { shape, point: Option<usize> }` (no point = dynamic glue) or `End::Free { x, y }`, `route` (`straight`, `orthogonal`, `curved`), arrowheads per end (`none`, `triangle`, `open`, `diamond`, `circle`), label text, `Style`, `waypoints` (stored bends), `avoid` (route around shapes), `jumps` (line jumps), `ext` |
| `Style` | Fill, line colour and width, font size and colour, bold, italic; colours serialize as `#rrggbb` |

Routing and glue are pure functions of shape geometry and the connector's stored `waypoints`
(`resolve_ends`, `route_path`, `connector_geometry`), so moving a shape moves its connectors with
no extra bookkeeping. Bends of `avoid` connectors come from `autoroute::{reroute, refresh}`
(`vizcraft-routing`), stored because routing is too costly to redo every frame; the canvas and
SVG export draw through `jumps::page_geometry` for line jumps. `autolayout::{layout_items,
layout_incremental}` place shapes with `vizcraft-layout`. `Document::{find_page, add_page, adopt}`
serve the importers and page commands. Hit
testing (`shape_at`, `connector_at`, `shapes_in_rect`, `connection_point_at`) uses the real
outline and rotation. Loading is tolerant: missing or wrong-typed fields take defaults, and
`sanitize` clamps sizes, coordinates and text length. `vizcraft-stencils` provides `sets()`,
`master(id)`, `search(query)` and `new_shape(master_id, id, x, y, w?, h?)`.

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
| model | `crates/diagram` | Grows layers, groups and containers, themes, shape data, waypoints and auto-layout (Cascades 2 to 4) |
| model | `crates/stencils` | Grows the domain stencil sets and a full keyword index (Cascades 3 and 4) |
| model | `crates/formats` | Grows .drawio read and write, Mermaid and CSV import, PNG and PDF export, .vsdx import (Cascades 2 to 4) |
| L4 | `crates/engine` | Gains `cmd/` modules for the new command prefixes |
| L6 | `crates/ui-egui` | Diagram canvas (handles, connector drag, snap, rulers, guides), page tabs, new panes and ribbon tabs |

`diagram`, `stencils` and `formats` sit below `engine` (layer class `model`) and depend on
neither egui nor the engine; `formats` may add `stencils` for import mapping, and the engine calls
it from `file.*` commands. `xtask/src/layers.rs` gains each crate when it lands.

### Diagram model sketch (later cascades)

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

New prefixes join the existing `file.*`, `edit.*`, `view.*`, `shape.*`, `connector.*`, `insert.*`, `format.*`, `window.*`,
`help.*`, `ui.*` and `app.*`:

| Prefix | Covers | Module |
|---|---|---|
| `page.*` | Add, delete, rename, reorder, page setup, background page (`page.list`, `page.get`, `page.set` are built) | `crates/engine/src/cmd/page.rs` |
| `layer.*` | Add, delete, rename, flags, assign shapes | `crates/engine/src/cmd/layer.rs` |
| `arrange.*` | Align, distribute, order, group, ungroup, flip, rotate, size and position | `crates/engine/src/cmd/arrange.rs` |
| `design.*` | Themes, quick styles, page background, format painter | `crates/engine/src/cmd/design.rs` |
| `data.*` | Shape data, data graphics, data import | `crates/engine/src/cmd/data.rs` |

Every new command follows [Everything is a command](#everything-is-a-command): undoable when it
changes the document, covered by `hostile_params_never_panic`, never opening a dialog when run
by id.

### Model-backed workbench (Cascades 4A to 8)

From Cascade 4A, shapes become views of typed elements in a semantic model, and notations ship as
data packs. The reasoning and sources are in
[docs/research/professional-features.md](research/professional-features.md); the pieces and their
order are in [ROADMAP.md](../ROADMAP.md).

#### New crates

All four sit in the model layer: below `engine`, no egui, no engine dependency, wasm safe, with the
standard crate-root `deny` list. When each lands, `MODEL_SUFFIXES` in `xtask/src/layers.rs` gains
its suffix (`-model`, `-notation`, `-patterns`, `-codegen`) and the contract table above gains its
row.

| Layer | Crate | Job | Depends on |
|---|---|---|---|
| model | `crates/model` (`vizcraft-model`) | Semantic model: elements, relationships, containment, views, identity registry, structured issues, model queries and derived tables | serde only; `diagram` depends on it, because the diagram `Document` holds the model (`Document.model`) so undo, dirty tracking and the panic restore cover both |
| model | `crates/notation` (`vizcraft-notation`) | Notation pack schema and loader, built-in Basic and Flowchart packs, connection table, conditional styles, viewpoints, identifier formats, rules engine, model sync pass | `model`, `diagram`; `stencils` builds its masters from the pack glyphs |
| model | `crates/patterns` (`vizcraft-patterns`) | Pattern generators with typed parameters, provenance and reparameterize, emitting model edits | `model`, `notation`, `layout` |
| model | `crates/codegen` (`vizcraft-codegen`) | Generator framework under the regeneration contract: specs, manifests, deterministic writers, merge, targets | `model`, `notation` |

`formats` gains the model section of `.vizcraft` and pack-specific importers and exporters
(SCXML, SVD, DDL, BPMN XML and so on); `engine` gains the command modules for the new prefixes and
runs live rules after each command; `mcp` gains notation schema discovery and returns issues with
each batch; `ui-egui` gains the model tree, scoped palette, Inspector, bottom dock and canvas
insertion.

#### Semantic model sketch

| Type | Holds |
|---|---|
| `Element` | Stable id (caller-chosen or registry-issued), pack-qualified type (`pid.CentrifugalPump`, `c4.Container`), typed properties checked against the type's schema, tags, owner (its parent in the containment tree) |
| `Relationship` | Id, pack-qualified type, source and target element ids with optional ports, properties |
| Containment | Tree of elements under the project root (packages, plant areas, composite states); an element has one owner |
| `View` | Id, viewpoint (the named filter of allowed types), layout; geometry keyed by element id, never stored in the model |
| `Shape.element`, `Connector.relationship` | Optional references from today's diagram items; an item without one stays free drawing, so Cascade 1 to 3 documents keep working |

Deleting a shape removes it from its view; deleting from the model removes the element, its
relationships and every view item that references it. One element may appear in many views.

#### Notation pack schema outline

A pack is a versioned JSON or TOML file, loaded by `vizcraft-notation`, and is the single source
for the palette, hover suggestions, the magic connector, the validator, the Inspector and MCP
schema discovery. Basic and Flowchart are the first two packs.

| Section | Declares |
|---|---|
| Header | Pack id (the type prefix), version, audience, schema version |
| Element types | Name, property schema (type, unit, enum, required, default), ports, default size |
| Glyphs | Original outlines drawn from public specification descriptions |
| Connection table | Source type, relationship type, target type, multiplicity |
| Conditional styles | Style changes keyed on property values |
| Palette groups and viewpoints | Grouping for the palette; named filters of allowed types per view |
| Identifier formats | Composed identifiers (for example letters plus loop number plus suffix) used by the identity registry |
| Rules | Rule id, severity (Error, Warning, Info), live or batch, the check |
| Templates, patterns, exporters | What the pack supports, by id |

#### Rules engine

Cheap rules run live after every document-changing command (illegal relationship, missing required
property, duplicate tag); expensive rules run in batch on a selection, page or project
(reachability, cross-sheet continuity). An issue is a structured record of rule id, severity,
element ids and message. Issues feed the Problems tab, and the MCP server returns the issues a
command batch raised in that batch's result, so an agent can repair its own mistakes.

#### Identity registry

A project-wide registry in `vizcraft-model` hands out and checks tags and reference designations:
auto-numbering, uniqueness, and identifiers composed from a pack's identifier format. Off-page
connectors are paired objects matched by identifier; an end with no partner raises an issue.

#### Derived table views

Tables and matrices (instrument index, I/O list, line list, cause-and-effect matrix, relationship
matrix, RACI) are views over the model computed by queries in `vizcraft-model`, editable in place:
an edit in a table is a model command. They export through `formats` as CSV, XLSX or Markdown.
Generated diagrams (implied-relationship roll-ups, drill-down child views, loop diagrams) are views
too.

#### Code-scaffold generation contract

`vizcraft-codegen` writes code only under these rules:

| Clause | Rule |
|---|---|
| Generation is a command | `generate.preview`, `generate.run`, `generate.check`; CLI `vizcraft-cli generate --check` for CI; reachable over MCP |
| Configuration in the repository | A generator spec file names target, options, output directory and model scope |
| Explicit ownership | A manifest lists each generated file and its last checksum; only owned files inside the declared directory are written |
| Separation by default | Generation gap per language (Rust trait implemented by the user; C `extern` prototypes); partial classes where the language has them |
| Regions only when unavoidable | Single-file targets use protected regions keyed by element id, never by position or name |
| Traceability | `vizcraft:id=<element id>` on every generated construct; renames go through a rename map |
| Determinism | Stable ordering and formatting; an unchanged model regenerates byte-identically |
| No silent overwrite | Preview diff; a changed owned file is three-way merged (last generated, on disk, new) or generation stops with an issue |
| Honest targets | Each target declares a construct-support matrix; unsupported constructs raise issues or are flattened on opt-in |
| Standard intermediates | Emit SVD, DBC, OpenAPI, AsyncAPI, protobuf, PLCopen XML rather than runtimes |
| Verified fidelity | Where an importer exists, tests re-import generated output and compare it with the model |

Re-import follows the same rule in reverse: imported elements keep model ids, re-import updates
the model, keeps view geometry for existing ids, places new elements with `layout.incremental` and
flags removed ones.

#### Pattern generators

A pattern in `vizcraft-patterns` is a generator with typed parameters (counts, labels, mode enums,
tag prefixes, a data table). `pattern.insert(name, args)` emits model elements and relationships,
applies one of the layout primitives in `vizcraft-layout`, and commits as one undoable compound
command. The inserted group records the pattern id and parameter values (provenance), so
`pattern.reparameterize` can regenerate it with new values while keeping manual label edits. The
parameter schema of every pattern is discoverable through `pattern.list` and MCP.

#### Planned workbench command prefixes

These join the prefixes above:

| Prefix | Covers | Cascade |
|---|---|---|
| `model.*` | Project-level model operations: containment moves, show in all views, delete from model | 4A |
| `element.*` | Create, set property, retag, delete from view | 4A |
| `relationship.*` | Create, retype, reconnect, set property | 4A |
| `validate.*` | Run batch rules, list and filter issues | 4A |
| `notation.*` | List packs, describe a type, legal relationships from an element | 4A |
| `pattern.*` | `pattern.list`, `pattern.insert`, `pattern.reparameterize` | 5A |
| `generate.*` | `generate.preview`, `generate.run`, `generate.check` | 5B |
| `layout.full`, `layout.incremental`, `layout.route` | Rearrange everything; pin existing nodes and place new ones; reroute connectors without moving nodes | 3 (built) |

### MCP server mapping

Built in Cascade 1 (contract): the MCP server adds no behaviour of its own. Generic tools
(`command_list`, `command_run`, `command_batch`) expose the registry, so every command above is
reachable the day it lands; `doc_inspect` and `screenshot` map to `ui.inspect` and
`ui.screenshot`, through the `Remote` backend over the control channel. Built in Cascade 3: the
draw.io-style tools (`open_xml`, `open_mermaid`, `open_csv`, `search_shapes`, `get_reference`,
`list_pages`, `get_page`, `set_page`) each send one engine command (`file.import`, `page.*`,
`shape.search`; `get_reference` is static text), so undo and the control channel see them.
Planned: a `Headless` backend hosting a `Session` in process, and MCP resources. Tool list, resources and
launch: [docs/mcp.md](mcp.md). File formats and their public sources: [docs/formats.md](formats.md).
