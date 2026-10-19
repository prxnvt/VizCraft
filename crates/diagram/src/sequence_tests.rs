use super::*;
use crate::container;

fn lifeline(id: &str, x: f64, actor: bool) -> Shape {
    let kind = if actor { ShapeKind::ActorLifeline } else { ShapeKind::Lifeline };
    let mut s = Shape::new(id, kind, x, 40.0, 100.0, 300.0);
    s.text = id.to_uppercase();
    s.sequence = Some(Part::Lifeline { destroyed: false });
    s
}

fn message(id: &str, from: &str, to: &str, sort: Sort, text: &str) -> Connector {
    let mut c = Connector::new(id, End::dynamic(from), End::dynamic(to));
    c.text = text.into();
    c.message = Some(Message { sort });
    c
}

fn fragment(id: &str, operator: Operator, guards: &[&str]) -> Shape {
    let mut s = Shape::new(id, ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    s.sequence =
        Some(Part::Fragment { operator, operands: guards.iter().map(|g| Operand { guard: g.to_string(), at: 0.0 }).collect(), covers: Vec::new() });
    fragment_look(&mut s);
    s
}

/// Client, api and db; a call, a nested call and its reply, then an alt with two replies.
fn sample() -> Document {
    let mut doc = Document::new();
    let page = doc.page_mut().unwrap();
    page.shapes = vec![
        lifeline("client", 40.0, true),
        lifeline("api", 200.0, false),
        lifeline("db", 400.0, false),
        fragment("f1", Operator::Alt, &["found", "else"]),
    ];
    page.connectors = vec![
        message("m1", "client", "api", Sort::Sync, "GET /orders/42"),
        message("m2", "api", "db", Sort::Sync, "query"),
        message("m3", "db", "api", Sort::Reply, "rows"),
        message("m4", "api", "client", Sort::Reply, "200 OK"),
        message("m5", "api", "client", Sort::Reply, "404 Not Found"),
    ];
    let ev = |k: &str, id: &str| -> Event { serde_json::from_value(serde_json::json!({ k: id })).unwrap() };
    page.sequence = Some(Interaction {
        events: vec![
            ev("message", "m1"),
            ev("message", "m2"),
            ev("message", "m3"),
            ev("open", "f1"),
            ev("message", "m4"),
            ev("else", "f1"),
            ev("message", "m5"),
            ev("close", "f1"),
        ],
        ..Interaction::default()
    });
    settle(&mut doc);
    doc
}

fn settle(doc: &mut Document) {
    let page = doc.page_mut().unwrap();
    settle_page(page, true).unwrap();
    number_pending(doc);
}

fn y_of(page: &Page, c: &Connector) -> (f64, f64) {
    let g = crate::route::connector_geometry(page, c);
    let pts = g.path.flatten(0.5);
    let first = pts.first().and_then(|p| p.points.first()).copied().unwrap();
    let last = pts.last().and_then(|p| p.points.last()).copied().unwrap();
    (first.y, last.y)
}

#[test]
fn serde_round_trip_and_old_files_and_unknown_events_load() {
    let doc = sample();
    let page = doc.page().unwrap();
    let text = serde_json::to_string(page).unwrap();
    let back: Page = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, page);
    let v = serde_json::to_value(page).unwrap();
    assert_eq!(v["sequence"]["events"][0], serde_json::json!({"message": "m1"}));
    assert_eq!(v["shapes"][0]["sequence"]["part"], "fragment");
    assert_eq!(v["connectors"][0]["message"]["sort"], "sync");
    // A page from before sequences, and one with junk and unknown events.
    let old: Page = serde_json::from_str(r#"{"id": "p1", "shapes": [{"id": "s1"}], "connectors": [{"id": "c1"}]}"#).unwrap();
    assert!(old.sequence.is_none() && old.shapes[0].sequence.is_none() && old.connectors[0].message.is_none());
    let junk: Page = serde_json::from_str(
        r#"{"id": "p1", "sequence": {"events": [{"message": "c1"}, {"teleport": "x"}, 7, {"open": 3}], "activations": "sideways", "row": "tall"},
            "shapes": [{"id": "s1", "sequence": {"part": "wormhole"}}, {"id": "s2", "sequence": {"part": "fragment", "operator": "zz", "operands": [1, {"guard": "x"}]}}],
            "connectors": [{"id": "c1", "message": {"sort": 9}}]}"#,
    )
    .unwrap();
    let seq = junk.sequence.as_ref().unwrap();
    assert_eq!(seq.events, vec![Event::Message("c1".into())]);
    assert_eq!(seq.activations, Activations::Auto);
    assert!(junk.shapes[0].sequence.is_none());
    assert!(matches!(&junk.shapes[1].sequence, Some(Part::Fragment { operator: Operator::Alt, operands, .. }) if operands.len() == 1));
    assert_eq!(junk.connectors[0].message, Some(Message { sort: Sort::Sync }));
    let mut p = junk;
    p.sanitize();
    assert_eq!(p.sequence.as_ref().unwrap().row, 36.0);
}

