# `b6e-fluidpower` (fluid power pack), Block 6E wave 4

Opus, medium effort. Port 8076. Worktree `.claude/worktrees/b6e-fluidpower`, branch `b6e-fluidpower` from the integration branch `claude/engineering-diagramming-standards-305d6b` (the session names the commit). `CARGO_TARGET_DIR=target-b6e-fluidpower CARGO_INCREMENTAL=0`. Commit prefix `B6E:`.

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

- JIC (owner): a project with JIC symbols draws fluid power in the ANSI Y32.10 variant; add
  `fluidpower.jic` as an alias of `fluidpower.ansi` so `variant_for` resolves it, with a test.
- `wire.draw {type}` and `ui.tool {line}` exactly as in the common section; `FluidPorts` (open
  ports) becomes a `NetCheck` test if electrical has merged first.

## Draft brief

# `b6e-fluidpower`: ISO 1219 hydraulic and pneumatic circuits, port sets, editions, line kinds and the displacement-step diagram (Block 6E wave 4)

Opus, medium effort. Port 8076. Worktree `.claude/worktrees/b6e-fluidpower`, branch `b6e-fluidpower` from the integration branch `claude/engineering-diagramming-standards-305d6b` after `b6-doctypes` merges. `CARGO_TARGET_DIR=target-b6e-fluidpower CARGO_INCREMENTAL=0`. Commit prefix `B6E:`. Build the engine first, then the UI. The owner does the visual checks, so make no in-app walkthroughs; end with the Owner checklist.

Read first: AGENTS.md, `/Users/pranav/Desktop/VizCraft-Blocks-6-6E-source-of-truth.md` (optionality first; command first; document types, not templates), `docs/research/ee-me-diagramming-standards.md` (`fluidpower` pack row, branch row, "Fluid power check"), `docs/research/ee-me/process-fluid.md` section 4 and "Fluid power pack", `docs/briefs/b6-wave1/b6e-builders.md`, `docs/briefs/b6e-frontend/report.md`, the `b6-doctypes` report and `crates/notation/src/template.rs` as merged (the document type and example format).

**Goal.** An engineer can choose "Hydraulic Circuit" or "Pneumatic Circuit" on the New page and get an empty ISO A3 sheet with an ISO 7200 title block on the 2.5 mm module. The palette offers ISO 1219 symbols: valves, pumps and cylinders built from parameters, plus pressure valves, accessories and sensors. Ports are labelled with ISO 9461 letters or ISO 5599 numbers. The engineer draws working, pilot and drain lines with P, gets component codes (`0P1`, `1V1`, `1A1`), and can switch between the 2012 and 1995 editions (or the ANSI Y32.10 legacy drawing) in one undo step without anything moving. Problems lists:
- open ports
- actuators that no pump drives
- a fixed pump without a relief valve
- valves not drawn at rest
- pilot or drain lines on the wrong ports
- unreachable steps in a displacement-step chart drawn beside the circuit

The pressure path can be traced into the Analysis dock. Every one of these is a command reachable from the CLI, the control channel and MCP.

## Scope

**Pack `crates/notation/packs/fluidpower.json` (new).** Every glyph carries `source` ("own design, drawn from the written rules of ISO 1219-1" or similar). The repo only allows written descriptions and public-domain US government documents as sources. Never use Commons, QElectroTech or vendor art, except to check geometry.

- **Viewpoints:**
  - `fluidpower.Hydraulic`: hydraulic types, neutral types, Enclosure, StepChart, ComponentList.
  - `fluidpower.Pneumatic`: the same with the pneumatic types. The step chart sits beside the circuit, as the research example shows.
  - `fluidpower.Sequence`: StepChart and ComponentList only.
- **Abstract supertypes** (through `extends`). `fluidpower.Component` has the properties `circuit` (integer, default 1; 0 is the power unit), `installation`, `nominalPressure` (bar), `partNumber`, `description` and `plugged` (port ids, space separated). Its class types carry the ISO 1219-2 letter:

  | Type | Letter |
  |---|---|
  | `Source` | P |
  | `Actuator` | A |
  | `PrimeMover` | M |
  | `DirectionalValve`, `PressureValve`, `FlowValve` | V |
  | `Sensor` | S |
  | `Conditioning`, `Tank` | Z |

