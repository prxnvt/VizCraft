//! Flowchart parsing.

mod support;

use support::ok;
use vizcraft_mermaid::flowchart::{FlowEdge, Flowchart, LinkTarget};
use vizcraft_mermaid::{ArrowKind, Diagram, DiagramKind, Direction, GroupKind, LineStyle, Shape, StyleProp, parse};

fn flow(text: &str) -> Flowchart {
    match ok(text).diagram {
        Diagram::Flowchart(f) => f,
        other => panic!("expected a flowchart, got {:?}", other.kind()),
    }
}

fn node<'a>(f: &'a Flowchart, id: &str) -> &'a vizcraft_mermaid::flowchart::FlowNode {
    f.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
}

fn one_edge(text: &str) -> FlowEdge {
    let f = flow(text);
    assert_eq!(f.edges.len(), 1, "{text}: {:?}", f.edges);
    f.edges.into_iter().next().unwrap()
}

#[test]
fn directions() {
    for (kw, dir) in [
        ("TB", Direction::TopToBottom),
        ("TD", Direction::TopToBottom),
        ("BT", Direction::BottomToTop),
        ("LR", Direction::LeftToRight),
        ("RL", Direction::RightToLeft),
    ] {
        assert_eq!(flow(&format!("flowchart {kw}\nA-->B")).direction, dir, "{kw}");
        assert_eq!(flow(&format!("graph {kw}\nA-->B")).direction, dir, "{kw}");
    }
    assert_eq!(flow("flowchart\nA").direction, Direction::TopToBottom);
}

#[test]
fn header_with_statements_on_the_same_line() {
    let f = flow("graph LR; A-->B; B-->C");
    assert_eq!(f.direction, Direction::LeftToRight);
    assert_eq!(f.edges.len(), 2);
}

#[test]
fn every_classic_shape() {
    let cases = [
        ("A[rect]", Shape::Rectangle, "rect"),
        ("A(rounded)", Shape::Rounded, "rounded"),
        ("A([stadium])", Shape::Stadium, "stadium"),
        ("A[[sub]]", Shape::Subroutine, "sub"),
        ("A[(db)]", Shape::Cylinder, "db"),
        ("A((circle))", Shape::Circle, "circle"),
        ("A(((double)))", Shape::DoubleCircle, "double"),
        ("A>flag]", Shape::Asymmetric, "flag"),
        ("A{decide}", Shape::Diamond, "decide"),
        ("A{{hex}}", Shape::Hexagon, "hex"),
        ("A[/lean right/]", Shape::Parallelogram, "lean right"),
        ("A[\\lean left\\]", Shape::ParallelogramAlt, "lean left"),
        ("A[/trap\\]", Shape::Trapezoid, "trap"),
        ("A[\\trap alt/]", Shape::TrapezoidAlt, "trap alt"),
    ];
    for (src, shape, label) in cases {
        let f = flow(&format!("flowchart TD\n{src}"));
        let n = node(&f, "A");
        assert_eq!(n.shape, shape, "{src}");
        assert_eq!(n.label, label, "{src}");
    }
}

#[test]
fn quoted_labels_and_entities() {
    let f = flow("flowchart TD\nA[\"A (quoted) [label]\"] --> B[\"#quot;hi#quot; #35;1 #9829;\"]\nC[\"two<br/>lines\"]\nD[\"`**markdown**`\"]");
    assert_eq!(node(&f, "A").label, "A (quoted) [label]");
    assert_eq!(node(&f, "B").label, "\"hi\" #1 \u{2665}");
    assert_eq!(node(&f, "C").label, "two\nlines");
    assert_eq!(node(&f, "D").label, "**markdown**");
}

#[test]
fn entity_semicolon_does_not_split_statements() {
    let f = flow("flowchart TD\nA-- a#59;b -->B; B --> C");
    assert_eq!(f.edges[0].label.as_deref(), Some("a;b"));
    assert_eq!(f.edges.len(), 2);
}

#[test]
fn multi_line_quoted_label() {
    let f = flow("flowchart TD\nA[\"first\nsecond\"] --> B");
    assert_eq!(node(&f, "A").label, "first\nsecond");
}

