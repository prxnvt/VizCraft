# `b6e-frontend`: engineering drawing in the app (Block 6E wave 1 frontend, pulled forward)

Written on 2026-10-19 on the integration branch `claude/engineering-diagramming-standards-305d6b`.
This branch was pulled forward by the user so engineering sheets can be drawn by hand in the app
before the rest of Block 6 lands. It runs in its own Claude session, in parallel with the
integration session, which is building Block 6 wave 2 (UML, ER, sequence and topology packs; engine
only, no `crates/ui-egui` changes except retaken snapshot images). The user merges this branch
into the integration branch when it is done.

## Setup

- Worktree `.claude/worktrees/b6e-frontend`, branch `b6e-frontend`, from the integration branch
  at the commit that added this file.
- `export CARGO_TARGET_DIR=target-b6e-frontend CARGO_INCREMENTAL=0` for every cargo command (the
  disk filled once with five parallel builds; incremental caches are off on purpose).
- At most two cargo builds at a time from this session, including any subagents you start: the
  integration session runs three of its own.
- Control ports 8080 to 8089; screenshots under `/private/tmp/claude-501/b6e-frontend/`. Launch the
  app on the secondary monitor (`--window-pos -2500,100`), quit it through `app.quit` as soon as
  you have looked, and check with `pgrep -fl vizcraft`. Prefer the headless `ui_shot` example.
- Read AGENTS.md in full before starting, and `docs/cascade-plan.md` sections "Block 6E", "How
  Blocks 6 and 6E run" and "Block 6 and 6E wave 1 contracts" (its State paragraph lists the
  follow-ups this branch picks up). The wave 1 briefs in `docs/briefs/b6-wave1/` describe the
  engine you are building on; each has a section 4 listing what its frontend needs.

## Goal

An engineer can draw a standard-conforming sheet by hand in the app: pick a sheet and standard,
place symbols, draw wires and pipes that join at junctions and carry numbers, rotate and mirror
symbols with single keys, edit a symbol through its parameters, see cross references and zones,
and fix what the Problems panel reports. Everything stays a command first, then a UI over it.

## Scope

Canvas and tools:

- Wire and Pipe tools (`canvas/interact/wire.rs`): start on a free pin, draw orthogonally on the
  page module grid (`snap::module_step`, alignment lines from `snap::Kind::Module`), end on a pin, a
  wire (junction) or empty board; commit through `wire.draw`. Escape returns to the pointer.
  Dropping a device on a wire calls `wire.splice`; deleting one heals through `shape.delete`.
- Bare-letter keys: R `shape.rotate {turns: 1}`, X and Y `shape.mirror {axis}`, W the Wire tool,
  P the Pipe tool. They fire only while the canvas has focus and no text is being edited, are
  commands in the palette, and the key dispatcher and its shortcut test are extended to support
  bare letters (the first in the app).
- The canvas draws what the exports already draw: wire numbers and names (`net::label_spots`),
  line markers (`line_marks::marks`), glyph text slots upright (`Shape.glyph.labels`, the same
  rule as `formats/src/svg/glyph_labels.rs::anchor`), table compartment header rows and column
  rules (`Band.header`, `Band.columns`). The canvas path cache key must include `Freeform.frame`.
- Net highlight: hovering a wire highlights its net across the page through `Session.nets`;
  click holds it; the status bar shows the net number and pin count.
- Cross references: clicking a generated cross-reference label (`ext vizcraft.xref.for`) runs
  `xref.go`; hovering shows `xref.list` in a tooltip. The status bar shows the zone under the
  pointer (`sheet::zone_of`).
- The junction dot is too small at 3 line widths; make it clearly visible at normal zoom (an
  engine constant in `diagram/src/net.rs` is fine to change; keep the exports in step).

Symbols you can place now:

- No shipped pack uses the ten glyph builders yet (the electrical, process and fluid power packs
  come in wave 4). Add a palette section "Engineering symbols (preview)" with one entry per builder
  (contact, coil, bubble, valve, cylinder, pump, gate, feature control frame, weld, texture), each
  placing a free shape drawn by that builder with its defaults. If the engine has no command for
  that, add `glyph.insert {builder, params?, x?, y?, page?}` in `crates/engine/src/cmd/glyph.rs`
  with a test; it is the one engine command this branch may add. Wave 4 packs replace the section.

