//! VizCraft in the browser.
//!
//! Runs the same [`vizcraft_ui_egui::CraftApp`] as the desktop app through eframe's web runner
//! (wgpu: WebGPU where available, WebGL2 otherwise). Build with `apps/vizcraft-web/build.sh`
//! (or `trunk build --release` from that directory); see `docs/web.md`.
//!
//! Differences from the desktop app: one document per page (New Window replaces the document),
//! no native menu bar (the in-window bar is drawn), no TCP control channel, Open uses the browser
//! file picker (bytes arrive through `Services::inbox`), Save downloads the document, and
//! preferences live in `localStorage`. URL flags: `?webgl` forces WebGL2, `?sample` opens the
//! sample document, `?noprefs` ignores saved preferences.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vizcraft-web only runs in the browser: build it with apps/vizcraft-web/build.sh (docs/web.md)");
}
