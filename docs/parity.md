# Feature parity catalog

This catalog lists the features VizCraft aims for, grouped by area, with the cascade in
[ROADMAP.md](../ROADMAP.md) that schedules each and its state. It covers Visio-class editing
and the engineering-workbench features (semantic model, notation packs, validation and
identity, workspace, patterns, code-scaffold generation and the three audience packs) from
[docs/research/professional-features.md](research/professional-features.md). It names features
only: this is a clean-room project, so descriptions are our own words and never another product's
wording (see [AGENTS.md](../AGENTS.md)). The MCP tools that mirror draw.io's MCP server are listed
under Agent integration.

When a feature lands, change its State in the same commit as the code. States:

| State | Meaning |
|---|---|
| `planned` | Scheduled in a cascade, not built |
| `partial` | Some of it works; Notes say what is missing |
| `done` | Built, tested and reachable by command id |
| `done (chrome)` | Provided by the application chrome that exists today |

## Application chrome

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Undo and redo | 0 | done (chrome) | Engine undo stacks; diagram commands plug into them |
| Zoom | 0 | done (chrome) | `view.zoom`, `view.zoomIn`, `view.zoomOut` |
| Fit to Window | 2 | done | `view.zoomFit {page?}` (Mod+Shift+H) fits the page or the drawing into the canvas; the app fits on its own after opening a file or a replacing import that the canvas cuts off |
| Pan | 0 | done (chrome) | `view.pan`, `view.resetPan`; middle-drag, Cmd+drag (Ctrl off macOS) or Space+drag |
| Multiple windows | 0 | done (chrome) | One process, one window per document |
| Side panes | 0 | done (chrome) | Left and right panes, resizable and reorderable; diagram panes are added per cascade |
| Ribbon | 0 | done (chrome) | Tabs, groups, contextual tabs; diagram tabs are added per cascade |
| Dropdowns show their current choice | 2C | done | Every ribbon menu, combo list and gallery with a current value marks it with a right-aligned check mark; `ui.inspect` lists the open menu's rows with `checked` |
| Command palette | 0 | done (chrome) | Search over every command id and label |
| Control channel | 0 | done (chrome) | JSON lines over localhost; see [docs/control-protocol.md](control-protocol.md) |

## Shapes and stencils

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Basic shapes | 1 | done | Rectangle, rounded rectangle, ellipse, diamond, triangle, parallelogram, hexagon, cylinder, document, cloud; `shape.add {stencil}` |
| Select, move, resize, rotate | 1 | done | Canvas handles (Shift keeps aspect, snaps rotation to 15 degrees), arrow-key nudge, and `shape.*` commands |
| Multi-select and marquee | 1 | done | Shift-click, marquee, `edit.select {ids, add?}`, `edit.selectAll` |
| Shapes pane | 1 | done | Basic and Flowchart sets with drawn previews; click or drag to insert |
| Basic and Flowchart stencils | 1 | done | Original shapes defined in code (`vizcraft-stencils`), inserted with `shape.add` |
| Connection points | 1 | done | Four per built-in shape; shown on hover, connectors drag from and glue to them |
| Stencil search | 3 | done | Search field in the Shapes pane ranks masters by exact name, word prefix, keyword and substring; `ui.shapesSearch` sets it by id |
| Custom stencils | Future | planned | Plugin stencil sets loaded from files |