#[test]
fn settle_is_idempotent_and_messages_are_horizontal_on_their_rows() {
    let mut doc = sample();
    let once = doc.clone();
    settle(&mut doc);
    assert_eq!(doc, once, "a second settle changes nothing");
    let page = doc.page().unwrap();
    let rows = rows(page);
    assert_eq!(rows.len(), 8);
    for c in &page.connectors {
        let (y0, y1) = y_of(page, c);
        assert!((y0 - y1).abs() < 1e-6, "{} is horizontal", c.id);
        let row = rows.iter().find(|(id, _)| id == &c.id).unwrap().1;
        assert!((y0 - row).abs() < 1e-6, "{} sits on its row", c.id);
        assert!(matches!(c.start, End::Glued { point: Some(p), .. } if p >= 4));
        assert_eq!(c.route, Route::Straight);
    }
    for w in rows.windows(2) {
        assert!(w[1].1 > w[0].1, "{rows:?}");
    }
    // Bars: api has one (opened by m1, answered by m4 and m5 on both alt paths), db one.
    let bars: Vec<&Shape> = page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Activation { .. }))).collect();
    assert_eq!(bars.len(), 2, "{bars:?}");
    assert!(bars.iter().all(|b| b.locks.position && b.locks.delete && is_plain_id(&b.id)));
    // Z order: fragment, lifelines, bars.
    let ranks: Vec<u8> = page
        .shapes
        .iter()
        .map(|s| match s.sequence {
            Some(Part::Fragment { .. }) => 1,
            Some(Part::Lifeline { .. }) => 2,
            _ => 3,
        })
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
    // The fragment spans its rows, and its second operand starts at the separator.
    let f = page.shape("f1").unwrap();
    let Some(Part::Fragment { operands, .. }) = &f.sequence else { panic!() };
    let m5 = rows.iter().find(|(id, _)| id == "m5").unwrap().1;
    assert!(f.y + operands[1].at < m5 && f.y + f.h > m5);
    // Lifeline head text sits in the head.
    let api = page.shape("api").unwrap();
    let (r, _) = api.text_rect();
    assert!(r.y0.abs() < 1e-6 && (r.height() - HEAD).abs() < 1e-6);
}

fn is_plain_id(id: &str) -> bool {
    crate::is_plain_id(id)
}

#[test]
fn glue_indices_follow_a_move() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    let before = page.connector("m2").unwrap().start.clone();
    let seq = page.sequence.as_mut().unwrap();
    move_block(&mut seq.events, "m2", &Position::After("m3".into())).unwrap();
    settle(&mut doc);
    let page = doc.page().unwrap();
    let rows = rows(page);
    let idx: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(&idx[..3], &["m1", "m3", "m2"]);
    let after = page.connector("m2").unwrap().start.clone();
    assert_ne!(before, after, "m2 is glued to a later point now");
    for c in &page.connectors {
        let (y0, _) = y_of(page, c);
        let row = rows.iter().find(|(id, _)| id == &c.id).unwrap().1;
        assert!((y0 - row).abs() < 1e-6, "{}", c.id);
    }
    // A fragment cannot move into itself.
    let page = doc.page_mut().unwrap();
    let seq = page.sequence.as_mut().unwrap();
    assert!(move_block(&mut seq.events, "f1", &Position::Before("m4".into())).is_err());
    assert!(move_block(&mut seq.events, "f1", &Position::In { fragment: "f1".into(), operand: 0 }).is_err());
    move_block(&mut seq.events, "f1", &Position::Before("m1".into())).unwrap();
    assert_eq!(seq.events.first(), Some(&Event::Open("f1".into())));
}

