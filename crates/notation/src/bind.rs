//! The sync pass: keeps the model and its views in step after every document change.
//!
//! The engine calls [`sync`] after each command that changed the document, inside the same undo
//! step, before connector rerouting. It only touches items that are already bound or that join
//! bound items, so free drawing (and every Cascade 1 to 3 document) is left exactly as it was.
//!
//! Running it twice with the same `before` changes nothing the second time. Lookups are indexed
//! once per step, so the pass is linear in the document except for the few new ids it hands out.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::{Color, Connector, Document, End, Style};
use vizcraft_model::{Element, PropValue, Registry, Relationship};

use crate::schema::ConditionalStyle;
use crate::{Catalog, legal};

/// Bring views and model into agreement after a command turned `before` into `doc`, in this order:
///
/// 1. A shape whose `element` is missing from the model, and a connector whose `relationship` is
///    missing, become free drawing (the reference is cleared).
/// 2. A second shape for the same element on one page is a copy: it gets a new element (fresh id
///    from `Document::next_id("e")`, same type and properties, name kept, tag re-issued by the
///    identity registry when the scope numbers automatically, else cleared).
/// 3. A connector glued at both ends to bound shapes gets a relationship when it has none: type
///    from [`crate::legal::relationship_for`] (an illegal pair still binds, so the rules engine
///    can report it), id from `next_id("r")`, the connector's text as its name. A bound
///    connector's relationship follows its glue (source and target from the glued shapes'
///    elements, ports from glued connection-point indexes when the type declares ports); a bound
///    connector with a free end keeps its relationship as it was.
/// 4. A relationship that some connector drew in `before` and no connector draws now is removed
///    from the model: deleting the last connector view deletes the relationship. Relationships
///    that never had a view (made by `relationship.create` between elements not on one page) stay,
///    and so does a relationship whose source or target differs from `before` (reconnected by this
///    command, which may drop views on pages that do not draw the new end).
/// 5. Every bound shape's text becomes its element's name, and every bound connector's text its
///    relationship's name.
/// 6. Style fields named by conditional styles (see `ConditionalStyle`) are set on bound shapes
///    when the element's match changes, or once on a new element or view.
/// 7. Packs of every element and relationship type in use are recorded in `doc.model.packs`.
///
/// Details of each step:
///
/// - Step 2 keeps the element on the first shape (z order) that already drew it in `before`, else
///   on the first shape; the other shapes on that page get copies. A copy that cannot be added
///   (the model is full) leaves its shape as free drawing.
/// - Step 3: when several connectors draw one relationship, the first whose glued elements equal
///   the relationship's ends keeps it (else the first glued at both ends moves it); a connector
///   glued between other elements gets a copy of the relationship with a fresh id. Ports: when an
///   end's element type declares ports, a connector glued at connection point `i` sets that end's
///   port to the id of port `i` (dynamic glue or an index past the ports: none); a type with no
///   ports clears the port.
/// - Step 5 decides between text and name per element (and relationship): a name changed by the
///   command wins; else a view whose text the command changed gives the name; else a new item
///   with no name takes its first non-empty view text; else the name stays. Every view then shows
///   the name.
/// - Step 6 changes style only when a match changes, so hand styling wins otherwise. Per style
///   field (fill, line, text colour, line width), the first matching conditional style of the
///   element's type that names the field is compared between `before` and `doc`. For a shape that
///   already drew an element that already existed, a field whose match changed takes the new
///   style's value, or the glyph's value when nothing matches now (fill and line from the type's
///   glyph, text colour and line width from the default style); unchanged fields are left alone.
///   A new element or a newly bound shape gets the fields that match now, and keeps its glyph
///   style where nothing matches. A style matches when the element's property (or the property's
///   schema default) equals `equals`; a style with no `equals` matches when the property has any
///   value.
pub fn sync(doc: &mut Document, before: &Document, cat: &Catalog) {
    vizcraft_diagram::views::sanitize_refs(doc);
    // Nothing can be bound in a model with no items, and nothing drawn in `before` can need
    // removing: free drawing leaves here untouched.
    if doc.model.elements.is_empty() && doc.model.relationships.is_empty() {
        return;
    }
    copies(doc, before, cat);
    connectors(doc, cat);
    drop_unviewed(doc, before);
    names(doc, before);
    styles(doc, before, cat);
    record_packs(doc, cat);
}

