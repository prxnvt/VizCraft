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

/// Read the imported page as a model. Containers and groups (subgraphs) are not elements, and a
/// shape whose id cannot be a model id is left out; both are counted in a warning.
fn read_incoming(page: &Page, fallback: &str, warnings: &mut Vec<String>) -> Incoming {
    let mut inc = Incoming { model: Model::new(), shapes: HashMap::new(), connectors: HashMap::new() };
    let (mut frames, mut odd) = (0usize, 0usize);
    let mut seen: HashSet<&str> = HashSet::new();
    for s in page.shapes.iter().take(vizcraft_model::MAX_ELEMENTS) {
        if container::is_container(s) || ar::is_group(s) {
            frames += 1;
            continue;
        }
        if !is_plain_id(&s.id) || s.id.len() > MAX_ID || !seen.insert(s.id.as_str()) {
            odd += 1;
            continue;
        }
        let mut e = Element::new(s.id.clone(), type_of(s, fallback));
        e.name = cut(s.text.trim(), MAX_TEXT);
        e.tag = s.ext.get(TAG_KEY).map(|t| cut(t.trim(), MAX_TAG)).unwrap_or_default();
        for (k, v) in s.ext.iter().filter_map(|(k, v)| k.strip_prefix(PROP_PREFIX).map(|k| (k, v))) {
            if !k.is_empty() && k.len() <= MAX_KEY && e.properties.len() < MAX_PROPERTIES {
                e.properties.insert(k.to_string(), PropValue::Text(cut(v, MAX_TEXT)));
            }
        }
        let mut shape = s.clone();
        shape.parent = None;
        shape.element = None;
        shape.ext.retain(|k, _| k != TAG_KEY && k != TYPE_KEY && !k.starts_with(PROP_PREFIX));
        inc.shapes.insert(e.id.clone(), shape);
        inc.model.elements.push(e);
    }
    if frames > 0 {
        warnings.push(format!("{frames} containers and groups are not model elements and were not merged"));
    }
    if odd > 0 {
        warnings.push(format!("{odd} shapes have no usable id and were not merged"));
    }
    let types: HashMap<&str, &str> = inc.model.elements.iter().map(|e| (e.id.as_str(), e.type_id.as_str())).collect();
    let mut untyped = 0usize;
    for (i, c) in page.connectors.iter().take(vizcraft_model::MAX_RELATIONSHIPS).enumerate() {
        let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
        let (Some(ta), Some(tb)) = (types.get(a), types.get(b)) else { continue };
        let Some((rt, _)) = vizcraft_notation::legal::relationship_for(cat(), ta, tb) else {
            untyped += 1;
            continue;
        };
        // Not a plain id, so it can never collide with an element id in the id map.
        let id = format!("#r{i}");
        let mut r = Relationship::new(id.clone(), rt, a, b);
        r.name = cut(c.text.trim(), MAX_TEXT);
        inc.connectors.insert(id, c.clone());
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
        if let Some(p) = s.doc.pages.get_mut(index) {
            p.shapes.push(shape);
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
