# `b6e-process` (process pack), Block 6E wave 4

Opus, medium effort. Port 8075. Worktree `.claude/worktrees/b6e-process`, branch `b6e-process` from the integration branch `claude/engineering-diagramming-standards-305d6b` (the session names the commit). `CARGO_TARGET_DIR=target-b6e-process CARGO_INCREMENTAL=0`. Commit prefix `B6E:`.

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

- Default drawing is ISO 10628 (owner): `process.iso` is the default variant for every process
  document type and the project family `iec` (and `iso`) resolves to it; `process.isa` is one choice
  away. Re-word the draft's defaults and examples (examples may show ISA as a second variant golden).
- Use the shared `IdScope.numberBy`, `wire.draw {type}` and `NetCheck` tests instead of parallel
  definitions where the draft adds its own (`PortsConnected` becomes a `NetCheck` test).
- Loop diagram terminals and cables reuse the electrical pack's terminal and cable types if
  `b6e-electrical` has merged first; otherwise keep yours and note it for unification.

## Draft brief

# `b6e-process`: the process pack (Block 6E wave 4)

Opus, medium effort. Port 8075. Worktree `.claude/worktrees/b6e-process`, branch `b6e-process` from the integration branch once `b6-doctypes` has merged. `CARGO_TARGET_DIR=target-b6e-process CARGO_INCREMENTAL=0`. Commit prefix `B6E:`. Read first: AGENTS.md, `/Users/pranav/Desktop/VizCraft-Blocks-6-6E-source-of-truth.md`, `docs/research/ee-me-diagramming-standards.md` (the `process` rows), `docs/research/ee-me/process-fluid.md` (sections 1, 2, 3, 5, 6 and "Implications"), and the `b6-doctypes` report, which defines the document type format.

**Goal.** When this lands, a process engineer can start an empty P&ID, PFD, instrument loop diagram, HVAC flow diagram, plumbing riser or piping isometric from New. Each starts on its own sheet and title block, with ISA-5.1 symbols by default and ISO 10628 or ISO 14617 one Variant click away (everything redraws in place, one undo step).

On that page they can:
- place equipment and give it nozzles;
- draw pipes whose line number is built from size, service, sequence and class, and that keep their number through valves and get a new one at a branch;
- create a loop so a bubble reads `LIC-101`, and switch loop numbering between parallel and serial;
- pair off-page connectors across sheets;
- trace a loop from sensor to valve and generate its ISA-5.4 loop diagram;
- open the line list, equipment list, instrument index, valve list, I/O list and stream table as live tables, and export them to CSV or XLSX.

Every one of these is a command reachable by id.

## Scope

**Pack JSON.** New file `crates/notation/packs/process.json`.
- Pack id `process`, name "Process Plant". The name avoids a clash with the existing Visio-style Process tab, because the notation tab takes the pack's name.
- Module `iso2_5mm`.

Viewpoints:
- `process.PFD`
- `process.PID`
- `process.Loop`
- `process.HVAC`
- `process.Plumbing`
- `process.Isometric`

Element types use DEXPI 1.4 class names. Check every name against the public DEXPI 1.4 reference pages, which are CC BY 4.0. Where you cannot confirm a name, use a plain name and leave its DEXPI mapping empty.

