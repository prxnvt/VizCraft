# Block 6 lessons for the wave 4 engineering packs

These are the mistakes found while Block 6 was built (waves 1 to 3, `b6e-frontend` and `b6-doctypes`), each with the rule that came out of it. Read this before you add icons, connector kinds, ribbon groups, Inspector groups, dock tabs, dialogs, canvas tools or palette entries. Sources are in parentheses. Commit hashes refer to the integration branch. "Owner, date" means the owner's own words in a session.

## Icons

- Icons are drawn in code in `icons.rs`. The test `every_control_id_resolves_to_a_command` checks `icons::known` only on the tabs a default app shows (Home, Insert, Design, View, Help, Connector), so an unknown icon on a contextual or notation tab silently falls back to a letter tile. For every new tab or pack group, add a test that opens that page type and asserts each icon is known. (crates/ui-egui/tests/ribbon.rs)
- Generic icons already cover unrelated commands. `delete` is used by six ids (edit.delete, key.remove, lane.remove, model.delete, element.deleteFromView, datagraphic.apply). `close` is used by Clear Step, link.remove and a tool button, and `plus`, `refresh` and `settings` by three or four ids each. Reuse a generic icon only for the same verb. Every new kind of object (contact, coil, valve, pipe, terminal, instrument) gets its own drawn icon. (scan of crates/ui-egui/src/ribbon)
- A placeholder shipped and is still there: the Valve button's icon is an omega marked "to be replaced in `b6e-ux`". Draw the real symbol outline. If you cannot, list the placeholder in your report with its file and line, and do not leave it unmentioned. (docs/briefs/b6e-frontend/report.md; icons.rs around line 651)
- Fork and Join looked the same because UML draws both as one bar. The fix kept the standard bar on the canvas and drew the difference in the palette, picker and Insert icons (one flow in and several out, against several in and one out), with the type's description as a tooltip. When a standard draws two things alike, tell them apart in the icon and tooltip and never by changing the standard drawing. (owner, 2026-10-19: "Join and Fork are indistinguishable"; 94e1271)
- Palette, picker and ribbon gallery rows for path and builder glyphs used to draw a generic tile from `paint_kind`. Use `icons::paint_master`, which draws the master's real outline, so every pack symbol's row shows the symbol itself. (b6-gallery brief; 20c1d9b)
- Add new icon names as your own block at the end of `NAMES` (`// <Pack> > <Group>`) with their `paint` arms. Never reorder the list: icons.rs conflicted at every merge, and its hunks needed the HEAD side's closing brace restored. (wave 3 resolutions; memory, Block 4 merge lessons)
- Relationship and connector-kind rows draw a short line with the kind's end arrow. A new line kind (pilot, drain, pneumatic, instrument signal) should show its own dash and end in its row so it can be told apart without reading the label. (b6-ux brief "Row icons")

## Connector kinds and line ends