## Connectors and glue

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Straight, orthogonal and curved connectors | 1 | done | `connector.add`, `connector.setRoute` |
| Dynamic glue | 1 | done | `connector.glue` naming only a shape; routing is derived from shape geometry |
| Point glue | 1 | done | `connector.glue` with a connection point index |
| Arrowheads | 1 | done (13 ends in 2C) | `connector.setArrows`: none, triangle, open, diamond, circle, hollow triangle, hollow diamond, bar, open circle, crow's foot, crow's foot with bar, crow's foot with circle, bar with circle; one geometry table (`diagram::arrows`) draws them on the canvas, in SVG and in PDF |
| Connector kinds | 2C | done | Named kinds declared by notation packs (`connectorTypes`: route, ends, dash, width, colour, jumps, optional relationship type); `connector.add {kind}`, `connector.setKind`, `connector.kinds`; a kind that names a relationship type wins over the connection table when binding; ribbon gallery in wave 2 |
| Connector labels | 1 | done | `connector.setText` |
| Waypoints | 2 | partly done (3) | Stored bends on connectors, set by routing and read from .drawio; dragging bends by hand is planned |
| Line jumps | 2 | done (3) | Hops where connectors cross; `connector.setJumps`, Design tab |
| Obstacle-avoiding rerouting | 2 | done (3) | Orthogonal routes around shapes, kept clear after every change; `layout.route`, `connector.reroute` |

## Text

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Text in shapes | 1 | done | Double-click or the Text tool edits in place; `shape.setText`, `connector.setText` |
| Font, size, colour, bold, italic | 1 | done (B2 extends) | Home > Font with family, size steps, underline, strikethrough, case, highlight; `text.setFormat`, `text.grow`, `text.setCase`; families other than monospace draw in Inter on the canvas |
| Alignment and wrapping in the shape | 2 | done (B2) | Home > Paragraph: alignment, justify, bullets, indent, direction, margins; `text.setParagraph`; vertical text not drawn in export yet |
| Text blocks | 2 | done (B2) | Text Block tool and `text.setBlock`; text boxes on empty page (`shape.addTextBox`); the editor rotates with the shape |
| Fields and symbols | 2 | done (B2) | `{{date}}`, `{{pageName}}`, `{{data:key}}` and others in shape text (`text.insertField`); Insert Symbol dialog (`text.insertSymbol`); header and footer use `{page}`-style fields (two syntaxes for now) |

## Pages

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Multiple pages and page tabs | 2 | done (B2) | Active page edited on the canvas; page tab strip; `page.add`, `page.delete`, `page.duplicate`, `page.rename`, `page.move`, `page.setActive`, with `page.list`, `page.get`, `page.set` |
| Background pages | 2 | done (B2) | `page.add {background}`, `page.setBackground`; drawn beneath the page on the canvas and in export, not selectable |
| Page setup | 2 | done (B2) | `page.setup` (size, orientation, units, drawing scale, auto-size, board); Page Setup dialog with a Board row; Design > Page Setup group |
| Infinite board | 2C | done | `Page.board` is infinite for new pages and fixed for older files; `Page::frame()` (the drawing bounds plus a margin, or the sheet) is what the canvas, zoom to fit, rulers, exports, print and draw.io write use; no sheet, shadow or page breaks on an infinite board; `page.setup {board}`; ribbon Board menu in wave 2 |
| Auto-size page to content | 2 | done (B2) | Page grows right and down after any change past the edge; `page.fitToDrawing {margin?}` |

## Layers

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Layers with visible, lock and print flags | 2 | done (B2) | `layer.add`, `layer.delete`, `layer.set` (visible, locked, print, snap, glue, colour), `layer.assign`, `layer.unassign`; hidden shapes are not drawn or hit, locked ones refuse edits; print skips are recorded for the print command |
| Layers pane | 2 | done (B2) | Add, rename, toggle flags, assign the selection; Home > Editing > Layers menu |
| Layer colour | 2 | done (B2) | Line and text take the layer colour, fill moves halfway to it |

## Arrange and layout

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Align | 2 | done (B2) | `arrange.align {edge, to?}` to the first shape or the page |
| Distribute | 2 | done (B2) | `arrange.distribute {axis}`; `arrange.autoSpace {gap?}` |
| Order | 2 | done (B2) | `arrange.order {how}` |
| Group and ungroup | 2 | done (B2) | `arrange.group`, `arrange.ungroup`; nested groups |
| Rotate and flip | 2 | partly done (B2) | `arrange.rotate`, `arrange.flip`; flip mirrors positions and angles but not uneven outlines (no flip field yet) |
| Size and position | 2 | done (B2) | `arrange.setBounds {id, x?, y?, w?, h?, angle?}`; a group scales its members |
| Auto-layout | 3 | done | Layered, tree, radial, force; `layout.full` (page or selection), `layout.incremental` (existing shapes pinned), `layout.route` |

