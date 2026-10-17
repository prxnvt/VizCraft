//! Repository tasks: `cargo xtask <task>`.
//!
//! Pure Rust (std + serde_json). External tools (`cargo`, `git`) are invoked through
//! `std::process::Command`. Every task returns `Result<(), String>`; `main` prints the error and
//! exits 1.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod assets;
mod ico;
mod layers;
mod version;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  ci              fmt --check, clippy -D warnings, test, assets, layers, wasm (stops at first failure)
  layers          enforce the crate dependency layering (docs/architecture.md, Layers)
  assets          check that every file under assets/ and packaging/ has a row in ATTRIBUTION.md
  wasm            cargo check --target wasm32-unknown-unknown for the engine, ui-egui and *-web crates
  ico <out.ico> <in.png>...
                  pack square PNGs (<= 256 px) into a Windows .ico (used by packaging/icons.sh)
  version         print the workspace version and the git commit
  help            this text
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => cmd_ci(),
        Some("layers") => cmd_layers(),
        Some("assets") => assets::run(&root()),
        Some("wasm") => cmd_wasm().map(|_| ()),
        Some("ico") => ico::run(&rest),
        Some("version") => version::run(&root()),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprint!("{USAGE}");
            Err(format!("unknown task `{other}`"))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root: the parent of the xtask crate.
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf).unwrap_or_else(|| manifest.to_path_buf())
}

/// `cargo` as the invoking cargo (the `CARGO` variable), run from the workspace root.
pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

/// Run a command, echoing it, and turn a non-zero exit into an error.
pub fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    eprintln!("$ {what}");
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

/// `cargo metadata --no-deps` as JSON.
pub fn metadata() -> Result<serde_json::Value, String> {
    let out = cargo().args(["metadata", "--format-version", "1", "--no-deps"]).output().map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: bad JSON: {e}"))
}

fn cmd_layers() -> Result<(), String> {
    let crates = layers::from_metadata(&metadata()?)?;
    println!("Dependency layering (docs/architecture.md, Layers)\n");
    println!("{:<24} {:<10} dependencies (workspace, and the UI crates the rules watch)", "crate", "layer");
    for c in &crates {
        let shown: Vec<String> = c
            .deps
            .iter()
            .filter(|d| d.workspace || layers::watched(&d.name))
            .map(|d| {
                let k = match d.kind {
                    layers::DepKind::Normal => "",
                    layers::DepKind::Dev => " (dev)",
                    layers::DepKind::Build => " (build)",
                };
                format!("{}{k}", d.name)
            })
            .collect();
        println!("{:<24} {:<10} {}", c.name, layers::describe(layers::classify(c)), shown.join(", "));
    }
    let violations = layers::check(&crates);
    println!();
    if violations.is_empty() {
        println!("OK: {} crates, no layering violations.", crates.len());
        Ok(())
    } else {
        println!("{} violation(s):", violations.len());
        for v in &violations {
            println!("  - {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

/// How a CI step ended when it did not fail.
enum Outcome {
    Passed,
    Skipped(String),
}

/// Workspace crates that must build for the web: the engine, the egui front end and the web
/// shell(s) (`apps/*-web`, which are not default members and so are only ever built here).
fn wasm_set() -> Result<Vec<String>, String> {
    let crates = layers::from_metadata(&metadata()?)?;
    let set: Vec<String> = crates
        .iter()
        .filter(|c| matches!(layers::classify(c), Some(layers::Class::Engine | layers::Class::UiEgui)) || c.name.ends_with("-web"))
        .map(|c| c.name.clone())
        .collect();
    if set.is_empty() { Err("wasm: no engine or ui-egui crate in the workspace".into()) } else { Ok(set) }
}

/// Whether the wasm32 standard library is installed for the active toolchain.
fn wasm_target_installed() -> bool {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let Ok(out) = Command::new(rustc).args(["--print", "sysroot"]).output() else { return false };
    let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Path::new(&sysroot).join("lib/rustlib/wasm32-unknown-unknown/lib").is_dir()
}

fn cmd_wasm() -> Result<Outcome, String> {
    if !wasm_target_installed() {
        let msg = "wasm32-unknown-unknown target not installed (rustup target add wasm32-unknown-unknown)";
        println!("wasm: skipped: {msg}");
        return Ok(Outcome::Skipped(msg.into()));
    }
    let set = wasm_set()?;
    let mut c = cargo();
    c.args(["check", "--target", "wasm32-unknown-unknown"]);
    for pkg in &set {
        c.args(["-p", pkg]);
    }
    let what = format!("cargo check --target wasm32-unknown-unknown -p {}", set.join(" -p "));
    run(c, &what)?;
    println!("wasm: ok ({})", set.join(", "));
    Ok(Outcome::Passed)
}

fn cmd_ci() -> Result<(), String> {
    type Step = (&'static str, Box<dyn Fn() -> Result<Outcome, String>>);
    let pass = |r: Result<(), String>| r.map(|()| Outcome::Passed);
    let steps: Vec<Step> = vec![
        (
            "fmt",
            Box::new(move || {
                let mut c = cargo();
                c.args(["fmt", "--all", "--", "--check"]);
                pass(run(c, "cargo fmt --all -- --check"))
            }),
        ),
        (
            "clippy",
            Box::new(move || {
                let mut c = cargo();
                c.args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
                pass(run(c, "cargo clippy --workspace --all-targets -- -D warnings"))
            }),
        ),
        (
            "test",
            Box::new(move || {
                let mut c = cargo();
                c.args(["test", "--workspace"]);
                pass(run(c, "cargo test --workspace"))
            }),
        ),
        ("assets", Box::new(move || pass(assets::run(&root())))),
        ("layers", Box::new(move || pass(cmd_layers()))),
        ("wasm", Box::new(cmd_wasm)),
    ];
    let mut done: Vec<(&str, Outcome)> = Vec::new();
    for (name, f) in &steps {
        eprintln!("\n=== ci: {name} ===");
        match f() {
            Ok(outcome) => done.push((name, outcome)),
            Err(e) => {
                println!("\nCI summary:");
                print_done(&done);
                println!("  FAIL  {name}: {e}");
                for (n, _) in steps.iter().skip(done.len() + 1) {
                    println!("  skip  {n}");
                }
                return Err(format!("ci failed at `{name}`"));
            }
        }
    }
    println!("\nCI summary: all {} steps passed", done.len());
    print_done(&done);
    Ok(())
}

fn print_done(done: &[(&str, Outcome)]) {
    for (name, outcome) in done {
        match outcome {
            Outcome::Passed => println!("  ok    {name}"),
            Outcome::Skipped(why) => println!("  skip  {name}: {why}"),
        }
    }
}