#[test]
fn deleting_items_prunes_their_events() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.remove_connector("m3");
    settle(&mut doc);
    let page = doc.page().unwrap();
    assert!(!page.sequence.as_ref().unwrap().events.contains(&Event::Message("m3".into())));
    // A deleted lifeline leaves its messages with free ends; their events go.
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.detach_shape("db");
    page.remove_shape("db");
    settle(&mut doc);
    let page = doc.page().unwrap();
    let events = &page.sequence.as_ref().unwrap().events;
    assert!(!events.contains(&Event::Message("m2".into())) && events.contains(&Event::Message("m1".into())));
    assert!(page.connector("m2").unwrap().end.shape().is_none(), "the end is free");
    assert!(page.shapes.iter().all(|s| !matches!(&s.sequence, Some(Part::Activation { lifeline, .. }) if lifeline == "db")));
    // A fragment whose close went missing is dropped and loses its part.
    let mut doc = sample();
    let seq = doc.page_mut().unwrap().sequence.as_mut().unwrap();
    seq.events.retain(|e| !matches!(e, Event::Close(_)));
    settle(&mut doc);
    let page = doc.page().unwrap();
    assert!(page.shape("f1").unwrap().sequence.is_none());
    assert!(page.sequence.as_ref().unwrap().events.iter().all(|e| e.id() != "f1"));
}

#[test]
fn kind_outlines_and_dashes_survive_junk_sizes() {
    for kind in [ShapeKind::Lifeline, ShapeKind::ActorLifeline] {
        for (w, h) in [(0.0, 0.0), (f64::NAN, 5.0), (f64::INFINITY, -3.0), (1e300, 1e300), (100.0, 300.0)] {
            let o = kind.outline(w, h);
            let _ = o.bounds();
            let _ = o.flatten(0.5);
            let d = kind.detail(w, h).unwrap();
            let dashes = d.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
            assert!(dashes <= MAX_DASHES + 4, "{kind:?} {w}x{h}: {dashes}");
        }
        assert_eq!(kind.connection_points().len(), 4);
        let b = kind.outline(100.0, 300.0).bounds().unwrap();
        assert!((b.y1 - 300.0).abs() < 1e-9, "the bounds reach the stem's foot");
        assert_eq!(ShapeKind::from_name(kind.name()), Some(kind));
    }
    let d = ShapeKind::Lifeline.detail(100.0, 140.0).unwrap();
    assert_eq!(d.els.iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 10, "6 on, 4 off over 100");
    assert!(!ShapeKind::ALL.contains(&ShapeKind::Lifeline));
    let mut s = lifeline("a", 0.0, false);
    (s.w, s.h) = (100.0, 300.0);
    assert!(stem_hit(&s, Point::new(52.0, 200.0), 4.0) && stem_hit(&s, Point::new(10.0, 50.0), 4.0) && !stem_hit(&s, Point::new(10.0, 200.0), 4.0));
}

#[test]
fn alt_decor_has_the_tab_one_dashed_separator_and_two_guards() {
    let doc = sample();
    let f = doc.page().unwrap().shape("f1").unwrap();
    let decor = container::decor(f);
    let tabs = decor.iter().filter(|d| matches!(d, Decor::Line { points, dashed: false, .. } if points.len() == 6)).count();
    let seps = decor.iter().filter(|d| matches!(d, Decor::Line { dashed: true, .. })).count();
    let guards: Vec<&str> = decor.iter().filter_map(|d| if let Decor::Text { text, .. } = d { Some(text.as_str()) } else { None }).collect();
    assert_eq!((tabs, seps), (1, 1));
    assert_eq!(guards, ["alt", "[found]", "[else]"]);
    assert!(fragment_decor(&Shape::default()).is_empty());
}

