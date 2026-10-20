# Contributing to VizCraft

VizCraft is a desktop diagramming app in Rust and egui with the feature range of Microsoft Visio
as its target and an MCP server for agents. People keep their work in it, so a few rules below
are firm: no crashes, no borrowed assets, everything reachable as a command. The plan is in
[ROADMAP.md](ROADMAP.md) and the feature catalog in [docs/parity.md](docs/parity.md); the
reasoning behind the design is in [docs/architecture.md](docs/architecture.md), and the working
rules for agents (which apply to people as well) are in [AGENTS.md](AGENTS.md).

## Before you start

- Read `docs/architecture.md`. It is the contract between the crates. If your change needs the
  contract to move, change the document in the same commit as the code and say so in the message.
- Pick the crate that owns the behaviour: engine for anything a command does, `ui-egui` for how
  it looks, `apps/*` for platform glue. `cargo xtask layers` will tell you if a dependency points
  the wrong way.

## Making a change

1. Branch from `main`. Keep one topic per branch.
2. Every user-visible behaviour is a command (`CommandSpec`) with a test. No panics in non-test
   code; hostile input is an error, never a crash.
3. Run `cargo xtask ci` before every commit. It runs fmt, clippy with `-D warnings`, the tests,
   the asset-attribution check, the layering check and the wasm build; all of them must pass.
4. Any new file under `assets/` gets a row in [ATTRIBUTION.md](ATTRIBUTION.md) in the same
   commit, and must be original or openly licensed. Nothing from Microsoft, Adobe, Avid or
   Autodesk, ever. Stencils are drawn in code and original; no vendor product or brand icons.
5. Commit messages are plain sentences about what changed and why, without attribution lines or
   emoji. Prefix with the cascade or task when there is one (`C2: layers pane`).
6. A feature that lands updates its row in [docs/parity.md](docs/parity.md); a new MCP tool is
   documented in [docs/mcp.md](docs/mcp.md); a new or changed file format in
   [docs/formats.md](docs/formats.md).
7. Open a pull request. Describe what changed, why, and how you checked it (a screenshot from the
   control channel is welcome for UI work).

## Licence

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of
the repository ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

## Trademarks

Microsoft and Visio are trademarks of the Microsoft group of companies; Adobe of Adobe Inc.; Avid
of Avid Technology, Inc.; Autodesk of Autodesk, Inc. The names are used only to identify products;
VizCraft is not affiliated with or endorsed by their owners.
