# Cascade plan for docs/TODO.md

This plan sequences everything in [TODO.md](TODO.md) (the Visio ribbon reference and the
diagramming standards list) into blocks of parallel branches. ROADMAP.md stays the source of truth
for what has landed; this file decides order, ownership and briefs. It starts from main at 9170b25.

## How blocks run

- A block is a set of branches with disjoint file ownership. Branches in a block run at the same
  time, at most five agents at once (Opus, medium effort), each in its own worktree with its own
  `CARGO_TARGET_DIR=target-<branch>` and control port.
- The main session merges a block, runs `cargo xtask ci`, updates ROADMAP.md, docs/parity.md,
  docs/mcp.md and docs/formats.md from the agents' reports, then starts the next block. Agents
  never edit those four files, which removes the usual merge conflicts.
- Shared registries are append-only: an agent may add one line to `crates/engine/src/cmd/mod.rs`,
  one entry to a tab's group list in `crates/ui-egui/src/ribbon/<tab>/mod.rs` and one to the pane list. The
  main session resolves those one-line conflicts at merge. One known overlap remains: in 2A,
  `b2-layers` and `b2-styles` both touch shape drawing in the canvas; merge `b2-layers` second.
- Detailed briefs are written only for the next block to run, against the code as it is then.
  Later blocks list scope and ownership only, so briefs never go stale.

## Brief template

Each agent gets this and nothing else. Fill the angle brackets; leave out any line that does not
apply.

```
Build one branch of VizCraft, a Rust and egui diagramming app.
Rules: read AGENTS.md sections "Never crash", "Everything is a command", "Layering",
"Quality gates" and "Writing style". Skip the rest.
Worktree <path>, branch <name> from main@<sha>. export CARGO_TARGET_DIR=target-<name>.
Control port <port>; screenshots under <scratch dir>. Prefer the headless ui_shot example.
Goal: <one paragraph>.
Own: <paths>. Change nothing else. If you need a change outside these paths, put it in your report.
You may append one line to: <registry files>.
Do not edit ROADMAP.md, docs/parity.md, docs/mcp.md or docs/formats.md.
Commands: <ids and params>.
Done when: <tests and checks>; cargo xtask ci is green.
Stop: commit on your branch with prefix <Bn>:, no merge, no push. Remove nothing outside your
worktree. Report in at most 15 lines: commits, command ids added, doc rows to change, follow-ups.
```

## Dependency graph

| Block | Needs | Can overlap with |
|---|---|---|
| 0 Housekeeping | none | none |
| 1 Foundations | 0 | none |
| 2A Drawing substrate, wave 1 | 1 | none |
| 2B Drawing substrate, wave 2 | 1, 2A (groups, pages) | none |
| 3 Workspace (ROADMAP 4B) | 1, 2A pages, 2B Shape Data pane | 4 |
| 4 Visio extras | 1, 2A, 2B | 3 |
| 5 Generators and glyphs (ROADMAP 5A, 5B) | 1, 3 dock | late 3 and 4 |
| 7 First deep packs (ROADMAP 6) | 5 (glyphs for drawing, patterns and codegen for generator pieces) | 8 once a track's first pack lands |
| 8 Second-wave packs and interop (ROADMAP 7 plus new standards) | 7 per track | 9 |
| 9 General pack and broad interop (ROADMAP 8) | 2B containers, 5 patterns | 8 |

Block numbering skips 6 so blocks 7 to 9 line up with ROADMAP Cascades 6 to 8 plus one.

## Block 0: housekeeping (main session, no agents)

1. Merge `claude/zealous-volhard-327f58` (a9d7584, Fit to Window) into main.
2. Remove the locked `vizcraft-rename` worktree and branch (1c70f2d is already in main).
3. Commit docs/TODO.md and this file.

## Block 1: foundations (3 agents)

Every later branch wants new document fields, a ribbon and pane layout that several agents can
extend without colliding, and compound undo. These three are file-disjoint.

