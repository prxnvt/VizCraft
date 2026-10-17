//! Model, hit testing, glue and routing tests.

use proptest::prelude::*;

use crate::*;

fn page_with(shapes: Vec<Shape>, connectors: Vec<Connector>) -> Page {
    Page { shapes, connectors, ..Page::default() }
}

#[test]
fn new_document_has_one_blank_page() {
    let d = Document::new();
    assert_eq!(d.pages.len(), 1);
    let p = d.page().unwrap();
    assert_eq!((p.width, p.height), (1100.0, 850.0));
    assert!(p.shapes.is_empty() && p.connectors.is_empty());
}

#[test]
fn next_id_is_unique_and_skips_taken_ids() {
    let mut d = Document::new();
    let a = d.next_id("s");
    let b = d.next_id("c");
    assert_ne!(a, b);
    assert!(a.starts_with('s') && b.starts_with('c'));
    // A file that has ids beyond the counter: the counter skips them.
    let seq = d.id_seq;
    d.page_mut().unwrap().shapes.push(Shape::new(format!("s{}", seq + 1), ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
    let c = d.next_id("s");
    assert_eq!(c, format!("s{}", seq + 2));
    d.id_seq = u64::MAX - 1;
    let _ = d.next_id("s");
    let _ = d.next_id("s");
}

#[test]
fn lookups_and_removal() {
    let mut d = Document::new();
    let s = Shape::new("s1", ShapeKind::Ellipse, 0.0, 0.0, 50.0, 50.0);
    let t = Shape::new("s2", ShapeKind::Rectangle, 200.0, 0.0, 50.0, 50.0);
    let c = Connector::new("c1", End::dynamic("s1"), End::dynamic("s2"));
    *d.page_mut().unwrap() = page_with(vec![s, t], vec![c]);
    assert!(d.shape("s1").is_some() && d.connector("c1").is_some());
    d.shape_mut("s2").unwrap().x = 300.0;
    assert_eq!(d.page().unwrap().connectors_glued_to("s1"), vec!["c1".to_string()]);
    let before = route_polyline(d.page().unwrap(), d.connector("c1").unwrap());
    d.page_mut().unwrap().detach_shape("s1");
    let c = d.connector("c1").unwrap();
    assert!(matches!(c.start, End::Free { .. }));
    assert_eq!(route_polyline(d.page().unwrap(), c).first(), before.first());
    assert!(d.remove_shape("s1").is_some());
    assert!(d.remove_shape("s1").is_none());
    assert!(d.remove_connector("c1").is_some());
}

#[test]
fn rotated_shape_hit_testing() {
    // A long thin bar rotated 90 degrees about its centre (60, 55) becomes vertical.
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 10.0, 50.0, 100.0, 10.0);
    let page = page_with(vec![s.clone()], vec![]);
    assert!(hit::shape_at(&page, Point::new(15.0, 55.0), 0.0).is_some());
    s.angle_deg = 90.0;
    let page = page_with(vec![s], vec![]);
    assert!(hit::shape_at(&page, Point::new(15.0, 55.0), 0.0).is_none(), "the old horizontal extent is empty after rotation");
    assert!(hit::shape_at(&page, Point::new(60.0, 10.0), 0.0).is_some(), "the rotated bar reaches up");
    assert!(hit::shape_at(&page, Point::new(60.0, 100.0), 0.0).is_some(), "and down");
    assert!(hit::shape_at(&page, Point::new(70.0, 10.0), 2.0).is_none());
    assert!(hit::shape_at(&page, Point::new(f64::NAN, 10.0), 2.0).is_none());
}

#[test]
fn hit_respects_the_actual_outline_and_z_order() {
    let diamond = Shape::new("d", ShapeKind::Diamond, 0.0, 0.0, 100.0, 100.0);
    let page = page_with(vec![diamond], vec![]);
    // The frame corner is outside the diamond.
    assert!(hit::shape_at(&page, Point::new(5.0, 5.0), 1.0).is_none());
    assert!(hit::shape_at(&page, Point::new(50.0, 50.0), 0.0).is_some());
    // Within tolerance of an edge from outside.
    assert!(hit::shape_at(&page, Point::new(26.0, 22.0), 3.0).is_some());

    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 50.0, 50.0, 100.0, 100.0);
    let page = page_with(vec![a, b], vec![]);
    assert_eq!(hit::shape_at(&page, Point::new(75.0, 75.0), 0.0).unwrap().id, "b");
    assert_eq!(hit::shape_at(&page, Point::new(25.0, 25.0), 0.0).unwrap().id, "a");
}

