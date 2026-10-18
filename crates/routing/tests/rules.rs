//! One test per numbered rule of "Connectors cross shapes" in docs/cascade-plan.md, at the level
//! the router owns: the crossing oracle, side choice inside the search, obstacles a request may
//! pass through, straight and rounded routes that stay clear, reported failure, routes that do
//! not depend on the other requests (what incremental rerouting relies on), and the drag budget
//! with fan-out of connectors that share a side.

use vizcraft_routing::{End, Hit, Obstacle, Options, Point, Request, Route, Side, crossings, nudge_routes, round_bends, route_all, route_orthogonal};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// The four side midpoints of a box, as dynamic glue offers them.
fn sides(o: &Obstacle) -> Vec<(Point, Side)> {
    let r = o.rect;
    let (cx, cy) = ((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0);
    vec![(p(cx, r.min.y), Side::Top), (p(r.max.x, cy), Side::Right), (p(cx, r.max.y), Side::Bottom), (p(r.min.x, cy), Side::Left)]
}

fn dynamic(obstacles: &[Obstacle], k: usize) -> End {
    let c = sides(&obstacles[k]);
    End::at(c[1].0).with_side(c[1].1).glued_to(k).with_candidates(c)
}

/// The oracle used for every check below: the route enters nothing but its own stubs.
fn clear(r: &Route, obstacles: &[Obstacle], req: &Request) -> bool {
    crossings(&r.points, obstacles, &req.ignore, req.start.obstacle, req.end.obstacle, -0.5).is_empty()
}

#[test]
fn rule1_the_oracle_reports_segments_that_enter_shapes_but_not_stubs() {
    let obstacles = [Obstacle::new(0.0, 0.0, 100.0, 50.0), Obstacle::new(200.0, 0.0, 300.0, 50.0), Obstacle::new(130.0, -100.0, 170.0, 200.0)];
    // A straight line across the wall: segment 0 enters obstacle 2.
    let line = [p(100.0, 25.0), p(200.0, 25.0)];
    assert_eq!(crossings(&line, &obstacles, &[], Some(0), Some(1), 0.0), vec![Hit { obstacle: 2, segment: 0 }]);
    // Starting on an own edge and leaving is fine; turning back through the own shape is not.
    let back = [p(100.0, 25.0), p(110.0, 25.0), p(110.0, 60.0), p(50.0, 60.0), p(50.0, 25.0)];
    assert_eq!(crossings(&back, &obstacles, &[], Some(0), None, 0.0), vec![Hit { obstacle: 0, segment: 3 }]);
    // A stub from a point inside the own box (a rotated shape's bounding box) is allowed.
    let stub = [p(90.0, 25.0), p(90.0, -20.0), p(120.0, -20.0)];
    assert!(crossings(&stub, &obstacles, &[], Some(0), None, 0.0).is_empty());
    // Ignored obstacles never count; clearance grows the rest.
    assert!(crossings(&line, &obstacles, &[2], Some(0), Some(1), 0.0).is_empty());
    let near = [p(110.0, 210.0), p(190.0, 210.0)];
    assert!(crossings(&near, &obstacles, &[], None, None, 0.0).is_empty());
    assert_eq!(crossings(&near, &obstacles, &[], None, None, 15.0), vec![Hit { obstacle: 2, segment: 0 }]);
    // Junk is safe.
    assert!(crossings(&[p(f64::NAN, 0.0), p(1.0, 1.0)], &obstacles, &[], None, None, f64::NAN).is_empty());
    assert!(crossings(&[], &obstacles, &[], None, None, 0.0).is_empty());
}

#[test]
fn rule2_side_choice_happens_inside_the_search() {
    // B sits right of A, but a wall hugs A's right side: the cheapest clear route leaves A from
    // another side instead of leaving into the wall and wrapping round.
    let obstacles = [Obstacle::new(0.0, 100.0, 100.0, 160.0), Obstacle::new(115.0, 60.0, 135.0, 200.0), Obstacle::new(400.0, 100.0, 500.0, 160.0)];
    let req = Request::new(dynamic(&obstacles, 0), dynamic(&obstacles, 2));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(!r.fallback && clear(&r, &obstacles, &req), "{r:?}");
    let (s, e) = (r.start_candidate.unwrap(), r.end_candidate.unwrap());
    assert_ne!(s, 1, "the start does not leave into the wall");
    assert_eq!(e, 3, "the end enters B from its left, facing the route");
    assert_eq!(r.points.first(), Some(&sides(&obstacles[0])[s].0));
    assert_eq!(r.points.last(), Some(&sides(&obstacles[2])[e].0));
    // The shapes alone decide: the same request gives the same route.
    assert_eq!(route_orthogonal(&obstacles, &req, &Options::default()), r);
    // Without candidates the fixed side is honoured and the route has to wrap round the wall.
    let fixed = Request::new(End::at(p(100.0, 130.0)).with_side(Side::Right).glued_to(0), End::at(p(400.0, 130.0)).with_side(Side::Left).glued_to(2));
    let w = route_orthogonal(&obstacles, &fixed, &Options::default());
    assert!(w.start_candidate.is_none() && w.points.len() > r.points.len());
}

#[test]
fn rule3_requests_pass_through_the_obstacles_they_ignore_but_not_bands() {
    // A container (0) holds both ends; its heading band (1) runs across the top.
    let obstacles = [
        Obstacle::new(0.0, 0.0, 600.0, 400.0),
        Obstacle::new(0.0, 0.0, 600.0, 30.0),
        Obstacle::new(50.0, 100.0, 150.0, 160.0),
        Obstacle::new(450.0, 100.0, 550.0, 160.0),
        Obstacle::new(250.0, 40.0, 350.0, 300.0),
    ];
    let req = Request::new(dynamic(&obstacles, 2), dynamic(&obstacles, 3));
    let blocked = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(blocked.fallback, "inside an obstacle nothing routes");
    let req = req.ignoring(vec![0]);
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(!r.fallback && clear(&r, &obstacles, &req), "{r:?}");
    // The wall reaches up to just under the band, so the route goes below it, not through the band.
    assert!(r.points.iter().all(|q| q.y > 30.0), "{r:?}");
}

#[test]
fn rule4_straight_checks_and_rounded_bends_stay_clear() {
    let obstacles = [Obstacle::new(0.0, 0.0, 60.0, 40.0), Obstacle::new(300.0, 200.0, 360.0, 240.0), Obstacle::new(110.0, 20.0, 200.0, 70.0)];
    // The straight line between the facing points crosses the middle box; another pair is clear.
    assert!(!crossings(&[p(60.0, 20.0), p(300.0, 220.0)], &obstacles, &[], Some(0), Some(1), 4.0).is_empty());
    assert!(crossings(&[p(30.0, 40.0), p(300.0, 220.0)], &obstacles, &[], Some(0), Some(1), 4.0).is_empty());
    // An L bend next to a box: the arc shrinks rather than cut the corner into the box.
    let path = [p(0.0, 200.0), p(200.0, 200.0), p(200.0, 0.0)];
    let wall = [Obstacle::new(150.0, 150.0, 195.0, 195.0)];
    let free = round_bends(&path, &[], &[], 0.5);
    let tight = round_bends(&path, &wall, &[], 0.5);
    let (Some(a), Some(b)) = (free[0], tight[0]) else { panic!("both bends round: {free:?} {tight:?}") };
    assert!(b.to.y > a.to.y, "the arc near the box is smaller: {a:?} {b:?}");
    assert!(round_bends(&path, &[Obstacle::new(150.0, 150.0, 199.6, 199.6)], &[], 0.5)[0].is_none(), "no room at all");
    // Straight runs and ends have nothing to round.
    assert_eq!(round_bends(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)], &[], &[], 0.5), vec![None]);
    assert!(round_bends(&[p(0.0, 0.0)], &[], &[], 0.5).is_empty());
}

