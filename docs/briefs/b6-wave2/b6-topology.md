# `b6-topology` (engine half), Block 6 wave 2

Opus, medium effort. Port 8067. Worktree `.claude/worktrees/b6-topology`, branch `b6-topology` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (now; the session names
the commit). `CARGO_TARGET_DIR=target-b6-topology CARGO_INCREMENTAL=0`. Commit prefix `B6:`.

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

- Runs in parallel with `b6-uml`. Both add `Check` variants, `bind.rs` passes, `schema.rs`
  fields, `catalog.rs` BUILTIN lines and `mermaid.rs` arms; append only, the integration session
  merges. Your owner-sync pass is `nesting::owners`, called by one line after the existing steps.
- You own `connector.setStep`, `connector.renumberSteps` and `cmd/step.rs`; `b6-c4` (wave 4)
  extends them rather than recreating them.
- The template hook test (`hooks_run_when_registered_and_are_skipped_otherwise`) now expects
  `key.insert` in `applied`; update it as drafted.
- Add a one-line `template::check` warning when a template sets `titleBlock` without a sheet.
- Key freshness stays explicit (`key.refresh`); no auto-refresh in `session.rs`.

## Draft brief

**(1) Brief**

**Goal.** Ship three packs: generic cloud topology, network topology (physical and logical views over one model) and data flow (two drawing variants). Add nesting rules, per-type container borders, numbered step badges, the generated key behind the existing `key.insert` hook, Mermaid `architecture-beta` read and write, and six templates. Everything runs by command id and is checked over the CLI.

**Gaps found in the code.**
- An element type with `container: true` (schema.rs:355) is drawn as a plain glyph today. Only `model.rs::check_container` reads the flag.
- `bind::sync` never relates a shape's `parent` to the element's `owner`.
- No check exists for empty names, or for a relationship end that sits on no port.
- `crates/mermaid` refuses `architecture-beta` (`OTHER_MERMAID_TYPES`), and there is no Mermaid writer at all.
- `sheet.titleBlock` fails on a page with no sheet.
- `layout.full` would move a key group like any other shape.

**Schema** (in new `crates/notation/src/nesting.rs`, following the template.rs pattern):
- `ElementType.allowed_parents: Vec<String>` (`allowedParents`, skipped when empty). Entries are qualified type ids, `*`, or `top` (no owner allowed). Supertypes count, through `infer::Facts::lists`. An empty list means the type does not nest by drawing, so every existing pack behaves as before.
- `Glyph.container: Option<nesting::ContainerLook {dash, lineWidth, corner, heading: band|line|plain, headingPosition, band, margin}>`. A glyph with it becomes a real container master: `Master.container` in `vizcraft-stencils`, so `instantiate` sets `Shape.container`, `behavior.container` and the `Dash`, and `redraw` reapplies the look but keeps margin, `auto_fit` and lock. This gives every type its own border.
- `nesting::check_pack` (one loader line) checks that:
  - own-pack `allowedParents` ids are container types;
  - the look's names, colours and numbers are valid;
  - no variant maps a container type, because `variant.rs` resizes shapes to the master's size.

**Checks.** Three new `Check` variants, appended before `Unknown`, all run by `nesting::check(doc, cat, pack, rule, out)`:
- `NestingAllowed {types, drawn}`: the model owner's type is not listed, or there is no owner and `top` is not listed. With `drawn`, it also reports a bound shape whose nearest bound container shape has a type that is not allowed.
- `Labelled {types}`: elements or relationships with an empty name.
- `EndsOnPorts {relationship}`: an end with no `source_port` or `target_port` on a type that declares ports.

**Sync step 9.** New bind.rs step, `nesting::owners(doc, before, cat)`. For a bound shape whose `parent` changed (or that is newly bound), and whose type declares `allowedParents`:
- inside a container shape whose element type is allowed: set `owner` to that element through `Model::set_owner` (cycles refused);
- moved out to the top: clear the owner it had from that container;
- inside a container whose type is not allowed: leave the owner alone, and the rule reports it.

**Packs.** No vendor word appears in any id, label, keyword or description.

