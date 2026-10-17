//! Sequence diagram parsing.

mod support;

use support::ok;
use vizcraft_mermaid::sequence::{BlockKind, BranchKind, MessageArrow, NotePlacement, ParticipantKind, SeqItem, SequenceDiagram};
use vizcraft_mermaid::{ArrowKind, Diagram, DiagramKind, Direction, GroupKind, LineStyle, Shape, parse};

fn seq(text: &str) -> SequenceDiagram {
    match ok(text).diagram {
        Diagram::Sequence(d) => d,
        other => panic!("expected a sequence diagram, got {:?}", other.kind()),
    }
}

fn messages(d: &SequenceDiagram) -> Vec<&vizcraft_mermaid::sequence::Message> {
    d.items
        .iter()
        .filter_map(|i| match i {
            SeqItem::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

#[test]
fn every_arrow() {
    let cases = [
        ("->", MessageArrow::Solid),
        ("-->", MessageArrow::Dotted),
        ("->>", MessageArrow::SolidArrow),
        ("-->>", MessageArrow::DottedArrow),
        ("-x", MessageArrow::SolidCross),
        ("--x", MessageArrow::DottedCross),
        ("-)", MessageArrow::SolidAsync),
        ("--)", MessageArrow::DottedAsync),
        ("<<->>", MessageArrow::BidirectionalSolid),
        ("<<-->>", MessageArrow::BidirectionalDotted),
    ];
    for (tok, arrow) in cases {
        let d = seq(&format!("sequenceDiagram\nAlice{tok}Bob: hello there"));
        let m = messages(&d);
        assert_eq!(m.len(), 1, "{tok}");
        assert_eq!((m[0].from.as_str(), m[0].to.as_str()), ("Alice", "Bob"), "{tok}");
        assert_eq!(m[0].arrow, arrow, "{tok}");
        assert_eq!(m[0].text, "hello there", "{tok}");
    }
}

#[test]
fn participants_actors_and_aliases() {
    let d = seq("sequenceDiagram\nparticipant B as Bob the builder\nactor A\nA->>B: hi\nA->>C: implicit");
    let ids: Vec<&str> = d.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["B", "A", "C"]);
    assert_eq!(d.participants[0].label, "Bob the builder");
    assert_eq!(d.participants[1].kind, ParticipantKind::Actor);
    assert_eq!(d.participants[2].kind, ParticipantKind::Participant);
}

#[test]
fn activations() {
    let d = seq("sequenceDiagram\nA->>+B: call\nB-->>-A: return\nactivate A\ndeactivate A");
    let m = messages(&d);
    assert!(m[0].activate && !m[0].deactivate);
    assert!(m[1].deactivate && !m[1].activate);
    assert!(d.items.contains(&SeqItem::Activate("A".into())));
    assert!(d.items.contains(&SeqItem::Deactivate("A".into())));
}

#[test]
fn notes() {
    let d = seq("sequenceDiagram\nNote left of A: l\nNote right of A: r\nnote over A,B: both");
    let notes: Vec<_> = d
        .items
        .iter()
        .filter_map(|i| match i {
            SeqItem::Note(n) => Some((n.placement, n.participants.clone(), n.text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(notes[0], (NotePlacement::LeftOf, vec!["A".to_string()], "l".to_string()));
    assert_eq!(notes[1].0, NotePlacement::RightOf);
    assert_eq!(notes[2], (NotePlacement::Over, vec!["A".to_string(), "B".to_string()], "both".to_string()));
}

#[test]
fn blocks_nest_and_branch() {
    let d = seq("sequenceDiagram\nloop outer\n alt yes\n  A->>B: x\n else no\n  A->>B: y\n end\nend");
    assert_eq!(d.items.first(), Some(&SeqItem::BlockStart { kind: BlockKind::Loop, label: "outer".into() }));
    assert!(d.items.contains(&SeqItem::Branch { kind: BranchKind::Else, label: "no".into() }));
    assert_eq!(d.items.iter().filter(|i| **i == SeqItem::BlockEnd).count(), 2);
}

#[test]
fn boxes() {
    let d =
        seq("sequenceDiagram\nbox rgb(10,20,30) Back end\nparticipant A\nend\nbox Purple\nparticipant B\nend\nbox Just a title\nparticipant C\nend");
    assert_eq!(d.boxes[0].color.as_deref(), Some("rgb(10,20,30)"));
    assert_eq!(d.boxes[0].label, "Back end");
    assert_eq!(d.boxes[1].color.as_deref(), Some("Purple"));
    assert_eq!(d.boxes[2].color, None);
    assert_eq!(d.boxes[2].label, "Just a title");
    assert_eq!(d.participants[2].in_box, Some(2));
}

#[test]
fn unknown_statements_warn() {
    let p = ok("sequenceDiagram\nA->>B: ok\nnonsense here\nelse stray\nend\nlink A: Dashboard @ https://example.com");
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![3, 4, 5, 6]);
}

#[test]
fn syntax_errors_have_positions() {
    let cases: &[(&str, usize, usize)] =
        &[("sequenceDiagram\nloop forever\nA->>B: x\n", 2, 1), ("sequenceDiagram\n  A->>: x", 2, 7), ("sequenceDiagram\n->>B: x", 2, 1)];
    for &(src, line, column) in cases {
        let e = parse(src).expect_err(src);
        assert_eq!((e.line, e.column), (line, column), "{src}: {e}");
    }
}

#[test]
fn graph_view() {
    let p = ok(support::SEQUENCE);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let Diagram::Sequence(d) = &p.diagram else { panic!() };
    assert!(d.autonumber);
    assert_eq!(d.meta.title.as_deref(), Some("Checkout"));
    assert!(d.participants.iter().any(|p| p.id == "Log" && p.created && p.destroyed));
    let g = p.diagram.to_graph();
    assert_eq!(g.kind, DiagramKind::Sequence);
    assert_eq!(g.direction, Direction::LeftToRight);
    assert_eq!(g.node("User").unwrap().shape, Shape::Actor);
    assert_eq!(g.node("W").unwrap().label, "Web app");
    assert_eq!(g.node("W").unwrap().group.as_deref(), Some("box0"));
    let b = g.group("box0").unwrap();
    assert_eq!(b.kind, GroupKind::Box);
    assert_eq!(b.label, "Front end");
    assert_eq!(b.members, vec!["User", "W"]);
    let ret = g.edges.iter().find(|e| e.label.as_deref() == Some("200 OK")).unwrap();
    assert_eq!(ret.line, LineStyle::Dashed);
    assert_eq!(ret.end, ArrowKind::Arrow);
    let talk = g.edges.iter().find(|e| e.label.as_deref() == Some("talk")).unwrap();
    assert_eq!((talk.start, talk.end), (ArrowKind::Arrow, ArrowKind::Arrow));
    let track = g.edges.iter().find(|e| e.label.as_deref() == Some("track event")).unwrap();
    assert_eq!(track.end, ArrowKind::OpenArrow);
    assert!(g.nodes.iter().any(|n| n.shape == Shape::Note && n.label == "handshake"));
}
