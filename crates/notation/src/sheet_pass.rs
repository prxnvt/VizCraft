//! The drawing-sheet part of the sync pass (`crate::bind::sync`, after step 8), and the sheet
//! checks of the rules engine.
//!
//! After every change, in the same undo step:
//!
//! 1. A list compartment whose declaration has a table query (`CompartmentDecl::query`) is filled
//!    from the model: one tab-separated row per listed element, sorted, with the query's columns.
//! 2. A balloon (`sheet.Balloon`) takes the tag of the part its `sheet.CallsOut` leader points at
//!    as its name, and its views show it.
//! 3. Every view of an element with cross-reference partners (through a relationship whose type
//!    has `kind: crossReference`, or by sharing a tag in a paired identifier scope) keeps one
//!    generated text label below it: a text box whose `parent` is the view (so it moves with it and
//!    is selected on its own) marked with `ext["vizcraft.xref.for"]` = the view's id. Its text is
//!    the partners' locations, one per line, in the document's `xrefFormat` (`/{sheet}.{zone}`) or,
//!    for a partner on the same page, `xrefSameSheet` (`{zone}`). A label the user moved stays
//!    where it was put; a deleted label comes back unless the element's `xrefLabel` property is
//!    `off`; a label whose element has no partners left is removed.
//!
//! Running it twice changes nothing the second time.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::compartment::{self as cp, Column, CompartmentKind};
use vizcraft_diagram::sheet;
use vizcraft_diagram::text_fields::TEXT_BOX_STENCIL;
use vizcraft_diagram::{Color, Document, Page, Shape, ShapeKind, Style};
use vizcraft_model::identity::{ComposeCtx, IdFormat};
use vizcraft_model::{Element, Issue, Registry};

use crate::Catalog;
use crate::schema::{Check, Pack, RelationshipKind, RuleDecl, TableQuery};

/// `ext` key on a generated cross-reference label: the id of the view it labels.
pub const EXT_XREF_FOR: &str = "vizcraft.xref.for";
/// Balloon element type.
pub const BALLOON: &str = "sheet.Balloon";
/// Part element type.
pub const PART: &str = "sheet.Part";
/// Parts list element type.
pub const PARTS_LIST: &str = "sheet.PartsList";
/// Revision element type.
pub const REVISION: &str = "sheet.Revision";
/// The leader relationship from a balloon to its part.
pub const CALLS_OUT: &str = "sheet.CallsOut";
/// Default cross-reference format for a partner on another sheet.
pub const XREF_FORMAT: &str = "/{sheet}.{zone}";
/// Default cross-reference format for a partner on the same sheet.
pub const XREF_SAME_SHEET: &str = "{zone}";
/// Font size of a cross-reference label.
const LABEL_FONT: f64 = 10.0;
/// Most partner lines one label shows.
const MAX_LABEL_LINES: usize = 32;

/// Run the sheet pass over `doc`.
pub fn run(doc: &mut Document, cat: &Catalog) {
    tables(doc, cat);
    balloons(doc);
    xref_labels(doc, cat);
}

fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

/// One cell of a table row.
fn cell(e: &Element, col: &str) -> String {
    match col {
        "id" => e.id.clone(),
        "tag" => e.tag.clone(),
        "name" => e.name.clone(),
        "type" => e.type_id.clone(),
        k => e.prop(k).map(|v| v.display()).unwrap_or_default(),
    }
}

/// A sort key: numbers numerically, then revision letters in order, then text.
fn sort_key(v: &str) -> (u8, u64, usize, String) {
    let t = v.trim();
    if let Ok(n) = t.parse::<u64>() {
        return (0, n, 0, String::new());
    }
    if let Some(r) = sheet::revision_rank(t, sheet::RevisionScheme::Iso7200) {
        return (1, 0, r, String::new());
    }
    (2, 0, 0, t.to_lowercase())
}

