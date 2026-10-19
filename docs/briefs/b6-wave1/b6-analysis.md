# `b6-analysis` (engine half), Block 6 and 6E wave 1

Opus, medium effort. Port 8062. Worktree `.claude/worktrees/b6-analysis`, branch `b6-analysis` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that added this
file). `CARGO_TARGET_DIR=target-b6-analysis`. Commit prefix `B6:`. Backend only: touch no file under
`crates/ui-egui` or `apps/vizcraft`.

## Resolutions (these override the draft below where they differ)

The cross-branch contracts are in docs/cascade-plan.md under "Block 6 and 6E wave 1 contracts".
For this branch:

- Nets live in `Model.nets` with `Session.nets: NetIndex` (b6e-substrate). The later net analyses build `Graph::from_edges` from that index, so keep `Context` passed by reference so fields can be added.
- Downstream follows source to target. Packs may declare a dependency direction later; not now. Comparing with git HEAD is refused for now.

## Draft brief

**(1) Brief: `b6-analysis`, engine half (Opus, port 8062, worktree `.claude/worktrees/b6-analysis`, branch `b6-analysis` from `claude/engineering-diagramming-standards-305d6b`@9eecade, `CARGO_TARGET_DIR=target-b6-analysis`, prefix `B6:`)**

Goal: graph analysis and model compare over the semantic model, reachable by command id, the CLI and MCP, with no UI.

New crate `crates/analysis` (`vizcraft-analysis`):
- It sits in the model layer and depends only on `vizcraft-model`, `serde`, `serde_json` and `thiserror`. It is wasm-safe and its root carries the AGENTS.md deny lints.
- The engine turns pack knowledge into plain lists before it calls the crate. So the crate does not need `vizcraft-notation`, and the rules in `notation/src/rules.rs` can later reuse its Tarjan.

Files and types:
- **`graph.rs`.** `Graph` uses indices: `ids: Vec<String>` (element ids in model order), `index: HashMap<String, usize>`, `edges: Vec<Edge {from, to, rel: String}>`, and `out`/`inc: Vec<Vec<(usize, usize)>>`.
  - `Graph::from_model(&Model, &EdgeFilter)`. `EdgeFilter` is `{forward: Selector, backward: Vec<String>, elements: Selector, containment: bool}`, reusing `vizcraft_model::query::Selector`. Inverse relationship types go in `backward` and are followed backwards. With `containment: true`, owner-to-child links become edges.
  - `Graph::from_edges(nodes, edges)` is the entry point 6E uses to build net or pin graphs.
  - Relationships whose ends are missing are skipped, and a repeated element id keeps the first one.
  - `Direction` is `Downstream`, `Upstream` or `Both`. Downstream follows source to target.
- **`scc.rs`.** `strongly_connected(&Graph)` is iterative Tarjan. `cycles(&Graph) -> Vec<Group>` keeps groups of two or more elements, or an element with a self loop.
- **`reach.rs`.** `reach(&Graph, starts, Direction, max_depth) -> Vec<(usize, u32)>` is a breadth-first walk that records the depth of each element. `unreachable(&Graph, roots)` uses the elements with no incoming edge as roots when none are given.
- **`cut.rs`.** `articulation_points` and `bridges` work on the undirected view with iterative lowlink. They track edge ids, not parent nodes, so a doubled relationship is never reported as a bridge.
- **`paths.rs`.**
  - `shortest_path(&Graph, from, to, Direction) -> Option<Path {nodes, edges}>`, breadth-first.
  - `longest_path(&Graph) -> LongestPath {path, cyclic}`. It sorts topologically with Kahn's algorithm. When the graph has cycles it works on the condensation and sets `cyclic`.
- **`metrics.rs`.** `metrics(&Model, &Graph) -> Metrics`:
  - one row per element: `{id, fan_in, fan_out, layer, containment_depth}`;
  - `totals`: elements, relationships, components, cycles, longest path;
  - `critical: Path`.
  - Containment depth uses one owner index with a memo. Calling `Model::ancestors` per element rebuilds `by_id` every time, which is O(n²).
- **`diff.rs`.** `diff(before: &Model, after: &Model) -> ModelDiff {elements: Changes, relationships: Changes, truncated}`.
  - `Changes` is `{added, removed, changed: Vec<ItemChange {id, fields: Vec<FieldChange {field, before: Option<Value>, after: Option<Value>}>}>}`, keyed by id.
  - Fields compared: type, name, tag, owner, source, target, both ports, and each `properties.<key>`.
  - This is separate from `sync::diff`, which matches by import record and is not symmetric.
- **`result.rs`.**
  - `AnalysisResult {kind, summary, elements, relationships, marks: BTreeMap<String, Mark>, groups: Vec<Group {label, elements, relationships}>, table: Option<ResultTable {columns, rows: Vec<(id, Vec<Value>)>}>, truncated, rev: u64}`.
  - `Mark` is `Focus`, `Upstream`, `Downstream`, `Cycle`, `Unreachable`, `Cut`, `Path`, `Added`, `Removed` or `Changed`.
  - `AnalysisError` is `Unknown(id)`, `Params(String)` or `TooLarge`.
