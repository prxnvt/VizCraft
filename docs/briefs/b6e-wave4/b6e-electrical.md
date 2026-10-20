# `b6e-electrical` (electrical pack), Block 6E wave 4

Opus, medium effort. Port 8074. Worktree `.claude/worktrees/b6e-electrical`, branch `b6e-electrical` from the integration branch `claude/engineering-diagramming-standards-305d6b` (the session names the commit). `CARGO_TARGET_DIR=target-b6e-electrical CARGO_INCREMENTAL=0`. Commit prefix `B6E:`.

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

- Ladder Diagram takes the project default (IEC, ISO A3), per the owner; drop the JIC on ASME B suggestion.
- `numberPer` becomes the shared `IdScope.numberBy` (above). This branch owns `Check::NetCheck`,
  `Port.netGroup` and `wire.draw {type}` as defined in the common section.

## Draft brief

# `b6e-electrical`: the electrical pack, ladder tool, terminal strips, cables and the electrical rule check (Block 6E wave 4)

Opus, medium effort. Port 8074. Worktree `.claude/worktrees/b6e-electrical`, branch `b6e-electrical`, created from the integration branch `claude/engineering-diagramming-standards-305d6b` after `b6-doctypes` merges. `CARGO_TARGET_DIR=target-b6e-electrical CARGO_INCREMENTAL=0`. Commit prefix `B6E:`. Only one cargo build at a time from this session.

Read first:
- AGENTS.md
- `/Users/pranav/Desktop/VizCraft-Blocks-6-6E-source-of-truth.md`
- `docs/research/ee-me-diagramming-standards.md` (the `electrical` row and the decisions)
- `docs/research/ee-me/electrical.md` (sections 1 to 8 and "Implications")
- `docs/briefs/b6-wave1/{b6e-substrate,b6e-sheets,b6e-builders}.md`
- `docs/briefs/b6e-frontend/report.md`
- `docs/briefs/b6-doctypes/brief.md` and its report: the document type and example JSON format. Every type and example below is written in that format. Its fields are viewpoint, variant or symbol standard, sheet, title block, module, rulesets, insertion direction and key.

Rules for this branch:
- Do not edit ROADMAP.md, docs/parity.md, docs/mcp.md, docs/formats.md, docs/control-protocol.md or docs/cascade-plan.md. Put their rows in the report.
- Do not merge, rebase, push, or touch another worktree.
- Do not launch the GUI to look. The owner does the visual checks.

**Goal.** An electrical engineer can do all of the following, from the New page or by command id over the CLI, the control channel or MCP:
- Start an empty control circuit, ladder, one-line, three-line, PLC I/O, panel, interconnection or logic document. It opens on an ISO A3 sheet with IEC 60617 symbols by default; ANSI (IEEE 315) and JIC symbols and ASME sheets are always one choice away.
- Place devices drawn in the chosen standard and wire them. Wires carry numbers derived from rungs on a ladder.
- Link contacts to coils so both show where the other is. The coil carries a contact mirror.
- Build terminal strips and cables with cores.
- Switch the whole project between IEC, ANSI and JIC symbols, and between the IEC 81346, JIC and Y14.44 tag formats. Every symbol redraws in place and every designation recomposes, in one undo step.
- See electrical rule check findings in Problems.
- Read and export wire, cable, terminal and I/O lists as live tables (CSV and XLSX).

## Scope

**Pack JSON** (new `crates/notation/packs/electrical.json`):
- Audience `engineering` and `electrical`.
- `module: iso2_5mm`.
- Document properties `tagFormat` (enum `iec81346` (default), `jic`, `y1444`) and `wireNumbering` (enum `page`, `rung`, `sequential`).
- Every glyph has a `source` that is either "own design" or starts with "drawn from …". Sources are written descriptions in the IEC, IEEE, JIC and NEMA texts as summarised in the research note, or the public domain MIL-STD-15-1 and MIL-STD-806B. Cite clause names only. Never trace a figure, and never cite an IEC 60617 id that you have not verified.