- **Medium as subtypes, not as a property.** `apply::glyph_of` takes params from the type's glyph, so each medium needs its own type and glyph:
  - Hydraulic: `HydraulicPump`, `HydraulicMotor`, `HydraulicCylinder`, `HydraulicValve` (iso9461, 4/3 spring-centred solenoid), `ReliefValve`, `ReducingValve`, `SequenceValve`, `Reservoir`, `Accumulator`.
  - Pneumatic: `Compressor`, `AirMotor`, `PneumaticCylinder`, `PneumaticValve` (5/2, iso5599, pilot both sides, no return), `PressureRegulator`, `PressureSource`, `ServiceUnit`, `Silencer`, `Exhaust`.
  - Neutral path glyphs with declared ports: `Filter`, `Cooler`, `CheckValve` (A to B one way), `Throttle`, `FlowControl` (one-way), `ShutOffValve`, `PressureGauge`, `PressureSwitch`, `LimitSwitch`, `ElectricMotor` (port `shaft`).
  - Two more:
    - `Enclosure`: a container with a dash-dot look, for assemblies and the power unit.
    - `ComponentList`: a list compartment like `sheet.PartsList`. Its query types are `fluidpower.Component`; subtypes match through `infer::chain_of`. Columns: tag, name, description, nominalPressure, partNumber.
  - Every type with a builder glyph declares the builder's default port ids in order. The test `shipped_builder_glyphs_name_known_builders_and_their_types_declare_the_builders_port_ids` enforces this.
- **Relationships and connector types:**
  - Connector types: `fluidpower.working` (solid, `net: "fluid"`), `fluidpower.pilot` and `fluidpower.drain` (dashed, fluid), `fluidpower.mechanical` (double or thick line, no net), `fluidpower.signal` (thin, `net: "signal"`).
  - Relationship types: `WorkingLine`, `PilotLine`, `DrainLine`, `MechanicalLink`, `SignalLine`.
  - Connection table: working lines join any Component; pilot lines join Component to DirectionalValve or PressureValve; drain lines join Component to Tank; mechanical links join Actuator or PrimeMover to Sensor, DirectionalValve or Source; signal lines join Sensor and DirectionalValve.
- **Variants (symbol standard), all choosable, default `fluidpower.iso`.** No variant moves a shape.

  | Variant | Label | `options` | Connectors | Module |
  |---|---|---|---|---|
  | `fluidpower.iso` | ISO 1219 (2012) | `codes: "numeric"`, `codeFormat` | pilot and drain both dashed | page's |
  | `fluidpower.iso1995` | ISO 1219 (1995 circuit rules) | `codes: "letters"`, `codeFormat: "{installation}{circuit}{letter}{n}"` | PilotLine to `fluidpower.pilotLong`, DrainLine to `fluidpower.drainShort` | page's |
  | `fluidpower.ansi` | ANSI Y32.10 (US, legacy) | letter codes | same glyphs; the description says Y32.10 follows the same function-not-construction rules | `inch0_1` |

  Family resolution follows `template::variant_for`: a project family `iso` gives `fluidpower.iso`, `ansi` gives `fluidpower.ansi`, and `iec` (the default) falls to the document type's variant. JIC is an open question.
- **Identifier scope** `fluidpower.component`: all Component types, `unique: true`, `autoNumber: false`. Codes are written by `fluid.renumber`.
- **Rules.** Citations name clauses only, for example "ISO 1219-2:2012, port marking". Severity is warning unless noted; all rules are live.

  | Rule id | Check | Note |
  |---|---|---|
  | `fluidpower.illegalLine` | `illegalRelationship` | |
  | `fluidpower.viewpoint` | `viewpointType` | |
  | `fluidpower.duplicateCode` | `duplicateIdentifier` | |
  | `fluidpower.openPort` | `fluidPorts` | |
  | `fluidpower.pressurePath` | `fluidPressurePath` | |
  | `fluidpower.reliefAfterPump` | `fluidRelief` | |
  | `fluidpower.supply` | `fluidSupply` | |
  | `fluidpower.restPosition` | `fluidRestPosition` | |
  | `fluidpower.portSet` | `fluidPortSet` | info |
  | `fluidpower.lineEnds` | `fluidLineEnds` | |
  | `fluidpower.codeFormat` | `fluidCodes` | info |
  | `fluidpower.stepReachable` | `fluidSteps` | |
  | `fluidpower.idleStep` | `fluidSteps {idle: true}` | info |

  Rulesets:
  - `fluidpower.circuit`: common rules.
  - `fluidpower.hydraulic`: adds relief and supply.
  - `fluidpower.pneumatic`: adds the port set rule.
  - `fluidpower.sequence`: step rules.

