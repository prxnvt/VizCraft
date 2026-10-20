# `b6e-sheets` (engine half), Block 6 and 6E wave 1

Opus, medium effort. Port 8072. Worktree `.claude/worktrees/b6e-sheets`, branch `b6e-sheets` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that added this
file). `CARGO_TARGET_DIR=target-b6e-sheets`. Commit prefix `B6E:`. Backend only: touch no file under
`crates/ui-egui` or `apps/vizcraft`.

## Resolutions (these override the draft below where they differ)

The cross-branch contracts are in docs/cascade-plan.md under "Block 6 and 6E wave 1 contracts".
For this branch:

- Rename `sheet.sizes` to `sheet.presets {}`. It returns the standards and sizes with default zone counts plus preset ids of the form `<standard>.<size>.<orientation>` (standard `iso` or `asme`, size `a0`..`a4` or `a`..`e`, orientation `landscape` or `portrait`), ISO A3 landscape first.
- `sheet.set` also accepts `sheet: "<preset id>"` as a shorthand. With no standard or size given, it uses `Document.standards.sheet` (owned by `b6-templates`) when set, else `iso.a3.landscape`. Read `Document.standards`; do not define your own copy. Until `b6-templates` merges, the fallback is the constant.
- Add `sheet.titleBlock {page?, kind?: "iso7200" | "asme" | "none"}`. It sets `Sheet.titleBlock` and regenerates the frame; `b6-templates` calls it.
- Cross-reference labels are generated child text shapes, as drafted. Test that `parent` on a non-group shape moves and selects correctly in `arrange`. If it does not, use an unparented shape keyed by `ext vizcraft.xref.for`. A deleted label comes back unless the element property `xrefLabel` is `off`.
- Composed tags (`{page}`, `{zone}`, `{rung}`) change on demand through `sheet.renumber`. Add a live Info rule, `sheet.staleTag`, that flags a tag whose context has changed.
- Mark the ASME zone counts, margins and directions in code comments as secondary-source data to verify against MIL-STD-100G.
- `title_block::Block` is the API `b6-topology` reuses for its key in wave 2; keep it public and documented.

## Draft brief

**(1) Brief**

`b6e-sheets` engine half (Opus, port 8072, worktree `.claude/worktrees/b6e-sheets`, branch `b6e-sheets` from `claude/engineering-diagramming-standards-305d6b` at 9eecade, `CARGO_TARGET_DIR=target-b6e-sheets`, prefix `B6E:`; touches no `crates/ui-egui` or `apps/vizcraft` file).

Goal: drawing sheets as engine data and generated output, all reachable by command id and checked over the CLI and the control channel:
- sheet standards, sizes and zones;
- ISO 5457 and ASME Y14.1 frames;
- zone, sheet and document-property fields;
- the ISO 7200 property set;
- the title block generator;
- revision tables, parts lists and balloons;
- composed designations;
- cross references;
- the `sheet` pack.

ISO is the default everywhere and ASME is always offered.

Sheet model (new `crates/diagram/src/sheet.rs`):
- `Page.sheet: Option<Sheet>` (serde default, skipped when `None`, sanitised in `Page::sanitize`). `Sheet { standard: SheetStandard (Iso5457 default | AsmeY14_1), size: SheetSize (IsoA0..IsoA4 | AnsiA..AnsiE, default IsoA3), orientation: Orientation, zones: ZoneScheme { cols, rows, lettersFrom: Top|Bottom, numbersFrom: Left|Right }, number: Option<u32>, titleBlock: TitleBlockKind (Iso7200 | Asme | None), revisionScheme: AsmeY14_35 | Iso7200 }`. Field names are camelCase on disk, as in every serde type.
- `PAPER_SIZES` holds name, mm or inch dimensions and world units (100 per inch, 100/25.4 per mm). `geometry(page) -> SheetGeometry { trimmed, frame, col_edges, row_edges }`.
- ISO 5457 geometry: left border 20 mm and the others 10 mm, frame line 0.7 mm. Grid fields are 50 mm counted outwards from the centring axes, with the remainder going to the corner fields. Field counts, long side by short: A0 24x16, A1 16x12, A2 12x8, A3 8x6, A4 6x4. Letters run down and skip I and O; numbers run across. Grid lines are 0.35 mm and characters 3.5 mm. A4 is labelled on the top and right only. Four centring marks.
- ASME Y14.1: A 8.5x11 to E 34x44 in. Zone counts come from the research table (A 2x2, B 4x2, C 4x4, D 8x4, E 8x8). Numbers run from the right and letters from the bottom, per MIL-STD-100G. Keep these as data in `ASME_ZONES`.
- `zone_of(page, Point) -> Option<Zone { row: String, col: u32 }>`, displayed as `B4`; `None` outside the frame or with no sheet.
- `sheet_number(doc, index)`: `Sheet.number`, else the position among foreground pages that have a sheet. `sheet_count(doc)`.
- `next_revision(prev, scheme)`: ASME letters skip I O Q S X Z, AA follows Y, at most 2 characters, initial issue `-`; ISO skips I and O.
- `compose_ctx(doc, element) -> ComposeCtx`: page, zone and rung from the element's first view.
- `refresh_frames(doc, deco_index)`.