| Family | Types (supertype first) |
|---|---|
| Plant | `Unit` (container; tag `{n}`) |
| Equipment | `Equipment`, `Vessel`, `Tank`, `ProcessColumn`, `HeatExchanger` (`kind`: shellTube, plate, airCooled, ...), `Pump` (`kind`), `Compressor`, `Blower`, `Fan`, `Filter`, `Separator`, `Agitator`, `Furnace`, `Dryer`, `CoolingTower`, `Centrifuge`. Properties: `classCode`, `service`, design and operating P and T, `material`, `size`, `duty`, `driver`, `vendor`, `datasheet`. Tag `{classCode}-{owner:process.Unit.tag}{n:2}{suffix}` gives `P-101A` |
| Nozzles | `Nozzle` (allowedParents Equipment; one process port; `subTag`, `nominalDiameter`, `nominalPressure`; tag `{owner.tag}-{subTag}`) |
| Piping | `PipingNetworkSystem`, which is the line (`nominalDiameter`, `fluidCode`, `pipingClass`, `insulation`, `tracing`, design, operating and test P and T, `label` once/each/off). `PipingComponent` (two process ports) with `GateValve`, `GlobeValve`, `BallValve`, `ButterflyValve`, `PlugValve`, `NeedleValve`, `CheckValve`, `SafetyValveOrFitting` (`setPressure`, inlet and outlet sizes), `PipeReducer`, `Strainer`, `Flange`, `LineBlind`, `Cap`. Valves carry `actuator`, `fail` (FO, FC, FL, FL/DO, FL/DC), `normallyClosed`, `ends` and `letters`. `PipeOffPageConnector` |
| Instrumentation | `InstrumentationLoopFunction` (the loop: container; `variable`, `loopType`, `service`, `suffix`; tag `{n:3}`). `ProcessInstrumentationFunction` (the bubble: `letters`, `suffix`, `system`, `location`, `ioType`, `signal`, `range`, `setpoint`, `alarmHigh`, `alarmLow`, `datasheet`, `manufacturer`, `model`). `ProcessSignalGeneratingFunction` (inline primary element). `SignalOffPageConnector`. Instruments and actuated valves have allowedParents set to the loop, with tag `{letters}-{owner:process.InstrumentationLoopFunction.tag}{suffix}` |
| Loop diagram | `JunctionBox`, `TerminalStrip`, `Terminal` (per-owner `{n}`), `Cable` (`pairs`; tag `{owner:...Loop.tag}-C{n}`), `MarshallingCabinet`, `IOChannel` (`rack`, `slot`, `channel`), `Barrier`, `PowerSupply` |
| HVAC | `AirHandlingUnit`, `Coil`, `Humidifier`, `Damper` (`kind`: volume, fire, smoke, backdraft, motorised), `Diffuser`, `TerminalUnit` (VAV or CAV), `Silencer`, `Chiller`, `Boiler`, `DuctRun` (extends the line; `size` as W x H or a diameter, `airflow`) |
| Plumbing | `Fixture` (`kind`, `fixtureUnits`, `system`), `WaterHeater`, `BackflowPreventer`, `WaterMeter` |
| Isometric | `Elbow`, `Tee`, `Weld` (shop or field), `PipeSupport`, `NorthArrow`, `BillOfMaterials` (a table compartment, `TableQuery` over item types) |

**Relationships.**
- `process.PipeConnection`: the connector type of every pipe kind names it. The connection table allows it only between piping nodes: nozzles, piping components, inline primary elements and off-page connectors.
- `process.Signal`: allowed only between instrument functions, actuated valves and signal off-page connectors.
- `process.Measures`: from an instrument to equipment or a line, for the instrument index.
- `process.Continues`: kind `crossReference`, so the sheet pass labels both ends of an off-page pair with sheet and zone.

**Connector kinds.** Each has a `net`, a `marker` and widths that are multiples of the module (ISO 10628-1 widths in `process.iso`).
- Pipes, net kind `pipe`, with `jumps: true` so crossings that do not connect are interrupted: `processLine`, `secondaryLine`, `utilityLine`.
- Signals (no net):
  - `electricSignal` (dashed);
  - `pneumaticSignal` (`doubleSlash`);
  - `hydraulicSignal` (`l`);
  - `capillary` (`cross`);
  - `softwareLink` (`circle`);
  - `wireless` (`wave`);
  - `mechanicalLink`.
- HVAC, net kind `fluid`: `supplyAir`, `returnAir`, `exhaustAir`, `chilledWaterSupply`, `chilledWaterReturn`.
- Plumbing: `coldWater`, `hotWater`, `hotWaterReturn`, `sanitary`, `vent`.