**Document types** (exact JSON in the `b6-doctypes` format; discipline "Fluid power"). Every field stays changeable at creation and afterwards.

| Type | Viewpoint | Default standard | Sheet and title block | Module | Rulesets | Insertion |
|---|---|---|---|---|---|---|
| Hydraulic Circuit | `fluidpower.Hydraulic` | `fluidpower.iso` | `iso.a3.landscape`, ISO 7200 | `iso2_5mm` | circuit, hydraulic, `sheet.drawingSheet` | BT (energy flows upward, ISO 1219-2 layout) |
| Pneumatic Circuit | `fluidpower.Pneumatic` | `fluidpower.iso` | `iso.a3.landscape`, ISO 7200 | `iso2_5mm` | circuit, pneumatic, sequence, `sheet.drawingSheet` | BT |
| Displacement-Step Diagram | `fluidpower.Sequence` | `fluidpower.iso` | `iso.a4.landscape`, ISO 7200 | `iso2_5mm` | sequence | LR |

There is no key. ASME sheets come from the standards bar as usual.

**Examples** (Help > Examples and test fixtures; each must pass `crates/engine/tests/route_quality.rs`, so draw lines with explicit bends):
1. Hydraulic circuit: reservoir, fixed pump with electric motor, relief valve with gauge, filter, 4/3 solenoid valve, double-acting cylinder. Codes `0Z1`, `0P1`, `0M1`, `0V1`, `1V1`, `1A1`.
2. Pneumatic circuit with sequence: pressure source, service unit, 5/2 double-pilot valve (ISO 5599: 1 to 5, 12, 14), cylinder, two 3/2 roller limit valves `1S1` and `1S2`, a start button valve, and a step chart beside the circuit.
3. Hydraulic power unit: `fluid.powerUnit` output in an Enclosure on ISO A4.

**Builders.**
- New `crates/stencils/src/builders/pressure_valve.rs` (`pressureValve`): `function` (relief, reducing, sequence), `pilot` (internal or external), `drain` (internal or external), `adjustable`, `medium`. One square with an offset arrow, a spring and a dashed pilot. Ports: relief P T; reducing P A, plus L when draining externally; sequence P A L; plus X for an external pilot.
- New `step_chart.rs` (`stepChart`), drawn from the written description of VDI 3260. Param `rows` is text, one line per actuator: `<code> <state per step>`, for example `1A1 0 1 1 0 0`. States are 0 (retracted) or 1 (extended), at most 16 rows and 32 steps. It draws two levels per row, step columns numbered at the top, and sloped segments where a state changes.
- `valve.rs` (now owned by this branch):
  - Pilot ports at the operator sides: X and Y (ISO 9461), or `1n` (ISO 5599) where n is the output joined to 1 in that side's square.
  - Electrical ports `el` and `er` on solenoids; mechanical ports `ml` and `mr` on rollers and plungers.
  - Port order: fluid ports first, then pilots, then the rest, so adding an operator never shifts a glued fluid port.
  - Move `port_set`, `default_paths`, `find` and the token parser into `vizcraft_notation::fluid::paths` and call them from there.
- `pump.rs`: a `shaft` port at the end of the shaft.
- `cylinder.rs`: a `rod` port at the rod tip.
- Re-bless the affected goldens in `crates/stencils/testdata/builders/`: `valve-5-2-pneumatic`, the three `pump-*` and the three `cylinder-*`. Add 3 or more goldens per new builder.

**Circuit model and checks (new `crates/notation/src/fluid/`).** Stencils depends on notation, so the circuit logic lives here.
- `paths.rs`: the path grammar, port sets and defaults (the constants `valve.rs` uses).
- `mod.rs`: `Circuit::build(doc, cat, page?)`:
  - Ports per pin come from `Shape.glyph.ports`, else the type's declared ports.
  - Nets come from `vizcraft_diagram::net::NetIndex::build`.
  - Internal edges per class. A directional valve takes the union over its positions; params come in layers: element property, then shape `glyph.params`, then the type glyph's params under the page variant, then the `paths.rs` defaults. A check valve is one-way. Flow valves and conditioning pass through. Actuators do not join A to B.
  - `trace(from)` gives the reached ports, nets and connectors.
