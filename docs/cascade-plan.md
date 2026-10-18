# Cascade plan for docs/TODO.md

This plan sequences everything in [TODO.md](TODO.md) (the Visio ribbon reference and the
diagramming standards list) into blocks of parallel branches. ROADMAP.md stays the source of truth
for what has landed; this file decides order, ownership and briefs. It starts from main at 9170b25.

## How blocks run

- A block is a set of branches with disjoint file ownership. Branches in a block run at the same
  time, at most five agents at once (Opus or Sonnet as the block says, medium effort), each in its own worktree with its own
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
| 2C Diagramming correctness | 2B (containers, text) | none |
| 3 Workspace (ROADMAP 4B) | 1, 2A pages, 2B Shape Data pane, 2C (conformance profile and `Issue.fixes`, connector kinds, `describe`, disk packs) | 4 |
| 4 Visio extras | 1, 2A, 2B, 2C board frame (`b4-fixes`); inside the block `b4-pictures` needs `b4-shapeops` merged | 3 |
| 5 Generators and glyphs (ROADMAP 5A, 5B) | 1, 3 dock, 4 table reader (`b4-datagfx`) | late 3 and 4 |
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

## Block 2C: diagramming correctness (8 branches in two waves)

State: done. Wave 1 (seven branches) merged into main on 2026-10-18 with `cargo xtask ci` green;
wave 2 (`b2c-ribbon`) merged the same day. Worktrees removed. Integration notes: every clash between branches was a read of
the stored sheet size that the board branch's guard test caught (describe, the drawing pack's
off-board check, three tests planting off-board shapes or crossings); fixed by reading
`Page::frame()` or pinning a test page to a fixed board or to `page.setRouting {avoid: never}`.
Follow-ups left for later briefs: `Page::rect()` is still the sheet in lane, container, arrange
and lasso code, and generated borders and `autolayout::grow_page` use it; the Shapes pane is
built once, so a pack loaded later does not appear in it; paste does not remap slot member ids;
the draw.io writer exports routed end points as extra points; the router's final stub is
shorter than an arrowhead; `design.setBackground` and `design.setBorderTitle` act on page 0
rather than the active page; the ISO 5807 rule references name symbols, not clauses. All but the
last are folded into the Block 3 and Block 4 briefs below (`b4-fixes` holds the ones no feature
branch owns); the clause references wait for Block 7's Flowchart completion.

Before the morphology templates, checkers and code exports of Blocks 5 and 7 build on the
drawing layer, the drawing layer has to be right. This block fixes what is wrong today
(connectors cross shapes, a fixed sheet, dropdowns that do not show their state, no sections
inside shapes, no connector kinds, a validator nobody can switch on or off, an MCP surface that
makes agents read colours) and lays the groundwork for the conformance engine. It starts from
main at 9912127. Runs before Block 3, which consumes it (the Problems tab shows conformance
issues, the scoped palette lists connector types).

Models: a branch whose brief is a precise specification of data and sweeps runs on Sonnet; a
branch that has to make design judgements inside geometry, rendering or the rules engine runs on
Opus. Both at medium effort.

### What is wrong and how each fix works

**Connectors cross shapes.** Today `avoid` is a per-connector flag that is off by default
(`crates/diagram/src/model.rs`, `Connector::default`). A connector without it draws the plain
two-bend route from `route::orthogonal_points`, a straight segment or one cubic, none of which
looks at other shapes. With it, `autoroute::refresh` reroutes through `vizcraft-routing` after
every command, but four things still let a line cross a shape: dynamic glue picks the connection
point nearest the other end before routing (`route::resolve_through`), so the line often leaves
on the side that faces the obstacle and has to wrap around its own shape; every non-group shape
is an obstacle, so a connector between two members of a container or lane starts inside an
obstacle, finds no path, and falls back to the crossing route; Straight and Curved never avoid
anything; and a failed search clears the bends silently (`r.fallback`), so the crossing is
invisible to the user and the agent. During a drag the preview draws the stale bends until the
`shape.move` commits on release.

The fix makes avoidance the page policy rather than a flag, and makes the single avoiding
polyline the source of truth that every route style renders:

1. State the invariant and make it checkable: no segment of a connector's drawn polyline enters
   the outline of any solid shape except the stubs inside its own end shapes, with the router's
   margin. `autoroute::clear_of_shapes` grows into `autoroute::crossings(page) -> Vec<Crossing>`
   (connector id, shape id, segment), the oracle every test and the drawing rule below use.
2. Side choice becomes part of routing. A dynamically glued end hands the router every
   connection point of its shape as a candidate (point glue stays fixed); the grid search runs
   once with all start stubs as sources and all end stubs as targets, so the cheapest clear route
   decides which side the line leaves and enters ("correct to the other side" and "go around"
   fall out of the same search, and the choice is deterministic because the shapes alone decide
   it). The chosen point is written back to the end as its resolved point, not as point glue, so
   the end stays dynamic.
3. The obstacle set is membership-aware: a container, lane set or lane that encloses either end
   is not an obstacle (its heading band still is); groups never are; shapes on hidden layers are
   not; the end shapes are not. Rotated shapes stay bounding boxes.
4. Straight and Curved avoid too. Straight: among the candidate point pairs, the first pair whose
   segment is clear is used; with none, the connector draws the orthogonal detour (the page policy
   `straightFallback: "detour" | "keep"` lets a circuit page keep hard straight lines and report
   them instead). Curved: the orthogonal polyline is rounded at its bends with the largest corner
   radius that stays clear (checked by sampling the arc against the obstacles), so the curve
   never cuts a corner into a shape.
5. Failure is never silent: after the margin retry and the wider search regions, a connector
   with no clear route keeps the fallback bends and is reported as a drawing issue
   (`drawing.connectorCrossesShape`, see the conformance branch, which reads `crossings`).
6. Rerouting is incremental: `refresh(doc, before)` reroutes only connectors whose ends or whose
   bend corridor touch a shape whose bounds changed between `before` and `doc` (plus every
   connector on a page whose shape count changed), so turning avoidance on for every connector
   does not make every command cost a full page route. `layout.route` still routes everything.
7. The drag preview reroutes the connectors touching the moved shapes once per frame with a
   small grid budget, and the commit runs the real pass; a shape is never seen being crossed
   while it moves. Fan-out spacing: the nudge pass also spreads first and last segments of
   connectors sharing a side, which fixes the collinear-run follow-up.

Proof: a routing property test (random obstacle fields and end pairs, five hundred cases, assert
`crossings` is empty or the route is reported as unroutable), the sample document, the Mermaid,
CSV and draw.io imports, and the layout algorithms all pass through `crossings` with an empty
result; a snapshot of a drag mid-way shows no crossing.

**A fixed sheet.** `Page` has `width` and `height` and twenty-six places read them (canvas
sheet and shadow, initial pan, zoom to fit, rulers, page breaks, SVG, PNG, PDF, print tiling,
draw.io write, Mermaid and CSV import sizing, `ui.inspect`, Page Setup). The fix adds one field,
`Page.board: Board { Infinite, Fixed }`, serialised as `"board": "infinite" | "fixed"`, default
Infinite for new documents and pages and Fixed for files that lack the key (an old file keeps
its look), and one helper, `Page::frame(&self) -> Rect`: the page rectangle for Fixed, and the
drawing bounds plus a margin (the `page.fitToDrawing` margin; a default Letter-sized frame at the
origin when the page is empty) for Infinite. Every consumer reads `frame()` instead of the
fields. On an infinite board the canvas draws no sheet and no shadow, the grid covers the
viewport, rulers measure from the world origin, page breaks hide, zoom to fit fits the drawing,
exports and print use the frame, and Page Setup greys the size fields behind a Board choice.
`page.setup` gains `board`. Proof: SVG and PNG goldens of the same drawing on both board kinds,
a v4 file without the key loads Fixed, `ui.inspect` reports the board kind.

