//! Cross-reference commands: list where each element's partners are (sheet and zone), and go to
//! a partner. Partners are linked by a relationship whose type is a cross reference (the `sheet`
//! pack's `sheet.References` and `sheet.Continues`) or share a tag in a paired identifier scope;
//! the sync pass labels each view with its partners' locations (see
//! `vizcraft_notation::sheet_pass`).

use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_notation::sheet_pass::{self, EXT_XREF_FOR, Location, Partner};

use super::model::{cat, element_of};
use super::{opt_str, select, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Most entries `xref.list` returns.
const MAX_XREFS: usize = 10_000;

/// Return the cross-reference commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("xref.list", "Cross References", "Design > Sheet", list).params(
            r#"{"page"?: id|name|index, "element"?: element or shape id} -> {"xrefs": [{"relationship"|null, "from": {"element", "tag", "page", "sheet", "zone"}, "to": {...}, "text"}], "unresolved": [{"relationship"|null, "from", "to"}]}; read-only"#,
        ),
        CommandSpec::new("xref.go", "Go to Cross Reference", "Design > Sheet", go).params(
            r#"{"id": element, view or location label id, "index"?: n (which partner, default 0)} -> {"page", "shape", "element"}; shows the partner's page and selects its view; not an undo step"#,
        ),
    ]
}

fn loc_json(doc: &Document, element: &str, at: Option<&Location>) -> Value {
    let tag = doc.model.element(element).map(|e| e.tag.clone()).unwrap_or_default();
    match at {
        Some(l) => json!({"element": element, "tag": tag, "page": l.page, "shape": l.view, "sheet": l.sheet, "zone": l.zone}),
        None => json!({"element": element, "tag": tag, "page": null, "shape": null, "sheet": null, "zone": null}),
    }
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let doc = &s.doc;
    let page = match v.get("page") {
        None | Some(Value::Null) => None,
        Some(_) => Some(super::sheet::page_index(s, v)?),
    };
    let element = match opt_str(v, "element")? {
        None => None,
        Some(id) => Some(element_of(doc, id).ok_or_else(|| EngineError::Params(format!("`element`: `{}` is not an element", short(id))))?),
    };
    let partners = sheet_pass::partners(doc, cat());
    let mut ids: Vec<&String> = partners.keys().collect();
    // Model order, so equal documents list equally.
    let order: std::collections::HashMap<&str, usize> = doc.model.elements.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    ids.sort_by_key(|id| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
    let (mut xrefs, mut unresolved) = (Vec::new(), Vec::new());
    for id in ids {
        if element.as_ref().is_some_and(|e| e != id) {
            continue;
        }
        let from = sheet_pass::location(doc, id);
        if page.is_some() && from.as_ref().map(|l| l.page_index) != page {
            continue;
        }
        for Partner { element: to_id, relationship } in partners.get(id).map(Vec::as_slice).unwrap_or_default() {
            if xrefs.len() + unresolved.len() >= MAX_XREFS {
                break;
            }
            let to = sheet_pass::location(doc, to_id);
            match (&from, &to) {
                (Some(f), Some(t)) => {
                    let tag = doc.model.element(to_id).map(|e| e.tag.as_str()).unwrap_or_default();
                    xrefs.push(json!({
                        "relationship": relationship, "from": loc_json(doc, id, Some(f)), "to": loc_json(doc, to_id, Some(t)),
                        "text": sheet_pass::location_text(doc, f.page_index, t, tag),
                    }));
                }
                _ => unresolved
                    .push(json!({"relationship": relationship, "from": loc_json(doc, id, from.as_ref()), "to": loc_json(doc, to_id, to.as_ref())})),
            }
        }
    }
    Ok(json!({"xrefs": xrefs, "unresolved": unresolved}))
}

fn go(s: &mut Session, v: &Value) -> CmdResult {
    let id = super::model::id_param(v, "id")?.to_string();
    // A location label stands for the view it labels.
    let via_label = s.doc.pages.iter().flat_map(|p| &p.shapes).find(|sh| sh.id == id).and_then(|sh| sh.ext.get(EXT_XREF_FOR).cloned());
    let target = via_label.unwrap_or(id);
    let element =
        element_of(&s.doc, &target).ok_or_else(|| EngineError::Params(format!("`id`: `{}` is not an element, a view or a label", short(&target))))?;
    let index = match v.get("index") {
        None | Some(Value::Null) => 0,
        Some(x) => x.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| EngineError::Params("`index` must be a whole number".into()))?,
    };
    let partners = sheet_pass::partners(&s.doc, cat());
    let list = partners.get(&element).map(Vec::as_slice).unwrap_or_default();
    if list.is_empty() {
        return Err(EngineError::Params("the element has no cross-reference partners".into()));
    }
    let p = list.get(index).ok_or_else(|| EngineError::Params(format!("`index` must be below {}", list.len())))?;
    let at = sheet_pass::location(&s.doc, &p.element).ok_or_else(|| EngineError::Failed("the partner is not drawn on any page".into()))?;
    if s.doc.active_index() != at.page_index || s.doc.active_page.is_empty() {
        s.doc.set_active_index(at.page_index);
    }
    select(s, vec![at.view.clone()]);
    // Going somewhere is not an edit: no undo step, and the edited flag stays as it was.
    let dirty = s.dirty;
    s.mark_saved();
    s.dirty = dirty;
    Ok(json!({"page": at.page, "shape": at.view, "element": p.element}))
}