| Part | Content |
|---|---|
| Viewpoints | `electrical.Schematic`, `Ladder`, `OneLine`, `ThreeLine`, `Io`, `Panel`, `Interconnection`, `Logic`; each lists its types and relationships, so the `b6-doctypes` palette scoping shows only them |
| Abstract class types (IEC 81346-2 backbone) | `Sensing` B, `Protecting` F, `Relay` K, `Motor` M, `Indicating` P, `PowerSwitch` Q, `Passive` R, `OperatorControl` S, `Transformer` T, `Conductor` W, `Terminal` X, `Generating` G. Each declares `iecCode`, `jicCode` and `y1444Code` defaults. Use single-letter main classes unless the research note or the standard text confirms a two-letter subclass. The codes are editable per element |
| Device types (`extends` a class) | `Coil` (builder `coil`), `Contact` (builder `contact`), `Pushbutton`, `SelectorSwitch`, `LimitSwitch`, `ProcessSwitch` (contact builder with `actuator`), `Fuse`, `Breaker`, `Disconnect`, `Overload` (new `switchgear` builder), `PilotLight`, `Meter`, `Horn`, `Motor`, `Generator`, `Utility` (new `machine` builder or path glyphs), `Transformer`, `CurrentTransformer`, `VoltageTransformer`, `Relay` (C37.2 `function` property), `Resistor`, `Capacitor`, `Inductor` (new `passive` builder), `Diode`, `Transistor`, `OpAmp`, `Battery`, `Earth` (`kind`: protective, functional, chassis), `IoModule` and `Ic` (new `block` builder), `IoPoint`, `TerminalStrip` (container), `Terminal`, `Plug`, `Cable`, `Core`, `Continuation`, `Gate` (builder `gate`), `FlipFlop`, `LogicInput`, `LogicOutput`, `Ladder`, `Function` (`=`) and `Location` (`+`) containers, `Enclosure`, `DinRail`, `Duct` |
| Typed properties | `complementNo`, `complementNc` (coil); `form` (contact); `rung` (derived); `colour`, `crossSection` on nets; `kva`, `primaryV`, `secondaryV`, `vectorGroup`; `frame`, `trip`; `ratio`, `polarity`; `function` (C37.2); `channel`, `address`, `signal`; `number`, `level`, `bridge`, `netIn`, `netOut` (terminal, the last two derived); `cores`, `type`, `length` (cable); `width`, `height` (footprint) |
| Ports | On the module grid, with ids matching the builders: `13`/`14`, `21`/`22`, `A1`/`A2`, `1`/`2`, and so on. New `Port.netGroup`: ports in one group pass one net through, as the two sides of a terminal do |
| Connector kinds | `electrical.wire` (net `wire`); `electrical.bus` (net `potential`, heavy line); `electrical.signal` (dashed); `electrical.senses` (dashed, relationship `Senses`); `electrical.cableEnd` (relationship `CableEnd`); `electrical.mechanical` (dashed linkage) |
| Relationships | `ContactOf` (contact to coil, `kind: crossReference`, new `xrefEnds: "source"`); `Senses` (CT or PT to relay); `CableEnd` (cable to device, plug or strip; `outMax` 2) |
| Identifiers | `electrical.device`: format `{prefix}{code}{n}`, unique, autoNumber, new `numberPer: "code"`, so K1, K2, F1 each count separately. `electrical.contact` is not unique; it holds the coil's tag. `electrical.terminal`: `{owner.tag}:{n}`, perOwner. `electrical.continuation`: paired, tag is the net name |
| Variants | `electrical.iec` (default; rectangular gates; `iso2_5mm`). `electrical.ansi` (IEEE 315; distinctive gates; `inch0_1`). `electrical.jic` (JIC and NEMA ICS 19 forms, falling back to ANSI glyphs; `inch0_1`). `electrical.footprint` (panel: every device drawn by the new `footprint` builder). The ids follow `template::variant_for`, so `Document.standards.symbols` (`iec`, `ansi`, `jic`) selects them |

