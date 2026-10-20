//! The net pass of the sync step: keeps nets, wire segments and junctions consistent after every
//! document change. See [`nets`].

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::net::{Component, components, ends_on, is_junction, join};
use vizcraft_diagram::{Connector, Document, End};
use vizcraft_model::{Net, NetKind};

use crate::Catalog;

/// The connector types whose connectors are segments, by id, with their net kind.
fn net_kinds(cat: &Catalog) -> HashMap<&str, NetKind> {
    cat.packs().iter().flat_map(|p| p.connector_types.iter()).filter_map(|t| Some((t.id.as_str(), NetKind::from_name(t.net.as_deref()?)?))).collect()
}

/// Bring nets and segments into agreement, in this order. Does nothing when the document has
/// no segment and no net, so documents without wiring are untouched.
///
/// 1. A segment is a connector with `net` set, or one whose `kind` names a connector type that
///    declares a net kind. Segments are joined into components only through glue: the same
///    junction or the same pin (shape and connection point). Geometry never joins them.
/// 2. A component with no net yet gets a fresh one (`next_id("n")`) of the connector type's
///    kind. In a component holding one net id, every segment takes it.
/// 3. A component holding several ids, at most one of them named, folds into the named one, or
///    into the oldest (first in the model) when none is named; properties are unioned and the
///    first value wins. Two named nets meeting are not merged: the engine reports a live
///    `net.conflict` issue with a fix that runs `net.merge {force: true}`.
/// 4. An unnamed net spread over components that are no longer connected is split: the largest
///    component keeps the id, the others get fresh nets with the same kind, properties and label.
///    A named net may span components and pages.
/// 5. Unnamed nets with no segments are removed; named ones are kept.
/// 6. Junctions are tidied: one with nothing glued to it is deleted; with one connector it is
///    deleted and that end is left free where it was; with two connectors of the same net it is
///    healed away by joining them ([`join`]).
///
/// Running it again changes nothing.
pub fn nets(doc: &mut Document, _before: &Document, cat: &Catalog) {
    let kinds = net_kinds(cat);
    let is_segment = |c: &Connector| c.net.is_some() || (!c.kind.is_empty() && kinds.contains_key(c.kind.as_str()));
    let any_segment = doc.pages.iter().any(|p| p.connectors.iter().any(is_segment));
    if !any_segment && doc.model.nets.is_empty() {
        return;
    }
    let comps: Vec<Component> = doc.pages.iter().enumerate().flat_map(|(pi, p)| components(pi, p, is_segment)).collect();
    let comps = join_net_groups(doc, cat, comps);
    assign(doc, &comps, &kinds);
    split(doc, &comps);
    drop_unused(doc);
    tidy_junctions(doc);
}

/// The `netGroup` of the port at connection point `point` of shape `shape` on page `pi`: the
/// builder's port id there (else the type's port `point`), looked up on the element's type and
/// its supertypes. `None` for a port of its own.
pub fn net_group(doc: &Document, cat: &Catalog, pi: usize, shape: &str, point: usize) -> Option<String> {
    let s = doc.pages.get(pi)?.shape(shape)?;
    let e = doc.model.element(s.element.as_deref()?)?;
    let chain = crate::infer::chain_of(cat, &e.type_id);
    let built = s.glyph.as_ref().and_then(|g| g.ports.get(point)).cloned();
    let port_id = match built {
        Some(id) => id,
        None => chain.iter().find_map(|t| cat.element_type(t).and_then(|(_, et)| et.ports.get(point)).map(|p| p.id.clone()))?,
    };
    chain
        .iter()
        .filter_map(|t| cat.element_type(t).map(|(_, et)| et))
        .find_map(|et| et.ports.iter().find(|p| p.id == port_id))
        .map(|p| p.net_group.clone())
        .filter(|g| !g.is_empty())
}