**Dropdowns do not show their state.** Ribbon menus are `(label, id, params)` tuples
(`ribbon/mod.rs`, `MenuEntry`) drawn by `widgets::menu_item`, which has no notion of a current
value; the Size menu fakes it by appending "(current)" to a label. The window menu bar already
draws a check mark (`menus.rs`, `check_mark`), but on the left. The fix makes `MenuEntry` a
struct with an optional `checked: fn(&CraftApp) -> bool`, moves `check_mark` into `widgets.rs`,
and has `menu_item` paint it right-aligned (after the shortcut column) whenever `checked` holds,
with the row's accessibility info marked selected. The combo list (`widgets::combo`) and the
quick-style and theme galleries get the same mark on the current item. Then every menu that has
a current value declares it: Orientation, Size (the "(current)" suffix goes), Connector Style,
Re-Layout (the page view's algorithm and direction), zoom presets, units, themes and variants,
arrowheads. The window menu bar keeps its left-hand mark, which is the platform convention
there. Proof: snapshot tests of each open menu; `ui.inspect` lists menu rows with `checked`.

**No sections inside shapes.** A shape has one text and one text block. The fix adds
compartments: `Shape.compartments: Vec<Compartment>`, each `{id, kind: paragraph | list | slot,
title?, text, rows: [text], align, height: auto | n, style overrides}`, drawn as stacked bands
inside the outline below the main text, separated by the outline's line; `paragraph` is one text
block, `list` is one row per line (a UML attribute list, a tag's fields, a card's checklist),
`slot` is a region that owns child shapes (`parent` set to the shape; a member is clamped into
the slot rectangle and moves with it, the same contract containers use). Each compartment
contributes two connection points (left and right midpoints) after the kind's defaults, and an
end may name them as a port (`End::Glued { port }`), so an ER attribute row or a pin row can be
glued to. Text editing works per compartment (double-click a band edits that band). A pack's
`ElementType` may declare `compartments` so a notation fills rows from properties; the Flowchart
pack declares none, the groundwork is the schema and the sync. SVG, PNG and PDF draw them;
draw.io round-trips them through `ext`. Commands: `compartment.add`, `compartment.set`,
`compartment.remove`, `compartment.move`, and `text.*` take a `compartment` id. Proof: a UML-style
class with three compartments and an equipment tag with a list round-trip through the native
format and export to SVG, PDF and draw.io; a slot member follows its parent; hit testing picks
the row.

**No connector kinds.** A connector has a route, two of five arrowheads and a style; nothing
names what it is. The fix mirrors stencils: a pack declares `connectorTypes` (id, label,
category, route, start and end ends, dash, width, colour, jumps, keywords, and optionally the
relationship type it means), the Basic pack ships Line, Arrow, Two-Way, Dependency (dashed,
open end), Bus (thick), Signal (thin, dot start), and the Flowchart pack ships Flow. `Connector`
gains `kind: String` (the type id, as `Shape.stencil`), `connector.add` takes `kind` and applies
the defaults, `connector.setKind` re-applies them keeping glue, text and bends, and
`connector.kinds` lists them. Line ends grow from five to the notation set every later pack
needs: hollow triangle, hollow diamond, bar, open circle, crow's foot, crow's foot with bar,
crow's foot with circle, bar with circle, each drawn in the canvas, SVG and PDF from one
geometry table in a new `diagram/src/arrows.rs`. `bind::sync` prefers a kind's relationship type
over the connection table when the kind names one (an illegal pair still binds and the rule
reports it). Proof: every end renders in a golden SVG; a kind round-trips natively and through
draw.io `ext`; a Flow kind between two flowchart shapes creates a `flowchart.Flow`.

**Settings scattered, Orientation too big.** Done in the second wave once the commands exist:
Design > Page Setup becomes a stack of small menus (Orientation, Board, Size) beside Auto Size
and the dialog launcher; Design > Connectors holds the page routing policy (Avoid Shapes: always
or never; Straight lines: detour or keep; Line Jumps) and the default connector kind; Design >
Layout keeps Re-Layout and Route; the Connector contextual tab gains a Kind gallery and the new
ends; Insert gains Compartment; Process > Diagram Validation gains the Rules menu (Off, then one
entry per ruleset with its check mark), Live Checking, Next Issue and Previous Issue. The Home
tab's Undo group becomes a small stack so the tab fits in 1280 px.

**Agents pay for colours.** `page.get` (`format: "vizcraft"`), `shape.get`, the
`vizcraft://page/{id}` and `vizcraft://shape/{id}` resources and `get_page` return every shape
with its full `style` (fill, line, font, dash, gradient, shadow, glow), its `ext` draw.io
string, waypoints and connection points. An agent designing a circuit reads all of it to learn
that `s3` connects to `s4`. The fix is one detail ladder for every read path: `structure` (ids,
kinds and stencils, texts and names, element and relationship ids, glue with ports, containment,
compartments, data rows; no coordinates), `layout` (adds integer x, y, w, h, route style, board
frame), `full` (adds style, ext, waypoints, connection points). New commands `doc.describe`,
`page.describe` and `shape.describe` take `detail` (default `structure`) and `format: "json" |
"text"`, where text is an indented outline (`s3 flowchart.process "Validate" -> s4 (Flow)`)
that costs a fraction of the JSON. The MCP resources default to `structure` (`?detail=full`
asks for the rest), `doc_inspect` takes `detail`, `command_list` takes `brief` (id and label
only), and the server `instructions` tell agents to read structure first and ask for style only
when styling. Nothing is hidden: `full` still returns everything, and every write command still
accepts style. Proof: a test that the structure description of the sample document is under a
byte budget and contains no colour, font or style key; the headless MCP test suite reads
structure and full.

**A validator nobody chose.** The rules engine exists (`vizcraft-notation::rules`, twelve
data-driven checks, live and batch, `validate.run` and `validate.list`, issues on every MCP
call), but it runs whenever a pack is in use, with no way to say which standard a document is
held to, to switch it off for a sketch, or to add a check the vocabulary lacks without Rust. The
proper names: what you call morphology checking is well-formedness or conformance checking (UML
calls them well-formedness rules, RDF has SHACL shapes, electronics has electrical and design
rule checks); the part that derives facts before checking (a Pump is an Equipment, so Equipment
rules apply; A inside B inside C means A inside C) is the reasoner. The groundwork:

1. A document-level conformance profile, `Document.conformance { enabled, rulesets, overrides,
   suppressions }`. Off by default for free drawing; choosing a notation for a page turns it on
   with that pack's default ruleset. A pack declares named rulesets (`flowchart.iso5807`,
   later `uml.strict`), each a list of rule ids; the user picks one or several per document;
   an override changes a rule's severity or disables it; a suppression silences one issue key
   with a reason. `rules::live` and `rules::batch` read the profile first and run nothing when
   it is off.
2. A built-in `drawing` pack with no glyphs, only rules any page can enable without a notation:
   connector crosses shape (reads `autoroute::crossings`), unglued connector end, overlapping
   shapes, shape outside the board frame, text overflowing its block.
3. The reasoner: a fact index built once per run from the model and packs: type hierarchy
   (`ElementType.extends`, so a type list in a check matches subtypes), transitive containment,
   inverse relationships, and reachability along a relationship type. Checks consult the index.
4. New checks in the vocabulary, each data with parameters: `forbiddenCycle` (along a
   relationship type), `requiredPath` (from type A to type B along a type), `connected` (no
   islands among these types), `containment` (which types may own which), `portDirection`
   (output to input only; a circuit and FBD need it), `portFanIn` (at most n drivers),
   `uniqueProperty` (within an owner), `propertyCompare` (one property against another or a
   constant: design pressure at least operating pressure), `nameFormat` (a pattern), and
   `crossesShape`, `ungluedEnd`, `overlap`, `offBoard` for the drawing pack.
5. Issues gain `fixes: [{label, command, params}]` so the future Problems tab and an agent can
   repair (reroute, glue to nearest, add the missing property); `RuleDecl` gains `reference`
   (the standard and clause the rule comes from) and `validate.explain {rule}` returns it.
6. Packs load from disk as well as the binary: `notation.load {path}` and a `packs` folder next
   to the preferences, so a new morphology can be dropped in and tried; each pack may ship
   `tests/*.json` cases (a document and the issue keys expected) that a notation test and
   `vizcraft-cli validate <file> [--ruleset id]` run, which is how rules get added and tested
   one at a time later.
7. Navigation without a pane: `validate.next` and `validate.previous` select the next issue's
   items on their page, so the Process tab is usable before Block 3 builds the Problems dock.

Proof: the Flowchart pack declares its ruleset and its tests pass; a free-drawing document
reports nothing until the drawing ruleset is enabled, then reports a crossing; a suppression
hides exactly one issue; `validate.explain` returns the clause; a disk pack loads and its test
cases run; `hostile_params_never_panic` stays green over the new commands.

### Branches

Wave 1 runs the first five at once and starts `b2c-menus` when the first branch reports. Wave 2
runs after the main session has merged wave 1. Ports 8021 to 8028; worktrees under
`.claude/worktrees/<branch>`.

| Branch | Model | Own | Commands |
|---|---|---|---|
| `b2c-router` (8021) | Opus | `crates/routing`, `crates/diagram/src/{autoroute.rs, route.rs, jumps.rs}`, `crates/ui-egui/src/canvas/draw/connectors.rs`, `crates/ui-egui/src/canvas/interact/drag.rs`, new `crates/engine/tests/routing_invariant.rs`; `Page` gains `routing: RoutingPolicy` (own struct in new `diagram/src/routing_policy.rs`) | `page.setup` gains `routing: {avoid, straightFallback}` (added in the main merge; the branch exposes `cmd::page::apply_routing` in its own file `cmd/routing.rs` with `page.setRouting`) |
| `b2c-board` (8022) | Sonnet | `Page::frame` and `Page.board` in new `diagram/src/board.rs`; every consumer listed above in `canvas/{mod.rs, draw/mod.rs, draw/overlays.rs, rulers.rs}`, `formats/src/{svg.rs frame only, png.rs, pdf.rs}`, `formats/src/drawio/write.rs`, `engine/src/cmd/{page.rs, print.rs, view.rs}`, `ui-egui/src/page_setup.rs`, `ui-egui/src/control.rs` | `page.setup {board}`, `file.new` creates infinite pages |
| `b2c-compartments` (8023) | Opus | new `diagram/src/compartment.rs`, `Shape.compartments` and `End::Glued.port`, `diagram/src/hit.rs`, `canvas/draw/{shapes.rs, text.rs}`, `canvas/interact/edit_text.rs`, new `formats/src/svg/compartments.rs` and `pdf/compartments.rs` (one call line each in `svg.rs` and `pdf.rs`), `formats/src/drawio/{read.rs, write.rs}` ext round trip, new `engine/src/cmd/compartment.rs`, `engine/src/cmd/text.rs` `compartment` param, `notation/src/schema.rs` `ElementType.compartments` and `bind.rs` row sync | `compartment.add`, `compartment.set`, `compartment.remove`, `compartment.move`; `text.*` take `compartment` |
| `b2c-connector-kinds` (8024) | Sonnet | new `diagram/src/arrows.rs` (the `Arrow` enum moves here with the new ends and one geometry table; `route.rs` keeps a one-line re-export), `Connector.kind`, `notation/src/schema.rs` `ConnectorType` and `Pack.connector_types`, `notation/packs/{basic,flowchart}.json`, `notation/src/bind.rs` kind precedence, new `stencils/src/connectors.rs`, new `engine/src/cmd/connector_kind.rs`, end drawing in `formats/src/{svg.rs arrowheads only, pdf.rs arrowheads only}` and `canvas/draw/connectors.rs` arrowhead function only (coordinate with `b2c-router`: arrowheads are one function; merge router second) | `connector.add {kind}`, `connector.setKind`, `connector.kinds` |
| `b2c-conformance` (8025) | Opus | `crates/notation/src/{rules.rs, rules_tests.rs, schema.rs rulesets and reference, loader.rs disk packs}`, new `notation/src/{infer.rs, drawing.rs}`, new `notation/packs/drawing.json` and `notation/packs/tests/`, `crates/model/src/issue.rs` fixes, `Document.conformance` in new `diagram/src/conformance.rs`, `engine/src/cmd/{validate.rs, notation.rs}`, `apps/vizcraft-cli` `validate` subcommand | `validate.run` and `validate.list` read the profile; `validate.setProfile`, `validate.suppress`, `validate.explain`, `validate.next`, `validate.previous`, `notation.load`, `notation.rulesets` |
| `b2c-mcp-detail` (8026) | Sonnet | new `engine/src/cmd/describe.rs`, `crates/mcp/src/{tools.rs, resources.rs, server.rs, diagram.rs, tests.rs}` | `doc.describe`, `page.describe`, `shape.describe`; MCP `doc_inspect {detail}`, `command_list {brief}`, resource `?detail=` |
| `b2c-menus` (8027) | Sonnet | `crates/ui-egui/src/ribbon/**` (checked state only, no reorganisation), `widgets.rs`, `menus.rs` (shared `check_mark` only), `control.rs` menu rows in `ui.inspect` | none |
| `b2c-ribbon` (8028, wave 2) | Sonnet | `crates/ui-egui/src/ribbon/**`, `icons.rs` for the new controls, `dialogs.rs` Page Setup Board row | none; wires every command above |

