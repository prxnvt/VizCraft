//! The ER commands, the four notations on the `er.schema` template, and DBML re-import.

use serde_json::{Value, json};
use vizcraft_diagram::compartment::CompartmentKind;
use vizcraft_diagram::connector_look::LabelFrame;
use vizcraft_diagram::{Arrow, Compound, Document, Point};
use vizcraft_model::PropValue;

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A session holding the `er.schema` template: customers, orders and order_items.
fn schema() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "er.schema"}));
    s
}

/// The shape on the first page that draws the element named `name`.
fn shape_of<'a>(s: &'a Session, name: &str) -> &'a vizcraft_diagram::Shape {
    let page = &s.doc.pages[0];
    let id = s.doc.model.elements.iter().find(|e| e.name == name && e.owner.is_none()).map(|e| e.id.clone()).unwrap();
    page.shapes.iter().find(|x| x.element.as_deref() == Some(id.as_str())).unwrap()
}

fn relationship_named<'a>(s: &'a Session, name: &str) -> &'a vizcraft_model::Relationship {
    s.doc.model.relationships.iter().find(|r| r.name == name).unwrap()
}

fn connector_of<'a>(s: &'a Session, rel: &str) -> &'a vizcraft_diagram::Connector {
    s.doc.pages[0].connectors.iter().find(|c| c.relationship.as_deref() == Some(rel)).unwrap()
}

const TABLES: [&str; 3] = ["customers", "orders", "order_items"];

fn centres(s: &Session) -> Vec<Point> {
    TABLES.iter().map(|t| shape_of(s, t).frame().center()).collect()
}

fn same_centres(a: &[Point], b: &[Point], why: &str) {
    for (p, q) in a.iter().zip(b) {
        assert!((p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6, "{why}: {p:?} moved to {q:?}");
    }
}

#[test]
fn the_schema_template_draws_tables_with_key_rows_and_crows_feet() {
    let mut s = schema();
    let customers = shape_of(&s, "customers");
    let keys = customers.compartments.iter().find(|c| c.id == "keys").unwrap();
    assert_eq!(keys.rows, ["PK\tid\tinteger"]);
    let attrs = customers.compartments.iter().find(|c| c.id == "attrs").unwrap();
    assert_eq!((attrs.kind, attrs.rows.len()), (CompartmentKind::List, 2));
    let places = relationship_named(&s, "places");
    let c = connector_of(&s, &places.id);
    assert_eq!((c.start_arrow, c.end_arrow), (Arrow::DoubleBar, Arrow::CrowsFootCircle));
    assert!(s.run("validate.run", &json!({})).unwrap()["issues"].as_array().unwrap().is_empty());
}

#[test]
fn variant_set_redraws_each_notation_in_one_step_keeping_centres() {
    let mut s = schema();
    let places = relationship_named(&s, "places").id.clone();
    let start = centres(&s);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/er");
    for (variant, file) in [("er.chen", "chen"), ("er.barker", "barker"), ("er.idef1x", "idef1x"), ("er.ie", "ie")] {
        let before = s.doc.clone();
        let was = centres(&s);
        run(&mut s, "variant.set", json!({"variant": variant}));
        same_centres(&was, &centres(&s), variant);
        let after = s.doc.clone();
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "{variant}: one undo step takes it back");
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(s.doc, after, "{variant}: redo");
        let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
        let path = dir.join(format!("{file}.golden.svg"));
        if std::env::var("VIZCRAFT_BLESS").is_ok() || std::env::var("VIZCRAFT_UPDATE_GOLDEN").is_ok() {
            std::fs::write(&path, &svg).unwrap();
        }
        assert_eq!(svg, std::fs::read_to_string(&path).unwrap_or_default(), "{variant}: SVG differs from {}", path.display());
        let c = connector_of(&s, &places).clone();
        let customers = shape_of(&s, "customers").clone();
        let satellites = s.doc.pages[0].shapes.iter().filter(|x| x.ext.contains_key(vizcraft_notation::satellites::EXT_SATELLITE_OF)).count();
        match variant {
            "er.chen" => {
                assert!(customers.compartments.is_empty(), "Chen drops the column bands");
                assert_eq!(satellites, 9, "one ellipse per column");
                let id_col = s.doc.pages[0].shapes.iter().find(|x| x.text == "id" && x.parent.as_deref() == Some(customers.id.as_str())).unwrap();
                assert!(id_col.style.underline && id_col.element.is_some(), "a bound, underlined key attribute");
                assert_eq!((c.start_label.as_str(), c.end_label.as_str(), c.label_frame), ("1", "N", LabelFrame::Diamond));
                assert!(c.halves.end.double && !c.halves.start.double, "orders take part totally");
            }
            "er.barker" => {
                assert_eq!(satellites, 0);
                let keys = customers.compartments.iter().find(|x| x.id == "keys").unwrap();
                assert_eq!(keys.rows, ["#\t*\tid"]);
                assert_eq!((c.start_arrow, c.end_arrow), (Arrow::None, Arrow::CrowsFoot));
                assert_eq!(c.halves.start.dash, Some(vizcraft_diagram::Dash::Dashed), "a customer may place no order");
                assert!(c.start_label.is_empty() && c.label_frame == LabelFrame::Box);
            }
            "er.idef1x" => {
                assert_eq!((c.end_arrow, c.end_label.as_str()), (Arrow::Dot, ""));
                let contains = connector_of(&s, &relationship_named(&s, "contains").id);
                assert_eq!((contains.end_arrow, contains.end_label.as_str()), (Arrow::Dot, "P"));
            }
            _ => {
                assert_eq!(satellites, 0, "leaving Chen removes the ellipses");
                assert_eq!(customers.compartments.len(), 2, "the column bands come back");
                assert_eq!((c.start_arrow, c.end_arrow), (Arrow::DoubleBar, Arrow::CrowsFootCircle));
                assert!(c.halves.is_plain() && c.start_label.is_empty() && c.end_label.is_empty());
            }
        }
    }
    same_centres(&start, &centres(&s), "the round trip");
    let ie = shape_of(&s, "customers");
    assert_eq!(ie.style.compound, Compound::Single);
}

