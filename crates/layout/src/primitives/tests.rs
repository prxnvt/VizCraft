//! Each primitive places a small graph as its module says, with no overlaps, and refuses bad roles.

use super::{Nest, Parallel, Primitive};
use crate::{Direction, Edge, Graph, Layout, LayoutError, Node, Options, Rect, layout_primitive};

fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    Graph {
        nodes: (0..n).map(|i| Node::new(60.0 + (i % 3) as f64 * 20.0, 30.0 + (i % 2) as f64 * 10.0)).collect(),
        edges: edges.iter().map(|&(s, t)| Edge::new(s, t)).collect(),
        groups: Vec::new(),
    }
}

fn rects(g: &Graph, out: &Layout) -> Vec<Rect> {
    let mut sizes: Vec<(f64, f64)> = g.nodes.iter().map(|n| (n.width, n.height)).collect();
    for r in &out.resized {
        sizes[r.node] = (r.width, r.height);
    }
    sizes.iter().zip(&out.nodes).map(|(&(w, h), p)| Rect::new(p.x, p.y, w, h)).collect()
}

/// Lays out and checks the shared contract: normalised, inside the bounds, no overlaps except
/// between a container and what it holds.
fn run(g: &Graph, p: &Primitive, opts: &Options, nested_in: &[(usize, usize)]) -> (Layout, Vec<Rect>) {
    let out = layout_primitive(g, p, opts).unwrap();
    let r = rects(g, &out);
    assert_eq!(out.nodes.len(), g.nodes.len());
    assert_eq!(out.edges.len(), g.edges.len());
    assert!(out.bounds.x == 0.0 && out.bounds.y == 0.0);
    for (i, a) in r.iter().enumerate() {
        assert!(
            a.x >= -1e-6 && a.y >= -1e-6 && a.right() <= out.bounds.width + 1e-6 && a.bottom() <= out.bounds.height + 1e-6,
            "{} node {i} outside bounds",
            p.name()
        );
        for (j, b) in r.iter().enumerate().skip(i + 1) {
            let nested = nested_in.iter().any(|&(c, m)| (c, m) == (i, j) || (c, m) == (j, i));
            assert!(nested || !a.overlaps(b, 1e-6), "{}: {i} and {j} overlap", p.name());
        }
    }
    (out, r)
}

#[test]
fn bands_do_not_interleave_and_keep_the_flow_across_bands() {
    // 0 -> 1 -> 2 -> 3 alternating between two lanes, plus 4 in lane 0.
    let g = graph(5, &[(0, 1), (1, 2), (2, 3), (0, 4)]);
    for direction in [Direction::LeftRight, Direction::TopBottom] {
        let opts = Options { direction, ..Options::default() };
        let p = Primitive::Bands { bands: vec![vec![0, 2, 4], vec![1, 3]] };
        let (_, r) = run(&g, &p, &opts, &[]);
        let horizontal = direction.is_horizontal();
        let lo = |r: &Rect| if horizontal { r.y } else { r.x };
        let hi = |r: &Rect| if horizontal { r.bottom() } else { r.right() };
        let first_end = [0, 2, 4].iter().map(|&v| hi(&r[v])).fold(f64::MIN, f64::max);
        let second_start = [1, 3].iter().map(|&v| lo(&r[v])).fold(f64::MAX, f64::min);
        assert!(first_end < second_start, "{direction:?}: bands interleave");
        let along = |v: usize| if horizontal { r[v].x } else { r[v].y };
        assert!(along(0) < along(1) && along(1) < along(2) && along(2) < along(3), "{direction:?}: the flow runs across bands");
    }
    assert!(layout_primitive(&g, &Primitive::Bands { bands: vec![vec![0, 1, 2, 3]] }, &Options::default()).is_err(), "node 4 has no band");
    assert!(layout_primitive(&g, &Primitive::Bands { bands: vec![vec![0, 1, 2, 3, 4, 4]] }, &Options::default()).is_err(), "a node in two places");
}

#[test]
fn the_parallel_band_fans_out_between_two_steps_and_joins() {
    // start -> a, b, c -> join -> end.
    let g = graph(6, &[(0, 1), (0, 2), (0, 3), (1, 4), (2, 4), (3, 4), (4, 5)]);
    let opts = Options { direction: Direction::LeftRight, ..Options::default() };
    let p = Primitive::Pipeline { parallel: Some(Parallel { from: 0, to: 4, members: vec![1, 2, 3] }) };
    let (_, r) = run(&g, &p, &opts, &[]);
    for m in [1, 2, 3] {
        assert!(r[m].x > r[0].right() && r[m].right() < r[4].x, "member {m} sits between from and to");
    }
    assert!(r[1].bottom() <= r[2].y && r[2].bottom() <= r[3].y, "members fan out across the flow");
    let mid = |v: usize| r[v].center().y;
    assert!(
        (mid(0) - mid(4)).abs() < 1e-6 && (mid(4) - mid(5)).abs() < 1e-6 && (mid(2) - mid(0)).abs() < 1e-6,
        "steps on one axis, the band centred on it"
    );
    assert!(r[4].right() < r[5].x);
    let bad = Primitive::Pipeline { parallel: Some(Parallel { from: 0, to: 5, members: vec![1, 2, 3] }) };
    assert!(matches!(layout_primitive(&g, &bad, &opts), Err(LayoutError::InvalidRoles { .. })), "from and to not consecutive");
    let bad = Primitive::Pipeline { parallel: Some(Parallel { from: 0, to: 4, members: vec![1, 9] }) };
    assert!(layout_primitive(&g, &bad, &opts).is_err(), "unknown member");
}

