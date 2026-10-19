# `b6-uml` (engine half), Block 6 wave 2

Opus, medium effort. Port 8063. Worktree `.claude/worktrees/b6-uml`, branch `b6-uml` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (now; the session names
the commit). `CARGO_TARGET_DIR=target-b6-uml CARGO_INCREMENTAL=0`. Commit prefix `B6:`.

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

- `Pack.sections` is the seam for `b6-sequence`: it appends one `{"id": "sequence", ...}`
  section object as the last element of `sections`. Leave `uml.Sequence` out and do not assert
  viewpoint, rule or section counts in tests that sequence would break.
- `cmd/file/merge.rs` hunk (container shapes with `import.type` become owner elements, connector
  `import.type` and `import.prop.*` become typed relationships with properties) is built here
  first; `b6-er` starts after this branch merges and reuses it. Keep it general, not UML-specific.
- `RelationshipType.end_labels` and `Check::EndMarkerMatchesProperty` are general mechanisms
  `b6-er` builds on; keep them pack-neutral.

## Draft brief

**(1) Brief**

**Goal.** Ship a UML 2.5.1 pack with these viewpoints: Class, Component, Package, Deployment, Use Case, Activity and State Machine. It brings typed member rows, the six relationship ends, the rules from the standards table and seven templates. Add a PlantUML writer, plus Mermaid class and state diagrams read into the model and written back. Everything is headless and reachable by command id. Sequence is not in this branch; `b6-sequence` adds it later as one appended section.

**Pack.** New `crates/notation/packs/uml.json`, plus one `BUILTIN` line in `catalog.rs`.
- **Layout and the Sequence seam.**
  - New field `Pack.sections: Vec<PackSection {id, label, glyphs, connectorTypes, elementTypes, relationshipTypes, connections, conditionalStyles, palette, viewpoints, rules, rulesets, templates}>`.
  - New `crates/notation/src/sections.rs::flatten(&mut Pack)` runs first in `loader::finish`. It appends each section's lists to the pack's lists. A section ruleset whose id already exists extends that ruleset's `rules` instead of being added. Duplicate or non-lowercase section ids and more than 32 sections are problems.
  - `Pack.sections` is serialized with `skip_serializing_if` empty, so `describe` and `vizcraft://notation` still show flat lists.
  - Top level of `uml.json` holds the shared core: `uml.Note`, `uml.Anchor`, the Association family, Generalization, Realization, Dependency and Usage, and the ruleset `uml.uml251` (first, so it is the default).
  - `sections` lists `class`, `component`, `package`, `deployment`, `usecase`, `activity` and `statemachine`, in that order. **`b6-sequence` appends one `{"id": "sequence", ...}` object as the last element of `sections`. That is its whole hunk.**
- **Element types.**
  - **Class section.**
    - `uml.Class` has properties `attributes` and `operations` (text, one member per line), `isAbstract` (bool) and `stereotype`.
    - Its compartments are `attrs` and `ops`, each `{kind: list, rowsFrom: "attributes"}` (and `"operations"`).
    - A conditional style `{property: isAbstract, equals: true, italic: true}`.
    - `uml.Interface` and `uml.Enumeration` (with `literals`) use `keyword` `interface` and `enumeration`.
  - **Package section:** `uml.Package` (container, tabbed-folder path glyph).
  - **Component section:** `uml.Component` (container, keyword `component`, tab icon as `detail`).
  - **Deployment section:** `uml.Artifact`; `uml.Node` (container, 3-D box path); `uml.Device` and `uml.ExecutionEnvironment` (both `extends: uml.Node`).
  - **Use case section:** `uml.Actor` (stick-figure path), `uml.UseCase` (ellipse), `uml.Subject` (container boundary).
  - **Activity section:** `uml.InitialNode`, `uml.ActivityFinal`, `uml.FlowFinal`, `uml.Action`, `uml.ObjectNode`, `uml.Decision`, `uml.ForkNode`, `uml.JoinNode`, `uml.Partition` (container), `uml.SendSignal`, `uml.AcceptEvent`, `uml.TimeEvent`.
  - **State machine section:** `uml.StateMachine` and `uml.Region` (containers); `uml.State` (container, properties `entry`, `do`, `exit` shown in a paragraph compartment); `uml.Initial`, `uml.Final`, `uml.Choice`, `uml.Junction`, `uml.Fork`, `uml.Join`, `uml.ShallowHistory`, `uml.DeepHistory`, `uml.Terminate`.
  - Every `path` glyph carries a `source` string naming the UML 2.5.1 notation clause it follows (OMG formal/17-12-05).
