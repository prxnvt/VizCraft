//! The satellite pass of the sync pass (`crate::bind::sync`, after the end pass): on a page whose
//! rendering variant lists satellite types (`Variant::satellites`; Chen lists `er.Attribute`),
//! every element of those types owned by an element drawn on the page is drawn as a shape of its
//! own around its owner's view, joined to it by a plain line.
//!
//! A satellite is a bound shape (its element is the owned one, so the Inspector edits it) marked
//! with `ext["vizcraft.satelliteOf"]` = the owner view's id, with that view as its `parent` so it
//! moves with it. Its line is a connector glued to both, marked the same way; the sync pass never
//! makes a relationship of it. New satellites sit on a ring around the owner; ones already there
//! stay where they are. A key (`pk`) satellite has underlined text. On a page whose variant lists
//! no such type, and for elements no longer owned, the satellites and their lines are removed.
//! A satellite the user deletes comes back while the page keeps the variant, like a
//! cross-reference label.
//!
//! Running it twice changes nothing the second time.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::{Arrow, Color, Connector, Document, End, Route, Shape, ShapeKind};
use vizcraft_model::{Element, PropValue};

use crate::Catalog;

/// `ext` key on a satellite shape and its line: the id of the owner's view.
pub const EXT_SATELLITE_OF: &str = "vizcraft.satelliteOf";

/// Most satellites drawn around one view.
pub const MAX_PER_OWNER: usize = 256;

/// Space between an owner's frame and its satellites' ring.
const GAP: f64 = 40.0;

/// Whether `e` is a key element (`pk` true).
fn is_key(e: &Element) -> bool {
    matches!(e.prop("pk"), Some(PropValue::Bool(true)))
}

/// A new satellite shape for `e`, number `i` of `n` on the ring around `owner`.
fn new_satellite(cat: &Catalog, variant: &str, id: String, owner: &Shape, e: &Element, i: usize, n: usize) -> Shape {
    let glyph = cat.master_for_type_in(&e.type_id, variant).and_then(|g| cat.glyph(g)).map(|(_, g)| g);
    let (w, h) = glyph.map(|g| (g.w, g.h)).filter(|(w, h)| *w > 0.0 && *h > 0.0).unwrap_or((100.0, 40.0));
    let kind = glyph.and_then(|g| ShapeKind::from_name(&g.outline)).unwrap_or(ShapeKind::Ellipse);
    let c = owner.frame().center();
    let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / n.max(1) as f64;
    let (rx, ry) = (owner.w / 2.0 + GAP + w / 2.0, owner.h / 2.0 + GAP + h / 2.0);
    let mut s = Shape::new(id, kind, c.x + rx * angle.cos() - w / 2.0, c.y + ry * angle.sin() - h / 2.0, w, h);
    if let Some(g) = glyph {
        s.stencil = g.id.clone();
        s.style.fill = Color::parse(&g.fill).unwrap_or(s.style.fill);
        s.style.line = Color::parse(&g.line).unwrap_or(s.style.line);
    }
    s.element = Some(e.id.clone());
    s.text = e.name.clone();
    s.style.underline = is_key(e);
    s.parent = Some(owner.id.clone());
    s.layers = owner.layers.clone();
    s.ext.insert(EXT_SATELLITE_OF.into(), owner.id.clone());
    s.sanitize();
    s
}

/// The line from `owner` to satellite `sat`.
fn new_line(id: String, owner: &str, sat: &str) -> Connector {
    let mut c = Connector::new(id, End::dynamic(owner), End::dynamic(sat));
    c.route = Route::Straight;
    c.end_arrow = Arrow::None;
    c.ext.insert(EXT_SATELLITE_OF.into(), owner.to_string());
    c.sanitize();
    c
}