Shared files: each wave 1 branch may add fields to its own struct in `diagram/src/model.rs`
(`Page.routing`, `Page.board`, `Shape.compartments`, `Connector.kind`, `Document.conformance`,
`End::Glued.port`) and one test each to `formats/src/native_tests.rs`; the main session merges
those. `engine/src/session.rs` is not edited by anyone: `b2c-router` changes the signature of
`autoroute::refresh` to take `before` and the main session updates the one call line;
`b2c-conformance` gates inside `rules::live`. Merge order: menus, mcp-detail, conformance,
board, connector-kinds, router (last, because it owns the connector drawing file that
connector-kinds touches at one function), then compartments (it adds `End::Glued.port`, which
router's end resolution must then honour; the main session makes that one change and re-runs the
routing invariant test).

Done when for the block: the sample document, a UML-style class diagram with compartments and
hollow-triangle ends, and a P&ID-style sketch with tags and a bus kind can each be built over
the control channel on an infinite board with avoidance on and show no crossing; the Design,
Connector and Process tabs show every setting above with its check mark; the MCP structure
description of the sample document fits the byte budget; `validate.run` with the drawing
ruleset finds a planted crossing and `validate.setProfile {enabled: false}` silences it;
`cargo xtask ci` is green.

### Briefs

Each brief below is pasted after the template above with the shared lines filled in
(worktree `.claude/worktrees/<branch>`, branch `<branch>` from main at the commit that added this section,
`CARGO_TARGET_DIR=target-<branch>`, the port from the table, prefix `2C:`).

`b2c-router` (Opus). Goal: connectors never cross shapes. Make the avoiding orthogonal polyline
the single route every style renders, with side choice inside the search, a membership-aware
obstacle set, incremental rerouting, an audited fallback and a live drag preview, as described
under "Connectors cross shapes" in docs/cascade-plan.md (read that section; it is the
specification). Router API: `End` gains `candidates: Vec<(Point, Side)>` for dynamic glue and
`route_all` runs one multi-source, multi-target search per request; `Route` reports the chosen
start and end candidate index. `autoroute::refresh(doc, before)` reroutes only connectors
touched by changed bounds; `autoroute::crossings(page)` is the oracle; `autoroute::preview(page,
moving_ids, budget)` serves the drag. Straight and Curved follow rule 4. `Page.routing`
defaults to avoid always, straight detour. Done when: `crates/engine/tests/routing_invariant.rs`
passes (five hundred random cases, the sample, the three imports, every `layout.full`
algorithm); `cargo test -p vizcraft-routing` has a test per rule; a drag snapshot shows no
crossing; `cargo xtask ci` green. Report the one-line `session.rs` change you need.

`b2c-board` (Sonnet). Goal: an infinite board as the default page, as under "A fixed sheet".
Add `Board` and `Page::frame` in `diagram/src/board.rs`; replace every read of `page.width`
and `page.height` outside `board.rs` and `page.setup` with `frame()` (grep the list in the plan;
leave none behind, a test greps for it); draw no sheet, shadow or page breaks on an infinite
board; rulers from the origin; `view.zoomFit` and `initial_pan` fit the frame; exports, print
and draw.io write use the frame; `page.setup {board}`; `file.new` and `page.add` default
Infinite; files without the key load Fixed; Page Setup dialog gets a Board row and greys sizes
when infinite. Done when: SVG and PNG goldens for both kinds, a v4 file loads Fixed,
`ui.inspect` shows `board`, `cargo xtask ci` green.

`b2c-compartments` (Opus). Goal: sections inside shapes, as under "No sections inside shapes".
Data in `diagram/src/compartment.rs` with sanitising caps (at most 32 compartments, 256 rows,
text caps as `Shape.text`); layout function returning each band's rectangle for drawing, hit
testing and export to share; connection points appended after the kind's defaults and
addressable by `End::Glued.port = "<compartment id>.left|right"`; slot membership through
`parent` with the container clamp contract; per-band in-place editing; SVG and PDF modules;
draw.io `ext` round trip; `ElementType.compartments` schema with `bind` filling list rows from
named properties. Done when: the round-trip, export, hit and slot tests in the plan pass;
`hostile_params_never_panic` green over the new commands; `cargo xtask ci` green.

`b2c-connector-kinds` (Sonnet). Goal: named connector kinds and the notation line ends, as
under "No connector kinds". Move `Arrow` to `diagram/src/arrows.rs` with the eight new ends and
one geometry table (`fn outline(arrow, tip, dir, line_width) -> (Vec<PathEl>, filled: bool,
trim: f64)`) that the canvas, SVG and PDF all draw from; `ConnectorType` in the pack schema with
the Basic and Flowchart entries listed in the plan; `Connector.kind`; `connector.add {kind}`,
`connector.setKind`, `connector.kinds`; `bind::sync` kind precedence; a `stencils` connector
catalog with search. Done when: a golden SVG shows every end at both ends; kinds round-trip
natively and through draw.io; the Flow kind test passes; `cargo xtask ci` green.

`b2c-conformance` (Opus). Goal: the conformance engine groundwork, as under "A validator nobody
chose", items 1 to 7. The profile lives in `diagram/src/conformance.rs` and is read by
`rules::live` and `rules::batch` before anything runs; the reasoner is `notation/src/infer.rs`
(a `Facts` index built once per run); the new checks extend `Check` and each has a unit test
in `rules_tests.rs` using the `test_doc` helpers; the drawing pack is `packs/drawing.json`
with its checks in `notation/src/drawing.rs` (crossing via
`vizcraft_diagram::autoroute::clear_of_shapes` as it is on main); `Issue.fixes`; `RuleDecl.reference`;
disk packs through `loader` with the same caps as built-in ones and `notation.load`; pack test
cases under `packs/tests/` run by a notation test and by `vizcraft-cli validate`. Done when:
the proofs listed in the plan pass; `cargo xtask ci` green.

`b2c-mcp-detail` (Sonnet). Goal: agents read structure, not colours, as under "Agents pay for
colours". `cmd/describe.rs` builds the three detail levels and the text outline from the
document (no UI); `vizcraft://page/{id}` and `vizcraft://shape/{id}` default to structure and
honour `?detail=`; `doc_inspect {detail}`; `command_list {brief}`; trim the tool descriptions
and rewrite `INSTRUCTIONS` to send agents to `describe` first. Done when: the byte-budget test
(structure of the sample document under 4 KB, no `fill`, `line`, `font`, `style`, `shadow`,
`gradient` keys), headless MCP tests for both levels and the text format, `cargo xtask ci`
green. Report the docs/mcp.md rows.

`b2c-menus` (Sonnet). Goal: every dropdown shows its current choice with a check mark on the
right, as under "Dropdowns do not show their state". `MenuEntry` becomes a struct with a const
constructor so existing tuples change mechanically; `menu_item` and `combo` paint the mark
right-aligned; every menu with a current value declares `checked`; the Size menu loses
"(current)"; `ui.inspect` lists open menu rows with `checked`. No reorganisation of groups.
Done when: snapshot tests of each open menu, an inspect test, `cargo xtask ci` green.

`b2c-ribbon` (Sonnet, wave 2, after the merge). Goal: organise the new settings as under
"Settings scattered, Orientation too big", wiring the commands that wave 1 added by id; new
icons drawn in `icons.rs` for Board, Kind, Rules, Live Checking, Next and Previous Issue,
Compartment. Done when: screenshots of Design, Connector, Insert, Process and Home at 1280 px
with no overflow; every control runs by id; `cargo xtask ci` green.

## Block 3: workspace (ROADMAP Cascade 4B, 5 branches)

State: planned. Briefs written on 2026-10-18 against main at c291529 (Block 2C merged). Runs
alongside Block 4 under the five-agent cap: Block 3's five branches start first and a Block 4
branch starts each time one of them reports.

Cascade 4B as ROADMAP.md states it, with the Visio items folded in: Drawing Explorer becomes the
model tree's Diagrams branch and the Issues window becomes the Problems tab. The block consumes
Block 2C: the Problems tab reads the conformance profile and runs `Issue.fixes`, the scoped palette
lists connector kinds from `connector.kinds`, hover suggestions rest on the legal-pair table and
the Flow kind, and the Shapes pane is rebuilt so a pack loaded from disk appears in it (today
`vizcraft_stencils::sets()` is built once per process).