/// Components on one page that touch pins of one shape in the same `netGroup` become one
/// component: a terminal passes its net from one side to the other.
fn join_net_groups(doc: &Document, cat: &Catalog, comps: Vec<Component>) -> Vec<Component> {
    let mut first: HashMap<(usize, String, String), usize> = HashMap::new();
    let mut parent: Vec<usize> = (0..comps.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        // Bounded by the number of components: each step moves strictly up the forest.
        for _ in 0..parent.len() {
            match parent.get(i) {
                Some(&p) if p != i => i = p,
                _ => break,
            }
        }
        i
    }
    let mut any = false;
    for (k, comp) in comps.iter().enumerate() {
        for (shape, point) in &comp.pins {
            let Some(group) = net_group(doc, cat, comp.page, shape, *point) else { continue };
            match first.get(&(comp.page, shape.clone(), group.clone())) {
                Some(&other) => {
                    let (a, b) = (root(&mut parent, other), root(&mut parent, k));
                    if a != b
                        && let Some(slot) = parent.get_mut(b)
                    {
                        *slot = a;
                        any = true;
                    }
                }
                None => {
                    first.insert((comp.page, shape.clone(), group), k);
                }
            }
        }
    }
    if !any {
        return comps;
    }
    let mut out: Vec<Component> = Vec::new();
    let mut at: HashMap<usize, usize> = HashMap::new();
    for k in 0..comps.len() {
        let r = root(&mut parent, k);
        let Some(comp) = comps.get(k) else { continue };
        match at.get(&r) {
            Some(&i) => {
                if let Some(m) = out.get_mut(i) {
                    m.segments.extend(comp.segments.iter().copied());
                    m.junctions.extend(comp.junctions.iter().cloned());
                    m.pins.extend(comp.pins.iter().cloned());
                }
            }
            None => {
                at.insert(r, out.len());
                out.push(comp.clone());
            }
        }
    }
    out
}

/// The net of segment `ci` on page `pi`.
fn net_at(doc: &Document, pi: usize, ci: usize) -> Option<&str> {
    doc.pages.get(pi)?.connectors.get(ci)?.net.as_deref()
}

/// Give every segment of `comp` the net `id`.
fn set_net(doc: &mut Document, comp: &Component, id: &str) {
    let Some(page) = doc.pages.get_mut(comp.page) else { return };
    for &ci in &comp.segments {
        if let Some(c) = page.connectors.get_mut(ci)
            && c.net.as_deref() != Some(id)
        {
            c.net = Some(id.to_string());
        }
    }
}

/// Steps 2 and 3: every component ends up with one net, unless two named nets meet in it.
fn assign(doc: &mut Document, comps: &[Component], kinds: &HashMap<&str, NetKind>) {
    // Indexed once: nets added below are new and unnamed, so neither map needs them.
    let order: HashMap<String, usize> = doc.model.nets.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
    let named_ids: HashSet<String> = doc.model.nets.iter().filter(|n| n.named()).map(|n| n.id.clone()).collect();
    for comp in comps {
        let mut ids: Vec<String> = comp.segments.iter().filter_map(|&ci| net_at(doc, comp.page, ci)).map(str::to_string).collect();
        ids.sort_by_key(|n| order.get(n).copied().unwrap_or(usize::MAX));
        ids.dedup();
        if ids.is_empty() {
            let kind = comp
                .segments
                .iter()
                .find_map(|&ci| doc.pages.get(comp.page)?.connectors.get(ci).and_then(|c| kinds.get(c.kind.as_str()).copied()))
                .unwrap_or_default();
            if doc.model.nets.len() >= vizcraft_model::net::MAX_NETS {
                continue;
            }
            let id = doc.next_id("n");
            doc.model.nets.push(Net::new(id.clone(), kind));
            set_net(doc, comp, &id);
            continue;
        }
        let named: Vec<&String> = ids.iter().filter(|n| named_ids.contains(n.as_str())).collect();
        let target = match named.as_slice() {
            [] => ids.first().cloned(),
            [one] => Some((*one).clone()),
            // Two named nets meet: nothing is merged; segments without a net join the first.
            [first, ..] => {
                let first = (*first).clone();
                let Some(page) = doc.pages.get_mut(comp.page) else { continue };
                for &ci in &comp.segments {
                    if let Some(c) = page.connectors.get_mut(ci)
                        && c.net.is_none()
                    {
                        c.net = Some(first.clone());
                    }
                }
                continue;
            }
        };
        let Some(target) = target else { continue };
        // Fold: the others' properties join the target's, first value wins.
        for other in ids.iter().filter(|n| **n != target) {
            let props = doc.model.net(other).map(|n| n.properties.clone()).unwrap_or_default();
            if let Some(t) = doc.model.net_mut(&target) {
                for (k, v) in props {
                    if t.properties.len() >= vizcraft_model::MAX_PROPERTIES {
                        break;
                    }
                    t.properties.entry(k).or_insert(v);
                }
            }
        }
        set_net(doc, comp, &target);
    }
}