## Containers, swimlanes and groups

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Groups | 2 | done (B2) | Click selects the outermost group, next click one level down; move, resize, rotate, delete and copy carry members |
| Containers | 2 | done (B2) | `container.*`: drop in and drag out to change membership, auto-fit, margins, heading position and style, lock, select contents, disband; Container tab; drawn on canvas and in export |
| Swimlanes | 2 | done (B2) | `lane.*`: lane sets of lanes, phases, reorder, flip orientation; cross-functional flowchart |
| Callouts | 2 | done (B2) | `callout.add`, `callout.set`: leader follows the target; Callout tab |
| Pools and lanes | 6 | planned | BPMN variant of swimlanes |

## Themes and styles

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Themes | 2 | done (B2) | Six of our own with four variants each; `design.setTheme`, `design.setVariant`; hand styling stays |
| Quick styles | 2 | done (B2) | Twelve presets from the theme; `design.applyQuickStyle` |
| Fill, line, shadow, gradient | 2 | done (B2) | Dash, caps, rounding, gradients, transparency, shadow, soft edges, glow via `shape.setStyle`; connectors draw dash, caps and transparency on the canvas too |
| Format painter | 2 | done (B2) | `edit.formatPainter {from?, to?}` and a one-shot tool (double-click keeps it armed) |
| Duplicate and Paste Special | 2 | done (B2) | `edit.duplicate` (Mod+D), `edit.pasteSpecial {as: shapes\|text\|style}` |
| Change Shape | 2 | done (B2) | `edit.changeShape {stencil}` keeps text, data, layers, glue and hand formatting |
| Select by type, lasso and area select | 2 | done (B2) | `edit.selectByType`; `ui.tool` `lasso` and `area` |
| Backgrounds and borders and titles | 2 | done (B2) | `design.setBackground`, `design.setBorderTitle` on a generated background page |
| Page-wide connector style | 2 | done (B2) | `design.setConnectorStyle {style}` |
| Page breaks, connection points and AutoConnect toggles | 2 | partly done (B2) | `view.pageBreaks` (Letter paper assumed), `view.connectionPoints`, `view.autoConnect` (stored only, no canvas behaviour yet) |
| Light and dark app theme | 0 | done (chrome) | Window chrome only; diagram colours follow the document theme |

## Snap, grid, rulers and guides

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Grid | 2 | done (B2) | `view.grid` |
| Snap to grid, shapes and points | 2 | partly done (B2) | `view.snap`, Snap and Glue dialog; move drags only (not resize or connector ends); Alt turns it off |
| Rulers | 2 | done (B2) | `view.rulers`; shows the selection extent |
| Guides | 2 | done (B2) | Dragged from the rulers; `guide.add`, `guide.move`, `guide.delete` |
| Dynamic alignment guides | 2 | done (B2) | Alignment lines while dragging |
| Pan and Zoom pane | 2 | planned | Page overview with the visible area as a frame |

## Data

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Shape data | 2 | done (B2) | Typed rows on shapes (string, number, bool, date, enum, url; unit, required, default); page data keys shapes opt in to (stored in page `ext` for now); element properties shown read-only; `data.*`; Shape Data pane, Define Shape Data dialog, Data tab Show/Hide |
| Data graphics | 8 | planned | Callouts, icon sets, colour by value |
| Import from CSV | 3 | done | draw.io-style CSV; org charts from CSV in Cascade 4 |

## Diagram types and templates

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Flowchart | 1 | done | Flowchart stencil set in the Shapes pane and Insert tab |
| Cross-functional flowchart | 8 | planned | Template on swimlanes from Cascade 2 |
| Org chart | 8 | planned | Stencils and a CSV-driven template |
| UML | 6 | planned | Class, sequence, state, activity, use case |
| BPMN | 6 | planned | Events, activities, gateways, pools and lanes |
| ER | 6 | planned | Entities and crow's foot relationships |
| Network | 8 | planned | Generic devices drawn originally; no vendor brand icons |
| Timeline | 8 | planned | Timeline shapes and template |

