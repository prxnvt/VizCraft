//! Analysis commands: graph analyses over the semantic model (`analyze.*`) and model compare
//! (`model.diff`), with the result kept in `Session.analysis` for views to highlight.
//!
//! The algorithms are `vizcraft_analysis`; this module supplies the catalog's type knowledge
//! (subtypes, inverse relationship types), resolves shape and connector ids to the model items
//! they draw, stamps the result with the document revision and stores it. An analysis is session
//! state: it never changes the document, so it makes no undo step and leaves the document clean.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};
use vizcraft_analysis::{Analysis, AnalysisError, AnalysisResult, Context, Types, analyses};
use vizcraft_notation::Catalog;
use vizcraft_notation::infer::Facts;

use super::model::{cat, check_len, element_of, has_elements};
use super::{MAX_IDS, opt_str, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the analysis commands live in the UI.
const LOCATION: &str = "Model > Analyze";

/// Return the analysis commands.
pub fn specs() -> Vec<CommandSpec> {
    let p = |id: &str| analyses().iter().find(|a| a.id() == id).map_or("{}", |a| a.params());
    vec![
        CommandSpec::new("analyze.dependencies", "Dependencies", LOCATION, dependencies).params(p("dependencies")).when(has_elements),
        CommandSpec::new("analyze.cycles", "Cycles", LOCATION, cycles).params(p("cycles")).when(has_elements),
        CommandSpec::new("analyze.unreachable", "Unreachable Elements", LOCATION, unreachable).params(p("unreachable")).when(has_elements),
        CommandSpec::new("analyze.spof", "Single Points of Failure", LOCATION, spof).params(p("spof")).when(has_elements),
        CommandSpec::new("analyze.metrics", "Metrics", LOCATION, metrics).params(p("metrics")).when(has_elements),
        CommandSpec::new("analyze.path", "Path", LOCATION, path).params(p("path")).when(has_elements),
        CommandSpec::new("analyze.select", "Select Analysis Result", LOCATION, select_result)
            .params(r#"{} -> {"primary", "items"}; selects the shapes and connectors on the active page that draw the ids of the stored result"#)
            .when(has_elements),
        CommandSpec::new("analyze.result", "Analysis Result", LOCATION, result).params(
            r#"{} -> the stored result ({"kind", "summary", "elements", "relationships", "marks": {id: mark}, "groups", "table"?, "truncated", "rev", "stale", "names", ..the kind's own fields}) or null; stale is true when the document changed since"#,
        ),
        CommandSpec::new("analyze.clear", "Clear Analysis", LOCATION, clear).params(r#"{} -> {"cleared": bool}; drops the stored result"#),
        CommandSpec::new("model.diff", "Compare Model", LOCATION, diff).params(
            r#"{"against"?: "saved"|"file", "path"?: string} -> {"elements": {"added", "removed", "changed": [{"id", "fields": [{"field", "before", "after"}]}]}, "relationships": same, "truncated", "summary"}; compares the model saved on disk (default: the document's own file; a path implies against file) with the current one, keyed by id, and stores it as the result of kind diff; against head is not supported yet"#,
        ),
    ]
}

/// The ids of every analysis `run_kind` accepts, built-ins first.
pub fn kinds() -> Vec<&'static str> {
    analyses().iter().map(|a| a.id()).collect()
}

/// The catalog's type knowledge for an analysis.
pub struct CatalogTypes<'a> {
    facts: Facts<'a>,
    cat: &'a Catalog,
}

impl<'a> CatalogTypes<'a> {
    /// The type knowledge `cat` has about `model`.
    pub fn new(model: &'a vizcraft_model::Model, cat: &'a Catalog) -> Self {
        CatalogTypes { facts: Facts::new(model, cat), cat }
    }
}

impl Types for CatalogTypes<'_> {
    fn is_a(&self, ty: &str, ancestor: &str) -> bool {
        self.facts.is_a(ty, ancestor)
    }
    /// The type `rel` declares itself the inverse of, and every type declared the inverse of it.
    fn inverses(&self, rel: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(inv) = self.cat.relationship_type(rel).and_then(|(_, t)| t.inverse_of.clone()) {
            out.push(inv);
        }
        for p in self.cat.packs() {
            for t in &p.relationship_types {
                if t.inverse_of.as_deref() == Some(rel) && !out.contains(&t.id) {
                    out.push(t.id.clone());
                }
            }
        }
        out
    }
}