/// Step 4: an unnamed net over several components keeps the largest; the rest get copies.
fn split(doc: &mut Document, comps: &[Component]) {
    let mut by_net: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, comp) in comps.iter().enumerate() {
        let ids: HashSet<&str> = comp.segments.iter().filter_map(|&ci| net_at(doc, comp.page, ci)).collect();
        // A component with two nets in it is a conflict between named nets; leave it alone.
        if ids.len() == 1
            && let Some(id) = ids.into_iter().next()
        {
            by_net.entry(id.to_string()).or_default().push(k);
        }
    }
    let mut plan: Vec<(String, Vec<usize>)> =
        by_net.into_iter().filter(|(id, ks)| ks.len() > 1 && doc.model.net(id).is_some_and(|n| !n.named())).collect();
    plan.sort();
    for (id, ks) in plan {
        let keep = ks.iter().copied().fold(None::<usize>, |best, k| match best {
            Some(b) if comps.get(b).map_or(0, |c| c.segments.len()) >= comps.get(k).map_or(0, |c| c.segments.len()) => Some(b),
            _ => Some(k),
        });
        let Some(template) = doc.model.net(&id).cloned() else { continue };
        for k in ks.into_iter().filter(|k| Some(*k) != keep) {
            if doc.model.nets.len() >= vizcraft_model::net::MAX_NETS {
                break;
            }
            let fresh = doc.next_id("n");
            doc.model.nets.push(Net { id: fresh.clone(), ..template.clone() });
            if let Some(comp) = comps.get(k) {
                set_net(doc, comp, &fresh);
            }
        }
    }
}

/// Step 5: unnamed nets no segment names any more go.
fn drop_unused(doc: &mut Document) {
    let used: HashSet<String> = doc.pages.iter().flat_map(|p| p.connectors.iter().filter_map(|c| c.net.clone())).collect();
    doc.model.nets.retain(|n| n.named() || used.contains(&n.id));
}

