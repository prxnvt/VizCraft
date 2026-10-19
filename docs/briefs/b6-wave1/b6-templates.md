# `b6-templates` (engine half), Block 6 and 6E wave 1

Opus, medium effort. Port 8061. Worktree `.claude/worktrees/b6-templates`, branch `b6-templates` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that added this
file). `CARGO_TARGET_DIR=target-b6-templates`. Commit prefix `B6:`. Backend only: touch no file under
`crates/ui-egui` or `apps/vizcraft`.

## Resolutions (these override the draft below where they differ)

The cross-branch contracts are in docs/cascade-plan.md under "Block 6 and 6E wave 1 contracts".
For this branch:

- `Template.module` is a string with substrate's module names (`none`, `iso2_5mm`, `inch0_1`) or a number of page units, not millimetres.
- Hook command ids are fixed:
  - `page.setModule {page?, module}` (b6e-substrate)
  - `sheet.set {page?, sheet: "<standard>.<size>.<orientation>"}` (b6e-sheets)
  - `sheet.titleBlock {page?}` (b6e-sheets)
  - `key.insert {page?}` (b6-topology, wave 2)
  `sheetChoices` comes from `sheet.presets` when that command is registered.
- You own `Document.standards`; `b6e-sheets` reads it.
- Also expose `template::build(&Template, &Choices) -> Result<Document>` for the frontend gallery.

## Draft brief

**(1) Brief**

`b6-templates` engine half (Opus, port 8061, worktree `.claude/worktrees/b6-templates`, branch `b6-templates` from `claude/engineering-diagramming-standards-305d6b` at 9eecade, `CARGO_TARGET_DIR=target-b6-templates`, prefix `B6:`).

**Goal:** templates as pack data, and the commands that list them, open a document or page from one, and render a preview, all headless. The New gallery, the preset and the Home arrangement come later.

**Schema.** `Pack.templates` (today `Vec<String>`, unused by every shipped pack and by all code) becomes `Vec<Template>`. `Template` and its parts live in new `crates/notation/src/template.rs`, `#[serde(default, rename_all = "camelCase")]`:
- `id` is `<pack>.<name>`; also `label`, `description`, `audience: Vec<String>`, `keywords`.
- `viewpoint: String`: empty means none (Basic has no viewpoints).
- `seed: Vec<SeedStep {command, params: serde_json::Value, optional: bool}>`: the `script.run` list form. An `optional` step whose command is not registered is skipped instead of failing.
- `layout: Option<TemplateLayout {algorithm?, direction?, primitive?, roles: Value, nodeSpacing?, layerSpacing?}>`.
- `key: bool`, `titleBlock: bool`, `codeForms: Vec<String>` (grammar ids such as `mermaid.flowchart`).
- Engineering fields: `variant: String` (a variant of this pack), `sheet: String` (an opaque preset id `<standard>.<size>[.<orientation>]`, such as `iso.a3.landscape`), `module: Option<f64>` (symbol grid module in mm).

For compatibility, `Template` deserializes through an untagged repr. A bare string entry (the old form) loads as nothing and is dropped silently, so old third-party packs still load.

**Loader.** One line in `finish` in `loader.rs`: `problems.extend(crate::template::check(&pack))`. The check covers:
- at most 128 templates, each seed at most 512 steps;
- id prefix, duplicates, a non-empty label;
- the viewpoint and variant exist in the pack;
- seed command ids are non-empty with object or null params;
- algorithm, direction and primitive are in const lists that mirror `vizcraft_diagram::autolayout` and `layout.primitive` (an engine test asserts they match);
- `module` is finite and between 0.1 and 50;
- string lengths are capped.

**Document field.** Per-project standard choice: `Document.standards: Standards {symbols: String, sheet: String}` in new `crates/diagram/src/standards.rs`. It has serde defaults, `sanitize` caps it at 64 bytes, and empty means the defaults `iec` and `iso.a3.landscape`. The symbol family is the variant's local name by convention (`electrical.iec`, `electrical.jic`), so `variant_for(pack, family)` resolves `<pack>.<family>` and the `Variant` schema does not change.

**Engine.** New `crates/engine/src/cmd/template.rs` (`specs()` plus one line in `cmd/mod.rs`). The pipeline is `fn steps(s, &Template, &Choices, page) -> (Vec<(String, Value)>, Vec<Hook>)`, run inside one transaction:
1. `model.setViewpoint`
2. `variant.set` when a variant resolves
3. the sheet hook
4. the module hook
5. the seed, with absent optional steps filtered out
6. the layout: `layout.full {algorithm, direction}` or `layout.primitive {primitive, roles}`, only when the seed draws shapes, since an empty page disables layout
7. the title block hook, if `titleBlock` is set
8. the key hook, if `key` is set