## Review

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Comments | 8 | planned | On shapes and pages |
| Hyperlinks | 8 | planned | To pages, shapes and URLs |
| Validation | 4A | partly done (4A) | Rule sets declared by notation packs, live and batch, with structured issues (`validate.*`); the issues pane is Cascade 4B |
| Find and replace | 2 | done (B2) | `edit.find`, `edit.replace` over text, typed shape data rows and format data, page or document, match case, whole word; Find dialog (Mod+F); replacements in typed rows must fit the row's type |

## Semantic model and notation packs

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Semantic model | 4A | done | `vizcraft-model`: elements, relationships and a containment tree, with stable ids and pack-qualified types |
| Views over the model | 4A | done | One element shown in many views; geometry lives in the view, keyed by element id |
| Shape-to-element references | 4A | done | Optional `Shape.element` and `Connector.relationship`; shapes without one stay free drawing |
| Delete from view or from model | 4A | done | Removing a shape from a view keeps the element unless asked |
| Model section in `.vizcraft` | 4A | done | Versioned; older files load as free drawing |
| Notation pack format | 4A | done | `vizcraft-notation`: versioned JSON or TOML packs with element types, property schemas, ports, glyphs, palette groups, viewpoints, identifier formats and rules |
| Connection table | 4A | done | Legal source type, relationship type and target type, with multiplicity |
| Conditional styles | 4A | done | Styles keyed on property values, declared in the pack; applied when the property changes, hand styling otherwise stays |
| Viewpoints | 4A | done | Named filters of allowed types per view |
| Basic and Flowchart packs | 4A | done | The existing sets re-expressed as the first two packs |
| Rules engine, live | 4A | done | Cheap rules after every command |
| Rules engine, batch | 4A | done | Expensive rules on selection, page or project |
| Structured issues | 4A | done | Rule id, severity, element ids, message; returned with every MCP command batch |
| Notation schema discovery over MCP | 4A | done | Agents list packs, types, properties and legal relationships |
| Implied relationships | 6 | planned | Parent-level links derived from child links (C4 first) |
| Impact analysis, relationship matrix, baselines and model diff | 7 | planned | Analysis over the model |
| Runtime-loaded packs | 8 | planned | Third-party packs loaded from files at run time |

## Workspace and insertion

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Model tree pane | 4B | planned | Default left tab; containment tree with views per element and a Diagrams branch |
| Model tree Context mode | 4B | planned | Follows the canvas selection |
| Scoped Palette pane | 4B | planned | Replaces the full-height tile grid; filtered by the active notation and viewpoint |
| Patterns tab | 5A | planned | Pattern library filtered by audience and notation, with parameter forms |
| Typed Inspector | 4B | planned | Grows the Shape Data pane; Standard and Expert modes; relationships list; appears in N views; Format tab |
| Problems tab | 4B | planned | Validation issues; click to navigate |
| Tables tab | 5B | planned | Model-derived lists and matrices, edited in place |
| Search results tab | 4B | planned | Bottom dock |
| Generate tab | 5B | planned | Scaffold preview diff and run log |
| Agent activity view | 4B | planned | Each agent batch as one undoable transaction |
| Hover suggest-and-connect | 4B | planned | Clone and connect, suggesting only legal next element types |
| Double-click insert picker | 4B | planned | Type to insert on empty canvas |
| Keyboard chaining | 4B | planned | Alt+Shift+Arrow adds a connected element |
| Magic connector | 4B | planned | Offers only legal relationship types |
| Command palette over the model | 4B | planned | Cmd/Ctrl+K over commands, element types, patterns and elements by name or tag |
| Notation ribbon tabs | 4B | planned | Contextual tab per active notation, plus Model, Validate and Generate tabs |
| New from notation | 4B | planned | Backstage grouped by audience |
| Drill-down breadcrumbs | 4B | planned | Navigation between parent and child views |