| Branch | Goal | Own | Done when |
|---|---|---|---|
| `b1-doc-fields` (port 7991) | Add every document field later blocks need, with serde defaults, sanitising and `.vizcraft` v4 (v3 still loads). Style: font family, underline, strikethrough, text case, highlight, horizontal and vertical alignment, bullets, indent, text direction, margins, dash pattern, line cap, corner rounding, gradient fill, transparency, shadow, soft edges, glow. Shape: layer ids, parent group or container id, text block offset, size and angle, hyperlinks, screen tip, protection locks, behaviour flags, image asset ref and crop. Page: size, orientation, units, drawing scale, auto-size, background page ref, header and footer, guides, layer list. Document: properties (title, author, subject, keywords), comments (anchor, author, time, text, replies), image asset store. SVG and PNG render the new style and text fields. No commands. | `crates/diagram`, `crates/formats/src/{native,native_tests,svg,png}.rs` | Round-trip tests for every field; a v3 file loads; hostile values sanitise; SVG golden tests for gradient, dash, shadow, alignment |
| `b1-ui-split` (port 7992) | Pure refactor so branches own files: `ribbon.rs` becomes `ribbon/mod.rs` plus one directory per tab and one file per group, so two branches never share a group file; `panes.rs` becomes `panes/` with one file per pane; `canvas/draw.rs` and `canvas/interact.rs` split by concern (shapes, text, connectors, overlays, selection, drag, tools). Add empty-but-registered Data, Process, Review and Developer tabs that stay hidden until they have controls; move Check Diagram (`validate.*`) onto Process. No behaviour change. | `crates/ui-egui` | Screenshots of every tab match before and after; ci green |
| `b1-engine-txn` (port 7993) | Compound transactions: one undo step with a label for any command batch (control channel, MCP `command_batch`, later patterns); a transaction log on Session that later feeds the agent activity view; selection-as-fragment (extract, insert with id remapping) for clipboard, duplicate, paste special, format painter and change shape; command-script record and replay (JSON list of command ids and params) as the macro substrate. | `crates/engine/src` except `cmd/`; `crates/mcp` batch call site | Tests: a batch undoes in one step; fragment round trip keeps connections; a recorded script replays to the same document |

## Block 2A: drawing substrate, wave 1 (5 agents)

ROADMAP Cascade 2 pieces plus the Visio Home, Design and View items that sit on them.

| Branch | Scope | Own | Commands |
|---|---|---|---|
| `b2-pages` | Page tabs, add, delete, duplicate, rename, reorder; background pages; Page Setup dialog (size, orientation, units, scale, auto-size, fit to drawing); canvas edits the active page (fixes the first-page-only follow-up); Insert New Page | `cmd/page.rs`, `canvas/` page switching, `ribbon/design/page_setup.rs`, page tab strip | `page.add`, `page.delete`, `page.duplicate`, `page.rename`, `page.move`, `page.setActive`, `page.setup`, `page.setBackground`, `page.fitToDrawing` |
| `b2-layers` | Layers with visible, lock, print, snap, glue and colour flags; assign shapes; Layers pane; hit testing, drawing, export and print honour the flags | `cmd/layer.rs`, `panes/layers.rs`, filters in hit and draw | `layer.add`, `layer.delete`, `layer.set`, `layer.assign`, `layer.unassign` |
| `b2-arrange` | Align, auto align, distribute, auto space, order (front, back, forward, backward), rotate and flip, size and position, group and ungroup; Home Arrange group | `cmd/arrange.rs`, `ribbon/home/arrange.rs` | `arrange.align`, `arrange.distribute`, `arrange.autoSpace`, `arrange.order`, `arrange.flip`, `arrange.rotate`, `arrange.setBounds`, `arrange.group`, `arrange.ungroup` |
| `b2-snap` | Grid display, snap to grid, shapes, guides and connection points; rulers; draggable guides; dynamic alignment guides while dragging; page breaks overlay; connection points toggle; AutoConnect toggle; Snap and Glue dialog; View Show and Visual Aids groups | `diagram` snap math in a new `snap.rs` (agreed exception to b1 ownership), `canvas/` overlays and drag, `ribbon/view/` | `view.grid`, `view.rulers`, `view.guides`, `view.pageBreaks`, `view.connectionPoints`, `view.autoConnect`, `view.snapSettings`, `guide.add`, `guide.delete` |
| `b2-styles` | Fill, line (weight, dash, arrows, rounding), gradient, transparency, shadow, soft edges and glow on the canvas; our own document themes with variants; quick styles gallery; Backgrounds; Borders and Titles; page-wide connector style (right angle, straight, curved); Home Shape Styles group and Design Themes, Variants, Backgrounds groups | `cmd/design.rs`, `canvas/` style drawing, `ribbon/design/` theme, variant, background and layout groups, `ribbon/home/shape_styles.rs` | `design.setTheme`, `design.setVariant`, `design.applyQuickStyle`, `design.setBackground`, `design.setBorderTitle`, `design.setConnectorStyle`, `shape.setStyle` gains the new keys |

