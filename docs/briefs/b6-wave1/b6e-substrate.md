# `b6e-substrate` (engine half), Block 6 and 6E wave 1

Opus, medium effort. Port 8071. Worktree `.claude/worktrees/b6e-substrate`, branch `b6e-substrate` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that added this
file). `CARGO_TARGET_DIR=target-b6e-substrate`. Commit prefix `B6E:`. Backend only: touch no file under
`crates/ui-egui` or `apps/vizcraft`.

## Resolutions (these override the draft below where they differ)

The cross-branch contracts are in docs/cascade-plan.md under "Block 6 and 6E wave 1 contracts".
For this branch:

- Add `page.setModule {page?, module: "none" | "iso2_5mm" | "inch0_1" | number of page units}` (one undo step); `b6-templates` calls it. Units are page units, 100 per inch.
- `Shape.orient` is the name every branch uses.
- `arrange.flip` stays positional and does not toggle `orient.mirror`. Only point glue to the same pin joins nets; dynamic glue to one shape does not. Terminal pass-through (`Port.netGroup`) is left to `b6e-electrical`. Markers repeat every 12 line widths. Glyph sizes in modules are deferred.
- Fix `Model::is_empty` so it counts nets and patterns, with a regression test.
- A shape with the default `orient` must produce bit-identical output: add a test over the sample document.

## Draft brief

**(1) Brief**

`b6e-substrate` (Opus, port 8071, worktree `.claude/worktrees/b6e-substrate`, branch `b6e-substrate` from `claude/engineering-diagramming-standards-305d6b` at 9eecade, `CARGO_TARGET_DIR=target-b6e-substrate`, prefix `B6E:`). Do not touch `crates/ui-egui` or `apps/vizcraft`.

**Goal:** the engine half of the net and orientation substrate. That covers nets as model objects, wires that join at junctions, the net pass in the sync step, line labels and line markers in the exports, the symbol grid module, glyph orientation with labels kept upright, and the `net.*`, `wire.*`, `shape.rotate {turns}` and `shape.mirror` commands. Everything is reachable by command id, and old documents open unchanged.

**Nets.** New `crates/model/src/net.rs`:
- `Net {id, kind, name, number, properties: BTreeMap<String, PropValue>, label}`.
- `NetKind` is `wire` (default), `potential`, `pipe`, `fluid` or `signal`. `NetLabel` is `once` (default), `each` or `off`.
- Well-known property keys: `colour`, `gauge`, `size`, `service`, `class`.
- `Net::named()` is true when the name or number is not empty.
- `Model.nets: Vec<Net>` uses serde defaults, is skipped when empty, is capped (`MAX_NETS` = 100 000) and is sanitised in `Model::sanitize` (empty or duplicate ids dropped, text capped).
- `Model::contains_id` covers nets, because nets share the document's id space. `next_id("n")` hands out net ids.
- `Model::is_empty` must also count nets and patterns. Otherwise `native::save` silently drops the model section of a document that only has wires.

**Membership.** `Connector.net: Option<String>` in `diagram/src/model.rs`, stored like `relationship`: capped to `MAX_ID` and cleared by `views::sanitize_refs` when it names no net. A "segment" is a connector with `net` set, or a connector whose `kind` names a `ConnectorType` with a `net` kind.

**Junctions.** The research says "a connector end may glue to another connector, which splits it". That is built on the existing glue rather than a new `End` variant:
- A junction is an `Ellipse` shape with stencil `vizcraft.junction` and one connection point at (0.5, 0.5). It is recognised by `diagram::net::is_junction`, the same way groups are recognised by `vizcraft.group`.
- `net::junction(id, at, dot, line_width)` sizes it at 3 times the line width. It is filled with the line colour when `dot` is true. For pipe tees (`dot` false) it is transparent with zero line width.
- `net::split_at(page, connector, at, new_id, junction_id)` projects `at` onto the route and splits the connector into two. The bends before the cut stay on the first half and the rest go to a new connector that copies the style, kind and net. Both halves are point-glued to the junction.
- Ends glued to the junction resolve through the normal `route::resolve_ends`, so moving the junction moves every wire on it with no new routing code.
- SVG, PDF and draw.io already draw the junction because it is an ordinary shape.
- Two `autoroute.rs` hunks:
  - Junctions are never obstacles.
  - `routing_policy::routes` returns `c.avoid` for net segments. Segments keep their drawn bends, as in every ECAD tool, even on a page whose policy is "always avoid".

