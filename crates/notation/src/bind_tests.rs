use vizcraft_diagram::{Color, Connector, Document, End, Page, Point, Style};
use vizcraft_model::{Element, PropValue};

use super::*;
use crate::catalog;
use crate::rules::test_doc::{Doc, free_shape, glued, glyph_shape};

/// Sync `doc` against `before` and check a second run changes nothing.
fn synced(mut doc: Document, before: &Document) -> Document {
    sync(&mut doc, before, catalog());
    let once = doc.clone();
    sync(&mut doc, before, catalog());
    assert_eq!(doc, once, "sync is not idempotent");
    doc
}

fn glyph_fill(type_id: &str) -> (Color, Color) {
    let s = glyph_shape("x", type_id, 0.0, 0.0);
    (s.style.fill, s.style.line)
}

/// A Cascade 1 style document: free shapes, free and glued connectors, styles and text.
fn cascade1() -> Document {
    let mut d = Document::new();
    d.title = "Old".into();
    let mut a = free_shape("s1", 0.0);
    a.text = "Start".into();
    a.style.fill = Color::rgb(1, 2, 3);
    d.pages[0].shapes.push(a);
    d.pages[0].shapes.push(free_shape("s2", 200.0));
    let mut c = glued("c1", "s1", "s2");
    c.text = "go".into();
    d.pages[0].connectors.push(c);
    d.pages[0].connectors.push(Connector::new("c2", End::point("s1", 1), End::free(Point::new(9.0, 9.0))));
    d.pages.push(Page::new("p2", "Two"));
    d.id_seq = 7;
    d
}

#[test]
fn free_drawing_passes_through_byte_identical() {
    let before = cascade1();
    let mut doc = before.clone();
    doc.pages[0].shapes[1].x = 50.0;
    let expect = serde_json::to_string(&doc).unwrap();
    let out = synced(doc, &before);
    assert_eq!(serde_json::to_string(&out).unwrap(), expect);
    assert!(out.model.is_empty());
}

#[test]
fn free_items_in_a_model_document_are_untouched() {
    let mut b = Doc::new();
    b.el("a", "Process", "A");
    let mut f = free_shape("f1", 900.0);
    f.text = "note".into();
    f.style.fill = Color::rgb(9, 9, 9);
    b.doc.pages[0].shapes.push(f.clone());
    // Free connector between a bound and a free shape, and one with a free end.
    b.doc.pages[0].connectors.push(glued("k1", "s_a", "f1"));
    b.doc.pages[0].connectors.push(Connector::new("k2", End::dynamic("s_a"), End::free(Point::new(1.0, 1.0))));
    let before = b.build();
    let out = synced(before.clone(), &before);
    assert_eq!(out.pages[0].shape("f1"), Some(&f));
    assert_eq!(out.pages[0].connector("k1"), before.pages[0].connector("k1"));
    assert_eq!(out.pages[0].connector("k2"), before.pages[0].connector("k2"));
    assert!(out.model.relationships.is_empty());
}

#[test]
fn step1_dangling_references_become_free() {
    let mut b = Doc::new();
    b.el("a", "Process", "A");
    b.doc.pages[0].shapes.push({
        let mut s = free_shape("ghost", 0.0);
        s.element = Some("gone".into());
        s
    });
    let mut c = glued("cg", "s_a", "ghost");
    c.relationship = Some("rgone".into());
    b.doc.pages[0].connectors.push(c);
    let before = b.build();
    let out = synced(before.clone(), &before);
    assert_eq!(out.pages[0].shape("ghost").unwrap().element, None);
    assert_eq!(out.pages[0].connector("cg").unwrap().relationship, None);
}

