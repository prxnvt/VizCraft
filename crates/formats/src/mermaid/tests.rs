use vizcraft_diagram::{Arrow, Color, Document, End, Page, Rect, Shape};

use super::import;
use crate::FormatError;

fn page(src: &str) -> Page {
    let mut imported = import(src).unwrap();
    assert_eq!(imported.pages.len(), 1);
    imported.pages.remove(0)
}

fn shape<'a>(p: &'a Page, id: &str) -> &'a Shape {
    p.shape(id).unwrap_or_else(|| panic!("no shape {id}: {:?}", p.shapes.iter().map(|s| &s.id).collect::<Vec<_>>()))
}

fn glued(e: &End) -> &str {
    e.shape().unwrap()
}

/// Shapes that are drawn nodes, not group backgrounds or lifelines.
fn nodes(p: &Page) -> Vec<&Shape> {
    p.shapes.iter().filter(|s| !s.ext.contains_key("mermaid.group") && s.ext.get("mermaid.shape").map(String::as_str) != Some("lifeline")).collect()
}

fn assert_no_overlap(p: &Page) {
    let ns = nodes(p);
    for (i, a) in ns.iter().enumerate() {
        for b in ns.iter().skip(i + 1) {
            let (ra, rb) = (a.bounds(), b.bounds());
            let overlap = ra.x0 < rb.x1 - 0.5 && rb.x0 < ra.x1 - 0.5 && ra.y0 < rb.y1 - 0.5 && rb.y0 < ra.y1 - 0.5;
            assert!(!overlap, "{} {:?} overlaps {} {:?}", a.id, ra, b.id, rb);
        }
    }
}

fn inside(outer: Rect, inner: Rect) -> bool {
    outer.x0 <= inner.x0 && outer.y0 <= inner.y0 && outer.x1 >= inner.x1 && outer.y1 >= inner.y1
}

#[test]
fn flowchart_maps_shapes_edges_and_labels() {
    let p = page("flowchart TD\n  A[Start] --> B{Ok?}\n  B -->|yes| C([Done])\n  B -.-> D[(Store)]\n  C ==> E{{Prep}}\n  E --x F>Flag]\n");
    assert_eq!(p.shapes.len(), 6);
    assert_eq!(p.connectors.len(), 5);
    assert_eq!(shape(&p, "A").stencil, "flowchart.process");
    assert_eq!(shape(&p, "B").stencil, "flowchart.decision");
    assert_eq!(shape(&p, "C").stencil, "flowchart.terminator");
    assert_eq!(shape(&p, "D").stencil, "flowchart.database");
    assert_eq!(shape(&p, "E").stencil, "flowchart.preparation");
    assert_eq!(shape(&p, "A").text, "Start");
    for c in &p.connectors {
        assert!(matches!(c.start, End::Glued { point: None, .. }) && matches!(c.end, End::Glued { point: None, .. }));
    }
    let yes = p.connectors.iter().find(|c| c.text == "yes").unwrap();
    assert_eq!((glued(&yes.start), glued(&yes.end)), ("B", "C"));
    assert_eq!(yes.end_arrow, Arrow::Triangle);
    let dotted = p.connectors.iter().find(|c| glued(&c.end) == "D").unwrap();
    assert_eq!(dotted.ext.get("mermaid.line").map(String::as_str), Some("dotted"));
    let thick = p.connectors.iter().find(|c| glued(&c.end) == "E").unwrap();
    assert!(thick.style.line_width > 2.0);
    let cross = p.connectors.iter().find(|c| glued(&c.end) == "F").unwrap();
    assert_eq!(cross.end_arrow, Arrow::Open);
    // Top to bottom: each successor sits below its source.
    assert!(shape(&p, "B").y > shape(&p, "A").y + shape(&p, "A").h - 1.0);
    // Never smaller than the master's default size.
    assert!(shape(&p, "A").w >= 140.0 && shape(&p, "A").h >= 60.0);
    assert_no_overlap(&p);
    // The drawing starts at the margin.
    let left = p.shapes.iter().map(|s| s.bounds().x0).fold(f64::INFINITY, f64::min);
    let top = p.shapes.iter().map(|s| s.bounds().y0).fold(f64::INFINITY, f64::min);
    assert!((left - 40.0).abs() < 1e-6 && (top - 40.0).abs() < 1e-6, "{left} {top}");
}

#[test]
fn long_labels_grow_the_shape() {
    let p = page("flowchart LR\n  A[\"A rather long label that needs a much wider box than usual\"] --> B\n");
    let a = shape(&p, "A");
    assert!(a.w > 300.0, "{}", a.w);
}

