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
| model | `crates/model` (`vizcraft-model`) | Semantic model: elements, relationships, containment, views (`View` on a page), the identity registry (`identity`: identifier formats, auto-numbering, uniqueness, pairs) and structured issues (`Issue`, `Severity`). Depends on serde only |
| model | `crates/notation` (`vizcraft-notation`) | Notation packs: the pack schema (`schema`), JSON and TOML loader with consistency checks, the built-in Basic and Flowchart packs (`catalog()`), connection-table queries (`legal`), the rules engine (`rules::{live, batch}`) and the model sync pass (`bind::sync`). Depends on `model` and `diagram` |
| model | `crates/diagram` (`vizcraft-diagram`) | Geometry (points, rects, transforms, paths), shape outlines and connection points, the document model, hit testing, layer rules (`layer`), glue and connector routing, auto-layout (`autolayout`), obstacle-avoiding routes (`autoroute`) line jumps (`jumps`), align, distribute, z order and groups (`arrange`), diagram items as views of model items (`views`), and snapping to the grid, guides, shapes and connection points (`snap`). The `Document` holds the semantic model. Depends on serde, `model`, `layout` and `routing` |
| model | `crates/stencils` (`vizcraft-stencils`) | Built-in stencil catalog: Basic and Flowchart sets, masters with default size and style, ranked keyword search over an index built once (`search`); its sets and masters are built once from the glyphs of the notation packs. Depends on `diagram` and `notation` |
| model | `crates/formats` (`vizcraft-formats`) | File formats, bytes in and bytes out: `.vizcraft` native JSON (`native::save`, `native::load`; version 3 carries the model section, version 4 the richer style, shape, page and document fields), SVG export (`svg::export_page`, `svg::export_page_with` and `SvgOptions { print, date }`: header and footer fields expanded, non-printing shapes left out when printing), PNG export (`png::render_page`: the SVG export rasterised on the CPU by `resvg` with the bundled Inter faces, no system fonts, builds for wasm), PDF export and print layout (`pdf::export_pages`, `pdf::plan`, `pdf::print_pages`: the same SVG drawn into vector PDF pages by `krilla` and `krilla-svg` with subsets of the bundled Inter faces embedded; builds for wasm), `.drawio` (`drawio::{load, save, import, page_xml, master_style}`), Mermaid (`mermaid::import`) and CSV (`csv::import`) import into `Imported { pages, warnings }`. Depends on `diagram`, `stencils`, `drawio`, `mermaid` and `layout` |
| model | `crates/mermaid` (`vizcraft-mermaid`) | Mermaid parser: flowchart, sequence, class, state and ER text to a typed IR with a common node, edge and group view (`Diagram::to_graph`); warnings for unsupported statements. No workspace dependencies |
| model | `crates/drawio` (`vizcraft-drawio`) | draw.io codec: `.drawio` files (inline or compressed pages), the mxGraphModel cell IR, style strings, HTML labels, page list/get/set, and draw.io-style CSV import. No workspace dependencies |
| model | `crates/layout` (`vizcraft-layout`) | Automatic layout over an index-based graph: layered (Sugiyama), tidy tree, radial and force-directed, all four directions, deterministic. No workspace dependencies |
| model | `crates/routing` (`vizcraft-routing`) | Connector routing over obstacle rectangles: orthogonal routes around shapes, nudging of shared segments, line-jump positions. No workspace dependencies |
| L4 | `crates/engine` (`vizcraft-engine`) | `Session`, undo, command registry, I/O; `engine::doc` re-exports the diagram `Document` |
| L6 | `crates/ui-egui` (`vizcraft-ui-egui`) | egui front end: theme, chrome, menus, ribbon, panes, dialogs, keys, control |
| platform | `crates/macos-open` | Finder Open Documents (odoc) Apple Event, safe API |
| mcp | `crates/mcp` (`vizcraft-mcp`) | Stdio JSON-RPC MCP server, launched by `vizcraft-cli mcp`; same rank as `ui-egui`, so neither depends on the other, and no UI toolkit. Depends on `engine` for the `Headless` backend, which hosts a `Session` in process (see [docs/mcp.md](mcp.md)) |
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

