# `b6-er` (engine half), Block 6 wave 2

Opus, medium effort. Port 8065. Worktree `.claude/worktrees/b6-er`, branch `b6-er` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (after b6-uml merges; the session names
the commit). `CARGO_TARGET_DIR=target-b6-er CARGO_INCREMENTAL=0`. Commit prefix `B6:`.

## Resolutions (these override the draft below where they differ)

Common to wave 2 (see docs/cascade-plan.md "Block 6 wave 2"):

- A separate session is building `b6e-frontend` (the engineering drawing UI) in
  `crates/ui-egui` at the same time. Change no source under `crates/ui-egui` or `apps/vizcraft`.
  If a new pack changes ui-egui snapshot images, retake only those PNGs and list them; the
  integration session regenerates snapshots after the merges anyway.
- Formats come from public documentation only; AGENTS.md lists PlantUML, DBML, Structurizr DSL
  and D2 as of a979a97.
- Mermaid grammar mappings live in `crates/formats/src/mermaid/<grammar>.rs` (the folder already
  holds `sequence.rs`), one module line and one dispatch arm in `mermaid.rs` each.
- New `Check` variants go before `Unknown`; new bind passes get a named function and one call
  line in `bind::sync`, not a renumbering of existing steps.

For this branch:

- Starts after `b6-uml` merges, from the integration branch at that point. `b6-uml` already
  built: the `cmd/file/merge.rs` import of container owners, connector `import.type` and
  `import.prop.*`; `RelationshipType.end_labels` with a bind pass that writes end labels; and
  `Check::EndMarkerMatchesProperty`. Reuse them: add `import.children` to the same merge code
  rather than a parallel path, and make the end pass (`ConnectorType.ends`) the general mechanism
  that can also set UML end labels, or keep UML's pass and document why both exist. Do not leave
  two passes writing the same connector fields.
- Mermaid ER goes in `crates/formats/src/mermaid/er.rs`, not `mermaid_er.rs`.
- Keep `Arrow::ALL` as the 14-entry picker list and add `Arrow::EVERY`, as drafted.
- Columns as owned `er.Attribute` elements is approved; class members in UML stay text rows.

## Draft brief

# `b6-er` (engine half), Block 6 wave 2

Opus, medium effort. Port 8065. Worktree `.claude/worktrees/b6-er`, branch `b6-er` from the integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that adds this file; drafted at f9f2029). `export CARGO_TARGET_DIR=target-b6-er CARGO_INCREMENTAL=0`. Commit prefix `B6:`. Backend only: change no source file under `crates/ui-egui` or `apps/vizcraft`. If the new pack changes ui-egui snapshot images, run `UPDATE_SNAPSHOTS=1 cargo test -p vizcraft-ui-egui --test snapshots` and list only the retaken PNGs. Formats come from public documentation only: DBML from https://dbml.dbdiagram.io/docs/, Mermaid erDiagram from the Mermaid syntax docs. The main session adds DBML to AGENTS.md and docs/formats.md at merge.