/// Element index by id; the model is appended to only during the pass, so indexes stay valid.
fn element_index(doc: &Document) -> HashMap<String, usize> {
    let mut m = HashMap::with_capacity(doc.model.elements.len());
    for (i, e) in doc.model.elements.iter().enumerate() {
        m.entry(e.id.clone()).or_insert(i);
    }
    m
}

/// Step 2: a second shape of one element on one page gets a copy of the element.
fn copies(doc: &mut Document, before: &Document, cat: &Catalog) {
    let before_el: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.shapes.iter()).filter_map(|s| s.element.as_deref().map(|e| (s.id.as_str(), e))).collect();
    let mut plan: Vec<(usize, usize, String)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        let mut first: HashMap<&str, usize> = HashMap::new();
        let mut prior: HashMap<&str, usize> = HashMap::new();
        for (si, s) in page.shapes.iter().enumerate() {
            let Some(e) = s.element.as_deref() else { continue };
            first.entry(e).or_insert(si);
            if before_el.get(s.id.as_str()) == Some(&e) {
                prior.entry(e).or_insert(si);
            }
        }
        for (si, s) in page.shapes.iter().enumerate() {
            let Some(e) = s.element.as_deref() else { continue };
            let keep = prior.get(e).or_else(|| first.get(e)).copied();
            if keep != Some(si) {
                plan.push((pi, si, e.to_string()));
            }
        }
    }
    if plan.is_empty() {
        return;
    }
    let index = element_index(doc);
    let scopes = cat.scopes();
    for (pi, si, e) in plan {
        let Some(src) = index.get(&e).and_then(|&i| doc.model.elements.get(i)).cloned() else { continue };
        let id = doc.next_id("e");
        let mut copy = Element { id: id.clone(), tag: String::new(), ..src };
        copy.tag = Registry::new(&doc.model, &scopes).next_tag(&copy).ok().flatten().unwrap_or_default();
        let bound = doc.model.add_element(copy).is_ok().then_some(id);
        if let Some(shape) = doc.pages.get_mut(pi).and_then(|p| p.shapes.get_mut(si)) {
            shape.element = bound;
        }
    }
}

/// Where a connector glued at both ends to bound shapes attaches.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Glue {
    source: String,
    target: String,
    source_point: Option<usize>,
    target_point: Option<usize>,
}

fn glue_of(shape_el: &HashMap<&str, &str>, c: &Connector) -> Option<Glue> {
    let (End::Glued { shape: s, point: sp }, End::Glued { shape: t, point: tp }) = (&c.start, &c.end) else { return None };
    Some(Glue {
        source: (*shape_el.get(s.as_str())?).to_string(),
        target: (*shape_el.get(t.as_str())?).to_string(),
        source_point: *sp,
        target_point: *tp,
    })
}

/// The port an end attaches to: by connection-point index when the element's type declares ports.
fn port(doc: &Document, index: &HashMap<String, usize>, cat: &Catalog, element: &str, point: Option<usize>) -> Option<String> {
    let e = index.get(element).and_then(|&i| doc.model.elements.get(i))?;
    let (_, t) = cat.element_type(&e.type_id)?;
    t.ports.get(point?).map(|p| p.id.clone())
}

/// What step 3 does to one connector.
enum Bind {
    /// Move the relationship's ends to this glue.
    Follow(String, Glue),
    /// Give the connector a copy of this relationship, glued here.
    Copy(usize, usize, String, Glue),
    /// Give the connector a new relationship.
    New(usize, usize, Glue),
}