#[test]
fn invisible_links_are_laid_out_but_not_drawn() {
    let p = page("flowchart LR\n  A --> B\n  B ~~~ C\n");
    assert_eq!(p.connectors.len(), 1);
    assert!(shape(&p, "C").x > shape(&p, "B").x);
}

#[test]
fn direction_lr_places_successors_to_the_right() {
    let p = page("flowchart LR\n  A --> B --> C\n");
    let (a, b, c) = (shape(&p, "A"), shape(&p, "B"), shape(&p, "C"));
    assert!(b.x >= a.x + a.w && c.x >= b.x + b.w);
    let p = page("flowchart RL\n  A --> B\n");
    assert!(shape(&p, "B").x + shape(&p, "B").w <= shape(&p, "A").x);
}

#[test]
fn subgraph_rectangle_is_behind_and_around_its_members() {
    let p = page("flowchart TB\n  X --> A\n  subgraph G [Group one]\n    A --> B\n  end\n  B --> Y\n  Y --> G\n");
    let gi = p.shapes.iter().position(|s| s.id == "G").unwrap();
    let g = &p.shapes[gi];
    assert_eq!(g.text, "Group one");
    for m in ["A", "B"] {
        let mi = p.shapes.iter().position(|s| s.id == m).unwrap();
        assert!(gi < mi, "the group is drawn before {m}");
        assert!(inside(g.bounds(), p.shapes[mi].bounds()), "{m} inside the group");
    }
    for out in ["X", "Y"] {
        let b = shape(&p, out).bounds();
        assert!(!g.bounds().intersects(&b) || b.x1 <= g.bounds().x0 || b.y1 <= g.bounds().y0, "{out} outside the group");
    }
    // An edge to the subgraph glues to its rectangle.
    assert!(p.connectors.iter().any(|c| glued(&c.start) == "Y" && glued(&c.end) == "G"));
    assert_no_overlap(&p);
}

#[test]
fn nested_subgraphs_nest() {
    let p = page(
        "flowchart LR\n  subgraph outer\n    A\n    subgraph inner [Inner]\n      direction TB\n      B --> C\n    end\n  end\n  A --> B\n  D --> A\n",
    );
    let outer = shape(&p, "outer").bounds();
    let inner = shape(&p, "inner").bounds();
    assert!(inside(outer, inner));
    for m in ["B", "C"] {
        assert!(inside(inner, shape(&p, m).bounds()));
    }
    assert!(inside(outer, shape(&p, "A").bounds()));
    assert!(!inside(outer, shape(&p, "D").bounds()));
    // The inner group runs top to bottom.
    assert!(shape(&p, "C").y >= shape(&p, "B").y + shape(&p, "B").h);
    assert_no_overlap(&p);
}

#[test]
fn class_defs_and_styles_apply() {
    let p = page(
        "flowchart TD\n  A:::hot --> B\n  classDef hot fill:#ff0000,stroke:#00ff00,stroke-width:4px,color:#0000ff\n  style B fill:#010203,stroke-dasharray: 5 5\n",
    );
    let a = shape(&p, "A");
    assert_eq!(a.style.fill, Color::rgb(255, 0, 0));
    assert_eq!(a.style.line, Color::rgb(0, 255, 0));
    assert_eq!(a.style.font_color, Color::rgb(0, 0, 255));
    assert!((a.style.line_width - 4.0).abs() < 1e-9);
    let b = shape(&p, "B");
    assert_eq!(b.style.fill, Color::rgb(1, 2, 3));
    assert_eq!(b.ext.get("mermaid.line").map(String::as_str), Some("dashed"));
}

