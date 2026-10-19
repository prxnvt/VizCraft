//! Generation commands: generators from `vizcraft-codegen` run over the document's model under the
//! code-scaffold generation contract (docs/architecture.md).
//!
//! `generate.preview`, `generate.run` and `generate.check` share their params: a spec file
//! (`spec`, a `.vizcraft-gen.toml` or JSON file whose `output` is relative to it) or an inline
//! `target` and `output`, plus optional `options`, `scope` and `renames`. A relative inline output
//! and a relative spec path are resolved against the document's folder. None of them changes the
//! document, so none is an undo step. In the web build there is no folder to write: run and check
//! are errors and preview plans against nothing on disk.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use vizcraft_codegen::writer::{self, Action, Disk};
use vizcraft_codegen::{Construct, Plan, Spec};

use super::{opt_bool, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the generate commands: `generate.targets`, `generate.preview`, `generate.run`,
/// `generate.check`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("generate.targets", "Generator Targets", "Generate > Targets", targets).params(
            r#"{} -> {"targets": [{"id", "label", "singleFile", "support": {"element"|"property"|"relationship"|"containment"|"port": "supported"|"flattened"|"unsupported"}}]}"#,
        ),
        CommandSpec::new("generate.preview", "Preview", "Generate > Generate", preview).params(PREVIEW_DOC),
        CommandSpec::new("generate.run", "Generate", "Generate > Generate", run).params(RUN_DOC),
        CommandSpec::new("generate.check", "Check Output", "Generate > Generate", check).params(CHECK_DOC),
    ]
}

// The params are shared by preview, run and check; each doc spells them out so agents read one
// self-contained line per command.
const PREVIEW_DOC: &str = r#"{"spec"?: path to a generator spec (.toml or .json; relative to the document's folder), "target"?: target id (with output, instead of spec), "output"?: folder (relative to the document's folder), "options"?: {..} (the target's options; replace the spec's), "scope"?: {"packs"?: [id], "types"?: [type id], "elements"?: [id]} (replaces the spec's), "renames"?: bool (make identifiers afresh from tags and names)} -> {"target", "output", "files": [{"path", "action": "create"|"unchanged"|"update"|"merge"|"conflict"|"delete", "diff"}], "issues", "renames": [{"id", "from", "to"}], "stopped"}; writes nothing"#;
const RUN_DOC: &str = r#"{"spec"?: path to a generator spec (.toml or .json; relative to the document's folder), "target"?: target id (with output, instead of spec), "output"?: folder (relative to the document's folder), "options"?: {..} (the target's options; replace the spec's), "scope"?: {"packs"?: [id], "types"?: [type id], "elements"?: [id]} (replaces the spec's), "renames"?: bool (make identifiers afresh from tags and names)} -> {"target", "output", "written", "merged", "deleted", "unchanged", "stopped"?: true, "issues"}; writes only owned files inside the output folder; a conflict stops the run before anything is written"#;
const CHECK_DOC: &str = r#"{"spec"?: path to a generator spec (.toml or .json; relative to the document's folder), "target"?: target id (with output, instead of spec), "output"?: folder (relative to the document's folder), "options"?: {..} (the target's options; replace the spec's), "scope"?: {"packs"?: [id], "types"?: [type id], "elements"?: [id]} (replaces the spec's), "renames"?: bool (make identifiers afresh from tags and names)} -> {"ok", "stale": [path], "missing": [path], "modified": [path], "issues"}; ok when the output is what a run would write; modified lists files edited by hand"#;

/// Longest spec path, target id or output folder param, in bytes.
const MAX_PATH_PARAM: usize = 4096;

fn fail(e: vizcraft_codegen::CodegenError) -> EngineError {
    match e {
        vizcraft_codegen::CodegenError::Spec(m) | vizcraft_codegen::CodegenError::Path(m) => EngineError::Params(m),
        other => EngineError::Failed(other.to_string()),
    }
}

/// A string param of at most [`MAX_PATH_PARAM`] bytes, not blank.
fn path_param<'a>(v: &'a Value, k: &str) -> Result<Option<&'a str>, EngineError> {
    match opt_str(v, k)? {
        Some(t) if t.len() > MAX_PATH_PARAM => Err(EngineError::Params(format!("`{k}` is longer than {MAX_PATH_PARAM} bytes"))),
        Some(t) if t.trim().is_empty() => Err(EngineError::Params(format!("`{k}` is empty"))),
        Some(t) if t.contains('\0') => Err(EngineError::Params(format!("`{k}` has a NUL character"))),
        t => Ok(t),
    }
}

/// The folder relative params are resolved against: the document's folder.
fn doc_folder(s: &Session) -> Option<PathBuf> {
    s.path.as_ref().map(|p| p.parent().map(Path::to_path_buf).unwrap_or_default())
}