/// `x` with a shape id swapped for the element it draws; anything else unchanged, so the
/// analysis reports an id that names nothing.
fn resolve_one(s: &Session, k: &str, x: &Value) -> Result<Value, EngineError> {
    match x {
        Value::String(t) => {
            check_len(k, t)?;
            Ok(Value::String(element_of(&s.doc, t).unwrap_or_else(|| t.clone())))
        }
        other => Ok(other.clone()),
    }
}

/// The params with the element params (`id`, `from`, `to`) resolved from shape ids.
fn resolve(s: &Session, v: &Value) -> Result<Value, EngineError> {
    let mut out = match v {
        Value::Object(o) => o.clone(),
        Value::Null => Map::new(),
        _ => return Err(EngineError::Params("params must be an object".into())),
    };
    for k in ["id", "from", "to"] {
        let Some(x) = out.get(k) else { continue };
        let resolved = match x {
            Value::Array(a) => {
                if a.len() > MAX_IDS {
                    return Err(EngineError::Params(format!("`{k}` has more than {MAX_IDS} ids")));
                }
                Value::Array(a.iter().map(|x| resolve_one(s, k, x)).collect::<Result<_, _>>()?)
            }
            x => resolve_one(s, k, x)?,
        };
        out.insert(k.to_string(), resolved);
    }
    Ok(Value::Object(out))
}

fn analysis_err(e: AnalysisError) -> EngineError {
    match e {
        AnalysisError::Unknown(id) => EngineError::Params(format!("`{id}` is not an element in the graph analysed, or a shape drawing one")),
        AnalysisError::Params(t) => EngineError::Params(t),
        AnalysisError::TooLarge => EngineError::Failed(e.to_string()),
    }
}

/// What the result calls each id it names: an element's or relationship's name, else the id.
fn names(s: &Session, r: &AnalysisResult) -> Value {
    let m = &s.doc.model;
    let mut by_id: HashMap<&str, &str> = HashMap::with_capacity(m.elements.len() + m.relationships.len());
    for e in &m.elements {
        by_id.entry(e.id.as_str()).or_insert(e.name.as_str());
    }
    for r in &m.relationships {
        by_id.entry(r.id.as_str()).or_insert(r.name.as_str());
    }
    let mut out = Map::new();
    for id in r.ids() {
        let name = by_id.get(id.as_str()).copied().filter(|n| !n.is_empty()).unwrap_or(&id).to_string();
        out.insert(id, Value::String(name));
    }
    Value::Object(out)
}

/// The result as a command returns it: its fields, its kind's own fields at the top level, the
/// names of its ids, and whether the document has changed since.
fn result_json(s: &Session, r: &AnalysisResult) -> CmdResult {
    let mut v = serde_json::to_value(r).map_err(|e| EngineError::Failed(e.to_string()))?;
    if let Some(o) = v.as_object_mut() {
        if let Some(Value::Object(detail)) = o.remove("detail") {
            for (k, x) in detail {
                o.entry(k).or_insert(x);
            }
        }
        o.insert("names".into(), names(s, r));
        o.insert("stale".into(), Value::Bool(r.rev != s.rev()));
    }
    Ok(v)
}

/// Run the analysis `kind` over the document's model with `v`, store the result in
/// `Session.analysis` and return it. Every `analyze.*` analysis goes through here, as do the CLI
/// and any analysis a later pack adds to `vizcraft_analysis::analyses`.
pub fn run_kind(s: &mut Session, kind: &str, v: &Value) -> CmdResult {
    let a: &dyn Analysis = analyses()
        .iter()
        .find(|a| a.id() == kind)
        .copied()
        .ok_or_else(|| EngineError::Params(format!("unknown analysis `{}` (one of {})", short(kind), kinds().join(", "))))?;
    let params = resolve(s, v)?;
    let types = CatalogTypes::new(&s.doc.model, cat());
    let cx = Context { model: &s.doc.model, types: &types };
    let mut r = a.run(&cx, &params).map_err(analysis_err)?;
    r.rev = s.rev();
    let out = result_json(s, &r)?;
    s.analysis = Some(r);
    Ok(out)
}