/// Step 3: connectors glued between bound shapes get relationships; bound ones follow glue.
fn connectors(doc: &mut Document, cat: &Catalog) {
    let mut plan: Vec<Bind> = Vec::new();
    {
        let mut rels: HashMap<&str, &Relationship> = HashMap::with_capacity(doc.model.relationships.len());
        for r in &doc.model.relationships {
            rels.entry(r.id.as_str()).or_insert(r);
        }
        // Glued connectors per relationship, in document order.
        let mut by_rel: HashMap<&str, Vec<(usize, usize, Glue)>> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (pi, page) in doc.pages.iter().enumerate() {
            let shape_el: HashMap<&str, &str> = page.shapes.iter().filter_map(|s| s.element.as_deref().map(|e| (s.id.as_str(), e))).collect();
            if shape_el.is_empty() {
                continue;
            }
            for (ci, c) in page.connectors.iter().enumerate() {
                let Some(g) = glue_of(&shape_el, c) else { continue };
                match c.relationship.as_deref() {
                    Some(r) if rels.contains_key(r) => {
                        let list = by_rel.entry(r).or_default();
                        if list.is_empty() {
                            order.push(r);
                        }
                        list.push((pi, ci, g));
                    }
                    Some(_) => {}
                    None => plan.push(Bind::New(pi, ci, g)),
                }
            }
        }
        for r in order {
            let Some(rel) = rels.get(r) else { continue };
            let Some(list) = by_rel.get(r) else { continue };
            let winner = list.iter().find(|(_, _, g)| g.source == rel.source && g.target == rel.target).or_else(|| list.first());
            let Some((wp, wc, wg)) = winner else { continue };
            plan.push(Bind::Follow(r.to_string(), wg.clone()));
            for (pi, ci, g) in list {
                if (pi, ci) != (wp, wc) && (g.source != wg.source || g.target != wg.target) {
                    plan.push(Bind::Copy(*pi, *ci, r.to_string(), g.clone()));
                }
            }
        }
    }
    if plan.is_empty() {
        return;
    }
    let index = element_index(doc);
    let rel_index: HashMap<String, usize> = doc.model.relationships.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect();
    for step in plan {
        match step {
            Bind::Follow(r, g) => {
                let sp = port(doc, &index, cat, &g.source, g.source_point);
                let tp = port(doc, &index, cat, &g.target, g.target_point);
                if let Some(rel) = rel_index.get(&r).and_then(|&i| doc.model.relationships.get_mut(i)) {
                    set_if_changed(&mut rel.source, g.source);
                    set_if_changed(&mut rel.target, g.target);
                    set_if_changed(&mut rel.source_port, sp);
                    set_if_changed(&mut rel.target_port, tp);
                }
            }
            Bind::Copy(pi, ci, r, g) => {
                let Some(template) = rel_index.get(&r).and_then(|&i| doc.model.relationships.get(i)).cloned() else { continue };
                let rel = Relationship {
                    source_port: port(doc, &index, cat, &g.source, g.source_point),
                    target_port: port(doc, &index, cat, &g.target, g.target_point),
                    source: g.source,
                    target: g.target,
                    ..template
                };
                add_relationship(doc, pi, ci, rel);
            }
            Bind::New(pi, ci, g) => {
                let type_of = |id: &str| index.get(id).and_then(|&i| doc.model.elements.get(i)).map(|e| e.type_id.clone());
                let (Some(st), Some(tt)) = (type_of(&g.source), type_of(&g.target)) else { continue };
                let Some((type_id, _legal)) = legal::relationship_for(cat, &st, &tt) else { continue };
                let name = doc.pages.get(pi).and_then(|p| p.connectors.get(ci)).map(|c| c.text.clone()).unwrap_or_default();
                let rel = Relationship {
                    type_id,
                    source_port: port(doc, &index, cat, &g.source, g.source_point),
                    target_port: port(doc, &index, cat, &g.target, g.target_point),
                    source: g.source,
                    target: g.target,
                    name: cut_text(name),
                    ..Relationship::default()
                };
                add_relationship(doc, pi, ci, rel);
            }
        }
    }
}

