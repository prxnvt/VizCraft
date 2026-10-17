# Contributing to VizCraft

VizCraft is the baseline the "-craft" apps are made from, so a change here reaches every app
that is created after it. That is the reason for the small number of firm rules below; the
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
   Autodesk, ever.
5. Commit messages are plain sentences about what changed and why, without attribution lines or
   emoji. Prefix with the cascade or task when there is one (`C2: pane manager drag handles`).
6. Open a pull request. Describe what changed, why, and how you checked it (a screenshot from the
   control channel is welcome for UI work).

## Licence

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of
the repository ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
