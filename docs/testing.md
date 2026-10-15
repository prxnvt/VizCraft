# Testing a craft

Four layers, cheapest first. Reach for the next one only when the one before cannot see what
you changed.

| Layer | What it checks | Needs |
|---|---|---|
| Engine unit tests (`crates/engine/src/tests.rs`) | commands, undo, I/O, hostile params | nothing |
| UI tests with `egui_kittest` (`crates/ui-egui/tests/`) | the chrome laid out and driven through real egui input and the control channel | nothing (no GPU) |
| Headless screenshots (`crates/ui-egui/examples/ui_shot.rs`) | what the window looks like | a GPU (wgpu offscreen), no window |
| The running app over the control channel | the real thing: eframe, native menus, file dialogs | a window |

`cargo xtask ci` runs the first two with rustfmt, clippy `-D warnings`, the asset check, the
layering check and the wasm build. Run it before every commit with your own target directory:

```sh
CARGO_TARGET_DIR=target-my-branch cargo xtask ci
```

## Engine unit tests

Every command lands with a test in `crates/engine/src/tests.rs`. The `run` helper there unwraps
with the command id in the panic message; `s()` is a session on the sample document. Two tests
are the safety net for the whole registry and must stay green:

- `hostile_params_never_panic` runs every command with junk params (null, arrays, huge and
  negative numbers, NaN, control characters) and checks the document survived.
- `a_panicking_command_restores_the_document` proves `Session::run` catches a panic and puts the
  document back.

A crash fix lands with a regression test next to these.

## UI tests with egui_kittest

`crates/ui-egui/tests/control.rs` is the pattern. A harness lays the whole app out at a fixed
size with no GPU:

```rust
let (tx, rx) = std::sync::mpsc::channel::<ControlRequest>();
let app = CraftApp::new(Session::new(sample_document()), Services::default()).with_control(rx);
let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
    |ui, app: &mut CraftApp| {
        let ctx = ui.ctx().clone();
        app.logic(&ctx);
        app.ui(ui);
    },
    app,
);
```

Then either drive it the way a person would, with kittest's queries (`harness.get_by_label("Save").click()`,
`harness.run_steps(6)`), or the way an agent would, by sending `ControlRequest`s through `tx`
and stepping until the reply arrives. The `Rig` in that file does the second; copy it. Two things
to know:

- Synthetic input from `ui.click` and friends goes through `CraftApp::raw_input_hook`, which
  eframe calls for you but a harness does not. Take `harness.input_mut()`, pass it through the
  hook, put it back, then `harness.step()` (see `Rig::step`).
- Do not raise `with_max_steps` into the thousands. `build_ui_state` runs the app until it stops
  asking for a repaint, up to that many frames, and the app repaints every frame while input or a
  wait is queued; a huge limit turns the build into a minute-long loop.

Assert on state (`harness.state().session`, `ui.inspect` replies), not on pixels. Snapshot tests
(`harness.snapshot(...)`) need the `wgpu` feature and a GPU; keep them out of `cargo xtask ci`.

## Headless screenshots: ui_shot

`crates/ui-egui/examples/ui_shot.rs` renders the window offscreen and runs a script of control
requests against it. No window opens, nothing steals focus, and it works with the screen locked,
so it is the first choice whenever a picture is all you need.

```sh
cat > /tmp/shot.jsonl <<'EOF'
{"method": "engine.execute", "params": {"command": "view.zoomIn", "params": {}}}
{"method": "ui.set", "params": {"path": "ribbonCollapsed", "value": true}}
{"method": "ui.screenshot", "params": {"path": "/tmp/ribbon-collapsed.png"}}
EOF
cargo run -p protocraft-ui-egui --example ui_shot -- --size 1440x920 --dark /tmp/shot.jsonl
```

Each line is a control request without the `id` (`docs/control-protocol.md`); one reply line is
printed per request. `ui.screenshot` is rendered by the harness and written as PNG at `--size`
times `--scale` (default 2, so 2880x1840 pixels). `--dark` starts in dark mode, `--empty` with a
blank document instead of the sample. Exit code 1 means a request failed. Open the PNG and look
at it; that is the point.

Keep screenshots out of the repository: write them to `/tmp` or a gitignored folder and delete
them before committing.

## The running app over the control channel

When only the real app will do (native menus, file dialogs, window geometry, multi-window), start
it with a control port and talk to it with the CLI:

```sh
PROTOCRAFT_NO_PREFS=1 cargo run --release -p protocraft -- --sample --control 7990
protocraft-cli control 7990 ui.inspect
protocraft-cli control 7990 engine.execute '{"command":"view.zoomIn","params":{}}'
protocraft-cli control 7990 ui.screenshot '{"path":"/tmp/zoomed.png"}'
protocraft-cli control 7990 app.quit
```

`control` takes `host:port` or just the port, prints the reply line, and exits 0 only when the
reply says `ok: true`. `control PORT -` forwards request lines from stdin, one reply each, for a
whole script. `PROTOCRAFT_NO_PREFS=1` keeps the run from reading or overwriting your own
preferences. `--control 0` picks a free port and prints it:

```sh
protocraft --control 0 --sample 2> /tmp/app.log &
PORT=$(grep -o 'listening on 127.0.0.1:[0-9]*' /tmp/app.log | sed 's/.*://')
```

### Window etiquette

The app opens a real window on the developer's desk. Keep it out of their way:

- Put it on a secondary monitor as soon as it is up. The windows agent is adding
  `PROTOCRAFT_WINDOW_POS=x,y` to the desktop app so the window opens where you say; until then,
  or on a build without it, move it right after launch:

  ```sh
  osascript -e 'tell application "System Events" to set position of every window of process "protocraft" to {-2500, 100}'
  ```

  (on this machine the left external monitor spans x -2560..0).
- Never leave the app open or a prompt waiting. End every run with `app.quit` over the control
  channel; it exits without asking. If a Save / Don't Save / Cancel prompt is showing (something
  closed a dirty window), dismiss it through the channel first: `ui.inspect` shows
  `"dialog": {"dialog": "quit"}`, then `ui.click` on Don't Save or `ui.key` as the dialog
  documents, then `app.quit`.
- Confirm nothing is left: `pgrep -fl protocraft || echo none`, and kill stragglers from your own
  target directory with `pkill -f 'target-<branch>.*protocraft'`.
- Prefer `ui_shot` over the real app whenever a screenshot is all you need.

## Parallel-agent etiquette

Several agents work in this repository at once, in separate worktrees (`AGENTS.md`).

- One `CARGO_TARGET_DIR` per worktree (`target-<branch>`; `/target-*/` is ignored). Sharing one
  produces lock contention and "file not found" errors mid-build. Delete yours when the branch is
  merged.
- One control port per agent, above 7990 and listed in your task, or `--control 0` and parse the
  port. Two apps on one port: the second fails to bind and runs without control, and your script
  talks to someone else's app.
- Your own screenshot paths (`/tmp/<branch>-*.png`), so two runs do not overwrite each other's
  pictures; delete them before committing.
- Edit only the files you own. A change you need elsewhere goes in your report to the
  orchestrator, not in someone else's module.
- Tests that open the real app do not belong in `cargo test`; they would collide on ports and
  windows. Use the kittest harness or `ui_shot` instead.
