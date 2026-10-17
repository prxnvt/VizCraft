# VizCraft roadmap

VizCraft is built in cascades: groups of pieces that several agents build in parallel against
the contract in [docs/architecture.md](docs/architecture.md), then integrate. Each cascade ends
with `cargo xtask ci` green on `main` and the app runnable. This file says what each cascade
contains and what has landed; update the state column when a piece merges.

## Where we are

| Measure | Value |
|---|---|
| Cascade | 4 complete; baseline ready for new crafts (DAW, diagram, animation, schema) |
| App runs | full chrome (ribbon, panes, menus, dialogs, control channel, multi-window, web shell) |
| Crafts made from the baseline | verified end-to-end with new-craft test |

## Cascade 0: skeleton and contract (done)

| Piece | State |
|---|---|
| Workspace layout, crate stubs, lints, rustfmt and clippy configuration | done |
| `docs/architecture.md`, the contract everything below is built against | done |

## Cascade 1: foundation, docs, xtask (done)

Three agents, three branches, no shared files.

| Piece | Contents | State |
|---|---|---|
| Foundation | `vizcraft-engine`: `Session`, undo and redo, `Registry` and `CommandSpec`, the notes sample model, JSON I/O, `file.*`, `edit.*`, `view.*` commands, `hostile_params_never_panic`; `ui-egui` scaffolding: `theme::Tokens`, fonts from `assets/fonts`, `UiState`, `CraftApp` with `logic` and `ui`; the assets (fonts) in place | done |
| Docs | `README.md`, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `ROADMAP.md`, licences and `NOTICE`, `ATTRIBUTION.md`, `docs/control-protocol.md`, `docs/new-craft.md`, the placeholder app icon and `packaging/icons.sh`, the macOS `Info.plist.in` and `packaging/README.md` | done |
| xtask | `cargo xtask ci` (fmt, clippy, tests, assets, layers, wasm), `layers`, `assets`, `wasm`, `new-craft` | done |

Done when: `cargo xtask ci` passes on the merged branch, `cargo run -p vizcraft -- --sample`
opens the sample in a window with the theme applied, and `cargo xtask new-craft probe` produces
a sibling that passes `ci` under its new name.

## Cascade 2: the chrome (done)

Six pieces built in parallel against the `CraftApp` and `Session` APIs from Cascade 1.

| Piece | Contents | State |
|---|---|---|
| Multi-window and native menu | `apps/vizcraft/src/windows.rs`: root viewport plus immediate viewports, one `CraftApp` each, Cmd+N in-process, root hand-over on close, quit asking each dirty window in turn, Cmd+W; `native_menu.rs` (muda, macOS) built from `menus::menu_tree` with the app and Window menus; `macos-open` with the real `imp` | done |
| Ribbon | `ribbon.rs`: tab strip, groups from command ids, large and small buttons, toggles, dropdowns, collapse, contextual tabs; `icons.rs` drawn in code | done |
| Pane manager | `panes/`: `Pane` trait, left and right side panels, drag to resize and reorder, `PaneLayout` persisted in `UiState`, `view.*` toggles | done |
| In-window menu bar and dialogs | `menus.rs`: `menu_tree`, `entry`, `menu_bar`, `in_window_menus`; `dialogs.rs` with the Save-changes, About and Preferences dialogs driven by `ui_requests`; `keys.rs` shortcut parsing and dispatch; `chrome.rs` title bar (integrated on macOS) and status bar; `backstage.rs` | done |
| Control channel and CLI | `control.rs` handling every method in `docs/control-protocol.md` with screenshots through the viewport; `apps/vizcraft/src/control_server.rs`; `--control`, `--sample`, file arguments; `apps/vizcraft-cli` running commands headless and printing the registry; preferences at the platform path with `VIZCRAFT_NO_PREFS`; `window_geometry.rs` | done |

Done when: every window strip in the contract's layout order is on screen, both light and dark,
the menus agree between the native bar and the in-window bar, and a script over the control
channel can open the sample, change a note, switch tabs, toggle a pane and screenshot the
result.

## Cascade 3: integration, rename, packaging, kittest (done)

| Piece | Contents | State |
|---|---|---|
| Integration | Merge the Cascade 2 branches, resolve the seams between ribbon, panes and menus, one pass over the whole chrome for consistency | done |
| Rename proof | `cargo xtask new-craft` run end to end; the result builds, passes `ci`, launches with the new name, bundle id and preferences folder; fixes to whatever the rename missed | done |
| Packaging | `packaging/macos/package.sh`, `env.sh` and `entitlements.plist` adapted from WordCraft; a bundle that `crafts/rebuild.sh` installs; rendered icons committed with their attribution rows; Linux and Windows packaging scaffolds | done |
| kittest | `egui_kittest` tests in `crates/ui-egui/tests/`: a frame of the sample in light and dark with snapshot comparison, menu and ribbon smoke tests, pane toggles, the Save-changes dialog flow | done |

Done when: a new craft made from `main` can be installed into `/Applications` by `rebuild.sh`
with its own name and icon, and the kittest snapshots guard the chrome.

## Cascade 4: generic baseline extensions (done)

Generalize VizCraft from a text-focused template into a clean, reusable foundation for diverse desktop crafts (arrangers/DAWs, diagram editors, animation suites, database schema visualizers).

| Piece | Contents | State |
|---|---|---|
| Selection generalization | `Selection`: primary selection, multi-item IDs, continuous range support, helper methods | done |
| 2D viewport pan and zoom | `ViewState.pan`, `view.pan`, `view.resetPan`, `Viewport2D` screen/world math in `canvas.rs` | done |
| Transport controls | `TransportState`, playback controls (`transport.*`) for time/timeline crafts | done |
| Pluggable side panes | `PaneSpec::new`, `set_panes`, `reset_panes` in `panes.rs` for modular pane registries | done |
| Export and Import | `file.export` and `file.import` command slots wired to menus and backstage | done |

## Future work

Not scheduled, listed so they are not forgotten:

- `apps/vizcraft-web`: the wasm build as a runnable app (the `wasm` gate already compiles it).
- An MCP server over the control channel, shared by the crafts the way WordCraft's is.
- A command palette with fuzzy search over `engine.commands`.
- Autosave and crash recovery in the baseline, so every craft inherits them.