`cloud.json`
- Containers: Cloud, Account, Region, Zone, VirtualNetwork, PublicSubnet and PrivateSubnet (both extend Subnet), SecurityGroup, Cluster, Namespace, Group.
- Nodes, as original path glyphs with `source: "own design"`: Compute, Container, Function, Database, Queue, Bucket, LoadBalancer, Gateway, User, Internet, ExternalService, Junction.
- Relationship types: Request (solid, with a `protocol` property), Event (dashed), Replication (dotted), Peering (no ends).
- Nesting, as `allowedParents`:
  - a subnet sits in a VirtualNetwork;
  - a VirtualNetwork sits in a Region or an Account;
  - a Zone sits in a Region;
  - a Namespace sits in a Cluster;
  - managed services (Function, Queue, Bucket) sit in a Region, the Cloud or `top`;
  - User, Internet and ExternalService sit at `top` or in a Group.
- Rules: `cloud.nesting` (drawn true), `cloud.nodeLabelled`, `cloud.lineLabelled`, `cloud.ingressReachesService` (`requiredPath` Gateway to Container), `viewpointType`.
- Viewpoints: `cloud.Topology`, `cloud.Cluster`.

`network.json`
- Physical: Site and Rack are containers. The devices all extend `network.Device` with `inOut` ports: Router, Switch (`layer` l2 or l3), Firewall (ports outside, inside, dmz, mgmt), AccessPoint, Server, Storage, Workstation, PatchPanel. Their `allowedParents` are `[network.Rack, network.Site, top]`.
- Logical: Zone (container, `trust` enum) owns Subnet nodes (`cidr` required, `vlan` integer, `gateway`). AddressTable is a list compartment with a `TableQuery` over Subnet.
- Relationship types: `Cable` (port to port: `medium`, `speed`), `Attached` (device to subnet: `address`, `vlan`), `Routes` (router or firewall to subnet).
- Viewpoints: `network.Physical`, `network.Logical`.
- One model: devices are drawn on both pages with `element.show`. Their owner is the rack. On the logical page they may sit inside a Zone shape without being re-owned, because Zone is not in their `allowedParents`.
- Rules: `cableEndsOnPorts`, `subnetGateway` (`minIncoming` of Routes ≥ 1), `vlanRange` (two `propertyCompare` rules, 1 to 4094), `nesting` (drawn false), `labelled`.

`dfd.json`
- Types: ExternalEntity, Process (`dfd.process` scope, auto-numbered), DataStore, TrustBoundary (container with a dashed red look, plain heading), and `dfd.Flow` (labelled arrow).
- Drawing: the default drawing is Yourdon and DeMarco (circle process, open-ended store). `variants: [dfd.yourdon (empty maps), dfd.ganeSarson]`; Gane-Sarson is a rounded process with an id band, a store with an id cell and a shadowed entity.
- Connections: Flow from Process to anything; from entity or store only to a Process.
- Rules: `illegalFlow`, `processHasInput` and `processHasOutput` (`minIncoming` and `minOutgoing`), `storeUsed`, `flowLabelled`, `duplicateProcess`, `viewpointType`. Elements nest by `allowedParents: [dfd.TrustBoundary, top]`.
- Viewpoints: `dfd.Context`, `dfd.Level`, `dfd.Threat`.
- STRIDE stays in Block 7.

**Step badges.** The research's "Cloud substrate" row puts `connector.setStep` here, but its branch table gives `cmd/step.rs` to `b6-c4` (wave 4). `cloud.topology` needs numbered steps, so this branch builds the field and the two commands, and b6-c4 later adds the steps-contiguous rule.
- `Connector.step: u16` (serde default 0 means none, skipped when 0, clamped to 999).
- `vizcraft_diagram::step_badge::badge(&Connector, &[Point]) -> Option<Badge {center, radius, text}>`, at a quarter of the way along the route so it does not cover the middle label. SVG draws it through new `formats/src/svg/step_badges.rs`; PNG and PDF follow because they render from the SVG.

