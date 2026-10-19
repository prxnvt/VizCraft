//! Behaviour tests for the public routing API.

use std::time::{Duration, Instant};

use vizcraft_routing::{
    End, JumpOrder, Obstacle, Options, Point, Rect, Request, Route, Side, clear_of_obstacles, jumps, nudge_routes, route_all, route_all_around,
    route_orthogonal,
};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn bends(r: &Route) -> usize {
    r.points.len().saturating_sub(2)
}

/// Axis-aligned, no zero-length segments, no collinear interior points.
fn assert_well_formed(r: &Route) {
    for w in r.points.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(a.x == b.x || a.y == b.y, "segment not axis-aligned: {a:?} {b:?}");
        assert!(a != b, "zero-length segment at {a:?}");
    }
    for w in r.points.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let collinear = (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y);
        assert!(!collinear, "collinear interior point {b:?}");
    }
}

/// Every segment except the stubs at glued ends avoids every expanded obstacle, and the stubs
/// avoid every obstacle other than their own.
fn assert_clear(r: &Route, obstacles: &[Obstacle], req: &Request, margin: f64) {
    let n = r.points.len();
    let interior = Route { points: r.points.get(1..n.saturating_sub(1)).unwrap_or_default().to_vec(), fallback: false, ..Route::default() };
    assert!(clear_of_obstacles(&interior, obstacles, &[], margin), "interior crosses an obstacle: {:?}", r.points);
    let ignore: Vec<usize> = [req.start.obstacle, req.end.obstacle].into_iter().flatten().collect();
    assert!(clear_of_obstacles(r, obstacles, &ignore, margin), "route crosses an obstacle: {:?}", r.points);
}

/// Pairs of segments from different routes that run along the same line and overlap, where at
/// least one of them is an interior segment (only those can be nudged).
fn shared_runs(routes: &[Route]) -> usize {
    let segs: Vec<(usize, bool, Point, Point)> = routes
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| {
            let n = r.points.len().saturating_sub(1);
            r.points.windows(2).enumerate().map(move |(si, w)| (ri, si > 0 && si + 1 < n, w[0], w[1]))
        })
        .collect();
    let mut count = 0;
    for (i, a) in segs.iter().enumerate() {
        for b in &segs[i + 1..] {
            if a.0 == b.0 || !(a.1 || b.1) {
                continue;
            }
            let horizontal = a.2.y == a.3.y && b.2.y == b.3.y && a.2.y == b.2.y;
            let vertical = a.2.x == a.3.x && b.2.x == b.3.x && a.2.x == b.2.x;
            let overlap = |a0: f64, a1: f64, b0: f64, b1: f64| a0.min(a1).max(b0.min(b1)) < a0.max(a1).min(b0.max(b1));
            if (horizontal && overlap(a.2.x, a.3.x, b.2.x, b.3.x)) || (vertical && overlap(a.2.y, a.3.y, b.2.y, b.3.y)) {
                count += 1;
            }
        }
    }
    count
}

/// Deterministic xorshift generator so the stress tests are reproducible without dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

#[test]
fn straight_line_when_aligned_and_clear() {
    let req = Request::new(End::at(p(0.0, 0.0)), End::at(p(100.0, 0.0)));
    let r = route_orthogonal(&[], &req, &Options::default());
    assert!(!r.fallback);
    assert_eq!(r.points, vec![p(0.0, 0.0), p(100.0, 0.0)]);
}

#[test]
fn one_bend_without_obstacles() {
    let req = Request::new(End::at(p(0.0, 0.0)), End::at(p(100.0, 80.0)));
    let r = route_orthogonal(&[], &req, &Options::default());
    assert!(!r.fallback);
    assert_well_formed(&r);
    assert_eq!(bends(&r), 1);
    assert_eq!(r.points.first(), Some(&p(0.0, 0.0)));
    assert_eq!(r.points.last(), Some(&p(100.0, 80.0)));
}

