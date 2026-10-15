//! BaseCraft command line: the engine without a window.
//!
//! ```text
//! basecraft-cli commands                       table of id, label, shortcut, enabled
//! basecraft-cli run [--open FILE] --cmd ID[=JSON] ... [--save OUT]
//! basecraft-cli --version
//! ```
//!
//! `run` opens a document (or starts blank), runs each command in order, prints each result as
//! a JSON line and optionally saves. A failing command stops the run with exit code 1.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::process::ExitCode;

use basecraft_engine::{Document, Session};
use serde_json::{Value, json};

const USAGE: &str = "basecraft-cli commands | run [--open FILE] --cmd ID[=JSON] ... [--save OUT] | --version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("commands") => commands(),
        Some("run") => run(&args[1..]),
        Some("--version") => {
            println!("basecraft-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Every command, as a fixed-width table (enablement is for a fresh blank document).
fn commands() -> ExitCode {
    let s = Session::new(Document::new());
    let infos = s.commands();
    let w_id = infos.iter().map(|c| c.id.len()).max().unwrap_or(2).max(2);
    let w_label = infos.iter().map(|c| c.label.len()).max().unwrap_or(5).max(5);
    let w_sc = infos.iter().map(|c| c.shortcut.len()).max().unwrap_or(8).max(8);
    println!("{:<w_id$}  {:<w_label$}  {:<w_sc$}  enabled", "id", "label", "shortcut");
    for c in infos {
        let enabled = if c.enabled { "yes".to_string() } else { format!("no ({})", c.reason) };
        println!("{:<w_id$}  {:<w_label$}  {:<w_sc$}  {enabled}", c.id, c.label, c.shortcut);
    }
    ExitCode::SUCCESS
}

/// `run [--open FILE] --cmd ID[=JSON] ... [--save OUT]`.
fn run(args: &[String]) -> ExitCode {
    let mut open: Option<String> = None;
    let mut save: Option<String> = None;
    let mut cmds: Vec<(String, Value)> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--open" => open = it.next().cloned(),
            "--save" => save = it.next().cloned(),
            "--cmd" => {
                let Some(spec) = it.next() else {
                    eprintln!("--cmd needs ID[=JSON]");
                    return ExitCode::from(2);
                };
                let (id, params) = match spec.split_once('=') {
                    Some((id, js)) => match serde_json::from_str::<Value>(js) {
                        Ok(v) => (id.to_string(), v),
                        Err(e) => {
                            eprintln!("{id}: bad JSON params: {e}");
                            return ExitCode::from(2);
                        }
                    },
                    None => (spec.clone(), json!({})),
                };
                cmds.push((id, params));
            }
            other => {
                eprintln!("unknown argument {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let mut s = Session::new(Document::new());
    if let Some(path) = open
        && let Err(e) = s.run("file.open", &json!({"path": path}))
    {
        eprintln!("open: {e}");
        return ExitCode::FAILURE;
    }
    for (id, params) in cmds {
        match s.run(&id, &params) {
            Ok(v) => println!("{}", json!({"command": id, "ok": true, "result": v})),
            Err(e) => {
                println!("{}", json!({"command": id, "ok": false, "error": e.to_string()}));
                return ExitCode::FAILURE;
            }
        }
        // A command may ask the UI for something; headless there is none, so say so.
        for req in std::mem::take(&mut s.ui_requests) {
            eprintln!("{id}: UI request ignored: {req}");
        }
    }
    if let Some(path) = save
        && let Err(e) = s.run("file.save", &json!({"path": path}))
    {
        eprintln!("save: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