What exists to build on: `model.tree`, `model.views`, `model.move`, `model.delete`,
`element.show`, `element.deleteFromView`, `element.rename` and `model.setViewpoint` in
`crates/engine/src/cmd/{model.rs, element.rs}`; `Page.view` (a `View { viewpoint, layout }`, the
viewpoint id carrying its pack as a prefix); `Pack.palette`, `Pack.viewpoints`, `Pack.audience` and
`ElementType.properties` in `crates/notation/src/schema.rs`; `legal::legal_from`,
`legal::relationship_for` and `legal::is_legal`; `Registry::is_free` in `crates/model/src/identity.rs`;
`Session.issues`, `validate.*`, `Issue.fixes` and `Session::transactions()`; the pane registry
(`panes/mod.rs`: one file per pane, one line in `default_panes!`, one `ui.pane.<key>` row in
`UI_COMMANDS`); `canvas::insert_master`, `CanvasState.hover_shape` and the `Tool` enum; the
command palette (`Dialog::Commands` in `dialogs.rs`, `ui.commands`, Mod+Shift+P).

Models: `b3-insert` runs on Opus (placement geometry, interaction design and the suggestion
engine); the other four are UI wiring over commands that already exist, so they run on Sonnet. All
at medium effort.

### Branches

Ports 8031 to 8035; worktrees under `.claude/worktrees/<branch>`; branches from main at the commit
that added this section.

| Branch | Model | Own | Commands |
|---|---|---|---|
| `b3-tree` (8031) | Sonnet | new `crates/ui-egui/src/panes/model_tree.rs` (pane key stays `navigation`, title "Model Tree", so `view.navigationPane`, `ui.pane.navigation` and saved layouts keep working); `panes/navigation.rs` is deleted | UI only: `ui.tree {mode?: "model" or "context", branch?: "model" or "diagrams", expand?: [ids], collapse?: [ids], reveal?: id}`; `ui.inspect` gains `tree` |
| `b3-palette` (8032) | Sonnet | `crates/ui-egui/src/panes/shapes.rs` rewritten as the scoped palette (key stays `shapes`, title "Palette"), `crates/ui-egui/src/backstage.rs` (New from notation), new `crates/ui-egui/src/ribbon/notation/` (one contextual tab per active pack) and `ribbon/model/` (Model tab), `crates/stencils/src/lib.rs` (sets rebuilt when the catalog changes) | UI only: `ui.palette {query?, group?, viewpointFilter?: bool}`; `ui.shapesSearch` keeps working; `ui.inspect` gains `palette` |
| `b3-inspector` (8033) | Sonnet | `crates/ui-egui/src/panes/shape_data.rs` becomes `panes/inspector.rs` (key stays `shapeData`, title "Inspector"; `panes/properties.rs` is absorbed as the Document section and deleted), `crates/ui-egui/src/ribbon/data/show_hide.rs` (labels), new `crates/engine/src/cmd/inspector.rs` | `inspector.get {id}`, `element.checkTag {id, tag}`; UI `ui.inspector {mode?: "standard" or "expert", tab?: "properties" or "format"}` |
| `b3-dock` (8034) | Sonnet | new `crates/ui-egui/src/dock/` (`mod.rs` layout and tabs, `problems.rs`, `search.rs`, `activity.rs`, `tables.rs` and `generate.rs` as labelled empty slots), `crates/ui-egui/src/ribbon/process/diagram_validation.rs` (Problems button; Check opens the dock) | UI only: `ui.dock {open?: bool, tab?: "problems" or "search" or "activity" or "tables" or "generate", height?}`, `ui.search {query, scope?}`; `ui.inspect` gains `dock` |
| `b3-insert` (8035) | Opus | new `crates/engine/src/suggest.rs` and `crates/engine/src/cmd/suggest.rs`, new `crates/ui-egui/src/canvas/interact/insertion.rs` and `canvas/draw/insertion.rs`, `crates/ui-egui/src/canvas/interact/keys.rs` (Alt+Shift+Arrow), new `crates/ui-egui/src/insert_picker.rs` and `command_palette.rs` (the `Dialog::Commands` body moves here) | `suggest.next {id, direction?}`, `suggest.connect {from, to}`, `suggest.search {query, kinds?}`, `insert.next {id, type?, direction, relationship?, text?}`, `insert.connect {from, to, relationship?}`; UI `ui.insertPicker {x?, y?, query?}`; `ui.commands` gains Mod+K as a second shortcut |

Shared files, one line each unless said otherwise, merged by the main session: `panes/mod.rs`
(`default_panes!`: `b3-tree` swaps `navigation` for `model_tree`, `b3-inspector` swaps
`shape_data` for `inspector` and removes `properties`); `lib.rs` (`UI_COMMANDS` rows; `UiState`
gains `dock` from `b3-dock` and `inspector` from `b3-inspector`; `CraftApp::ui` gains the
`dock::show` call between `panes::show` and `canvas::show`); `control.rs` (`ui.inspect` fields);
`ribbon/mod.rs` `tabs()` (two tabs from `b3-palette`); `engine/src/lib.rs` (`pub mod suggest;`)
and `engine/src/cmd/mod.rs` (`inspector`, `suggest`); `dialogs.rs` (two lines from `b3-insert`:
the `Dialog::Commands` arm calls `command_palette::show`); `icons.rs` is append-only (one match
arm per new icon, keep-both at merge). `ribbon/view/show.rs` belongs to `b4-view`: `b3-dock`
reports the Dock toggle line it wants there instead of editing it. Merge order: tree, inspector,
palette, dock, insert (it touches the most canvas code); snapshots regenerated once after the
last merge.

Done when for the block: a new Flowchart document opened from the backstage's New page shows the
model tree, the scoped palette grouped as the pack's palette says, the Inspector typed by the
element schema and a Problems tab that jumps to the offending shape; over the control channel a
chain of five `insert.next` steps from a Terminator builds a flowchart that `validate.run` finds
nothing wrong with and `autoroute::crossings` finds empty; a planted crossing appears in the
Problems tab and its Fix button reroutes it; a pack loaded with `notation.load` appears in the
palette without a restart; `ui.inspect` reports tree, palette, inspector and dock state; `cargo
xtask ci` is green.

### Briefs

Each brief below is pasted after the template above with the shared lines filled in (worktree
`.claude/worktrees/<branch>`, branch `<branch>` from main at the commit that added this section,
`CARGO_TARGET_DIR=target-<branch>`, the port from the table, prefix `B3:`).

`b3-tree` (Sonnet). Goal: the model tree pane that takes over the left dock, as ROADMAP Cascade
4B describes it. Two branches at the root: Model (the containment tree from `model.tree`, root
elements grouped by type when the model has more than one type, each element row showing its
name, tag and the pages it appears on from `model.views`) and Diagrams (every page, its shapes
and connectors by text or id, the Drawing Explorer of the TODO). Context mode follows the canvas
selection: the tree expands to and highlights the selected items' elements, and the Model branch
filters to their owners and neighbours. Clicking a row selects its items on their page (switching
the active page with `page.setActive` first); double-click renames through `element.rename`
(inline editor); drag an element onto another to re-own it with `model.move`; a right-click menu
offers Show in All Views (`model.views`, then select), Delete from View (`element.deleteFromView`)
and Delete from Model (`model.delete`, with a confirmation dialog since it removes every view);
the pane header has Model and Context mode buttons with the check mark convention. Free drawing
(no model) shows only the Diagrams branch. Keep the pane key `navigation` so nothing else
changes; delete `panes/navigation.rs`. `ui.tree` sets mode, branch, expanded ids and reveals an
id; `ui.inspect` gains `tree: {mode, branch, expanded, selected}`. Cap the rows drawn per frame
(virtualise with `egui::ScrollArea::show_rows`) so a model of ten thousand elements stays smooth.
Done when: kittest tests that the sample's elements list under the Model branch grouped by type,
that the Diagrams branch lists the pages and their shapes, that a click selects and switches
page, that Context mode follows a selection change, that a rename runs `element.rename`; a
snapshot of the pane in light mode; `ui.inspect` test; `cargo xtask ci` green.

`b3-palette` (Sonnet). Goal: the scoped palette, New from notation and the notation and Model
ribbon tabs. Rewrite `panes/shapes.rs` as a compact list (icon, label, one row each, a tile grid
only for the Drawing group) with a search field at the top: when the active page has a view, the
pack's `palette` groups come first (types filtered by the page's viewpoint unless
`viewpointFilter: false`), then a Connectors group listing that pack's connector kinds from
`connector.kinds` plus the Basic ones, then one Drawing group at the bottom with the Basic
primitives; with no view, every pack's groups appear collapsed under the pack name with Drawing
open. Click inserts at `canvas::insert_point`, drag inserts where dropped (keep `MasterDrag`);
a connector kind row sets the connector tool's kind (`ui.tool {tool: "connector", kind}`, a new
param kept in a `CraftApp.connector_kind` field and passed to `connector.add` where the connector
drag commits in `canvas/interact/drag.rs`: one line there, which `b4-shapeops` owns, so report it
if it grows). Search covers types, kinds and Drawing masters, best first. Fix the stale stencil cache:
`vizcraft_stencils::sets()` rebuilds when `vizcraft_notation::catalog()` returns a different
catalog (compare pointer identity; keep the leaked strings bounded by the number of pack loads),
so `notation.load` shows up in the palette and in `shape.catalog`. Backstage New page: packs
grouped by `audience` with name and description; choosing one runs `file.new` then
`model.setViewpoint {viewpoint}` with the pack's first viewpoint, and the palette follows.
`ribbon/notation/`: one contextual tab built from data, named after the active page's pack,
visible when the page has a view, with an Insert group (one menu per palette group, items run
`shape.add {stencil}` through `vizcraft_notation::catalog().master_for_type`), a Viewpoint menu
(check marks, `model.setViewpoint`) and a Check button (`validate.run`). `ribbon/model/`: a Model
tab with a Model group (Model Tree pane toggle, Show in All Views, Move in Model, Delete from
Model, Delete from View). Drill-down breadcrumbs: a path bar above the canvas (document title,
page name, and the owner element when a page's view name matches an element's child view
convention `<element id>/<viewpoint>`; plain pages show two crumbs) that switches pages on click;
put it in `panes/shapes.rs`'s sibling file `crates/ui-egui/src/breadcrumbs.rs` (new, yours) with
one call line in `canvas/mod.rs` above the page strip. Done when: kittest tests that a Flowchart
page shows Steps, Data, Links, Connectors and Drawing groups in that order; that the viewpoint
filter hides a type outside the viewpoint; that a disk pack loaded in the test appears; that New
from notation gives a page with the viewpoint set; snapshots of the palette and the notation tab
at 1280 px; `cargo xtask ci` green.

