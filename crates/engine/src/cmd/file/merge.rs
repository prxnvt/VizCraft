//! The re-import contract: `file.import {merge: true}` and `file.reimport`.
//!
//! A merging import turns the imported page into model elements (the shape's type from its
//! `import.type` data, its master's element type, or the `type` param) and relationships (a
//! connector glued at both ends, typed by the pack's connection table), then matches them to the
//! model with [`vizcraft_model::sync::diff`]: by the record of the last import from the same
//! source, by id, by tag, by name within the type. Matched elements take the new name and
//! properties and keep their shapes where they are; new elements are drawn with the imported
//! shape's look and placed by `layout_incremental` beside the matched ones; connectors follow the
//! relationships. An element the source no longer has stays, flagged by an `import.removed`
//! issue whose fix runs `model.delete`. The import is recorded in `Document.imports`, and it is
//! all one command, so one undo step.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Algorithm, Direction, Options};
use vizcraft_diagram::import_record::{self, ImportRecord};
use vizcraft_diagram::{Connector, End, Page, Route, Shape, arrange as ar, container, is_plain_id};
use vizcraft_formats::Imported;
use vizcraft_formats::csv::{PROP_PREFIX, TAG_KEY, TYPE_KEY};
use vizcraft_formats::er::CHILDREN_KEY;
use vizcraft_model::sync::{self, Changes};
use vizcraft_model::{Element, MAX_ID, MAX_KEY, MAX_PROPERTIES, MAX_TAG, MAX_TEXT, Model, PropValue, Relationship};

use super::{IMPORT_GAP, ImportOpts};
use crate::cmd::model::{auto_tag, cat, cut, fill_defaults, model_err, rename_element};
use crate::cmd::{delete_on_page, prune_selection, select, short};
use crate::{CmdResult, EngineError, Session};

/// The element type a shape becomes when nothing else names one.
pub(crate) const DEFAULT_TYPE: &str = "flowchart.Process";

/// The imported page read as a model, with the shapes and connectors that drew it.
struct Incoming {
    model: Model,
    /// Incoming element id to the imported shape drawing it.
    shapes: HashMap<String, Shape>,
    /// Incoming relationship id to the imported connector drawing it.
    connectors: HashMap<String, Connector>,
}

/// The element type for an imported shape: its `import.type` when the catalog knows it, else its
/// master's type, else `fallback`.
fn type_of(s: &Shape, fallback: &str) -> String {
    s.ext
        .get(TYPE_KEY)
        .filter(|t| cat().element_type(t).is_some())
        .cloned()
        .or_else(|| cat().type_for_master(&s.stencil).map(str::to_string))
        .unwrap_or_else(|| fallback.to_string())
}

/// An imported property value typed by the schema of `type_id` (`true` for a bool property,
/// a number for a numeric one); text otherwise, or when it does not read.
fn typed_value(type_id: &str, key: &str, text: &str, relationship: bool) -> PropValue {
    use vizcraft_notation::PropKind;
    let kind = if relationship {
        cat().relationship_type(type_id).and_then(|(_, t)| t.property(key)).map(|p| p.kind)
    } else {
        cat().element_type(type_id).and_then(|(_, t)| t.property(key)).map(|p| p.kind)
    };
    let t = text.trim();
    match kind {
        Some(PropKind::Bool) if t == "true" || t == "false" => PropValue::Bool(t == "true"),
        Some(PropKind::Number | PropKind::Integer) if t.parse::<f64>().is_ok_and(f64::is_finite) => {
            PropValue::Number(t.parse::<f64>().unwrap_or(0.0))
        }
        _ => PropValue::Text(cut(text, MAX_TEXT)),
    }
}

/// The relationship type for an imported connector: its `import.type` when the catalog knows it,
/// else the one its connector kind names, else the connection table's choice for the ends.
fn relationship_type_of(c: &Connector, source_type: &str, target_type: &str) -> Option<String> {
    if let Some(t) = c.ext.get(TYPE_KEY).filter(|t| cat().relationship_type(t).is_some()) {
        return Some(t.clone());
    }
    let from_kind = (!c.kind.is_empty())
        .then(|| cat().packs().iter().find_map(|p| p.connector_type(&c.kind)).and_then(|k| k.relationship_type.clone()))
        .flatten()
        .filter(|t| cat().relationship_type(t).is_some());
    from_kind.or_else(|| vizcraft_notation::legal::relationship_for(cat(), source_type, target_type).map(|(t, _)| t))
}