/// `p` as given when absolute, else under the document's folder.
fn resolve(s: &Session, p: &str, what: &str) -> Result<PathBuf, EngineError> {
    let path = Path::new(p);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    doc_folder(s)
        .map(|d| d.join(path))
        .ok_or_else(|| EngineError::Params(format!("a relative {what} needs a saved document; give an absolute path or save the document first")))
}

/// The spec and output folder the params name.
fn request(s: &Session, v: &Value) -> Result<(Spec, PathBuf, bool), EngineError> {
    if !v.is_object() && !v.is_null() {
        return Err(EngineError::Params("params must be an object".into()));
    }
    let renames = opt_bool(v, "renames")?.unwrap_or(false);
    let spec_path = path_param(v, "spec")?;
    let target = path_param(v, "target")?;
    let output = path_param(v, "output")?;
    let options = match v.get("options") {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => Some(o.clone()),
        Some(_) => return Err(EngineError::Params("`options` must be an object".into())),
    };
    let scope = match v.get("scope") {
        None | Some(Value::Null) => None,
        Some(sc @ Value::Object(_)) => Some(Spec::from_value(&json!({"scope": sc})).map_err(|e| EngineError::Params(e.to_string()))?.scope),
        Some(_) => return Err(EngineError::Params("`scope` must be an object".into())),
    };
    let (mut spec, out) = match (spec_path, target, output) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(EngineError::Params("give `spec`, or `target` and `output`, not both".into()));
        }
        (Some(p), None, None) => {
            let file = resolve(s, p, "spec path")?;
            let (spec, base) = Spec::read(&file).map_err(fail)?;
            let out = spec.output_dir(&base).map_err(fail)?;
            (spec, out)
        }
        (None, Some(t), Some(o)) => {
            let spec = Spec::from_value(&json!({"target": t, "output": o})).map_err(fail)?;
            let out = if Path::new(o).is_absolute() {
                PathBuf::from(o)
            } else {
                let base = doc_folder(s).ok_or_else(|| {
                    EngineError::Params("a relative output needs a saved document; give an absolute folder or save the document first".into())
                })?;
                spec.output_dir(&base).map_err(fail)?
            };
            (spec, out)
        }
        (None, Some(_), None) => return Err(EngineError::Params("`target` needs an `output` folder".into())),
        (None, None, Some(_)) => return Err(EngineError::Params("`output` needs a `target`".into())),
        (None, None, None) => return Err(EngineError::Params("give a `spec` file, or a `target` and an `output` folder".into())),
    };
    if out.parent().is_none() {
        return Err(EngineError::Params(format!("`{}` is a filesystem root; name a folder for the output", short(&out.to_string_lossy()))));
    }
    if let Some(o) = options {
        spec.options = o;
    }
    if let Some(sc) = scope {
        spec.scope = sc;
    }
    let spec = spec.checked().map_err(fail)?;
    Ok((spec, out, renames))
}

/// The plan for the params, against the output folder on disk (nothing on disk on the web).
fn make_plan(s: &Session, v: &Value) -> Result<(Plan, PathBuf), EngineError> {
    let (spec, out, renames) = request(s, v)?;
    let disk: Box<dyn Disk> = if cfg!(target_arch = "wasm32") { Box::new(writer::NoDisk) } else { Box::new(writer::FsDisk::new(&out)) };
    let plan = writer::plan(&spec, &s.doc.model, vizcraft_notation::catalog(), renames, disk.as_ref()).map_err(fail)?;
    Ok((plan, out))
}

fn issues_json(plan_issues: &[vizcraft_model::Issue]) -> CmdResult {
    serde_json::to_value(plan_issues).map_err(|e| EngineError::Failed(e.to_string()))
}

/// The built-in targets and what each can express.
fn targets(_s: &mut Session, _v: &Value) -> CmdResult {
    let list: Vec<Value> = vizcraft_codegen::targets()
        .iter()
        .map(|t| {
            let support: serde_json::Map<String, Value> =
                Construct::ALL.iter().map(|c| (c.name().to_string(), json!(vizcraft_codegen::target::support_of(*t, *c).name()))).collect();
            json!({"id": t.id(), "label": t.label(), "singleFile": t.single_file(), "support": support})
        })
        .collect();
    Ok(json!({"targets": list}))
}