/// The value of `key` on `e`, else its schema default.
fn value_or_default<'a>(cat: &'a Catalog, e: &'a Element, key: &str) -> Option<&'a vizcraft_model::PropValue> {
    e.prop(key).or_else(|| cat.element_type(&e.type_id).and_then(|(_, t)| t.property(key)).and_then(|p| p.default.as_ref()))
}

/// The rows a table query gives for the element `owner` (whose `types` property may override
/// the listed types): with `owned`, only elements it owns; with a `filter`, only elements whose
/// property (or its default) matches; cells mapped through `values`.
pub fn table_rows(doc: &Document, cat: &Catalog, owner: &Element, q: &TableQuery) -> Vec<String> {
    rows_of(doc.model.elements.iter(), cat, owner, q)
}

/// The element ids behind [`table_rows`], one per row in the same order: what a canvas edit of a
/// cell writes to (an ER column's attribute).
pub fn table_row_ids(doc: &Document, cat: &Catalog, owner: &Element, q: &TableQuery) -> Vec<String> {
    rows_with_ids(doc.model.elements.iter(), cat, owner, q).into_iter().map(|(_, id)| id).collect()
}

/// [`table_rows`] over `candidates` (every element, or the owner's children when the query is
/// `owned`).
fn rows_of<'a>(candidates: impl Iterator<Item = &'a Element>, cat: &Catalog, owner: &Element, q: &TableQuery) -> Vec<String> {
    rows_with_ids(candidates, cat, owner, q).into_iter().map(|(row, _)| row).collect()
}

/// The rows of [`rows_of`], each with the id of the element it shows.
fn rows_with_ids<'a>(candidates: impl Iterator<Item = &'a Element>, cat: &Catalog, owner: &Element, q: &TableQuery) -> Vec<(String, String)> {
    let override_types: Vec<String> = owner
        .prop("types")
        .and_then(|v| v.as_str().map(str::to_string))
        .map(|t| t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();
    let types = if override_types.is_empty() { &q.types } else { &override_types };
    let cols: Vec<&String> = q.columns.iter().take(cp::MAX_COLUMNS).collect();
    let shown = |col: &str, e: &Element| -> String {
        let raw = cell(e, col);
        match q.values.get(col) {
            Some(map) => {
                let v = if raw.is_empty() { value_or_default(cat, e, col).map(|v| v.display()).unwrap_or_default() } else { raw };
                map.get(&v).cloned().unwrap_or_default()
            }
            None => raw,
        }
    };
    let mut rows: Vec<(Vec<String>, String, String)> = candidates
        .filter(|e| e.id != owner.id && crate::infer::chain_of(cat, &e.type_id).iter().any(|t| types.iter().any(|x| x == t)))
        .filter(|e| !q.owned || e.owner.as_deref() == Some(owner.id.as_str()))
        .filter(|e| match &q.filter {
            None => true,
            Some(f) => match (&f.equals, value_or_default(cat, e, &f.property)) {
                (None, v) => v.is_some(),
                (Some(_), None) => false,
                (Some(a), Some(b)) => a == b,
            },
        })
        .take(cp::MAX_ROWS)
        .map(|e| {
            let cells: Vec<String> = cols.iter().map(|c| shown(c, e).replace(['\t', '\n', '\r'], " ")).collect();
            let key = if q.sort_by.is_empty() { String::new() } else { cell(e, &q.sort_by) };
            (cells, key, e.id.clone())
        })
        .collect();
    if !q.sort_by.is_empty() {
        rows.sort_by_key(|(_, k, _)| sort_key(k));
    }
    rows.into_iter().map(|(c, _, id)| (c.join("\t"), id)).collect()
}

/// The columns of a table query's compartment.
fn query_columns(q: &TableQuery) -> Vec<Column> {
    q.columns
        .iter()
        .take(cp::MAX_COLUMNS)
        .enumerate()
        .map(|(i, c)| Column {
            title: q.headers.get(i).cloned().unwrap_or_else(|| c.clone()),
            width: q.widths.get(i).copied().filter(|w| w.is_finite() && *w > 0.0).unwrap_or(80.0),
        })
        .collect()
}

/// Step 1: fill query compartments, with the declarations of each page's rendering variant
/// (`Catalog::compartments_in`).
fn tables(doc: &mut Document, cat: &Catalog) {
    let has_query = |d: &crate::schema::CompartmentDecl| d.query.is_some();
    if cat.packs().iter().all(|p| {
        p.element_types.iter().all(|t| !t.compartments.iter().any(has_query))
            && p.variants.iter().all(|v| !v.compartments.values().flatten().any(has_query))
    }) {
        return;
    }
    // Filled tables per (element, variant): (compartment id, columns, rows).
    type Plan = Vec<(String, Vec<Column>, Vec<String>, bool)>;
    let mut plans: HashMap<(String, String), Plan> = HashMap::new();
    let by_id: HashMap<&str, &Element> = doc.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut children: HashMap<&str, Vec<&Element>> = HashMap::new();
    for e in &doc.model.elements {
        if let Some(o) = e.owner.as_deref() {
            children.entry(o).or_default().push(e);
        }
    }
    for page in &doc.pages {
        let variant = page.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|id| by_id.get(id).copied()) else { continue };
            let key = (e.id.clone(), variant.clone());
            if plans.contains_key(&key) {
                continue;
            }
            let plan: Plan = cat
                .compartments_in(&e.type_id, &variant)
                .iter()
                .filter_map(|d| {
                    let q = d.query.as_ref()?;
                    let rows = if q.owned {
                        rows_of(children.get(e.id.as_str()).map(|v| v.iter().copied()).into_iter().flatten(), cat, e, q)
                    } else {
                        table_rows(doc, cat, e, q)
                    };
                    Some((d.id.clone(), query_columns(q), rows, q.owned))
                })
                .collect();
            if !plan.is_empty() {
                plans.insert(key, plan);
            }
        }
    }
    if plans.is_empty() {
        return;
    }
    for page in &mut doc.pages {
        let variant = page.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
        for s in &mut page.shapes {
            let Some(plan) = s.element.clone().and_then(|e| plans.get(&(e, variant.clone()))) else { continue };
            let mut centred = false;
            for (id, cols, rows, owned) in plan {
                let Some(c) = s.compartments.iter_mut().find(|c| &c.id == id && c.kind == CompartmentKind::List) else { continue };
                set_if_changed(&mut c.columns, cols.clone());
                set_if_changed(&mut c.rows, rows.clone());
                c.sanitize();
                centred |= *owned;
            }
            grow_to_fit(s, centred);
        }
    }
}