- **`lib.rs`.** The extension seam:
  - `pub trait Analysis { fn id(&self) -> &'static str; fn label(&self) -> &'static str; fn params(&self) -> &'static str; fn run(&self, cx: &Context, params: &Value) -> Result<AnalysisResult, AnalysisError>; }`
  - `Context<'a> {model: &'a Model, types: &'a dyn Types}`, where `trait Types { fn is_a(&self, ty, ancestor) -> bool; fn inverses(&self, rel) -> Vec<String>; }`. `NoTypes` matches types exactly.
  - `pub fn analyses() -> &'static [&'static dyn Analysis]`, a static list in which each later pack appends one line. The six built-ins are `dependencies`, `cycles`, `unreachable`, `spof`, `metrics` and `path`.
  - Caps: `MAX_DEPTH_PARAM = 1024`, `MAX_RESULT_IDS = 10_000` (past it, `truncated`), `MAX_TYPES = 64`, and type ids of at most `MAX_KEY` bytes.
  - Every algorithm is iterative and linear, and its output is in model order.

New `crates/engine/src/cmd/analyze.rs`:
- `struct CatalogTypes` implements `Types` over `vizcraft_notation::infer::Facts::is_a` and the catalog's `inverse_of`. The engine expands each named type to itself plus its subtypes.
- `pub fn run_kind(s: &mut Session, kind: &str, v: &Value) -> CmdResult` is the engine seam:
  1. find the analysis in `analyses()`;
  2. map shape and connector ids through `cmd::model::element_of` and `req_relationship`;
  3. run it;
  4. stamp `rev = s.rev()`;
  5. store the result in `Session.analysis`;
  6. return its JSON plus `names` (id to label, from `query::label_of`).
- Analyses are session state and never change the document, so they add no undo step and do not mark it dirty.

Commands (location "Model > Analyze"; `.when(has_elements)` except `analyze.clear`, `analyze.result` and `model.diff`):
- `analyze.dependencies {id?: element or shape (default: primary selection), direction?: "downstream"|"upstream"|"both", depth?: 1..1024, types?: [relationship type], containment?: bool}` returns `{elements, relationships, levels: [[ids]], names}`.
- `analyze.cycles {types?}` returns `{groups, elements, relationships}`.
- `analyze.unreachable {from?: [ids], types?}` returns `{elements, roots}`.
- `analyze.spof {types?}` returns `{elements: cut points, relationships: bridges}`.
- `analyze.metrics {types?, sort?: "fanIn"|"fanOut"|"layer"|"depth", limit?}` returns `{rows, totals, critical}`.
- `analyze.path {from, to, direction?, types?}` returns `{elements, relationships, length}`, or a Params error when there is no path.
- `analyze.select {}` selects the shapes and connectors on the active page that draw the stored result's ids (via `vizcraft_diagram::views::views_of`).
- `analyze.result {}` returns the stored result or null.
- `analyze.clear {}` drops it.
- `model.diff {against?: "saved" (default) | "file", path?: string}`:
  - "saved" re-reads `s.path` with `io::read_limited` and `io::load_bytes`. With no path it is an error.
  - A given `path` implies "file", and `against: "head"` is refused as not yet supported.
  - The current model counts as "after".
  - It returns `ModelDiff` and stores it as kind `diff`, with marks `Added` and `Changed` on current ids and the removed ids kept in `groups`.

CLI: new `apps/vizcraft-cli/src/analyze.rs` adds `vizcraft-cli analyze FILE --kind dependencies|cycles|unreachable|spof|metrics|path|diff [--id ID] [--from ID] [--to ID] [--direction D] [--depth N] [--types T,T] [--against FILE] [--check] [--json]`.
- It opens FILE in a `Session` and calls `run_kind` (or `model.diff` with `path`).
- Text output is the summary plus a named id list.
- `--check` exits 1 when cycles, unreachable elements, single points of failure or diff changes are found. A usage error exits 2.
- `--kind` also accepts any id in `analyses()`, so later packs reach the CLI with no extra work.

Done when there are:
- crate unit tests:
  - cycles on two linked cycles plus a self loop;
  - reach depth 1 versus unlimited on a chain;
  - reach in both directions;
  - unreachable ignores a cycle that has no source;
  - the articulation point of a bow-tie, and no bridge on a doubled edge;
  - shortest path, and the absent case;
  - longest path in a DAG, and the cyclic case marked `cyclic`;
  - fan-in and fan-out;
  - containment depth, linear on a 256-deep chain;
  - diff of added, removed, renamed, retyped, reowned and changed-property items keyed by id;
  - a 100,000-element chain and a 200,000-edge star run without stack overflow;
  - a proptest that random models never panic and every output id exists;