#[test]
fn step2_a_second_view_on_one_page_is_a_copy() {
    let mut b = Doc::new();
    b.el("a", "Process", "Pack box").tag("a", "S1").prop("a", "owner", PropValue::Text("Ops".into()));
    let before = b.build();
    let mut doc = before.clone();
    let mut dup = doc.pages[0].shapes[0].clone();
    dup.id = "s_dup".into();
    // Put the copy behind the original: the shape that drew the element before keeps it.
    doc.pages[0].shapes.insert(0, dup);
    let out = synced(doc, &before);
    assert_eq!(out.pages[0].shape("s_a").unwrap().element.as_deref(), Some("a"));
    let new_id = out.pages[0].shape("s_dup").unwrap().element.clone().unwrap();
    assert_ne!(new_id, "a");
    assert!(new_id.starts_with('e'));
    let copy = out.model.element(&new_id).unwrap();
    assert_eq!((copy.type_id.as_str(), copy.name.as_str(), copy.tag.as_str()), ("flowchart.Process", "Pack box", "S2"));
    assert_eq!(copy.prop("owner"), Some(&PropValue::Text("Ops".into())));

    // A scope that does not number automatically leaves the copy without a tag.
    let mut b = Doc::new();
    b.el("o", "OffPageReference", "").tag("o", "4");
    let before = b.build();
    let mut doc = before.clone();
    let mut dup = doc.pages[0].shapes[0].clone();
    dup.id = "s_o2".into();
    doc.pages[0].shapes.push(dup);
    let out = synced(doc, &before);
    let e = out.pages[0].shape("s_o2").unwrap().element.clone().unwrap();
    assert_eq!(out.model.element(&e).unwrap().tag, "");

    // Views on two pages are one element, not copies.
    let mut b = Doc::new();
    b.el("a", "Process", "A");
    let mut doc = b.build();
    let mut other = doc.pages[0].shapes[0].clone();
    other.id = "s_a2".into();
    doc.pages.push(Page::new("p2", "Two"));
    doc.pages[1].shapes.push(other);
    let out = synced(doc.clone(), &doc);
    assert_eq!(out.pages[1].shapes[0].element.as_deref(), Some("a"));
    assert_eq!(out.model.elements.len(), 1);
}

#[test]
fn step3_glued_connectors_get_relationships() {
    let mut b = Doc::new();
    b.el("a", "Database", "A").el("b", "Database", "B").el("c", "Process", "C");
    let before = b.build();
    let mut doc = before.clone();
    let mut c = glued("k", "s_a", "s_b");
    c.text = "copy".into();
    doc.pages[0].connectors.push(c);
    let out = synced(doc, &before);
    let rid = out.pages[0].connector("k").unwrap().relationship.clone().unwrap();
    assert!(rid.starts_with('r'));
    let r = out.model.relationship(&rid).unwrap();
    // Illegal, but bound so the rules engine can report it.
    assert_eq!((r.type_id.as_str(), r.source.as_str(), r.target.as_str(), r.name.as_str()), ("flowchart.Flow", "a", "b", "copy"));
    assert_eq!(crate::rules::live(&out, catalog())[0].rule, "flowchart.illegalFlow");

    // Reglued: the relationship follows.
    let before = out.clone();
    let mut doc = out;
    doc.pages[0].connector_mut("k").unwrap().end = End::dynamic("s_c");
    let out = synced(doc, &before);
    let r = out.model.relationship(&rid).unwrap();
    assert_eq!((r.source.as_str(), r.target.as_str()), ("a", "c"));

    // A free end keeps the relationship as it was.
    let before = out.clone();
    let mut doc = out;
    doc.pages[0].connector_mut("k").unwrap().end = End::free(Point::new(3.0, 3.0));
    let out = synced(doc, &before);
    let r = out.model.relationship(&rid).unwrap();
    assert_eq!((r.source.as_str(), r.target.as_str()), ("a", "c"));
    assert_eq!(out.pages[0].connector("k").unwrap().relationship.as_deref(), Some(rid.as_str()));
}