**Goal.** An entity-relationship pack with Conceptual, Logical and Physical viewpoints. It has four notations through the Block 5 variant machinery: IE (crow's foot, the default drawing, also listed as `er.ie`), `er.chen`, `er.barker` and `er.idef1x`. `variant.set` switches between them without re-placing anything, as one undo step. Also in scope:
- the line ends, per-half dash, double lines and double outlines the notations need, drawn by SVG and PDF;
- typed column rows;
- an ER layout primitive;
- DBML read and write;
- Mermaid erDiagram read into the model and written back;
- four rules;
- three templates.

All of it is reachable by command id and checked over the CLI and the control channel.

**Diagram substrate (`crates/diagram`).**
- **New ends.** `Arrow::DoubleBar` (two bars: one and only one) and `Arrow::Dot` (a small filled dot on the line end, for the IDEF1X child end) join the `outline` table in `arrows.rs`; Dot is the only new filled end. The canvas draws every end through this table already. `Arrow::ALL` stays the 14-entry picker list, because `ui-egui/src/ribbon/connector/arrows.rs` has a test that pins it. Add `pub const EVERY: [Arrow; 16]` (ALL plus the new ends) and switch the arrows.rs tests, `formats/tests/arrows.rs` and `native_tests.rs` to it.
- **Double outline.** `Style.compound: Compound {Single (default), Double}` (enum in `style_fields.rs` via `named_enum!`, field in `model.rs` `Style` and `Style::basic`, serde default, skipped when Single).
- **New file `crates/diagram/src/connector_look.rs`.**
  - `Half {dash: Option<Dash>, double: bool}` and `Halves {start: Half, end: Half}`, stored as `Connector.halves` (serde default, skipped when default, sanitised).
  - `LabelFrame {Box (default), Diamond, DoubleDiamond}` as `Connector.label_frame`.
  - `split_half(polyline) -> (Vec<Point>, Vec<Point>)` cuts at half the arc length.
  - `parallel(polyline, d)` gives the two offset lines of a double line, mitred at bends.
  - `inner_outline(&Shape) -> Option<Path>` is the kind's outline of the frame inset by `2.5 * line_width` (rectangle, rounded rectangle, ellipse, diamond, freeform; none when too small).
  - `label_frame_path(center, w, h, frame)`.
- **SVG.** `formats/src/svg.rs` draws all of this in `shape_svg` and `connector_svg`; PDF and PNG follow because they render the SVG.
- **draw.io.** `formats/src/drawio/{write,read}.rs` map DoubleBar to and from `ERmandOne` (today `ERmandOne` reads as Bar) and Dot to `oval`.

**Pack `crates/notation/packs/er.json`.** Our own colours. Registered with one `BUILTIN` line in `catalog.rs`.

Element types:
- `er.Entity` (rectangle with a name band). Properties: `schema`, `note`, `indexes` (DBML index lines kept verbatim), `group` (DBML TableGroup).
- `er.WeakEntity`, which `extends` `er.Entity`.
- `er.Attribute` (an ellipse glyph, used only by Chen). Properties: `dataType`, `pk`, `fk`, `unique` and `increment` (bools), `nullable` (bool, default true, as in DBML), `default`, `note`, and `references` (`<entity name>.<column name>`).
- `er.Enum` (Physical only). Property: `values`, one per line.

Columns are `er.Attribute` elements whose `owner` is their entity. This is the model truth for rules, DBML, re-import and the Model Tree. They are drawn as rows of two table compartments on the entity: `keys` (pk true, above the key line, as IE and IDEF1X draw it) and `attrs` (the rest).

Relationship types:
- `er.Relationship` (non-identifying) and `er.Identifying`.
- Both carry required enums `sourceCardinality` and `targetCardinality` with values `one`, `zeroOrOne`, `oneOrMore`, `zeroOrMore`, plus `onDelete` and `onUpdate`.
- Direction is parent (the referenced side) to child.
- Connections: Relationship from `*` to `*`; Identifying from `[er.Entity]` to `[er.WeakEntity]`.

Variants:
- `er.ie` maps nothing.
- `er.chen`: weak entity drawn as a double-outline rectangle; relationship drawn straight with a `diamond` label frame; identifying with a `doubleDiamond` frame.
- `er.barker`: rounded boxes; both relationship types solid.
- `er.idef1x`: weak entity as a rounded rectangle (identifier-dependent); identifying solid, non-identifying dashed (dashed is also the IE default).

Viewpoints `er.Conceptual`, `er.Logical`, `er.Physical`, with matching rulesets.

**Schema hunks.** The ends come from each relationship's properties, not from the connector type alone, so:
- `ConnectorType.ends: Option<EndRules {start: Vec<EndRule>, end: Vec<EndRule>}>` with `EndRule {property, values: BTreeMap<value, EndLook {arrow?, label?, dash?, double?}>}`. Later rules add fields.
- `ConnectorType.labelFrame: String`.
- `Glyph.compound: String`.
- `Variant.satellites: Vec<String>`: element types drawn as their own shapes around their owner on this variant.
- `Variant.compartments: BTreeMap<type, Vec<CompartmentDecl>>`: overrides the type's compartments, an empty list removes them.
- `TableQuery.owned: bool` (only elements the compartment's element owns), `TableQuery.filter: Option<{property, equals}>`, and `TableQuery.values: BTreeMap<column, BTreeMap<value text, shown text>>`. For example `pk` true shows `PK`; in Barker `pk` shows `#` and `nullable` shows `o` or `*`.
- `Check::PrimaryKey {types, viewpoints}` and `Check::ForeignKey {types}`, dispatched to new `crates/notation/src/er_rules.rs` the way the sheet checks go to `sheet_pass`.

The loader checks every new name, adding to the existing `variants()` check.

**End mapping data (look-across, as Mermaid and the standards draw it).**

| Notation | What each end gets |
|---|---|
| IE | Its own cardinality: `one` doubleBar, `zeroOrOne` barCircle, `oneOrMore` crowsFootBar, `zeroOrMore` crowsFootCircle |
| Chen | Label `1` or `N` (`M` at the source of a many-to-many). The half nearest an entity with total participation (the other end's minimum is 1) is `double` |
| Barker | crowsFoot where the end is many. The half is dashed where the entity there is optional (the other end's minimum is 0). Identifying adds the UID bar: crowsFootBar or bar at the child end |
| IDEF1X | Child end: `dot` with label `P` (oneOrMore), `Z` (zeroOrOne), `1` (one), or nothing (zeroOrMore). Parent end: `hollowDiamond` when `zeroOrOne` |

**Sync passes (two lines in `bind::sync`, after `styles`).**
- **Ends:** new `crates/notation/src/end_pass.rs` sets arrows, end labels, halves and the label frame of every relationship connector whose kind has `ends`. It is idempotent and changes a field only when it differs, so `variant.set`'s `k.apply` and the pass agree.
- **Satellites:** new `crates/notation/src/satellites.rs`. On a page whose variant lists `er.Attribute` as a satellite, it draws each owned attribute as a bound ellipse around its entity. Each is joined by a plain connector with `ext vizcraft.satelliteOf`, and key attributes get `style.underline`. Leaving the variant removes them.
- **Compartments:** bind step 8 and `sheet_pass::tables` resolve declarations through new `Catalog::compartments_in(type, variant)`. They drop a declared compartment the current variant does not declare, and leave the user's own compartments alone.

**Layout.** New `Primitive::Er {satellites: Vec<Satellite<T> {owner, members}>}` (`#[serde(default)]`) in new `crates/layout/src/primitives/er.rs`:
- layered in the flow direction, parents before children;
- orthogonal edges;
- node and layer gaps of at least two end-marker lengths (`8 + 2.5 * lw`) plus the widest end label;
- parallel edges spread;
- satellites on a ring around their owner, clear of the entity's edges.

`layout.primitive {primitive: "er"}` with no roles derives the satellites from the page.

**Formats.**
- New `crates/formats/src/er.rs` holds the IR `Schema {tables, refs, enums, groups}` and `to_page(&Schema, warnings) -> Page`, laid out with the er primitive.
  - Entity shapes: stencil `er.entity`, id = table name when plain, ext `import.type` and `import.prop.*`, and `import.children` (JSON `[{id: "<table>.<column>", type: "er.Attribute", name, props}]`, within `MAX_TEXT`). Preview compartments are added so a plain import draws.
  - Connectors: glued, with `import.type` (`er.Identifying` when the FK columns are all in the child's PK) and `import.prop.sourceCardinality` / `targetCardinality`. The parent end is `one`, or `zeroOrOne` when the FK is nullable; the child end is `zeroOrMore` (DBML cannot say more).
  - `from_doc(&Document, page) -> (Schema, warnings)` reads the ER elements drawn on the page.
- New `crates/formats/src/dbml.rs`: `import(text) -> Imported` and `write(&Schema) -> String`.
  - Read: Project (kept as a warning), Table with schema, alias and settings, column types with arguments and quoted types, settings (`pk`, `primary key`, `null`, `not null`, `unique`, `increment`, `default:`, `note:`, inline `ref:`), `Note:`, `indexes {}` (a composite `[pk]` sets `pk`), short and long `Ref` including composite refs with `<`, `>`, `-`, `<>` and `[delete:, update:]`, Enum, TableGroup, `//` and `/* */` comments, `'''` strings, escapes.
  - Unsupported (sticky notes, TablePartial) becomes a warning.
  - Caps: 2 MB of text, 5 000 tables, 256 columns each.
  - The writer is canonical: model order, two-space indent, fixed setting order. A relationship with no FK columns is left out with a warning.
- New `crates/formats/src/mermaid_er.rs`: `build(&ErDiagram, warnings) -> Result<Page>` goes through the IR. The `vizcraft-mermaid` crate already parses erDiagram fully (entities, aliases, `type name PK, FK "comment"`, the four cardinalities, `--` versus `..`).
  - `write(&Schema) -> (String, warnings)`: `||`, `|o`, `}|`, `}o`; `--` for identifying and `..` for non-identifying; label `: "x"` (required by Mermaid); an `id["Name"]` alias when the name is not a plain Mermaid name; type characters Mermaid refuses are replaced, with a warning.
  - The dispatch line in `mermaid.rs` is `Diagram::Er(er) => mermaid_er::build(er, &mut warnings)?`.

**Engine.**
- Shared `io.rs`: `IMPORT_FORMATS` gains `dbml`, the `.dbml` extension and an `import_bytes` arm, so `file.import {format: "dbml", merge: true}` and `file.reimport` work.
- Shared merge hunk in `cmd/file/merge.rs`:
  - a connector's `import.type` (when the catalog knows it and a connection row allows it) wins over `relationship_for`;
  - `import.prop.*` become relationship properties;
  - `import.children` become owned elements.
- Shared hunk in `model/src/sync.rs`: the name pass matches an owned element only under its matched owner, so the `id` columns of two tables never cross-match; `apply` also updates the properties of matched relationships.
- New `crates/engine/src/cmd/er.rs`:
  - **`er.addColumn {entity, name, dataType?, pk?, fk?, nullable?, unique?, references?, id?} -> {id}`**: one undo step; refuses anything that is not an entity.
  - **`er.setCardinality {ids?: [relationship or connector id], source?, target?} -> {ids}`**.
  - **`er.write {format: "dbml"|"mermaid", page?, path?} -> {text, warnings, path?}`**: b6-code wraps it later as `code.write`.
- Templates use the `Pack.templates` format, with seeds of `element.create {owner, properties}` and `relationship.create {properties}`:
  - `er.schema` "Database schema": Physical, IE, three tables with typed columns, PK and FK, two crow's-foot relationships, primitive `er`, codeForms `dbml` and `mermaid.er`;
  - `er.conceptual`: Conceptual, Chen, two entities with attributes, one 1:N relationship;
  - `er.logical`: Logical, IDEF1X (Barker offered), codeForms `dbml`.
- `template.rs` `PRIMITIVES` gains `er`.

**Done when:**
- **Ends and substrate:** outline tests cover the new ends; `only_solid_heads_are_filled` expects Dot; `crates/formats/testdata/arrows.golden.svg` is regenerated with `VIZCRAFT_UPDATE_GOLDEN=1`; draw.io round trips DoubleBar. `split_half` and `parallel` are tested on orthogonal and diagonal routes. `inner_outline` sits inside the outline for every kind. Old files load with `Compound::Single`, default halves and the Box frame. Inline SVG goldens cover a double rectangle, a half-dashed and half-double connector and a diamond label frame.
- **Layout:** `Primitive::Er` places shapes with no overlaps, keeps the gaps, puts parents first and places satellites, and refuses bad roles.
- **Pack test cases** under `crates/notation/packs/tests/er/`:

  | Case | Expected issues |
  |---|---|
  | `clean_schema.json` | none |
  | `missing_cardinality.json` | `er.cardinality` |
  | `weak_entity_not_identified.json` | `er.weakEntityIdentified` (MinIncoming on `er.Identifying`) |
  | `identifying_to_strong_entity.json` | `er.illegal` |
  | `physical_table_without_pk.json` | `er.primaryKey` (only on Physical pages) |
  | `fk_to_missing_column.json` | `er.foreignKey` |
  | `fk_type_mismatch.json` | `er.foreignKey`, warning |

- **Templates:** every er template builds through `template.new` with no error-level issue.
- **Variant goldens:** `variant.set` across `er.ie`, `er.chen`, `er.barker` and `er.idef1x` on `er.schema` is one undo step each and keeps the shapes' centres. Chen adds bound attribute ellipses, drops the column bands and shows `1` and `N` labels; switching back restores both. One golden SVG per variant at `crates/engine/testdata/er/{ie,chen,barker,idef1x}.golden.svg`.
- **DBML goldens** in `crates/formats/testdata/dbml/{shop,composite_keys,enums_and_groups}.dbml`: `write(read(x))` equals `x.golden.dbml`, and the golden is a fixpoint.
- **Mermaid goldens** in `crates/formats/testdata/mermaid_er/*.mmd`, the same way; `er_diagram_has_attributes_and_cardinalities` is rewritten for the new mapping.
- **Readers never panic:** DBML and Mermaid ER readers on truncated and garbage input.
- **Re-import:** `file.import {format: "dbml", merge: true}`, then edit one column's type, then `file.reimport` keeps ids and geometry, lists the column as `updated`, and flags a dropped column with `import.removed`.
- **Writer:** `er.write` returns DBML for the active page.
- **Hostile params:** `hostile_params_never_panic` gains junk rows for `er.*` and for `layout.primitive {primitive: "er", roles: {satellites: 5}}`.
- `cargo xtask ci` is green.

## Own

- `crates/notation/packs/er.json`
- `crates/notation/packs/tests/er/*.json`
- `crates/notation/src/{end_pass.rs, satellites.rs, er_rules.rs}`
- `crates/diagram/src/connector_look.rs`
- `crates/layout/src/primitives/er.rs`
- `crates/formats/src/{er.rs, dbml.rs, mermaid_er.rs}`
- `crates/formats/testdata/{dbml,mermaid_er}/*`
- `crates/engine/src/cmd/er.rs` (with its tests)
- `crates/engine/testdata/er/*.golden.svg`
- the retaken ui-egui snapshot PNGs, if any

## Shared, with the exact hunk this branch adds

| File | Hunk |
|---|---|
| `diagram/src/arrows.rs` | two variants, two names, two `outline` arms, `EVERY`, test updates |
| `diagram/src/style_fields.rs` | `Compound` |
| `diagram/src/model.rs` | `Style.compound` (and `Style::basic`), `Connector.halves`, `Connector.label_frame`, sanitize calls |
| `diagram/src/lib.rs` | module line |
| `formats/src/svg.rs` | inner outline, halves, double lines, label frame |
| `formats/src/drawio/{read,write}.rs` | the two arrow arms |
| `formats/src/mermaid.rs` | `mod mermaid_er;` and the dispatch line |
| `formats/src/mermaid/tests.rs` | the rewritten ER test |
| `formats/src/lib.rs` | `pub mod dbml; pub mod er;` |
| `formats/tests/arrows.rs` and its golden | `EVERY` |
| `formats/src/native_tests.rs` | `EVERY`, one round trip for the new fields |
| `notation/src/schema.rs` | ConnectorType `ends` and `labelFrame`, `Glyph.compound`, Variant `satellites` and `compartments`, the three TableQuery fields, two `Check` variants |
| `notation/src/loader.rs` | checks for the new fields |
| `notation/src/validate.rs` | the two Check arms |
| `notation/src/rules.rs` | one dispatch arm |
| `notation/src/bind.rs` | two pass lines; `compartments_in` in step 8 |
| `notation/src/sheet_pass.rs` | `owned`, `filter`, `values`, `compartments_in` in `table_rows` and `tables` |
| `notation/src/catalog.rs` | `BUILTIN` line, `compartments_in` |
| `notation/src/template.rs` | `PRIMITIVES` gains `er` |
| `notation/src/lib.rs` | module lines |
| `stencils/src/lib.rs` | `master_from_glyph` reads `compound` |
| `stencils/src/connectors.rs` | `ConnectorKind.label_frame` and `apply` |
| struct literals | `stencils/tests/reload.rs`, `stencils/src/ports.rs`, `notation/src/bind_tests.rs` |
| `layout/src/primitives/mod.rs` | the variant, `name`, `try_map`, `references`, the dispatch arm, the table row |
| `engine/src/cmd/layout.rs` | params doc and default er roles |
| `engine/src/io.rs` | `dbml` import |
| `engine/src/cmd/file.rs` | `IMPORT_PARAMS` text |
| `engine/src/cmd/file/merge.rs` | `import.type` and `import.prop.*` on connectors, `import.children` |
| `model/src/sync.rs` | owner-scoped name pass, relationship properties in `apply` |
| `engine/src/cmd/mod.rs` | `pub mod er;` and `v.extend(er::specs());` |
| `engine/src/tests.rs` | the junk rows |
| `mcp/src/reference.rs` | the erDiagram row |

## What a later frontend branch needs

- **Canvas** (`canvas/draw/shapes.rs`, `connectors.rs`): `Compound::Double` from `inner_outline`, halves from `split_half` and `parallel`, label frames from `label_frame_path`.
- **Arrows menu:** rows for "Double bar" and "Dot", then make `ALL` equal `EVERY` and drop the test's 14-entry pin.
- **Inspector relationship ends:** cardinality pickers running `er.setCardinality`.
- **Column rows:** double-click a row to edit it (`element.setProperty` on the owned attribute), add one with `er.addColumn`.
- **New gallery:** cards for the er templates.
- **Code pane (b6-code):** `er.write` and `file.import {format: "dbml"}`.
- **Drag from a column to another table to create a foreign key:** needs ports per row (`"<compartment>.<row>.left"`), which the compartment ports do not have yet.
- **Snapshots:** palette and template preview snapshots.

## Risks and open questions

- **Rows contract shared with b6-uml.** `TableQuery.owned` and `filter` and the `import.children` merge contract are likely what b6-uml wants for class attributes. Land them here once and point b6-uml at them, or the two branches will conflict in `sheet_pass.rs`, `schema.rs` and `merge.rs`.
- **Same files as other wave 2 branches.** `Arrow` and `arrows.golden.svg` will conflict with any other wave 2 branch that adds ends (b6-uml), and `Primitive` with b6-sequence. Regenerate the goldens at merge.
- **Chen is only partly covered.** Relationships stay connectors with a diamond label frame, so n-ary relationships and attributes on a relationship cannot be drawn. A satellite ellipse the user deletes comes back while the page is Chen (the same behaviour as cross-reference labels). An opt-out property is an open question.
- **Lossy formats.** DBML has no optionality on the child end and no identifying flag; both are derived from the FK columns. Mermaid has no FK targets and limits the type characters. Each loss comes back as a warning; the goldens prove only what each format can say.
- **Hand-set ends are overwritten.** The end pass replaces hand-set arrows on ER connectors, because the ends are the cardinality.
- **Where Chen and IDEF1X labels sit.** The look-across convention for Chen labels and the IDEF1X `P`/`Z`/`1` letters follow the standards digest. Please confirm them before the goldens are fixed.

## Trademarks

Claude is a trademark of Anthropic, PBC (Opus is one of its Claude models); draw.io of JGraph Ltd;
UML of Object Management Group, Inc.; dbdiagram.io of Holistics Software. The names are used only
to identify products and formats; VizCraft is not affiliated with or endorsed by their owners.