- engine tests:
  - each command on a model built in code (the `engtest` convention);
  - `types` follows a subtype and an inverse type backwards;
  - a shape id stands for its element;
  - `Session.analysis` is set, `analyze.clear` empties it, and there is no undo step and the document stays clean;
  - `analyze.select` selects the drawing items;
  - `model.diff` against a saved temp file after a rename reports one changed name, and with no path it is an error;
  - `hostile_params_never_panic_on_analyze` in `analyze.rs` runs junk params against a model-bearing document, because the sample has no model and the global test only reaches the disabled path;
- the global `hostile_params_never_panic` stays green;
- CLI tests in new `apps/vizcraft-cli/tests/analyze.rs`: `--json` cycles, `--check` exits 1 on a cycle, and diff of two files;
- `cargo xtask ci` is green.

**(2) Own:**
- `crates/analysis/` (new): `Cargo.toml`, `src/{lib.rs, graph.rs, scc.rs, reach.rs, cut.rs, paths.rs, metrics.rs, diff.rs, result.rs, builtin.rs, tests.rs}`
- `crates/engine/src/cmd/analyze.rs` (new)
- `apps/vizcraft-cli/src/analyze.rs` (new)
- `apps/vizcraft-cli/tests/analyze.rs` (new)

**(3) Shared, append-only hunks:**
- `Cargo.toml`: `vizcraft-analysis = { path = "crates/analysis" }` in `# internal`.
- `xtask/src/layers.rs`: `"-analysis"` appended to `MODEL_SUFFIXES`, plus one assert in the `classification` test.
- `crates/engine/Cargo.toml`: `vizcraft-analysis = { workspace = true }`.
- `crates/engine/src/cmd/mod.rs`: `pub mod analyze;` and `v.extend(analyze::specs());` after `table::specs()`.
- `crates/engine/src/session.rs`, three lines:
  - field `pub analysis: Option<vizcraft_analysis::AnalysisResult>` with a doc comment;
  - `analysis: None` in `Session::new`;
  - `self.analysis = None;` in `set_document`.
- `crates/engine/src/tests.rs`: `"analyze"` in the prefix list.
- `apps/vizcraft-cli/src/main.rs`: `mod analyze;`, the `Some("analyze") => analyze::analyze(&args[1..])` arm, and one usage entry in both the doc comment and `USAGE`.
- `Cargo.lock` is regenerated.
- Untouched: `crates/model` (`types.rs` stays free for `b6e-substrate` nets and `b6e-sheets`), `crates/formats`, `crates/notation`, `cmd/model.rs` (`model.diff`'s spec lives in `analyze.rs`).

**(4) What the frontend half needs:**
- **State to read.** `Session.analysis` holds `marks` for colours by mark, `groups` and `table` for the dock, and `rev`. Show the result as stale when `rev != s.rev()`, and skip ids `model.contains_id` no longer finds.
- **Commands to call.** It maps ids to shapes with `vizcraft_diagram::views::views_of`. The Analysis dock's Select calls `analyze.select` and Clear calls `analyze.clear`. Explain can come from `summary` plus `names`.
- **Model tab Analyze group.** Upstream, Downstream and Both with a depth spinner, Cycles, Unreachable, Single points of failure, Metrics, Path, Compare and Clear highlights, all dispatching the ids above.
- **Compare page.** It reads the `diff` result. Removed items are absent from the current model, so the page has to draw them from the "against" document; it needs either `model.diff {includeBefore: true}` or a frontend-side load.
- **Files it owns.** `canvas/draw/highlights.rs` draws the marks. `ui.inspect` gains `analysis: {kind, count, rev}`. The frontend also owns `ribbon/model/analyze.rs` and `dock/analysis.rs`.

**(5) Risks and open questions:**
- **Direction wording.** Downstream follows source to target. For "depends on" relationships, callers are therefore upstream. Should packs declare a dependency direction?
- **No weights.** Longest path counts edges only. A `weight` property key for critical-path durations is deferred.
- **Comparing with git HEAD.** It is not wasm-safe and needs a `Services` hook. Refused for now.
- **Unreachable without roots.** With no `from` and no source element, everything is unreachable. Pack rules (`Check::Unreachable`) are typed and stay separate.
- **6E nets.** If `b6e-substrate` puts nets outside `Model`, `net.*` analyses need `Context` to gain an optional `&Document`. The trait should take `Context` by reference now so a field can be added later.
- **Duplicated Tarjan.** It now exists in both notation and analysis. Unifying them later needs notation to depend on analysis, which is allowed because analysis does not depend on notation.
- **`analyze.result` and `analyze.select` are additions** beyond the requested list. Agents and MCP need them because `ui.inspect` is UI-only.
- **Session state is lost on Undo.** `Session.analysis` is outside the document, so Undo neither restores nor clears it. The `rev` stamp is the mitigation.