`b3-inspector` (Sonnet). Goal: the typed Inspector grown from the Shape Data pane. Rename the
file and title, keep the key. Sections, each collapsible: Identity (name through
`element.rename`, tag through `element.retag` with a live check as you type that calls
`element.checkTag` on every change and colours the field red with the conflicting element named
when the tag is taken), Properties (for a model element: one editor per `ElementType.properties`
entry typed by `PropKind` with unit, enum values and required marker, through
`element.setProperty`; shape data rows as today through `data.*`; page keys at the end),
Relationships (every relationship the element takes part in: type label, direction arrow, the
other element's name; click selects the connector drawing it on this page or shows the page it is
on), Views ("Appears in N views", each page named, click switches), Shape (kind, stencil, text,
position and size through `shape.*` and `arrange.setBounds`), and the Document section the
Properties pane held (title, author, revision; delete `panes/properties.rs`, keep its
`ui.pane.properties` row pointing at the Inspector's Document section). Two modes, persisted in
`UiState.inspector`: Standard hides ids, `ext` keys and read-only model fields; Expert shows them.
A Format secondary tab at the top switches the body to fill, line and text controls that run
`shape.setStyle` and `text.setFormat`. `inspector.get {id}` in `cmd/inspector.rs` assembles
everything the pane shows in one call (`{element?, type?, identity: {tag, format, free, conflict?},
properties: [{key, label, kind, unit, values, required, value, source}], relationships: [{id, type,
direction, other, otherName, pages}], views: [{page, pageName, items}], shape: {...}}`) so agents
and the pane read the same thing; `element.checkTag` returns `{free, scope, conflict?}` from
`Registry::is_free`. Multiple selection edits the shared rows as today. Done when: engine tests
for both commands (the Flowchart sample: a taken tag reports its conflict, a free one is free;
properties carry their schema; views count matches `model.views`); kittest tests that the pane
shows typed editors for a flowchart Process, that typing a taken tag colours the field, that
Expert mode shows the id; snapshots of both modes; `hostile_params_never_panic` green; `cargo
xtask ci` green.

`b3-dock` (Sonnet). Goal: the bottom dock with Problems, Search, Activity and the Tables and
Generate slots. `dock/mod.rs` holds `DockLayout {open, height, tab}` (persisted in
`UiState.dock`, sanitised like `PaneLayout`), a tab strip with a close button, a draggable
splitter between the dock and the canvas, and `show` drawn in `CraftApp::ui` after the side
panes so the dock sits between them. Problems: one row per issue, merging the live
`Session.issues` with the last `validate.run` result by key; columns severity (icon), message,
rule, page, items; a severity filter and a text filter; click selects the items on their page
(`page.setActive`, `edit.select`), the row's Explain button shows `validate.explain` (rule,
reference, message) in a popover, Suppress runs `validate.suppress {key, reason}` after a
one-line reason field, and each `Issue.fixes` entry is a button that runs its command and params
as one transaction; a toolbar with Check (`validate.run`), the Rules menu (the same entries and
check marks as Process, through `validate.setProfile`), Live Checking toggle, Next and Previous;
when the profile is off the tab shows "Checking is off" with a Turn On button. Search: a query
field over `edit.find {query, scope}` with scope Page or Document, results as rows (page, item,
text, data key), click selects, a Replace button opens the Find dialog with the query. Activity:
`Session::transactions()` newest first with label, origin badge (ui, control, mcp, script),
command count and an expandable list of ids, and an "Undo to before this" button that runs
`edit.undo` once per record from the newest down to and including the chosen one (enabled only
while every one of them is still on the undo stack). Tables and Generate: empty tabs saying
which block fills them. The dock opens itself the first time Check runs from the ribbon
(`ribbon/process/diagram_validation.rs` gets a Problems button and Check sets `ui.dock {open:
true, tab: "problems"}` after `validate.run`). `ui.dock` and `ui.search` as in the table;
`ui.inspect` gains `dock: {open, height, tab, rows}`. Done when: kittest tests that a planted
crossing on a page with the drawing ruleset appears as a row, that clicking selects the
connector, that the Fix button clears the issue, that Suppress hides exactly one row, that
Search lists a text hit and selects it, that Activity lists a control-channel batch and undoes
it; snapshots of each tab; `ui.inspect` test; `cargo xtask ci` green. Report the one line you
want in `ribbon/view/show.rs` (Dock toggle) and the UI_COMMANDS rows.