/// Grow `s` to fit its main text and compartments when they no longer fit. A shape whose rows
/// list what its element owns (an ER table's columns) grows about its centre, so redrawing it in
/// another notation (`variant.set` keeps centres) and back does not walk it down the page; any
/// other shape grows downward.
pub fn grow_to_fit(s: &mut Shape, centred: bool) {
    let need = cp::fit_height(s);
    if need > s.h {
        if centred {
            s.y -= (need - s.h) / 2.0;
        }
        s.h = need;
        s.sanitize();
    }
}

/// Step 2: balloons show their part's tag.
fn balloons(doc: &mut Document) {
    let model = &doc.model;
    let mut names: HashMap<String, String> = HashMap::new();
    for r in model.relationships.iter().filter(|r| r.type_id == CALLS_OUT) {
        if names.contains_key(&r.source) {
            continue;
        }
        let (Some(b), Some(p)) = (model.element(&r.source), model.element(&r.target)) else { continue };
        if b.type_id == BALLOON {
            names.insert(b.id.clone(), p.tag.clone());
        }
    }
    if names.is_empty() {
        return;
    }
    for e in &mut doc.model.elements {
        if let Some(n) = names.get(&e.id) {
            set_if_changed(&mut e.name, n.clone());
        }
    }
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            if let Some(n) = s.element.as_deref().and_then(|e| names.get(e)) {
                set_if_changed(&mut s.text, n.clone());
            }
        }
    }
}

