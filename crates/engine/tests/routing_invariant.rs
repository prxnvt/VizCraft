//! The routing invariant, end to end: no connector's drawn line enters a shape it must avoid,
//! except a connector the router reports it could not route (its fallback). Checked over seeded
//! random obstacle fields, the sample document, the Mermaid, CSV and draw.io (JGraph) imports, and
//! every `layout.full` algorithm, with the page policy at its default (every connector avoids
//! shapes).

use serde_json::json;
use vizcraft_diagram::autoroute::{crossings, head_trim, reroute, unroutable};
use vizcraft_diagram::{Connector, End, Page, Point, Route, Shape, ShapeKind};
use vizcraft_engine::{Document, Session};

const DRAWIO: &str = include_str!("../../formats/testdata/drawio-docs-example.drawio");

const DRAWIO_PAGES: &str = include_str!("../../formats/testdata/multipage-compressed.drawio");

const MERMAID: &str =
    "flowchart TB\n  X --> A\n  subgraph G [Group one]\n    A --> B\n  end\n  B --> Y\n  Y --> G\n  Y --> Z{Ok?}\n  Z -->|yes| A\n  Z -->|no| X\n";

const MERMAID_NESTED: &str = "flowchart LR\n  subgraph outer\n    A\n    subgraph inner [Inner]\n      direction TB\n      B --> C\n    end\n  end\n  A --> B\n  D --> A\n  C --> D\n";

const CSV: &str = "\
# label: %name%
# identity: id
# connect: {\"from\": \"manager\", \"to\": \"id\", \"invert\": true}
# layout: verticaltree
id,name,manager
1,Ada,
2,Grace,1
3,Linus,1
4,Barbara,2
5,Ken,2
6,Dennis,3
";

const ALGORITHMS: [&str; 4] = ["layered", "tree", "radial", "force"];

/// Every crossing on `page` belongs to a connector the router cannot route.
fn assert_invariant(page: &Page, what: &str) {
    let found = crossings(page);
    if found.is_empty() {
        return;
    }
    let fallback = unroutable(page);
    for x in &found {
        assert!(
            fallback.contains(&x.connector),
            "{what}: connector {} crosses shape {} on segment {} and is not reported as unroutable ({:?})",
            x.connector,
            x.shape,
            x.segment,
            page.connector(&x.connector).map(|c| &c.waypoints)
        );
    }
}

fn assert_session(s: &Session, what: &str) {
    for (i, page) in s.doc.pages.iter().enumerate() {
        assert_invariant(page, &format!("{what}, page {i}"));
    }
}

/// A small deterministic generator, so a failure names its case and reruns the same.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() % 1_000_000) as f64 / 1_000_000.0 * (hi - lo)
    }
}

const KINDS: [ShapeKind; 5] = [ShapeKind::Rectangle, ShapeKind::Ellipse, ShapeKind::Diamond, ShapeKind::RoundedRectangle, ShapeKind::Hexagon];

/// A field of non-overlapping shapes, some rotated, and a few connectors between them (and to
/// free points) in every route style and glue kind.
fn random_page(rng: &mut Rng) -> Page {
    let mut page = Page::new("p1", "Page-1");
    let count = 4 + rng.below(12);
    let mut tries = 0;
    while page.shapes.len() < count && tries < 400 {
        tries += 1;
        let (w, h) = (rng.range(30.0, 140.0), rng.range(20.0, 110.0));
        let (x, y) = (rng.range(0.0, 900.0), rng.range(0.0, 700.0));
        let mut s = Shape::new(format!("s{}", page.shapes.len()), KINDS[rng.below(KINDS.len())], x, y, w, h);
        if rng.below(6) == 0 {
            s.angle_deg = [30.0, 45.0, 90.0][rng.below(3)];
        }
        let b = s.bounds();
        let clear = page.shapes.iter().all(|o| {
            let ob = o.bounds();
            b.x1 + 8.0 < ob.x0 || ob.x1 + 8.0 < b.x0 || b.y1 + 8.0 < ob.y0 || ob.y1 + 8.0 < b.y0
        });
        if clear {
            page.shapes.push(s);
        }
    }
    let n = page.shapes.len();
    for k in 0..3 {
        let a = rng.below(n);
        let mut b = rng.below(n);
        if b == a {
            b = (a + 1) % n;
        }
        let end = |rng: &mut Rng, page: &Page, i: usize| -> End {
            let id = page.shapes[i].id.clone();
            match rng.below(5) {
                0 => End::point(id, rng.below(4)),
                1 => {
                    // A free point outside every shape.
                    for _ in 0..50 {
                        let p = Point::new(rng.range(-50.0, 1000.0), rng.range(-50.0, 800.0));
                        if page.shapes.iter().all(|s| !s.bounds().inflate(12.0).contains(p)) {
                            return End::free(p);
                        }
                    }
                    End::dynamic(id)
                }
                _ => End::dynamic(id),
            }
        };
        let (start, finish) = (end(rng, &page, a), end(rng, &page, b));
        let mut c = Connector::new(format!("c{k}"), start, finish);
        c.route = [Route::Orthogonal, Route::Orthogonal, Route::Straight, Route::Curved][rng.below(4)];
        page.connectors.push(c);
    }
    page
}