/// The elements an imported shape's `import.children` says its element `owner` owns (an ER
/// table's columns): `[{"id", "type", "name", "props": {key: text}}]`. An entry of a type the
/// catalog does not know, with no id, or with an id already used is left out and counted in a
/// warning.
fn children_of(owner: &str, json: &str, seen: &mut HashSet<String>, warnings: &mut Vec<String>) -> Vec<Element> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else {
        warnings.push(format!("the columns of {} do not read", short(owner)));
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for v in list.iter().take(vizcraft_model::MAX_ELEMENTS) {
        let (Some(id), Some(ty)) = (v.get("id").and_then(Value::as_str), v.get("type").and_then(Value::as_str)) else {
            skipped += 1;
            continue;
        };
        let id = cut(id, MAX_ID);
        if id.is_empty() || cat().element_type(ty).is_none() || !seen.insert(id.clone()) {
            skipped += 1;
            continue;
        }
        let mut e = Element::new(id, ty);
        e.name = cut(v.get("name").and_then(Value::as_str).unwrap_or_default().trim(), MAX_TEXT);
        e.owner = Some(owner.to_string());
        if let Some(Value::Object(props)) = v.get("props") {
            for (k, val) in props {
                let Some(text) = val.as_str() else { continue };
                if !k.is_empty() && k.len() <= MAX_KEY && e.properties.len() < MAX_PROPERTIES {
                    let value = typed_value(&e.type_id, k, text, false);
                    e.properties.insert(k.clone(), value);
                }
            }
        }
        out.push(e);
    }
    if skipped > 0 {
        warnings.push(format!("{skipped} owned elements of {} were not merged (unknown type or repeated id)", short(owner)));
    }
    out
}

/// Read the imported page as a model. Containers and groups (subgraphs) are not elements unless
/// they carry `import.type` (a package, a composite state); members take the nearest such shape
/// above them as their owner. A shape whose id cannot be a model id is left out; both are
/// counted in a warning.
fn read_incoming(page: &Page, fallback: &str, warnings: &mut Vec<String>) -> Incoming {
    let mut inc = Incoming { model: Model::new(), shapes: HashMap::new(), connectors: HashMap::new() };
    let (mut frames, mut odd) = (0usize, 0usize);
    let mut seen: HashSet<&str> = HashSet::new();
    let mut seen_children: HashSet<String> = HashSet::new();
    for s in page.shapes.iter().take(vizcraft_model::MAX_ELEMENTS) {
        // Activation bars, destruction marks and fragments are drawn by the sequence settle pass.
        if s.sequence.as_ref().is_some_and(|p| !p.is_element_view()) {
            continue;
        }
        let typed = s.ext.get(TYPE_KEY).is_some_and(|t| cat().element_type(t).is_some());
        if (container::is_container(s) || ar::is_group(s)) && !typed {
            frames += 1;
            continue;
        }
        if !is_plain_id(&s.id) || s.id.len() > MAX_ID || !seen.insert(s.id.as_str()) {
            odd += 1;
            continue;
        }
        let mut e = Element::new(s.id.clone(), type_of(s, fallback));
        // A `«keyword»` line above the name is drawing, not part of the name.
        e.name = cut(vizcraft_notation::bind::strip_keyword(&s.text).trim(), MAX_TEXT);
        e.tag = s.ext.get(TAG_KEY).map(|t| cut(t.trim(), MAX_TAG)).unwrap_or_default();
        for (k, v) in s.ext.iter().filter_map(|(k, v)| k.strip_prefix(PROP_PREFIX).map(|k| (k, v))) {
            if !k.is_empty() && k.len() <= MAX_KEY && e.properties.len() < MAX_PROPERTIES {
                let value = typed_value(&e.type_id, k, v, false);
                e.properties.insert(k.to_string(), value);
            }
        }
        let children = s.ext.get(CHILDREN_KEY).map(|json| children_of(&e.id, json, &mut seen_children, warnings)).unwrap_or_default();
        let mut shape = s.clone();
        shape.parent = None;
        shape.element = None;
        shape.ext.retain(|k, _| k != TAG_KEY && k != TYPE_KEY && k != CHILDREN_KEY && !k.starts_with(PROP_PREFIX));
        inc.shapes.insert(e.id.clone(), shape);
        inc.model.elements.push(e);
        for c in children {
            if inc.model.elements.len() < vizcraft_model::MAX_ELEMENTS {
                inc.model.elements.push(c);
            }
        }
    }
    if frames > 0 {
        warnings.push(format!("{frames} containers and groups are not model elements and were not merged"));
    }
    if odd > 0 {
        warnings.push(format!("{odd} shapes have no usable id and were not merged"));
    }
    // Owners: the nearest ancestor shape that became an element.
    let parents: HashMap<&str, Option<&str>> = page.shapes.iter().map(|s| (s.id.as_str(), s.parent.as_deref())).collect();
    let ids: HashSet<String> = inc.model.elements.iter().map(|e| e.id.clone()).collect();
    for e in &mut inc.model.elements {
        let mut at = parents.get(e.id.as_str()).copied().flatten();
        for _ in 0..vizcraft_model::MAX_DEPTH {
            let Some(p) = at else { break };
            if p == e.id {
                break;
            }
            if ids.contains(p) {
                e.owner = Some(p.to_string());
                break;
            }
            at = parents.get(p).copied().flatten();
        }
    }
    let types: HashMap<&str, &str> = inc.model.elements.iter().map(|e| (e.id.as_str(), e.type_id.as_str())).collect();
    let mut untyped = 0usize;
    for (i, c) in page.connectors.iter().take(vizcraft_model::MAX_RELATIONSHIPS).enumerate() {
        let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
        let (Some(ta), Some(tb)) = (types.get(a), types.get(b)) else { continue };
        let Some(rt) = relationship_type_of(c, ta, tb) else {
            untyped += 1;
            continue;
        };
        // Not a plain id, so it can never collide with an element id in the id map.
        let id = format!("#r{i}");
        let mut r = Relationship::new(id.clone(), rt, a, b);
        r.name = cut(vizcraft_notation::bind::strip_keyword(&c.text).trim(), MAX_TEXT);
        for (k, v) in c.ext.iter().filter_map(|(k, v)| k.strip_prefix(PROP_PREFIX).map(|k| (k, v))) {
            if !k.is_empty() && k.len() <= MAX_KEY && r.properties.len() < MAX_PROPERTIES {
                let value = typed_value(&r.type_id, k, v, true);
                r.properties.insert(k.to_string(), value);
            }
        }
        let mut drawn = c.clone();
        drawn.ext.retain(|k, _| k != TYPE_KEY && !k.starts_with(PROP_PREFIX));
        inc.connectors.insert(id, drawn);
        inc.model.relationships.push(r);
    }
    if untyped > 0 {
        warnings.push(format!("{untyped} connectors join types with no relationship between them and were not merged"));
    }
    inc
}

