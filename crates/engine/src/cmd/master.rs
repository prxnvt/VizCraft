//! Custom masters (the Document Stencil): make a master from the selection, list, remove and
//! insert them, and export or import them as a `.vizcraft-stencil` file.
//!
//! A master is a [`CustomMaster`] snapshot built from a selection [`Fragment`]: the shapes, the
//! connectors among them with their glue, and the model items they draw. Inserting goes back
//! through [`Fragment::insert`], so a copy gets fresh ids and keeps its inner glue.

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::CustomMaster;
use vizcraft_diagram::master::{STENCIL_EXTENSION, from_stencil_json, to_stencil_json};

use super::{Kinds, MAX_DELTA, num, opt_str, select, short, targets, with_sel};
use crate::fragment::Fragment;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// The custom master commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("master.add", "Add to Document Stencil", "Document Stencil", add)
            .params(r#"{"ids"?: [string] (default: the selection), "name"?: string (default "Master N")} -> {"id", "name"}; connectors between the shapes come along"#)
            .when(super::has_shapes),
        CommandSpec::new("master.list", "List Document Masters", "Document Stencil", list).params(r#"{} -> {"masters": [{"id", "name", "w", "h", "shapes", "connectors"}]}"#),
        CommandSpec::new("master.remove", "Remove Master", "Document Stencil", remove).params(r#"{"id": string} -> {"removed"}"#).when(has_masters),
        CommandSpec::new("master.insert", "Insert Master", "Document Stencil", insert)
            .params(r#"{"id": string, "x"?: n, "y"?: n} -> {"ids": [new shape and connector ids]}; x, y is the copy's top-left (default: centred on the page)"#)
            .when(has_masters),
        CommandSpec::new("master.export", "Export Document Stencil", "Document Stencil", export)
            .params(r#"{"path": string} -> {"path", "masters": n}; writes every master as a .vizcraft-stencil file (the extension is added when missing)"#)
            .when(has_masters),
        CommandSpec::new("master.import", "Import Stencil", "Document Stencil", import)
            .params(r#"{"path": string} -> {"ids": [new master ids]}; reads a .vizcraft-stencil file into the document's masters"#),
    ]
}

/// Enabled when the document has a master.
fn has_masters(s: &Session) -> Option<&'static str> {
    if s.doc.masters.is_empty() { Some("add a master to the Document Stencil first") } else { None }
}

/// The first `m<n>` id no master uses.
fn fresh_id(masters: &[CustomMaster]) -> String {
    let used: HashSet<&str> = masters.iter().map(|m| m.id.as_str()).collect();
    (1..).map(|n| format!("m{n}")).find(|id| !used.contains(id.as_str())).unwrap_or_default()
}

fn room(s: &Session, more: usize) -> Result<(), EngineError> {
    if s.doc.masters.len().saturating_add(more) > CustomMaster::MAX_MASTERS {
        Err(EngineError::Params(format!("a document keeps at most {} masters", CustomMaster::MAX_MASTERS)))
    } else {
        Ok(())
    }
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let mut ids = targets(s, v, Kinds::Both)?;
    room(s, 1)?;
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    // Connectors glued at both ends to chosen shapes come along, so the master keeps its glue.
    let inner: Vec<String> = page
        .connectors
        .iter()
        .filter(|c| !ids.contains(&c.id) && [c.start.shape(), c.end.shape()].iter().all(|e| e.is_some_and(|sh| ids.iter().any(|i| i == sh))))
        .map(|c| c.id.clone())
        .collect();
    ids.extend(inner);
    let frag = Fragment::of_items(&s.doc, &ids);
    if frag.shapes.is_empty() {
        return Err(EngineError::Params("select at least one shape for the master".into()));
    }
    if frag.shapes.len() > CustomMaster::MAX_ITEMS || frag.connectors.len() > CustomMaster::MAX_ITEMS {
        return Err(EngineError::Params(format!("a master holds at most {} shapes", CustomMaster::MAX_ITEMS)));
    }
    let id = fresh_id(&s.doc.masters);
    let name = match opt_str(v, "name")?.map(str::trim) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => format!("Master {}", s.doc.masters.len().saturating_add(1)),
    };
    let mut m = CustomMaster::new(id.clone(), &name, frag.shapes, frag.connectors)
        .ok_or_else(|| EngineError::Params("select at least one shape for the master".into()))?;
    m.elements = frag.elements;
    m.relationships = frag.relationships;
    m.sanitize();
    let name = m.name.clone();
    s.doc.masters.push(m);
    Ok(json!({"id": id, "name": name}))
}

fn list(s: &mut Session, _v: &Value) -> CmdResult {
    let masters: Vec<Value> = s
        .doc
        .masters
        .iter()
        .map(|m| json!({"id": m.id, "name": m.name, "w": m.w, "h": m.h, "shapes": m.shapes.len(), "connectors": m.connectors.len()}))
        .collect();
    Ok(json!({"masters": masters}))
}

fn master_index(s: &Session, id: &str) -> Result<usize, EngineError> {
    s.doc.masters.iter().position(|m| m.id == id).ok_or_else(|| EngineError::Params(format!("no master `{}` in the Document Stencil", short(id))))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let i = master_index(s, id)?;
    s.doc.masters.remove(i);
    Ok(json!({"removed": id}))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let i = master_index(s, id)?;
    let x = num(v, "x")?;
    let y = num(v, "y")?;
    let m = s.doc.masters.get(i).cloned().ok_or_else(|| EngineError::Params(format!("no master `{}`", short(id))))?;
    let centre = s.doc.page().map(|p| p.frame().center()).unwrap_or_default();
    let dx = x.unwrap_or(centre.x - m.w / 2.0).clamp(-MAX_DELTA, MAX_DELTA);
    let dy = y.unwrap_or(centre.y - m.h / 2.0).clamp(-MAX_DELTA, MAX_DELTA);
    let frag = Fragment { shapes: m.shapes, connectors: m.connectors, elements: m.elements, relationships: m.relationships };
    let ids = frag.insert(&mut s.doc, dx.round(), dy.round())?;
    select(s, ids.clone());
    with_sel(s, json!({"ids": ids}))
}

/// `path` with the stencil extension added when it has none.
fn stencil_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(&format!(".{STENCIL_EXTENSION}")) || lower.ends_with(".json") {
        path.to_string()
    } else {
        format!("{path}.{STENCIL_EXTENSION}")
    }
}

fn export(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    if path.trim().is_empty() {
        return Err(EngineError::Params("`path` must name a file".into()));
    }
    let path = stencil_path(path);
    let text = to_stencil_json(&s.doc.masters).map_err(EngineError::Failed)?;
    std::fs::write(&path, text).map_err(|e| EngineError::Failed(format!("{}: {e}", short(&path))))?;
    Ok(json!({"path": path, "masters": s.doc.masters.len()}))
}

fn import(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    let bytes = crate::io::read_limited(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let masters = from_stencil_json(&text).map_err(|e| EngineError::Failed(format!("{}: {e}", short(path))))?;
    if masters.is_empty() {
        return Err(EngineError::Failed(format!("{}: the stencil has no masters", short(path))));
    }
    room(s, masters.len())?;
    let mut ids = Vec::new();
    for mut m in masters {
        m.id = fresh_id(&s.doc.masters);
        ids.push(m.id.clone());
        s.doc.masters.push(m);
    }
    Ok(json!({"ids": ids}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Connector, Document, End, Shape, ShapeKind};

    use crate::Session;

    fn pair() -> Session {
        let mut doc = Document::new();
        let page = doc.page_mut().unwrap();
        page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 100.0, 100.0, 80.0, 40.0));
        page.shapes.push(Shape::new("b", ShapeKind::Ellipse, 300.0, 100.0, 40.0, 40.0));
        page.connectors.push(Connector::new("c", End::point("a", 1), End::dynamic("b")));
        doc.id_seq = 10;
        Session::new(doc)
    }

    #[test]
    fn a_master_from_the_selection_inserts_a_copy_with_fresh_ids_and_glue_kept() {
        let mut s = pair();
        s.run("edit.select", &json!({"ids": ["a", "b"]})).unwrap();
        let r = s.run("master.add", &json!({"name": "Pair"})).unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        assert_eq!(r["name"], "Pair");
        let l = s.run("master.list", &json!({})).unwrap();
        assert_eq!(l["masters"][0]["shapes"], 2);
        assert_eq!(l["masters"][0]["connectors"], 1);
        let ins = s.run("master.insert", &json!({"id": id, "x": 0, "y": 500})).unwrap();
        let new: Vec<String> = ins["ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        assert_eq!(new.len(), 3);
        assert!(new.iter().all(|i| !["a", "b", "c"].contains(&i.as_str())));
        let page = s.doc.page().unwrap();
        let c = page.connector(new.get(2).unwrap()).unwrap();
        assert_eq!(c.start.shape(), Some(new[0].as_str()));
        assert_eq!(c.end.shape(), Some(new[1].as_str()));
        let first = page.shape(&new[0]).unwrap();
        assert_eq!((first.x, first.y), (0.0, 500.0));
        assert_eq!(s.sel.items, new);
        // A second insert is another copy.
        s.run("master.insert", &json!({"id": id})).unwrap();
        assert_eq!(s.doc.page().unwrap().shapes.len(), 6);
        s.run("master.remove", &json!({"id": id})).unwrap();
        assert!(s.doc.masters.is_empty());
        assert!(s.run("master.insert", &json!({"id": id})).is_err());
    }

    #[test]
    fn masters_export_and_import_as_a_stencil_file() {
        let mut s = pair();
        s.run("master.add", &json!({"ids": ["a"], "name": "Box"})).unwrap();
        let dir = std::env::temp_dir().join(format!("vizcraft-stencil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("mine");
        let r = s.run("master.export", &json!({"path": base.to_string_lossy()})).unwrap();
        let path = r["path"].as_str().unwrap().to_string();
        assert!(path.ends_with(".vizcraft-stencil"), "{path}");
        let mut other = Session::new(Document::new());
        let r = other.run("master.import", &json!({"path": path})).unwrap();
        assert_eq!(r["ids"], json!(["m1"]));
        assert_eq!(other.doc.masters[0].name, "Box");
        assert_eq!(other.doc.masters[0].shapes, s.doc.masters[0].shapes);
        let junk = dir.join("junk.vizcraft-stencil");
        std::fs::write(&junk, "{\"vizcraft\": \"clip\"}").unwrap();
        assert!(other.run("master.import", &json!({"path": junk.to_string_lossy()})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