#[test]
fn wrap_refuses_non_contiguous_ids_and_operands_edit() {
    let doc = sample();
    let mut events = doc.page().unwrap().sequence.clone().unwrap().events;
    let err = wrap(&mut events, &["m1".into(), "m3".into()], "f2").unwrap_err();
    assert!(err.contains("not contiguous") && err.contains("m2"), "{err}");
    assert!(wrap(&mut events, &["m3".into(), "m4".into()], "f2").unwrap_err().contains("nesting level"));
    assert!(wrap(&mut events, &["ghost".into()], "f2").is_err());
    let at = wrap(&mut events, &["m2".into(), "m1".into()], "f2").unwrap();
    assert_eq!(at, 0);
    assert_eq!(events[0], Event::Open("f2".into()));
    assert_eq!(events[3], Event::Close("f2".into()));
    assert_eq!(add_operand(&mut events, "f2", "m2").unwrap(), 1);
    assert!(add_operand(&mut events, "f2", "m2").is_err(), "already starts an operand");
    assert!(add_operand(&mut events, "f2", "m4").is_err(), "not inside");
    assert_eq!(remove_operand(&mut events, "f2", 1).unwrap(), 0);
    assert!(remove_operand(&mut events, "f2", 1).is_err());
    assert!(unwrap(&mut events, "f2"));
    assert_eq!(events, doc.page().unwrap().sequence.clone().unwrap().events);
    assert_eq!(index_for(&events, &Position::In { fragment: "f1".into(), operand: 1 }).unwrap(), 7);
}

#[test]
fn checks_find_order_and_reply_problems() {
    let doc = sample();
    let page = doc.page().unwrap();
    assert_eq!(check_order(page), vec![]);
    assert_eq!(check_replies(page, true), vec![]);
    // A reply before its call, and a message after a destroy.
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    let seq = page.sequence.as_mut().unwrap();
    move_block(&mut seq.events, "m3", &Position::Before("m2".into())).unwrap();
    let f = check_replies(page, false);
    assert!(f.iter().any(|x| x.items == ["m3"] && x.what.contains("no open call")), "{f:?}");
    assert!(check_replies(page, true).iter().any(|x| x.what.contains("never answered")));
    let o = check_order(page);
    assert!(o.iter().any(|x| x.what.contains("drawn above")), "the drawing has the old order: {o:?}");
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.connector_mut("m2").unwrap().message = Some(Message { sort: Sort::Destroy });
    let o = check_order(page);
    assert!(o.iter().any(|x| x.what.contains("destroyed") && x.items.contains(&"m3".to_string())), "{o:?}");
    // An else in an opt, and an unbalanced fragment.
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    if let Some(Part::Fragment { operator, .. }) = &mut page.shape_mut("f1").unwrap().sequence {
        *operator = Operator::Opt;
    }
    assert!(check_order(page).iter().any(|x| x.what.contains("one operand")));
    page.sequence.as_mut().unwrap().events.pop();
    assert!(check_order(page).iter().any(|x| x.what.contains("never closes")));
    // A call answered twice on one path.
    let mut doc = sample();
    let seq = doc.page_mut().unwrap().sequence.as_mut().unwrap();
    seq.events.retain(|e| !matches!(e, Event::Open(_) | Event::Else(_) | Event::Close(_)));
    let page = doc.page().unwrap();
    assert!(check_replies(page, false).iter().any(|x| x.what.contains("answered twice")));
}

#[test]
fn relayout_packs_from_the_margin_and_rename_follows_copies() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.shape_mut("db").unwrap().x = 5000.0;
    let out = relayout(page).unwrap();
    assert_eq!(out.ids, ["client", "api", "db"]);
    assert!(page.shape("db").unwrap().x < 1000.0);
    number_pending(&mut doc);
    let page = doc.page().unwrap();
    assert!(page.shapes.iter().all(|s| !s.id.starts_with(PENDING)));
    let mut copy = page.clone();
    let shapes: HashMap<String, String> = [("api".to_string(), "api2".to_string()), ("f1".into(), "f9".into())].into();
    let conns: HashMap<String, String> = [("m1".to_string(), "m1b".to_string())].into();
    rename_ids(&mut copy, &shapes, &conns);
    let ev = &copy.sequence.as_ref().unwrap().events;
    assert_eq!(ev[0], Event::Message("m1b".into()));
    assert!(ev.contains(&Event::Open("f9".into())));
    assert!(copy.shapes.iter().any(|s| matches!(&s.sequence, Some(Part::Activation { lifeline, .. }) if lifeline == "api2")));
}

