//! Behavioural tests for the sequence diagram layout.

use vizcraft_layout::sequence::{MessageSort, NoteSide, SeqEvent, SeqInput, SeqLayout, SeqLifeline, SeqOptions, layout};
use vizcraft_layout::{Edge, Graph, LayoutError, Node, Options, Primitive, layout_primitive};

fn lifelines(n: usize) -> Vec<SeqLifeline> {
    (0..n).map(|_| SeqLifeline { width: 100.0, head: 40.0, actor: false, destroyed: false }).collect()
}

fn msg(from: usize, to: usize, sort: MessageSort) -> SeqEvent {
    SeqEvent::Message { from, to, sort, label: (60.0, 16.0) }
}

fn run(n: usize, events: Vec<SeqEvent>) -> SeqLayout {
    layout(&SeqInput { lifelines: lifelines(n), events }, &SeqOptions::default()).unwrap()
}

fn open(forks: bool) -> SeqEvent {
    SeqEvent::Open { forks, covers: Vec::new(), label: 40.0 }
}

#[test]
fn columns_increase_by_at_least_the_gap() {
    let l = run(4, vec![msg(0, 3, MessageSort::Sync)]);
    for w in l.columns.windows(2) {
        assert!(w[1] - w[0] >= 100.0 + 60.0 - 1e-9, "{:?}", l.columns);
    }
    for (h, c) in l.heads.iter().zip(&l.columns) {
        assert!((h.x + h.width / 2.0 - c).abs() < 1e-9);
    }
    assert!(l.bounds.x.abs() < 1e-9 && l.heads[0].x.abs() < 1e-9, "a packed layout starts at 0");
}

#[test]
fn rows_strictly_increase() {
    let events = vec![
        msg(0, 1, MessageSort::Sync),
        msg(1, 2, MessageSort::Async),
        open(true),
        msg(2, 1, MessageSort::Sync),
        SeqEvent::Else { label: 30.0 },
        SeqEvent::Note { over: vec![1], side: NoteSide::Right, size: (80.0, 30.0) },
        SeqEvent::Close,
        msg(1, 0, MessageSort::Reply),
    ];
    let l = run(3, events);
    assert_eq!(l.rows.len(), 8);
    for w in l.rows.windows(2) {
        assert!(w[1] > w[0], "{:?}", l.rows);
    }
    for (r, route) in l.rows.iter().zip(&l.routes) {
        if let Some(p) = route.first() {
            assert!((p.y - r).abs() < 1e-9, "a message starts on its row");
        }
        if route.len() == 2 {
            assert!((route[0].y - route[1].y).abs() < 1e-9, "messages are horizontal");
        }
    }
}

#[test]
fn nested_calls_nest_their_bars() {
    let l = run(3, vec![msg(0, 1, MessageSort::Sync), msg(1, 2, MessageSort::Sync), msg(2, 1, MessageSort::Reply), msg(1, 0, MessageSort::Reply)]);
    assert_eq!(l.activations.len(), 2);
    let b = l.activations.iter().find(|a| a.lifeline == 1).unwrap();
    let c = l.activations.iter().find(|a| a.lifeline == 2).unwrap();
    assert_eq!((b.depth, c.depth), (1, 1));
    assert!(b.y0 < c.y0 && c.y1 < b.y1, "{b:?} {c:?}");
    assert!((b.y1 - l.rows[3]).abs() < 1e-9 && (c.y1 - l.rows[2]).abs() < 1e-9);
    // The call into B ends at B's bar, the call out of B leaves from it.
    assert!((l.routes[0][1].x - (l.columns[1] - 5.0)).abs() < 1e-9);
    assert!((l.routes[1][0].x - (l.columns[1] + 5.0)).abs() < 1e-9);
}

#[test]
fn a_self_call_nests_at_depth_two() {
    let l = run(2, vec![msg(0, 1, MessageSort::Sync), msg(1, 1, MessageSort::Sync), msg(1, 1, MessageSort::Reply), msg(1, 0, MessageSort::Reply)]);
    let depths: Vec<usize> = l.activations.iter().map(|a| a.depth).collect();
    assert_eq!(depths, vec![1, 2]);
    assert_eq!(l.routes[1].len(), 4, "a self message has two bends");
    assert!(l.routes[1][1].x > l.routes[1][0].x);
}

#[test]
fn alt_operands_fork_so_two_replies_give_one_bar() {
    let events = vec![
        msg(0, 1, MessageSort::Sync),
        open(true),
        msg(1, 0, MessageSort::Reply),
        SeqEvent::Else { label: 30.0 },
        msg(1, 0, MessageSort::Reply),
        SeqEvent::Close,
    ];
    let l = run(2, events);
    assert_eq!(l.activations.len(), 1);
    assert!((l.activations[0].y1 - l.rows[4]).abs() < 1e-9, "the bar reaches the second reply");
    assert!((l.routes[4][0].x - (l.columns[1] - 5.0)).abs() < 1e-9, "the second reply leaves from the bar");
    assert_eq!(l.separators[0].len(), 1);
}

