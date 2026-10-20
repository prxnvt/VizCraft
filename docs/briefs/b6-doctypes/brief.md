# `b6-doctypes`: document types, the New page at launch, notation-scoped palette

Opus, medium effort. Port 8063. Worktree `.claude/worktrees/b6-doctypes`, branch `b6-doctypes` from
the integration branch at 7ce4c86 (wave 3 done). `CARGO_TARGET_DIR=target-b6-doctypes
CARGO_INCREMENTAL=0`. Commit prefix `B6:`. Written by the integration session from the owner's
review of wave 3 on 2026-10-19; there is no planning-agent draft, so read the code first and make
the design calls this brief leaves to you, recording each in the report.

Read first: AGENTS.md; this brief; `/Users/pranav/Desktop/VizCraft-Blocks-6-6E-source-of-truth.md`
(the owner's rules; optionality first); `docs/briefs/b6-wave3/b6-gallery.md` and the code it built
(`crates/ui-egui/src/gallery.rs`, `gallery_previews.rs`, `preset.rs`); `crates/notation/src/template.rs`;
`crates/engine/src/cmd/template.rs`; `crates/ui-egui/src/panes/shapes.rs` and `palette.rs`.

## The owner's words

- "The app should launch with the New document page."
- "Templates should be more like document types, the differences are in which shapes are presented
  as defaults and compliance-related stuff, nobody wants pre-made things to work around in the name
  of templates."
- On examples (answered): the filled-in samples move to Help > Examples; New offers only empty
  document types; the test suite keeps using the samples.
- "If I select a standard of Entity Relationship Diagram, why are the other standards' types of
  connectors present in the left-hand sidebar? They should be more out-of-the-way."

## Scope

### 1. Document types (engine and packs)

- A pack's `templates` become **document types**: an empty document set up for a kind of work. A
  document type sets the page's viewpoint (so the palette shows its shapes), the default notation
  variant or symbol standard, the sheet, title block and zones when the discipline uses them, the
  symbol grid module, the compliance rules that run (rulesets, live checking on or off), the
  insertion direction, whether a key is kept, and its code forms. It draws **no content**: no
  seed steps on a new document. A title block and frame are setup, not content, so they stay.
- The current seeded templates become **examples**: a separate pack list (`examples`, same shape
  as today's templates, seeds and layouts kept) so nothing is lost and every golden and test that
  uses them keeps working. Choose the cleanest representation and say why: a new `Pack.examples`
  array, or a `kind: "type" | "example"` field. The template preview goldens and the
  route-quality test keep running over the examples.
- Document types per pack, reviewed: drop duplicates that differ only in content (for example ER
  `schema`, `logical`, `conceptual` are one Entity Relationship type whose viewpoint is a choice;
  a DFD context diagram and a threat model differ in viewpoint and rules, so they may stay two).
  Name them as document kinds a person recognises ("Class Diagram", "Entity Relationship
  Diagram", "Sequence Diagram", "Cloud Architecture", "Network Diagram (physical)", "Data Flow
  Diagram", "Flowchart", "Engineering Sheet" for the sheet pack). Every choice the type implies
  (viewpoint, notation, sheet, rules) stays changeable at creation and afterwards.
- Add what a type needs and the schema lacks: at least the ruleset ids it turns on (compliance),
  with loader checks. Keep `template.*` command ids working (agents and the MCP docs use them):
  `template.new` makes an empty document of a type; add `example.list` and `example.open` (or
  `template.new {example: true}`, your call) for the samples. `vizcraft-cli new` follows.
- Compatibility: older documents are unaffected (templates only act at creation).

### 2. The New document page at launch

- Launching the app with no file to open shows File > New (the gallery) instead of a blank
  page. Opening files from the command line or Finder goes straight to them. Escape or Blank
  Drawing gives the blank page as today. Kittest for both paths; the `apps/vizcraft` startup
  hunk is allowed.
- The gallery lists document types, grouped by discipline (Software, Data, Infrastructure,
  General, and Engineering once wave 4 packs add Electrical, Mechanical, Process and Fluid power).
  A card's preview shows what the type gives: the empty sheet or page with its frame and title
  block if any, and a row of the shapes its palette offers first. It does not show a filled diagram.
- Keep the standards bar (symbols IEC default, ANSI, JIC, others; sheet ISO A3 default, ASME
  sizes), the notation and sheet choice in the details panel, Recent, search and the unsaved
  notice. Add Page from a type works the same.

### 3. Help > Examples

- A Help menu and Help tab entry "Examples..." opens the same gallery in an examples mode listing
  the samples with their filled previews; opening one creates a new document from it (one undo
  step, the unsaved notice applies). `ui.gallery {mode: "examples"}`.

### 4. A palette scoped to the chosen notation

- The left Shapes pane shows, for the active page, only the element types and connector kinds
  that belong to its viewpoint **and its active notation variant** (`Pack.variants[].connectors`
  maps relationship types to connector kinds; types a variant draws differently are the same
  type). Connector kinds of the pack's other notations (in ER: the Chen, Barker and IDEF1X
  relationships while IE is chosen) and the generic drawing connectors (Line, Arrow, Two-Way,
  Dependency, Bus, Signal) move to a collapsed group at the bottom, "More connectors" (or two
  groups, other notations and general drawing; your call), still searchable and usable. Switching
  the variant updates the pane. The same scoping applies to the notation tab's Insert menus and
  the `/` picker's shape rows.
- Shapes from other packs stay behind the existing "Only this viewpoint" toggle as today.

## Rules

- One cargo build at a time from your session; commit as you go; no emoji and no AI attribution.
- Do not edit ROADMAP.md, docs/parity.md, docs/mcp.md, docs/formats.md, docs/control-protocol.md
  or docs/cascade-plan.md; put their rows in the report. Do not merge, rebase, push, or touch other
  worktrees.
- The owner does purely visual and few-click checks by hand: do not launch the GUI to look. Keep
  every automated test (kittest, snapshots retaken and listed, goldens re-blessed after looking at
  the PNG exports, engine tests, `cargo xtask ci` green).
- Wave 4 (electrical, process, fluid power and mechanical packs) is built after this branch on
  the document-type format you define, so document it clearly in `crates/notation/src/template.rs`
  and in the report: what a pack author writes for a new document type.

## Report (your final message)

Commits; the document-type and example format with one worked example of each in JSON; the list of
document types per pack after the review (and what was folded or dropped); command ids added or
changed; shared-file hunks; snapshots and goldens changed; rows for the docs you may not edit; an
**Owner checklist** of at most 10 steps (one action and what they should see each, for a
non-engineer); follow-ups.
