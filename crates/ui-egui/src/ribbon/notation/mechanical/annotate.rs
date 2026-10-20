//! Sheet > Annotate: insert a feature control frame, datum feature symbol, welding symbol or
//! surface texture symbol of the annotation pack at the visible centre of the page (`shape.add`
//! with the master the page's variant draws them with), and the Standard menu that switches the
//! symbols on the page between ISO and ASME (`annotation.setStandard`), the current one checked.

use serde_json::{Value, json};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, MenuEntry, Size};

/// `shape.add` params for master `id` (or the page variant's master for its type), centred.
fn place(app: &CraftApp, id: &str) -> Value {
    let variant = app.session.doc.page().and_then(|p| p.view.as_ref()).map(|v| v.variant.clone()).unwrap_or_default();
    let at = crate::canvas::insert_point(app);
    match vizcraft_stencils::variant_master(id, &variant) {
        Some(m) => json!({"stencil": m.id, "x": (at.x - m.w / 2.0).round(), "y": (at.y - m.h / 2.0).round()}),
        None => json!({"stencil": id, "x": at.x.round(), "y": at.y.round()}),
    }
}

fn fcf(app: &CraftApp) -> Value {
    place(app, "annotation.fcf")
}
fn datum(app: &CraftApp) -> Value {
    place(app, "annotation.datum")
}
fn weld(app: &CraftApp) -> Value {
    place(app, "annotation.weld")
}
fn texture(app: &CraftApp) -> Value {
    place(app, "annotation.texture")
}

/// The `standard` values of the annotation symbols the page draws.
fn standards(app: &CraftApp) -> Vec<String> {
    let doc = &app.session.doc;
    let Some(page) = doc.page() else { return Vec::new() };
    page.shapes
        .iter()
        .filter_map(|s| s.element.as_deref().and_then(|e| doc.model.element(e)))
        .filter(|e| matches!(e.type_id.as_str(), "annotation.FeatureControlFrame" | "annotation.WeldingSymbol" | "annotation.SurfaceTexture"))
        .map(|e| e.prop("standard").and_then(|v| v.as_str()).unwrap_or_default().to_string())
        .collect()
}

fn all_of(app: &CraftApp, f: fn(&str) -> bool) -> bool {
    let s = standards(app);
    !s.is_empty() && s.iter().all(|x| f(x))
}

fn is_iso(app: &CraftApp) -> bool {
    all_of(app, |x| matches!(x, "iso1101" | "iso2553" | "iso21920"))
}
fn is_asme(app: &CraftApp) -> bool {
    all_of(app, |x| matches!(x, "asmeY14_5" | "aws" | "asmeY14_36"))
}
fn is_page(app: &CraftApp) -> bool {
    all_of(app, str::is_empty)
}

/// The Sheet tab's Annotate group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Annotate",
        vec![
            Stack(vec![
                Button { id: "shape.add", icon: "fcf", label: "Control Frame", size: Size::Small, params: fcf, checked: None },
                Button { id: "shape.add", icon: "datum", label: "Datum", size: Size::Small, params: datum, checked: None },
            ]),
            Stack(vec![
                Button { id: "shape.add", icon: "weld", label: "Weld", size: Size::Small, params: weld, checked: None },
                Button { id: "shape.add", icon: "texture", label: "Texture", size: Size::Small, params: texture, checked: None },
            ]),
            Menu {
                label: "Annotation Standard",
                icon: "annotationStandard",
                big: false,
                menu: vec![
                    MenuEntry::new("ISO (1101, 2553, 21920)", "annotation.setStandard", json!({"standard": "iso"})).checked(is_iso),
                    MenuEntry::new("ASME (Y14.5, AWS A2.4, Y14.36)", "annotation.setStandard", json!({"standard": "asme"})).checked(is_asme),
                    MenuEntry::new("Follow the Page Variant", "annotation.setStandard", json!({"standard": ""})).checked(is_page),
                ],
            },
        ],
    )
}
