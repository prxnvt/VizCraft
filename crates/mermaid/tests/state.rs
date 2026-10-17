//! State diagram parsing.

mod support;

use support::ok;
use vizcraft_mermaid::state::{NoteSide, StateDiagram, StateKind};
use vizcraft_mermaid::{ArrowKind, Diagram, DiagramKind, Direction, GroupKind, LineStyle, Shape, parse};

fn states(text: &str) -> StateDiagram {
    match ok(text).diagram {
        Diagram::State(s) => s,
        other => panic!("expected a state diagram, got {:?}", other.kind()),
    }
}

fn state<'a>(d: &'a StateDiagram, id: &str) -> &'a vizcraft_mermaid::state::State {
    d.states.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("no state {id}"))
}

#[test]
fn both_headers() {
    assert_eq!(states("stateDiagram\ns1 --> s2").transitions.len(), 1);
    assert_eq!(states("stateDiagram-v2\ns1 --> s2").transitions.len(), 1);
}

#[test]
fn simple_states_and_descriptions() {
    let d = states("stateDiagram-v2\ns1\nstate \"A long name\" as s2\ns3 : first line\ns3 : second line\nstate s4 : declared description");
    assert_eq!(state(&d, "s1").kind, StateKind::Simple);
    assert_eq!(state(&d, "s2").label.as_deref(), Some("A long name"));
    assert_eq!(state(&d, "s3").descriptions, vec!["first line", "second line"]);
    assert_eq!(state(&d, "s4").descriptions, vec!["declared description"]);
}

#[test]
fn transitions_with_labels_and_pseudo_states() {
    let d = states("stateDiagram-v2\n[*] --> Still\nStill --> Moving : push\nMoving-->Still: stop\nMoving --> [*]");
    let t: Vec<(&str, &str, Option<&str>)> = d.transitions.iter().map(|t| (t.from.as_str(), t.to.as_str(), t.label.as_deref())).collect();
    assert_eq!(
        t,
        vec![("[*]start", "Still", None), ("Still", "Moving", Some("push")), ("Moving", "Still", Some("stop")), ("Moving", "[*]end", None)]
    );
    assert_eq!(state(&d, "[*]start").kind, StateKind::Start);
    assert_eq!(state(&d, "[*]end").kind, StateKind::End);
}

#[test]
fn pseudo_state_kinds() {
    let d = states("stateDiagram-v2\nstate c <<choice>>\nstate f <<fork>>\nstate j <<join>>");
    assert_eq!(state(&d, "c").kind, StateKind::Choice);
    assert_eq!(state(&d, "f").kind, StateKind::Fork);
    assert_eq!(state(&d, "j").kind, StateKind::Join);
}

#[test]
fn composite_states_nest() {
    let d = states(
        "stateDiagram-v2\n[*] --> First\nstate First {\n  [*] --> Second\n  state Second {\n    [*] --> Third\n    Third --> [*]\n  }\n}\nstate \"Named\" as N {\n  x\n}",
    );
    assert_eq!(state(&d, "First").kind, StateKind::Composite);
    assert_eq!(state(&d, "Second").parent.as_deref(), Some("First"));
    assert_eq!(state(&d, "Third").parent.as_deref(), Some("Second"));
    assert_eq!(state(&d, "First/[*]start").parent.as_deref(), Some("First"));
    assert_eq!(state(&d, "Second/[*]end").kind, StateKind::End);
    assert_eq!(state(&d, "N").label.as_deref(), Some("Named"));
    assert_eq!(state(&d, "x").parent.as_deref(), Some("N"));
    assert_eq!(state(&d, "[*]start").parent, None);
}

#[test]
fn concurrency_regions() {
    let d = states("stateDiagram-v2\nstate Active {\n a1 --> a2\n --\n b1 --> b2\n --\n c1\n}");
    assert_eq!(state(&d, "Active").regions, 3);
    assert_eq!(state(&d, "a1").region, 0);
    assert_eq!(state(&d, "b2").region, 1);
    assert_eq!(state(&d, "c1").region, 2);
    let g = Diagram::State(d).to_graph();
    let regions: Vec<&str> = g.groups.iter().filter(|g| g.kind == GroupKind::Region).map(|g| g.id.as_str()).collect();
    assert_eq!(regions, vec!["Active/region0", "Active/region1", "Active/region2"]);
    assert_eq!(g.node("b1").unwrap().group.as_deref(), Some("Active/region1"));
    assert_eq!(g.group("Active/region1").unwrap().members, vec!["b1", "b2"]);
    assert_eq!(g.group("Active/region1").unwrap().parent.as_deref(), Some("Active"));
}

