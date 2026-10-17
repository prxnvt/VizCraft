//! Class diagram parsing.

mod support;

use support::ok;
use vizcraft_mermaid::class::{ClassDiagram, Classifier, MemberKind, RelationEnd, RelationKind, Visibility};
use vizcraft_mermaid::{ArrowKind, Diagram, DiagramKind, Direction, GroupKind, LineStyle, Shape, parse};

fn cd(text: &str) -> ClassDiagram {
    match ok(text).diagram {
        Diagram::Class(d) => d,
        other => panic!("expected a class diagram, got {:?}", other.kind()),
    }
}

fn class<'a>(d: &'a ClassDiagram, id: &str) -> &'a vizcraft_mermaid::class::Class {
    d.classes.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no class {id}"))
}

#[test]
fn members_in_a_body() {
    let d = cd(
        "classDiagram\nclass BankAccount {\n  +String owner\n  -BigDecimal balance$\n  #deposit(amount) bool\n  ~withdraw(amount)* int\n  +getMessages(List~string~ messages) List~string~\n  helper()\n  +count()$\n}",
    );
    let m = &class(&d, "BankAccount").members;
    assert_eq!(m.len(), 7);
    assert_eq!(m[0].kind, MemberKind::Attribute);
    assert_eq!(m[0].visibility, Some(Visibility::Public));
    assert_eq!((m[0].ty.as_deref(), m[0].name.as_str()), (Some("String"), "owner"));
    assert_eq!(m[0].text, "+String owner");
    assert_eq!(m[1].visibility, Some(Visibility::Private));
    assert_eq!(m[1].classifier, Some(Classifier::Static));
    assert_eq!(m[1].text, "-BigDecimal balance");
    assert_eq!(m[2].kind, MemberKind::Method);
    assert_eq!(m[2].visibility, Some(Visibility::Protected));
    assert_eq!(m[2].name, "deposit");
    assert_eq!(m[2].parameters.as_deref(), Some("amount"));
    assert_eq!(m[2].ty.as_deref(), Some("bool"));
    assert_eq!(m[3].visibility, Some(Visibility::Package));
    assert_eq!(m[3].classifier, Some(Classifier::Abstract));
    assert_eq!(m[3].ty.as_deref(), Some("int"));
    assert_eq!(m[4].text, "+getMessages(List<string> messages) : List<string>");
    assert_eq!(m[5].visibility, None);
    assert_eq!(m[5].text, "helper()");
    assert_eq!(m[6].classifier, Some(Classifier::Static));
}

#[test]
fn members_by_colon_and_annotations() {
    let d = cd("classDiagram\nAnimal : +int age\nAnimal : +isMammal() bool\n<<interface>> Animal\nclass Shape {\n <<abstract>>\n}");
    let a = class(&d, "Animal");
    assert_eq!(a.members.len(), 2);
    assert_eq!(a.members[1].kind, MemberKind::Method);
    assert_eq!(a.annotations, vec!["interface"]);
    assert_eq!(class(&d, "Shape").annotations, vec!["abstract"]);
}

#[test]
fn generics_labels_and_backticks() {
    let d =
        cd("classDiagram\nclass Square~Shape~\nclass Box~List~int~~ {\n}\nclass A[\"Readable label\"]\nclass `Odd name!`\nSquare~Shape~ <|-- Cube");
    assert_eq!(class(&d, "Square").generic.as_deref(), Some("Shape"));
    assert_eq!(class(&d, "Square").display_name(), "Square<Shape>");
    assert_eq!(class(&d, "Box").generic.as_deref(), Some("List<int>"));
    assert_eq!(class(&d, "A").display_name(), "Readable label");
    assert!(d.classes.iter().any(|c| c.id == "Odd name!"));
    assert_eq!(d.relations[0].from, "Square");
}

