//! Model commands: project-level operations on the semantic model (containment, show in all views, delete from model, page viewpoint).
//!
//! This module also holds the helpers the element and relationship commands share: resolving an
//! element from its id or a shape that draws it, reading property params, creating an element with
//! its tag and pack, renaming an element or relationship together with every view of it, and
//! placing a shape for an element. Every helper leaves model and views in agreement on its own,
//! so a command is right whether or not the post-command sync pass changes anything.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::Document;
use vizcraft_model::{Element, MAX_KEY, MAX_PROPERTIES, MAX_TAG, MAX_TEXT, ModelError, PropValue, Registry, View};
use vizcraft_notation::Catalog;
use vizcraft_notation::schema::PropertySchema;

use super::{MAX_IDS, delete_on_page, opt_str, prune_selection, short};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all model commands: `model.get`, `model.tree`, `model.move`, `model.views`, `model.delete`, `model.setViewpoint`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("model.get", "Model", "Model > Model", get).params(
            r#"{"ids"?: [string]} -> {"schemaVersion", "packs", "elements", "relationships"}; with ids, only those elements and relationships (each must exist)"#,
        ),
        CommandSpec::new("model.tree", "Model Tree", "Model > Model", tree)
            .params(r#"{"root"?: element id} -> {"id", "type", "name", "tag", "children": [same], "truncated"?: true} for root, else {"children": [the root elements]}; nesting stops at 32 levels (truncated: true)"#)
            .when(has_elements),
        CommandSpec::new("model.move", "Move in Model", "Model > Model", move_element)
            .params(r#"{"id": element id, "owner": element id or null} -> {"id", "owner"}; the owner's type must be a container; moving an element inside itself is refused"#)
            .when(has_elements),
        CommandSpec::new("model.views", "Show in All Views", "Model > Model", views)
            .params(r#"{"id": element or relationship id} -> {"views": [{"page", "pageName", "items"}]}; page is the page id, items the shapes or connectors drawing it"#)
            .when(has_items),
        CommandSpec::new("model.delete", "Delete from Model", "Model > Model", delete).params(
            r#"{"ids": [element or relationship ids]} -> {"elements", "relationships", "items"}; removes the elements with every element they own, their relationships, and every shape and connector drawing them on any page"#,
        )
        .when(has_items),
        CommandSpec::new("model.setViewpoint", "Page Viewpoint", "Model > Views", set_viewpoint)
            .params(r#"{"page"?: page id, name or index, "viewpoint": viewpoint id or null} -> {"page", "viewpoint"}; default page: the active one"#),
    ]
}

/// Longest id-like param looked at; model and diagram ids are at most 64 bytes, so anything
/// longer is refused rather than searched for.
pub const MAX_PARAM: usize = 256;

/// Deepest nesting `model.tree` returns: each level is two levels of JSON, and JSON readers
/// commonly stop at 128.
pub const MAX_TREE_DEPTH: usize = 32;

/// The notation catalog every command reads.
pub fn cat() -> &'static Catalog {
    vizcraft_notation::catalog()
}

/// Enabled when the model has at least one element.
pub fn has_elements(s: &Session) -> Option<&'static str> {
    if s.doc.model.elements.is_empty() { Some("the model has no elements") } else { None }
}

/// Enabled when the model has at least one element or relationship.
pub fn has_items(s: &Session) -> Option<&'static str> {
    if s.doc.model.elements.is_empty() && s.doc.model.relationships.is_empty() { Some("the model is empty") } else { None }
}

/// Enabled when the model has at least one relationship.
pub fn has_relationships(s: &Session) -> Option<&'static str> {
    if s.doc.model.relationships.is_empty() { Some("the model has no relationships") } else { None }
}

/// `t` cut to at most `max` bytes at a char boundary.
pub fn cut(t: &str, max: usize) -> String {
    if t.len() <= max {
        return t.to_string();
    }
    let mut end = max;
    while end > 0 && !t.is_char_boundary(end) {
        end -= 1;
    }
    t.get(..end).unwrap_or_default().to_string()
}

/// A required id-like string param of at most [`MAX_PARAM`] bytes.
pub fn id_param<'a>(v: &'a Value, k: &str) -> Result<&'a str, EngineError> {
    let t = p::req_str(v, k)?;
    check_len(k, t)
}

/// `t` when it is at most [`MAX_PARAM`] bytes, else a params error naming `k`.
pub fn check_len<'a>(k: &str, t: &'a str) -> Result<&'a str, EngineError> {
    if t.len() > MAX_PARAM {
        return Err(EngineError::Params(format!("`{k}` is longer than {MAX_PARAM} bytes")));
    }
    Ok(t)
}

/// A model refusal as a command error: a full model is a failure, anything else bad params.
pub fn model_err(e: ModelError) -> EngineError {
    match e {
        ModelError::TooMany(_) | ModelError::Format(_) => EngineError::Failed(e.to_string()),
        _ => EngineError::Params(e.to_string()),
    }
}

/// The element `id` stands for: an element id, or the id of a shape on any page that draws an
/// element.
pub fn element_of(doc: &Document, id: &str) -> Option<String> {
    if doc.model.element(id).is_some() {
        return Some(id.to_string());
    }
    doc.shape(id).and_then(|sh| sh.element.as_deref()).filter(|e| doc.model.element(e).is_some()).map(str::to_string)
}

/// The required element param `k` (an element id or a shape drawing one).
pub fn req_element(s: &Session, v: &Value, k: &str) -> Result<String, EngineError> {
    let id = id_param(v, k)?;
    element_of(&s.doc, id).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not an element or a shape of one", short(id))))
}