Frames (`decor.rs`):
- When the decorated page has a sheet, `build` draws the sheet frame, zone ticks and labels, centring marks and title block instead of `deco.border`. It still draws `deco.background`.
- `set` and `refresh` keep the generated page while `Page.sheet` is set and remove it when both the sheet and the decoration are gone.
- A page with no sheet builds byte-identical shapes to today.

Title block generator (new `crates/diagram/src/title_block.rs`, reusable by b6-topology's key generator):
- `Block { w_mm, h_mm, slots: Vec<Slot { x, y, w, h (mm), label, text, size, bold }> }`, `iso7200()` and `asme()` in our own layouts, and `shapes(&Block, origin, units_per_mm, id: impl Fn(&str) -> Option<String>) -> Vec<Shape>`.
- Slot text holds field placeholders (`{{doc:legalOwner}}`, `{{sheet}}`/`{{sheets}}`), so it never goes stale.

Fields:
- `text_fields.rs` gains `Field::Zone` (the zone of the shape's centre), `Sheet`, `Sheets` and `Doc(key)`, parsed like `data:key` and added to `FIELDS`.
- `print_fields.rs` gains `{sheet}` and `{sheets}`.

Document properties:
- `DocProperties.fields: BTreeMap<String, String>`: plain keys, at most 64 entries, values capped at `MAX_SHORT`.
- `{{doc:title}}` reads `properties.title`; `creator` falls back to `author`; `revisionIndex` falls back to the last `sheet.Revision`.

Schema (`crates/notation/src/schema.rs`):
- `Pack.documentProperties: Vec<PropertySchema>`; `PropertySchema.maxChars: Option<u32>` and `group`.
- `RelationshipType.kind: RelationshipKind (Plain default | CrossReference)`.
- `CompartmentDecl.query: Option<TableQuery { types, columns, headers, widths, sortBy }>`.
- The loader checks each of them.

Table compartments (`compartment.rs`):
- `Compartment.columns: Vec<Column { title, width }>` (at most 16). A list row's cells are separated by tabs. `layout` gives `Band.columns` (x edges) and a header row.
- `formats/src/svg/compartments.rs` and `pdf/compartments.rs` draw the header row, column rules and clipped cells.

`sheet` pack (new `crates/notation/packs/sheet.json`, registered in `catalog.rs`). Every glyph carries `source` (`own design`).

| Kind | Contents |
|---|---|
| Element types | `sheet.Revision` (rev, zone, description, date, approved); `sheet.RevisionTable` (list compartment, query over Revision; headers ZONE REV DESCRIPTION DATE APPROVED); `sheet.Part` (quantity, partNumber, description, material; the tag is the item number); `sheet.PartsList` (`types` property, default `sheet.Part`; query sorted by item); `sheet.Balloon`; `sheet.OffSheet` |
| Relationships | `sheet.CallsOut` (Balloon to Part, the leader connector); `sheet.Continues` (OffSheet to OffSheet, `crossReference`); `sheet.References` (any to any, `crossReference`) |
| Identifiers | `sheet.item` (`{n}`, unique, autoNumber); `sheet.revision` (unique); `sheet.offSheet` (paired) |
| Viewpoints | `sheet.Frame`, `sheet.Assembly` |
| Variants | `sheet.iso5457`, `sheet.asmeY14_1` (table and balloon glyphs) |
| Document properties | ISO 7200 names and limits: legalOwner M, identificationNumber M 16, revisionIndex O 2, dateOfIssue M 10, title M 30, supplementaryTitle O 60, approvalPerson M 20, creator M 20, documentType M 30 (enum from ISO 29845 and Y14.24), responsibleDepartment O 10, technicalReference O 20, classification O, documentStatus O 20, language O 4, paperSize O 4. ASME extras, optional: drawingNumber, cageCode, scale, material, finish, weight, drawnDate, checkedBy, checkedDate, approvedDate. Plus xrefFormat (default `/{sheet}.{zone}`) and xrefSameSheet (default `{zone}`) |
| Rules | sheet.mandatoryFields; sheet.fieldLength; sheet.revisionSequence; sheet.balloonRows (each balloon calls out one listed part, and each part has a balloon); sheet.unresolvedXref; sheet.unmatchedOffSheet |

Rules engine (`rules.rs`): new `Check::{DocumentFields, RevisionSequence, BalloonRows, CrossReferences}`.

Sheet sync pass (new `crates/notation/src/sheet_pass.rs`, one call line in `bind::sync` after step 8):
- fills query compartments from `vizcraft_model::query::list`;
- sets each balloon's name to its part's tag;
- keeps one generated cross-reference label per view of an element with partners. Partners are reached through crossReference relationships, or by sharing a tag in a paired scope. The label is a text-box child (`parent` = the view; `ext vizcraft.xref.for`) placed below the view. Its text is the partners' locations in the configured form, one per line. A label moved by the user stays where it was put; the label is removed when the element has no partners.
- Idempotent.

Identity (`crates/model/src/identity.rs`):
- `Part::{Owner(Option<type>), Page(pad), Zone, Rung(pad)}` parsed from `{owner.tag}`, `{owner:TYPE.tag}` (the nearest ancestor of TYPE), `{page}`, `{page:W}`, `{zone}`, `{rung}` and `{rung:W}`.
- `ComposeCtx { page, zone, rung }` and `IdFormat::compose_in` / `number_in_ctx`.
- Owner parts resolve through `Registry`'s model, so `next_tag` keeps its signature. `=F1+A2-K3`, `CR-4-25` and `2003` (`{page}{rung:3}`) compose.
- `IdScope.perOwner: bool` (default false) numbers and checks uniqueness within each owner.

Commands, each a `CommandSpec` with a test:

| Command | Params and result |
|---|---|
| `sheet.set` | `{page?, standard?, size?, orientation?, zones?: {cols?, rows?}, number?: n\|null, titleBlock?, revisionScheme?, off?: bool}`. Sets the page to the trimmed size, board fixed, units mm for ISO and in for ASME, sets `View.variant` on a sheet-pack view, regenerates the frame; one undo step |
| `sheet.get` | `{page?}` -> `{sheet, geometry, zones}` |
| `sheet.list` | `{}` -> per page: `{page, number, count, size, standard}` |
| `sheet.sizes` | `{}` -> standards and sizes with default zone counts |
| `sheet.frames` | `{page?\|all}` -> regenerated counts |
| `sheet.zoneAt` | `{page?, x, y}` -> `{zone, row, col}` |
| `sheet.revise` | `{description, approved?, date?, zone?}` -> `{rev, element}`. Adds a Revision element with the next letter and sets `revisionIndex` |
| `sheet.balloon` | `{part, page?, x?, y?}`. Places a balloon and its glued leader |
| `sheet.renumber` | `{scope?, page?}`. Recomposes context tags, keeping their numbers |
| `doc.setProperty` | `{key, value}`. Refuses keys no loaded pack declares and wrong kinds; keeps a value over the limit and reports it |
| `doc.properties` | `{}` -> `[{key, label, group, required, maxChars, value, issue?}]` |
| `xref.list` | `{page?, element?}` -> `[{relationship, from: {element, tag, page, sheet, zone}, to, text}]` plus `unresolved` |
| `xref.go` | `{id, index?}`. Makes the partner's page active and selects its view; not an undo step |
| `page.setup` | gains `size?` (a `PAPER_SIZES` name) |

Done when:
- `zone_of` unit tests give A3 8x6 and A4 6x4 with no I or O, and ASME counts and direction.
- A frame golden SVG and PDF for ISO A3 and ASME D.
- The existing decor tests and goldens pass unchanged.
- The title block shows the set properties through `{{doc:*}}`, follows `doc.setProperty` and is idempotent under `refresh`.
- `sheet.revise` from `-` gives A, B, …, H, J (I skipped) under ASME, and Y is followed by AA.
- A parts list fills from three parts and a balloon shows its item.
- `=F1+A2-K3` composes from an owner chain and `perOwner` numbers K1 under each owner.
- A contact to coil `sheet.References` across pages labels both ends `/2.C3` and `/1.B4`, and moving the coil updates the label.
- `xref.go` lands on the partner.
- An unmatched off-sheet reference raises an issue.
- Native round trips of `Page.sheet`, `fields`, `columns` and `perOwner`; a 1.0 document opens unchanged.
- `vizcraft-cli validate` passes the cases in `crates/notation/packs/tests/sheet/`.
- `hostile_params_never_panic` is green with junk sizes, zones and keys.
- cargo xtask ci is green.

**(2) Own**
- New: `crates/diagram/src/sheet.rs`, `crates/diagram/src/title_block.rs`, `crates/notation/packs/sheet.json`, `crates/notation/packs/tests/sheet/`, `crates/notation/src/sheet_pass.rs`, `crates/engine/src/cmd/sheet.rs`, `crates/engine/src/cmd/xref.rs`.
- Existing:
  - `crates/diagram/src/decor.rs`: the sheet arm in `build`; the keep and remove rule in `set` and `refresh`.
  - `crates/diagram/src/text_fields.rs`: Zone, Sheet, Sheets and Doc fields.
  - `crates/diagram/src/print_fields.rs`: `{sheet}` and `{sheets}`.
  - `crates/diagram/src/doc_fields.rs`: `DocProperties.fields`.
  - `crates/diagram/src/compartment.rs`: `columns`, `Band.columns`.
  - `crates/formats/src/{svg,pdf}/compartments.rs`: column drawing.
  - `crates/engine/src/cmd/document.rs`: `doc.setProperty`, `doc.properties`.
  - `crates/engine/src/cmd/page.rs`: `setup {size}`.
  - `crates/notation/src/catalog.rs`: one `include_str` row.

**(3) Shared** (the main session merges; one hunk each)
- `crates/notation/src/schema.rs`: `Pack.documentProperties`, `PropertySchema.maxChars`/`group`, `RelationshipType.kind`, `CompartmentDecl.query`. b6e-builders adds `Glyph.builder` and b6-templates `Pack.templates` there.
- `crates/notation/src/loader.rs`: checks for those four.
- `crates/notation/src/rules.rs`: the four `Check` variants.
- `crates/notation/src/bind.rs`: one `sheet_pass::run` call line after step 8. b6e-substrate owns the net pass.
- `crates/model/src/identity.rs`: the `Part` variants, `ComposeCtx`, `compose_in`, `number_in_ctx`, `IdScope.perOwner`.
- `crates/diagram/src/model.rs`: `Page.sheet` with its serde default, `Page::new` and sanitising. b6e-substrate adds glue and `Shape.orientation`.
- `crates/diagram/src/lib.rs`: two module lines.
- `crates/engine/src/cmd/mod.rs`: `sheet` and `xref` spec lines.
- `crates/engine/src/cmd/design.rs`: the sheet-on-border note in `set_border_title`.
- `crates/formats/src/native_tests.rs`: one round-trip test.
- `crates/engine/src/tests.rs`: hostile cases.
- `crates/model/src/types.rs` is untouched.

**(4) What the frontend branch needs**
- A Sheet ribbon group: standard and size menus from `sheet.sizes`, Frame (`sheet.frames`), Title block, a Revise dialog (`sheet.revise`), Balloon (`sheet.balloon`).
- Page Setup's Size menu fed by `sheet.sizes` instead of `page_setup::PRESETS`, plus a Sheet standard menu.
- Status bar zone from `sheet::zone_of`, or `sheet.zoneAt` over the control channel.
- Canvas drawing of `Band.columns`. Until then cells show tab-spaced.
- A click on an xref label (`ext vizcraft.xref.for`) runs `xref.go`; hover shows `xref.list`.
- The Sheets navigator from `sheet.list`.
- The Insert Field menu must filter `doc` the way it filters `data` (`ribbon/insert/text.rs`), with a key submenu from `doc.properties`.
- A document property form in the Inspector from `doc.properties`.
- `ui.inspect` fields for the sheet and zone.

**(5) Risks and open questions**
- ASME margins, zone counts and zone direction are secondary sources. Check them against MIL-STD-100G and NASA GSFC before the goldens freeze.
- The ISO 7200 block size is our own (the 180 mm width is unconfirmed).
- Generated cross-reference labels are foreground shapes:
  - Does `parent` on a non-group shape move and select correctly in `arrange`?
  - Should a deleted label come back, or does it need a hide flag?
  - The alternative is a callout, `container.rs`.
- `{page}`, `{zone}` and `{rung}` tags change only through `sheet.renumber`, which is EPLAN's on-demand behaviour. The question is whether they should update live instead.
- `{rung}` reads the element's `rung` property until b6e-electrical's ladder tool writes it.
- b6-topology must agree on the `title_block::Block` API before wave 2.
- `schema.rs`, `bind.rs` and `identity.rs` hunks conflict with b6e-builders, b6e-substrate and b6-templates. Merge in the order substrate, sheets, builders.
- Struct literals of `Page` or `CompartmentDecl` outside `::new` will break when the fields are added; grep for them in the first commit.
- Mixed sheet and non-sheet pages: `{{sheet}}` falls back to the page number.

## Trademarks

Claude is a trademark of Anthropic, PBC (Opus is one of its Claude models); EPLAN of EPLAN GmbH &
Co. KG. The names are used only to identify products and formats; VizCraft is not affiliated with
or endorsed by their owners.