**Variants.** The symbol standard is a choice. Each variant lists only the glyphs whose written descriptions actually differ from the default.
- The default drawing is ISA-5.1 for instruments, with equipment drawn in North American practice from written descriptions, PIP PIC001 style.
- `process.isa`: empty maps, so the family `isa` resolves; module `inch0_1`.
- `process.iso`: ISO 10628-2 equipment and IEC 62424 PCE request ovals; module `iso2_5mm`.
- `process.iso14617`: ISO 14617 basic symbols with supplementary symbols composed onto them; give registration numbers in `keywords` only where the research note confirms them (2111, 2112, 201, 203).

HVAC, plumbing and isometric have one drawing each, marked `own design`, because ASHRAE 134 and SMACNA were not verified. Every path or builder glyph carries a `source` of either `own design` or `drawn from ...`.

**Identifier scopes:**

| Scope | Format | Behaviour |
|---|---|---|
| `process.unit` | `{n}` | |
| `process.equipment` | `{classCode}-{owner:process.Unit.tag}{n:2}{suffix}` | auto-numbered |
| `process.nozzle` | `{owner.tag}-{subTag}` | |
| `process.line` | `{nominalDiameter}-{fluidCode}-{n:3}-{pipingClass}` | auto-numbered; the field order is configurable |
| `process.loop` | `{n:3}` | auto-numbered; `numberBy: variable`, so numbering is parallel by default |
| `process.instrument` | as given for instruments above | covers bubbles and actuated valves |
| `process.offPage` | | paired |
| `process.signalOffPage` | | paired |
| cable, terminal, JB | as above | terminal is per-owner |

**Rules** (citations name the clause only):

| Rule id | Check | Reference |
|---|---|---|
| `process.tagLetters` | new `InstrumentTag {types, allowed}`: letter positions per the ISA table, and the first letter must equal the loop's `variable` | ANSI/ISA-5.1, identification letters |
| `process.duplicateTag`, `.duplicateEquipment`, `.duplicateLine`, `.duplicateLoop` | `DuplicateIdentifier` | ISA-5.1, loop identification; PIP PIC001, equipment and line numbering |
| `process.unpairedOffPage`, `.unpairedSignalOffPage` | `UnmatchedPair` | ISO 10628-1, continuation arrows; PIP PIC001, off-page connectors |
| `process.offPageContinuity` | new `OffPageContinuity`: a pair on one sheet, or partners whose segments are on different lines | PIP PIC001, off-page connectors |
| `process.portsConnected` | new `PortsConnected {types}`: a nozzle, valve or fitting port with no segment and no cap | ISO 10628-1, P&ID minimum content |
| `process.loopComplete` (batch) | new `LoopComplete`: a control loop missing a measuring element, or with no final element reachable along `process.Signal` | ISA-5.1, loop |
| `process.loopInLoop` | `NestingAllowed`: an instrument or actuated valve outside any loop | ISA-5.1, tag numbers |
| `process.oneLoopPerSheet` | new `LoopSheet`: on Loop pages, one loop per sheet, every loop tag present, no terminal with a single wire, each I/O channel used once | ISA-5.4, loop diagram content |
| `process.lineData` | `RequiredProperty` on the line (size, fluid, class) | PIP PIC001, line numbers |
| `process.reliefData` | `RequiredProperty` on relief valves (set pressure and sizes) | PIP PIC001, pressure relief |
| `process.designAboveOperating` | `PropertyCompare` (`ge`) on lines and equipment | house rule |
| `process.signalLegal`, `.pipeLegal` | `IllegalRelationship` | ISA-5.1, instrument lines |
| `process.streamsNumbered` | new `StreamsNumbered {viewpoints}` on PFD pages | ISO 10628-1, PFD minimum content |
| `process.ductData`, `process.fixtureData` | `RequiredProperty` | house rule |
| `process.viewpoint` | `ViewpointType` | |

