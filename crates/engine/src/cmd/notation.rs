//! Notation commands: list packs, describe a pack or type, legal relationships from an element or type. They read the catalog only.
//!
//! These are how an agent discovers what it may build: which packs exist, what types and
//! properties a pack defines, and which relationships may join two things. They never change the
//! document, so they are always enabled and never add an undo step.

use serde_json::{Value, json};
use vizcraft_notation::{Catalog, legal};

use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all notation commands: `notation.list`, `notation.describe`, `notation.legal`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("notation.list", "Notation Packs", "Model > Notation", list)
            .params(r#"{} -> {"packs": [{"id", "version", "name", "audience", "description", "types", "glyphs"}]}; types and glyphs are counts"#),
        CommandSpec::new("notation.describe", "Describe Notation", "Model > Notation", describe).params(
            r#"{"pack": string, "type"?: string} -> the whole pack schema; with type: {"type": element type, "pack", "legal": [{"relationship", "targets"}]} or {"relationship": relationship type, "pack"}; type may omit the "<pack>." prefix"#,
        ),
        CommandSpec::new("notation.legal", "Legal Relationships", "Model > Notation", legal_cmd).params(
            r#"{"from": string, "to"?: string} -> {"from": type, "relationships": [{"relationship", "targets"}], "to"?: type, "legal"?: bool}; from and to are element ids, shape ids of element views, or element type ids; with to, only relationships reaching its type are listed"#,
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
        assert_eq!(ids, ["basic", "flowchart"]);
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