#[test]
fn fragments_contain_their_routes_and_nest_with_margin() {
    let events = vec![
        open(false),
        msg(0, 1, MessageSort::Sync),
        open(true),
        msg(1, 1, MessageSort::Async),
        msg(1, 2, MessageSort::Async),
        SeqEvent::Close,
        SeqEvent::Close,
    ];
    let l = run(3, events);
    let (outer, inner) = (l.fragments[0], l.fragments[1]);
    for (k, route) in l.routes.iter().enumerate() {
        for p in route {
            assert!(p.x > outer.x && p.x < outer.right() && p.y > outer.y && p.y < outer.bottom(), "route {k} {p:?} in {outer:?}");
            if (3..5).contains(&k) {
                assert!(p.x > inner.x && p.x < inner.right(), "route {k} in {inner:?}");
            }
        }
    }
    assert!(inner.x - outer.x >= 8.0 - 1e-9 && outer.right() - inner.right() >= 8.0 - 1e-9, "{outer:?} {inner:?}");
    assert!(inner.y - outer.y >= 8.0 && outer.bottom() - inner.bottom() >= 8.0, "{outer:?} {inner:?}");
}

#[test]
fn create_and_destroy_rows_are_honoured() {
    let l =
        run(3, vec![msg(0, 1, MessageSort::Sync), msg(1, 2, MessageSort::Create), msg(1, 2, MessageSort::Destroy), msg(1, 0, MessageSort::Reply)]);
    let head = l.heads[2];
    assert!((head.center().y - l.rows[1]).abs() < 1e-9, "the created head is centred on its row");
    assert!(head.y > l.heads[0].y);
    assert!((l.stems[2].1 - l.rows[2]).abs() < 1e-9, "the stem ends at the destroy row");
    assert!(l.stems[0].1 > l.rows[3]);
    let mut destroyed = lifelines(2);
    destroyed[1].destroyed = true;
    let l = layout(
        &SeqInput { lifelines: destroyed, events: vec![msg(0, 1, MessageSort::Async), msg(0, 0, MessageSort::Async)] },
        &SeqOptions::default(),
    )
    .unwrap();
    assert!((l.stems[1].1 - l.rows[0]).abs() < 1e-9, "a destroyed lifeline ends at its last event");
}

#[test]
fn unbalanced_or_out_of_range_input_is_an_error() {
    let bad = |n: usize, events: Vec<SeqEvent>| layout(&SeqInput { lifelines: lifelines(n), events }, &SeqOptions::default());
    for events in [
        vec![SeqEvent::Close],
        vec![SeqEvent::Else { label: 0.0 }],
        vec![open(true)],
        vec![msg(0, 5, MessageSort::Sync)],
        vec![SeqEvent::Activate(9)],
        vec![SeqEvent::Note { over: vec![], side: NoteSide::Over, size: (1.0, 1.0) }],
    ] {
        assert!(matches!(bad(2, events.clone()), Err(LayoutError::InvalidRoles { primitive: "sequence", .. })), "{events:?}");
    }
}

#[test]
fn too_many_events_is_an_error_and_junk_sizes_do_not_panic() {
    let events: Vec<SeqEvent> = (0..5000).map(|_| msg(0, 1, MessageSort::Async)).collect();
    assert!(layout(&SeqInput { lifelines: lifelines(2), events }, &SeqOptions::default()).is_err());
    let junk = vec![
        SeqLifeline { width: f64::NAN, head: f64::INFINITY, actor: true, destroyed: false },
        SeqLifeline { width: -5.0, head: 1e300, actor: false, destroyed: true },
    ];
    let events = vec![
        SeqEvent::Message { from: 0, to: 1, sort: MessageSort::Sync, label: (f64::NAN, f64::INFINITY) },
        SeqEvent::Note { over: vec![0, 1], side: NoteSide::Over, size: (f64::NAN, -1.0) },
    ];
    let opts = SeqOptions { row: f64::NAN, column_gap: -3.0, activation_width: 1e9, auto_activations: true, keep_columns: Some(vec![f64::NAN, 1.0]) };
    let l = layout(&SeqInput { lifelines: junk, events }, &opts).unwrap();
    assert!(l.columns.iter().all(|c| c.is_finite()) && l.rows.iter().all(|r| r.is_finite()));
}

#[test]
fn kept_columns_stay_and_are_only_pushed_apart() {
    let opts = SeqOptions { keep_columns: Some(vec![500.0, 900.0, 910.0]), ..SeqOptions::default() };
    let l = layout(&SeqInput { lifelines: lifelines(3), events: vec![msg(0, 1, MessageSort::Async)] }, &opts).unwrap();
    assert_eq!(&l.columns[..2], &[500.0, 900.0]);
    assert!(l.columns[2] >= 900.0 + 160.0 - 1e-9);
}

#[test]
fn the_primitive_puts_heads_in_a_row() {
    let node = Node { width: 80.0, height: 40.0, pin: None, order: None };
    let graph = Graph {
        nodes: vec![node; 3],
        edges: vec![Edge { source: 2, target: 0, weight: 1.0, min_len: 1 }, Edge { source: 0, target: 1, weight: 1.0, min_len: 1 }],
        groups: Vec::new(),
    };
    let l = layout_primitive(&graph, &Primitive::Sequence { lifelines: Some(vec![2, 0]) }, &Options::default()).unwrap();
    assert!(l.nodes[2].x < l.nodes[0].x && l.nodes[0].x < l.nodes[1].x, "{:?}", l.nodes);
    assert!(l.nodes.iter().all(|p| p.y.abs() < 1e-9));
    assert!(layout_primitive(&graph, &Primitive::Sequence { lifelines: Some(vec![0, 0]) }, &Options::default()).is_err());
    assert!(layout_primitive(&graph, &Primitive::Sequence { lifelines: None }, &Options::default()).is_ok());
}