#[test]
fn detour_around_a_blocking_box() {
    let obstacles = [Obstacle::new(80.0, 0.0, 120.0, 100.0)];
    let opts = Options::default();
    // Free ends: up, across and down is cheapest.
    let free = Request::new(End::at(p(0.0, 50.0)), End::at(p(200.0, 50.0)));
    let r = route_orthogonal(&obstacles, &free, &opts);
    assert!(!r.fallback);
    assert_well_formed(&r);
    assert_clear(&r, &obstacles, &free, opts.margin);
    assert_eq!(bends(&r), 2, "{:?}", r.points);
    // Leaving right and entering from the left forces four bends.
    let sided = Request::new(End::at(p(0.0, 50.0)).with_side(Side::Right), End::at(p(200.0, 50.0)).with_side(Side::Left));
    let r = route_orthogonal(&obstacles, &sided, &opts);
    assert!(!r.fallback);
    assert_well_formed(&r);
    assert_clear(&r, &obstacles, &sided, opts.margin);
    assert_eq!(bends(&r), 4, "{:?}", r.points);
}

#[test]
fn side_directions_are_respected() {
    let opts = Options::default();
    let req = Request::new(End::at(p(0.0, 0.0)).with_side(Side::Top), End::at(p(100.0, 100.0)).with_side(Side::Left));
    let r = route_orthogonal(&[], &req, &opts);
    assert!(!r.fallback);
    assert_well_formed(&r);
    let pts = &r.points;
    assert_eq!(pts[0], p(0.0, 0.0));
    assert_eq!(pts[1].x, 0.0);
    assert!(pts[1].y <= -opts.margin, "first segment must go up at least one margin: {pts:?}");
    let (last, before) = (pts[pts.len() - 1], pts[pts.len() - 2]);
    assert_eq!(last, p(100.0, 100.0));
    assert_eq!(before.y, 100.0);
    assert!(before.x <= 100.0 - opts.margin, "last segment must arrive moving right: {pts:?}");
}

#[test]
fn glued_ends_leave_their_shapes_through_the_side() {
    let obstacles = [Obstacle::new(0.0, 0.0, 60.0, 40.0), Obstacle::new(200.0, 100.0, 260.0, 140.0), Obstacle::new(100.0, -50.0, 140.0, 200.0)];
    let opts = Options::default();
    let req = Request::new(End::at(p(60.0, 20.0)).with_side(Side::Right).glued_to(0), End::at(p(200.0, 120.0)).with_side(Side::Left).glued_to(1));
    let r = route_orthogonal(&obstacles, &req, &opts);
    assert!(!r.fallback);
    assert_well_formed(&r);
    assert_clear(&r, &obstacles, &req, opts.margin);
    assert_eq!(r.points.first(), Some(&p(60.0, 20.0)));
    assert_eq!(r.points.last(), Some(&p(200.0, 120.0)));
    assert!(r.points[1].x > 60.0 && r.points[1].y == 20.0);
    let n = r.points.len();
    assert!(r.points[n - 2].x < 200.0 && r.points[n - 2].y == 120.0);
}

#[test]
fn glued_end_without_side_uses_the_nearest_side() {
    let obstacles = [Obstacle::new(0.0, 0.0, 60.0, 40.0)];
    let req = Request::new(End::at(p(30.0, 40.0)).glued_to(0), End::at(p(30.0, 200.0)));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(!r.fallback);
    assert_eq!(r.points, vec![p(30.0, 40.0), p(30.0, 200.0)]);
}

#[test]
fn self_loop_on_one_shape_goes_around_it() {
    let obstacles = [Obstacle::new(0.0, 0.0, 60.0, 40.0)];
    let opts = Options::default();
    let req = Request::new(End::at(p(60.0, 20.0)).with_side(Side::Right).glued_to(0), End::at(p(30.0, 0.0)).with_side(Side::Top).glued_to(0));
    let r = route_orthogonal(&obstacles, &req, &opts);
    assert!(!r.fallback);
    assert_well_formed(&r);
    assert_clear(&r, &obstacles, &req, opts.margin);
}

#[test]
fn many_obstacles_routes_stay_clear() {
    // A regular field of boxes with routes threading through it.
    let mut obstacles = Vec::new();
    for i in 0..6 {
        for j in 0..6 {
            let (x, y) = (f64::from(i) * 100.0, f64::from(j) * 100.0);
            obstacles.push(Obstacle::new(x, y, x + 50.0, y + 50.0));
        }
    }
    let opts = Options::default();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..40 {
        let a = rng.next() as usize % obstacles.len();
        let b = rng.next() as usize % obstacles.len();
        if a == b {
            continue;
        }
        let (ra, rb) = (obstacles[a].rect, obstacles[b].rect);
        let req = Request::new(
            End::at(p(ra.max.x, (ra.min.y + ra.max.y) / 2.0)).with_side(Side::Right).glued_to(a),
            End::at(p((rb.min.x + rb.max.x) / 2.0, rb.min.y)).with_side(Side::Top).glued_to(b),
        );
        let r = route_orthogonal(&obstacles, &req, &opts);
        assert!(!r.fallback, "{a} -> {b}");
        assert_well_formed(&r);
        assert_clear(&r, &obstacles, &req, opts.margin);
    }
}