#[test]
fn compensation_rows_align_under_their_steps() {
    let g = graph(5, &[(0, 1), (1, 2), (0, 3), (1, 4)]);
    let opts = Options { direction: Direction::LeftRight, ..Options::default() };
    let p = Primitive::Chain { forward: vec![0, 1, 2], compensation: vec![Some(3), Some(4), None] };
    let (_, r) = run(&g, &p, &opts, &[]);
    assert!(r[0].right() < r[1].x && r[1].right() < r[2].x);
    for (f, c) in [(0, 3), (1, 4)] {
        assert!((r[f].center().x - r[c].center().x).abs() < 1e-6, "compensation {c} under step {f}");
        assert!(r[c].y > r[f].bottom());
    }
    assert!(layout_primitive(&g, &Primitive::Chain { forward: vec![0, 1, 2], compensation: vec![Some(3)] }, &opts).is_err(), "node 4 has no role");
}

#[test]
fn the_matrix_has_equal_cells() {
    let g = graph(6, &[]);
    let p = Primitive::Matrix { rows: Some(vec![vec![Some(0), Some(1), Some(2)], vec![Some(3), None, Some(4)], vec![Some(5)]]), cols: None };
    let (_, r) = run(&g, &p, &Options::default(), &[]);
    let cx = |v: usize| r[v].center().x;
    let cy = |v: usize| r[v].center().y;
    let pitch_x = cx(1) - cx(0);
    assert!(pitch_x > 0.0 && ((cx(2) - cx(1)) - pitch_x).abs() < 1e-6 && ((cx(4) - cx(3)) - 2.0 * pitch_x).abs() < 1e-6);
    let pitch_y = cy(3) - cy(0);
    assert!(pitch_y > 0.0 && ((cy(5) - cy(3)) - pitch_y).abs() < 1e-6 && (cx(5) - cx(0)).abs() < 1e-6);
    let by_cols = Primitive::Matrix { rows: None, cols: Some(vec![vec![Some(0), Some(3), Some(5)], vec![Some(1)], vec![Some(2), Some(4)]]) };
    assert_eq!(layout_primitive(&g, &by_cols, &Options::default()).unwrap().nodes, layout_primitive(&g, &p, &Options::default()).unwrap().nodes);
    assert!(layout_primitive(&g, &Primitive::Matrix { rows: None, cols: None }, &Options::default()).is_err());
}

#[test]
fn the_timeline_spaces_events_equally_with_labels_below() {
    let g = graph(5, &[(0, 1), (1, 2)]);
    let opts = Options { direction: Direction::LeftRight, ..Options::default() };
    let p = Primitive::Timeline { events: None, labels: vec![Some(3), None, Some(4)] };
    let (_, r) = run(&g, &p, &opts, &[]);
    let c = |v: usize| r[v].center().x;
    assert!(((c(1) - c(0)) - (c(2) - c(1))).abs() < 1e-6 && c(1) > c(0));
    assert!((c(3) - c(0)).abs() < 1e-6 && r[3].y >= r[0].bottom() && (c(4) - c(2)).abs() < 1e-6);
}

#[test]
fn nested_containers_contain_their_children_with_padding() {
    // 0 holds 1 and 2; 2 holds 3 and 4; 5 is on the top level; edges 3 -> 4 and 1 -> 5.
    let g = graph(6, &[(3, 4), (1, 5), (1, 3)]);
    let p = Primitive::Nested {
        containers: vec![
            Nest { id: 0, members: vec![1, 2], padding: Some(15.0), header: Some(10.0) },
            Nest { id: 2, members: vec![3, 4], padding: None, header: None },
        ],
    };
    let (out, r) = run(&g, &p, &Options::default(), &[(0, 1), (0, 2), (0, 3), (0, 4), (2, 3), (2, 4)]);
    assert_eq!(out.resized.iter().map(|x| x.node).collect::<Vec<_>>(), vec![0, 2]);
    let inside = |outer: &Rect, inner: &Rect, pad: f64, head: f64| {
        inner.x >= outer.x + pad - 1e-6
            && inner.y >= outer.y + pad + head - 1e-6
            && inner.right() <= outer.right() - pad + 1e-6
            && inner.bottom() <= outer.bottom() - pad + 1e-6
    };
    for m in [1, 2] {
        assert!(inside(&r[0], &r[m], 15.0, 10.0), "{m} inside 0");
    }
    for m in [3, 4] {
        assert!(inside(&r[2], &r[m], 20.0, 0.0), "{m} inside 2");
    }
    assert!(!r[5].overlaps(&r[0], 1e-6));
    assert!(r[3].bottom() < r[4].y, "children laid out layered inside their container");
    let cycle = Primitive::Nested {
        containers: vec![
            Nest { id: 0, members: vec![1], padding: None, header: None },
            Nest { id: 1, members: vec![0], padding: None, header: None },
        ],
    };
    assert!(layout_primitive(&g, &cycle, &Options::default()).is_err(), "containers holding each other");
}