- On a notation page, a connector drawn by hand between two actions did not become a Control Flow, so the join rule counted 0 incoming flows. The Connector tool now creates the first relationship the rules allow (`suggest::connect`). A hand-drawn wire, pipe or signal line must likewise become the model object your rules count. Add a kittest for it. (owner, 2026-10-19; b6-ux resolution; c2ae6bf)
- Palette order: the chosen notation's own connector kinds come first. Other notations' kinds and the generic Line, Arrow, Two-Way, Dependency, Bus and Signal go in a collapsed "More connectors" group that is still searchable. With IEC, IEEE 315 and JIC (or ISO 10628 and ISA, or the ISO 1219 editions), the active variant's kinds lead. (owner, 2026-10-19: "They should be more out-of-the-way"; 8233951)
- A wire ran straight through a directional valve, because wave 1 exempted net segments from avoiding shapes. Now a wire drawn without bends routes around shapes and leaves each pin outward. A wire drawn with bends, or on a never-avoid page, keeps its bends. Route every new line kind through `wire.draw {type}` and this policy, and do not build a second router. (owner, 2026-10-19: "I need it to run *around* it"; 8994724)
- Two wires drawn one after the other lay on top of each other, because only connectors routed in the same pass were spread apart. `autoroute::refresh` now reroutes every connector whose run a new line lands on. Call `refresh`, and never route one connector on its own. (owner, 2026-10-19: "the two distinct wires are overlapping"; 5618982)
- Connectors ran along their own end shapes' edges and through other shapes. The standard is now: leave and enter perpendicular with a stub, keep a clearance, never share a run, fan out along a shared side, and keep labels off shapes and off other labels. `route_quality` enforces this over every example, so new examples must pass it. (owner, 2026-10-19: "they gotta branch out a little" and "Why are connectors and arrows moving through objects? Unreadable diagram"; fd14113)
- Layouts must leave room for routes. Chen attribute ovals sat on the relationship axis and pushed the line onto the box edges. In ladder rungs, P&ID line runs and block diagrams, keep tags, bubbles and notes off the wire axis. (b6-routing brief)
- Labels sat on their lines with the line running through them, and step badges sat on line labels. Sequence labels now sit above the line in the canvas and in every export. Label placement elsewhere is still the midpoint, so check that wire numbers, line numbers and flow labels clear junctions and other labels. (owner, 2026-10-19; 390492a; wave 3 State)
- The canvas did not draw wire numbers, line markers, table columns, ER halves, double outlines, label frames or step badges, while the SVG export did. Any new line look (dashed pilot, drain, capillary, pneumatic, electric signal) must be drawn by the canvas through the same diagram helper the SVG uses, with a canvas snapshot. (owner finding "Canvas did not draw wire numbers"; 8c101db; ef26375)
- New line ends go in `Arrow::EVERY` and the Arrows menu, and the pinned `ALL` test is kept. Tests that list arrows must follow the full list. (b6-polish resolutions; cb501dd)
- Junction dots were 3 line widths wide and hard to see at normal zoom, so they became 6. Junctions are shapes, so the exports follow automatically. Check new connection marks at 100% zoom. (owner finding; ca57f55)
- After `relationship.create` started drawing with the relationship's connector type, another pack's golden changed: the cloud Event lines turned dashed. Set every pack connector type's dash and ends correctly from the start, and re-check all goldens when a line look changes. (3432d37; wave 2 State)
- Once a click held a net's highlight, hovering any other wire still showed the held net, so wires joined at both ends looked as if they had no net. The item under the pointer wins over a held highlight. Apply the same rule to loop, pressure and signal traces. (owner, 2026-10-19: "wires that are alr connected on both ends don't rly show the Net"; ea3ce80)

## Ribbon

- A Sheet group on Design pushed that tab past 1280 points and failed the fit test, and a Symbols group did the same to Insert. Pack tools go in the notation tab through the one `pack_groups(pack_id)` match, or in a contextual tab, never on Home, Insert or Design. (docs/briefs/b6e-frontend/report-panes.md; memory, wave 4 decisions)
- Fit tests exist for Home, Design, Insert, Process, Generate, View, Developer, Review, Picture, Ink, Connector and Sequence, but none was found for the notation tabs or the Sheet tab. Add a 1280-wide fit test for each pack's notation tab with all its groups showing. (crates/ui-egui/tests scan)
- The owner wants small settings as Small buttons in stacks, not Big buttons: "For simple settings like Orientation, make them smaller and put them in stacks (why should this have to be a large button?)". Keep Big for a group's main tool. (owner, 2026-10-18)
- In every drop-down, the current choice shows a check mark on the right (`MenuEntry.checked`). This applies to Variant, symbol standard, edition, sheet size and line kind menus. (owner, 2026-10-18; docs/architecture.md ribbon/mod.rs)
- Every control id must resolve to a registry command or a `UI_COMMANDS` row with a label. Tool buttons are `ui.tool` with params and a checked state. (crates/ui-egui/tests/ribbon.rs)
- Ribbon controls cannot be disabled per params, so Upstream with nothing selected errored into the status bar. Give commands an `enabled` hook that returns a reason, so the button dims instead of failing. (b6-analyze-ui brief, risks)
- The Process tab repeated the notation tab's Diagram Validation group and was trimmed on notation pages. Never put the same group on two tabs that are visible at once. (58dea99)
- The Steps tool button was labelled "Steps", the same as the flowchart Steps insert menu, so it was renamed "Steps Tool". Labels must be unique wherever they show together, both for the user and for kittest's `get_by_label`. (ebb2155)
- Re-Layout is a split button that lists the notation's own layouts first. Register your pack's layouts so they lead that list. (ed258ff)
- One gesture is one undo step. Use `edit.batch {label}` for multi-command actions, as Fix ends and the operand-separator drag do. Two known violations remain, Tab plus a typed name and dropping a device on a wire; do not add more. (b6-ux brief; 3eb43bc; owner-source-of-truth section 6)
- The engineering key commands are in the command palette but in no menu, so they are hard to find. Give each new tool and key command a menu entry too. (docs/briefs/b6e-frontend/report.md)

