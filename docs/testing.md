# Testing VizCraft

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

GitHub Actions runs the same command on every push to `main` and every pull request
(`.github/workflows/rust.yml`, one Ubuntu job): it installs the X11, Wayland, xkbcommon and GL
development packages that eframe's winit backends link against, a stable toolchain with rustfmt,
clippy and the `wasm32-unknown-unknown` target so the wasm step runs instead of skipping itself,
caches the cargo registry and `target/` with `Swatinem/rust-cache`, and sets
`VIZCRAFT_SKIP_SNAPSHOTS` so the pixel snapshot tests stay out of the gate there too. A green
run means exactly what a green `cargo xtask ci` means on your machine, nothing more.

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
cargo run -p vizcraft-ui-egui --example ui_shot -- --size 1440x920 --dark /tmp/shot.jsonl
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
VIZCRAFT_NO_PREFS=1 cargo run --release -p vizcraft -- --sample --control 7990
vizcraft-cli control 7990 ui.inspect
vizcraft-cli control 7990 engine.execute '{"command":"view.zoomIn","params":{}}'
vizcraft-cli control 7990 ui.screenshot '{"path":"/tmp/zoomed.png"}'
vizcraft-cli control 7990 app.quit
```

`control` takes `host:port` or just the port, prints the reply line, and exits 0 only when the
reply says `ok: true`. `control PORT -` forwards request lines from stdin, one reply each, for a
whole script. `VIZCRAFT_NO_PREFS=1` keeps the run from reading or overwriting your own
preferences. `--control 0` picks a free port and prints it:

```sh
vizcraft --control 0 --sample 2> /tmp/app.log &
PORT=$(grep -o 'listening on 127.0.0.1:[0-9]*' /tmp/app.log | sed 's/.*://')
```

### Desktop app etiquette

The app opens a real window on the developer's desk. Keep it out of their way:

- Put it on a secondary monitor. `--window-pos x,y` opens the first window with its top-left
  corner there, overriding the saved geometry; a window opened later (Cmd+N) cascades from the
  one in front, so move every window after one opens:

  ```sh
  VIZCRAFT_NO_PREFS=1 vizcraft --sample --control 7990 --window-pos -2500,100
  osascript -e 'tell application "System Events" to tell process "vizcraft" to set position of every window to {-2500, 100}'
  ```

  (on this machine the left external monitor spans x -2560..0).
- Never leave the app open or a prompt waiting. End every run with `app.quit` over the control
  channel; it exits without asking. If a Save / Don't Save / Cancel prompt is showing (something
  closed a dirty window), dismiss it through the channel first: `ui.inspect` shows
  `"dialog": {"dialog": "quit"}`, then `ui.click` on Don't Save or `ui.key` as the dialog
  documents, then `app.quit`.
- Confirm nothing is left: `pgrep -fl vizcraft || echo none`, and kill stragglers from your own
  target directory with `pkill -f 'target-<branch>.*vizcraft'`.
- Prefer `ui_shot` over the real app whenever a screenshot is all you need.

## Performance trace

When the app feels slow or uses too much memory, run it with the trace on:

```sh
cargo run --release -p vizcraft -- --sample --trace
```

`--trace` (or `VIZCRAFT_TRACE=1`) writes into `traces/` at the top of the repository (ignored by
git; `VIZCRAFT_TRACE_DIR` chooses another folder). Press Cmd+Shift+M (Ctrl+Shift+M elsewhere;
`ui.traceMark`) at the moment something feels wrong; the mark lands on the timeline and in the
summary.

| File | Contents |
|---|---|
| `vizcraft-<time>-<pid>.summary.json` | Rewritten every 5 seconds and at exit. `session` (build commit, debug or release, GPU adapter and backend, present mode, display scale and predicted frame interval, control port, flags). Frame time; the gap between frames, overall and split into `frameGapPendingMs` (the previous frame asked for an immediate repaint, so one display refresh is normal and more is a stall) and `frameGapIdleMs` (it did not: idle, or a timer). Frames with no input. Every span's count, total, mean, p95 and worst, including `eframe.turnaround` (pending gap minus frame time: eframe, winit, compositor and OS between two frames the app wanted back to back). The 25 slowest frames with their span trees. `stalls` counts and `longestStalls` (below). Repaint causes by source line. `notes` (wakes from outside the UI thread: control-channel requests by method, menu events, open requests). Memory once a second (process RSS, document and undo snapshot estimates, egui textures and text caches). Marks |
| `vizcraft-<time>-<pid>.trace.json` | Chrome trace events, opened in [Perfetto](https://ui.perfetto.dev) or `chrome://tracing`. Every frame is an event with its input by kind (`inputKinds`), how many events the control channel injected (`synthetic`), whether the UI asked to be drawn again (`repaintPending`) and the viewport commands it sent (`ticks`); frames slower than `VIZCRAFT_TRACE_DETAIL_MS` (default 8) carry their nested spans; stalls are events; memory is a counter track. The array is closed at exit; a killed app leaves it open, which both viewers accept |
| `vizcraft-<time>-<pid>-stall-<n>.txt` | macOS only: the process's native stacks, captured with the system `sample` tool for one second while the main thread was stalled (at most 10 per run; `VIZCRAFT_TRACE_NO_SAMPLE=1` turns it off). Look for the main thread: that is where the time outside the app's own code goes |

