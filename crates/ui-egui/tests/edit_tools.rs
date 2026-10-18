//! The Home tab's editing tools on the canvas, driven with real pointer and key events: the
//! format painter (one click, kept armed, Escape), lasso select, the ribbon buttons and Mod+D.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::{Color, Point};
use vizcraft_engine::{Document, Session};
use vizcraft_ui_egui::{CraftApp, Services, Tool};

/// A blank page with three rectangles in a row: `a` is red.
fn harness() -> (Harness<'static, CraftApp>, [String; 3]) {
    let mut app = CraftApp::new(Session::new(Document::new()), Services::default());
    let mut ids = Vec::new();
    for x in [100, 300, 500] {
        let r = app.run("shape.add", json!({"stencil": "basic.rectangle", "x": x, "y": 200, "text": format!("at {x}")})).unwrap();
        ids.push(r["id"].as_str().unwrap().to_string());
    }
    app.run("shape.setStyle", json!({"ids": [ids[0]], "fill": "#ff0000", "dash": "dashed"})).unwrap();
    app.run("edit.selectNone", json!({})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    let ids: [String; 3] = ids.try_into().unwrap();
    (h, ids)
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).unwrap();
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn centre(h: &Harness<'_, CraftApp>, id: &str) -> Pos2 {
    screen(h, h.state().session.doc.page().unwrap().shape(id).unwrap().center())
}

fn fill(h: &Harness<'_, CraftApp>, id: &str) -> Color {
    h.state().session.doc.page().unwrap().shape(id).unwrap().style.fill
}

fn click(h: &mut Harness<'_, CraftApp>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(30);
}

fn key(h: &mut Harness<'_, CraftApp>, k: Key, m: Modifiers) {
    h.event(Event::ModifiersChanged(m));
    h.event(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m });
    h.event(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

#[test]
fn format_painter_paints_once_or_until_escape() {
    let (mut h, [a, b, c]) = harness();
    let red = Color::rgb(255, 0, 0);
    // Nothing selected: the painter has nothing to copy from.
    assert!(h.state_mut().run("ui.tool", json!({"tool": "formatPainter"})).is_err());
    assert_eq!(h.state().tool, Tool::Pointer);

    h.state_mut().run("edit.select", json!({"ids": [a]})).unwrap();
    h.state_mut().run("ui.tool", json!({"tool": "formatPainter"})).unwrap();
    h.run_steps(2);
    let p = centre(&h, &b);
    click(&mut h, p);
    assert_eq!(fill(&h, &b), red);
    assert_eq!(h.state().session.doc.page().unwrap().shape(&b).unwrap().text, "at 300", "text stays");
    assert_eq!(h.state().tool, Tool::Pointer, "one click disarms it");
    let p = centre(&h, &c);
    click(&mut h, p);
    assert_ne!(fill(&h, &c), red, "a pointer click does not paint");

    h.state_mut().run("ui.tool", json!({"tool": "formatPainter", "from": a, "keep": true})).unwrap();
    h.state_mut().run("shape.setStyle", json!({"ids": [b], "fill": "#00ff00"})).unwrap();
    for id in [&b, &c] {
        let p = centre(&h, id);
        click(&mut h, p);
    }
    assert_eq!((fill(&h, &b), fill(&h, &c)), (red, red));
    assert_eq!(h.state().tool, Tool::FormatPainter { keep: true }, "kept armed");
    key(&mut h, Key::Escape, Modifiers::NONE);
    assert_eq!(h.state().tool, Tool::Pointer);
    assert!(h.state().canvas.painter_from.is_none());
}

#[test]
fn the_ribbon_format_painter_arms_from_the_selection() {
    let (mut h, [a, ..]) = harness();
    h.state_mut().run("edit.select", json!({"ids": [a]})).unwrap();
    h.run_steps(2);
    h.get_by_label("Format Painter").click();
    h.run_steps(3);
    assert_eq!(h.state().tool, Tool::FormatPainter { keep: false });
    assert_eq!(h.state().canvas.painter_from.as_deref(), Some(a.as_str()));
}

#[test]
fn lasso_selects_the_shapes_it_encloses() {
    let (mut h, [a, b, _]) = harness();
    h.state_mut().run("ui.tool", json!({"tool": "lasso"})).unwrap();
    // A freehand loop around a and b (they span x 100 to 400, y 200 to about 260).
    let path: Vec<Pos2> = [(60.0, 150.0), (250.0, 120.0), (450.0, 150.0), (460.0, 330.0), (250.0, 360.0), (60.0, 330.0), (62.0, 160.0)]
        .iter()
        .map(|(x, y)| screen(&h, Point::new(*x, *y)))
        .collect();
    h.event(Event::PointerMoved(path[0]));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: path[0], button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for p in &path[1..] {
        h.event(Event::PointerMoved(*p));
        h.run_steps(1);
    }
    let last = *path.last().unwrap();
    h.event(Event::PointerButton { pos: last, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().session.sel.items, vec![a, b]);
    assert_eq!(h.state().tool, Tool::Lasso, "lasso stays the tool");
    h.state_mut().run("ui.tool", json!({"tool": "area"})).unwrap();
    assert_eq!(h.state().tool, Tool::Pointer, "area select is the pointer");
}

#[test]
fn mod_d_duplicates_the_selection() {
    let (mut h, [a, ..]) = harness();
    h.state_mut().run("edit.select", json!({"ids": [a]})).unwrap();
    h.run_steps(2);
    let n = h.state().session.doc.page().unwrap().shapes.len();
    key(&mut h, Key::D, Modifiers::COMMAND);
    assert_eq!(h.state().session.doc.page().unwrap().shapes.len(), n + 1);
    assert_ne!(h.state().session.sel.items, vec![a]);
}