#[test]
fn five_hundred_random_fields_route_clear_or_report_failure() {
    let mut rng = Rng(0x2c2c_2c2c_5eed);
    let (mut routed, mut failed) = (0usize, 0usize);
    for case in 0..500 {
        let mut page = random_page(&mut rng);
        let fallback = reroute(&mut page, None);
        routed += page.connectors.len();
        failed += fallback.len();
        for x in crossings(&page) {
            assert!(
                fallback.contains(&x.connector),
                "case {case}: {} crosses {} on segment {} without being reported: {:?}",
                x.connector,
                x.shape,
                x.segment,
                page.connector(&x.connector).map(|c| (&c.start, &c.end, c.route, &c.waypoints))
            );
        }
        // Rerouting a routed page changes nothing: the result depends on the shapes alone.
        let again = page.clone();
        reroute(&mut page, None);
        assert_eq!(page, again, "case {case}: rerouting is not stable");
    }
    // Shapes 8 apart leave room almost everywhere; failures must stay rare.
    assert!(failed * 50 <= routed, "{failed} of {routed} connectors had no clear route");
}

#[test]
fn random_fields_stay_clear_through_commands() {
    let mut rng = Rng(0xfeed_beef_cafe);
    for case in 0..40 {
        let page = random_page(&mut rng);
        let mut doc = Document::new();
        doc.pages = vec![page];
        let mut s = Session::new(doc);
        // Any change reroutes the connectors it can affect; a page that was never routed is
        // routed by the first command that touches its shapes.
        s.run("layout.route", &json!({})).unwrap();
        assert_session(&s, &format!("case {case} routed"));
        for step in 0..4 {
            let n = s.doc.page().unwrap().shapes.len();
            let id = format!("s{}", rng.below(n));
            let (dx, dy) = (rng.range(-150.0, 150.0).round(), rng.range(-150.0, 150.0).round());
            s.run("shape.move", &json!({"ids": [id], "dx": dx, "dy": dy})).unwrap();
            assert_session(&s, &format!("case {case} step {step} after moving {id}"));
        }
    }
}

#[test]
fn the_sample_document_stays_clear() {
    let mut s = Session::new(vizcraft_engine::sample::sample_document());
    assert!(crossings(s.doc.page().unwrap()).is_empty(), "the sample's plain routes are clear");
    s.run("layout.route", &json!({})).unwrap();
    assert!(crossings(s.doc.page().unwrap()).is_empty());
    // Park the fix-up step across the main column's connectors, then move it back.
    s.run("shape.move", &json!({"ids": ["s6"], "dx": -260, "dy": 120})).unwrap();
    assert_session(&s, "sample with Fix input moved");
    assert!(crossings(s.doc.page().unwrap()).is_empty());
    s.run("shape.move", &json!({"ids": ["s6"], "dx": 260, "dy": -120})).unwrap();
    assert!(crossings(s.doc.page().unwrap()).is_empty());
    for algorithm in ALGORITHMS {
        let mut t = Session::new(vizcraft_engine::sample::sample_document());
        t.run("layout.full", &json!({"algorithm": algorithm})).unwrap();
        assert_session(&t, &format!("sample after layout.full {algorithm}"));
    }
}