**Key.** New `vizcraft_diagram::key`:
- `KEY_EXT = "vizcraft.key"`;
- `KeyRow {label, sample: Sample::{Glyph(master), Line(kind), Step(n)}}`;
- `key::block(heading, &[KeyRow]) -> (title_block::Block, Vec<SampleCell>)`: a heading slot, then per row a 20 mm sample cell and a 50 mm label, at most 40 rows plus an "and N more" row;
- `key::is_key(&Shape)`.

`engine/src/cmd/key.rs` builds the rows from the active page:
- the bound element types in first-seen order, each sample drawn from `cat().master_for_type_in(type, variant)` (container types show a small sample with their border);
- the connector kinds, labelled from the connector type, else from the relationship type;
- step rows `n  label`.

It draws them with `title_block::shapes(&block, origin, sheet::WORLD_PER_MM, id)`, plus master samples and line samples. A line sample is a connector of that kind glued between two transparent unbound anchor shapes, so it moves with the group. Everything is grouped through `arrange::group`, and every member carries `KEY_EXT`. Nothing is bound, so the model rules ignore it. `drawing.rs` skips `KEY_EXT` items in `solid`, `unglued` and `crossing`, and the layout commands leave key groups where they are.

**Mermaid.**
- New `crates/mermaid/src/architecture.rs`, parsing to `ArchitectureDiagram`. It covers only the statements the public mermaid.js.org architecture page documents: `group`, `service`, `junction`, `in`, edges with L, R, T and B side letters and `--`, `-->`, `<--`, `<-->`, and the `{group}` edge modifier. The same caps as the other grammars apply; anything else becomes a warning.
- New `crates/formats/src/mermaid/architecture.rs`:
  - `build` (import): icons map as database to Database, disk to Bucket, internet to Internet, server to Compute and cloud to Cloud; unknown or icon-pack names become Compute with `ext mermaid.icon` and a warning. Groups become cloud container shapes with their members' `parent` set, and `csv::TYPE_KEY` is set so `file.import {merge: true}` makes typed elements. Sides glue to the connection point on that side. A `%% type: <id> <type>` comment restores types the five icons cannot carry.
  - `write(doc, page) -> Result<(String, Vec<String>), FormatError>`: stable ids from element tags or ids; sides read back from the glued point's position. Names and steps the grammar cannot hold come back as warnings.

**Commands:**
- `key.insert {page?, x?, y?, title?: "Key", exclude?: [type or kind ids], steps?: true}` returns `{page, id|null, rows: [{label, kind: type|line|step, ref}], truncated}`. It replaces the page's key, keeping its corner unless `x` and `y` are given. The default position is 40 units right of the drawing's bounds. A page with nothing to explain returns `id: null` and is not an error, so the template hook never fails.
- `key.refresh {page?, all?}` returns `{pages, rows}`.
- `key.remove {page?}` returns `{removed}`.
- `key.get {page?}` returns `{id, rows, exclude, x, y}`.
- `connector.setStep {id: connector or relationship id, step: 1..999|null}` returns `{id, step}`.
- `connector.renumberSteps {page?}` returns `{renumbered}`: 1 to n in the current order, ties broken by page order.
- `topology.toMermaid {page?}` returns `{text, warnings}`. It has no `path` param, so the hostile test cannot write files; b6-code later wraps it as `code.write`.
- Reading `architecture-beta` needs no new command; it goes through `file.import {format: "mermaid"}`.

**Templates.**
- `cloud.topology`: seeds use `element.create {type, name, x, y}` with `as`, then `container.addMember {container: "${vnet.shape}", ids: [...]}` (sync step 9 sets the owners), then relationships and `connector.setStep`; layout `{primitive: "nested", direction: "LR"}`; `key: true`; codeForms `mermaid.architecture`.
- `cloud.cluster`.
- `network.logical`: layered TB, with an AddressTable.
- `network.physical`: nested racks in a site, cables glued to ports with `connector.glue {point}`, `sheet: "iso.a3.landscape"`, `titleBlock: true`.
- `dfd.context`: radial.
- `dfd.threat`: layered LR, one boundary.
- A template that sets `titleBlock` must also set a sheet.

