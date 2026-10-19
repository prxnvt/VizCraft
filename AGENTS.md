# VizCraft: instructions for agents

VizCraft is a clean-room, open-source desktop diagramming app in Rust and egui, aiming at the
feature range of Microsoft Visio, with an MCP server modelled on draw.io's so agents can build and
edit diagrams. The chrome is finished; the diagram model is being built cascade by cascade
([ROADMAP.md](ROADMAP.md)). These rules apply to every change, by people and agents alike.

The design contract is [docs/architecture.md](docs/architecture.md). Read it before changing
anything that crosses a crate boundary; change it only together with the code and say so in the
commit message.

## Never crash

People trust VizCraft with their diagrams; a crash loses them. This outranks feature work: never
ship a feature through a panic path, and fix a crash before building on top of it.

- No panics in non-test code: no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`. No `unsafe` (`unsafe_code = "forbid"` in the workspace; `macos-open` is the
  one audited exception, with `deny` and a single `imp` module).
- Errors are `Result<T, E>` through the crate's error type and `?`. An unfinished feature returns
  an error or is disabled; it never panics.
- Input-derived numbers are hostile (files, command params, control-channel requests): `get()`
  instead of `[i]`, slice strings only at char boundaries, checked or saturating arithmetic,
  clamp sizes, cap allocations, bound recursion.
- Last-resort guard: `Session::run` turns an escaped panic into an error and restores the
  document. It is a safety net, not a licence.
- Prove it: every crash fix lands with a regression test. The engine test
  `hostile_params_never_panic` runs every command with junk params; keep it green.
- Production crate roots carry
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`;
  new crates start with it. Tests may `unwrap` (`clippy.toml`).

## Assets: no Microsoft, Adobe, Avid or Autodesk iconography or images

This rule is non-negotiable. Breaking it is the worst mistake you can make in this repository.

- Never copy, trace, screenshot-crop or recreate icons, images, templates, themes, clip art,
  fonts, cursors, sounds or any other asset from a Microsoft, Adobe, Avid or Autodesk product.
  Not even temporarily, not even as a placeholder.
- Every asset is original (drawn in code; prefer this: `crates/ui-egui/src/icons.rs`), public
  domain or CC0, OSI open source, or Creative Commons allowing redistribution, or contributed under
  this repository's licence by the person who made it.
- Every asset file has a row in [ATTRIBUTION.md](ATTRIBUTION.md) (author, source, licence).
  `cargo xtask assets` (part of `cargo xtask ci`) fails otherwise. Add the row in the same commit
  as the file.
- Sample documents, templates and theme colours are our own words and colours.

## Clean room