fn imported(text: &str, format: &str) -> Session {
    let mut s = Session::new(Document::new());
    s.run("file.import", &json!({"text": text, "format": format, "replace": true})).unwrap();
    s
}

#[test]
fn imports_stay_clear_and_so_do_their_layouts() {
    for (name, text, format) in [
        ("mermaid", MERMAID, "mermaid"),
        ("nested mermaid", MERMAID_NESTED, "mermaid"),
        ("csv", CSV, "csv"),
        ("drawio", DRAWIO, "drawio"),
        ("multipage drawio", DRAWIO_PAGES, "drawio"),
    ] {
        let s = imported(text, format);
        assert!(s.doc.pages.iter().any(|p| !p.connectors.is_empty()), "{name} imported connectors");
        assert_session(&s, name);
        for algorithm in ALGORITHMS {
            let mut t = imported(text, format);
            t.run("layout.full", &json!({"algorithm": algorithm})).unwrap();
            assert_session(&t, &format!("{name} after layout.full {algorithm}"));
        }
    }
}

/// A page built through commands with a group, a container around two shapes, a three-lane
/// swimlane with two shapes per lane, a callout and connectors across all of them.
fn parts() -> Session {
    let mut s = Session::new(Document::new());
    let run = |s: &mut Session, id: &str, v: serde_json::Value| s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"));
    let add = |s: &mut Session, id: &str, x: f64, y: f64| {
        s.run("shape.add", &json!({"stencil": "flowchart.process", "x": x, "y": y, "id": id, "text": id})).unwrap();
    };
    add(&mut s, "ga", 60.0, 60.0);
    add(&mut s, "gb", 220.0, 60.0);
    run(&mut s, "arrange.group", json!({"ids": ["ga", "gb"], "id": "g"}));
    add(&mut s, "n1", 500.0, 60.0);
    add(&mut s, "n2", 700.0, 60.0);
    add(&mut s, "k1", 1000.0, 60.0);
    add(&mut s, "k2", 1000.0, 200.0);
    run(&mut s, "container.add", json!({"ids": ["k1", "k2"], "heading": "Box", "id": "box"}));
    let lanes = run(&mut s, "lane.addSet", json!({"x": 60, "y": 300, "lanes": 3, "title": "Process", "id": "set"}))["lanes"].clone();
    for (i, lane) in lanes.as_array().unwrap().iter().enumerate() {
        for (j, x) in [160.0, 400.0].into_iter().enumerate() {
            let id = format!("l{i}{j}");
            add(&mut s, &id, x + 120.0 * i as f64, 360.0 + 140.0 * i as f64);
            run(&mut s, "container.addMember", json!({"container": lane, "ids": [id]}));
        }
    }
    run(&mut s, "callout.add", json!({"target": "n1", "text": "Note", "id": "co"}));
    for (a, b) in [
        ("ga", "n1"),
        ("n1", "n2"),
        ("n2", "l00"),
        ("l00", "l01"),
        ("l01", "l10"),
        ("l10", "l11"),
        ("l11", "l20"),
        ("l20", "l21"),
        ("gb", "l10"),
        ("l00", "l21"),
        ("k1", "k2"),
        ("n2", "k2"),
        ("l21", "k1"),
    ] {
        run(&mut s, "connector.add", json!({"from": {"shape": a}, "to": {"shape": b}}));
    }
    s
}

#[test]
fn groups_containers_and_swimlanes_stay_clear() {
    let mut s = parts();
    assert_session(&s, "parts");
    let page = s.doc.page().unwrap();
    assert!(crossings(page).is_empty(), "every connector in the parts page has a clear route: {:?}", crossings(page));
    // Moving a lane member and the container keeps everything clear.
    s.run("shape.move", &json!({"ids": ["l10"], "dx": 40, "dy": 10})).unwrap();
    s.run("shape.move", &json!({"ids": ["box"], "dx": -200, "dy": 40})).unwrap();
    assert_session(&s, "parts after moves");
    for algorithm in ALGORITHMS {
        let mut t = parts();
        t.run("layout.full", &json!({"algorithm": algorithm})).unwrap();
        assert_session(&t, &format!("parts after layout.full {algorithm}"));
    }
}

