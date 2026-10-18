//! Subprocess commands: `process.createSubprocess` moves shapes onto a page of their own and
//! leaves one Predefined Process shape linked to it; `process.linkSubprocess` links an existing
//! shape to a page. The link is an ordinary page hyperlink (`link.follow` goes in).

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::{Hyperlink, LinkKind, Rect, Shape, ShapeKind, arrange};

use super::page::MAX_PAGES;
use super::{ids_param, opt_str, select, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Process > Subprocess";

/// Size of the shape left behind, in page units.
const SHAPE_W: f64 = 140.0;
/// Height of the shape left behind, in page units.
const SHAPE_H: f64 = 70.0;
/// Longest page name this command makes, in bytes.
const MAX_NAME: usize = 128;

/// Return the subprocess commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("process.createSubprocess", "Create Subprocess", LOCATION, create)
            .params(r#"{"ids"?: [shape] (default: the selected shapes; [] makes an empty subprocess page), "name"?: string} -> {page, shape, moved: [ids], connectors: [ids]}; the shapes (groups with their members) and the connectors between them move to a new page, a Predefined Process shape linked to it takes their place, connectors from outside re-glue to it, and it is selected; one undo step"#),
        CommandSpec::new("process.linkSubprocess", "Link to Existing Subprocess", LOCATION, link)
            .params(r#"{"id"?: shape (default: the primary selection), "page": id|name|index} -> {id, page, index}; adds a page link to the shape (none when it already has one to that page)"#),
    ]
}

/// `name` or a default, made unique among the page names.
fn page_name(s: &Session, v: &Value) -> Result<String, EngineError> {
    let base = match opt_str(v, "name")?.map(str::trim) {
        Some("") | None => "Subprocess".to_string(),
        Some(n) => super::model::cut(n, MAX_NAME),
    };
    let taken = |n: &str| s.doc.pages.iter().any(|p| p.name == n);
    if !taken(&base) {
        return Ok(base);
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).ok_or_else(|| EngineError::Failed("no free page name".into()))
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    if s.doc.pages.len() >= MAX_PAGES {
        return Err(EngineError::Params(format!("a document has at most {MAX_PAGES} pages")));
    }
    let given = ids_param(v)?;
    let set: Vec<String> = {
        let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        let mut roots: Vec<String> = Vec::new();
        match &given {
            Some(ids) => {
                for id in ids {
                    if page.shape(id).is_none() {
                        return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(id))));
                    }
                    roots.push(id.clone());
                }
            }
            None => roots.extend(s.sel.items.iter().filter(|i| page.shape(i).is_some()).cloned()),
        }
        if roots.is_empty() && given.is_none() {
            return Err(EngineError::Params("select shapes first, or pass `ids` ([] for an empty subprocess page)".into()));
        }
        let all = arrange::with_descendants(page, &roots);
        all.into_iter().filter(|i| page.shape(i).is_some()).collect()
    };
    super::layer::ensure_unlocked(s, &set)?;
    let name = page_name(s, v)?;
    let new_page = s.doc.next_id("p");
    let new_shape = s.doc.next_id("s");
    let inside: HashSet<&str> = set.iter().map(String::as_str).collect();

    // Take the shapes and the connectors between them off the active page, keeping their ids.
    let (moved_shapes, moved_connectors, centre, mut page) = {
        let from = s.doc.page_mut().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
        // The new page takes the sheet, units, scale, board, background and layers of this one.
        let mut page = from.clone();
        page.id = new_page.clone();
        page.name = name.clone();
        page.shapes = Vec::new();
        page.connectors = Vec::new();
        page.view = None;
        page.is_background = false;

        let mut bounds: Option<Rect> = None;
        let mut shapes: Vec<Shape> = Vec::new();
        for id in from.shapes.iter().filter(|x| inside.contains(x.id.as_str())).map(|x| x.id.clone()).collect::<Vec<_>>() {
            if let Some(mut sh) = from.remove_shape(&id) {
                let b = sh.bounds();
                bounds = Some(bounds.map_or(b, |r| r.union(&b)));
                if sh.parent.as_ref().is_some_and(|p| !inside.contains(p.as_str())) {
                    sh.parent = None;
                }
                shapes.push(sh);
            }
        }
        let both_inside = |c: &vizcraft_diagram::Connector| {
            c.start.shape().is_some_and(|x| inside.contains(x)) && c.end.shape().is_some_and(|x| inside.contains(x))
        };
        let connector_ids: Vec<String> = from.connectors.iter().filter(|c| both_inside(c)).map(|c| c.id.clone()).collect();
        let connectors: Vec<_> = connector_ids.iter().filter_map(|id| from.remove_connector(id)).collect();
        let centre = match bounds {
            Some(b) => (b.x0 + b.width() / 2.0, b.y0 + b.height() / 2.0),
            None => (f64::from(s.view.pan[0]), f64::from(s.view.pan[1])),
        };
        (shapes, connectors, centre, page)
    };
    let moved_ids: Vec<String> = moved_shapes.iter().map(|x| x.id.clone()).collect();
    let connector_ids: Vec<String> = moved_connectors.iter().map(|c| c.id.clone()).collect();
    page.shapes = moved_shapes;
    page.connectors = moved_connectors;

    // The stand-in shape, linked to the new page.
    let mut stand = Shape::new(new_shape.clone(), ShapeKind::PredefinedProcess, centre.0 - SHAPE_W / 2.0, centre.1 - SHAPE_H / 2.0, SHAPE_W, SHAPE_H);
    stand.text = name.clone();
    stand.links.push(Hyperlink { kind: LinkKind::Page, target: new_page.clone(), label: name.clone() });
    let from = s.doc.page_mut().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    // Connectors from outside that ended on a moved shape now end on the stand-in.
    for c in &mut from.connectors {
        for end in [&mut c.start, &mut c.end] {
            if end.shape().is_some_and(|x| inside.contains(x)) {
                *end = vizcraft_diagram::End::dynamic(new_shape.clone());
            }
        }
    }
    from.shapes.push(stand);
    let active = s.doc.active_index();
    if s.doc.active_page.is_empty() {
        s.doc.active_page = s.doc.pages.get(active).map(|p| p.id.clone()).unwrap_or_default();
    }
    let at = active.saturating_add(1).min(s.doc.pages.len());
    s.doc.pages.insert(at, page);
    // Comments on the moved shapes go with them.
    for c in &mut s.doc.comments {
        if c.anchor.shape.as_ref().is_some_and(|x| inside.contains(x.as_str())) {
            c.anchor.page = new_page.clone();
        }
    }
    select(s, vec![new_shape.clone()]);
    Ok(json!({"page": new_page, "shape": new_shape, "moved": moved_ids, "connectors": connector_ids}))
}