/// One partner of an element: the partner's element id and the relationship that links them
/// (`None` when they share a paired tag).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partner {
    /// The partner element.
    pub element: String,
    /// The cross-reference relationship, if any.
    pub relationship: Option<String>,
}

/// Whether relationship type `type_id` is a cross reference.
pub fn is_cross_reference(cat: &Catalog, type_id: &str) -> bool {
    cat.relationship_type(type_id).is_some_and(|(_, t)| t.kind == RelationshipKind::CrossReference)
}

/// Every element's cross-reference partners, in model order.
pub fn partners(doc: &Document, cat: &Catalog) -> HashMap<String, Vec<Partner>> {
    let mut out: HashMap<String, Vec<Partner>> = HashMap::new();
    for r in doc.model.relationships.iter().filter(|r| is_cross_reference(cat, &r.type_id)) {
        if r.source == r.target {
            continue;
        }
        out.entry(r.source.clone()).or_default().push(Partner { element: r.target.clone(), relationship: Some(r.id.clone()) });
        out.entry(r.target.clone()).or_default().push(Partner { element: r.source.clone(), relationship: Some(r.id.clone()) });
    }
    for scope in cat.packs().iter().flat_map(|p| p.identifiers.iter()).filter(|s| s.paired) {
        let mut by_tag: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in doc.model.elements.iter().filter(|e| !e.tag.is_empty() && scope.types.contains(&e.type_id)) {
            by_tag.entry(e.tag.as_str()).or_default().push(e.id.as_str());
        }
        for ids in by_tag.values().filter(|v| v.len() > 1) {
            for a in ids {
                for b in ids.iter().filter(|b| *b != a) {
                    let list = out.entry((*a).to_string()).or_default();
                    if !list.iter().any(|p| p.element == *b) {
                        list.push(Partner { element: (*b).to_string(), relationship: None });
                    }
                }
            }
        }
    }
    out
}

/// Where a view is: its page, sheet number and zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// Page index in `doc.pages`.
    pub page_index: usize,
    /// Page id.
    pub page: String,
    /// The view's shape id.
    pub view: String,
    /// The sheet number (the page number on a page without a sheet).
    pub sheet: u32,
    /// The zone of the view's centre; empty off the frame or without a sheet.
    pub zone: String,
}

/// The first view of `element` on a foreground page.
pub fn location(doc: &Document, element: &str) -> Option<Location> {
    doc.pages
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_background)
        .find_map(|(i, p)| p.shapes.iter().find(|s| s.element.as_deref() == Some(element)).map(|s| locate(doc, i, p, s)))
}

fn locate(doc: &Document, i: usize, p: &Page, s: &Shape) -> Location {
    Location {
        page_index: i,
        page: p.id.clone(),
        view: s.id.clone(),
        sheet: sheet::sheet_number(doc, i),
        zone: sheet::zone_of(p, s.center()).map(|z| z.to_string()).unwrap_or_default(),
    }
}

/// The text one partner location shows from page `from` in the document's formats.
pub fn location_text(doc: &Document, from: usize, at: &Location, tag: &str) -> String {
    let fields = &doc.properties.fields;
    let format = if at.page_index == from {
        fields.get("xrefSameSheet").filter(|f| !f.is_empty()).map_or(XREF_SAME_SHEET, String::as_str)
    } else {
        fields.get("xrefFormat").filter(|f| !f.is_empty()).map_or(XREF_FORMAT, String::as_str)
    };
    let page = at.page_index.saturating_add(1).to_string();
    let out = format.replace("{sheet}", &at.sheet.to_string()).replace("{zone}", &at.zone).replace("{page}", &page).replace("{tag}", tag);
    out.chars().take(200).collect()
}

/// Whether `e` asks for no location label.
fn labels_off(e: &Element) -> bool {
    e.prop("xrefLabel").and_then(|v| v.as_str()).is_some_and(|v| v.eq_ignore_ascii_case("off"))
}

