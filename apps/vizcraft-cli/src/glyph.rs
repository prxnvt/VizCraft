//! `vizcraft-cli glyph`: draw one parametric glyph and print its SVG.
//!
//! ```text
//! vizcraft-cli glyph BUILDER [--param KEY=VALUE]... [--module N]
//! ```
//!
//! BUILDER is a builder name (`contact`, `valve`) or a pack glyph id drawn by one. Values read as
//! numbers, `true` or `false` when they look like one, else as text. The module is in page units
//! (100 per inch); without it the standard's module is used. Warnings about clamped or ignored
//! values go to stderr. Runs `glyph.build` on a headless session; no document is touched.

use serde_json::{Map, Value, json};
use vizcraft_engine::{Document, Session};

use crate::{CliError, failed, usage};

/// A `--param` value as JSON: a number, a flag, or text.
fn value(text: &str) -> Value {
    match text {
        "true" => json!(true),
        "false" => json!(false),
        t => t.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| json!(f)).unwrap_or_else(|| json!(t)),
    }
}

/// Run the `glyph` subcommand.
pub fn glyph(args: &[String]) -> Result<(), CliError> {
    let mut name: Option<&str> = None;
    let mut params = Map::new();
    let mut module: Option<f64> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--param" => {
                let kv = it.next().ok_or_else(|| usage("--param needs KEY=VALUE"))?;
                let (k, v) = kv.split_once('=').ok_or_else(|| usage(format!("--param `{kv}` is not KEY=VALUE")))?;
                params.insert(k.to_string(), value(v));
            }
            "--module" => {
                let m = it.next().ok_or_else(|| usage("--module needs a number"))?;
                module =
                    Some(m.parse::<f64>().ok().filter(|f| f.is_finite() && *f > 0.0).ok_or_else(|| usage("--module must be a positive number"))?);
            }
            other if other.starts_with("--") => return Err(usage(format!("glyph: unknown option `{other}`"))),
            other if name.is_none() => name = Some(other),
            other => return Err(usage(format!("glyph: unexpected argument `{other}`"))),
        }
    }
    let name = name.ok_or_else(|| usage("glyph: a builder name or glyph id is required"))?;
    let key = if name.contains('.') { "glyph" } else { "builder" };
    let mut req = json!({key: name, "params": params});
    if let Some(m) = module {
        req["module"] = json!(m);
    }
    let mut s = Session::new(Document::new());
    let r = s.run("glyph.build", &req).map_err(|e| failed(e.to_string()))?;
    for w in r["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        eprintln!("vizcraft-cli: glyph: {w}");
    }
    print!("{}", r["svg"].as_str().unwrap_or_default());
    Ok(())
}
