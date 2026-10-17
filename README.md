# VizCraft

VizCraft is a desktop diagramming app in Rust and [egui](https://github.com/emilk/egui) 0.36,
aiming at the feature range of Microsoft Visio: shapes and stencils, glued connectors, pages and
layers, themes, shape data, auto-layout, and import and export of the common diagram formats.
Agents drive it through an MCP server modelled on draw.io's, so a model can open, build and edit
diagrams the way a person does.

Today the application chrome is complete and the diagram model is being built. The window,
native and in-window menus, ribbon, side panes, several windows in one process, preferences, a
command registry, a command-line tool and a JSON control channel all work; the document is the
diagram model (pages, shapes, connectors) being built in Cascade 1. What lands when is in [ROADMAP.md](ROADMAP.md), and the Visio-class
feature catalog with the state of each feature is in [docs/parity.md](docs/parity.md).

The design is written down once, in [docs/architecture.md](docs/architecture.md). That file is
the contract the crates are built against; this README tells you how to use the repository and
links to the contract rather than repeating it.

## Layout

| Path | What it is |
|---|---|
| `crates/engine` | `vizcraft-engine`: `Session`, undo, selection, the command registry, file I/O; its document is the `vizcraft-diagram` model. Knows nothing about egui. |
| `crates/ui-egui` | `vizcraft-ui-egui`: the egui front end. Theme, icons, chrome, menus, ribbon, panes, canvas viewport, dialogs, keys, control channel handling. Depends on `egui` only, not eframe. |
| `crates/macos-open` | Finder "Open Documents" (odoc Apple Event) support, behind a safe API. |
| `apps/vizcraft` | The desktop binary: eframe, native menu (muda on macOS), file dialogs (rfd), multi-window host, control server. |
| `apps/vizcraft-web` | Browser shell: wasm32-unknown-unknown target run through trunk and wasm-bindgen; see [docs/web.md](docs/web.md). |
| `apps/vizcraft-cli` | Headless command line: list the registry, run commands against a document, inspect a file, talk to a running app. |
| `xtask` | `cargo xtask ci`, `layers`, `assets`, `wasm`, `ico`, `version`. |
| `assets/` | Fonts (Inter, JetBrains Mono) and the app icon. Every file here has a row in [ATTRIBUTION.md](ATTRIBUTION.md). |
| `packaging/` | Icon rendering, the macOS bundle and the Linux desktop entry; see [packaging/README.md](packaging/README.md). |
| `docs/` | The architecture contract, the parity catalog, formats, the MCP design, the control protocol, testing and the web build (links below). |

These crates are planned and do not exist yet:

| Path (planned) | What it will be |
|---|---|
| `crates/diagram` | `vizcraft-diagram`: pages, layers, shapes, connectors, groups and containers, text, styles and themes, shape data; geometry, hit testing, glue, connector routing, auto-layout. |
| `crates/stencils` | The built-in stencil library: original shapes defined in code, categories and a keyword search index. |
| `crates/formats` | The native `.vizcraft` format, .drawio read and write, Mermaid and CSV import, SVG export, later .vsdx import. |
| `crates/mcp` | `vizcraft-mcp`: the MCP server, launched by `vizcraft-cli mcp`. |

The layering (engine below UI below apps) is enforced by `cargo xtask layers`, so a dependency
pointing the wrong way fails CI rather than quietly spreading.

## Running

```sh
cargo run --release -p vizcraft                                # an empty document
cargo run --release -p vizcraft -- --sample                    # the built-in sample document
cargo run --release -p vizcraft -- project.vizcraft            # open a file
cargo run --release -p vizcraft -- --sample --control 7990     # plus the control channel
cargo run --release -p vizcraft -- --window-pos -2500,100      # place the window (x,y)
```

`--control <port>` (or `VIZCRAFT_CONTROL_PORT=<port>`) opens a JSON-lines server on
`127.0.0.1` that runs commands, injects input and takes screenshots of the window. It is how
agents look at what they built and how UI tests drive the app:

```sh
printf '%s\n' '{"id":1,"method":"engine.execute","params":{"command":"file.new","params":{}}}' \
               '{"id":2,"method":"ui.screenshot","params":{"path":"/tmp/vizcraft.png"}}' \
  | nc 127.0.0.1 7990
```

The methods, their parameters and replies are in [docs/control-protocol.md](docs/control-protocol.md).
Other flags: `--in-window-menus` (or `VIZCRAFT_IN_WINDOW_MENUS=1`) draws the menu bar inside the
window instead of the macOS menu bar, and `--version` prints the version.

The command line runs the same commands without a window. `vizcraft-cli commands` prints the
registry (ids, labels, shortcuts, enablement), `vizcraft-cli run` runs a sequence of commands
against a document and saves the result, `vizcraft-cli inspect` summarises a file, and
`vizcraft-cli control` sends requests to a running app. `cargo run --release -p vizcraft-cli --
--help` lists them all.

Preferences (theme, ribbon state, pane layout, window geometry, recent files) persist in
`ui.json` under the platform's application-support folder; `VIZCRAFT_NO_PREFS=1` runs without
reading or writing them, which tests and screenshot sessions use.

## MCP server

The MCP server runs over stdio (`vizcraft-cli mcp [--connect ADDR]`) against a running app
through the control channel. Today it has the generic tools (`command_list`, `command_run`,
`command_batch`, `doc_inspect`, `screenshot`); it grows to draw.io parity, with an in-process
headless session, in Cascade 3. Every engine command is
reachable through generic command tools, and dedicated tools mirror the workflows draw.io's MCP
server offers (open XML, Mermaid or CSV, search shapes, list and edit pages). The design and the
tool table are in [docs/mcp.md](docs/mcp.md); the file formats it reads and writes are in
[docs/formats.md](docs/formats.md).

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
| `ribbon` | Tab strip plus the ribbon body, collapsible, with contextual tabs. Groups are built from command ids. |
| `panes` | Left and right side panels, their layout persisted as part of `UiState`. |
| `canvas` | `Viewport2D`: pan and zoom of the central canvas, world and screen coordinates. |
| `dialogs` | Modal dialogs; commands never open them directly, they ask through `ui_requests`. |
| `keys` | Shortcut parsing (`Mod+S`) and dispatch to command ids. |
| `control` | Handling of control-channel requests: run, inspect, click, key, screenshot. |
| `window_geometry` | Saving and sanitising window position and size between runs. |
| `backstage` | The optional full-window File page. |

`CraftApp` ties them together: `logic` runs once per frame before layout (control requests,
shortcuts, close requests, drops), `ui` lays the window out top to bottom (title bar, menu bar
when in-window menus are on, ribbon, status bar, panes, then the central canvas). The desktop app
owns one `CraftApp` per window; the public API is listed in the contract.

## Documentation

| Document | What it covers |
|---|---|
| [docs/architecture.md](docs/architecture.md) | The design contract: crates, layers, `Session`, commands, the UI API, and the planned diagram crates. |
| [docs/parity.md](docs/parity.md) | The Visio-class feature catalog, with the cascade and state of each feature. |
| [docs/formats.md](docs/formats.md) | The file formats VizCraft reads and writes, each with its public specification. |
| [docs/mcp.md](docs/mcp.md) | The MCP server: backends, tools, resources, launch. |
| [docs/control-protocol.md](docs/control-protocol.md) | The JSON-lines control channel: methods, params, replies. |
| [docs/testing.md](docs/testing.md) | Tests, snapshots and desktop-app etiquette. |
| [docs/web.md](docs/web.md) | The browser build. |
| [ROADMAP.md](ROADMAP.md) | The cascades and what has landed. |

## Quality gates

`cargo xtask ci` runs rustfmt, clippy with `-D warnings`, the tests, the asset-attribution check,
the layering check and the wasm build. It must pass before every commit. The rules behind it,
and the rules for agents working in this repository, are in [AGENTS.md](AGENTS.md).

## Licence

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
Third-party material is listed in [NOTICE](NOTICE) and, per file, in
[ATTRIBUTION.md](ATTRIBUTION.md). Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md).
