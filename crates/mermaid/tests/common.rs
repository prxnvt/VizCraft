//! Header detection, preprocessing, errors and limits shared by every diagram type.

mod support;

use support::ok;
use vizcraft_mermaid::{Diagram, DiagramKind, limits, parse};

#[test]
fn detects_each_type() {
    let cases = [
        ("flowchart TD\nA", DiagramKind::Flowchart),
        ("graph LR\nA", DiagramKind::Flowchart),
        ("stateDiagram\nA", DiagramKind::State),
        ("stateDiagram-v2\nA", DiagramKind::State),
        ("erDiagram\nA", DiagramKind::Er),
        ("classDiagram\nclass A", DiagramKind::Class),
        ("classDiagram-v2\nclass A", DiagramKind::Class),
        ("sequenceDiagram\nA->>B: x", DiagramKind::Sequence),
    ];
    for (src, kind) in cases {
        assert_eq!(ok(src).diagram.kind(), kind, "{src}");
    }
}

#[test]
fn skips_front_matter_comments_and_directives() {
    let src = "\n\n---\ntitle: \"My chart\"\nconfig:\n  theme: forest\n---\n%% a comment\n%%{init: {\"flowchart\": {\"curve\": \"basis\"}}}%%\n%%{\n  init: {}\n}%%\n   flowchart LR\n%% inside\nA --> B\n";
    let p = ok(src);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(p.diagram.meta().title.as_deref(), Some("My chart"));
    let Diagram::Flowchart(f) = &p.diagram else { panic!() };
    assert_eq!(f.edges.len(), 1);
}

#[test]
fn accessibility_text() {
    let p = ok("erDiagram\naccTitle: Shop data\naccDescr {\n  Customers place\n  orders.\n}\nA ||--o{ B : r");
    let m = p.diagram.meta();
    assert_eq!(m.acc_title.as_deref(), Some("Shop data"));
    assert_eq!(m.acc_descr.as_deref(), Some("Customers place\norders."));
    let p = ok("classDiagram\naccDescr: one line\nclass A");
    assert_eq!(p.diagram.meta().acc_descr.as_deref(), Some("one line"));
}

#[test]
fn windows_line_endings_and_bom() {
    let p = ok("\u{feff}flowchart LR\r\nA --> B\r\nB --> C\r\n");
    let Diagram::Flowchart(f) = &p.diagram else { panic!() };
    assert_eq!(f.edges.len(), 2);
}

#[test]
fn empty_and_comment_only_input_is_an_error() {
    for src in ["", "   \n\n", "%% only a comment\n%% and another"] {
        let e = parse(src).unwrap_err();
        assert_eq!((e.line, e.column), (1, 1), "{src:?}");
    }
}

#[test]
fn unclosed_front_matter_is_an_error() {
    let e = parse("\n---\ntitle: x\nflowchart LR\nA").unwrap_err();
    assert_eq!((e.line, e.column), (2, 1));
}

#[test]
fn unsupported_and_unknown_types() {
    let e = parse("%% c\n  gantt\n  title x").unwrap_err();
    assert_eq!((e.line, e.column), (2, 3));
    assert!(e.message.contains("not supported"), "{e}");
    let e = parse("pie title Pets").unwrap_err();
    assert!(e.message.contains("not supported"), "{e}");
    let e = parse("hello world").unwrap_err();
    assert!(e.message.contains("unknown diagram type"), "{e}");
    assert_eq!(e.to_string(), format!("line 1, column 1: {}", e.message));
}

#[test]
fn oversized_input_is_rejected() {
    let mut big = String::from("flowchart LR\n");
    while big.len() <= limits::MAX_INPUT_BYTES {
        big.push_str("%% padding padding padding padding padding padding padding\n");
    }
    let e = parse(&big).unwrap_err();
    assert!(e.message.contains("limit"), "{e}");
}

#[test]
fn statement_cap() {
    let mut src = String::from("flowchart LR\n");
    for _ in 0..=limits::MAX_STATEMENTS {
        src.push_str("A\n");
    }
    let e = parse(&src).unwrap_err();
    assert!(e.message.contains("statements"), "{e}");
}

#[test]
fn edge_cap_from_fan_out() {
    // 250 x 250 = 62,500 links from one statement passes the edge cap.
    let left: Vec<String> = (0..250).map(|i| format!("a{i}")).collect();
    let right: Vec<String> = (0..250).map(|i| format!("b{i}")).collect();
    let src = format!("flowchart LR\n{} --> {}", left.join(" & "), right.join(" & "));
    let e = parse(&src).unwrap_err();
    assert!(e.message.contains("edges"), "{e}");
}

#[test]
fn node_cap() {
    let mut src = String::from("flowchart LR\n");
    let per_line = 10;
    let mut n = 0;
    while n <= limits::MAX_NODES {
        let ids: Vec<String> = (0..per_line).map(|k| format!("n{}", n + k)).collect();
        src.push_str(&ids.join(" & "));
        src.push('\n');
        n += per_line;
    }
    let e = parse(&src).unwrap_err();
    assert!(e.message.contains("nodes"), "{e}");
}

#[test]
fn nesting_cap() {
    let depth = limits::MAX_DEPTH + 1;
    let flow = format!("flowchart LR\n{}A\n{}", "subgraph s\n".repeat(depth), "end\n".repeat(depth));
    let mut flow_ids = String::from("flowchart LR\n");
    for i in 0..depth {
        flow_ids.push_str(&format!("subgraph s{i}\n"));
    }
    let state = format!("stateDiagram-v2\n{}", (0..depth).map(|i| format!("state S{i} {{\n")).collect::<String>());
    let class = format!("classDiagram\n{}", (0..depth).map(|i| format!("namespace N{i} {{\n")).collect::<String>());
    let seq = format!("sequenceDiagram\n{}", "loop x\n".repeat(depth));
    // Reopening one subgraph id repeatedly still counts as nesting.
    for src in [flow, flow_ids, state, class, seq] {
        let e = parse(&src).unwrap_err();
        assert!(e.message.contains("nested deeper"), "{e}");
    }
}

#[test]
fn warning_cap_summarises() {
    let mut src = String::from("flowchart LR\n");
    for _ in 0..(limits::MAX_WARNINGS + 10) {
        src.push_str("end\n");
    }
    let p = ok(&src);
    assert_eq!(p.warnings.len(), limits::MAX_WARNINGS + 1);
    assert!(p.warnings.last().unwrap().message.contains("10 more warnings"));
}

#[test]
fn parsed_values_serialise_round_trip() {
    for src in support::ALL {
        let p = ok(src);
        let json = serde_json::to_string(&p).unwrap();
        let back: vizcraft_mermaid::Parsed = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        let g = p.diagram.to_graph();
        let gj = serde_json::to_string(&g).unwrap();
        let gb: vizcraft_mermaid::Graph = serde_json::from_str(&gj).unwrap();
        assert_eq!(gb, g);
    }
}
