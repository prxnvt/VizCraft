# `b6e-builders` (engine half), Block 6 and 6E wave 1

Opus, medium effort. Port 8073. Worktree `.claude/worktrees/b6e-builders`, branch `b6e-builders` from the
integration branch `claude/engineering-diagramming-standards-305d6b` (the commit that added this
file). `CARGO_TARGET_DIR=target-b6e-builders`. Commit prefix `B6E:`. Backend only: touch no file under
`crates/ui-egui` or `apps/vizcraft`.

## Resolutions (these override the draft below where they differ)

The cross-branch contracts are in docs/cascade-plan.md under "Block 6 and 6E wave 1 contracts".
For this branch:

- Read `Shape.orient` and `View.module` (named so by `b6e-substrate`; `snap::module_step(page)` in page units). Until substrate merges, use `module: None` and the `glyph_labels::anchor` helper as drafted.
- On rebuild, when the page has a module, move the shape so its first port lands on the module grid. A manual resize is kept until the next rebuild, which redraws at true size; the frontend may lock the aspect ratio later.
- Builder port ids win on the shape. Wave 2 packs declare matching ids, and a stencils test over shipped packs checks they match.
- Provenance: accept `own design` or a `drawn from ...` prefix, with the forbidden-word list as drafted. ISO 1219 uses the 2.5 mm module.
- Also fix two Block 5 follow-ups in files you touch. A path glyph is normalised to its unit box instead of stretched to its bounds. PDF fills closed subpaths even-odd, as SVG already does.

## Draft brief

**(1) Brief**

`b6e-builders` engine half (Opus). Goal: parametric glyphs. A pack glyph names a Rust builder and its defaults. The builder draws the symbol from the element's properties and the page's standard on the symbol grid. The sync pass redraws it when those inputs change, and every builder and path glyph in a shipped pack states where its drawing came from.

*Schema* (`crates/notation/src/schema.rs`):
- `Glyph.builder: String` (builder name; the outline is then `"builder"`, constant `loader::BUILDER_OUTLINE`).
- `Glyph.params: BTreeMap<String, PropValue>` (defaults).
- `Glyph.source: String` (provenance, at most 256 bytes).
- All three use serde defaults and are skipped when empty, so existing packs and path glyphs read and write byte for byte as before.

*Loader* (`loader.rs`): a builder glyph is checked as `freeform`, the way path glyphs are. The loader reports:
- `builder` or `params` on a non-builder glyph;
- a builder glyph with no builder name;
- more than 64 params, a key longer than 64 bytes, or a text value longer than 1 KiB.
New `loader::load_shipped(name, text)`, used by `Catalog::builtin()`, also requires a non-empty `source` on every builder and path glyph. Disk packs only get a warning, so existing user packs still load.

*Builders* (new `crates/stencils/src/builders/`):
- `mod.rs` holds the registry:
  - `BuilderSpec {name, label, description, params: &'static [ParamSpec], sources: &'static [(&str, &str)], build: fn(&Params, &Ctx) -> Built}`
  - `ParamSpec {key, label, kind: ParamKind::{Choice(&[&str]), Int{min,max}, Number{min,max}, Bool, Text{max}}, default, description}`
  - `pub fn specs() -> &'static [BuilderSpec]`, `pub fn spec(name)`
  - `pub fn build(name, raw: &BTreeMap<String, PropValue>, module: Option<f64>) -> Result<Built, BuildError>`
