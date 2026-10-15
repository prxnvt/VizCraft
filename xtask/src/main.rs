//! `cargo xtask <task>`. Cascade 0 stub; the tasks land in cascade 1.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "" | "help" => {
            println!("cargo xtask <ci|layers|assets|wasm|new-craft>");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("xtask: unknown task `{other}`");
            ExitCode::FAILURE
        }
    }
}
