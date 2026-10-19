//! `vizcraft-cli new`: start a document from a template, or list the templates, without a window.
//!
//! ```text
//! vizcraft-cli new --template ID [--variant V] [--sheet S] [--svg PREVIEW.svg] [--json] OUT
//! vizcraft-cli new --list [--audience A] [--json]
//! ```
//!
//! The first form runs `template.new` with `path: OUT` and, with `--svg`, writes the template's
//! preview (`template.preview`) beside it. The second lists the templates (`template.list`).
//!
//! Exit codes: 0 done, 1 the template is unknown or a file could not be written, 2 for a usage
//! mistake.

use serde_json::{Map, Value, json};
use vizcraft_engine::{Document, Session};

use crate::{CliError, failed, usage};

/// Parsed arguments.
#[derive(Default)]
struct Args {
    list: bool,
    audience: Option<String>,
    template: Option<String>,
    variant: Option<String>,
    sheet: Option<String>,
    svg: Option<String>,
    out: Option<String>,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| usage(format!("new: {name} needs a value")));
        match arg.as_str() {
            "--list" => a.list = true,
            "--json" => a.json = true,
            "--audience" => a.audience = Some(value("--audience")?),
            "--template" => a.template = Some(value("--template")?),
            "--variant" => a.variant = Some(value("--variant")?),
            "--sheet" => a.sheet = Some(value("--sheet")?),
            "--svg" => a.svg = Some(value("--svg")?),
            other if other.starts_with("--") => return Err(usage(format!("new: unknown option `{other}`"))),
            other => {
                if a.out.is_some() {
                    return Err(usage(format!("new: unexpected argument `{other}`")));
                }
                a.out = Some(other.to_string());
            }
        }
    }
    if a.list {
        if a.template.is_some() || a.variant.is_some() || a.sheet.is_some() || a.svg.is_some() || a.out.is_some() {
            return Err(usage("new: --list takes only --audience and --json"));
        }
    } else {
        if a.audience.is_some() {
            return Err(usage("new: --audience goes with --list"));
        }
        if a.template.is_none() {
            return Err(usage("new needs --template ID (or --list)"));
        }
        if a.out.is_none() {
            return Err(usage("new needs the file to write"));
        }
    }
    Ok(a)
}

/// `new ...`: see the module doc.
pub fn new(args: &[String]) -> Result<(), CliError> {
    let a = parse(args)?;
    let mut s = Session::new(Document::new());
    if a.list {
        return list(&mut s, &a);
    }
    let (Some(template), Some(out)) = (&a.template, &a.out) else { return Err(usage("new needs --template ID and the file to write")) };
    let mut params = Map::new();
    params.insert("template".into(), json!(template));
    if let Some(v) = &a.variant {
        params.insert("variant".into(), json!(v));
    }
    if let Some(sh) = &a.sheet {
        params.insert("sheet".into(), json!(sh));
    }
    let preview = Value::Object(params.clone());
    params.insert("path".into(), json!(out));
    let r = s.run("template.new", &Value::Object(params)).map_err(|e| failed(e.to_string()))?;
    if let Some(svg_path) = &a.svg {
        let p = s.run("template.preview", &preview).map_err(|e| failed(e.to_string()))?;
        let svg = p.get("svg").and_then(Value::as_str).unwrap_or_default();
        std::fs::write(svg_path, svg).map_err(|e| failed(format!("{svg_path}: {e}")))?;
    }
    if a.json {
        println!("{r}");
        return Ok(());
    }
    let shapes: usize = s.doc.pages.iter().map(|p| p.shapes.len()).sum();
    println!("wrote {out} from {template} ({shapes} shapes)");
    for (what, key) in [("applied", "applied"), ("skipped", "skipped")] {
        let ids: Vec<&str> = r.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        if !ids.is_empty() {
            println!("  {what}: {}", ids.join(", "));
        }
    }
    if let Some(svg_path) = &a.svg {
        println!("  preview: {svg_path}");
    }
    Ok(())
}

/// `new --list [--audience A] [--json]`.
fn list(s: &mut Session, a: &Args) -> Result<(), CliError> {
    let params = match &a.audience {
        Some(x) => json!({"audience": x}),
        None => json!({}),
    };
    let r = s.run("template.list", &params).map_err(|e| failed(e.to_string()))?;
    if a.json {
        println!("{r}");
        return Ok(());
    }
    let items = r.get("templates").and_then(Value::as_array).cloned().unwrap_or_default();
    let field = |t: &Value, k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let w = items.iter().map(|t| field(t, "id").len()).max().unwrap_or(2).max(2);
    for t in &items {
        println!("{:<w$}  {}  ({})", field(t, "id"), field(t, "label"), field(t, "packName"));
    }
    Ok(())
}
