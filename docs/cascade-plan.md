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
| 3 Workspace (ROADMAP 4B) | 1, 2A pages, 2B Shape Data pane, 2C (conformance profile, connector kinds) | 4 |
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

## Block 2C: diagramming correctness (8 branches in two waves)

State: wave 1 (seven branches) merged into main on 2026-10-18 with `cargo xtask ci` green;
wave 2 (`b2c-ribbon`) running. Integration notes: every clash between branches was a read of
the stored sheet size that the board branch's guard test caught (describe, the drawing pack's
off-board check, three tests planting off-board shapes or crossings); fixed by reading
`Page::frame()` or pinning a test page to a fixed board or to `page.setRouting {avoid: never}`.
Follow-ups left for later briefs: `Page::rect()` is still the sheet in lane, container, arrange
and lasso code, and generated borders and `autolayout::grow_page` use it; the Shapes pane is
built once, so a pack loaded later does not appear in it; paste does not remap slot member ids;
the draw.io writer exports routed end points as extra points; the router's final stub is
shorter than an arrowhead; `design.setBackground` and `design.setBorderTitle` act on page 0
rather than the active page; the ISO 5807 rule references name symbols, not clauses.

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