/// Add `rel` under a fresh id and bind connector `ci` of page `pi` to it; when the model refuses
/// it (full), the connector stays as it was.
fn add_relationship(doc: &mut Document, pi: usize, ci: usize, mut rel: Relationship) {
    let id = doc.next_id("r");
    rel.id = id.clone();
    if doc.model.add_relationship(rel).is_ok()
        && let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci))
    {
        c.relationship = Some(id);
    }
}

fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

/// Step 4: relationships whose last connector view went with this command.
fn drop_unviewed(doc: &mut Document, before: &Document) {
    let drawn_before: HashSet<&str> = before.pages.iter().flat_map(|p| p.connectors.iter()).filter_map(|c| c.relationship.as_deref()).collect();
    if drawn_before.is_empty() {
        return;
    }
    let drawn_now: HashSet<&str> = doc.pages.iter().flat_map(|p| p.connectors.iter()).filter_map(|c| c.relationship.as_deref()).collect();
    let ends_before: HashMap<&str, (&str, &str)> =
        before.model.relationships.iter().map(|r| (r.id.as_str(), (r.source.as_str(), r.target.as_str()))).collect();
    // A relationship reconnected by this command lost its old views on purpose; it is not an
    // orphan, so only one whose ends are unchanged goes with its last connector.
    let unchanged = |r: &Relationship| ends_before.get(r.id.as_str()).is_some_and(|&(s, t)| s == r.source && t == r.target);
    doc.model.relationships.retain(|r| drawn_now.contains(r.id.as_str()) || !drawn_before.contains(r.id.as_str()) || !unchanged(r));
}

/// A name or text cut to the model's text limit at a character boundary.
fn cut_text(mut t: String) -> String {
    if t.len() > vizcraft_model::MAX_TEXT {
        let mut end = vizcraft_model::MAX_TEXT;
        while end > 0 && !t.is_char_boundary(end) {
            end -= 1;
        }
        t.truncate(end);
    }
    t
}

/// The name an item and its views agree on (see step 5 in [`sync`]).
fn agreed_name(name: &str, before_name: Option<&str>, views: &[(&str, &str)], before_text: &HashMap<&str, &str>) -> String {
    if before_name.is_some_and(|b| b != name) {
        return name.to_string();
    }
    if let Some((_, t)) = views.iter().find(|(id, t)| before_text.get(id).is_some_and(|b| b != t)) {
        return (*t).to_string();
    }
    if before_name.is_none()
        && name.is_empty()
        && let Some((_, t)) = views.iter().find(|(_, t)| !t.is_empty())
    {
        return (*t).to_string();
    }
    name.to_string()
}

/// Step 5: names and view texts agree.
fn names(doc: &mut Document, before: &Document) {
    let before_shape_text: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.shapes.iter()).map(|s| (s.id.as_str(), s.text.as_str())).collect();
    let before_conn_text: HashMap<&str, &str> =
        before.pages.iter().flat_map(|p| p.connectors.iter()).map(|c| (c.id.as_str(), c.text.as_str())).collect();
    let before_el: HashMap<&str, &str> = before.model.elements.iter().map(|e| (e.id.as_str(), e.name.as_str())).collect();
    let before_rel: HashMap<&str, &str> = before.model.relationships.iter().map(|r| (r.id.as_str(), r.name.as_str())).collect();

    // Agreed names, computed with shared borrows, then written.
    let mut el_names: HashMap<String, String> = HashMap::new();
    let mut rel_names: HashMap<String, String> = HashMap::new();
    {
        let mut shape_views: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        let mut conn_views: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for page in &doc.pages {
            for s in &page.shapes {
                if let Some(e) = s.element.as_deref() {
                    shape_views.entry(e).or_default().push((s.id.as_str(), s.text.as_str()));
                }
            }
            for c in &page.connectors {
                if let Some(r) = c.relationship.as_deref() {
                    conn_views.entry(r).or_default().push((c.id.as_str(), c.text.as_str()));
                }
            }
        }
        for e in &doc.model.elements {
            if let Some(v) = shape_views.get(e.id.as_str()) {
                el_names.insert(e.id.clone(), agreed_name(&e.name, before_el.get(e.id.as_str()).copied(), v, &before_shape_text));
            }
        }
        for r in &doc.model.relationships {
            if let Some(v) = conn_views.get(r.id.as_str()) {
                rel_names.insert(r.id.clone(), agreed_name(&r.name, before_rel.get(r.id.as_str()).copied(), v, &before_conn_text));
            }
        }
    }
    if el_names.is_empty() && rel_names.is_empty() {
        return;
    }
    for e in &mut doc.model.elements {
        if let Some(n) = el_names.get(&e.id)
            && e.name != *n
        {
            e.name = cut_text(n.clone());
        }
    }
    for r in &mut doc.model.relationships {
        if let Some(n) = rel_names.get(&r.id)
            && r.name != *n
        {
            r.name = cut_text(n.clone());
        }
    }
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            if let Some(n) = s.element.as_deref().and_then(|e| el_names.get(e))
                && s.text != *n
            {
                s.text = n.clone();
            }
        }
        for c in &mut page.connectors {
            if let Some(n) = c.relationship.as_deref().and_then(|r| rel_names.get(r))
                && c.text != *n
            {
                c.text = n.clone();
            }
        }
    }
}