/// A grid of shapes with generous gaps and a few connectors between random pairs, so every
/// connector has room for a full stub at both ends.
fn roomy_page(rng: &mut Rng) -> Page {
    let mut page = Page::new("p1", "Page-1");
    for i in 0..9 {
        let (col, row) = ((i % 3) as f64, (i / 3) as f64);
        page.shapes.push(Shape::new(format!("s{i}"), ShapeKind::Rectangle, col * 220.0, row * 160.0, 90.0, 50.0));
    }
    for k in 0..6 {
        let a = rng.below(9);
        let b = (a + 1 + rng.below(8)) % 9;
        let mut c = Connector::new(format!("c{k}"), End::dynamic(format!("s{a}")), End::dynamic(format!("s{b}")));
        c.route = Route::Orthogonal;
        page.connectors.push(c);
    }
    page
}

#[test]
fn the_last_segment_is_at_least_the_arrowhead_trim() {
    let mut rng = Rng(0x57ab_57ab_1234);
    let mut checked = 0usize;
    for case in 0..200 {
        let mut page = roomy_page(&mut rng);
        let fallback = reroute(&mut page, None);
        for c in page.connectors.iter().filter(|c| !fallback.contains(&c.id)) {
            // Only a route the router stored holds its whole polyline, ends included.
            if !vizcraft_diagram::route::stored_route(&page, c) {
                continue;
            }
            let w = &c.waypoints;
            let (Some(a), Some(b), Some(first), Some(second)) = (w.last(), w.len().checked_sub(2).and_then(|i| w.get(i)), w.first(), w.get(1)) else {
                continue;
            };
            let lw = c.style.line_width;
            let (start_trim, end_trim) = (head_trim(c.start_arrow, lw), head_trim(c.end_arrow, lw));
            assert!(
                a.distance(*b) + 1e-6 >= end_trim,
                "case {case}: {} ends in a segment of {} but its arrowhead needs {end_trim}",
                c.id,
                a.distance(*b)
            );
            assert!(first.distance(*second) + 1e-6 >= start_trim, "case {case}: {} starts in a segment shorter than {start_trim}", c.id);
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} routes were checked");
}

/// Every route stored on `page` leaves and enters its shapes through a segment at least as long as
/// the arrowhead's trim: the head's tip never lies inside the stub's bend.
fn assert_stubs(page: &Page, what: &str) -> usize {
    let mut checked = 0;
    for c in &page.connectors {
        let w = &c.waypoints;
        let (Some(first), Some(second), Some(last), Some(before)) = (w.first(), w.get(1), w.last(), w.len().checked_sub(2).and_then(|i| w.get(i)))
        else {
            continue;
        };
        let lw = c.style.line_width;
        assert!(last.distance(*before) + 1e-6 >= head_trim(c.end_arrow, lw), "{what}: {} ends in a stub shorter than its arrowhead", c.id);
        assert!(first.distance(*second) + 1e-6 >= head_trim(c.start_arrow, lw), "{what}: {} starts in a stub shorter than its arrowhead", c.id);
        checked += 1;
    }
    checked
}

#[test]
fn no_arrow_tip_lies_inside_a_stub_on_the_sample() {
    let mut s = Session::new(vizcraft_engine::sample::sample_document());
    s.run("connector.setArrows", &json!({"start": "triangle", "end": "triangle"})).ok();
    s.run("layout.full", &json!({"algorithm": "layered"})).unwrap();
    for page in &s.doc.pages {
        assert_stubs(page, "sample");
    }
    // Wide lines and big heads on a roomy page: the stub grows with the head, per end.
    let mut rng = Rng(0x7a11_7a11_9a11);
    let mut checked = 0;
    for _ in 0..50 {
        let mut page = roomy_page(&mut rng);
        for c in &mut page.connectors {
            c.start_arrow = vizcraft_diagram::Arrow::Triangle;
            c.end_arrow = vizcraft_diagram::Arrow::HollowDiamond;
            c.style.line_width = 6.0;
        }
        let fallback = reroute(&mut page, None);
        page.connectors.retain(|c| !fallback.contains(&c.id));
        checked += assert_stubs(&page, "roomy");
    }
    assert!(checked > 50, "only {checked} routes were checked");
}