/// The required relationship param `k` (a relationship id or a connector drawing one).
pub fn req_relationship(s: &Session, v: &Value, k: &str) -> Result<String, EngineError> {
    let id = id_param(v, k)?;
    if s.doc.model.relationship(id).is_some() {
        return Ok(id.to_string());
    }
    s.doc
        .connector(id)
        .and_then(|c| c.relationship.as_deref())
        .filter(|r| s.doc.model.relationship(r).is_some())
        .map(str::to_string)
        .ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not a relationship or a connector of one", short(id))))
}

/// A property key: 1 to `MAX_KEY` bytes.
pub fn check_key(k: &str) -> Result<(), EngineError> {
    if k.is_empty() || k.len() > MAX_KEY {
        return Err(EngineError::Params(format!("a property key must be 1 to {MAX_KEY} bytes")));
    }
    Ok(())
}

/// A property value from JSON: a boolean, a finite number or text (cut to `MAX_TEXT`).
pub fn prop_value(x: &Value, key: &str) -> Result<PropValue, EngineError> {
    match x {
        Value::Bool(b) => Ok(PropValue::Bool(*b)),
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(PropValue::Number).ok_or_else(|| bad_value(key)),
        Value::String(t) => Ok(PropValue::Text(cut(t, MAX_TEXT))),
        _ => Err(bad_value(key)),
    }
}

fn bad_value(key: &str) -> EngineError {
    EngineError::Params(format!("property `{}` must be true or false, a number or text", short(key)))
}

/// The optional `properties` param: an object of at most `MAX_PROPERTIES` keys, each value a
/// boolean, number or text.
pub fn props_param(v: &Value) -> Result<BTreeMap<String, PropValue>, EngineError> {
    let mut out = BTreeMap::new();
    match v.get("properties") {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) => {
            if m.len() > MAX_PROPERTIES {
                return Err(EngineError::Params(format!("`properties` has more than {MAX_PROPERTIES} keys")));
            }
            for (k, x) in m {
                check_key(k)?;
                out.insert(k.clone(), prop_value(x, k)?);
            }
        }
        Some(_) => return Err(EngineError::Params("`properties` must be an object".into())),
    }
    Ok(out)
}

/// Add each schema default whose key is missing.
pub fn fill_defaults(props: &mut BTreeMap<String, PropValue>, schema: &[PropertySchema]) {
    for p in schema {
        if let Some(d) = &p.default
            && !props.contains_key(&p.key)
            && props.len() < MAX_PROPERTIES
        {
            props.insert(p.key.clone(), d.clone());
        }
    }
}

/// Properties from an untrusted source (a pasted clip) cut to the model's limits: at most
/// `MAX_PROPERTIES`, keys of 1 to `MAX_KEY` bytes, finite numbers, text cut to `MAX_TEXT`.
pub fn clean_props(props: &BTreeMap<String, PropValue>) -> BTreeMap<String, PropValue> {
    props
        .iter()
        .filter(|(k, v)| !k.is_empty() && k.len() <= MAX_KEY && !matches!(v, PropValue::Number(n) if !n.is_finite()))
        .take(MAX_PROPERTIES)
        .map(|(k, v)| {
            let v = match v {
                PropValue::Text(t) => PropValue::Text(cut(t, MAX_TEXT)),
                other => other.clone(),
            };
            (k.clone(), v)
        })
        .collect()
}

/// The `key` and `value` params of a set-property command: `value` null removes the key.
pub fn key_value(v: &Value) -> Result<(String, Option<PropValue>), EngineError> {
    let key = p::req_str(v, "key")?;
    check_key(key)?;
    let value = match v.get("value") {
        None => return Err(EngineError::Params("`value` is required (null removes the property)".into())),
        Some(Value::Null) => None,
        Some(x) => Some(prop_value(x, key)?),
    };
    Ok((key.to_string(), value))
}

/// Set or remove one property in `props`, refusing a new key past `MAX_PROPERTIES`.
pub fn set_prop(props: &mut BTreeMap<String, PropValue>, key: String, value: Option<PropValue>) -> Result<(), EngineError> {
    match value {
        None => {
            props.remove(&key);
        }
        Some(x) => {
            if !props.contains_key(&key) && props.len() >= MAX_PROPERTIES {
                return Err(EngineError::Params(format!("an item holds at most {MAX_PROPERTIES} properties")));
            }
            props.insert(key, x);
        }
    }
    Ok(())
}

/// The next automatic tag for `e` from the identity registry of every loaded pack, or `None`
/// when its type's scope does not number automatically.
pub fn auto_tag(doc: &Document, e: &Element) -> Result<Option<String>, EngineError> {
    let scopes = cat().scopes();
    Registry::new(&doc.model, &scopes).next_tag(e).map(|t| t.map(|t| cut(&t, MAX_TAG))).map_err(model_err)
}

/// Check that `owner` is an element whose type its pack marks as a container.
pub fn check_container(doc: &Document, owner: &str) -> Result<(), EngineError> {
    let e = doc.model.element(owner).ok_or_else(|| EngineError::Params(format!("`owner`: `{}` is not an element", short(owner))))?;
    if cat().element_type(&e.type_id).is_some_and(|(_, t)| t.container) {
        Ok(())
    } else {
        Err(EngineError::Params(format!("`owner`: `{}` is a {} and cannot own elements", short(owner), e.type_id)))
    }
}