## Panes, Inspector, dock tabs

- Double-clicking a class box's text edited the drawn text, and the next sync overwrote it. Canvas and Inspector edits must write the model property the text is built from. For tags, terminal numbers, wire and line numbers, the property changes and the text is recomposed. (owner-source-of-truth 3.6; ff37d2b)
- Repeating rows are edited in a grid with row commands. ER columns use a grid (Name, Type, PK, FK, Null, Unique) with Add column, and UML members use `uml.addMember`, `setMember`, `removeMember` and `moveMember`. Do the same for terminals, cable cores, I/O points and FMEA rows, one undo step per row edit. (a54d246; 4270d8c)
- The Inspector called `glyph.params` on every selected shape. On a shape no builder draws, the error waited in `Session.status` and appeared after the next command, naming a shape that was no longer selected, and twelve snapshots had it baked in. A pane may only query where the query applies, and a read must never leave an error behind. (owner finding; ba76f90)
- `ui.editText` with nothing to edit raised an error. It now shows a status hint. When a request has nothing to act on, show a hint, not an error. (7e1bd39)
- Two branches' Inspector sections both matched a sequence message (Relationship and Sequence), and this was found only at the merge. Where your pack group edits an object, hide the generic section that would edit it again. (0a71d8a)
- Put the pack's most-used group first: symbol parameters lead the Inspector. Show title block fields under readable names, not raw keys. (88fa73b)
- The Inspector caches `glyph.params`, `net.of` and `doc.properties` per revision. Never run engine queries every frame: the owner's first complaint about the app was lag. (report-panes.md; owner, 2026-10-18: "It's so laggy")
- Pane contents must fit a narrow pane: the Sequence group and the gallery's detail combo boxes had to be resized. Test at the default pane width. (390492a; 3ee5d02)
- Users saw ids instead of names. Analysis metrics listed parallel branches as "e8" and "e10", and the Nets tab shows a net's id when it has no name. Rows in dock tabs, lists and CLI output show names. (owner's pasted CLI output, 2026-10-19; 18b58b9; report.md)
- Every dock tab row offers Go to, as the Nets tab (segments and junction) and the Analysis tab (Go to, Select, Re-run, Clear) do. When a target is missing, say why in the status bar. (report.md; b6-analyze-ui brief)
- Tables drew wrong: ER tables had blank lines above the key and column rows, unaligned columns, a clipped `varchar(255` and two header rows. The title block's Size and Sheet values crowded their captions, and changing the title block pushed key text flush against the cell rule. Lists built on `title_block::Block` (wire, cable, terminal, parts and line lists) need one header row, aligned columns, `fit_cell` with an ellipsis and the left inset. (owner, 2026-10-19: "newlines in the schma's objects aren't displaying right"; ef26375; fcc4237; f4d0d75)
- A generated frame page appeared as its own page tab ("Wiring Background"). Hide generated pages from the page tabs. (owner finding; e1b85d9)
- Analysis dimming washed out the container around a marked item. Dimming and halos must leave the marked item's surroundings readable. (5735667)
- Adding a pane takes one line in `default_panes!` and a `ui.pane.<key>` row. The pane lists in `panes/layout.rs` and the counts in `tests/panes.rs` conflicted at every merge, so expect to keep both sides. (docs/architecture.md; memory, Block 4)

## Dialogs and the New page