#[test]
fn route_all_nudges_shared_channel_apart() {
    let opts = Options::default();
    let reqs = [
        Request::new(End::at(p(0.0, 0.0)).with_side(Side::Right), End::at(p(200.0, 100.0)).with_side(Side::Left)),
        Request::new(End::at(p(0.0, 20.0)).with_side(Side::Right), End::at(p(200.0, 120.0)).with_side(Side::Left)),
    ];
    let single: Vec<Route> = reqs.iter().map(|r| route_orthogonal(&[], r, &opts)).collect();
    // Unnudged, both connectors drop down the same vertical line.
    let vertical_x = |r: &Route| r.points.windows(2).find(|w| w[0].x == w[1].x).map(|w| w[0].x);
    assert_eq!(vertical_x(&single[0]), vertical_x(&single[1]));
    let routes = route_all(&[], &reqs, &opts);
    let (x0, x1) = (vertical_x(&routes[0]).unwrap(), vertical_x(&routes[1]).unwrap());
    assert!(((x0 - x1).abs() - opts.segment_spacing).abs() < 1e-9, "{x0} {x1}");
    for (r, req) in routes.iter().zip(&reqs) {
        assert!(!r.fallback);
        assert_well_formed(r);
        assert_eq!(r.points.first(), Some(&req.start.point));
        assert_eq!(r.points.last(), Some(&req.end.point));
    }
}

#[test]
fn nudging_keeps_routes_clear_of_obstacles() {
    let obstacles = [Obstacle::new(0.0, 0.0, 40.0, 100.0), Obstacle::new(200.0, 0.0, 240.0, 100.0), Obstacle::new(100.0, 40.0, 130.0, 60.0)];
    let opts = Options::default();
    let reqs: Vec<Request> = (0..5)
        .map(|k| {
            let y = 10.0 + f64::from(k) * 20.0;
            Request::new(End::at(p(40.0, y)).with_side(Side::Right).glued_to(0), End::at(p(200.0, 100.0 - y)).with_side(Side::Left).glued_to(1))
        })
        .collect();
    let routes = route_all(&obstacles, &reqs, &opts);
    assert_eq!(shared_runs(&routes), 0, "{:?}", routes.iter().map(|r| &r.points).collect::<Vec<_>>());
    for (r, req) in routes.iter().zip(&reqs) {
        assert!(!r.fallback);
        assert_well_formed(r);
        assert_clear(r, &obstacles, req, opts.margin);
    }
}

#[test]
fn crossing_penalty_is_accepted() {
    let opts = Options { crossing_penalty: Some(500.0), ..Options::default() };
    let reqs = [Request::new(End::at(p(0.0, 50.0)), End::at(p(200.0, 50.0))), Request::new(End::at(p(100.0, 0.0)), End::at(p(150.0, 100.0)))];
    let routes = route_all(&[], &reqs, &opts);
    let polys: Vec<Vec<Point>> = routes.iter().map(|r| r.points.clone()).collect();
    // The second connector can bend so it never crosses the first.
    assert!(jumps(&polys, JumpOrder::LaterOverEarlier).iter().all(Vec::is_empty), "{polys:?}");
}

#[test]
fn an_overlap_penalty_keeps_a_route_off_an_earlier_run() {
    // Two connectors from just above each other on a box's right side to one point on another
    // box's left side: the second may join the first's line early or late at the same cost, and
    // the penalty makes it late (only the last stub is shared).
    let obstacles = [Obstacle::new(0.0, 0.0, 100.0, 100.0), Obstacle::new(300.0, 0.0, 400.0, 100.0)];
    let end = |x: f64, y: f64, side: Side, k: usize| End::at(p(x, y)).with_side(side).glued_to(k);
    let reqs = [
        Request::new(end(100.0, 50.0, Side::Right, 0), end(300.0, 50.0, Side::Left, 1)),
        Request::new(end(100.0, 40.0, Side::Right, 0), end(300.0, 50.0, Side::Left, 1)),
    ];
    let shares = |r: &Route| r.points.windows(2).any(|w| w[0].y == 50.0 && w[1].y == 50.0 && w[0].x.min(w[1].x) < 280.0);
    let opts = Options { segment_spacing: 0.0, overlap_penalty: Some(2.0), ..Options::default() };
    let apart = route_all(&obstacles, &reqs, &opts);
    assert!(!shares(&apart[1]), "{:?}", apart[1].points);
    // A line drawn elsewhere counts as an earlier run too.
    let one = route_all_around(&obstacles, &reqs[1..], &opts, &[apart[0].points.clone()]);
    assert!(!shares(&one[0]), "{:?}", one[0].points);
}