#[test]
fn every_relation_kind() {
    use RelationEnd as E;
    let cases = [
        ("A <|-- B", E::Inheritance, false, E::None, RelationKind::Inheritance),
        ("A --|> B", E::None, false, E::Inheritance, RelationKind::Inheritance),
        ("A *-- B", E::Composition, false, E::None, RelationKind::Composition),
        ("A --* B", E::None, false, E::Composition, RelationKind::Composition),
        ("A o-- B", E::Aggregation, false, E::None, RelationKind::Aggregation),
        ("A --o B", E::None, false, E::Aggregation, RelationKind::Aggregation),
        ("A --> B", E::None, false, E::Arrow, RelationKind::Association),
        ("A <-- B", E::Arrow, false, E::None, RelationKind::Association),
        ("A -- B", E::None, false, E::None, RelationKind::Link),
        ("A ..> B", E::None, true, E::Arrow, RelationKind::Dependency),
        ("A <.. B", E::Arrow, true, E::None, RelationKind::Dependency),
        ("A ..|> B", E::None, true, E::Inheritance, RelationKind::Realization),
        ("A <|.. B", E::Inheritance, true, E::None, RelationKind::Realization),
        ("A .. B", E::None, true, E::None, RelationKind::DashedLink),
        ("A ()-- B", E::Lollipop, false, E::None, RelationKind::Lollipop),
        ("A --() B", E::None, false, E::Lollipop, RelationKind::Lollipop),
        ("A <|--|> B", E::Inheritance, false, E::Inheritance, RelationKind::Inheritance),
        ("A *--* B", E::Composition, false, E::Composition, RelationKind::Composition),
        ("A<|--B", E::Inheritance, false, E::None, RelationKind::Inheritance),
    ];
    for (src, from_end, dashed, to_end, kind) in cases {
        let d = cd(&format!("classDiagram\n{src}"));
        let r = &d.relations[0];
        assert_eq!((r.from.as_str(), r.to.as_str()), ("A", "B"), "{src}");
        assert_eq!((r.from_end, r.dashed, r.to_end), (from_end, dashed, to_end), "{src}");
        assert_eq!(r.kind(), kind, "{src}");
    }
}

#[test]
fn cardinalities_and_labels() {
    let d = cd("classDiagram\nCustomer \"1\" --> \"*\" Ticket : buys\nGalaxy \"1..*\" o-- Star\nA -- B : plain label");
    let r = &d.relations[0];
    assert_eq!(r.from_cardinality.as_deref(), Some("1"));
    assert_eq!(r.to_cardinality.as_deref(), Some("*"));
    assert_eq!(r.label.as_deref(), Some("buys"));
    assert_eq!(d.relations[1].from_cardinality.as_deref(), Some("1..*"));
    assert_eq!(d.relations[1].to_cardinality, None);
    assert_eq!(d.relations[2].label.as_deref(), Some("plain label"));
}

#[test]
fn namespaces_and_notes() {
    let d = cd(
        "classDiagram\nnamespace Shapes {\n  class Triangle\n  class Rect {\n    +int w\n  }\n  namespace Inner {\n    class Dot\n  }\n}\nTriangle <|-- Rect\nnote \"free note\"\nnote for Rect \"two\\nlines\"",
    );
    assert_eq!(class(&d, "Triangle").namespace.as_deref(), Some("Shapes"));
    assert_eq!(class(&d, "Rect").namespace.as_deref(), Some("Shapes"));
    assert_eq!(class(&d, "Rect").members.len(), 1);
    assert_eq!(class(&d, "Dot").namespace.as_deref(), Some("Inner"));
    assert_eq!(d.namespaces[1].parent.as_deref(), Some("Shapes"));
    assert_eq!(d.notes.len(), 2);
    assert_eq!(d.notes[0].target, None);
    assert_eq!(d.notes[1].target.as_deref(), Some("Rect"));
    assert_eq!(d.notes[1].text, "two\nlines");
}

