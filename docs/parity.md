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
| Window arrangement | B4 | done (B4) | View > Window: New Window (`ui.window.new {same?}`, the same file in a second window with its own copy of the edits), Arrange All, Cascade and Switch Windows (`ui.window.*`) |
| Presentation mode | B4 | done (B4) | `ui.present {on?}`: full screen, the drawing only; arrow keys and PageUp and PageDown switch pages, Escape leaves |
| Page Width | B4 | done (B4) | `view.zoomWidth {page?}` fits the frame's width to the canvas |
| Task Panes menu | B4 | done (B4) | View > Show lists every pane with its check mark, plus the Dock toggle |
| Add-ons | B4 | done (B4) | Number Shapes, Array Shapes and Move Shapes under View > Macros > Add-Ons; `addon.numberShapes`, `addon.arrayShapes`, `addon.moveShapes`, each one undo step |
| Macros | B4 | done (B4) | Record and Stop over `script.*`, Save, Run and Delete; `macro.save`, `macro.list`, `macro.run`, `macro.delete` keep one JSON file per macro in the `macros` folder beside the preferences (`VIZCRAFT_MACROS_DIR` for headless runs) |
| Side panes | 0 | done (chrome) | Left and right panes, resizable and reorderable; diagram panes are added per cascade |
| Ribbon | 0 | done (chrome) | Tabs, groups, contextual tabs; diagram tabs are added per cascade |
| Dropdowns show their current choice | 2C | done | Every ribbon menu, combo list and gallery with a current value marks it with a right-aligned check mark; `ui.inspect` lists the open menu's rows with `checked` |
| Settings organised in the ribbon | 2C | done | Design > Page Setup is a stack of small Orientation, Board and Size menus beside Auto Size; Design > Connectors holds Connector Style, Avoid Shapes (Always, Never), Straight Lines (Detour, Keep Straight), Line Jumps and Default Kind; the Connector tab has a Kind menu and Start and End Arrow menus of all thirteen ends; Insert > Diagram Parts has Compartment (Paragraph, List, Slot); Process > Diagram Validation has Validate, Rules (Off and one entry per ruleset), Live Checking, Next Issue and Previous Issue; Home's Undo group is an icon stack and Delete sits by Cut and Copy so the tab fits 1280 px |
| Command palette | 0 | done (chrome) | Search over every command id and label |
| Control channel | 0 | done (chrome) | JSON lines over localhost; see [docs/control-protocol.md](control-protocol.md) |

