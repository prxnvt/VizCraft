//! Tests of segments, junctions, components, the net index and line labels.

use vizcraft_model::{Net, NetKind, NetLabel};

use super::*;
use crate::geom::Point;
use crate::model::{Color, Connector, Document, End, Page, Shape};
use crate::route::{resolve_ends, route_polyline};
use crate::{Route, ShapeKind};

fn wire(id: &str, a: End, b: End, net: Option<&str>) -> Connector {
    Connector { route: Route::Orthogonal, end_arrow: crate::Arrow::None, net: net.map(str::to_string), ..Connector::new(id, a, b) }
}

fn free(x: f64, y: f64) -> End {
    End::free(Point::new(x, y))
}

/// A device with pins on its left and right sides (connection points 0 and 1).
fn device(id: &str, x: f64, y: f64) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, 40.0, 20.0);
    s.connection_points = vec![Point::new(0.0, 0.5), Point::new(1.0, 0.5)];
    s
}

#[test]
fn crossing_wires_stay_apart_and_shared_pins_join() {
    let mut page = Page::new("p", "P");
    page.connectors.push(wire("h", free(0.0, 50.0), free(200.0, 50.0), Some("n1")));
    page.connectors.push(wire("v", free(100.0, 0.0), free(100.0, 200.0), Some("n2")));
    let comps = components(0, &page, |c| c.net.is_some());
    assert_eq!(comps.len(), 2, "a crossing joins nothing");
    // Two wires on one pin join; dynamic glue to the same shape does not.
    page.shapes.push(device("r1", 300.0, 300.0));
    page.connectors.push(wire("a", End::point("r1", 0), free(250.0, 310.0), Some("n3")));
    page.connectors.push(wire("b", End::point("r1", 0), free(250.0, 400.0), Some("n4")));
    page.connectors.push(wire("c", End::dynamic("r1"), free(500.0, 310.0), Some("n5")));
    page.connectors.push(wire("d", End::dynamic("r1"), free(500.0, 400.0), Some("n6")));
    let comps = components(0, &page, |c| c.net.is_some());
    let of = |id: &str| comps.iter().position(|k| k.segments.iter().any(|&i| page.connectors[i].id == id)).unwrap();
    assert_eq!(of("a"), of("b"));
    assert_ne!(of("c"), of("d"));
    assert_eq!(comps[of("a")].pins, vec![("r1".to_string(), 0)]);
    assert_eq!(comps.len(), 5);
}

#[test]
fn splitting_a_wire_adds_one_junction_and_the_junction_carries_every_end() {
    let mut page = Page::new("p", "P");
    page.shapes.push(device("r1", 0.0, 90.0));
    page.shapes.push(device("r2", 300.0, 90.0));
    page.connectors.push(wire("w1", End::point("r1", 1), End::point("r2", 0), Some("n1")));
    let before = route_polyline(&page, page.connector("w1").unwrap());
    let q = split_at(&mut page, "w1", Point::new(150.0, 130.0), "w2", "j1", true).expect("a cut in the middle");
    assert_eq!(q, Point::new(150.0, 100.0), "the cut is projected onto the route");
    let j = page.shape("j1").unwrap();
    assert!(is_junction(j) && j.connection_points.len() == 1);
    assert_eq!(j.center(), q);
    let (a, b) = (page.connector("w1").unwrap(), page.connector("w2").unwrap());
    assert_eq!((a.end.shape(), b.start.shape()), (Some("j1"), Some("j1")));
    assert_eq!((a.net.as_deref(), b.net.as_deref()), (Some("n1"), Some("n1")), "the new half keeps the net");
    // The two halves draw the line it was.
    let mut joined = route_polyline(&page, a);
    joined.extend(route_polyline(&page, b).into_iter().skip(1));
    let simplify = |pts: Vec<Point>| -> Vec<Point> { pts.into_iter().filter(|p| p.x != 150.0).collect() };
    assert_eq!(simplify(joined), simplify(before.clone()));
    // A branch to the junction, then the junction moves: all three ends follow.
    page.connectors.push(wire("w3", End::point("j1", 0), free(150.0, 300.0), Some("n1")));
    page.shapes.iter_mut().find(|s| s.id == "j1").unwrap().y += 40.0;
    let moved = Point::new(150.0, 140.0);
    for (id, start) in [("w1", false), ("w2", true), ("w3", true)] {
        let ends = resolve_ends(&page, page.connector(id).unwrap());
        let p = if start { ends.start.pos } else { ends.end.pos };
        assert!(p.distance(moved) < 1e-9, "{id} follows the junction: {p:?}");
    }
    // No split at an end, onto taken ids or of a missing connector.
    assert!(split_at(&mut page, "w3", Point::new(150.0, 300.0), "x1", "x2", true).is_none());
    assert!(split_at(&mut page, "w3", Point::new(150.0, 200.0), "w1", "x2", true).is_none());
    assert!(split_at(&mut page, "nope", Point::new(150.0, 200.0), "x1", "x2", true).is_none());
    assert!(split_at(&mut page, "w3", Point::new(f64::NAN, 200.0), "x1", "x2", true).is_none());
}