**Done when:**
- Pack cases pass under `crates/notation/packs/tests/`:
  - `cloud/`: subnet_outside_network, unlabelled_node_and_line, tidy_topology (expect []);
  - `network/`: cable_without_ports, subnet_without_gateway, vlan_out_of_range, device_in_zone_on_logical_page (expect []);
  - `dfd/`: entity_to_store, black_hole_and_miracle, unlabelled_flow, gane_sarson_tidy (expect []);
  - `drawing/key_is_not_checked.json` (expect []).
- `vizcraft-cli validate crates/notation/packs/tests/{cloud,network,dfd}` exits 0.
- Unit tests:
  - `nesting::check_pack` rejects bad `allowedParents`, an invalid look, and a variant that maps a container type;
  - sync step 9 sets, keeps and clears owners, and leaves flowchart documents byte-identical;
  - drawing a device inside a Zone keeps its rack owner;
  - a test reads `cloud.json`, `network.json` and `dfd.json` and finds none of aws, amazon, azure, google, gcp, oracle, ibm, alibaba, kubernetes, k8s, docker, vpc, ec2, s3.
- Every new template builds through `template.new` with no error-level `validate.run` issue and one undo step. `cloud.topology` has steps 1 to 5, a key, every node inside its container, and an `analyze.spof` result naming the load balancer.
- `template_tests::hooks_run_when_registered_and_are_skipped_otherwise` now expects `key.insert` in `applied`.
- Key tests:
  - rows match the types and kinds on the page;
  - insert followed by refresh is idempotent;
  - `exclude` works;
  - moving the group carries the line samples;
  - `layout.full` leaves the key alone.
- Goldens under `crates/engine/src/cmd/key/goldens/` (cloud-topology.svg and dfd-gane-sarson.svg, from `template.preview`, with badges and key) and `crates/formats/src/mermaid/architecture/goldens/*.mmd`, rewritten with `VIZCRAFT_BLESS=1`.
- Mermaid tests: read then write then read keeps ids, types, groups, sides and arrows; hostile text (100k groups, nesting deeper than `MAX_DEPTH`, a cyclic `in`) returns an error and never panics.
- `Connector.step` round-trips in `native_tests.rs`, and an old file loads with step 0.
- `hostile_params_never_panic` stays green, with junk rows for `key.*`, `connector.setStep` (step -1, 1e9, `{}`) and `topology.toMermaid`.
- `cargo xtask ci` is green.

**(2) Own:**
- `crates/notation/packs/{cloud,network,dfd}.json`
- `crates/notation/packs/tests/{cloud,network,dfd}/*.json`
- `crates/notation/packs/tests/drawing/key_is_not_checked.json`
- `crates/notation/src/nesting.rs`, `crates/notation/src/nesting_tests.rs`
- `crates/diagram/src/key.rs`, `crates/diagram/src/step_badge.rs`
- `crates/mermaid/src/architecture.rs`
- `crates/formats/src/mermaid/architecture.rs` with its `tests.rs` and `goldens/`
- `crates/formats/src/svg/step_badges.rs`
- `crates/engine/src/cmd/key.rs` with `key/tests.rs` and `key/goldens/`
- `crates/engine/src/cmd/step.rs`
- `crates/engine/src/cmd/topology.rs` and `crates/engine/src/cmd/topology_tests.rs`