- `Params::resolve` clamps every value to its spec and falls back to the default, with a warning in `Built.warnings`. It never fails on a bad value; only an unknown builder name is an error.
- `Built {fills: Path, lines: Path, w, h, ports: Vec<BuiltPort {id, label, x, y, dir}>, labels: Vec<BuiltLabel {id, text, x, y, size, anchor}>, warnings}`, all in module units with the origin at top left. Every port sits on whole module coordinates.
- Module: `module` in page units (100 per inch). `None` means the standard's default: 2.5 mm (9.8425) for `iec60617`, `iso1219` and `iso2553`; 0.1 inch (10) for `jic`, `ieee315`, `ansi` and `aws`. The page's `View.module` from `b6e-substrate` wins when it is set.
- One file per builder:
  - `contact.rs`: `form` no or nc; `timing` none, onDelay or offDelay; `poles` 1 to 6; `standard` iec60617 (default), jic or ieee315. Ports `13`/`14` or `21`/`22` per pole.
  - `coil.rs`: `standard`, `kind` relay, contactor, timer or solenoid. Ports A1, A2. Tag slot.
  - `bubble.rs`: ISA-5.1. `system` discrete, sharedDisplay, computer or safety; `location` field, panelFront, panelRear, localFront or localRear; `letters` up to 6 characters and `number` up to 8, drawn inside as slots.
  - `valve.rs`: ISO 1219 directional valve. `positions` 2 or 3; `ports` 2 to 5; `paths` as a text grammar, one group per position separated by `;`, e.g. `"P-A B-T; X; P-B A-T"`, where `X` is all blocked and `P|` blocks P; `actuatorLeft` and `actuatorRight` from lever, button, pedal, roller, plunger, solenoid, pilot or none; `return` spring, detent or none; `rest` (rest position index); `portLabels` iso9461 or iso5599; `medium` hydraulic or pneumatic. Port labels appear on the rest square only.
  - `cylinder.rs`: `acting` single or double, `rod` single or through, `cushion` none, a, b or both, `springReturn`, `medium`.
  - `pump.rs`: `role` pump or motor, `flow` uni or bi, `variable`, `drain`, `medium`.
  - `gate.rs`: `function` and, or, not, nand, nor or xor; `inputs` 1 to 16; `invert` as a text mask, e.g. `"01"`; `shapes` rectangular (IEC 60617-12, the default) or distinctive (IEEE 91).
  - `fcf.rs`: `characteristic` (14 values), `diameter`, `tolerance`, `modifier`, `datums` (up to 3, separated by `|`), `standard` iso1101 or asmeY14_5. Cells laid out as slots.
  - `weld.rs`: `standard` iso2553 (dashed identification line) or aws, `arrowSide` and `otherSide` from about 10 elementary symbols, `size`, `length`, `allAround`, `field`, `tail`.
  - `texture.rs`: `standard` iso21920 or asmeY14_36, `removal` any, required or prohibited, `lay`, `value`, `process`.
- Caps:
  - every count and text length has a limit;
  - a builder emits at most 2048 path commands;
  - no indexing with `[]`;
  - every module carries `//!` provenance in its doc comment, and `sources` holds the same text.

*Shape state* (new `crates/diagram/src/glyph_state.rs`): `GlyphState {builder, params: BTreeMap<String, PropValue> (the shape's own overrides), ports: Vec<String>, labels: Vec<GlyphLabel {id, text, x, y (0..1), size, anchor}>}`, sanitized and capped (64 params, 256 ports, 64 labels, 256 bytes per label). It lives in `Shape.glyph: Option<GlyphState>` with a serde default.

*Masters and placement* (`crates/stencils/src/lib.rs`):
- `Master.glyph: Option<GlyphState>`.
- `master_from_glyph` builds a builder glyph from its defaults: `path` normalized to the unit box, `w` and `h` = extent × default module, `ports` from `Built`. This is what the palette and `place_shape` use.
- `Master::redraw` copies `glyph`.
- New `builders/apply.rs`: `pub fn apply(shape, built, module)` sets kind `Freeform`, the path in local coordinates, `w` and `h` (top left kept), `connection_points`, `glyph.ports` and `glyph.labels`.

