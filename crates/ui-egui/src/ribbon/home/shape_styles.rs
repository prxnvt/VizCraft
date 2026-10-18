//! Home > Shape Styles: the quick styles gallery, fill and line colour, line weight, and the Fill,
//! Line and Effects menus (gradient, transparency, shadow, dash, arrows, rounding, soft edges and
//! glow).

use egui::{Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::theme::{self, QUICK_STYLES, VARIANTS};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, LINE_WEIGHTS, MenuEntry, color32, selected_shape, selected_style, short_num};
use crate::theme::Tokens;
use crate::widgets;

/// Size of one quick style cell.
const CELL: egui::Vec2 = egui::Vec2::new(24.0, 20.0);

/// The quick styles of the active theme: one row per intensity, one column per accent.
fn quick_styles(app: &mut CraftApp, ui: &mut Ui) {
    let id = "design.applyQuickStyle";
    let t = Tokens::get(ui.ctx());
    let on = widgets::enabled(app, id);
    let doc_theme = app.session.doc.theme.clone();
    let mut chosen = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for row in 0..QUICK_STYLES / VARIANTS {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for col in 0..VARIANTS {
                    let n = row * VARIANTS + col;
                    let Some(q) = theme::quick_style(&doc_theme, n) else { continue };
                    let (r, resp) = ui.allocate_exact_size(CELL, Sense::click());
                    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, &q.name));
                    if on && resp.hovered() {
                        ui.painter().rect_filled(r, 3.0, t.hover);
                    }
                    let swatch = r.shrink2(vec2(4.0, 4.0));
                    let fill = q.gradient.as_ref().and_then(|g| g.stops.first()).map_or(q.fill, |s| s.color);
                    let (fill, line) = if on { (color32(fill), color32(q.line)) } else { (t.text_disabled, t.text_disabled) };
                    ui.painter().rect(swatch, 2.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
                    if q.shadow.is_some() && on {
                        let edge = Rect::from_min_max(swatch.left_bottom(), swatch.right_bottom() + vec2(0.0, 1.5));
                        ui.painter().rect_filled(edge.translate(vec2(1.0, 0.0)), 0.0, t.shadow);
                    }
                    let resp = widgets::tooltip(app, resp, &q.name, id);
                    if on && resp.clicked() {
                        chosen = Some(n);
                    }
                }
            });
        }
    });
    if let Some(n) = chosen {
        let _ = app.run(id, json!({"style": n}));
    }
}

fn set_style(label: &'static str, params: Value) -> MenuEntry {
    (label, "shape.setStyle", params)
}

/// Fill: gradient, transparency and shadow.
fn fill_menu() -> Vec<MenuEntry> {
    vec![
        set_style("No Gradient", json!({"gradient": false})),
        set_style("Linear Gradient", json!({"gradient": "linear"})),
        set_style("Radial Gradient", json!({"gradient": "radial"})),
        set_style("Opaque", json!({"fillTransparency": 0})),
        set_style("25% Transparent", json!({"fillTransparency": 0.25})),
        set_style("50% Transparent", json!({"fillTransparency": 0.5})),
        set_style("75% Transparent", json!({"fillTransparency": 0.75})),
        set_style("No Shadow", json!({"shadow": false})),
        set_style("Shadow", json!({"shadow": true})),
    ]
}

/// Line: dash pattern, arrows (connectors), corner rounding, cap and transparency.
fn line_menu() -> Vec<MenuEntry> {
    let mut out = vec![
        set_style("Solid", json!({"dash": "solid"})),
        set_style("Dashed", json!({"dash": "dashed"})),
        set_style("Dotted", json!({"dash": "dotted"})),
        set_style("Dash Dot", json!({"dash": "dashDot"})),
        set_style("Long Dash", json!({"dash": "longDash"})),
    ];
    out.extend([
        ("No Arrows", "connector.setArrows", json!({"start": "none", "end": "none"})),
        ("Arrow at End", "connector.setArrows", json!({"start": "none", "end": "triangle"})),
        ("Arrows at Both Ends", "connector.setArrows", json!({"start": "triangle", "end": "triangle"})),
    ]);
    out.extend([
        set_style("Square Corners", json!({"cornerRadius": 0})),
        set_style("Rounded Corners", json!({"cornerRadius": 8})),
        set_style("Very Rounded Corners", json!({"cornerRadius": 20})),
        set_style("Round Line Caps", json!({"lineCap": "round"})),
        set_style("Square Line Caps", json!({"lineCap": "square"})),
        set_style("Flat Line Caps", json!({"lineCap": "butt"})),
        set_style("Line Opaque", json!({"lineTransparency": 0})),
        set_style("Line 50% Transparent", json!({"lineTransparency": 0.5})),
    ]);
    out
}

/// Effects: shadow, soft edges and glow.
fn effects_menu() -> Vec<MenuEntry> {
    vec![
        set_style("No Shadow", json!({"shadow": false})),
        set_style("Offset Shadow", json!({"shadow": {"dx": 3, "dy": 3, "blur": 2}})),
        set_style("Soft Shadow", json!({"shadow": {"dx": 0, "dy": 4, "blur": 10, "color": "#00000040"}})),
        set_style("No Soft Edges", json!({"softEdges": 0})),
        set_style("Soft Edges 2.5", json!({"softEdges": 2.5})),
        set_style("Soft Edges 5", json!({"softEdges": 5})),
        set_style("Soft Edges 10", json!({"softEdges": 10})),
        set_style("No Glow", json!({"glow": false})),
        set_style("Small Glow", json!({"glow": 4})),
        set_style("Large Glow", json!({"glow": 10})),
    ]
}

/// The Home tab's Shape Styles group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Shape Styles",
        vec![
            Custom(quick_styles),
            Stack(vec![
                Colors {
                    id: "shape.setStyle",
                    param: "fill",
                    icon: "fill",
                    name: "Fill",
                    show_label: true,
                    none: Some("No Fill"),
                    current: |a| selected_shape(a).map(|s| color32(s.style.fill)),
                },
                Colors {
                    id: "shape.setStyle",
                    param: "line",
                    icon: "lineColor",
                    name: "Line",
                    show_label: true,
                    none: None,
                    current: |a| selected_style(a).map(|s| color32(s.line)),
                },
                Row(vec![
                    Label { icon: "lineWeight", text: "" },
                    Combo {
                        id: "shape.setStyle",
                        param: "lineWidth",
                        name: "Line Weight",
                        width: 66.0,
                        items: |_| LINE_WEIGHTS.iter().map(|w| format!("{} px", short_num(f64::from(*w)))).collect(),
                        current: |a| selected_style(a).map(|s| format!("{} px", short_num(s.line_width))).unwrap_or_default(),
                    },
                ]),
            ]),
            Stack(vec![
                Menu { label: "Fill Options", icon: "fill", big: false, menu: fill_menu() },
                Menu { label: "Line Options", icon: "lineColor", big: false, menu: line_menu() },
                Menu { label: "Effects", icon: "shape", big: false, menu: effects_menu() },
            ]),
        ],
    )
}