/// What [`create_element`] needs.
#[derive(Default)]
pub struct NewElement {
    /// Qualified element type; the caller has checked it.
    pub type_id: String,
    /// Name, cut to `MAX_TEXT`.
    pub name: String,
    /// The tag to store as given; `None` asks the identity registry for the next number.
    pub tag: Option<String>,
    /// Owner element; the caller has checked it.
    pub owner: Option<String>,
    /// Properties; schema defaults are added for missing keys.
    pub properties: BTreeMap<String, PropValue>,
    /// Id chosen by the caller (checked with `new_id`), else the next `e` id.
    pub id: Option<String>,
}

/// Add an element to the model: id, schema defaults, tag (given, or the next auto number when its
/// scope numbers automatically, else empty), and the pack recorded. Returns its id and tag.
pub fn create_element(doc: &mut Document, ne: NewElement) -> Result<(String, String), EngineError> {
    let id = match ne.id {
        Some(id) => id,
        None => doc.next_id("e"),
    };
    let mut e = Element::new(id.clone(), ne.type_id);
    e.name = cut(&ne.name, MAX_TEXT);
    e.owner = ne.owner;
    e.properties = ne.properties;
    if let Some((_, t)) = cat().element_type(&e.type_id) {
        fill_defaults(&mut e.properties, &t.properties);
    }
    e.tag = match ne.tag {
        Some(t) => cut(&t, MAX_TAG),
        None => auto_tag(doc, &e)?.unwrap_or_default(),
    };
    let tag = e.tag.clone();
    let pack = cat().pack_ref(e.pack());
    doc.model.add_element(e).map_err(model_err)?;
    if let Some(p) = pack {
        doc.model.use_pack(p);
    }
    Ok((id, tag))
}

/// Set an element's name and the text of every shape drawing it on any page.
pub fn rename_element(doc: &mut Document, id: &str, name: &str) -> Result<(), EngineError> {
    let name = cut(name, MAX_TEXT);
    let e = doc.model.element_mut(id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(id))))?;
    e.name = name.clone();
    for sh in doc.pages.iter_mut().flat_map(|p| p.shapes.iter_mut()).filter(|sh| sh.element.as_deref() == Some(id)) {
        sh.text = name.clone();
        sh.sanitize();
    }
    Ok(())
}

/// Set a relationship's name and the text of every connector drawing it on any page.
pub fn rename_relationship(doc: &mut Document, id: &str, name: &str) -> Result<(), EngineError> {
    let name = cut(name, MAX_TEXT);
    let r = doc.model.relationship_mut(id).ok_or_else(|| EngineError::Params(format!("`{}` is not a relationship", short(id))))?;
    r.name = name.clone();
    for c in doc.pages.iter_mut().flat_map(|p| p.connectors.iter_mut()).filter(|c| c.relationship.as_deref() == Some(id)) {
        c.text = name.clone();
        c.sanitize();
    }
    Ok(())
}

/// The `page` param as a page index; absent is the active page (the first, as
/// `Document::page` reads it).
pub fn page_param(doc: &Document, v: &Value) -> Result<usize, EngineError> {
    match opt_str(v, "page")? {
        None => Ok(0),
        Some(r) => doc.find_page(check_len("page", r)?).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r)))),
    }
}

/// Add a shape drawing element `id` on page `page` with its top-left at `x, y` (already
/// clamped), from the master its type's glyph names (the page's rendering variant's glyph when
/// it maps the type), its text the element's name. A type that declares ports gives the shape
/// its port positions as connection points, so port `i` is point `i`. Returns the shape id.
pub fn place_shape(doc: &mut Document, id: &str, page: usize, x: f64, y: f64, w: Option<f64>, h: Option<f64>) -> Result<String, EngineError> {
    let e = doc.model.element(id).ok_or_else(|| EngineError::Params(format!("`{}` is not an element", short(id))))?;
    let variant = doc.pages.get(page).and_then(|p| p.view.as_ref()).map(|v| v.variant.as_str()).unwrap_or_default();
    let master = cat()
        .master_for_type_in(&e.type_id, variant)
        .ok_or_else(|| EngineError::Params(format!("type `{}` has no glyph to draw it with", short(&e.type_id))))?;
    let name = e.name.clone();
    let ports = vizcraft_stencils::port_points(&e.type_id);
    let sid = doc.next_id("s");
    let mut shape = vizcraft_stencils::new_shape(master, sid.clone(), x, y, w, h)
        .ok_or_else(|| EngineError::Failed(format!("the stencil `{master}` is missing")))?;
    shape.element = Some(id.to_string());
    shape.text = name;
    if let Some(points) = ports {
        shape.connection_points = points;
    }
    shape.sanitize();
    let page = doc.pages.get_mut(page).ok_or_else(|| EngineError::Params("no such page".into()))?;
    page.shapes.push(shape);
    Ok(sid)
}

/// `{"views": [{"page", "pageName", "items"}]}` entries for model item `id`.
pub fn views_json(doc: &Document, id: &str) -> Value {
    let views: Vec<Value> = vizcraft_diagram::views::views_of(doc, id)
        .into_iter()
        .map(|v| json!({"page": v.page_id, "pageName": doc.pages.get(v.page).map(|p| p.name.as_str()).unwrap_or_default(), "items": v.items}))
        .collect();
    Value::Array(views)
}

fn to_json<T: serde::Serialize>(t: &T) -> CmdResult {
    serde_json::to_value(t).map_err(|e| EngineError::Failed(e.to_string()))
}

