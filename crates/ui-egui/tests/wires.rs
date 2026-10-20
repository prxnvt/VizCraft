//! The engineering canvas under egui_kittest: the Wire tool drawing between pins and branching
//! onto a wire at a junction, Escape back to the pointer, the bare-letter keys (and that they
//! never fire while text is being typed), the net highlight, and the preview palette section
//! placing a builder symbol. Assertions read the session and the canvas state, never pixels.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::{Point, net, route_polyline};
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::{CraftApp, Services, Tool};

fn harness() -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(Document::new()), Services::default());
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

/// The page point at the middle of the canvas.
fn centre(h: &Harness<'_, CraftApp>) -> Point {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).unwrap();
    let c = vp.screen_to_world(app.canvas.rect.unwrap().center());
    Point::new(f64::from(c.x).round(), f64::from(c.y).round())
}

fn run(h: &mut Harness<'_, CraftApp>, id: &str, v: serde_json::Value) -> serde_json::Value {
    let r = h.state_mut().run(id, v).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(2);
    r
}

fn pin(h: &Harness<'_, CraftApp>, shape: &str, i: usize) -> Point {
    h.state().session.doc.page().unwrap().shape(shape).unwrap().connection_point_world(i).unwrap()
}

fn drag(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key) {
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

/// A contact and a coil placed side by side in view; returns their ids.
fn contact_and_coil(h: &mut Harness<'_, CraftApp>) -> (String, String) {
    let c = centre(h);
    let a = run(h, "glyph.insert", json!({"builder": "contact", "x": c.x - 200.0, "y": c.y - 80.0}));
    let b = run(h, "glyph.insert", json!({"builder": "coil", "x": c.x + 120.0, "y": c.y - 80.0}));
    (a["id"].as_str().unwrap().to_string(), b["id"].as_str().unwrap().to_string())
}

fn wires(h: &Harness<'_, CraftApp>) -> Vec<vizcraft_diagram::Connector> {
    h.state().session.doc.page().unwrap().connectors.iter().filter(|c| c.net.is_some()).cloned().collect()
}

#[test]
fn the_wire_tool_joins_two_pins_and_a_branch_makes_a_junction() {
    let mut h = harness();
    let (a, b) = contact_and_coil(&mut h);
    run(&mut h, "ui.wire", json!({}));
    assert_eq!(h.state().tool, Tool::Wire { pipe: false });
    let (from, to) = (screen(&h, pin(&h, &a, 1)), screen(&h, pin(&h, &b, 1)));
    drag(&mut h, from, to);
    let w = wires(&h);
    assert_eq!(w.len(), 1, "one wire drawn");
    assert_eq!(serde_json::to_value(&w[0].start).unwrap(), json!({"shape": a, "point": 1}));
    assert_eq!(serde_json::to_value(&w[0].end).unwrap(), json!({"shape": b, "point": 1}));
    assert_eq!(h.state().tool, Tool::Wire { pipe: false }, "the tool stays armed for the next wire");
    // A branch from empty page onto the middle of the wire's longest run splits it at a junction.
    let poly = route_polyline(h.state().session.doc.page().unwrap(), &w[0]);
    let (p, q) = poly.windows(2).map(|s| (s[0], s[1])).max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).unwrap();
    let mid = p.midpoint(q);
    let start = screen(&h, Point::new(mid.x, mid.y + 120.0));
    let end = screen(&h, mid);
    drag(&mut h, start, end);
    let page = h.state().session.doc.page().unwrap().clone();
    assert_eq!(page.shapes.iter().filter(|s| net::is_junction(s)).count(), 1, "a junction where the branch meets the wire");
    let w = wires(&h);
    assert_eq!(w.len(), 3, "the wire's two halves and the branch");
    assert!(w.iter().all(|c| c.net == w[0].net), "one net");
    // Escape returns to the pointer.
    key(&mut h, Key::Escape);
    assert_eq!(h.state().tool, Tool::Pointer);
}