#[test]
fn marquee_and_connection_point_hits() {
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 100.0);
    let c = Connector::new("c", End::point("a", 1), End::point("b", 3));
    let page = page_with(vec![a.clone(), b], vec![c]);
    let ids: Vec<_> = hit::shapes_in_rect(&page, Rect::new(-1.0, -1.0, 150.0, 150.0)).iter().map(|s| s.id.clone()).collect();
    assert_eq!(ids, vec!["a"]);
    assert_eq!(hit::connectors_in_rect(&page, Rect::new(-1.0, -1.0, 401.0, 101.0)).len(), 1);
    assert_eq!(hit::connection_point_at(&a, Point::new(98.0, 52.0), 5.0), Some(1));
    assert_eq!(hit::connection_point_at(&a, Point::new(50.0, 50.0), 5.0), None);
    assert_eq!(hit::connector_at(&page, Point::new(200.0, 51.0), 2.0).unwrap().id, "c");
    assert_eq!(hit::hit(&page, Point::new(200.0, 50.0), 2.0), Some(Hit::Connector("c".into())));
    assert_eq!(hit::hit(&page, Point::new(10.0, 10.0), 2.0), Some(Hit::Shape("a".into())));
    assert_eq!(hit::hit(&page, Point::new(200.0, 300.0), 2.0), None);
}

#[test]
fn dynamic_glue_picks_the_nearest_point() {
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 100.0);
    let below = Shape::new("c", ShapeKind::Rectangle, 0.0, 400.0, 100.0, 100.0);
    let page = page_with(vec![a, b, below], vec![]);
    let right = Connector::new("x", End::dynamic("a"), End::dynamic("b"));
    let e = resolve_ends(&page, &right);
    assert_eq!(e.start.pos, Point::new(100.0, 50.0));
    assert_eq!(e.end.pos, Point::new(300.0, 50.0));
    assert_eq!(e.start.point, Some(1));
    let down = Connector::new("y", End::dynamic("a"), End::dynamic("c"));
    let e = resolve_ends(&page, &down);
    assert_eq!(e.start.pos, Point::new(50.0, 100.0));
    assert_eq!(e.end.pos, Point::new(50.0, 400.0));
    let free = Connector::new("z", End::dynamic("a"), End::free(Point::new(-200.0, 40.0)));
    assert_eq!(resolve_ends(&page, &free).start.pos, Point::new(0.0, 50.0));
}

#[test]
fn point_glue_follows_rotation_and_moves() {
    let mut a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 50.0);
    a.angle_deg = 90.0;
    let page = page_with(vec![a], vec![Connector::new("c", End::point("a", 1), End::free(Point::new(500.0, 500.0)))]);
    let c = page.connectors.first().unwrap();
    let e = resolve_ends(&page, c);
    // The right midpoint (100, 25) rotated 90 degrees about (50, 25) lands at (50, 75), facing down.
    assert!(e.start.pos.distance(Point::new(50.0, 75.0)) < 1e-9, "{:?}", e.start.pos);
    let d = e.start.dir.unwrap();
    assert!(d.distance(Point::new(0.0, 1.0)) < 1e-9);

    let mut page = page;
    page.shape_mut("a").unwrap().x += 10.0;
    let moved = resolve_ends(&page, page.connectors.first().unwrap());
    assert!(moved.start.pos.distance(Point::new(60.0, 75.0)) < 1e-9);
}

fn axis_aligned(pts: &[Point]) -> bool {
    pts.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9)
}

#[test]
fn orthogonal_routes_are_axis_aligned() {
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0);
    let b = Shape::new("b", ShapeKind::Diamond, 250.0, 200.0, 80.0, 80.0);
    for (sp, ep) in [(Some(1), Some(0)), (Some(2), Some(3)), (Some(0), Some(2)), (None, None), (Some(3), Some(1))] {
        let mk = |s: &str, p: Option<usize>| End::Glued { shape: s.into(), point: p };
        let c = Connector::new("c", mk("a", sp), mk("b", ep));
        let page = page_with(vec![a.clone(), b.clone()], vec![c.clone()]);
        let pts = route_polyline(&page, &c);
        assert!(pts.len() >= 2);
        assert!(axis_aligned(&pts), "{sp:?} {ep:?}: {pts:?}");
        let e = resolve_ends(&page, &c);
        assert_eq!(pts.first(), Some(&e.start.pos));
        assert_eq!(pts.last(), Some(&e.end.pos));
    }
    // Facing points in a vertical line collapse to one straight segment.
    let top = Shape::new("t", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 50.0);
    let bottom = Shape::new("u", ShapeKind::Rectangle, 0.0, 200.0, 100.0, 50.0);
    let c = Connector::new("c", End::point("t", 2), End::point("u", 0));
    let page = page_with(vec![top, bottom], vec![c.clone()]);
    assert_eq!(route_polyline(&page, &c), vec![Point::new(50.0, 50.0), Point::new(50.0, 200.0)]);
}