/// Step 6: junctions with fewer than three connectors on them go.
fn tidy_junctions(doc: &mut Document) {
    for page in &mut doc.pages {
        let junctions: Vec<String> = page.shapes.iter().filter(|s| is_junction(s)).map(|s| s.id.clone()).collect();
        for j in junctions {
            let ends = ends_on(page, &j);
            match ends.as_slice() {
                [] => {
                    page.remove_shape(&j);
                }
                [(c, at_start)] => {
                    let centre = page.shape(&j).map(|s| s.center());
                    if let (Some(conn), Some(p)) = (page.connector_mut(c), centre) {
                        let end = if *at_start { &mut conn.start } else { &mut conn.end };
                        *end = End::free(p);
                    }
                    page.remove_shape(&j);
                }
                [(a, _), (b, _)] if a != b => {
                    let same_net = page.connector(a).map(|c| c.net.clone()) == page.connector(b).map(|c| c.net.clone());
                    if same_net && join(page, a, b, &j) {
                        page.remove_shape(&j);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::net::split_at;
    use vizcraft_diagram::{Connector, Document, End, Point, Route, Shape, ShapeKind};
    use vizcraft_model::{Net, NetKind};

    use super::nets;

    fn wire(id: &str, a: End, b: End, net: Option<&str>) -> Connector {
        Connector { route: Route::Orthogonal, net: net.map(str::to_string), ..Connector::new(id, a, b) }
    }

    fn free(x: f64, y: f64) -> End {
        End::free(Point::new(x, y))
    }

    fn pass(doc: &mut Document) {
        let before = doc.clone();
        nets(doc, &before, crate::catalog());
        let once = doc.clone();
        nets(doc, &once, crate::catalog());
        assert_eq!(*doc, once, "a second pass changes nothing");
    }

    fn net_of(doc: &Document, c: &str) -> String {
        doc.connector(c).and_then(|c| c.net.clone()).unwrap_or_default()
    }

    #[test]
    fn documents_without_wiring_are_untouched() {
        let mut doc = Document::new();
        doc.pages[0].connectors.push(Connector::new("c", free(0.0, 0.0), free(10.0, 10.0)));
        let before = doc.clone();
        pass(&mut doc);
        assert_eq!(doc, before);
    }

    #[test]
    fn crossing_wires_stay_two_nets_and_a_shared_junction_makes_one() {
        let mut doc = Document::new();
        doc.model.nets.push(Net::new("n1", NetKind::Wire));
        doc.model.nets.push(Net::new("n2", NetKind::Wire));
        doc.pages[0].connectors.push(wire("h", free(0.0, 50.0), free(200.0, 50.0), Some("n1")));
        doc.pages[0].connectors.push(wire("v", free(100.0, 0.0), free(100.0, 200.0), Some("n2")));
        pass(&mut doc);
        assert_ne!(net_of(&doc, "h"), net_of(&doc, "v"));
        assert_eq!(doc.model.nets.len(), 2);
        // A junction on h with v's end on it joins them; the older net wins.
        split_at(&mut doc.pages[0], "h", Point::new(100.0, 50.0), "h2", "j1", true).unwrap();
        doc.pages[0].connector_mut("v").unwrap().start = End::point("j1", 0);
        pass(&mut doc);
        assert_eq!((net_of(&doc, "h"), net_of(&doc, "h2"), net_of(&doc, "v")), ("n1".into(), "n1".into(), "n1".into()));
        assert_eq!(doc.model.nets.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"], "the emptied unnamed net goes");
    }

    #[test]
    fn two_named_nets_meeting_are_not_merged() {
        let mut doc = Document::new();
        doc.model.nets.push(Net { name: "L1".into(), ..Net::new("n1", NetKind::Potential) });
        doc.model.nets.push(Net { name: "L2".into(), ..Net::new("n2", NetKind::Potential) });
        let mut r = Shape::new("r1", ShapeKind::Rectangle, 100.0, 100.0, 40.0, 20.0);
        r.connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
        doc.pages[0].shapes.push(r);
        doc.pages[0].connectors.push(wire("a", End::point("r1", 0), free(0.0, 110.0), Some("n1")));
        doc.pages[0].connectors.push(wire("b", End::point("r1", 0), free(0.0, 300.0), Some("n2")));
        pass(&mut doc);
        assert_eq!((net_of(&doc, "a"), net_of(&doc, "b")), ("n1".into(), "n2".into()));
        assert_eq!(vizcraft_diagram::net::NetIndex::build(&doc).conflicts.len(), 1);
        // A named net and an unnamed one fold into the named one, properties unioned.
        doc.model.net_mut("n2").unwrap().name.clear();
        doc.model.net_mut("n2").unwrap().properties.insert("gauge".into(), vizcraft_model::PropValue::Text("1.5".into()));
        pass(&mut doc);
        assert_eq!(net_of(&doc, "b"), "n1");
        assert_eq!(doc.model.nets.len(), 1);
        assert!(doc.model.net("n1").unwrap().properties.contains_key("gauge"));
    }

    #[test]
    fn deleting_a_middle_segment_splits_an_unnamed_net_and_heals_its_junctions() {
        // w1 and w4 meet w2 at j1; w2 meets w3 and w5 at j2. One net.
        let mut doc = Document::new();
        doc.model.nets.push(Net::new("n1", NetKind::Wire));
        let p = &mut doc.pages[0];
        p.connectors.push(wire("w1", free(0.0, 0.0), free(200.0, 0.0), Some("n1")));
        split_at(p, "w1", Point::new(100.0, 0.0), "w2", "j1", true).unwrap();
        split_at(p, "w2", Point::new(150.0, 0.0), "w3", "j2", true).unwrap();
        p.connectors.push(wire("w4", End::point("j1", 0), free(100.0, 100.0), Some("n1")));
        p.connectors.push(wire("w5", End::point("j2", 0), free(150.0, 100.0), Some("n1")));
        pass(&mut doc);
        assert_eq!(doc.pages[0].shapes.len(), 2, "both junctions have three wires");
        doc.pages[0].remove_connector("w2");
        pass(&mut doc);
        let p = &doc.pages[0];
        assert!(p.shapes.is_empty(), "each junction is left with two wires and heals away");
        assert_eq!(p.connectors.len(), 2, "w1 took w4 and w3 took w5");
        assert_ne!(net_of(&doc, "w1"), net_of(&doc, "w3"), "the halves are separate nets now");
        assert_eq!(doc.model.nets.len(), 2);
    }

    #[test]
    fn a_kind_that_declares_a_net_gives_its_connectors_one_and_lone_junctions_go() {
        let mut pack: crate::Pack = crate::loader::load_json(
            r#"{"id": "nettest", "version": "1.0.0", "name": "Net test", "connectorTypes": [{"id": "nettest.pipe", "label": "Pipe", "net": "pipe", "marker": "cross"}]}"#,
        )
        .unwrap();
        pack.module = vizcraft_model::Module::Iso2_5mm;
        let cat = crate::Catalog::new(vec![pack]);
        let mut doc = Document::new();
        doc.pages[0].connectors.push(Connector { kind: "nettest.pipe".into(), ..wire("p1", free(0.0, 0.0), free(100.0, 0.0), None) });
        doc.pages[0].shapes.push(vizcraft_diagram::net::junction("j9", Point::new(500.0, 500.0), true, 1.0, vizcraft_diagram::Color::BLACK));
        let before = doc.clone();
        nets(&mut doc, &before, &cat);
        let id = net_of(&doc, "p1");
        assert_eq!(doc.model.net(&id).map(|n| n.kind), Some(NetKind::Pipe));
        assert!(doc.pages[0].shapes.is_empty(), "a junction with nothing on it goes");
        let once = doc.clone();
        nets(&mut doc, &once, &cat);
        assert_eq!(doc, once);
    }
}