VizCraft may look at the products it resembles (see
[Product and observation](#product-and-observation)) the way a user does: run them, use their
interfaces with synthetic documents, take screenshots of their windows only (by window id, never
the whole desktop), and keep those screenshots in a gitignored folder, never committed. Never
read, disassemble or copy anything inside another vendor's application bundle; never commit files
that application produced; never copy its wording beyond feature names. File formats come from
public specifications (see [Formats](#formats)). Never copy GPL, LGPL or AGPL code.

## Product and observation

VizCraft resembles Microsoft Visio for features and draw.io for agent integration. Both may be
observed only as the [Clean room](#clean-room) section allows: run them, use their interfaces
with synthetic diagrams, screenshot their windows into a gitignored folder. Feature names (glue,
containers, swimlanes, Shape Data, themes) may be used; wording, help text, stencil artwork,
templates, themes and colour schemes may not be copied. For draw.io, that includes its shape
libraries and the text of its MCP tool descriptions: our tools cover the same workflows in our
own words.

## Formats

Every file format is implemented from public specifications only: the draw.io (mxGraph) file
format documentation, the Mermaid syntax documentation, the Microsoft MS-VSDX open specification,
the ECMA-376 Office Open XML specification (SpreadsheetML, for XLSX tables), and the SVG and PDF specifications. Never read another application's parser or writer source to
learn a format, and never commit files another application produced; test files are written by
our own code or by hand. Each format, its source and its state are listed in
[docs/formats.md](docs/formats.md); update it with the code.

## Stencils

Every stencil (shape master) is original and drawn in code, in the planned `crates/stencils`. Never
Microsoft stencils, never shapes traced from another tool, and never vendor product or brand icons
(cloud provider icons, company logos, product marks) even if another tool ships them. Domain
notations (UML, BPMN, ER crow's foot, network) are drawn from the public notation, not from
another tool's rendering of it. A stencil set that needs image files follows the
[Assets](#assets-no-microsoft-adobe-avid-or-autodesk-iconography-or-images) rules.

## MCP

The MCP server (`crates/mcp`, launched by `vizcraft-cli mcp`) exposes engine commands by
id. Every new command is reachable through the generic command tool with no extra work, because
the tool reads the registry. Add a dedicated MCP tool only for a workflow draw.io's MCP server
covers (opening XML, Mermaid or CSV, shape search, page listing and editing) or one that cannot be
expressed as a sequence of commands. Programmatic calls never open dialogs. Document every tool,
its parameters and its result in [docs/mcp.md](docs/mcp.md) in the same commit as the tool.

## Parity catalog

[docs/parity.md](docs/parity.md) lists the Visio-class features VizCraft aims for, grouped by
area, each with its cascade and state. When a feature lands, update its state in the same commit;
when you add a feature that is not listed, add a row.

## Everything is a command

User-visible behaviour is a `CommandSpec` in the engine registry (id, label, location, shortcut,
params doc, `enabled`, `run`) plus a test. UI-only commands are `ui.*` and live in `ui-egui`,
listed in `UI_COMMANDS` so the palette, menus and control channel see them. Menus, ribbon,
shortcuts, command palette, CLI and control channel all dispatch by id. Programmatic calls never
open dialogs: a command that needs one pushes a request to `session.ui_requests` and the UI
handles it. Id prefixes and the struct are in the contract.

When you add behaviour, add the command first, then wire it into the ribbon or menu. If a thing
can be clicked but not run by id, it is not finished.

## Layering

Engine below UI below apps, enforced by `cargo xtask layers`. Nothing in `engine` depends on
egui, eframe, winit, rfd or muda. `ui-egui` depends on `egui` only. Only `apps/*` touch eframe,
rfd and muda. The UI is thin: it reads `Session` state and dispatches commands; it does not hold
document logic. Colours come from `theme::Tokens` only; fonts are Inter and JetBrains Mono from
`assets/fonts`.

Rust only: no handwritten JavaScript or TypeScript. Never break the wasm build.

## Quality gates

Before every commit: `cargo xtask ci` (rustfmt, clippy `-D warnings`, tests, assets, layers,
wasm). Commit after every arc of work that builds. Rust 2024, `rustfmt.toml` (max width 150).
Comments say why, in plain words; doc comments on every public item.

## Running and looking at the app

- `cargo run --release -p vizcraft -- --sample --control 7990` starts the app with the sample
  document and the control channel on `127.0.0.1:7990` (`VIZCRAFT_CONTROL_PORT` works too).
- Drive it with one JSON object per line. Run a command, then take a screenshot and read the PNG:

  ```sh
  printf '%s\n' '{"id":1,"method":"engine.execute","params":{"command":"view.zoomIn","params":{}}}' \
                 '{"id":2,"method":"ui.screenshot","params":{"path":"/tmp/shot.png"}}' | nc 127.0.0.1 7990
  ```

  Methods, params and replies: [docs/control-protocol.md](docs/control-protocol.md). `ui.inspect`
  returns the UI state as JSON, so check state through it before reaching for a screenshot.
- For UI work, look at the result. Take the screenshot, open the PNG, compare with what you meant.
  Set `VIZCRAFT_NO_PREFS=1` so the run does not pick up or overwrite your own preferences.
- `cargo run --release -p vizcraft-cli -- --help` lists the headless subcommands (run commands
  against a file, print the registry).
- Desktop window etiquette: see [docs/testing.md](docs/testing.md#desktop-app-etiquette).
  Launch on the secondary monitor (`--window-pos -2500,100`), quit immediately via
  control channel (`app.quit`) or Don't Save, and verify with `pgrep -fl vizcraft`. Prefer
  the headless `ui_shot` example when only screenshots are needed.
- Shell gotcha: `mv` and `cp` may be aliased interactive on the dev machine; use `/bin/mv -f`
  and `/bin/cp -f`.

## Parallel agents

Several agents work in this repository at once, in separate worktrees.

- Use your own `CARGO_TARGET_DIR` (for example `target-<branch>`; the ignore list covers
  `/target-*/`). Sharing one target directory between worktrees produces confusing "file not found"
  build errors and lock contention. Delete yours when the work is merged; it is only a cache.
- Edit only the crates and files you own. If you need a change in someone else's crate, write it
  down for the orchestrator instead of making it.
- Use your own control port (pick one above 7990 that nobody else listed) and your own screenshot
  paths, so two running apps do not collide.
- Never touch another worktree, never rewrite shared history, never force-push.

## Writing style

- No emoji anywhere: code, comments, docs, commit messages, UI text, test data. Use words.
- Commit messages are plain sentences about what changed and why; prefix with the cascade or task
  when there is one (`C2: ribbon tab strip`). No attribution trailers of any kind.
- Markdown tables for tabular facts, prose for everything else. Link to
  `docs/architecture.md` rather than restating it.

## Roadmap

[ROADMAP.md](ROADMAP.md) tracks the cascades and what has landed. Update it when a cascade task
lands.
