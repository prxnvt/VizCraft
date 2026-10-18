//! Hyperlink and screen tip commands: `link.add`, `link.set`, `link.remove`, `link.follow` and
//! `shape.setScreenTip`. A link is a kind and a target on a shape (`Shape.links`); following a
//! page, shape or element link moves the view and selection inside the document, while a URL or
//! file link hands the target to the front end as an `openExternal` request on
//! `session.ui_requests`, because the engine never opens anything itself.

use serde_json::{Value, json};
use vizcraft_diagram::{Hyperlink, LinkKind, Shape};

use super::{MAX_IDS, ids_param, opt_str, select, short, targets};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Insert > Links";

/// Return the link commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("link.add", "Link", LOCATION, add)
            .key("Mod+K")
            .params(r#"{"id"?: shape (default: the primary selection), "kind"?: "url"|"file"|"page"|"shape"|"element", "target"?: string, "label"?: string} -> {id, index, link}; a page target is a page id, name or index (stored as the id); without `target` the Link dialog opens in the app"#)
            .when(super::has_shapes),
        CommandSpec::new("link.set", "Edit Link", LOCATION, set)
            .params(r#"{"id": shape, "index": n, "kind"?: string, "target"?: string, "label"?: string} -> {id, index, link}; only the given fields change"#)
            .when(has_links),
        CommandSpec::new("link.remove", "Remove Link", LOCATION, remove)
            .params(r#"{"id": shape, "index": n} -> {id, removed}"#)
            .when(has_links),
        CommandSpec::new("link.follow", "Follow Link", LOCATION, follow)
            .params(r#"{"id": shape, "index"?: n (default 0)} -> {kind, target, page?, from?, selected?} for a page, shape or element link (the page changes and the target is selected, undo does not undo it); {request: "openExternal", target} for a url or file link, also pushed to the app"#)
            .when(has_links),
        CommandSpec::new("shape.setScreenTip", "Screen Tip", LOCATION, set_screen_tip)
            .params(r#"{"ids"?: [shape], "text": string} -> {ids}; the tooltip shown when the pointer rests on the shape; empty clears it"#)
            .when(super::has_shapes),
    ]
}

fn has_links(s: &Session) -> Option<&'static str> {
    if s.doc.pages.iter().any(|p| p.shapes.iter().any(|x| !x.links.is_empty())) { None } else { Some("add a link first") }
}

/// The shape a link command acts on: the `id` param, else the primary selection; on the active
/// page and unlocked.
fn the_shape(s: &Session, v: &Value) -> Result<String, EngineError> {
    let page = s.doc.page().ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let id = match opt_str(v, "id")? {
        Some(i) => i.to_string(),
        None => s
            .sel
            .primary
            .clone()
            .filter(|i| page.shape(i).is_some())
            .or_else(|| s.sel.items.iter().find(|i| page.shape(i).is_some()).cloned())
            .ok_or_else(|| EngineError::Params("`id` (shape) is required, or select a shape first".into()))?,
    };
    if page.shape(&id).is_none() {
        return Err(EngineError::Params(format!("`{}` is not a shape on this page", short(&id))));
    }
    Ok(id)
}

fn link_index(s: &Session, id: &str, v: &Value, default: Option<usize>) -> Result<usize, EngineError> {
    let i = match v.get("index") {
        None | Some(Value::Null) => default.ok_or_else(|| EngineError::Params("`index` (number) is required".into()))?,
        Some(x) => x
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| EngineError::Params("`index` must be a whole number".into()))?,
    };
    let len = s.doc.shape(id).map_or(0, |sh| sh.links.len());
    if i >= len {
        return Err(EngineError::Params(format!("`{}` has {len} link(s); there is no link {i}", short(id))));
    }
    Ok(i)
}

fn kind_param(v: &Value) -> Result<Option<LinkKind>, EngineError> {
    match opt_str(v, "kind")? {
        None => Ok(None),
        Some(k) => LinkKind::from_name(k)
            .map(Some)
            .ok_or_else(|| EngineError::Params(format!("`kind` must be url, file, page, shape or element, not `{}`", short(k)))),
    }
}

/// A web address the app may open: only http, https and mailto, so a link can never launch a
/// program through a custom scheme.
fn web_address(t: &str) -> bool {
    let lower = t.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|p| lower.starts_with(p)) && !t.chars().any(char::is_control)
}

