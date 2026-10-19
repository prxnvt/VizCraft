# `b6-routing` (connector routing and layout quality), Block 6 wave 3

Opus, medium effort. Port 8060. Worktree `.claude/worktrees/b6-routing`, branch `b6-routing` from
the integration branch `claude/engineering-diagramming-standards-305d6b` after `b6e-frontend`
merges. `CARGO_TARGET_DIR=target-b6-routing CARGO_INCREMENTAL=0`. Commit prefix `B6:`. Screenshots
under `/private/tmp/claude-501/b6-routing/`; the GUI only on the secondary monitor
(`--window-pos -2500,100`), quit through `app.quit`, checked with `pgrep -fl vizcraft`; prefer the
CLI (`vizcraft-cli new --template ID --svg OUT.svg OUT.vizcraft`) and PNG export to look at results.

Written by the integration session from the user's own look at the wave 2 templates in the app on
2026-10-19 (no planning-agent draft). Read AGENTS.md, then the common resolutions of any other
wave 3 brief in this folder (they apply here too: shared-file rules, merge order, one build at a
time, the report as your final message, no edits to ROADMAP, parity, mcp, formats,
control-protocol or the cascade plan).

## Goal

Connectors that read cleanly. The user's words: "I don't want connectors/wires to align with the
boxes they're from, they gotta branch out a little" and "Why are connectors and arrows moving
through objects? Unreadable diagram". Every shipped template, laid out and routed by the engine,
draws with no connector running through or along a shape, no two connectors sharing a run, and
no label sitting on a shape or another label; and a person dragging shapes gets the same quality.

## What the user saw (reproduce each first, from the CLI, and keep the PNGs)

1. `er.conceptual` (Chen): the "places" relationship between Customer and Order runs straight down
   the right edges of both entity boxes, collinear with them, with its "1" and "N" labels in the
   corners. The attribute ellipses `name` and `orderNo` sit on the axis between the two entities,
   so the relationship has nowhere to go but along the box edges.
2. `uml.activity`: the flows from Pack goods and Send invoice to the join leave from the facing
   sides of the two actions and run down the same vertical into the join, overlapping.
3. `dfd.threat` (moved here from `b6-polish`): two-way flows share runs, both external entities
   land on one side of the trust boundary, the Gane-Sarson process and open store get no fan-out
   because `autoroute::spread_of` returns 0 for Freeform shapes.
4. Every other template, checked by the test below.

## Scope

- **A route quality checker** (`crates/diagram/src/route_quality.rs`, new, pure): for a page,
  report each connector segment that passes within the clearance of a shape that is not one of
  its ends; each segment that runs along an edge of its own end shape (collinear within the
  clearance for more than a short stub) instead of leaving it; each pair of connectors that share a
  collinear run longer than the clearance; each label box that overlaps a shape or another label.
  A test runs it over every shipped template (`template.list`) and fails on any finding, listing
  them; this is the regression net for the whole branch.
- **Stand-off from shapes.** The router keeps a clearance from every shape that is not an end
  (`autoroute.rs` `MARGIN`, `KEEP_CLEARANCE`, and the obstacle inflation in `crates/routing`), and a
  route leaves and enters an end shape perpendicular to the side it attaches to, with a first and
  last stub at least the clearance long, never running along that shape's edge. The straight-pair
  shortcut (`straight_pair`) is taken only when the straight line also meets the stand-off. The
  clearance is a page routing option with a default (the page module when one is set, else
  10 units), a field of `RoutingPolicy` (`crates/diagram/src/routing_policy.rs`, `Page.routing`)
  set through `page.setRouting {clearance}` and shown in Design > Connectors with the other policy
  fields; it is the user's choice, not a constant.
- **Fan-out at shared sides.** Connectors that meet the same side of a shape spread along it in
  order of where their other ends lie, so they do not cross each other at the side; `spread_of`
  handles Freeform shapes whose outline has a straight run along that side (from `b6-polish`'s
  draft: a straight segment along the side through each frame-edge connection point at least
  `±spread` long). Flows into a fork or join bar enter from its long side, spread.
- **Layout primitives that leave room for routes.**
  - ER Chen (`crates/layout/src/primitives/er.rs`): attributes go on the ring around their entity
    away from the sides its relationships use, so the relationship axis between two entities stays
    clear; the relationship diamond (the connector label frame) sits on that axis.
  - Nested containers (`container.rs` `nested()`, from `b6-polish`'s draft): drop a lifted edge
    whose reverse is already lifted in the same scope, so two-way pairs do not decide the side.
  - Activity (layered with fork and join): the actions between a fork and its join line up so
    their flows run straight down into the bar.
  - Check the template pipeline ends with a whole-page route after the layout, so connectors are
    spread together; add it in `cmd/template.rs` if missing.
- **Interactive routing keeps the rules**: `autoroute::preview` and `refresh` during a drag use the
  same stand-off, within `PREVIEW_BUDGET`.

## Own

`crates/diagram/src/route_quality.rs`, `crates/diagram/src/autoroute.rs`, `crates/routing/`,
`crates/layout/src/primitives/{er.rs,container.rs}` and the layered fork and join handling, the
re-blessed goldens, the `dfd.threat` template in `dfd.json` if its layout options change.

## Shared, with the hunk this branch adds

- `crates/diagram/src/lib.rs`: the module line.
- `crates/diagram/src/routing_policy.rs`: the clearance field, serde default, sanitised.
- `crates/engine/src/cmd/routing.rs`: `clearance` in `page.setRouting`, with a test; the Design >
  Connectors routing control gains the field (one hunk in its ribbon file).
- `crates/engine/src/cmd/template.rs`: the final route, only if missing.
- Goldens: every engine golden and ui-egui snapshot whose routes change, re-blessed with
  `VIZCRAFT_BLESS=1` and `UPDATE_SNAPSHOTS=1` after you have looked at each diff. Many will
  change; list them and say in one line per family what changed.

## Tests

- The template-wide quality test above, green.
- Unit tests in `autoroute.rs` and `crates/routing`: a straight line between two aligned boxes
  with a third box beside the axis keeps the clearance; a route between two boxes stacked with
  their right edges aligned leaves from the bottom of the upper and enters the top of the lower,
  never along the edges; two connectors into one side spread without crossing; a Freeform box
  spreads; the clearance option round-trips and changes the route.
- Layout tests: Chen attributes off the relationship axis; the nested two-way pair keeps its first
  direction; activity actions between a fork and join aligned with the bar.
- `cargo test -p vizcraft-ui-egui` with any canvas snapshot that changes retaken; `cargo xtask ci`
  green.

## Done when

`vizcraft-cli new --template ID --svg` for every template plus a PNG export of `er.conceptual`,
`uml.activity`, `dfd.threat`, `cloud.topology` and `uml.classDiagram`, each looked at: no route
along a box edge, no route through a shape, no shared runs, labels clear. Then in the app on 8060:
open `er.conceptual`, drag Order to the right and back, and see the route keep its stand-off while
dragging and after release.

## Report

Commits; the clearance option and its command param (for docs/mcp.md); the quality checker; every
re-blessed golden and retaken snapshot family; follow-ups (cases the checker still allows and why).