`b3-insert` (Opus). Goal: insertion on the canvas, as ROADMAP Cascade 4B describes it, over a
suggestion engine in the engine. `suggest.rs`: `next(doc, shape_id) -> Vec<Suggestion {type,
label, relationship, kind}>` from `legal::legal_from` on the shape's element type (every legal
target type with its relationship type; for a free shape, the same stencil joined by the Basic
Arrow kind), ordered by how often that pair already occurs on the page then by palette order;
`connect(doc, from, to) -> Vec<RelationshipChoice {id, label, legal}>` from the connection table
(illegal pairs listed last and marked, since a rule reports them); `search(doc, query, kinds)`
over commands (the registry and `UI_COMMANDS`), element types of the packs in use, elements by
name or tag, and later patterns. `insert.next` is one transaction: create the element and its
shape one step away in `direction` (step = the source's size plus the page's layout gap, pushed
clear of overlaps the way `layout.incremental` places a new shape, never crossing the frame of a
container the source is not in), create the relationship and its connector glued to both (the
pack's kind for that relationship type, else Arrow), reroute, return the ids; `insert.connect`
creates the relationship and connector between two existing shapes. Canvas: hover handles (four
small arrows outside the hovered shape's sides after 300 ms, drawn in `draw/insertion.rs`); a
click on one runs `insert.next` with the single suggestion, or opens a popover listing the
suggestions when there are several (type icon, label, relationship); dragging a handle onto
another shape is the magic connector (drop opens the `connect` choices, one legal choice applies
at once), dragging onto empty canvas drops a new shape there through `insert.next` with the
popover; the ghost of the shape and connector draws while dragging. Double-click on empty
canvas opens the insert picker (`insert_picker.rs`: a type-to-filter list of the page's palette
types and the Drawing primitives, Enter inserts at the point, Escape closes). Alt+Shift+Arrow
with one shape selected runs `insert.next` in that direction with the first suggestion and
selects the new shape, so a chain can be typed. The command palette moves to
`command_palette.rs` and reads `suggest.search`: commands run, types insert at the canvas
centre, elements select and reveal (switching page); Mod+K joins Mod+Shift+P. Done when: engine
tests that `suggest.next` on a flowchart Process lists only Flowchart types with Flow, that
`insert.next` yields a glued, bound, non-overlapping shape with `crossings` empty, that a chain
of five steps validates clean, that `insert.connect` between two shapes with no legal pair still
connects and raises the rule's issue, that `search` finds an element by tag; kittest tests for
the handles appearing, a handle click inserting, Alt+Shift+Right chaining, the picker inserting,
the palette selecting an element; a snapshot with handles visible; `hostile_params_never_panic`
green; `cargo xtask ci` green.

## Block 4: Visio extras (7 branches, 5 agents at a time, alongside Block 3)

State: planned. Briefs written on 2026-10-18 against main at c291529. Branches start as Block 3
branches report, in the order of the table below; `b4-pictures` starts only after `b4-shapeops`
is merged, because ink strokes and boolean results are both freeform path shapes, which
`b4-shapeops` adds. `b4-fixes` is new in this block: it holds the Block 2C follow-ups that no
other branch owns.

Decisions on the two questions the plan left open:

| Question | Decision |
|---|---|
| CAD drawing insert (`.dwg`, `.dxf`) | `.dwg` is dropped for good: it has no public specification, and the only open reader is GPL, which AGENTS.md forbids. `.dxf` is deferred, not built in Block 4: an SVG or PNG exported from the CAD tool inserted as a picture (`b4-pictures`) covers the underlay use today. If the industrial track in Block 7 wants a floor-plan underlay, add Autodesk's published DXF Reference to the Formats list in AGENTS.md then and read the LINE, LWPOLYLINE, CIRCLE, ARC and TEXT entities only. The CAD Drawing Tools contextual tab goes with it. |
| Pan and Zoom pane in `b4-view` | Kept. On an infinite board it is a minimap of `Page::frame()` (the drawing's bounds) with the visible area as a draggable, resizable rectangle; on a fixed board it frames the sheet. Hidden by default, opened from View > Show > Task Panes. It finishes ROADMAP Cascade 2. |

Models: `b4-shapeops` (boolean geometry and the freeform path), `b4-pictures` (image drawing,
crop, ink recognition) and `b4-datagfx` (a layout of graphics around shapes shared by canvas and
SVG) run on Opus; `b4-fixes`, `b4-links`, `b4-view` and `b4-review` are precise sweeps and UI
wiring and run on Sonnet. All at medium effort.

### Branches

Ports 8036 to 8042; worktrees under `.claude/worktrees/<branch>`; the first five branch from main
at the commit that added this section, `b4-pictures` and `b4-datagfx` from the main commit the
main session names when they start.

| Branch | Model | Own | Commands |
|---|---|---|---|
| `b4-fixes` (8036) | Sonnet | `crates/engine/src/cmd/{lane.rs, container.rs, arrange.rs, design.rs}`, `crates/engine/src/fragment.rs`, `crates/diagram/src/{decor.rs, autolayout.rs, autoroute.rs}`, `crates/diagram/tests/board_reads.rs`, `crates/formats/src/drawio/write.rs`, `crates/ui-egui/src/canvas/interact/select.rs` (tests only) | none new; `design.setBackground` and `design.setBorderTitle` default to the active page |
| `b4-shapeops` (8037) | Opus | `Shape.path` and `Shape.name` in `crates/diagram/src/shape_fields.rs`, `ShapeKind::Freeform` in `crates/diagram/src/kind.rs`, new `crates/diagram/src/{shapeop.rs, master.rs}`, `Document.masters` and `Document.protection` in `crates/diagram/src/doc_fields.rs`, new `crates/engine/src/cmd/{shapeop.rs, master.rs, protect.rs}`, `crates/ui-egui/src/canvas/interact/drag.rs` and `canvas/draw/overlays.rs` (locks), `crates/ui-egui/src/ribbon/developer/`, new `crates/ui-egui/src/panes/document_stencil.rs` and `shape_name_dialog.rs` | `shapeop.union`, `shapeop.combine`, `shapeop.fragment`, `shapeop.intersect`, `shapeop.subtract`, `shapeop.offset {distance}`, `shapeop.join`, `shapeop.trim` (all `{ids?}`); `shape.setLocks {ids?, width?, height?, aspect?, position?, rotation?, delete?, select?, text?}`, `shape.setBehavior {ids?, container?, nonPrinting?, hidden?, noGlue?, noSnap?}`, `shape.rename {id, name}`, `doc.protect {shapes?, backgrounds?}`; `master.add {ids?, name}`, `master.list`, `master.remove {id}`, `master.insert {id, x?, y?}`, `master.export {path}`, `master.import {path}` |
| `b4-links` (8038) | Sonnet | new `crates/engine/src/cmd/{link.rs, comment.rs, process.rs}`, new `crates/ui-egui/src/panes/comments.rs`, `canvas/draw/links.rs`, `canvas/interact/links.rs`, `link_dialog.rs`, `ribbon/insert/links.rs`, `ribbon/review/comments.rs`, `ribbon/process/subprocess.rs` | `link.add {id, kind: url or file or page or shape or element, target, label?}`, `link.set {id, index, ...}`, `link.remove {id, index}`, `link.follow {id, index?}`, `shape.setScreenTip {ids?, text}`; `comment.add {page?, shape?, text}`, `comment.reply {id, text}`, `comment.edit {id, text}`, `comment.delete {id}`, `comment.list {page?}`; `process.createSubprocess {ids?, name?}`, `process.linkSubprocess {id, page}` |
| `b4-view` (8039) | Sonnet | `crates/engine/src/cmd/view.rs`, new `crates/engine/src/cmd/{addon.rs, macro.rs}`, new `crates/ui-egui/src/panes/pan_zoom.rs`, `presentation.rs`, `addon_dialog.rs`, `crates/ui-egui/src/ribbon/view/{show.rs, zoom.rs}` and new `ribbon/view/{views.rs, window.rs, macros.rs}`, `apps/vizcraft/src/{main.rs, windows.rs, control_server.rs}` | `view.zoomWidth {page?}`; `addon.numberShapes {ids?, start?, position?: before or after, separator?, renumber?}`, `addon.arrayShapes {ids?, rows, cols, dx, dy}`, `addon.moveShapes {ids?, dx?, dy?, distance?, angle?, copy?}`; `macro.save {name, script?}`, `macro.list`, `macro.run {name}`, `macro.delete {name}`; UI `ui.present {on?}`, `ui.window.new {same?}`, `ui.window.arrangeAll`, `ui.window.cascade`, `ui.window.switch {index}` |
| `b4-review` (8040) | Sonnet | new `crates/engine/src/proof.rs` and `cmd/{proof.rs, report.rs, document.rs}`, `assets/dictionaries/en-US.txt` with its `ATTRIBUTION.md` row, `Shape.language`, `Document.language` and `Document.ignored_words` fields, new `crates/ui-egui/src/{spelling_dialog.rs, info_page.rs}`, new `ribbon/review/{proofing.rs, language.rs, reporting.rs}` | `proof.check {ids? or page? or all?}`, `proof.suggest {word}`, `proof.ignore {word}`, `proof.setLanguage {ids?, language}`; `report.list`, `report.run {report: inventory or data or connections, scope: page or document, format: csv or html or table, path?}`; `doc.setProperties {title?, subject?, author?, keywords?, description?}`, `doc.inspectHidden`, `doc.removeHidden {what: [...]}` |
| `b4-pictures` (8041) | Opus | new `crates/engine/src/cmd/{picture.rs, ink.rs}`, `ImageRef.adjust` in `crates/diagram/src/shape_fields.rs`, new `crates/diagram/src/ink.rs`, new `crates/ui-egui/src/canvas/draw/pictures.rs`, `canvas/interact/{crop.rs, ink.rs}`, `ribbon/picture/`, `ribbon/ink/`, `ribbon/insert/illustrations.rs`, `ribbon/review/ink.rs`, new `crates/formats/src/svg/pictures.rs` | `picture.insert {path? or bytes?, mediaType?, x?, y?, w?, h?}`, `picture.change {id, path? or bytes?}`, `picture.crop {id, crop: {x, y, w, h}}`, `picture.adjust {id, brightness?, contrast?, transparency?, recolor?}`, `picture.reset {id}`, `picture.compress {ids? or all?, maxDimension?, quality?}`; `ink.add {points, width?, color?, highlighter?}`, `ink.erase {ids}`, `ink.toShapes {ids?}`; UI `ui.insertPicture`, `ui.tool` gains `crop`, `pen`, `highlighter`, `eraser` |
| `b4-datagfx` (8042) | Opus | new `crates/diagram/src/data_graphic.rs`, `Document.data_graphics`, `Document.data_links` and `Shape.data_graphic` fields, new `crates/engine/src/cmd/{datagraphic.rs, datalink.rs}`, new `crates/formats/src/table.rs` (CSV and XLSX table reading), new `crates/ui-egui/src/canvas/draw/data_graphics.rs`, `crates/formats/src/svg/data_graphics.rs`, `panes/external_data.rs`, `datagraphic_dialog.rs`, `ribbon/data/{external_data.rs, display_data.rs}` | `datagraphic.define {id?, name, items}`, `datagraphic.list`, `datagraphic.remove {id}`, `datagraphic.apply {ids?, graphic}`, `datagraphic.legend {page?, graphic?}`; `datalink.link {path, format?, sheet?, keyColumn, dataKey?, columns?}`, `datalink.list`, `datalink.refresh {id?}`, `datalink.bind {id, row, shape}`, `datalink.unlink {id}`; UI `ui.linkData` |

Shared files, merged by the main session: struct fields in `crates/diagram/src/model.rs`,
`shape_fields.rs` and `doc_fields.rs` as the table lists them, each with serde defaults,
sanitising and one test in `formats/src/native_tests.rs` (the Block 2C convention); `lib.rs`
(`UI_COMMANDS` rows; `Tool` variants from `b4-pictures`; `Services.open_external` from
`b4-links`; `CraftApp.window_request` and `presenting` from `b4-view`); `dialogs.rs` (two lines
per new dialog: the enum variant and its `show` arm); `panes/mod.rs` `default_panes!` (comments,
pan_zoom, document_stencil, external_data); `ribbon/mod.rs` `tabs()` (Picture and Ink from
`b4-pictures`; Developer exists and `b4-shapeops` fills it); `ribbon/<tab>/mod.rs` `groups!`
lines (Insert: links, illustrations; Review: comments, ink, proofing, language, reporting;
Process: subprocess; View: views, window, macros; Data: external_data, display_data);
`engine/src/cmd/mod.rs` and `engine/src/lib.rs` (`pub mod proof;`); `control.rs` (`ui.inspect`
fields; `engine.execute` gains `origin?` from `b4-view`); `icons.rs` append-only; workspace
`Cargo.toml` dependencies (`i_overlay` from `b4-shapeops`; `calamine` and `csv` from
`b4-datagfx`; the `raster-images` feature of `resvg` from `b4-pictures`), each MIT or Apache,
pure Rust, building for wasm or gated behind `cfg(not(target_arch = "wasm32"))` with an error
there. `backstage.rs` belongs to `b3-palette`: `b4-review` reports the one Info nav line.
`apps/vizcraft/src/main.rs` belongs to `b4-view`: `b4-links` reports the one line that fills
`Services.open_external`. Merge order: fixes, shapeops, links, view, review, pictures, datagfx;
snapshots regenerated once after the last merge.

Done when for the block: over the control channel the sample document gains a picture with a
crop and a brightness change, an ink stroke converted to a rectangle, two shapes unioned into
one, a hyperlink into a subprocess page that `link.follow` reaches and returns from, a comment
thread, a data graphic driven by a linked CSV that `datalink.refresh` updates, and an inventory
report on a new page; `proof.check` finds a planted misspelling; the Review, Developer, Data,
Picture, Ink and View tabs screenshot at 1280 px with no overflow; the Pan and Zoom pane pans
the canvas; `cargo xtask ci` is green.

### Briefs

Each brief below is pasted after the template above with the shared lines filled in (worktree
`.claude/worktrees/<branch>`, branch `<branch>` from main at the commit named in the Branches
paragraph, `CARGO_TARGET_DIR=target-<branch>`, the port from the table, prefix `B4:`).

`b4-fixes` (Sonnet). Goal: the Block 2C follow-ups that no other branch owns, each a precise
change with a test. (1) The sheet is read where the frame is meant: `lane.add`, `lane.addSet`
and `container.add` place a new item at `Page::frame().center()` when nothing is selected
(`cmd/lane.rs`, `cmd/container.rs`); `arrange.align {to: "page"}` and the other `to: page`
anchors use `frame()` (`cmd/arrange.rs`); generated border and title pages (`decor.rs`) decorate
the frame of an infinite board and `decor::refresh` redraws them when the frame changes (a moved
shape grows the border), while a fixed board keeps the sheet; `autolayout::grow_page` grows only
a fixed board and is a no-op on an infinite one; the two `page.rect()` reads in `select.rs` tests
become `frame()`; `crates/diagram/tests/board_reads.rs` loses the `decor.rs` and `autolayout.rs`
rows from `ALLOWED` and stays green. (2) `Fragment::insert` remaps `compartments[].members`
through the id map and drops members left behind (`fragment.rs`), so pasting a slot keeps its
members. (3) `drawio/write.rs` writes only interior bends: when `route::stored_route` (make it `pub`, it is
`pub(crate)` today) says the waypoints hold the whole polyline, the first and last points are left
out, and a reread gives the same drawing. (4) `design::page_index` defaults to the active page rather than index 0, so the
Design tab's Backgrounds and Borders act on the page shown. (5) The router's final stub is at
least the arrowhead's trim: `autoroute.rs` sets each request's stub length from
`arrows::outline` for the connector's ends at its line width, and `routing_invariant.rs` gains
the check that the last segment is at least that long. The ISO 5807 clause references are not
yours; they wait for Block 7. Done when: tests for each of the five (a lane set on an empty
infinite board lands at the frame centre; align to page on an infinite board uses the frame; a
border follows a moved shape on an infinite board and stays put on a fixed one; a pasted slot
has its members; a routed connector round-trips through draw.io with only interior bends;
`design.setBackground` with page 2 active decorates page 2; no arrow tip inside a stub);
`cargo xtask ci` green.

`b4-shapeops` (Opus). Goal: freeform geometry, boolean operations, locks and behaviour, shape
names, custom masters and the Developer tab. The freeform path: `Shape.path: Option<Freeform
{els: Vec<PathEl>, closed: bool}>` in local coordinates from (0, 0) to (w, h), sanitised and
capped at 4096 elements; `ShapeKind::Freeform` whose `outline(w, h)` is the stored path (the
unit square when none), so hit testing, the canvas, SVG, PNG and PDF draw it with no further
change; an open path draws no fill. Boolean operations in `diagram/src/shapeop.rs` through
`i_overlay` (MIT, pure Rust): union, combine (exclusive or), fragment (every piece), intersect,
subtract (the first selected minus the rest), offset by a distance; join (open paths end to end
within a tolerance) and trim (split at intersections) on open paths; the result takes the
first shape's style, text, data, layers and parent, the inputs are removed, fragment yields
several shapes, and every operation is one undo step; results round-trip natively and to draw.io
through `ext` (`vizcraftPath`). Locks and behaviour: `shape.setLocks` and `shape.setBehavior`
write the Block 1 fields; `cmd/protect.rs` holds `check(page, ids, Lock) -> Result` and
`doc.protect` writes `Document.protection {shapes, backgrounds}`, read by the same check; one
call line each in `shape.move`, `shape.resize`, `shape.rotate`, `shape.delete`,
`cmd::delete_items`, `connector.glue` (no_glue), `snap.rs` (no_snap) and `page.setBackground`
(backgrounds), reported if you cannot keep them to one line; the canvas drag (`drag.rs`) refuses
a move or resize the locks forbid and `overlays.rs` hides the handles for locked dimensions;
`select: true` makes a shape unselectable by click (still by id). `shape.rename` sets
`Shape.name`, shown by `describe` and the Inspector later. Custom masters:
`Document.masters: Vec<CustomMaster>` in `diagram/src/master.rs` (a `Fragment`-shaped snapshot
with a name and bounds), `master.*` as in the table, export and import as a JSON file with the
`.vizcraft-stencil` extension written by `master.rs` itself; the Document Stencil pane lists
them with previews, click inserts, a button adds the selection. Developer tab: Shape Design
group (Operations menu, Shape Name, Behavior menu with check marks, Protection menu with check
marks), Show/Hide group (Document Stencil pane toggle, Drawing Explorer runs `ui.pane.navigation`
then `ui.tree {branch: "diagrams"}`). Done when: engine tests (union of two overlapping
rectangles is one freeform whose area is the union within one percent; subtract leaves a hole
drawn even-odd; fragment of two overlapping rectangles gives three; offset grows the bounds by
the distance; join and trim as described; a freeform round-trips natively and through draw.io; a
position-locked shape refuses `shape.move`; `doc.protect {shapes: true}` refuses
`shape.delete`; a master from the selection inserts a copy with fresh ids and glue kept); a
golden SVG of a fragment result; a kittest test that a locked shape has no move drag; Developer
tab screenshot at 1280 px; `hostile_params_never_panic` green; `cargo xtask ci` green.

`b4-links` (Sonnet). Goal: hyperlinks, screen tips, comments and subprocesses over the Block 1
fields (`Shape.links`, `Shape.screen_tip`, `Document.comments`). `link.*` as in the table;
`link.follow` on a page, shape or element link switches page and selects (`element.show` for an
element with no shape on the active page); on a URL or file link it pushes
`{"request": "openExternal", "target"}` to `session.ui_requests`, which the UI hands to
`Services.open_external` (a new optional callback; report the `apps/vizcraft/src/main.rs` line
that fills it with the `open` crate or `std::process::Command`). Canvas: `draw/links.rs` draws a
small link glyph at the top right of a shape with links and comment markers at comment anchors
while the Comments pane is open; `interact/links.rs` shows the screen tip as a tooltip after a
hover delay and follows the first link on Mod+click (one call line each in `draw/mod.rs` and
`interact/mod.rs`). `link_dialog.rs` edits a shape's links (kind, target with a page or shape
picker, label). Comments: `comment.*` as in the table, author from `Session.author`, time from
`cmd::now_iso`; the Comments pane lists the active page's threads with reply fields, Previous and
Next select the anchor shape; Review > Comments has New Comment, Delete, Previous, Next and a
Show Comments toggle with its check mark. Subprocess: `process.createSubprocess` moves the
selected shapes and the connectors among them to a new page (a `Fragment` extract and insert,
then delete), puts one Predefined Process shape with the name and a page link where they were,
re-glues connectors from outside to it, and selects it, as one undo step;
`process.linkSubprocess` adds a page link to an existing shape; Process > Subprocess has Create
from Selection, Create New (an empty page plus the linked shape) and Link to Existing (a menu of
pages). Done when: engine tests (links round-trip; follow switches page and selects; a URL
follow pushes the request; a comment thread round-trips and lists per page; subprocess creation
moves, re-glues and links, and undo restores in one step); a kittest test with a recording
`open_external` service; snapshots of the Comments pane and the Review tab; `hostile_params_never_panic`
green; `cargo xtask ci` green.

`b4-view` (Sonnet). Goal: presentation mode, Page Width, the Pan and Zoom pane, Task Panes,
window arrangement, the add-ons and the Macros group. `view.zoomWidth` fits the frame's width to
the canvas. `presentation.rs`: `ui.present {on?}` hides title bar, menus, ribbon, panes, dock and
status bar and asks for full screen (`egui::ViewportCommand::Fullscreen`); Left, Right, PageUp and
PageDown switch pages, Escape leaves; `CraftApp::ui` consults it in one line. Pan and Zoom pane
(key `panZoom`, right side, hidden by default): the frame and every shape as filled rectangles,
the visible area as a draggable, resizable frame that sets `view.pan` and `view.zoom`; the pane
follows pan and zoom changes. View > Show gets a Task Panes menu (every pane with its check mark,
from `panes::window_menu_items`) and the Dock toggle `b3-dock` reports. View > Window: New
Window (`ui.window.new {same: true}` opens the same file in a new window; the two windows do not
share edits, and the status bar says so), Arrange All (tile), Cascade and a Switch Windows menu,
through `CraftApp.window_request` consumed in `windows.rs` with the monitor size from the
viewport info. Add-ons under View > Macros > Add-Ons with small dialogs: Number Shapes (text
prefix or suffix in z-order, `renumber` strips an existing number first), Array Shapes (rows by
columns of copies at the pitch, one undo step), Move Shapes (by offset or by distance and angle,
optionally copying). Macros: Record and Stop (over `script.recordStart` and `script.recordStop`,
the button toggling with a check mark), Save (names the script just stopped), a Run menu of saved
macros, Delete; `cmd/macro.rs` stores one JSON file per macro in the directory `macro::set_dir`
names (the app passes the `macros` folder beside the preferences file, `VIZCRAFT_MACROS_DIR`
overrides it for headless runs), names sanitised to a safe file stem, at most 256 macros and
64 KB each. Also fold in: `engine.execute` accepts `origin?: "mcp"` so the control server runs
it with `run_as(Origin::Mcp)`, and report the one line the MCP Remote backend needs to send it.
Done when: tests for `view.zoomWidth`, each add-on (numbering order and prefix, array count and
pitch, move by angle), a macro saved and run from a temporary directory, the pure window
arrangement function (positions for two and five windows); kittest tests that dragging the Pan
and Zoom frame pans and that presentation hides the chrome; snapshots of the pane, presentation
mode and the View tab at 1280 px; `cargo xtask ci` green.

`b4-review` (Sonnet). Goal: spelling, proofing language, the Info backstage page and shape
reports. Dictionary: a SCOWL-derived en-US word list (size 60) at `assets/dictionaries/en-US.txt`
with its `ATTRIBUTION.md` row (verify the licence text from the SCOWL distribution and cite it);
`engine/src/proof.rs` loads it with `include_str!`, indexes words by length and checks tokens of
letters and apostrophes, skipping words with digits, all-capitals words, `{{...}}` fields, URLs,
file paths and the document's `ignored_words`; suggestions are edit-distance one, then two when
none, capped at eight. `proof.check` walks shape texts, compartments and connector texts,
returning each misspelling with ids, offsets and suggestions; `proof.setLanguage` writes
`Shape.language` (a BCP 47 tag; `Document.language` is the default; `none` skips the shape;
only `en-US` has a dictionary, other tags are skipped with a note in the result). Review >
Proofing opens `spelling_dialog.rs` (Not in Dictionary, suggestions, Ignore, Ignore All, Change,
Change All, each change through `shape.setText` or `text.*`), Review > Language has a Proofing
Language menu with check marks. Info page (`info_page.rs`, reached from the backstage nav line you
report for `b3-palette`'s file): editable properties through `doc.setProperties`, statistics
(pages, shapes, connectors, elements, comments, images, data rows), Inspect for Hidden Data
(`doc.inspectHidden` counts comments, hidden shapes and layers, screen tips, data rows, images and
properties; `doc.removeHidden {what}` removes the chosen kinds in one undo step). Shape Reports:
`report.list` names the built-in reports (inventory: id, name, type, text, page; data: one column
per data key in use; connections: from, to, kind, relationship); `report.run` returns rows and
columns and, by format, writes a CSV file (a small quoting writer in `cmd/report.rs`), an HTML
file with one table and the document title, or an on-page table (a shape with the report name as
text and one list compartment with one tab-separated row per record, placed at the frame's right
edge); XLSX waits for Block 5. Review > Reporting has a Shape Reports menu (report, scope,
format). Done when: tests that `proof.check` flags "teh" with "the" among the suggestions and
skips fields, URLs and capitals; that an ignored word survives a native round-trip; that
`language: none` skips; a CSV golden for the sample's inventory; an HTML row count; the on-page
table's row count; inspect-hidden counts and removal in one undo step; `cargo xtask assets`
green with the new row; snapshots of the Info page and the Review tab; `cargo xtask ci` green.

`b4-pictures` (Opus; starts after `b4-shapeops` is merged). Goal: pictures and ink. Pictures:
`picture.insert` decodes PNG and JPEG headers for size, keeps SVG as text, stores the bytes once
in `Document.images` (deduplicated by content hash) and adds a Rectangle shape with a
transparent outline and `image: Some(ImageRef)` sized to the picture (scaled to a quarter of the
frame when larger); `ImageRef.adjust {brightness, contrast, transparency, recolor: none or
grayscale or sepia or a tint colour}` is new; `picture.crop` takes fractions of the source; the
canvas (`draw/pictures.rs`, one call line where `draw/shapes.rs` fills a shape) draws through a
texture cache keyed by asset, adjustments and scale, rasterising SVG with `resvg` at the drawn
size; SVG export (`svg/pictures.rs`, one call line in `svg.rs`) writes an `<image>` with a data
URI, a clip path for the crop and a filter for the adjustments, so PNG and PDF follow (enable
the `raster-images` feature of `resvg`); `picture.compress` downscales to `maxDimension` and
re-encodes opaque images as JPEG at `quality`. The crop tool (`Tool::Crop`, `interact/crop.rs`)
drags the crop rectangle's handles and commits `picture.crop` on release. The Picture contextual
tab (visible when a picture shape is selected): Adjust (Brightness and Contrast menus with steps
and check marks, Recolor menu, Transparency menu, Compress, Change Picture through
`pick_open("picture")`, Reset), Picture Styles (border weight and colour through
`shape.setStyle`), Size (Crop toggle). Insert > Illustrations has Picture (`ui.insertPicture`
picks a file then runs `picture.insert`). Ink: a stroke is a freeform open-path shape (from
`b4-shapeops`) with round caps and `ext["vizcraft.ink"]` set to `pen` or `highlighter`
(highlighter: width 12, line transparency 0.5, drawn behind other shapes); `ink.add` simplifies
the points (Ramer-Douglas-Peucker, tolerance 1.5 units, at most 4096 points), `ink.erase`
deletes strokes, `ink.toShapes` recognises a closed stroke whose points fit an ellipse within
15 percent as an Ellipse, one with four corners as a Rectangle, a nearly straight one as a Line
connector, and leaves the rest; the Pen, Highlighter and Eraser tools (`interact/ink.rs`) collect
a drag and commit one `ink.add` on release, the eraser deletes strokes under a click or drag.
Review > Ink: Pen, Highlighter, Eraser, Lasso Select (`ui.tool lasso`), Colour and Thickness
menus, Convert to Shapes. Done when: tests that a PNG inserted from bytes is stored once and
round-trips; a golden SVG with the data URI, clip path and filter; a PNG export pixel check at
the picture's centre; a PDF export containing an image object; crop and adjust round-trip and
change the pixels; compress shrinks the asset; an ink stroke commits as one shape and undoes in
one step; `ink.toShapes` on synthetic square and circle strokes gives a Rectangle and an Ellipse;
screenshots of the Picture and Ink tabs at 1280 px; `hostile_params_never_panic` green with junk
base64; `cargo xtask ci` green.

`b4-datagfx` (Opus). Goal: data graphics, legends and external data links. `diagram/src/data_graphic.rs`:
`DataGraphic {id, name, items: Vec<DataItem>}` where an item names its source (a shape data key
or an element property) and its kind: text callout (position, label, number format), data bar
(min, max, position, colour), icon set (rules of operator, value and icon name from `icons.rs`
names), colour by value (rules of operator and value to fill and optional line, or ranges); a
layout function returns each item's rectangle around the shape so the canvas and SVG draw the
same thing, and `evaluate(doc, shape)` reads shape data rows, page keys and element properties
in `diagram` (the model is reachable there); colour by value applies at draw time and never
writes `shape.style`. `Document.data_graphics` holds the definitions, `Shape.data_graphic`
names one. `datagraphic.*` as in the table; `datagraphic.legend` builds a group of shapes (title,
one row per rule with a swatch and label) at the frame's right edge, regenerated on request.
Canvas and SVG modules with one call line each in `draw/mod.rs` and `svg.rs`. External data:
`formats/src/table.rs` reads a CSV (the `csv` crate) or XLSX (`calamine`, gated off wasm) table
into columns and rows with caps (10 000 rows, 256 columns, 4 KB cells) and infers column types
(number, bool, date, text); `datalink.link` stores a `DataLink {id, path, format, sheet,
keyColumn, dataKey, columns, lastRefresh}` in `Document.data_links` and refreshes it;
`datalink.refresh` rereads the file and, for each row, sets data rows on every shape whose
`dataKey` value equals the row's key, defining missing page keys with the inferred types, in one
undo step, returning linked and unmatched counts; `datalink.bind` sets the key on a shape and
refreshes it. The External Data pane lists the rows with a linked mark, Link to Selection and
Refresh; Data > External Data has Link Data to Shapes (`ui.linkData` picks the file), Refresh All
and the pane toggle; Data > Display Data has a Data Graphics gallery with the current one checked,
Edit Data Graphic (`datagraphic_dialog.rs`: the item list with typed fields), Insert Legend and
Remove. Done when: tests that a graphic with all four kinds on a shape with a number row renders
(snapshot) and exports (golden SVG) at the rectangles the layout function returns; that colour by
value leaves `shape.style` unchanged; that the legend has one row per rule; that a CSV with three
rows linked by a Tag column sets rows on the two matching shapes and reports one unmatched; that
a refresh after the file changes updates values in one undo step; an XLSX read test on a minimal
workbook assembled in the test from hand-written XML parts (no file from another application);
`hostile_params_never_panic` green with junk paths; `cargo xtask ci` green.

## Block 5: generators and glyphs (6 branches, 5 at a time)

ROADMAP 5A and 5B, plus one notation-drawing foundation every pack in Block 7 needs.

| Branch | Scope |
|---|---|
| `b5-patterns` | Pattern framework, `pattern.*` commands, provenance, Patterns tab, starter workflow patterns |
| `b5-layout` | Pins and model order, band and chain primitives, grid and container primitives (`crates/layout`) |
| `b5-codegen` | Generator framework, ownership manifest, traceability and regions, `generate.*` commands, CLI `--check` |
| `b5-tables` | CSV, XLSX and Markdown table exporters; derived table and matrix views in the Tables tab; Generate dock |
| `b5-reimport` | Re-import contract and table-driven pattern sync (after `b5-patterns` and `b5-tables`) |
| `b5-glyphs` | Connector ends and compartments landed in Block 2C; here: BPMN flows and the Chen relationship end, ports drawn on shapes, glyph path extensions, and notation rendering variants so one model can draw in several notations (ER: Information Engineering, Chen, Barker, IDEF1X; DFD: Yourdon and DeMarco, Gane and Sarson) (`crates/stencils`, `diagram/kind.rs`, glyph drawing in canvas and SVG) |

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
| CAD Drawing (`.dwg`, `.dxf`) and the CAD Drawing Tools tab | `.dwg` has no public specification and the only open reader is GPL; `.dxf` is deferred (see the Block 4 decisions) | Insert an SVG or PNG exported from the CAD tool as a picture (`b4-pictures`) |
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
| AADL textual export | The AADL standard is a paid SAE document; draw the notation only, or buy and use the standard? | 8 |
| PNML (Petri net) and DMN, CMMN XML | Add each public schema to the Formats list in AGENTS.md when the pack starts | 8 |