#[test]
fn new_shape_syntax() {
    let f = flow("flowchart TD\nA@{ shape: cyl, label: \"Orders DB\" }\nB@{ shape: doc }\nC@{shape: diamond}\nD@{ shape: sm-circ }\nA --> B");
    assert_eq!(node(&f, "A").shape, Shape::Cylinder);
    assert_eq!(node(&f, "A").label, "Orders DB");
    assert_eq!(node(&f, "B").shape, Shape::Document);
    assert_eq!(node(&f, "B").label, "B");
    assert_eq!(node(&f, "C").shape, Shape::Diamond);
    assert_eq!(node(&f, "D").shape, Shape::SmallCircle);
}

#[test]
fn new_shape_names_map() {
    for (name, shape) in [
        ("rect", Shape::Rectangle),
        ("rounded", Shape::Rounded),
        ("stadium", Shape::Stadium),
        ("subproc", Shape::Subroutine),
        ("circle", Shape::Circle),
        ("dbl-circ", Shape::DoubleCircle),
        ("hex", Shape::Hexagon),
        ("lean-r", Shape::Parallelogram),
        ("lean-l", Shape::ParallelogramAlt),
        ("trap-b", Shape::Trapezoid),
        ("trap-t", Shape::TrapezoidAlt),
        ("docs", Shape::MultiDocument),
        ("cloud", Shape::Cloud),
        ("tri", Shape::Triangle),
        ("text", Shape::Text),
        ("fork", Shape::ForkBar),
        ("hourglass", Shape::Hourglass),
        ("bolt", Shape::Bolt),
        ("delay", Shape::Delay),
        ("das", Shape::HorizontalCylinder),
        ("odd", Shape::Asymmetric),
    ] {
        let f = flow(&format!("flowchart TD\nA@{{ shape: {name} }}"));
        assert_eq!(node(&f, "A").shape, shape, "{name}");
    }
}

#[test]
fn unknown_new_shape_is_kept_with_a_warning() {
    let p = ok("flowchart TD\nA@{ shape: wobble, icon: fa:car }");
    let Diagram::Flowchart(f) = &p.diagram else { panic!() };
    assert_eq!(node(f, "A").shape, Shape::Other("wobble".into()));
    assert_eq!(p.warnings.len(), 2, "{:?}", p.warnings);
    assert!(p.warnings.iter().all(|w| w.line == 2));
}

#[test]
fn link_kinds() {
    let cases: &[(&str, LineStyle, ArrowKind, ArrowKind, u8)] = &[
        ("A-->B", LineStyle::Solid, ArrowKind::None, ArrowKind::Arrow, 1),
        ("A---B", LineStyle::Solid, ArrowKind::None, ArrowKind::None, 1),
        ("A-.->B", LineStyle::Dotted, ArrowKind::None, ArrowKind::Arrow, 1),
        ("A-.-B", LineStyle::Dotted, ArrowKind::None, ArrowKind::None, 1),
        ("A==>B", LineStyle::Thick, ArrowKind::None, ArrowKind::Arrow, 1),
        ("A===B", LineStyle::Thick, ArrowKind::None, ArrowKind::None, 1),
        ("A~~~B", LineStyle::Invisible, ArrowKind::None, ArrowKind::None, 1),
        ("A--oB", LineStyle::Solid, ArrowKind::None, ArrowKind::Circle, 1),
        ("A--xB", LineStyle::Solid, ArrowKind::None, ArrowKind::Cross, 1),
        ("A<-->B", LineStyle::Solid, ArrowKind::Arrow, ArrowKind::Arrow, 1),
        ("A o--o B", LineStyle::Solid, ArrowKind::Circle, ArrowKind::Circle, 1),
        ("A x--x B", LineStyle::Solid, ArrowKind::Cross, ArrowKind::Cross, 1),
        ("A<==>B", LineStyle::Thick, ArrowKind::Arrow, ArrowKind::Arrow, 1),
        ("A<-.->B", LineStyle::Dotted, ArrowKind::Arrow, ArrowKind::Arrow, 1),
        ("A--->B", LineStyle::Solid, ArrowKind::None, ArrowKind::Arrow, 2),
        ("A---->B", LineStyle::Solid, ArrowKind::None, ArrowKind::Arrow, 3),
        ("A----B", LineStyle::Solid, ArrowKind::None, ArrowKind::None, 2),
        ("A-..->B", LineStyle::Dotted, ArrowKind::None, ArrowKind::Arrow, 2),
        ("A-...-B", LineStyle::Dotted, ArrowKind::None, ArrowKind::None, 3),
        ("A===>B", LineStyle::Thick, ArrowKind::None, ArrowKind::Arrow, 2),
        ("A=====B", LineStyle::Thick, ArrowKind::None, ArrowKind::None, 3),
        ("A~~~~B", LineStyle::Invisible, ArrowKind::None, ArrowKind::None, 2),
    ];
    for &(src, line, start, end, length) in cases {
        let e = one_edge(&format!("flowchart LR\n{src}"));
        assert_eq!((e.from.as_str(), e.to.as_str()), ("A", "B"), "{src}");
        assert_eq!(e.line, line, "{src}");
        assert_eq!(e.start, start, "{src}");
        assert_eq!(e.end, end, "{src}");
        assert_eq!(e.length, length, "{src}");
        assert_eq!(e.label, None, "{src}");
    }
}

