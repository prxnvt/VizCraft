//! The Layers pane and the Home > Editing > Layers menu under egui_kittest: adding a layer,
//! toggling a flag, assigning the selection, and the canvas skipping hidden shapes.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use vizcraft_engine::Session;
use vizcraft_ui_egui::{CraftApp, Services};

fn harness() -> Harness<'static, CraftApp> {
    let app = CraftApp::new(Session::new(vizcraft_engine::sample::sample_document()), Services::default());
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(64).build_ui_state(
        |ui, app: &mut CraftApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn layers(h: &Harness<'static, CraftApp>) -> Vec<vizcraft_diagram::Layer> {
    h.state().session.doc.page().map(|p| p.layers.clone()).unwrap_or_default()
}

#[test]
fn the_pane_starts_closed_and_layer_properties_opens_it() {
    let mut h = harness();
    assert!(!h.state().ui.panes.is_open("layers"));
    h.get_by_label("Layers").click();
    h.run_steps(3);
    h.get_by_label_contains("Layer Properties").click();
    h.run_steps(3);
    assert!(h.state().ui.panes.is_open("layers"));
}

#[test]
fn pane_adds_layers_toggles_flags_and_assigns_the_selection() {
    let mut h = harness();
    // Alone on its side, so every row has room.
    for name in ["history", "properties"] {
        h.state_mut().run("ui.pane", json!({"name": name, "value": false})).unwrap();
    }
    h.state_mut().run("ui.pane", json!({"name": "layers", "value": true})).unwrap();
    h.run_steps(3);
    h.get_by_label("Add").click();
    h.run_steps(3);
    let ls = layers(&h);
    assert_eq!(ls.len(), 1);
    assert_eq!(ls[0].name, "Layer 1");
    h.get_by_label("Lock Layer 1").click();
    h.run_steps(3);
    assert!(layers(&h)[0].locked);
    h.get_by_label("Lock Layer 1").click();
    h.run_steps(3);
    assert!(!layers(&h)[0].locked);
    // The added layer is current, so the selection can go on it.
    let first = h.state().session.doc.page().unwrap().shapes[0].id.clone();
    h.state_mut().run("edit.select", json!({"ids": [first.clone()]})).unwrap();
    h.run_steps(3);
    h.get_by_label("Assign Selection").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.page().unwrap().shape(&first).unwrap().layers, vec![ls[0].id.clone()]);
    // Hiding the layer drops the shape from the selection.
    h.get_by_label("Show Layer 1").click();
    h.run_steps(3);
    assert!(!layers(&h)[0].visible);
    assert!(h.state().session.sel.items.is_empty());
}

#[test]
fn ribbon_menu_assigns_the_selection_to_a_layer() {
    let mut h = harness();
    h.state_mut().run("layer.add", json!({"name": "Walls"})).unwrap();
    let first = h.state().session.doc.page().unwrap().shapes[0].id.clone();
    h.state_mut().run("edit.select", json!({"ids": [first.clone()]})).unwrap();
    h.run_steps(3);
    h.get_by_label("Layers").click();
    h.run_steps(3);
    h.get_by_label("Walls").click();
    h.run_steps(3);
    assert_eq!(h.state().session.doc.page().unwrap().shape(&first).unwrap().layers, vec!["layer1".to_string()]);
}