- `rules.rs`: one `check(doc, cat, pack, rule, out)` for the nine `Fluid*` variants, building the circuit once per call.
  - Fixes are `fluid.plug` and `fluid.renumber`.
  - Exhaust ports of pneumatic valves may stay open.
  - The relief check walks from a fixed pump's P port through nets and pass-through parts only, never through a directional valve.
  - The rest position check: for a two-position valve with a return spring, rest must be the square on the spring side; with three positions, the middle.
- `steps.rs`: parse `rows` and resolve codes to actuators by tag.
  - For each change between steps, find the directional valve feeding the actuator's A or B port through nets and pass-throughs.
  - Some selectable position (rest, an end square with an operator, or the spring-centred middle) must join the pressure port to the driving port, and for double-acting the other side to an exhaust.
  - A row naming no actuator is reported.

**Engine commands (new `crates/engine/src/cmd/fluid/{mod.rs, steps.rs, power_unit.rs, tests.rs}`).** One undo step each unless marked read-only.

| Command | Params, result |
|---|---|
| `fluid.insert` | `{type, params?, x?, y?, page?}` makes the element and its view, with params as shape overrides. Returns `{element, shape}` |
| `fluid.renumber` | `{page?, codes?: "letters"\|"numeric"}`; the default comes from the variant `options.codes`. Numbers per circuit and letter, left to right then bottom to top, and rewrites step chart rows. Returns `{changed: [{element, from, to}]}` |
| `fluid.setPortSet` | `{page?, ids?, set: "iso9461"\|"iso5599"}` sets `portLabels` on directional valves |
| `fluid.plug` | `{id, ports: [id], plugged?: bool}` |
| `fluid.circuit` | `{page?}`, read-only: components, ports, nets, internal paths, sources and sinks |
| `fluid.trace` | `{from: element or {shape, point}, positions?: "rest"\|"any"}`; stores an `AnalysisResult` in `Session.analysis`, so halos and the Analysis dock work with no new UI. No undo step |
| `fluid.stepChart` | `{page?, actuators?, steps?, x?, y?}` creates a StepChart whose rows are the actuators sorted by code |
| `fluid.steps` | `{chart}`, read-only: rows, states, findings |
| `fluid.setStep` | `{chart, row, step, state}` |
| `fluid.insertStep` | `{chart, at?}` |
| `fluid.deleteStep` | `{chart, at}` |
| `fluid.powerUnit` | `{page?, x?, y?, pump?: "fixed"\|"variable", cooler?, accumulator?, gauge?, circuit?: 0}`, a transaction: an Enclosure with reservoir, pump, motor, relief valve, filter and options, joined by typed `wire.draw` lines pin to pin |

`fluid.powerUnit` is a command rather than a `crates/patterns` pattern because `Emission` carries no element properties and no port ids. Port-aware patterns are a follow-up.

**Formats.** No new file format. The component list exports through the existing Tables CSV and XLSX. SVG, PDF and draw.io draw everything as shapes and connectors.

**UI.**
- New contextual tab "Circuit" in `crates/ui-egui/src/ribbon/fluidpower/`. It shows while the active viewpoint is in `fluidpower`; the generic notation tab keeps Insert, Viewpoint, Variant (the edition) and Check. Groups:
  - Symbols: Valve... (`ui.valve`), Pressure Valve menu, Pump, Cylinder, Power Unit.
  - Lines: Working, Pilot, Drain, Mechanical, Signal. Each is `ui.tool {tool: "pipe", line: <type>}` and is checked while armed.
  - Codes: Renumber, a letters or numeric menu, a Port set menu.
  - Sequence: Step Chart, Insert Step, Delete Step.
  - Analyze: Trace Pressure, Clear (`analyze.clear`).
  - Reports: Component List.
- Pipe tool default: when no line is armed, P uses the first connector type of the active pack whose `net` is `fluid` or `pipe`. This is a generic rule, so electrical and process can share it.
- The Valve dialog on a fluidpower page commits `fluid.insert` with the page's valve type.
- New Inspector group `panes/inspector/step_chart.rs`: a grid of rows by steps of toggle cells, each click one `fluid.setStep`; plus and minus step buttons; the findings from `fluid.steps` under the grid.