/// What a run would do, with a diff per file.
fn preview(s: &mut Session, v: &Value) -> CmdResult {
    let (plan, out) = make_plan(s, v)?;
    let files: Vec<Value> = plan.files.iter().map(|f| json!({"path": f.path, "action": f.action.name(), "diff": f.diff})).collect();
    Ok(json!({
        "target": plan.target,
        "output": out.to_string_lossy(),
        "files": files,
        "issues": issues_json(&plan.issues)?,
        "renames": serde_json::to_value(&plan.renames).map_err(|e| EngineError::Failed(e.to_string()))?,
        "stopped": plan.stopped,
    }))
}

/// Generate into the output folder.
fn run(s: &mut Session, v: &Value) -> CmdResult {
    if cfg!(target_arch = "wasm32") {
        return Err(EngineError::Failed("generate.run writes files, which the web build cannot do; use generate.preview".into()));
    }
    let (plan, out) = make_plan(s, v)?;
    let report = writer::apply(&plan, &out).map_err(fail)?;
    let mut r = json!({
        "target": plan.target,
        "output": out.to_string_lossy(),
        "written": report.written,
        "merged": report.merged,
        "deleted": report.deleted,
        "unchanged": report.unchanged,
        "issues": issues_json(&report.issues)?,
    });
    if report.stopped
        && let Some(o) = r.as_object_mut()
    {
        o.insert("stopped".into(), json!(true));
    }
    s.status = if report.stopped {
        "Generation stopped: hand edits conflict with the new output".into()
    } else {
        format!("Generated {} files, merged {}, deleted {}", report.written.len(), report.merged.len(), report.deleted.len())
    };
    Ok(r)
}