/// Step 3: one location label per view of an element with partners.
fn xref_labels(doc: &mut Document, cat: &Catalog) {
    let has_labels = doc.pages.iter().any(|p| p.shapes.iter().any(|s| s.ext.contains_key(EXT_XREF_FOR)));
    let partners = partners(doc, cat);
    if partners.is_empty() && !has_labels {
        return;
    }
    // Wanted text per view id.
    let mut wanted: HashMap<String, String> = HashMap::new();
    let mut located: HashMap<&str, Option<Location>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
        for s in &page.shapes {
            let Some(e) = s.element.as_deref() else { continue };
            let Some(list) = partners.get(e) else { continue };
            if doc.model.element(e).is_none_or(labels_off) {
                continue;
            }
            let mut lines = Vec::new();
            for p in list.iter().take(MAX_LABEL_LINES) {
                let at = located.entry(p.element.as_str()).or_insert_with(|| location(doc, &p.element));
                if let Some(at) = at {
                    let tag = doc.model.element(&p.element).map(|x| x.tag.as_str()).unwrap_or_default();
                    lines.push(location_text(doc, pi, at, tag));
                }
            }
            if !lines.is_empty() {
                wanted.insert(s.id.clone(), lines.join("\n"));
            }
        }
    }
    let mut plans: Vec<(usize, Vec<(String, String)>)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        let missing: Vec<(String, String)> = page
            .shapes
            .iter()
            .filter(|s| wanted.contains_key(&s.id))
            .filter(|s| !page.shapes.iter().any(|l| l.ext.get(EXT_XREF_FOR) == Some(&s.id)))
            .map(|s| (s.id.clone(), String::new()))
            .collect();
        if !missing.is_empty() {
            plans.push((pi, missing));
        }
    }
    for (_, list) in &mut plans {
        for (_, id) in list.iter_mut() {
            *id = doc.next_id("s");
        }
    }
    for (pi, page) in doc.pages.iter_mut().enumerate() {
        // Drop labels whose view is gone or no longer wants one.
        let views: HashSet<String> = page.shapes.iter().map(|s| s.id.clone()).collect();
        page.shapes.retain(|s| match s.ext.get(EXT_XREF_FOR) {
            Some(v) => views.contains(v) && wanted.contains_key(v),
            None => true,
        });
        for s in page.shapes.iter_mut() {
            let Some(text) = s.ext.get(EXT_XREF_FOR).and_then(|v| wanted.get(v)) else { continue };
            if s.text != *text {
                s.text = text.clone();
                let need = label_height(text);
                if s.h < need {
                    s.h = need;
                }
                s.sanitize();
            }
        }
        let Some((_, missing)) = plans.iter().find(|(p, _)| *p == pi) else { continue };
        for (view, id) in missing {
            let Some(v) = page.shape(view) else { continue };
            let Some(text) = wanted.get(view) else { continue };
            let label = new_label(id.clone(), v, text);
            page.shapes.push(label);
        }
    }
}

fn label_height(text: &str) -> f64 {
    text.lines().count().max(1) as f64 * LABEL_FONT * cp::LINE_HEIGHT + 4.0
}

/// A location label for view `v`, just below it.
fn new_label(id: String, v: &Shape, text: &str) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, v.x, v.y + v.h + 2.0, v.w.max(60.0), label_height(text));
    s.stencil = TEXT_BOX_STENCIL.to_string();
    s.style = Style { fill: Color::TRANSPARENT, line: Color::TRANSPARENT, line_width: 0.0, font_size: LABEL_FONT, ..Style::DEFAULT };
    s.style.h_align = vizcraft_diagram::HAlign::Left;
    s.style.v_align = vizcraft_diagram::VAlign::Top;
    s.text = text.to_string();
    s.parent = Some(v.id.clone());
    s.layers = v.layers.clone();
    s.ext.insert(EXT_XREF_FOR.into(), v.id.clone());
    s.sanitize();
    s
}

// ------------------------------------------------------------------------------------ checks