## Own (files)

New:
- `crates/notation/packs/fluidpower.json`
- `crates/notation/packs/tests/fluidpower/*.json`
- `crates/notation/src/fluid/{mod.rs, paths.rs, rules.rs, steps.rs, tests.rs}`
- `crates/stencils/src/builders/{pressure_valve.rs, step_chart.rs}` and their goldens
- `crates/engine/src/cmd/fluid/`
- `crates/ui-egui/src/ribbon/fluidpower/{mod.rs, symbols.rs, lines.rs, codes.rs, sequence.rs, analyze.rs}`
- `crates/ui-egui/src/panes/inspector/step_chart.rs`
- `crates/ui-egui/tests/{fluidpower.rs, fluidpower_snapshots.rs}`

Existing files taken over: `crates/stencils/src/builders/{valve.rs, pump.rs, cylinder.rs}` and their goldens.

## Shared, with the exact hunk this branch adds

| File | Hunk |
|---|---|
| `notation/src/schema.rs` | Before `Unknown`: `FluidPorts`, `FluidPressurePath`, `FluidRelief`, `FluidSupply`, `FluidRestPosition`, `FluidPortSet`, `FluidLineEnds`, `FluidCodes`, `FluidSteps { idle: bool }`, each documented |
| `notation/src/rules.rs` | One arm in `Ctx::check` sending all nine to `crate::fluid::rules::check(self.doc, self.cat, pack, rule, out)` |
| `notation/src/validate.rs` | One arm `=> {}` for the nine |
| `notation/src/lib.rs` | `pub mod fluid;` and a doc bullet |
| `notation/src/catalog.rs` | `BUILTIN` row `("fluidpower.json", include_str!("../packs/fluidpower.json"))` |
| `model/src/net.rs` | `NetKind::dotted()` includes `Fluid` (ISO 1219 joins with a dot). Existing junctions keep their fill |
| `stencils/src/builders/mod.rs` | `mod pressure_valve; mod step_chart;` and two `SPECS` entries (array length 10 to 12) |
| `engine/src/cmd/mod.rs` | `pub mod fluid;` and `v.extend(fluid::specs());` |
| `engine/src/cmd/wire.rs` | `wire.draw` gains `"type"?: connector type id`. It sets `Connector.kind` through `connector_kind::apply_kind`, and the type's `net` gives the net kind when `kind` is absent. Params text updated |
| `engine/src/session.rs` | `is_transaction` gains `"fluid.powerUnit"` |
| `engine/src/tests.rs` | Two junk rows: `{"type": "fluidpower.HydraulicValve", "params": {"paths": "P-Q;;;X\|", "positions": 1e308}, "chart": "s2", "row": 1e20, "step": -1, "state": "x", "set": 5, "codes": [], "ports": [null, 7]}` and one with 70 000-character `rows` and `from: {"shape": "s2", "point": 1e20}` |
| `ui-egui/src/ribbon/mod.rs` | `mod fluidpower;` and `fluidpower::tab(app),` in `tabs` |
| `ui-egui/src/lib.rs` | `CraftApp.line_type: Option<String>`; the `"ui.tool"` arm reads `line` |
| `ui-egui/src/canvas/interact/wire.rs` | `finish` adds `type` (the armed line or the pack default) |
| `ui-egui/src/panes/inspector.rs` | `mod step_chart;` and one call |
| `ui-egui/src/valve_dialog.rs` | OK branch on a fluidpower page |
| `ui-egui/src/icons.rs` | Icons: `workingLine`, `pilotLine`, `drainLine`, `stepChart`, `tracePressure`, `powerUnit` |

xtask: no hunk; provenance already reads every pack.

## Tests

- **Notation:**
  - `fluid/tests.rs`: path grammar round trips.
  - The circuit graph over a hand-built document.
  - Each `Fluid*` check, positive and negative.
  - `packs/tests/fluidpower/` cases: `open_port`, `fixed_pump_without_relief`, `pilot_on_working_port`, `valve_not_at_rest`, `unreachable_step`, and a clean circuit.
  - The loader accepts the pack with no warnings.