Done when for the block: a two-page document with a background page, two layers (one locked),
aligned and grouped shapes, a theme, guides and snapping can be built by hand and over the control
channel.

## Block 2B: drawing substrate, wave 2 (5 agents)

| Branch | Scope | Own | Commands |
|---|---|---|---|
| `b2-text` | Font family, size steps, underline, strikethrough, text case, highlight, vertical and horizontal alignment, justify, bullets, indent, direction; Text Block tool (move, size, rotate the text area); standalone text box (fixes Text tool on empty page); in-place editor rotates with the shape; Insert Symbol; Fields (date, page name, page number, shape data, geometry) | `cmd/text.rs`, `canvas/` text editing, `ribbon/home/font.rs`, `ribbon/home/paragraph.rs`, `ribbon/insert/text.rs` | `text.setFormat`, `text.setParagraph`, `text.setCase`, `text.insertSymbol`, `text.insertField`, `text.setBlock`, `shape.addTextBox` |
| `b2-edit` | Paste special, duplicate, format painter, Change Shape (keeps text, data, glue, formatting), Select by type, lasso and area select, Find and Replace over text and shape data | `cmd/edit.rs`, `canvas/` lasso, `dialogs.rs` find dialog | `edit.pasteSpecial`, `edit.duplicate`, `edit.formatPainter`, `edit.changeShape`, `edit.selectByType`, `edit.find`, `edit.replace` |
| `b2-containers` | Containers that hold and fit members (margins, heading, lock, select contents, disband, container styles); swimlanes and cross-functional flowchart lanes and phases with flip orientation; callouts attached to a target (style, line, target orientation); Container and Callout contextual tabs; Insert Diagram Parts group | `cmd/container.rs`, `ribbon/container/`, `ribbon/callout/`, `ribbon/insert/diagram_parts.rs` | `container.add`, `container.addMember`, `container.removeMember`, `container.fit`, `container.set`, `container.disband`, `lane.add`, `lane.phaseAdd`, `callout.add`, `callout.set` |
| `b2-data` | Shape Data pane with typed keys (type, unit, enum, required, default hooks); Data tab Shape Data Window toggle | `cmd/data.rs`, `panes/shape_data.rs`, `ribbon/data/` | `data.set`, `data.remove`, `data.defineKey`, `view.shapeDataPane` |
| `b2-output` | PDF export (one page or all, vector); PNG all pages; header and footer rendering; print with page tiling and preview; Save As offers PDF | `crates/formats/src/pdf.rs` (new), `png.rs`, `apps/vizcraft` print | `file.export` gains `pdf` and `pages`, `file.print`, `page.setHeaderFooter` |

Done when: ROADMAP Cascade 2 done-when (multi-page cross-functional flowchart with swimlanes,
layers, theme, typed data, aligned shapes, exported to PNG and PDF, by hand and over the control
channel).

## Block 3: workspace (ROADMAP Cascade 4B, 5 agents)

Scope as in ROADMAP 4B. Visio items folded in: Drawing Explorer becomes the model tree's
Diagrams branch; the Issues window becomes the Problems tab.

| Branch | Scope | Own |
|---|---|---|
| `b3-tree` | Model tree pane, Context mode, Drawing Explorer branch | `panes/model_tree.rs` |
| `b3-palette` | Scoped Palette, New from notation in the backstage, notation ribbon tabs, drill-down breadcrumbs | `panes/palette.rs`, `backstage.rs`, `ribbon/notation/` |
| `b3-inspector` | Typed Inspector grown from the Shape Data pane | `panes/shape_data.rs` becomes `panes/inspector.rs` |
| `b3-dock` | Bottom dock with Problems, Search results, Tables and Generate slots, agent activity view over the b1 transaction log | `crates/ui-egui/src/dock/` (new) |
| `b3-insert` | Hover suggest-and-connect, double-click picker, keyboard chaining, magic connector, command palette over the model; suggestion queries in the engine | `canvas/` insertion module, `crates/engine/src/suggest.rs` (new) |

