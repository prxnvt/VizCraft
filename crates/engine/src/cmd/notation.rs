//! Notation commands: list packs, describe a pack or type, legal relationships from an element or
//! type, the rulesets packs declare, and loading a pack from disk.
//!
//! These are how an agent discovers what it may build: which packs exist, what types and
//! properties a pack defines, and which relationships may join two things. They never change the
//! document, so they are always enabled and never add an undo step. `notation.load` adds a pack
//! to the catalog every session reads (see `vizcraft_notation::catalog`), so a new notation can
//! be dropped in and tried without a rebuild.

use std::path::Path;

use serde_json::{Value, json};
use vizcraft_notation::{Catalog, NotationError, cases, legal};

use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all notation commands: `notation.list`, `notation.describe`, `notation.legal`,
/// `notation.rulesets`, `notation.load`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("notation.list", "Notation Packs", "Model > Notation", list)
            .params(r#"{} -> {"packs": [{"id", "version", "name", "audience", "description", "types", "glyphs", "variants": [{"id", "label"}]}]}; types and glyphs are counts; variants are the rendering variants (see variant.list)"#),
        CommandSpec::new("notation.describe", "Describe Notation", "Model > Notation", describe).params(
            r#"{"pack": string, "type"?: string} -> the whole pack schema (glyphs with their path data, variants with their glyph and connector maps); with type: {"type": element type, "pack", "legal": [{"relationship", "targets"}]} or {"relationship": relationship type, "pack"}; type may omit the "<pack>." prefix"#,
        ),
        CommandSpec::new("notation.legal", "Legal Relationships", "Model > Notation", legal_cmd).params(
            r#"{"from": string, "to"?: string} -> {"from": type, "relationships": [{"relationship", "targets"}], "to"?: type, "legal"?: bool}; from and to are element ids, shape ids of element views, or element type ids; with to, only relationships reaching its type are listed"#,
        ),
        CommandSpec::new("notation.rulesets", "Rulesets", "Process > Validate", rulesets).params(
            r#"{} -> {"rulesets": [{"id", "label", "description", "pack", "rules", "active"}]}; every named ruleset of every pack; active: the document's profile holds it"#,
        ),
        CommandSpec::new("notation.load", "Load Notation Pack", "Model > Notation", load).params(
            r#"{"path": pack file (.json or .toml)} -> {"pack": {"id", "version", "name", "rules", "rulesets"} | null, "errors": [string], "cases": [{"name", "passed", "problems"}]}; checks the pack with the built-in limits and adds it to the catalog (a later load of the same id replaces it; built-in packs cannot be replaced), then runs its test cases from tests/<pack id>/ beside the file"#,
        ),
    ]
}

/// Longest id or type param looked at; real ids are far shorter, so anything longer is refused
/// rather than searched for.
const MAX_PARAM: usize = 256;

/// A required id-like string param of at most [`MAX_PARAM`] bytes.
fn id_param<'a>(v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
    let t = p::req_str(v, k)?;
    check_len(k, t)
}

fn check_len<'a>(k: &str, t: &'a str) -> Result<&'a str, EngineError> {
    if t.len() > MAX_PARAM {
        return Err(EngineError::Params(format!("`{k}` is longer than {MAX_PARAM} bytes")));
    }
    Ok(t)
}

/// Longest path param accepted, in bytes.
const MAX_PATH: usize = 4096;

/// Every ruleset in the catalog, with whether the document's profile holds it.
fn rulesets(s: &mut Session, _v: &Value) -> CmdResult {
    let profile = vizcraft_notation::rules::profile(&s.doc, cat());
    let list: Vec<Value> = cat()
        .rulesets()
        .into_iter()
        .map(|(pack, r)| {
            json!({
                "id": r.id,
                "label": r.label,
                "description": r.description,
                "pack": pack.id,
                "rules": r.rules,
                "active": profile.enabled && profile.rulesets.contains(&r.id),
            })
        })
        .collect();
    Ok(json!({"rulesets": list}))
}

