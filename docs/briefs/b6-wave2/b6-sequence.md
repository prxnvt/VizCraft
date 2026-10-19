# `b6-sequence` (engine half), Block 6 wave 2

Opus, medium effort. Port 8064. Worktree `.claude/worktrees/b6-sequence`, branch `b6-sequence` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (after b6-uml merges; the session names
the commit). `CARGO_TARGET_DIR=target-b6-sequence CARGO_INCREMENTAL=0`. Commit prefix `B6:`.

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

- Starts after `b6-uml` merges. `uml.json` uses `Pack.sections`: append the sequence content
  as ONE section object `{"id": "sequence", ...}` at the end of `sections` (glyphs, connector
  types, element and relationship types, connections, palette, viewpoint, rules, ruleset
  extension and the template inside it), instead of appending to the top-level arrays. Reuse
  `uml.Note` and `uml.Actor` where they fit.
- The re-import merge skip for generated activation and destruction shapes (one line in
  `cmd/file/merge.rs`) belongs to this branch.
- Rule `reference` strings cite UML 2.5.1 sections you have checked, never invented clause
  numbers.

## Draft brief

**(1) Brief**

**Goal.** UML sequence diagrams as a substrate. A Sequence page is an ordinary page with an ordered list of events. After every command a settle pass redraws the page from that list: lifeline columns, message rows, activations derived from call and reply pairs, and fragments that span their lifelines. Engine commands edit the order without a mouse. Two rules check order and replies. Mermaid `sequenceDiagram` is read into the substrate and written back, and a PlantUML writer is added. The "Sequence diagram" template ships. The canvas draws all of it with no UI code, for three reasons: the lifeline head and dashed stem are a shape kind's `outline` and `detail`; activations and destruction marks are ordinary generated shapes; fragments draw through `container::decor`, which the canvas already calls.

**Model.** New `crates/diagram/src/sequence.rs` adds three optional fields, each `#[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]`, so old files load unchanged:
- **`Page.sequence: Option<Interaction>`**: `Interaction { events, activations: auto|explicit, autonumber, row: 36, column_gap: 60 }`.
  - `events: Vec<Event>` is the single source of time order. On disk each event is a one-key object: `{"message": c}`, `{"open": f}`, `{"else": f}`, `{"close": f}`, `{"note": s}`, `{"activate": l}`, `{"deactivate": l}`.
  - Caps: `MAX_EVENTS` 4096; unknown events are dropped.
- **`Shape.sequence: Option<Part>`**, one of:
  - `lifeline {destroyed}`
  - `activation {lifeline, depth}` and `destruction {lifeline}`: generated and owned by settle
  - `fragment {operator, operands: [{guard, at}], covers: [lifeline]}`. Operators: `alt opt loop par break critical ref neg strict seq region`; `region` is Mermaid's `rect`. Caps: 32 operands, 512-byte guards.
  - `note {over, side}`
- **`Connector.message: Option<Message {sort}>`**: `sync | async | reply | create | destroy | lost | found`.

**Drawing.**
- **Kinds.** `ShapeKind::Lifeline` and `ShapeKind::ActorLifeline` are added. Their outline is the head only: a box of `sequence::HEAD` (40) for a lifeline, and for an actor a stick-figure head with the label band (`ACTOR_HEAD`, 64). Their `detail` is the stem, drawn as dash segments (6 on, 4 off, at most 2000, with the period stretched beyond that) plus the figure's body lines. Both kinds stay out of `ShapeKind::ALL`, as `Freeform` does, so the galleries and the ribbon test are unaffected.
- **Head text.** Settle puts the head text in `Shape.text_block`.
- **Fragments.** A fragment is a shape with a locked, transparent `Container` (heading size 0, style `Plain`). `sequence::fragment_decor(s)` returns `Decor` items for the operator pentagon tab and its text, one dashed separator per extra operand at `at`, and the `[guard]` texts.
- **Generated shapes.** Activations are white `uml.activation` rectangles, 10 wide. Destruction marks are freeform crosses. Both are locked for position, size and delete.
- **Messages.** Messages are `Route::Straight` connectors. A self message gets two waypoints.

**Glue.** Settle rewrites each lifeline's stored `connection_points`: its 4 frame midpoints first, then one point per message end in event order, offset to the edge of the active bar. It then re-indexes every message end as `End::point(lifeline, 4 + k)`. Because messages stay glued, the existing behaviour still applies: bind step 3, `connector.delete`, the analysis graph, and the `danglingConnector` rule.