**Hook seam.** A const table `HOOKS: &[(flag, command id)]` maps each optional step to a command id agreed with the other branches: `sheet.set {page?, sheet}` and `sheet.titleBlock {page?}` from b6e-sheets, `key.insert {page?}` from b6-topology, and `grid.setModule {page?, module}` from b6e-substrate. Each is looked up with `s.registry.get(id)`; if absent it is skipped and named in the result's `skipped`. Otherwise it runs and is named in `applied`.

**Commands:**
- **`template.list {audience?, pack?, query?}`** returns `{templates: [{id, label, description, pack, packName, audience, keywords, viewpoint, codeForms, key, titleBlock, variant, variants: [{id, label, family}], sheet, module}]}` in catalog order. `query` matches case-insensitively over label, keywords, id and description.
- **`template.new {template, path?, variant?, sheet?}`** works in five steps:
  - It validates every param before touching the session, so junk never clears history.
  - It runs the pipeline in a scratch `Session::new(Document::new())` with `registry = s.registry.clone()` as a dry run, so a failing seed leaves the user's document and history intact.
  - It calls `s.set_document(Document::new())` and `s.path = None`, then runs the same steps through `s.transaction("New from <label>", ..)`, which is one undo step back to a blank page.
  - It sets `doc.standards` from the resolved choice. Variant precedence is the param, then the template's `variant`, then `variant_for(pack, "iec")`, then the default drawing.
  - It selects the shape of the first model element on the page and saves through `file.save` when `path` is given.

  It returns `{template, page, variant, sheet, applied, skipped, primary, items, path?}`.
- **`template.addPage {template, variant?, sheet?, name?}`** applies the same pipeline after `page.add`. Its defaults come from `doc.standards`, which is the point of remembering per project.
- **`template.standards {symbols?, sheet?}`** reads or sets `doc.standards` (setting is an undo step). It returns `{symbols, sheet, symbolChoices: ["iec","ansi","jic","ieee","isa","iso"], sheetChoices}`. `sheetChoices` comes from `sheet.presets` when that command is registered, else `["iso.a3.landscape","ansi.b.landscape"]`. IEC and ISO A3 are first, and ANSI, JIC and ASME are always offered.
- **`template.preview {template, variant?, sheet?}`** runs the dry-run pipeline and returns `{template, svg, width, height}`. `svg` is UTF-8 text from `vizcraft_formats::svg::export_page`, capped at 2 MB. It changes nothing in the session, so the formats crate is untouched.

**Session.** A hunk in `session.rs`: `is_transaction` gains `"template.new" | "template.addPage"`. Without it, `Session::run` would record a second undo step back to the old document (inner `run`s reset `no_undo`) and run `bind::sync` against another file. `recordable` also skips `template.new`, like `file.`.

**CLI.** New `apps/vizcraft-cli/src/new.rs`:
- `vizcraft-cli new --template ID [--variant V] [--sheet S] [--svg PREVIEW.svg] [--json] OUT` runs `template.new {path: OUT}`.
- `vizcraft-cli new --list [--audience A] [--json]` lists templates.
- Exit codes are 1 on failure and 2 on usage errors.

**Data.**
- `basic.json` gets `basic.blank`.
- `flowchart.json` gets `flowchart.blank`, `flowchart.decision` (Terminator, Process, Decision with Yes and No to two Processes, a final Terminator; layered TB; codeForms `mermaid.flowchart`) and `flowchart.ciPipeline`. The CI pipeline seed is a Source terminator plus `pattern.insert {pattern: "workflow.parallelSplit", args: {branches: 3, labels: ["Unit tests","Lint","Integration tests"], split: "Build", join: "Deploy"}, x: 200, y: 0}`, a Release terminator and flows, laid out with the pipeline primitive, with keywords `ci`, `cd`, `pipeline`, `build`, `deploy`.
- The shipped seeds use `shape.add {stencil, id}` plus `relationship.create {from, to}` on those shape ids, so they need no result binding.
- Test packs built in code (the `engtest` convention) carry a template with `variant`, `sheet`, `module`, `key` and `titleBlock`.

**Done when:**
- `template::check` tests: a bad prefix, an unknown viewpoint or variant, an unknown algorithm or primitive, too many steps, a bare-string entry dropped, and every `BUILTIN` loading.
- Every shipped template builds through `template.new`, every seed command exists in the registry, and the result has no error-level `validate.run` issue.
- `flowchart.ciPipeline` records one `Model.patterns` instance and no shapes overlap.
- After `template.new`, `undo_depth() == 1`, and one undo leaves a blank page.
- A seed that fails leaves the previous document and its undo stack unchanged.
- With a fake `key.insert` and a fake `sheet.titleBlock` registered in the session registry, the flags run them and they show in `applied`; without them, they show in `skipped`.
- The test-pack template picks its `iec` variant by default and `jic` on `variant: "engtest.jic"`; `template.addPage` uses `doc.standards`.
- `Document.standards` round-trips natively, and an old file loads with it empty.
- `template.preview` returns SVG that starts with `<svg` and contains the seeded labels, and the session's `rev` is unchanged.
- `template.list` filters by audience, pack and query.
- CLI integration tests in new `apps/vizcraft-cli/tests/new.rs`: `new --template flowchart.decision OUT` writes a file that `inspect` reads, `--svg` writes a preview, an unknown id exits 1, and a missing OUT exits 2.
- `hostile_params_never_panic` stays green, with two junk rows added for this command family: `{"template": "flowchart.decision", "variant": 5, "sheet": {}, "path": {}}` and `{"template": "x" repeated 1000 times}`.
- `cargo xtask ci` is green.