/// Load a pack file into the catalog and run its test cases.
fn load(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(EngineError::Params(format!("`path` must be 1 to {MAX_PATH} bytes")));
    }
    let path = Path::new(path);
    if !path.is_file() {
        return Err(EngineError::Failed(format!("no pack file at {}", short(&path.display().to_string()))));
    }
    let pack = match vizcraft_notation::catalog::read_pack(path) {
        Ok(p) => p,
        Err(NotationError::Invalid(problems)) => return Ok(json!({"pack": null, "errors": problems, "cases": []})),
        Err(e) => return Ok(json!({"pack": null, "errors": [e.to_string()], "cases": []})),
    };
    let summary = json!({
        "id": pack.id,
        "version": pack.version,
        "name": pack.name,
        "rules": pack.rules.len(),
        "rulesets": pack.rulesets.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
    });
    let id = pack.id.clone();
    let cat = match vizcraft_notation::catalog::add_pack(pack) {
        Ok(c) => c,
        Err(e) => return Ok(json!({"pack": null, "errors": [e], "cases": []})),
    };
    // Its rules may apply to the open document now.
    s.refresh_issues();
    let dir = path.parent().map(|d| cases::cases_dir(d, &id));
    let outcomes = dir.map(|d| cases::run_dir(&d, cat)).unwrap_or_default();
    let cases: Vec<Value> = outcomes.iter().map(|o| json!({"name": o.name, "passed": o.passed(), "problems": o.problems})).collect();
    Ok(json!({"pack": summary, "errors": [], "cases": cases}))
}

fn cat() -> &'static Catalog {
    vizcraft_notation::catalog()
}

/// Every loaded pack in one line each.
fn list(_s: &mut Session, _v: &Value) -> CmdResult {
    let packs: Vec<Value> = cat()
        .packs()
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "version": p.version,
                "name": p.name,
                "audience": p.audience,
                "description": p.description,
                "types": p.element_types.len(),
                "glyphs": p.glyphs.len(),
                "variants": p.variants.iter().map(|x| json!({"id": x.id, "label": x.label})).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({"packs": packs}))
}

fn to_json<T: serde::Serialize>(t: &T) -> CmdResult {
    serde_json::to_value(t).map_err(|e| EngineError::Failed(e.to_string()))
}

/// A whole pack, or one element or relationship type of it.
fn describe(_s: &mut Session, v: &Value) -> CmdResult {
    let pack_id = id_param(v, "pack")?;
    let pack = cat().pack(pack_id).ok_or_else(|| EngineError::Params(format!("unknown notation pack `{}`", short(pack_id))))?;
    let Some(ty) = opt_str(v, "type")? else {
        return to_json(pack);
    };
    let ty = check_len("type", ty)?;
    // A bare local name (`Decision`) is read in the named pack.
    let qualified = if ty.contains('.') { ty.to_string() } else { format!("{}.{ty}", pack.id) };
    if let Some(t) = pack.element_type(&qualified) {
        return Ok(json!({"type": to_json(t)?, "pack": pack.id, "legal": to_json(&legal::legal_from(cat(), &t.id))?}));
    }
    if let Some(r) = pack.relationship_type(&qualified) {
        return Ok(json!({"relationship": to_json(r)?, "pack": pack.id}));
    }
    Err(EngineError::Params(format!("pack `{}` has no element or relationship type `{}`", pack.id, short(ty))))
}

/// The element type `id` stands for: a model element's type, the type of the element a shape on
/// any page draws, or `id` itself when it is a known element type.
fn type_of(s: &Session, k: &str, id: &str) -> Result<String, EngineError> {
    let model = &s.doc.model;
    if let Some(e) = model.element(id) {
        return Ok(e.type_id.clone());
    }
    let bound = s.doc.pages.iter().find_map(|p| p.shape(id)).and_then(|sh| sh.element.as_deref()).and_then(|e| model.element(e));
    if let Some(e) = bound {
        return Ok(e.type_id.clone());
    }
    if cat().element_type(id).is_some() {
        return Ok(id.to_string());
    }
    Err(EngineError::Params(format!("`{k}`: `{}` is not an element, a shape of an element, or an element type", short(id))))
}

