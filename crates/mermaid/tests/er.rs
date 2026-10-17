//! ER diagram parsing.

mod support;

use support::ok;
use vizcraft_mermaid::er::{Cardinality, ErDiagram, KeyKind};
use vizcraft_mermaid::{ArrowKind, Diagram, DiagramKind, Direction, LineStyle, Shape, parse};

fn er(text: &str) -> ErDiagram {
    match ok(text).diagram {
        Diagram::Er(d) => d,
        other => panic!("expected an ER diagram, got {:?}", other.kind()),
    }
}

#[test]
fn every_cardinality_symbol() {
    use Cardinality::*;
    let cases = [
        ("|o--o|", ZeroOrOne, ZeroOrOne),
        ("||--||", ExactlyOne, ExactlyOne),
        ("}o--o{", ZeroOrMore, ZeroOrMore),
        ("}|--|{", OneOrMore, OneOrMore),
        ("||--o{", ExactlyOne, ZeroOrMore),
        ("|o..|{", ZeroOrOne, OneOrMore),
    ];
    for (op, l, r) in cases {
        let d = er(&format!("erDiagram\nA {op} B : rel"));
        let rel = &d.relationships[0];
        assert_eq!((rel.left_cardinality, rel.right_cardinality), (l, r), "{op}");
        assert_eq!(rel.identifying, op.contains("--"), "{op}");
        assert_eq!(rel.label.as_deref(), Some("rel"));
    }
}

#[test]
fn relationships_without_spaces_labels_or_with_quotes() {
    let d = er("erDiagram\nA||--o{B : x\nC ||--|| D\nE }o..o{ F : \"many to many\"\n\"ENTITY ONE\" ||--|| G : y");
    assert_eq!(d.relationships.len(), 4);
    assert_eq!((d.relationships[0].left.as_str(), d.relationships[0].right.as_str()), ("A", "B"));
    assert_eq!(d.relationships[1].label, None);
    assert_eq!(d.relationships[2].label.as_deref(), Some("many to many"));
    assert!(!d.relationships[2].identifying);
    assert_eq!(d.relationships[3].left, "ENTITY ONE");
}

#[test]
fn word_cardinalities() {
    use Cardinality::*;
    let d = er(
        "erDiagram\nA one or zero to one or more B : r1\nC zero or many optionally to only one D : r2\nE 1+ to 0+ F\nG many(0) to many(1) H\nI 1 to zero or one J",
    );
    let got: Vec<(Cardinality, Cardinality, bool)> =
        d.relationships.iter().map(|r| (r.left_cardinality, r.right_cardinality, r.identifying)).collect();
    assert_eq!(
        got,
        vec![
            (ZeroOrOne, OneOrMore, true),
            (ZeroOrMore, ExactlyOne, false),
            (OneOrMore, ZeroOrMore, true),
            (ZeroOrMore, OneOrMore, true),
            (ExactlyOne, ZeroOrOne, true),
        ]
    );
}

#[test]
fn attribute_blocks() {
    let d = er(
        "erDiagram\nCAR {\n  string registrationNumber PK\n  string make \"brand name\"\n  int ownerId FK,UK\n  varchar(32) model PK, FK \"composite\"\n  string[] tags\n}\nEMPTY {\n}\nONE { string name }",
    );
    let car = &d.entities[0];
    assert_eq!(car.attributes.len(), 5);
    let a0 = &car.attributes[0];
    assert_eq!((a0.ty.as_str(), a0.name.as_str()), ("string", "registrationNumber"));
    assert_eq!(a0.keys, vec![KeyKind::Primary]);
    assert_eq!(car.attributes[1].comment.as_deref(), Some("brand name"));
    assert_eq!(car.attributes[2].keys, vec![KeyKind::Foreign, KeyKind::Unique]);
    let a3 = &car.attributes[3];
    assert_eq!(a3.ty, "varchar(32)");
    assert_eq!(a3.keys, vec![KeyKind::Primary, KeyKind::Foreign]);
    assert_eq!(a3.comment.as_deref(), Some("composite"));
    assert_eq!(car.attributes[4].ty, "string[]");
    assert!(d.entities[1].attributes.is_empty());
    assert_eq!(d.entities[2].attributes.len(), 1);
}

#[test]
fn bad_attribute_lines_warn() {
    let p = ok("erDiagram\nA {\n  string\n  int x ZZ\n  int ok\n}");
    let Diagram::Er(d) = &p.diagram else { panic!() };
    assert_eq!(d.entities[0].attributes.len(), 1);
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn aliases_and_styles() {
    let d = er(
        "erDiagram\np[Person]\nc[\"Customer account\"] {\n string id\n}\np ||--o{ c : owns\nclassDef red fill:#f00\nclass p red\nc:::red\nstyle p stroke:#000\ndirection RL",
    );
    assert_eq!(d.entities[0].alias.as_deref(), Some("Person"));
    assert_eq!(d.entities[1].alias.as_deref(), Some("Customer account"));
    assert_eq!(d.entities[1].attributes.len(), 1);
    assert_eq!(d.entities[0].classes, vec!["red"]);
    assert_eq!(d.entities[1].classes, vec!["red"]);
    assert_eq!(d.entities[0].style.len(), 1);
    assert_eq!(d.direction, Direction::RightToLeft);
    assert_eq!(d.entities.len(), 2);
}

#[test]
fn unknown_statements_warn() {
    let p = ok("erDiagram\nA ||--|| B\nwhat is this\n}\nA ||--|| B extra");
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![3, 4, 5]);
}

#[test]
fn syntax_errors_have_positions() {
    let cases: &[(&str, usize, usize)] =
        &[("erDiagram\nA {\n string name\n", 2, 1), ("erDiagram\nA ||--o{ ", 2, 9), ("erDiagram\nA ||--o] B", 2, 3), ("erDiagram\n  p[Person", 2, 4)];
    for &(src, line, column) in cases {
        let e = parse(src).expect_err(src);
        assert_eq!((e.line, e.column), (line, column), "{src}: {e}");
    }
}

#[test]
fn graph_view() {
    let p = ok(support::ER);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let g = p.diagram.to_graph();
    assert_eq!(g.kind, DiagramKind::Er);
    assert_eq!(g.direction, Direction::LeftToRight);
    let person = g.node("p").unwrap();
    assert_eq!(person.shape, Shape::Entity);
    assert_eq!(person.label, "Person");
    assert_eq!(person.details, vec!["string name PK \"full name\"", "int age", "string email UK", "int address_id FK, UK \"points at ADDRESS\""]);
    let places = &g.edges[0];
    assert_eq!((places.source.as_str(), places.target.as_str()), ("CUSTOMER", "ORDER"));
    assert_eq!(places.start, ArrowKind::ExactlyOne);
    assert_eq!(places.end, ArrowKind::ZeroOrMore);
    assert_eq!(places.line, LineStyle::Solid);
    assert_eq!(places.label.as_deref(), Some("places"));
    let uses = &g.edges[2];
    assert_eq!(uses.line, LineStyle::Dashed);
    assert_eq!(uses.start, ArrowKind::OneOrMore);
    assert_eq!(g.node("LINE-ITEM").unwrap().label, "LINE-ITEM");
    let drives = g.edges.iter().find(|e| e.label.as_deref() == Some("drives")).unwrap();
    assert_eq!(drives.line, LineStyle::Dashed);
    assert_eq!(drives.end, ArrowKind::OneOrMore);
}
