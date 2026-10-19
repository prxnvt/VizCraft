# `b6e-fe-panes`: report

The panes, ribbon and dock half of `b6e-frontend`, built on the sub-branch `b6e-fe-panes` (final
commit `2911c40`, `cargo xtask ci` green). Written down by the main session from the builder's
summary.

## What was built

- Inspector: a Symbol group from `glyph.params` that writes each value where it belongs (element
  property or shape override through `glyph.setParams`), with editors from the parameter kinds; a
  net group on a selected connector (`net.of`, number and name edited through `net.set`); a Sheet
  group (sheet and zone of the page); a document property form for the title block fields from
  `doc.properties`.
- Nets dock tab: `net.list` with number, kind and pin count, `net.members` for the selected net,
  Go to, and the number edited in place through `net.set`.
- Revise dialog over `sheet.revise`; Valve dialog over `glyph.builders` and `glyph.build` with a
  live preview; Place dispatches `glyph.insert {builder: "valve", params}`; Rebuild Symbols runs
  `glyph.rebuild`.
- Generated frame pages are hidden from the page tabs and stay reachable by command.
- Insert > Field filters `doc` fields the way `data` fields are filtered, with a submenu of
  document property keys.

## Ribbon placement

The Sheet tools are a contextual Sheet tab, shown while the active page has a sheet, not a Design
group: a Design group pushed Design past 1280 points and failed the ribbon fit test, and a Symbols
group did the same to Insert. The tab holds Standard and Size from `sheet.presets` (ISO first,
ASME offered), a Title Block menu (ISO 7200, ASME, None, Redraw Frame), Revise, Balloon,
Renumber, Valve and Rebuild Symbols. To put a page on a sheet, Design > Page Setup's Size menu has
sheet-size submenus per standard, and the Page Setup dialog has Sheet standard and Sheet size
rows. The Edit menu also has Valve and Rebuild Symbols.

## Engine changes

| File | Change | Why |
|---|---|---|
| `crates/engine/src/cmd/document.rs`, `cmd/edit.rs` | New `write_title`: `edit.setTitle`, `doc.setProperties` and `doc.setProperty {key: "title"}` all set `properties.title` and keep `Document.title` equal; the Inspector reads `doc_value("title")`. Old files load unchanged; where the two differ, the property wins. Test `every_title_writer_sets_the_one_title` | The two title keys disagreed |
| `crates/notation/src/sheet_pass.rs` `issue()` | Sheet-pass messages are the finding only ("Legal owner is empty."); other packs still lead with the rule description | The message began with the rule description |
| `crates/diagram/src/title_block.rs` `shapes()` | Text margins zero, captions 0.4 mm from the top, values centred under the caption with a 1 mm side pad; ISO and ASME goldens re-blessed | Size and Sheet values crowded their labels |
| `crates/engine/src/cmd/sheet.rs` `revise` | With no description it pushes `{"open": "revise"}`, changes nothing and returns `{"rev": null, "element": null}` | The ribbon's Revise button opens the dialog |
| `crates/engine/src/cmd/sheet.rs` `balloon` | The leader uses the new `sheet.leader` connector type | The pack now has a leader kind |
| `cmd/connector_kind.rs`, `stencils/connectors.rs` tests | Connector kind count 7 to 8 | The new leader kind |

## Sheet pack additions

Palette groups `sheet.callouts` and `sheet.tables`, viewpoint `sheet.Sheet`, ruleset
`sheet.drawingSheet` with all eight rules, connector type `sheet.leader`.

## New commands and fields (for docs/mcp.md and docs/parity.md)

- `ui.nets {net?, goto?}` returns `{selected, editing, nets: [{id, number, name, kind, pins, segments}], members}`.
- `ui.valve {}` opens the Valve dialog.
- `ui.dock` accepts the tab `nets`; `ui.dialog` accepts `revise` and `valve`.
- `sheet.revise` with no description opens the Revise dialog.
- `ui.inspect` adds `inspector.fields {symbol, sheet, zone}` and a top-level `nets`.
- Parity rows to mark done: Inspector symbol, net and document form; Nets tab; Sheet tab; Revise
  and Valve dialogs; Insert Field document properties; hidden frame page tabs.

## Shared-file hunks

`lib.rs` (two module lines, `UI_COMMANDS` entries for `ui.dialog`, `ui.dock`, `ui.nets`,
`ui.valve`, a `ui.valve` arm), `control.rs` (two `inspect` fields), `dialogs.rs` (`Revise` and
`Valve` variants), `panes/mod.rs` (one re-export), `ribbon/mod.rs` (the `sheet` tab).

## Tests

`crates/ui-egui/tests/engineering_panes.rs`: the poles edit redraws the contact, the connector net
group, the Nets tab (list, number edit, Go to), the document property form, ISO A3 to ASME B
changing the frame and title block, the Insert > Field document submenu. Unit tests: hidden page
tab, Page Setup sheet rows, Revise dialog, valve path parser.

## Left undone or different from the brief

- Hand-made background pages keep their tabs; only generated frame pages are hidden.
- The Inspector caches `glyph.params`, `net.of` and `doc.properties` per revision in egui memory.
- ASME zone data and the ISO 7200 block width are still unchecked against MIL-STD-100G and the
  standard.