**Layout.** New `crates/layout/src/sequence.rs` is pure:
- The entry point is `layout(&SeqInput {lifelines: [{width, head, actor}], events}, &SeqOptions {row, column_gap, activation_width: 10, auto_activations, keep_columns: Option<Vec<f64>>}) -> Result<SeqLayout, LayoutError>`. `SeqLayout` holds `columns`, `heads`, `stems`, `rows`, `routes`, `activations [{lifeline, depth, y0, y1}]`, `fragments [Rect]`, `separators`, `notes` and `bounds`.
- Columns are packed left to right and widened for message labels and self-loops, using the rule in today's `formats/src/mermaid/sequence.rs`. With `keep_columns`, the current x positions are kept and only pushed apart where they would overlap.
- Each event takes a row: a message one row (taller for its label); a self message two; `open` a tab row; `else` a separator row; `close` half a row; activate and deactivate none.
- In `auto` mode, a sync call pushes an activation on its receiver (a self call nests at `depth + 1`), and a reply pops the innermost activation opened by a call from the replying lifeline's caller.
- The operands of `alt`, `opt`, `break`, `critical` and `par` fork from the stack as it was at `open`. A call that is never answered ends after the receiver's last message inside the caller's activation.
- A created lifeline's head is centred on its create row. A destroyed lifeline's stem ends at its last event.
- A fragment spans the lifelines it covers plus 24, with 8 more per nesting level.
- Unbalanced or out-of-range events return `LayoutError::InvalidRoles {primitive: "sequence", ..}`.
- `Primitive::Sequence { lifelines?: [T] }` is added. Its graph adapter treats nodes as columns and edges in index order as sync messages.

**Settle.** `sequence::settle_doc(doc, before)` runs only on pages whose interaction or items differ from `before`. It:
- prunes events that name missing items;
- adopts connectors carrying a `message` that are glued to two lifelines, appending them in z order;
- clears a fragment part that no event names;
- runs the layout with `keep_columns`;
- applies the result: frames, `text_block`s, glue, waypoints, operand `at`s, and the generated shapes (reused per lifeline in order, new ids from `doc.next_id("s")`);
- orders z as fragments, then lifelines, then activations, then notes.

It is idempotent. `sequence::relayout(page)` is the full layout that re-packs the columns.

**Commands.** New `crates/engine/src/cmd/sequence.rs`. Every command except `sequence.new` is enabled only when the page has an interaction; refusals name the bad id.
- `sequence.new {page?}`: adds the interaction and sets viewpoint `uml.Sequence` when the pack is loaded.
- `sequence.addLifeline {name, id?, actor?, at?, represents?}` → `{id, element}`.
- `sequence.addMessage {from, to?, label?, sort?, id?, before?|after?, in?: {fragment, operand}}` → `{id, relationship, order}`. It creates the `uml.Message` relationship itself, with property `sort`, so the property lands in the same step.
- `sequence.reply {call, label?}`: places the reply after the last event nested in the call.
- `sequence.move {id, before?|after?|in?}` (exactly one of the three) → `{id, order}`. It moves a message or a note, or a whole fragment block, and refuses to move a block into itself.
- `sequence.setSort {id, sort}`: updates the kind, arrows and dash, and the `sort` property.
- `sequence.addNote {text, over, side?, id?, before?|after?}`.
- `sequence.wrap {ids, operator, guard?, covers?, id?}`: `ids` must be contiguous at one nesting level, otherwise the error names the gap.
- `sequence.unwrap {id}`.
- `sequence.addOperand {id, before, guard?}`.
- `sequence.removeOperand {id, operand}`.
- `sequence.setFragment {id, operator?, guards?, covers?}`.
- `sequence.layout {page?, row?, columnGap?}`.
- `sequence.order {page?}` → `{events: [{kind, id, order, from, to, sort, label, fragment, operand, depth}], findings}`. Read only.
- `sequence.write {page?, grammar: "mermaid"|"plantuml"}` → `{text, warnings}`. Read only.

`layout.primitive {primitive: "sequence"}` on a Sequence page runs `sequence::relayout`.

**Rules.**
- `Check::MessageOrder` reports:
  - a message above an earlier one, or on another message's row. This is for hand-drawn or imported pages, where the order is taken from y.
  - a message to a created lifeline before its create message, or any message after a lifeline's destroy.
  - an unbalanced fragment, or an `else` in `opt`, `loop`, `break` or `ref`.
  - a message drawn outside the operand it is listed in.
