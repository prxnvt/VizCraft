# Diagramming for electrical and mechanical engineers: standards digest, UX review and a plan

*Research date: 2026-10-19, against main at a056cfe (Block 5 briefs; Block 5 wave 1 running in
its own worktrees). This report is the electrical and mechanical counterpart of
[swe-diagramming-ux.md](swe-diagramming-ux.md), which covers software and systems notations and
proposes Block 6 (templates, the engineering workspace preset, the Code pane and the Analyze
group; approved by the owner on 2026-10-19, briefs not yet written). It digests the drawing standards engineers in those two disciplines work to (IEEE and
ANSI, IEC and ISO, ISA, ASME, JIC and NFPA), reviews the workbench as it is from their point of
view, and proposes the packs, templates, workspace features and engine pieces that would make
VizCraft the most direct way for an electrical or mechanical engineer to produce a correct,
standard-conforming diagram. The four research notes behind it, with a source link on every
claim, are in [ee-me/](ee-me/): [electrical.md](ee-me/electrical.md),
[process-fluid.md](ee-me/process-fluid.md), [mechanical.md](ee-me/mechanical.md) and
[tool-ux.md](ee-me/tool-ux.md). Recommendations marked "(synthesis)" are this report's design
proposals; every other claim is sourced in those notes. The block proposal at the end and every
recommendation in its decisions table were approved by the owner on 2026-10-19; the approval and
how the block runs are recorded in [docs/cascade-plan.md](../cascade-plan.md).*

## Summary

Electrical and mechanical engineering diagrams differ from software diagrams in three ways that
shape everything below. First, they are governed by published standards with symbol tables,
sheet formats and numbering rules, and two families of those standards (the IEC and ISO family
used in most of the world, the IEEE, ANSI and ASME family used in North America) draw the same
thing differently, so a tool has to let a project pick a family and redraw. Second, their lines
mean physical connections: a wire is a net with a number and a potential, a pipe is a line with
a number, a size and a service, a hydraulic line carries pressure from a pump to an actuator, and
a tool that only draws arrows between boxes cannot answer the questions engineers ask of their
drawings (what is connected to what, is every port used, does every contact have a coil). Third,
the drawing is a controlled document: it sits on a sheet with a frame, zones, a title block and a
revision table, and every device, wire, line and part on it has an identifier that other
documents (lists, schedules, bills of material) key on.

The incumbents confirm where the value is. The electrical CAD tools engineers pay for (EPLAN,
AutoCAD Electrical, SOLIDWORKS Electrical, E3.series) sell a project tree keyed by function and
location, automatic wire numbering and device tagging, coil-to-contact cross references, terminal
strips, and reports generated from the database; their forums are full of wire numbers that float
off their wires because they were stored as graphics. The P&ID tools (Plant 3D, AVEVA Diagrams,
Hexagon Smart P&ID) compose tags per class from a format, pair off-page connectors, and validate
the drawing set. The fluid power tools (Automation Studio, FluidSIM) build directional valves
from parameters and simulate. General diagramming tools, ConceptDraw and Visio among them, ship
large symbol libraries organised by discipline with no model behind them, and that is what their
users complain about. The open niche is therefore the same one the earlier research found for
software: a fast local tool whose drawing is a view of a typed, identified, validated model, with
the standards' symbol sets as redrawable variants.

VizCraft's substrate is already well placed. Packs are data with element types, ports, typed
properties, connection tables, identifier formats and rules that cite their standard clause; the
identity registry numbers tags; off-page pairs are matched; tables over the model, path glyphs,
ports drawn on shapes and per-page drawing variants are landing in Block 5. What it lacks for
these disciplines is a net model for wires and pipes with junctions, sheet frames with zones and
field-bound title blocks, cross-references computed from sheet and zone, composed reference
designations across location and function containers, parametric glyphs for the composed symbol
families (ISO 1219 valves, ISA bubbles, IEC contacts), orientation-aware symbol placement, and
the engineering checks and reports that make the drawing worth more than its picture.

**(synthesis)** The proposal: a shared "sheet and net" substrate, then an Electrical track and
a Mechanical track of notation packs, each pack with its standard-conforming variants (IEC 60617
versus IEEE 315 and JIC symbol sets; ISO 5457 and 7200 versus ASME Y14.1 sheets), a template
catalogue of about thirty starting points, contextual ribbon groups for wires, nets, sheets and
reports, a Sheets navigator and a Nets branch in the Model Tree, and an analysis layer that
extracts connectivity, runs electrical and fluid power rule checks, and produces the standard
document set (bills of material, wire, cable, terminal, line, instrument, valve and I/O lists) as
live tables. The block proposal at the end sequences this after Block 5, sharing the template
system, key and title block pieces of the proposed Block 6, and ahead of the Block 7 industrial
track, whose P&ID and loop packs it absorbs.

## The workbench today, seen by an electrical or mechanical engineer

Screenshots were rendered headlessly from this branch with `ui_shot` at 1440 by 920 points and
are kept out of the repository; the swe report describes the same views in its table. The
findings below are the ones specific to these disciplines; the general ones (the sample is a free
drawing, the New page has two cards, the Home tab is an office ribbon, no text form, no key) are
in the swe report and are not repeated.

