# VizCraft

VizCraft is an open-source desktop diagramming application written in Rust with
[egui](https://github.com/emilk/egui). It targets the feature range of Microsoft Visio and is
oriented towards engineering work: every shape can be a view of a typed element in a semantic
model, notations are defined as data packs with their own validation rules, and an MCP server
lets AI agents build and edit diagrams through the same commands a person uses.

VizCraft is under active development. Progress is tracked in [ROADMAP.md](ROADMAP.md), and the
state of each planned feature is listed in [docs/parity.md](docs/parity.md).

## Requirements

- Rust 1.90 or later (edition 2024).
- macOS, Linux or Windows. The browser build is described in [docs/web.md](docs/web.md).

## Building and running

```sh
cargo run --release -p vizcraft
```

Options follow `--`, for example `cargo run --release -p vizcraft -- --sample`.

| Option | Effect |
|---|---|
| `<file>` | Opens a `.vizcraft` or `.drawio` file |
| `--sample` | Starts with the built-in sample document |
| `--control <port>` | Starts the local JSON control channel ([docs/control-protocol.md](docs/control-protocol.md)) |
| `--trace` | Records a performance trace into `traces/` ([docs/testing.md](docs/testing.md#performance-trace)) |
| `--help` | Lists every option |

The headless command-line tool runs commands against documents without a window:

```sh
cargo run --release -p vizcraft-cli -- --help
```

## Automation and agents

Every user-visible action is a command with a stable id (for example `file.save` or
`shape.add`). The ribbon, menus, keyboard shortcuts, command palette, command-line tool, control
channel and MCP server all run commands by id, so anything a person can do can also be scripted.
The MCP server is started with `vizcraft-cli mcp`; its tools are documented in
[docs/mcp.md](docs/mcp.md).

## File formats

VizCraft saves its own `.vizcraft` format, reads and writes draw.io files, imports Mermaid and
CSV, and exports SVG, PNG and PDF. Each format and the public specification it follows are listed
in [docs/formats.md](docs/formats.md).

## Repository layout

| Path | Contents |
|---|---|
| `apps/vizcraft` | Desktop application |
| `apps/vizcraft-cli` | Command-line tool and MCP launcher |
| `apps/vizcraft-web` | Browser build |
| `crates/engine` | Session, undo, command registry and file I/O |
| `crates/ui-egui` | User interface |
| `crates/diagram` | Diagram document: pages, shapes, connectors, geometry |
| `crates/model`, `crates/notation` | Semantic model, notation packs and validation rules |
| `crates/formats`, `crates/drawio`, `crates/mermaid` | File formats |
| `crates/layout`, `crates/routing` | Automatic layout and connector routing |
| `crates/stencils` | Built-in shape library |
| `crates/mcp` | MCP server |
| `xtask` | Build and quality tooling |

The design of these crates and the rules between them are documented in
[docs/architecture.md](docs/architecture.md).

## Contributing

Contributions are welcome. Before opening a pull request, read [CONTRIBUTING.md](CONTRIBUTING.md)
and [AGENTS.md](AGENTS.md), and make sure the full check passes:

```sh
cargo xtask ci
```

This runs formatting, Clippy, the test suite, the asset attribution check, the dependency
layering check and the WebAssembly build. Testing practice is described in
[docs/testing.md](docs/testing.md).

## Licence

VizCraft is licensed under either the [MIT licence](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option. Third-party material is listed in
[NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