/// The whole model, or only the named elements and relationships.
fn get(s: &mut Session, v: &Value) -> CmdResult {
    let m = &s.doc.model;
    let Some(ids) = super::ids_param(v)? else {
        return Ok(
            json!({"schemaVersion": m.schema_version, "packs": to_json(&m.packs)?, "elements": to_json(&m.elements)?, "relationships": to_json(&m.relationships)?}),
        );
    };
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let known: HashSet<&str> = m.elements.iter().map(|e| e.id.as_str()).chain(m.relationships.iter().map(|r| r.id.as_str())).collect();
    if let Some(bad) = ids.iter().find(|id| !known.contains(id.as_str())) {
        return Err(EngineError::Params(format!("`{}` is not an element or relationship", short(bad))));
    }
    let elements: Vec<_> = m.elements.iter().filter(|e| wanted.contains(e.id.as_str())).collect();
    let relationships: Vec<_> = m.relationships.iter().filter(|r| wanted.contains(r.id.as_str())).collect();
    Ok(
        json!({"schemaVersion": m.schema_version, "packs": to_json(&m.packs)?, "elements": to_json(&elements)?, "relationships": to_json(&relationships)?}),
    )
}

/// The containment tree from one element, or from the project root.
fn tree(s: &mut Session, v: &Value) -> CmdResult {
    let m = &s.doc.model;
    // Children per owner, built once, so the walk is linear in the model size.
    let mut kids: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (i, e) in m.elements.iter().enumerate() {
        kids.entry(e.owner.as_deref()).or_default().push(i);
    }
    let node = |i: usize| -> Value {
        // Recursion is bounded by MAX_TREE_DEPTH, and each element has one owner, so even a
        // cyclic model built by hand visits a bounded number of nodes per root.
        fn build(m: &vizcraft_model::Model, kids: &HashMap<Option<&str>, Vec<usize>>, i: usize, depth: usize) -> Value {
            let Some(e) = m.elements.get(i) else { return Value::Null };
            let below = kids.get(&Some(e.id.as_str())).map(Vec::as_slice).unwrap_or_default();
            let mut n = json!({"id": e.id, "type": e.type_id, "name": e.name, "tag": e.tag});
            if depth >= MAX_TREE_DEPTH {
                n["children"] = json!([]);
                if !below.is_empty() {
                    n["truncated"] = json!(true);
                }
            } else {
                n["children"] = Value::Array(below.iter().map(|&c| build(m, kids, c, depth + 1)).collect());
            }
            n
        }
        build(m, &kids, i, 1)
    };
    match opt_str(v, "root")? {
        Some(root) => {
            let root = check_len("root", root)?;
            let i = m
                .elements
                .iter()
                .position(|e| e.id == root)
                .ok_or_else(|| EngineError::Params(format!("`root`: `{}` is not an element", short(root))))?;
            Ok(node(i))
        }
        None => {
            let roots = kids.get(&None).cloned().unwrap_or_default();
            Ok(json!({"children": roots.into_iter().map(node).collect::<Vec<_>>()}))
        }
    }
}

/// Move an element under a container element, or to the root.
fn move_element(s: &mut Session, v: &Value) -> CmdResult {
    let id = req_element(s, v, "id")?;
    let owner = match v.get("owner") {
        None => return Err(EngineError::Params("`owner` (element id or null) is required".into())),
        Some(Value::Null) => None,
        Some(Value::String(o)) => {
            let o = check_len("owner", o)?;
            let o = element_of(&s.doc, o).ok_or_else(|| EngineError::Params(format!("`owner`: `{}` is not an element", short(o))))?;
            check_container(&s.doc, &o)?;
            Some(o)
        }
        Some(_) => return Err(EngineError::Params("`owner` must be an element id or null".into())),
    };
    s.doc.model.set_owner(&id, owner.as_deref()).map_err(model_err)?;
    Ok(json!({"id": id, "owner": owner}))
}

/// Every page and item that draws an element or relationship.
fn views(s: &mut Session, v: &Value) -> CmdResult {
    let id = id_param(v, "id")?;
    if !s.doc.model.contains_id(id) {
        return Err(EngineError::Params(format!("`{}` is not an element or relationship", short(id))));
    }
    Ok(json!({"views": views_json(&s.doc, id)}))
}

/// Delete elements (with what they own) and relationships from the model and every view.
fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let ids = super::ids_param(v)?.ok_or_else(|| EngineError::Params("`ids` (array of element or relationship ids) is required".into()))?;
    if ids.is_empty() {
        return Err(EngineError::Params("`ids` is empty".into()));
    }
    if ids.len() > MAX_IDS {
        return Err(EngineError::Params(format!("`ids` has more than {MAX_IDS} entries")));
    }
    let m = &s.doc.model;
    let known: HashSet<&str> = m.elements.iter().map(|e| e.id.as_str()).chain(m.relationships.iter().map(|r| r.id.as_str())).collect();
    if let Some(bad) = ids.iter().find(|id| !known.contains(id.as_str())) {
        return Err(EngineError::Params(format!("`{}` is not an element or relationship", short(bad))));
    }
    let removed = s.doc.model.remove(&ids);
    let gone: Vec<String> = removed.elements.iter().chain(removed.relationships.iter()).cloned().collect();
    let doomed = vizcraft_diagram::views::items_referencing(&s.doc, &gone);
    let mut items = Vec::new();
    if !doomed.is_empty() {
        for page in &mut s.doc.pages {
            items.extend(delete_on_page(page, &doomed).items);
        }
    }
    prune_selection(s);
    // Not `with_sel`: its `items` (the selection) would overwrite the deleted items.
    Ok(json!({"elements": removed.elements, "relationships": removed.relationships, "items": items}))
}