#[test]
fn step3_a_duplicated_connector_between_other_elements_gets_its_own_relationship() {
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("b", "Process", "B").el("c", "Process", "C").flow("r1", "a", "b");
    let before = b.build();
    let mut doc = before.clone();
    let mut dup = doc.pages[0].connectors[0].clone();
    dup.id = "c_dup".into();
    dup.end = End::dynamic("s_c");
    // Earlier in z order than the original: the connector that matches the relationship keeps it.
    doc.pages[0].connectors.insert(0, dup);
    let out = synced(doc, &before);
    let r1 = out.model.relationship("r1").unwrap();
    assert_eq!((r1.source.as_str(), r1.target.as_str()), ("a", "b"));
    let rid = out.pages[0].connector("c_dup").unwrap().relationship.clone().unwrap();
    assert_ne!(rid, "r1");
    let r = out.model.relationship(&rid).unwrap();
    assert_eq!((r.type_id.as_str(), r.source.as_str(), r.target.as_str()), ("flowchart.Flow", "a", "c"));
}

#[test]
fn step3_ports_come_from_connection_points() {
    let pack: crate::Pack = serde_json::from_value(serde_json::json!({
        "id": "t", "version": "1.0.0",
        "elementTypes": [
            {"id": "t.Block", "ports": [{"id": "in", "x": 0.0, "y": 0.5}, {"id": "out", "x": 1.0, "y": 0.5}]},
            {"id": "t.Plain"}
        ],
        "relationshipTypes": [{"id": "t.Wire"}],
        "connections": [{"source": ["*"], "relationship": "t.Wire", "target": ["*"]}]
    }))
    .unwrap();
    let cat = Catalog::new(vec![pack]);
    let mut doc = Document::new();
    for (id, ty) in [("a", "t.Block"), ("b", "t.Block"), ("p", "t.Plain")] {
        doc.model.add_element(Element::new(id, ty)).unwrap();
        let mut s = free_shape(&format!("s_{id}"), 0.0);
        s.element = Some(id.into());
        doc.pages[0].shapes.push(s);
    }
    doc.id_seq = 100;
    let before = doc.clone();
    doc.pages[0].connectors.push(Connector::new("k1", End::point("s_a", 1), End::point("s_b", 0)));
    doc.pages[0].connectors.push(Connector::new("k2", End::dynamic("s_a"), End::point("s_b", 3)));
    doc.pages[0].connectors.push(Connector::new("k3", End::point("s_p", 1), End::point("s_a", 0)));
    sync(&mut doc, &before, &cat);
    let rel = |k: &str| doc.model.relationship(doc.pages[0].connector(k).unwrap().relationship.as_deref().unwrap()).unwrap().clone();
    let r = rel("k1");
    assert_eq!((r.type_id.as_str(), r.source_port.as_deref(), r.target_port.as_deref()), ("t.Wire", Some("out"), Some("in")));
    let r = rel("k2");
    assert_eq!((r.source_port, r.target_port), (None, None), "dynamic glue and an index past the ports");
    let r = rel("k3");
    assert_eq!((r.source_port.as_deref(), r.target_port.as_deref()), (None, Some("in")), "a type with no ports");
    let once = doc.clone();
    sync(&mut doc, &before, &cat);
    assert_eq!(doc, once);
    assert_eq!(doc.model.packs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["t"]);
}

/// Two Flowchart Process shapes (a legal Flow pair) and a connector of `kind` between them.
fn kind_doc(kind: &str) -> (Document, Document) {
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("b", "Process", "B");
    let before = b.build();
    let mut doc = before.clone();
    let mut c = glued("k", "s_a", "s_b");
    c.kind = kind.into();
    doc.pages[0].connectors.push(c);
    (doc, before)
}

fn relationship_type_of(doc: &Document) -> String {
    let rid = doc.pages[0].connector("k").unwrap().relationship.clone().unwrap();
    doc.model.relationship(&rid).unwrap().type_id.clone()
}

#[test]
fn step3_the_flow_kind_between_flowchart_shapes_creates_a_flow() {
    let (doc, before) = kind_doc("flowchart.flow");
    let out = synced(doc, &before);
    assert_eq!(relationship_type_of(&out), "flowchart.Flow");
    // A kind without a relationship type, and an unknown kind, leave the choice to the table.
    for kind in ["basic.arrow", "vendor.nothing", ""] {
        let (doc, before) = kind_doc(kind);
        assert_eq!(relationship_type_of(&synced(doc, &before)), "flowchart.Flow", "{kind}");
    }
}