#[test]
fn a_message_label_sits_above_its_line() {
    let doc = sample();
    let page = doc.page().unwrap();
    for id in ["m1", "m2", "m3", "m4", "m5"] {
        let c = page.connector(id).unwrap();
        let g = crate::route::connector_geometry(page, c);
        let (y0, y1) = y_of(page, c);
        assert!((y0 - y1).abs() < 1e-9);
        let r = label_rect(c, g.label_pos).unwrap();
        assert!((r.y1 - (y0 - LABEL_GAP)).abs() < 1e-9, "{id}: the box ends {LABEL_GAP} above the line");
        let (a, b) = (g.polyline.first().unwrap().x, g.polyline.last().unwrap().x);
        assert!((r.center().x - (a + b) / 2.0).abs() < 1e-9, "{id}: centred over the line");
    }
    // A plain connector keeps its label on the line.
    let mut plain = page.connector("m1").unwrap().clone();
    plain.message = None;
    let g = crate::route::connector_geometry(page, &plain);
    assert!((g.label_pos.y - y_of(page, &plain).0).abs() < 1e-9);
}

#[test]
fn a_self_message_label_sits_right_of_its_loop() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.connectors.push(message("me", "api", "api", Sort::Async, "tick"));
    settle(&mut doc);
    let page = doc.page().unwrap();
    let c = page.connector("me").unwrap();
    let g = crate::route::connector_geometry(page, c);
    assert_eq!(g.polyline.len(), 4, "{:?}", g.polyline);
    let loop_x = g.polyline.iter().map(|p| p.x).fold(f64::MIN, f64::max);
    let r = label_rect(c, g.label_pos).unwrap();
    assert!((r.x0 - (loop_x + 4.0)).abs() < 1e-9);
    let mid = (g.polyline[0].y + g.polyline[3].y) / 2.0;
    assert!((r.center().y - mid).abs() < 1e-9);
    // Lost and found labels sit above their stub.
    let lost = label_anchor(&[Point::new(0.0, 100.0), Point::new(60.0, 100.0)], "x", &crate::Style::default()).unwrap();
    assert!(lost.y < 100.0 && (lost.x - 30.0).abs() < 1e-9);
    assert_eq!(label_anchor(&[], "x", &crate::Style::default()), None);
}

#[test]
fn message_kind_connectors_and_lifeline_shapes_are_adopted() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    // A lifeline-kind shape with no part, as `shape.add` of the master leaves it.
    let mut cache = Shape::new("cache", ShapeKind::Lifeline, 600.0, 40.0, 100.0, 300.0);
    cache.text = "Cache".into();
    page.shapes.push(cache);
    let mut adopted = Connector::new("c9", End::dynamic("api"), End::dynamic("cache"));
    adopted.kind = "uml.asyncMessage".into();
    let mut plain = Connector::new("c10", End::dynamic("api"), End::dynamic("cache"));
    plain.kind = "basic".into();
    let mut half = Connector::new("c11", End::dynamic("api"), End::free(Point::new(900.0, 900.0)));
    half.kind = "uml.syncMessage".into();
    page.connectors.extend([adopted, plain, half]);
    settle(&mut doc);
    let page = doc.page().unwrap();
    assert!(matches!(page.shape("cache").unwrap().sequence, Some(Part::Lifeline { destroyed: false })));
    assert_eq!(page.connector("c9").unwrap().message, Some(Message { sort: Sort::Async }));
    assert_eq!(page.connector("c10").unwrap().message, None, "a plain connector stays drawing");
    assert_eq!(page.connector("c11").unwrap().message, None, "one end free");
    let events = &page.sequence.as_ref().unwrap().events;
    assert_eq!(events.last(), Some(&Event::Message("c9".into())));
    // Settle stays idempotent.
    let mut again = doc.clone();
    settle(&mut again);
    assert_eq!(again, doc);
    assert_eq!(Sort::from_connector_type("uml.replyMessage"), Some(Sort::Reply));
    assert_eq!(Sort::from_connector_type("uml.class"), None);
}