- `Check::RepliesMatched { requireReplies }` reports a reply with no open sync call from its receiver to its sender on that path (operands are paths), and a call answered twice on one path. With `requireReplies`, it also reports a call never answered.

The logic lives in `sequence::check_order` and `sequence::check_replies`; new `crates/notation/src/sequence_rules.rs` turns the findings into `Issue`s.

**Pack.** Append to `uml.json`:
- glyphs `uml.lifeline` (outline `lifeline`), `uml.actorLifeline`, `uml.activation`, `uml.fragment` and `uml.destruction`, each with a `source`;
- element types `uml.Lifeline` (`represents`) and `uml.ActorLifeline` (extends `uml.Lifeline`);
- relationship type `uml.Message` (`sort` enum `synchCall asynchCall reply createMessage deleteMessage`);
- connector types `uml.syncMessage` (triangle), `uml.asyncMessage` (open), `uml.replyMessage` (open, dashed), `uml.createMessage` (open, dashed) and `uml.destroyMessage`;
- connection row Lifeline–Message–Lifeline;
- palette group `uml.sequence`;
- viewpoint `uml.Sequence`;
- rules `uml.messageOrder` and `uml.repliesMatched` (live, warning);
- template `uml.sequence`.
  - Seed: `sequence.new`; lifelines `client` (actor), `api` and `db`; messages m1 (client→api, sync "GET /orders/42"), m2 (api→db, sync "query"), m3 (db→api, reply "rows"), m4 (api→client, reply "200 OK") and m5 (api→client, reply "404 Not Found"); then `sequence.wrap {id: "f1", ids: ["m4","m5"], operator: "alt", guard: "found"}` and `sequence.addOperand {id: "f1", before: "m5", guard: "else"}`.
  - Layout `{primitive: "sequence"}`; `codeForms` `["mermaid.sequence", "plantuml.sequence"]`.

**Formats.**
- **Mermaid read.** Rewrite `crates/formats/src/mermaid/sequence.rs` so it builds the substrate:
  - participants and actors become lifelines; `create` and `destroy` map to sorts and `destroyed`;
  - arrows map to sorts (`->>` sync, `-)`/`--)` async, `-->>` reply), and an arrow token that is not canonical is kept in `ext["mermaid.arrow"]`;
  - blocks become fragments and `else`/`and`/`option` become operands; `rect` becomes `region`;
  - activate, deactivate and `+`/`-` become explicit events, and their presence switches the interaction to `explicit`;
  - notes become note parts; boxes stay background rectangles with `ext["mermaid.box"]`.
- **Mermaid write.** New `crates/formats/src/mermaid/sequence_write.rs`, `write(page) -> (String, Vec<String>)`. In `auto` mode it writes `+`/`-` on calls and replies, so GitHub draws the same bars. `ref`, `lost` and `found` are lossy: a note or comment plus a warning.
- **PlantUML.** New `crates/formats/src/plantuml_sequence.rs`: `participant`/`actor`, `->`, `->>`, `-->`, `create`, `destroy`, `alt`/`else`/`opt`/`loop`/`par`/`break`/`critical`/`end`, `ref over`, activate and deactivate, `note`.

**Done when.**
- **Layout tests** (`crates/layout/tests/sequence.rs`):
  - columns increase by at least the gap;
  - rows strictly increase;
  - nested calls A→B→C give B depth 1 and C depth 1 with nested spans;
  - a self call nests at depth 2;
  - alt operands fork, so two replies give one bar;
  - fragment rects contain their routes and nest with at least 8 of margin;
  - create and destroy rows are honoured;
  - unbalanced input is an error;
  - 5000 events is an error, and NaN widths do not panic.
- **Diagram tests** (`sequence_tests.rs`):
  - serde round trip, and old files and unknown events load;
  - settle is idempotent, and every message is horizontal at its row y;
  - glue indices follow a `move`;
  - deleting a message prunes its event, and a lifeline's delete leaves free ends that `uml.danglingConnector` reports;
  - kind outlines and dashes survive junk sizes, and the dash count stays at or under 2000;
  - the alt decor has the pentagon, one dashed separator and two guard texts;
  - `wrap` refuses non-contiguous ids.