#[test]
fn notes() {
    let d = states("stateDiagram-v2\nA\nnote right of A : short note\nnote left of A\n  line one\n  line two\nend note");
    assert_eq!(d.notes.len(), 2);
    assert_eq!(d.notes[0].side, NoteSide::Right);
    assert_eq!(d.notes[0].text, "short note");
    assert_eq!(d.notes[1].side, NoteSide::Left);
    assert_eq!(d.notes[1].text, "line one\nline two");
}

#[test]
fn directions_and_styles() {
    let d = states(
        "stateDiagram-v2\ndirection LR\nstate C {\n direction TB\n x\n}\nclassDef hot fill:#f00,color:white\nclass x,y hot\nz:::hot\n[*] --> w:::hot\nstyle x stroke:#000",
    );
    assert_eq!(d.direction, Direction::LeftToRight);
    assert_eq!(state(&d, "C").direction, Some(Direction::TopToBottom));
    assert_eq!(state(&d, "x").classes, vec!["hot"]);
    assert_eq!(state(&d, "y").classes, vec!["hot"]);
    assert_eq!(state(&d, "z").classes, vec!["hot"]);
    assert_eq!(state(&d, "w").classes, vec!["hot"]);
    assert_eq!(state(&d, "x").style.len(), 1);
    assert_eq!(d.class_defs[0].props.len(), 2);
}

#[test]
fn rendering_options_are_accepted_silently() {
    let p = ok("stateDiagram-v2\nhide empty description\nscale 350 width\nA --> B");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

#[test]
fn unknown_statements_warn() {
    let p = ok("stateDiagram-v2\nA --> B\nthis is not valid\n}\n--\nstate X <<weird>>\nnote over A : nope\nA --> B extra");
    assert_eq!(p.warnings.iter().map(|w| w.line).collect::<Vec<_>>(), vec![3, 4, 5, 6, 7, 8]);
}

#[test]
fn syntax_errors_have_positions() {
    let cases: &[(&str, usize, usize)] = &[
        ("stateDiagram-v2\nstate A {\n B\n", 2, 1),
        ("stateDiagram-v2\nA -->", 2, 6),
        ("stateDiagram-v2\n  --> B", 2, 3),
        ("stateDiagram-v2\nnote left of A\n text", 2, 1),
        ("stateDiagram-v2\nstate \"unterminated as A", 2, 7),
    ];
    for &(src, line, column) in cases {
        let e = parse(src).expect_err(src);
        assert_eq!((e.line, e.column), (line, column), "{src}: {e}");
    }
}

#[test]
fn graph_view() {
    let p = ok(support::STATE);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let g = p.diagram.to_graph();
    assert_eq!(g.kind, DiagramKind::State);
    assert_eq!(g.direction, Direction::LeftToRight);
    assert_eq!(g.node("[*]start").unwrap().shape, Shape::Start);
    assert_eq!(g.node("[*]end").unwrap().shape, Shape::End);
    let running = g.node("Running").unwrap();
    assert_eq!(running.label, "Running the job");
    assert_eq!(running.details, vec!["progress shown"]);
    assert_eq!(running.shape, Shape::Rounded);
    let check = g.node("Check").unwrap();
    assert_eq!(check.shape, Shape::Choice);
    assert_eq!(check.annotations, vec!["choice"]);
    assert_eq!(g.node("Split").unwrap().shape, Shape::ForkBar);
    assert_eq!(g.node("Merge").unwrap().annotations, vec!["join"]);
    assert_eq!(g.node("Failed").unwrap().classes, vec!["bad"]);
    assert_eq!(g.node("Done").unwrap().classes, vec!["good"]);
    // Composite states are groups, not nodes.
    assert!(g.node("Active").is_none());
    let active = g.group("Active").unwrap();
    assert_eq!(active.kind, GroupKind::CompositeState);
    assert_eq!(g.node("Working").unwrap().group.as_deref(), Some("Active/region0"));
    assert_eq!(g.node("Logging").unwrap().group.as_deref(), Some("Active/region1"));
    assert!(g.node("Active/region1/[*]end").is_some());
    let pause = g.edges.iter().find(|e| e.label.as_deref() == Some("pause")).unwrap();
    assert_eq!((pause.source.as_str(), pause.target.as_str()), ("Running", "Paused"));
    assert_eq!(pause.end, ArrowKind::Arrow);
    // Notes are note nodes joined to their state by a dotted edge.
    let note = g.nodes.iter().find(|n| n.shape == Shape::Note && n.label.contains("timeout")).unwrap();
    let link = g.edges.iter().find(|e| e.source == note.id).unwrap();
    assert_eq!(link.target, "Paused");
    assert_eq!(link.line, LineStyle::Dotted);
    assert_eq!(link.end, ArrowKind::None);
}