#[test]
fn link_text_forms() {
    let cases: &[(&str, &str, LineStyle, ArrowKind, u8)] = &[
        ("A-- some text -->B", "some text", LineStyle::Solid, ArrowKind::Arrow, 1),
        ("A-- open text ---B", "open text", LineStyle::Solid, ArrowKind::None, 1),
        ("A-- long --->B", "long", LineStyle::Solid, ArrowKind::Arrow, 2),
        ("A-->|pipe text|B", "pipe text", LineStyle::Solid, ArrowKind::Arrow, 1),
        ("A---|open pipe|B", "open pipe", LineStyle::Solid, ArrowKind::None, 1),
        ("A-. dotted text .->B", "dotted text", LineStyle::Dotted, ArrowKind::Arrow, 1),
        ("A-.->|dotted pipe|B", "dotted pipe", LineStyle::Dotted, ArrowKind::Arrow, 1),
        ("A== thick text ==>B", "thick text", LineStyle::Thick, ArrowKind::Arrow, 1),
        ("A==>|\"quoted pipe\"|B", "quoted pipe", LineStyle::Thick, ArrowKind::Arrow, 1),
        ("A -- spaced --> B", "spaced", LineStyle::Solid, ArrowKind::Arrow, 1),
        ("A -->|spaced pipe| B", "spaced pipe", LineStyle::Solid, ArrowKind::Arrow, 1),
    ];
    for &(src, label, line, end, length) in cases {
        let e = one_edge(&format!("flowchart LR\n{src}"));
        assert_eq!((e.from.as_str(), e.to.as_str()), ("A", "B"), "{src}");
        assert_eq!(e.label.as_deref(), Some(label), "{src}");
        assert_eq!(e.line, line, "{src}");
        assert_eq!(e.end, end, "{src}");
        assert_eq!(e.length, length, "{src}");
    }
}

#[test]
fn chaining_and_fan_out() {
    let f = flow("flowchart LR\nA --> B --> C\nD & E --> F & G\nH --> I & J --> K");
    let pairs: Vec<(&str, &str)> = f.edges.iter().map(|e| (e.from.as_str(), e.to.as_str())).collect();
    assert_eq!(pairs, vec![("A", "B"), ("B", "C"), ("D", "F"), ("D", "G"), ("E", "F"), ("E", "G"), ("H", "I"), ("H", "J"), ("I", "K"), ("J", "K")]);
    assert_eq!(f.nodes.len(), 11);
}

#[test]
fn node_ids_with_dashes_dots_and_unicode() {
    let f = flow("flowchart LR\nmy-node --> other.node\ncafé-1 --> B");
    let ids: Vec<&str> = f.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["my-node", "other.node", "café-1", "B"]);
}

#[test]
fn later_mentions_keep_the_first_definition_unless_redefined() {
    let f = flow("flowchart LR\nA[Start] --> B\nA --> C\nB(Changed)");
    assert_eq!(node(&f, "A").label, "Start");
    assert_eq!(node(&f, "B").label, "Changed");
    assert_eq!(node(&f, "B").shape, Shape::Rounded);
}

#[test]
fn edge_ids() {
    let f = flow("flowchart LR\nA e1@--> B\ne1@{ animate: true }");
    assert_eq!(f.edges.first().and_then(|e| e.id.clone()).as_deref(), Some("e1"));
    assert_eq!(f.nodes.len(), 2);
    let p = ok("flowchart LR\nA e1@--> B\ne1@{ animate: true }");
    assert_eq!(p.warnings.len(), 1);
}