fn issue(rule: &RuleDecl, what: &str, subjects: Vec<String>, doc: &Document) -> Issue {
    let mut items = Vec::new();
    let mut page = None;
    for sub in &subjects {
        for (pi, p) in doc.pages.iter().enumerate() {
            for s in p.shapes.iter().filter(|s| s.element.as_deref() == Some(sub.as_str())) {
                items.push(s.id.clone());
                page = match page {
                    None => Some(Some(pi)),
                    Some(Some(q)) if q == pi => Some(Some(pi)),
                    _ => Some(None),
                };
            }
        }
    }
    // Each sheet finding is a whole sentence about the problem ("Legal owner is empty."), so the
    // message is the finding alone; the rule's description, which states what should hold, would
    // read as part of the problem. The Problems panel shows the rule beside it.
    let mut i = Issue::new(rule.id.clone(), rule.severity, crate::rules::cut(what.trim(), crate::rules::MAX_MESSAGE))
        .with_subjects(subjects)
        .with_items(items);
    i.page = page.flatten().and_then(|p| doc.pages.get(p)).map(|p| p.id.clone());
    i
}

fn label(e: &Element) -> String {
    let n = if !e.name.is_empty() {
        e.name.clone()
    } else if !e.tag.is_empty() {
        e.tag.clone()
    } else {
        e.id.clone()
    };
    format!("`{}`", n.chars().take(60).collect::<String>())
}

/// The first foreground page with a sheet.
fn first_sheet(doc: &Document) -> Option<(usize, &vizcraft_diagram::sheet::Sheet)> {
    doc.pages.iter().enumerate().find_map(|(i, p)| if p.is_background { None } else { p.sheet.as_ref().map(|s| (i, s)) })
}

/// Run one sheet check.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    match &rule.check {
        Check::DocumentFields { length } => document_fields(doc, pack, rule, *length, out),
        Check::RevisionSequence => revision_sequence(doc, rule, out),
        Check::BalloonRows => balloon_rows(doc, rule, out),
        Check::CrossReferences => cross_references(doc, cat, rule, out),
        Check::StaleTag => stale_tags(doc, cat, rule, out),
        _ => {}
    }
}

fn document_fields(doc: &Document, pack: &Pack, rule: &RuleDecl, length: bool, out: &mut Vec<Issue>) {
    let Some((pi, _)) = first_sheet(doc) else { return };
    for p in &pack.document_properties {
        let value = vizcraft_diagram::text_fields::doc_value(doc, pi, &p.key);
        let name = if p.label.is_empty() { p.key.clone() } else { p.label.clone() };
        if length {
            let n = value.chars().count();
            if let Some(max) = p.max_chars.filter(|m| n > *m as usize) {
                out.push(issue(rule, &format!("{name} has {n} characters; at most {max} fit."), Vec::new(), doc));
            }
        } else if p.required && value.trim().is_empty() {
            out.push(issue(rule, &format!("{name} is empty."), Vec::new(), doc));
        }
    }
}

fn revision_sequence(doc: &Document, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let scheme = first_sheet(doc).map(|(_, s)| s.revision_scheme).unwrap_or_default();
    let mut revs: Vec<(usize, &Element, String)> = Vec::new();
    for e in doc.model.elements.iter().filter(|e| e.type_id == REVISION) {
        let rev = e.prop("rev").map(|v| v.display()).unwrap_or_default();
        match sheet::revision_rank(&rev, scheme) {
            Some(r) => revs.push((r, e, rev)),
            None => out.push(issue(rule, &format!("Revision {} is not a revision letter of the sheet's scheme.", label(e)), vec![e.id.clone()], doc)),
        }
    }
    revs.sort_by_key(|(r, _, _)| *r);
    for w in revs.windows(2) {
        let (Some((ra, a, va)), Some((rb, b, vb))) = (w.first(), w.get(1)) else { continue };
        if ra == rb {
            out.push(issue(rule, &format!("Revision {vb} is used twice."), vec![a.id.clone(), b.id.clone()], doc));
        } else if rb.saturating_sub(*ra) > 1 {
            let missing = sheet::next_revision(va, scheme).unwrap_or_default();
            out.push(issue(rule, &format!("Revision {vb} follows {va}; {missing} is missing."), vec![b.id.clone()], doc));
        }
    }
}