#[test]
fn straight_and_curved_routes() {
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 300.0, 300.0, 100.0, 100.0);
    let mut c = Connector::new("c", End::point("a", 1), End::point("b", 0));
    c.route = Route::Straight;
    let page = page_with(vec![a, b], vec![]);
    assert_eq!(route_polyline(&page, &c), vec![Point::new(100.0, 50.0), Point::new(350.0, 300.0)]);
    c.route = Route::Curved;
    let path = route_path(&page, &c);
    assert!(matches!(path.els.get(1), Some(PathEl::CubicTo(..))));
    let poly = route_polyline(&page, &c);
    assert!(poly.len() > 4);
    assert_eq!(poly.first(), Some(&Point::new(100.0, 50.0)));
}

#[test]
fn arrowheads() {
    let tip = Point::new(100.0, 0.0);
    let dir = Point::new(1.0, 0.0);
    let t = arrowhead(Arrow::Triangle, tip, dir, 1.0).unwrap();
    assert!(t.filled);
    assert!(t.line_end.x < tip.x);
    let b = t.path.bounds().unwrap();
    assert!((b.x1 - 100.0).abs() < 1e-9 && b.y0 < 0.0 && b.y1 > 0.0);
    assert!(arrowhead(Arrow::None, tip, dir, 1.0).is_none());
    assert!(arrowhead(Arrow::Triangle, tip, Point::ZERO, 1.0).is_none());
    assert!(!arrowhead(Arrow::Open, tip, dir, 1.0).unwrap().filled);
    for a in [Arrow::Diamond, Arrow::Circle] {
        let h = arrowhead(a, tip, dir, f64::NAN).unwrap();
        assert!(h.path.bounds().unwrap().x1 <= 100.0 + 1e-9);
    }
    // A heavier line gets a bigger head.
    let big = arrowhead(Arrow::Triangle, tip, dir, 6.0).unwrap();
    assert!(big.line_end.x < t.line_end.x);
}