/// Check `target` for `kind` and return the form that is stored: a page target becomes the page
/// id, so renaming the page keeps the link working.
fn check_target(s: &Session, kind: LinkKind, target: &str) -> Result<String, EngineError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(EngineError::Params("`target` must not be empty".into()));
    }
    if target.len() > Hyperlink::MAX_TARGET {
        return Err(EngineError::Params(format!("`target` is longer than {} bytes", Hyperlink::MAX_TARGET)));
    }
    match kind {
        LinkKind::Url => {
            if !web_address(target) {
                return Err(EngineError::Params("a url link starts with http://, https:// or mailto:".into()));
            }
            Ok(target.to_string())
        }
        LinkKind::File => {
            if target.chars().any(char::is_control) {
                return Err(EngineError::Params("a file path must not hold control characters".into()));
            }
            Ok(target.to_string())
        }
        LinkKind::Page => {
            let i = s.doc.find_page(target).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(target))))?;
            s.doc.pages.get(i).map(|p| p.id.clone()).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(target))))
        }
        LinkKind::Shape => {
            if s.doc.shape(target).is_none() {
                return Err(EngineError::Params(format!("no shape `{}`", short(target))));
            }
            Ok(target.to_string())
        }
        LinkKind::Element => {
            if s.doc.model.element(target).is_none() {
                return Err(EngineError::Params(format!("no element `{}`", short(target))));
            }
            Ok(target.to_string())
        }
    }
}

fn label_param(v: &Value) -> Result<Option<String>, EngineError> {
    Ok(opt_str(v, "label")?.map(|l| super::model::cut(l.trim(), Hyperlink::MAX_LABEL)))
}

