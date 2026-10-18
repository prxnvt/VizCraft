//! `vizcraft-cli validate`: the rules engine without a window.
//!
//! ```text
//! vizcraft-cli validate FILE|DIR [--ruleset ID]... [--format text|json] [--packs DIR]
//! ```
//!
//! FILE is a document (any format `file.open` reads) or a pack test case (`{"document",
//! "profile"?, "expect"}`); DIR is a folder of test cases (`packs/tests/<pack>`). A document is
//! checked with every live and batch rule its conformance profile selects, or with the named
//! rulesets; a case or folder of cases is run against its expectations. Packs come from the
//! binary, from the folder in `VIZCRAFT_PACKS_DIR`, and from `--packs DIR`.
//!
//! Exit codes: 0 when the document has no errors (warnings and info do not count) or every case
//! passed, 1 otherwise, 2 for a usage mistake.

use std::path::Path;

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};
use vizcraft_notation::cases;

use crate::{CliError, failed, usage};

/// How results are printed.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Text,
    Json,
}

/// Parsed arguments.
struct Args {
    target: String,
    rulesets: Vec<String>,
    format: Format,
    packs: Option<String>,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut target: Option<String> = None;
    let mut rulesets = Vec::new();
    let mut format = Format::Text;
    let mut packs = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ruleset" => rulesets.push(it.next().ok_or_else(|| usage("--ruleset needs a ruleset id"))?.clone()),
            "--format" => {
                format = match it.next().map(String::as_str) {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    _ => return Err(usage("--format is text or json")),
                }
            }
            "--json" => format = Format::Json,
            "--packs" => packs = Some(it.next().ok_or_else(|| usage("--packs needs a folder"))?.clone()),
            other if other.starts_with("--") => return Err(usage(format!("validate: unknown option `{other}`"))),
            other => {
                if target.is_some() {
                    return Err(usage(format!("validate: unexpected argument `{other}`")));
                }
                target = Some(other.to_string());
            }
        }
    }
    let target = target.ok_or_else(|| usage("validate needs a file or a folder of test cases"))?;
    Ok(Args { target, rulesets, format, packs })
}

/// `validate FILE|DIR [--ruleset ID]... [--format text|json] [--packs DIR]`.
pub fn validate(args: &[String]) -> Result<(), CliError> {
    let a = parse(args)?;
    if let Some(dir) = &a.packs {
        let cat = vizcraft_notation::catalog::use_packs_dir(Path::new(dir)).map_err(failed)?;
        for e in cat.errors() {
            eprintln!("vizcraft-cli: pack: {e}");
        }
    }
    let path = Path::new(&a.target);
    if path.is_dir() {
        if !a.rulesets.is_empty() {
            return Err(usage("--ruleset does not apply to test cases; each case names its profile"));
        }
        let outcomes = cases::run_dir(path, vizcraft_notation::catalog());
        if outcomes.is_empty() {
            return Err(failed(format!("{}: no test cases (*.json) in the folder", a.target)));
        }
        return report_cases(&a, &outcomes);
    }
    // A test case is JSON with a document and expectations; anything else is a document.
    let case_text = cases::read_capped(path).ok().filter(|t| serde_json::from_str::<Value>(t).is_ok_and(|v| cases::is_case(&v)));
    if let Some(text) = case_text {
        if !a.rulesets.is_empty() {
            return Err(usage("--ruleset does not apply to a test case; the case names its profile"));
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| a.target.clone());
        let outcome = cases::run_text(&name, &text, vizcraft_notation::catalog());
        return report_cases(&a, std::slice::from_ref(&outcome));
    }
    document(&a)
}

/// Check one document and print its issues; an error-severity issue fails.
fn document(a: &Args) -> Result<(), CliError> {
    let mut s = Session::new(Document::new());
    s.run("file.open", &json!({"path": a.target})).map_err(|e| failed(format!("{}: {e}", a.target)))?;
    if !a.rulesets.is_empty() {
        s.run("validate.setProfile", &json!({"enabled": true, "rulesets": a.rulesets})).map_err(|e| failed(e.to_string()))?;
    }
    let r = s.run("validate.run", &json!({})).map_err(|e| failed(e.to_string()))?;
    let issues = r.get("issues").and_then(Value::as_array).cloned().unwrap_or_default();
    let count = |sev: &str| issues.iter().filter(|i| i.get("severity").and_then(Value::as_str) == Some(sev)).count();
    let (errors, warnings, infos) = (count("error"), count("warning"), count("info"));
    let profile = s.run("validate.setProfile", &json!({})).ok().and_then(|p| p.get("profile").cloned()).unwrap_or(Value::Null);
    match a.format {
        Format::Json => {
            let out = json!({"file": a.target, "profile": profile, "issues": issues, "errors": errors, "warnings": warnings, "infos": infos});
            println!("{}", serde_json::to_string_pretty(&out).map_err(|e| failed(e.to_string()))?);
        }
        Format::Text => {
            for i in &issues {
                let field = |k: &str| i.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                let items: Vec<&str> =
                    i.get("items").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let page = i.get("page").and_then(Value::as_str).map(|p| format!(" {p}")).unwrap_or_default();
                println!("{:<7} {}{page} [{}]: {}", field("severity"), field("rule"), items.join(", "), field("message"));
            }
            println!("{}: {errors} errors, {warnings} warnings, {infos} info", a.target);
        }
    }
    if errors > 0 { Err(failed(format!("{}: {errors} errors", a.target))) } else { Ok(()) }
}

/// Print case outcomes; any failure fails.
fn report_cases(a: &Args, outcomes: &[cases::Outcome]) -> Result<(), CliError> {
    let failed_n = outcomes.iter().filter(|o| !o.passed()).count();
    match a.format {
        Format::Json => {
            let list: Vec<Value> =
                outcomes.iter().map(|o| json!({"name": o.name, "passed": o.passed(), "problems": o.problems, "issues": o.issues})).collect();
            let out = json!({"cases": list, "passed": outcomes.len() - failed_n, "failed": failed_n});
            println!("{}", serde_json::to_string_pretty(&out).map_err(|e| failed(e.to_string()))?);
        }
        Format::Text => {
            for o in outcomes {
                println!("{} {}", if o.passed() { "pass" } else { "FAIL" }, o.name);
                for p in &o.problems {
                    println!("       {p}");
                }
            }
            println!("{} cases: {} passed, {failed_n} failed", outcomes.len(), outcomes.len() - failed_n);
        }
    }
    if failed_n > 0 { Err(failed(format!("{failed_n} test cases failed"))) } else { Ok(()) }
}
