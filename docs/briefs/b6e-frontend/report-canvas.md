# `b6e-fe-canvas`: report

The canvas half of `b6e-frontend`, built on the sub-branch `b6e-fe-canvas` (final commit `42be19a`,
`cargo xtask ci` green). Written down by the main session from the builder's summary.

## What was built

- Wire and Pipe tools (`canvas/interact/wire.rs`): a drag from a pin, junction, wire or empty board
  to any of those commits one `wire.draw`. Free ends snap to the page module grid; the preview
  splits the wire it lands on. The tool stays armed until Escape.
- Splice: a device dropped or moved onto a wire runs `wire.splice`. Delete already heals through
  `shape.delete`.
- Bare-letter keys: R rotates, X and Y mirror, W arms Wire, P arms Pipe. They fire only when no
  widget has focus and no in-place edit, dialog, menu, File page, presentation or insert picker is
  up.
- Canvas drawing: wire numbers, line markers, upright glyph text slots and table header and column
  rules, matching the SVG export. The fill cache key now includes `Freeform.frame` and also
  `Shape.orient` (it was missing, so R and X did not redraw).
- Net highlight: hovering a wire highlights its net, a click holds it, Escape or a click elsewhere
  releases it. The status bar shows the net number and pin count ("Net 2003, 2 pins").
- Cross references: clicking a generated label runs `xref.go`; hovering shows `xref.list` in a
  tooltip. The status bar shows the zone under the pointer on sheet pages.
- Preview palette: a closed "Engineering symbols (preview)" section with one row per builder,
  placing symbols through `glyph.insert` by click or drag.

## Engine changes

| File | Change | Why |
|---|---|---|
| `crates/engine/src/cmd/glyph.rs` | New `glyph.insert {builder, params?, x?, y?, page?}` returning `{id, builder, shapes, primary, items}`; one undo step, selects the shape, puts the first pin on the module grid | No command placed a free builder shape; the preview palette needs one |
| `crates/engine/src/cmd/shape.rs` | `shape.rotate` and `shape.mirror` report the turned shapes as `primary` and add `shapes` | They reported the old selection |
| `crates/diagram/src/net.rs` | Junction dot 6 line widths instead of 3 (about 2.3 mm on a 1.5 wire) | Too small at normal zoom; exports draw junctions as ordinary shapes, so SVG and PDF follow |

## New commands (for docs/mcp.md and docs/parity.md)

- Engine: `glyph.insert` as above.
- UI: `ui.wire` (W), `ui.pipe` (P), `ui.rotateQuarter` (R), `ui.mirrorX` (X), `ui.mirrorY` (Y);
  `ui.tool` also accepts `wire` and `pipe`.
- `ui.inspect` gains `diagram.net`, `diagram.zone` and a `wire` dragging state.

## Shared-file hunks

- `crates/ui-egui/src/lib.rs`: `Tool::Wire` with its name and parse arms, five `UI_COMMANDS` rows,
  one `ui_command` hook.
- `crates/ui-egui/src/chrome.rs`: three lines in the status bar.
- `crates/ui-egui/tests/palette.rs`: the expected group list gains "Symbols".

## Snapshots

37 macOS window snapshots retaken because the new palette section adds a row; new snapshot
`canvas-wire-number`.

## Tests

`crates/ui-egui/tests/wires.rs` covers drawing a wire, the branch junction, Escape, each key, the
keys staying silent in a text field, in-place editing and the command palette, net hover and hold,
and the palette placing a coil. Engine tests cover `glyph.insert` and the rotate and mirror
`primary`.

## Left undone or different from the brief

- Dropping a device on a wire takes two undo steps (the insert, then the splice).
- The UI check for "on a wire" is coarse; the engine refuses a bad fit and the status bar says why.
- Moving a shape does not yet snap to the module grid.
- The key commands are not in a menu (menus belonged to the panes half).
- No command sets line markers yet, so the snapshot sets one directly on the document.
- The net highlight covers the active page only.
