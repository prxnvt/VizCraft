# `b6e-mechanical` (mechanical, annotation, reliability and IDEF0 packs), Block 6E wave 4

Opus, medium effort. Port 8077. Worktree `.claude/worktrees/b6e-mechanical`, branch `b6e-mechanical` from the integration branch `claude/engineering-diagramming-standards-305d6b` (the session names the commit). `CARGO_TARGET_DIR=target-b6e-mechanical CARGO_INCREMENTAL=0`. Commit prefix `B6E:`.

## Resolutions (these override the draft below where they differ)

Common to wave 4 (the owner's decisions of 2026-10-19 and the integration session's resolutions):

- Starts from the integration branch after `b6-doctypes` is merged; the session names the commit.
  Write document types and examples exactly in the format the `b6-doctypes` report and
  `crates/notation/src/template.rs` define; where this draft guessed the format, the merged code wins.
- Defaults (owner): every document type takes the project's standards, IEC symbols and an ISO A3
  landscape sheet with an ISO 7200 title block, unless this brief names a discipline default
  below; every alternative stays one choice away at creation and afterwards. Ladder Diagram uses
  the project default too (JIC and ASME B one click away). P&ID and process diagrams default to
  ISO 10628 (ISA-5.1 one click away). For fluid power, a project that picks JIC symbols draws with
  the ANSI Y32.10 variant (`fluidpower.jic` is an alias of `fluidpower.ansi`).
- Tags (owner): tags built from properties (location, size, loop, code) recompose immediately when
  a property changes, keeping their number, as EPLAN does. Tags with page, zone or rung parts keep
  the sheet's on-demand renumbering.