/// Relationships that may start at `from`, optionally only those that may reach `to`.
fn legal_cmd(s: &mut Session, v: &Value) -> CmdResult {
    let from = type_of(s, "from", id_param(v, "from")?)?;
    let mut rels = legal::legal_from(cat(), &from);
    let Some(to) = opt_str(v, "to")? else {
        return Ok(json!({"from": from, "relationships": to_json(&rels)?}));
    };
    let to = type_of(s, "to", check_len("to", to)?)?;
    rels.retain(|l| l.targets.contains(&to));
    Ok(json!({"from": from, "to": to, "relationships": to_json(&rels)?, "legal": !rels.is_empty()}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::Document;
    use vizcraft_model::Element;

    use crate::Session;

    fn session() -> Session {
        Session::new(Document::new())
    }

    fn rel_ids(r: &Value) -> Vec<String> {
        r["relationships"].as_array().unwrap().iter().map(|l| l["relationship"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn list_names_every_builtin_pack_with_counts() {
        let mut s = session();
        let before = s.doc.clone();
        let r = s.run("notation.list", &json!({})).unwrap();
        let packs = r["packs"].as_array().unwrap();
        let ids: Vec<_> = packs.iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(ids.get(..3), Some(&["basic", "flowchart", "drawing"][..]), "built-in packs first; packs loaded by other tests follow");
        let f = &packs[1];
        assert_eq!(
            (f["name"].as_str(), f["version"].as_str(), f["types"].as_u64(), f["glyphs"].as_u64()),
            (Some("Flowchart"), Some("1.0.0"), Some(12), Some(12))
        );
        assert!(f["audience"].is_array() && f["description"].as_str().is_some_and(|d| !d.is_empty()));
        assert_eq!((packs[0]["types"].as_u64(), packs[0]["glyphs"].as_u64()), (Some(0), Some(10)));
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }

    #[test]
    fn describe_a_pack_and_its_types() {
        let mut s = session();
        let pack = s.run("notation.describe", &json!({"pack": "flowchart"})).unwrap();
        assert_eq!(pack["id"], "flowchart");
        assert_eq!(pack["elementTypes"].as_array().unwrap().len(), 12);
        assert_eq!(pack["rules"].as_array().unwrap().len(), 12);
        let t = s.run("notation.describe", &json!({"pack": "flowchart", "type": "flowchart.PredefinedProcess"})).unwrap();
        assert_eq!((t["pack"].as_str(), t["type"]["id"].as_str()), (Some("flowchart"), Some("flowchart.PredefinedProcess")));
        assert_eq!(t["type"]["properties"][0]["key"], "reference");
        assert_eq!(t["legal"][0]["relationship"], "flowchart.Flow");
        // A local name is read in the named pack.
        let t = s.run("notation.describe", &json!({"pack": "flowchart", "type": "Decision"})).unwrap();
        assert_eq!(t["type"]["id"], "flowchart.Decision");
        let r = s.run("notation.describe", &json!({"pack": "flowchart", "type": "flowchart.Flow"})).unwrap();
        assert_eq!((r["relationship"]["id"].as_str(), r["relationship"]["endArrow"].as_str()), (Some("flowchart.Flow"), Some("triangle")));
        let e = s.run("notation.describe", &json!({"pack": "flowchart", "type": "flowchart.Ghost"})).unwrap_err().to_string();
        assert!(e.contains("flowchart.Ghost"), "{e}");
        let e = s.run("notation.describe", &json!({"pack": "uml"})).unwrap_err().to_string();
        assert!(e.contains("uml"), "{e}");
        for bad in [json!({}), json!({"pack": 3}), json!({"pack": "flowchart", "type": 1}), json!({"pack": "x".repeat(100_000)})] {
            assert!(s.run("notation.describe", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn legal_from_types_elements_and_shapes() {
        let mut s = session();
        let r = s.run("notation.legal", &json!({"from": "flowchart.Database"})).unwrap();
        assert_eq!(r["from"], "flowchart.Database");
        assert_eq!(rel_ids(&r), ["flowchart.Flow"]);
        let targets = r["relationships"][0]["targets"].as_array().unwrap();
        assert!(!targets.iter().any(|t| t == "flowchart.Database"));
        let r = s.run("notation.legal", &json!({"from": "flowchart.Database", "to": "flowchart.Database"})).unwrap();
        assert_eq!((r["legal"].as_bool(), r["to"].as_str()), (Some(false), Some("flowchart.Database")));
        assert!(rel_ids(&r).is_empty());
        let r = s.run("notation.legal", &json!({"from": "flowchart.Process", "to": "flowchart.Decision"})).unwrap();
        assert_eq!(r["legal"], true);
        assert_eq!(rel_ids(&r), ["flowchart.Flow"]);

        // An element of the model, and a shape that draws it, stand for the element's type.
        s.doc.model.add_element(Element::new("e1", "flowchart.Terminator")).unwrap();
        s.doc.model.add_element(Element::new("e2", "flowchart.Data")).unwrap();
        let mut shape = vizcraft_stencils::new_shape("flowchart.data", "s1", 0.0, 0.0, None, None).unwrap();
        shape.element = Some("e2".into());
        s.doc.pages[0].shapes.push(shape);
        let r = s.run("notation.legal", &json!({"from": "e1", "to": "s1"})).unwrap();
        assert_eq!((r["from"].as_str(), r["to"].as_str(), r["legal"].as_bool()), (Some("flowchart.Terminator"), Some("flowchart.Data"), Some(true)));
        let r = s.run("notation.legal", &json!({"from": "s1", "to": "flowchart.Data"})).unwrap();
        assert_eq!(r["legal"], false);
        // A free shape is not an element.
        s.doc.pages[0].shapes.push(vizcraft_stencils::new_shape("basic.rectangle", "s2", 0.0, 0.0, None, None).unwrap());
        assert!(s.run("notation.legal", &json!({"from": "s2"})).is_err());
        assert!(!s.can_undo());
    }

    #[test]
    fn a_pack_loads_from_disk_and_runs_its_cases() {
        let dir = std::env::temp_dir().join(format!("vizcraft-notation-load-{}", std::process::id()));
        let cases = dir.join("tests").join("engtest");
        std::fs::create_dir_all(&cases).unwrap();
        let pack = json!({
            "id": "engtest", "version": "1.0.0", "name": "Engine test",
            "elementTypes": [{"id": "engtest.Node"}],
            "rules": [{"id": "engtest.named", "severity": "error", "reference": "Test clause 1", "check": {"kind": "nameFormat", "types": ["engtest.Node"], "pattern": "N*"}}],
            "rulesets": [{"id": "engtest.strict", "label": "Strict", "rules": ["engtest.named"]}]
        });
        std::fs::write(dir.join("engtest.json"), pack.to_string()).unwrap();
        let model = json!({"schemaVersion": 1, "elements": [{"id": "e1", "type": "engtest.Node", "name": "bad"}, {"id": "e2", "type": "engtest.Node", "name": "N2"}]});
        let document = json!({"format": "vizcraft", "version": 4, "model": model, "document": {"pages": [{"id": "p1", "name": "P"}]}});
        std::fs::write(cases.join("names.json"), json!({"document": document, "expect": [{"rule": "engtest.named"}]}).to_string()).unwrap();
        std::fs::write(cases.join("wrong.json"), json!({"document": document, "expect": []}).to_string()).unwrap();
        std::fs::write(dir.join("broken.json"), "{\"id\": \"Broken Pack\"}").unwrap();

        let mut s = session();
        let r = s.run("notation.load", &json!({"path": dir.join("engtest.json")})).unwrap();
        assert_eq!(r["errors"], json!([]));
        assert_eq!((r["pack"]["id"].as_str(), r["pack"]["rules"].as_u64()), (Some("engtest"), Some(1)));
        let results: Vec<(String, bool)> =
            r["cases"].as_array().unwrap().iter().map(|c| (c["name"].as_str().unwrap().to_string(), c["passed"].as_bool().unwrap())).collect();
        assert_eq!(results, [("names.json".to_string(), true), ("wrong.json".to_string(), false)]);
        // The pack is in the catalog now: its rule explains and its ruleset lists.
        let e = s.run("validate.explain", &json!({"rule": "engtest.named"})).unwrap();
        assert_eq!((e["reference"].as_str(), e["ruleset"].as_str()), (Some("Test clause 1"), Some("engtest.strict")));
        let rs = s.run("notation.rulesets", &json!({})).unwrap();
        assert!(rs["rulesets"].as_array().unwrap().iter().any(|r| r["id"] == "engtest.strict" && r["pack"] == "engtest"));
        // A broken pack reports why; a missing file and a built-in id are refused.
        let r = s.run("notation.load", &json!({"path": dir.join("broken.json")})).unwrap();
        assert!(r["pack"].is_null() && !r["errors"].as_array().unwrap().is_empty(), "{r}");
        assert!(s.run("notation.load", &json!({"path": dir.join("missing.json")})).is_err());
        let fake = dir.join("flowchart.json");
        std::fs::write(&fake, json!({"id": "flowchart", "version": "9.0.0"}).to_string()).unwrap();
        let r = s.run("notation.load", &json!({"path": fake})).unwrap();
        assert!(r["errors"][0].as_str().unwrap().contains("built in"), "{r}");
        for bad in [json!({}), json!({"path": 3}), json!({"path": ""}), json!({"path": "p".repeat(10_000)})] {
            assert!(s.run("notation.load", &bad).is_err(), "{bad}");
        }
        assert!(!s.can_undo());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legal_refuses_unknown_and_junk() {
        let mut s = session();
        let e = s.run("notation.legal", &json!({"from": "nothing"})).unwrap_err().to_string();
        assert!(e.contains("nothing"), "{e}");
        assert!(s.run("notation.legal", &json!({"from": "flowchart.Process", "to": "nowhere"})).is_err());
        // Basic has glyphs but no element types.
        assert!(s.run("notation.legal", &json!({"from": "basic.rectangle"})).is_err());
        for bad in [json!({}), json!({"from": 1}), json!({"from": "flowchart.Process", "to": []}), json!({"from": "y".repeat(10_000)})] {
            assert!(s.run("notation.legal", &bad).is_err(), "{bad}");
        }
    }
}
