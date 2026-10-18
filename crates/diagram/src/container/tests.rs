use super::*;
use crate::geom::MAX_COORD;

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape::new(id, ShapeKind::Rectangle, x, y, w, h)
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn frame(p: &Page, id: &str) -> (f64, f64, f64, f64) {
    let s = p.shape(id).unwrap();
    (s.x, s.y, s.w, s.h)
}

fn z(p: &Page) -> String {
    p.shapes.iter().map(|s| s.id.clone()).collect::<Vec<_>>().join(",")
}

/// A container `k` at 0,0 400x300 and shapes `a` inside it, `b` outside.
fn page() -> Page {
    let mut p = Page::new("p1", "Page 1");
    p.shapes.push(rect("a", 50.0, 80.0, 40.0, 40.0));
    p.shapes.push(rect("b", 600.0, 80.0, 40.0, 40.0));
    p.shapes.push(new_container("k", 0.0, 0.0, 400.0, 300.0, "Box"));
    p
}

#[test]
fn members_draw_above_and_move_with_the_container_which_keeps_its_frame() {
    let mut p = page();
    assert_eq!(add_members(&mut p, "k", &ids(&["a"])).unwrap(), ids(&["a"]));
    assert_eq!(z(&p), "b,k,a", "a member draws over its container");
    arrange::translate(&mut p, &ids(&["k"]), 10.0, 20.0);
    assert_eq!(frame(&p, "a"), (60.0, 100.0, 40.0, 40.0));
    assert_eq!(frame(&p, "k"), (10.0, 20.0, 400.0, 300.0), "no refit unless asked");
    // A member moving alone leaves the frame as it is.
    arrange::translate(&mut p, &ids(&["a"]), 5.0, 0.0);
    assert_eq!(frame(&p, "k"), (10.0, 20.0, 400.0, 300.0));
    // Resizing the container does not scale its members.
    arrange::set_frame(&mut p, "k", 10.0, 20.0, 800.0, 300.0);
    assert_eq!(frame(&p, "a"), (65.0, 100.0, 40.0, 40.0));
    // A click on a member selects the member: the group chain stops at the container.
    assert!(arrange::group_chain(&p, "a").is_empty());
}

#[test]
fn fit_and_auto_fit_wrap_the_members_with_margins_and_heading() {
    let mut p = page();
    add_members(&mut p, "k", &ids(&["a"])).unwrap();
    assert!(fit(&mut p, "k"));
    // Margin 20, heading 28 on top.
    assert_eq!(frame(&p, "k"), (30.0, 32.0, 80.0, 108.0));
    p.shape_mut("k").unwrap().container.as_mut().unwrap().auto_fit = true;
    arrange::translate(&mut p, &ids(&["a"]), 100.0, 0.0);
    assert_eq!(frame(&p, "k"), (130.0, 32.0, 80.0, 108.0), "auto-fit follows the member");
    // Heading on the left: the band moves to the side and the text turns.
    p.shape_mut("k").unwrap().container.as_mut().unwrap().heading_position = HeadingPosition::Left;
    arrange::refit_groups(&mut p);
    assert_eq!(frame(&p, "k"), (102.0, 60.0, 108.0, 80.0));
    let tb = p.shape("k").unwrap().text_block.unwrap();
    assert_eq!((tb.w, tb.h, tb.angle_deg), (80.0, 28.0, 270.0));
}

#[test]
fn drop_in_adds_and_drag_fully_out_removes() {
    let mut p = page();
    // b dropped inside k joins it.
    let ch = drop_changes(&p, &ids(&["b"]), -400.0, 50.0);
    assert_eq!(ch, vec![("b".to_string(), Some("k".to_string()))]);
    apply_changes(&mut p, &ch);
    arrange::translate(&mut p, &ids(&["b"]), -400.0, 50.0);
    assert_eq!(p.shape("b").unwrap().parent.as_deref(), Some("k"));
    assert_eq!(z(&p), "a,k,b");
    // Partly out: still a member.
    assert!(drop_changes(&p, &ids(&["b"]), 190.0, 0.0).is_empty());
    // Fully out: leaves.
    let ch = drop_changes(&p, &ids(&["b"]), 400.0, 0.0);
    assert_eq!(ch, vec![("b".to_string(), None)]);
    // Moving the container itself never changes its own members.
    add_members(&mut p, "k", &ids(&["a"])).unwrap();
    assert!(drop_changes(&p, &ids(&["k"]), 1000.0, 0.0).is_empty());
}