fn link_json(l: &Hyperlink) -> Value {
    json!({"kind": l.kind.name(), "target": l.target, "label": l.label})
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    super::layer::ensure_unlocked(s, [&id])?;
    let Some(target) = opt_str(v, "target")? else {
        s.ui_requests.push(json!({"open": "links"}));
        return Ok(json!({"dialog": "links", "id": id}));
    };
    let kind = kind_param(v)?.unwrap_or(LinkKind::Url);
    let target = check_target(s, kind, target)?;
    let label = label_param(v)?.unwrap_or_default();
    let sh = s.doc.shape_mut(&id).ok_or_else(|| EngineError::Failed("the shape vanished".into()))?;
    if sh.links.len() >= Hyperlink::MAX_PER_SHAPE {
        return Err(EngineError::Params(format!("a shape has at most {} links", Hyperlink::MAX_PER_SHAPE)));
    }
    let link = Hyperlink { kind, target, label };
    let out = link_json(&link);
    sh.links.push(link);
    Ok(json!({"id": id, "index": sh.links.len().saturating_sub(1), "link": out}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    super::layer::ensure_unlocked(s, [&id])?;
    let index = link_index(s, &id, v, None)?;
    let old = s.doc.shape(&id).and_then(|sh| sh.links.get(index)).cloned().ok_or_else(|| EngineError::Failed("the link vanished".into()))?;
    let kind = kind_param(v)?.unwrap_or(old.kind);
    // A changed kind with no new target keeps the old text, which must then fit the new kind.
    let target = check_target(s, kind, opt_str(v, "target")?.unwrap_or(&old.target))?;
    let label = label_param(v)?.unwrap_or(old.label);
    let link = Hyperlink { kind, target, label };
    let out = link_json(&link);
    let slot = s.doc.shape_mut(&id).and_then(|sh| sh.links.get_mut(index)).ok_or_else(|| EngineError::Failed("the link vanished".into()))?;
    *slot = link;
    Ok(json!({"id": id, "index": index, "link": out}))
}

fn remove(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    super::layer::ensure_unlocked(s, [&id])?;
    let index = link_index(s, &id, v, None)?;
    let sh = s.doc.shape_mut(&id).ok_or_else(|| EngineError::Failed("the shape vanished".into()))?;
    let gone = sh.links.remove(index);
    Ok(json!({"id": id, "removed": link_json(&gone)}))
}

/// Make page `index` the active page the way `page.setActive` does: showing another page is not
/// an edit, so it is no undo step and leaves the unsaved flag as it was.
pub(super) fn show_page(s: &mut Session, index: usize) {
    if s.doc.active_index() != index || s.doc.active_page.is_empty() {
        s.doc.set_active_index(index);
        keep_history(s);
    }
}

/// End the running command without a second undo step, leaving the unsaved flag as it is.
fn keep_history(s: &mut Session) {
    let dirty = s.dirty;
    s.mark_saved();
    s.dirty = dirty;
}

/// The page that holds shape `id`.
fn page_of_shape(s: &Session, id: &str) -> Option<usize> {
    s.doc.pages.iter().position(|p| p.shape(id).is_some())
}

/// The first shape drawing element `element`, on the active page when it has one, else on the
/// first other page that does: `(page index, shape id)`.
fn shape_of_element(s: &Session, element: &str) -> Option<(usize, String)> {
    let drawn = |sh: &&Shape| sh.element.as_deref() == Some(element);
    let active = s.doc.active_index();
    let order = std::iter::once(active).chain((0..s.doc.pages.len()).filter(|i| *i != active));
    for i in order {
        if let Some(sh) = s.doc.pages.get(i).and_then(|p| p.shapes.iter().find(drawn)) {
            return Some((i, sh.id.clone()));
        }
    }
    None
}

fn follow(s: &mut Session, v: &Value) -> CmdResult {
    let id = the_shape(s, v)?;
    let index = link_index(s, &id, v, Some(0))?;
    let link = s.doc.shape(&id).and_then(|sh| sh.links.get(index)).cloned().ok_or_else(|| EngineError::Failed("the link vanished".into()))?;
    follow_link(s, &link)
}

/// Follow one link: move inside the document, or push the request for the front end.
pub fn follow_link(s: &mut Session, link: &Hyperlink) -> CmdResult {
    let from = s.doc.page().map(|p| p.id.clone());
    let kind = link.kind.name();
    match link.kind {
        LinkKind::Url | LinkKind::File => {
            if link.kind == LinkKind::Url && !web_address(&link.target) {
                return Err(EngineError::Params("this url link does not start with http://, https:// or mailto:".into()));
            }
            let req = json!({"request": "openExternal", "target": link.target});
            s.ui_requests.push(req.clone());
            Ok(json!({"kind": kind, "target": link.target, "request": "openExternal", "from": from}))
        }
        LinkKind::Page => {
            let i = s.doc.find_page(&link.target).ok_or_else(|| EngineError::Params(format!("the page `{}` no longer exists", short(&link.target))))?;
            show_page(s, i);
            select(s, Vec::new());
            Ok(json!({"kind": kind, "target": link.target, "page": s.doc.page().map(|p| p.id.clone()), "from": from}))
        }
        LinkKind::Shape => {
            let i = page_of_shape(s, &link.target).ok_or_else(|| EngineError::Params(format!("the shape `{}` no longer exists", short(&link.target))))?;
            show_page(s, i);
            select(s, vec![link.target.clone()]);
            Ok(with_selected(s, kind, &link.target, from))
        }
        LinkKind::Element => {
            if s.doc.model.element(&link.target).is_none() {
                return Err(EngineError::Params(format!("the element `{}` no longer exists", short(&link.target))));
            }
            match shape_of_element(s, &link.target) {
                Some((i, shape)) => {
                    show_page(s, i);
                    select(s, vec![shape]);
                    Ok(with_selected(s, kind, &link.target, from))
                }
                None => {
                    // Not drawn anywhere: draw it on the active page at the middle of the view.
                    let [x, y] = s.view.pan;
                    let r = s.run("element.show", &json!({"id": link.target, "x": f64::from(x), "y": f64::from(y)}))?;
                    // `element.show` is already its own undo step; this command adds no second one.
                    keep_history(s);
                    let shape = r.get("shape").and_then(Value::as_str).unwrap_or_default().to_string();
                    Ok(with_selected(s, kind, &shape, from))
                }
            }
        }
    }
}

fn with_selected(s: &Session, kind: &str, target: &str, from: Option<String>) -> Value {
    json!({"kind": kind, "target": target, "page": s.doc.page().map(|p| p.id.clone()), "from": from, "selected": s.sel.items})
}

fn set_screen_tip(s: &mut Session, v: &Value) -> CmdResult {
    let text = opt_str(v, "text")?.ok_or_else(|| EngineError::Params("`text` (string) is required".into()))?;
    if text.len() > Shape::MAX_SCREEN_TIP {
        return Err(EngineError::Params(format!("`text` is longer than {} bytes", Shape::MAX_SCREEN_TIP)));
    }
    if ids_param(v)?.is_some_and(|a| a.len() > MAX_IDS) {
        return Err(EngineError::Params("too many ids".into()));
    }
    let ids = targets(s, v, super::Kinds::Shapes)?;
    super::layer::ensure_unlocked(s, &ids)?;
    for id in &ids {
        if let Some(sh) = s.doc.shape_mut(id) {
            sh.screen_tip = text.to_string();
        }
    }
    Ok(json!({"ids": ids}))
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
    fn links_add_set_remove_and_round_trip() {
        let mut s = session();
        let a = id_of(&s, "Start");
        s.run("link.add", &json!({"id": a, "kind": "url", "target": "https://example.com", "label": "Site"})).unwrap();
        s.run("link.add", &json!({"id": a, "kind": "file", "target": "notes/readme.txt"})).unwrap();
        let r = s.run("link.set", &json!({"id": a, "index": 1, "label": "Readme"})).unwrap();
        assert_eq!(r["link"]["label"], "Readme");
        // A page link is stored by page id even when given by name.
        let name = s.doc.page().unwrap().name.clone();
        s.run("link.add", &json!({"id": a, "kind": "page", "target": name})).unwrap();
        let page_id = s.doc.page().unwrap().id.clone();
        assert_eq!(s.doc.shape(&a).unwrap().links[2].target, page_id);
        let doc: vizcraft_diagram::Document = serde_json::from_value(serde_json::to_value(&s.doc).unwrap()).unwrap();
        assert_eq!(doc.shape(&a).unwrap().links, s.doc.shape(&a).unwrap().links);
        s.run("link.remove", &json!({"id": a, "index": 0})).unwrap();
        assert_eq!(s.doc.shape(&a).unwrap().links.len(), 2);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.shape(&a).unwrap().links.len(), 3);
    }

    #[test]
    fn bad_links_are_refused() {
        let mut s = session();
        let a = id_of(&s, "Start");
        for bad in [
            json!({"id": a, "kind": "url", "target": "javascript:alert(1)"}),
            json!({"id": a, "kind": "url", "target": "ftp://x"}),
            json!({"id": a, "kind": "page", "target": "nowhere"}),
            json!({"id": a, "kind": "shape", "target": "zz9"}),
            json!({"id": a, "kind": "element", "target": "zz9"}),
            json!({"id": a, "kind": "bogus", "target": "x"}),
            json!({"id": a, "kind": "file", "target": ""}),
            json!({"id": "zz9", "kind": "file", "target": "x"}),
        ] {
            assert!(s.run("link.add", &bad).is_err(), "{bad}");
        }
        assert!(s.doc.shape(&a).unwrap().links.is_empty());
        s.run("link.add", &json!({"id": a, "target": "https://example.com"})).unwrap();
        assert!(s.run("link.set", &json!({"id": a, "index": 5, "label": "x"})).is_err());
        assert!(s.run("link.follow", &json!({"id": a, "index": 5})).is_err());
    }

    #[test]
    fn add_without_a_target_opens_the_dialog() {
        let mut s = session();
        let a = id_of(&s, "Start");
        s.run("edit.select", &json!({"ids": [a]})).unwrap();
        s.run("link.add", &json!({})).unwrap();
        assert_eq!(s.ui_requests.pop(), Some(json!({"open": "links"})));
    }

    #[test]
    fn follow_switches_page_and_selects_without_an_undo_step() {
        let mut s = session();
        let (a, b) = (id_of(&s, "Start"), id_of(&s, "End"));
        s.run("page.add", &json!({"name": "Detail"})).unwrap();
        let detail = s.doc.page().unwrap().id.clone();
        s.run("page.setActive", &json!({"page": 0})).unwrap();
        s.run("link.add", &json!({"id": a, "kind": "page", "target": "Detail"})).unwrap();
        s.run("link.add", &json!({"id": a, "kind": "shape", "target": b})).unwrap();
        let before = (s.undo_label().map(str::to_string), s.transactions().len());
        let r = s.run("link.follow", &json!({"id": a})).unwrap();
        assert_eq!(s.doc.page().unwrap().id, detail);
        assert_eq!(r["page"], json!(detail));
        assert!(s.sel.items.is_empty());
        assert_eq!((s.undo_label().map(str::to_string), s.transactions().len()), before, "following a link is not an edit");
        // Back on the first page, the shape link selects its target.
        s.run("page.setActive", &json!({"page": 0})).unwrap();
        s.run("link.follow", &json!({"id": a, "index": 1})).unwrap();
        assert_eq!(s.sel.items, vec![b]);
    }

    #[test]
    fn follow_on_a_url_pushes_the_open_request() {
        let mut s = session();
        let a = id_of(&s, "Start");
        s.run("link.add", &json!({"id": a, "target": "https://example.com/x"})).unwrap();
        s.ui_requests.clear();
        let r = s.run("link.follow", &json!({"id": a})).unwrap();
        assert_eq!(r["request"], "openExternal");
        assert_eq!(s.ui_requests, vec![json!({"request": "openExternal", "target": "https://example.com/x"})]);
    }

    #[test]
    fn follow_an_element_with_no_shape_draws_it() {
        let mut s = session();
        let a = id_of(&s, "Start");
        let e = s.run("element.create", &json!({"type": "flowchart.Data", "name": "Log"})).unwrap()["id"].as_str().unwrap().to_string();
        s.run("link.add", &json!({"id": a, "kind": "element", "target": e})).unwrap();
        let r = s.run("link.follow", &json!({"id": a})).unwrap();
        let shape = r["selected"][0].as_str().unwrap().to_string();
        assert_eq!(s.doc.shape(&shape).unwrap().element.as_deref(), Some(e.as_str()));
    }

    #[test]
    fn screen_tips_set_and_clear() {
        let mut s = session();
        let a = id_of(&s, "Start");
        s.run("shape.setScreenTip", &json!({"ids": [a], "text": "The entry point"})).unwrap();
        assert_eq!(s.doc.shape(&a).unwrap().screen_tip, "The entry point");
        s.run("shape.setScreenTip", &json!({"ids": [a], "text": ""})).unwrap();
        assert!(s.doc.shape(&a).unwrap().screen_tip.is_empty());
        assert!(s.run("shape.setScreenTip", &json!({"ids": [a], "text": "x".repeat(5000)})).is_err());
    }
}