#[test]
fn labels_spread_parallel_runs_further_apart() {
    // Two parallel runs on one line, each with a label on it: nudging parts them by more than the
    // label's height, not just the run spacing.
    let reqs = [
        Request::new(End::at(p(0.0, 0.0)), End::at(p(300.0, 200.0))).labelled(60.0, 18.0),
        Request::new(End::at(p(0.0, 10.0)), End::at(p(300.0, 210.0))).labelled(60.0, 18.0),
    ];
    let mut routes = vec![
        Route { points: vec![p(0.0, 0.0), p(150.0, 0.0), p(150.0, 200.0), p(300.0, 200.0)], ..Route::default() },
        Route { points: vec![p(0.0, 10.0), p(150.0, 10.0), p(150.0, 210.0), p(300.0, 210.0)], ..Route::default() },
    ];
    nudge_routes(&[], &reqs, &mut routes, &Options::default());
    let xs: Vec<f64> = routes.iter().map(|r| r.points[1].x).collect();
    assert!((xs[0] - xs[1]).abs() >= 60.0, "{xs:?}");
}

#[test]
fn jumps_for_crossing_l_shapes() {
    let polys = vec![vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], vec![p(50.0, -50.0), p(50.0, 50.0), p(150.0, 50.0)]];
    let later = jumps(&polys, JumpOrder::LaterOverEarlier);
    assert!(later[0].is_empty());
    assert_eq!(later[1].len(), 2);
    assert_eq!((later[1][0].segment, later[1][0].distance, later[1][0].point, later[1][0].over), (0, 50.0, p(50.0, 0.0), 0));
    assert_eq!((later[1][1].segment, later[1][1].distance, later[1][1].point), (1, 50.0, p(100.0, 50.0)));
    let earlier = jumps(&polys, JumpOrder::EarlierOverLater);
    assert!(earlier[1].is_empty());
    assert_eq!(earlier[0].len(), 2);
    assert_eq!((earlier[0][0].segment, earlier[0][0].distance), (0, 50.0));
    assert_eq!((earlier[0][1].segment, earlier[0][1].distance), (1, 50.0));
}

#[test]
fn jumps_ignore_shared_endpoints_touches_and_overlaps() {
    let polys = vec![
        vec![p(0.0, 0.0), p(100.0, 0.0)],
        vec![p(0.0, 0.0), p(0.0, 100.0)],                    // shares the start point
        vec![p(50.0, 0.0), p(50.0, 80.0)],                   // T-junction on the first
        vec![p(20.0, 0.0), p(80.0, 0.0)],                    // collinear overlap
        vec![p(100.0, 0.0), p(100.0, 50.0), p(-10.0, 50.0)], // starts at the first one's end
    ];
    let out = jumps(&polys, JumpOrder::LaterOverEarlier);
    assert!(out[0].is_empty() && out[1].is_empty() && out[2].is_empty() && out[3].is_empty());
    // The last one crosses polyline 1 and polyline 2 properly, but nothing else.
    let overs: Vec<usize> = out[4].iter().map(|j| j.over).collect();
    assert_eq!(overs, vec![2, 1]);
    assert!(out[4].iter().all(|j| j.segment == 1));
}