#[test]
fn drop_positions_steps_and_owners() {
    let doc = sample();
    let page = doc.page().unwrap();
    let events = page.sequence.as_ref().unwrap().events.clone();
    let rows: HashMap<String, f64> = rows(page).into_iter().collect();
    let y = |id: &str| rows[id];
    // Between m1 and m2: before m2.
    let (p, line) = drop_position(page, (y("m1") + y("m2")) / 2.0, None).unwrap();
    assert_eq!(p, Position::Before("m2".into()));
    assert!((line - (y("m1") + y("m2")) / 2.0).abs() < 1e-9);
    // Just above the separator: the end of operand 0; just above the bottom: the end of operand 1.
    let sep = page.shape("f1").map(|f| {
        f.y + match &f.sequence {
            Some(Part::Fragment { operands, .. }) => operands[1].at,
            _ => 0.0,
        }
    });
    let (p, _) = drop_position(page, sep.unwrap() - 1.0, Some("m1")).unwrap();
    assert_eq!(p, Position::In { fragment: "f1".into(), operand: 0 });
    let f = page.shape("f1").unwrap();
    let (p, _) = drop_position(page, f.y + f.h - 1.0, None).unwrap();
    assert_eq!(p, Position::In { fragment: "f1".into(), operand: 1 });
    // Below everything: after the last top-level block; the dragged block is skipped.
    assert_eq!(drop_position(page, 1e6, None).unwrap().0, Position::After("f1".into()));
    assert_eq!(drop_position(page, 1e6, Some("f1")).unwrap().0, Position::After("m3".into()));
    assert_eq!(drop_position(page, y("m2") - 1.0, Some("m2")).unwrap().0, Position::Before("m3".into()));
    // Steps: past a sibling, into the next operand, out at the edges, over a fragment whole.
    assert_eq!(step(&events, "m1", false), Err("`m1` is already first".into()));
    assert_eq!(step(&events, "m1", true), Ok(Position::After("m2".into())));
    assert_eq!(step(&events, "m3", true), Ok(Position::After("f1".into())));
    assert_eq!(step(&events, "m4", false), Ok(Position::Before("f1".into())));
    assert_eq!(step(&events, "m4", true), Ok(Position::Before("m5".into())));
    assert_eq!(step(&events, "m5", false), Ok(Position::In { fragment: "f1".into(), operand: 0 }));
    assert_eq!(step(&events, "m5", true), Ok(Position::After("f1".into())));
    assert_eq!(step(&events, "f1", false), Ok(Position::Before("m3".into())));
    assert!(step(&events, "ghost", true).is_err());
    assert_eq!(enclosing(&events, "m5"), Some(("f1".into(), 1)));
    assert_eq!(enclosing(&events, "m1"), None);
    // Lifelines are found by head, stem and bar; a bar's owner is its lifeline.
    let api = page.shape("api").unwrap();
    let stem = Point::new(api.x + api.w / 2.0 + 2.0, api.y + api.h - 10.0);
    assert_eq!(lifeline_at(page, stem, 3.0).as_deref(), Some("api"));
    assert_eq!(lifeline_at(page, Point::new(api.x - 40.0, stem.y), 3.0), None);
    let bar = page.shapes.iter().find(|s| matches!(&s.sequence, Some(Part::Activation { lifeline, .. }) if lifeline == "api")).unwrap();
    assert_eq!(lifeline_at(page, bar.center(), 0.0).as_deref(), Some("api"));
    assert_eq!(owner(page, &bar.id), "api");
    assert_eq!(owner(page, "m1"), "m1");
    assert_eq!(column_at(page, -1e6), 0);
    assert_eq!(column_at(page, api.x + api.w / 2.0 + 1.0), 2);
    assert_eq!(column_at(page, 1e6), 3);
    // The alt's separator dragged above 200 OK makes 200 OK start the second operand; dragged
    // below everything it starts at the last block.
    assert_eq!(separator_target(page, "f1", 1, y("m4") - 1.0).as_deref(), Some("m5"), "m4 cannot start operand 1: operand 0 keeps it");
    assert_eq!(separator_target(page, "f1", 1, 1e6).as_deref(), Some("m5"));
    assert_eq!(separator_target(page, "f1", 0, 0.0), None);
    assert_eq!(separator_target(page, "m1", 1, 0.0), None);
}

/// Text decor's box, estimated as the layout does.
fn text_rect(d: &container::Decor) -> Option<Rect> {
    let container::Decor::Text { center, text, size, .. } = d else { return None };
    let w = text.chars().count() as f64 * size * 0.6;
    Some(Rect::new(center.x - w / 2.0, center.y - size / 2.0, center.x + w / 2.0, center.y + size / 2.0))
}

