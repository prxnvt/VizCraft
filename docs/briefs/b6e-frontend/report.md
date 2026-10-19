# `b6e-frontend`: report for the integration session

The branch was built in two halves on sub-branches merged back here: the canvas half
([report-canvas.md](report-canvas.md)) and the panes, ribbon and dock half
([report-panes.md](report-panes.md)). Those two reports hold the details (engine changes, new
command ids, shared-file hunks, tests). This file is what the integration session needs at the
merge, plus the end-to-end check and what is left.

## State

Every scope item of the brief is built. `cargo xtask ci` is green on the merged branch. All UI
snapshots were regenerated after the two halves merged (39 PNGs conflicted only because both
halves retook them).

## End-to-end check (the brief's "Done when")

Run from a blank document over the control channel with screenshots, driving the real UI (key
presses, clicks and drags, not only command ids):

| Step | Result |
|---|---|
| Set an ISO A3 sheet | Frame, zones and ISO 7200 title block drawn; the Sheet contextual tab appears |
| Fill the title block from the Inspector | Typing into the document property form fills the block; the window title and Inspector title agree ("Motor starter") |
| Place a contact and a coil from the preview palette | Dragged from "Engineering symbols (preview)"; the Inspector shows the Symbol group |
| Draw a wire between their pins with W | W arms the tool; drag pin to pin commits `wire.draw`; status "Net n5, 2 pins" |
| Branch a second wire and see the junction dot | Junction made, dot clearly visible at normal zoom |
| See the net number on the canvas | After `net.renumber` the canvas draws "1" on the wire |
| R and X on the contact | Turns and mirrors; terminal numbers 13 and 14 stay upright; the glued wire follows the pin |
| Change the contact's poles in the Inspector | Poles 2 redraws with 13/14 and 23/24 and the mechanical link; one undo step |
| Hover a wire | The whole net (both segments, the branch and the pins) highlights; status "Net 1, 2 pins" |
| Switch the page to ASME B from the Sheet group | Choosing ASME Y14.1 gives ANSI B landscape, 4 by 2 zones, the ASME title block with the same field values, rulers in inches |
| No "Background" page tab | Only "Page-1" shows throughout |
| Problems panel | "Date of issue is empty." with no rule description in front |
| Nets tab | Lists net 1 (wire, 2 pins, 3 segments); Go to selects its segments and junction |

## Fix made in the main session after the merge

The Inspector asked every selected shape for `glyph.params`. On a shape no builder draws, the run
failed and its error waited in `Session.status` until the next command, which then showed "`s3` is
not drawn by a glyph builder" in the status bar, naming a shape that was no longer selected. The
Inspector now asks only when a builder draws the shape (`panes/inspector.rs`,
`drawn_by_builder`), with the regression test
`selecting_a_plain_shape_leaves_no_symbol_error_in_the_status_bar` (it fails on the old code). Twelve UI snapshots had the stray error baked into their status bar; they were retaken and differ from the old ones only in that strip.

After the user tried the app: once a click held a net's highlight, hovering any other wire kept
showing the held net, so wires connected at both ends looked as if they had no net. The wire
under the pointer now wins over the held net, for the highlight and the status bar, and the held
net returns when the pointer leaves the wires (`draw/nets.rs::highlighted`,
`interact/nets.rs::status`; regression test
`hovering_another_wire_shows_its_net_while_one_is_held`, which fails on the old code).

Second round, from the user's look at a wire to a valve port: wires ran straight through
symbols, because wave 1 made net segments exempt from the page's avoid-shapes policy (they kept
their drawn bends). Now `wire.draw` without `points`, on a page whose routing avoids shapes (the
default), sets the wire's own `avoid` flag, so it leaves each pin outward and routes around
shapes like any connector; a wire drawn with `points`, and every wire on a page set to
`avoid: never`, keeps its bends as before, and wires in older files are unchanged. Splitting at a
junction (`net::split_at`) and splicing a device (`wire.splice`) keep the flag on both halves and
let the router route them afresh; splice matches the device's pins against the wire's route
without the device, since an avoiding wire has already turned aside from it. Healing
(`net::join`) keeps the flag only when both halves had it. Test
`a_wire_drawn_without_bends_routes_around_shapes_and_drawn_bends_are_kept`; the splice and heal
test now compares against the straight wire as drawn. This reverses the substrate brief's
decision that wires never follow the page policy: for docs/parity.md and docs/architecture.md,
a wire routes around shapes unless it was drawn with bends or its page never avoids.