## Shapes and stencils

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Basic shapes | 1 | done | Rectangle, rounded rectangle, ellipse, diamond, triangle, parallelogram, hexagon, cylinder, document, cloud; `shape.add {stencil}` |
| Select, move, resize, rotate | 1 | done | Canvas handles (Shift keeps aspect, snaps rotation to 15 degrees), arrow-key nudge, and `shape.*` commands |
| Compartments (sections inside a shape) | 2C | done | Paragraph, list and slot bands stacked under the main text; a slot owns child shapes that move with it and stay inside; each band adds left and right connection points that glue ends name as ports (`{shape, port: "attrs.left"}`); per-band in-place editing; SVG, PNG, PDF and draw.io; `compartment.add`, `compartment.set`, `compartment.remove`, `compartment.move`, and `text.*` with `compartment`; a pack's `ElementType.compartments` fills list rows from properties; Insert > Compartment in wave 2 |
| Multi-select and marquee | 1 | done | Shift-click, marquee, `edit.select {ids, add?}`, `edit.selectAll` |
| Palette pane | 4B | done (B3) | The Shapes pane rebuilt as the scoped palette: compact rows (a tile grid only for Drawing), the active pack's palette groups filtered by the page's viewpoint, a Connectors group of the pack's kinds plus the Basic ones, and a Drawing group of primitives; click or drag to insert; `ui.palette {query?, group?, viewpointFilter?}`; `vizcraft_stencils::sets()` rebuilds when a pack is loaded at run time |
| Basic and Flowchart stencils | 1 | done | Original shapes defined in code (`vizcraft-stencils`), inserted with `shape.add` |
| Connection points | 1 | done | Four per built-in shape; shown on hover, connectors drag from and glue to them |
| Stencil search | 3 | done | Search field in the Palette ranks types, connector kinds and Drawing masters by exact name, word prefix, keyword and substring; `ui.shapesSearch` sets it by id |
| Freeform shapes and boolean operations | B4 | done (B4) | `ShapeKind::Freeform` with a stored path (`Shape.path`, at most 4096 elements); Developer > Operations: `shapeop.union`, `combine`, `fragment`, `intersect`, `subtract`, `offset {distance}`, `join`, `trim`, each one undo step, results round-trip natively and through draw.io as `vizcraftPath`; holes draw and hit-test even-odd since B5 |
| Locks, behaviour and protection | B4 | done (B4) | `shape.setLocks` (width, height, aspect, position, rotation, delete, select, text), `shape.setBehavior` (container, non-printing, hidden, no glue, no snap), `doc.protect {shapes?, backgrounds?}`; commands and canvas drags refuse what a lock forbids; the select lock keeps clicks and marquees off the shape (glue still reaches it) and the text lock is held by `shape.setText`, `text.*` and the in-place editor since B5 |
| Shape names | B4 | done (B4) | `shape.rename {id, name}` and the Developer tab's Shape Name dialog |
| Custom masters (Document Stencil) | B4 | done (B4) | `master.add`, `master.list`, `master.remove`, `master.insert`, `master.export` and `master.import` (a `.vizcraft-stencil` JSON file); the Document Stencil pane with previews |
| Pictures | B4 | done (B4) | `picture.insert {path? or bytes?, mediaType?, x?, y?, w?, h?}` (PNG, JPEG, SVG; bytes stored once in `Document.images` by content hash), `picture.change`, `picture.crop` (fractions of the source), `picture.adjust {brightness?, contrast?, transparency?, recolor?}`, `picture.reset`, `picture.compress {maxDimension?, quality?}`; the canvas draws through a texture cache and SVG, PNG and PDF export write an `<image>` with a clip path and a colour-matrix filter; Insert > Illustrations > Picture, the Picture contextual tab (Adjust, Picture Styles, Crop tool `ui.tool crop`) |
| Ink | B4 | done (B4) | Pen, Highlighter and Eraser tools (`ui.tool pen`, `highlighter`, `eraser`; `ui.inkPen {color?, width?}`); a stroke is one open freeform shape (`ink.add {points, width?, color?, highlighter?}`, Ramer-Douglas-Peucker simplified, at most 4096 points), `ink.erase`, `ink.toShapes` recognises axis-aligned rectangles, ellipses and straight lines; Review > Ink |
| Custom stencils | Future | planned | Plugin stencil sets loaded from files |

## Connectors and glue

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Straight, orthogonal and curved connectors | 1 | done | `connector.add`, `connector.setRoute` |
| Dynamic glue | 1 | done | `connector.glue` naming only a shape; routing is derived from shape geometry |
| Point glue | 1 | done | `connector.glue` with a connection point index |
| Arrowheads | 1 | done (13 ends in 2C, `slash` in B5) | `connector.setArrows`: none, triangle, open, diamond, circle, hollow triangle, hollow diamond, bar, open circle, crow's foot, crow's foot with bar, crow's foot with circle, bar with circle; one geometry table (`diagram::arrows`) draws them on the canvas, in SVG and in PDF |
| Connector end labels | B5 | done (B5) | `Connector.start_label` and `end_label` drawn beside each end by the canvas and the exports (Chen cardinalities, UML multiplicities); `connector.setEndLabels {ids?, start?, end?}`; Connector tab End Labels group; draw.io carries them as `vizcraftStartLabel` and `vizcraftEndLabel` |
| Connector kinds | 2C | done | Named kinds declared by notation packs (`connectorTypes`: route, ends, dash, width, colour, jumps, optional relationship type); `connector.add {kind}`, `connector.setKind`, `connector.kinds`; a kind that names a relationship type wins over the connection table when binding; ribbon gallery in wave 2 |
| Connector labels | 1 | done | `connector.setText` |
| Waypoints | 2 | partly done (3) | Stored bends on connectors, set by routing and read from .drawio; dragging bends by hand is planned |
| Line jumps | 2 | done (3) | Hops where connectors cross; `connector.setJumps`, Design tab |
| Obstacle-avoiding rerouting | 2 | done (2C) | Page policy, on by default for every route style: a dynamically glued end offers every connection point and one search picks the side with the cheapest clear route; containers and lanes enclosing an end are not obstacles; straight lines detour (or keep and report, per `page.setRouting {straightFallback}`); curves round the clear polyline; rerouting is incremental and the drag preview reroutes live; a connector with no clear route keeps its fallback and `autoroute::crossings` reports it, which the `drawing.connectorCrossesShape` rule shows; `page.setRouting`, `layout.route`, `connector.reroute` |
| Connectors never cross shapes | 2C | done | `crates/engine/tests/routing_invariant.rs`: five hundred random fields, the sample, the imports, containers and lanes, and every layout algorithm end with no crossing |

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
| Pan and Zoom pane | 2 | done (B4) | Minimap of the frame and every shape with the visible area as a draggable, resizable rectangle that sets `view.pan` and `view.zoom`; hidden by default (`ui.pane.panZoom`) |

