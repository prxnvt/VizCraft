# VizCraft Blocks 6 and 6E: features, intended behaviour, and the owner's decisions

Source of truth for whichever agent fixes bugs or finishes features after the Block 6 and 6E
branches are merged. Written on 2026-10-19 by the integration session. Where this file and a brief
disagree on intent, this file wins. Where code and this file disagree, the code is a bug unless the
owner has since said otherwise.

Repository: `~/Workspace/crafts/VizCraft`. Integration branch:
`claude/engineering-diagramming-standards-305d6b` (worktree
`.claude/worktrees/engineering-diagramming-standards-305d6b`). Plans:
`docs/cascade-plan.md` (sections "Block 6", "Block 6E", "How Blocks 6 and 6E run", the wave sections),
briefs in `docs/briefs/b6-wave1/`, `b6-wave2/`, `b6-wave3/`, `b6e-frontend/` (with `report.md`).
Research: `docs/research/swe-diagramming-ux.md`, `docs/research/ee-me-diagramming-standards.md`,
`docs/research/professional-features.md`, `docs/research/ee-me/`.

---

## 1. Rules the owner set (apply to every change)

1. **Optionality is the biggest part of this application.** Every standard, notation, style,
   layout or placement choice is the user's, with a sensible default, never hardcoded. Defaults:
   IEC 60617 symbols, ISO A3 landscape sheet, ISO 7200 title block. ANSI (IEEE 315), JIC and ASME
   sheets are always offered alongside, everywhere a standard is chosen (New page, Page Setup,
   Variant menu, `variant.set`, `sheet.set`). Switching a standard redraws in place without
   re-placing anything, in one undo step.
2. **Command first, UI second.** Every feature is an engine command reachable by id (CLI, control
   channel, MCP), and the UI is a thin layer over it. One user gesture is one undo step unless a
   brief says otherwise.
3. **Backend before frontend.** Engine, packs, formats and compatibility land first; canvas tools,
   panes, ribbon and dock come after on the merged engine.
4. **Branches.** All Block 6 and 6E work merges into the integration branch, not main. Main is
   left alone until the owner says to bring it over. Nothing is pushed without being asked.
5. **Engineering keys are bare letters** (as in KiCad and EPLAN): R rotates a quarter turn, X and Y
   mirror, W starts a wire, P starts a pipe. They fire only while the canvas has keyboard focus
   and no text is being edited, and each is also a command in the command palette. Wave 3 adds
   Tab, Enter and `/` as canvas keys under the same rule.
6. **The owner is not a software engineer.** When asking them to test, give exact paste-ready
   Terminal commands, say which window to run them in, and say what they should see.