/// Set or clear the viewpoint a page shows.
fn set_viewpoint(s: &mut Session, v: &Value) -> CmdResult {
    let page = page_param(&s.doc, v)?;
    let viewpoint = match v.get("viewpoint") {
        None => return Err(EngineError::Params("`viewpoint` (viewpoint id or null) is required".into())),
        Some(Value::Null) => None,
        Some(Value::String(t)) => {
            let t = check_len("viewpoint", t)?;
            let known = cat().pack(vizcraft_model::pack_of(t)).and_then(|p| p.viewpoint(t)).is_some();
            if !known {
                return Err(EngineError::Params(format!("unknown viewpoint `{}`", short(t))));
            }
            Some(t.to_string())
        }
        Some(_) => return Err(EngineError::Params("`viewpoint` must be a viewpoint id or null".into())),
    };
    let pg = s.doc.pages.get_mut(page).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let layout = pg.view.as_ref().map(|v| v.layout.clone()).unwrap_or_default();
    // The rendering variant stays with the page across a viewpoint change.
    let variant = pg.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
    pg.view = match &viewpoint {
        Some(vp) => Some(View { viewpoint: vp.clone(), layout, variant }),
        None if layout.is_empty() && variant.is_empty() => None,
        None => Some(View { viewpoint: String::new(), layout, variant }),
    };
    let id = pg.id.clone();
    Ok(json!({"page": id, "viewpoint": viewpoint}))
}

/// Fixtures shared by the model, element and relationship command tests.
#[cfg(test)]
pub(crate) mod fixture {
    use serde_json::{Value, json};
    use vizcraft_diagram::Document;

    use crate::Session;

    /// A session on a blank document.
    pub fn blank() -> Session {
        Session::new(Document::new())
    }