A stall is a gap in which the UI asked to be drawn again and no frame began within
`VIZCRAFT_TRACE_STALL_MS` (default 100; `0` turns the watchdog off). A watchdog thread notices it
and starts the stack capture; the next frame closes it with the span tree and repaint causes of
the frame before, and the input kinds of the frame after. A frame that arrives carrying more than
four pointer moves is a stall too (`pileup`): the pointer kept moving while no frame ran. Each
stall says what woke the frame after it (`wake`): `immediate` means the UI had asked for a frame
at once and it came late, which is a real stall; `external` means something outside the UI
thread woke the app (a control-channel request, a native menu event, a Finder open request; each
leaves a note), so the gap was somebody else's pause; `timer` means a delayed repaint request (a tooltip
delay, the status bar's one-second timer), whose delay egui does not disclose, so the gap may
have been a legitimate wait. `stalls.unconfirmed` counts everything but `immediate`, and only
`immediate` gaps feed `frameGapPendingMs`.

Spans cover each command (`cmd <id>`) and its phases (document clone and compare, model sync,
theme follow-up, page auto-size, connector rerouting, undo record, issue refresh), the window
sections (title bar, ribbon, panes one by one, canvas, dialogs), the canvas phases (drag
preview, grid, background pages, shapes, connector geometry with line jumps, connectors,
overlays, rulers) and the host calls that block the UI thread (`blocking.pickOpen`,
`blocking.pickSave`, `blocking.print`). `eframe.previousFrameCpu` is eframe's own measure of
layout, tessellation and paint, which the spans cannot see; it leaves out the wait for the GPU
surface, which only the gap statistics and the stall captures show. Tracing is off unless asked
for and costs one atomic load per span when off (`vizcraft_engine::trace`).

### Running a lag test

1. Build and run the release binary: `cargo run --release -p vizcraft -- --sample --trace`. The
   summary's `session.debugAssertions` must read `false`; a debug build lags on its own.
2. Leave the control channel off, or accept that its requests show as `notes` and its input as
   `synthetic`, so a scripted run is never mistaken for a person.
3. When it lags, press Cmd+Shift+M, then keep moving the pointer for a second: the queued moves
   turn the lag into a `pileup` stall with the frame before it attached.
4. Quit normally so the trace is closed. Read `longestStalls` first, then the `-stall-<n>.txt`
   capture named by a confirmed stall, then `frameGapPendingMs` against `frameMs`.

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

## Snapshot tests

`crates/ui-egui/tests/snapshots.rs` renders the whole window offscreen (egui_kittest with wgpu,
1280x800 at 1 pixel per point) and compares it pixel by pixel with the PNGs in
`crates/ui-egui/tests/snapshots/<os>/`. One test, eight pictures: the Home tab in light and dark
mode, the View tab's ribbon, the in-window Edit menu open, the collapsed ribbon's popup, both pane
sides folded, the About dialog and the command palette. They are test fixtures, not assets, so
they need no `ATTRIBUTION.md` rows (`cargo xtask assets` scans `assets/` and `packaging/` only).

The test is part of `cargo test` and so of `cargo xtask ci`, and it passes with a note instead of
failing when it cannot run: no wgpu adapter on the machine, `VIZCRAFT_SKIP_SNAPSHOTS=1` in the
environment, or no reference folder for this OS yet (shortcut text is platform-specific, so
macOS references do not match Linux or Windows; the first run on a new OS records its own set).

When a snapshot differs the test fails and writes `<name>.new.png` and `<name>.diff.png` next to
the reference (both gitignored). Look at the diff; if the change is intended, record the new
references and commit the PNGs with the code that changed them:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p vizcraft-ui-egui --test snapshots
```

`UPDATE_SNAPSHOTS=1` rewrites only the snapshots that failed; `UPDATE_SNAPSHOTS=force` rewrites
all of them. Keep the set small (under 3 MB in total) and at 1 pixel per point; `ui_shot` is the
tool for high-resolution pictures to look at.
