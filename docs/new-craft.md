# Making a new craft

This is the path from `cargo xtask new-craft` to an app that ships. Each step leaves a working,
committed app behind, so you can stop at any point and still have something that runs. The design
behind each step is in [architecture.md](architecture.md); this file says what to do, in order.

The example below makes "GridCraft", a spreadsheet. Replace the name and the model with your own.

## 1. Create the repository

```sh
cd ~/Workspace/crafts/protocraft
cargo xtask new-craft gridcraft              # writes ../gridcraft
cd ../gridcraft
cargo xtask ci                               # proves the copy builds and passes every gate
cargo run --release -p gridcraft -- --sample
```

`new-craft` copies the repository to a sibling folder (without `target/` and `.git/`), renames
`protocraft`, `ProtoCraft` and `PROTOCRAFT` in every file name and every file (crate and package
names, the binaries, the bundle id `ai.storyteller.gridcraft`, the preferences folder
`GridCraft`, `GRIDCRAFT_CONTROL_PORT` and `GRIDCRAFT_NO_PREFS`, the icon file names, these docs),
and runs `git init` with one commit. The name must be lowercase ASCII letters; the capitalised
form is the lowercase form with its first letter raised, so choose a name that reads well that
way (`gridcraft` becomes `GridCraft`).

What you have now is ProtoCraft under a new name: the chrome, the notes sample, the control
channel and the CLI all work. Commit nothing else until `cargo xtask ci` is green, then tag it;
it is the point you compare against when something breaks later.

Then make the repository yours:

- `README.md`: replace the first section with what the craft is; keep the layout and running
  sections, they still hold.
- `AGENTS.md`: add the craft-specific sections (what product it resembles and how it may be
  observed, which public specifications its formats come from, its parity catalog if it keeps
  one). The shared rules stay.
- `ROADMAP.md`: replace the ProtoCraft cascades with the craft's milestones.
- `NOTICE`, `LICENSE-*`: the copyright line reads "GridCraft contributors" after the rename;
  adjust if the craft has another owner.

## 2. Replace the sample document model

The sample model lives in three files in `crates/engine/src`, and the chrome only ever reaches
it through `Session`, so swapping it does not touch the UI crate.

1. **`doc.rs`**: define your `Document`. Keep it a plain value type: `Clone` (undo snapshots are
   clones; make it cheap with `Rc` or persistent structures when it gets large), `Serialize` and
   `Deserialize` (for the file format and for tests), `Default` (for `file.new`). Give it the
   edit primitives your commands will need as methods that return `Result`, never panic, and treat
   every index as hostile.
2. **`io.rs`**: `save_bytes(name, doc)` and `load_bytes(name, bytes)`. The file name chooses the
   format (by extension), so the sample's `.craft.json` can stay as a debugging format while you
   add the real one. Loading is where untrusted bytes enter the app: bound every size, reject what
   you do not understand with an error, and add a test that feeds truncated and garbage files.
3. **`sample.rs`**: the document `--sample` opens. Make it the document you want on every
   screenshot: realistic, your own text and numbers, small enough to load instantly.
4. **`ViewState`** in `session.rs`: add what the canvas needs that is not document state
   (selection, scroll position, which sheet is active). Pane visibility flags and zoom are
   already there.

Delete the notes commands (`note.*`) and their tests. Keep `file.*`, `edit.*` and `view.*`; they
are what the chrome exercises, and they are written against `Document`'s `Default`, `Clone` and
the `io` module, so they keep working. Run `cargo xtask ci`. The app opens, draws nothing
meaningful in the centre yet, and every menu still works. Commit.

## 3. Draw the canvas

`crates/ui-egui/src/canvas.rs` (create it; the baseline's central area is a placeholder that
prints the notes) draws `session.doc` and `session.view` and turns pointer and keyboard input into
commands. Rules that keep it maintainable:

- The canvas reads state and dispatches commands by id. It does not mutate the document; a click
  that changes something calls `app.run("cell.select", params)`.
- Colours from `theme::Tokens`, fonts from the loaded families. The canvas must look right in both
  light and dark mode; `ui.set {"path": "dark", "value": true}` plus a screenshot is the test.
- Report geometry in `ui.inspect` (the canvas rectangle, the selection, the caret), so scripts and
  tests can act on the document without pixel hunting. Add a `ui.clickCell` or similar if window
  coordinates are awkward for your model.

Commit when the sample document is visible and a selection can be made.

## 4. Extend the registry

Commands are `CommandSpec`s grouped by file in `crates/engine/src/cmd/` and collected into the
`Registry`. For each feature:

1. Write the command: id with the right prefix (`insert.row`, `format.bold`, `view.freezePanes`),
   label, `location` ("Home > Cells" is both the tooltip hint and where the ribbon will put it),
   shortcut in `Mod+...` form, a one-line params doc, an `enabled` check with a reason when
   disabled, and `run`.