## Block 4: Visio extras (6 branches, 5 at a time, alongside Block 3)

| Branch | Scope | Own |
|---|---|---|
| `b4-links` | Hyperlinks (URL, file, page, shape, element), ScreenTips, comments with threads (pane, previous, next, show), Process Subprocess (create new page, link existing) | `cmd/link.rs`, `cmd/comment.rs`, `panes/comments.rs`, `ribbon/review/comments.rs`, `ribbon/process/subprocess.rs` |
| `b4-pictures` | Insert picture (PNG, JPEG, SVG) into the asset store, crop tool, Picture contextual tab (brightness, contrast, recolour, transparency, border, change, reset, compress); Ink (pen, highlighter, eraser, lasso, colour, thickness, convert ink to shapes) | `cmd/picture.rs`, `cmd/ink.rs`, `canvas/` crop and ink tools, picture and ink contextual tabs |
| `b4-shapeops` | Boolean operations (union, combine, fragment, intersect, subtract, offset, join, trim) through an MIT or Apache polygon crate; Protection locks; Behaviour; Shape Name; custom stencils and Document Stencil; Developer tab | `cmd/shapeop.rs`, `cmd/master.rs`, `ribbon/developer/`, `crates/stencils` custom masters |
| `b4-view` | Presentation mode, Zoom dialog, Page Width, Pan and Zoom pane, Task Panes menu, New Window on the same document, Arrange All, Cascade, Switch Windows; add-ons Number Shapes, Array Shapes, Move Shapes; Macros group over b1 scripts (record, run, list, delete) | `cmd/view.rs`, `cmd/addon.rs`, `cmd/macro.rs`, `panes/pan_zoom.rs`, `apps/vizcraft` windows |
| `b4-review` | Spelling against a permissively licensed dictionary, proofing language per text; Info backstage page (properties, protect diagram, inspect for hidden data); Shape Reports to CSV, HTML and an on-page table (XLSX after Block 5) | `cmd/proof.rs`, `cmd/report.rs`, `backstage.rs` Info page |
| `b4-datagfx` | Data graphics (text callouts, data bars, icon sets, colour by value) from shape data and element properties; Insert Legend; External Data link to a CSV or XLSX file with Refresh All (the substitute for Excel, Access and SQL wizards) | `cmd/datagraphic.rs`, `cmd/datalink.rs`, `ribbon/data/` groups |

## Block 5: generators and glyphs (6 branches, 5 at a time)

ROADMAP 5A and 5B, plus one notation-drawing foundation every pack in Block 7 needs.