/// The flow direction the imported arrangement shows: where targets mostly sit from sources.
fn direction_of(page: &Page) -> Direction {
    let (mut dx, mut dy) = (0.0f64, 0.0f64);
    for c in &page.connectors {
        let (Some(a), Some(b)) = (c.start.shape().and_then(|s| page.shape(s)), c.end.shape().and_then(|s| page.shape(s))) else { continue };
        let (ca, cb) = (a.bounds().center(), b.bounds().center());
        if (cb.x - ca.x).is_finite() && (cb.y - ca.y).is_finite() {
            dx += cb.x - ca.x;
            dy += cb.y - ca.y;
        }
    }
    if dx.abs() > dy.abs() {
        if dx >= 0.0 { Direction::LeftRight } else { Direction::RightLeft }
    } else if dy < 0.0 {
        Direction::BottomTop
    } else {
        Direction::TopBottom
    }
}

/// The first shape on `page` drawing element `id`.
fn view_on(page: &Page, id: &str) -> Option<String> {
    page.shapes.iter().find(|s| s.element.as_deref() == Some(id)).map(|s| s.id.clone())
}

/// Ids for the added elements and relationships: the incoming id when it is plain and free, else
/// a fresh one.
fn new_ids(s: &mut Session, ch: &Changes) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut given: HashSet<String> = HashSet::new();
    for id in &ch.added {
        let mine = if is_plain_id(id) && !s.doc.contains_id(id) && !given.contains(id) { id.clone() } else { fresh(s, "e", &given) };
        given.insert(mine.clone());
        out.insert(id.clone(), mine);
    }
    for id in &ch.relationships.added {
        let mine = fresh(s, "r", &given);
        given.insert(mine.clone());
        out.insert(id.clone(), mine);
    }
    out
}

/// A fresh id with `prefix`, free in the document and not among `given`.
fn fresh(s: &mut Session, prefix: &str, given: &HashSet<String>) -> String {
    loop {
        let id = s.doc.next_id(prefix);
        if !given.contains(&id) {
            return id;
        }
    }
}

