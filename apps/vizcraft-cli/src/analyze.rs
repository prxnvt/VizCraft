//! `vizcraft-cli analyze`: run a graph analysis or a model compare over a document without a
//! window.
//!
//! ```text
//! vizcraft-cli analyze FILE --kind KIND [--id ID] [--from ID] [--to ID] [--direction D] [--depth N]
//!                      [--types T,T] [--against FILE] [--check] [--json]
//! ```
//!
//! KIND is `dependencies`, `cycles`, `unreachable`, `spof`, `metrics`, `path`, `diff`, or any
//! analysis a pack adds. `diff` compares FILE's model (as after) with the one in `--against` (as
//! before). `--check` is for CI: it exits 1 when cycles, unreachable elements, single points of
//! failure or differences are found.
//!
//! Exit codes: 0 done (and, with `--check`, nothing found), 1 something was found or a file or
//! analysis failed, 2 a usage mistake.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use vizcraft_engine::cmd::analyze;
use vizcraft_engine::{Document, Session};

use crate::{CliError, failed, usage};

/// Parsed arguments.
#[derive(Default)]
struct Args {
    file: String,
    kind: String,
    id: Option<String>,
    from: Option<String>,
    to: Option<String>,
    direction: Option<String>,
    depth: Option<u32>,
    types: Option<Vec<String>>,
    against: Option<String>,
    check: bool,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut a = Args::default();
    let mut file = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| usage(format!("{name} needs a value")));
        match arg.as_str() {
            "--kind" => a.kind = value("--kind")?,
            "--id" => a.id = Some(value("--id")?),
            "--from" => a.from = Some(value("--from")?),
            "--to" => a.to = Some(value("--to")?),
            "--direction" => a.direction = Some(value("--direction")?),
            "--depth" => {
                let d = value("--depth")?;
                a.depth = Some(d.parse().map_err(|_| usage(format!("analyze: --depth takes a whole number, not `{d}`")))?);
            }
            "--types" => a.types = Some(value("--types")?.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).collect()),
            "--against" => a.against = Some(value("--against")?),
            "--check" => a.check = true,
            "--json" => a.json = true,
            other if other.starts_with("--") => return Err(usage(format!("analyze: unknown option `{other}`"))),
            other => {
                if file.is_some() {
                    return Err(usage(format!("analyze: unexpected argument `{other}`")));
                }
                file = Some(other.to_string());
            }
        }
    }
    a.file = file.ok_or_else(|| usage("analyze needs a document"))?;
    if a.kind.is_empty() {
        return Err(usage("analyze needs --kind"));
    }
    let kinds = analyze::kinds();
    if a.kind != "diff" && !kinds.contains(&a.kind.as_str()) {
        return Err(usage(format!("analyze: unknown kind `{}` (one of {}, diff)", a.kind, kinds.join(", "))));
    }
    if a.kind == "diff" && a.against.is_none() {
        return Err(usage("analyze: --kind diff needs --against FILE"));
    }
    if a.kind != "diff" && a.against.is_some() {
        return Err(usage("analyze: --against goes with --kind diff"));
    }
    if a.check && matches!(a.kind.as_str(), "dependencies" | "metrics" | "path") {
        return Err(usage(format!("analyze: --check does not apply to {}", a.kind)));
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

/// The params the flags ask for.
fn params(a: &Args) -> Value {
    let mut p = Map::new();
    if let Some(id) = &a.id {
        p.insert("id".into(), json!(id));
    }
    if let Some(from) = &a.from {
        // Unreachable takes a list of roots; the others take one element.
        let v = if a.kind == "unreachable" { json!(from.split(',').map(str::trim).collect::<Vec<_>>()) } else { json!(from) };
        p.insert("from".into(), v);
    }
    if let Some(to) = &a.to {
        p.insert("to".into(), json!(to));
    }
    if let Some(d) = &a.direction {
        p.insert("direction".into(), json!(d));
    }
    if let Some(d) = a.depth {
        p.insert("depth".into(), json!(d));
    }
    if let Some(t) = &a.types {
        p.insert("types".into(), json!(t));
    }
    if let Some(f) = &a.against {
        p.insert("against".into(), json!("file"));
        p.insert("path".into(), json!(absolute(f)));
    }
    Value::Object(p)
}

fn strings(r: &Value, k: &str) -> Vec<String> {
    r.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// Whether the result found something a `--check` fails on.
fn found(kind: &str, r: &Value) -> bool {
    if kind == "diff" {
        return ["elements", "relationships"].iter().any(|k| {
            ["added", "removed", "changed"].iter().any(|c| r.get(k).and_then(|x| x.get(c)).and_then(Value::as_array).is_some_and(|a| !a.is_empty()))
        });
    }
    !strings(r, "elements").is_empty() || !strings(r, "relationships").is_empty()
}

fn print_text(kind: &str, r: &Value) {
    if kind == "diff" {
        if let Some(s) = r.get("summary").and_then(Value::as_str) {
            println!("{s}");
        }
        for k in ["elements", "relationships"] {
            for id in strings(r.get(k).unwrap_or(&Value::Null), "added") {
                println!("  added    {id}");
            }
            for id in strings(r.get(k).unwrap_or(&Value::Null), "removed") {
                println!("  removed  {id}");
            }
            for c in r.get(k).and_then(|x| x.get("changed")).and_then(Value::as_array).into_iter().flatten() {
                let fields: Vec<&str> =
                    c.get("fields").and_then(Value::as_array).into_iter().flatten().filter_map(|f| f.get("field")?.as_str()).collect();
                println!("  changed  {} ({})", c.get("id").and_then(Value::as_str).unwrap_or_default(), fields.join(", "));
            }
        }
        return;
    }
    if let Some(s) = r.get("summary").and_then(Value::as_str) {
        println!("{s}");
    }
    let name = |id: &str| r.get("names").and_then(|n| n.get(id)).and_then(Value::as_str).unwrap_or(id).to_string();
    if let Some(rows) = r.get("rows").and_then(Value::as_array) {
        println!("  {:<24} {:>6} {:>7} {:>6} {:>6}", "element", "fanIn", "fanOut", "layer", "depth");
        for row in rows {
            let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
            let n = |k: &str| row.get(k).and_then(Value::as_u64).unwrap_or(0);
            println!("  {:<24} {:>6} {:>7} {:>6} {:>6}", name(id), n("fanIn"), n("fanOut"), n("layer"), n("containmentDepth"));
        }
        return;
    }
    for id in strings(r, "elements") {
        println!("  {id}  {}", name(&id));
    }
    for id in strings(r, "relationships") {
        println!("  {id}  {} (relationship)", name(&id));
    }
}

/// `analyze FILE --kind KIND [options] [--check] [--json]`.
pub fn analyze(args: &[String]) -> Result<(), CliError> {
    let a = parse(args)?;
    let mut s = Session::new(Document::new());
    s.run("file.open", &json!({"path": absolute(&a.file)})).map_err(|e| failed(format!("{}: {e}", a.file)))?;
    let p = params(&a);
    let command = if a.kind == "diff" { "model.diff".to_string() } else { format!("analyze.{}", a.kind) };
    let r = if s.registry.get(&command).is_some() {
        s.run(&command, &p)
    } else {
        // An analysis a pack added without its own command.
        analyze::run_kind(&mut s, &a.kind, &p)
    }
    .map_err(|e| failed(e.to_string()))?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&r).map_err(|e| failed(e.to_string()))?);
    } else {
        print_text(&a.kind, &r);
    }
    if a.check && found(&a.kind, &r) {
        return Err(failed(format!("analyze --check: the {} analysis found something", a.kind)));
    }
    Ok(())
}