- Programmatic calls never open dialogs. `sheet.revise` with no description pushes `{"open": "revise"}` to `ui_requests`, changes nothing and returns nulls, and `ui.dialog` names the dialog. Copy this for terminal strip, cable, loop and valve-style dialogs. (report-panes.md)
- Parametric symbol dialogs show a live preview from `glyph.build`, as the Valve dialog does, and place the symbol through `glyph.insert`. (88fa73b; report-panes.md)
- The owner: "templates should be more like document types ... nobody wants pre-made things to work around". A document type sets the viewpoint, notation or standard, sheet, title block, module and rulesets and draws no content. Filled samples go in `Pack.examples` under Help > Examples. (owner, 2026-10-19; da5ab75)
- The owner asked "Where's the Mechanical and Electrical engineering ones?" when the New page had none. Wave 4 adds types under Engineering (Electrical, Mechanical, Process, Fluid power). Checklists must say plainly what is not built yet. (owner, 2026-10-19)
- The app launches on the New page unless it is opened with files. Each file on the command line opens in its own window; before the fix, only the first one opened. (owner, 2026-10-19: "It only opened the class diagram even tho I ran the entire copy paste"; 20c1d9b; 8233951)
- Standards are offered everywhere a standard is chosen: the New page standards bar, Page Setup, the Variant menu, `variant.set` and `sheet.set`. ASME sits beside ISO, and `ansi.` is accepted for `asme.`. (owner rule 1; ea72e6c)
- `template.new` without standards wrote ER notation ids (`ie`, `chen`) into `Document.standards.symbols`. Keep notation variant ids and symbol families apart: only `SYMBOL_CHOICES` families go into `symbols`. (b6-gallery resolution; a710500)
- A New page card shows what the type gives (the empty sheet with its frame and title block, and a row of the palette's first shapes), not a filled diagram. Failed previews show "No preview". On an error the gallery stays open and the document is unchanged, because the engine does a dry run first. (b6-doctypes brief; 3ee5d02; b6-gallery brief)
- The engineering workspace preset is offered, never forced: an unset preset keeps the office look. (b6-gallery resolution)
- `sheet.set` does not set a module, so on a fresh sheet the Wire tool snapped to nothing until `page.setModule` ran. Each engineering document type must set its module explicitly. (b6e-frontend report)

## Canvas tools and keys

- Bare-letter keys fire only while the canvas has the keyboard (`keys::bare_letters_ok`: no widget focused, no text being edited). Each one is a `ui.*` command in the palette. A kittest proves the key stays silent in a text field. New pack keys go through the same gate and must not take R, X, Y, W, P, `/`, Tab or Enter. (b6e-frontend brief; crates/ui-egui/src/keys.rs; wires.rs tests)
- Escape returns to the pointer from every tool. Test it for each new tool (ladder, line kinds, Steps-like tools). (owner-source-of-truth 3.3; report-canvas.md)
- Decide what Tab and Enter do on each new page kind. They do nothing on a sequence page (`notation_layout::sequence_page`), so give ladder and P&ID pages an explicit behaviour too, even if that is nothing. (b6-sequence-ui resolution)
- Alt+arrow moves and other modified keys also stay out of text fields. (a384155)
- Dropping a device on a wire splices it in, but as two undo steps (insert, then splice). The UI check for "on a wire" is coarse: the engine refuses a bad fit and the status bar says why. Do not copy the two-step pattern; wrap new drop actions in one batch. (report-canvas.md)
- Moving a shape does not snap to the module grid yet. `glyph.insert` snaps the first pin through the shape's transform, which was wrong after a quarter turn until it was fixed at merge. Test that turned and mirrored symbols land their pins on the grid. (report.md; 213bf38)
- R, X and Y keep text upright and glued wires follow their pins. `shape.rotate` and `shape.mirror` used to report the old selection as `primary`. Command results must name the shapes they changed. (report.md; ca57f55)
- Activation bars were position-locked, so dragging one only printed "locked". Clicks and drags on generated parts now go to their owner (stem and bar hits pick the lifeline), and fragments, bars and marks show no connection points. Decide hits and connection points for frames, title blocks, rails, junctions and generated tags. (b6-sequence-ui brief; a384155; 390492a)
- A drag previews in UI state and commits one command on release. Sequence previews come from the settle pass run on a copy, "so what you see while dragging is what you get". Rerouting during a drag uses the same clearance within `PREVIEW_BUDGET`. (docs/architecture.md; b6-sequence-ui brief; b6-routing brief)
- The net highlight covers the active page only. Off-page connectors and cross references need their partner shown or reachable. (report.md)

## Palette

- The owner: "We can't give an entire left-side pane to just basic polygons, this isn't for kids" (2026-10-17), and later "Get the Drawing section out of the left sidebar ... It's annoying and juvenile" (2026-10-20). No Basic primitives appear in the Palette pane for any document type. They stay reachable through search, the `/` picker and Insert > Basic Shapes. (eada009)
- The palette shows the page's viewpoint and active variant. Switching the variant updates the pane, the notation tab's Insert menus and the `/` picker. Other packs' shapes stay behind "Only this viewpoint". Kittests cover the switch. (03c9a4b; 8233951)
- The new sheet pack added an unexpected "Drawing Sheet" section to the Shapes pane and broke snapshots. Its agent deleted the pack's palette and viewpoints to get green tests, and `b6e-frontend` had to add them back. Never remove pack features to make snapshots pass: retake the snapshots and list them. (integration session transcript; e1b85d9)
- `uml.Decision` was likewise left out of the palette to keep a search test green, and had to be put back. Scope the test, never the product. (78fc720; a016492)
- Labels must be unique in search. "Decision" clashed with the flowchart one, and the cloud pack's "Cloud", "Group" and "Container" clashed with app buttons (renamed "Cloud Boundary", "Loose Group", "Container Service"). Repeated rows name their pack. Name kinds specifically ("Pilot Line", not "Line"). (78fc720; 9178a69; a016492)
- Palette rows carry the type's description as a tooltip. (94e1271)
- When the four packs are merged, remove the "Engineering symbols (preview)" section in `palette.rs`. Do not let the builders appear twice. (docs/briefs/b6e-wave4/README.md step 6)

## Engine and model

- Every pass that runs in sync is idempotent: test that a second sync changes nothing. A pass that removes ids must prune the selection, because the selection may still hold a removed id. (b6e-substrate brief risks; b6e-electrical and b6e-process briefs)
- Every new field has serde defaults, is lenient, sanitised and skipped when default. Old files must load and save byte-identical; the substrate merge compared SVG and draw.io exports against the base commit. (b6e-substrate brief; integration session transcript)
- `hostile_params_never_panic` must cover every new command id. Sheet, document property and cross-reference commands needed a follow-up commit for this. (5d34097; wave 3 resolutions)
- Commands that move or change shapes call `layer::ensure_unlocked` first, as container, addon and the other commands do. (crates/engine/src/cmd/*.rs)
- `Fragment` paste remaps only glue and owners. Pasted sequence fragments lose their part. Any new id-valued field (net, loop, line, tag owner, cross-reference partner) must be remapped in `fragment.rs` or rebuilt by sync, with a copy-paste test. (crates/engine/src/fragment.rs; b6-sequence brief)
- There is one document title. Two keys disagreed (the Inspector read `properties.title`, while `doc.setProperty` wrote the ISO 7200 field), so now every writer sets both and the property wins on load. Give any value shown in two places (tag, designation, line number) one canonical key. (owner finding; f4d0d75)
- Problems messages state only the problem ("Date of issue is empty."). Only the sheet pass was fixed; other packs still put the rule description first. Write your pack's messages as findings. (owner finding; report-panes.md)
- Analyses never make an undo step and never dirty the document; Compare is an overlay, not a generated page. Loop trace, pressure trace, cut sets and the electrical rule check follow the same rule. (b6-analyze-ui brief)
- Examples are test fixtures: `route_quality` and the preview goldens run over every example, so each new example must pass them. (fd14113; b6-doctypes brief)
- Contract seams broke at merge. Builders read a placeholder "no grid" until they were wired to the substrate's module, and `sheet.set` ignored `Document.standards`. Shared generic pieces (`IdScope.numberBy`, `wire.draw {type}`, `ui.tool {line}`, `Port.netGroup`, `Check::NetCheck`) must end up defined once, with one owner. (wave 1 State; docs/briefs/b6e-wave4/README.md)
- No command could change an element's type, so the Fork and Join rules had no fix. If a pack rule needs a fix that retypes something, add the command or report the gap. (wave 3 State)
- Keep command results compact, with ids and model facts and no styling. The owner does not want MCP clients "forced to ingest information about colors and fonts". (owner, 2026-10-18)
- Tags built from properties recompose immediately and keep their number. Page, zone and rung parts renumber on demand. (owner rule 12)

## Standards and optionality

- "Optionality is the biggest part of this application. IEC and ISO A3 can be default but the user should be able to choose ANSI and JIC." Every standard, notation, layout or placement choice has a default and stays switchable. (owner, 2026-10-19)
- Switching a standard redraws in place as one undo step, and nothing moves. ISO A3 to ASME B kept the title block's field values, and the ER Variant switch moves no shape. Test that positions stay the same. (report.md; owner-source-of-truth 3.8)
- Tuning values belong to the user. The routing clearance became a page option (`page.setRouting {clearance}`, default the module or 10) shown in Design > Connectors, not a constant. (b6-routing brief)
- Wave 4 defaults: a ladder takes the project default (IEC, ISO A3). P&ID defaults to ISO 10628 with ISA-5.1 one click away. JIC fluid power draws in the ANSI Y32.10 style. (owner rule 11)
- Mixed symbol sets on one sheet are reported by a rule, not blocked. (cascade plan, Block 6E decisions)
- Nothing becomes the silent default: the engineering preset and the standards are offered, never imposed. (b6-gallery resolution)
- When a source cannot be checked, say so. The ASME zone data is still unverified because no public MIL-STD-100G text was found. Glyphs carry a mandatory `source` and are drawn only from written descriptions and US government public domain documents. (wave 3 State; Block 6E decisions)

## Tests and snapshots

- Every UI branch changes the shared snapshot PNGs. One new palette row retook 37 of them, and 39 conflicted only because both halves of `b6e-frontend` retook them. Retake only the PNGs your change alters and list them. The integration session regenerates once after the merge with `UPDATE_SNAPSHOTS=1` over all ui-egui tests. (report-canvas.md; report.md; wave 3 resolutions; memory)
- Look at every PNG before blessing it. Twelve snapshots were blessed with a stray status-bar error in them, and the key goldens lost their text inset without anyone noticing until the merge. (ba76f90; b6-polish resolution)
- Re-bless goldens with `VIZCRAFT_BLESS=1` only after looking at the PNG exports, and report one line per family saying what changed. (docs/briefs/b6e-wave4/README.md; b6-routing brief)
- Count tests break when a sibling adds a pack. The connector kind count (7 to 8) conflicted, and the stencils search count of 22 had to become a prefix filter with the glyph-set list taken from the catalog. Write counts as per-pack filters. (report.md; 9178a69; memory, wave 4 decisions)
- Add kittests for palette scoping, for keys staying silent in text fields and for one undo step per gesture. Regression tests should fail on the old code, as `hovering_another_wire_shows_its_net_while_one_is_held` does. (03c9a4b; report.md)
- Snapshots never run on the CI runner (`VIZCRAFT_SKIP_SNAPSHOTS=1`, and with no wgpu adapter they pass with a note), so only a local macOS run catches them. Run `--test snapshots` locally before reporting green. (docs/testing.md; CI workflow 9912127)
- Failed runs leave `*.new.png` and `*.diff.png` files in `tests/snapshots/macos`. They are gitignored, but delete them before you report. Keep the whole set under 3 MB at 1 pixel per point. (integration session transcript; docs/testing.md)
- kittest `get_by_label` fails when two widgets share a label, so a label clash breaks tests as well as confusing users. (ebb2155)
- Once the tests pass, run the fuzzer or stress tester over the new commands. (owner, 2026-10-20: "use fuzzer when tests pass")

## Merge and build hygiene

- Shared lists are append points: `Check` variants before `Unknown`, `SPECS`, `Arrow::EVERY`, `analyses()`, `cmd/mod.rs`, `icons.rs` NAMES, ribbon lists, catalog rows, `UI_COMMANDS`, `ui_command` arms, `inspect` keys and dock tabs. Append in your own block, never reorder, and keep both sides on conflict. (docs/briefs/b6e-wave4/README.md; wave 3 resolutions)
- A closing brace went missing between adjacent `#[test]` functions in three places in wave 1 and two lists in wave 2. After resolving conflicts, compile the tests before starting CI. (integration session transcript)
- Enum conflicts end with both sides partway through a variant, so they need a closing `},` between the two sides. This hit schema.rs's `Check` enum twice in wave 2. (wave 2 State)
- `dialogs.rs`, `canvas/mod.rs` and `icons.rs` take multi-line hunks despite the one-line rule, and their conflicts need the HEAD side's closing brace restored. (memory, Block 2 and Block 4 merge lessons)
- Do not renumber shared numbered lists. Renumbered sync steps in `bind.rs` doc comments merged with broken indentation and needed a fix commit. Add new steps at the end. (3ccccab)
- Run `cargo fmt` after every conflict resolution: two merges needed a rustfmt commit afterwards. (5f55744; 3e6cd7c)
- The same fix commit landed three times on parallel lines (3432d37, 6cab175 and da0c39e; also 3ccccab, 44d656b and 6689f2b). To give a running branch an integration fix, merge the integration HEAD into it rather than committing the fix again.
- The disk filled while five builds ran. Agents hit ENOSPC in the middle of CI, could not delete their own target or write their report, and `b6e-sheets` had nothing committed. Commit as you go, run at most three builds at once, use your own `CARGO_TARGET_DIR` with `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0`, and delete the target dir once green and committed. (integration session transcript; memory, wave 4)
- The GitHub runner ran out of disk on the debug test build, and CI was fixed by stripping debug info and deleting preinstalled toolchains. Do not add heavy dev-dependencies or turn debug info back on. (9912127)
- Briefs drafted against another worktree had stale paths and line numbers. Re-check them against your checkout before editing. (wave 3 resolutions)
- `b6e-frontend` changed `crates/routing`, which it did not own, and the integration session had to review it. If you must touch a file another branch owns, keep the change small and name it in the report. (report.md)

## Owner hand-off

- "How tf do I open it? I'm not a SWE ok give me commands for things and where to run them ffs." Every check gives the `cd` path, one exact command per line, which window to paste it into and what they should see. (owner, 2026-10-19)
- A multi-line command wrapped when pasted and ran as two ("zsh: command not found: ci.vizcraft"). Give single-line commands with no continuations. (owner's pasted terminal output, 2026-10-19)
- SVG previews opened in another app, VectorCraft, because it handles `.svg` by default, and the owner asked "lmao what am I looking at". Hand over `.vizcraft` files opened with the built VizCraft binary, or PNGs, and say what the picture shows. (owner, 2026-10-19)
- The wave 1 CLI checkpoint left the owner lost ("I got no idea what's going on tbh"). Prefer in-app checks on files you have already built. (owner, 2026-10-19)
- "Build my diagrams for me initially so I can visually verify and drag stuff around and see what breaks." Generate example files with `vizcraft-cli` into one folder and list their paths. (owner, 2026-10-20)
- "Ight lets get the dir and command again cz I closed everything" and "Where do I access the images?" Repeat the full launch block in every hand-off, and give absolute paths with an `open` command. (owner, 2026-10-19)
- "Yk I can do visual tests myself right?" Agents skip in-app walkthroughs and end with an Owner checklist of at most 10 steps, each one action and one expected sight, written for a non-engineer. (owner, 2026-10-19; memory)
- Say up front what is not built yet, so a missing feature is not taken for a bug, as the missing Electrical and Mechanical types were. (owner, 2026-10-19)

## Sources read

- docs/briefs/owner-source-of-truth.md (sections 1 to 6)
- docs/cascade-plan.md: Block 6, Block 6E, How Blocks 6 and 6E run, wave 1 contracts, wave 2, wave 3, wave 4
- docs/briefs/b6-wave3/{b6-ux,b6-polish,b6-routing,b6-sequence-ui,b6-gallery,b6-analyze-ui}.md (resolutions and selected sections)
- docs/briefs/b6e-frontend/{report,report-canvas,report-panes}.md; docs/briefs/b6-doctypes/brief.md; docs/briefs/b6e-wave4/README.md; grep of b6-wave1 and b6-wave2 briefs
- git log 48d62ac..b04c20c (153 commits) with `git show --stat` or small diffs for ebb2155, 78fc720, 9178a69, 0a71d8a, 7e1bd39, 3ee5d02, eada009, 3432d37, 6cab175, 3ccccab and others
- docs/testing.md (snapshot tests), docs/architecture.md (UI commands, ribbon and pane tables), AGENTS.md (grep)
- crates/ui-egui/tests/ribbon.rs, crates/ui-egui/src/keys.rs, icons.rs, the ribbon icon usage scan, crates/engine/src/fragment.rs and cmd/edit.rs
- Owner turns from session transcripts of 2026-10-17 to 2026-10-20 in the VizCraft, b6e-frontend and integration-worktree project folders, and assistant notes on merges and disk exhaustion
- Memory files vizcraft-status.md (merge lessons, wave 4 decisions), owner-test-handoff.md, owner-does-visual-checks.md