*Resolution* (`builders/apply.rs`): `pub fn effective(shape, element, cat, variant) -> BTreeMap`. Later layers win:
1. spec defaults;
2. the pack glyph's `params` (the glyph comes from `master_for_type_in(type, variant)`, so a variant switches the standard through its glyph mapping);
3. the shape's `glyph.params`;
4. element properties whose keys the builder declares.

*Sync* (new `builders/sync.rs`): `pub fn rebuild(doc, before, cat) -> Vec<String>`. It reruns the builder only for shapes that are new, whose stencil or `glyph.params` changed, whose element's type or properties changed, or whose page `View.variant` or `module` changed. Values are written only when they differ, so a second run changes nothing. `Session::run` calls it right before `bind::sync`, inside the same guard and undo step. It runs before bind so that bind sees the new ports.

*Ports*:
- `bind::port`: when the glued shape has `glyph.ports`, port `i` is `glyph.ports[i]`.
- `ports::port_marks_in` marks those ports, taking the direction from the same port id on the type, else `inOut`.

*SVG*: `svg/glyph_labels.rs` (new) writes the slots upright, after the outline.

*Provenance*: `cargo xtask assets` (new `xtask/src/provenance.rs`) reads every `crates/notation/packs/*.json` with `serde_json`. Each glyph whose outline is `path` or `builder` needs a `source` that is "own design" or starts with "drawn from". It refuses sources containing traced, copied, Visio, Microsoft, AutoCAD, Autodesk, Adobe or Avid.

Commands (new `crates/engine/src/cmd/glyph.rs`):

| Command | Params | Returns |
|---|---|---|
| `glyph.builders` | `{builder?}` | specs with param schemas, standards and sources |
| `glyph.build` | `{builder or glyph, params?, module?}` | `{d, lines, svg, w, h, module, ports, labels, warnings}`; changes no document |
| `glyph.params` | `{id}` | `{builder, glyph, params: [{key, value, from: default, glyph, shape or element, spec}]}` |
| `glyph.setParams` | `{ids?, params: {key: value or null}}` | sets or clears shape overrides, checked against the spec; one undo step |
| `glyph.rebuild` | `{ids?, page?}` | redraws from current inputs (after a builder code change) |

`notation.load` refuses a pack that names an unknown builder. CLI: `vizcraft-cli glyph BUILDER [--param k=v]... [--module N]` prints the `glyph.build` SVG.

Done when:
- **Golden SVGs:** at least 3 cases per builder under `crates/stencils/testdata/builders/`, covering IEC and JIC contacts, a 4/3 valve with a spring, a 5/2 pneumatic valve, a distinctive 3-input NAND, and an ISO and an AWS weld.
- **Builder properties:** every builder's ports sit on whole module coordinates for every default and every standard; `build` gives byte-identical output twice; a proptest-style loop of 2000 random and junk param maps returns `Ok` or `Err` and never panics.
- **Loader:** loader tests for the new problems; `load_shipped` refuses a missing `source`.
- **Provenance check:** xtask tests for a missing, bad and good `source`.
- **Engine, on a test pack built in code (the `engtest` convention):**
  - a placed contact element's `element.setProperty poles 3` redraws it with 6 ports, and undo restores it in one step;
  - `variant.set` from IEC to JIC redraws the contact with the JIC glyph and its 0.1 inch size and keeps the poles;
  - a connector glued to point 3 binds to port `23`;
  - a second sync changes nothing;
  - the palette master's path matches `glyph.build` with defaults;
  - the bubble's slots appear in the page SVG.
- **Compatibility:** existing path-glyph and loader tests unchanged; the native round trip of `Shape.glyph` is tested in `native_tests.rs`; `hostile_params_never_panic` is green with junk `builder` and `params` entries.
- `cargo xtask ci` is green.