fn crosses(r: &Rect, x: f64, y0: f64, y1: f64) -> bool {
    x > r.x0 && x < r.x1 && y1 > r.y0 && y0 < r.y1
}

#[test]
fn fragment_tabs_guards_and_labels_stay_clear_of_stems_and_bars_and_operands_are_evenly_spaced() {
    let doc = sample();
    let page = doc.page().unwrap();
    let f = page.shape("f1").unwrap();
    let mut boxes: Vec<(String, Rect)> = Vec::new();
    for d in fragment_decor(f) {
        match &d {
            container::Decor::Line { points, dashed: false, .. } => boxes.push(("tab".into(), Rect::bounding(points.iter().copied()).unwrap())),
            container::Decor::Text { text, .. } => boxes.push((text.clone(), text_rect(&d).unwrap())),
            _ => {}
        }
    }
    assert_eq!(boxes.len(), 4, "{boxes:?}");
    for c in &page.connectors {
        let g = crate::route::connector_geometry(page, c);
        boxes.push((c.id.clone(), label_rect(c, g.label_pos).unwrap()));
    }
    for s in &page.shapes {
        let (what, xs, y0, y1) = match &s.sequence {
            Some(Part::Lifeline { .. }) => ("stem", vec![s.x + s.w / 2.0], s.y + HEAD.min(s.h), s.y + s.h),
            Some(Part::Activation { .. }) => ("bar", vec![s.x, s.x + s.w / 2.0, s.x + s.w], s.y, s.y + s.h),
            _ => continue,
        };
        for (name, r) in &boxes {
            for &x in &xs {
                assert!(!crosses(r, x, y0, y1), "the {what} of {} crosses {name} {r:?}", s.id);
            }
        }
    }
    // Rows: one row below the top and the separator, half a row above the separator and bottom.
    let row = page.sequence.as_ref().unwrap().row;
    let rows: HashMap<String, f64> = rows(page).into_iter().collect();
    let Some(Part::Fragment { operands, .. }) = &f.sequence else { panic!() };
    let sep = f.y + operands[1].at;
    for (got, want) in [(rows["m4"] - f.y, row), (sep - rows["m4"], row / 2.0), (rows["m5"] - sep, row), (f.y + f.h - rows["m5"], row / 2.0)] {
        assert!((got - want).abs() < 1e-9, "{got} != {want}");
    }
    // The frame reaches half a column gap past its stems and clear of the bars.
    let gap = page.sequence.as_ref().unwrap().column_gap;
    let client = page.shape("client").unwrap();
    let api = page.shape("api").unwrap();
    assert!(client.x + client.w / 2.0 - f.x >= gap / 2.0 - 1e-9);
    assert!(f.x + f.w - (api.x + api.w / 2.0) >= gap / 2.0 - 1e-9);
}

#[test]
fn a_label_over_a_passed_lifeline_moves_clear_of_its_stem() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.shapes.push(lifeline("mid", 330.0, false));
    // "query" runs from api to db, over the new lifeline; a call from client to mid opens a bar
    // on it before.
    page.connectors.push(message("c9", "client", "mid", Sort::Sync, "a long label for a short hop"));
    let events = &mut page.sequence.as_mut().unwrap().events;
    events.insert(1, Event::Message("c9".into()));
    settle(&mut doc);
    let page = doc.page().unwrap();
    let mid = page.shape("mid").unwrap();
    let stem = mid.x + mid.w / 2.0;
    for id in ["m2", "m3", "c9"] {
        let c = page.connector(id).unwrap();
        let g = crate::route::connector_geometry(page, c);
        let r = label_rect(c, g.label_pos).unwrap();
        assert!(r.x1 <= stem - ACTIVATION_W || r.x0 >= stem + ACTIVATION_W || id == "c9", "{id}: {r:?} meets the stem at {stem}");
        for s in page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Activation { .. }))) {
            let overlaps = r.x0 < s.x + s.w && r.x1 > s.x && r.y0 < s.y + s.h && r.y1 > s.y;
            assert!(!overlaps, "{id}: {r:?} meets bar {:?}", (s.x, s.y, s.w, s.h));
        }
        assert!((r.y1 - (g.polyline[0].y - LABEL_GAP)).abs() < 1e-9, "{id} stays above its line");
    }
}