## Data

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Shape data | 2 | done (B2) | Typed rows on shapes (string, number, bool, date, enum, url; unit, required, default); page data keys shapes opt in to (stored in page `ext` for now); element properties shown read-only; `data.*`; Inspector pane (B3, was Shape Data), Define Shape Data dialog, Data tab Show/Hide |
| Data graphics | B4 | done (B4) | `DataGraphic` definitions (text callout, data bar, icon set, colour by value over a shape data key or an element property) in `Document.data_graphics`, one named per shape; `datagraphic.define`, `list`, `remove`, `apply`, `legend` (a group of shapes at the frame's right edge); the canvas and SVG draw the same layout; colour by value never writes `shape.style`; Data > Display Data gallery and dialog |
| External data links | B4 | done (B4) | `datalink.link {path, format?, sheet?, keyColumn, dataKey?, columns?}` reads a CSV or XLSX table (10 000 rows, 256 columns, 4 KB cells; XLSX not on the web) and sets data rows on the shapes whose key matches; `datalink.refresh`, `datalink.bind`, `datalink.unlink`, `datalink.list`; the External Data pane and Data > External Data |
| Import from CSV | 3 | done | draw.io-style CSV; org charts from CSV in Cascade 4 |

## Diagram types and templates

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Flowchart | 1 | done | Flowchart stencil set in the Shapes pane and Insert tab |
| Cross-functional flowchart | 8 | planned | Template on swimlanes from Cascade 2 |
| Org chart | 8 | planned | Stencils and a CSV-driven template |
| UML | B6 | planned | Class, sequence, state, activity, use case, component, deployment, package (B6); communication, interaction overview, timing, composite structure, profile (7); see docs/research/swe-diagramming-ux.md |
| BPMN | 6 | planned | Events, activities, gateways, pools and lanes |
| ER | B6 | planned | One pack with Information Engineering, Chen, Barker and IDEF1X variants; typed column rows; DBML read and write; DDL in Block 7 |
| Network | B6 | planned | Physical and logical views over one model, VLAN ids, zones; generic devices drawn originally; no vendor brand icons |
| Cloud topology | B6 | planned | Vendor-neutral boundary groups with nesting rules, node glyphs, request and event lines, numbered step badges, a key; Mermaid architecture-beta read and write |
| Data flow | B6 | planned | Yourdon and DeMarco and Gane and Sarson variants, trust boundaries; STRIDE lists in Block 7 |
| Templates | B6 | partial | Engine done (B6 wave 1): `Pack.templates` with seeds that name their results, layout, variant, sheet and module, optional hooks (`sheet.set`, `sheet.titleBlock`, `page.setModule`, `key.insert`); `template.list`, `template.new`, `template.addPage`, `template.standards` (`Document.standards`, IEC and ISO A3 by default), `template.preview` (SVG); `vizcraft-cli new`; Basic and Flowchart templates. The New gallery, previews as cards and Recent are the frontend wave |
| Engineering workspace preset | B6 | planned | `ui.preset {engineering or office}`: rulers, grid, page frame, pane and Home tab defaults |
| Code pane | B6 | planned | The page as Mermaid, DBML or Structurizr DSL text in sync with the canvas; `code.read`, `code.write`; PlantUML and D2 writers |
| Analysis | B6 | partial | Engine done (B6 wave 1): `vizcraft-analysis` crate; `analyze.dependencies`, `cycles`, `unreachable`, `spof`, `metrics`, `path`, `select`, `result`, `clear` and `model.diff` with the result in `Session.analysis`; `vizcraft-cli analyze --check`. Canvas highlights, the Analyze ribbon group, the Analysis dock tab and the Compare page are the frontend wave |
| Key and title block | B6 | planned | Generated key shapes (`key.insert`, `key.refresh`) and a title block shape with fields |
| Sequence editing | B6 | planned | Lifeline columns, message rows that reorder, fragments spanning lifelines; Mermaid sequence read and write |
| Fluid power | B6E | planned | The `fluidpower` pack: ISO 1219 hydraulic and pneumatic circuits, valves built from positions, ports and path tables, ISO 9461 and 5599 port sets, displacement-step chart, port and pressure path rules |
| Mechanical schematics | B6E | planned | The `mechanical` pack: ISO 3952 kinematic and transmission diagrams, bond graphs, control block diagrams; `mech.mobility`, `mech.ratio` |
| Sheets and title blocks | B6E | partial | Engine done (B6E wave 1): `Page.sheet` with ISO 5457 and ASME Y14.1 sizes, zones and frames, the title block generator over ISO 7200 document properties, revision tables (`sheet.revise`), parts lists and balloons, `{{zone}}`, `{{sheet}}`, `{{sheets}}`, `{{doc:key}}` fields; `sheet.*`, `doc.setProperty`, `doc.properties`; the `sheet` pack. Ribbon, Page Setup menus, canvas table columns and the sheet palette are the frontend wave; ASME zone data to verify against MIL-STD-100G |
| Engineering annotation | B6E | planned | The `annotation` pack: feature control frames and datums (ASME Y14.5 or ISO 1101), welding symbols (AWS A2.4 or ISO 2553), surface texture |
| Reliability | B6E | planned | The `reliability` pack: fault trees, reliability block diagrams, event trees, FMEA table; `rel.cutsets`, `rel.availability` |
| IDEF0 | B6E | planned | The `idef0` pack: ICOM boxes and decomposition pages with the FIPS 183 rules; IDEF3 and SADT in Block 8 |
| Nets and junctions | B6E | partial | Engine done (B6E wave 1): `Model.nets`, `Connector.net`, junction shapes, split and heal, the net pass in sync with `net.conflict`, `Session.nets`, wire numbers and line markers in SVG and PDF; `net.*`, `wire.draw`, `wire.splice`, `wire.heal`, `shape.delete {heal}`. The Wire and Pipe tools, W and P keys, net highlight and the Nets dock tab are the frontend wave |
| Symbol orientation and parametric glyphs | B6E | partial | Engine done (B6E wave 1): `Shape.orient` (quarter turns and mirror, text upright), `shape.rotate {turns}`, `shape.mirror`, `View.module` with snap and `page.setModule`; ten builders (contact, coil, bubble, valve, cylinder, pump, gate, fcf, weld, texture) behind `Glyph.builder`, `Shape.glyph`, rebuilt before bind; `glyph.*`; provenance checked by `cargo xtask assets`; `vizcraft-cli glyph`. Canvas glyph labels, R, X and Y keys and the Inspector Symbol group are the frontend wave |
| Timeline | 8 | planned | Timeline shapes and template |

## Review

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Comments | B4 | done (B4) | Threads anchored to a page or shape with replies; `comment.add`, `reply`, `edit`, `delete`, `list`, `show`, `next`, `previous`; the Comments pane and Review > Comments; markers on the canvas while the pane is open reveal their thread; comments on a deleted shape move to the page |
| Hyperlinks and screen tips | B4 | done (B4) | `link.add {id, kind: url, file, page, shape or element, target, label?}` (Mod+Shift+K opens the Link dialog), `link.set`, `link.remove`, `link.follow` (switches page and selects, or asks the app to open a URL or file), `shape.setScreenTip`; link glyph on the canvas, Mod+click follows, tooltip after a hover |
| Subprocesses | B4 | done (B4) | `process.createSubprocess {ids?, name?}` moves the selection to a new page behind one linked Predefined Process shape in one undo step; `process.linkSubprocess {id, page}`; Process > Subprocess |
| Spelling | B4 | done (B4) | `proof.check`, `proof.suggest`, `proof.ignore`, `proof.setLanguage` over a SCOWL en-US word list (`assets/dictionaries`); fields, URLs, paths, capitals and words with digits are skipped; Review > Proofing opens the Spelling dialog, Review > Language sets the proofing language (`Shape.language`, `Document.language`, `none` skips); only `en-US` has a dictionary; F7 opens the dialog |
| Shape reports | B4 | done (B4) | `report.list` and `report.run {report: inventory, data or connections, scope, format: csv, html or table, path?}`; the table format places a compartment shape at the frame's right edge; Review > Reporting; XLSX waits for Block 5 |
| Info page and hidden data | B4 | done (B4) | Backstage > Info: `doc.setProperties`, statistics, `doc.inspectHidden` and `doc.removeHidden {what}` (one undo step) |
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
| Model tree pane | 4B | done (B3) | Default left tab (pane key `navigation`): Model branch (containment tree from `model.tree`, roots grouped by type, each row with name, tag and pages) and Diagrams branch (every page with its shapes and connectors, the Drawing Explorer); click selects and switches page, double-click renames, drag re-owns through `model.move`, row menu for Show in All Views, Delete from View and Delete from Model; rows virtualised; `ui.tree {mode?, branch?, expand?, collapse?, reveal?}` |
| Model tree Context mode | 4B | done (B3) | Expands to and highlights the selection's elements and filters the Model branch to owners and neighbours; relationships are not rows yet |
| Scoped Palette pane | 4B | done (B3) | See Palette pane above; with no view every pack's groups appear collapsed under the pack name |
| Patterns tab | 5A | done (B5) | Patterns pane (`ui.pane.patterns`, `ui.patterns {query?, pattern?}`) lists the packs' patterns with typed parameter forms and shows the selected instance's arguments with Apply; Insert > Patterns has a Pane toggle and an Insert menu of the active pack's patterns; the command palette inserts patterns too |
| Pattern commands | 5A | done (B5) | `pattern.list {pack?, audience?}` with each parameter schema, `pattern.insert {pattern, args?, page?, x?, y?}` (one undo step: elements, relationships, shapes laid out by a layout primitive, grouped, recorded in `Model.patterns`), `pattern.reparameterize {instance, args}` keeps hand-edited labels, `pattern.instances {page?}`, `pattern.forget {instance}` |
| Starter workflow patterns | 5A | done (B5) | Parallel split, exclusive choice, N-of-M join, multiple instances and cancel region over the Flowchart pack; splits draw as Decisions named Split since Flowchart has no fork symbol, and the cancel region is a container with no model element |
| Typed Inspector | 4B | done (B3) | The Shape Data pane grown into the Inspector (pane key `shapeData`): Identity (name, tag with a live `element.checkTag`), Properties typed by the element schema, shape data rows and page keys, Relationships, Views, Shape and Document properties sections; Standard and Expert modes (`ui.inspector {mode?, tab?}`); Format tab over `shape.setStyle` and `text.setFormat`; `inspector.get {id}` assembles the same report for agents; the Properties pane became the Document section |
| Problems tab | 4B | done (B3) | Bottom dock tab: live issues merged with the last `validate.run` by key, severity and text filters, click selects on the page, Explain, Suppress and one button per `Issue.fixes` entry, toolbar with Check, Rules, Live Checking, Next and Previous; Check from the ribbon opens it; `ui.dock {open?, tab?, height?}` |
| Tables tab | 5B | done (B5) | `table.list {type? or pack?, columns?, filter?}` and `table.matrix {rows, cols, relationship? or property?}` as views over the model, edited in place (`table.set`, `table.toggle`), exported with `table.export {format: csv, xlsx or markdown}`; `ui.tables`; XLSX export needs a path, so the web build exports CSV and Markdown only |
| Search results tab | 4B | done (B3) | Dock tab over `edit.find` with Page or Document scope; click selects; `ui.search {query, scope?}` |
| Generate tab | 5B | done (B5) | Generate dock tab with the preview diff and run log, and a Generate ribbon tab; `ui.generate {spec?, target?, output?, run?}`; the Markdown catalogue target is the first target |
| Agent activity view | 4B | done (B3) | Activity dock tab lists `Session::transactions()` newest first with origin badge and command ids, and "Undo to before this" |
| Hover suggest-and-connect | 4B | done (B3) | Four handles outside the hovered shape (while the AutoConnect aid is on); a click runs `insert.next` with the one suggestion or opens a popover of `suggest.next` (legal targets with their relationship, most frequent on the page first); dragging a handle to empty canvas drops a new shape there |
| Double-click insert picker | 4B | done (B3) | Type-to-filter list of the page's palette types and Drawing primitives; Enter inserts at the point; `ui.insertPicker {x?, y?, query?}` |
| Keyboard chaining | 4B | done (B3) | Alt+Shift+Arrow runs `insert.next` in that direction with the first suggestion and selects the new shape |
| Magic connector | 4B | done (B3) | Dragging a handle onto another shape opens the `suggest.connect` choices (illegal pairs last and marked); one legal choice applies at once; `insert.connect {from, to, relationship?}` |
| Command palette over the model | 4B | done but patterns (B3) | Mod+K or Mod+Shift+P; `suggest.search {query, kinds?}` over commands, element types of the packs in use and elements by name or tag; commands run, types insert at the view centre, elements select and reveal; patterns wait for Cascade 5A |
| Notation variants | B5 | done (B5) | `Pack.variants` (one drawing per element type per variant), `View.variant` on the page, `variant.list {page?}` and `variant.set {page?, variant}` redraw the page in one undo step keeping shape centres; Variant menu on the notation tab; the ER and DFD packs fill it in Block 7 |
| Glyph paths and ports | B5 | done (B5) | `Glyph.path` and `Glyph.detail` draw a type's own outline (`notation/src/glyph_path.rs`, `Master.path`); `ElementType.ports` draw as port marks on the canvas and in SVG (`<g class="ports">`) and bind by connection point index; freeform fills are even-odd on the canvas and in SVG, so holes stay empty; connector kinds from a pack loaded at run time now draw |
| Notation ribbon tabs | 4B | partly done (B3) | One contextual tab per active pack (Insert menus per palette group, Viewpoint menu, Check) and a Model tab; Validate and Generate tabs wait for Cascade 5B |
| New from notation | 4B | done (B3) | Backstage New page lists packs by audience; choosing one runs `file.new` then `model.setViewpoint` |
| Drill-down breadcrumbs | 4B | done (B3) | Path bar above the canvas (document, page, owner element for `<element id>/<viewpoint>` views); click switches pages |

## Validation and identity

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Identity registry | 4A | done | Project-wide tags and reference designations |
| Auto-numbering | 4A | done | Loop numbers, line numbers and designations handed out by the registry |
| Uniqueness checks | 4A | partly done (4A) | Duplicate tags raise a live issue; the Inspector checks a tag as you type through `element.checkTag {id, tag}` (B3) |
| Composed identifiers | 4A | done | Identifier formats declared by the pack (for example `{letters}-{n:3}`), composed and auto-numbered by the registry |
| ISA-5.1 tags and composed line numbers | B6E | planned | Configurable formats in the P&ID pack, not hardcoded letter tables |
| IEC 81346 reference designations | B6E | partial | Identifier parts `{owner.tag}`, `{owner:TYPE.tag}`, `{page}`, `{zone}`, `{rung}` and per-owner scopes done (B6E wave 1, `=F1+A2-K3` composes); the electrical pack is wave 4 |
| Off-page connector pairing | 4A | done | Paired objects, matched by identifier |
| Unmatched off-page detection | 4A | done | An off-page connector with no continuation raises an issue |
| Cross-reference display | B6E | partial | Engine done (B6E wave 1): cross-reference relationship kind, generated labels in the configured form (`/2.C3`), `xref.list`, `xref.go`, unmatched off-sheet issue. Click navigation is the frontend wave |
| Flowchart rules | 4A | done | Unconnected connectors and dangling decisions, declared in the Flowchart pack; grouped in the `flowchart.iso5807` ruleset with standard references (2C) |
| Conformance profile | 2C | done | `Document.conformance`: on or off, chosen rulesets (a pack id means every rule of the pack), severity overrides, suppressions by issue key; `validate.setProfile`, `validate.suppress`; a profile the user never set behaves as before (every rule of the packs in use) |
| Rulesets and references | 2C | done | Packs declare named rulesets; every rule may cite its standard; `notation.rulesets`, `validate.explain {rule}` |
| Drawing rules | 2C | done | The `drawing` pack, no glyphs: connector crosses shape (batch), unglued end, overlapping shapes, off the board, text overflow, each with a fix where one exists |
| Reasoner | 2C | done | A fact index per run: type hierarchy (`extends`), transitive containment, inverse relationships (`inverseOf`), reachability; type lists in checks match subtypes |
| Rule vocabulary | 2C | done | Added `forbiddenCycle`, `requiredPath`, `connected`, `containment`, `portDirection`, `portFanIn`, `uniqueProperty`, `propertyCompare`, `nameFormat` and the drawing checks to the twelve of 4A |
| Fixes on issues | 2C | done | `Issue.fixes: [{label, command, params}]` (at most four); the Problems tab and agents run them |
| Packs from disk | 2C | done | `notation.load {path}`, the `packs` folder beside the preferences file (scanned at app start), `VIZCRAFT_PACKS_DIR` for headless runs; pack test cases under `tests/<pack>/` run by `vizcraft-cli validate` and the notation tests |
| Issue navigation | 2C | done | `validate.next`, `validate.previous` select the issue's items on their page; ribbon buttons in wave 2 |
| Statechart checks | 6 | planned | Unreachable states, dead ends, unknown events |
| Memory map checks | 6 | planned | Overlap, alignment, overflow |
| Cross-sheet continuity | B6E | planned | P&ID batch rule |
| Port compatibility | 7 | planned | Component and port architecture pack |

## Patterns

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Pattern framework | 5A | planned | `vizcraft-patterns`: typed-parameter generators that emit model elements in one undo step |
| `pattern.insert` | 5A | planned | One command with a discoverable parameter schema, also over MCP |
| `pattern.reparameterize` | 5A | planned | Change parameters later, keeping manual label edits |
| Pattern provenance | 5A | planned | The inserted group records pattern id and parameter values |
| Table-driven pattern sync | 5B | done (B5) | Both ways where unambiguous: RACI to swimlanes, parent-id table to org chart, cause-and-effect table to matrix; `pattern.sync {instance, direction: fromTable or toTable, table? or path?}` over the `table.raciLanes`, `table.tree` and `table.matrix` patterns; an ambiguous model (two lanes, two parents, two relationships in one cell) is refused as `pattern.syncAmbiguous` |
| Layout primitives | 5A | done (B5) | `layout.primitive {primitive: pipeline, bands, chain, matrix, nested, hubSpoke, rings, timeline or voter, ids?, roles?, direction?, nodeSpacing?, layerSpacing?}`; pinned nodes (`layout.incremental` pins what it keeps) and `layout.full {keepOrder}` for model order |
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
| Generation as commands | 5B | done (B5) | `generate.targets`, `generate.preview`, `generate.run`, `generate.check {spec? or target, output?, options?, scope?, renames?}`; Generate tab, CLI and MCP |
| CI check mode | 5B | done (B5) | `vizcraft-cli generate FILE [--spec or --target and --output] [--check or --preview] [--json]` exits 1 when output is stale or a run stopped, 2 for usage |
| Generator spec file | 5B | done (B5) | Target, options, output directory and model scope, versioned in the repository (`vizcraft-codegen` `spec.rs`); Browse picks it with `pick_open("spec")` |
| Ownership manifest | 5B | done (B5) | Every generated file with its last checksum (`sha2`); writes only owned files in the declared directory |
| Generation gap by default | 5B | planned (the Markdown catalogue is a single-file target; the separate-file contract waits for the first code target) | Rust: generated module plus a user-implemented trait; C: generated `.c` calling `extern` prototypes |
| Protected regions keyed by element id | 5B | done (B5) | Only for single-file targets such as a PLC POU; the Markdown catalogue uses them |
| Traceability markers | 5B | done (B5) | `vizcraft:id=<element id>` on every generated construct; rename map through `renames` |
| Deterministic output | 5B | done (B5) | Regenerating an unchanged model is byte-identical |
| No silent overwrite | 5B | done (B5) | Preview diff; three-way merge (`diffy`) or stop with an issue when an owned file changed |
| Construct-support matrices | 5B | done (B5) | Each target declares its matrix; unsupported constructs raise issues, or are flattened on opt-in |
| Standard intermediates | 6, 7 | planned | SVD, DBC, OpenAPI, AsyncAPI, protobuf, PLCopen XML rather than runtimes |
| Fidelity tests by re-import | 6 | planned | DDL, SCXML, SVD and OpenAPI output re-imported in tests and compared with the model |
| Re-import contract | 5B | done (B5) | `file.import {.., merge?, source?, type?}` and `file.reimport {source?, page?, layout?, direction?, routing?, type?}`: imported elements keep model ids (CSV `id` column, Mermaid node ids), geometry is kept for existing ids, new elements are placed with the pinned incremental layout, removed ones raise `import.removed` issues; `Document.imports` records each source |
| Table exports | 5B | planned | CSV, XLSX, Markdown |
| Embedded targets | 6, 7 | planned | Statechart C and Rust, C headers, SVD, ld `MEMORY`, packet code, WaveJSON (6); interface headers, task tables, ROS 2 skeletons, DBC (7) |
| Enterprise targets | 6, 7 | planned | Structurizr DSL, DDL, DBML, migrations, ORM entities, class skeletons, BPMN XML, OpenAPI, AsyncAPI, protobuf, STRIDE lists (6); PlantUML, D2, DOT, ArchiMate exchange, HTML site (7) |
| Industrial targets | 6, 7 | planned | ST interlocks, ST CASE machines, FB skeletons, global variable lists, industrial lists (6); PLCopen XML, NodeSet2, Proteus XML, CAEX, B2MML (7) |

## Industrial automation pack

| Feature | Cascade | State | Notes |
|---|---|---|---|
| P&ID and PFD | B6E | planned | The `process` pack with ISA and ISO 10628 variants; DEXPI-named equipment, nozzles, piping segments, inline valves, instrumentation functions, signal lines |
| Paired off-page connectors | B6E | planned | On the 4A identity registry |
| Line, equipment, instrument and valve lists | B6E | planned | Table views; CSV and XLSX |
| Instrument index and I/O list | B6E | planned | I/O points with signal type, range, rack, slot, channel |
| Alarm list | 6 | planned | ISA-18.2-shaped |
| Loops and loop diagrams | B6E | planned | Loops as model objects; loop diagrams generated per loop; `loop.trace` |
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
| Electrical | B6E | planned | The `electrical` pack with schematic, ladder, one-line, three-line, I/O, panel, interconnection and logic viewpoints and IEC 60617, IEEE 315 and JIC variants; IEC 81346 designations, contacts and coils with cross-references, terminals, wires, potentials; terminal, wire and cable lists, BOM |
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
| C4 | B6 | planned | Landscape, context, container, component, dynamic (numbered steps) and deployment viewpoints; implied relationships; drill-down levels; Structurizr DSL and Mermaid C4 read and write |
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