/// Whether the output folder holds what a run would write.
fn check(s: &mut Session, v: &Value) -> CmdResult {
    if cfg!(target_arch = "wasm32") {
        return Err(EngineError::Failed("generate.check reads files, which the web build cannot do; use generate.preview".into()));
    }
    let (plan, _) = make_plan(s, v)?;
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    let mut modified = Vec::new();
    for f in &plan.files {
        match f.action {
            Action::Create => missing.push(f.path.clone()),
            Action::Update | Action::Merge | Action::Conflict | Action::Delete => stale.push(f.path.clone()),
            Action::Unchanged => {}
        }
        if f.hand_edited {
            modified.push(f.path.clone());
        }
    }
    if stale.is_empty() && missing.is_empty() && plan.manifest_changed {
        stale.push(vizcraft_codegen::Manifest::path());
    }
    let ok = stale.is_empty() && missing.is_empty();
    Ok(json!({"ok": ok, "stale": stale, "missing": missing, "modified": modified, "issues": issues_json(&plan.issues)?}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sample flowchart with a model behind it: the sample's shapes are plain drawing, so
    /// three typed elements and a flow are added beside them.
    fn sample() -> Session {
        let mut s = Session::new(crate::sample::sample_document());
        for (id, ty, name, x) in [
            ("gin", "flowchart.Process", "Gather input", 40),
            ("ok", "flowchart.Decision", "Valid?", 240),
            ("save", "flowchart.Process", "Save diagram", 440),
        ] {
            s.run("element.create", &json!({"type": ty, "name": name, "id": id, "x": x, "y": 700})).unwrap();
        }
        s.run("relationship.create", &json!({"from": "gin", "to": "ok", "id": "f1", "name": "checked"})).unwrap();
        s
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-engine-generate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn params(out: &Path) -> Value {
        json!({"target": "markdown-catalog", "output": out.to_string_lossy()})
    }

    fn bytes(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() { stack.push(p) } else { out.push((p.clone(), std::fs::read(&p).unwrap())) }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn targets_list_the_markdown_catalogue_and_its_matrix() {
        let mut s = sample();
        let r = s.run("generate.targets", &json!({})).unwrap();
        let t = &r["targets"][0];
        assert_eq!((t["id"].as_str(), t["singleFile"].as_bool()), (Some("markdown-catalog"), Some(true)));
        assert_eq!(t["support"]["port"], "flattened");
        assert_eq!(t["support"]["element"], "supported");
    }

    #[test]
    fn the_catalogue_over_the_sample_runs_twice_byte_identically_and_check_sees_a_model_change() {
        let mut s = sample();
        let out = tmp("run");
        let before_rev = s.rev();
        let r = s.run("generate.preview", &params(&out)).unwrap();
        assert_eq!(r["files"][0]["action"], "create");
        assert!(r["files"][0]["diff"].as_str().unwrap().contains("| Gather input |"));
        assert!(!out.join("catalog.md").exists(), "preview writes nothing");
        let r = s.run("generate.check", &params(&out)).unwrap();
        assert_eq!((r["ok"].as_bool(), r["missing"][0].as_str()), (Some(false), Some("catalog.md")));

        let r = s.run("generate.run", &params(&out)).unwrap();
        assert_eq!(r["written"], json!(["catalog.md"]));
        assert!(r.get("stopped").is_none());
        let text = std::fs::read_to_string(out.join("catalog.md")).unwrap();
        assert!(text.contains("<!-- vizcraft:id=gin -->") && text.contains("<!-- vizcraft:id=f1 -->"), "{text}");
        assert!(out.join(".vizcraft-gen/manifest.json").is_file());
        let first = bytes(&out);
        let r = s.run("generate.run", &params(&out)).unwrap();
        assert_eq!((r["written"].as_array().map(Vec::len), r["unchanged"][0].as_str()), (Some(0), Some("catalog.md")));
        assert_eq!(bytes(&out), first, "a second run changes no byte");
        assert_eq!(s.run("generate.check", &params(&out)).unwrap()["ok"], true);
        assert_eq!(s.rev(), before_rev, "generation never changes the document");

        s.run("element.rename", &json!({"id": "save", "name": "Store diagram"})).unwrap();
        let r = s.run("generate.check", &params(&out)).unwrap();
        assert_eq!((r["ok"].as_bool(), r["stale"][0].as_str()), (Some(false), Some("catalog.md")));
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_spec_file_names_target_output_and_scope() {
        let mut s = sample();
        let dir = tmp("spec");
        std::fs::write(dir.join(".vizcraft-gen.toml"), "name = \"cat\"\ntarget = \"markdown-catalog\"\noutput = \"gen\"\n[options]\nfile = \"model.md\"\n[scope]\ntypes = [\"flowchart.Decision\"]\n").unwrap();
        // A relative spec path is resolved against the document's folder.
        s.path = Some(dir.join("doc.vizcraft"));
        let r = s.run("generate.run", &json!({"spec": ".vizcraft-gen.toml"})).unwrap();
        assert_eq!(r["written"], json!(["model.md"]));
        let text = std::fs::read_to_string(dir.join("gen/model.md")).unwrap();
        assert!(text.contains("Valid?") && !text.contains("Gather input"));
        // An inline relative output goes under the document's folder too.
        let r = s.run("generate.run", &json!({"target": "markdown-catalog", "output": "inline"})).unwrap();
        assert_eq!(r["written"], json!(["catalog.md"]));
        assert!(dir.join("inline/catalog.md").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn junk_paths_and_params_are_errors_never_panics_or_stray_files() {
        let mut s = sample();
        let dir = tmp("junk");
        s.path = Some(dir.join("doc.vizcraft"));
        let long = "x".repeat(5000);
        let junk = [
            json!(null),
            json!([]),
            json!("text"),
            json!({}),
            json!({"spec": 5}),
            json!({"spec": ""}),
            json!({"spec": "\u{0}"}),
            json!({"spec": long}),
            json!({"spec": "/no/such/file.toml"}),
            json!({"spec": dir.to_string_lossy()}),
            json!({"spec": "spec.toml", "target": "markdown-catalog"}),
            json!({"target": "markdown-catalog"}),
            json!({"output": "out"}),
            json!({"target": "nope", "output": "out"}),
            json!({"target": "markdown-catalog", "output": "../../escape"}),
            json!({"target": "markdown-catalog", "output": "a/../../b"}),
            json!({"target": "markdown-catalog", "output": "/"}),
            json!({"target": "markdown-catalog", "output": "out", "options": {"file": "../../x.md"}}),
            json!({"target": "markdown-catalog", "output": "out", "options": {"file": "/etc/x.md"}}),
            json!({"target": "markdown-catalog", "output": "out", "options": {"file": ".vizcraft-gen/manifest.json"}}),
            json!({"target": "markdown-catalog", "output": "out", "options": 5}),
            json!({"target": "markdown-catalog", "output": "out", "scope": {"packs": 7}}),
            json!({"target": "markdown-catalog", "output": "out", "scope": "all"}),
            json!({"target": "markdown-catalog", "output": "out", "renames": "yes"}),
            json!({"target": [], "output": {}}),
        ];
        std::fs::write(dir.join("spec.toml"), "target = \"markdown-catalog\"\noutput = \"../../../outside\"\n").unwrap();
        std::fs::write(dir.join("bad.toml"), "[[[").unwrap();
        for id in ["generate.preview", "generate.run", "generate.check"] {
            for j in junk.iter().chain([&json!({"spec": "spec.toml"}), &json!({"spec": "bad.toml"})]) {
                assert!(s.run(id, j).is_err(), "{id}: {j}");
            }
        }
        // Nothing was written anywhere but the two spec files.
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(!dir.parent().unwrap().join("escape").exists() && !Path::new("/etc/x.md").exists());
        // An unsaved document has no folder for a relative output.
        s.path = None;
        assert!(s.run("generate.preview", &json!({"target": "markdown-catalog", "output": "out"})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
