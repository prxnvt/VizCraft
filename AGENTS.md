# VizCraft: instructions for agents

VizCraft is the baseline the "-craft" apps start from (`cargo xtask new-craft <name>`): a
clean-room, open-source Rust and egui desktop app with the chrome finished and a trivial sample
document inside. These instructions travel with every craft made from it. Where a craft has its
own additions (a parity catalog, format specs, an MCP server), its own `AGENTS.md` adds them
below; the rules here still apply.

The design contract is [docs/architecture.md](docs/architecture.md). Read it before changing
anything that crosses a crate boundary; change it only together with the code and say so in the
commit message.

## Never crash

People trust a craft with their work; a crash loses it. This outranks feature work: never ship a
feature through a panic path, and fix a crash before building on top of it.

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

This rule is non-negotiable. Breaking it is the worst mistake you can make in a craft.

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

A craft may look at the product it resembles the way a user does: run it, use its interface with
synthetic documents, take screenshots of its window only (by window id, never the whole desktop),
and keep those screenshots in a gitignored folder, never committed. Never read, disassemble or
copy anything inside another vendor's application bundle; never commit files that application
produced; never copy its wording beyond feature names. File formats come from public
specifications. Never copy GPL, LGPL or AGPL code.

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