#[test]
fn step3_a_kinds_relationship_type_beats_the_connection_table() {
    let pack: crate::Pack = serde_json::from_value(serde_json::json!({
        "id": "t", "version": "1.0.0",
        "elementTypes": [{"id": "t.Node"}],
        "relationshipTypes": [{"id": "t.Wire"}, {"id": "t.Pipe"}],
        "connections": [{"source": ["*"], "relationship": "t.Wire", "target": ["*"]}],
        "connectorTypes": [
            {"id": "t.pipe", "relationshipType": "t.Pipe"},
            {"id": "t.ghost", "relationshipType": "t.Ghost"},
            {"id": "t.plain"}
        ]
    }))
    .unwrap();
    let cat = Catalog::new(vec![pack]);
    let relationship = |kind: &str| {
        let mut doc = Document::new();
        for id in ["a", "b"] {
            doc.model.add_element(Element::new(id, "t.Node")).unwrap();
            let mut s = free_shape(&format!("s_{id}"), 0.0);
            s.element = Some(id.into());
            doc.pages[0].shapes.push(s);
        }
        doc.id_seq = 100;
        let before = doc.clone();
        let mut c = glued("k", "s_a", "s_b");
        c.kind = kind.into();
        doc.pages[0].connectors.push(c);
        sync(&mut doc, &before, &cat);
        let rid = doc.pages[0].connector("k").unwrap().relationship.clone().unwrap();
        doc.model.relationship(&rid).unwrap().type_id.clone()
    };
    assert_eq!(relationship("t.pipe"), "t.Pipe");
    // A kind naming a relationship type the catalog lacks, or none, falls back to the table.
    assert_eq!(relationship("t.ghost"), "t.Wire");
    assert_eq!(relationship("t.plain"), "t.Wire");
    assert_eq!(relationship(""), "t.Wire");
}

#[test]
fn step3_a_kind_names_an_illegal_pair_and_the_rule_reports_it() {
    let mut b = Doc::new();
    b.el("a", "Database", "A").el("b", "Database", "B");
    let before = b.build();
    let mut doc = before.clone();
    let mut c = glued("k", "s_a", "s_b");
    c.kind = "flowchart.flow".into();
    doc.pages[0].connectors.push(c);
    let out = synced(doc, &before);
    assert_eq!(relationship_type_of(&out), "flowchart.Flow");
    assert_eq!(crate::rules::live(&out, catalog())[0].rule, "flowchart.illegalFlow");
}

#[test]
fn step4_deleting_the_last_connector_deletes_the_relationship() {
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("b", "Process", "B").el("c", "Process", "C").flow("r1", "a", "b").rel("r2", "b", "c");
    let before = b.build();
    let mut doc = before.clone();
    doc.remove_connector("c_r1");
    let out = synced(doc, &before);
    assert!(out.model.relationship("r1").is_none());
    // Never drawn, so kept.
    assert!(out.model.relationship("r2").is_some());

    // Reconnected in the same command (as `relationship.reconnect` does when the new end is not on
    // the connector's page): the relationship stays though its last view went.
    let mut doc = before.clone();
    doc.remove_connector("c_r1");
    doc.model.relationship_mut("r1").unwrap().target = "c".into();
    let out = synced(doc, &before);
    assert_eq!(out.model.relationship("r1").map(|r| r.target.as_str()), Some("c"));

    // A second view keeps it alive.
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("b", "Process", "B").flow("r1", "a", "b");
    let mut c = glued("c_two", "s_a", "s_b");
    c.relationship = Some("r1".into());
    b.doc.pages[0].connectors.push(c);
    let before = b.build();
    let mut doc = before.clone();
    doc.remove_connector("c_r1");
    assert!(synced(doc, &before).model.relationship("r1").is_some());
}