#[test]
fn drops_pick_the_innermost_container_and_respect_locks() {
    let mut p = page();
    p.shapes.push(new_container("inner", 200.0, 100.0, 150.0, 150.0, "Inner"));
    add_members(&mut p, "k", &ids(&["inner"])).unwrap();
    // b into the inner container (inside k too).
    let ch = drop_changes(&p, &ids(&["b"]), -350.0, 70.0);
    assert_eq!(ch, vec![("b".to_string(), Some("inner".to_string()))]);
    apply_changes(&mut p, &ch);
    arrange::translate(&mut p, &ids(&["b"]), -350.0, 70.0);
    // Out of inner but still in k: moves up to k.
    let ch = drop_changes(&p, &ids(&["b"]), -200.0, 0.0);
    assert_eq!(ch, vec![("b".to_string(), Some("k".to_string()))]);
    // A locked container keeps its members and takes no new ones.
    p.shape_mut("inner").unwrap().container.as_mut().unwrap().locked = true;
    assert!(drop_changes(&p, &ids(&["b"]), 1000.0, 0.0).is_empty());
    assert_eq!(add_members(&mut p, "inner", &ids(&["a"])), Err(ContainerError::Locked));
    assert_eq!(remove_members(&mut p, "inner", &ids(&["b"])), Err(ContainerError::Locked));
    assert_eq!(disband(&mut p, "inner"), Err(ContainerError::Locked));
    assert!(drop_changes(&p, &ids(&["a"]), 160.0, 40.0).is_empty(), "dropping into a locked container changes nothing");
}

#[test]
fn wrap_remove_and_disband() {
    let mut p = Page::new("p1", "Page 1");
    p.shapes.push(rect("a", 0.0, 0.0, 10.0, 10.0));
    p.shapes.push(rect("b", 100.0, 50.0, 20.0, 20.0));
    let c = new_container("k", 0.0, 0.0, 1.0, 1.0, "K");
    assert_eq!(wrap(&mut p, c, &ids(&["a", "b"])).unwrap(), ids(&["a", "b"]));
    assert_eq!(z(&p), "k,a,b");
    assert_eq!(frame(&p, "k"), (-20.0, -48.0, 160.0, 138.0));
    assert_eq!(remove_members(&mut p, "k", &ids(&["b"])).unwrap(), ids(&["b"]));
    assert_eq!(p.shape("b").unwrap().parent, None);
    assert_eq!(remove_members(&mut p, "k", &ids(&["b"])), Err(ContainerError::NoShapes));
    assert_eq!(add_members(&mut p, "k", &ids(&["k"])), Err(ContainerError::NoShapes), "not into itself");
    assert_eq!(disband(&mut p, "k").unwrap(), ids(&["a"]));
    assert!(p.shape("k").is_none());
    assert_eq!(p.shape("a").unwrap().parent, None);
    assert_eq!(add_members(&mut p, "a", &ids(&["b"])), Err(ContainerError::NotAContainer));
}

/// A horizontal lane set `s` at 0,0 with lanes `l1`, `l2`, `l3` and a shape in each.
fn lanes_page() -> Page {
    let mut p = Page::new("p1", "Page 1");
    p.shapes.push(new_lane_set("s", 0.0, 0.0, false, "Process"));
    for (i, l) in ["l1", "l2", "l3"].iter().enumerate() {
        add_lane(&mut p, "s", new_lane(*l, &format!("Lane {}", i + 1), false), None).unwrap();
    }
    for (i, m) in ["m1", "m2", "m3"].iter().enumerate() {
        let y = SET_TITLE + LANE_THICKNESS * i as f64 + 50.0;
        p.shapes.push(rect(m, 100.0, y, 60.0, 40.0));
        add_members(&mut p, &format!("l{}", i + 1), &ids(&[m])).unwrap();
    }
    p
}

fn lane_of(p: &Page, m: &str) -> String {
    p.shape(m).unwrap().parent.clone().unwrap()
}

/// Whether member `m` sits inside lane `l`'s frame.
fn inside(p: &Page, m: &str, l: &str) -> bool {
    p.shape(l).unwrap().frame().contains_rect(&p.shape(m).unwrap().bounds())
}