- Shared generic pieces, one definition for all four branches (whoever merges second drops its copy
  and uses the first; write them exactly so):
  - `IdScope.numberBy: String` (serde default empty): numbering and uniqueness are grouped by that
    property's value (electrical uses `numberBy: "code"`, process `numberBy: "variable"`); no
    separate `numberPer`.
  - `wire.draw {type?: connector type id}`: sets `Connector.kind` through
    `connector_kind::apply_kind`; the type's `net` gives the net kind when `kind` is absent.
  - `ui.tool {tool: "wire"|"pipe", line?: connector type id}` with `CraftApp.line_type`; with no
    line armed, W and P use the first connector type of the active pack whose `net` matches.
  - `Port.netGroup: String`: ports of one shape in the same group pass one net through.
  - Generic net checks live in one `Check::NetCheck {test, ..}` (electrical's definition); process
    and fluid power add tests to it rather than parallel variants where a test is generic
    (unconnected pin or port, single-pin net, duplicate number).
  - Append points (`Check` variants before `Unknown`, `SPECS`, `Arrow::EVERY`, `analyses()`,
    `cmd/mod.rs`, `icons.rs`, ribbon tab lists, catalog rows): append only, keep-both at merge.
- Merge order: electrical, process, fluidpower, mechanical. At most three of the four build at once.
- The owner does visual checks: no in-app walkthroughs; end the report with the Owner checklist,
  with the launch command from the integration worktree after merge.
- Do not remove the "Engineering symbols (preview)" palette section; the integration session does
  that after all four merge.
- Do not edit ROADMAP.md, docs/parity.md, docs/mcp.md, docs/formats.md, docs/control-protocol.md or
  docs/cascade-plan.md; put their rows in the report. No emoji, no AI attribution.
- The owner's rules are in `docs/briefs/owner-source-of-truth.md` (a copy of the file on the
  owner's Desktop; the repo copy is the one to follow). Where a draft below points at the Desktop
  path, read the repo copy.
- Document type format, as merged from `b6-doctypes` (`crates/notation/src/template.rs` documents
  it): `Pack.templates` holds empty document types and `Pack.examples` holds seeded samples, with
  ids unique across both. A type has `id`, `label`, `description`, `discipline` (use
  `engineering`), `audience`, `keywords`, `viewpoint`, optional `viewpoints: [{viewpoint,
  rulesets?}]` (a choice's rulesets replace the type's), `variant`, `sheet`, `titleBlock`, `module`,
  `rulesets`, `liveChecks`, `layout` (its `direction` is the insertion direction and the
  notation's preferred Re-Layout; the layout never runs at creation), `key`, `codeForms`. An
  example has the same fields plus `seed`. A type with a seed fails to load. `key: true` does not
  insert a key on an empty page (a follow-up). New commands: `example.list`, `example.open`;
  `template.new` makes an empty document. Leave a draft's `variant` empty where it says "takes the
  project default", so `Document.standards` decides.
- The Basic primitives are no longer listed in the Palette pane (owner); do not add drawing
  primitives to a pack palette to compensate.

For this branch:

- Merges last of the four. Append-only hunks as listed.
- FMEA: an empty worksheet with its headers counts as setup in the FMEA document type only if the
  merged format allows setup content; otherwise the palette offers the worksheet first.

## Draft brief

# `b6e-mechanical`: mechanical, annotation, reliability and IDEF0 packs (Block 6E wave 4)

Opus, medium effort. Port 8077. Worktree `.claude/worktrees/b6e-mechanical`, branch `b6e-mechanical`, cut from the integration branch `claude/engineering-diagramming-standards-305d6b` after `b6-doctypes` merges. `CARGO_TARGET_DIR=target-b6e-mechanical CARGO_INCREMENTAL=0`. Commit prefix `B6E:`. Build the engine half first (packs, rules, analyses, commands, tests), then the UI half.

Read first:
- AGENTS.md and `/Users/pranav/Desktop/VizCraft-Blocks-6-6E-source-of-truth.md` (owner rules 1, 2, 7, 8 and 10).
- `docs/research/ee-me-diagramming-standards.md`: the template catalogue rows Kinematic through GD&T, and the pack table rows `mechanical`, `annotation`, `reliability` and `idef0`.
- `docs/research/ee-me/mechanical.md`, sections 3, 4 and 6, and "Implications".
- `docs/briefs/b6-wave1/b6e-builders.md` and `docs/briefs/b6e-frontend/report.md`.
- The `b6-doctypes` report and `crates/notation/src/template.rs`, for the document-type and example format.

**Goal.** When this lands, an engineer can open New, pick Kinematic Diagram, Power Transmission, Bond Graph, Control Block Diagram, Annotated Detail Drawing, Fault Tree, Reliability Block Diagram, Event Tree, FMEA Worksheet or IDEF0 Function Model, and get an empty page with:
- that discipline's shapes first in the palette;
- the project sheet: ISO A3 landscape with an ISO 7200 title block by default, with ASME offered;
- the right compliance rules running live.

They can then:
- draw a four-bar linkage and read its mobility;
- trace a gear ratio from motor to load;
- have bond causality assigned automatically;
- list a control loop's Mason terms;
- put datum-resolved feature control frames, welding symbols and surface texture on any sheet, switching ISO and ASME conventions in one undo step;
- get the minimal cut sets of a fault tree, the availability of a reliability block diagram and the outcome frequencies of an event tree;
- keep FMEA RPNs current;
- decompose an IDEF0 box into a child page whose boundary arrows carry FIPS 183 ICOM codes.

Every action is a command id first, and a ribbon button second.

## Scope

**Shared conventions for all four packs**
- Every path glyph carries a `source` that either says `own design` or starts with `drawn from` followed by a written description. Allowed origins are the ISO 3952 or ISO 14617-7 symbol descriptions, NUREG-0492 (Fault Tree Handbook, US NRC, public domain) and FIPS 183 (public domain).
- No glyph is traced from any figure or tool. `cargo xtask assets` (`xtask/src/provenance.rs`) checks this already, so no new asset files and no ATTRIBUTION rows are needed.
- Rule `reference` fields cite the standard and clause name only.
- Ports sit on whole module positions at the glyph's default size.

**`mechanical.json`** (new). Glyph variant `mechanical.iso3952` (default, module `iso2_5mm`). No ANSI or IEEE kinematic symbol set exists, so the Variant menu lists only ISO 3952 and says so. ASME sheets stay choosable.

| Viewpoint | Element types (ports) | Relationship and connector kinds |
|---|---|---|
| `mechanical.Kinematic` | `Joint` (`kind`: revolute, prismatic, screw, cylindrical, spherical, planar, pinInSlot, camContact, rolling; `dof` derived from kind; ports `a`, `b`; tag unique, set by hand, for example A, B), `Ground` (hatched frame), `Plate` (ternary or higher link), `Cam`, `Follower`, `Spring` | `mechanical.Link` (thick straight bar between joints, property `number`); a link touching `Ground` or a `Plate` belongs to that body |
| `mechanical.Transmission` | `Motor`, `Load`, `Gear` (`kind` spur, internal, bevel, worm, rack; `teeth`, `starts`), `Pulley` (`diameter`), `Sprocket` (`teeth`), `Coupling` (kind fixed, compensating, elastic), `Clutch` (kind; `actuation` M H P E drawn as a letter slot; `engaged`), `Brake`, `Bearing`, `FrictionWheel` | `mechanical.Shaft` (thick line: components joined by shaft segments rotate together), `mechanical.Mesh` (thin dotted), `mechanical.Belt` (`crossed`), `mechanical.Chain` (dash-dot) |
| `mechanical.BondGraph` | `Se`, `Sf`, `MSe`, `MSf`, `R`, `C`, `I`, `TF`, `GY`, `J0`, `J1`. Glyphs are the type letters drawn as strokes, with `textPlace` `below` so the parameter shows under the letter | `mechanical.Bond` (harpoon at the power end; `causal` start or end draws the stroke) |
| `mechanical.ControlBlock` | `TransferBlock` (`gain` text, `dynamic` bool), `Gain` (triangle), `Integrator`, `Delay`, `Saturation`, `SummingJunction` (ports `in1`..`in4`, `out`), `BranchPoint`, `Input`, `Output` | `mechanical.Signal` (`sign` + or -, shown as the end label at a summing junction) |

Rulesets:
- `mechanical.kinematic`: rules `mechanical.jointLinks`, `oneGround`, `mobility`, `planarPair`.
- `mechanical.transmission`: rules `mechanical.mounted` (existing `connected`), `supported`, `meshTeeth` (existing `requiredProperty`), `ratioConsistent`.
- `mechanical.bondGraph`: rules `mechanical.causalStroke` (existing `requiredProperty`), `effortSetter`, `flowSetter`, `sourceCausality`, `derivativeCausality` (info).
- `mechanical.controlBlock`: rules `mechanical.blockPorts` (existing `portFanIn`), `signedInputs`, `danglingSignal` (existing `danglingConnector`), `algebraicLoop`.

Citations: "ISO 3952-1, kinematic pairs", "ISO 3952-2, gears", "ISO 3952-3, couplings, clutches and brakes". Bond graph and control rules carry "textbook convention" and no standard reference.

**`annotation.json`** (new). Types:
- `FeatureControlFrame`: `fcf` builder, port `leader`. Adds the property `featureOfSize`.
- `DatumFeatureSymbol`: path glyph. A letter in a square, joined by a leader to a triangle. Property `letter`.
- `DatumTarget`.
- `WeldingSymbol`: `weld` builder, port `arrow`.
- `SurfaceTexture`: `texture` builder, port `surface`.
- Connector `annotation.leader`.

Each builder type declares an optional `standard` property, so a single symbol can override the page's family.

Variants:
- `annotation.iso` (default): ISO 1101, ISO 2553 and ISO 21920 glyph params.
- `annotation.asme`: ASME Y14.5-2018, AWS A2.4 and ASME Y14.36.

Viewpoint `annotation.Detail`. The types are also offered on any sheet page from the Sheet tab.

Rulesets:
- `annotation.gdt`: rules `datumsResolve`, `datumLetters` (I, O and Q excluded, unique), `toleranceValue`, `formNoDatum`, `datumRequired`, `retiredCharacteristic` (concentricity and symmetry flagged only when ASME is in effect), `modifierOnSize`.
- `annotation.welding`: rules `weldSize`, `processNumber` (ISO 4063 reference numbers, a bounded table).
- `annotation.texture`: rule `textureValue`.

**`reliability.json`** (new).

| Viewpoint | Types | Relationships |
|---|---|---|
| `reliability.FaultTree` | `TopEvent`, `IntermediateEvent`, `BasicEvent` (`probability`), `HouseEvent` (`state`), `UndevelopedEvent`, `ConditioningEvent`; gates `And`, `Or`, `Xor`, `PriorityAnd`, `Inhibit`, `Voting` (`k`); `TransferIn` and `TransferOut` (paired identifier scope `reliability.transfer`) | `reliability.Input` (child to gate, orthogonal, no arrow) |
| `reliability.RBD` | `Start`, `End`, `Block` (`availability`, or `mtbf` and `mttr`, or `failureRate` and `repairRate`), `Voting` (`k`), `Standby` | `reliability.Path` |
| `reliability.EventTree` | `Initiator` (`frequency`), `SafetyFunction` (`failureProbability`), `BranchPoint` (`function`), `Outcome` (`consequence`) | `reliability.Success`, `reliability.Failure` |
| `reliability.FMEA` | `FmeaWorksheet` (container; table compartment over the `FailureMode` elements it owns), `FailureMode` with IEC 60812 columns: `item`, `function`, `mode`, `cause`, `localEffect`, `nextEffect`, `endEffect` (required), `severity`, `occurrence`, `detection`, `rpn`, `actions` | none |

Variants:
- `reliability.iec61025` (default): the event and gate shapes as NUREG-0492 describes them.
- `reliability.rectangular`: IEC 60617-12 boxes (&, ≥1, =1, k/n).

Rulesets:
- `reliability.fta`: `oneTopEvent`, `gateInputs` (existing `minIncoming` 2, with Inhibit and Voting excluded), `inhibitInputs`, `votingK`, `acyclic` (existing `forbiddenCycle`), `transfersPaired` (existing `unmatchedPair`), basic events as leaves (existing `illegalRelationship`).
- `reliability.rbd`: `startToEnd` (existing `requiredPath`), `onAPath` (existing `unreachable`), `votingK`, `blockData`.
- `reliability.eta`: `twoBranches`, `endsInOutcome`, `probabilityRange`.
- `reliability.fmea`: `scaleRange` (existing `propertyCompare`, 1 to 10), `rpnCurrent` (fix runs `rel.rpn`), `endEffect` (existing `requiredProperty`).

Citations: IEC 61025:2006, IEC 61078:2016, IEC 62502:2010 and IEC 60812:2018 by clause name.

**`idef0.json`** (new). Variant `idef0.fips183` only.
- `Activity`: the new `idef0box` builder. The box is drawn with ports `I1`..`I6`, `C1`..`C6`, `O1`..`O6` and `M1`..`M6` on the four sides, counts set by the properties `inputs`, `controls`, `outputs` and `mechanisms`. It has a box number slot at the lower right and an optional call port.
- `Boundary`: the ICOM code is its tag.
- `idef0.Arrow`: orthogonal. The `tunnel` property (start, end or none) draws the new `parens` end.
- `idef0.Call`.

Viewpoints `idef0.Context` (A-0) and `idef0.Decomposition`. The node number is kept in `Page.ext["idef0.node"]`.

Ruleset `idef0.fips183`:
- `contextOneBox`;
- `boxCount` (3 to 6; existing `maxPerOwner` for the upper bound);
- `boxNumbers` (1 to n, staircase order);
- `controlAndOutput`;
- `oneCall`;
- `icomCodes` (boundary codes in parent order, or tunnelled);
- `labelled` (existing `labelled`);
- `onPorts` (existing `endsOnPorts`);
- `feedbackRouting` (info).

Citations: "FIPS 183, diagram syntax rules", "FIPS 183, box and arrow syntax rules", "FIPS 183, diagram rules".

**Document types.** The exact JSON follows the `b6-doctypes` format. Every field below stays changeable at creation and afterwards.

| Key | Viewpoint | Standard (variant) | Sheet, title block | Module | Rulesets | Insert |
|---|---|---|---|---|---|---|
| `mechanical.kinematic` | Kinematic | ISO 3952 | project sheet (ISO A3 landscape), ISO 7200 | iso2_5mm | kinematic | right |
| `mechanical.transmission` | Transmission | ISO 3952 | project sheet | iso2_5mm | transmission | right |
| `mechanical.bondGraph` | BondGraph | textbook | project sheet | none | bondGraph | right |
| `mechanical.controlBlock` | ControlBlock | textbook | project sheet | none | controlBlock | right |
| `annotation.detail` | Detail | ISO (ASME choosable; with an ASME sheet, ASME is preselected) | project sheet | none | gdt, welding, texture, `sheet.drawingSheet` | none |
| `reliability.faultTree` | FaultTree | IEC 61025 (rectangular choosable) | project sheet | none | fta | down |
| `reliability.rbd` | RBD | IEC 61078 | project sheet | none | rbd | right |
| `reliability.eventTree` | EventTree | IEC 62502 | project sheet | none | eta | right |
| `reliability.fmea` | FMEA | IEC 60812 | project sheet | none | fmea | down |
| `idef0.model` | Context | FIPS 183 | project sheet | none | fips183 | right |

All ten types are keyed in the gallery's Engineering group under Mechanical, Reliability or Systems.

**Examples** (Help > Examples; also test fixtures and route-quality inputs):

| Key | Content |
|---|---|
| `mechanical.fourBar` | M = 1 |
| `mechanical.sliderCrank` | slider-crank linkage |
| `mechanical.gearbox` | motor, coupling, two gear pairs on bearings, belt to a load |
| `mechanical.massSpringDamper` | bond graph |
| `mechanical.piLoop` | unity feedback PI loop |
| `annotation.bracket` | datums A, B and C; two frames; a fillet weld on both sides; Ra 1.6 |
| `reliability.pumpFaultTree` | fault tree with a transfer pair |
| `reliability.coolingRbd` | series blocks, a parallel pair and a 2-out-of-3 group |
| `reliability.coolingEventTree` | event tree |
| `reliability.pumpFmea` | three failure modes |
| `idef0.makeWidget` | an A-0 page and an A0 page with three boxes |

All example text is our own words.

**Not in this branch:** bow-tie, Ishikawa, signal-flow-graph viewpoint, Open-PSA export, and the FIPS 183 diagram form. Record them as follow-ups. There are no new file formats; FMEA and analysis tables export through `table.export`.

## Engine additions (each with its test)

The four new analyses below are added to `vizcraft_analysis::analyses()`, so `analyze.*`, `vizcraft-cli analyze` and the Analysis dock pick them up with no further work. Each `mech.*` and `rel.*` analysis command collects the elements drawn on the target page (default: the active page), passes them as `elements`, and calls `analyze::run_kind`. Analyses make no undo step. Every algorithm is iterative and capped; when a cap is hit the result sets `truncated` instead of failing.

**Analysis commands**

| Command (new) | Analysis kind | Params, then result |
|---|---|---|
| `mech.mobility` | `mobility` | `{page?, space?: planar or spatial}`, then `{n, j1, j2, m, formula}` plus a table. Planar uses the Kutzbach count 3(n-1)-2J1-J2; spatial uses 6(n-1)-Σ(6-f). A joint joining k bodies counts k-1 times. Test: four-bar gives 1, five-bar 2, triangle 0, slider-crank 1. |
| `mech.ratio` | `ratio` | `{from, to, speed?}`, then `{ratio: "1:6", value, sign, path, conflicts, speed?}` as an exact fraction. Signs: external mesh negative, internal positive, worm starts over teeth, crossed belt negative; a disengaged clutch cuts the path. Test: 20/40 then 15/45 gives 1:6 with the same direction; a loop with two different ratios is reported as a conflict. |
| `mech.mason` | `mason` | `{input?, output?}`, then forward paths, loops, non-touching groups, Δ, and the formula text (numeric when every gain is a number). Loops come from Johnson's algorithm, capped at 256 loops, 64 paths and order-4 groups. Test: G/(1+GH). |
| `rel.cutsets` | `cutsets` | `{top?, maxOrder? (default 8), limit? (default 10000)}`, then a table of sets (events, order, probability), the rare-event and min-cut-upper-bound top probabilities, and Fussell-Vesely importance. Computed bottom-up in topological order with minimisation. Voting k/n expands to combinations. XOR and PAND are approximated as OR and AND, and the result says so. A house event with state true is removed from its sets; with state false it prunes the branch. Transfers are followed by tag. Tests: A OR (B AND C) gives {A} and {B, C}; 2-out-of-3 gives three pairs; a 2000-iteration junk-graph loop never panics. |
| `rel.availability` | `availability` | `{start?, end?, time?}`, then the system A (and R(t) when failure rates are given) plus a per-block table. Series and parallel reduction first; k-of-n by dynamic programming; standby with perfect switching; a bridge is solved exactly by pivoting on up to 20 blocks, and past that it is an error. Tests: 0.81, 0.99, bridge 0.97848 at p = 0.9. |
| `rel.outcomes` | `outcomes` | `{initiator?}`, then the frequency of each outcome. Test: the probabilities across the branches add up. |

**Editing commands** (each change is one undo step)

| Command (new) | Params, then result | Test |
|---|---|---|
| `mech.causality` | `{page?}`, then `{assigned, conflicts}`. Applies the textbook sequential causality assignment (sources, then integral C and I, then R). | Mass-spring-damper strokes match the textbook; undo restores them. |
| `rel.rpn` | `{ids?}` | Recomputes `rpn` as S×O×D; the `rpnCurrent` issue clears afterwards. |
| `idef0.decompose` | `{id?}` | Makes child page A&lt;n&gt; with a Decomposition view, a page link from the box, and `Boundary` stubs coded I1, C1, O1, M1 in side order. Test: A0 box 2 gives page A2 with four codes. |
| `idef0.renumber` | `{page?}` | Numbers boxes in staircase order. |
| `idef0.icom` | `{page?}` | Recomputes the boundary codes. |
| `annotation.setStandard` | `{page?, gdt?, weld?, texture?}` | Sets or clears the `standard` property on every annotation element drawn on the page. Test: the frames redraw as ASME and `retiredCharacteristic` fires. |

`idef0.nodes {}` is read-only and returns the node index.

**Rule engine.** New check modules, each following the `er_rules::check` pattern:
- `crates/notation/src/mech_rules.rs`;
- `annotation_rules.rs`;
- `rel_rules.rs`;
- `idef0_rules.rs`.

Each module defines its sub-check enum, with `#[serde(other)] Unknown` so that a newer pack still loads.

**New builder** `crates/stencils/src/builders/idef0box.rs`. It must give the same output on every run, put its ports on the module grid, and pass the shipped-pack port-id test `shipped_builder_glyphs_name_known_builders_and_their_types_declare_the_builders_port_ids`. Add three golden SVGs.

**New line ends** `harpoon`, `harpoonBar` and `parens` in `vizcraft_diagram::arrows`, through `outline` and `EVERY`, so the SVG, PDF and canvas exports draw them from one geometry. Test: a geometry unit test and a native round trip.

## Own (files)

New:
- `crates/notation/packs/{mechanical,annotation,reliability,idef0}.json`
- `crates/notation/packs/tests/{mechanical,annotation,reliability,idef0}/*.json`, with at least one failing and one clean case per rule
- `crates/notation/src/{mech_rules,annotation_rules,rel_rules,idef0_rules}.rs`
- `crates/analysis/src/{mech.rs, reliability.rs}`, with in-file tests
- `crates/engine/src/cmd/{mech.rs, rel.rs, idef0.rs, annotation.rs}`, plus `mech_tests.rs`, `rel_tests.rs` and `idef0_tests.rs` (wired with `#[path]` as `net_tests.rs` is)
- `crates/stencils/src/builders/idef0box.rs`
- `crates/stencils/testdata/builders/idef0box_*.svg`
- `crates/ui-egui/src/ribbon/notation/mechanical/{mod.rs, mech.rs, reliability.rs, idef0.rs, annotate.rs}`
- `crates/ui-egui/tests/{mechanical.rs, mechanical_snapshots.rs}` and their PNGs

UI groups:
- Mechanical pages get an Analyze group: Mobility, Ratio (takes the two selected shapes), Mason, Assign Causality.
- Reliability pages get Analyze (Cut Sets, Availability, Outcomes) and FMEA (Add Failure Mode, which runs `element.create` owned by the selected worksheet; Recompute RPN).
- IDEF0 pages get Decompose, Renumber, ICOM Codes.
- The Sheet tab gets an Annotate group: Feature Control Frame, Datum, Weld, Texture, and a Standard menu that runs `annotation.setStandard`.

Results show in the existing Analysis dock through `ResultTable`, so there is no dock hunk. The Inspector Symbol group already edits `fcf`, `weld`, `texture` and `idef0box` parameters, so there is no Inspector hunk.

## Shared, with the exact hunk this branch adds

| File | Hunk |
|---|---|
| `crates/notation/src/schema.rs` | Four `Check` variants before `Unknown`: `Mechanism { test: MechTest }`, `Annotation { test }`, `Reliability { test }`, `Idef0 { test }` (enums live in the new modules) |
| `crates/notation/src/rules.rs` | One dispatch arm sending the four variants to their modules |
| `crates/notation/src/lib.rs` | Four `pub mod` lines |
| `crates/notation/src/catalog.rs` | Four lines in the shipped pack list |
| `crates/analysis/src/lib.rs` | `pub mod mech; pub mod reliability;` and six entries appended to `analyses()` |
| `crates/engine/src/cmd/mod.rs` | Four `pub mod` lines and four `v.extend(..::specs())` lines |
| `crates/engine/src/tests.rs` | Junk rows: `{"top": 5, "maxOrder": 1e300, "limit": -1, "time": "NaN", "space": "warp", "from": [], "to": {}, "speed": 1e308}` and `{"gdt": 7, "weld": "", "texture": [], "id": "x".repeat(1000), "page": "\u{0}"}` |
| `crates/stencils/src/builders/mod.rs` | `idef0box::SPEC` appended; the `SPECS` array length goes up by one (combine with other branches' builders at the merge) |
| `crates/diagram/src/arrows.rs` | Three variants, names, `EVERY` length, `outline` arms; plus the one-line arms the compiler demands in `formats/src/drawio/{read,write}.rs` and `ui-egui/src/ribbon/connector/arrows.rs` |
| `crates/ui-egui/src/ribbon/notation/mod.rs` | `mod mechanical;` and, in `tab(app)`, append `mechanical::groups(pack)` |
| `crates/ui-egui/src/ribbon/sheet/mod.rs` | `tab()` appends `notation::mechanical::annotate::group()` |
| `crates/ui-egui/src/icons.rs` | Append-only icons: mobility, ratio, cutSets, availability, decompose, causality |

## Tests

- **Pack cases:** `vizcraft-cli validate` over `packs/tests/<pack>/`, every rule covered.
- **Loader:** all four packs pass `load_shipped`; xtask provenance passes.
- **Analyses:** the unit numbers above, a 100,000-element chain with no stack overflow, and output that is the same on every run.
- **Engine:**
  - each command's happy path;
  - each edit is one undo step;
  - analyses leave the document clean;
  - `variant.set` between `annotation.iso` and `annotation.asme`, and `reliability.rectangular`, redraws in place with centres kept;
  - `hostile_params_never_panic` is green.
- **Document types:** each makes an empty document with the right viewpoint, variant, sheet and rulesets.
- **Examples:** each passes its own rulesets with no issues and passes the route-quality checker; preview goldens are blessed after looking at the PNGs.
- **Kittest:**
  - the Analyze group appears only on mechanical pages;
  - Mobility fills the Analysis dock;
  - Decompose adds a page;
  - the Sheet tab shows Annotate.
- **Snapshots:** each new ribbon group, and the four-bar, fault tree, IDEF0 A0 and GD&T example pages.
- `cargo xtask ci` is green.

## Owner checklist (at most 10 steps)

Build and launch from Terminal:

```sh
cd ~/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b
CARGO_TARGET_DIR=target-eng-standards CARGO_INCREMENTAL=0 cargo build --release -p vizcraft
./target-eng-standards/release/vizcraft
```

1. Look at the New page that opens. You should see Kinematic Diagram, Fault Tree, IDEF0 and the other new cards under Engineering.
2. Open Kinematic Diagram. You should see an empty A3 sheet with a title block, and joints, links and ground first in the palette.
3. Go to Help > Examples > Four-bar linkage, then click the notation tab > Mobility. The Analysis dock should read M = 1.
4. Delete one link, then click Mobility again. It should read M = 0, and Problems should say the mechanism cannot move.
5. In the Gearbox example, select the motor and the load, then click Ratio. You should see 1:6.
6. In the Pump fault tree example, click Cut Sets. You should see single events and pairs, each with a probability.
7. In the Cooling RBD example, click Availability. You should see one system number and one row per block.
8. In the Bracket example, go to the Sheet tab > Standard > ASME. The frames and the weld should redraw in place, and Ctrl+Z should undo the switch in one step.
9. In the Make widget example, select box 2 and click Decompose. A page A2 should open with arrows labelled I1, C1, O1 and M1.
10. In the Pump FMEA example, change a severity in the Tables dock. Problems should flag the stale RPN, and its fix should recompute it.

## Report (final message)

- The commits.
- The command ids: the 13 new `mech.*`, `rel.*`, `idef0.*` and `annotation.*` commands, plus the four `analyze.*` kinds they run.
- Each shared hunk exactly as merged.
- New and changed snapshots and goldens, each checked by eye.
- Rows the agent may not edit:
  - ROADMAP: the wave 4 mechanical branch landed;
  - parity: mobility, ratio, Mason, causality, cut sets, availability, outcomes, FMEA, GD&T, welding, texture, IDEF0 decomposition;
  - mcp: each new command;
  - formats: none.
- The design calls made, and the Owner checklist above.

## Risks and open questions

- **Kinematic modelling.** Links are relationships between joints, and ternary links are `Plate` bodies. This draws like a skeleton diagram, but n is derived. If you change it, say so.
- **Bond graph labels.** Bond-graph letters are stroke glyphs with the parameter below. The textbook inline form "C: k" would need a text-only glyph outline, which is not in this branch.
- **Unverified gate shapes.** Whether IEC 61025 Annex A also lists the rectangular gate forms is unverified; the shapes are drawn from NUREG-0492 either way.
- **Partial process-number table.** The ISO 4063 table is partial, so unknown numbers are only a warning.
- **FMEA setup or content.** An empty worksheet with headers counts as setup in the FMEA document type. If `b6-doctypes` forbids it, the palette offers the worksheet first instead.
- **Contested shared files.** `SPECS`, `Arrow::EVERY`, the `Check` enum and `analyses()` are append points that the other wave 4 branches also touch. Expect keep-both conflicts at merge.
- **Preview palette section.** The "Engineering symbols (preview)" palette section should go only after all four packs land. That belongs to `b6e-ux` or the integration session, not this branch.

### Critical Files for Implementation
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/schema.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/rules.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/analysis/src/lib.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/engine/src/cmd/analyze.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/ui-egui/src/ribbon/notation/mod.rs