#[test]
fn step5_text_and_names_agree() {
    let mut b = Doc::new();
    b.el("a", "Process", "Old").el("b", "Process", "B").flow("r1", "a", "b");
    let mut second = b.doc.pages[0].shapes[0].clone();
    second.id = "s_a_p2".into();
    b.doc.pages.push(Page::new("p2", "Two"));
    b.doc.pages[1].shapes.push(second);
    let before = b.build();

    // Text edited on one view: the name and the other view follow.
    let mut doc = before.clone();
    doc.pages[0].shape_mut("s_a").unwrap().text = "New".into();
    doc.pages[0].connector_mut("c_r1").unwrap().text = "yes".into();
    let out = synced(doc, &before);
    assert_eq!(out.model.element("a").unwrap().name, "New");
    assert_eq!(out.pages[1].shape("s_a_p2").unwrap().text, "New");
    assert_eq!(out.model.relationship("r1").unwrap().name, "yes");

    // Name changed in the model: every view follows.
    let mut doc = before.clone();
    doc.model.element_mut("a").unwrap().name = "Renamed".into();
    doc.model.relationship_mut("r1").unwrap().name = "no".into();
    let out = synced(doc, &before);
    assert_eq!(out.pages[0].shape("s_a").unwrap().text, "Renamed");
    assert_eq!(out.pages[1].shape("s_a_p2").unwrap().text, "Renamed");
    assert_eq!(out.pages[0].connector("c_r1").unwrap().text, "no");

    // A new view with no text shows the existing name rather than erasing it.
    let mut doc = before.clone();
    let mut fresh = glyph_shape("s_new", "flowchart.Process", 0.0, 400.0);
    fresh.element = Some("b".into());
    doc.pages.push(Page::new("p3", "Three"));
    doc.pages[2].shapes.push(fresh);
    let out = synced(doc, &before);
    assert_eq!(out.model.element("b").unwrap().name, "B");
    assert_eq!(out.pages[2].shape("s_new").unwrap().text, "B");

    // A new element with no name takes its view's text.
    let mut doc = before.clone();
    doc.model.add_element(Element::new("n", "flowchart.Process")).unwrap();
    let mut s = glyph_shape("s_n", "flowchart.Process", 0.0, 600.0);
    s.element = Some("n".into());
    s.text = "Typed".into();
    doc.pages[0].shapes.push(s);
    assert_eq!(synced(doc, &before).model.element("n").unwrap().name, "Typed");
}