#[test]
fn styles_and_interaction() {
    let p = ok(
        "classDiagram\nclass A:::hot\nclass B\nclassDef hot fill:#f00\ncssClass \"A,B\" cold\nB:::warm\nstyle A stroke:#000,stroke-width:2px\nlink A \"https://example.com\" \"Tip\"\nclick B href \"https://example.org\"\ncallback A \"fn\" \"Tip two\"\nclick B call fn2()\ndirection LR",
    );
    let Diagram::Class(d) = &p.diagram else { panic!() };
    assert_eq!(class(d, "A").classes, vec!["hot", "cold"]);
    assert_eq!(class(d, "B").classes, vec!["cold", "warm"]);
    assert_eq!(class(d, "A").style.len(), 2);
    assert_eq!(d.clicks.len(), 4);
    assert_eq!(d.clicks[0].url.as_deref(), Some("https://example.com"));
    assert_eq!(d.clicks[1].url.as_deref(), Some("https://example.org"));
    assert_eq!(d.clicks[2].callback.as_deref(), Some("fn"));
    assert_eq!(d.direction, Direction::LeftToRight);
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![10, 11]);
}

#[test]
fn unknown_statements_warn() {
    let p = ok("classDiagram\nA <|-- B\n%% comment\nwhat is this\n}\nnote\nA <|-- B extra\nnamespace Foo");
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![4, 5, 6, 7, 8]);
}

#[test]
fn syntax_errors_have_positions() {
    let cases: &[(&str, usize, usize)] = &[
        ("classDiagram\nclass A {\n +x\n", 2, 1),
        ("classDiagram\nnamespace N {\n class A\n", 2, 1),
        ("classDiagram\nA <|-- ", 2, 7),
        ("classDiagram\nA \"1\" B", 2, 7),
        ("classDiagram\nclass A[\"x\"", 2, 8),
        ("classDiagram\n<<interface A", 2, 1),
    ];
    for &(src, line, column) in cases {
        let e = parse(src).expect_err(src);
        assert_eq!((e.line, e.column), (line, column), "{src}: {e}");
    }
}

#[test]
fn graph_view() {
    let p = ok(support::CLASS);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let g = p.diagram.to_graph();
    assert_eq!(g.kind, DiagramKind::Class);
    assert_eq!(g.direction, Direction::RightToLeft);
    let animal = g.node("Animal").unwrap();
    assert_eq!(animal.shape, Shape::Class);
    assert_eq!(animal.annotations, vec!["abstract"]);
    // Attributes first, then methods.
    assert_eq!(animal.details, vec!["+String name", "-int age", "+makeSound() : void", "+getAge() : int", "+breathe() : bool"]);
    assert_eq!(animal.classes, vec!["hot"]);
    let duck = g.node("Duck").unwrap();
    assert_eq!(duck.label, "Mallard duck");
    assert_eq!(duck.link.as_deref(), Some("https://example.com/duck"));
    assert_eq!(duck.tooltip.as_deref(), Some("About ducks"));
    assert_eq!(g.node("Square").unwrap().label, "Square<Shape>");
    assert_eq!(g.node("Flyer").unwrap().annotations, vec!["interface"]);
    let inherit = &g.edges[0];
    assert_eq!((inherit.source.as_str(), inherit.target.as_str()), ("Animal", "Duck"));
    assert_eq!(inherit.start, ArrowKind::Triangle);
    let legs = &g.edges[1];
    assert_eq!(legs.start, ArrowKind::DiamondFilled);
    assert_eq!(legs.source_label.as_deref(), Some("1"));
    assert_eq!(legs.target_label.as_deref(), Some("many"));
    assert_eq!(legs.label.as_deref(), Some("has"));
    assert_eq!(g.edges[2].start, ArrowKind::DiamondHollow);
    assert_eq!(g.edges[4].line, LineStyle::Dashed);
    assert_eq!(g.edges[4].end, ArrowKind::Arrow);
    assert_eq!(g.edges[5].end, ArrowKind::Triangle);
    assert_eq!(g.edges[8].start, ArrowKind::Lollipop);
    let zoo = g.group("Zoo").unwrap();
    assert_eq!(zoo.kind, GroupKind::Namespace);
    assert_eq!(zoo.members, vec!["Keeper", "Cage"]);
    assert_eq!(g.node("Cage").unwrap().group.as_deref(), Some("Zoo"));
    let notes: Vec<&str> = g.nodes.iter().filter(|n| n.shape == Shape::Note).map(|n| n.label.as_str()).collect();
    assert_eq!(notes, vec!["A small farm", "can fly\ncan swim"]);
    assert!(g.edges.iter().any(|e| e.target == "Duck" && e.line == LineStyle::Dotted));
}