/// Style field values to set on a shape: `None` leaves the field alone.
#[derive(Clone, Debug, Default, PartialEq)]
struct Fields {
    fill: Option<Color>,
    line: Option<Color>,
    font_color: Option<Color>,
    line_width: Option<f64>,
}

impl Fields {
    fn is_empty(&self) -> bool {
        *self == Fields::default()
    }
}

/// A conditional style by position: (pack index in the catalog, index in its list).
type StyleId = (usize, usize);

/// Per style field, the first matching conditional style that names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Matches {
    fill: Option<StyleId>,
    line: Option<StyleId>,
    font_color: Option<StyleId>,
    line_width: Option<StyleId>,
}

/// Whether `cs` matches element `e` (the property's schema default stands in for a missing value).
fn matches(cs: &ConditionalStyle, e: &Element, default: Option<&PropValue>) -> bool {
    let v = e.prop(&cs.property).or(default);
    match (&cs.equals, v) {
        (None, v) => v.is_some(),
        (Some(_), None) => false,
        (Some(a), Some(b)) => a == b,
    }
}

/// The styles matching `e` now, field by field.
fn matches_for(cat: &Catalog, e: &Element) -> Matches {
    let mut m = Matches::default();
    let Some((_, t)) = cat.element_type(&e.type_id) else { return m };
    for (pi, pack) in cat.packs().iter().enumerate() {
        for (ci, cs) in pack.conditional_styles.iter().enumerate() {
            if !pack.lists(&cs.types, &e.type_id) || !matches(cs, e, t.property(&cs.property).and_then(|p| p.default.as_ref())) {
                continue;
            }
            let id = Some((pi, ci));
            if m.fill.is_none() && cs.fill.is_some() {
                m.fill = id;
            }
            if m.line.is_none() && cs.line.is_some() {
                m.line = id;
            }
            if m.font_color.is_none() && cs.font_color.is_some() {
                m.font_color = id;
            }
            if m.line_width.is_none() && cs.line_width.is_some() {
                m.line_width = id;
            }
        }
    }
    m
}

fn style_at(cat: &Catalog, id: StyleId) -> Option<&ConditionalStyle> {
    cat.packs().get(id.0).and_then(|p| p.conditional_styles.get(id.1))
}