#[test]
fn step6_conditional_style_applies_and_reverts() {
    let mut b = Doc::new();
    b.el("p", "Process", "P").el("d", "Decision", "D");
    let before = b.build();
    let (fill, line) = glyph_fill("flowchart.Process");

    let mut doc = before.clone();
    doc.model.element_mut("p").unwrap().properties.insert("automated".into(), PropValue::Bool(true));
    let out = synced(doc, &before);
    let st = &out.pages[0].shape("s_p").unwrap().style;
    assert_eq!((st.fill, st.line), (Color::parse("#cfe8dd").unwrap(), Color::parse("#1d4f3f").unwrap()));
    // Fields no style names are not controlled.
    assert_eq!(st.font_color, Style::DEFAULT.font_color);

    let before2 = out.clone();
    let mut doc = out;
    doc.model.element_mut("p").unwrap().properties.insert("automated".into(), PropValue::Bool(false));
    let out = synced(doc, &before2);
    let st = &out.pages[0].shape("s_p").unwrap().style;
    assert_eq!((st.fill, st.line), (fill, line));

    // Hand styling sticks: a later command that leaves the match alone does not touch it.
    let mut doc = before.clone();
    doc.pages[0].shape_mut("s_p").unwrap().style.fill = Color::rgb(1, 1, 1);
    let styled = synced(doc, &before);
    assert_eq!(styled.pages[0].shape("s_p").unwrap().style.fill, Color::rgb(1, 1, 1));
    let mut doc = styled.clone();
    doc.pages[0].shape_mut("s_p").unwrap().x += 40.0;
    doc.model.element_mut("p").unwrap().properties.insert("owner".into(), PropValue::Text("Ops".into()));
    let out = synced(doc, &styled);
    assert_eq!(out.pages[0].shape("s_p").unwrap().style.fill, Color::rgb(1, 1, 1));

    // A match change still overrides it, and turning it off brings back the glyph value.
    let mut doc = out.clone();
    doc.model.element_mut("p").unwrap().properties.insert("automated".into(), PropValue::Bool(true));
    let on = synced(doc, &out);
    assert_eq!(on.pages[0].shape("s_p").unwrap().style.fill, Color::parse("#cfe8dd").unwrap());
    let mut doc = on.clone();
    doc.model.element_mut("p").unwrap().properties.remove("automated");
    let off = synced(doc, &on);
    assert_eq!(off.pages[0].shape("s_p").unwrap().style.fill, fill);

    // A new element that matches gets the style once; a new view of a matching element too.
    let mut doc = before.clone();
    let mut e = Element::new("n", "flowchart.Process");
    e.properties.insert("automated".into(), PropValue::Bool(true));
    doc.model.add_element(e).unwrap();
    let mut s = glyph_shape("s_n", "flowchart.Process", 0.0, 300.0);
    s.element = Some("n".into());
    doc.pages[0].shapes.push(s);
    let out = synced(doc, &before);
    assert_eq!(out.pages[0].shape("s_n").unwrap().style.line, Color::parse("#1d4f3f").unwrap());
    let mut doc = on.clone();
    let mut view = glyph_shape("s_p2", "flowchart.Process", 0.0, 0.0);
    view.element = Some("p".into());
    doc.pages.push(Page::new("p2", "Two"));
    doc.pages[1].shapes.push(view);
    let out = synced(doc, &on);
    assert_eq!(out.pages[1].shape("s_p2").unwrap().style.fill, Color::parse("#cfe8dd").unwrap());
    // A new element that matches nothing keeps its glyph style, even a hand-set one.
    let mut doc = before.clone();
    doc.model.add_element(Element::new("m", "flowchart.Process")).unwrap();
    let mut s = glyph_shape("s_m", "flowchart.Process", 0.0, 500.0);
    s.element = Some("m".into());
    s.style.fill = Color::rgb(3, 3, 3);
    doc.pages[0].shapes.push(s);
    assert_eq!(synced(doc, &before).pages[0].shape("s_m").unwrap().style.fill, Color::rgb(3, 3, 3));
}

#[test]
fn step7_packs_in_use_are_recorded() {
    let mut b = Doc::new();
    b.el("p", "Process", "P");
    let before = b.build();
    assert!(before.model.packs.is_empty());
    let out = synced(before.clone(), &before);
    assert_eq!(out.model.packs, [catalog().pack_ref("flowchart").unwrap()]);
    // Unknown packs are not recorded.
    let mut doc = before.clone();
    doc.model.add_element(Element::new("z", "zzz.Thing")).unwrap();
    assert_eq!(synced(doc, &before).model.packs.len(), 1);
}

#[test]
fn a_full_document_syncs_idempotently() {
    let mut b = Doc::new();
    b.el("t", "Terminator", "Start")
        .el("a", "Process", "A")
        .el("d", "Decision", "D")
        .el("x", "Database", "X")
        .el("y", "Database", "Y")
        .flow("r1", "t", "a")
        .flow("r2", "a", "d")
        .tag("a", "S1")
        .tag("d", "S2");
    b.doc.pages[0].shapes.push(free_shape("free", 1000.0));
    let before = b.build();
    let mut doc = before.clone();
    let mut dup = doc.pages[0].shape("s_a").unwrap().clone();
    dup.id = "s_a_copy".into();
    doc.pages[0].shapes.push(dup);
    doc.pages[0].connectors.push(glued("k1", "s_x", "s_y"));
    doc.pages[0].connectors.push(glued("k2", "s_a_copy", "s_d"));
    doc.remove_connector("c_r1");
    doc.model.element_mut("a").unwrap().properties.insert("automated".into(), PropValue::Bool(true));
    let out = synced(doc, &before);
    assert!(out.model.relationship("r1").is_none());
    assert_eq!(out.model.relationships.len(), 3);
    let copy = out.pages[0].shape("s_a_copy").unwrap().element.clone().unwrap();
    assert_eq!(out.model.element(&copy).unwrap().tag, "S3");
    let k2 = out.model.relationship(out.pages[0].connector("k2").unwrap().relationship.as_deref().unwrap()).unwrap();
    assert_eq!(k2.source, copy);
    // Relationship ends always name elements.
    for r in &out.model.relationships {
        assert!(out.model.element(&r.source).is_some() && out.model.element(&r.target).is_some());
    }
}