#[test]
fn connector_geometry_shortens_under_heads_and_places_label() {
    let a = Shape::new("a", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    let b = Shape::new("b", ShapeKind::Rectangle, 300.0, 0.0, 100.0, 100.0);
    let c = Connector::new("c", End::point("a", 1), End::point("b", 3));
    let page = page_with(vec![a, b], vec![c.clone()]);
    let g = connector_geometry(&page, &c);
    assert!(g.start_head.is_none());
    let head = g.end_head.unwrap();
    assert!(matches!(g.line.els.last(), Some(PathEl::LineTo(p)) if *p == head.line_end));
    assert!((g.label_pos.x - 200.0).abs() < 1e-9 && (g.label_pos.y - 50.0).abs() < 1e-9);
}

#[test]
fn missing_glue_target_does_not_panic() {
    let c = Connector::new("c", End::dynamic("ghost"), End::point("ghost", 99));
    let page = page_with(vec![], vec![c.clone()]);
    let g = connector_geometry(&page, &c);
    assert_eq!(g.ends.start.pos, Point::ZERO);
    let s = Shape::new("s", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    let page = page_with(vec![s], vec![]);
    let c = Connector::new("c", End::point("s", 99), End::free(Point::new(100.0, 5.0)));
    // A bad point index falls back to dynamic glue.
    assert_eq!(resolve_ends(&page, &c).start.pos, Point::new(10.0, 5.0));
}

#[test]
fn serde_round_trip() {
    let mut d = Document::new();
    d.title = "Round trip".into();
    d.core.author = "Ada".into();
    let mut s = Shape::new("s1", ShapeKind::Cloud, 10.0, 20.0, 120.0, 80.0);
    s.text = "Hello".into();
    s.angle_deg = 30.0;
    s.style.fill = Color { r: 1, g: 2, b: 3, a: 128 };
    let mut c = Connector::new("c2", End::point("s1", 2), End::free(Point::new(400.0, 300.0)));
    c.route = Route::Curved;
    c.start_arrow = Arrow::Diamond;
    c.text = "label".into();
    let c2 = Connector::new("c3", End::dynamic("s1"), End::dynamic("s1"));
    *d.page_mut().unwrap() = page_with(vec![s], vec![c, c2]);
    d.id_seq = 3;
    let json = serde_json::to_string_pretty(&d).unwrap();
    assert!(json.contains("\"angleDeg\"") && json.contains("\"#01020380\"") && json.contains("\"idSeq\""));
    assert!(json.contains("\"shape\": \"s1\"") && json.contains("\"cloud\""));
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
}

#[test]
fn tolerant_load() {
    let d: Document = serde_json::from_str("{}").unwrap();
    assert_eq!(d, Document::new());
    let d: Document = serde_json::from_str(r#"{"pages": []}"#).unwrap();
    assert_eq!(d.pages.len(), 1);
    let junk = r##"{
        "title": 5,
        "core": "nope",
        "idSeq": -4,
        "pages": [7, {"width": null, "height": "tall", "shapes": [
            {"id": "s1", "kind": "starship", "x": "left", "w": 1e300, "h": -5, "angleDeg": 720.5,
             "style": {"fill": "#zzz", "line": "#abc", "lineWidth": 1e9, "bold": "yes"},
             "connectionPoints": [{"x": 2, "y": -1}, "bad"]},
            42
        ], "connectors": [
            {"id": "c1", "start": {"shape": "s1", "point": "two"}, "end": [1, 2], "route": 3, "endArrow": "harpoon"}
        ]}]
    }"##;
    let d: Document = serde_json::from_str(junk).unwrap();
    assert_eq!(d.title, "");
    let p = d.page().unwrap();
    assert_eq!((p.width, p.height), (Page::DEFAULT_WIDTH, Page::DEFAULT_HEIGHT));
    let s = p.shapes.first().unwrap();
    assert_eq!(p.shapes.len(), 1);
    assert_eq!(s.kind, ShapeKind::Rectangle);
    assert_eq!((s.x, s.w, s.h), (0.0, MAX_EXTENT, 0.0));
    assert!((s.angle_deg - 0.5).abs() < 1e-9);
    assert_eq!(s.style.fill, Color::BLACK);
    assert_eq!(s.style.line, Color::rgb(0xaa, 0xbb, 0xcc));
    assert_eq!(s.style.line_width, Style::MAX_LINE_WIDTH);
    assert!(!s.style.bold);
    assert_eq!(s.connection_points, vec![Point::new(1.0, 0.0)]);
    let c = p.connectors.first().unwrap();
    assert_eq!(c.start, End::dynamic("s1"));
    assert_eq!(c.end, End::default());
    assert_eq!((c.route, c.end_arrow), (Route::Orthogonal, Arrow::None));
}

#[test]
fn color_parse() {
    assert_eq!(Color::parse("#fff"), Some(Color::WHITE));
    assert_eq!(Color::parse("336699"), Some(Color::rgb(0x33, 0x66, 0x99)));
    assert_eq!(Color::parse("#33669980").map(|c| c.a), Some(0x80));
    assert_eq!(Color::parse("#é12"), None);
    assert_eq!(Color::parse("#12345"), None);
    assert_eq!(Color::rgb(0x33, 0x66, 0x99).to_hex(), "#336699");
}

fn any_kind() -> impl Strategy<Value = ShapeKind> {
    (0..ShapeKind::ALL.len()).prop_map(|i| ShapeKind::ALL[i])
}

fn any_num() -> impl Strategy<Value = f64> {
    prop_oneof![-2000.0..2000.0f64, Just(f64::NAN), Just(f64::INFINITY), Just(-1e300), Just(0.0)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn random_geometry_never_panics(
        kind in any_kind(), other in any_kind(),
        x in any_num(), y in any_num(), w in any_num(), h in any_num(), a in any_num(),
        px in any_num(), py in any_num(), tol in any_num(),
        sp in proptest::option::of(0usize..8), ep in proptest::option::of(0usize..8), route in 0..3u8,
    ) {
        let mut s = Shape::new("s", kind, x, y, w, h);
        s.angle_deg = a;
        let t = Shape::new("t", other, y, x, h, w);
        let route = [Route::Straight, Route::Orthogonal, Route::Curved][route as usize];
        let mut c = Connector::new("c", End::Glued { shape: "s".into(), point: sp }, End::Glued { shape: "t".into(), point: ep });
        c.route = route;
        c.start_arrow = Arrow::Circle;
        let free = Connector::new("f", End::dynamic("s"), End::free(Point::new(px, py)));
        let mut page = page_with(vec![s.clone(), t], vec![c.clone(), free.clone()]);
        let p = Point::new(px, py);
        let _ = hit::shape_at(&page, p, tol);
        let _ = hit::connector_at(&page, p, tol);
        let _ = hit::shapes_in_rect(&page, Rect::new(x, y, px, py));
        let _ = hit::connectors_in_rect(&page, Rect::new(x, y, px, py));
        let _ = hit::connection_point_at(&s, p, tol);
        let _ = connector_geometry(&page, &c);
        let _ = connector_geometry(&page, &free);
        page.sanitize();
        let g = connector_geometry(&page, page.connectors.first().unwrap());
        prop_assert!(g.ends.start.pos.is_finite() && g.ends.end.pos.is_finite());
        if route == Route::Orthogonal {
            prop_assert!(axis_aligned(&g.polyline), "{:?}", g.polyline);
        }
        page.detach_shape("s");
        let json = serde_json::to_string(&page).unwrap();
        let _: Page = serde_json::from_str(&json).unwrap();
    }
}