**(2) Own:**
- New: `crates/stencils/src/builders/{mod.rs, params.rs, apply.rs, sync.rs, contact.rs, coil.rs, bubble.rs, valve.rs, cylinder.rs, pump.rs, gate.rs, fcf.rs, weld.rs, texture.rs}`, `crates/stencils/testdata/builders/*.svg`, `crates/diagram/src/glyph_state.rs`, `crates/engine/src/cmd/glyph.rs`, `crates/formats/src/svg/glyph_labels.rs`, `xtask/src/provenance.rs`, `apps/vizcraft-cli/src/glyph.rs`.
- Existing:
  - `crates/stencils/src/lib.rs`: `Master.glyph`, `master_from_glyph`, `redraw`, the `pub mod builders` line, and `built: None` in the old-table test.
  - `crates/stencils/src/ports.rs`: marks from `glyph.ports`.
  - `crates/notation/src/glyph_path.rs`: none.

**(3) Shared (the hunk this branch adds):**
- `notation/src/schema.rs`: the three `Glyph` fields.
- `notation/src/loader.rs`: the `BUILDER_OUTLINE` swap in `finish`, a `builder_glyphs()` check, `load_shipped`.
- `notation/src/catalog.rs`: `builtin()` calls `load_shipped`.
- `notation/src/bind.rs`: the shape-port lookup in `port()` (`b6e-substrate` adds a net pass; no overlap).
- `diagram/src/model.rs`: the `Shape.glyph` field and its sanitize call (substrate adds `orientation`); `diagram/src/lib.rs` module line.
- `engine/src/session.rs`: one `rebuild` call line before `bind::sync`.
- `engine/src/cmd/mod.rs`: `pub mod glyph;` and its specs.
- `engine/src/cmd/notation.rs`: the builder-name check in `notation.load`.
- `engine/src/tests.rs`: two junk rows.
- `formats/src/svg.rs`: one call line in `shape_svg`, plus the module line.
- `formats/src/native_tests.rs`: one test.
- `xtask/src/main.rs`: `assets` runs `provenance::check`.
- `apps/vizcraft-cli/src/main.rs`: one match line and the usage text.

From `b6e-substrate` this branch reads `View.module` and `Shape.orientation`. Until those merge, builders take `module: None`, and label placement uses the unoriented frame through one helper, `glyph_labels::anchor(shape, label)`, so the merge changes one function.

**(4) Frontend half will need:**
- An Inspector "Symbol" group built from `glyph.params`. It writes through `element.setProperty` for keys with `from: element` or declared by the type, and through `glyph.setParams` otherwise, with editors from the spec kinds.
- The canvas drawing `Shape.glyph.labels` upright, the same rule as `glyph_labels.rs`.
- Palette previews from `Master.path`, which already works.
- A canvas call to `ports::port_marks`, unchanged.
- A Valve builder dialog over `glyph.builders` and `glyph.build`.
- A key or menu for `glyph.rebuild`.
- The `ui.inspect` symbol fields.

**(5) Risks and open questions:**
- **Rebuild anchor:** the redraw keeps the top left, so pins stay on the grid only if the shape was on the grid. Should it snap to the page module when one is set?
- **Resize:** a hand resize stretches the drawing until the next parameter change, which redraws at true size. Should builder shapes lock their aspect ratio instead?
- **Port ids:** builder port ids versus a type's declared ports. The proposal is that the builder wins on the shape and wave 2 packs declare matching ids; a stencils test over shipped packs checks they match.
- **Text-grammar params** (valve paths, datums, inversion mask) are terse for people. The Inspector may want structured editors later, which `glyph.params` already supports.
- **Merge order:** builders merge after substrate, so the `model.rs`, `bind.rs` and `session.rs` hunks get resolved at the merge. Substrate should expose `View.module` in page units, or say what unit it uses.
- **Hostile input:** a builder whose output exceeds the caps is truncated with a warning, never refused, so a hostile document still opens.
- **Unverified rules:** the forbidden-word list and the "drawn from" prefix are new rules; confirm the wording with the user.
- **ISO 1219 module:** its module value is unverified (`process-fluid.md` line 62), so 2.5 mm is assumed.