#[test]
fn lanes_stack_and_keep_members_on_reorder_resize_add_and_remove() {
    let mut p = lanes_page();
    assert_eq!(lanes(&p, "s"), ids(&["l1", "l2", "l3"]));
    assert_eq!(frame(&p, "l2"), (0.0, SET_TITLE + LANE_THICKNESS, LANE_LENGTH, LANE_THICKNESS));
    assert_eq!(frame(&p, "s").3, SET_TITLE + 3.0 * LANE_THICKNESS);
    // Reorder: l3 first; every member stays in its lane.
    move_lane(&mut p, "l3", 0).unwrap();
    assert_eq!(lanes(&p, "s"), ids(&["l3", "l1", "l2"]));
    assert_eq!(frame(&p, "l3").1, SET_TITLE);
    for (m, l) in [("m1", "l1"), ("m2", "l2"), ("m3", "l3")] {
        assert_eq!(lane_of(&p, m), l);
        assert!(inside(&p, m, l), "{m} left {l}");
    }
    // Resize l3 taller: the lanes after it move down with their members.
    let l3 = frame(&p, "l3");
    arrange::set_frame(&mut p, "l3", l3.0, l3.1, l3.2, l3.3 + 60.0);
    assert_eq!(frame(&p, "l1").1, SET_TITLE + LANE_THICKNESS + 60.0);
    assert!(inside(&p, "m1", "l1") && inside(&p, "m2", "l2"));
    // Lengthening one lane lengthens the set and every lane.
    let y1 = frame(&p, "l1").1;
    arrange::set_frame(&mut p, "l1", 0.0, y1, 900.0, LANE_THICKNESS);
    assert_eq!(frame(&p, "s").2, 900.0);
    assert_eq!(frame(&p, "l2").2, 900.0);
    // The set grown thicker gives the room to its last lane.
    let s = frame(&p, "s");
    arrange::set_frame(&mut p, "s", s.0, s.1, s.2, s.3 + 30.0);
    assert_eq!(frame(&p, "l2").3, LANE_THICKNESS + 30.0);
    // Insert a lane at 1, then remove l3 (with its member): the rest close up.
    add_lane(&mut p, "s", new_lane("l4", "New", false), Some(1)).unwrap();
    assert_eq!(lanes(&p, "s"), ids(&["l3", "l4", "l1", "l2"]));
    assert!(inside(&p, "m1", "l1"));
    let gone = arrange::with_descendants(&p, &ids(&["l3"]));
    for id in &gone {
        p.remove_shape(id);
    }
    arrange::refit_groups(&mut p);
    assert_eq!(frame(&p, "l4").1, SET_TITLE);
    assert!(inside(&p, "m1", "l1") && inside(&p, "m2", "l2"));
    // Moving the whole set carries lanes and members.
    arrange::translate(&mut p, &ids(&["s"]), 100.0, 100.0);
    assert_eq!(frame(&p, "l4").1, SET_TITLE + 100.0);
    assert!(inside(&p, "m1", "l1"));
}

#[test]
fn shapes_drop_into_lanes_and_lanes_stay_put() {
    let mut p = lanes_page();
    p.shapes.push(rect("x", 1000.0, 0.0, 30.0, 30.0));
    let y = frame(&p, "l2").1 + 20.0;
    let ch = drop_changes(&p, &ids(&["x"]), 200.0 - 1000.0, y);
    assert_eq!(ch, vec![("x".to_string(), Some("l2".to_string()))]);
    // A member dragged from lane 1 into lane 3 changes lane.
    let ch = drop_changes(&p, &ids(&["m1"]), 0.0, 2.0 * LANE_THICKNESS);
    assert_eq!(ch, vec![("m1".to_string(), Some("l3".to_string()))]);
    // Lanes are never dropped anywhere.
    assert!(drop_changes(&p, &ids(&["l1"]), 0.0, 500.0).is_empty());
}

#[test]
fn phases_and_flip() {
    let mut p = lanes_page();
    add_phase(&mut p, "s", 400.0, "Review").unwrap();
    // The phase band pushes the lanes down.
    assert_eq!(frame(&p, "l1").1, SET_TITLE + PHASE_BAND);
    assert!(inside(&p, "m1", "l1"));
    let d = decor(p.shape("s").unwrap());
    assert!(d.iter().any(|x| matches!(x, Decor::Text { text, .. } if text == "Review")));
    assert!(d.iter().any(|x| matches!(x, Decor::Line { dashed: true, .. })));
    let before: Vec<Point> = ["m1", "m2", "m3"]
        .iter()
        .map(|m| p.shape(m).unwrap().center() - Point::new(frame(&p, &lane_of(&p, m)).0, frame(&p, &lane_of(&p, m)).1))
        .collect();
    assert!(flip(&mut p, "s").unwrap());
    // Vertical: lanes side by side, members mirrored inside their lanes.
    assert_eq!(frame(&p, "l1").0, PHASE_BAND);
    assert_eq!(frame(&p, "l2").0, PHASE_BAND + LANE_THICKNESS);
    assert_eq!(p.shape("l1").unwrap().container.as_ref().unwrap().heading_position, HeadingPosition::Top);
    for (i, m) in ["m1", "m2", "m3"].iter().enumerate() {
        let l = lane_of(&p, m);
        assert!(inside(&p, m, &l), "{m} left {l}");
        let off = p.shape(m).unwrap().center() - Point::new(frame(&p, &l).0, frame(&p, &l).1);
        assert!((off.x - before[i].y).abs() < 1e-9 && (off.y - before[i].x).abs() < 1e-9);
    }
    assert!(!flip(&mut p, "s").unwrap());
    assert!(inside(&p, "m2", "l2"));
    assert_eq!(flip(&mut p, "l1"), Err(ContainerError::NotALaneSet));
}

