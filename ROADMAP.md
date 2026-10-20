# VizCraft roadmap

VizCraft is built in cascades: groups of pieces that several agents build in parallel, each in
its own branch, against the contract in [docs/architecture.md](docs/architecture.md), then
integrate. Each cascade ends with `cargo xtask ci` green on `main` and the app runnable. This
file says what each cascade contains and what has landed; update the State column when a piece
merges, and the matching rows in [docs/parity.md](docs/parity.md) when a feature lands.

## Where we are

VizCraft is becoming a local, model-backed engineering workbench for three audiences: industrial
automation, enterprise and backend software, and embedded and low-level software. Every shape
becomes a view of a typed element in a semantic model; notations ship as data packs that drive the
palette, connection rules, validation and agent discovery; a model tree replaces the full-height
shape grid, with a typed Inspector on the right and Problems and Tables in a bottom dock; insertion
happens mostly on the canvas; parameterised pattern generators and code-scaffold generators work
over the model, and generated code is regenerated under a contract that never destroys hand-written
code. The research and reasoning behind this plan are in
[docs/research/professional-features.md](docs/research/professional-features.md). Cascades 2 and 3
finish the drawing substrate and interop; Cascade 4A builds the model core before any domain
notation, and the audience packs follow in Cascade 6.

| Measure | Value |
|---|---|
| Cascade | 0 and 1 done; Cascade 3 done (interop, layout, routing, MCP tools, resources, Headless backend, stencil search); Cascade 4A (model core) done; Cascade 2 (trimmed) done (Blocks 2A, 2B and the Pan and Zoom pane in Block 4 of docs/cascade-plan.md; done-when covered by `crates/engine/tests/cascade2_done_when.rs`); Block 2C (diagramming correctness: routing policy so connectors never cross shapes, infinite board by default, compartments, connector kinds and thirteen line ends, conformance profile with rulesets and a reasoner, structure-first `describe` for agents, check marks in every dropdown, settings organised in the ribbon) done; Cascade 4B (workspace redesign: model tree, scoped palette, typed Inspector, bottom dock, on-canvas insertion, command palette over the model, notation tabs, breadcrumbs; Block 3 of docs/cascade-plan.md) done; Block 4 (Microsoft Visio extras: freeform and boolean shapes, locks and protection, custom masters, Developer tab, hyperlinks, comments, subprocesses, presentation mode, Pan and Zoom pane, windows, add-ons, macros, spelling, reports, Info page, pictures, ink, data graphics and external data links) done; Block 5 (generators and glyphs: layout primitives with pins, notation glyph paths, ports, variants and end labels, tables and matrices with CSV, XLSX and Markdown export, the codegen framework with the Markdown catalogue target, the pattern framework with the starter workflow patterns, re-import and table-driven pattern sync) done; Blocks 6 (templates and the engineering workspace) and 6E (electrical and mechanical engineering) next, integrating on the branch named in docs/cascade-plan.md, then Block 7 (first deep packs) |
| Direction | Model-backed engineering workbench: semantic model, notation packs, rules, patterns and code-scaffold generators, then industrial, embedded and enterprise packs |
| App runs | a small diagram editor: canvas with shapes, text and glued connectors (obstacle-avoiding routes, line jumps), Shapes pane with search, Home, Insert, Design and Connector ribbon tabs, auto-layout, .drawio open and save, Mermaid and CSV import, inside the full chrome (menus, dialogs, command palette, control channel, multiple windows, web shell) |
| Document model | `vizcraft-model` (elements, relationships, containment, identity registry, issues) held by the diagram document, `vizcraft-notation` (Basic and Flowchart packs, rules engine), `vizcraft-diagram` (multi-page documents reachable through `page.*` and JGraph's draw.io files; the canvas shows the first page until page tabs land), `vizcraft-stencils` (masters built from the pack glyphs), saved as `.vizcraft` or `.drawio`, exported as SVG or PNG |
| MCP server | generic tools plus the draw.io-style diagram tools; resources for commands, stencils, pages, shapes and notation packs; Remote backend (a running app) and Headless backend (in process, no window) |

## Cascade 0: rename and documentation reset (done)

| Piece | Contents | State |
|---|---|---|
| Rename | Crates, binaries, bundle id `io.github.prxnvt.vizcraft`, `VIZCRAFT_*` environment variables, preferences folder `VizCraft` and file names renamed with the former template's rename tool; snapshots updated | done |
| Template machinery removed | The xtask subcommand that copied the template, and the template marker file, deleted | done |
| Documentation rewritten | README, AGENTS, this roadmap, the architecture contract, the parity catalog and the planned MCP and format specs describe VizCraft the diagramming app | done |

## Cascade 1: diagram core (done)

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

## Cascade 2: editing richness (trimmed, done)

Cascade 2 is trimmed to the pieces that later cascades build on: containers carry statechart
composites, swimlanes, zones and capability maps; layers and arrange are prerequisites for
patterns; the Shape Data pane is the first form of the typed Inspector.

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Pages | Multiple pages, page tabs, background pages, page setup (size, orientation, units); `page.*` editing commands | `crates/diagram`, `crates/engine/src/cmd/page.rs`, `crates/ui-egui/src/canvas.rs` | done (Block 2A `b2-pages`: active page, page tabs, background pages, page setup, auto-size, `page.*` editing commands) |
| Layers | Layers with visible, lock and print flags, assign shapes; Layers pane; `layer.*` commands | `crates/diagram`, `crates/engine/src/cmd/layer.rs`, `crates/ui-egui/src/panes.rs` | done (Block 2A `b2-layers`) |
| Groups, containers and swimlanes | Group and ungroup, containers that hold members and fit them, swimlanes; the substrate for statechart composites, zones, lanes and nested-container patterns | `crates/diagram`, `crates/engine/src/cmd/arrange.rs` | done (groups in Block 2A `b2-arrange`; containers, swimlanes and callouts in Block 2B `b2-containers`) |
| Arrange | Align, distribute, bring forward and send backward, flip, rotate, size and position; Arrange ribbon tab | `crates/engine/src/cmd/arrange.rs`, `crates/ui-egui/src/ribbon.rs` | done (Block 2A `b2-arrange`) |
| Snap, grid, rulers, guides | Snap to grid, shapes and connection points; grid display; rulers; draggable guides; dynamic alignment guides while dragging | `crates/ui-egui/src/canvas.rs`, `crates/diagram` (snap math) | done (Block 2A `b2-snap`; snapping on move drags only) |
| Light themes and styles | A small set of our own document themes, quick styles, format painter, fill, line, shadow, gradient; Design ribbon tab; `design.*` commands. Kept light: notation packs add conditional styles in Cascade 4A | `crates/diagram`, `crates/engine/src/cmd/design.rs`, `crates/ui-egui/src/ribbon.rs` | done (Block 2A `b2-styles`, format painter in Block 2B `b2-edit`) |
| Better routing | Line jumps where connectors cross; orthogonal rerouting that avoids obstacles | `crates/routing` (router and jumps), `crates/diagram` (`autoroute`, `jumps`, stored waypoints) | done (wired in Cascade 3) |
| Shape Data pane (first Inspector) | Typed key and value data on shapes, edited in a right-dock pane; each key may carry schema hooks (type, unit, enum, required, default) that notation packs fill in later; Cascade 4B grows the pane into the Inspector; `data.*` commands | `crates/engine/src/cmd/data.rs`, `crates/ui-egui/src/panes.rs` | done (Block 2B `b2-data`) |
| Pan and Zoom pane | Overview of the page with the visible area as a draggable frame | `crates/ui-egui/src/panes/pan_zoom.rs` | done (Block 4 `b4-view`) |
| Find and replace | Search shape text and shape data, replace | `crates/engine/src/cmd/edit.rs`, `crates/ui-egui/src/dialogs.rs` | done (Block 2B `b2-edit`) |
| PNG and PDF export | Raster and vector export of a page or all pages | `crates/formats` | done (Block 2B `b2-output`: PDF, multi-page PNG, headers and footers, print) |

Done when: a multi-page cross-functional flowchart with swimlanes, layers, a theme, typed shape
data and aligned shapes can be built and exported to PNG and PDF, by hand and over the control
channel.

In parallel: every piece runs in its own branch; Cascade 2 runs alongside Cascade 3 and the
Cascade 4A specification work.

## Cascade 3: draw.io interop and MCP parity

| Piece | Contents | Lands in | State |
|---|---|---|---|
| .drawio read and write | Multi-page files, compressed pages (deflate and base64), styles mapped to VizCraft styles, unknown keys kept for round trips; `file.open`, `file.save`, `file.export`, `file.import`, `page.list`, `page.get`, `page.set` | `crates/drawio` (codec), `crates/formats` (mapping), `crates/engine/src/cmd/{file,page}.rs` | done (groups, containers and layers are flattened until Cascade 2 models them) |
| Mermaid import | Flowchart, sequence, class, state and ER diagrams, laid out, subgraphs as background boxes | `crates/mermaid` (parser), `crates/formats` (mapping) | done |
| CSV import | draw.io-style CSV with layout and connection hints | `crates/drawio` (parser), `crates/formats` (mapping) | done |
| Auto-layout | Layered, tree, radial and force layouts: `layout.full` (page or selection), `layout.incremental` (existing shapes pinned), `layout.route` (connectors only); Design ribbon tab | `crates/layout` (algorithms), `crates/diagram` (`autolayout`), `crates/engine/src/cmd/layout.rs`, `crates/ui-egui/src/ribbon.rs` | done |
| Stencil search | Keyword index over every stencil; search box in the Shapes pane | `crates/stencils`, `crates/ui-egui/src/panes.rs` | done (ranked index in `stencils::search`, `ui.shapesSearch`) |
| MCP draw.io-style tools | `open_xml`, `open_mermaid`, `open_csv`, `search_shapes`, `get_reference`, `list_pages`, `get_page`, `set_page`, with `layout` and `routing` options; each tool is one engine command (`file.import`, `page.*`, `shape.search`) | `crates/mcp`, `crates/engine` | done |
| MCP resources and Headless backend | Resources for pages, shapes, commands and stencils; a Headless backend that hosts a `Session` in process, so no window is needed | `crates/mcp`, `crates/formats` (PNG), `apps/vizcraft-cli` | done (`vizcraft-cli mcp --headless` and the no-app fallback; page screenshots through `file.export` PNG on both backends; `shape.get`, `shape.catalog`) |

Done when: a .drawio file round-trips with its pages and connections intact, and an agent can
create, inspect and edit a diagram from Mermaid or CSV through the MCP tools with or without a
running window.

Additions requested by the research for Cascade 3 (done; see
[docs/research/professional-features.md](docs/research/professional-features.md)): layout is
exposed as three commands, because agents and people share documents: `layout.full` rearranges
everything, `layout.incremental` pins existing nodes and places only new ones, and `layout.route`
reroutes connectors without moving any node. Importers (draw.io, Mermaid, CSV) preserve source ids
where they are free and assign stable ids otherwise, and their mapping layer stays a thin function
from parser IR to commands, so that once Cascade 4A lands they write model elements and re-imports
keep view geometry for existing ids. As built, importers produce pages that `Document::adopt`
merges, keeping plain free source ids and renaming only clashes, and `file.import` with a
`layout` on a non-empty page places new shapes with `layout.incremental`'s placement.

## Cascade 4A: model core (done)

The semantic model and notation packs, built before any domain notation so no domain stencil has
to be reworked later. The contract is in
[docs/architecture.md](docs/architecture.md#semantic-model-and-notation-packs-vizcraft-model-vizcraft-notation).

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Semantic model types | `vizcraft-model`: `Element` (stable id, pack-qualified type, typed properties, tags, owner), `Relationship` (id, type, source and target elements with optional ports, properties), containment tree, `View` (viewpoint and layout, geometry keyed by element id); model queries | `crates/model` (new) | done |
| Shape and connector references | Optional `Shape.element` and `Connector.relationship` references; shapes without one stay free drawing, so Cascade 1 to 3 documents keep working; view geometry stays in the diagram | `crates/diagram`, `crates/model` | done |
| Native format model section | Versioned `.vizcraft` with a model section; older files load as free drawing; tolerant loading and sanitizing as today | `crates/formats` | done |
| Notation pack schema and loader | `vizcraft-notation`: versioned pack schema read from JSON or TOML: element types with property schemas and ports, original glyph definitions, connection table with multiplicity, conditional styles, palette groups, viewpoints, identifier formats, rule declarations, supported templates, patterns and exporters; loader returns errors, never panics | `crates/notation` (new) | done |
| Basic and Flowchart packs | The existing Basic and Flowchart sets re-expressed as the first two packs, glyphs migrated from `vizcraft-stencils`, flowchart rules (unconnected connectors, dangling decisions) declared as data | `crates/notation`, `crates/stencils` | done |
| Rules engine | Live rules after every command (illegal relationship, missing required property, duplicate tag) and batch rules on selection, page or project; structured issues (rule id, severity, element ids, message) | `crates/notation`, `crates/engine` | done |
| Identity registry | Project-wide registry for tags and reference designations: auto-numbering, uniqueness, composed identifiers from pack identifier formats, paired off-page references with unmatched-end detection | `crates/model`, `crates/notation` | done |
| Model commands | `model.*`, `element.*`, `relationship.*`: create, set property, retag, move in containment, show in all views, delete from view versus delete from model; undoable and covered by `hostile_params_never_panic` | `crates/engine/src/cmd/` (new `model.rs`, `element.rs`, `relationship.rs`) | done |
| Validation and notation commands | `validate.*` (run batch rules, list issues) and `notation.*` (list packs, describe a type, legal relationships from an element) | `crates/engine/src/cmd/` (new `validate.rs`, `notation.rs`) | done |
| MCP model integration | Notation schema discovery (resources and the generic tools); every command batch result carries the structured issues it raised | `crates/mcp` | done |

Done when: a flowchart built from the Flowchart pack is a model of elements and relationships, an
illegal connection or a duplicate tag raises a structured issue live, the model and its views
survive save and reload, Cascade 1 documents still open as free drawing, and an agent can discover
the pack schema and read the issues returned with each batch.

In parallel: the type and schema specification starts during Cascade 2; once the model types and
pack schema are frozen, the rules, identity, command and MCP pieces run in parallel, and Cascade
4B starts.

As built: the diagram `Document` holds the model, so `vizcraft-diagram` depends on
`vizcraft-model` (undo, dirty tracking and the panic guard cover both); `.vizcraft` version 3
carries the model as its own section; Flowchart masters placed with `shape.add` (and so from the
Shapes pane) become elements, and connectors between them become `flowchart.Flow` relationships
through the sync pass that runs after every command; the Flowchart pack declares its connection
table (data does not flow straight between data symbols), single-exit, terminator-direction,
duplicate step tag, missing reference, property type and viewpoint rules live, and dangling
decision, unconnected connector, unmatched reference and unreachable rules in batch; steps are
numbered `S1`, `S2` by the identity registry; MCP `command_run` and `command_batch` return the
issues each call raised, and the packs are MCP resources.

## Cascade 4B: workspace redesign

The model tree takes over the left dock, the typed Inspector the right, and insertion moves onto
the canvas.

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Model tree pane | Default left tab: containment tree (elements grouped by type with their views), Diagrams branch, Context mode that follows the canvas selection, show in all views, delete from view versus model; `ui.tree` | `crates/ui-egui/src/panes/model_tree.rs` | done (Block 3 `b3-tree`) |
| Scoped Palette pane | Replaces the full-height Basic and Flowchart tile grid: compact, searchable, filtered to the active view's notation and viewpoint, grouped by the pack's palette groups, free drawing primitives in one Drawing group at the bottom; the stencil cache rebuilds when a pack loads; `ui.palette` | `crates/ui-egui/src/palette.rs`, `panes/shapes.rs`, `crates/stencils` | done (Block 3 `b3-palette`) |
| Typed Inspector | Grows the Shape Data pane: properties typed by the element schema, Standard and Expert modes, composed tag or designation with a live uniqueness check, relationships list, appears in N views, Format as a secondary tab; `inspector.get`, `element.checkTag`, `ui.inspector` | `crates/ui-egui/src/panes/inspector.rs`, `crates/engine/src/cmd/inspector.rs` | done (Block 3 `b3-inspector`) |
| Bottom dock | New dock with Problems (issues, click to navigate), Tables and Generate slots (filled by Cascade 5B), Search results; `ui.dock`, `ui.search` | `crates/ui-egui/src/dock/` | done (Block 3 `b3-dock`) |
| Agent activity view | Each agent command batch listed as one undoable transaction in the bottom dock, so a person can review and revert | `crates/ui-egui/src/dock/activity.rs` | done (Block 3 `b3-dock`) |
| On-canvas insertion | Hover handles that clone and connect and suggest only legal next elements; double-click type-to-insert picker; Alt+Shift+Arrow keyboard chaining; magic connector offering only legal relationship types; `suggest.*`, `insert.next`, `insert.connect` | `crates/engine/src/suggest.rs`, `canvas/interact/insertion.rs`, `insert_picker.rs` | done (Block 3 `b3-insert`) |
| Command palette over the model | Cmd/Ctrl+K searches commands, element types, patterns and model elements by name or tag | `crates/ui-egui/src/command_palette.rs` | done but patterns (Block 3 `b3-insert`) |
| Backstage, ribbon and drill-down | New from notation or template grouped by audience; contextual ribbon tab per active notation plus Model, Validate and Generate tabs; drill-down breadcrumbs for child views | `crates/ui-egui/src/backstage.rs`, `ribbon/notation/`, `ribbon/model/`, `breadcrumbs.rs` | done but the Validate and Generate tabs (Block 3 `b3-palette`) |

Done when: a new Flowchart document opened from the backstage shows the model tree, a scoped
palette, a typed Inspector and a Problems tab that jumps to the offending shape, and a person can
build the flowchart mostly from the canvas (hover suggestions, the double-click picker, keyboard
chaining) without opening the palette.

In parallel: starts once the 4A interfaces settle; each pane is its own piece; runs alongside
Cascade 5A.

## Cascade 5A: pattern engine and layout primitives

Patterns are generators with typed parameters that emit model elements, not bare shapes.

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Pattern framework | `vizcraft-patterns`: generator definition with typed parameters (counts, labels, mode enums, tag prefixes, a data table), emits elements and relationships, applies a layout primitive, commits as one compound undo step; the inserted group records pattern id and parameter values (provenance) | `crates/patterns` (new) | done (Block 5 `b5-patterns`) |
| Pattern commands | `pattern.list` with a discoverable parameter schema, `pattern.insert(name, args)`, `pattern.reparameterize` (for example a saga from four steps to six) keeping manual label edits; reachable over MCP | `crates/engine/src/cmd/pattern.rs` (new), `crates/mcp` | done (Block 5 `b5-patterns`) |
| Pins and model order | Pinned nodes and model-order preservation in layered layout, used by `layout.incremental`, re-import and reparameterize | `crates/layout` | done (Block 5 `b5-layout`) |
| Band and chain primitives | Horizontal bands and swimlanes (partitioned layered), pipeline with a parallel band, two-row forward and compensation chain | `crates/layout` | done (Block 5 `b5-layout`) |
| Grid and container primitives | Matrix, nested-container packer, hub and spoke, concentric rings, timeline, N-channel voter | `crates/layout` | done (Block 5 `b5-layout`) |
| Patterns tab | Left dock tab filtered by audience and notation; each entry opens a parameter form | `crates/ui-egui/src/panes.rs` | done (Block 5 `b5-patterns`) |
| Starter workflow patterns | Workflow Patterns fragments over the Flowchart pack (parallel split, exclusive choice, N-of-M join, multiple instances, cancel region), proving the framework before the audience packs | `crates/patterns` | done (Block 5 `b5-patterns`) |

Done when: a parallel split with N branches inserts as model elements laid out by the pipeline
primitive in one undo step, `pattern.reparameterize` changes N while keeping edited labels, and an
agent can list the patterns and their parameter schemas over MCP.

In parallel: each layout primitive is its own piece; runs alongside Cascades 4B and 5B.

## Cascade 5B: codegen framework and table views

Generation is a command, under a contract that never destroys hand-written code (summarised in
[docs/architecture.md](docs/architecture.md#code-scaffold-generation-contract)).

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Generator framework | `vizcraft-codegen`: generator spec file in the repository (target, options, output directory, model scope), target definition with a construct-support matrix, deterministic writer (stable order and formatting) | `crates/codegen` (new) | done (Block 5 `b5-codegen`; the Markdown catalogue is the first target) |
| Ownership manifest and safe writes | Manifest beside the output with the checksum last written; writes only owned files inside the declared directory; on a changed checksum, three-way merge or stop with an issue; rename map so identifiers do not churn | `crates/codegen` | done (Block 5 `b5-codegen`) |
| Traceability and regions | `vizcraft:id=<element id>` markers on every generated construct; protected regions keyed by element id for single-file targets only | `crates/codegen` | done (Block 5 `b5-codegen`) |
| Generate commands and CLI | `generate.preview`, `generate.run`, `generate.check`; `vizcraft-cli generate --check` for CI; reachable over MCP | `crates/engine/src/cmd/generate.rs` (new), `apps/vizcraft-cli` | done (Block 5 `b5-codegen`) |
| Generate dock and ribbon tab | Preview diff and run log in the bottom dock; Generate ribbon tab | `crates/ui-egui` | done (Block 5 `b5-codegen`) |
| Table exporters | CSV, XLSX and Markdown writers for model-derived tables | `crates/formats` | done (Block 5 `b5-tables`) |
| Derived table and matrix views | Tables and matrices as views over the model, edited in place, in the Tables tab (lists, relationship matrix, RACI) | `crates/model` (queries), `crates/ui-egui` | done (Block 5 `b5-tables`) |
| Re-import contract | Imported elements keep model ids; re-import updates the model, keeps view geometry for existing ids, places new elements with `layout.incremental` and flags removed ones | `crates/formats`, `crates/model` | done (Block 5 `b5-reimport`) |
| Table-driven pattern sync | Patterns generated from a table sync both ways where the mapping is unambiguous (RACI to swimlanes, parent-id table to org chart, cause-and-effect table to matrix) | `crates/patterns`, `crates/model` | done (Block 5 `b5-reimport`) |

Done when: a reference target (a Markdown element catalogue over the Flowchart pack) generates
under the contract: regenerating an unchanged model is byte-identical, a hand edit to an owned file
is merged or stopped with an issue, `vizcraft-cli generate --check` fails when output is stale, and
a model-derived table edits in place and exports CSV and XLSX.

In parallel: runs alongside Cascade 5A; the framework, manifest and command pieces come first, the
table pieces run beside them.

## Blocks 6 and 6E: templates, analysis and the engineering substrate

Built on the integration branch `claude/engineering-diagramming-standards-305d6b`
([docs/cascade-plan.md](docs/cascade-plan.md), "How Blocks 6 and 6E run"), backend first.

| Piece | Contents | Crates | State |
|---|---|---|---|
| Templates (engine) | `Pack.templates`, the template pipeline with optional hooks, `Document.standards`, `template.*`, `vizcraft-cli new` | `crates/notation`, `crates/diagram`, `crates/engine` | done (wave 1 `b6-templates`); gallery in the frontend wave |
| Analysis (engine) | `vizcraft-analysis`, `analyze.*`, `model.diff`, `vizcraft-cli analyze` | `crates/analysis`, `crates/engine` | done (wave 1 `b6-analysis`); highlights and dock in the frontend wave |
| Nets, wires and orientation (engine) | Nets as model objects, junctions, split and heal, line labels and markers, symbol grid module, quarter-turn and mirror orientation; `net.*`, `wire.*`, `page.setModule`, `shape.mirror` | `crates/model`, `crates/diagram`, `crates/notation`, `crates/engine`, `crates/formats` | done (wave 1 `b6e-substrate`); Wire and Pipe tools and keys in the frontend wave |
| Sheets and cross references (engine) | ISO 5457 and ASME Y14.1 sheets with zones, title blocks, revision tables, parts lists, document properties, composed identifiers, cross references; `sheet.*`, `doc.*`, `xref.*`; the `sheet` pack | `crates/diagram`, `crates/model`, `crates/notation`, `crates/engine`, `crates/formats` | done (wave 1 `b6e-sheets`); ribbon and navigator in the frontend wave |
| Parametric glyphs (engine) | Ten symbol builders, `Shape.glyph`, rebuild before bind, glyph provenance in `cargo xtask assets`; `glyph.*`, `vizcraft-cli glyph` | `crates/stencils`, `crates/notation`, `crates/diagram`, `crates/engine`, `xtask` | done (wave 1 `b6e-builders`); Inspector Symbol group in the frontend wave |
| UML and sequence (engine) | `uml` pack in sections, member rows, end labels, eight templates, PlantUML export, Mermaid class, state and sequence; the sequence substrate and `sequence.*` | `crates/notation`, `crates/diagram`, `crates/layout`, `crates/formats`, `crates/engine` | done (wave 2 `b6-uml`, `b6-sequence`); editing UI in the frontend wave |
| Topology (engine) | `cloud`, `network` and `dfd` packs, nesting with owner sync, container looks, step badges, generated keys, Mermaid architecture-beta | `crates/notation`, `crates/diagram`, `crates/mermaid`, `crates/formats`, `crates/engine` | done (wave 2 `b6-topology`); Key group and badges on the canvas in the frontend wave |
| ER (engine) | `er` pack in IE, Chen, Barker and IDEF1X, new line ends and looks, owned columns, ER layout, DBML and Mermaid ER, `er.*` | `crates/diagram`, `crates/notation`, `crates/layout`, `crates/formats`, `crates/engine` | done (wave 2 `b6-er`); line looks on the canvas and column editing in the frontend wave |
| Engineering drawing (frontend) | Wire and Pipe tools on the symbol grid, the first bare-letter keys (R, X, Y, W, P), net numbers, markers, upright glyph text and table columns on the canvas, net highlight, cross-reference navigation and the zone in the status bar, an Engineering symbols (preview) palette with `glyph.insert`, the Inspector Symbol, Net, Sheet and Document groups, the Nets dock tab, the contextual Sheet tab with the Revise and Valve dialogs, sheet sizes in Page Setup; wires route around shapes unless drawn with bends | `crates/ui-egui`, small engine hunks | done (`b6e-frontend`, the 6E wave 1 frontend pulled forward) |
| Analysis (frontend) | Analyze and Compare groups on the Model tab, canvas highlights, the Analysis dock tab, Compare with ghosts of removed items | `crates/ui-egui`, `crates/engine` | done (wave 3 `b6-analyze-ui`) |
| Templates (frontend) | The New gallery with previews and the standards bar, Add Page from template, the engineering workspace preset, every command-line file in its own window | `crates/ui-egui`, `crates/engine`, `apps/vizcraft` | done (wave 3 `b6-gallery`) |
| Sequence editing (frontend) | Sequence pages edited on the canvas (reorder, columns, stem-to-stem messages, fragments and guards), the Sequence tab and Inspector group, labels above the line and fragment geometry clear of stems | `crates/ui-egui`, `crates/diagram`, `crates/layout`, `crates/engine` | done (wave 3 `b6-sequence-ui`) |
| Keyboard insertion and row editing (frontend) | Tab, Enter and `/` insertion, label and end-label editing, model-writing band edits, Model Tree relationship rows, Inspector ends, UML members and ER columns, notation-aware Re-Layout, the Steps tool and Key group | `crates/ui-egui`, `crates/engine`, `crates/notation` | done (wave 3 `b6-ux`) |
| Document types (frontend and engine) | Templates become empty document types per discipline; examples under Help > Examples; the app starts on the New page; the palette shows the chosen notation first and leaves the Basic primitives out of the pane | `crates/notation`, `crates/engine`, `crates/ui-egui`, `apps/vizcraft` | done (`b6-doctypes`) |
| Connector routing (engine) | Stand-off clearance as a page option, square ends, no shared runs, fan-out, label clearance, layouts that leave room for routes, a quality check over every template | `crates/diagram`, `crates/routing`, `crates/layout`, `crates/engine` | done (wave 3 `b6-routing`) |
| Drawing follow-ups | ER cells that fit with one header row, step badges clear of labels and on the canvas, names below actor and user figures (`textPlace`), Inter Italic in PNG and PDF, Fork and Join icons and flow-balance rules, ER line looks on the canvas, key and title block text inset | `crates/diagram`, `crates/formats`, `crates/notation`, `crates/ui-egui` | done (wave 3 `b6-polish`) |
| Electrical (wave 4) | `electrical` pack: IEC 60617, IEEE 315 and JIC symbols and panel footprints, IEC 81346, JIC and Y14.44 designations recomposed live, ten empty document types and ten examples, the Ladder tool with rung wire numbers, contact-to-coil links and coil mirrors, terminal strips, cables and cores, PLC I/O points, a 28-rule electrical check, wire, cable, terminal, I/O and nets lists, netlist export; `electrical.*`, `ladder.*`, `terminal.*`, `cable.*`, `io.assign`, `net.export` | `crates/notation`, `crates/stencils`, `crates/model`, `crates/engine`, `crates/formats`, `crates/ui-egui` | done (wave 4 `b6e-electrical`) |
| Process (wave 4) | `process` pack (Process Plant): DEXPI-named equipment, nozzles, piping, instruments, loops and the loop-diagram items, HVAC, plumbing and isometric types; ISO 10628 by default with ISA-5.1 and ISO 14617 one choice away; line numbers that follow line data live and survive inline valves, loops with parallel or serial numbering, loop trace and loop diagrams, off-page pairs across sheets, process lists (line, equipment, instrument, valve, I/O, stream, nozzle, fixture units, BOM) to CSV and XLSX, project tag formats (`Model.idFormats`), DEXPI class map; six empty document types and six examples; `loop.*`, `line.assign`, `offpage.*`, `plant.*`, `identity.*` | `crates/notation`, `crates/stencils`, `crates/model`, `crates/engine`, `crates/formats`, `crates/ui-egui` | done (wave 4 `b6e-process`) |
| Fluid power (wave 4) | `fluidpower` pack: ISO 1219 hydraulic and pneumatic components (30 types over ten ISO 1219-2 classes), working, pilot, drain, mechanical and signal lines, editions ISO 1219 2012 (default), 1995 and ANSI Y32.10 (`fluidpower.jic` alias), ISO 9461 and ISO 5599 port sets, component codes letter or numeric, a circuit model with 13 checks, pressure trace, displacement-step charts (VDI 3260), a one-step power unit; Hydraulic Circuit, Pneumatic Circuit and Displacement-Step Diagram document types with three examples; `fluid.*`, `ui.tool {line}` | `crates/notation`, `crates/stencils`, `crates/model`, `crates/engine`, `crates/ui-egui` | done (wave 4 `b6e-fluidpower`) |
| Mechanical, annotation, reliability and IDEF0 (wave 4) | `mechanical` pack (kinematic, transmission, bond graph and control block viewpoints, ISO 3952 symbols, 36 types, link, shaft, mesh, belt, chain, bond and signal kinds), `annotation` pack (feature control frames, datums, datum targets, welding and surface texture symbols in ISO or ASME), `reliability` pack (fault tree, RBD, event tree, FMEA worksheet), `idef0` pack (context and decomposition, the `idef0box` builder, boundary ends, tunnelled arrows); 10 document types and 11 examples; three line ends (harpoon, harpoon with bar, parentheses); six analyses (mobility, ratio, Mason, cut sets, availability, outcomes); `mech.*`, `rel.*`, `idef0.*`, `annotation.setStandard`; Mechanism, Reliability Analysis, FMEA and Function Model groups on the notation tab and an Annotate group on the Sheet tab | `crates/notation`, `crates/stencils`, `crates/diagram`, `crates/analysis`, `crates/engine`, `crates/formats`, `crates/ui-egui` | done (wave 4 `b6e-mechanical`) |
| Multi-selection transforms | `shape.transform` moves, scales and turns a set of shapes and connectors as one body in one undo step (bends carried, glued ends kept, symbols keep their size and turn in quarter turns, module snapping); the canvas draws one frame with resize and rotate handles around a multi-selection and previews the drag | `crates/diagram`, `crates/engine`, `crates/ui-egui` | done (owner request after wave 4) |
| Packs on the substrate | C4 and code forms (`b6-c4`, `b6-code`) | | planned (after wave 4) |

## Cascade 6: audience packs

Three tracks run fully in parallel, each a set of notation packs (data in `vizcraft-notation`
format), exporters and generators (model-layer code in `crates/formats` and `crates/codegen`,
tested with golden files) and the track's patterns from the catalogue. Every pack climbs the same
export ladder: tables and documentation, then open interchange formats, then code scaffolds.

### Track 6.1: embedded and systems

| Piece | Contents | Lands in | State |
|---|---|---|---|
| Statechart pack | Composite and orthogonal states, history, guards, entry, do and exit actions, events with payloads; live checks for unreachable states, dead ends and unknown events | pack data, `crates/notation` (rules) | planned |
| SCXML import and export | Round trip through the W3C format, stable ids | `crates/formats` | planned |
| Statechart code targets | C switch style (MISRA-friendly) or table style; Rust `statig` (hierarchical) or `smlang` (flat, rejecting or flattening on opt-in); QP-style C handlers; transition-coverage test sequences | `crates/codegen` | planned |
| Register map pack | Address map, register file, register, field (access, reset, side effects), enums; SVD import through `svd-parser`; C header (offsets, masks, optional struct overlay), SVD export (feeds svd2rust), WaveDrom bitfield JSON, Markdown register tables | pack data, `crates/formats`, `crates/codegen` | planned |
| Memory map pack | Address-ordered regions (flash, RAM, bootloader, A/B slots); overlap, alignment and overflow validation; GNU ld `MEMORY {}` block and C region constants | pack data, `crates/codegen` | planned |
| Packet and frame pack | Field rows on a 32-bit grid (offset, width, endianness, type) reusing the bitfield model; C packed struct and macros, Rust (de)serialiser, RFC-style ASCII, test vectors | pack data, `crates/codegen` | planned |
| Timing pack | Signal lanes, edges and markers; WaveJSON import and export; SVG | pack data, `crates/formats` | planned |
| Embedded patterns | Hierarchical statechart; boot sequence; bootloader A/B update; power states; watchdog supervision; ISR-to-task deferral; producer and consumer ring buffer; driver or comms stack layering; MooN voting and duty/standby (shared with Track 6.3) | `crates/patterns` | planned |

### Track 6.2: enterprise and software

| Piece | Contents | Lands in | State |
|---|---|---|---|
| C4 pack | Person, software system, container, component, deployment nodes; implied relationships; drill-down views; Structurizr DSL import and export; Mermaid C4 import and Mermaid export; Docker Compose and Kubernetes skeletons from deployment views | pack data, `crates/formats`, `crates/codegen`, `crates/mermaid` | planned |
| ER pack | Conceptual, logical and physical levels, entities and crow's foot relationships; SQL DDL export per dialect and DDL import (first reverse-engineering target) | pack data, `crates/formats` | planned |
| ER generators | DBML, migration diff between model versions, ORM entities (Diesel, SeaORM, SQLAlchemy, JPA) | `crates/codegen` | planned |
| UML pack | Class, component, deployment, sequence, activity, use case and state (state shares the statechart model); Mermaid class, sequence and state import into the model | pack data, `crates/formats`, `crates/mermaid` | planned |
| UML generators | Rust traits and structs, Java, C#, TypeScript and Python skeletons by generation gap; JSON Schema | `crates/codegen` | planned |
| BPMN pack | Pools, lanes, tasks, events, gateways, subprocesses; BPMN 2.0 XML import and export with DI and stable ids; engine extensions as an optional profile | pack data, `crates/formats` | planned |
| API and events | Services with interfaces, channels and messages linked to ER and class types; OpenAPI 3.1, AsyncAPI 3.0, protobuf with gRPC services, event JSON Schema | pack data, `crates/codegen` | planned |
| DFD and threat model | External entity, process, store, flow, trust boundary; STRIDE threat list (Markdown, CSV, JSON), pytm export | pack data, `crates/codegen` | planned |
| Enterprise patterns | Microservice topology, gateway/BFF, saga, transactional outbox, CQRS with event sourcing, pub/sub, strangler fig, hexagonal, medallion, CI/CD, multi-region deployment, linked to their scaffolds | `crates/patterns` | planned |

### Track 6.3: industrial automation

| Piece | Contents | Lands in | State |
|---|---|---|---|
| P&ID and PFD pack | DEXPI-named equipment, nozzles, piping segments, inline valves, instrumentation functions, signal lines; configurable ISA-5.1 tag formats and composed line numbers; paired off-page connectors; cross-sheet continuity check | pack data, `crates/notation` | planned |
| Industrial lists | Line, equipment, instrument and valve lists; instrument index; I/O list; ISA-18.2-shaped alarm list; as table views and CSV/XLSX | pack data, `crates/model` (queries), `crates/formats` | planned |
| Loops and I/O | Loops as model objects; loop diagrams as generated views per loop with location sections; I/O points (signal type, range, rack, slot, channel) | pack data, `crates/model` | planned |
| Cause-and-effect and logic | Cause-and-effect matrix view linked to tags; interlock and permissive logic trees; Structured Text Boolean interlocks per matrix row | pack data, `crates/codegen` | planned |
| SFC and control modules | SFC steps and transitions to ST `CASE` state machines; motor and valve modules (states, feedbacks, interlocks, hand/off/auto) to ST function block skeletons; global variable list from the I/O list; Structured Text only, never Instruction List | pack data, `crates/codegen` | planned |
| OT architecture | Purdue bands as a starting draft, IEC 62443 zones (SL-T) and conduits, assets; zone, conduit and asset inventory CSV | pack data, `crates/formats` | planned |
| Industrial patterns | Single PID loop, cascade, ratio, feedforward plus feedback, split range, selector/override, interlock, permissive, cause-and-effect matrix, SFC start-up or shutdown, motor or valve module, MooN voting, duty/standby, Purdue reference, IEC 62443 zones and conduits | `crates/patterns` | planned |

Done when: in each track the first deep pack (statechart, C4 and ER, P&ID) builds a valid example
from its template, raises its live issues, and exports through its ladder; generated statechart C
and Rust, DDL and Structured Text regenerate byte-identically and re-import where an importer exists.

In parallel: the three tracks run fully in parallel, and each piece inside a track is its own
branch once its pack's schema is written.

## Cascade 7: second-wave interop and analysis

| Piece | Contents | Lands in | State |
|---|---|---|---|
| PLCopen XML | IEC 61131-10 POUs and SFC export from the industrial packs | `crates/codegen` | planned |
| OPC UA NodeSet2 | Tag hierarchy export | `crates/codegen` | planned |
| DEXPI Proteus XML | P&ID export; DEXPI 2.0 once final | `crates/formats` | planned |
| AutomationML CAEX | OT architecture and hierarchy export | `crates/formats` | planned |
| ISA-88/95 pack | Physical model (cell, unit, equipment module, control module) and procedural model (procedure to phase) edited as an outline with generated tree layout; equipment hierarchy CSV; B2MML and BatchML; ISA-88 hierarchy and phase state machine patterns | pack data, `crates/formats`, `crates/patterns` | planned |
| Electrical pack | IEC 81346 three-aspect reference designations, contacts and coils with cross-reference images, terminals, wires, potentials; terminal, wire and cable lists, BOM | pack data, `crates/notation`, `crates/formats` | planned |
| ArchiMate pack | ArchiMate 4 pack once the specification is read; ArchiMate 3.2 Open Exchange Format import; Open Exchange export, HTML report | pack data, `crates/formats` | planned |
| Text notation interop | PlantUML, D2 and DOT import and export; DOT import with collapse and filter commands from day one | `crates/formats` | planned |
| Reverse engineering | Import from OpenAPI, AsyncAPI, Kubernetes and Compose manifests, and Terraform under the re-import contract | `crates/formats` | planned |
| Register and memory interop | SystemRDL and IP-XACT register import; GNU ld map import | `crates/formats` | planned |
| Second-wave embedded packs | Component and port architecture (AUTOSAR port kinds, typed buses, interface headers, port compatibility validation); RTOS and ROS 2 graphs (tasks, ISRs, queues, nodes, topics; task tables, ROS 2 package skeleton); CAN and protocol sequences (DBC export, DBC import later, per-lifeline protocol skeletons) | pack data, `crates/codegen` | planned |
| Second-wave enterprise packs | DDD and event storming (bounded contexts, context-map patterns, repository-per-context skeletons, CML import later); capability map and Team Topologies (heat-mapped tiles, CSV import, catalogue tables) | pack data, `crates/codegen`, `crates/patterns` | planned |
| Statechart simulator | Highlights active states, validated against the SCXML conformance corpus | `crates/model`, `crates/ui-egui` | planned |
| Model analysis | Impact analysis, relationship matrix, baselines and model diff | `crates/model`, `crates/ui-egui` | planned |
| HTML documentation site | Static site of the model, its views and tables | `crates/formats` | planned |

Done when: a P&ID exports Proteus XML and its control logic exports PLCopen XML that a public
schema validates, a statechart runs in the simulator against the SCXML corpus, and a model diff
between two baselines lists added, changed and removed elements.

In parallel: each format and pack is its own piece; all run in parallel.

## Cascade 8: General pack and broad interop

General drawing stays available, as a pack, but is no longer the identity of the app.

| Piece | Contents | Lands in | State |
|---|---|---|---|
| General pack | Flowchart, swimlane and mind map in one pack built on the 4A Basic and Flowchart packs; cross-functional flowchart template on Cascade 2 swimlanes | pack data, `crates/stencils` | planned |
| Network | Generic network device shapes drawn originally; no vendor brand or cloud icons | pack data, `crates/stencils` | planned |
| Org chart | Org chart shapes and a template built from a CSV or parent-id table, synced both ways | pack data, `crates/formats`, `crates/patterns` | planned |
| Timeline | Timeline shapes and template on the timeline layout primitive | pack data, `crates/stencils` | planned |
| General patterns | Approval chain, RACI to swimlanes, SIPOC, value stream map, Kanban, stage-gate | `crates/patterns` | planned |
| Data graphics | Callouts, icon sets and colour by value driven by element properties and shape data | `crates/diagram`, `crates/engine/src/cmd/data.rs` | planned |
| Comments and hyperlinks | Comments on shapes, elements and pages; hyperlinks between pages, shapes, elements and URLs | `crates/diagram`, `crates/model`, `crates/engine` | planned |
| .vsdx import | Import from the public MS-VSDX specification (OPC zip and XML) | `crates/formats` | planned |
| Runtime-loaded packs | Third-party notation packs loaded from files at run time (replaces Plugin stencils) | `crates/notation` | planned |
| Print | Page tiling and printing | `crates/formats`, `apps/vizcraft` | done early (Block 2B `b2-output`) |

Done when: each General diagram type has a template that opens to a valid example, a sample .vsdx
built from the public specification imports, and a pack loaded from a file at run time appears in
the palette and validator.

In parallel: every piece is independent and can start any time after Cascade 4A.

## Future

Not scheduled, listed so they are not forgotten:

| Item | Notes |
|---|---|
| Web app | `apps/vizcraft-web` as a full diagram editor in the browser (the shell and the `wasm` gate exist) |
| Collaboration | Shared editing of one document |

## Trademarks

Microsoft and Visio are trademarks of the Microsoft group of companies; draw.io is a trademark of
JGraph Ltd; UML and BPMN of Object Management Group, Inc.; ArchiMate of The Open Group; OPC UA of
the OPC Foundation; AUTOSAR of the AUTOSAR development partnership; Docker of Docker, Inc.;
Terraform of HashiCorp, Inc.; Team Topologies of Team Topologies Ltd. The names are used only to
identify products and formats; VizCraft is not affiliated with or endorsed by their owners.