fn dependencies(s: &mut Session, v: &Value) -> CmdResult {
    // No id: the primary selection's element.
    if v.get("id").is_none_or(Value::is_null) {
        let id = s.sel.primary.as_deref().and_then(|p| element_of(&s.doc, p));
        let Some(id) = id else { return Err(EngineError::Params("`id` is required when no shape of an element is selected".into())) };
        let mut o = v.as_object().cloned().unwrap_or_default();
        o.insert("id".into(), Value::String(id));
        return run_kind(s, "dependencies", &Value::Object(o));
    }
    run_kind(s, "dependencies", v)
}

fn cycles(s: &mut Session, v: &Value) -> CmdResult {
    run_kind(s, "cycles", v)
}

fn unreachable(s: &mut Session, v: &Value) -> CmdResult {
    run_kind(s, "unreachable", v)
}

fn spof(s: &mut Session, v: &Value) -> CmdResult {
    run_kind(s, "spof", v)
}

fn metrics(s: &mut Session, v: &Value) -> CmdResult {
    run_kind(s, "metrics", v)
}

fn path(s: &mut Session, v: &Value) -> CmdResult {
    run_kind(s, "path", v)
}

fn select_result(s: &mut Session, _: &Value) -> CmdResult {
    let Some(r) = &s.analysis else { return Err(EngineError::Params("there is no analysis result; run an analyze command first".into())) };
    let wanted: HashSet<String> = r.ids().into_iter().collect();
    let Some(page) = s.doc.page() else { return Err(EngineError::Failed("the document has no page".into())) };
    let hit = |x: Option<&str>| x.is_some_and(|x| wanted.contains(x));
    let ids: Vec<String> = page
        .shapes
        .iter()
        .filter(|sh| hit(sh.element.as_deref()))
        .map(|sh| sh.id.clone())
        .chain(page.connectors.iter().filter(|c| hit(c.relationship.as_deref())).map(|c| c.id.clone()))
        .take(MAX_IDS)
        .collect();
    select(s, ids);
    with_sel(s, json!({}))
}

fn result(s: &mut Session, _: &Value) -> CmdResult {
    match &s.analysis {
        Some(r) => result_json(s, r),
        None => Ok(Value::Null),
    }
}

fn clear(s: &mut Session, _: &Value) -> CmdResult {
    let cleared = s.analysis.take().is_some();
    Ok(json!({"cleared": cleared}))
}

/// The file `model.diff` compares against.
fn diff_path(s: &Session, v: &Value) -> Result<String, EngineError> {
    let path = opt_str(v, "path")?;
    if let Some(p) = path {
        check_len("path", p).map_err(|_| EngineError::Params("`path` is too long".into()))?;
    }
    match (opt_str(v, "against")?, path) {
        (Some("head"), _) => Err(EngineError::Params("comparing with git HEAD is not supported yet; save a copy and give its path".into())),
        (Some("saved"), Some(_)) => Err(EngineError::Params("`path` goes with against file, not saved".into())),
        (Some("file"), None) => Err(EngineError::Params("against file needs `path`".into())),
        (Some("saved") | None, None) => s
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| EngineError::Params("the document has not been saved; give `path` to compare with a file".into())),
        (Some("file") | None, Some(p)) => Ok(p.to_string()),
        (Some(other), _) => Err(EngineError::Params(format!("`against` must be saved or file, not `{}`", short(other)))),
    }
}