| Area | What exists | What an electrical or mechanical engineer misses |
|---|---|---|
| Page and sheet | Page setup with Letter, Legal, Tabloid, A3, A4 and A5 presets, orientation, units (inches, feet, millimetres, centimetres, metres, points, pixels), a drawing scale, headers and footers with `{page}`, `{pages}`, `{pageName}`, `{title}` and `{date}` fields; Borders and Titles as a generated background page with a frame, banner or margin and a title block showing the document title and page name | ANSI A to E and ISO A0 to A2 sheets; a frame with zone letters and numbers along the margins; a title block with the fields drawings carry (drawing number, sheet n of m, revision, scale, projection symbol, drawn, checked, approved, date, company, project, material and finish where relevant); a revision table; a parts list block; per-project values filled once and shown on every sheet |
| Grid and snap | Grid display and snap to grid, shapes, connection points and guides; alignment guides while dragging; the grid spacing follows the page units | A symbol grid the pack dictates (2.5 mm or 0.1 inch for schematics, the ISO 10628 module for process drawings) with every pin on it, so wires meet pins without manual nudging; orthogonal-only wire drawing; a snap to the symbol grid that is on by default for a schematic page and off for a free drawing |
| Shapes | Sixteen built-in outlines; freeform shapes; path glyphs, ports drawn as small squares with labels and per-page variants in Block 5; rotation by angle; flips through Arrange | Symbols with lead stubs that extend past the body to the grid; rotation and mirroring by single keys with the label staying upright; symbol orientation stored as a property so a contact drawn vertically in a ladder and horizontally in a circuit is the same element; text inside the symbol driven by properties (the ISA tag letters in a bubble, the device number in a relay circle, the 4/3 position boxes of a valve); composed symbols built from a parameter set rather than one glyph per combination |
| Connectors | Straight, orthogonal and curved routes with obstacle avoidance and line jumps; thirteen line ends plus the Block 5 slash; dash patterns solid, dashed, dotted, dash-dot and long dash; connector kinds from the pack; glue to shapes and to connection points; end labels in Block 5 | Wires that join other wires: a junction (a dot) where one wire ends on another; a net that groups every wire segment with the same potential and carries the wire number; wire numbers and colours drawn along the segment; line kinds with along-the-line markers (ISA's pneumatic double slash, electric dashed, capillary crossed, software dotted with circles; bus lines drawn thick; a line that crosses without joining drawn with a jump or nothing, by project rule); gap or jump policy per page; pipe lines with a size, class and service shown as a composed line number tag; connectors that leave a port in the port's direction on a fixed grid |
| Model | Elements with types, typed properties, tags composed from identifier formats with auto-numbering, containment, relationships with ports, viewpoints, rules with standard references, rulesets and a conformance profile, paired off-page references | Location and function containers whose tags compose into the device designation (`=F1+A2-K3` in IEC 81346, or a panel and sheet prefix in North American practice); a tag format that reads the owner's tag; cross-references from a relationship (a contact to its coil) shown on both symbols as sheet and zone; nets and potentials as model objects; cables as owners of wires; terminal strips as ordered containers of terminals; a sheet zone computed from a shape's position |
| Validation | Live and batch rules from a fixed vocabulary of twenty-six checks with fixes, a Problems dock with navigation and filters | The checks these disciplines expect, which need the net model: unconnected pin, net with one connection, two outputs on a net, a contact without a coil, a coil with more contacts than its complement, a wire without a number, duplicate wire numbers, a terminal used twice, an instrument tag not matching the ISA grammar, a loop with no final element, a valve port left open, a pump with no reservoir, a line number not composed from the project format |
| Tables and reports | `report.run` over shapes and shape data (CSV and HTML); `table.list`, `table.matrix`, CSV and XLSX export landing in Block 5 | The document set: bill of material grouped by part number, wire list (from, to, number, colour, gauge), cable schedule, terminal plan, device list, line list, equipment list, instrument index, valve list, I/O list, parts list bound to balloon callouts; each as a live table that edits the model and as a sheet placed in the document |
| Analysis | Rules only; the swe report proposes dependency, cycle, reachability and single point of failure analysis over the model graph | Connectivity extraction (which pins are on one net), netlist export, net highlight on hover, loop tracing from sensor to final element, fluid power port and pressure path checks, kinematic mobility, fault tree cut sets, reliability block availability, cable length totals through the drawing scale |
| Ribbon and panes | Home, Insert, Design, Data, Process, Model, Review, View, Developer, Help; contextual tabs for connectors, containers, callouts, pictures, ink and the active notation; Palette, Model Tree, Inspector, History, Layers, Comments, Pan and Zoom; Problems, Search, Activity, Tables and Generate in the dock | A Wire tool and a Pipe tool that draw orthogonally on the symbol grid and make junctions; Rotate and Mirror on single keys; a Variant switch (IEC or ANSI); a Sheet group (frame, title block, zones, revision); a Reports group; a Sheets navigator with thumbnails; a Nets branch in the Model Tree; a terminal strip editor; a cross-reference jump on the symbol |

What works, and the design should keep, is the same list as the swe report: everything is a
command, packs are data, insertion on the canvas is model-aware, the Inspector numbers tags live,
Problems carries fixes, breadcrumbs drill down, the board is infinite and the router avoids
obstacles with line jumps. Two existing pieces matter more here than there: the drawing scale
on a page (a cable length can be measured), and the paired identifier scope (an off-sheet wire
or pipe continuation is a pair the registry already matches).

## Standards digest

What each family of standards fixes, condensed to what a pack needs. The full tables with
editions, families, counts and sources are in the four notes under [ee-me/](ee-me/). Two facts
apply throughout. Every IEC, ISO, IEEE, ASME, ISA and AWS text is paid and copyrighted, so
glyphs are drawn in code from the written descriptions (and from the public domain MIL-STD and
NASA documents where they exist), never traced from a standard's figure or another tool's
library; IEC 60617 symbol ids and ISO 14617 registration numbers may be cited as metadata. And
the standards come in two families that draw the same element differently, which is why the
Block 5 variant machinery (one element type, one glyph per variant, a per-page switch) is the
right foundation.

### Electrical symbols, designations and documents

| Standard | What a pack takes from it |
|---|---|
| IEEE 315-1975 (ANSI Y32.2), 315A-1986 | The North American symbol set, about 22 functional sections and several hundred symbols with one-line forms beside complete forms; class designation letters in clause 22. Inactive-reserved since 2019 but still what JIC and NFPA libraries draw |
| IEEE 91-1984 and 91a | Logic symbols in two shape sets, distinctive (from MIL-STD-806B, public domain) and rectangular (IEC 60617-12); the 1962 edition forbade mixing the two on one diagram, which is a rule |
| IEC 60617 database | The international symbol set, about 1500 to 1900 symbols in the areas of the former parts 2 to 13 (qualifying symbols, conductors, passive components, semiconductors, energy, switchgear and protection, instruments and lamps, telecommunications, installation plans, binary logic, analogue). Paid subscription; ids such as `S00006` are stable keys. Qualifying symbols compose onto base outlines, the model for parametric glyphs |
| IEC 61082-1:2014 | Document kinds (overview, function and circuit diagrams; wiring, terminal and cable documents), the sheet grid with lettered rows (I and O excluded) and numbered columns, and cross-reference forms such as `/B2`, `/2/` and `/3/B2`; every interrupted line carries page and zone at both ends |
| IEC 81346-1:2022 and 81346-2:2019 | Reference designations as trees of aspects: `=` function, `+` location, `-` product, `%` type; the class letter table (B sensing, F protecting, K relays, M motors, P indicating, Q power switching, R passive, S operator controls, T transforming, W conductors, X terminals); a tag is letters plus a number, composed down the owner chain |
| IEC 60445:2021, IEC 60204-1, NFPA 79-2024 | Conductor and terminal identifiers (L1 L2 L3 N PE), reserved colours (green-yellow only for PE), wire colours by circuit class, orange for circuits live with the disconnect off |
| IEEE C37.2-2022 | Device function numbers for one-line and protection diagrams (27 undervoltage, 50 and 51 overcurrent, 52 breaker, 86 lockout, 87 differential, about 95 numbers with suffixes) |
| ASME Y14.44-2008 | Electronics class letters (R, C, L, U, Q, J, P, K, S, T) and the unit and assembly hierarchy (`1A12A2R3`) |
| JIC EMP-1 and EGP-1, NEMA ICS 19-2002 (R2022) | The North American industrial control set: contacts as two bars (NC with a diagonal), circle coils with the tag inside, overloads, pilot lights with rays, lozenge fuses; abbreviations CR, M, OL, LS, PB, SOL, TR, PL, SS, CB, FU, DISC; the JIC documents were folded into NFPA 79 in 1985 but their symbol style persists in every tool's "JIC" library |
| Ladder conventions | Vertical rails L1 and L2, rungs numbered down the left rail, wire numbers derived from the rung (2003 is page 2 line 3), the contact locations listed beside the coil with NC underlined, continuation arrows between sheets, devices drawn de-energised |
| IEC 61131-3:2025, IEC 60848:2013, IEC 61499 | Ladder, function block and sequential function chart elements and the rules a validator can check: every network starts at the left rail and ends in a coil, power flows left to right, steps and transitions alternate, every macro-step has an exit, events are associated with data |

### Process, instrumentation, piping and fluid power

| Standard | What a pack takes from it |
|---|---|
| ANSI/ISA-5.1-2024 (2009 text read) | The tag grammar: first letter (measured variable), modifier, succeeding letters (readout then output functions), loop number, suffix; the letter table by column; the bubble matrix (circle for discrete, circle in square for shared display, diamond in square for safety systems, hexagon for computer, crossed by none, one, dashed, double or double dashed lines for field, panel front, panel rear, local front, local rear); the line kinds (pneumatic slashed, electric dashed, hydraulic, capillary crossed, software, fieldbus, wireless); fail positions FO, FC, FL; nine loop numbering schemes (parallel, serial, mixed) |
| ISA-5.2, 5.4, 18.2, 20 | Binary logic for interlocks; loop diagrams with one loop per sheet in field, junction box, marshalling, control room and panel sections; alarm identification; instrument datasheets keyed by tag |
| ISO 10628-1:2014 and 10628-2:2012 | Block diagram, PFD and P&ID minimum content; the 2.5 mm module; line widths 1.0, 0.5 and 0.25 mm by role; flow left to right; unconnected crossings interrupt the vertical or thinner line; continuation arrows carry a reference; about 400 equipment and piping symbols in 29 groups with ISO 14617 registration numbers |
| ISO 14617 parts 1 to 15 (parts 1 and 2 reissued 2025) | The registered symbol library behind the application standards: connections, actuators, measurement and control, basic mechanical elements, valves, pumps and compressors, fluid power converters, heat transfer, separating and mixing, material processing, transport, installation; supplementary symbols compose onto basic ones |
| PIP PIC001 (May 2026) | Process Industry Practices P&ID criteria: equipment class letters, line numbers composed from size, service, sequence, class and insulation, where a sequence changes (branches to other equipment, relief valve inlet and outlet) and where it does not (control valves, class breaks), off-page connector placement and content, three major items per sheet at most |
| DEXPI 1.4 (Proteus XML) and 2.0, CC BY 4.0 | A ready P&ID class model: about 168 equipment classes with nozzles as node owners, piping network systems and segments running node to node with size, class, fluid and flow direction, 20 valve classes, 35 instrumentation classes (process instrumentation function, signal generating and actuating functions, loops, signal off-page connectors), each with an ISO 15926 reference data URI; the obvious export target and taxonomy |
| ASME Y32.2.3, Y32.2.4, Y32.2.6, Y32.4, Y32.11, Y32.18; ASHRAE 134; SMACNA | Mostly historical or stabilised US symbol sets for pipe fittings (five joint styles per fitting), HVAC, heat power, plumbing fixtures and PFDs; Y32.18 (mechanical and acoustical elements as two-terminal graphs) is in effect; ASHRAE 134 and SMACNA govern today's HVAC drawings; isometrics are a practice (30 degree axes, north arrow, welds, BOM) |
| ISO 1219-1:2012, 1219-2:2012, 1219-3:2016; ASME Y32.10; JIC H-1 and P-1 | Fluid power symbols composed from basic elements on a module grid, never showing construction; directional valves as position squares with flow paths, ports named per ISO 9461 (P T A B L X Y) or ISO 5599 (1 2 3 4 5 12 14), actuators outside the end squares, port numbers only on the rest square, drawn unactuated; component codes (`1V3`, circuit 0 the power unit; the 2012 edition prefers numeric codes to avoid the 81346 clash); ISO 1219-3 stacking of connectable symbols; Y32.10 and the JIC standards are the superseded US and machine tool predecessors; the displacement-step diagram (VDI 3260) as the sequence chart beside a pneumatic circuit |

### Mechanical sheets, annotation and schematics

| Standard | What a pack takes from it |
|---|---|
| ASME Y14.1-2020 | Sheet sizes A to E and F with margins, zones and the title, revision and application blocks in fixed positions; the public domain NASA GSFC drawing manual and MIL-STD-100G are the free references for the geometry |
| ISO 5457:1999 and ISO 7200:2004 | Exact sheet geometry (trimmed and drawing space for A0 to A4, 20 mm left and 10 mm other borders, 0.7 mm frame, 50 mm grid fields with counts per size, letters down and numbers across, I and O excluded, centring and trimming marks, A4 portrait only) and the title block fields with obligation and character limits (eight mandatory: legal owner, identification number, date of issue, sheet number, title, approval person, creator, document type; optional revision index, number of sheets, language, department, status, page counts, paper size) |
| ASME Y14.2, ISO 128-2 | Line types and two widths per sheet in a 2:1 ratio |
| ASME Y14.24-2020, ISO 29845 | The drawing types a document carries (detail, assembly, installation, arrangement, interface, mechanical schematic, the seven electrical diagram kinds, wiring harness, cable assembly, kit, drawing tree) and their minimum content |
| ASME Y14.34, Y14.35 | Parts, data, index, application and wire lists; the revision history block (zone, rev, description, date, approved), revision letters omitting I, O, Q, S, X and Z, AA after Y, a revision symbol with a leader and a zone |
| ASME Y14.5-2018, ISO 1101, ISO 5459:2024 | Fourteen geometric characteristics (ASME dropped concentricity and symmetry in 2018, ISO keeps them), the feature control frame grammar (characteristic, tolerance with diameter flag and modifier, up to three datums in precedence), datum feature symbols; every referenced datum must exist |
| ASME Y14.36, ISO 21920-1 | Surface texture symbol with three variants and parameter slots |
| ISO 2553:2019, AWS A2.4:2020 | Welding symbol: arrow line, reference line, weld symbols on the arrow and other side (ISO uses a dashed identification line, AWS puts the other side above), size as throat or leg, process number in the tail; the two conventions are a variant |
| ISO 3952 parts 1 to 4, ISO 14617-7 | Kinematic symbols: motion arrows, pairs (revolute, prismatic, screw, cylindrical, spherical, planar), links, four-bar members, friction wheels, gears, cams and followers, Geneva and ratchet, couplings, clutches and brakes with actuation qualifiers, springs; the Kutzbach mobility count `M = 3(n-1) - 2J1 - J2` is a check |
| IEC 61025:2006, 61078:2016, 60812:2018, 62502:2010 | Fault tree events and gates with transfer pairs, reliability block diagrams with series, parallel, k-out-of-n and standby, FMEA table columns with RPN, event trees; each with closed-form validator rules |
| FIPS 183 (public domain), ISO/IEC/IEEE 31320-1 | IDEF0 boxes with ICOM arrows and the exact well-formedness rules (three to six boxes per diagram, each with at least one control and one output, tunnelling, feedback routing) |
| SysML v2 (OMG formal, September 2025) | Part definitions and usages, typed ports, interface and connection definitions; out of scope here, shares the port model |
| Bond graphs, control block diagrams, signal flow graphs | Textbook notations with strict rules (one causal stroke per bond, one effort setter per 0-junction, signed summing inputs, Mason's gain formula) that make good validator and analysis targets |

### Catalogue precedent: how the libraries are organised today

ConceptDraw DIAGRAM, which the owner points to as the closest existing product, organises its
engineering content as solutions, each a set of stencil libraries plus sample drawings and a few
templates, grouped by discipline: an Electrical Engineering solution of 26 libraries and 926
symbols (analog and digital logic, delay elements, circuits, electron tubes, field effect
transistors, inductors, integrated circuits, lamps and readouts, logic gates, maintenance, power
sources, qualifying symbols, resistors, rotating equipment, diodes, semiconductors, stations,
switches and relays, terminals and connectors, thermal, transformers and windings, transistors,
transmission paths, radio frequency, composite assemblies); a Mechanical Engineering solution of
8 libraries and 602 symbols (welding, valve assembly, pneumatic and hydraulic pumps and motors,
fluid power valves and equipment, dimensioning and tolerancing, bearings and springs); a Chemical
and Process Engineering solution of 8 libraries and 468 symbols (process flow, heating,
industrial equipment, instruments, pumps, valves and fittings, vessels, annotation) with PFD and
P&ID templates; One-line Diagrams with 4 libraries and 116 symbols; Digital Electronics with 9
libraries and 158 symbols, among them five logic gate styles (ANSI, BS, DIN, IEC, NEMA); Basic
Circuits, Power Circuits, Fault Tree Analysis (14 symbols), Electronic Block Diagrams and others.
Each sample names the libraries it used and an estimated drawing time. None of the solution pages
names a standard beyond "internationally accepted", and none describes wire numbering, cross
references, nets, tags or reports; the symbols are vector shapes dropped from a library. Visio
Professional's Engineering category is similar (Basic Electrical, Circuits and Logic, Fluid
Power, Industrial Control Systems, Parts and Assembly, Piping and Instrumentation, Plumbing and
Piping, Process Flow, Systems), with glue through connection points and, for P&ID, a pipeline that
splits when a valve is dropped on it and a renumber command.

**(synthesis)** The catalogue shape is worth taking: a discipline, inside it a handful of named
starting points, each with a scoped palette, a few samples and an estimate of what it takes to
draw. The content is not: every symbol is drawn originally, and the library is a view of a typed
pack rather than the product. What those products leave undone (nets, tags, cross-references,
sheets with zones, reports, checks) is the whole reason to build this.

## Principles for an intuitive engineering diagrammer (synthesis)

The ten principles of the swe report hold here (start from a use case, text and canvas are one
model, keyboard-first creation, layout by default, every diagram self-describing, lint never
block, levels are pages, import what exists, analysis over the model, do not look like an office
suite). Seven more are specific to these disciplines.

1. **One standard per project, redrawable.** The project picks IEC or ANSI and JIC for
   electrical symbols, ISA or ISO 10628 for process, ISO 5457 and 7200 or ASME Y14.1 for sheets,
   ISO 2553 or AWS A2.4 for welds. Each is a pack variant on shared element types, so switching
   redraws every view without re-placing anything, and a rule forbids mixing shape sets on one
   sheet.
2. **Wires and pipes are nets, not arrows.** A wire ending on a wire makes a junction; a net is
   a model object that owns every segment with the same potential and carries the number; a
   crossing never connects; merging two named nets asks first. The number is a property of the
   net, never a graphic glued to a line.
3. **Pins live on the symbol grid.** The pack fixes the module (2.5 mm, 0.1 inch) and every
   port of every glyph sits on it, so symbols placed on the grid connect by abutment and wires
   are orthogonal by default.
4. **Symbols are parameters, not catalogues.** A contact is NO or NC, timed or not, with a pole
   count; a valve is positions, ports, a path table, actuators and a return; an ISA bubble is a
   system column and a location row; a feature control frame is a characteristic, a tolerance and
   datums. The glyph is rendered from the properties, so the Inspector edits the symbol.
5. **Identifiers compose down the tree.** Function and location containers contribute their
   tags to the device designation (`=F1+A2-K3`), rungs contribute to wire numbers (`2003`), loops
   to instrument tags (`FIC-101`), lines to their composed number; the registry numbers and
   checks all of them, and renumbering keeps cross-references.
6. **The sheet is part of the document.** Frames, zones, title blocks, revision tables and parts
   lists are generated from document and page properties, zones are computed from position, and
   every cross-reference reads sheet and zone (`/3.B4`) so a printed set stays navigable.
7. **Reports are the second view of the model.** The bill of material, wire list, terminal plan,
   line list and instrument index are live tables that edit the model and place as sheets in the
   same document.

## Template catalogue (synthesis)

A template is data in a pack, as the swe report defines it: viewpoint, seed script, layout, key
and title block, code forms, keywords, plus here the sheet standard, the symbol variant and the
module. Each row below is one template on the New page, grouped under Electrical, Process and
Mechanical audience chips; the packs are the ones designed in the next section.

| Template | Pack and viewpoint | Variants | Seed | Sheet | Checks that matter |
|---|---|---|---|---|---|
| Control circuit (IEC) | Electrical / Schematic | IEC 60617 glyphs, IEC 81346 tags | Supply rails L1 and N, a fuse, a pushbutton pair, a contactor coil with a holding contact, a motor on a separate power page | ISO A3 landscape, zone grid, ISO 7200 title block | Designations unique, every wire numbered, contact to coil cross references resolve, interrupted lines paired |
| Ladder diagram (JIC) | Electrical / Ladder | JIC and NEMA glyphs, CR1 and PB1 tags | Rails L1 and L2, rungs 1 to 10 with line numbers, a start and stop rung with a seal-in contact, a pilot light rung | ANSI B landscape, zone grid, ASME title block | Rung-derived wire numbers, one output per rung, NC cross references underlined |
| Motor control (power and control) | Electrical / Schematic | both | Disconnect, fuses, contactor, overload, three-phase motor on one page; the control circuit on a second | as chosen | Overload contact wired into the control circuit, terminal numbers typed |
| One-line power distribution | Electrical / One-line | IEEE 315 one-line forms, C37.2 device numbers | Utility with fault current, main breaker, bus with voltage, transformer, two feeders with breakers and a motor | ANSI D | Every bus has a voltage, every breaker a frame and trip, relays carry valid device numbers with a CT or PT |
| Three-line and protection | Electrical / Three-line | IEEE | A bus, three phases, CTs with polarity, an 87 and a 51 relay | ANSI D | CT secondaries only to relays and meters |
| PLC I/O wiring | Electrical / I/O | both | A digital input module with eight channels wired to field terminals through a terminal strip; an output module page | as chosen | Each channel used once, every field terminal mapped, address and tag agree with the I/O list |
| Panel layout and terminal plan | Electrical / Panel | both | A panel outline with a DIN rail, the devices of the schematic as footprints, a terminal strip with bridges | as chosen | Every schematic device placed once, every terminal connected |
| Logic diagram | Electrical / Logic | IEEE 91 distinctive or IEC rectangular | Two inputs, an AND, an OR, a flip-flop, an output | ANSI B | No mixed shape sets, every input driven, one driver per net |
| Circuit schematic (electronics) | Electrical / Schematic | IEEE 315 or IEC 60617; Y14.44 R1 C1 U1 tags | A supply, a divider, an op-amp, a decoupling capacitor, ground | ANSI B | Unconnected pins, one driver per net, polarity on polarised parts |
| Cable and interconnection | Electrical / Interconnection | both | Two enclosures, a cable with three cores, connectors with pin numbers | as chosen | Every cable has two ends, cores match connector pins |
| Process flow diagram | Process / PFD | ISO 10628 or ISA | Feed, a heat exchanger, a column, a pump, a product stream with stream numbers and a stream table | ISO A1 | Streams numbered with rates and conditions |
| P&ID | Process / P&ID | ISA-5.1 bubbles with ISO 10628 equipment, or PIP style | A vessel with nozzles, an inlet line with a control valve, a level loop (LT, LIC, LV), a relief valve, two off-page connectors | ISO A1 or ANSI D | Tag grammar, line numbers composed, off-page pairs, nozzles connected, loop complete |
| Instrument loop diagram | Process / Loop | ISA-5.4 | Field, junction box, marshalling and control room lanes with the loop's transmitter, cable pairs, terminals and I/O channel | ANSI B | One loop per sheet, every terminal landed, channel unique |
| Cause and effect matrix | Process / Logic | ISA-5.2 | Three causes against four effects (reuses the Block 7 matrix view) | ANSI B | Every cause and effect is a tag |
| Hydraulic circuit | Fluid power / Hydraulic | ISO 1219 (filled triangles) | Reservoir, fixed pump with motor, relief valve with gauge, filter, a 4/3 solenoid valve, a double-acting cylinder | ISO A3 | Every port connected or plugged, relief valve after the pump, pressure path pump to actuator to tank |
| Pneumatic circuit with sequence | Fluid power / Pneumatic | ISO 1219 (hollow triangles), ISO 5599 ports | Air supply unit, a 5/2 pilot valve, a cylinder with two limit switches, the displacement-step diagram beside it | ISO A3 | Ports match the valve type, steps reachable from the valve path tables |
| Hydraulic power unit | Fluid power / Hydraulic | ISO 1219 | The power unit as a pattern (reservoir, pump, relief, filter, cooler, gauge) | ISO A4 | as hydraulic |
| HVAC flow diagram | Process / HVAC | ASHRAE | Air handler with supply and return ducts, two VAV boxes, diffusers, a chilled water coil with supply and return | ANSI C | Every run has a size and airflow, dampers typed |
| Plumbing riser | Process / Plumbing | ANSI Y32.4 | A meter, a backflow device, a riser with three floors of fixtures, cold and hot water systems | ANSI C | Every fixture on a system, fixture unit sums |
| Piping isometric | Process / Isometric | practice | One line number on a 30 degree grid with elbows, a valve, field and shop welds, a north arrow and a BOM | ANSI B | BOM rows match item tags, welds typed |
| Sheet with title block (ISO) | Sheet / Frame | ISO 5457 and 7200 | An A3 frame with zones, centring marks, the title block with the eight mandatory fields bound to document properties, a revision table | ISO A3 | Mandatory fields filled, sheet n of m, revision letters sequential |
| Sheet with title block (ASME) | Sheet / Frame | ASME Y14.1, Y14.35 | An ANSI B frame with zones, title block, revision history and application blocks | ANSI B | as ISO plus the Y14.35 letter sequence |
| Assembly with parts list | Sheet / Assembly | both | A picture or drawing placeholder, five balloons bound to a parts list block | ANSI C | Balloon to row bijection, item numbers unique and sequential |
| Kinematic diagram | Mechanical / Kinematic | ISO 3952 | A four-bar linkage with ground, numbered links, lettered joints and the mobility count | ISO A4 | Joints join two links, mobility shown, warn when zero or less |
| Power transmission | Mechanical / Transmission | ISO 3952 and 14617-7 | Motor, coupling, gearbox with two gear pairs, shaft with bearings, a belt drive to a load | ISO A4 | Ratio along the path, every shaft mounted |
| Bond graph | Mechanical / Bond graph | textbook | A source, a 1-junction, R, C and I elements with causal strokes | ISO A4 | One stroke per bond, one effort setter per 0-junction, one flow setter per 1-junction |
| Control block diagram | Mechanical / Control | textbook | Input, summing junction, controller block, plant block, feedback branch, output | ISO A4 | Signed inputs, no dangling signals, loops listed for Mason's formula |
| Fault tree | Reliability / FTA | IEC 61025 | A top event, an OR gate, an AND gate, four basic events, a transfer pair | ANSI B | One top event, gates with two or more inputs, acyclic, transfers paired |
| Reliability block diagram | Reliability / RBD | IEC 61078 | Start, two series blocks, a parallel pair, a 2-out-of-3 group, end | ANSI B | Start-to-end path, k at most n, availability computed |
| FMEA worksheet | Reliability / FMEA | IEC 60812 | A table with the standard columns and three rows (reuses the Tables dock) | ANSI B | RPN recomputed, every mode has an effect |
| IDEF0 | Systems / IDEF0 | FIPS 183 | An A-0 context box, then A0 with three boxes and ICOM arrows | ANSI B | Three to six boxes, each with a control and an output, ICOM codes in parent order |
| GD&T and welding annotation sheet | Annotation | ASME Y14.5 or ISO 1101; AWS or ISO 2553 | A detail picture placeholder with a datum symbol, two feature control frames, a welding symbol and a surface texture symbol | ANSI B | Datums resolve, modifiers only on features of size, side placement per convention |

## Pack designs (synthesis)

The research notes end in detailed pack syntheses (element types, ports, properties, identifier
formats, connection tables and validations); this section fixes the pack boundaries and the
substrate they share so the briefs can be written from the notes.

| Pack | Viewpoints | Variants | Taxonomy backbone | Notes link |
|---|---|---|---|---|
| `electrical` | Schematic, Ladder, One-line, Three-line, I/O, Panel, Interconnection, Logic | `iec60617`, `ieee315` (electronics), `jic` (industrial control); logic `distinctive` and `rectangular` | IEC 81346-2 classes as element supertypes (Sensing, Protecting, Relay, Motor, Indicating, PowerSwitch, Passive, OperatorControl, Transformer, Conductor, Terminal), with JIC and Y14.44 letter tables as tag formats | [electrical.md](ee-me/electrical.md), Implications |
| `process` | PFD, P&ID, Loop, Logic (cause and effect), HVAC, Plumbing, Isometric | `isa` and `iso10628` bubbles and equipment; `pip` line numbering; `ashrae` | DEXPI classes with their reference data URIs, ISA-5.1 tag grammar as the identifier format with a parallel or serial switch | [process-fluid.md](ee-me/process-fluid.md), Implications |
| `fluidpower` | Hydraulic, Pneumatic, Sequence (displacement-step) | ISO 1219 with hydraulic or pneumatic triangle fill; `iso9461` or `iso5599` port labels; 1995 letter codes or 2012 numeric codes | ISO 14617 registration numbers; the parameterised DirectionalValve, Pump, Cylinder and PressureValve glyphs | [process-fluid.md](ee-me/process-fluid.md), Fluid power pack |
| `sheet` | Frame, Assembly | `iso5457` with ISO 7200 fields, `asmey14_1` with Y14.35 and Y14.100 blocks | Sheet, Frame, TitleBlock, RevisionTable, PartsList, ApplicationBlock, ProjectionSymbol, Balloon, RevisionSymbol; fields bound to document and page properties | [mechanical.md](ee-me/mechanical.md), Sheet and Title Block pack |
| `mechanical` | Kinematic, Transmission, Bond graph, Control block, Signal flow | ISO 3952 only; AWS or ISO welding and ASME or ISO GD&T in the annotation types | Links, joints, shafts, gears, couplings, clutches, brakes; bond graph elements and junctions; blocks, summing junctions, branch points | [mechanical.md](ee-me/mechanical.md), Mechanical Schematic and Control Block Diagram packs |
| `annotation` | used on any page | `asme` or `iso` tolerancing, `aws` or `iso2553` welding | FeatureControlFrame, DatumFeatureSymbol, DatumTarget, WeldingSymbol, SurfaceTexture, SectionLine, ViewLabel, Note | [mechanical.md](ee-me/mechanical.md), Annotation pack |
| `reliability` | FTA, RBD, ETA, Bow-tie, Ishikawa, FMEA table | IEC 61025 symbols | Events and gates, blocks and voting groups, the FMEA columns | [mechanical.md](ee-me/mechanical.md), Reliability pack |
| `idef0` | A-0 and decomposition pages | FIPS 183 | Box with ICOM ports, decomposition as child pages (breadcrumbs) | [mechanical.md](ee-me/mechanical.md), section 4 |

The Block 7 industrial track's P&ID and PFD pack, industrial lists and loops and I/O pieces are
this `process` pack; its cause and effect, SFC and control module pieces, Structured Text
generation and PLCopen export stay in Block 7 and build on it. The Block 7 ladder and function
block packs (IEC 61131-3) share the `electrical` Ladder viewpoint's rails and rung substrate but
remain a separate programming pack because their elements are variables, not devices.

### The sheet and net substrate

Everything above needs five engine pieces that no pack can supply as data. They are the
substrate the block builds first.

| Piece | What it is | Where |
|---|---|---|
| Nets and junctions | `Net` as a model object (id, name or number, kind: wire, potential, pipe line, fluid line, signal) owning connector segments; a `Junction` glyph the canvas inserts where a wire ends on a wire or a pipe (a connector end may glue to another connector, which splits it); a net index rebuilt after every command by the sync pass, so `net.of(pin)` and `net.members(net)` are cheap; merging two named nets is a command that asks; crossings never connect; the wire number, colour and gauge are properties of the net, drawn along its segments by the canvas and the exports | `vizcraft-model` (Net), `vizcraft-diagram` (connector-to-connector glue, junction decor, along-the-line labels and markers), `vizcraft-notation` (`bind::sync` net pass), `engine/cmd/net.rs` (`net.list`, `net.highlight`, `net.merge`, `net.split`, `net.renumber`), `canvas/interact/wire.rs` (the Wire and Pipe tools) |
| Symbol grid and orientation | A per-page module from the pack (`View.module`, 2.5 mm or 0.1 inch) that the grid, snap and the Wire tool use; glyph ports declared on the module; `Shape.orientation` (rotation in quarter turns and mirror flags) applied to the glyph while the label block stays upright; `shape.rotate` and `shape.mirror` on single keys through the existing `ui.*` layer; lead stubs as part of the glyph path | `vizcraft-diagram` (orientation, text block upright flag), `vizcraft-notation` (module, port grid check in the loader), `canvas/interact/keys.rs` |
| Parametric glyphs | A glyph whose path is produced by a named builder from the element's properties instead of a fixed path: `contact` (normally open or closed, timed, poles, variant), `coil`, `bubble` (ISA system and location), `valve` (positions, ports, path table, actuators, return, port labels), `cylinder`, `pump`, `gate` (inputs, inversions, shape set), `fcf` (feature control frame cells), `weld`, `texture`; builders are code in `vizcraft-stencils` keyed by name from the pack, re-run by the sync pass when a property changes, so the Inspector edits the symbol | `vizcraft-notation` schema (`Glyph.builder`, `Glyph.params`), `vizcraft-stencils/src/builders/`, the Block 5 path glyph machinery |
| Sheets, zones and fields | `Page.sheet` (standard, size, orientation, zone scheme) with a generated frame on the page's decoration page (the Borders and Titles machinery, extended); `zone_of(point)` from the sheet; `{{zone}}`, `{{sheet}}` and `{{doc:key}}` text fields; document properties grown into a typed, pack-declared set (project, drawing number, revision, company, drawn, checked, approved); a revision table and a parts list as compartment shapes bound to model queries; the title block as a glyph with field slots | `vizcraft-diagram` (sheet, zones, decor), `diagram/src/doc_fields.rs`, `engine/cmd/sheet.rs` (`sheet.set`, `sheet.frames`, `sheet.revise`), `page_setup.rs` (ANSI A to E, ISO A0 to A2 presets) |
| Composed designations and cross references | `IdFormat` gains `{owner.tag}` and `{owner:type.tag}` parts (the nearest owner of a type) and `{page}`, `{zone}` and `{rung}` parts, so `=F1+A2-K3`, `CR-4-25` and `2003` compose; a `CrossReference` relationship kind (contact to coil, master to slave, off-sheet pair) whose views draw the partner's sheet and zone text on each end and navigate on click; `Registry` numbers within the owner scope | `vizcraft-model/src/identity.rs`, `vizcraft-notation` (`crossReference` check and the mirror compartment on coils), `engine/cmd/xref.rs` (`xref.list`, `xref.go`) |

### Analysis and reports

The swe report proposes a `vizcraft-analysis` crate for graph algorithms over the model. The
engineering packs add net and discipline analyses to it, each a command and a dock result.

| Analysis | What it answers | Command |
|---|---|---|
| Connectivity | The pins, terminals and off-sheet references on a net; a netlist in a KiCad-style text form and JSON | `net.list`, `net.export` |
| Electrical rule check | Unconnected pin, single-pin net, two drivers, floating input, duplicate designation, coil complement exceeded, missing or duplicate wire number, unpaired interrupted line, terminal used twice, mixed shape sets; each a pack rule with a fix where one exists, suppressible per object with a reason | pack rules, `validate.run` |
| Loop trace | From a primary element through transmitter and controller to the final element; the loop's cables, terminals and channel | `loop.trace` |
| Fluid power check | Every port connected or plugged, pressure path from pump to actuator to tank, relief after a fixed pump, rest position consistency, displacement-step reachability from the valve path tables | pack rules |
| Mobility and ratios | Kutzbach count of a kinematic diagram; gear ratio along a transmission path | `mech.mobility`, `mech.ratio` |
| Reliability | Minimal cut sets of a fault tree; series and parallel availability of a reliability block diagram; RPN recomputation | `rel.cutsets`, `rel.availability` |
| Lengths and quantities | Cable and pipe lengths through the drawing scale; bill of material grouped by part number with quantities; fixture unit sums | `table.list` with aggregate columns |
| Reports | Bill of material, wire list, cable schedule, terminal plan, device list, line list, equipment list, instrument index, valve list, I/O list, parts list; each a `table.list` or relationship list over the model, editable where the field is user-owned, exported to CSV and XLSX, and placeable as a sheet | Block 5 tables plus relationship and net tables |

## Workspace changes (synthesis)

### New page

The Block 6 gallery with three more audience chips (Electrical, Process, Mechanical) and the
templates above as cards with a drawn preview, the standard family shown as a badge (IEC, ANSI,
ISA, ISO 1219) and an estimate of the seed's drawing time as ConceptDraw shows on its samples.
Choosing a template asks two things once, remembered per project: the symbol standard and the
sheet standard. Everything else is the template's.

### Ribbon

| Tab | Change |
|---|---|
| Home (engineering preset) | As the swe report arranges it, plus a Wire and a Pipe tool beside Connector in the Insert group, Rotate and Mirror beside Align, and Net Highlight in the Check group |
| Notation tab (per pack) | Electrical: Symbols (one menu per palette group, the Variant menu), Wires (Wire, Junction, Number wires, Potentials, Net highlight), Devices (Cross references, Terminal strip, Cable), Sheet (Frame, Title block, Zones, Revise), Reports (BOM, Wire list, Terminal plan, Cable schedule, Device list), Check. Process: Symbols, Lines (Pipe, Signal, Line numbers, Off-page pair), Tags (Renumber, Loop, Instrument index), Sheet, Reports (Line list, Equipment list, Instrument index, Valve list, I/O list), Check. Fluid power: Symbols (Valve builder, Pump, Cylinder), Lines (Working, Pilot, Drain), Sequence (Displacement-step chart), Reports (Parts list), Check. Sheet and Mechanical: Frame, Title block, Revision table, Parts list and Balloons, Annotate (FCF, Datum, Weld, Texture), Analyze (Mobility, Ratio, Cut sets, Availability) |
| Design | Page Setup's Size menu gains ANSI A to E and ISO A0 to A2; a Sheet standard menu; the symbol grid module beside the grid settings |
| Model | The Analyze home of the swe report plus Nets and Cross references |
| Data | Reports reach the same tables through the Tables dock |

### Side panes and dock

- **Palette**: grouped by the pack's families (contacts, coils, protection, operator controls,
  motors, terminals; valves, pumps, cylinders, accessories; equipment, valves, instruments,
  lines), each glyph previewed in the page's variant; a search that matches symbol names, IEC
  60617 ids and ISO 14617 numbers, class letters and JIC abbreviations.
