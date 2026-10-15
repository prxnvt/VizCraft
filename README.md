# ProtoCraft

ProtoCraft is the baseline every new "-craft" app starts from. It is a complete, empty desktop
app in Rust and [egui](https://github.com/emilk/egui) 0.36: window chrome, native and in-window
menus, a ribbon, side panes, several windows in one process, preferences, a command registry, a
command-line tool and a JSON control channel that lets people and agents drive the running app.
Everything a WordCraft, GridCraft, DeckCraft or VectorCraft shares lives here once, so a new
craft begins with the chrome finished and spends its time on the thing that makes it different:
the document model.

In place of a real model ProtoCraft carries a trivial sample, "notes": a list of titled text
entries, saved as JSON (`.craft.json`). It exists to exercise the chrome and to show where a real
model plugs in; the first job of a new craft is to replace it.

The design is written down once, in [docs/architecture.md](docs/architecture.md). That file is
the contract the crates are built against; this README tells you how to use the repository, and
links to the contract rather than repeating it.

## Layout

| Path | What it is |
|---|---|
| `crates/engine` | `protocraft-engine`: `Session`, undo, the command registry, the sample document model, file I/O. Knows nothing about egui. |
| `crates/ui-egui` | `protocraft-ui-egui`: the egui front end. Theme, icons, chrome, menus, ribbon, panes, dialogs, keys, control channel handling. Depends on `egui` only, not eframe. |
| `crates/macos-open` | Finder "Open Documents" (odoc Apple Event) support, behind a safe API. |
| `apps/protocraft` | The desktop binary: eframe, native menu (muda on macOS), file dialogs (rfd), multi-window host, control server. |
| `apps/protocraft-cli` | Headless command line: run commands against a document, list the registry. |
| `xtask` | `cargo xtask ci`, `layers`, `assets`, `wasm`, `new-craft`. |
| `assets/` | Fonts (Inter, JetBrains Mono) and the app icon. Every file here has a row in [ATTRIBUTION.md](ATTRIBUTION.md). |
| `packaging/` | Icon rendering and the macOS bundle template; see [packaging/README.md](packaging/README.md). |
| `docs/` | The [architecture contract](docs/architecture.md), the [control protocol](docs/control-protocol.md) and [how to make a new craft](docs/new-craft.md). |

The layering (engine below UI below apps) is enforced by `cargo xtask layers`, so a dependency
pointing the wrong way fails CI rather than quietly spreading.

## Running

```sh
cargo run --release -p protocraft                      # an empty document
cargo run --release -p protocraft -- --sample          # the built-in sample document
cargo run --release -p protocraft -- notes.craft.json  # open a file
cargo run --release -p protocraft -- --sample --control 7990   # plus the control channel
```

`--control <port>` (or `PROTOCRAFT_CONTROL_PORT=<port>`) opens a JSON-lines server on
`127.0.0.1` that runs commands, injects input and takes screenshots of the window. It is how
agents look at what they built and how UI tests drive the app:

```sh
printf '%s\n' '{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}' \
               '{"id":2,"method":"ui.screenshot","params":{"path":"/tmp/protocraft.png"}}' \
  | nc 127.0.0.1 7990
```

The methods, their parameters and replies are in [docs/control-protocol.md](docs/control-protocol.md).

The command line runs the same commands without a window: it prints the registry (ids, labels,
shortcuts, locations) and runs a sequence of commands against a document, saving the result.
`cargo run --release -p protocraft-cli -- --help` lists the subcommands.

Preferences (theme, ribbon state, pane layout, window geometry, recent files) persist in
`ui.json` under the platform's application-support folder; `PROTOCRAFT_NO_PREFS=1` runs without
reading or writing them, which tests and screenshot sessions use.

## Making a new craft

```sh
cargo xtask new-craft gridcraft                                  # writes ../gridcraft
cargo xtask new-craft notecraft --display-name "Note Craft"      # a display name of your own
cargo xtask new-craft gridcraft --into ~/elsewhere/gridcraft     # another destination
```

`new-craft` copies the tracked files of this repository to a sibling folder and renames
`protocraft`, `ProtoCraft` and `PROTOCRAFT` everywhere (crate names, binaries, the bundle id
`ai.storyteller.<name>`, the preferences folder, environment variables, the icon file names, docs),
runs `cargo fmt` on the copy (a name of a different length moves lines across the rustfmt width),
then initialises a fresh git history with one commit and prints the next steps. The name is
lowercase ASCII letters ending in `craft`; the display name defaults to the name with its first
letter and the C of Craft raised (`gridcraft` becomes `GridCraft`), or `--display-name` sets it
(letters, digits and single spaces; where it is glued to an identifier, as in
`GridCraftOpenDocumentsHandler`, the spaces are dropped). A destination that already exists, in
any letter case, is refused. What you get is a working app called by the new name, with the sample
model still inside. From there, [docs/new-craft.md](docs/new-craft.md) walks through replacing the
model, growing the command registry, adding ribbon tabs, panes and menus, and packaging.

## Replacing the sample document model

The sample model is deliberately small so that swapping it is mechanical. The shape, in short:

1. `crates/engine/src/doc.rs` defines `Document`. Replace it with your own type; `Session` holds
   one and only asks it to be `Clone` (for undo snapshots) and serialisable (for I/O).
2. `crates/engine/src/io.rs` turns a `Document` into bytes and back (`save_bytes`, `load_bytes`,
   keyed by file name so one craft can read several formats). The sample writes JSON.
3. `crates/engine/src/sample.rs` returns the document `--sample` opens. Write one that shows your
   craft off; it is what every screenshot and test starts from.
4. The command registry (`crates/engine/src/cmd/`) is where behaviour goes. Keep `file.*`,
   `edit.*` and `view.*` as they are, delete the sample's note commands, add your own.
5. The central canvas in `crates/ui-egui` draws whatever `session.doc` is. Everything around it
   (chrome, menus, ribbon, panes, dialogs) only knows commands by id and keeps working.

Steps with file names and the order to do them in: [docs/new-craft.md](docs/new-craft.md).

## How the chrome is organised

Every user-visible action is a command in the engine registry: an id such as `file.save`, a
label, the ribbon or menu location it belongs to, a shortcut, a one-line params doc, an `enabled`
check and a `run` function. The menus, the ribbon, the shortcuts, the command palette, the CLI
and the control channel all dispatch by that id, so adding a command once makes it reachable
everywhere, and a command run from a script behaves exactly like the same command clicked. UI-only
commands (`ui.*`: switch tab, show a pane, open a dialog) live in `ui-egui` and are listed the
same way so nothing is hidden from automation. The exact `CommandSpec` is in the contract.

The front end is a set of modules in `crates/ui-egui`, each owning one strip of the window:

| Module | Owns |
|---|---|
| `theme` | `Tokens`: every colour, light and dark. Nothing else names a colour. |
| `icons` | The icon set, drawn in code. No image files. |
| `chrome` | The title bar (integrated with the traffic lights on macOS) and the status bar. |
| `menus` | One menu tree, shown as egui menus in the window or handed to the native macOS bar. |
| `ribbon` | Tab strip plus the ribbon body, collapsible. Groups are built from command ids. |
| `panes` | Left and right side panels, their layout persisted as part of `UiState`. |
| `dialogs` | Modal dialogs; commands never open them directly, they ask through `ui_requests`. |
| `keys` | Shortcut parsing (`Mod+S`) and dispatch to command ids. |
| `control` | Handling of control-channel requests: run, inspect, click, key, screenshot. |
| `window_geometry` | Saving and sanitising window position and size between runs. |
| `backstage` | The optional full-window File page. |

`CraftApp` ties them together: `logic` runs once per frame before layout (control requests,
shortcuts, close requests, drops), `ui` lays the window out top to bottom (title bar, menu bar
when in-window menus are on, ribbon, status bar, panes, then the central canvas). The desktop app
owns one `CraftApp` per window; the public API is listed in the contract.

## Quality gates

`cargo xtask ci` runs rustfmt, clippy with `-D warnings`, the tests, the asset-attribution check,
the layering check and the wasm build. It must pass before every commit. The rules behind it,
and the rules for agents working in a craft, are in [AGENTS.md](AGENTS.md).

## Status

The plan, cascade by cascade, is in [ROADMAP.md](ROADMAP.md).

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
Third-party material is listed in [NOTICE](NOTICE) and, per file, in
[ATTRIBUTION.md](ATTRIBUTION.md). Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md).