/// The values a shape of `e` gets. `changed_only`: only fields whose match differs from `was`,
/// set to the new match's value or, when nothing matches now, the glyph's value. Otherwise the
/// fields that match now, with their values. A colour that does not parse leaves its field alone.
fn fields_for(cat: &Catalog, e: &Element, now: Matches, was: Option<Matches>) -> Fields {
    let glyph = cat.element_type(&e.type_id).and_then(|(_, t)| cat.glyph(&t.glyph)).map(|(_, g)| g);
    let d = Style::DEFAULT;
    let glyph_fill = glyph.and_then(|g| Color::parse(&g.fill)).unwrap_or(d.fill);
    let glyph_line = glyph.and_then(|g| Color::parse(&g.line)).unwrap_or(d.line);
    let pick = |now: Option<StyleId>, was: Option<Option<StyleId>>| -> Option<Option<StyleId>> {
        match was {
            // Changed match: the new style, or none (the glyph value).
            Some(w) if w != now => Some(now),
            Some(_) => None,
            // New element or view: only a current match is applied.
            None => now.map(Some),
        }
    };
    let colour = |sel: Option<Option<StyleId>>, get: fn(&ConditionalStyle) -> Option<&String>, glyph_value: Color| -> Option<Color> {
        match sel? {
            Some(id) => style_at(cat, id).and_then(get).and_then(|t| Color::parse(t)),
            None => Some(glyph_value),
        }
    };
    let width = match pick(now.line_width, was.map(|w| w.line_width)) {
        None => None,
        Some(None) => Some(d.line_width),
        Some(Some(id)) => style_at(cat, id).and_then(|cs| cs.line_width).filter(|w| w.is_finite()).map(|w| w.clamp(0.0, Style::MAX_LINE_WIDTH)),
    };
    Fields {
        fill: colour(pick(now.fill, was.map(|w| w.fill)), |cs| cs.fill.as_ref(), glyph_fill),
        line: colour(pick(now.line, was.map(|w| w.line)), |cs| cs.line.as_ref(), glyph_line),
        font_color: colour(pick(now.font_color, was.map(|w| w.font_color)), |cs| cs.font_color.as_ref(), d.font_color),
        line_width: width,
    }
}

/// Step 6: conditional styles on bound shapes, applied only when an element's match changes (or
/// on a new element or view), so hand styling made with `shape.setStyle` sticks.
fn styles(doc: &mut Document, before: &Document, cat: &Catalog) {
    if cat.packs().iter().all(|p| p.conditional_styles.is_empty()) {
        return;
    }
    let before_el: HashMap<&str, &Element> = before.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let before_shapes: HashSet<&str> = before.pages.iter().flat_map(|p| p.shapes.iter()).map(|s| s.id.as_str()).collect();
    // Per element: what a shape that already drew it gets (changes only), and what a new view gets.
    let mut plans: HashMap<String, (Fields, Fields)> = HashMap::new();
    for e in &doc.model.elements {
        let now = matches_for(cat, e);
        let fresh = fields_for(cat, e, now, None);
        let changed = match before_el.get(e.id.as_str()) {
            Some(b) => fields_for(cat, e, now, Some(matches_for(cat, b))),
            // A new element: every view of it is new.
            None => fresh.clone(),
        };
        if !(changed.is_empty() && fresh.is_empty()) {
            plans.entry(e.id.clone()).or_insert((changed, fresh));
        }
    }
    if plans.is_empty() {
        return;
    }
    for page in &mut doc.pages {
        for s in &mut page.shapes {
            let Some((changed, fresh)) = s.element.as_deref().and_then(|e| plans.get(e)) else { continue };
            let f = if before_shapes.contains(s.id.as_str()) { changed } else { fresh };
            if let Some(v) = f.fill {
                set_if_changed(&mut s.style.fill, v);
            }
            if let Some(v) = f.line {
                set_if_changed(&mut s.style.line, v);
            }
            if let Some(v) = f.font_color {
                set_if_changed(&mut s.style.font_color, v);
            }
            if let Some(v) = f.line_width {
                set_if_changed(&mut s.style.line_width, v);
            }
        }
    }
}

/// Step 7: every pack whose types are in use is listed in the model.
fn record_packs(doc: &mut Document, cat: &Catalog) {
    let mut seen: HashSet<&str> = doc.model.packs.iter().map(|p| p.id.as_str()).collect();
    let mut add = Vec::new();
    for pack in doc.model.elements.iter().map(|e| e.pack()).chain(doc.model.relationships.iter().map(|r| r.pack())) {
        if seen.insert(pack)
            && let Some(r) = cat.pack_ref(pack)
        {
            add.push(r);
        }
    }
    for r in add {
        doc.model.use_pack(r);
    }
}

#[cfg(test)]
#[path = "bind_tests.rs"]
mod tests;