#[test]
fn random_scattered_obstacles_route_clear() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let obstacles: Vec<Obstacle> = (0..150)
        .map(|_| {
            let (x, y) = (rng.range(0.0, 3000.0), rng.range(0.0, 3000.0));
            Obstacle::new(x, y, x + rng.range(20.0, 90.0), y + rng.range(20.0, 60.0))
        })
        .collect();
    let reqs: Vec<Request> = (0..30)
        .map(|_| {
            let a = rng.next() as usize % obstacles.len();
            let b = rng.next() as usize % obstacles.len();
            let (ra, rb) = (obstacles[a].rect, obstacles[b].rect);
            Request::new(
                End::at(p((ra.min.x + ra.max.x) / 2.0, ra.max.y)).with_side(Side::Bottom).glued_to(a),
                End::at(p(rb.min.x, (rb.min.y + rb.max.y) / 2.0)).with_side(Side::Left).glued_to(b),
            )
        })
        .collect();
    let opts = Options::default();
    let routes = route_all(&obstacles, &reqs, &opts);
    let real = routes.iter().filter(|r| !r.fallback).count();
    assert!(real >= 20, "only {real} of {} connectors found a route", routes.len());
    for (r, req) in routes.iter().zip(&reqs) {
        assert_eq!(r.points.first(), Some(&req.start.point));
        assert_eq!(r.points.last(), Some(&req.end.point));
        if !r.fallback {
            assert_well_formed(r);
            assert_clear(r, &obstacles, req, 0.0);
        }
    }
}

#[test]
fn routing_is_deterministic() {
    let obstacles: Vec<Obstacle> = {
        let mut rng = Rng(42);
        (0..60)
            .map(|_| {
                let (x, y) = (rng.range(0.0, 1000.0), rng.range(0.0, 1000.0));
                Obstacle::new(x, y, x + rng.range(10.0, 60.0), y + rng.range(10.0, 60.0))
            })
            .collect()
    };
    let reqs: Vec<Request> = {
        let mut rng = Rng(7);
        (0..20).map(|_| Request::new(End::at(p(rng.range(-50.0, 1050.0), -40.0)), End::at(p(rng.range(-50.0, 1050.0), 1100.0)))).collect()
    };
    let opts = Options::default();
    let first = route_all(&obstacles, &reqs, &opts);
    for _ in 0..3 {
        assert_eq!(route_all(&obstacles, &reqs, &opts), first);
    }
    assert!(first.iter().filter(|r| !r.fallback).count() >= 15, "most connectors should find a real route");
    for (r, req) in first.iter().zip(&reqs) {
        if !r.fallback {
            assert_well_formed(r);
            assert_clear(r, &obstacles, req, 0.0);
        }
    }
}

#[test]
fn end_inside_an_obstacle_falls_back() {
    let obstacles = [Obstacle::new(100.0, 100.0, 200.0, 200.0)];
    let req = Request::new(End::at(p(0.0, 0.0)), End::at(p(150.0, 150.0)));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(r.fallback);
    assert_eq!(r.points.first(), Some(&p(0.0, 0.0)));
    assert_eq!(r.points.last(), Some(&p(150.0, 150.0)));
    assert_well_formed(&r);
}

#[test]
fn enclosed_end_falls_back() {
    // Four walls around the end with no gap.
    let obstacles = [
        Obstacle::new(-100.0, -100.0, 100.0, -80.0),
        Obstacle::new(-100.0, 80.0, 100.0, 100.0),
        Obstacle::new(-100.0, -100.0, -80.0, 100.0),
        Obstacle::new(80.0, -100.0, 100.0, 100.0),
    ];
    let req = Request::new(End::at(p(0.0, 0.0)), End::at(p(500.0, 0.0)));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(r.fallback);
}

#[test]
fn tight_gap_retries_with_smaller_margin() {
    // An enclosure whose only opening is narrower than two margins but still passable.
    let obstacles = [
        Obstacle::new(0.0, -100.0, 100.0, -80.0),
        Obstacle::new(0.0, 80.0, 100.0, 100.0),
        Obstacle::new(80.0, -100.0, 100.0, 100.0),
        Obstacle::new(0.0, -100.0, 20.0, -6.0),
        Obstacle::new(0.0, 6.0, 20.0, 100.0),
    ];
    let req = Request::new(End::at(p(-50.0, 0.0)).with_side(Side::Right), End::at(p(50.0, 0.0)).with_side(Side::Left));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(!r.fallback);
    assert_eq!(r.points, vec![p(-50.0, 0.0), p(50.0, 0.0)]);
    assert!(clear_of_obstacles(&r, &obstacles, &[], 5.0));
}