/// Merge `imported` (from `source`, in `format`) into the model and the target page; see the
/// module doc. `direction_given` says whether the caller chose a direction for the placement.
pub(crate) fn merge(
    s: &mut Session,
    imported: Imported,
    opts: &ImportOpts,
    source: &str,
    format: &str,
    fallback: &str,
    direction_given: bool,
) -> CmdResult {
    if opts.replace {
        return Err(EngineError::Params("`replace` cannot be combined with `merge`".into()));
    }
    let Imported { pages, mut warnings } = imported;
    let mut pages = pages.into_iter();
    let Some(src) = pages.next() else { return Err(EngineError::Failed("the source holds no page to import".into())) };
    if pages.next().is_some() {
        warnings.push("only the first page of the source is merged; the others were left out".into());
    }
    let record = import_record::find(&s.doc.imports, source).and_then(|i| s.doc.imports.get(i)).cloned().unwrap_or_default();
    let index = match &opts.page {
        Some(r) => match s.doc.find_page(r) {
            Some(i) => i,
            None => s.doc.add_page(r),
        },
        None => s.doc.find_page(&record.page).filter(|_| !record.page.is_empty()).unwrap_or_else(|| s.doc.active_index()),
    };
    let inc = read_incoming(&src, fallback, &mut warnings);
    let ch = sync::diff(&s.doc.model, &inc.model, &record.elements, &record.relationships);
    let ids = new_ids(s, &ch);
    let applied = sync::apply(&mut s.doc.model, &inc.model, &ch, &ids).map_err(model_err)?;
    // Matched relationships take the incoming properties (multiplicities, guards).
    for r in &inc.model.relationships {
        let Some(rid) = applied.relationships.get(&r.id) else { continue };
        if let Some(cur) = s.doc.model.relationship_mut(rid) {
            for (k, v) in &r.properties {
                if cur.properties.len() < MAX_PROPERTIES || cur.properties.contains_key(k) {
                    cur.properties.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // New elements get their type's defaults, a tag where the scope numbers them, and the pack.
    let added: Vec<String> = ch.added.iter().filter_map(|id| applied.elements.get(id).cloned()).collect();
    for id in &added {
        let Some(e) = s.doc.model.element(id).cloned() else { continue };
        let tag = if e.tag.is_empty() { auto_tag(&s.doc, &e)?.unwrap_or_default() } else { e.tag.clone() };
        let pack = cat().pack_ref(e.pack());
        if let Some(x) = s.doc.model.element_mut(id) {
            if let Some((_, t)) = cat().element_type(&x.type_id) {
                fill_defaults(&mut x.properties, &t.properties);
            }
            x.tag = tag;
        }
        if let Some(p) = pack {
            s.doc.model.use_pack(p);
        }
    }
    // Matched elements show their new names everywhere.
    for id in ch.updated.iter() {
        let name = s.doc.model.element(id).map(|e| e.name.clone()).unwrap_or_default();
        rename_element(&mut s.doc, id, &name)?;
    }
    // Connectors of relationships the source no longer has go with them.
    if !applied.removed_relationships.is_empty() {
        let doomed: Vec<String> = s
            .doc
            .pages
            .iter()
            .flat_map(|p| p.connectors.iter())
            .filter(|c| c.relationship.as_ref().is_some_and(|r| applied.removed_relationships.contains(r)))
            .map(|c| c.id.clone())
            .collect();
        for pg in &mut s.doc.pages {
            delete_on_page(pg, &doomed);
        }
    }

    // Shapes: matched elements keep theirs; the others are drawn from the imported shapes.
    let page_has = |s: &Session| s.doc.pages.get(index).map(|p| p.shapes.iter().any(|x| x.parent.is_none())).unwrap_or(false);
    let had_shapes = page_has(s);
    let mut kept_shapes: Vec<String> = Vec::new();
    let mut new_shapes: Vec<String> = Vec::new();
    let mut drawn_as: HashMap<&str, String> = HashMap::new();
    for e in &inc.model.elements {
        let Some(mid) = applied.elements.get(&e.id) else { continue };
        if let Some(sh) = s.doc.pages.get(index).and_then(|p| view_on(p, mid)) {
            kept_shapes.push(sh);
            continue;
        }
        let Some(mut shape) = inc.shapes.get(&e.id).cloned() else { continue };
        shape.id = s.doc.next_id("s");
        shape.element = Some(mid.clone());
        shape.text = s.doc.model.element(mid).map(|x| x.name.clone()).unwrap_or_default();
        shape.sanitize();
        new_shapes.push(shape.id.clone());
        drawn_as.insert(e.id.as_str(), shape.id.clone());
        if let Some(p) = s.doc.pages.get_mut(index) {
            p.shapes.push(shape);
        }
    }
    // New members stay inside their new owner's container shape, as the source drew them.
    for e in &inc.model.elements {
        let (Some(own), Some(o)) = (drawn_as.get(e.id.as_str()), e.owner.as_deref().and_then(|o| drawn_as.get(o))) else { continue };
        if let Some(p) = s.doc.pages.get_mut(index)
            && p.shape(o).is_some_and(container::is_container)
            && let Some(sh) = p.shapes.iter_mut().find(|x| &x.id == own)
        {
            sh.parent = Some(o.clone());
        }
    }
    let mut new_connectors: Vec<String> = Vec::new();
    for r in &inc.model.relationships {
        let Some(rid) = applied.relationships.get(&r.id) else { continue };
        let Some(rel) = s.doc.model.relationship(rid).cloned() else { continue };
        let Some(p) = s.doc.pages.get(index) else { break };
        if p.connectors.iter().any(|c| c.relationship.as_deref() == Some(rid.as_str())) {
            continue;
        }
        let (Some(a), Some(b)) = (view_on(p, &rel.source), view_on(p, &rel.target)) else { continue };
        let mut c = inc.connectors.get(&r.id).cloned().unwrap_or_default();
        c.id = s.doc.next_id("c");
        c.start = End::dynamic(a);
        c.end = End::dynamic(b);
        c.waypoints.clear();
        c.text = rel.name.clone();
        c.relationship = Some(rid.clone());
        c.sanitize();
        new_connectors.push(c.id.clone());
        if let Some(p) = s.doc.pages.get_mut(index) {
            p.connectors.push(c);
        }
    }

    // Placement: beside the matched shapes when there are some, else the source's arrangement
    // set beside whatever the page already holds.
    let direction = if direction_given { opts.direction } else { direction_of(&src) };
    if let Some(p) = s.doc.pages.get_mut(index)
        && !new_shapes.is_empty()
    {
        if kept_shapes.is_empty() {
            let others: Vec<String> = p.shapes.iter().filter(|x| x.parent.is_none() && !new_shapes.contains(&x.id)).map(|x| x.id.clone()).collect();
            if had_shapes && let (Some(h), Some(n)) = (ar::bounds_of(p, &others), ar::bounds_of(p, &new_shapes)) {
                crate::cmd::arrange::move_shapes(p, &new_shapes, h.x1 + IMPORT_GAP - n.x0, h.y0 - n.y0);
            }
            if let Some(algorithm) = opts.layout {
                let o = Options { algorithm, direction, ..Options::default() };
                match autolayout::layout_items(p, Some(&new_shapes), &o) {
                    Ok(notes) => warnings.extend(notes),
                    Err(e) => warnings.push(format!("the layout was not applied: {e}")),
                }
            }
        } else {
            let o = Options { algorithm: opts.layout.unwrap_or(Algorithm::Layered), direction, ..Options::default() };
            match autolayout::layout_incremental(p, &new_shapes, &o) {
                Ok(notes) => warnings.extend(notes),
                Err(e) => warnings.push(format!("the new shapes were not placed: {e}")),
            }
        }
    }
    if opts.avoid
        && let Some(p) = s.doc.pages.get_mut(index)
        && !new_connectors.is_empty()
    {
        for id in &new_connectors {
            if let Some(c) = p.connector_mut(id) {
                c.route = Route::Orthogonal;
                c.avoid = true;
            }
        }
        vizcraft_diagram::autoroute::reroute(p, Some(&new_connectors));
    }

    // The record: what each source id became, the relationships made, the elements flagged.
    let mut elements = applied.elements.clone();
    for (k, v) in &record.elements {
        if ch.removed.contains(v) && !elements.contains_key(k) {
            elements.insert(k.clone(), v.clone());
        }
    }
    let page_id = s.doc.pages.get(index).map(|p| p.id.clone()).unwrap_or_default();
    import_record::store(
        &mut s.doc.imports,
        ImportRecord {
            source: source.to_string(),
            format: format.to_string(),
            time: vizcraft_diagram::text_fields::system_now_secs().unwrap_or(0),
            page: page_id.clone(),
            elements,
            relationships: applied.relationships.values().cloned().collect(),
            removed: ch.removed.clone(),
        },
    );

    if index == s.doc.active_index() {
        let mut shown = kept_shapes.clone();
        shown.extend(new_shapes.iter().cloned());
        select(s, shown);
    }
    prune_selection(s);
    let issues: Vec<Value> = import_record::removed_issues(&s.doc)
        .into_iter()
        .filter(|i| i.subjects.iter().any(|x| ch.removed.contains(x)))
        .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
        .collect();
    let (a, u, r, k) = ch.counts();
    let from = if source.is_empty() { String::new() } else { format!(" from {}", short(source)) };
    s.status = format!("Merged{from}: {a} added, {u} updated, {r} removed, {k} kept");
    let updated_ids = ch.updated.clone();
    let kept_ids = ch.kept.clone();
    Ok(json!({
        "page": page_id,
        "source": source,
        "added": a,
        "updated": u,
        "removed": r,
        "kept": k,
        "ids": applied.elements,
        "addedIds": added,
        "updatedIds": updated_ids,
        "removedIds": ch.removed,
        "keptIds": kept_ids,
        "shapes": new_shapes,
        "relationships": {
            "added": ch.relationships.added.iter().filter(|id| applied.relationships.contains_key(*id)).count(),
            "kept": ch.relationships.kept.len(),
            "removed": applied.removed_relationships.len(),
        },
        "issues": issues,
        "warnings": warnings,
    }))
}

/// The `type` param: the element type for shapes nothing else types; defaults to
/// [`DEFAULT_TYPE`], which the Flowchart pack defines.
pub(crate) fn fallback_type(v: &Value) -> Result<String, EngineError> {
    match super::opt_str(v, "type")? {
        None => Ok(DEFAULT_TYPE.to_string()),
        Some(t) if cat().element_type(t).is_some() => Ok(t.to_string()),
        Some(t) => Err(EngineError::Params(format!("`type`: no loaded pack defines `{}`", short(t)))),
    }
}

/// The `source` param: at most [`import_record::MAX_SOURCE`] bytes with no control characters.
pub(crate) fn source_param(v: &Value) -> Result<Option<String>, EngineError> {
    match super::opt_str(v, "source")? {
        None => Ok(None),
        Some(t) if t.len() <= import_record::MAX_SOURCE && !t.chars().any(char::is_control) => Ok(Some(t.to_string())),
        Some(_) => {
            Err(EngineError::Params(format!("`source` must be text of at most {} bytes with no control characters", import_record::MAX_SOURCE)))
        }
    }
}

/// `file.reimport {source?, ..}`: read the recorded source file again and merge it.
pub(crate) fn reimport(s: &mut Session, v: &Value) -> CmdResult {
    let wanted = source_param(v)?;
    let rec = match &wanted {
        Some(src) => import_record::find(&s.doc.imports, src)
            .and_then(|i| s.doc.imports.get(i))
            .ok_or_else(|| EngineError::Params(format!("nothing was imported from `{}`", short(src))))?,
        None => s
            .doc
            .imports
            .iter()
            .rev()
            .find(|r| !r.source.is_empty())
            .ok_or_else(|| EngineError::Failed("no import names a source file to read again".into()))?,
    }
    .clone();
    if rec.source.is_empty() {
        return Err(EngineError::Failed("that import came from text, not a file; import the new text with `merge`".into()));
    }
    let mut opts = super::import_opts(v)?;
    if opts.page.is_none() && !rec.page.is_empty() && s.doc.find_page(&rec.page).is_some() {
        opts.page = Some(rec.page.clone());
    }
    let bytes = crate::io::read_limited(&rec.source)?;
    let wanted_format = (!rec.format.is_empty()).then_some(rec.format.as_str());
    let format = crate::io::import_format(Some(&rec.source), wanted_format, &bytes)?;
    let imported = crate::io::import_bytes(format, &bytes)?;
    let fallback = fallback_type(v)?;
    merge(s, imported, &opts, &rec.source, format, &fallback, v.get("direction").is_some())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{Document, Session};

    const FIRST: &str = "flowchart TD\nA[Start] --> B[Check]\nB --> C[Old step]\n";
    const SECOND: &str = "flowchart TD\nA[Start] --> B[Check stock]\nB --> D[New step]\n";

    fn merge(s: &mut Session, text: &str) -> Value {
        s.run("file.import", &json!({"text": text, "merge": true, "source": "flow.mmd"})).unwrap()
    }

    fn shape_of(s: &Session, element: &str) -> vizcraft_diagram::Shape {
        s.doc.page().unwrap().shapes.iter().find(|x| x.element.as_deref() == Some(element)).cloned().unwrap()
    }

    fn temp_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vizcraft-merge-test-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_changed_mermaid_flowchart_merges_in_one_undo_step() {
        let mut s = Session::new(Document::new());
        let r = merge(&mut s, FIRST);
        assert_eq!((r["added"].as_u64(), r["kept"].as_u64()), (Some(3), Some(0)), "{r}");
        for id in ["A", "B", "C"] {
            assert!(s.doc.model.element(id).is_some(), "node {id} keeps its id as the element id");
        }
        assert_eq!(s.doc.model.element("B").unwrap().type_id, "flowchart.Process");
        assert_eq!(s.doc.model.relationships.len(), 2);
        let (a0, b0) = (shape_of(&s, "A"), shape_of(&s, "B"));
        // Move A by hand: the re-import must keep it there.
        s.run("shape.move", &json!({"ids": [a0.id], "dx": 40, "dy": 0})).unwrap();
        let a0 = shape_of(&s, "A");
        let before = s.doc.clone();

        let r = merge(&mut s, SECOND);
        assert_eq!(
            (r["added"].as_u64(), r["updated"].as_u64(), r["removed"].as_u64(), r["kept"].as_u64()),
            (Some(1), Some(1), Some(1), Some(1)),
            "{r}"
        );
        assert_eq!(r["ids"]["B"], "B");
        let (a1, b1) = (shape_of(&s, "A"), shape_of(&s, "B"));
        assert_eq!((a1.id.as_str(), a1.x, a1.y), (a0.id.as_str(), a0.x, a0.y), "a kept node keeps its shape and place");
        assert_eq!((b1.x, b1.y), (b0.x, b0.y));
        assert_eq!(s.doc.model.element("B").unwrap().name, "Check stock");
        assert_eq!(b1.text, "Check stock");
        // The new node sits below the node it follows, overlapping nothing.
        let d = shape_of(&s, "D");
        assert!(d.y >= b1.y + b1.h, "D at {} is below B at {}", d.y, b1.y);
        for x in s.doc.page().unwrap().shapes.iter().filter(|x| x.id != d.id) {
            assert!(!d.bounds().intersects(&x.bounds()), "D overlaps {}", x.id);
        }
        // The flow to the removed node went with it; the element stays, flagged.
        assert!(s.doc.model.element("C").is_some());
        let flows: Vec<(&str, &str)> = s.doc.model.relationships.iter().map(|r| (r.source.as_str(), r.target.as_str())).collect();
        assert!(flows.contains(&("B", "D")) && !flows.contains(&("B", "C")), "{flows:?}");
        let flagged: Vec<_> = s.issues.iter().filter(|i| i.rule == "import.removed").collect();
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].subjects, vec!["C".to_string()]);
        assert_eq!(r["issues"].as_array().unwrap().len(), 1);

        // One undo step brings the whole first state back.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc.model, before.model);
        assert_eq!(s.doc.pages, before.pages);
        s.run("edit.redo", &json!({})).unwrap();
        assert!(s.doc.model.element("D").is_some());

        // The fix deletes the element and its shape.
        let fix = s.issues.iter().find(|i| i.rule == "import.removed").unwrap().fixes[0].clone();
        s.run(&fix.command, &fix.params).unwrap();
        assert!(s.doc.model.element("C").is_none());
        assert!(s.doc.page().unwrap().shapes.iter().all(|x| x.element.as_deref() != Some("C")));
        assert!(s.issues.iter().all(|i| i.rule != "import.removed"));
        // Importing the same text again changes nothing.
        let r = merge(&mut s, SECOND);
        assert_eq!(
            (r["added"].as_u64(), r["updated"].as_u64(), r["removed"].as_u64(), r["kept"].as_u64()),
            (Some(0), Some(0), Some(0), Some(3)),
            "{r}"
        );
    }

    #[test]
    fn csv_id_and_tag_columns_round_trip_ids() {
        let csv = "# label: %name%\nid,name,tag,cost\npump1,Feed pump,P-101,5\nvalve2,Inlet valve,V-202,\n";
        let mut s = Session::new(Document::new());
        let r = s.run("file.import", &json!({"text": csv, "merge": true, "source": "plant.csv"})).unwrap();
        assert_eq!(r["added"], 2);
        let pump = s.doc.model.element("pump1").unwrap();
        assert_eq!((pump.name.as_str(), pump.tag.as_str()), ("Feed pump", "P-101"));
        assert_eq!(pump.properties["cost"], vizcraft_model::PropValue::Text("5".into()));
        assert_eq!(s.doc.model.element("valve2").unwrap().tag, "V-202");
        // Saved and opened again, a re-import finds the same elements by the recorded ids.
        let dir = temp_dir("csv");
        let path = dir.join("plant.vizcraft");
        s.run("file.save", &json!({"path": path.to_string_lossy()})).unwrap();
        let mut t = Session::new(Document::new());
        t.run("file.open", &json!({"path": path.to_string_lossy()})).unwrap();
        let renamed = csv.replace("Feed pump", "Feed pump A");
        let r = t.run("file.import", &json!({"text": renamed, "merge": true, "source": "plant.csv"})).unwrap();
        assert_eq!((r["added"].as_u64(), r["updated"].as_u64(), r["kept"].as_u64()), (Some(0), Some(1), Some(1)), "{r}");
        assert_eq!(r["ids"]["pump1"], "pump1");
        assert_eq!(t.doc.model.element("pump1").unwrap().name, "Feed pump A");
        // A tag matches when the id changed.
        let retagged = csv.replace("pump1", "pump9");
        let r = t.run("file.import", &json!({"text": retagged, "merge": true, "source": "other.csv"})).unwrap();
        assert_eq!(r["ids"]["pump9"], "pump1", "{r}");
    }

    #[test]
    fn reimport_rereads_the_recorded_path() {
        let dir = temp_dir("reimport");
        let path = dir.join("flow.mmd");
        std::fs::write(&path, FIRST).unwrap();
        let mut s = Session::new(Document::new());
        assert!(s.run("file.reimport", &json!({})).is_err(), "disabled with no import");
        s.run("file.import", &json!({"path": path.to_string_lossy(), "merge": true})).unwrap();
        assert_eq!(s.doc.imports[0].source, path.to_string_lossy());
        assert_eq!(s.doc.imports[0].format, "mermaid");
        std::fs::write(&path, SECOND).unwrap();
        let r = s.run("file.reimport", &json!({})).unwrap();
        assert_eq!((r["added"].as_u64(), r["removed"].as_u64()), (Some(1), Some(1)), "{r}");
        assert_eq!(s.doc.model.element("B").unwrap().name, "Check stock");
        let r = s.run("file.reimport", &json!({"source": path.to_string_lossy()})).unwrap();
        assert_eq!(r["added"], 0);
        assert!(s.run("file.reimport", &json!({"source": "never imported"})).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(s.run("file.reimport", &json!({})).is_err(), "a missing file is an error");
    }

    #[test]
    fn hostile_merge_params_and_junk_paths_never_panic() {
        let junk = [
            json!({"merge": true}),
            json!({"merge": "yes", "text": "flowchart TD\nA-->B"}),
            json!({"merge": true, "text": "flowchart TD\nA-->B", "replace": true}),
            json!({"merge": true, "text": "flowchart TD\nA-->B", "type": "nope.Type"}),
            json!({"merge": true, "text": "flowchart TD\nA-->B", "source": "\u{0}"}),
            json!({"merge": true, "text": "flowchart TD\nA-->B", "source": "s".repeat(10_000)}),
            json!({"merge": true, "text": "flowchart TD\nA-->B", "source": 5, "page": "new page"}),
            json!({"merge": true, "path": "/nonexistent/\u{0}/x.mmd"}),
            json!({"merge": true, "path": ""}),
            json!({"merge": true, "path": {}}),
            json!({"merge": true, "text": "# label: %x%\nid,x\n,\n\u{0},\u{ffff}\n"}),
            json!({"merge": true, "text": "flowchart TD\n%% tag: A\n%% tag:\nA[ #] --> A"}),
        ];
        for v in &junk {
            let mut s = Session::new(crate::sample::sample_document());
            let _ = s.run("file.import", v);
            let _ = s.run("file.reimport", v);
            s.doc.imports.push(vizcraft_diagram::import_record::ImportRecord {
                source: "/nonexistent/dir/\u{7f}".into(),
                format: "junk".into(),
                ..Default::default()
            });
            let _ = s.run("file.reimport", v);
            assert!(s.run("file.reimport", &json!({})).is_err());
        }
    }
}