fn diff(s: &mut Session, v: &Value) -> CmdResult {
    if !(v.is_object() || v.is_null()) {
        return Err(EngineError::Params("params must be an object".into()));
    }
    let path = diff_path(s, v)?;
    let bytes = crate::io::read_limited(&path)?;
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let before = crate::io::load_bytes(&name, &bytes)?;
    let d = vizcraft_analysis::diff(&before.model, &s.doc.model);
    let mut r = vizcraft_analysis::diff_result(&d);
    r.rev = s.rev();
    let mut out = serde_json::to_value(&d).map_err(|e| EngineError::Failed(e.to_string()))?;
    if let Some(o) = out.as_object_mut() {
        o.insert("summary".into(), Value::String(r.summary.clone()));
    }
    s.analysis = Some(r);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::model::fixture::{blank, flow, run, shapes_of};
    use crate::Session;

    /// The flow fixture with a loop back from `ok` to `do` and an island element `lone`.
    fn looped() -> Session {
        let mut s = flow();
        run(&mut s, "relationship.create", json!({"from": "ok", "to": "do", "id": "f3"}));
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Lone", "id": "lone", "x": 300, "y": 0}));
        s
    }

    fn strs(v: &Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn dependencies_walks_down_and_up_with_levels_and_names() {
        let mut s = flow();
        let r = run(&mut s, "analyze.dependencies", json!({"id": "start"}));
        assert_eq!(strs(&r["elements"]), ["start", "do", "ok"]);
        assert_eq!(strs(&r["relationships"]), ["f1", "f2"]);
        assert_eq!(r["levels"], json!([["start"], ["do"], ["ok"]]));
        assert_eq!(r["names"]["do"], "Do");
        assert_eq!(r["names"]["f2"], "next");
        assert_eq!(r["marks"]["start"], "focus");
        let r = run(&mut s, "analyze.dependencies", json!({"id": "ok", "direction": "upstream", "depth": 1}));
        assert_eq!(strs(&r["elements"]), ["do", "ok"]);
        assert_eq!(r["marks"]["do"], "upstream");
    }

    #[test]
    fn a_shape_id_stands_for_its_element_and_the_selection_is_the_default() {
        let mut s = flow();
        let shape = shapes_of(&s, "do")[0].clone();
        let r = run(&mut s, "analyze.dependencies", json!({"id": shape}));
        assert_eq!(strs(&r["elements"]), ["do", "ok"]);
        let start = shapes_of(&s, "start")[0].clone();
        crate::cmd::select(&mut s, vec![start]);
        let r = run(&mut s, "analyze.dependencies", json!({}));
        assert_eq!(strs(&r["elements"]), ["start", "do", "ok"]);
        let from = shapes_of(&s, "start")[0].clone();
        let r = run(&mut s, "analyze.path", json!({"from": from, "to": "ok"}));
        assert_eq!(r["length"], 2);
        crate::cmd::select(&mut s, Vec::new());
        assert!(s.run("analyze.dependencies", &json!({})).is_err());
    }

    #[test]
    fn cycles_unreachable_spof_metrics_and_path_on_a_built_model() {
        let mut s = looped();
        let r = run(&mut s, "analyze.cycles", json!({}));
        assert_eq!(strs(&r["elements"]), ["do", "ok"]);
        assert_eq!(strs(&r["groups"][0]["relationships"]), ["f2", "f3"]);
        let r = run(&mut s, "analyze.unreachable", json!({"from": ["start"]}));
        assert_eq!(strs(&r["elements"]), ["lone"]);
        let r = run(&mut s, "analyze.unreachable", json!({}));
        assert_eq!(strs(&r["roots"]), ["start", "lone"]);
        assert!(r["elements"].as_array().unwrap().is_empty());
        let r = run(&mut s, "analyze.spof", json!({}));
        assert_eq!(strs(&r["elements"]), ["do"]);
        assert_eq!(strs(&r["relationships"]), ["f1"]);
        let r = run(&mut s, "analyze.metrics", json!({"sort": "fanIn"}));
        assert_eq!(r["rows"][0]["id"], "do");
        assert_eq!(r["rows"][0]["fanIn"], 2);
        assert_eq!(r["totals"]["components"], 2);
        assert_eq!(r["totals"]["cycles"], 1);
        assert_eq!(r["cyclic"], true);
        assert_eq!(r["table"]["columns"], json!(["fanIn", "fanOut", "layer", "containmentDepth"]));
        // Every table row is named, not only the elements on the critical path.
        let rows = r["table"]["rows"].as_array().unwrap();
        assert!(rows.iter().all(|row| r["names"].get(row["id"].as_str().unwrap()).is_some()), "{}", r["names"]);
        assert!(r["names"].get("lone").is_some());
        let r = run(&mut s, "analyze.path", json!({"from": "start", "to": "ok"}));
        assert_eq!((strs(&r["elements"]), r["length"].clone()), (vec!["start".to_string(), "do".into(), "ok".into()], json!(2)));
        assert!(s.run("analyze.path", &json!({"from": "start", "to": "lone"})).is_err());
        assert!(s.run("analyze.cycles", &json!({"types": ["flowchart.Nope"]})).unwrap()["elements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_result_is_session_state_with_no_undo_step_and_a_clean_document() {
        let mut s = flow();
        s.dirty = false;
        let (steps, rev, doc) = (s.transactions().len(), s.rev(), s.doc.clone());
        assert_eq!(run(&mut s, "analyze.result", json!({})), Value::Null);
        run(&mut s, "analyze.cycles", json!({}));
        run(&mut s, "analyze.dependencies", json!({"id": "do"}));
        let stored = s.analysis.clone().unwrap();
        assert_eq!((stored.kind.as_str(), stored.rev), ("dependencies", rev));
        assert_eq!((s.transactions().len(), s.rev(), s.dirty, &s.doc), (steps, rev, false, &doc));
        let r = run(&mut s, "analyze.result", json!({}));
        assert_eq!((r["kind"].clone(), r["stale"].clone()), (json!("dependencies"), json!(false)));
        run(&mut s, "element.rename", json!({"id": "do", "name": "Done"}));
        assert_eq!(run(&mut s, "analyze.result", json!({}))["stale"], true);
        assert_eq!(run(&mut s, "analyze.clear", json!({}))["cleared"], true);
        assert!(s.analysis.is_none());
        assert_eq!(run(&mut s, "analyze.clear", json!({}))["cleared"], false);
        run(&mut s, "analyze.cycles", json!({}));
        s.set_document(vizcraft_diagram::Document::new());
        assert!(s.analysis.is_none(), "a new document drops the result");
    }

    #[test]
    fn select_selects_the_drawing_items_of_the_result() {
        let mut s = flow();
        assert!(s.run("analyze.select", &json!({})).is_err());
        run(&mut s, "analyze.dependencies", json!({"id": "do"}));
        let r = run(&mut s, "analyze.select", json!({}));
        let mut want = shapes_of(&s, "do");
        want.extend(shapes_of(&s, "ok"));
        want.extend(shapes_of(&s, "f2"));
        let mut got = strs(&r["items"]);
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(s.sel.items.len(), 3);
    }

    #[test]
    fn diff_against_the_saved_file_reports_a_rename_and_needs_a_path() {
        let mut s = flow();
        assert!(s.run("model.diff", &json!({})).is_err(), "never saved: no file to compare with");
        assert!(s.run("model.diff", &json!({"against": "head"})).is_err());
        let dir = std::env::temp_dir().join(format!("vizcraft-analyze-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("flow.vizcraft");
        run(&mut s, "file.saveAs", json!({"path": file.to_string_lossy()}));
        let r = run(&mut s, "model.diff", json!({}));
        assert!(r["elements"]["changed"].as_array().unwrap().is_empty(), "{r}");
        run(&mut s, "element.rename", json!({"id": "do", "name": "Done"}));
        let r = run(&mut s, "model.diff", json!({}));
        assert_eq!(r["elements"]["changed"], json!([{"id": "do", "fields": [{"field": "name", "before": "Do", "after": "Done"}]}]));
        let stored = s.analysis.clone().unwrap();
        assert_eq!((stored.kind.as_str(), stored.marks.get("do")), ("diff", Some(&vizcraft_analysis::Mark::Changed)));
        let r = run(&mut s, "model.diff", json!({"path": file.to_string_lossy()}));
        assert_eq!(r["elements"]["changed"][0]["id"], "do");
        let mut b = blank();
        let r = run(&mut b, "model.diff", json!({"against": "file", "path": file.to_string_lossy()}));
        assert_eq!(strs(&r["elements"]["removed"]), ["start", "do", "ok"]);
        assert_eq!(b.analysis.as_ref().unwrap().groups[0].elements.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A small pack with a subtype and an inverse relationship type, added to the catalog once.
    fn pack() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let pack = json!({
                "id": "antest", "version": "1.0.0",
                "elementTypes": [{"id": "antest.Node"}, {"id": "antest.Sub", "extends": "antest.Node"}, {"id": "antest.Thing"}],
                "relationshipTypes": [{"id": "antest.dependsOn"}, {"id": "antest.usedBy", "inverseOf": "antest.dependsOn"}],
                "connections": [{"source": ["*"], "relationship": "antest.dependsOn", "target": ["*"]}, {"source": ["*"], "relationship": "antest.usedBy", "target": ["*"]}]
            });
            let p = vizcraft_notation::loader::load_json(&pack.to_string()).unwrap();
            vizcraft_notation::catalog::add_pack(p).unwrap();
        });
    }

    #[test]
    fn types_follow_a_subtype_and_an_inverse_type_backwards() {
        pack();
        let mut s = blank();
        for (id, ty) in [("a", "antest.Node"), ("b", "antest.Sub"), ("c", "antest.Node"), ("x", "antest.Thing")] {
            // Not drawn: the pack has no glyphs, and the analysis reads only the model.
            run(&mut s, "element.create", json!({"type": ty, "id": id}));
        }
        run(&mut s, "relationship.create", json!({"from": "a", "to": "b", "type": "antest.dependsOn", "id": "r1"}));
        // c usedBy b: b depends on c.
        run(&mut s, "relationship.create", json!({"from": "c", "to": "b", "type": "antest.usedBy", "id": "r2"}));
        run(&mut s, "relationship.create", json!({"from": "a", "to": "x", "type": "antest.dependsOn", "id": "r3"}));
        let r = run(&mut s, "analyze.dependencies", json!({"id": "a", "types": ["antest.dependsOn"], "elements": ["antest.Node"]}));
        assert_eq!(strs(&r["elements"]), ["a", "b", "c"]);
        assert_eq!(strs(&r["relationships"]), ["r1", "r2"]);
        let r = run(&mut s, "analyze.dependencies", json!({"id": "a", "types": ["antest.dependsOn"]}));
        assert_eq!(strs(&r["elements"]), ["a", "b", "c", "x"]);
        let r = run(&mut s, "analyze.dependencies", json!({"id": "a"}));
        assert_eq!(strs(&r["elements"]), ["a", "b", "x"], "with no types every link is followed forwards");
    }

    /// The global junk-params test runs on the sample, which has no model, so it only reaches the
    /// disabled path of most of these; this one runs them on a model-bearing document.
    #[test]
    fn hostile_params_never_panic_on_analyze() {
        let junk = [
            json!(null),
            json!({}),
            json!([]),
            json!("text"),
            json!({"id": 5, "from": {}, "to": [], "direction": 3, "depth": "x"}),
            json!({"id": "do", "depth": 0}),
            json!({"id": "do", "depth": 1e308, "direction": "both"}),
            json!({"id": "x".repeat(70_000), "from": ["x".repeat(300)], "to": "\u{0}"}),
            json!({"from": vec!["do"; 20_000]}),
            json!({"from": [null, 1, {}], "types": [1, null]}),
            json!({"types": vec!["a.b"; 100], "elements": "x", "containment": "yes"}),
            json!({"types": ["x".repeat(500)], "containment": true, "sort": "fanIn", "limit": -1}),
            json!({"sort": 5, "limit": 1e300}),
            json!({"from": "start", "to": "ok", "direction": "upstream"}),
            json!({"against": "head", "path": "/nonexistent/\u{0}.vizcraft"}),
            json!({"against": "file"}),
            json!({"against": 7, "path": []}),
            json!({"path": "x".repeat(10_000)}),
        ];
        let ids: Vec<&str> = s_ids();
        for id in ids {
            for j in &junk {
                for prime in [false, true] {
                    let mut s = flow();
                    if prime {
                        run(&mut s, "analyze.cycles", json!({"containment": true}));
                        let shapes = shapes_of(&s, "do");
                        crate::cmd::select(&mut s, shapes);
                    }
                    let _ = s.run(id, j);
                    if let Some(r) = &s.analysis {
                        for x in r.ids() {
                            assert!(s.doc.model.contains_id(&x), "{id} {j}: {x}");
                        }
                    }
                    let page = s.doc.page().unwrap();
                    for x in &s.sel.items {
                        assert!(page.contains_id(x), "{id}: stale selection {x}");
                    }
                }
            }
        }
    }

    fn s_ids() -> Vec<&'static str> {
        super::specs().iter().map(|c| c.id).collect()
    }
}