Rules. Every rule cites a clause name in `reference`. Existing checks are reused where they fit; new checks are marked *new*.

| Rule id | Check | Reference |
|---|---|---|
| `electrical.uniqueDesignation` (error) | DuplicateIdentifier `electrical.device` | IEC 81346-1, reference designations |
| `electrical.contactWithoutCoil` (error) | MinOutgoing `ContactOf` 1 | IEC 61082-1, cross-references |
| `electrical.coilComplement` (error) | *new* `ContactComplement` | IEC 61082-1, contact representation |
| `electrical.unconnectedPin` (warning) | *new* `NetCheck {test: unconnectedPin}` | IEC 61082-1, connections |
| `electrical.singlePinNet`, `.wireNumbered`, `.duplicateWireNumber` | `NetCheck` tests `singlePin`, `unnumbered`, `duplicateNumber` | IEC 60204-1 and NFPA 79, conductor identification |
| `electrical.reservedColour` (error) | `NetCheck {test: reservedColour}`: green-yellow only on PE | IEC 60445, identification by colours |
| `electrical.terminalFanIn` (warning) | `NetCheck {test: pinFanIn, types: [Terminal], max: 1}` | IEC 60204-1, connections and routing |
| `electrical.terminalNumbers` (error) | UniqueProperty `number` within `TerminalStrip` | IEC 60445, terminal identification |
| `electrical.continuationPaired`, `.xrefResolved` | UnmatchedPair `electrical.continuation`; CrossReferences | IEC 61082-1, interrupted lines |
| `electrical.oneOutputPerRung`, `.coilAtRight` (info) | *new* `RungOutputs {max}` | NEMA ICS 19, ladder diagrams |
| `electrical.mixedSymbols` (warning) | *new* `MixedSymbols`: builder shapes whose standard or gate shape set differs from the page variant | IEEE 91, shape sets |
| `electrical.oneDriver`, `.inputDriven` | `NetCheck` tests `drivers` (max 1) and `undriven` | IEEE 91 and IEC 60617-12, logic |
| `electrical.busVoltage` | `NetCheck {test: property, connectorType: electrical.bus, property: voltage}` | IEEE 141, one-line diagram |
| `electrical.breakerRatings`, `.transformerRatings` | RequiredProperty | IEEE 141 |
| `electrical.deviceNumber` (error) | *new* `DeviceNumber`: C37.2 number table and suffix grammar (`50/51`, `51G`, `87T`, `50BF`) | IEEE C37.2, device function numbers |
| `electrical.relaySensed`, `.ctSecondary` | MinIncoming `Senses` 1; `NetCheck {test: allowedPeers}` | IEEE 242, protection |
| `electrical.channelOnce`, `.addressUnique` | UniqueProperty | IEC 61131-3, I/O addressing |
| `electrical.cableEnds`, `.cableCores` | MinOutgoing `CableEnd` 2; *new* `CableCores` | IEC 61082-1, cable documents |
| `electrical.panelPlaced` | *new* `PlacedOn {viewpoint, min: 1, max: 1}` | own practice |

Rulesets:
- `electrical.schematic`, `.ladder`, `.oneLine`, `.io`, `.panel`, `.interconnection` and `.logic` each select the rules that matter for that diagram.
- `electrical.erc` holds all of them.
- Sheet rule ids (`sheet.mandatoryFields`, and others) are listed by id; cross-pack ids already resolve in `rules::selected`.

**Document types** (b6-doctypes format; no content is drawn):
- Every type leaves the variant and the sheet to the project's standards, so IEC and ISO A3 landscape with the ISO 7200 title block come first and every standard stays choosable.
- The one exception is Ladder. It suggests JIC on ASME B (see open questions).