#[test]
fn subgraphs() {
    let f = flow(
        "flowchart TB\n\
         c1-->a2\n\
         subgraph one\n a1-->a2\n end\n\
         subgraph ide2 [Two words]\n direction LR\n b1-->b2\n subgraph inner[\"Inner one\"]\n x1\n end\n end\n\
         subgraph \"Quoted title\"\n q1\n end\n\
         subgraph\n anon\n end\n\
         one --> ide2",
    );
    let ids: Vec<&str> = f.subgraphs.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["one", "ide2", "inner", "Quoted title", "subGraph1"]);
    let one = &f.subgraphs[0];
    assert_eq!(one.title, "one");
    assert_eq!(one.nodes, vec!["a1", "a2"]);
    let two = &f.subgraphs[1];
    assert_eq!(two.title, "Two words");
    assert_eq!(two.direction, Some(Direction::LeftToRight));
    assert_eq!(two.nodes, vec!["b1", "b2"]);
    let inner = &f.subgraphs[2];
    assert_eq!(inner.parent.as_deref(), Some("ide2"));
    assert_eq!(inner.title, "Inner one");
    assert_eq!(inner.nodes, vec!["x1"]);
    assert_eq!(node(&f, "a2").subgraph.as_deref(), Some("one"));
    assert_eq!(node(&f, "c1").subgraph, None);
    // Linking subgraphs by id does not create nodes for them.
    assert!(f.nodes.iter().all(|n| n.id != "one" && n.id != "ide2"));
    let last = f.edges.last().unwrap();
    assert_eq!((last.from.as_str(), last.to.as_str()), ("one", "ide2"));
}

#[test]
fn subgraph_referenced_before_it_is_defined() {
    let f = flow("flowchart LR\nA --> grp\nsubgraph grp\n B\nend");
    assert!(f.nodes.iter().all(|n| n.id != "grp"));
    assert_eq!(f.subgraphs[0].nodes, vec!["B"]);
}

#[test]
fn styling_statements() {
    let f = flow(
        "flowchart LR\n\
         A --> B --> C\n\
         classDef green fill:#9f6,stroke:#333,stroke-width:2px;\n\
         classDef a,b color:red\n\
         class A,B green\n\
         C:::green\n\
         D[Dee]:::b\n\
         style A fill:#f9f,stroke-dasharray: 5\\, 5\n\
         linkStyle 1 stroke:#f00,stroke-width:3px\n\
         linkStyle default interpolate basis stroke:#999\n\
         subgraph S\n E\n end\n\
         style S fill:#eee\n\
         class S green",
    );
    let green = f.class_defs.iter().find(|c| c.name == "green").unwrap();
    assert_eq!(green.props, vec![StyleProp::new("fill", "#9f6"), StyleProp::new("stroke", "#333"), StyleProp::new("stroke-width", "2px")]);
    assert!(f.class_defs.iter().any(|c| c.name == "a") && f.class_defs.iter().any(|c| c.name == "b"));
    assert_eq!(node(&f, "A").classes, vec!["green"]);
    assert_eq!(node(&f, "C").classes, vec!["green"]);
    assert_eq!(node(&f, "D").classes, vec!["b"]);
    assert_eq!(node(&f, "D").label, "Dee");
    assert_eq!(node(&f, "A").style, vec![StyleProp::new("fill", "#f9f"), StyleProp::new("stroke-dasharray", "5, 5")]);
    assert_eq!(f.link_styles.len(), 2);
    assert_eq!(f.link_styles[0].targets, LinkTarget::Indices(vec![1]));
    assert_eq!(f.link_styles[1].targets, LinkTarget::Default);
    assert_eq!(f.link_styles[1].props[0], StyleProp::new("interpolate", "basis"));
    // Default styles first, then the indexed ones.
    assert_eq!(f.edges[0].style, vec![StyleProp::new("interpolate", "basis"), StyleProp::new("stroke", "#999")]);
    assert_eq!(f.edges[1].style.len(), 4);
    assert_eq!(f.edges[1].style[2], StyleProp::new("stroke", "#f00"));
    let s = &f.subgraphs[0];
    assert_eq!(s.style, vec![StyleProp::new("fill", "#eee")]);
    assert_eq!(s.classes, vec!["green"]);
}

#[test]
fn link_style_out_of_range_warns() {
    let p = ok("flowchart LR\nA-->B\nlinkStyle 3 stroke:#f00");
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.warnings[0].line, 3);
}