Panes, ribbon and dock:

- Inspector: a "Symbol" group from `glyph.params` that edits through `element.setProperty` (keys
  owned by the element) or `glyph.setParams` (shape overrides), with editors from the parameter
  kinds; net membership and number on a selected connector (`net.of`, `net.set`); a document
  property form from `doc.properties` on the page or document; the sheet and zone of a page.
- Nets dock tab: `net.list` with number, kind and pin count, `net.members` for the selected net,
  Go to select it, editing the number in place through `net.set`.
- A Sheet ribbon group (on Design, or a contextual Sheet tab; your call, matching how the ribbon
  is organised): standard and size menus from `sheet.presets` (ISO A3 first, ASME offered), Frame
  (`sheet.frames`), Title block (`sheet.titleBlock`), Revise (a small dialog over `sheet.revise`),
  Balloon (`sheet.balloon`), Renumber (`sheet.renumber`). Page Setup's Size menu gains the sheet
  presets and a Sheet standard menu.
- A Valve dialog over `glyph.builders` and `glyph.build` (positions, ports, path table,
  actuators, return, port labels) with a live preview, and a menu entry for `glyph.rebuild`.
- The `sheet` pack's palette groups, viewpoints, ruleset and a balloon leader connector type
  (left out in wave 1 so snapshots stayed still); retake the snapshots this changes.
- Insert > Field: filter `doc` fields the way `data` fields are filtered, with a submenu of
  document property keys from `doc.properties`.

Fixes the user found on the first look (see the cascade plan State paragraph):

- The generated frame page shows as its own page tab ("Wiring Background"); hide decoration
  pages from the page tabs (they stay reachable by command).
- The Inspector's Document title reads `properties.title` while `doc.setProperty {key: "title"}`
  writes the ISO 7200 field: make one key win so they agree (the engine change is small and
  belongs here).
- The `sheet.mandatoryFields` issue message starts with the rule description ("The title block's
  mandatory fields are filled in.") before the problem; the message should state only the
  problem ("Legal owner is empty").
- The title block's Size and Sheet values crowd their labels.
- `shape.rotate` and `shape.mirror` report the old selection as `primary`; report the shapes they
  turned.

## Out of scope

The template gallery and preset, the Analyze ribbon group and dock, and the software packs' UI
(Block 6 wave 3 in the integration session). Do not edit ROADMAP.md, docs/parity.md,
docs/mcp.md, docs/formats.md or docs/cascade-plan.md: write what they need in
`docs/briefs/b6e-frontend/report.md` instead, and the integration session applies it at the
merge. Engine changes stay small and are listed in the report.

## Done when

From a blank document, over the control channel and by eye in screenshots: set an ISO A3 sheet,
fill the title block from the Inspector, place a contact and a coil from the preview palette,
draw a wire between their pins with W, branch a second wire onto it and see the junction dot,
see the net number drawn on the canvas, press R and X on the contact and watch it turn and mirror
with its text upright, change the contact's poles in the Inspector and watch it redraw, hover a
wire and see its net highlighted, switch the page to ASME B from the Sheet group and see the frame
and title block change, and see no "Background" page tab. Kittest or snapshot tests cover the
tools, keys, Inspector groups, Nets tab and Sheet group; `cargo xtask ci` is green.

## Working with the user

The user is not a software engineer. Whenever you ask them to try something, give a ready-to-paste
command, say which window to run it in (Terminal, from which folder), and say what they should
see. To let them try the app, build it with `cargo build --release -p vizcraft` and give the full
path to `target-b6e-frontend/release/vizcraft`. Commit after every arc of work that builds, with
prefix `B6E:`. No merge into the integration branch, no push, no force-push.

## Trademarks

Claude is a trademark of Anthropic, PBC; UML of Object Management Group, Inc. The names are used
only to identify products and formats; VizCraft is not affiliated with or endorsed by their
owners.