- **Model Tree**: the project tree in the pack's aspects (functions and locations as containers,
  devices under them), a Nets branch (every net with its number and pin count), a Sheets branch
  with thumbnails, a Cables branch with cores; dragging a device into a location re-tags it.
- **Inspector**: the designation shown as its composed parts (function, location, product,
  number) with the owner contributions greyed; symbol parameters (contact kind, poles, valve
  positions and ports, bubble system and location) as the top group so the symbol is edited
  here; net membership and wire number on a connector; cross references as a clickable list;
  sheet and title block fields on a page.
- **Dock**: Problems with electrical and fluid power rule results; Tables with the reports;
  Nets (the members of the highlighted net with a Go button); Analysis for the mobility, ratio
  and reliability results.

### Canvas

- The Wire tool starts from a free pin on click, draws orthogonally on the module grid, ends
  on a pin, a wire (junction dot) or empty board (a free end that the checks report); Escape
  returns to the pointer; the Pipe tool does the same with the line's kind and number.
- Dropping a device on a wire or a valve on a pipe splits the segment and glues it; deleting
  heals the line (Visio's P&ID behaviour, generalised to every net kind).
- The ladder viewpoint adds a Ladder tool (rails, rung spacing, first number, increment) and
  snaps devices onto rungs, which number their wires.
- Hover over a wire highlights its net across the page; click holds it; the status bar shows
  the number and pin count.
- R rotates, X and Y mirror, with the label upright; Tab inserts the legal next element along
  the wire direction as in the swe report.
- A cross reference text on a coil or an off-sheet arrow navigates to the partner on click
  and shows it in a tooltip on hover.
- Zones draw along the frame; the status bar shows the zone under the pointer; a shape's
  zone is in its Inspector.

### Agent and text interface

Every command above is an MCP tool through the generic command tool, and the packs are
resources agents can read. Two additions follow the research: a text form where elements and
nets are first class and the layout is derived (the shape of Schemdraw, SKiDL and WaveJSON), so
an agent can say "add a three-pole contactor K3 between rungs 12 and 14 and seal it in with its
own contact" and the ladder layout places it; and importers for KiCad netlists, WaveJSON and CSV
part, cable and I/O lists under the re-import contract, so agents bring existing engineering data
in rather than redrawing it. The Code pane of the swe report hosts the text form with a grammar
per viewpoint.

## Block 6E: electrical and mechanical engineering (synthesis, approved)

Block 6 (approved) covers software templates and packs; this is the sibling block for the two
disciplines here. It needs Block 5 merged (path glyphs, ports, variants, end labels, tables,
patterns) and shares three pieces of Block 6 that should be built once, there: the template
system and New gallery (`b6-templates`), the key and title block generator (`b6-topology`), and
the analysis crate (`b6-analysis`). Wave 1 of this block can start beside Block 6's wave 1,
since the substrate, sheets and builders branches touch none of those files; wave 2 waits for
`b6-templates` and `b6-topology` to merge.

Sequencing change for the owner to decide: the Block 7 industrial track's P&ID and PFD, lists and
loop pieces move here as the `process` pack, and the Block 7 electrical pack (planned for Cascade
7) becomes the `electrical` pack here, two cascades earlier. Block 7 keeps SFC, control modules,
cause and effect logic generation, Structured Text and PLCopen, and builds them on these packs.

| Branch (port) | Model | Scope | Own (new unless noted) | Starts |
|---|---|---|---|---|
| `b6e-substrate` (8071) | Opus | Nets and junctions, connector-to-connector glue, the Wire and Pipe tools, along-the-line labels and markers, symbol grid module, orientation with upright labels, rotate and mirror keys, net commands and the Nets dock tab | `model/src/net.rs`, `diagram/src/net.rs` and the glue hunk in `model.rs`, `notation/src/bind.rs` net pass, `engine/cmd/net.rs`, `canvas/interact/wire.rs`, `canvas/draw/nets.rs`, `dock/nets.rs`, `canvas/interact/keys.rs` rotate and mirror hunk | wave 1 |
| `b6e-sheets` (8072) | Opus | Sheet standards and presets, frames with zones, typed document properties, title block, revision table, parts list and balloons, zone and sheet fields, `{owner.tag}` and `{page}`, `{zone}`, `{rung}` identifier parts, the cross reference relationship and its views, the `sheet` pack | `diagram/src/sheet.rs`, `decor.rs` frame hunk, `doc_fields.rs`, `model/src/identity.rs` parts hunk, `engine/cmd/sheet.rs`, `engine/cmd/xref.rs`, `page_setup.rs` presets, `notation/packs/sheet.json`, `ribbon/notation/sheet.rs` | wave 1 |
| `b6e-builders` (8073) | Opus | Parametric glyph builders (contact, coil, bubble, valve, cylinder, pump, gate, feature control frame, weld, texture) behind `Glyph.builder`, rebuilt by the sync pass, previewed in the palette and edited from the Inspector | `notation/src/schema.rs` builder hunk, `stencils/src/builders/`, `panes/inspector.rs` symbol parameters group, `palette.rs` preview hunk | wave 1 |
| `b6e-electrical` (8074) | Opus | The `electrical` pack with its viewpoints, three symbol variants, 81346 and JIC and Y14.44 tag formats, the ladder tool, cross references on coils, terminal strips and cables, ERC rules, the reports, the templates | `notation/packs/electrical.json` and `tests/electrical/`, `canvas/interact/ladder.rs`, `engine/cmd/ladder.rs`, `ribbon/notation/electrical/`, `formats/src/netlist.rs` | wave 2 (after substrate, sheets, builders) |
| `b6e-process` (8075) | Opus | The `process` pack (PFD, P&ID, loop, HVAC, plumbing, isometric viewpoints), ISA and ISO 10628 variants, tag and line number formats with the parallel or serial switch, off-page pairs, split-and-heal on pipes, loop trace, the lists, DEXPI class mapping for the later Proteus export, the templates | `notation/packs/process.json` and tests, `engine/cmd/loop.rs`, `ribbon/notation/process/`, `formats/src/dexpi_map.rs` | wave 2 |
| `b6e-fluidpower` (8076) | Opus | The `fluidpower` pack over the valve, pump and cylinder builders, ISO 9461 and 5599 port sets, 1995 and 2012 codes, working, pilot and drain line kinds, the displacement-step chart as a table view, fluid rules, the hydraulic power unit pattern, the templates | `notation/packs/fluidpower.json` and tests, `engine/cmd/sequence.rs`, `ribbon/notation/fluidpower/`, the pattern in `crates/patterns` | wave 2 |
| `b6e-mechanical` (8077) | Opus | The `mechanical`, `annotation`, `reliability` and `idef0` packs, mobility, ratio, cut set and availability analyses, the FMEA table, the templates | `notation/packs/{mechanical,annotation,reliability,idef0}.json` and tests, `crates/analysis/src/{mech.rs, reliability.rs}`, `engine/cmd/{mech.rs, rel.rs}`, `ribbon/notation/mechanical/` | wave 2 |
| `b6e-ux` (8078) | Sonnet | Sheets navigator with thumbnails, Nets and Cables branches in the Model Tree, composed designation display in the Inspector, net highlight on hover, cross reference navigation, zone in the status bar, the palette search over standard ids | `panes/model_tree.rs` nets and sheets hunks, `panes/sheets.rs`, `panes/inspector.rs` designation hunk, `canvas/draw/highlights.rs` net hunk, `chrome.rs` status hunk, `palette.rs` search hunk | wave 2 |

Eight branches in two waves: three in wave 1 (substrate, sheets, builders; the shared files
follow the Block 5 rules, registries append-only, the main session merges), five in wave 2 once
all three are merged. Merge order: substrate, sheets, builders, then electrical, process,
fluidpower, mechanical, ux; snapshots regenerated after builders and after ux. Every pack ships
its `tests/<pack>/` cases for `vizcraft-cli validate`, a provenance note per glyph ("drawn from
the IEC 60617 description of S00123", "own design"), and a template preview snapshot. The main
session adds DEXPI (CC BY 4.0) and the KiCad netlist format to the Formats list in AGENTS.md and
docs/formats.md when the block starts.

Done when for the block: from the New page an engineer picks "Ladder diagram (JIC)", gets rails
and numbered rungs on an ANSI B sheet with a title block, presses W on a pushbutton pin and
draws a wire to a coil that gets its rung-derived number, drops a contact that cross-references
the coil on both symbols with sheet and zone, switches the project to IEC and watches every
symbol redraw and every tag recompose, sees the unconnected pin in Problems, and exports a wire
list and a BOM; picks "P&ID", composes `FIC-101` from the bubble's letters, draws a line whose
number composes from size, service and sequence, pairs an off-page connector and sees the
unpaired one reported; picks "Hydraulic circuit", builds a 4/3 valve from its parameters and sees
the open port reported; picks "Kinematic diagram" and reads the mobility count; `cargo xtask ci`
is green.

## Decisions needed before the block starts

| Question | Recommendation |
|---|---|
| Move the P&ID, loop and electrical packs ahead of the Block 7 tracks | Yes: they share the net and sheet substrate every later industrial pack needs, and the owner's stated goal is these two disciplines |
| Run Block 6E beside Block 6 or after it | Beside: wave 1 (substrate, sheets, builders) alongside Block 6's wave 1, wave 2 after `b6-templates` and `b6-topology` merge, so the template system, key and title block are built once |
| Nets as model objects or as a derived index only | Model objects: the wire number, colour and gauge are properties with identity, which is the lesson of every AutoCAD Electrical forum thread about floating wire numbers; the index is rebuilt by the sync pass |
| Parametric glyphs as code builders or as pack data | Code builders keyed by name from the pack, because the composition rules (valve positions, frame cells) are algorithms; the pack supplies parameters and defaults, and a fixed path glyph remains the simple case |
| Symbol standard chosen per project or per page | Per project with a per-page override through the Block 5 variant, and a rule that reports a sheet whose pages mix sets |
| Title block fields: ISO 7200 names or our own | ISO 7200 names and limits as the canonical keys, with the ASME extras as optional fields, so one document property set fills either frame |
| Glyph provenance | Mandatory per glyph in the pack (a `source` string naming the standard clause or "own design"), checked by `cargo xtask assets` like image files, so the clean-room rule is auditable |
| Licence posture for reference material | Draw from written descriptions and public domain US government documents (MIL-STD-806B, MIL-STD-100G, NASA GSFC X-673-64-1F, FIPS 183); use Wikimedia Commons, QElectroTech and KiCad files only to check geometry, never as artwork, since CC BY-SA would make a pack share-alike |
| Ladder and function block programming (IEC 61131-3) in this block or Block 7 | Block 7, on this block's rails and rung substrate; this block draws device ladders, Block 7 draws programs and generates Structured Text |

## Trademarks

Microsoft and Visio are trademarks of the Microsoft group of companies; Autodesk, AutoCAD, AutoCAD
Electrical and Plant 3D of Autodesk, Inc.; SOLIDWORKS of Dassault Systemes; EPLAN of EPLAN GmbH &
Co. KG; E3.series of Zuken; AVEVA of AVEVA Group Limited; Smart P&ID of Hexagon AB; Automation
Studio of Famic Technologies Inc.; FluidSIM of Festo SE & Co. KG; ConceptDraw of CS Odessa; SysML
and OMG of Object Management Group, Inc.; Wikimedia of the Wikimedia Foundation, Inc.; Claude of
Anthropic, PBC (Opus and Sonnet are Claude models). The names are used only to identify products
and formats; VizCraft is not affiliated with or endorsed by their owners.