## Validation and identity

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Identity registry | 4A | done | Project-wide tags and reference designations |
| Auto-numbering | 4A | done | Loop numbers, line numbers and designations handed out by the registry |
| Uniqueness checks | 4A | partly done (4A) | Duplicate tags raise a live issue (built); the Inspector check as you type is Cascade 4B |
| Composed identifiers | 4A | done | Identifier formats declared by the pack (for example `{letters}-{n:3}`), composed and auto-numbered by the registry |
| ISA-5.1 tags and composed line numbers | 6 | planned | Configurable formats in the P&ID pack, not hardcoded letter tables |
| IEC 81346 reference designations | 7 | planned | Electrical pack |
| Off-page connector pairing | 4A | done | Paired objects, matched by identifier |
| Unmatched off-page detection | 4A | done | An off-page connector with no continuation raises an issue |
| Cross-reference display | 7 | planned | Live cross-references between paired items in the electrical pack |
| Flowchart rules | 4A | done | Unconnected connectors and dangling decisions, declared in the Flowchart pack |
| Statechart checks | 6 | planned | Unreachable states, dead ends, unknown events |
| Memory map checks | 6 | planned | Overlap, alignment, overflow |
| Cross-sheet continuity | 6 | planned | P&ID batch rule |
| Port compatibility | 7 | planned | Component and port architecture pack |

## Patterns

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Pattern framework | 5A | planned | `vizcraft-patterns`: typed-parameter generators that emit model elements in one undo step |
| `pattern.insert` | 5A | planned | One command with a discoverable parameter schema, also over MCP |
| `pattern.reparameterize` | 5A | planned | Change parameters later, keeping manual label edits |
| Pattern provenance | 5A | planned | The inserted group records pattern id and parameter values |
| Table-driven pattern sync | 5B | planned | Both ways where unambiguous: RACI to swimlanes, parent-id table to org chart, cause-and-effect table to matrix |
| Layout primitives | 5A | planned | Pipeline with parallel band, two-row chain, hub and spoke, bands and swimlanes, matrix, nested containers, concentric rings, timeline, N-channel voter |
| Single PID loop | 6 | planned | Tag, loop number, controller type; layered with feedback return; ST FB call and I/O entries |
| Cascade, ratio, feedforward plus feedback, split range, selector/override | 6 | planned | Industrial P&ID; layered with nested inner loop; ST skeleton per loop |
| Interlock, permissive, cause-and-effect matrix | 6 | planned | Industrial logic; logic tree or matrix; ST Boolean logic per row |
| SFC start-up or shutdown, ISA-88 phase state machine | 6, 7 | planned | SFC in Cascade 6, ISA-88 phase machine with the ISA-88/95 pack in 7; ST CASE machine, PLCopen SFC |
| Motor or valve control module | 6 | planned | Containers; ST FB skeleton |
| MooN voting, duty/standby | 6 | planned | Shared by industrial and embedded; N-channel voter; voter logic stub |
| Purdue reference, IEC 62443 zones and conduits | 6 | planned | Horizontal bands and containers; asset and conduit inventory |
| ISA-88 physical or procedural hierarchy | 7 | planned | Tree from a table; CAEX or B2MML |
| Hierarchical statechart | 6 | planned | Layered inside nested containers; C or Rust state machine |
| Boot sequence, bootloader A/B update, power states, watchdog supervision | 6 | planned | Layered or hub; state machine plus linker regions |
| ISR-to-task deferral, producer/consumer ring buffer | 6 | planned | Swimlanes by execution context or pipeline; task table and queue declarations |
| Driver or comms stack layering | 6 | planned | Horizontal bands; interface headers |
| Microservice topology, gateway/BFF | 6 | planned | Layered or hub; Compose or Kubernetes skeleton |
| Saga, transactional outbox | 6 | planned | Two-row chain or hub; handler and compensation stubs |
| CQRS plus event sourcing, pub/sub | 6 | planned | Two-path layered or hub; AsyncAPI and event schemas |
| Strangler fig, hexagonal | 6 | planned | Layered or concentric rings; port traits and adapter stubs |
| Medallion, CI/CD | 6 | planned | Pipeline; pipeline config skeleton |
| Multi-region deployment | 6 | planned | Nested containers; Terraform module skeleton |
| Workflow pattern fragments | 5A | planned | Parallel split, exclusive choice, N-of-M join, multiple instances, cancel region; BPMN XML once the BPMN pack lands in 6 |
| Approval chain, RACI to swimlanes, SIPOC, value stream map, Kanban, stage-gate | 8 | planned | General pack; swimlanes, matrix or timeline; BPMN XML or CSV |
| Org chart from table, capability map, Team Topologies, context map | 7, 8 | planned | Org chart in the General pack (8); capability, Team Topologies and DDD packs in 7; catalogue CSV |

