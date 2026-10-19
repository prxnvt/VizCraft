//! Provenance of drawn glyphs, part of `cargo xtask assets`.
//!
//! Every glyph of a shipped pack (`crates/notation/packs/*.json`) whose outline is `path` or
//! `builder` must say where its drawing came from in `source`: `own design`, or text starting
//! `drawn from` that names the written description of the public notation it follows. Every
//! glyph builder module (`crates/stencils/src/builders/*.rs`) states the same in a
//! `//! Provenance:` line. A note naming a tracing, a copy or a vendor whose assets this
//! repository never uses is refused.

use std::path::Path;

use serde_json::Value;

/// Words a provenance note may not contain, compared without case.
pub const FORBIDDEN: &[&str] = &["traced", "copied", "visio", "microsoft", "autocad", "autodesk", "adobe", "avid"];

/// Builder modules that hold no symbol of their own.
const NOT_BUILDERS: &[&str] = &["mod.rs", "params.rs", "apply.rs", "sync.rs", "tests.rs"];

/// Why `source` is not an acceptable provenance note, or `None` when it is.
pub fn source_problem(source: &str) -> Option<String> {
    let lower = source.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return Some("no `source`".into());
    }
    if !(lower.starts_with("own design") || lower.starts_with("drawn from")) {
        return Some(format!("`source` must be `own design` or start with `drawn from`, not `{}`", short(source)));
    }
    FORBIDDEN.iter().find(|w| lower.contains(*w)).map(|w| format!("`source` may not mention `{w}`"))
}

fn short(t: &str) -> String {
    t.chars().take(60).collect()
}

/// Problems with the drawn glyphs of one pack file's text.
pub fn pack_problems(name: &str, text: &str) -> Vec<String> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return vec![format!("{name}: not JSON: {e}")],
    };
    let mut out = Vec::new();
    for g in v.get("glyphs").and_then(Value::as_array).into_iter().flatten() {
        let outline = g.get("outline").and_then(Value::as_str).unwrap_or_default();
        if outline != "path" && outline != "builder" {
            continue;
        }
        let id = g.get("id").and_then(Value::as_str).unwrap_or("?");
        let source = g.get("source").and_then(Value::as_str).unwrap_or_default();
        if let Some(p) = source_problem(source) {
            out.push(format!("{name}: glyph `{id}`: {p}"));
        }
    }
    out
}

/// Problems with one builder module's text: its `//! Provenance:` line.
pub fn module_problems(name: &str, text: &str) -> Vec<String> {
    let docs: Vec<&str> = text.lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim()).collect();
    let docs = docs.join(" ");
    match docs.split_once("Provenance:") {
        None => vec![format!("{name}: no `//! Provenance:` line in the module docs")],
        Some((_, rest)) => source_problem(rest).map(|p| format!("{name}: provenance: {p}")).into_iter().collect(),
    }
}

/// Every problem under `root`, and how many glyphs and modules were checked.
pub fn check(root: &Path) -> Result<(usize, Vec<String>), String> {
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for (dir, ext, is_pack) in [("crates/notation/packs", "json", true), ("crates/stencils/src/builders", "rs", false)] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        let mut files: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == ext)).collect();
        files.sort();
        for path in files {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !is_pack && NOT_BUILDERS.contains(&name.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            checked += 1;
            problems.extend(if is_pack { pack_problems(&name, &text) } else { module_problems(&name, &text) });
        }
    }
    Ok((checked, problems))
}

/// `cargo xtask assets`, the provenance half.
pub fn run(root: &Path) -> Result<(), String> {
    let (n, problems) = check(root)?;
    if !problems.is_empty() {
        return Err(format!("provenance:\n  {}", problems.join("\n  ")));
    }
    println!("provenance: {n} pack and builder files state where their drawings came from");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(source: Option<&str>) -> String {
        let mut g = serde_json::json!({"id": "p.g", "outline": "builder", "builder": "contact"});
        if let Some(s) = source {
            g["source"] = s.into();
        }
        serde_json::json!({"id": "p", "glyphs": [g, {"id": "p.box", "outline": "rectangle"}]}).to_string()
    }

    #[test]
    fn a_missing_bad_or_good_source() {
        assert!(pack_problems("p.json", &pack(None))[0].contains("no `source`"));
        assert!(pack_problems("p.json", &pack(Some("found online")))[0].contains("must be `own design`"));
        assert!(pack_problems("p.json", &pack(Some("drawn from a Visio stencil")))[0].contains("visio"));
        assert!(pack_problems("p.json", &pack(Some("own design, traced over a scan")))[0].contains("traced"));
        assert!(pack_problems("p.json", &pack(Some("own design"))).is_empty());
        assert!(pack_problems("p.json", &pack(Some("drawn from the written rules of ISO 1219-1"))).is_empty());
        assert!(pack_problems("p.json", "{{").len() == 1);
        assert!(module_problems("x.rs", "//! A builder.\n").len() == 1);
        assert!(module_problems("x.rs", "//! A builder.\n//!\n//! Provenance: own design, drawn from\n//! IEC 60617.\nuse x;\n").is_empty());
    }

    #[test]
    fn the_repository_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let (n, problems) = check(&root).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert!(n >= 13, "three packs and ten builders, found {n}");
    }
}