#[test]
fn joining_through_a_junction_gives_back_the_route() {
    let mut page = Page::new("p", "P");
    page.connectors.push(wire("w1", free(0.0, 0.0), free(200.0, 100.0), Some("n1")));
    let before = route_polyline(&page, page.connector("w1").unwrap());
    split_at(&mut page, "w1", Point::new(100.0, 40.0), "w2", "j1", false).unwrap();
    assert_eq!(page.shape("j1").unwrap().style.fill, Color::TRANSPARENT, "a pipe tee draws no dot");
    // Join in either direction: the second half reversed first.
    super::reverse(page.connector_mut("w2").unwrap());
    assert!(join(&mut page, "w1", "w2", "j1"));
    assert!(page.connector("w2").is_none());
    let after = route_polyline(&page, page.connector("w1").unwrap());
    assert_eq!(after, before);
    assert!(!join(&mut page, "w1", "w1", "j1"));
    assert!(!join(&mut page, "w1", "nope", "j1"));
}

#[test]
fn the_index_maps_segments_pins_junctions_and_conflicts() {
    let mut doc = Document::new();
    doc.model.nets.push(Net { name: "L1".into(), ..Net::new("n1", NetKind::Potential) });
    doc.model.nets.push(Net { name: "L2".into(), ..Net::new("n2", NetKind::Potential) });
    doc.model.nets.push(Net::new("n3", NetKind::Wire));
    let page = &mut doc.pages[0];
    page.shapes.push(device("r1", 100.0, 100.0));
    page.connectors.push(wire("a", End::point("r1", 0), free(0.0, 110.0), Some("n1")));
    page.connectors.push(wire("b", End::point("r1", 0), free(0.0, 300.0), Some("n2")));
    page.connectors.push(wire("c", End::point("r1", 1), free(400.0, 110.0), Some("n3")));
    let ix = NetIndex::build(&doc);
    assert_eq!(ix.net_of_connector("c"), Some("n3"));
    assert_eq!(ix.nets["n1"].pins, vec![("r1".to_string(), 0)]);
    assert_eq!(ix.of_pin.get(&("r1".to_string(), 1)).map(String::as_str), Some("n3"));
    assert_eq!(ix.conflicts.len(), 1);
    assert_eq!(ix.conflicts[0].nets, ["n1", "n2"]);
    assert_eq!(ix.conflicts[0].segments, ["a", "b"]);
    assert_eq!(NetIndex::build(&Document::new()), NetIndex::default());
}

#[test]
fn labels_go_once_on_the_longest_segment_or_on_each() {
    let mut doc = Document::new();
    doc.model.nets.push(Net { number: "2003".into(), ..Net::new("n1", NetKind::Wire) });
    doc.model.nets.push(Net { name: "L1".into(), label: NetLabel::Each, ..Net::new("n2", NetKind::Wire) });
    doc.model.nets.push(Net { name: "hidden".into(), label: NetLabel::Off, ..Net::new("n3", NetKind::Wire) });
    let page = &mut doc.pages[0];
    page.connectors.push(wire("short", free(0.0, 0.0), free(50.0, 0.0), Some("n1")));
    page.connectors.push(wire("long", free(100.0, 0.0), free(100.0, 300.0), Some("n1")));
    page.connectors.push(wire("e1", free(0.0, 400.0), free(100.0, 400.0), Some("n2")));
    page.connectors.push(wire("e2", free(0.0, 500.0), free(100.0, 500.0), Some("n2")));
    page.connectors.push(wire("off", free(0.0, 600.0), free(100.0, 600.0), Some("n3")));
    let spots = label_spots(&doc, &doc.pages[0]);
    let on: Vec<(&str, &str)> = spots.iter().map(|s| (s.connector.as_str(), s.text.as_str())).collect();
    assert_eq!(on, [("long", "2003"), ("e1", "L1"), ("e2", "L1")]);
    let vertical = &spots[0];
    assert_eq!(vertical.angle, -90.0, "a vertical run reads from the bottom");
    assert!(vertical.pos.x < 100.0 && (vertical.pos.y - 150.0).abs() < 1e-9, "beside the line: {:?}", vertical.pos);
    assert_eq!(spots[1].angle, 0.0);
    assert!(spots[1].pos.y < 400.0, "above a horizontal run");
    assert!(label_spots(&Document::new(), &doc.pages[0]).is_empty(), "no nets in the model, no labels");
}

#[test]
fn pins_and_junction_lookups_ignore_dynamic_and_missing_glue() {
    let mut page = Page::new("p", "P");
    page.shapes.push(device("r1", 0.0, 0.0));
    page.shapes.push(junction("j1", Point::new(5.0, 5.0), true, 1.5, Color::BLACK));
    assert_eq!(pin_of(&page, &End::point("r1", 1)), Some(("r1".to_string(), 1)));
    assert_eq!(pin_of(&page, &End::dynamic("r1")), None);
    assert_eq!(pin_of(&page, &End::point("ghost", 1)), None);
    assert_eq!(pin_of(&page, &End::point("j1", 0)), None, "a junction is not a pin");
    assert!(junction_of(&page, &End::dynamic("j1")).is_some());
    let j = page.shape("j1").unwrap();
    assert_eq!((j.w, j.style.fill), (4.5, Color::BLACK));
    assert_eq!(ends_on(&page, "r1"), Vec::<(String, bool)>::new());
    let mut junk = junction("j2", Point::new(f64::NAN, f64::INFINITY), true, f64::NAN, Color::BLACK);
    junk.sanitize();
    assert!(junk.x.is_finite() && junk.w > 0.0);
}