Third round, from the user: two wires of different nets drawn one after the other shared a
channel and lay on top of each other. Runs are spread apart only among connectors routed in the
same pass, and `autoroute::refresh` routed the new wire alone. `refresh` now looks, after
routing the changed connectors, for routed connectors whose drawn line shares a run with them
(`sharing_runs`, `share_run`) and routes them together once more. The spreading order in
`crates/routing/src/nudge.rs` also gained a pairwise score (`side_vote`): where a neighbouring
segment turns off strictly inside another member's extent, it takes the side it turns toward,
which removes crossings the old neighbour-sum rule caused (groups over 64 skip it). The sort stays
a total order. Four UI snapshots changed and were checked by eye: the patterns pane and form (the
four branch lines into the merge no longer cross) and the tables list and matrix (a connector
that lay on another's run now shows separately). Test `a_new_wire_does_not_run_on_top_of_another`
(no shared run, no crossing, no shape crossed; fails on the old router). `crates/routing` is not
owned by this branch; the change is small and the integration session should review it.

## Engine changes in this branch (all small)

| File | Change |
|---|---|
| `crates/engine/src/cmd/glyph.rs` | New `glyph.insert {builder, params?, x?, y?, page?}` (the one new engine command the brief allowed) |
| `crates/engine/src/cmd/shape.rs` | `shape.rotate` and `shape.mirror` report the turned shapes as `primary` and in `shapes` |
| `crates/diagram/src/net.rs` | Junction dot 6 line widths instead of 3; the exports follow because they draw junctions as shapes |
| `crates/engine/src/cmd/document.rs`, `cmd/edit.rs` | One document title: every title writer sets `properties.title` and `Document.title` together; on load the property wins |
| `crates/notation/src/sheet_pass.rs` | Sheet-pass issue messages state only the problem |
| `crates/diagram/src/title_block.rs` | Captions at the top of each cell, values centred below them; ISO and ASME goldens re-blessed |
| `crates/engine/src/cmd/sheet.rs` | `sheet.revise` with no description opens the Revise dialog; `sheet.balloon` uses the `sheet.leader` connector type |
| `crates/notation/packs/sheet.json` | Palette groups `sheet.callouts` and `sheet.tables`, viewpoint `sheet.Sheet`, ruleset `sheet.drawingSheet`, connector type `sheet.leader` |

Expected merge conflicts with wave 2: retaken snapshot PNGs (regenerate after the merge), the
connector kind count in `cmd/connector_kind.rs` and the stencils connector test (7 to 8 here),
and `notation/packs/sheet.json` if wave 2 touches it.

## For the docs the integration session owns

- docs/mcp.md: `glyph.insert`; the `primary` and `shapes` results of `shape.rotate` and
  `shape.mirror`; `sheet.revise` opening the dialog when called with no description; UI commands
  `ui.wire`, `ui.pipe`, `ui.rotateQuarter`, `ui.mirrorX`, `ui.mirrorY`, `ui.nets`, `ui.valve`;
  `ui.tool` accepting `wire` and `pipe`; `ui.dock` accepting `nets`; `ui.dialog` accepting
  `revise` and `valve`.
- docs/control-protocol.md (not on the forbidden list, but left alone here so the wording matches
  the rest): `ui.inspect` gains `diagram.net`, `diagram.zone`, a `wire` dragging state,
  `inspector.fields {symbol, sheet, zone}` and a top-level `nets`; `tool` can be `wire`.
- docs/parity.md: mark done the Wire and Pipe tools, bare-letter engineering keys, canvas net
  numbers and markers, net highlight, cross-reference navigation on the canvas, zone in the
  status bar, Inspector symbol parameters, connector net and document property form, the Nets
  tab, the Sheet tab, the Revise and Valve dialogs, Insert Field document properties, and hidden
  frame page tabs. Composed designations and cross references stay partial (no Sheets
  navigator yet).
- ROADMAP.md: the Block 6E wave 1 frontend landed early on `b6e-frontend`.
- docs/formats.md: no change (the junction size is a drawing change, not a format change).

## Left undone, deviations and things noticed

- Hand-made background pages keep their tabs; only generated frame pages are hidden.
- Dropping a symbol on a wire takes two undo steps (insert, then splice).
- Moving a shape does not snap to the page module grid yet; `sheet.set` does not set a module,
  so on a fresh sheet the Wire tool snaps only once `page.setModule` is used.
- Nets get numbers only through `net.renumber` (Nets tab or command); a new net has none.
- Net numbers on vertical wires are drawn turned with the wire, as the SVG export does.
- The Nets tab's Name column shows the net id when the net has no name.
- The Valve button's icon is a placeholder (an omega shape), to be replaced in `b6e-ux`.
- The engineering key commands are in the command palette but not in a menu.
- The net highlight covers the active page only.
- ASME zone data and the ISO 7200 block width are still to be checked against MIL-STD-100G and
  the standard.