- **Stencils:** new goldens; port ids on whole modules; the junk-param loop for both new builders; the declared-ports test passes.
- **Engine:**
  - Every `fluid.*` command, with undo.
  - `variant.set` from iso to iso1995 changes only connector dashes and keeps every shape's x, y.
  - `fluid.renumber` letters and numeric.
  - `fluid.trace` writes `Session.analysis` and makes no undo step.
  - `fluid.powerUnit` gives one undo step with every port glued and no `fluidpower.openPort` issue.
  - The examples pass `route_quality`; `hostile_params_never_panic` is green.
- **UI kittest:**
  - The Circuit tab shows only on a fluidpower page.
  - Pilot arms the Pipe tool, and a drag commits `wire.draw` with `type: "fluidpower.pilot"`.
  - The Inspector grid click runs `fluid.setStep`.
  - Snapshots: the Circuit tab, the step chart group, both examples on the canvas. List every retaken snapshot.
- `cargo xtask ci` is green.

## Owner checklist

Run in Terminal:

```sh
cd ~/Workspace/crafts/VizCraft/.claude/worktrees/b6e-fluidpower && CARGO_TARGET_DIR=target-b6e-fluidpower CARGO_INCREMENTAL=0 cargo build --release -p vizcraft && ./target-b6e-fluidpower/release/vizcraft --window-pos -2500,100
```

1. The New page opens. Choose Fluid power > Hydraulic Circuit. You should see an empty A3 sheet with a title block, and the palette shows Sources, Valves, Actuators.
2. Drag in a Hydraulic Pump, a Relief Valve, a Hydraulic Valve and a Hydraulic Cylinder. Problems lists open ports, and "pump without a relief valve" until you connect the relief valve.
3. Press P and drag from the pump's top port to the valve's P port. A solid line appears; a branch onto it shows a dot.
4. Circuit tab > Pilot, then draw a line. It is dashed.
5. Notation tab > Variant > ISO 1219 (1995). Pilot lines change dash and nothing moves. Ctrl+Z restores both in one step.
6. Circuit > Renumber. The labels read like `0P1`, `1V1`, `1A1`.
7. Select the pump, then Circuit > Trace Pressure. The path is highlighted and the Analysis dock lists it.
8. Help > Examples > Pneumatic circuit with sequence. The circuit has a step chart beside it.
9. Click the chart, then click a cell in the Inspector grid. The chart redraws; an impossible move shows in Problems.
10. Quit, and check that `pgrep -fl vizcraft` prints nothing.

## Report

Your final message lists:
- the commits
- the command ids (all `fluid.*`, the `wire.draw type` param, the `ui.tool line` param)
- every shared hunk as landed
- the goldens re-blessed and the snapshots retaken
- the document types and examples as shipped
- the deviations (a power unit command rather than a pattern; medium as subtypes)
- rows for:
  - ROADMAP (fluidpower pack landed)
  - docs/parity.md (fluid power symbols, line kinds, codes, editions, checks, step chart, trace)
  - docs/mcp.md (`fluid.*`, `wire.draw type`, `ui.tool line`)
  - docs/formats.md (the pack check kinds; no new file format)

Do not edit those files.

## Risks and open questions

- **Unverified facts.** The research note did not verify the ISO 1219-1 module (2.5 mm is assumed), the ISO 1219-2:2012 code wording, or the 1995 versus 2012 dash rule.
  - Keep code formats and dashes as pack data, so they can be corrected without code changes.
  - The Codes menu offers both forms on every edition.
  - Ask the owner to have the standard text checked.
- **Parallel hunks.** `wire.draw type`, `ui.tool line`, the Pipe default, `SPECS` length, `icons.rs` and `ribbon/mod.rs` are likely touched by electrical and process too. Write them exactly as specified here so the merge keeps one copy.
- **Glue by index.** Port order changes when params change. The fluid-first order limits this; a glued line to a removed pilot port is still dropped.
- **JIC.** No public JIC H-1 or P-1 drawing rules exist. Should a JIC project fall back to ISO, or should `fluidpower.jic` be an alias of `ansi`? This is the owner's call.
- **No retype command.** Switching a component's medium means delete and re-insert.
- **The "Engineering symbols (preview)" palette section** stays; the integration session removes it after the four packs merge.
- **Performance.** Live fluid checks rebuild `NetIndex` per rule call. Measure on a 2 000-shape circuit; if slow, move `fluidSteps` to batch mode.

### Critical Files for Implementation
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/stencils/src/builders/valve.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/schema.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/rules.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/engine/src/cmd/wire.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/stencils/src/builders/apply.rs