**Net pass.** New `notation/src/bind.rs` step `nets(doc, before, cat)`. It is called first in `sync`, before the early return for an empty model, and does nothing when the document has no segment and no net. In order:
1. A segment whose kind declares a net but which has none gets a fresh `Net`. Its kind, colour and gauge come from the `ConnectorType`.
2. Segments are joined into components only through glue: the same junction, or the same pin (shape plus connection point index). Geometry never joins them, so a crossing never merges two nets.
3. In each component:
   - If it holds one net id, every segment takes it.
   - If it holds several ids and at most one is named, they fold into the named net, or into the oldest when none is named. Properties are unioned and the first value wins.
   - If two named ids meet, nothing is merged. A live issue `net.conflict` is raised through `refresh_issues`, carrying a `Fix` that runs `net.merge {nets, force: true}`.
4. An unnamed net spread over components that are no longer connected is split. The largest component keeps the id; the others get fresh nets with the same kind and properties. A named net may span components and pages: that is how potentials and off-page continuation work.
5. Unnamed nets with no segments are removed; named ones are kept.
6. Junctions are tidied. A junction left with no segments is deleted. With one segment it is deleted and that end is freed where it was. With two it is healed by `net::join` (the second connector is merged into the first and its bends concatenated).

**Index.** `diagram/src/net.rs`:
- `NetIndex::build(&Document)` is linear in the document. It holds `nets: BTreeMap<id, NetEntry {segments, pins, junctions, pages}>`, `of_connector` and `of_pin: HashMap<(shape, point), id>`.
- `Session.nets: NetIndex` is rebuilt wherever `refresh_issues` runs (after every change, undo, redo and open). This keeps `bind::sync`'s signature unchanged for the other branches.

**Line labels and markers.**
- `net::label_spots(page, c, text, mode) -> Vec<LabelSpot {pos, angle, anchor}>`. The text is the net's number, else its name. With `once` it goes on the longest segment of the net; with `each` on every segment. Text sits beside the line, upright, reading from the bottom on vertical runs (IEC 61082).
- New `diagram/src/line_marks.rs`: `Connector.marker: LineMarker` with the values `none`, `doubleSlash` (ISA pneumatic), `cross` (capillary), `l` (hydraulic), `circle` (data link) and `wave` (electromagnetic). `marks(points, marker, line_width) -> Path` repeats the mark every 12 line widths. It is one table, like `arrows::outline`, so the canvas will later draw exactly what the exports draw.
- `ConnectorType` gains `net: Option<String>` (a net kind) and `marker: String`, and `stencils/src/connectors.rs` applies them.
- New `formats/src/svg/nets.rs` draws the labels and markers. It is one call line in `svg.rs`; PDF follows for free because it is built from the SVG.

**Module.**
- `View.module: Module` in `model/src/types.rs`, with the values `none` (default), `iso2_5mm` (9.8425 units), `inch0_1` (10 units) and `custom {units}`.
- `Pack.module` and `Variant.module` hold the same names, and the pack default for engineering packs is `iso2_5mm`.
- `variant.set` copies the variant's module onto the page; `viewpoint` set copies the pack's.
- `snap::module_step(page) -> Option<f64>` and `snap::Targets.module`. When a module is set, `snap_point` and `snap_rect` put that step ahead of the zoom grid.
- `validate::check` reports every port of a glyph with a typed element whose offset from the glyph's first port is not a whole number of modules (tolerance 0.05 M), checked against each module the pack and its variants name.
- `place_shape` on a page with a module snaps the shape's first port onto the grid.

**Orientation.** `Shape.orient: Orient {turns: u8 0..=3, mirror: bool}` in `shape_fields.rs`.
- Stored in canonical dihedral form: mirroring across Y is stored as mirror plus two turns, so equal orientations compare equal. Skipped when default; `turns` is reduced mod 4 on load.
- `Shape::transform()` becomes: mirror and turn about the frame centre, then `angle_deg`, then translate. Outline, detail, hit testing, connection points and `connection_direction` all follow with no further change.
- New `Shape::text_frame() -> (world centre, angle)` uses `angle_deg + block angle` only, so text never turns or mirrors. `svg.rs` and `svg/compartments.rs` switch to it; the canvas already places text this way.

**Commands.** New `engine/src/cmd/net.rs` and `cmd/wire.rs`, each a `CommandSpec` with params doc and tests:
- `net.list {page?, kind?}` returns `[{id, kind, name, number, segments, pins, pages, properties}]`.
- `net.of {connector? | shape+point? | junction?}` returns the net.
- `net.members {net}` returns `{segments, pins, junctions}`.
- `net.set {net, name?, number?, kind?, label?, properties?: {k: value or null}}` refuses a name that another net already holds.
- `net.merge {nets: [a, b, ...], into?, force?}` refuses two named nets without `force`.
- `net.split {net, at: connector}` gives the component holding that connector a new net.
- `net.renumber {nets? | page?, start, step?, prefix?, scheme: "sequential" or "page"}` gives `2003` for page 2, line 3 from the y band. It skips named nets unless `force`.
- `net.assign {ids, net? or kind?}` turns connectors into segments, or `net: null` releases them.
- `shape.rotate` gains `turns?: int` (relative, clockwise; one of `angle` and `turns` is required). `shape.mirror {ids?, axis: "x" or "y"}` is new. Both check `Lock::Rotation`, and the R, X and Y keys will call them.
- `wire.draw {from, to, points?, kind?, net?}`:
  - Each end is `{shape, point}`, `{connector, x, y}` (which splits the connector and makes a junction), `{junction}` or `{x, y}`.
  - `points` snap to the module and make an orthogonal run.
  - With no `net` it joins the net at either end or creates one of `kind` (default `wire`; a `pipe` kind is drawn without a dot).
  - Returns `{id, net, junctions}`.