    /// Run a command that must succeed.
    pub fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id} {v}: {e}"))
    }

    /// Run a command that must fail with bad params, leaving the document as it was.
    pub fn refused(s: &mut Session, id: &str, v: Value) -> String {
        let before = s.doc.clone();
        let e = s.run(id, &v).expect_err(&format!("{id} {v} should be refused"));
        assert!(matches!(e, crate::EngineError::Params(_)), "{id} {v}: {e}");
        assert_eq!(s.doc, before, "{id} {v} changed the document");
        e.to_string()
    }

    /// Start -> Do (S1) -> Ok? (S2), drawn on the active page with flows between them, built with
    /// element.create and relationship.create.
    pub fn flow() -> Session {
        let mut s = blank();
        run(&mut s, "element.create", json!({"type": "flowchart.Terminator", "name": "Start", "id": "start", "x": 0, "y": 0}));
        run(&mut s, "element.create", json!({"type": "flowchart.Process", "name": "Do", "id": "do", "x": 0, "y": 150}));
        run(&mut s, "element.create", json!({"type": "flowchart.Decision", "name": "Ok?", "id": "ok", "x": 0, "y": 300}));
        run(&mut s, "relationship.create", json!({"from": "start", "to": "do", "id": "f1"}));
        run(&mut s, "relationship.create", json!({"from": "do", "to": "ok", "id": "f2", "name": "next"}));
        s
    }

    /// The ids of the shapes drawing `element` on every page.
    pub fn shapes_of(s: &Session, element: &str) -> Vec<String> {
        vizcraft_diagram::views::views_of(&s.doc, element).into_iter().flat_map(|v| v.items).collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::End;

    use super::fixture::{blank, flow, refused, run, shapes_of};
    use crate::Session;

    fn ids(v: &serde_json::Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap_or_else(|| x.as_str().unwrap()).to_string()).collect()
    }

    #[test]
    fn get_returns_the_model_or_the_named_items() {
        let mut s = flow();
        let before = s.doc.clone();
        let m = run(&mut s, "model.get", json!({}));
        assert_eq!(m["schemaVersion"], 1);
        assert_eq!(m["packs"], json!([{"id": "flowchart", "version": "1.0.0"}]));
        assert_eq!(ids(&m["elements"]), ["start", "do", "ok"]);
        assert_eq!(ids(&m["relationships"]), ["f1", "f2"]);
        let m = run(&mut s, "model.get", json!({"ids": ["ok", "f2"]}));
        assert_eq!((ids(&m["elements"]), ids(&m["relationships"])), (vec!["ok".to_string()], vec!["f2".to_string()]));
        refused(&mut s, "model.get", json!({"ids": ["ok", "nope"]}));
        refused(&mut s, "model.get", json!({"ids": "ok"}));
        assert_eq!(s.doc, before);
        // An empty model is still readable.
        let mut b = blank();
        assert_eq!(run(&mut b, "model.get", json!({}))["elements"], json!([]));
        assert!(!b.can_undo());
    }

    #[test]
    fn tree_nests_owned_elements_and_stops_deep_chains() {
        let mut s = flow();
        assert!(s.run("model.tree", &json!({})).is_ok());
        assert!(blank().run("model.tree", &json!({})).is_err(), "disabled on an empty model");
        // No built-in type is a container yet, so the nesting is made directly.
        s.doc.model.set_owner("ok", Some("do")).unwrap();
        let t = run(&mut s, "model.tree", json!({}));
        assert_eq!(ids(&t["children"]), ["start", "do"]);
        assert_eq!(t["children"][1]["children"][0]["id"], "ok");
        assert_eq!(t["children"][1]["tag"], "S1");
        let t = run(&mut s, "model.tree", json!({"root": "do"}));
        assert_eq!((t["type"].as_str(), ids(&t["children"])), (Some("flowchart.Process"), vec!["ok".to_string()]));
        refused(&mut s, "model.tree", json!({"root": "nope"}));

        let mut s = blank();
        for i in 0..40 {
            let mut e = vizcraft_model::Element::new(format!("n{i}"), "flowchart.Process");
            e.owner = (i > 0).then(|| format!("n{}", i - 1));
            s.doc.model.add_element(e).unwrap();
        }
        let mut t = run(&mut s, "model.tree", json!({"root": "n0"}));
        let mut depth = 1;
        while let Some(c) = t["children"].get(0).cloned() {
            t = c;
            depth += 1;
        }
        assert_eq!(depth, super::MAX_TREE_DEPTH);
        assert_eq!(t["truncated"], true);
    }

    #[test]
    fn move_needs_a_container_owner() {
        let mut s = flow();
        let e = refused(&mut s, "model.move", json!({"id": "ok", "owner": "do"}));
        assert!(e.contains("cannot own"), "{e}");
        refused(&mut s, "model.move", json!({"id": "ok"}));
        refused(&mut s, "model.move", json!({"id": "nope", "owner": null}));
        refused(&mut s, "model.move", json!({"id": "ok", "owner": "nope"}));
        refused(&mut s, "model.move", json!({"id": "ok", "owner": 3}));
        s.doc.model.set_owner("ok", Some("do")).unwrap();
        let r = run(&mut s, "model.move", json!({"id": "ok", "owner": null}));
        assert_eq!(r, json!({"id": "ok", "owner": null}));
        assert_eq!(s.doc.model.element("ok").unwrap().owner, None);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.element("ok").unwrap().owner.as_deref(), Some("do"));
    }

    #[test]
    fn views_lists_every_page_drawing_an_item() {
        let mut s = flow();
        let p2 = s.doc.add_page("Second");
        let shown = run(&mut s, "element.show", json!({"id": "do", "page": "Second", "x": 5, "y": 5}));
        let r = run(&mut s, "model.views", json!({"id": "do"}));
        let views = r["views"].as_array().unwrap();
        assert_eq!(views.len(), 2);
        assert_eq!((views[0]["page"].as_str(), views[0]["pageName"].as_str()), (Some("p1"), Some("Page-1")));
        assert_eq!(views[1]["page"].as_str(), Some(s.doc.pages[p2].id.as_str()));
        assert_eq!(views[1]["items"], json!([shown["shape"]]));
        let r = run(&mut s, "model.views", json!({"id": "f2"}));
        assert_eq!(r["views"][0]["items"].as_array().unwrap().len(), 1);
        refused(&mut s, "model.views", json!({"id": "s1"}));
        refused(&mut s, "model.views", json!({}));
    }

    #[test]
    fn delete_cascades_to_owned_elements_relationships_and_views() {
        let mut s = flow();
        s.doc.add_page("Second");
        run(&mut s, "element.show", json!({"id": "ok", "page": "Second", "x": 0, "y": 0}));
        s.doc.model.set_owner("ok", Some("do")).unwrap();
        let before = s.doc.clone();
        let r = run(&mut s, "model.delete", json!({"ids": ["do"]}));
        assert_eq!(r["elements"], json!(["do", "ok"]));
        assert_eq!(r["relationships"], json!(["f1", "f2"]));
        // Both shapes of `ok` (two pages), the shape of `do` and both connectors.
        assert_eq!(r["items"].as_array().unwrap().len(), 5);
        assert_eq!(s.doc.model.elements.len(), 1);
        assert!(s.doc.model.relationships.is_empty());
        assert!(s.doc.pages.iter().all(|p| p.connectors.is_empty()));
        assert_eq!(s.doc.pages[0].shapes.len(), 1);
        assert!(s.doc.pages[1].shapes.is_empty());
        assert!(s.sel.items.iter().all(|i| s.doc.pages[0].contains_id(i)));
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before);
        // A relationship alone takes only its connector.
        let r = run(&mut s, "model.delete", json!({"ids": ["f1"]}));
        assert_eq!((r["elements"].as_array().unwrap().len(), r["relationships"].clone()), (0, json!(["f1"])));
        assert_eq!(s.doc.pages[0].shapes.len(), 3);
        refused(&mut s, "model.delete", json!({"ids": ["nope"]}));
        refused(&mut s, "model.delete", json!({"ids": []}));
        refused(&mut s, "model.delete", json!({}));
    }

    #[test]
    fn set_viewpoint_sets_and_clears_a_page_view() {
        let mut s = blank();
        let r = run(&mut s, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
        assert_eq!(r, json!({"page": "p1", "viewpoint": "flowchart.Flowchart"}));
        assert_eq!(s.doc.pages[0].view.as_ref().map(|v| v.viewpoint.as_str()), Some("flowchart.Flowchart"));
        run(&mut s, "model.setViewpoint", json!({"page": "p1", "viewpoint": null}));
        assert_eq!(s.doc.pages[0].view, None);
        run(&mut s, "edit.undo", json!({}));
        assert!(s.doc.pages[0].view.is_some());
        refused(&mut s, "model.setViewpoint", json!({"viewpoint": "flowchart.Nope"}));
        refused(&mut s, "model.setViewpoint", json!({"viewpoint": 4}));
        refused(&mut s, "model.setViewpoint", json!({}));
        refused(&mut s, "model.setViewpoint", json!({"page": "nowhere", "viewpoint": null}));
    }

    #[test]
    fn shape_add_with_flowchart_masters_builds_elements() {
        let mut s = blank();
        let a = run(&mut s, "shape.add", json!({"stencil": "flowchart.terminator", "x": 0, "y": 0, "text": "Begin"}));
        let b = run(&mut s, "shape.add", json!({"stencil": "flowchart.process", "x": 0, "y": 100, "text": "Work"}));
        let c = run(&mut s, "shape.add", json!({"stencil": "flowchart.decision", "x": 0, "y": 200}));
        let free = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 300}));
        assert!(free.get("element").is_none());
        assert_eq!(s.doc.shape(free["id"].as_str().unwrap()).unwrap().element, None);
        run(&mut s, "connector.add", json!({"from": {"shape": a["id"]}, "to": {"shape": b["id"]}}));
        let m = run(&mut s, "model.get", json!({}));
        let els = m["elements"].as_array().unwrap();
        assert_eq!(els.len(), 3);
        assert_eq!((els[0]["type"].as_str(), els[0]["name"].as_str()), (Some("flowchart.Terminator"), Some("Begin")));
        assert_eq!((els[1]["tag"].as_str(), els[2]["tag"].as_str()), (Some("S1"), Some("S2")));
        assert_eq!(els[1]["properties"]["automated"], false, "schema default filled");
        assert_eq!(m["packs"][0]["id"], "flowchart");
        for (r, e) in [(&a, &els[0]), (&b, &els[1]), (&c, &els[2])] {
            assert_eq!(r["element"], e["id"]);
            assert_eq!(s.doc.shape(r["id"].as_str().unwrap()).unwrap().element.as_deref(), e["id"].as_str());
        }
        // One undo takes back the shape and its element together.
        run(&mut s, "edit.undo", json!({}));
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.elements.len(), 3);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.elements.len(), 2);
        assert!(s.doc.shape(c["id"].as_str().unwrap()).is_none());
    }

    #[test]
    fn set_text_on_a_bound_item_renames_the_model_item_and_every_view() {
        let mut s = flow();
        s.doc.add_page("Second");
        let other = run(&mut s, "element.show", json!({"id": "do", "page": "Second", "x": 0, "y": 0}))["shape"].as_str().unwrap().to_string();
        let first = shapes_of(&s, "do")[0].clone();
        run(&mut s, "shape.setText", json!({"id": first, "text": "Work hard"}));
        assert_eq!(s.doc.model.element("do").unwrap().name, "Work hard");
        assert_eq!(s.doc.shape(&other).unwrap().text, "Work hard");
        assert_eq!(s.doc.shape(&first).unwrap().text, "Work hard");
        let c = s.doc.pages[0].connectors.iter().find(|c| c.relationship.as_deref() == Some("f2")).unwrap().id.clone();
        run(&mut s, "connector.setText", json!({"id": c, "text": "then"}));
        assert_eq!(s.doc.model.relationship("f2").unwrap().name, "then");
        assert_eq!(s.doc.connector(&c).unwrap().text, "then");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.relationship("f2").unwrap().name, "next");
        // Free items keep the old behaviour.
        let free = run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}))["id"].as_str().unwrap().to_string();
        run(&mut s, "shape.setText", json!({"id": free, "text": "plain"}));
        assert_eq!(s.doc.shape(&free).unwrap().text, "plain");
    }

    #[test]
    fn deleting_views_keeps_elements_but_not_undrawn_relationships() {
        let mut s = flow();
        let ok_shape = shapes_of(&s, "ok")[0].clone();
        run(&mut s, "shape.delete", json!({"ids": [ok_shape]}));
        assert!(s.doc.model.element("ok").is_some(), "the element stays in the model");
        // The flow into `ok` was detached, not deleted, so its relationship stays.
        assert!(s.doc.model.relationship("f2").is_some());
        let c1 = s.doc.pages[0].connectors.iter().find(|c| c.relationship.as_deref() == Some("f1")).unwrap().id.clone();
        run(&mut s, "edit.select", json!({"ids": [c1]}));
        run(&mut s, "edit.delete", json!({}));
        assert!(s.doc.model.relationship("f1").is_none(), "its last view went");
        assert_eq!(s.doc.model.elements.len(), 3);
    }

    #[test]
    fn save_and_reload_keep_model_and_views() {
        let mut s = flow();
        run(&mut s, "model.setViewpoint", json!({"viewpoint": "flowchart.Flowchart"}));
        let bytes = crate::io::save_bytes("x.vizcraft", &s.doc).unwrap();
        let back = crate::io::load_bytes("x.vizcraft", &bytes).unwrap();
        assert_eq!(back.model, s.doc.model);
        assert_eq!(back.pages, s.doc.pages);
        assert!(back.pages[0].shapes.iter().all(|sh| sh.element.is_some()));
        assert!(back.pages[0].connectors.iter().all(|c| c.relationship.is_some()));
    }

    #[test]
    fn paste_copies_elements_and_relationships() {
        let mut s = flow();
        let mut all: Vec<String> = s.doc.pages[0].shapes.iter().map(|x| x.id.clone()).collect();
        all.extend(s.doc.pages[0].connectors.iter().map(|c| c.id.clone()));
        run(&mut s, "edit.select", json!({"ids": all}));
        let copied = run(&mut s, "edit.copy", json!({}));
        let clip: serde_json::Value = serde_json::from_str(copied["text"].as_str().unwrap()).unwrap();
        assert_eq!(clip["elements"].as_array().unwrap().len(), 3);
        assert_eq!(clip["relationships"].as_array().unwrap().len(), 2);
        let pasted = run(&mut s, "edit.paste", json!({}));
        assert_eq!(pasted["pasted"].as_array().unwrap().len(), 5);
        let m = &s.doc.model;
        assert_eq!((m.elements.len(), m.relationships.len()), (6, 4));
        let new_do = m.elements.iter().find(|e| e.name == "Do" && e.id != "do").unwrap();
        let new_ok = m.elements.iter().find(|e| e.name == "Ok?" && e.id != "ok").unwrap();
        assert_eq!((new_do.tag.as_str(), new_ok.tag.as_str()), ("S3", "S4"), "tags re-issued");
        assert_eq!(new_do.properties, m.element("do").unwrap().properties);
        let new_flow = m.relationships.iter().find(|r| r.source == new_do.id).unwrap();
        assert_eq!((new_flow.target.as_str(), new_flow.name.as_str()), (new_ok.id.as_str(), "next"));
        // Every pasted item draws a copy, never an original.
        for id in pasted["pasted"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()) {
            if let Some(sh) = s.doc.shape(id) {
                assert!(!["start", "do", "ok"].contains(&sh.element.as_deref().unwrap()));
            } else {
                let c = s.doc.connector(id).unwrap();
                assert!(!["f1", "f2"].contains(&c.relationship.as_deref().unwrap()));
                for end in [&c.start, &c.end] {
                    assert!(matches!(end, End::Glued { .. }));
                }
            }
        }
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.elements.len(), 3);

        // A clip whose items name model items it does not carry pastes as free drawing.
        let text = json!({"vizcraft": "clip", "shapes": [{"id": "a", "kind": "rectangle", "w": 10, "h": 10, "element": "ghost"}, {"id": "b", "kind": "rectangle", "w": 10, "h": 10, "element": "do"}],
            "connectors": [{"id": "k", "start": {"shape": "a"}, "end": {"shape": "b"}, "relationship": "f1"}]})
        .to_string();
        let pasted = run(&mut s, "edit.paste", json!({"text": text}));
        for id in pasted["pasted"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()) {
            assert!(s.doc.shape(id).is_none_or(|sh| sh.element.is_none()));
            assert!(s.doc.connector(id).is_none_or(|c| c.relationship.is_none()));
        }
        assert_eq!(s.doc.model.elements.len(), 3);
    }

    /// Junk params on every model-backed command, run against a document that has a model, so
    /// the bodies run (the registry-wide test uses the sample, whose model is empty).
    #[test]
    fn hostile_params_on_a_model_never_panic() {
        let junk = [
            json!(null),
            json!({}),
            json!({"id": 5, "ids": "x", "type": [], "from": {}, "to": 1, "name": 3, "key": "", "value": [], "owner": 7}),
            json!({"id": "do", "ids": ["do", 1], "type": "flowchart.Process", "x": 1e308, "y": -1e308, "w": -1, "h": 1e308, "page": 9}),
            json!({"id": "do", "key": "k".repeat(500), "value": f64::MAX, "tag": "t".repeat(300), "name": "n".repeat(70_000)}),
            json!({"id": "x".repeat(100_000), "from": "x".repeat(100_000), "to": "do", "root": "y".repeat(100_000), "viewpoint": "v".repeat(100_000)}),
            json!({"id": "f1", "from": "ok", "to": "start", "type": "flowchart.Flow", "strict": "yes", "properties": {"a": {}}}),
            json!({"id": "f1", "from": "start", "to": "start", "properties": {"": 1}, "owner": null, "viewpoint": null}),
            json!({"type": "flowchart.Database", "properties": {"k": [1]}, "x": 0}),
            json!({"type": "flowchart.Database", "id": "do", "tag": 4}),
            json!({"ids": ["start", "do", "ok", "f1", "f2"], "root": "do", "owner": "do", "id": "ok"}),
        ];
        let commands: Vec<&str> = crate::cmd::registry()
            .all()
            .iter()
            .map(|c| c.id)
            .filter(|id| {
                ["model.", "element.", "relationship.", "shape.add", "shape.setText", "connector.setText", "edit.paste"]
                    .iter()
                    .any(|p| id.starts_with(p))
            })
            .collect();
        assert!(commands.len() >= 20);
        for id in commands {
            for j in &junk {
                for select_all in [false, true] {
                    let mut s: Session = flow();
                    if select_all {
                        run(&mut s, "edit.selectAll", json!({}));
                    }
                    let _ = s.run(id, j);
                    for p in &s.doc.pages {
                        for sh in &p.shapes {
                            assert!(sh.x.is_finite() && sh.y.is_finite() && sh.w.is_finite() && sh.h.is_finite(), "{id}: {j}");
                            assert!(sh.element.as_deref().is_none_or(|e| s.doc.model.element(e).is_some()), "{id}: {j}: dangling element");
                        }
                        for c in &p.connectors {
                            assert!(
                                c.relationship.as_deref().is_none_or(|r| s.doc.model.relationship(r).is_some()),
                                "{id}: {j}: dangling relationship"
                            );
                        }
                    }
                    for i in &s.sel.items {
                        assert!(s.doc.pages[0].contains_id(i), "{id}: stale selection {i}");
                    }
                    let mut m = s.doc.model.clone();
                    m.sanitize();
                    assert_eq!(m, s.doc.model, "{id}: {j}: the model left something sanitize would change");
                }
            }
        }
    }
}