Command id prefixes: `file.*`, `edit.*`, `view.*`, `shape.*` (`crates/engine/src/cmd/shape.rs`), `arrange.*` (`cmd/arrange.rs`),
`connector.*` (`cmd/connector.rs`), `page.*` (`cmd/page.rs`), `layer.*` (`cmd/layer.rs`), `design.*` (`cmd/design.rs`), `layout.*` (`cmd/layout.rs`), `model.*`, `element.*`,
`relationship.*`, `validate.*`, `notation.*` (`cmd/{model,element,relationship,validate,notation}.rs`), `script.*` (`cmd/script.rs`), `guide.*` (`cmd/view.rs`), `insert.*`, `format.*`, `window.*`, `help.*`, `ui.*` (UI layer), `app.*` (app-level: `app.quit`,
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
| `shape.setText`, `shape.setStyle` | `{id, text}`; `{ids?, fill?, line?, lineWidth?, fontSize?, fontColor?, bold?, italic?, dash?, lineCap?, cornerRadius?, gradient?, fillTransparency?, lineTransparency?, shadow?, softEdges?, glow?}`; the effect keys are parsed in `cmd/effects.rs`, numbers clamped to the model's ranges, `gradient` `false`, `true`, `"linear"`, `"radial"` (built from each shape's fill) or `{kind?, angle?, stops}`, `shadow` and `glow` `false`, `true` or an object (`glow` also a radius); connectors take dash, line cap, rounding and line transparency only |
| `connector.add` | `{from, to, route?, text?, startArrow?, endArrow?, id?}`, an end is `{shape, point?}` or `{x, y}` -> `{id}` |
| `connector.glue` | `{id, end: "start" \| "end", shape?, point?, x?, y?}` |
| `connector.setRoute`, `connector.setArrows`, `connector.setText` | `{ids?, route}`; `{ids?, start?, end?}`; `{id, text}` |
| `edit.select`, `edit.selectNone`, `edit.selectAll`, `edit.delete` | `{ids, add?}`; `{}` |
| `edit.cut`, `edit.copy`, `edit.paste` | copy returns `{text}` holding a selection fragment `{"vizcraft": "clip", "shapes", "connectors"}`; paste takes `{text?}` and inserts it with fresh ids and the internal glue kept |
| `connector.reroute`, `connector.setJumps` | `{ids?, avoid?}` (true: route around shapes and keep clear; false: clear bends); `{ids?, on}` |
| `shape.search` | `{query, limit?}` -> `{masters: [{id, label, category, keywords, w, h, style}]}` (`style` is the draw.io style string) |

Arrange commands (Block 2, `cmd/arrange.rs` over `vizcraft_diagram::arrange`). Without `ids`
they act on the selected shapes; a shape named together with a group it is in moves once, with
the group. Each is one undo step.