#[test]
fn bare_letters_rotate_mirror_and_arm_the_tools() {
    let mut h = harness();
    let (a, _) = contact_and_coil(&mut h);
    run(&mut h, "edit.select", json!({"ids": [a]}));
    let orient = |h: &Harness<'_, CraftApp>| h.state().session.doc.page().unwrap().shape(&a).unwrap().orient;
    key(&mut h, Key::R);
    assert_eq!((orient(&h).turns, orient(&h).mirror), (1, false), "R turns a quarter");
    key(&mut h, Key::X);
    assert!(orient(&h).mirror, "X mirrors");
    key(&mut h, Key::Y);
    assert!(!orient(&h).mirror, "Y mirrors back the other way: {:?}", orient(&h));
    key(&mut h, Key::W);
    assert_eq!(h.state().tool, Tool::Wire { pipe: false });
    key(&mut h, Key::P);
    assert_eq!(h.state().tool, Tool::Wire { pipe: true });
    key(&mut h, Key::Escape);
    assert_eq!(h.state().tool, Tool::Pointer);
    // Shift+R is typing, not a shortcut.
    let before = orient(&h);
    h.event(Event::Key { key: Key::R, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::SHIFT });
    h.run_steps(2);
    assert_eq!(orient(&h), before);
}

#[test]
fn bare_letters_never_fire_while_text_is_typed() {
    let mut h = harness();
    let (a, _) = contact_and_coil(&mut h);
    run(&mut h, "edit.select", json!({"ids": [a]}));
    let orient = |h: &Harness<'_, CraftApp>| h.state().session.doc.page().unwrap().shape(&a).unwrap().orient;
    let before = orient(&h);
    // A focused text field (the palette's search) types the letters.
    let field = h.query_all_by_role(egui::accesskit::Role::TextInput).next().expect("a text field").rect();
    click(&mut h, field.center());
    assert!(h.state().session.sel.items == std::slice::from_ref(&a), "clicking the field leaves the selection");
    assert!(h.query_all_by_role(egui::accesskit::Role::TextInput).next().unwrap().is_focused(), "the field has the keyboard");
    for k in [Key::R, Key::X, Key::W] {
        key(&mut h, k);
    }
    assert_eq!(orient(&h), before, "R and X typed into the field");
    assert_eq!(h.state().tool, Tool::Pointer, "W typed into the field");
    // In-place text editing on the canvas types them too.
    h.state_mut().canvas.editing = None;
    key(&mut h, Key::Escape);
    run(&mut h, "edit.select", json!({"ids": [a]}));
    vizcraft_ui_egui::canvas::interact::start_editing(h.state_mut(), vizcraft_diagram::Hit::Shape(a.clone()));
    h.run_steps(3);
    assert!(h.state().canvas.editing.is_some(), "the symbol's text is being edited in place");
    key(&mut h, Key::R);
    assert_eq!(orient(&h), before, "R typed into the editor");
    // A dialog owns the keys as well.
    h.state_mut().canvas.editing = None;
    run(&mut h, "ui.commands", json!({}));
    key(&mut h, Key::R);
    assert_eq!(orient(&h), before, "R typed into the command palette");
}

#[test]
fn hovering_a_wire_highlights_its_net_and_a_click_holds_it() {
    let mut h = harness();
    let (a, b) = contact_and_coil(&mut h);
    let r = run(&mut h, "wire.draw", json!({"from": {"shape": a, "point": 1}, "to": {"shape": b, "point": 1}}));
    let (wire, net_id) = (r["id"].as_str().unwrap().to_string(), r["net"].as_str().unwrap().to_string());
    run(&mut h, "net.set", json!({"net": net_id, "number": "2003"}));
    run(&mut h, "edit.selectNone", json!({}));
    let c = h.state().session.doc.page().unwrap().connector(&wire).unwrap().clone();
    let poly = route_polyline(h.state().session.doc.page().unwrap(), &c);
    let (p, q) = poly.windows(2).map(|s| (s[0], s[1])).max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).unwrap();
    let on_wire = screen(&h, p.midpoint(q));
    h.event(Event::PointerMoved(on_wire));
    h.run_steps(2);
    assert_eq!(h.state().canvas.net_hover.as_deref(), Some(net_id.as_str()));
    let status = vizcraft_ui_egui::canvas::status_items(h.state());
    assert_eq!(status.first().map(String::as_str), Some("Net 2003, 2 pins"), "{status:?}");
    click(&mut h, on_wire);
    assert_eq!(h.state().canvas.net_held.as_deref(), Some(net_id.as_str()));
    let away = screen(&h, Point::new(p.x, p.y + 300.0));
    h.event(Event::PointerMoved(away));
    h.run_steps(2);
    assert_eq!(h.state().canvas.net_hover, None);
    assert_eq!(h.state().canvas.net_held.as_deref(), Some(net_id.as_str()), "held after the pointer leaves");
    key(&mut h, Key::Escape);
    assert_eq!(h.state().canvas.net_held, None, "Escape lets go");
    click(&mut h, on_wire);
    click(&mut h, away);
    assert_eq!(h.state().canvas.net_held, None, "a click elsewhere lets go");
}

