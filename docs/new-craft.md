# Making a new craft

This is the path from `cargo xtask new-craft` to an app that ships. Each step leaves a working,
committed app behind, so you can stop at any point and still have something that runs. The design
behind each step is in [architecture.md](architecture.md); this file says what to do, in order.

The example below makes "GridCraft", a spreadsheet. Replace the name and the model with your own.

## 1. Create the repository

```sh
cd ~/Workspace/crafts/craft-baseline                 # the template (ProtoCraft)
cargo xtask new-craft gridcraft                      # writes ../gridcraft
cd ../gridcraft
cargo xtask ci                                       # proves the copy builds and passes every gate
cargo run --release -p gridcraft -- --sample
```

`new-craft` copies the tracked files of the repository to a sibling folder (so no `target/`,
no `.git/`, nothing untracked: commit first if you want local changes in the copy), renames
`protocraft`, `ProtoCraft` and `PROTOCRAFT` in every file name and every file (crate and package
names, the binaries, the bundle id `ai.storyteller.gridcraft`, the preferences folder
`GridCraft`, `GRIDCRAFT_CONTROL_PORT`, `GRIDCRAFT_NO_PREFS` and `GRIDCRAFT_WINDOW_POS`, the icon
file names, these docs), runs `cargo fmt --all` on the copy (a name of a different length moves
lines across the rustfmt width, and `cargo xtask ci` starts with `fmt --check`), and runs
`git init` with one commit. It then prints the commands above as the next steps.

Options:

- The name must be lowercase ASCII letters ending in `craft`. The display name is the stem with
  its first letter raised plus `Craft` (`gridcraft` becomes `GridCraft`).
- `--display-name "Grid Craft"` sets the display name instead: ASCII letters and digits, words
  separated by single spaces. It is used in titles, menus, the About dialog, the preferences
  folder and the bundle name; where `ProtoCraft` is glued to an identifier in code
  (`ProtoCraftOpenDocumentsHandler`, the `ProtoCraftVersion` plist key) the spaces are dropped so
  the code still compiles. Packaging scripts are not yet quoted for a name with a space in it, so
  check `packaging/` before you ship such a craft.
- `--into <dir>` writes somewhere other than the sibling folder. A destination that already
  exists, in any letter case (`GridCraft` and `gridcraft` are one folder on macOS), is refused.
- The root marker `.craft-template` is not copied: it is what tells `crafts/rebuild.sh` to skip
  the template, so a craft in `~/Workspace/crafts/<name>` with an executable
  `packaging/macos/package.sh` is picked up by the installer automatically.
- If `git commit` fails for want of an identity, the copy is in place and staged; the message
  gives the `git config` lines and the commit to run.

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
- The About dialog (`crates/ui-egui/src/dialogs.rs`) links to the source repository; the rename
  turns the template's URL into `github.com/storytold/gridcraft`, so point it at the real one
  (or remove the link) before the first release.

## 2. Replace the sample document model

The sample model lives in `crates/engine/src`: `doc.rs` (`Document`, `Note`, `CoreProps`),
`io.rs` (the `.craft.json` format), `sample.rs` (the `--sample` document) and, in `session.rs`,
`Selection` (a character range in one note), which `Session.sel` holds. The commands that know
about notes are `insert.note` (`cmd/insert.rs`) and, in `cmd/edit.rs`, the text commands
`edit.cut`, `edit.copy`, `edit.paste`, `edit.selectAll`, `edit.select`, `edit.setText`,
`edit.setNoteTitle`, `edit.setTitle` and `edit.deleteNote`; `edit.undo` and `edit.redo` are
model-independent. The engine tests for all of them are in `crates/engine/src/tests.rs` (one file,
not next to each command).