**Rulesets:**
- `process.pid`: ISA plus PIP.
- `process.iso10628`: the drawing rules without `tagLetters`, because the IEC 62424 letter grammar was not verified.
- `process.pfd`, `process.loop`, `process.hvac`, `process.plumbing`, `process.isometric`.

**Document types.** Exact JSON follows the `b6-doctypes` format. Each has no content. Every standard stays choosable at creation and afterwards. Title block is ISO 7200, or ASME when an ASME sheet is chosen.

| Type | Viewpoint | Standard | Sheet | Module | Rulesets | Insert direction | Key |
|---|---|---|---|---|---|---|---|
| P&ID | PID | `process.isa` | iso.a1.landscape | from variant | process.pid, sheet.drawingSheet | LR | yes (line kinds) |
| Process Flow Diagram | PFD | `process.isa` | iso.a1.landscape | iso2_5mm | process.pfd, sheet.drawingSheet | LR | no |
| Instrument Loop Diagram | Loop | `process.isa` | iso.a3.landscape | from variant | process.loop | LR | no |
| HVAC Flow Diagram | HVAC | default | iso.a2.landscape | iso2_5mm | process.hvac | LR | yes |
| Plumbing Riser | Plumbing | default | iso.a2.landscape | iso2_5mm | process.plumbing | BT | yes |
| Piping Isometric | Isometric | default | iso.a3.landscape | none | process.isometric | none | no |

**Examples** (Help > Examples and test fixtures, one per type):
- **P&ID:** unit 1; vessel V-101 with three nozzles; an inlet line with a control valve; level loop 101 (LT, LIC, LV) with electric signals; relief valve PSV-101; pump P-101A; two paired off-page connectors on a second sheet.
- **PFD:** feed, exchanger, column, pump, numbered streams and a stream table.
- **Loop:** made from the P&ID example by `loop.diagram`.
- **HVAC:** an air handler, two VAV boxes, diffusers and a chilled water coil.
- **Plumbing:** a meter, a backflow preventer and three floors of fixtures on CW and HW.
- **Isometric:** one line on iso legs with elbows, a valve, shop and field welds, a north arrow and a BOM.

## Engine additions (each with its test)

**1. Builders.**
- New `crates/stencils/src/builders/process_valve.rs` (`processValve`). Params:
  - `body`: gate, globe, ball, butterfly, plug, needle, check, diaphragm, angle, threeWay;
  - `actuator`: none, manual, diaphragm, piston, motor, solenoid, selfActing;
  - `fail`;
  - `normallyClosed` (filled body);
  - `ends`;
  - `standard`: isa, iso10628, iso14617.
- Two flow ports sit on the module. A `//! Provenance:` line is required.
- `bubble.rs` gains `standard` (`isa5_1`, or `iec62424`, which draws an oval) and a `tag` param. The `tag` param is split into letters above the line and the number below, so the composed tag shows with no extra pass.
- Tests: every param combination builds within the caps; ports land on the grid; ISA and ISO variants give the same frame and the same ports.

**2. Process pass.** New `crates/notation/src/process_pass.rs`, run last in `bind::sync`. Idempotent: a second run changes nothing, and selection is pruned.
- **(a) Line assignment.** A pipe segment on a PID, HVAC, Plumbing or Isometric page with no `ext["process.line"]` is resolved like this:
  - it inherits the line across an inline component's other port, so the number stays the same through valves;
  - otherwise it gets a new `PipingNetworkSystem` (or a `DuctRun` on HVAC pages) tagged by the registry, so a branch gets a new sequence (PIP practice);
  - a split keeps the line, because it clones the connector `ext`.