- **Engine tests** (`cmd/sequence_tests.rs`):
  - each command is one undo step, and the template seed's commands give the expected `sequence.order`;
  - `uml.Message` relationships are bound with `sort`;
  - `template.new {template: "uml.sequence"}` has zero error issues and no `uml.messageOrder` or `uml.repliesMatched` issue;
  - `layout.primitive {primitive: "sequence"}` equals `sequence.layout`;
  - `page.duplicate` keeps the order and fragments;
  - two junk rows in `hostile_params_never_panic`: `{"id": "x" repeated 1000 times, "before": 5, "after": [], "operator": "zz", "ids": [null]}` and `{"from": "s1", "to": "s1", "sort": 9, "in": {"fragment": 3}}`.
- **Pack cases** in `crates/notation/packs/tests/uml/`:
  - `sequence_clean.json` (expects nothing);
  - `sequence_reply_before_call.json` (expects `uml.repliesMatched`);
  - `sequence_alt_replies.json` (expects nothing);
  - `sequence_message_after_destroy.json` (expects `uml.messageOrder`);
  - `sequence_unbalanced_fragment.json` (expects `uml.messageOrder`).
- **Goldens**, inline like `svg.rs`'s `golden_*`:
  - Mermaid canonical text for the template page;
  - `read(write(read(x)))` equal to `read(x)` for events, sorts, labels, guards and activation rects;
  - PlantUML text for the template page;
  - SVG for a two-lifeline call and reply: head, dash segments, activation rect and arrowheads;
  - a native round trip.
- **CLI.** `vizcraft-cli new --template uml.sequence --svg <scratch>/seq.svg <scratch>/seq.vizcraft` writes a file that `inspect` reads.
- `cargo xtask ci` is green.

**(2) Own**
- `crates/diagram/src/sequence.rs`, plus `sequence_tests.rs` included through `#[path]`
- `crates/layout/src/sequence.rs`
- `crates/layout/tests/sequence.rs`
- `crates/notation/src/sequence_rules.rs`
- `crates/notation/packs/tests/uml/sequence_*.json`
- `crates/engine/src/cmd/sequence.rs`, plus `sequence_tests.rs` included through `#[path]`
- `crates/formats/src/mermaid/sequence.rs` (rewritten, with its tests)
- `crates/formats/src/mermaid/sequence_write.rs`
- `crates/formats/src/plantuml_sequence.rs`

**(3) Shared, one hunk each**

| File | Hunk |
|---|---|
| `notation/packs/uml.json` | Entries appended at the tail of `glyphs`, `connectorTypes`, `elementTypes`, `relationshipTypes`, `connections`, `palette`, `viewpoints`, `rules` and `templates`, plus the two rule ids appended to the default ruleset's `rules`. No `b6-uml` entry is edited. |
| `formats/src/mermaid.rs` | `mod sequence;` becomes `pub mod sequence;`, plus `pub mod sequence_write;`. The dispatch line `Diagram::Sequence(seq) => sequence::build(seq, &mut warnings)` stays. |
| `formats/src/mermaid/tests.rs` | Delete `sequence_diagram_has_rows_lifelines_and_ordered_messages` and `sequence_autonumber_and_boxes` (around lines 223–300); their checks move, rewritten, into the new tests. |
| `formats/src/lib.rs` | `pub mod plantuml_sequence;` |
| `formats/src/plantuml.rs` | Only if `b6-uml`'s page writer exists: one arm `if page.sequence.is_some() { return crate::plantuml_sequence::write(page) }`. |
| `diagram/src/kind.rs` | `Lifeline` and `ActorLifeline` variants, two `named_enum!` rows (`"lifeline"`, `"actorLifeline"`), and `outline`, `detail` and `connection_points` arms that call `crate::sequence::{head_outline, stem_detail}`. `ALL` is unchanged. |
| `formats/src/drawio/write.rs` and `read.rs` | Write: `ShapeKind::Lifeline \| ShapeKind::ActorLifeline => {}` (a plain box carrying the `vizcraft` master key). Read: arms in `master_for_kind` (`"uml.lifeline"`, `"uml.actorLifeline"`) and in the `shape_key` match. |
| `diagram/src/model.rs` | The three fields, the `Default` impls and the `sanitize` calls. |
| `diagram/src/lib.rs` | `pub mod sequence;` |
| `diagram/src/container.rs` | Last line of `decor()`: `out.extend(crate::sequence::fragment_decor(s));` |
| `diagram/src/routing_policy.rs` | `if c.net.is_some() \|\| c.message.is_some() { return c.avoid; }` |
| `diagram/src/autolayout.rs` | First lines of `layout_primitive`: `if matches!(primitive, Primitive::Sequence { .. }) && page.sequence.is_some() { return crate::sequence::relayout(page); }` |
| `layout/src/lib.rs` | `pub mod sequence;` |
| `layout/src/primitives/mod.rs` | The `Sequence` variant, its `name`, `try_map` and `references` arms, the dispatch arm, and a table row. |
| `notation/src/schema.rs` | `MessageOrder` and `RepliesMatched { #[serde(default)] require_replies: bool }`, before `Unknown`. |
| `notation/src/rules.rs` | `Check::MessageOrder \| Check::RepliesMatched { .. } => crate::sequence_rules::check(self.doc, rule, out),` |
| `notation/src/validate.rs` | The same pattern with `=> {}`. |
| `notation/src/lib.rs` | `pub mod sequence_rules;` |
| `notation/src/template.rs` | `"sequence"` appended to `PRIMITIVES`. |
| `engine/src/cmd/mod.rs` | `pub mod sequence;` and `v.extend(sequence::specs());` |
| `engine/src/cmd/layout.rs` | `\|"sequence"` in the params doc, plus `sequence {"lifelines"?: [id]}`. |
| `engine/src/session.rs` | First block inside `if changed {`: `let _t = crate::trace::span("sequence.settle"); vizcraft_diagram::sequence::settle_doc(&mut self.doc, &before_doc);` |
| `engine/src/cmd/page.rs` | In `duplicate`: build a connector map, then call `vizcraft_diagram::sequence::rename_ids(&mut page, &map, &cmap)`. |
| `engine/src/tests.rs` | The two junk rows. |
| `docs/architecture.md` | A `sequence.*` prefix row and the three field rows, changed together with the code. |

