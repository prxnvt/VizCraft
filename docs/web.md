# The web shell (`apps/vizcraft-web`)

The same `CraftApp` as the desktop app, run in the browser through eframe's web runner on
wgpu (WebGPU where the browser has it, WebGL2 otherwise). It is a workspace member but not a
default member: `cargo build` and `cargo test` leave it alone, and `cargo xtask wasm` (part of
`cargo xtask ci`) type-checks it for `wasm32-unknown-unknown` next to the engine and `ui-egui`.

## What is different in a browser

| Desktop | Web |
|---|---|
| Native file dialogs (`Services::pick_open`, `pick_save`) | `Services::open_async`: rfd clicks a hidden `<input type="file">`; the chosen file's bytes land in `Services::inbox` and the app loads them on the next frame. Dropped files take the same route. |
| Save writes to a path | `Services::download`: `file.save` and `file.saveAs` become a Blob download of the `.vizcraft` file (the browser picks the folder) |
| Several windows in one process | One document per page; New Window replaces the document (`CraftApp::new_window` already falls back when `multi_window` is false) |
| Native Apple macOS menu bar | The in-window menu bar (`menus::in_window_menus` is true off macOS, and wasm counts as off macOS) |
| `ui.json` in the user's config folder | `UiState` as JSON under the `localStorage` key `vizcraft.ui`, written by eframe's autosave and read at start; window geometry is left to the browser |
| Control channel, CLI | None (there is no TCP in a page) |

URL flags: `?sample` opens the sample document, `?webgl` forces the WebGL2 backend, `?noprefs`
ignores and does not write saved preferences.

## Building

The wasm target must be installed: `rustup target add wasm32-unknown-unknown`.

`apps/vizcraft-web/build.sh` is the packaging-free build. It runs
`cargo build --release --target wasm32-unknown-unknown -p vizcraft-web`, then the
`wasm-bindgen` CLI writes the JS glue and `.wasm` into `dist/web` next to a copy of
`index.html` (the trunk directive stripped, a two-line module loader added; `wasm-opt -Oz` runs
if present). `--debug` builds the dev profile. The CLI version must match the `wasm-bindgen`
crate in `Cargo.lock`; the script prints the exact `cargo install wasm-bindgen-cli --version ...`
line when the CLI is missing. Neither `wasm-bindgen`, `trunk` nor `wasm-pack` ship with the
toolchain, so a fresh machine needs that one install.

With trunk installed, `cd apps/vizcraft-web && trunk build --release` (or `trunk serve`, port
8772) does the same from `Trunk.toml`, output in `dist/web`.

## Serving

`dist/web` is static files; any file server works, for example:

```sh
python3 -m http.server -d dist/web 8772
```

then open `http://127.0.0.1:8772/?sample`. Serve over `http://localhost` or HTTPS: browsers
only expose WebGPU on secure origins, and WebGL2 is the fallback elsewhere.

## Checking without a browser

```sh
cargo check --target wasm32-unknown-unknown -p vizcraft-web
cargo clippy --target wasm32-unknown-unknown -p vizcraft-web -- -D warnings
cargo xtask wasm
```

Nothing in `engine` or `ui-egui` may depend on threads, files, sockets or the clock without a
`cfg(not(target_arch = "wasm32"))` branch; `cargo xtask wasm` is the check that keeps it so.

## Trademarks

Apple and macOS are trademarks of Apple Inc. The names are used only to identify the platform;
VizCraft is not affiliated with or endorsed by Apple Inc.