/// Run the satellite pass over `doc`.
pub fn satellites(doc: &mut Document, cat: &Catalog) {
    let any_variant = cat.packs().iter().any(|p| p.variants.iter().any(|v| !v.satellites.is_empty()));
    let any_drawn = doc.pages.iter().any(|p| p.shapes.iter().any(|s| s.ext.contains_key(EXT_SATELLITE_OF)));
    if !any_variant && !any_drawn {
        return;
    }
    let mut owned: HashMap<&str, Vec<&Element>> = HashMap::new();
    for e in &doc.model.elements {
        if let Some(o) = e.owner.as_deref() {
            owned.entry(o).or_default().push(e);
        }
    }
    // Per page: satellites to add (owner view id, element) and the wanted set.
    let mut adds: Vec<(usize, Vec<(String, Element, usize, usize)>)> = Vec::new();
    let mut wanted: Vec<HashSet<(String, String)>> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        let variant = page.view.as_ref().map(|v| v.variant.as_str()).unwrap_or_default();
        let types: &[String] = cat.variant(variant).map(|(_, v)| v.satellites.as_slice()).unwrap_or_default();
        let mut want: HashSet<(String, String)> = HashSet::new();
        let mut add = Vec::new();
        if !types.is_empty() {
            let have: HashSet<(&str, &str)> =
                page.shapes.iter().filter_map(|s| Some((s.ext.get(EXT_SATELLITE_OF)?.as_str(), s.element.as_deref()?))).collect();
            for s in page.shapes.iter().filter(|s| !s.ext.contains_key(EXT_SATELLITE_OF)) {
                let Some(children) = s.element.as_deref().and_then(|e| owned.get(e)) else { continue };
                let list: Vec<&&Element> = children
                    .iter()
                    .filter(|c| crate::infer::chain_of(cat, &c.type_id).iter().any(|t| types.iter().any(|x| x == t)))
                    .take(MAX_PER_OWNER)
                    .collect();
                for (i, c) in list.iter().enumerate() {
                    want.insert((s.id.clone(), c.id.clone()));
                    if !have.contains(&(s.id.as_str(), c.id.as_str())) {
                        add.push((s.id.clone(), (**c).clone(), i, list.len()));
                    }
                }
            }
        }
        wanted.push(want);
        if !add.is_empty() {
            adds.push((pi, add));
        }
    }
    // Ids first (they need the whole document), then the edits page by page.
    let mut ids: Vec<(usize, Vec<(String, String)>)> = Vec::new();
    for (pi, add) in &adds {
        let pairs = add.iter().map(|_| (doc.next_id("s"), doc.next_id("c"))).collect();
        ids.push((*pi, pairs));
    }
    let keys: HashMap<&str, bool> = doc.model.elements.iter().map(|e| (e.id.as_str(), is_key(e))).collect();
    for (pi, page) in doc.pages.iter_mut().enumerate() {
        let want = wanted.get(pi).cloned().unwrap_or_default();
        let before = page.shapes.len();
        page.shapes.retain(|s| match (s.ext.get(EXT_SATELLITE_OF), s.element.as_deref()) {
            (Some(owner), Some(e)) => want.contains(&(owner.clone(), e.to_string())),
            (Some(_), None) => false,
            _ => true,
        });
        let removed = page.shapes.len() != before;
        if removed || page.connectors.iter().any(|c| c.ext.contains_key(EXT_SATELLITE_OF)) {
            let sats: HashSet<&str> = page.shapes.iter().filter(|s| s.ext.contains_key(EXT_SATELLITE_OF)).map(|s| s.id.as_str()).collect();
            page.connectors.retain(|c| !c.ext.contains_key(EXT_SATELLITE_OF) || c.end.shape().is_some_and(|s| sats.contains(s)));
        }
        for s in page.shapes.iter_mut().filter(|s| s.ext.contains_key(EXT_SATELLITE_OF)) {
            let key = s.element.as_deref().and_then(|e| keys.get(e)).copied().unwrap_or(false);
            if s.style.underline != key {
                s.style.underline = key;
            }
        }
        let (Some((_, add)), Some((_, pairs))) = (adds.iter().find(|(p, _)| *p == pi), ids.iter().find(|(p, _)| *p == pi)) else { continue };
        let variant = page.view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
        for ((owner_id, e, i, n), (sid, cid)) in add.iter().zip(pairs) {
            let Some(owner) = page.shape(owner_id) else { continue };
            let sat = new_satellite(cat, &variant, sid.clone(), owner, e, *i, *n);
            page.shapes.push(sat);
            page.connectors.push(new_line(cid.clone(), owner_id, sid));
        }
    }
}