| Type | Viewpoint | Rulesets | Insertion | Key |
|---|---|---|---|---|
| Control Circuit | Schematic | schematic | down | no |
| Ladder Diagram | Ladder | ladder | right | no |
| Motor Control | Schematic | schematic | down | no |
| One-Line Diagram | OneLine | oneLine | down | yes |
| Three-Line and Protection | ThreeLine | oneLine | down | no |
| PLC I/O Wiring | Io | io | right | no |
| Panel Layout and Terminal Plan | Panel (variant `footprint`) | panel | right | no |
| Cable and Interconnection | Interconnection | interconnection | right | no |
| Logic Diagram | Logic | logic | right | no |
| Electronic Schematic | Schematic (tags `y1444`) | schematic | right | no |

**Examples** (filled samples for Help > Examples and the tests):
- Control circuit (IEC): L1 and N, fuse, start and stop, contactor with holding contact.
- Ladder (JIC): rails, ten rungs, start, stop and seal-in, a pilot light.
- Motor control on two pages: power and control.
- One-line: utility, main breaker, bus, transformer, two feeders.
- Three-line: CTs with polarity, an 87 and a 51 relay.
- PLC I/O: an 8-channel DI module through a terminal strip.
- Panel: footprints on a DIN rail plus a strip.
- Logic: AND, OR, flip-flop.
- Electronics: divider, op-amp, decoupling.
- Interconnection: two enclosures and a 3-core cable.

## Engine additions (each with its test)

- **Builders** (new files in `crates/stencils/src/builders/`, 3 golden SVGs each under `testdata/builders/`):
  - `switchgear` (fuse, breaker, disconnect, overload; 1 to 4 poles; iec60617, ieee315 or jic)
  - `passive` (resistor as an IEC rectangle or an ANSI zigzag; capacitor, polarised or not; inductor; variable)
  - `machine` (motor or generator, AC or DC, 1 or 3 phases)
  - `block` (box with 1 to 32 pins a side, pin labels; used for PLC modules and ICs)
  - `footprint` (size from `width` and `height`, with the tag)
  - Changes to existing builders: `contact.rs` gains `actuator` (none, push, pull, turn, emergency, roller, pressure, level, flow, temperature, proximity, key) and form `co` (changeover, ports 11, 12, 14); `coil.rs` gains `kind: overload`.
  - Tests: pins on the module for every default and standard; a junk-params loop that must never panic.
- **Electrical sync pass** (new `crates/notation/src/electrical_pass.rs`):
  - Writes `code` and `prefix` from `tagFormat`. IEC gives `=F1+A2-`; Y14.44 gives the unit and assembly chain (`1A2`); JIC gives nothing.
  - Recomposes device tags live, keeping each number (as in EPLAN).
  - Copies a coil's tag onto contacts linked to it, and creates or removes `ContactOf` relationships from tag equality. When the coil is retagged its contacts follow; when a contact is retagged its link follows.
  - Writes a contact's `rung` from its ladder frame and a terminal's `netIn` and `netOut`.
  - Generates the coil's mirror as child text (`ext vizcraft.mirror.for`). IEC lists rows such as `13-14 /2.C3`. ANSI and JIC give a rung list to the right of the coil, with the NC list in a second label that is underlined, because underline is a whole-text style.
  - It is idempotent: test that a second sync changes nothing.