- **(b) Line removal.** A line element whose last segment went in this command is removed, as bind step 4 removes relationships.
- **(c) Live recomposition.** Tags of process scopes with property or owner parts, and no page, zone or rung parts, are recomposed live and keep their number (`IdFormat::number_in`). This is a deliberate difference from the sheet's on-demand policy: changing a line's size or a loop's number updates its tags at once.
- **(d) Line labels.** The line tag is written on the longest segment of each line on each page (or on each segment, or none, per the line's `label`). It is written as the relationship name for bound segments and as connector text otherwise.
- Tests:
  - `splice_keeps_line`, `branch_gets_new_line`, `size_edit_recomposes_line`, `loop_renumber_retags_instruments`, `pass_is_idempotent`;
  - an old document stays byte-identical.

**3. Project tag formats.**
- `Model.idFormats: BTreeMap<scope, IdOverride {format, numberBy}>`, with serde defaults, skipped when empty, at most 64 entries, format length capped at `MAX_FORMAT`.
- `IdScope.numberBy`: peers and uniqueness are grouped by that property's value. This is parallel numbering.
- `Registry::new` applies the overrides; the new `Registry::effective()` returns the merged scopes.
- Tests:
  - parallel allows F-101 and L-101; serial flags them as duplicates;
  - an override changes `next_tag`;
  - junk overrides are sanitised.

**4. Commands.** All are one undo step unless they are read-only.

| Id | Notes |
|---|---|
| `loop.create {variable, members?, loopType?, service?, suffix?}` | |
| `loop.assign {loop or null, elements}` | |
| `loop.trace {loop or element}` | Returns `{loop, chain: [{element, tag, role: primary, transmitter, controller, final}], signals, missing, complete, cables?, terminals?, channel?}`; read-only, no undo step |
| `loop.diagram {loop, sheet?}` | A new Loop page with lanes Field, Junction box, Marshalling, Control room and Panel front (`lane.addSet`), the loop's instrument views, JB, terminals, cable pairs as wire nets and the I/O channel |
| `loop.list` | |
| `line.assign {connectors or net, line?}` | With no `line` it makes a new line |
| `offpage.pair {a, b}` | Equal tags, a `process.Continues` relationship, and the two pipe nets merged with `force` (named, so the net pass never splits them) |
| `offpage.place {connector, end, toPage, x?, y?}` | |
| `plant.addNozzle {shape, side, at?, subTag?, nominalDiameter?}` | |
| `plant.numbering {scheme?: parallel or serial}` | |
| `plant.list {list: line, equipment, instrument, valve, io, stream, nozzle, fixtureUnits, bom; page?; format?: csv, xlsx, markdown; path?}` | Returns `{columns, ids, rows, editable}`; columns as in process-fluid.md section 6; exports through `vizcraft_formats::tables` |
| `plant.isoRun {from, legs: [{dir: n, s, e, w, up, down; length}], kind?}` | Straight legs on 30-degree axes |
| `plant.dexpiMap {type?}` | |
| `identity.formats` | |
| `identity.setFormat {scope, format?, numberBy?}` | |

The prefix is `plant.*` because `process.*` is already taken by `cmd/process.rs` (`process.createSubprocess`).

**5. Formats.** New `crates/formats/src/dexpi_map.rs`: process type id to DEXPI class name and RDL URI, only where verified. Test: every process type maps or is listed as unmapped. CSV and XLSX lists come from the existing writers, with goldens for the line list.

**6. UI** (in `crates/ui-egui`):
- **Pack groups on the notation tab:**
  - Lines: the Pipe tool with a kind menu, Assign Line, Place and Pair Off-page;
  - Tags: a Numbering menu with Parallel and Serial checked, Formats..., Renumber;
  - Loops: New Loop, Trace (selects the chain, shows status), Loop Diagram;
  - Equipment: Add Nozzle by side;
  - Lists: opens the Tables dock in pack-list mode, plus Export.
- **Inspector Process section:** a Line group on a pipe segment, a Loop group on an instrument or valve, a Nozzles list on equipment.
- **Tag Formats dialog:** `ui.dialog {dialog: "tagFormats"}`; OK runs a single `batch` call so the change is one undo step.
- **Pipe tool:** passes `connectorKind`, defaulting by viewpoint.
- **Icons:** drawn in code.
- **Kittest:** groups show on a P&ID page and not on a UML page; Numbering toggles; Line group edit is one undo step; the Tables dock shows line list rows; the dialog commits once.

## Own (files)
- `crates/notation/packs/process.json`
- `crates/notation/packs/tests/process/` (new; one case per rule plus `clean_pid.json`)
- `crates/notation/src/{process_pass.rs, process_rules.rs}` (new)
- `crates/stencils/src/builders/process_valve.rs` (new) and the `bubble.rs` param hunk
- `crates/engine/src/cmd/{loops.rs, plant.rs, identity.rs}` (new; `loops`, because `loop` is a keyword) and `crates/engine/src/cmd/plant_tests.rs` (new)
- `crates/formats/src/dexpi_map.rs` (new)
- `crates/ui-egui/src/ribbon/notation/process/` (new), `panes/inspector/process.rs` (new), `tag_formats_dialog.rs` (new), and `tests/process.rs` plus `tests/process_snapshots.rs` (new)

## Shared, with the exact hunk this branch adds
- `notation/src/schema.rs`: `Check::{InstrumentTag, LoopComplete, PortsConnected, LoopSheet, StreamsNumbered, OffPageContinuity}` placed before `Unknown`.
- `notation/src/rules.rs`: one arm routing those six to `crate::process_rules::check(self.doc, self.cat, pack, rule, out)`.
- `notation/src/bind.rs`: `crate::process_pass::run(doc, before, cat);` after `nesting::owners`, plus doc step 11.
- `notation/src/lib.rs`: two `mod` lines.
- `notation/src/catalog.rs`: the `("process.json", include_str!(..))` row and the built-in pack count. Loader checks for the new check params go in `validate.rs` if it lists them per check.
- `model/src/identity.rs`: `numberBy`, override merge, `effective()`.
- `model/src/types.rs`: `Model.idFormats` with sanitize, `is_empty`, `Default`.
- `notation/src/sheet_pass.rs` `stale_tags` and `engine/src/cmd/sheet.rs` `renumber`: read `reg.effective()`.
- `stencils/src/builders/mod.rs`: `mod process_valve;` and one `specs()` row.
- `engine/src/cmd/mod.rs`: `pub mod identity; pub mod loops; pub mod plant;` and three `specs()` lines.
- `engine/src/cmd/wire.rs`: `wire.draw` gains `connectorKind?`, applied through `connector_kind::apply_kind`; the type's `net` wins over `kind`.
- `engine/src/cmd/table.rs`: a `view_of` arm, `"pack"` with `list: "<pack>.<name>"`, dispatching to `plant::list_view`.
- `engine/src/tests.rs`: junk rows (legs, sides, schemes, scopes, list names).
- `formats/src/lib.rs`: `pub mod dexpi_map;`.
- Connector kind count tests in `cmd/connector_kind.rs` and stencils: rewrite them as per-pack prefix filters.
- `ui-egui`:
  - `ribbon/notation/mod.rs`: `mod process;` and pack-specific groups appended in `tab(app)` when `active_pack` is `process`;
  - `panes/inspector.rs`: `mod process;` and one `section(..)` line;
  - `dock/tables.rs`: the pack-list mode;
  - `canvas/interact/wire.rs`: `connectorKind`;
  - `lib.rs`: `UI_COMMANDS` rows;
  - `dialogs.rs`: the `tagFormats` entry;
  - `icons.rs`: new icons.
- No xtask hunk is expected: provenance already reads every pack and builder.

## Tests
- `vizcraft-cli validate` over `tests/process/`: each rule fires once on its case, and `clean_pid.json` gives no issues.
- Engine: each command is one undo step and is refused on bad params.
- Switching the variant leaves every shape frame identical, as one undo step.
- Every example opens clean and passes the route-quality checker.
- `hostile_params_never_panic` is green.
- SVG goldens: the bubble in ISA and IEC 62424; the process valve matrix; the P&ID example in all three variants; a line list CSV golden; an XLSX round trip.
- UI snapshots: the notation tab, the Inspector Line group, the Tables dock line list, the Tag Formats dialog. Retake them and list them.
- `cargo xtask ci` is green.

## Owner checklist
Launch with `cd ~/Workspace/crafts/VizCraft/.claude/worktrees/b6e-process && ./target-b6e-process/release/vizcraft`.
1. The New page shows a Process group with six types. Pick P&ID: you should see an empty A1 sheet with a title block and a palette of Equipment, Valves, Instruments and Lines.
2. Drag in a Vessel, then click Add Nozzle > Right: a nozzle labelled like `V-101-N1` appears on its side.
3. Press P and drag from the nozzle to empty space: a pipe appears, and a line number appears once you set Size, Fluid and Class in the Inspector's Line group.
4. Drop a Globe Valve onto the pipe: it cuts in, and both halves keep the same line number.
5. Place an Instrument, select it, set Letters to LIC, then click New Loop: the bubble reads LIC above and 101 below.
6. Variant > ISO 10628: the bubble turns into an oval and the valves redraw, nothing moves, and one Undo brings it back.
7. Tags > Numbering > Serial: nothing changes until two loops share a number, and then Problems lists it.
8. Lists > Line List: the Tables dock shows one row per line. Export > XLSX saves a file that opens in a spreadsheet app.
9. Help > Examples > P&ID, then delete one off-page arrow: Problems says it has no partner.
10. Select the level controller and click Trace: LT, LIC and LV are selected and the status bar says the loop is complete.

## Report (final message)
The report includes:
- commits;
- every new command id and its params;
- each shared hunk as it landed;
- snapshots retaken and goldens blessed, after looking at the PNG exports;
- deviations, including any DEXPI names or ISO numbers left unmapped.

The agent may not edit these docs, so the report carries their rows:
- **ROADMAP:** the Block 6E wave 4 process pack landed.
- **docs/parity.md:** P&ID, PFD, loop diagrams, HVAC, plumbing, isometric, the process lists, loop trace, parallel and serial numbering, project tag formats, off-page pairs.
- **docs/mcp.md:** all `loop.*`, `line.assign`, `offpage.*`, `plant.*`, `identity.*`, `wire.draw connectorKind`, `table.* view pack`, `ui.dialog tagFormats`.
- **docs/formats.md:** the DEXPI class mapping (CC BY 4.0, mapping only, no Proteus export yet) and the process lists through the CSV and XLSX writers.

## Risks and open questions
- **Overlap with `b6e-electrical`.** That branch may also add `wire.draw connectorKind`, project tag formats, a pack view in `table.rs` and an unconnected-pin check. The orchestrator should name one owner; the branch that merges second reuses the first's hunk.
- **Live tag recomposition** differs from the sheet branch's on-demand policy. Confirm with the owner.
- **The process pass writes model elements during sync** (it creates line elements). It needs the idempotence test and selection pruning.
- **Loop diagram terminals and cables** duplicate the electrical pack's terminal strips and cables. Unify them in a later wave.
- **Isometric.** The canvas has no 30-degree snap, so runs come from `plant.isoRun`; canvas iso snapping is left to `b6e-ux`.
- **Unverified standards.** The DEXPI class names and RDL URIs, ISO 14617 numbers, IEC 62424 letter rules and ASHRAE 134 contents are partly unverified. Leave any gap empty rather than guess.
- **Preview palette.** The "Engineering symbols (preview)" palette section stays; removing it after all four packs land falls to `b6e-ux`.
- **Open question:** the pack name "Process Plant", and whether a project symbol family of `iec` should pick `process.iso` instead of the ISA default.

### Critical Files for Implementation
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/schema.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/model/src/identity.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/bind.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/stencils/src/builders/bubble.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/engine/src/cmd/wire.rs