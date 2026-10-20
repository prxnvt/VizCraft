//! Pixel snapshots of the keyboard-insertion and row-editing work: the picker with relationship
//! rows, an end label being edited, the Inspector's relationship ends, members and columns, the
//! Model Tree's relationship rows, the Re-Layout menu with the notation's layouts and the Key and
//! Steps groups of a notation tab. Same harness and rules as `snapshots.rs` (needs a wgpu adapter,
//! skipped without one or with `VIZCRAFT_SKIP_SNAPSHOTS`; refresh with `UPDATE_SNAPSHOTS=1`).

use egui::{Event, Key, Modifiers, PointerButton, pos2};
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use serde_json::{Value, json};
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn output_dir() -> String {
    format!("tests/snapshots/{}", std::env::consts::OS)
}

/// Whether wgpu can render here (see `snapshots.rs`; this is the only test in this binary).
fn wgpu_available() -> bool {
    if std::env::var_os("VIZCRAFT_SKIP_SNAPSHOTS").is_some() {
        return false;
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let probe = std::panic::catch_unwind(WgpuTestRenderer::new);
    std::panic::set_hook(hook);
    probe.is_ok() && (std::path::Path::new(&output_dir()).is_dir() || std::env::var_os("UPDATE_SNAPSHOTS").is_some())
}

/// A window on a document made from `template`, with the in-window menu bar and no title bar so
/// the picture does not depend on the host.
fn window(template: &str) -> Harness<'static, CraftApp> {
    let mut s = Session::new(vizcraft_engine::Document::new());
    s.run("template.new", &json!({"template": template})).unwrap();
    s.run("edit.selectNone", &json!({})).unwrap();
    let mut app = CraftApp::new(s, Services::default());
    app.ui.in_window_menus = true;
    app.integrated_titlebar = false;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_pixels_per_point(1.0).with_step_dt(1.0 / 60.0).wgpu().build_ui_state(
        |ui, app: &mut CraftApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(6);
    h
}

fn run(h: &mut Harness<'static, CraftApp>, id: &str, params: Value) -> Value {
    let r = h.state_mut().run(id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
    h.run_steps(3);
    r
}

fn snap(results: &mut SnapshotResults, h: &mut Harness<'static, CraftApp>, name: &str) {
    h.run_steps(2);
    results.add(h.try_snapshot_options(name, &SnapshotOptions::new().output_path(output_dir())));
}

/// The id of the first shape drawing an element of `type_id`.
fn first_shape(h: &Harness<'static, CraftApp>, type_id: &str) -> String {
    let doc = &h.state().session.doc;
    doc.page()
        .unwrap()
        .shapes
        .iter()
        .find(|s| s.element.as_deref().and_then(|e| doc.model.element(e)).is_some_and(|e| e.type_id == type_id))
        .map(|s| s.id.clone())
        .unwrap_or_else(|| panic!("no {type_id}"))
}

fn double_click(h: &mut Harness<'static, CraftApp>, p: egui::Pos2) {
    h.run_steps(30);
    h.event(Event::PointerMoved(p));
    h.run_steps(1);
    for _ in 0..2 {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(2);
}

#[test]
fn ux_snapshots() {
    if !wgpu_available() {
        eprintln!("ux snapshots: skipped (no wgpu adapter, no references, or VIZCRAFT_SKIP_SNAPSHOTS)");
        return;
    }
    let mut results = SnapshotResults::new();

    // The picker with next-step rows from a selected class.
    let mut h = window("uml.class");
    let class = first_shape(&h, "uml.Class");
    run(&mut h, "edit.select", json!({"ids": [class]}));
    run(&mut h, "ui.insertPicker", json!({"query": "to"}));
    snap(&mut results, &mut h, "picker-relationships");

    // An empty end label of an association opened for editing, with the typed text showing.
    let mut h = window("uml.class");
    let (cid, rid) = {
        let doc = &h.state().session.doc;
        let c = doc
            .page()
            .unwrap()
            .connectors
            .iter()
            .find(|c| c.relationship.as_deref().and_then(|r| doc.model.relationship(r)).is_some_and(|r| r.type_id == "uml.Association"))
            .unwrap();
        (c.id.clone(), c.relationship.clone().unwrap())
    };
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "targetMultiplicity", "value": null}));
    run(&mut h, "connector.setEndLabels", json!({"ids": [cid], "end": ""}));
    let g = {
        let page = h.state().session.doc.page().unwrap();
        vizcraft_diagram::connector_geometry(page, page.connector(&cid).unwrap())
    };
    let (a, b) = (g.polyline[g.polyline.len() - 2], *g.polyline.last().unwrap());
    let near = vizcraft_diagram::Point::new(b.x + (a.x - b.x) * 0.08, b.y + (a.y - b.y) * 0.08);
    run(&mut h, "view.pan", json!({"x": (a.x + b.x) / 2.0, "y": (a.y + b.y) / 2.0}));
    run(&mut h, "edit.selectNone", json!({}));
    let at = {
        let vp = h.state().canvas.viewport(&h.state().session.view).unwrap();
        vp.world_to_screen(pos2(near.x as f32, near.y as f32))
    };
    double_click(&mut h, at);
    h.event(Event::Text("0..*".into()));
    h.run_steps(3);
    snap(&mut results, &mut h, "canvas-end-label-edit");

    // The Inspector's relationship section for the association: ends under their elements' names.
    let mut h = window("uml.class");
    run(&mut h, "relationship.setProperty", json!({"id": rid, "key": "sourceMultiplicity", "value": "3..1"}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(&mut h, "edit.select", json!({"ids": [cid]}));
    snap(&mut results, &mut h, "inspector-relationship-ends");

    // Members and the Model Tree of a class.
    let mut h = window("uml.class");
    let class = first_shape(&h, "uml.Class");
    let el = h.state().session.doc.page().unwrap().shape(&class).unwrap().element.clone().unwrap();
    run(&mut h, "uml.addMember", json!({"element": el, "list": "attributes", "text": "- total: Money [0..1]"}));
    run(&mut h, "uml.addMember", json!({"element": el, "list": "operations", "text": "+ pay(amount: Money): bool"}));
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(&mut h, "edit.select", json!({"ids": [class]}));
    snap(&mut results, &mut h, "inspector-members");
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": false}));
    run(&mut h, "ui.pane", json!({"name": "navigation", "value": true}));
    run(&mut h, "ui.tree", json!({"expand": [el]}));
    snap(&mut results, &mut h, "pane-model-tree-relationships");

    // The Inspector's columns grid of an ER table.
    let mut h = window("er.schema");
    let table = first_shape(&h, "er.Entity");
    run(&mut h, "ui.pane", json!({"name": "shapeData", "value": true}));
    run(&mut h, "edit.select", json!({"ids": [table]}));
    snap(&mut results, &mut h, "inspector-columns");

    // The Re-Layout menu with the state machine's own layouts first.
    let mut h = window("uml.stateMachine");
    run(&mut h, "ui.tab", json!({"tab": "Design"}));
    {
        use egui_kittest::kittest::Queryable;
        h.get_by_label("Re-Layout options").click();
        h.run_steps(4);
    }
    snap(&mut results, &mut h, "menu-relayout-notation");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // The notation tab with its Check, Key and Steps groups.
    let mut h = window("cloud.topology");
    run(&mut h, "ui.tab", json!({"tab": "Cloud Topology"}));
    snap(&mut results, &mut h, "tab-notation-key-steps");
}