**(2) Own:**
- `crates/notation/src/template.rs` (new)
- `crates/diagram/src/standards.rs` (new)
- `crates/engine/src/cmd/template.rs` (new, with its tests)
- `apps/vizcraft-cli/src/new.rs` (new)
- `apps/vizcraft-cli/tests/new.rs` (new)
- Optional: `crates/mcp/src/templates.rs` (new), a `vizcraft://templates` resource backed by `template.list`. It is optional because the generic command tool already reaches `template.*`, and `vizcraft://notation/{pack}` already serialises the whole `Pack`, templates included.

**(3) Shared, with the exact hunk this branch adds:**
- `crates/notation/src/schema.rs`: the field type becomes `pub templates: Vec<crate::template::Template>` and its doc comment changes. Nothing else.
- `crates/notation/src/loader.rs`: one line in `finish`, plus one test.
- `crates/notation/src/lib.rs`: `pub mod template;` and a `Template` re-export.
- `crates/diagram/src/model.rs`: `Document.standards` and the same field in the on-disk `DocumentFile`, with a sanitize call.
- `crates/diagram/src/lib.rs`: the module line.
- `crates/formats/src/native_tests.rs`: one round-trip test.
- `crates/engine/src/cmd/mod.rs`: `pub mod template;` and `v.extend(template::specs());`.
- `crates/engine/src/session.rs`: `is_transaction` and `recordable`.
- `crates/engine/src/tests.rs`: the two junk rows.
- `apps/vizcraft-cli/src/main.rs`: `mod new;`, one match arm, the usage text and the header doc lines.
- `crates/notation/packs/basic.json` and `flowchart.json`: a top-level `templates` key only.
- `crates/mcp/src/resources.rs`: one `list.extend` line, only if the optional resource is built.

**(4) What the later frontend half needs:**
- The `ui-egui/backstage.rs` gallery:
  - Cards come from `template.list`, with audience chips from the `audience` values. Electrical, Process and Mechanical appear once the 6E packs land.
  - Previews come from `template.preview` SVG, rasterised to a texture through the existing offscreen renderer and cached by id, pack version, variant and sheet. If an SVG raster path is missing, the frontend may instead render the dry-run document directly, which would need a `template::build(&Template, &Choices) -> Result<Document>` exposed by this branch. That is a one-function addition worth making here.
  - Picking a card runs `template.new` and then opens the Inspector.
  - A two-question dialog uses `symbolChoices`, `variants` and `sheetChoices`, pre-filled from `template.standards`, with IEC and ISO A3 first.
- Recent templates and the last-chosen standards go in preferences, not in the document.
- `ui.preset` and the Home arrangement.
- Template preview snapshots once wave 4 packs land.

**(5) Risks and open questions:**
- **Hook command ids.** `sheet.set {sheet}`, `sheet.titleBlock`, `key.insert` and `grid.setModule {module}` must be written into the b6e-sheets, b6-topology and b6e-substrate briefs now, or the hook rows will need a one-line rename at merge. The sheet id grammar `<standard>.<size>[.<orientation>]` also belongs to b6e-sheets.
- **Ownership of `Document.standards`.** b6e-sheets may want it for new-page defaults; they should read it, not redefine it. The alternative is to put it inside their typed document properties.
- **Family convention.** Deriving the symbol family from a variant's local name is a convention, not schema. If a variant needs to declare its family explicitly, `Variant.standard` would be a later hunk.
- **Dry run cost.** The dry run doubles seed cost and builds a full registry per scratch session. That is fine for small seeds; if previews of 30 or more templates get slow, add a `Session` constructor that takes a registry.
- **Seed references.** Seeds can refer to earlier steps only through caller-chosen shape and element ids. Result binding (`as` and `${name.id}`) is left out on purpose.
- **Code forms.** `codeForms` are unchecked strings until b6-code adds the `CodeForm` registry.
- **`path` param.** `template.new {path}` writes to disk. The hostile test only skips `file.*` commands, so the command must refuse a missing or unknown `template` before it reads `path`, as the junk rows check.
- **Shared JSON.** The pack JSONs are shared with b6-uml and b6-er only for their own new packs, so there is no hunk overlap expected.