**(3) Shared, with the exact hunk this branch adds:**
- `notation/src/schema.rs`: `ElementType.allowed_parents`, `Glyph.container`, and three `Check` variants before `Unknown`.
- `notation/src/rules.rs`: one match arm, `Check::NestingAllowed{..} | Check::Labelled{..} | Check::EndsOnPorts{..} => crate::nesting::check(self.doc, self.cat, pack, rule, out)`.
- `notation/src/loader.rs`: in `finish`, `problems.extend(crate::nesting::check_pack(&pack));`.
- `notation/src/bind.rs`: step 9 call and its doc lines.
- `notation/src/drawing.rs`: the `key::is_key` filter in three places.
- `notation/src/lib.rs`: `pub mod nesting;`.
- `notation/src/catalog.rs`: three `BUILTIN` lines, plus the expected ids and scope count in `builtin_packs_load_with_every_section`.
- `title_block.rs`: no hunk; this branch only reuses `Block`, `Slot` and `shapes`.
- `diagram/src/model.rs`: `Connector.step` and its sanitize line.
- `diagram/src/lib.rs`: two module lines.
- `stencils/src/lib.rs`: `Master.container`, and the instantiate and redraw lines.
- `mermaid/src/lib.rs`: `pub mod architecture`, the `Diagram::Architecture` variant, its three match arms and parse arm, and `architecture-beta` removed from `OTHER_MERMAID_TYPES`.
- `mermaid/src/graph.rs`: `DiagramKind::Architecture`.
- `formats/src/mermaid.rs`: `pub mod architecture;` and one `Diagram::Architecture(a) =>` arm.
- `formats/src/svg.rs`: `mod step_badges;` and one call in `connector_svg`.
- `formats/src/native_tests.rs`: one test.
- `engine/src/cmd/mod.rs`: three `pub mod` lines and three `v.extend` lines.
- `engine/src/cmd/layout.rs`: a filter that leaves `KEY_EXT` groups out.
- `engine/src/cmd/template_tests.rs`: the hook assertions.
- `engine/src/tests.rs`: the junk rows.

**(4) What a later frontend branch needs:**
- A Notation tab Key group (`ribbon/notation/key.rs`): Insert Key, Refresh Key, Remove Key, and a Steps tool where a click on a connector runs `connector.setStep` with the next number, plus Renumber.
- The canvas draws step badges from `step_badge::badge`. It also needs a check that container decor honours `style.dash` and `corner`.
- An Inspector "Include in key" row, writing `key.insert {exclude}`.
- Dropping a node into a group already re-owns it, through `container.drop` and sync step 9. The canvas shows a nesting issue on hover.
- Palette categories for the three packs, gallery cards (already served by `template.list`), the Code pane entry for `mermaid.architecture`, a key auto-refresh preference, and retaken palette and notation-tab snapshots.

**(5) Risks and open questions:**
- **Step badge ownership.** The research's engine table puts `connector.setStep` in the cloud substrate, but its branch table gives `cmd/step.rs` to `b6-c4`. This brief takes it; the b6-c4 brief must extend `step.rs`, not recreate it.
- **One owner per element.** Two network views share one owner, so physical containment (site, rack) owns devices, and logical membership is the `Attached` and `Routes` relationships plus Zone owning Subnet. If users expect zones to own devices, a later schema step is needed (viewpoint-scoped owners).
- **Owner sync is a new bind step.** It is opt-in through `allowedParents`. The reverse direction (`element.create {owner}` placing the shape inside the owner's container) is not built, so seeds call `container.addMember`. `model.move` can still make the drawing and the model disagree, which the drawn check reports.
- **Shared enums.**
  - `Check` gains variants from b6-er and b6-sequence too (`endMarkerMatchesProperty`, `messageOrder`); append-only, merge in order.
  - `Glyph` may also gain b6-er's double-outline field.
  - The Mermaid `Diagram` enum also takes the C4 variant later.
- **Key freshness.** `key.refresh` is explicit. Auto-refresh after every command would need a `session.rs` hunk and adds undo noise; this is the user's call.
- **Title blocks need a sheet.** `sheet.titleBlock` refuses a sheetless page, so a `titleBlock` template with no sheet fails at run time. Suggest a one-line `template::check` warning (templates' file).
- **architecture-beta is a beta grammar.** Implement from the docs page only and note the date read in `docs/formats.md` (orchestrator). Icon-pack names such as `logos:*` are never fetched or drawn.
- **Small open items.**
  - The Gane-Sarson id band shows a line but not the process number until glyph label slots exist.
  - "A public subnet needs an internet gateway" has no fitting check (it needs a "contains a type" check) and is deferred.
  - The level-0 rule ("one process per context page") has no fitting check either and is deferred.
  - Confirm that `TableQuery` compartments fill outside sheet pages.