- **Rules module** (new `crates/notation/src/electrical_rules.rs`): `NetCheck`, `ContactComplement`, `RungOutputs`, `MixedSymbols`, `DeviceNumber`, `CableCores` and `PlacedOn`, built on `vizcraft_diagram::net::NetIndex::build`. Write one case per rule under `crates/notation/packs/tests/electrical/`, each with a firing variant and a clean one.
- **Commands** (new `crates/engine/src/cmd/electrical.rs`, `cmd/ladder.rs`; tests in `cmd/electrical/tests.rs`). Each is a `CommandSpec` with a params doc; every edit is one undo step.
  - `electrical.setStandards {symbols?: iec|ansi|jic, tags?: iec81346|jic|y1444}`: sets `Document.standards.symbols` and `tagFormat`, runs the `variant.set` logic on every electrical page except footprint pages, and recomposes tags. Test that centres and glue are unchanged, K1 becomes CR1 and back, and one undo restores everything.
  - `electrical.link {contacts, coil}` and `electrical.mirror {coil}` (read only).
  - `ladder.insert {page?, x, y, rungs, spacing?, first?, increment?, rails?}`: rails are named potential nets L1 and L2 drawn with `wire.draw`, plus a `Ladder` frame element whose builder draws the rung numbers.
  - `ladder.set`.
  - `ladder.place {shapes, rung?}`: moves pins onto the rung line.
  - `ladder.numberWires {ladder?|page?, format?: "{page}{rung:2}"}`: suffix letters skip I and O. Test that sheet 2, rung 3 gives `203` with the default format, and `2003` with format `{page}{rung:3}` (the block's example).
  - `terminal.strip {page?, x, y, count, first?, orientation?}`, `terminal.add`, `terminal.renumber`, `terminal.bridge {ids, bridge|null}`.
  - `cable.create {from, to, cores, type?, page?, x?, y?}`, `cable.assign {cable, nets|connectors, cores?}`: sets each core's net and each net's `cable` and `core` properties.
  - `io.assign {module, channel, tag?, signal?}`.
  - `electrical.placeFootprints {page, ids?}`.
  - `net.export {format: "json"|"text", page?, path?}`: our own netlist format, written in new `crates/formats/src/netlist.rs` and documented there.
- **Tables:**
  - `table.list` and `table.export` gain `subject: "nets"|"connections"` (kind filter). The connections table has one row per wire segment, with each junction end resolved to a pin of the same net in `net.members` order; the rows are built in new `cmd/table_nets.rs`.
  - `table.set` accepts a net id and writes through `net.set`.
  - Wire list = connections of kind wire. Cable list = `table.list {type: electrical.Cable}` and `Core`. Terminal plan = `Terminal`. I/O list = `IoPoint`.
  - Test: CSV and XLSX of the I/O example.

## Own (files)

- New: `crates/notation/packs/electrical.json`, `packs/tests/electrical/*.json`, `crates/notation/src/{electrical_pass.rs, electrical_rules.rs}`, `crates/stencils/src/builders/{switchgear,passive,machine,block,footprint}.rs`, their goldens, `crates/engine/src/cmd/{electrical.rs, ladder.rs, table_nets.rs}`, `crates/engine/src/cmd/electrical/{tests.rs, goldens/}`, `crates/formats/src/netlist.rs`.
- UI, new: `crates/ui-egui/src/ribbon/notation/electrical/` (`mod.rs`, `standard.rs`, `ladder.rs`, `devices.rs`, `lists.rs`), `crates/ui-egui/src/canvas/interact/ladder.rs`, `crates/ui-egui/src/panes/inspector/electrical.rs`, `crates/ui-egui/tests/electrical.rs` and `electrical_snapshots.rs`.
- Existing: `builders/contact.rs`, `builders/coil.rs` (the param additions above).

## Shared, with the exact hunk this branch adds

- `notation/src/schema.rs`:
  - `Port.netGroup: String`
  - `RelationshipType.xrefEnds` (`both` default, `source`, `target`)
  - the Check variants `NetCheck {test, types, kinds, connectorType, property, peers, max}`, `ContactComplement`, `RungOutputs`, `MixedSymbols`, `DeviceNumber`, `CableCores` and `PlacedOn`, inserted just before `Unknown`.
- `notation/src/loader.rs` and `validate.rs`: checks for those fields and their params.
- `notation/src/rules.rs`: one arm, `Check::NetCheck {..} | … => crate::electrical_rules::check(self.doc, self.cat, pack, rule, out)`.
- `notation/src/bind.rs`: one line, `crate::electrical_pass::run(doc, before, cat)`, right before `sheet_pass::run`.
- `notation/src/bind_nets.rs`: pins on one shape whose ports share a `netGroup` join one component.
- `notation/src/sheet_pass.rs`: `xref_labels` skips the end that `xrefEnds` excludes.
- `notation/src/catalog.rs`: the `electrical.json` include row, and the `scopes().len()` count in its test.
- `notation/src/lib.rs`: two `mod` lines.
- `model/src/identity.rs`: `IdScope.numberPer` (numbering counts only peers with the same value of that property), with a test.
- `stencils/src/builders/mod.rs`: five `mod` lines and the `SPECS` array grown by five.
- `engine/src/cmd/mod.rs`: `pub mod electrical; pub mod ladder; pub mod table_nets;` and two `specs()` lines.
- `engine/src/cmd/table.rs`: the `subject` dispatch and the net id in `table.set`.
- `engine/src/tests.rs`: junk rows for ladder, terminal, cable, io and `setStandards` params.
- `formats/src/lib.rs`: `pub mod netlist;`.
- UI:
  - `ribbon/notation/mod.rs`: a `pack_groups(pack_id)` match with one arm `"electrical" => electrical::groups()`. The other wave 4 branches add their own arms.
  - `lib.rs`: `Tool::Ladder`, and `ui.tool` accepting `ladder`.
  - `canvas/interact/{mod.rs, tools.rs}`: ladder dispatch.
  - `panes/inspector.rs`: one call line.
  - `dock/tables.rs`: the Nets and Connections subjects.
  - `icons.rs`: ladder, terminal strip, cable, wire list and I/O icons, drawn in code.
- Unchanged: xtask. `provenance.rs` already scans every pack and builder.

**UI** (after the engine commits):
- The Electrical groups in the notation tab:
  - Standard: Symbols IEC, ANSI or JIC; Tags IEC 81346, JIC or Y14.44.
  - Ladder: Ladder tool, Number Wires.
  - Devices: Link to Coil, Terminal Strip, Cable, Assign I/O, Place Footprints.
  - Lists: Wire, Cable, Terminal, I/O, each opening the Tables dock on that subject, with CSV and XLSX export.
- The Ladder tool: drag a rectangle to get `ladder.insert` with the rung count from height divided by spacing, with a live preview of the rails. Escape returns to the pointer.
- Dropping or dragging a device inside a ladder frame runs `ladder.place` in the same `edit.batch`, so it is one undo step.
- The Inspector Electrical group: designation parts (read only), the coil's contact list with Go to (`xref.go`), Link to Coil, a terminal strip grid (number, type, bridge, internal and external wire, Add, Renumber, Bridge), and a cable's core list.

## Tests

- Pack loads through `load_shipped`; every shipped glyph has a `source`; every builder port id matches its type's ports (the stencils test).
- `vizcraft-cli validate` passes every case in `tests/electrical/`.
- Each example passes the route-quality checker.
- SVG goldens for the control circuit example in each of the IEC, ANSI and JIC variants, the ladder example, and the one-line example. Bless them only after opening each PNG export.
- `native_tests` round trip of an electrical document.
- `hostile_params_never_panic` green; `cargo xtask ci` green.
- Kittest: ribbon groups present only on an electrical page; Ladder tool drag commits one `ladder.insert`; device drop in a ladder snaps to a rung in one undo step; the Inspector terminal grid edits a terminal number; the Tables dock wire list.
- Snapshots: the notation tab, the Inspector Electrical group, the Tables wire list, and a ladder page. List them all in the report.

## Owner checklist (after the merge, in Terminal, from the integration worktree)

Build and launch with:

```sh
cd ~/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b && CARGO_TARGET_DIR=target-eng-standards CARGO_INCREMENTAL=0 cargo build --release -p vizcraft -p vizcraft-cli && ./target-eng-standards/release/vizcraft
```

1. On the New page, open the Engineering group and pick Control Circuit with nothing changed. You should see an empty A3 sheet with a title block and an Electrical tab, and the palette shows contacts, coils, fuses and pushbuttons.
2. Drop a Coil, then drop a Contact and type its Tag `K1` in the Inspector. The contact shows the coil's sheet and zone, and the coil shows a small contact list under it.
3. Press W and drag from one pin of the contact to a pin of the coil. A wire joins them, and the status bar names its net.
4. Electrical tab > Standard > Symbols > JIC, then Tags > JIC. Every symbol redraws in place and K1 becomes CR1. Ctrl+Z puts everything back in one step.
5. File > New > Ladder Diagram. Choose Ladder from the Electrical tab and drag down the page. You get two rails, L1 and L2, with rung numbers.
6. Drop a pushbutton and a coil onto rung 3, wire them to the rails, then click Number Wires. The wires get numbers built from the rung, such as 103.
7. Leave one coil pin unwired and open Problems. It lists the unconnected pin.
8. Click Terminal Strip. A strip X1 of terminals appears, and the Inspector shows a grid of them.
9. Click Lists > Wire List. The Tables dock shows a row per wire with from, to and number; Export CSV saves a file.
10. Open Help > Examples > Motor Control. A filled two-page sample opens.

## Report (final message)

- Commits.
- Every command id added: `electrical.*`, `ladder.*`, `terminal.*`, `cable.*`, `io.assign`, `net.export`, the `table.list` subject, `ui.tool` ladder.
- Each shared hunk, as landed.
- Snapshots retaken and goldens blessed.
- Doc rows, which the orchestrator writes:
  - ROADMAP: the electrical pack done.
  - docs/parity.md: electrical symbols, ladder tool, terminal strips, cables, ERC, wire and cable lists, project symbol switch.
  - docs/mcp.md: every new command with its params and result.
  - docs/formats.md: the netlist JSON and text format, and the table subjects for CSV and XLSX.
- Follow-ups.

## Risks and open questions

- **Ladder defaults.** The owner's rule makes IEC and ISO A3 the default everywhere, but ladders are a JIC and NFPA practice. This brief suggests JIC on ASME B for the Ladder type only; ask the owner. If the `b6-doctypes` format allows a setup step, the Ladder type could insert empty rails as setup, not content. Otherwise the user draws them with the Ladder tool.
- **KiCad netlist.** It is not on the AGENTS.md Formats list, so this branch writes only its own netlist format. Adding KiCad's public netlist documentation to that list is for the main session.
- **Designations.** They recompose live, as in EPLAN, while `sheet` context tags recompose on demand. The electrical pass runs before `nesting::owners`, so a device moved into a new location is retagged one sync later; check whether that is visible.
- **Overlap with the other wave 4 branches.** `NetCheck` and `Port.netGroup` are generic; process and fluid power may add similar checks in parallel. The orchestrator should keep one at merge.
- **Variant ids.** The research named the variants `iec60617` and `ieee315`; this brief uses `electrical.iec`, `.ansi` and `.jic` so that `Document.standards` selects them.
- **Data from secondary sources.** The two-letter 81346-2 subclasses, the JIC abbreviations beyond the twelve in the research, and the C37.2 number table all come from secondary sources. Mark them in code comments.
- **Junctions in the wire list.** Resolving junction ends to pins in daisy-chain order is a convention, not a standard.
- **The palette preview section.** The "Engineering symbols (preview)" section in `palette.rs` stays; removing it belongs to the orchestrator after all four packs merge.
- **Count tests.** Adding connector types and glyphs may break the connector-kind count tests and the palette search test. Check that they filter by prefix.

### Critical Files for Implementation
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/schema.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/rules.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/bind_nets.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/model/src/identity.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/stencils/src/builders/mod.rs