/// A test pack whose `test.Tag` views carry a list of fields filled from properties.
fn tag_catalog() -> crate::Catalog {
    use crate::schema::{CompartmentDecl, ElementType, Pack, PropertySchema};
    use vizcraft_diagram::CompartmentKind;
    let fields = CompartmentDecl {
        id: "fields".into(),
        kind: CompartmentKind::List,
        title: Some("Fields".into()),
        from_properties: vec!["flow".into(), "service".into(), "head".into(), "missing".into()],
    };
    let tag = ElementType {
        id: "test.Tag".into(),
        properties: vec![
            PropertySchema { key: "flow".into(), label: "Flow".into(), ..PropertySchema::default() },
            PropertySchema { key: "head".into(), default: Some(PropValue::Number(30.0)), ..PropertySchema::default() },
        ],
        compartments: vec![fields, CompartmentDecl { id: "notes".into(), ..CompartmentDecl::default() }],
        ..ElementType::default()
    };
    crate::Catalog::new(vec![Pack { id: "test".into(), element_types: vec![tag], ..Pack::default() }])
}

#[test]
fn declared_list_compartments_fill_rows_from_properties() {
    let cat = tag_catalog();
    let mut doc = Document::new();
    let mut e = Element::new("e1", "test.Tag");
    e.name = "P-101".into();
    e.properties.insert("flow".into(), PropValue::Text("12 m3/h".into()));
    e.properties.insert("service".into(), PropValue::Text("feed".into()));
    doc.model.add_element(e).unwrap();
    let mut s = free_shape("s1", 0.0);
    s.element = Some("e1".into());
    s.h = 20.0;
    doc.pages[0].shapes.push(s);
    let before = doc.clone();
    sync(&mut doc, &before, &cat);
    let once = doc.clone();
    sync(&mut doc, &before, &cat);
    assert_eq!(doc, once, "sync is not idempotent");
    let s = &doc.pages[0].shapes[0];
    let ids: Vec<&str> = s.compartments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["fields", "notes"]);
    let fields = &s.compartments[0];
    assert_eq!(fields.title.as_deref(), Some("Fields"));
    // Label when the schema has one, else the key; the schema default fills a missing value.
    assert_eq!(fields.rows, vec!["Flow: 12 m3/h".to_string(), "service: feed".to_string(), "head: 30".to_string()]);
    assert!(s.h >= vizcraft_diagram::compartment::fit_height(s), "the shape grew to fit");
    // A property change refills the rows; a user's own compartment is left alone.
    let before = doc.clone();
    doc.model.element_mut("e1").unwrap().properties.insert("flow".into(), PropValue::Text("15 m3/h".into()));
    doc.pages[0].shapes[0].compartments.push(vizcraft_diagram::Compartment::new("mine", vizcraft_diagram::CompartmentKind::Paragraph));
    sync(&mut doc, &before, &cat);
    let s = &doc.pages[0].shapes[0];
    assert_eq!(s.compartments[0].rows[0], "Flow: 15 m3/h");
    assert_eq!(s.compartments.len(), 3);
    // The built-in packs declare none: a flowchart view gets no compartments.
    assert!(catalog().packs().iter().all(|p| p.element_types.iter().all(|t| t.compartments.is_empty())));
}