#[test]
fn rule5_failure_is_reported_not_hidden() {
    // The end is boxed in by a ring of shapes.
    let mut obstacles = vec![Obstacle::new(200.0, 200.0, 240.0, 240.0)];
    for (x0, y0, x1, y1) in [(150.0, 150.0, 290.0, 170.0), (150.0, 270.0, 290.0, 290.0), (150.0, 150.0, 170.0, 290.0), (270.0, 150.0, 290.0, 290.0)] {
        obstacles.push(Obstacle::new(x0, y0, x1, y1));
    }
    obstacles.push(Obstacle::new(0.0, 0.0, 40.0, 40.0));
    let req = Request::new(dynamic(&obstacles, 5), dynamic(&obstacles, 0));
    let r = route_orthogonal(&obstacles, &req, &Options::default());
    assert!(r.fallback, "{r:?}");
    assert!(r.start_candidate.is_none() && r.end_candidate.is_none());
    assert!(!crossings(&r.points, &obstacles, &req.ignore, Some(5), Some(0), -0.5).is_empty(), "the fallback is caught by the oracle");
}

#[test]
fn rule6_a_route_does_not_depend_on_the_other_requests() {
    // What lets the diagram reroute only the connectors a change touches.
    let obstacles: Vec<Obstacle> = (0..12)
        .map(|i| Obstacle::new(f64::from(i % 4) * 200.0, f64::from(i / 4) * 150.0, f64::from(i % 4) * 200.0 + 80.0, f64::from(i / 4) * 150.0 + 50.0))
        .collect();
    let reqs: Vec<Request> = (0..12).map(|i| Request::new(dynamic(&obstacles, i), dynamic(&obstacles, (i * 5 + 3) % 12))).collect();
    let opts = Options { segment_spacing: 0.0, ..Options::default() };
    let together = route_all(&obstacles, &reqs, &opts);
    for (i, req) in reqs.iter().enumerate() {
        assert_eq!(route_orthogonal(&obstacles, req, &opts), together[i], "request {i}");
        assert!(clear(&together[i], &obstacles, req));
    }
}