#[test]
fn a_weak_entity_draws_a_double_outline_in_chen() {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "er.logical"}));
    run(&mut s, "variant.set", json!({"variant": "er.chen"}));
    assert_eq!(shape_of(&s, "Dependent").style.compound, Compound::Double);
    let supports = relationship_named(&s, "supports").id.clone();
    assert_eq!(connector_of(&s, &supports).label_frame, LabelFrame::DoubleDiamond);
    let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
    assert!(svg.matches("fill=\"#ffffff\" stroke").count() > 0);
}

#[test]
fn add_column_adds_an_owned_attribute_in_one_step_and_refuses_non_tables() {
    let mut s = schema();
    let customers = shape_of(&s, "customers").id.clone();
    let before = s.doc.clone();
    let r = run(&mut s, "er.addColumn", json!({"entity": customers, "name": "phone", "dataType": "varchar(20)", "unique": true}));
    let id = r["id"].as_str().unwrap().to_string();
    let e = s.doc.model.element(&id).unwrap();
    assert_eq!((e.type_id.as_str(), e.owner.clone()), ("er.Attribute", shape_of(&s, "customers").element.clone()));
    assert_eq!(e.prop("unique"), Some(&PropValue::Bool(true)));
    let attrs = shape_of(&s, "customers").compartments.iter().find(|c| c.id == "attrs").unwrap().rows.clone();
    assert!(attrs.contains(&"\tphone\tvarchar(20)".to_string()), "{attrs:?}");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc, before);
    let pk = run(&mut s, "er.addColumn", json!({"entity": customers, "name": "tenant", "pk": true}));
    assert_eq!(s.doc.model.element(pk["id"].as_str().unwrap()).unwrap().prop("nullable"), Some(&PropValue::Bool(false)));
    let column = id_of_column(&s, "customers", "email");
    for bad in [
        json!({"entity": column, "name": "x"}),
        json!({"entity": "nope", "name": "x"}),
        json!({"entity": customers}),
        json!({"entity": customers, "name": "  "}),
        json!({"entity": customers, "name": "x", "pk": "yes"}),
        json!({"entity": customers, "name": "x", "id": "bad id"}),
    ] {
        assert!(s.run("er.addColumn", &bad).is_err(), "{bad}");
    }
}