fn link(s: &mut Session, v: &Value) -> CmdResult {
    let r = match v.get("page") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err(EngineError::Params("`page` (id, name or index) is required".into())),
    };
    let pi = s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))?;
    let (page_id, page_name) =
        s.doc.pages.get(pi).map(|p| (p.id.clone(), p.name.clone())).ok_or_else(|| EngineError::Failed("no such page".into()))?;
    let active = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let id = match opt_str(v, "id")? {
        Some(i) => i.to_string(),
        None => s
            .sel
            .primary
            .clone()
            .filter(|i| active.shape(i).is_some())
            .ok_or_else(|| EngineError::Params("`id` (shape) is required, or select a shape first".into()))?,
    };
    if active.shape(&id).is_none() {
        return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(&id))));
    }
    super::layer::ensure_unlocked(s, [&id])?;
    let sh = s.doc.shape_mut(&id).ok_or_else(|| EngineError::Failed("the shape vanished".into()))?;
    if let Some(i) = sh.links.iter().position(|l| l.kind == LinkKind::Page && l.target == page_id) {
        return Ok(json!({"id": id, "page": page_id, "index": i}));
    }
    if sh.links.len() >= Hyperlink::MAX_PER_SHAPE {
        return Err(EngineError::Params(format!("a shape has at most {} links", Hyperlink::MAX_PER_SHAPE)));
    }
    sh.links.push(Hyperlink { kind: LinkKind::Page, target: page_id.clone(), label: page_name });
    Ok(json!({"id": id, "page": page_id, "index": sh.links.len().saturating_sub(1)}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Session, sample};

    fn session() -> Session {
        Session::new(sample::sample_document())
    }

    fn id_of(s: &Session, text: &str) -> String {
        s.doc.page().and_then(|p| p.shapes.iter().find(|x| x.text == text)).map(|x| x.id.clone()).unwrap()
    }

    #[test]
    fn creating_a_subprocess_moves_reglues_links_and_undoes_in_one_step() {
        let mut s = session();
        let (gather, valid, save) = (id_of(&s, "Gather input"), id_of(&s, "Valid?"), id_of(&s, "Save diagram"));
        let before = s.doc.clone();
        let steps = s.transactions().len();
        s.run("edit.select", &json!({"ids": [valid.clone(), save.clone()]})).unwrap();
        let r = s.run("process.createSubprocess", &json!({"name": "Check and save"})).unwrap();
        let (page, stand) = (r["page"].as_str().unwrap().to_string(), r["shape"].as_str().unwrap().to_string());
        assert_eq!(s.transactions().len(), steps + 1, "one undo step");
        // The active page keeps one stand-in for the two moved shapes.
        let main = s.doc.page().unwrap();
        assert!(main.shape(&valid).is_none() && main.shape(&save).is_none());
        let sh = main.shape(&stand).unwrap();
        assert_eq!(sh.text, "Check and save");
        assert_eq!(sh.kind, vizcraft_diagram::ShapeKind::PredefinedProcess);
        assert_eq!((sh.links[0].kind, sh.links[0].target.as_str()), (vizcraft_diagram::LinkKind::Page, page.as_str()));
        assert_eq!(s.sel.items, vec![stand.clone()]);
        // The connector from outside (Gather input to Valid?) now ends on the stand-in; the
        // connector between the two moved shapes went with them.
        let into = main.connectors.iter().find(|c| c.start.shape() == Some(gather.as_str())).unwrap();
        assert_eq!(into.end.shape(), Some(stand.as_str()));
        let sub = s.doc.pages.iter().find(|p| p.id == page).unwrap();
        assert!(sub.shape(&valid).is_some() && sub.shape(&save).is_some());
        assert_eq!(sub.connectors.len(), 1);
        assert!(
            main.connectors
                .iter()
                .all(|c| c.start.shape().is_none_or(|x| main.shape(x).is_some()) && c.end.shape().is_none_or(|x| main.shape(x).is_some()))
        );
        // Follow goes in; the link on the stand-in is an ordinary page link.
        s.run("link.follow", &json!({"id": stand})).unwrap();
        assert_eq!(s.doc.page().unwrap().id, page);
        s.run("page.setActive", &json!({"page": 0})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.pages, before.pages);
    }

    #[test]
    fn an_empty_subprocess_and_a_link_to_an_existing_page() {
        let mut s = session();
        assert!(s.run("process.createSubprocess", &json!({})).is_err(), "nothing selected");
        let r = s.run("process.createSubprocess", &json!({"ids": [], "name": "Later"})).unwrap();
        assert_eq!(s.doc.pages.len(), 2);
        assert!(r["moved"].as_array().unwrap().is_empty());
        let end = id_of(&s, "End");
        s.run("process.linkSubprocess", &json!({"id": end, "page": "Later"})).unwrap();
        let again = s.run("process.linkSubprocess", &json!({"id": end, "page": "Later"})).unwrap();
        assert_eq!(again["index"], 0);
        assert_eq!(s.doc.shape(&end).unwrap().links.len(), 1);
        // A second subprocess of the same name gets a free one.
        s.run("process.createSubprocess", &json!({"ids": [], "name": "Later"})).unwrap();
        assert!(s.doc.pages.iter().any(|p| p.name == "Later 2"));
    }

    #[test]
    fn a_group_moves_with_its_members_and_comments_follow() {
        let mut s = session();
        let (a, b) = (id_of(&s, "Start"), id_of(&s, "Gather input"));
        s.run("edit.select", &json!({"ids": [a.clone(), b.clone()]})).unwrap();
        s.run("arrange.group", &json!({})).unwrap();
        let group = s.sel.primary.clone().unwrap();
        s.run("comment.add", &json!({"text": "note", "shape": a})).unwrap();
        let r = s.run("process.createSubprocess", &json!({"ids": [group.clone()]})).unwrap();
        let moved: Vec<&str> = r["moved"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
        assert!(moved.contains(&group.as_str()) && moved.contains(&a.as_str()) && moved.contains(&b.as_str()));
        assert_eq!(s.doc.comments[0].anchor.page, r["page"].as_str().unwrap());
    }
}