## Code-scaffold generation

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Generation as commands | 5B | planned | `generate.preview`, `generate.run`, `generate.check`; Generate tab, CLI and MCP |
| CI check mode | 5B | planned | `vizcraft-cli generate --check` fails when output is stale |
| Generator spec file | 5B | planned | Target, options, output directory and model scope, versioned in the repository |
| Ownership manifest | 5B | planned | Every generated file with its last checksum; writes only owned files in the declared directory |
| Generation gap by default | 5B | planned | Rust: generated module plus a user-implemented trait; C: generated `.c` calling `extern` prototypes |
| Protected regions keyed by element id | 5B | planned | Only for single-file targets such as a PLC POU |
| Traceability markers | 5B | planned | `vizcraft:id=<element id>` on every generated construct; rename map |
| Deterministic output | 5B | planned | Regenerating an unchanged model is byte-identical |
| No silent overwrite | 5B | planned | Preview diff; three-way merge or stop with an issue when an owned file changed |
| Construct-support matrices | 5B | planned | Unsupported constructs raise issues, or are flattened on opt-in |
| Standard intermediates | 6, 7 | planned | SVD, DBC, OpenAPI, AsyncAPI, protobuf, PLCopen XML rather than runtimes |
| Fidelity tests by re-import | 6 | planned | DDL, SCXML, SVD and OpenAPI output re-imported in tests and compared with the model |
| Re-import contract | 5B | planned | Model ids kept; geometry kept for existing ids; new elements placed with `layout.incremental`; removed ones flagged |
| Table exports | 5B | planned | CSV, XLSX, Markdown |
| Embedded targets | 6, 7 | planned | Statechart C and Rust, C headers, SVD, ld `MEMORY`, packet code, WaveJSON (6); interface headers, task tables, ROS 2 skeletons, DBC (7) |
| Enterprise targets | 6, 7 | planned | Structurizr DSL, DDL, DBML, migrations, ORM entities, class skeletons, BPMN XML, OpenAPI, AsyncAPI, protobuf, STRIDE lists (6); PlantUML, D2, DOT, ArchiMate exchange, HTML site (7) |
| Industrial targets | 6, 7 | planned | ST interlocks, ST CASE machines, FB skeletons, global variable lists, industrial lists (6); PLCopen XML, NodeSet2, Proteus XML, CAEX, B2MML (7) |

## Industrial automation pack

