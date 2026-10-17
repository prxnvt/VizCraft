# VizCraft roadmap

VizCraft is built in cascades: groups of pieces that several agents build in parallel, each in
its own branch, against the contract in [docs/architecture.md](docs/architecture.md), then
integrate. Each cascade ends with `cargo xtask ci` green on `main` and the app runnable. This
file says what each cascade contains and what has landed; update the State column when a piece
merges, and the matching rows in [docs/parity.md](docs/parity.md) when a feature lands.

## Where we are

| Measure | Value |
|---|---|
| Cascade | 0 done; Cascade 1 (diagram core) in progress |
| App runs | full chrome: ribbon, panes, menus, dialogs, command palette, control channel, multiple windows, web shell |
| Document model | `vizcraft-diagram` model and `vizcraft-stencils` catalog in the engine; commands, canvas and formats in progress |
| MCP server | minimal server with the generic tools (Remote backend); draw.io-style tools in Cascade 3 |

## Cascade 0: rename and documentation reset (done)

| Piece | Contents | State |
|---|---|---|
| Rename | Crates, binaries, bundle id `ai.storyteller.vizcraft`, `VIZCRAFT_*` environment variables, preferences folder `VizCraft` and file names renamed with the former template's rename tool; snapshots updated | done |
| Template machinery removed | The xtask subcommand that copied the template, and the template marker file, deleted | done |
| Documentation rewritten | README, AGENTS, this roadmap, the architecture contract, the parity catalog and the planned MCP and format specs describe VizCraft the diagramming app | done |

## Cascade 1: diagram core

The smallest useful diagram editor: draw shapes, type in them, connect them, save, export SVG,
and drive it all from an agent.

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Diagram model | `vizcraft-diagram`: `Document`, `Page`, `Shape`, `Connector`, ids, geometry (position, size, rotation), text, basic style, connection points, hit testing in world space; geometry helpers ported from VectorCraft's `geom` where they fit | `crates/diagram` (new) | done |
| Native file format | `.vizcraft` JSON read and write, versioned, replacing the placeholder `.craft.json` model in the engine | `crates/formats` (new), `crates/engine/src/io.rs` | done |
| Basic shapes and stencils | Rectangle, rounded rectangle, ellipse, diamond, triangle, parallelogram, hexagon, cylinder, document, cloud; Basic and Flowchart stencil sets defined in code, with categories | `crates/stencils` (new) | done |
| Shape commands | `shape.add`, `shape.delete`, `shape.move`, `shape.resize`, `shape.rotate`, `shape.setText`, `shape.setStyle`, each undoable and covered by `hostile_params_never_panic` | `crates/engine/src/cmd/shape.rs` | done |
| Connector commands and routing | `connector.add`, `connector.glue`, `connector.setRoute`, `connector.setArrows`; dynamic glue (to the shape) and point glue (to a connection point); straight, orthogonal and curved routes; arrowheads | `crates/diagram` (routing), `crates/engine/src/cmd/connector.rs` | done |
| Diagram canvas | Draw the page through `Viewport2D`; select, multi-select, marquee; move, resize and rotate handles; connector drag from connection points; in-place text editing | `crates/ui-egui/src/canvas.rs` | done |
| Shapes pane | Stencil browser pane with the Basic and Flowchart sets; click or drag to insert | `crates/ui-egui/src/panes.rs` | done |
| Ribbon tabs | Home (clipboard, font, tools, shape styles) and Insert tabs wired to the new commands, icons drawn in `icons.rs` | `crates/ui-egui/src/ribbon.rs` | done |
| SVG export | `file.export` with `{format: "svg"}` writes the current page | `crates/formats` | done |
| Remove transport, restore Space pan | Delete `transport.*` and `TransportState` (playback has no use in a diagram app); Space+drag pans the canvas again alongside middle-drag and Cmd+drag | `crates/engine/src/cmd/transport.rs`, `session.rs`, `crates/ui-egui/src/canvas.rs` | done |
| App icon | An original VizCraft icon drawn for the app, rendered by `packaging/icons.sh`, with its `ATTRIBUTION.md` row | `packaging/`, `assets/` | done |
| Minimal MCP server | Stdio JSON-RPC server with `command_list`, `command_run`, `command_batch`, `doc_inspect`, `screenshot` over the control channel (Remote backend); protocol core ported from VectorCraft's `crates/mcp`; launched by `vizcraft-cli mcp` | `crates/mcp` (new), `apps/vizcraft-cli` | done |

Done when: a person can draw a small flowchart (shapes, text, glued connectors that follow when
shapes move), save it as `.vizcraft`, reopen it, export SVG, and an agent can build the same
diagram through the MCP server alone.