#[test]
fn spokes_clear_the_hub_and_rings_clear_each_other() {
    let g = graph(7, &[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6)]);
    let (_, r) = run(&g, &Primitive::HubSpoke { hub: 3 }, &Options::default(), &[]);
    let hub = r[3].center();
    let d = |v: usize| (r[v].center().x - hub.x).hypot(r[v].center().y - hub.y);
    let radius = d(0);
    for v in [0, 1, 2, 4, 5, 6] {
        assert!((d(v) - radius).abs() < 1e-6, "spokes on one circle");
        assert!(!r[v].overlaps(&r[3], 1e-6));
    }
    let (_, r) = run(&g, &Primitive::Rings { center: 0, rings: vec![vec![1, 2, 3], vec![4, 5, 6]] }, &Options::default(), &[]);
    let c = r[0].center();
    let d = |v: usize| (r[v].center().x - c.x).hypot(r[v].center().y - c.y);
    assert!(d(4) > d(1) && (d(1) - d(2)).abs() < 1e-6);
    assert!(layout_primitive(&g, &Primitive::Rings { center: 0, rings: vec![vec![1, 2, 3]] }, &Options::default()).is_err(), "unplaced nodes");
    assert!(layout_primitive(&g, &Primitive::HubSpoke { hub: 99 }, &Options::default()).is_err());
}

#[test]
fn channels_feed_the_voter() {
    // Three channels of two steps, all feeding 6.
    let g = graph(7, &[(0, 1), (2, 3), (4, 5), (1, 6), (3, 6), (5, 6)]);
    let p = Primitive::Voter { channels: vec![vec![0, 1], vec![2, 3], vec![4, 5]], voter: 6 };
    let (_, r) = run(&g, &p, &Options::default(), &[]);
    for (a, b) in [(0, 1), (2, 3), (4, 5)] {
        assert!(r[a].bottom() < r[b].y && r[b].bottom() < r[6].y, "channel {a} runs down into the voter");
    }
    assert!(r[1].right() < r[3].x && r[3].right() < r[5].x, "channels side by side");
    let left = r[..6].iter().map(|x| x.x).fold(f64::MAX, f64::min);
    let right = r[..6].iter().map(|x| x.right()).fold(f64::MIN, f64::max);
    assert!((r[6].center().x - (left + right) / 2.0).abs() < 1e-6, "the voter centred under the channels");
    assert!(layout_primitive(&g, &Primitive::Voter { channels: vec![vec![0, 1], vec![]], voter: 6 }, &Options::default()).is_err());
}

#[test]
fn roles_round_trip_through_json_and_map_ids() {
    let p: Primitive<String> = serde_json::from_value(serde_json::json!({"primitive": "hubSpoke", "roles": {"hub": "s3"}})).unwrap();
    assert_eq!(p.name(), "hubSpoke");
    let mapped = p.try_map(&mut |id: String| id.trim_start_matches('s').parse::<usize>().map_err(|_| ())).unwrap();
    assert_eq!(mapped, Primitive::HubSpoke { hub: 3 });
    let p: Primitive<String> = serde_json::from_value(serde_json::json!({"primitive": "pipeline", "roles": {}})).unwrap();
    assert!(p.references().is_empty());
    assert!(serde_json::from_value::<Primitive<String>>(serde_json::json!({"primitive": "spiral", "roles": {}})).is_err());
}

#[test]
fn a_two_way_pair_across_a_container_keeps_its_first_direction() {
    // 0 holds 1 and 2; 3 and 4 are outside. 3 talks to 1 and back, 2 talks to 4 and back: the
    // first flow of each pair sets the order, so 3 comes before the container and 4 after it.
    let g = graph(5, &[(3, 1), (1, 3), (1, 2), (2, 4), (4, 2)]);
    let p = Primitive::Nested { containers: vec![Nest { id: 0, members: vec![1, 2], padding: None, header: None }] };
    for direction in [Direction::TopBottom, Direction::LeftRight] {
        let (_, r) = run(&g, &p, &Options { direction, ..Options::default() }, &[(0, 1), (0, 2)]);
        let (before, after) = if direction.is_horizontal() {
            (r[3].right() <= r[0].x, r[0].right() <= r[4].x)
        } else {
            (r[3].bottom() <= r[0].y, r[0].bottom() <= r[4].y)
        };
        assert!(before && after, "{direction:?}: {r:?}");
    }
}