2. Write its test next to it: the happy path, the disabled case and a hostile-params case.
   `hostile_params_never_panic` covers every command automatically, but a feature-specific test
   says what the command should do, not only that it does not crash.
3. Register it. The command is now in the palette, in `engine.commands`, in the CLI and on the
   control channel, and in the menus and ribbon as soon as a tab or menu names its id.

Undo comes for free: `Session::run` snapshots the document before any command whose `run`
changes it and records the command's label as the undo label. A command that only changes view
state should say so (see the `view.*` commands) so it does not pollute the undo stack.

If a command needs a dialog (a name, a path, a choice), it does not open one. It pushes
`{"open": "<dialog>"}` with whatever the dialog needs to `session.ui_requests` and returns; the UI
opens the dialog, and the dialog's confirm button runs a second command with the chosen params.
The same command run from a script with full params skips the dialog entirely.

## 5. Ribbon tabs and panes

The ribbon in `crates/ui-egui/src/ribbon.rs` is a list of tabs, each a list of groups, each a
list of command ids with a presentation hint (large button, small button, toggle, dropdown,
gallery). Add a tab by adding to that list; the group label and button labels come from the
`CommandSpec`s, the enabled state from `Session::commands()`, the icon from `icons.rs` by command
id. A new icon is drawn in code, in `icons.rs`, as a function of the icon size and the
`Tokens`; no image files.

Contextual tabs (shown only when something is selected) are tabs with a condition closure; the
tab strip calls it each frame. The active tab is `UiState.tab` and is set by `ui.tab`, so a test
can switch tabs by command.

Panes live in `crates/ui-egui/src/panes/`. A pane is a struct implementing the `Pane` trait
(id, title, which side it prefers, `show(&mut self, app, ui)`) registered with the pane manager.
Visibility is a `ViewState` flag toggled by a `view.*` command (so it is in the menus) and the
layout (side, width, order) is in `UiState.panes` (so it persists). A pane shows state and
dispatches commands, the same as the canvas.

## 6. Menus

`menus::menu_tree` returns the menus (File, Edit, View, Window, Help) as `Item`s that name
command ids. Add your items there, once; the in-window menu bar and the native macOS bar are
both built from this tree, and `entry(app, id)` gives each the label, shortcut text, enabled and
checked state for display. Rules:

- Menu items name commands; they carry no logic of their own. A `Check` item reflects a boolean
  the command toggles.
- The app menu on macOS (About, Settings, Quit) is assembled by `apps/<name>/src/native_menu.rs`
  from `app.about`, `app.preferences` and `app.quit`; leave it alone unless you add an app-level
  command.
- Recent files are appended to File by the tree itself from `UiState.recent`.

Shortcuts come from the `CommandSpec`; `keys.rs` parses `Mod+Shift+S` style strings and the
menus display them with the platform's glyphs. A shortcut typed in the canvas, chosen in a menu
or sent over `ui.key` runs the same id.

## 7. The command line and the control channel

The CLI (`apps/<name>-cli`) already runs any command against a file and prints the registry.
Add subcommands for what people will script most (convert, export, inspect) as thin wrappers over
commands. Extend `ui.inspect` with what your canvas knows, and document any new control method in
`docs/control-protocol.md`, keeping the reply shape.

## 8. Tests

Three kinds, all run by `cargo xtask ci`:

- Engine tests next to the commands (fast, headless, the bulk of coverage).
- Round-trip tests for the file format: sample document to bytes to document and back equal.
- UI tests with `egui_kittest` in `crates/ui-egui/tests/`: build a `CraftApp` with the sample,
  run a frame, assert on widgets, and snapshot-compare screenshots in light and dark mode. The
  kittest harness is what keeps the chrome honest after the model changes under it.

## 9. Packaging

`packaging/` holds the icon script and the macOS bundle template; [the packaging
README](../packaging/README.md) describes the bundle and what the local installer
(`crafts/rebuild.sh`) expects. To ship:

1. Draw the app icon as an SVG in `assets/app-icon/<name>.svg` (original work), run
   `packaging/icons.sh` to render the PNG sizes and the `.icns`, and add a row to
   `ATTRIBUTION.md` for each file it writes.
2. Fill in `packaging/macos/Info.plist.in`: display name, bundle id, the document types the craft
   opens (declare an exported UTI for its own format), the category.
3. Provide `packaging/macos/package.sh` (build, assemble the bundle, sign, DMG); the baseline's
   Cascade 3 ships one to adapt. Then `./rebuild.sh <name>` from the crafts folder installs the
   app into `/Applications`.
4. Linux and Windows packaging follow the same shape (`packaging/linux`, `packaging/windows`),
   with the hicolor PNGs and the `.ico` from the icon script.

## 10. Keep the baseline close

When you fix something in the chrome, fix it in ProtoCraft too (or first), so the next craft
starts without the bug. A craft diverges in its model, its canvas, its commands and its tabs; the
title bar, menus, pane manager, preferences, control channel and the `xtask` gates should stay
recognisably the same code, which is what makes agents productive across the crafts.