#[test]
fn css_colours_are_read() {
    use super::common::color;
    assert_eq!(color("rgb(1, 2, 3)"), Some(Color::rgb(1, 2, 3)));
    assert_eq!(color("rgba(1,2,3,0)"), Some(Color { r: 1, g: 2, b: 3, a: 0 }));
    assert_eq!(color("#abc"), Some(Color::rgb(0xaa, 0xbb, 0xcc)));
    assert_eq!(color("Red"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(color("none"), Some(Color::TRANSPARENT));
    assert_eq!(color("rgb(1,2)"), None);
    assert_eq!(color("rgb(nan,2,3)"), None);
    assert_eq!(color("blurple"), None);
}

#[test]
fn state_diagram_maps_pseudo_states_and_composites() {
    let src = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : start\n  state Busy {\n    [*] --> Working\n    Working --> [*]\n  }\n  state fork1 <<fork>>\n  Busy --> fork1\n  fork1 --> Done\n  Done --> [*]\n";
    let imported = import(src).unwrap();
    let p = &imported.pages[0];
    let kinds: Vec<&str> = p.shapes.iter().filter_map(|s| s.ext.get("mermaid.shape").map(String::as_str)).collect();
    assert!(kinds.contains(&"start") && kinds.contains(&"end") && kinds.contains(&"forkBar"), "{kinds:?}");
    let start = p.shapes.iter().find(|s| s.ext.get("mermaid.shape").map(String::as_str) == Some("start")).unwrap();
    assert!(start.w <= 30.0 && start.style.fill != Color::WHITE);
    let bar = p.shapes.iter().find(|s| s.ext.get("mermaid.shape").map(String::as_str) == Some("forkBar")).unwrap();
    assert!(bar.h < 10.0 && bar.w > 40.0);
    let busy = shape(p, "Busy");
    assert_eq!(busy.ext.get("mermaid.group").map(String::as_str), Some("compositeState"));
    assert!(inside(busy.bounds(), shape(p, "Working").bounds()));
    let start_edge = p.connectors.iter().find(|c| c.text == "start").unwrap();
    assert_eq!((glued(&start_edge.start), glued(&start_edge.end)), ("Idle", "Busy"));
    // Every connector is glued at both ends to a shape on the page.
    for c in &p.connectors {
        assert!(p.shape(glued(&c.start)).is_some() && p.shape(glued(&c.end)).is_some());
    }
    assert_eq!(p.connectors.len(), 7);
    assert_no_overlap(p);
}

#[test]
fn class_diagram_has_member_rows_and_uml_markers() {
    let src = "classDiagram\n  class Animal {\n    <<abstract>>\n    +String name\n    +eat() void\n  }\n  Animal <|-- Dog\n  Animal \"1\" *-- \"many\" Leg : has\n";
    let p = page(src);
    assert_eq!(p.shapes.len(), 3);
    assert_eq!(p.connectors.len(), 2);
    // Classes are drawn as OMG UML classes: the name, then member compartments (b6-uml).
    let animal = shape(&p, "Animal");
    assert_eq!((animal.text.as_str(), animal.stencil.as_str(), animal.style.italic), ("Animal", "uml.class", true));
    let rows: Vec<&str> = animal.compartments.iter().flat_map(|c| c.rows.iter().map(String::as_str)).collect();
    assert_eq!(rows, ["+name: String", "+eat(): void"]);
    assert!(animal.h > 70.0);
    let inherit = p.connectors.iter().find(|c| glued(&c.end) == "Dog" || glued(&c.start) == "Dog").unwrap();
    assert_eq!((glued(&inherit.start), inherit.end_arrow), ("Dog", Arrow::HollowTriangle), "the child points at the parent");
    let has = p.connectors.iter().find(|c| c.text.contains("has")).unwrap();
    assert_eq!((has.text.as_str(), has.start_label.as_str(), has.end_label.as_str()), ("has", "1", "many"));
    assert_eq!(has.start_arrow, Arrow::Diamond);
    assert_no_overlap(&p);
}

#[test]
fn er_diagram_has_attributes_and_cardinalities() {
    let src =
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  CUSTOMER {\n    string name PK\n    string email\n  }\n  ORDER ||--|{ LINE-ITEM : contains\n";
    let p = page(src);
    assert_eq!(p.shapes.len(), 3);
    assert_eq!(p.connectors.len(), 2);
    // Entities are ER tables: the name on the shape, the columns as owned elements to merge and
    // as preview rows.
    let customer = shape(&p, "CUSTOMER");
    assert_eq!((customer.text.as_str(), customer.stencil.as_str()), ("CUSTOMER", "er.entity"));
    assert_eq!(customer.ext.get("import.type").map(String::as_str), Some("er.Entity"));
    let kids: serde_json::Value = serde_json::from_str(customer.ext.get(crate::er::CHILDREN_KEY).unwrap()).unwrap();
    assert_eq!((kids[0]["id"].as_str(), kids[0]["props"]["pk"].as_str()), (Some("CUSTOMER.name"), Some("true")));
    assert_eq!(customer.compartments[0].rows, ["PK\tname\tstring"]);
    // `--` is identifying, so ORDER's child is a weak entity; the ends are the cardinalities.
    let places = p.connectors.iter().find(|c| c.text == "places").unwrap();
    assert_eq!((places.start_arrow, places.end_arrow), (Arrow::DoubleBar, Arrow::CrowsFootCircle));
    assert_eq!(places.ext.get("import.type").map(String::as_str), Some("er.Identifying"));
    assert_eq!(places.ext.get("import.prop.sourceCardinality").map(String::as_str), Some("one"));
    assert_eq!(places.ext.get("import.prop.targetCardinality").map(String::as_str), Some("zeroOrMore"));
    assert_eq!(shape(&p, "ORDER").ext.get("import.type").map(String::as_str), Some("er.WeakEntity"));
    assert_eq!((glued(&places.start), glued(&places.end)), ("CUSTOMER", "ORDER"));
    assert_no_overlap(&p);
}

#[test]
fn warnings_pass_through_with_line_numbers() {
    let imported = import("flowchart TD\n  A --> B\n  this is not a statement ((\n").unwrap_or_default();
    // Either the parser rejects the line or warns about it; a warning names its line.
    for w in &imported.warnings {
        assert!(w.starts_with("line "), "{w}");
    }
}

#[test]
fn parse_errors_carry_line_and_column() {
    let err = import("flowchart TD\n  A[unclosed --> B\n").unwrap_err();
    let FormatError::Import(msg) = err else { panic!("wrong error {err:?}") };
    assert!(msg.contains("line 2") && msg.contains("column"), "{msg}");
    assert!(matches!(import("pie\n  \"a\" : 1\n"), Err(FormatError::Import(_))));
    assert!(matches!(import(""), Err(FormatError::Import(_))));
}

#[test]
fn ids_survive_adoption() {
    let p = page("flowchart TD\n  A --> B\n");
    let mut doc = Document::new();
    let ids = doc.adopt(0, p, true);
    assert!(ids.contains(&"A".to_string()) && ids.contains(&"B".to_string()));
    let c = &doc.page().unwrap().connectors[0];
    assert_eq!((glued(&c.start), glued(&c.end)), ("A", "B"));
}

#[test]
fn page_grows_to_fit_large_drawings() {
    let mut src = String::from("flowchart LR\n");
    for i in 0..40 {
        src.push_str(&format!("  n{i} --> n{}\n", i + 1));
    }
    let p = page(&src);
    let right = p.shapes.iter().map(|s| s.bounds().x1).fold(0.0, f64::max);
    assert!(p.width >= right + 40.0 - 1e-6 && p.width > Page::DEFAULT_WIDTH);
}

#[test]
fn hostile_and_truncated_text_never_panics() {
    let samples = [
        "flowchart TD\n  A[\"x\"] --> B{y} --> C((z))\n  subgraph S\n  D --> S\n  end\n  S --> S\n  A --> A\n",
        "stateDiagram-v2\n  state A {\n    state B {\n      [*] --> C\n    }\n  }\n  C --> A\n  note right of C : hi\n",
        "classDiagram\n  namespace N {\n    class A\n  }\n  A <|-- B\n  note for A \"n\"\n  class C:::missing\n  style A fill:notacolour,width:3\n",
        "erDiagram\n  A ||--o{ B : r\n  A { int id PK }\n",
        "sequenceDiagram\n  A->>A: x\n  Note left of A: left\n  Note over A,B: both\n  activate A\n  create participant C\n  A->>C: new\n  destroy C\n",
    ];
    for s in samples {
        let _ = import(s);
        for cut in 0..s.len() {
            if s.is_char_boundary(cut) {
                let _ = import(&s[..cut]);
            }
        }
    }
    let junk = [
        "\0\0\0",
        "flowchart",
        "graph LR;;;;",
        "flowchart TD\n  A@{ shape: nonsense }\n",
        "%%{init: {}}%%\nflowchart TD\n A-->B",
        "\u{feff}flowchart TD\nA-->B",
    ];
    for s in junk {
        let _ = import(s);
    }
    let mut deep = String::from("flowchart TD\n");
    for i in 0..80 {
        deep.push_str(&format!("subgraph g{i}\n"));
    }
    deep.push_str("A --> B\n");
    for _ in 0..80 {
        deep.push_str("end\n");
    }
    let _ = import(&deep);
}

#[test]
fn unknown_shapes_become_rectangles_with_a_warning() {
    let imported = import("flowchart TD\n  A@{ shape: nonsense }\n").unwrap();
    let a = shape(&imported.pages[0], "A");
    assert_eq!(a.ext.get("mermaid.shape").map(String::as_str), Some("nonsense"));
    assert_eq!(a.stencil, "basic.rectangle");
    assert!(imported.warnings.iter().any(|w| w.contains("nonsense")), "{:?}", imported.warnings);
}

#[test]
fn large_flowchart_imports_quickly() {
    let mut src = String::from("flowchart TD\n");
    for i in 1..3000 {
        src.push_str(&format!("  n{} --> n{i}\n", (i - 1) / 3));
    }
    let start = std::time::Instant::now();
    let p = page(&src);
    assert_eq!(p.shapes.len(), 3000);
    assert_eq!(p.connectors.len(), 2999);
    assert!(start.elapsed().as_secs() < 30, "took {:?}", start.elapsed());
}