fn id_of_column(s: &Session, table: &str, column: &str) -> String {
    let t = s.doc.model.elements.iter().find(|e| e.name == table && e.owner.is_none()).unwrap().id.clone();
    s.doc.model.elements.iter().find(|e| e.name == column && e.owner.as_deref() == Some(t.as_str())).unwrap().id.clone()
}

#[test]
fn set_cardinality_changes_the_properties_and_the_ends_follow() {
    let mut s = schema();
    let places = relationship_named(&s, "places").id.clone();
    let conn = connector_of(&s, &places).id.clone();
    let r = run(&mut s, "er.setCardinality", json!({"ids": [conn], "target": "oneOrMore"}));
    assert_eq!(r["ids"], json!([places]));
    assert_eq!(s.doc.model.relationship(&places).unwrap().prop("targetCardinality"), Some(&PropValue::Text("oneOrMore".into())));
    assert_eq!(connector_of(&s, &places).end_arrow, Arrow::CrowsFootBar);
    run(&mut s, "er.setCardinality", json!({"ids": [places], "source": "zeroOrOne"}));
    assert_eq!(connector_of(&s, &places).start_arrow, Arrow::BarCircle);
    // From the selection.
    run(&mut s, "edit.select", json!({"ids": [conn]}));
    run(&mut s, "er.setCardinality", json!({"source": "one"}));
    assert_eq!(connector_of(&s, &places).start_arrow, Arrow::DoubleBar);
    for bad in [
        json!({"ids": [places]}),
        json!({"ids": [places], "source": "many"}),
        json!({"ids": ["nope"], "source": "one"}),
        json!({"ids": "x", "source": "one"}),
        json!({"ids": [shape_of(&s, "orders").id.clone()], "source": "one"}),
    ] {
        assert!(s.run("er.setCardinality", &bad).is_err(), "{bad}");
    }
}

#[test]
fn write_returns_dbml_and_mermaid_for_the_page() {
    let mut s = schema();
    let r = run(&mut s, "er.write", json!({"format": "dbml"}));
    let text = r["text"].as_str().unwrap();
    assert!(text.contains("Table customers {\n  id integer [pk, increment]\n"), "{text}");
    assert!(text.contains("Ref places: orders.customer_id > customers.id"), "{text}");
    assert!(text.contains("Ref contains: order_items.order_id > orders.id"), "{text}");
    let (back, warnings) = vizcraft_formats::dbml::read(text).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(back.tables.len(), 3);
    let m = run(&mut s, "er.write", json!({"format": "mermaid"}));
    let text = m["text"].as_str().unwrap();
    assert!(text.starts_with("erDiagram\n") && text.contains("customers ||..o{ orders : \"places\""), "{text}");
    let path = std::env::temp_dir().join(format!("vizcraft-er-write-{}.dbml", std::process::id()));
    let r = run(&mut s, "er.write", json!({"format": "dbml", "path": path.to_string_lossy()}));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), r["text"].as_str().unwrap());
    let _ = std::fs::remove_file(&path);
    assert!(s.run("er.write", &json!({"format": "sql"})).is_err());
    assert!(s.run("er.write", &json!({})).is_err());
    let mut empty = Session::new(Document::new());
    let r = run(&mut empty, "er.write", json!({"format": "dbml"}));
    assert_eq!(r["text"], "");
    assert!(!r["warnings"].as_array().unwrap().is_empty());
}

const SHOP: &str = "Table users {\n  id int [pk]\n  email varchar\n  legacy_code varchar\n}\n\nTable orders {\n  id int [pk]\n  user_id int [not null, ref: > users.id]\n  id_copy int\n}\n";