fn balloon_rows(doc: &Document, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let model = &doc.model;
    let lists = model.elements.iter().any(|e| e.type_id == PARTS_LIST);
    let mut called: HashSet<&str> = HashSet::new();
    for b in model.elements.iter().filter(|e| e.type_id == BALLOON) {
        let targets: Vec<&str> = model
            .outgoing(&b.id)
            .filter(|r| r.type_id == CALLS_OUT)
            .filter_map(|r| model.element(&r.target))
            .filter(|t| t.type_id == PART)
            .map(|t| t.id.as_str())
            .collect();
        match targets.as_slice() {
            [] => out.push(issue(rule, &format!("Balloon {} calls out no part.", label(b)), vec![b.id.clone()], doc)),
            [one] => {
                called.insert(one);
            }
            _ => out.push(issue(rule, &format!("Balloon {} calls out {} parts; one each.", label(b), targets.len()), vec![b.id.clone()], doc)),
        }
    }
    if lists {
        for p in model.elements.iter().filter(|e| e.type_id == PART && !called.contains(e.id.as_str())) {
            out.push(issue(rule, &format!("Part {} has no balloon.", label(p)), vec![p.id.clone()], doc));
        }
    }
}

fn cross_references(doc: &Document, cat: &Catalog, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let drawn: HashSet<&str> = sheet::first_views(doc).into_keys().collect();
    for r in doc.model.relationships.iter().filter(|r| is_cross_reference(cat, &r.type_id)) {
        for end in [&r.source, &r.target] {
            if !drawn.contains(end.as_str()) {
                let what = match doc.model.element(end) {
                    Some(e) => format!("{} is not drawn on any page, so its partner cannot say where it is.", label(e)),
                    None => format!("A cross reference ends at `{end}`, which is not an element."),
                };
                out.push(issue(rule, &what, vec![r.id.clone(), end.clone()], doc));
            }
        }
    }
}

/// The tag `e` should have now in a context format, keeping its number; `None` when it cannot be
/// worked out (no number to keep) or the scope has no context parts.
pub fn recomposed(model: &vizcraft_model::Model, format: &IdFormat, e: &Element, ctx: &ComposeCtx) -> Option<String> {
    if !format.has_context() {
        return None;
    }
    let n = if format.has_number() { format.number_in(&e.tag, e)? } else { 0 };
    Some(format.compose_in(model, e, n, ctx))
}

fn stale_tags(doc: &Document, cat: &Catalog, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let scopes = cat.scopes();
    // Read through the project's overrides, so a changed format is the one checked.
    let reg = Registry::new(&doc.model, &scopes);
    let formats: Vec<(&vizcraft_model::IdScope, IdFormat)> =
        reg.effective().iter().filter_map(|s| IdFormat::parse(&s.format).ok().filter(IdFormat::has_context).map(|f| (s, f))).collect();
    if formats.is_empty() {
        return;
    }
    let ctxs = sheet::compose_ctxs(doc);
    for e in doc.model.elements.iter().filter(|e| !e.tag.is_empty()) {
        let Some(scope) = reg.scope_for(&e.type_id) else { continue };
        let Some((_, f)) = formats.iter().find(|(s, _)| s.id == scope.id) else { continue };
        let ctx = ctxs.get(&e.id).cloned().unwrap_or_default();
        if let Some(want) = recomposed(&doc.model, f, e, &ctx)
            && want != e.tag
        {
            let mut i = issue(rule, &format!("{} is tagged {} but its place makes it {want}.", label(e), e.tag), vec![e.id.clone()], doc);
            i.fixes.push(vizcraft_model::issue::Fix::new("Renumber", "sheet.renumber", serde_json::json!({})));
            out.push(i);
        }
    }
}