The UI crate reaches the model directly in a handful of places, so swapping it does touch
`crates/ui-egui`: `canvas.rs` (the note editor), `panes.rs` (the navigation and properties
panes), `chrome.rs` (the status bar's note count), `backstage.rs` (the document statistics),
`control.rs` (the `notes` field of `ui.inspect`) and the kittest tests in `tests/`. Searching the
crate for `doc.notes`, `doc.title`, `Note` and `sel` finds every one.

Do it in two passes so the app keeps building between commits:

1. **Extend first, then replace.** A new field on `Note` or `Document` with a `#[serde(default)]`
   type costs nothing: old files still load, every command and test keeps passing, and a command
   that uses it is one function plus one `CommandSpec` in `specs()` and one test in `tests.rs`
   (`hostile_params_never_panic` covers the new command automatically). Grow the model this way
   until the sample's shape no longer fits, then replace it.
2. **`doc.rs`**: define your `Document`. Keep it a plain value type: `Clone` (undo snapshots are
   clones; make it cheap with `Rc` or persistent structures when it gets large), `Serialize` and
   `Deserialize` (for the file format and for tests), `Default` (for `file.new`), `PartialEq`
   (the tests compare documents). Give it the edit primitives your commands will need as methods
   that return `Result`, never panic, and treat every index as hostile. `Session::new` calls
   `doc.ensure_nonempty()`; keep a method by that name or drop the call.
3. **`io.rs`**: `save_bytes(name, doc)` and `load_bytes(name, bytes)`. The file name chooses the
   format (by extension), so the sample's `.craft.json` can stay as a debugging format while you
   add the real one. Loading is where untrusted bytes enter the app: bound every size, reject what
   you do not understand with an error, and add a test that feeds truncated and garbage files.
4. **`sample.rs`**: the document `--sample` opens. Make it the document you want on every
   screenshot: realistic, your own text and numbers, small enough to load instantly.
5. **`Selection` and `ViewState`** in `session.rs`: replace `Selection` with what your canvas
   selects (a cell range, a set of shapes) and keep `Session.sel` as its home, so `sel_result`
   in `cmd/mod.rs` and the `selection` field of `ui.inspect` still mean something. `ViewState`
   takes what the canvas needs that is not document state and not selection (scroll position,
   the active sheet); pane visibility flags and zoom are already there.
6. **Commands**: delete `insert.note`, `edit.deleteNote`, `edit.setNoteTitle`, `edit.setText`,
   `edit.select` and their tests; rewrite `edit.cut`, `edit.copy`, `edit.paste`, `edit.selectAll`
   and `edit.setTitle` against the new model (the chrome exercises them, so keep the ids).
   `file.*`, `view.*`, `edit.undo` and `edit.redo` are written against `Document`'s `Default`,
   `Clone` and the `io` module, so they keep working untouched.
7. **UI**: fix the places listed above so they read the new model (the canvas can be a stub
   until step 3). Run `cargo xtask ci`. The app opens, draws nothing meaningful in the centre yet,
   and every menu still works. Commit.

## 3. Draw the canvas

`crates/ui-egui/src/canvas.rs` (the baseline's is a note editor: replace its `show`) draws
`session.doc`, `session.sel` and `session.view` and turns pointer and keyboard input into
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
2. Write its test in `crates/engine/src/tests.rs` (`s()` gives a session on the sample, `run`
   unwraps with the command id): the happy path, the disabled case and a bad-params case.
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

Panes live in `crates/ui-egui/src/panes.rs`. A pane is declared as a `PaneSpec` in `panes`
(key, title, side, default width, and a draw function). Visibility and layout (side, width, order)
live in `UiState.panes` (`PaneLayout`), persisted across launches. The Window menu builds its
list of open panes via `panes::window_menu_items(app)` with check marks. A pane shows state and
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
   Cascade 3 ships one to adapt. Once the craft sits in `~/Workspace/crafts/<name>` with that
   script executable, `crafts/rebuild.sh` picks it up automatically (the template itself is
   skipped because of its `.craft-template` marker, which `new-craft` does not copy), and
   `./rebuild.sh <name>` from the crafts folder installs the app into `/Applications`.
4. Linux and Windows packaging follow the same shape (`packaging/linux`, `packaging/windows`),
   with the hicolor PNGs and the `.ico` from the icon script.

## 10. Keep the baseline close

When you fix something in the chrome, fix it in ProtoCraft too (or first), so the next craft
starts without the bug. A craft diverges in its model, its canvas, its commands and its tabs; the
title bar, menus, pane manager, preferences, control channel and the `xtask` gates should stay
recognisably the same code, which is what makes agents productive across the crafts.