#[test]
fn dbml_merges_and_reimports_keeping_ids_and_geometry() {
    let path = std::env::temp_dir().join(format!("vizcraft-er-reimport-{}.dbml", std::process::id()));
    std::fs::write(&path, SHOP).unwrap();
    let mut s = Session::new(Document::new());
    let r = run(&mut s, "file.import", json!({"path": path.to_string_lossy(), "format": "dbml", "merge": true}));
    assert_eq!(r["added"], 8, "two tables and six columns: {r}");
    let users = s.doc.model.elements.iter().find(|e| e.name == "users").unwrap().clone();
    assert_eq!(users.type_id, "er.Entity");
    let email = id_of_column(&s, "users", "email");
    let orders_id = id_of_column(&s, "orders", "id");
    assert_ne!(id_of_column(&s, "users", "id"), orders_id, "each table owns its own id column");
    let rel = s.doc.model.relationships.first().unwrap().clone();
    assert_eq!((rel.type_id.as_str(), rel.source.as_str()), ("er.Relationship", users.id.as_str()));
    assert_eq!(rel.prop("targetCardinality"), Some(&PropValue::Text("zeroOrMore".into())));
    let frames: Vec<_> = s.doc.pages[0].shapes.iter().map(|x| (x.id.clone(), x.x, x.y)).collect();
    // The source changes one column's type and drops another.
    std::fs::write(&path, SHOP.replace("email varchar", "email text").replace("  legacy_code varchar\n", "")).unwrap();
    let r = run(&mut s, "file.reimport", json!({}));
    assert_eq!((r["added"].as_u64(), r["removed"].as_u64()), (Some(0), Some(1)), "{r}");
    assert!(r["updatedIds"].as_array().unwrap().contains(&json!(email)), "{r}");
    assert_eq!(s.doc.model.element(&email).unwrap().prop("dataType"), Some(&PropValue::Text("text".into())));
    assert_eq!(id_of_column(&s, "orders", "id"), orders_id, "ids kept");
    let now: Vec<_> = s.doc.pages[0].shapes.iter().map(|x| (x.id.clone(), x.x, x.y)).collect();
    assert_eq!(now, frames, "geometry kept");
    let issues = r["issues"].as_array().unwrap();
    assert!(issues.iter().any(|i| i["rule"] == "import.removed"), "{r}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn layout_primitive_er_puts_satellites_back_on_their_ring() {
    let mut s = Session::new(Document::new());
    run(&mut s, "template.new", json!({"template": "er.conceptual"}));
    let customer = shape_of(&s, "Customer").clone();
    let sat = s.doc.pages[0].shapes.iter().find(|x| x.parent.as_deref() == Some(customer.id.as_str())).unwrap().id.clone();
    run(&mut s, "shape.move", json!({"ids": [sat], "dx": 900, "dy": 900}));
    run(&mut s, "layout.primitive", json!({"primitive": "er"}));
    let c = shape_of(&s, "Customer").frame();
    let back = s.doc.pages[0].shape(&sat).unwrap().frame();
    assert!(back.center().distance(c.center()) < 300.0, "the satellite is next to its entity again");
    assert!(s.run("layout.primitive", &json!({"primitive": "er", "roles": {"satellites": 5}})).is_err());
}

#[test]
fn table_row_ids_line_up_with_the_rows_the_canvas_draws() {
    let s = schema();
    let cat = vizcraft_notation::catalog();
    for table in TABLES {
        let owner = s.doc.model.elements.iter().find(|e| e.name == table && e.owner.is_none()).unwrap();
        let mut checked = 0;
        for d in cat.compartments_in(&owner.type_id, "") {
            let Some(q) = d.query.as_ref() else { continue };
            let rows = vizcraft_notation::sheet_pass::table_rows(&s.doc, cat, owner, q);
            let ids = vizcraft_notation::sheet_pass::table_row_ids(&s.doc, cat, owner, q);
            assert_eq!(rows.len(), ids.len(), "{table}/{}", d.id);
            // The shape draws those very rows, in that order.
            let shape = shape_of(&s, table);
            let drawn = &shape.compartments.iter().find(|c| c.id == d.id).unwrap().rows;
            assert_eq!(&rows, drawn, "{table}/{}", d.id);
            // The first cell of an owned table is the attribute's name.
            for (row, id) in rows.iter().zip(&ids) {
                if q.owned && q.columns.contains(&"name".to_string()) {
                    let at = q.columns.iter().position(|c| c == "name").unwrap();
                    assert_eq!(row.split('\t').nth(at), Some(s.doc.model.element(id).unwrap().name.as_str()));
                }
            }
            checked += rows.len();
        }
        assert!(checked > 0, "{table} has table rows");
    }
}