- **Ends.** No new `Arrow` is needed.
  - `uml.generalization`: end `hollowTriangle`, source is the child.
  - `uml.realization`: dashed, end `hollowTriangle`.
  - `uml.composition`: start `diamond`. `uml.aggregation`: start `hollowDiamond`. For both, source is the whole and the relationship type's `endArrow` is `none`.
  - `uml.dependency`: dashed, end `open`.
  - `uml.association`: solid, no ends. `uml.directedAssociation`: end `open`.
  - Keyword-labelled dashed `open` types: `uml.usage` («use»), `uml.import`, `uml.merge`, `uml.include`, `uml.extend`, `uml.deploy`, `uml.manifest`.
  - `uml.transition` and `uml.controlFlow`: end `open`, with properties `trigger`, `guard`, `effect` (control flow: `guard`).
  - `uml.communicationPath`: solid. `uml.anchor`: dashed.
  - Association-family properties: `sourceMultiplicity`, `targetMultiplicity`, `sourceRole`, `targetRole`, and `navigability` (enum `unspecified`, `target`, `source`, `both`).

**Schema additions** in `schema.rs`. All are additive with serde defaults.
- `CompartmentDecl.rows_from: String`: the property's text, one row per line. This is the typed-rows contract `b6-er` reuses.
- `ElementType.keyword` and `RelationshipType.keyword`, at most 64 bytes.
- `RelationshipType.end_labels: EndLabels {start: Vec<String>, end: Vec<String>}`: property keys joined by a space, role first.
- `ConditionalStyle.italic: Option<bool>`.
- `Variant.options: BTreeMap<String, String>`.
- `Pack.sections`.
- `Check::Unreachable` gains `#[serde(default)] types: Vec<String>` (the elements checked; empty keeps today's behaviour) and accepts an empty `relationship`.

**Member grammar.** New `crates/notation/src/uml_text.rs`:
- `parse_member(&str) -> Member {visibility, name, ty, multiplicity, default, params, ret, modifiers, kind}` and `format_member(&Member, Visibility)`.
- `parse_multiplicity(&str) -> Result<(u32, Option<u32>)>`. It accepts `n`, `*`, `n..m` and `n..*` with n ≤ m.
- Inputs are capped and nothing panics.
- Static and abstract members are written as a leading `{static}` or `{abstract}` modifier until rows can carry markup.

**Sync (`bind.rs`).**
- Step 5 shows `«keyword»` (or `«stereotype»`) on a line above the name. When text comes back, a leading `«…»` line is stripped before the text becomes the name. The pass stays idempotent.
- Step 6 applies `italic`.
- Step 8 fills `rows_from` through `format_member` with the page variant's `options.memberVisibility` (`symbols` by default, or `keywords` or `hidden`). `options.members: suppressed` leaves the rows empty.
- New step 9 sets the connector's `start_label` and `end_label` from `end_labels`. An end label changed by the command writes back to its property, the same way the text and name rule works.
- Variants:
  - `uml.keywords`: visibility shown in words.
  - `uml.lollipop`: `uml.Interface` drawn as a ball glyph, `uml.Realization` drawn as a plain solid line.
  - `uml.nameOnly`: compartments suppressed.

**Checks.** New `Check` variants in `schema.rs`, run by one dispatch arm in `rules.rs` into new `crates/notation/src/rules_uml.rs`, with arms in `validate.rs`:
- `EndMarkerMatchesProperty {relationship, property, markers: [{value, start?, end?}]}`: generic, so `b6-er` can use it for crow's foot. A mismatch gets the fix `connector.setArrows {ids, start, end}`.
- `MultiplicitySyntax {types, properties, members}`.
- `MaxPerOwner {types, within, max}`: elements with no `within` ancestor are grouped by the page they are first drawn on.
- `UniqueMembers {types, property}`: attribute names only, since operations may overload.

**Rules in `uml.uml251`.** Each rule's `reference` names the UML 2.5.1 constraint it comes from (`no_cycles_in_generalization`, `initial_vertex`, MultiplicityElement 7.5.4).
- `illegalRelationship`: the connection table limits Realization targets to Interface.
- `multiplicity`: Composition has `inMax 1`, so a part has at most one whole.
- `forbiddenCycle` on `uml.Generalization` (error, batch) and on `uml.PackageImport`.
- `multiplicitySyntax`, `uniqueMembers`, and `endMarkerMatchesProperty` for navigability (info).
- `maxPerOwner {types: [uml.Initial], within: [uml.Region, uml.State, uml.StateMachine], max: 1}`.
- `unreachable` from `uml.Initial` along `uml.Transition` over the states and pseudostates.
- `minOutgoing` 1 on `uml.State` for dead ends (info), and 2 on `uml.Choice`.
- `minIncoming` 2 on the join types.
- Actions reachable from `uml.InitialNode`; use cases reachable from `uml.Actor` with `relationship: ""`.
- `minIncoming` Realization on Interface for satisfied interfaces (info).
- `minOutgoing` Deploy on Artifact (info; nesting the artifact in a node is the alternative).
- Plus viewpoint, dangling connector and property type.

**Templates** in `uml.json` sections. Seeds use `element.create {type, name, properties, owner, x, y}` with `as` and `${x.id}`.

| Template | Layout | Code forms |
|---|---|---|
| `uml.classDiagram` | layered TB | `mermaid.class`, `plantuml` |
| `uml.stateMachine` (inside a `uml.StateMachine`) | layered LR | `mermaid.state`, `plantuml` |
| `uml.activity` | layered TB | `plantuml` |
| `uml.useCase` | radial | `plantuml` |
| `uml.component` | layered LR | `plantuml` |
| `uml.deployment` | nested primitive | `plantuml` |
| `uml.package` | layered TB | `plantuml` |

The seeds follow the catalogue rows.

**Formats.**
- New `crates/formats/src/uml.rs` reads a page as UML: the elements it draws, members, owners and relationships.
- New `crates/formats/src/plantuml.rs` provides `write(doc, page) -> Result<(String, Vec<String>), FormatError>`, picking the diagram by the page's viewpoint. It writes ids as aliases, escapes names and caps output at 2 MB. Activity writes structured flows; edges it cannot structure come back as warnings.
- New `crates/formats/src/mermaid/class.rs` and `mermaid/state.rs`, each with `build` and `write`:
  - Plain import draws with `uml.*` masters and filled compartments.
  - Shape and connector `ext` carry `import.type` and `import.prop.*`.
  - Composite states, regions and namespaces become container shapes with `import.type`.
  - `[*]` ids become plain ids `<parent>_initial` and `<parent>_final`.
  - Cardinalities map to the multiplicity properties, with ends reversed where Mermaid writes them reversed (`A <|-- B` means B extends A).
  - Writers use `class e12["Order"]` and `state "Idle" as e5`, so ids are stable.
- `formats/Cargo.toml` gains `vizcraft-notation`.

**Commands.** No new ids. `file.export {path, format?: "mermaid"|"plantuml"}` (extensions `.mmd`/`.mermaid` and `.puml`/`.plantuml`) writes the active page and returns `warnings`. `file.import {format: "mermaid", text, merge: true}` reads class and state diagrams into the model.

**Done when.**
- **Loading:** `uml.json` loads through `load_shipped` with provenance on every glyph.
- **`sections` tests:** a section appends to the lists; a matching ruleset id extends the rule list; a duplicate section id is a problem.
- **`uml_text` tests:** 20 member rows round-trip; good and bad multiplicities (`5..2`, `..`, `-1`); hostile rows never panic.
- **`rules_uml.rs` unit tests** for each new check.
- **Bind tests:**
  - `attributes` lines become rows.
  - `variant.set uml.keywords` shows `public name: String`.
  - The «interface» line comes back off the text: editing it renames the element.
  - `isAbstract` turns italic.
  - Multiplicities show as end labels, and an edited end label writes back to its property.
- **Pack cases** in `crates/notation/packs/tests/uml/`: `class_clean`, `generalization_cycle`, `realization_to_class`, `composition_two_wholes`, `bad_multiplicity`, `duplicate_attribute`, `navigability_marker`, `two_initials_in_region`, `unreachable_state`, `use_case_without_actor`, `package_import_cycle`, `join_one_inflow`. `cargo run -p vizcraft-cli -- validate crates/notation/packs/tests/uml` exits 0.
- **Golden files** in `crates/formats/testdata/uml/`:
  - `class.mmd` and `state.mmd`, each with a `.out.mmd` expected output; read → write → read gives an equal model.
  - One `.puml` per template.
  - Regenerate with `VIZCRAFT_UPDATE_GOLDEN=1`.
- **Engine integration test** `crates/engine/tests/uml.rs`:
  - The merge import yields `uml.Class` with `attributes`, typed Generalization and Composition relationships with multiplicities, and Package and Region owners.
  - Re-importing an edited text keeps ids and positions.
  - Each `template.new` followed by `file.export` equals its golden.
  - The import is one undo step.
- **CLI test** `apps/vizcraft-cli/tests/uml.rs`: validate the folder, then `new --template uml.classDiagram OUT` and `run --open OUT --cmd file.export=…puml`.
- **Existing tests:** `every_shipped_template_builds…` and `hostile_params_never_panic` stay green.
- `cargo xtask ci` is green.

**(2) Own:**
- `crates/notation/packs/uml.json`
- `crates/notation/packs/tests/uml/*`
- `crates/notation/src/{sections,uml_text,rules_uml}.rs`
- `crates/formats/src/{uml,plantuml}.rs`
- `crates/formats/src/mermaid/{class,state}.rs`
- `crates/formats/testdata/uml/*`
- `crates/engine/tests/uml.rs`
- `apps/vizcraft-cli/tests/uml.rs`
- Retaken snapshot PNGs, if any.

**(3) Shared, with the exact hunk this branch adds:**
- **`notation/src/schema.rs`:** the fields and variants listed under "Schema additions" and "Checks". `b6-topology` (`nestingAllowed`) and `b6-sequence` (`messageOrder`) add their own variants next to them.
- **`notation/src/loader.rs`:** one `crate::sections::flatten(&mut pack);` line at the top of `finish`.
- **`notation/src/validate.rs`:** arms for the four checks and the new fields.
- **`notation/src/rules.rs`:** one dispatch arm, plus the `types` filter in `unreachable`.
- **`notation/src/bind.rs`:** steps 5, 6 and 8 hunks and new step 9.
- **`notation/src/lib.rs`:** three `mod` lines.
- **`notation/src/catalog.rs`:** the `("uml.json", include_str!(…))` line.
- **`stencils/src/lib.rs` test:** `search("")` counts only `basic.` and `flowchart.` ids. `b6-er` and `b6-topology` should make the identical edit.
- **`formats/src/mermaid.rs`:**
  - `mod class; mod state;`
  - the import match arms `Diagram::Class(d) => class::build(d, &mut warnings)?` and `Diagram::State(d) => state::build(…)?`
  - new `pub fn export(doc, page)`, a dispatch on the page's viewpoint pack with the class and state arms. `b6-er` adds its ER arm and `b6-topology` its architecture arm.
- **`formats/src/lib.rs`:** `pub mod plantuml; pub mod uml;`
- **`formats/Cargo.toml`:** one dependency line.
- **`engine/src/io.rs`:** `"mermaid"` and `"plantuml"` in `EXPORT_FORMATS`, the extension aliases, and two `export_page_bytes` arms. `b6-er` adds `dbml` the same way.
- **`engine/src/cmd/file.rs`:** the `file.export` params string.
- **`engine/src/cmd/file/merge.rs`:**
  - A container shape with `import.type` becomes an element; members take `owner` from their nearest such ancestor.
  - A connector's relationship type comes from its `import.type`, else its kind's `relationshipType`, else `legal::relationship_for`.
  - Connector `import.prop.*` become relationship properties.
  - `b6-topology` and `b6-er` need exactly this. Whichever merges first lands it; the others reuse it.

**(4) What a later frontend branch needs:**
- **Inspector member rows:** edit `attributes` and `operations` lines, with a visibility picker and multiplicity validation from `uml_text`.
- **Inspector ends:** relationship end editing for role, multiplicity and navigability, with the fix offered.
- **Row markup:** underlined static and italic abstract members on the canvas and in SVG, replacing the `{static}` text.
- **Ball and socket:** a `socket` `Arrow` (`arrows.rs` and canvas) for assembly connectors.
- **Containment:** dropping a shape into a container sets its model `owner` (needed by `maxPerOwner` and the writers).
- **Gallery and Variant menu:** New gallery cards and previews from `template.list` and `template.preview`, and the Variant menu entries for the three UML variants.
- **Import and Code pane:** a "merge into model" choice in the import UI. `b6-code` adopts the `class`, `state` and `plantuml` writers behind `CodeForm`.

**(5) Risks and open questions:**
- **PlantUML is not yet an allowed source.** AGENTS.md "Formats" must list the PlantUML language reference before the branch starts (main session).
- **`rowsFrom`, `endLabels` and `EndMarkerMatchesProperty` overlap with `b6-er`'s needs.** The `b6-er` brief must say to reuse them. Merge `b6-uml` first, or let `b6-er` cherry-pick the schema commit.
- **The keyword line is held in shape text.** Free text edits on the first line, and drawio and SVG round trips, need tests. The alternative is a frontend-drawn header band.
- **Variants are single-choice.** Combinations (keywords plus lollipop) are not possible; per-page options could follow later.
- **PlantUML activity export is partial.** Unstructured graphs are partly lost (warned); the alternative is to export activity through the state syntax.
- **Approximate rules.** "Required interfaces satisfied" and "artifact deployed" are approximations at info severity. Dead-end composite states may need an exception.
- **Changing an existing variant.** Changing `Unreachable` touches a variant other branches match on; the field is defaulted, so old packs load.
- **Snapshots may change** once the UML Shapes set exists.
