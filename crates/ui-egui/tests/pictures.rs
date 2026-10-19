//! Pictures and ink on the canvas, driven under egui_kittest on the sample flowchart: the Picture
//! and Ink tabs fit 1280 px, a picture is drawn through the texture cache, the Crop tool's handle
//! drag commits one crop, a Pen drag commits one stroke that undoes in one step, and the Eraser
//! removes it. Assertions read the session, never pixels.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_diagram::Point;
use vizcraft_engine::Session;
use vizcraft_ui_egui::ribbon::Overflow;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness(width: f32) -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    let mut h = Harness::builder().with_size(vec2(width, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, _| if x < w / 2 { image::Rgba([220, 40, 40, 255]) } else { image::Rgba([40, 80, 220, 255]) });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// Insert a 160 by 100 picture at the centre of the view, selected; returns its id.
fn picture(h: &mut Harness<'_, CraftApp>) -> String {
    let r = h.state_mut().run("picture.insert", json!({"bytes": vizcraft_diagram::base64::encode(&png(160, 100))})).unwrap();
    h.run_steps(3);
    r["id"].as_str().unwrap().to_string()
}

fn screen(h: &Harness<'_, CraftApp>, p: Point) -> Pos2 {
    let app = h.state();
    let vp = app.canvas.viewport(&app.session.view).expect("the canvas has been laid out");
    vp.world_to_screen(pos2(p.x as f32, p.y as f32))
}

fn drag(h: &mut Harness<'_, CraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        // A slight bow, so the stroke is not one straight segment.
        let p = from + (to - from) * t + vec2(0.0, (t * std::f32::consts::PI).sin() * 30.0);
        h.event(Event::PointerMoved(p));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

#[test]
fn the_picture_tab_fits_1280_wide_and_shows_while_a_picture_is_selected() {
    let mut h = harness(1280.0);
    picture(&mut h);
    h.state_mut().run("ui.tab", json!({"tab": "Picture"})).unwrap();
    h.run_steps(3);
    assert!(!Overflow::get(&h.ctx).overflows(), "{:?}", Overflow::get(&h.ctx));
    for label in ["Brightness", "Contrast", "Recolor", "Transparency", "Compress Pictures", "Change Picture", "Reset Picture", "Crop", "Border"] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing from Picture");
    }
    assert!(vizcraft_ui_egui::canvas::draw::pictures::cached(&h.ctx) >= 1, "the picture went through the texture cache");
    h.get_by_label("Brightness").click();
    h.run_steps(2);
    h.get_by_label("+20%").click();
    h.run_steps(3);
    let sel = h.state().session.sel.primary.clone().unwrap();
    let sh = h.state().session.doc.page().unwrap().shape(&sel).unwrap().clone();
    assert_eq!(sh.image.unwrap().adjust.brightness, 0.2);
    h.state_mut().run("edit.selectNone", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Brightness").is_none(), "the tab hides with nothing selected");
}

#[test]
fn the_ink_tab_fits_1280_wide_while_the_pen_is_out() {
    let mut h = harness(1280.0);
    h.state_mut().run("ui.tool", json!({"tool": "pen"})).unwrap();
    h.state_mut().run("ui.tab", json!({"tab": "Ink"})).unwrap();
    h.run_steps(3);
    assert!(!Overflow::get(&h.ctx).overflows(), "{:?}", Overflow::get(&h.ctx));
    for label in ["Pen", "Highlighter", "Eraser", "Lasso Select", "Colour", "Thickness", "Convert to Shapes", "Close Ink Tools"] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing from Ink");
    }
}

#[test]
fn a_pen_drag_commits_one_stroke_that_undoes_in_one_step_and_the_eraser_removes_it() {
    let mut h = harness(1600.0);
    h.state_mut().run("ui.tool", json!({"tool": "pen"})).unwrap();
    h.run_steps(2);
    let before = h.state().session.doc.page().unwrap().shapes.len();
    let c = h.state().canvas.viewport(&h.state().session.view).unwrap().screen_rect.center();
    drag(&mut h, c + vec2(-200.0, 200.0), c + vec2(-60.0, 200.0));
    let page = h.state().session.doc.page().unwrap();
    assert_eq!(page.shapes.len(), before + 1, "one stroke");
    let stroke = page.shapes.last().unwrap().clone();
    assert!(vizcraft_diagram::ink::is_stroke(&stroke));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.doc.page().unwrap().shapes.len(), before, "one undo step");
    h.state_mut().run("edit.redo", json!({})).unwrap();
    h.state_mut().run("ui.tool", json!({"tool": "eraser"})).unwrap();
    h.run_steps(2);
    // Sweep across the stroke's middle.
    let pts = vizcraft_diagram::ink::stroke_points(&stroke);
    let mid = pts[pts.len() / 2];
    let at = screen(&h, mid);
    drag(&mut h, at + vec2(-4.0, -40.0), at + vec2(4.0, 40.0));
    assert!(h.state().session.doc.page().unwrap().shape(&stroke.id).is_none(), "erased");
}

#[test]
fn a_crop_handle_drag_commits_one_crop() {
    let mut h = harness(1600.0);
    let id = picture(&mut h);
    h.state_mut().run("ui.tool", json!({"tool": "crop"})).unwrap();
    h.run_steps(2);
    let s = h.state().session.doc.page().unwrap().shape(&id).unwrap().clone();
    let from = screen(&h, Point::new(s.x + s.w, s.y + s.h / 2.0));
    let to = screen(&h, Point::new(s.x + s.w * 0.5, s.y + s.h / 2.0));
    h.event(Event::PointerMoved(from));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for i in 1..=8 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    let after = h.state().session.doc.page().unwrap().shape(&id).unwrap().clone();
    let crop = after.image.as_ref().unwrap().crop;
    assert!((crop.w - 0.5).abs() < 0.05, "{crop:?}");
    assert!((after.w - s.w * 0.5).abs() < 4.0 && (after.x - s.x).abs() < 1e-6, "the frame follows the crop: {} {}", after.x, after.w);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    let back = h.state().session.doc.page().unwrap().shape(&id).unwrap().clone();
    assert_eq!(back.image.unwrap().crop, vizcraft_diagram::CropRect::FULL, "one undo step");
    assert_eq!(back.w, s.w);
}

#[test]
fn a_dropped_png_becomes_a_picture_shape_not_a_document() {
    let inbox: vizcraft_ui_egui::Inbox = Default::default();
    let services = Services { inbox: Some(inbox.clone()), ..Services::default() };
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), services);
    let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    let shapes = h.state().session.doc.page().unwrap().shapes.len();
    let title = h.state().session.doc.title.clone();
    inbox.lock().unwrap().push(("Drop.PNG".into(), png(40, 20)));
    h.run_steps(3);
    let app = h.state();
    assert_eq!(app.session.doc.page().unwrap().shapes.len(), shapes + 1, "one picture shape was added");
    assert_eq!(app.session.doc.images.len(), 1);
    assert_eq!(app.session.doc.images[0].media_type, "image/png");
    assert_eq!(app.session.doc.title, title, "the document was not replaced");
}