**(4) What a later frontend branch needs**
- **Reorder interaction.** New `canvas/interact/sequence.rs`:
  - dragging a message row runs `sequence.move`, using `sequence::rows(page) -> [(event id, y)]` to find the target;
  - lifelines move horizontally only, and dragging one re-orders the columns;
  - a connector dropped between two stems runs `sequence.addMessage`;
  - wrapping the selection runs `sequence.wrap`, and dragging an operand separator runs `sequence.addOperand` or `sequence.move`.
- **Stem hit-testing.** Wire the provided `sequence::stem_hit(shape, p, tol)` into selection and glue. Until then, a lifeline is picked by its head.
- **Connection-point marks.** Do not draw a lifeline's stored points past index 4; they are message rows.
- **Ribbon, palette and Inspector.** A Sequence group (Lifeline, Actor, each message sort, Reply, Note, fragment operators), and Inspector fields bound to `sequence.setSort` and `sequence.setFragment`. Bind Alt+Up and Alt+Down to `sequence.move`.
- **Snapshots.** Template preview snapshots.

**(5) Risks and open questions**
- **The `uml.json` seam.** JSON has no comments, so the "delimited place" can only be the array tails. The `b6-uml` brief should leave `uml.Sequence` out, not assert viewpoint or rule counts, and say whether it already defines a note glyph or a `uml.Actor` worth reusing.
- **Point glue is settle-owned.** A hand edit of a lifeline's `connection_points` is overwritten. drawio export loses the interaction and writes plain boxes.
- **Re-import merge.** `file.import {merge: true, type}` (in `cmd/file/merge.rs`) would turn generated activation and destruction shapes into elements. The fix is a one-line skip for shapes whose `sequence` part is neither a lifeline nor a note. Who owns it is open: this branch or `b6-code`.
- **Paste.** Pasted fragments lose their part (only messages are adopted), and `autonumber` is kept for round trips only.
- **Lifeline head height.** It is a constant, so long labels widen the head rather than deepen it. Variable heads would need `Shape::local_outline` and `detail_world` overrides, and the canvas fill cache keys only on the kind and size.
- **Lossy Mermaid round trips.** `ref`, `neg`/`strict`/`seq`, `lost`/`found` and note placement. Text is identical only for the canonical form.
- **Rules on free drawing.** Rules run only when the `uml` pack is in use. A plain Mermaid import without merge is free drawing, so it is unchecked until it is bound.
- **Clause references.** Rule `reference` strings must cite UML 2.5.1 sections the agent has checked, not invented clause numbers.
