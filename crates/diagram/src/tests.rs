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

#[test]
fn adopt_keeps_free_plain_ids_and_renames_clashes() {
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.shapes.push(Shape::new("A", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
    let mut src = Page::new("x", "x");
    src.shapes.push(Shape::new("A", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
    src.shapes.push(Shape::new("B", ShapeKind::Ellipse, 50.0, 0.0, 10.0, 10.0));
    src.shapes.push(Shape::new("not plain!", ShapeKind::Ellipse, 90.0, 0.0, 10.0, 10.0));
    src.connectors.push(Connector::new("e1", End::dynamic("A"), End::dynamic("not plain!")));
    src.connectors.push(Connector::new("e2", End::dynamic("B"), End::dynamic("missing")));
    let ids = doc.adopt(0, src, false);
    assert_eq!(ids.len(), 5);
    assert_ne!(ids[0], "A", "A was taken, so the imported A is renamed");
    assert_eq!(ids[1], "B");
    assert!(is_plain_id(&ids[2]));
    let page = doc.page().unwrap();
    assert_eq!(page.shapes.len(), 4);
    let e1 = page.connector("e1").unwrap();
    assert_eq!(e1.start.shape(), Some(ids[0].as_str()));
    assert_eq!(e1.end.shape(), Some(ids[2].as_str()));
    let e2 = page.connector("e2").unwrap();
    assert_eq!(e2.start.shape(), Some("B"));
    assert!(e2.end.shape().is_none(), "glue to a shape that is nowhere becomes a free end");
}

#[test]
fn adopt_with_replace_clears_the_page_and_ignores_a_missing_page() {
    let mut doc = Document::new();
    doc.page_mut().unwrap().shapes.push(Shape::new("old", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0));
    let mut src = Page::new("x", "x");
    src.shapes.push(Shape::new("old", ShapeKind::Rectangle, 5.0, 5.0, 10.0, 10.0));
    assert_eq!(doc.adopt(0, src.clone(), true), vec!["old".to_string()]);
    assert_eq!(doc.page().unwrap().shapes.len(), 1);
    assert!(doc.adopt(7, src, true).is_empty());
}

#[test]
fn find_page_by_id_name_or_index_and_add_page() {
    let mut doc = Document::new();
    let i = doc.add_page("Second");
    assert_eq!(i, 1);
    let id = doc.pages[1].id.clone();
    assert_eq!(doc.find_page(&id), Some(1));
    assert_eq!(doc.find_page("Second"), Some(1));
    assert_eq!(doc.find_page("Page-1"), Some(0));
    assert_eq!(doc.find_page("1"), Some(1));
    assert_eq!(doc.find_page("2"), None);
    assert_eq!(doc.find_page("nope"), None);
    assert_eq!(doc.add_page(""), 2);
    assert_eq!(doc.pages[2].name, "Page-3");
}

#[test]
fn layout_items_places_a_chain_without_overlaps() {
    use crate::autolayout::{Options, layout_items};
    let mut page = Page::new("p", "p");
    // All start on top of each other.
    for id in ["a", "b", "c", "d"] {
        page.shapes.push(Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
    }
    for (a, b) in [("a", "b"), ("b", "c"), ("b", "d")] {
        let mut c = Connector::new(format!("{a}{b}"), End::dynamic(a), End::dynamic(b));
        c.waypoints.push(Point::new(1.0, 1.0));
        page.connectors.push(c);
    }
    layout_items(&mut page, None, &Options::default()).unwrap();
    for (i, s) in page.shapes.iter().enumerate() {
        assert!(s.x >= 40.0 - 1e-9 && s.y >= 40.0 - 1e-9);
        for t in page.shapes.iter().skip(i + 1) {
            let (a, b) = (s.frame(), t.frame());
            let overlap = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
            assert!(!overlap, "{} and {} overlap", s.id, t.id);
        }
    }
    // Top to bottom by default: each child is below its parent.
    assert!(page.shape("b").unwrap().y > page.shape("a").unwrap().y);
    assert!(page.connectors.iter().all(|c| c.waypoints.is_empty()), "stale bends are dropped");
}

#[test]
fn layout_items_on_a_selection_keeps_its_corner_and_leaves_others() {
    use crate::autolayout::{Options, layout_items};
    let mut page = Page::new("p", "p");
    page.shapes.push(Shape::new("a", ShapeKind::Rectangle, 300.0, 200.0, 100.0, 60.0));
    page.shapes.push(Shape::new("b", ShapeKind::Rectangle, 300.0, 200.0, 100.0, 60.0));
    page.shapes.push(Shape::new("z", ShapeKind::Rectangle, 0.0, 0.0, 100.0, 60.0));
    let ids = vec!["a".to_string(), "b".to_string()];
    layout_items(&mut page, Some(&ids), &Options::default()).unwrap();
    let (a, b) = (page.shape("a").unwrap(), page.shape("b").unwrap());
    assert!((a.x.min(b.x) - 300.0).abs() < 1e-6 && (a.y.min(b.y) - 200.0).abs() < 1e-6);
    assert_eq!((page.shape("z").unwrap().x, page.shape("z").unwrap().y), (0.0, 0.0));
}

#[test]
fn new_connector_fields_round_trip_and_stay_off_disk_when_empty() {
    let c = Connector::new("c1", End::dynamic("a"), End::dynamic("b"));
    let json = serde_json::to_string(&c).unwrap();
    assert!(["\"waypoints\"", "\"avoid\"", "\"jumps\"", "\"ext\""].iter().all(|k| !json.contains(k)), "{json}");
    let mut c2 = c.clone();
    c2.waypoints = vec![Point::new(1.0, 2.0)];
    c2.avoid = true;
    c2.jumps = true;
    c2.ext.insert("drawio.style".into(), "dashed=1".into());
    let back: Connector = serde_json::from_str(&serde_json::to_string(&c2).unwrap()).unwrap();
    assert_eq!(back, c2);
    let junk: Connector = serde_json::from_str(r#"{"waypoints": 5, "avoid": "yes", "ext": [1]}"#).unwrap();
    assert!(junk.waypoints.is_empty() && !junk.avoid && junk.ext.is_empty());
}

#[test]
fn non_finite_new_style_and_shape_numbers_are_sanitised() {
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    let st = &mut s.style;
    st.indent = f64::NAN;
    st.corner_radius = f64::INFINITY;
    st.fill_transparency = f64::NAN;
    st.line_transparency = f64::NEG_INFINITY;
    st.soft_edges = f64::NAN;
    st.text_margins = Margins { left: f64::NAN, top: f64::INFINITY, right: -1.0, bottom: 2.0 };
    st.gradient =
        Some(Gradient { kind: GradientKind::Linear, angle_deg: f64::NAN, stops: vec![GradientStop { offset: f64::NAN, color: Color::WHITE }] });
    st.shadow = Some(Shadow { dx: f64::NAN, dy: f64::INFINITY, blur: f64::NAN, color: Color::BLACK });
    st.glow = Some(Glow { radius: f64::NAN, color: Color::BLACK });
    s.text_block = Some(TextBlock { dx: f64::NAN, dy: f64::NEG_INFINITY, w: f64::NAN, h: f64::INFINITY, angle_deg: f64::NAN });
    s.image = Some(ImageRef { asset: "i".repeat(500), crop: CropRect { x: f64::NAN, y: 0.5, w: f64::INFINITY, h: 2.0 } });
    s.sanitize();
    let st = &s.style;
    assert_eq!((st.indent, st.corner_radius, st.fill_transparency, st.line_transparency, st.soft_edges), (0.0, 0.0, 0.0, 0.0, 0.0));
    assert_eq!((st.text_margins.left, st.text_margins.top, st.text_margins.right, st.text_margins.bottom), (4.0, 4.0, 0.0, 2.0));
    let g = st.gradient.as_ref().unwrap();
    assert_eq!((g.angle_deg, g.stops[0].offset), (0.0, 0.0));
    assert!(!g.is_drawable());
    let sh = st.shadow.unwrap();
    assert_eq!((sh.dx, sh.dy, sh.blur), (0.0, 0.0, 0.0));
    assert_eq!(st.glow.unwrap().radius, 0.0);
    let tb = s.text_block.unwrap();
    assert_eq!((tb.dx, tb.dy, tb.w, tb.h, tb.angle_deg), (0.0, 0.0, 0.0, 0.0, 0.0));
    let img = s.image.as_ref().unwrap();
    assert_eq!(img.asset.len(), model::MAX_ID);
    assert_eq!((img.crop.x, img.crop.y, img.crop.w, img.crop.h), (0.0, 0.5, 1.0, 0.5));

    let mut page = Page::new("p1", "Page-1");
    page.drawing_scale = DrawingScale { page: f64::NAN, drawing: 0.0, units: Units::Feet };
    page.guides = vec![Guide { orientation: GuideOrientation::Vertical, position: f64::NAN }];
    page.sanitize();
    assert_eq!((page.drawing_scale.page, page.drawing_scale.drawing), (1.0, 1.0));
    assert_eq!(page.guides[0].position, 0.0);
}

#[test]
fn transparency_scales_alpha() {
    let st = Style { fill: Color { r: 1, g: 2, b: 3, a: 200 }, fill_transparency: 0.5, line_transparency: 1.0, ..Style::DEFAULT };
    assert_eq!(st.effective_fill(), Color { r: 1, g: 2, b: 3, a: 100 });
    assert_eq!(st.effective_line().a, 0);
    assert_eq!(Style::DEFAULT.effective_fill(), Style::DEFAULT.fill);
}

#[test]
fn text_case_and_bullets() {
    assert_eq!(TextCase::Title.apply("mix the  batch-two"), "Mix The  Batch-two");
    assert_eq!(TextCase::Upper.apply("ab"), "AB");
    assert_eq!(TextCase::Lower.apply("AB"), "ab");
    assert_eq!(TextCase::Normal.apply("aB"), "aB");
    assert_eq!(Bullets::Number.marker(1), "2. ");
    assert_eq!(Bullets::None.marker(0), "");
    assert_eq!(Bullets::Number.marker(usize::MAX), format!("{}. ", usize::MAX));
}

#[test]
fn text_rect_is_the_frame_or_the_text_block() {
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
    assert_eq!(s.text_rect(), (Rect::from_xywh(0.0, 0.0, 100.0, 60.0), 0.0));
    s.text_block = Some(TextBlock { dx: 10.0, dy: -5.0, w: 40.0, h: 20.0, angle_deg: 90.0 });
    assert_eq!(s.text_rect(), (Rect::from_xywh(40.0, 15.0, 40.0, 20.0), 90.0));
}

#[test]
fn layers_and_hidden_flag_decide_visibility() {
    let mut page = Page::new("p1", "Page-1");
    page.layers = vec![Layer { id: "a".into(), ..Layer::default() }, Layer { id: "b".into(), visible: false, ..Layer::default() }];
    let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    assert!(page.shape_visible(&s));
    s.layers = vec!["b".into()];
    assert!(!page.shape_visible(&s));
    s.layers = vec!["b".into(), "a".into()];
    assert!(page.shape_visible(&s), "visible when any of its layers is");
    s.layers.clear();
    s.behavior.hidden = true;
    assert!(!page.shape_visible(&s));
}

#[test]
fn background_chain_is_deepest_first_and_bounded() {
    let mut doc = Document::new();
    for i in 2..=12 {
        let mut p = Page::new(format!("p{i}"), format!("B{i}"));
        p.is_background = true;
        doc.pages.push(p);
    }
    // p1 -> p2 -> ... -> p12, then p12 -> p2 makes a cycle.
    for i in 0..doc.pages.len() {
        let next = if i + 1 < doc.pages.len() { format!("p{}", i + 2) } else { "p2".into() };
        doc.pages[i].background = Some(next);
    }
    doc.sanitize();
    let chain: Vec<&str> = doc.background_chain(0).iter().map(|p| p.id.as_str()).collect();
    assert_eq!(chain.len(), model::MAX_BACKGROUND_DEPTH);
    assert_eq!(chain.last(), Some(&"p2"), "the nearest background is drawn last, just under the page");
    // p11 -> p12 -> p2 -> ... runs through the cycle without repeating a page or itself.
    let chain: Vec<&str> = doc.background_chain(10).iter().map(|p| p.id.as_str()).collect();
    let unique: std::collections::HashSet<&&str> = chain.iter().collect();
    assert_eq!(unique.len(), chain.len(), "{chain:?}");
    assert!(!chain.contains(&"p11") && chain.last() == Some(&"p12"), "{chain:?}");
    // A short cycle stops at the repeat.
    doc.pages[2].background = Some("p2".into());
    let chain: Vec<&str> = doc.background_chain(1).iter().map(|p| p.id.as_str()).collect();
    assert_eq!(chain, ["p3"]);
    assert!(doc.background_chain(99).is_empty());
}

#[test]
fn rounded_corners_round_straight_outlines_only() {
    let square = Path::polygon(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0)]);
    let r = square.rounded_corners(10.0);
    assert_eq!(r.els.iter().filter(|e| matches!(e, PathEl::CubicTo(..))).count(), 4);
    assert_eq!(r.els.first(), Some(&PathEl::MoveTo(Point::new(0.0, 10.0))));
    assert_eq!(r.bounds(), square.bounds());
    // The radius shrinks to half the shorter side.
    let thin = Path::polygon(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 4.0), Point::new(0.0, 4.0)]);
    let r = thin.rounded_corners(50.0);
    assert_eq!(
        r.els.get(1),
        Some(&PathEl::CubicTo(Point::new(0.0, 2.0 - 2.0 * 0.552_284_75), Point::new(2.0 - 2.0 * 0.552_284_75, 0.0), Point::new(2.0, 0.0)))
    );
    // An open polyline keeps its ends; curves and bad radii change nothing.
    let open = Path::polyline(&[Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(50.0, 50.0)]);
    let r = open.rounded_corners(5.0);
    assert_eq!(r.els.first(), Some(&PathEl::MoveTo(Point::new(0.0, 0.0))));
    assert_eq!(r.els.last(), Some(&PathEl::LineTo(Point::new(50.0, 50.0))));
    let curve = ShapeKind::Ellipse.outline(50.0, 30.0);
    assert_eq!(curve.rounded_corners(5.0), curve);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(square.rounded_corners(bad), square);
    }
}

#[test]
fn base64_round_trips_and_rejects_junk() {
    use crate::serde_util::base64::{decode, encode};
    for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0, 255, 128, 7]] {
        assert_eq!(decode(&encode(bytes)).as_deref(), Some(bytes));
    }
    assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(encode(b"fo"), "Zm8=");
    assert_eq!(decode("Zm9v\nYmFy").as_deref(), Some(&b"foobar"[..]));
    assert_eq!(decode("Zm9v*"), None);
}

proptest! {
    #[test]
    fn rounded_corners_never_panic(pts in proptest::collection::vec((-1e6f64..1e6, -1e6f64..1e6), 0..12), r in -10.0f64..1e7) {
        let pts: Vec<Point> = pts.into_iter().map(|(x, y)| Point::new(x, y)).collect();
        let _ = Path::polygon(&pts).rounded_corners(r);
        let _ = Path::polyline(&pts).rounded_corners(r);
    }

    #[test]
    fn base64_decode_never_panics(s in ".{0,64}") {
        let _ = crate::serde_util::base64::decode(&s);
    }
}