- `wire.splice {shape, connector?}` is the placement case. The device's two outermost ports are projected onto the wire, the stretch between them is cut away, the halves are glued to the ports and both keep the net. It refuses when the projections fall on different segments.
- `wire.heal {shape}` removes a device with exactly one segment on each of two pins and joins the two segments. It refuses when they are on two named nets.
- `shape.delete` and `cmd::delete_items` take `heal?: bool`, true by default only for shapes with exactly two segmented pins.
- Each of these commands is one undo step.

**Compatibility.**
- Every new field has serde defaults, is lenient, sanitised and skipped when default, so old `.vizcraft` files load and re-save byte-identical.
- draw.io:
  - Orientation is written as `rotation` = angle + 90 × turns plus `flipH`, with a `vizcraftOrient` attribute that is read back first. A foreign `flipH`/`flipV` reads into `orient`.
  - `vizcraftNet` and `vizcraftMarker` are written on edges.
  - Nets go as JSON in a `vizcraftNets` attribute on the page model and are restored on import. Other draw.io readers simply ignore them.
- Mermaid and CSV drop nets.

**Done when:**
- Model and diagram unit tests:
  - Two wires crossing with no junction stay two nets.
  - A wire drawn to the middle of a wire splits it, adds one junction, keeps one net, and moving the junction moves all three ends.
  - Deleting the branch heals the dot away and leaves one connector.
  - Two named nets joined by a wire are not merged and raise `net.conflict`, and `net.merge` refuses without `force`.
  - Deleting a middle segment of an unnamed net splits it in two.
  - `wire.splice` and then `wire.heal` give back the original route.
  - `Orient` is round-tripped through every one of the 8 states, and pins, outline and hit area agree after `turns: 1` and mirror.
  - `text_frame` has angle 0 after a turn.
  - A pack whose port is off the module fails `validate::check`, and a variant with `inch0_1` changes `View.module`.
  - `snap_point` lands on 9.8425-unit steps.
- `native_tests.rs`: a round trip of nets, `Connector.net`, `marker`, `orient` and `View.module`; a pre-6E file is byte-identical after load and save; junk values sanitise.
- draw.io tests: a wire, a junction and an oriented glyph keep orientation, net and marker through export and import.
- SVG goldens: a junction dot, a vertical wire number, double-slash and cross markers, and an upright label on a turned shape.
- Engine tests for every command, each one undo step, and `net.list` over the CLI.
- `hostile_params_never_panic` green with junk ends, turns and nets.
- `cargo xtask ci` is green.

**(2) Own** (new files marked "new")
- `crates/model/src/net.rs` (new)
- `crates/diagram/src/net.rs` (new), `crates/diagram/src/line_marks.rs` (new), `crates/diagram/src/net_tests.rs` (new)
- `crates/engine/src/cmd/net.rs` (new), `crates/engine/src/cmd/wire.rs` (new)
- `crates/formats/src/svg/nets.rs` (new)
- `crates/diagram/src/shape_fields.rs`: `Orient`
- `crates/diagram/src/snap.rs`: `module_step` and `Targets.module`
- `crates/diagram/src/routing_policy.rs`: the segment exemption in `routes`
- `crates/diagram/src/autoroute.rs`: junctions are not obstacles
- `crates/stencils/src/connectors.rs`: net kind and marker applied
- `crates/notation/src/validate.rs`: the module port check
- `crates/engine/src/cmd/shape.rs`: `rotate {turns}`, `mirror`, the `delete` heal param
- `crates/engine/src/cmd/variant.rs`: module copy
- `crates/formats/src/drawio/{read.rs, write.rs}`: orient, net and marker attributes
- `crates/formats/src/svg/compartments.rs`: switch to `text_frame`

**(3) Shared** (the exact hunk this branch adds)
- `crates/model/src/types.rs`: `Model.nets`; `is_empty` and `contains_id` include nets (and patterns); `View.module`. `b6e-sheets` may add page or sheet fields nearby.
- `crates/model/src/lib.rs`: `pub mod net;` and re-exports `Net`, `NetKind`, `NetLabel`, `Module`.
- `crates/diagram/src/model.rs`:
  - `Connector.net` and `Connector.marker`, with sanitising.
  - `Shape.orient`, with `transform()` composing it and the new `text_frame()`.
  - `views::sanitize_refs` clears a dangling `Connector.net`.