## Cascade 2: editing richness

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Pages | Multiple pages, page tabs, background pages, page setup (size, orientation, units); `page.*` commands | `crates/diagram`, `crates/engine/src/cmd/page.rs`, `crates/ui-egui/src/canvas.rs` | done |
| Layers | Layers with visible, lock and print flags, assign shapes; Layers pane; `layer.*` commands | `crates/diagram`, `crates/engine/src/cmd/layer.rs`, `crates/ui-egui/src/panes.rs` | planned |
| Groups and containers | Group and ungroup, containers that hold members, swimlanes | `crates/diagram`, `crates/engine/src/cmd/arrange.rs` | planned |
| Arrange | Align, distribute, bring forward and send backward, flip, rotate, size and position; Arrange ribbon tab | `crates/engine/src/cmd/arrange.rs`, `crates/ui-egui/src/ribbon.rs` | planned |
| Snap, grid, rulers, guides | Snap to grid, shapes and connection points; grid display; rulers; draggable guides; dynamic alignment guides while dragging | `crates/ui-egui/src/canvas.rs`, `crates/diagram` (snap math) | planned |
| Themes and styles | Themes, quick styles, format painter, fill, line, shadow, gradient; Design ribbon tab; `design.*` commands | `crates/diagram`, `crates/engine/src/cmd/design.rs`, `crates/ui-egui/src/ribbon.rs` | planned |
| Better routing | Line jumps where connectors cross; orthogonal rerouting that avoids obstacles | `crates/routing` (router and jumps, done), `crates/diagram` (wiring) | router done; wiring planned |
| Shape Data pane | Key and value data on shapes, edited in a pane; `data.*` commands | `crates/engine/src/cmd/data.rs`, `crates/ui-egui/src/panes.rs` | planned |
| Pan and Zoom pane | Overview of the page with the visible area as a draggable frame | `crates/ui-egui/src/panes.rs` | planned |
| Find and replace | Search shape text and shape data, replace | `crates/engine/src/cmd/edit.rs`, `crates/ui-egui/src/dialogs.rs` | planned |
| PNG and PDF export | Raster and vector export of a page or all pages | `crates/formats` | planned |

Done when: a multi-page cross-functional flowchart with swimlanes, layers, a theme and aligned
shapes can be built and exported to PNG and PDF, by hand and over the control channel.

## Cascade 3: draw.io interop and MCP parity

| Piece | Contents | Lands in | State |
|---|---|---|---|
| .drawio read and write | Multi-page files, compressed pages (deflate and base64), styles mapped to VizCraft styles; `quick-xml` and `zip` patterns ported from deckcraft's `pptx` and WordCraft's `docx`, adding `flate2` and `base64` | `crates/drawio` (codec, done), `crates/formats` (mapping) | codec done; mapping planned |
| Mermaid import | Flowchart, sequence, class, state and ER diagrams | `crates/mermaid` (parser, done), `crates/formats` (mapping) | parser done; mapping planned |
| CSV import | draw.io-style CSV with layout and connection hints | `crates/drawio` (parser, done), `crates/formats` (mapping) | parser done; mapping planned |
| Auto-layout | Layered, tree, radial and force layouts; re-layout a page or a selection; `layout.*` commands | `crates/layout` (algorithms, done), `crates/diagram`, `crates/engine/src/cmd/layout.rs` | algorithms done; commands planned |
| Stencil search | Keyword index over every stencil; search box in the Shapes pane | `crates/stencils`, `crates/ui-egui/src/panes.rs` | planned |
| MCP draw.io-style tools | `open_xml`, `open_mermaid`, `open_csv`, `search_shapes`, `get_reference`, `list_pages`, `get_page`, `set_page`, with `layout` and `routing` options | `crates/mcp` | planned |
| MCP resources and Headless backend | Resources for pages, shapes, commands and stencils; a Headless backend that hosts a `Session` in process, so no window is needed | `crates/mcp` | planned |

Done when: a .drawio file round-trips with its pages and connections intact, and an agent can
create, inspect and edit a diagram from Mermaid or CSV through the MCP tools with or without a
running window.

## Cascade 4: domain diagrams

| Piece | Contents | Lands in | State |
|---|---|---|---|
| UML | Class, sequence, state, activity and use case stencils and templates | `crates/stencils` | planned |
| BPMN | Events, activities, gateways, pools and lanes | `crates/stencils` | planned |
| ER | Entities and crow's foot relationships | `crates/stencils` | planned |
| Network | Generic network device stencils, drawn originally (no vendor brand icons) | `crates/stencils` | planned |
| Org chart | Org chart stencils and a template built from CSV | `crates/stencils`, `crates/formats` | planned |
| Timeline and cross-functional | Timeline shapes; cross-functional flowchart template | `crates/stencils` | planned |
| Data graphics | Callouts, icon sets and colour by value driven by shape data | `crates/diagram`, `crates/engine/src/cmd/data.rs` | planned |
| Validation | Diagram validation rules (unconnected connectors, flowchart rules) with an issues pane | `crates/diagram`, `crates/ui-egui/src/panes.rs` | planned |
| Comments and hyperlinks | Comments on shapes and pages; hyperlinks between pages, shapes and URLs | `crates/diagram`, `crates/engine` | planned |
| .vsdx import | Import from the public MS-VSDX specification (OPC zip and XML) | `crates/formats` | planned |

Done when: each diagram type has a template that opens to a valid example, validation flags a
broken flowchart, and a sample .vsdx built from the public specification imports.

## Future

Not scheduled, listed so they are not forgotten:

| Item | Notes |
|---|---|
| Web app | `apps/vizcraft-web` as a full diagram editor in the browser (the shell and the `wasm` gate exist) |
| Collaboration | Shared editing of one document |
| Plugin stencils | Stencil sets loaded from files at run time |
| Print | Page tiling and printing |