| Feature | Cascade | State | Notes |
|---|---|---|---|
| P&ID and PFD | 6 | planned | DEXPI-named equipment, nozzles, piping segments, inline valves, instrumentation functions, signal lines |
| Paired off-page connectors | 6 | planned | On the 4A identity registry |
| Line, equipment, instrument and valve lists | 6 | planned | Table views; CSV and XLSX |
| Instrument index and I/O list | 6 | planned | I/O points with signal type, range, rack, slot, channel |
| Alarm list | 6 | planned | ISA-18.2-shaped |
| Loops and loop diagrams | 6 | planned | Loops as model objects; loop diagrams generated per loop |
| Cause-and-effect matrix | 6 | planned | Matrix view linked to tags; ST Boolean interlocks per row |
| Interlock and permissive logic trees | 6 | planned | |
| SFC | 6 | planned | Steps and transitions; ST `CASE` state machines |
| Control modules | 6 | planned | Motor and valve modules with states, feedbacks, interlocks, hand/off/auto; ST function block skeletons; global variable list from the I/O list |
| Purdue bands and IEC 62443 zones and conduits | 6 | planned | Purdue as a starting draft; zones with SL-T; asset and conduit inventory CSV |
| PLCopen XML | 7 | planned | IEC 61131-10 POUs and SFC |
| OPC UA NodeSet2 | 7 | planned | Tag hierarchy |
| DEXPI Proteus XML | 7 | planned | DEXPI 2.0 once final |
| AutomationML CAEX | 7 | planned | |
| ISA-88/95 hierarchy | 7 | planned | Physical and procedural models as an outline; equipment hierarchy CSV; B2MML and BatchML |
| Electrical | 7 | planned | IEC 81346 designations, contacts and coils with cross-references, terminals, wires, potentials; terminal, wire and cable lists, BOM |
| MTP | Future | planned | Waits for the standard to settle |

## Embedded and systems pack

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Statechart | 6 | planned | Composite and orthogonal states, history, guards, entry, do and exit, events with payloads |
| SCXML import and export | 6 | planned | |
| Statechart to C | 6 | planned | Switch style or table style; QP-style handlers |
| Statechart to Rust | 6 | planned | `statig` (hierarchical) or `smlang` (flat) |
| Transition-coverage test sequences | 6 | planned | |
| Statechart simulator | 7 | planned | Validated against the SCXML conformance corpus |
| Register map | 6 | planned | Address map, register file, register, field, enums |
| SVD import and export | 6 | planned | Import through `svd-parser`; export feeds svd2rust |
| C register header | 6 | planned | Offsets, masks, optional struct overlay |
| WaveDrom bitfield JSON and Markdown register tables | 6 | planned | |
| SystemRDL and IP-XACT import | 7 | planned | |
| Memory map | 6 | planned | Regions; GNU ld `MEMORY {}` block and C constants |
| GNU ld map import | 7 | planned | |
| Packet and frame layouts | 6 | planned | 32-bit grid; C packed struct, Rust (de)serialiser, RFC-style ASCII, test vectors |
| Timing diagrams | 6 | planned | WaveJSON import and export; SVG |
| Component and port architecture | 7 | planned | AUTOSAR port kinds; DOT import; interface headers |
| RTOS and ROS 2 graphs | 7 | planned | Task tables (FreeRTOS, Zephyr, RTIC); ROS 2 package skeleton with launch file |
| CAN and protocol sequences | 7 | planned | DBC export (DBC import later); per-lifeline protocol skeletons |

## Enterprise and software pack

| Feature | Cascade | State | Notes |
|---|---|---|---|
| C4 | 6 | planned | Person, system, container, component, deployment; drill-down |
| Structurizr DSL import and export | 6 | planned | |
| Mermaid C4 import and Mermaid export | 6 | planned | |
| Compose and Kubernetes skeletons | 6 | planned | From deployment views |
| ER levels and DDL | 6 | planned | Conceptual, logical, physical; SQL DDL per dialect; DDL import |
| DBML, migration diff and ORM entities | 6 | planned | Diesel, SeaORM, SQLAlchemy, JPA |
| UML code skeletons | 6 | planned | Rust, Java, C#, TypeScript, Python by generation gap; JSON Schema |
| Mermaid class, sequence and state into the model | 6 | planned | Over the existing `vizcraft-mermaid` parser |
| BPMN 2.0 XML with DI | 6 | planned | Stable ids; engine extensions as an optional profile; DMN later |
| OpenAPI 3.1 and AsyncAPI 3.0 export | 6 | planned | Plus protobuf with gRPC services and event JSON Schema |
| DFD and threat model | 6 | planned | STRIDE threat list, pytm export |
| PlantUML, D2 and DOT import and export | 7 | planned | |
| ArchiMate 4 | 7 | planned | Once the specification is read; 3.2 Open Exchange import; HTML report |
| Reverse engineering | 7 | planned | OpenAPI, AsyncAPI, Kubernetes and Compose, Terraform |
| DDD and event storming | 7 | planned | Bounded contexts, context-map patterns, repository-per-context skeletons |
| Capability map and Team Topologies | 7 | planned | Heat-mapped tiles; CSV import |
| HTML documentation site | 7 | planned | |