#[test]
fn hostile_inputs_never_panic() {
    let opts = Options::default();
    let bad = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300, f64::MAX, f64::MIN_POSITIVE];
    for &v in &bad {
        let obstacles = [Obstacle::new(v, 0.0, 10.0, v), Obstacle::new(0.0, 0.0, 10.0, 10.0)];
        for req in [
            Request::new(End::at(p(v, 0.0)), End::at(p(100.0, 100.0))),
            Request::new(End::at(p(0.0, 0.0)), End::at(p(100.0, v))),
            Request::new(End::at(p(-20.0, 5.0)).glued_to(usize::MAX), End::at(p(50.0, 5.0)).glued_to(0)),
        ] {
            let r = route_orthogonal(&obstacles, &req, &opts);
            assert!(r.points.iter().all(Point::is_finite));
            let _ = route_all(&obstacles, &[req.clone(), req.clone()], &opts);
        }
        let weird =
            Options { margin: v, bend_penalty: v, crossing_penalty: Some(v), overlap_penalty: Some(v), segment_spacing: v, max_grid: usize::MAX };
        let req = Request::new(End::at(p(-20.0, 5.0)), End::at(p(50.0, 5.0)));
        let _ = route_all(&obstacles, &[req.clone(), req.clone()], &weird);
        let _ = jumps(&[vec![p(v, 0.0), p(0.0, v)], vec![p(0.0, 0.0), p(v, v)]], JumpOrder::LaterOverEarlier);
        let _ = clear_of_obstacles(&Route { points: vec![p(v, v)], fallback: false, ..Route::default() }, &obstacles, &[], v);
    }
    // Inverted rectangles behave like normal ones.
    let inverted = [Obstacle { rect: Rect { min: p(120.0, 100.0), max: p(80.0, 0.0) } }];
    let req = Request::new(End::at(p(0.0, 50.0)), End::at(p(200.0, 50.0)));
    let r = route_orthogonal(&inverted, &req, &opts);
    assert!(!r.fallback);
    assert!(clear_of_obstacles(&r, &inverted, &[], opts.margin));
    // Start equal to end.
    let same = route_orthogonal(&[], &Request::new(End::at(p(5.0, 5.0)), End::at(p(5.0, 5.0))), &opts);
    assert_eq!(same.points, vec![p(5.0, 5.0)]);
    // Empty inputs.
    assert!(route_all(&[], &[], &opts).is_empty());
    assert!(jumps(&[], JumpOrder::LaterOverEarlier).is_empty());
    assert_eq!(jumps(&[vec![], vec![p(1.0, 1.0)]], JumpOrder::EarlierOverLater).len(), 2);
}

#[test]
fn huge_coordinates_fall_back_with_finite_points() {
    let req = Request::new(End::at(p(-1e300, 0.0)), End::at(p(f64::MAX, 1e300)));
    let r = route_orthogonal(&[], &req, &Options::default());
    assert!(r.fallback);
    assert!(r.points.iter().all(Point::is_finite));
}

#[test]
fn thousands_of_random_obstacles_finish_quickly() {
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let obstacles: Vec<Obstacle> = (0..4000)
        .map(|_| {
            let (x, y) = (rng.range(0.0, 5000.0), rng.range(0.0, 5000.0));
            Obstacle::new(x, y, x + rng.range(1.0, 80.0), y + rng.range(1.0, 80.0))
        })
        .collect();
    let reqs: Vec<Request> = (0..20)
        .map(|_| Request::new(End::at(p(rng.range(0.0, 5000.0), rng.range(0.0, 5000.0))), End::at(p(rng.range(0.0, 5000.0), rng.range(0.0, 5000.0)))))
        .collect();
    let started = Instant::now();
    let routes = route_all(&obstacles, &reqs, &Options::default());
    assert_eq!(routes.len(), reqs.len());
    for (r, req) in routes.iter().zip(&reqs) {
        assert!(r.points.iter().all(Point::is_finite));
        if !r.fallback {
            assert_well_formed(r);
            assert_clear(r, &obstacles, req, 0.0);
        }
    }
    // Too many obstacles: always a fallback, never a long search.
    let many: Vec<Obstacle> = (0..6000).map(|k| Obstacle::new(f64::from(k), 0.0, f64::from(k) + 0.5, 1.0)).collect();
    let r = route_orthogonal(&many, reqs.first().unwrap(), &Options::default());
    assert!(r.fallback);
    assert!(started.elapsed() < Duration::from_secs(60), "took {:?}", started.elapsed());
}