/// The middle of `wire`'s longest run, in screen points.
fn on(h: &Harness<'_, CraftApp>, wire: &str) -> Pos2 {
    let page = h.state().session.doc.page().unwrap();
    let poly = route_polyline(page, page.connector(wire).unwrap());
    let (p, q) = poly.windows(2).map(|s| (s[0], s[1])).max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).unwrap();
    screen(h, p.midpoint(q))
}

#[test]
fn hovering_another_wire_shows_its_net_while_one_is_held() {
    // Regression (found by the user): a click held a loose wire's net, and from then on hovering
    // a wire connected at both ends kept showing the held net, so it looked as if connected
    // wires had no net.
    let mut h = harness();
    let (a, b) = contact_and_coil(&mut h);
    let joined = run(&mut h, "wire.draw", json!({"from": {"shape": a, "point": 1}, "to": {"shape": b, "point": 1}}));
    let pin0 = pin(&h, &a, 0);
    let loose = run(&mut h, "wire.draw", json!({"from": {"shape": a, "point": 0}, "to": {"x": pin0.x, "y": pin0.y - 120.0}}));
    run(&mut h, "net.set", json!({"net": joined["net"], "number": "1"}));
    run(&mut h, "net.set", json!({"net": loose["net"], "number": "2"}));
    run(&mut h, "edit.selectNone", json!({}));
    let status = |h: &Harness<'_, CraftApp>| vizcraft_ui_egui::canvas::status_items(h.state()).first().cloned();
    let at = on(&h, loose["id"].as_str().unwrap());
    click(&mut h, at);
    assert_eq!(status(&h).as_deref(), Some("Net 2, 1 pin"), "the loose wire's net is held");
    let at = on(&h, joined["id"].as_str().unwrap());
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
    assert_eq!(status(&h).as_deref(), Some("Net 1, 2 pins"), "the wire under the pointer wins over the held net");
    let away = screen(&h, Point::new(pin0.x + 300.0, pin0.y + 300.0));
    h.event(Event::PointerMoved(away));
    h.run_steps(2);
    assert_eq!(status(&h).as_deref(), Some("Net 2, 1 pin"), "the held net comes back off the wires");
}

#[test]
fn the_preview_palette_places_builder_symbols() {
    let mut h = harness();
    run(&mut h, "ui.palette", json!({"group": "engineeringPreview"}));
    h.run_steps(4);
    let n = h.state().session.doc.page().unwrap().shapes.len();
    h.get_by_label("Coil").click();
    h.run_steps(4);
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), n + 1);
    let s = page.shapes.last().unwrap();
    assert_eq!(s.glyph.as_ref().map(|g| g.builder.as_str()), Some("coil"));
    assert_eq!(h.state().session.sel.items, std::slice::from_ref(&s.id));
    let report = run(&mut h, "ui.palette", json!({}));
    let group = report["groups"].as_array().unwrap().iter().find(|g| g["section"] == "engineeringPreview").unwrap().clone();
    // One per builder: the wave 1 builders and those the engineering packs add.
    assert!(group["items"].as_array().unwrap().len() >= 10);
}
