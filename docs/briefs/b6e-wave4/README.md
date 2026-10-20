# Block 6E wave 4: how to run it

For the session that takes over the integration branch `claude/engineering-diagramming-standards-305d6b`
(worktree `.claude/worktrees/engineering-diagramming-standards-305d6b`). Waves 1 to 3, the
`b6e-frontend` drawing UI and `b6-doctypes` (document types, New page at launch, scoped palette)
are merged and `cargo xtask ci` is green. Read first: AGENTS.md,
[../owner-source-of-truth.md](../owner-source-of-truth.md) (the owner's rules and decisions; it
wins over any brief on intent), docs/cascade-plan.md "How Blocks 6 and 6E run" and "Block 6E wave
4", then the four briefs here. [lessons.md](lessons.md) lists the mistakes found while Block 6 was built and
the rule each one left behind; every wave 4 agent reads it before its UI work, and the integration
session uses its tests, snapshots and merge sections at each merge.

## Branches

| Brief | Branch (port) | Model |
|---|---|---|
| [b6e-electrical.md](b6e-electrical.md) | `b6e-electrical` (8074) | Opus, medium |
| [b6e-process.md](b6e-process.md) | `b6e-process` (8075) | Opus, medium |
| [b6e-fluidpower.md](b6e-fluidpower.md) | `b6e-fluidpower` (8076) | Opus, medium |
| [b6e-mechanical.md](b6e-mechanical.md) | `b6e-mechanical` (8077) | Opus, medium |

Each brief is a planning agent's draft headed by resolutions that override it. The owner's
decisions for this wave are in the resolutions (ladder and every type take the project default
IEC and ISO A3; P&ID defaults to ISO 10628; fluid power under JIC draws ANSI Y32.10; tags built
from properties recompose immediately).

## Procedure

1. Create each worktree from the integration HEAD:
   `git worktree add .claude/worktrees/<branch> -b <branch> HEAD` (from the integration worktree,
   using `../<branch>`).
2. Start electrical, process and fluid power as background subagents (Opus), one per branch, each
   told: work only in its worktree; `export CARGO_TARGET_DIR=target-<branch> CARGO_INCREMENTAL=0`;
   one build at a time; commit as it goes with prefix `B6E:`; no emoji, no AI attribution; do
   not edit ROADMAP.md, docs/parity.md, docs/mcp.md, docs/formats.md, docs/control-protocol.md or
   docs/cascade-plan.md; do not launch the GUI (the owner does visual checks); `cargo xtask ci`
   green before finishing; delete its target dir only when green and committed; its final message
   is the report the brief asks for, with an Owner checklist.
3. At most three builds at once on this machine (the disk filled once): start mechanical when one
   of the first three finishes.
4. Merge in the order electrical, process, fluid power, mechanical, with `git merge --no-ff`.
   Conflicts are append points (`Check` variants before `Unknown`, `SPECS`, `Arrow::EVERY`,
   `analyses()`, `cmd/mod.rs`, `icons.rs`, ribbon lists, catalog rows): keep both sides; an enum
   conflict may need a closing `},` between the two sides; check for a lost closing brace between
   adjacent `#[test]` functions. Shared generic pieces (`IdScope.numberBy`, `wire.draw {type}`,
   `ui.tool {line}`, `Port.netGroup`, `Check::NetCheck`) must end up defined once.
5. After each merge: `UPDATE_SNAPSHOTS=1 cargo test -p vizcraft-ui-egui`, re-bless goldens with
   `VIZCRAFT_BLESS=1` only after looking at the PNG exports, then `cargo xtask ci`
   (`CARGO_TARGET_DIR=target-eng-standards CARGO_INCREMENTAL=0`). Write the report's rows into
   ROADMAP.md, docs/parity.md, docs/mcp.md, docs/formats.md and docs/control-protocol.md, update
   the cascade plan's wave 4 State, commit, remove the worktree and branch and its target dir.
6. After all four: remove the "Engineering symbols (preview)" palette section in
   `crates/ui-egui/src/palette.rs` (the packs replace it), build the release app
   (`cargo build --release -p vizcraft -p vizcraft-cli`) and hand the owner one combined checklist
   from the four reports, with paste-ready Terminal commands (the owner is not a software
   engineer: say which window, what to paste and what they should see).

Then come `b6-c4` and `b6-code` (the software half of wave 4, briefs not yet written; scope in
docs/research/swe-diagramming-ux.md) and wave 5 (`b6e-ux`, pack ribbons, template preview snapshots).