#[test]
fn callout_leader_follows_either_shape() {
    let mut p = Page::new("p1", "Page 1");
    p.shapes.push(rect("t", 0.0, 0.0, 100.0, 50.0));
    p.shapes.push(new_callout("c", "t", "Note", 300.0, 0.0));
    let (a, b) = leader(&p, p.shape("c").unwrap()).unwrap();
    assert_eq!(b, Point::new(100.0, 25.0), "the target's side facing the callout");
    assert_eq!(a.x, 300.0);
    arrange::translate(&mut p, &ids(&["t"]), 0.0, 200.0);
    let (_, b) = leader(&p, p.shape("c").unwrap()).unwrap();
    assert_eq!(b, Point::new(50.0, 200.0), "now the top faces the callout");
    arrange::translate(&mut p, &ids(&["c"]), -300.0, 300.0);
    let (_, b) = leader(&p, p.shape("c").unwrap()).unwrap();
    assert_eq!(b, Point::new(50.0, 250.0));
    p.shape_mut("c").unwrap().container.as_mut().unwrap().side = Side::Left;
    assert_eq!(leader(&p, p.shape("c").unwrap()).unwrap().1, Point::new(0.0, 225.0));
    p.shape_mut("c").unwrap().container.as_mut().unwrap().leader = false;
    assert!(leader(&p, p.shape("c").unwrap()).is_none());
    assert!(page_leaders(&p).is_empty());
    p.shape_mut("c").unwrap().container.as_mut().unwrap().leader = true;
    assert_eq!(page_leaders(&p).len(), 2, "line and dot");
    p.remove_shape("t");
    assert!(leader(&p, p.shape("c").unwrap()).is_none(), "no target, no leader");
}

#[test]
fn presets_restyle_and_settings_round_trip() {
    let mut s = new_container("k", 0.0, 0.0, 100.0, 100.0, "K");
    apply_preset(&mut s, container_style("slate").unwrap());
    assert_eq!(s.style.font_color, Color::WHITE);
    let mut c = new_callout("c", "t", "x", 0.0, 0.0);
    apply_preset(&mut c, callout_style("bubble").unwrap());
    assert_eq!(c.kind, ShapeKind::Ellipse);
    let mut set = new_lane_set("s", 0.0, 0.0, true, "S");
    set.container.as_mut().unwrap().phases.push(Phase { at: 300.0, title: "One".into() });
    for shape in [s, c, set] {
        let json = serde_json::to_string(&shape).unwrap();
        let back: Shape = serde_json::from_str(&json).unwrap();
        assert_eq!(back, shape);
    }
    // Hostile settings load as defaults and are clamped.
    let back: Shape =
        serde_json::from_str(r#"{"id":"x","container":{"role":"moon","margin":"wide","headingSize":1e300,"leader":7,"phases":[{"at":-5},3]}}"#)
            .unwrap();
    let mut back = back;
    back.sanitize();
    let c = back.container.unwrap();
    assert_eq!(c.role, Role::Container);
    assert_eq!(c.margin, 0.0);
    assert_eq!(c.heading_size, Container::MAX_BAND);
    assert!(c.leader);
    assert_eq!(c.phases, vec![Phase { at: 0.0, title: String::new() }]);
}

#[test]
fn hostile_numbers_stay_in_range() {
    let mut p = lanes_page();
    arrange::translate(&mut p, &ids(&["s"]), f64::MAX, f64::NAN);
    arrange::set_frame(&mut p, "l1", f64::INFINITY, 0.0, 1e300, -5.0);
    add_phase(&mut p, "s", f64::NAN, "x").unwrap();
    flip(&mut p, "s").unwrap();
    let _ = drop_changes(&p, &ids(&["m1"]), f64::NAN, f64::INFINITY);
    for s in &p.shapes {
        assert!([s.x, s.y, s.w, s.h].iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD), "{s:?}");
    }
}