- `crates/diagram/src/lib.rs`: `pub mod net; pub mod line_marks;`.
- `crates/notation/src/schema.rs`: `Pack.module`, `Variant.module`, `ConnectorType.net` and `ConnectorType.marker`. `b6e-builders` adds `Glyph.builder` and `Glyph.params` and `b6-templates` adds `Pack.templates` in the same file; field-level hunks only.
- `crates/notation/src/bind.rs`: one `nets(doc, before, cat)` call at the top of `sync`, with its body in a new `bind_nets.rs` so the builders branch's rebuild step does not collide.
- `crates/engine/src/session.rs`: `pub nets: NetIndex`, rebuilt in `refresh_issues` together with the `net.conflict` issues.
- `crates/engine/src/cmd/mod.rs`: `pub mod net; pub mod wire;`, two `specs()` lines, and the `heal` arm in `delete_items`.
- `crates/engine/src/cmd/model.rs`: `place_shape`'s module snap.
- `crates/formats/src/svg.rs`: one `nets::draw` call line and the `text_frame` switch in `shape_text_svg`.
- `crates/formats/src/native_tests.rs`: appended tests.
- `crates/engine/src/tests.rs`: hostile params cases.
- `crates/engine/src/cmd/describe.rs`: may list a net on a connector (optional).

`b6-analysis` should read `Session.nets` or `NetIndex::build`, not rebuild connectivity. `b6e-sheets` owns zones, `{page}` and `{rung}` identifier parts; `net.renumber` is written so its `scheme: "page"` can later call `zone_of`.

**(4) What the frontend half needs from this branch**
- **Wire and Pipe tools** (`canvas/interact/wire.rs`): `snap::module_step`, `net::split_at` preview geometry and `hit::connector_at`. They commit through `wire.draw`; dropping a device on a wire calls `wire.splice`, and deleting one calls `shape.delete` with the default heal.
- **R, X, Y, W and P bare-letter keys** in `keys.rs` and its shortcut test: they map to `shape.rotate {turns: 1}`, `shape.mirror {axis}`, `ui.wire` and `ui.pipe`, and fire only while the canvas has focus and no text is being edited.
- **Canvas drawing** (`draw/nets.rs`): the same `net::label_spots` and `line_marks::marks` the exports use, plus net highlight through `Session.nets` and `net.members`.
- **Grid** (`drag.rs`): snap targets read `Targets.module`.
- **Nets dock tab and Model Tree branch:** `net.list` and `net.members`, with `net.set` for editing in place.
- **Inspector:** net membership and number on a connector through `net.of` and `net.set`.
- **Text:** canvas text already places the text-block centre through `transform()` with the plain angle; check pictures and text-block handles under mirroring.

**(5) Risks and open questions**
- **`shape.rotate` already exists** with an absolute `angle`. Adding `turns` keeps old callers working, but two rotation concepts now coexist. `arrange.flip` still mirrors positions only and will not toggle `orient.mirror`. Decide whether it should for glyphs.
- **Changing `Shape::transform()` affects everything**: autoroute obstacles, compartments, `arrange` group scaling, ink and pictures. A shape whose `orient` is the default must produce bit-identical output; add a test over the sample document.
- **Folding nets inside the sync pass** means geometry edits (junction heal) happen in sync, while selection may still hold a removed id. Call `prune_selection` after the pass and keep the pass idempotent; a second run must change nothing.
- **Exempting segments from page avoidance** changes routing only for connectors that have a net. Old documents have none, but `net.assign` on existing connectors stops their rerouting. Say so in the params doc.
- **`Model::is_empty` ignoring patterns today** is an existing bug: patterns-only models are not saved. Fixing it changes four `native_tests` assertions only if they build such a model.
- **Open questions:**
  - Should a terminal or off-page reference carry a net across its pins (a pass-through declared on the `ElementType`)? Proposed: a `Port.net_group` key, left to `b6e-electrical`.
  - Should two wires dynamically glued to one non-junction shape share a net? Proposed: no; only point glue to the same pin joins.
  - Should markers sit at fixed spacing or count per segment? ISA does not fix it; proposed 12 line widths.
  - Does the 1.6 percent difference between 2.5 mm and 0.1 inch mean glyph sizes should be declared in modules (`Glyph.modules`)? Deferred to `b6e-builders` and `b6e-electrical`.

**Critical files for implementation**
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/diagram/src/model.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/notation/src/bind.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/model/src/types.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/diagram/src/route.rs
- /Users/pranav/Workspace/crafts/VizCraft/.claude/worktrees/engineering-diagramming-standards-305d6b/crates/engine/src/cmd/shape.rs