| Command | Params -> result |
|---|---|
| `arrange.align` | `{ids?, edge: "left" \| "center" \| "right" \| "top" \| "middle" \| "bottom", to?: "selection" \| "page"}` -> `{moved}`; to the first shape's bounds (two or more shapes) or the page |
| `arrange.distribute` | `{ids?, axis: "horizontal" \| "vertical"}` -> `{moved}`; three or more shapes, equal gaps, the outer two stay |
| `arrange.autoSpace` | `{ids?, gap?}` (default 40) -> `{moved}`; along the direction the shapes spread most, each shape connected to one before it starts `gap` past it; nothing selected spaces the page |
| `arrange.order` | `{ids?, how: "front" \| "back" \| "forward" \| "backward"}` -> `{order}`; a group moves with its members; a member steps only within its group |
| `arrange.flip`, `arrange.rotate` | `{ids?, axis}` (mirrors centres and negates angles across the shapes' centre line); `{ids?, deg}` (relative; several shapes turn as one piece) |
| `arrange.setBounds` | `{id, x?, y?, w?, h?, angle?}` -> the frame; a group scales and turns its members |
| `arrange.group`, `arrange.ungroup` | `{ids?, id?}` -> `{id, members}` (the group is selected); `{ids?}` -> `{ungrouped, members}` (members selected) |

Groups: a group is a `Shape` with `stencil` `vizcraft.group` (`arrange::GROUP_STENCIL`),
transparent fill and line and no text, so it draws nothing but its selection frame. Its members
name it as `parent`; it sits in z order just below its first member, and its frame is the bounds
of its members measured in its own rotation (`arrange::refit_groups`, run after every arrange,
move, resize, rotate and delete). `shape.move`, `shape.resize` and `shape.rotate` on a group
transform its members (`arrange::{translate, set_frame, set_angle}`); deleting a group deletes its
members, a group left empty is deleted, and copying a group copies its members. Groups nest.
Auto routing does not treat groups as obstacles. On the canvas, a click on a member selects its
outermost group, the next click one level down, and once inside a group a click picks a sibling
directly; a marquee selects whole groups.

Design commands (Block 2, `cmd/design.rs`). Each document change is one undo step; a page is
named by id, name or zero-based index and defaults to the active page.

| Command | Params |
|---|---|
| `design.themes` | `{}` -> `{current: {theme, variant}, themes: [{id, name, font, variants}], quickStyles}` (read only) |
| `design.setTheme`, `design.setVariant` | `{theme: id \| "none", variant?}`; `{variant: 0..3}` -> `{theme, variant, restyled}` |
| `design.applyQuickStyle` | `{style: 0..11, ids?}`: row `style / 4` is subtle, moderate or intense, column `style % 4` the accent |
| `design.setBackground`, `design.setBorderTitle` | `{kind: "none" \| "solid" \| "gradient" \| "pattern", color?, page?}`; `{kind: "none" \| "frame" \| "banner" \| "margin", page?}` |
| `design.setConnectorStyle` | `{style: "rightAngle" \| "straight" \| "curved", page?}`: every connector on the page takes the route, loses its stored bends, and only right-angle connectors keep `avoid` (re-routed) |

Themes (`vizcraft_diagram::theme`): six built-in themes of our own, four variants each (fill,
line, connector and text colours, a page background colour, four accents), a font and effect
defaults (line width, corner radius, shadow, gradient). `Document::theme: DocTheme { id, variant }`
names the active one (empty id: no theme). A theme never records what it set: changing theme
moves each themed field (fill, line, text colour, line width, font, corner radius, shadow,
gradient) that still equals the old theme's value for that item to the new theme's value
(`theme::restyle`), so hand styling stays, as with conditional styles. "No theme" is the item's
base style (a shape's master style), so a document made before themes restyles fully and choosing
no theme again restores it. New shapes and connectors take the theme in
`design::after_change`, which `Session::run` calls after `bind::sync` inside the command's undo
step. Quick styles (`theme::quick_style`) are twelve presets from the active theme's accents; they
are hand styling once applied.

Backgrounds and borders (`vizcraft_diagram::decor`) live on a generated background page per
decorated page: `is_background`, `ext["vizcraft.design.for"]` naming the page it decorates, and
the chosen kinds and colour in `ext["vizcraft.design.background" | ".color" | ".border"]`. The
page's previous background page, if any, moves under the generated one; removing both
decorations removes the page and restores it. `decor::refresh` rebuilds the generated shapes from
those settings, the page size, the theme and the titles (the title property, else the document
title; the page name) and runs from `design::after_change` after every change, so title blocks
follow renames and frames follow page size. Export and the canvas need nothing special: both draw
the background chain.

Interop, layout and page commands. Each is one undo step except `page.setActive`; a page is named
by id, name or zero-based index (`Document::find_page`), and an optional `page` means the active
page (`Document::page`), which every page-scoped command acts on. After every change the session
grows each auto-size page whose drawing reaches past its right or bottom edge
(`cmd::page::auto_size`), in the same undo step.

| Command | Params |
|---|---|
| `file.import` | `{path? \| text?, format?: "drawio" \| "mermaid" \| "csv", page?, replace?, layout?, direction?, routing?: "orthogonal"}` -> `{page, ids, warnings}`; imported ids are kept when plain and free (`Document::adopt`); with a `layout` on a page that has shapes, the existing shapes stay and new ones are placed incrementally |
| `file.export` | `{path?, format?: "svg" \| "png" \| "pdf" \| "drawio", page?, pages?: "active" \| "all" \| [refs], scale?, print?}` -> `{exported, path, format, page, pages, paths}`; several PNG pages write `name-1.png`, `name-2.png` and so on |
| `file.print` | `{pages?, fit?, tiles?: [cols, rows], paper?, path?, send?}` -> `{path, sheets, pages, sent}`; writes a print-ready PDF and pushes `{"print": path}` to `ui_requests`, which the desktop app hands to the operating system (`apps/vizcraft/src/print.rs`); Print Preview is `ui.print` |
| `page.setHeaderFooter` | `{page? \| all?, header?: {left, center, right}, footer?, margin?}` -> `{pages, header, footer}`; fields `{page}`, `{pages}`, `{pageName}`, `{title}`, `{date}` (`diagram::print_fields`) |
| `page.list`, `page.get`, `page.set` | `{path?}`; `{page?, path?, format?: "drawio" \| "vizcraft"}`; `{page, xml, path?, layout?, direction?, routing?}`; with `path` they read or edit that file and leave the session alone |
| `page.add`, `page.delete`, `page.duplicate` | `{name?, background?: bool, index?}` -> `{page, index, name}` (takes the active page's setup and becomes active); `{page?}` (the last foreground page is refused; references to a deleted background page are cleared); `{page?}` -> `{page, index, name, ids}` (fresh ids, glue and parents remapped) |
| `page.rename`, `page.move`, `page.setActive` | `{page?, name}` (unique, at most 256 characters); `{page, index}`; `{page}` (not an undo step and not an edit) |
| `page.setup` | `{page?, width?, height?, orientation?, units?, drawingScale?: {page, drawing, units?}, autoSize?}`; orientation swaps width and height to match; with no params the UI opens the Page Setup dialog |
| `page.setBackground`, `page.fitToDrawing` | `{page?, background: page or null}` (a background page only, no loops); `{page?, margin?}` (moves the drawing to the margin and sizes the page around it) |
| `layout.full` | `{algorithm?: "layered" \| "tree" \| "radial" \| "force", direction?, nodeSpacing?, layerSpacing?, ids?}`: the named shapes, else a selection of two or more, else the page; the page grows to fit |
| `layout.incremental` | `{ids, algorithm?, direction?, nodeSpacing?, layerSpacing?}`: places only `ids`, every other shape stays |
| `layout.route` | `{ids?}`: routes connectors around shapes, never moves a shape |

Model commands (Cascade 4A). Element ids in params may also be the id of a shape that draws the
element, and relationship ids the id of a connector that draws it. Each document change is one
undo step; the read-only ones never change the document.

| Command | Params -> result |
|---|---|
| `model.get` | `{ids?}` -> `{schemaVersion, packs, elements, relationships}`; with ids only those (each must exist) |
| `model.tree` | `{root?}` -> node `{id, type, name, tag, children, truncated?}` for root, else `{children: [root elements]}`; 32 levels at most |
| `model.move` | `{id, owner: element id or null}` -> `{id, owner}`; the owner's type must be a container; cycles refused |
| `model.views` | `{id}` (element or relationship) -> `{views: [{page, pageName, items}]}` (show in all views) |
| `model.delete` | `{ids}` -> `{elements, relationships, items}`: delete from the model; owned elements, touching relationships and every view on every page go too |
| `model.setViewpoint` | `{page?, viewpoint: id or null}` -> `{page, viewpoint}`; the viewpoint must be in the catalog |
| `element.create` | `{type, name?, tag?, owner?, properties?, id?, x?, y?, w?, h?, page?}` -> `{id, tag, shape?}`; no tag: next auto number; schema defaults filled; x and y draw a view |
| `element.get` | `{id}` -> `{element, views, outgoing, incoming}` |
| `element.rename`, `element.setProperty` | `{id, name}` (every view shows the name); `{id, key, value}` (null removes) |
| `element.retag` | `{id, tag?}` -> `{id, tag}`; no tag: next auto number (refused when the scope does not number) |
| `element.show` | `{id, page?, x, y, w?, h?}` -> `{id, shape}`: another view; refused when that page already draws it |
| `element.deleteFromView` | `{ids?}` (bound shapes, default the selection) -> `{deleted, detached}`; elements stay in the model |
| `relationship.create` | `{from, to, type?, name?, properties?, id?, strict?}` -> `{id, type, legal, connector?}`; type defaults from the connection table; `strict` refuses an illegal one; a connector is drawn when both ends share a page |
| `relationship.retype`, `relationship.reconnect`, `relationship.setProperty` | `{id, type}` -> `{id, type, legal}`; `{id, from?, to?}` -> `{id, source, target, legal, removed}` (views reglued on their page or removed); `{id, key, value}` |
| `validate.run` | `{scope?: "all" \| "page" \| "selection", page?, ids?}` -> `{issues, live, batch}`: live issues for the scope plus the batch rules, sorted by severity, rule and first subject |
| `validate.list` | `{severity?: "error" \| "warning" \| "info", rule?}` -> `{issues}` from `Session.issues` |
| `notation.list` | `{}` -> `{packs: [{id, version, name, audience, description, types, glyphs}]}` (`types` and `glyphs` are counts) |
| `notation.describe` | `{pack, type?}` -> the pack schema as JSON; with `type` (qualified or local): `{type, pack, legal}` for an element type, `{relationship, pack}` for a relationship type |
| `notation.legal` | `{from, to?}` (element, shape or type ids) -> `{from, relationships: [{relationship, targets}]}`; with `to`, only relationships reaching its type, plus `to` and `legal` |

Shapes and connectors from the existing commands join the model as well: `shape.add` with a
master whose glyph names an element type (every Flowchart master) also creates that element and
returns `{id, element}`; `shape.setText` and `connector.setText` on bound items rename the model
item and every view; `edit.copy` carries the bound items' elements and relationships and
`edit.paste` makes new ones, so a paste is never an extra view of the original; deleting a shape
or connector removes it from the view, keeps elements in the model, and removes a relationship
whose last connector view went.

Layer commands (Block 2). A page is named by id, name or index and defaults to the active page; a
layer by id or name. Each change is one undo step. The rules live in `vizcraft_diagram::layer`: a
shape on no layer is visible, selectable, printable, snappable and gluable; on several layers it
is visible when any is visible and prints when any visible one prints, but it is locked when any
is locked and takes no snap or glue when any refuses it (its own `behavior` flags count too).
Hidden shapes are not drawn or hit; locked shapes are not hit or marquee-selected, and
`edit.select`, `shape.move`, `shape.resize`, `shape.rotate`, `shape.setText`, `shape.setStyle`,
`shape.delete`, `edit.delete` and `edit.cut` refuse them (`cmd::layer::ensure_unlocked` and
`ensure_selectable`); `connector.add` and `connector.glue` refuse glue to a hidden or no-glue
shape (`ensure_gluable`), and the canvas finds glue targets with `hit::glue_shape_at`. A layer
colour tints its shapes on the canvas (line and text take it, the fill moves halfway to it).
`layer::unprinted` lists what printing will skip; `file.print` (and `file.export` with `print: true`)
leaves those shapes out (`cmd/print.rs`).

| Command | Params -> result |
|---|---|
| `layer.add` | `{name, page?}` -> `{page, layer}`; names are unique per page, ignoring case |
| `layer.delete` | `{layer, page?, deleteShapes?}` -> `{page, layer, deleted}`; by default its shapes stay and leave the layer; `deleteShapes` deletes them, refused while any is locked |
| `layer.set` | `{layer, page?, name?, visible?, locked?, print?, snap?, glue?, color?}` -> `{page, layer}`; `color` null or `""` clears; hiding or locking drops those shapes from the selection |
| `layer.assign`, `layer.unassign` | `{ids?, layer, page?}` -> `{page, layer, changed}`; `ids` default to the selected shapes |

### Transactions and scripts

Engine infrastructure that later features build on (`crates/engine/src/session.rs`,
`crates/engine/src/fragment.rs`, `cmd/batch.rs`, `cmd/script.rs`):

| Command | Params -> result |
|---|---|
| `edit.batch` | `{label?, commands: [{command, params?}]}` -> `{label, completed, results: [{command, result}]}`; one undo step named `label` (default `Batch`); a failing command rolls the whole batch back and the error reads `commands[i] (id) failed: ...` |
| `script.recordStart` | `{}` -> `{recording: true}`; starts over when a recording is running |
| `script.recordStop` | `{}` -> `{count, script: [{command, params}]}`; disabled while nothing is recorded |
| `script.run` | `{script: [{command, params?}] or that list as a JSON string, label?}` -> as `edit.batch`; one undo step named `label` (default `Run Script`), logged with origin `script` |

- **Transactions.** `Session::transaction(label, steps)` runs `(id, params)` steps through
  `Session::run` with undo recording held: the outer transaction sets the undo and redo stacks
  aside, commits one undo step when the document changed, and on a failure restores document,
  selection, path, dirty flag and both stacks (so a `file.new` inside a failed batch is taken back
  too). Transactions nest up to `MAX_TXN_DEPTH` (8) and run at most `MAX_TXN_COMMANDS`; undo and
  redo are refused inside one. `edit.batch` and `script.run` are thin commands over it, and the
  MCP `command_batch` tool sends its list as one `edit.batch`.
- **Transaction log.** Every committed undo step appends a `TxnRecord {label, origin, commands}`
  to `Session::transactions()` (oldest first, at most `MAX_LOG` = 200, at most 256 ids each;
  coalesced typing extends the last record). `Origin` is `ui`, `control`, `mcp` or `script`;
  `Session::run_as(origin, id, params)` sets it for one call and the commands it runs. The
  headless MCP backend runs as `mcp`; the app's control channel still runs as `ui` until it calls
  `run_as`.
- **Fragments.** `Fragment::extract(doc, wanted)` (or `Fragment::of_selection`) takes shapes and
  connectors from the current page with the model elements and relationships they draw; glue to
  shapes outside the fragment becomes a free end. `Fragment::insert(doc, dx, dy)` adds a copy with
  fresh ids, glue between fragment items remapped and model items copied once each, and returns the
  new ids. `to_json` and `from_json` are the clipboard form. Copy, cut and paste use it; duplicate,
  paste special, format painter and change shape are meant to.
- **Scripts.** While recording, each command run directly (not inside a transaction) that
  succeeds is appended as `{command, params}`, except `view.*`, `ui.*`, `app.*`, `help.*`,
  `file.*` and `script.*` (`session::recordable`). Undo drops the recorded commands of the step
  it undid, with any recorded after it, and Redo puts them back, so a replay on the document as
  it was when recording started gives the same document.

## Engine (`vizcraft-engine`) public API

```rust
pub struct Session {
    pub doc: Document,                 // vizcraft_diagram::Document, re-exported by crates/engine/src/doc.rs
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub author: String,
    pub sel: Selection,                // generalized selection (primary, items: Vec<String>, range: Option<(u64, u64)>)
    pub view: ViewState,               // zoom (f32), pan ([f32; 2]), dark_mode, pane visibility flags, canvas: Option<[f32; 2]>, aids: cmd::view::Aids
    pub registry: Registry,            // Registry::get(id) -> Option<&CommandSpec>, Registry::all() -> &[CommandSpec]
    pub ui_requests: Vec<Value>,       // requests to the UI, drained by the UI after each command
    pub status: String,                // one-shot status message, taken by the UI
    pub clipboard: String,             // in-process clipboard for edit.cut / edit.copy / edit.paste
    pub issues: Vec<Issue>,            // live issues (vizcraft_model::Issue), recomputed after every change, undo, redo and open
    // private: undo/redo stacks
}
impl Session {
    pub fn new(doc: Document) -> Self;
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, EngineError>; // panic-guarded; unknown id is an error
    pub fn commands(&self) -> Vec<CommandInfo>;   // id, label, shortcut, location, enabled: bool, reason
    pub fn can_undo(&self) -> bool; pub fn can_redo(&self) -> bool; pub fn undo_label(&self) -> Option<&str>;
    pub fn run_as(&mut self, origin: Origin, id: &str, params: &Value) -> Result<Value, EngineError>;
    pub fn transaction(&mut self, label: &str, steps: &[(String, Value)]) -> Result<Vec<Value>, EngineError>; // one undo step, all or nothing
    pub fn transactions(&self) -> &VecDeque<TxnRecord>; // committed steps: label, origin, command ids
    pub fn record_start(&mut self); pub fn record_stop(&mut self) -> Option<Vec<Value>>; pub fn is_recording(&self) -> bool;
    pub fn rev(&self) -> u64;                     // bumps on every document change
    pub fn refresh_issues(&mut self);             // rerun the live rules (panic-guarded)
}
pub mod io { pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, EngineError>; pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Document, EngineError>;
             pub fn export_bytes(name: &str, format: Option<&str>, doc: &Document) -> Result<Vec<u8>, EngineError>; }
pub mod sample { pub fn sample_document() -> Document; }
pub mod fragment { pub struct Fragment { shapes, connectors, elements, relationships } // extract, of_selection, insert, to_json, from_json
}
```

Documents are `.vizcraft` files (`io::EXTENSION`), versioned JSON read and written by
`vizcraft-formats` (a model-class crate beside `diagram`); `io` dispatches by file name and
still reads `.craft.json` and `.json`; `.drawio` and `.drawio.xml` names open and save as draw.io.
`file.export {path, format?, page?, pages?}` writes the active page (or `page`, or `pages`) as SVG or PNG, the
foreground pages as PDF, or the document as draw.io; `file.import` brings in draw.io, Mermaid and CSV.
`ViewState::canvas` is the canvas size in points as the UI last laid it out (`None` headless,
never serialized); the UI writes it every frame so `view.zoomFit {page?}` computes zoom and pan in
the engine with the other view commands and replies with them synchronously. The UI runs
`view.zoomFit` after `file.open` or a replacing `file.import` when the page is cut off, unless the
params carry `"fit": false`.
`ViewState::aids` (`cmd::view::Aids`) holds the visual aids: grid, rulers, guides, page breaks,
connection points, AutoConnect, and snapping (on or off, the grid, guide, shape and connection
point targets, and a strength in screen points). They are view settings, never document data or
undo steps, set by `view.grid`, `view.rulers`, `view.guides`, `view.pageBreaks`,
`view.connectionPoints`, `view.autoConnect` (each `{value?}`, flipping without one) and
`view.snap {value?, grid?, guides?, shapes?, points?, strength?}`; `view.snapSettings` asks the UI
for the Snap and Glue dialog. The UI saves them in `UiState::aids`. Guides are page data
(`Page::guides`): `guide.add {orientation, position, page?}`, `guide.move {index, position,
page?}` and `guide.delete {index, page?}` change the document and undo like any edit. The canvas
snaps a move drag with `vizcraft_diagram::snap` (guides first, then shape edges, centres and
connection points, then the grid; Alt turns it off for the drag) and draws the alignment lines it
returns. After every command that
changes the document, `Session::run` calls `vizcraft_notation::bind::sync` (inside the command's
panic guard; not after a command that replaced the document) and then
`vizcraft_diagram::autoroute::refresh`, inside the same undo step, so the model and its views stay
in step and connectors that avoid shapes are rerouted when anything moves; then it recomputes
`Session.issues` with `vizcraft_notation::rules::live`. See
[docs/formats.md](formats.md). Engine crate root carries
`#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`.
A test `hostile_params_never_panic` runs every command with junk params.

## UI (`vizcraft-ui-egui`) public API

Modules: `theme`, `icons`, `widgets`, `chrome` (title bar + status bar), `menus` (menu tree +
in-window menu bar), `ribbon` (Home, Insert, Design, Process, View, Help, and the contextual
Connector tab; Data, Review and Developer exist but stay hidden while they have no groups),
`panes` (side panes, `PaneSpec`, `set_panes`; Shapes, Navigation, Properties, History),
`canvas` (the diagram canvas: `Viewport2D` screen/world projections; drawing of the active page
over its background pages, which are drawn only and never hit; shapes, text and connectors; selection, marquee, move, resize and rotate handles, connector drag and
reglue, in-place text editing, arrow-key nudge; pan with middle-drag, Space+left-drag or
Cmd+left-drag, Ctrl off macOS; `insert_point` and `insert_master` place new shapes), `dialogs`,
`keys`, `control`, `window_geometry`, `backstage` (optional File page), `page_tabs` (the page tab
strip under the canvas: click switches, double-click renames, plus adds, right-click inserts,
duplicates, renames, deletes and assigns a background) and `page_setup` (the Page Setup dialog
and the paper size presets the Design tab's Size menu shares).

Files are cut so that parallel work touches separate files:

| Path under `crates/ui-egui/src` | Holds | To add one |
| --- | --- | --- |
| `ribbon/mod.rs` | `Tab`, `Group`, `Control`, shared helpers, `tabs()` in Visio order | |
| `ribbon/draw.rs`, `ribbon/controls.rs` | Tab strip, body, collapsed popup; colour, gallery and caption controls | |
| `ribbon/<tab>/mod.rs`, `ribbon/<tab>/<group>.rs` | One directory per tab, one file per group | Group: a file with `fn group() -> Group` plus one line in the tab's `groups!` list |
| `panes/mod.rs` | `PaneSpec`, the pane list, `ui.pane*` commands | |
| `panes/layout.rs`, `panes/manager.rs` | `PaneLayout`; drawing of the sides, headers and splitters | |
| `panes/<pane>.rs` | One file per pane with its `SPEC` | Pane: a file plus one line in `default_panes!` and a `ui.pane.<key>` row in `UI_COMMANDS` |
| `canvas/mod.rs` | Viewport, `CanvasState`, pan and zoom, insertion, per-frame `show` | |
| `canvas/draw/` | `mod.rs` conversions and sheet; `shapes.rs` (background pages, then per shape glow, shadow, solid or gradient fill with soft edges, dashed and capped outline, as SVG export draws them; gradients turn with a rotated shape), `text.rs`, `connectors.rs`, `overlays.rs` (selection chrome) | |
| `canvas/tess.rs` | Ear-clipping triangulation, gradient meshes (linear cut into exact bands, radial subdivided), dash patterns, ring offsets and fading strips for blur, glow and soft edges | |
| `canvas/interact/` | `mod.rs` `Drag` and pointer dispatch; `select.rs`, `drag.rs`, `connect.rs`, `tools.rs`, `edit_text.rs`, `keys.rs` | |

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
    pub in_window_menus: bool, pub aids: vizcraft_engine::cmd::view::Aids,
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
| `Document` | `theme: DocTheme { id, variant }` (see Design commands), `title`, `core: CoreProps` (author, timestamps, revision), `pages: Vec<Page>` (always at least one), `id_seq`, `model: vizcraft_model::Model` (not in the document's serde form; the native file writes it as its own section); `next_id(prefix)` hands out `s1`, `c2`, `e3`, `r4` and so on, never reused; pages, shapes, connectors, elements and relationships share one id space (`contains_id`); `properties: DocProperties` (title, author, subject, keywords, description), `comments: Vec<Comment>` (id, `anchor: CommentAnchor { page, shape? }`, author, time, text, `replies: Vec<Reply>`), `images: Vec<ImageAsset>` (id, `media_type` `image/...`, `bytes` as base64 on disk; 16 MiB each, 48 MiB in all, 1024 assets); `background_chain(index)` lists the background pages under a page, deepest first; `active_page` (a page id, empty or unknown meaning the first) picks the page `page()`, `page_mut()` and `active_index()` return first |
| `Page` | Id, name, width and height (the size; 1100 x 850 by default), `shapes` and `connectors` in z order, `ext`, `view: Option<View>` (the viewpoint the page shows; `None` for free drawing), `orientation`, `units`, `drawing_scale: DrawingScale { page, drawing, units }`, `auto_size`, `background: Option<String>` (another page with `is_background`), `is_background`, `header` and `footer: HeaderFooter { left, center, right, margin? }` (slots may hold `{page}`-style fields), `guides: Vec<Guide { orientation, position }>`, `layers: Vec<Layer { id, name, visible, locked, print, snap, glue, color? }>`; `shape_visible` (not hidden, and on no layer or on a visible one) |
| `Shape` | Id, `stencil` master id (`basic.rectangle`), `kind: ShapeKind` (16 outlines), frame `x, y, w, h`, `angle_deg`, text, `Style`, optional custom connection points (normalized; kinds default to top, right, bottom, left), `ext` (data from other formats kept for round trips), `element: Option<String>` (the model element it draws; `None` is free drawing), `layers` (page layer ids), `parent: Option<String>` (group or container on the same page; cycles are broken on load), `text_block: Option<TextBlock { dx, dy, w, h, angle_deg }>` (`None` is the frame; `text_rect()`), `links: Vec<Hyperlink { kind: url, file, page, shape or element, target, label }>`, `screen_tip`, `locks: Locks` (width, height, aspect, position, rotation, delete, select, text), `behavior: Behavior` (container, non-printing, hidden, no glue, no snap), `image: Option<ImageRef { asset, crop: CropRect }>` |
| `Connector` | Id, `start` and `end` as `End::Glued { shape, point: Option<usize> }` (no point = dynamic glue) or `End::Free { x, y }`, `route` (`straight`, `orthogonal`, `curved`), arrowheads per end (`none`, `triangle`, `open`, `diamond`, `circle`), label text, `Style`, `waypoints` (stored bends), `avoid` (route around shapes), `jumps` (line jumps), `ext`, `relationship: Option<String>` (the model relationship it draws) |
| `Style` | Fill, line colour and width, font size and colour, bold, italic; colours serialize as `#rrggbb`. Text: `font_family`, `underline`, `strikethrough`, `text_case`, `highlight`, `h_align`, `v_align`, `bullets`, `indent`, `text_direction`, `text_margins: Margins`. Line: `dash`, `line_cap`, `corner_radius` (`Path::rounded_corners`). Effects: `gradient: Option<Gradient { kind, angle_deg, stops }>`, `fill_transparency` and `line_transparency` (0 to 1; `effective_fill`, `effective_line`), `shadow: Option<Shadow { dx, dy, blur, color }>`, `soft_edges`, `glow: Option<Glow { radius, color }>`. A `const fn Style::basic` builds a style in constants |

The richer fields live in `style_fields`, `shape_fields`, `page_fields` and `doc_fields`. The line
and effect style fields are set by `shape.setStyle` and drawn on the canvas; the rest are data
only so far. The SVG and PNG exports draw the ones that make sense in a static image. Every one has a serde default and is left out of files when it has its default
value, so older files load unchanged; `sanitize` clamps their numbers, caps string and list
lengths, and clears references that name nothing (unknown layers, parents, backgrounds, image
assets and comment anchors).

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
`master(id)`, `search(query)` and `new_shape(master_id, id, x, y, w?, h?)`. `views::{sanitize_refs,
views_of, items_referencing, elements_on_page}` relate diagram items to model items; loading clears
references to model items that do not exist, so those items load as free drawing.

## Semantic model and notation packs (`vizcraft-model`, `vizcraft-notation`)

Built in Cascade 4A. Shapes are views of typed elements; notations are data packs. Reasoning:
[docs/research/professional-features.md](research/professional-features.md).

| Type | Holds |
|---|---|
| `Model` | `schema_version`, `packs: Vec<PackRef>` (id, version), `elements`, `relationships`; `add_element`, `add_relationship`, `set_owner` (cycles refused), `descendants`, `ancestors`, `remove` (cascades to owned elements and touching relationships), `outgoing`, `incoming`, `children`, `sanitize` (caps, repairs references, breaks cycles) |
| `Element` | Id, pack-qualified `type` (`flowchart.Decision`), `name` (shown as the text of every view), `tag` (from the identity registry), `owner` (containment; `None` is the project root), typed `properties` (`PropValue`: bool, number, text) |
| `Relationship` | Id, pack-qualified `type`, `source` and `target` element ids, optional `source_port` and `target_port`, `name` (shown on every connector view), `properties` |
| `View` | `viewpoint` (pack-qualified) and `layout`, carried by a page as `Page.view` |
| `Issue` | `rule` (pack-qualified), `severity` (error, warning, info), `message`, `subjects` (element and relationship ids), `items` (shapes and connectors drawing them), `page?`; `key()` identifies the same problem across runs |
| `identity::{IdScope, IdFormat, Registry}` | Identifier scopes from packs (types, format such as `S{n}` or `{letters}-{n:3}`, unique, paired, auto-number); `next_tag`, `is_free`, `duplicates`, `unmatched` |

A notation pack (`vizcraft_notation::Pack`, schema version 1, JSON or TOML) declares, all with
pack-qualified ids: glyphs (built-in outline, size, our own colours, keywords and the element type
a placed glyph becomes; glyphs are the stencil masters), element types (label, glyph, property
schema with kind, unit, enum values, required and default, ports, container flag), relationship
types, the connection table (source types, relationship, target types, `outMax`, `inMax`),
conditional styles, palette groups, viewpoints, identifier scopes, rules, and the templates,
patterns and exporters it supports. `loader::{load, load_json, load_toml}` cap the size and run
`validate::check`, so a bad pack is an error, never a panic. `catalog()` holds the built-in Basic
(glyphs only: free drawing) and Flowchart packs.

Rules are data: each declares an id, severity, mode (live after every change, or batch on request)
and one check from a fixed vocabulary: `illegalRelationship`, `multiplicity`, `requiredProperty`,
`propertyType`, `duplicateIdentifier`, `unmatchedPair`, `viewpointType`, `danglingConnector`,
`minOutgoing`, `minIncoming`, `exclusiveDirections`, `unreachable`. `rules::live` and
`rules::batch` (with a `Scope` of all, a page or items) run the rules of every pack the model uses
and return issues sorted by severity, rule and first subject.

`bind::sync(doc, before, catalog)` runs after every document change: references to missing model
items are cleared; a second view of one element on a page becomes a new element; a connector glued
at both ends to bound shapes gets a relationship of the connection table's type for that pair
(an illegal pair still binds, so its rule reports it) and its relationship follows its glue (ports
from connection-point indexes); a relationship whose last connector view went is removed unless it
was reconnected in that command; names are mirrored to view texts; conditional style fields are
applied when an element's match changes (hand styling otherwise stays); used packs are recorded.
Free drawing is never touched, so Cascade 1 to 3 documents pass through unchanged.

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
| model | `crates/stencils` | Grows the domain stencil sets (Cascade 4) |
| model | `crates/formats` | Grows .drawio read and write, Mermaid and CSV import, PDF export, .vsdx import (Cascades 2 to 4) |
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
| `page.*` | Built: list, get, set, add, delete, duplicate, rename, move, setActive, setup, setBackground, fitToDrawing | `crates/engine/src/cmd/page.rs` |
| `layer.*` | Built: add, delete, rename, flags, assign shapes (see Layer commands above) | `crates/engine/src/cmd/layer.rs` |
| `arrange.*` | Built: align, distribute, order, group, ungroup, flip, rotate, size and position (see the arrange commands above) | `crates/engine/src/cmd/arrange.rs` |
| `design.*` | Format painter (themes, quick styles, backgrounds, borders and the connector style are built) | `crates/engine/src/cmd/design.rs` |
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

`vizcraft-model` and `vizcraft-notation` are built (Cascade 4A; see the contract above). The two
still planned sit in the model layer too: below `engine`, no egui, no engine dependency, wasm safe,
with the standard crate-root `deny` list. When each lands, `MODEL_SUFFIXES` in
`xtask/src/layers.rs` gains its suffix (`-patterns`, `-codegen`) and the contract table above gains
its row.

| Layer | Crate | Job | Depends on |
|---|---|---|---|
| model | `crates/patterns` (`vizcraft-patterns`) | Pattern generators with typed parameters, provenance and reparameterize, emitting model edits | `model`, `notation`, `layout` |
| model | `crates/codegen` (`vizcraft-codegen`) | Generator framework under the regeneration contract: specs, manifests, deterministic writers, merge, targets | `model`, `notation` |

`formats` gains pack-specific importers and exporters (SCXML, SVD, DDL, BPMN XML and so on), and
`ui-egui` gains the model tree, scoped palette, Inspector, bottom dock and canvas
insertion.

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

These join the prefixes above (the Cascade 4A prefixes `model.*`, `element.*`, `relationship.*`,
`validate.*` and `notation.*` are built):

| Prefix | Covers | Cascade |
|---|---|---|
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
Also built in Cascade 3: the `Headless` backend, which hosts a `Session` in the server process
and answers `engine.execute`, `engine.commands` and the document part of `ui.inspect` itself;
page screenshots through `file.export {format: "png"}`, so they need no window on either
backend; and the page, shape and stencil resources, read through `page.get`, `shape.get` and
`shape.catalog`. Built in Cascade 4A: notation resources (`vizcraft://notation`,
`vizcraft://notation/{pack}`, `vizcraft://notation/{pack}/{type}`, read through `notation.list` and
`notation.describe`), and `command_run` and `command_batch` results carry `issues` (the live
issues the call raised, from `validate.list` before and after) and `openIssues`. Tool list, resources and
launch: [docs/mcp.md](mcp.md). File formats and their public sources: [docs/formats.md](formats.md).