## Import and export

| Feature | Cascade | State | Notes |
|---|---|---|---|
| `.vizcraft` native format | 1 | done | Versioned JSON; see [docs/formats.md](formats.md) |
| SVG export | 1 | done | First page, through `file.export {path, format?}` |
| PNG export | 2 | done (B2) | `file.export {format: "png", page?, pages?, scale?}`; several pages write name-1.png, name-2.png and so on |
| PDF export | 2 | done (B2) | Vector, one PDF page per page (foreground pages by default), embedded Inter subsets; `file.export {format: "pdf", pages?}`; Save As PDF |
| Print | 2 | done (B2) | `file.print {pages?, fit?, tiles?, paper?, path?, send?}`: print-ready PDF without non-printing shapes, tiled or fitted; Print Preview (`ui.print`, Mod+P); sending to the printer is opt-in (the Print button sets it) |
| Headers and footers | 2 | done (B2) | `page.setHeaderFooter`; fields `{page}`, `{pages}`, `{pageName}`, `{title}`, `{date}` (UTC) |
| .drawio read and write | 3 | done | Multi-page, compressed pages; groups, containers and layers flattened for now |
| Mermaid import | 3 | done | Flowchart, sequence, class, state, ER |
| CSV import | 3 | done | draw.io-style CSV |
| .vsdx import | 8 | planned | From the public MS-VSDX specification |
| Export and import command slots | 0 | done | `file.export` writes SVG and .drawio; `file.import` reads .drawio, Mermaid and CSV |

## Agent integration

| Feature | Cascade | State | Notes |
|---|---|---|---|
| `command_list` | 1 | done | Every command id, label and params doc |
| `command_run` | 1 | done | Run one command by id |
| `command_batch` | 1 | done | Run several commands in order |
| `doc_inspect` | 1 | done | Document and UI state as JSON; `page?` adds one page's summary (Cascade 3); `detail?` replaces `pages` with the described pages (2C) |
| `describe` and detail levels | 2C | done | `doc.describe`, `page.describe`, `shape.describe` with `detail` (`structure`, `layout`, `full`) and `format` (`json`, `text`); the `describe` tool, `command_list {brief}`, and page and shape resources that default to structure with `?detail=`; the sample document's structure is about 1 KB of JSON or 400 bytes of text |
| `screenshot` | 1 | done | PNG of the window, or of one page rendered offscreen with `page?`, `window?`, `scale?` (Cascade 3) |
| `open_xml` | 3 | done | Load a diagram from .drawio XML |
| `open_mermaid` | 3 | done | Load a diagram from Mermaid text |
| `open_csv` | 3 | done | Load a diagram from CSV |
| `search_shapes` | 3 | done | Keyword search over stencils |
| `get_reference` | 3 | done | Format and style reference for agents |
| `list_pages` | 3 | done | Pages of the open document |
| `get_page` | 3 | done | One page as structured data |
| `set_page` | 3 | done | Replace or update one page |
| `layout` and `routing` options | 3 | done | Auto-layout and connector routing applied on open |
| Commands resource | 1 | done | `vizcraft://commands` |
| Resources for pages, shapes, stencils | 3 | done | `vizcraft://page/{id}`, `vizcraft://shape/{id}`, `vizcraft://stencils`, with templates |
| Remote backend | 1 | done | Drives a running app over the control channel |
| Headless backend | 3 | done | Hosts a `Session` in process, no window; `vizcraft-cli mcp --headless`, and the fallback when no app listens |

Tool shapes, resources and launch details are in [docs/mcp.md](mcp.md).
