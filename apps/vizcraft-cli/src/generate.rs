//! `vizcraft-cli generate`: run a generator over a document's model without a window.
//!
//! ```text
//! vizcraft-cli generate FILE [--spec SPEC | --target ID --output DIR] [--check | --preview] [--renames] [--json]
//! ```
//!
//! FILE is a document (any format `file.open` reads). The generator is a spec file (`--spec`), an
//! inline target and output folder, or, with neither, the `.vizcraft-gen.toml` beside FILE.
//! Relative `--spec` and `--output` paths are taken from the current folder. `--check` compares
//! the output folder with what a run would write (for CI), `--preview` prints the plan and its
//! diffs, and with neither the output is generated.
//!
//! Exit codes: 0 done (or the output is current), 1 a check found stale or missing output, a run
//! stopped on a conflict, or a file could not be read, 2 for a usage mistake.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};

use crate::{CliError, failed, usage};

/// What to do with the plan.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Run,
    Check,
    Preview,
}

/// Parsed arguments.
struct Args {
    file: String,
    spec: Option<String>,
    target: Option<String>,
    output: Option<String>,
    mode: Mode,
    renames: bool,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut a = Args { file: String::new(), spec: None, target: None, output: None, mode: Mode::Run, renames: false, json: false };
    let mut file = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| usage(format!("{name} needs a value")));
        match arg.as_str() {
            "--spec" => a.spec = Some(value("--spec")?),
            "--target" => a.target = Some(value("--target")?),
            "--output" => a.output = Some(value("--output")?),
            "--check" if a.mode == Mode::Run => a.mode = Mode::Check,
            "--preview" if a.mode == Mode::Run => a.mode = Mode::Preview,
            "--check" | "--preview" => return Err(usage("generate: give --check or --preview, not both")),
            "--renames" => a.renames = true,
            "--json" => a.json = true,
            other if other.starts_with("--") => return Err(usage(format!("generate: unknown option `{other}`"))),
            other => {
                if file.is_some() {
                    return Err(usage(format!("generate: unexpected argument `{other}`")));
                }
                file = Some(other.to_string());
            }
        }
    }
    a.file = file.ok_or_else(|| usage("generate needs a document"))?;
    match (&a.spec, &a.target, &a.output) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => return Err(usage("generate: give --spec, or --target and --output, not both")),
        (None, Some(_), None) => return Err(usage("generate: --target needs --output")),
        (None, None, Some(_)) => return Err(usage("generate: --output needs --target")),
        _ => {}
    }
    Ok(a)
}

/// `p` made absolute against the current folder.
fn absolute(p: &str) -> String {
    let path = Path::new(p);
    if path.is_absolute() {
        return p.to_string();
    }
    std::env::current_dir().map(|d| d.join(path)).unwrap_or_else(|_| PathBuf::from(p)).to_string_lossy().into_owned()
}

/// `generate FILE [--spec SPEC | --target ID --output DIR] [--check | --preview] [--renames] [--json]`.
pub fn generate(args: &[String]) -> Result<(), CliError> {
    let a = parse(args)?;
    let mut s = Session::new(Document::new());
    s.run("file.open", &json!({"path": absolute(&a.file)})).map_err(|e| failed(format!("{}: {e}", a.file)))?;
    let mut params = serde_json::Map::new();
    match (&a.spec, &a.target, &a.output) {
        (Some(spec), _, _) => {
            params.insert("spec".into(), json!(absolute(spec)));
        }
        (None, Some(t), Some(o)) => {
            params.insert("target".into(), json!(t));
            params.insert("output".into(), json!(absolute(o)));
        }
        _ => {
            let beside = Path::new(&absolute(&a.file)).parent().map(|d| d.join(".vizcraft-gen.toml")).unwrap_or_default();
            if !beside.is_file() {
                return Err(usage("generate: give --spec, or --target and --output (no .vizcraft-gen.toml beside the document)"));
            }
            params.insert("spec".into(), json!(beside.to_string_lossy()));
        }
    }
    if a.renames {
        params.insert("renames".into(), json!(true));
    }
    let params = Value::Object(params);
    let id = match a.mode {
        Mode::Run => "generate.run",
        Mode::Check => "generate.check",
        Mode::Preview => "generate.preview",
    };
    let r = s.run(id, &params).map_err(|e| failed(e.to_string()))?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&r).map_err(|e| failed(e.to_string()))?);
    } else {
        print_text(a.mode, &r);
    }
    match a.mode {
        Mode::Check if r.get("ok").and_then(Value::as_bool) != Some(true) => {
            Err(failed("generated output is stale; run `vizcraft-cli generate` to bring it up to date"))
        }
        Mode::Run if r.get("stopped").and_then(Value::as_bool) == Some(true) => {
            Err(failed("generation stopped: hand edits conflict with the new output"))
        }
        _ => Ok(()),
    }
}

fn strings(r: &Value, k: &str) -> Vec<String> {
    r.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn print_text(mode: Mode, r: &Value) {
    match mode {
        Mode::Run => {
            for (k, word) in [("written", "wrote"), ("merged", "merged"), ("deleted", "deleted"), ("unchanged", "unchanged")] {
                for p in strings(r, k) {
                    println!("{word:<9} {p}");
                }
            }
        }
        Mode::Check => {
            for k in ["stale", "missing", "modified"] {
                for p in strings(r, k) {
                    println!("{k:<9} {p}");
                }
            }
            if r.get("ok").and_then(Value::as_bool) == Some(true) {
                println!("generated output is current");
            }
        }
        Mode::Preview => {
            for f in r.get("files").and_then(Value::as_array).into_iter().flatten() {
                let field = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                println!("{:<9} {}", field("action"), field("path"));
                print!("{}", field("diff"));
            }
            for rn in r.get("renames").and_then(Value::as_array).into_iter().flatten() {
                let field = |k: &str| rn.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                println!("rename    {}: {} -> {}", field("id"), field("from"), field("to"));
            }
        }
    }
    for i in r.get("issues").and_then(Value::as_array).into_iter().flatten() {
        let field = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        eprintln!("{:<7} {}: {}", field("severity"), field("rule"), field("message"));
    }
}