#[test]
fn clicks() {
    let p = ok(
        "flowchart LR\nA-->B\nclick A \"https://example.com\" \"Tip A\" _blank\nclick B href \"https://example.org\"\nclick A callback \"Tip\"\nclick B call doThing()",
    );
    let Diagram::Flowchart(f) = &p.diagram else { panic!() };
    assert_eq!(f.clicks.len(), 4);
    assert_eq!(f.clicks[0].url.as_deref(), Some("https://example.com"));
    assert_eq!(f.clicks[0].tooltip.as_deref(), Some("Tip A"));
    assert_eq!(f.clicks[0].target.as_deref(), Some("_blank"));
    assert_eq!(f.clicks[1].url.as_deref(), Some("https://example.org"));
    assert_eq!(f.clicks[2].callback.as_deref(), Some("callback"));
    assert_eq!(f.clicks[3].callback.as_deref(), Some("doThing()"));
    // Callbacks are kept only as hints, with a warning each.
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![5, 6]);
}

#[test]
fn unknown_statements_warn_and_are_skipped() {
    let p = ok("flowchart LR\nA --> B\nA B C\nend\nB --> C");
    let Diagram::Flowchart(f) = &p.diagram else { panic!() };
    assert_eq!(f.edges.len(), 2);
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![3, 4]);
    // The skipped statement is applied all-or-nothing: no node `C` from line 3 before line 5.
    assert_eq!(f.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["A", "B", "C"]);
}

#[test]
fn syntax_errors_have_positions() {
    let cases: &[(&str, usize, usize)] = &[
        ("flowchart LR\nA[unclosed --> B", 2, 2),
        ("flowchart LR\n  A --> ", 2, 8),
        ("flowchart LR\nA -- text with no end B", 2, 3),
        ("flowchart LR\nA -->|open pipe B", 2, 6),
        ("flowchart LR\nA & --> B", 2, 4),
        ("flowchart LR\nsubgraph one\nA\n", 2, 1),
        ("flowchart LR\nA-->B; C[x", 2, 9),
        ("flowchart LR\nA@{ shape: rect", 2, 2),
    ];
    for &(src, line, column) in cases {
        let e = parse(src).expect_err(src);
        assert_eq!((e.line, e.column), (line, column), "{src}: {e}");
    }
}

#[test]
fn graph_view() {
    let p = ok(support::FLOWCHART);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let g = p.diagram.to_graph();
    assert_eq!(g.kind, DiagramKind::Flowchart);
    assert_eq!(g.direction, Direction::LeftToRight);
    assert_eq!(g.title.as_deref(), Some("Order handling"));
    assert_eq!(g.edges.len(), 10);
    let start = g.node("start").unwrap();
    assert_eq!(start.shape, Shape::Stadium);
    assert_eq!(start.label, "Order received");
    assert_eq!(start.link.as_deref(), Some("https://example.com/orders"));
    assert_eq!(start.tooltip.as_deref(), Some("Open orders"));
    assert_eq!(g.node("check").unwrap().classes, vec!["urgent"]);
    assert_eq!(g.node("pack").unwrap().group.as_deref(), Some("warehouse"));
    assert_eq!(g.node("ship").unwrap().group.as_deref(), Some("dock"));
    assert_eq!(g.node("a1").unwrap().shape, Shape::Trapezoid);
    assert_eq!(g.node("a2").unwrap().shape, Shape::TrapezoidAlt);
    let wh = g.group("warehouse").unwrap();
    assert_eq!(wh.kind, GroupKind::Subgraph);
    assert_eq!(wh.label, "Warehouse floor");
    assert_eq!(wh.direction, Some(Direction::TopToBottom));
    assert_eq!(wh.members, vec!["pack"]);
    assert_eq!(g.group("dock").unwrap().parent.as_deref(), Some("warehouse"));
    let yes = &g.edges[1];
    assert_eq!(yes.label.as_deref(), Some("yes"));
    let thick = &g.edges[3];
    assert_eq!(thick.line, LineStyle::Thick);
    let first = &g.edges[0];
    assert_eq!(first.style.last(), Some(&StyleProp::new("stroke-width", "4px")));
    assert_eq!(g.class_defs.len(), 1);
    // The graph view serialises.
    let json = serde_json::to_string(&g).unwrap();
    assert!(json.contains("\"Order received\""));
}