7. **House rules** (from the owner's global settings): no emoji anywhere; no AI or Claude
   attribution in commits, PRs or files; bounded subagents; no polling loops; compact output; a
   separate `CARGO_TARGET_DIR` per worktree with `CARGO_INCREMENTAL=0`; at most three cargo builds
   at once (the disk filled once); delete merged worktrees and their target folders; GUI test runs
   on the secondary monitor (`--window-pos -2500,100`), quit after looking, checked with
   `pgrep -fl vizcraft`; checkpoint status into the memory files.

8. **Document types, not templates** (owner, 2026-10-19, after wave 3): "templates should be more
   like document types, the differences are in which shapes are presented as defaults and
   compliance-related stuff, nobody wants pre-made things to work around". New offers empty
   document types: each sets the palette's shapes (viewpoint), the notation or symbol standard,
   the sheet, title block and grid where the discipline uses them, and the compliance rules. The
   filled-in samples live under Help > Examples (and stay as test fixtures). Built in `b6-doctypes`.
9. **The app launches on the New document page** unless it was opened with a file.
10. **The palette shows the chosen notation's shapes and connectors first.** With an ER page in IE
    notation, the Chen, Barker and IDEF1X connector kinds and the generic drawing connectors sit in
    a collapsed group at the bottom, still searchable ("they should be more out-of-the-way").
11. **Engineering defaults** (owner, 2026-10-19, for wave 4): a Ladder Diagram takes the project
    default (IEC, ISO A3) like everything else; P&ID and process diagrams default to ISO 10628 with
    ISA-5.1 one click away; a project that picks JIC draws fluid power in the ANSI Y32.10 style
    (no public JIC fluid set exists).
12. **Tags update immediately** when a property they are built from changes (location, size, loop,
    code), keeping their number, as EPLAN does; tags with page, zone or rung parts renumber on demand.
13. **No Drawing section in the sidebar** (owner: "annoying and juvenile"). The Basic primitives
    (rectangle, ellipse and the rest) are not listed in the left Palette pane for any document
    type or the blank page; they stay reachable through the shapes search, the `/` picker and
    Insert > Basic Shapes.

## 2. How to run what exists (for the owner, and for checking)

```sh
cd ~/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b
./target-eng-standards/release/vizcraft /tmp/vizcraft-wave2/er.schema.vizcraft
```

Example files from every template are in `/tmp/vizcraft-wave2/` (made with
`./target-eng-standards/release/vizcraft-cli new --template <id> <out>.vizcraft`). Known bug: giving
the app several files on the command line shows only the first (assigned to `b6-gallery`); one
app per file works:

```sh
for f in uml.classDiagram er.schema er.conceptual uml.useCase uml.stateMachine uml.activity uml.sequence; do ./target-eng-standards/release/vizcraft /tmp/vizcraft-wave2/$f.vizcraft & done
```

Rebuild the release app after merges: `CARGO_TARGET_DIR=target-eng-standards CARGO_INCREMENTAL=0
cargo build --release -p vizcraft -p vizcraft-cli`. Quality gate: `cargo xtask ci` (fmt, clippy,
test, assets, layers, wasm).

---

## 3. Features built, and what each is meant to do

### 3.1 Templates and project standards (wave 1, `b6-templates`)

- A template is a pack viewpoint plus a seeded skeleton, a layout, optional hooks (sheet, title
  block, key, symbol grid module) and code forms. `template.list`, `template.new`,
  `template.addPage`, `template.standards`, `template.preview`; `vizcraft-cli new`.
- `Document.standards {symbols, sheet}` is the project's choice; `sheet.set` with no preset uses
  it. Sheet ids are `<standard>.<size>.<orientation>`; `ansi.` is accepted for `asme.`.
- 21+ templates ship: basic, flowchart, cloud, network, dfd, eight UML, three ER.
- Intended (wave 3, `b6-gallery`): the File > New page is a gallery with previews, search,
  audience chips and Recent; a standards bar (symbols IEC default, ANSI, JIC and others; sheet ISO
  A3 default, every ISO and ASME size) written into `Document.standards`; Add Page from template;
  an engineering workspace preset that is offered on the first New page, never forced (an unset
  preset keeps today's office look).

### 3.2 Analysis (wave 1 engine, wave 3 UI)

- `vizcraft-analysis`: dependencies (upstream, downstream, both, with depth and relationship type
  filters), cycles, unreachable elements, single points of failure, metrics, paths, model diff
  against a saved file. `analyze.*`, `model.diff`, `vizcraft-cli analyze`. Analyses never change
  the document and never make an undo step.
- Intended (wave 3, `b6-analyze-ui`): Analyze and Compare groups on the Model tab; canvas halos
  with level chips (colour is never the only signal), dimming of the rest, an "out of date" state
  after edits; an Analysis dock tab with Go to, Select, Re-run and Clear; Compare shows added and
  changed items tinted and removed items as dashed ghosts from the compared file.

### 3.3 Nets, wires, orientation (wave 1 engine, `b6e-frontend` UI, done)

- Nets are model objects with numbers; wires join at junctions; splitting, splicing a device into
  a wire and healing on delete; wire numbers and line markers; a symbol grid module
  (`View.module`: ISO 2.5 mm, 0.1 inch, or custom) with `page.setModule`.
- `Shape.orient`: quarter turns and mirror, with text always kept upright.
- In the app: Wire and Pipe tools (W, P) that draw orthogonally on the module grid and commit
  `wire.draw`; Escape returns to the pointer; dropping a device on a wire splices it in.
- Hovering a wire highlights its whole net; a click holds it; the status bar shows the net number
  and pin count, and the sheet zone under the pointer.
- A wire drawn without bends routes around shapes and leaves each pin outward like a connector; a
  wire drawn with bends keeps them; on a page set to never avoid, wires keep their bends.
- Nets dock tab: list, members, Go to, edit the number in place.

### 3.4 Sheets, title blocks, cross references (wave 1 engine, `b6e-frontend` UI, done)

- ISO 5457 and ASME Y14.1 sheets with zones and frames, ISO 7200 title block fields (ASME extras
  optional), revision tables, parts lists, balloons, zone and sheet fields, composed identifiers,
  cross references. `sheet.*`, `doc.setProperty`, `doc.properties`, `xref.*`.
- The generated frame page is hidden from the page tabs (only hand-made background pages show).
- There is one document title: every writer sets the title property and the window title
  together, and on load the property wins.
- Problems panel messages state only the problem (no rule description in front).
- In the app: a contextual Sheet tab (standard and size from `sheet.presets` with ISO first and
  ASME offered, Frame, Title block, Revise dialog, Balloon, Renumber); Page Setup gains sheet
  sizes and a standard menu; the Inspector gains Sheet and Document groups; cross-reference labels
  navigate to their partner on click and list them on hover.

### 3.5 Parametric symbols (wave 1 engine, `b6e-frontend` UI, done)

- Ten builders: contact, coil, bubble, valve, cylinder, pump, gate, feature control frame, weld,
  texture. Each glyph carries a mandatory `source` provenance checked by `cargo xtask assets`;
  glyphs are drawn from written descriptions and US government public domain documents only.
- In the app: an "Engineering symbols (preview)" palette section places one free symbol per
  builder (`glyph.insert`); the Inspector Symbol group edits parameters (one undo step); a Valve
  dialog with live preview. The wave 4 packs replace the preview section.

### 3.6 UML (wave 2, `b6-uml`)

- `uml.json` in sections: class, component, package, deployment, use case, activity, state
  machine, sequence. Member rows (attributes, operations, literals) come from element properties;
  end labels (roles, multiplicities) come from relationship properties; four checks; PlantUML
  export; Mermaid class and state read and write.
- Variants: Default, Visibility in words, Lollipop interfaces, Names only (UML tab > Variant).
- Today, edit members through the Inspector (one per line, for example `- total: Money [0..1]`).
  Double-clicking text inside a class box is overwritten by the next sync: a known bug, fixed in
  wave 3 (`b6-ux` writes the model property instead).
- **Fork and Join (owner's finding, clarified):** UML 2.5.1 draws both as the same black bar; the
  difference is the flows (one in, several out is a Fork; several in, one out is a Join). Intended
  behaviour: the canvas keeps the standard bar; the palette, picker and Insert icons show the
  direction; a rule at info level reports a Fork used as a Join or the reverse, with a fix that
  changes the type where possible (`b6-polish`).
- **Hand-drawn flows (owner's finding):** on an activity page a connector drawn by hand must
  become the right model relationship (Control Flow), so rules count it. The Connector tool on a
  notation page defaults to the relationship the rules allow between its ends (`b6-ux`).

### 3.7 Sequence diagrams (wave 2 engine, wave 3 UI)

- `Page.sequence` holds the order of events; the layout settles lifelines, activation bars,
  messages and fragments from it. `sequence.*` commands; Mermaid and PlantUML sequence.
- **What the parts mean (explained to the owner):** boxes and the stick figure are participants;
  the dashed vertical lines are their timelines and run the full height; solid arrows with filled
  heads are calls; dashed arrows with open heads are replies; thin tall rectangles are activation
  (busy) bars; an `alt` box is a bracket around alternative message groups, with a guard such as
  `[found]` and `[else]` per part. Arrows inside an `alt` box are meant to be there (they are the
  messages it groups), and timelines pass through it.
- **What is wrong today (owner's finding, "awkward", "moving through objects"):**
  1. the `alt` frame's left edge sits on the Client timeline, so the timeline runs through the
     `alt` tab and the `[else]` guard;
  2. the frame's right edge almost touches the Orders API activation bar;
  3. spacing inside the frame is uneven: a tall gap under `[found]`, "200 OK" sitting on the dashed
     divider, the `[else]` half mostly empty;
  4. message labels sit on the line with the line running through them.
- Intended (`b6-sequence-ui`): the frame reaches at least half a column gap past its covered
  timelines and clear of their bars; tab and guards where no timeline crosses them; each operand's
  first message one row below its guard, dividers half a row below the last message; labels above
  the line on canvas and in every export; a test that nothing crosses a tab, guard or label. Plus
  canvas editing: hit the stem, drag columns sideways, drag messages to reorder, draw a message
  from stem to stem, wrap a selection in a fragment, edit guards by double-click, a Sequence tab,
  an Inspector Sequence group, Alt+arrow moves. Tab and Enter do nothing on a sequence page.

### 3.8 Entity-relationship (wave 2, `b6-er`)

- `er.json` with Conceptual, Logical and Physical viewpoints and four notations: Information
  Engineering (crow's foot, default), Chen, Barker, IDEF1X. Switch with Entity Relationship tab >
  Variant; one undo step; nothing moves. New line ends, per-half dashes, double lines and outlines;
  columns as owned elements; an ER layout; DBML and Mermaid ER read and write; `er.*`.
- **Owner's findings:** tables showed blank lines above the key rows and above the column rows,
  and columns did not line up on the canvas; long types are cut (`varchar(255` loses its bracket).
  Intended: one header row per table, aligned PK, name and type columns, no blank lines, cells
  that fit or end in an ellipsis, the canvas matching the export (`b6-polish`). The canvas must
  also draw Chen diamonds, Barker halves and double outlines as the export does (`b6-polish`).
- Intended UI (`b6-ux`): edit columns in a grid in the Inspector (Name, Type, PK, FK, Null,
  Unique), Add column, edit a cell by double-click on the canvas.

### 3.9 Cloud, network, data flow, keys (wave 2, `b6-topology`)

- `cloud.json`, `network.json`, `dfd.json`; containers nest by `allowedParents` with an owner sync;
  per-type container looks; numbered step badges on connectors; generated keys (`key.*`) built on
  the title block table; Mermaid architecture-beta read and write.
- Known: step badges sit on line labels; the User and Internet labels sit on their figures; the
  canvas does not draw badges; key text sits flush against the cell's left rule since the title
  block change (`b6-polish`). Steps tool, Key and Steps ribbon groups and "Include in key" come in
  `b6-ux`.

---

## 4. Connector routing: the owner's standard

The owner's words: "I don't want connectors/wires to align with the boxes they're from, they
gotta branch out a little" and "Why are connectors and arrows moving through objects? Unreadable
diagram". Intended behaviour (`b6-routing`, building on the `b6e-frontend` routing fixes):

- A connector never runs through a shape that is not one of its ends, and never runs along the
  edge of its own end shapes. It leaves and enters each end perpendicular to the side it attaches
  to, with a short straight stub first.
- It keeps a clearance from shapes. The clearance is a page routing option with a default (the
  symbol grid step when set, else 10 units), set through `page.setRouting` and shown in Design >
  Connectors; the user can change it.
- Two connectors never share a run; connectors meeting one side of a shape spread along it in
  order; labels never sit on a shape or another label.
- Layouts leave room for routes: in Chen ER, attribute ovals go around their entity away from the
  relationship axis (they sat on the axis and pushed "places" onto the box edges); flows into a
  fork or join enter from its long side, spread; two-way flows do not pile both entities on one
  side of a trust boundary.
- A checker runs over every shipped template in the test suite and fails on any violation.
- Already fixed in `b6e-frontend` (owner's findings while drawing): a wire ran straight through a
  valve to its port; two wires to a valve lay on top of each other; after a click held one net,
  hovering another wire still showed the held one.

---

## 5. Owner findings log (all of them, with where each went)

| Finding | Status |
|---|---|
| Canvas did not draw wire numbers (exports did) | fixed, `b6e-frontend` |
| Generated frame page showed as its own page tab | fixed, `b6e-frontend` |
| Inspector title and title block title disagreed | fixed, `b6e-frontend` (one title) |
| Mandatory field issue began with the rule description | fixed, `b6e-frontend` |
| Junction dot too small | fixed, `b6e-frontend` (six line widths) |
| Held net highlight hid other wires' nets | fixed, `b6e-frontend` |
| Wire ran through a valve | fixed, `b6e-frontend` |
| Two wires overlapped | fixed, `b6e-frontend` |
| Stray glyph error in the status bar after selecting a plain shape | fixed, `b6e-frontend` |
| Several files on the command line open only the first | `b6-gallery` |
| Fork and Join indistinguishable | clarified as standard; icons and rule in `b6-polish` |
| Hand-drawn activity flow not counted by the join rule | `b6-ux` |
| ER tables: blank lines, unaligned columns, clipped types | `b6-polish` |
| Chen relationship runs along the entity box edges | `b6-routing` |
| Connectors through objects, overlapping flows | `b6-routing` |
| Sequence: timelines through the `alt` tab and guard, frame on the bar, uneven spacing, labels on the line | `b6-sequence-ui` |
| Arrows inside the `alt` box | explained: intended |
| Key text flush against its cell rule | `b6-polish` |
| Templates should be empty document types; examples to Help > Examples | `b6-doctypes` |
| App should launch on the New page | `b6-doctypes` |
| Other notations' connectors cluttering the ER palette | `b6-doctypes` |
| Electrical and mechanical document types missing | wave 4 (after `b6-doctypes`) |
| Project storage size (asked if the product can be under 10 GB) | answered: the app is about 50 MB; the large folders are build caches, deletable |

---

## 6. Plan from here

- **Wave 3 (frontend, briefs in `docs/briefs/b6-wave3/`):** `b6-gallery`, `b6-analyze-ui`,
  `b6-sequence-ui`, `b6-ux` (Tab and Enter insertion, `/` picker with relationship and pattern
  rows, label and end-label editing, Model Tree relationship rows, Inspector ends, UML members and
  ER columns, notation-aware Re-Layout, Process tab trim, Steps tool and Key group), `b6-polish`,
  `b6-routing`. Merge order: sequence-ui, polish, routing, analyze-ui, ux, gallery. After the
  wave, the owner tests in the app before wave 4 starts.
- **Wave 4 (packs on the substrate):** `b6-c4` (C4 with Structurizr DSL), `b6-code` (Code pane
  keeping Mermaid, DBML or Structurizr text and the canvas in sync, D2 writer),
  `b6e-electrical` (IEC 60617, IEEE 315 and JIC variants; ladder; terminal strips and cables),
  `b6e-process` (P&ID and PFD with ISA and ISO 10628 variants, lists and loops),
  `b6e-fluidpower` (ISO 1219), `b6e-mechanical` (mechanical, annotation, reliability, IDEF0).
  Each pack's engine first, its ribbon after.
- **Wave 5 (frontend finish):** `b6e-ux` (Sheets navigator, Nets and Cables in the Model Tree,
  designations in the Inspector), pack ribbons, template preview snapshots.
- Open items noted but not yet assigned: moving a shape does not snap to the module grid;
  `sheet.set` does not set a module; new nets get numbers only through `net.renumber`; dropping a
  symbol on a wire is two undo steps; Tab plus naming is two undo steps; ASME zone data still to
  be checked against MIL-STD-100G (`b6-polish` attempts it); the Valve button icon is a
  placeholder (`b6e-ux`).