#[test]
fn rule7_small_budgets_route_locally_and_shared_sides_fan_out() {
    // A drag budget still routes a local detour.
    let obstacles = [Obstacle::new(0.0, 0.0, 60.0, 40.0), Obstacle::new(100.0, -60.0, 120.0, 100.0), Obstacle::new(200.0, 0.0, 260.0, 40.0)];
    let req = Request::new(dynamic(&obstacles, 0), dynamic(&obstacles, 2));
    let small = Options { max_grid: 2_000, ..Options::default() };
    let r = route_orthogonal(&obstacles, &req, &small);
    assert!(!r.fallback && clear(&r, &obstacles, &req), "{r:?}");

    // Two connectors leave A's right side to B and C on the right: their first segments run on
    // top of each other until nudging slides the ends apart along the side.
    let obstacles = [Obstacle::new(0.0, 0.0, 100.0, 100.0), Obstacle::new(400.0, -200.0, 500.0, -100.0), Obstacle::new(400.0, 200.0, 500.0, 300.0)];
    let start = |spread: f64| End::at(p(100.0, 50.0)).with_side(Side::Right).glued_to(0).with_spread(spread);
    let to = |k: usize, y: f64| End::at(p(400.0, y)).with_side(Side::Left).glued_to(k);
    let fixed = [Request::new(start(0.0), to(1, -150.0)), Request::new(start(0.0), to(2, 250.0))];
    let fanned = [Request::new(start(12.0), to(1, -150.0)), Request::new(start(12.0), to(2, 250.0))];
    let mut a = route_all(&obstacles, &fixed, &Options::default());
    let b = route_all(&obstacles, &fanned, &Options::default());
    assert_eq!(a[0].points[0], a[1].points[0], "fixed ends stay on the connection point");
    assert_ne!(b[0].points[0], b[1].points[0], "spread ends fan out: {b:?}");
    for r in &b {
        let s = r.points[0];
        assert!(s.x == 100.0 && (s.y - 50.0).abs() <= 12.0, "the end slides along A's right side only: {s:?}");
        assert!(clear(r, &obstacles, &fanned[0]));
    }
    // The upper target gets the upper slot, so the fan does not cross itself.
    assert!(b[0].points[0].y < b[1].points[0].y);
    // nudge_routes does the same to routes made elsewhere.
    nudge_routes(&obstacles, &fanned, &mut a, &Options::default());
    assert_eq!(a, b);
}