| Branch | Scope |
|---|---|
| `b5-patterns` | Pattern framework, `pattern.*` commands, provenance, Patterns tab, starter workflow patterns |
| `b5-layout` | Pins and model order, band and chain primitives, grid and container primitives (`crates/layout`) |
| `b5-codegen` | Generator framework, ownership manifest, traceability and regions, `generate.*` commands, CLI `--check` |
| `b5-tables` | CSV, XLSX and Markdown table exporters; derived table and matrix views in the Tables tab; Generate dock |
| `b5-reimport` | Re-import contract and table-driven pattern sync (after `b5-patterns` and `b5-tables`) |
| `b5-glyphs` | Rich connector ends (crow's foot family, UML ends, BPMN flows, Chen relationship), compartments, ports drawn on shapes, glyph path extensions, and notation rendering variants so one model can draw in several notations (ER: Information Engineering, Chen, Barker, IDEF1X; DFD: Yourdon and DeMarco, Gane and Sarson) (`crates/stencils`, `diagram/kind.rs`, glyph drawing in canvas and SVG) |

## Block 7: first deep packs (ROADMAP Cascade 6, three tracks)

One agent per track at a time, three to five concurrent. ROADMAP 6 scope, plus TODO items that
fit an existing pack:

| Track | Branches in order | Added from TODO |
|---|---|---|
| Systems | statechart and SCXML; statechart code targets; register, memory and packet maps; timing; embedded patterns | none |
| Enterprise | C4; ER with DDL; UML; BPMN; DFD and threat model; API and events; enterprise patterns | UML pack covers all 14 UML 2.5 diagrams (object, package, profile, composite structure, communication, interaction overview added); ER draws in IE, Chen, Barker and IDEF1X; DFD draws in Yourdon and DeMarco and Gane and Sarson, with Ward and Mellor control flows; Flowchart pack completed to the ISO 5807 symbol set |
| Industrial | P&ID and PFD; industrial lists; loops and I/O; cause-and-effect and logic; SFC and control modules; industrial patterns | Ladder Diagram and Function Block Diagram packs (IEC 61131-3) beside SFC, emitting Structured Text |

## Block 8: second-wave packs and interop (ROADMAP Cascade 7 plus new standards)

ROADMAP 7 scope, plus the standards in TODO Part 2 that no cascade covered:

| Track | New packs |
|---|---|
| Systems | SysML (block definition, internal block, parametric, requirements; shares the component and port model); AADL graphical notation; OPM with generated OPL sentences; FMC; SDL (on the statechart model); Petri nets with a token simulator |
| Enterprise | DMN (decision requirements diagrams, decision tables, a FEEL subset that evaluates); CMMN; EPC with alternation and connector rules; IDEF0 (ICOM boxes, decomposition) and IDEF3, with SADT as an IDEF0 variant; value stream map with lead time totals; framework matrix view (Zachman-style interrogatives against perspectives, in our own wording) and UAF views over SysML |
| Industrial and network | Generic cloud architecture pack (region, zone, network, subnet, security group, cluster containers, generic glyphs only); network topology with physical and logical views, VLANs and routing domains |

## Block 9: General pack and broad interop (ROADMAP Cascade 8)

ROADMAP 8 scope (print already moved to `b2-output`, data graphics and comments to Block 4), plus
the template-specific Visio tabs: Org Chart, Timeline, Cross-Functional Flowchart (lanes from
`b2-containers`) and a PivotDiagram-style pattern that builds a tree from a table. `.vsdx` gains
export beside import, since Save As `.vsdx` is in the TODO and MS-VSDX covers writing.

## TODO coverage

Already done: File New, Open, Save, Save As, Close, Options; Cut, Copy, Paste; Bold, Italic,
font colour and size; Pointer, Connector and Text tools; Re-Layout Page; line jumps; Zoom, Fit to
Window (after Block 0); Check Diagram as `validate.*`; draw.io, Mermaid and CSV interop.

Excluded, with the reason, unless you say otherwise:

| TODO item | Reason | Substitute |
|---|---|---|
| Online Pictures | Third-party image search service | Insert picture from file (`b4-pictures`) |
| Chart (Excel object), Object (OLE), ActiveX controls, COM add-ins | Windows component technologies | Data graphics and table views |
| Visual Basic, Macro Security | No embedded VBA | Command-script macros (`b1-engine-txn`, `b4-view`) |
| SharePoint workflow import and export | Proprietary product integration | none |
| Translate, Thesaurus | External service; no suitable open thesaurus chosen | Spelling only (`b4-review`) |
| 3-D rotation, bevel, reflection effects | Decorative, against the professional direction | Shadow, soft edges, glow (`b2-styles`) |
| Share by email, present online, Account and Office themes | Service and account features | App light and dark theme (done) |
| Cloud provider icon sets | AGENTS.md forbids vendor icons | Generic cloud architecture pack (Block 8) |

Decisions needed before their block starts:

| Item | Question | Block |
|---|---|---|
| CAD drawing (.dwg, .dxf) | DXF is not on the allowed public-specification list in AGENTS.md; add it or drop CAD insert? | 4 |
| AADL textual export | The AADL standard is a paid SAE document; draw the notation only, or buy and use the standard? | 8 |
| PNML (Petri net) and DMN, CMMN XML | Add each public schema to the Formats list in AGENTS.md when the pack starts | 8 |
