//! Home > Shape Styles: the quick styles gallery, fill and line colour, line weight, and the Fill,
//! Line and Effects menus (gradient, transparency, shadow, dash, arrows, rounding, soft edges and
//! glow).

use egui::{Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, vec2};
use serde_json::json;
use vizcraft_diagram::theme::{self, QUICK_STYLES, VARIANTS};
use vizcraft_diagram::{Arrow, Dash, GradientKind, LineCap, Style};

use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, LINE_WEIGHTS, MenuEntry, color32, selected_connector, selected_shape, selected_style, short_num};
use crate::theme::Tokens;
use crate::widgets;

/// Size of one quick style cell.
const CELL: egui::Vec2 = egui::Vec2::new(24.0, 20.0);

/// The quick style the selected shape wears: the first whose fill, line, text colour, line width,
/// gradient and shadow are what applying it sets. `None` when nothing matches or no shape is selected.
fn current_quick_style(app: &CraftApp) -> Option<usize> {
    let st = &selected_shape(app)?.style;
    (0..QUICK_STYLES).find(|n| {
        theme::quick_style(&app.session.doc.theme, *n).is_some_and(|q| {
            st.fill == q.fill
                && st.line == q.line
                && st.font_color == q.text
                && (st.line_width - q.line_width).abs() < 0.01
                && st.gradient == q.gradient
                && st.shadow == q.shadow
        })
    })
}

/// The quick styles of the active theme: one row per intensity, one column per accent.
pub(super) fn quick_styles(app: &mut CraftApp, ui: &mut Ui) {
    let id = "design.applyQuickStyle";
    let t = Tokens::get(ui.ctx());
    let on = widgets::enabled(app, id);
    let doc_theme = app.session.doc.theme.clone();
    let current = current_quick_style(app);
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
                    let is_current = current == Some(n);
                    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, on, is_current, &q.name));
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
                    if is_current {
                        widgets::check_badge(ui, r);
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

/// Whether the primary selected item's style passes `test`; false with nothing selected.
fn style_is(app: &CraftApp, test: impl Fn(&Style) -> bool) -> bool {
    selected_style(app).is_some_and(test)
}

/// Whether two style numbers are the same value (the commands store what the menu sends).
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.001
}

/// A `shape.setStyle` entry, drawn checked while the selected style passes the test.
macro_rules! style_entry {
    ($label:expr, $params:expr, |$s:ident| $test:expr) => {
        MenuEntry::new($label, "shape.setStyle", $params).checked(|a| style_is(a, |$s| $test))
    };
}

/// Fill: gradient, transparency and shadow.
fn fill_menu() -> Vec<MenuEntry> {
    vec![
        style_entry!("No Gradient", json!({"gradient": false}), |s| s.gradient.is_none()),
        style_entry!("Linear Gradient", json!({"gradient": "linear"}), |s| s.gradient.as_ref().is_some_and(|g| g.kind == GradientKind::Linear)),
        style_entry!("Radial Gradient", json!({"gradient": "radial"}), |s| s.gradient.as_ref().is_some_and(|g| g.kind == GradientKind::Radial)),
        style_entry!("Opaque", json!({"fillTransparency": 0}), |s| near(s.fill_transparency, 0.0)),
        style_entry!("25% Transparent", json!({"fillTransparency": 0.25}), |s| near(s.fill_transparency, 0.25)),
        style_entry!("50% Transparent", json!({"fillTransparency": 0.5}), |s| near(s.fill_transparency, 0.5)),
        style_entry!("75% Transparent", json!({"fillTransparency": 0.75}), |s| near(s.fill_transparency, 0.75)),
        style_entry!("No Shadow", json!({"shadow": false}), |s| s.shadow.is_none()),
        style_entry!("Shadow", json!({"shadow": true}), |s| s.shadow.is_some()),
    ]
}

/// Whether every selected connector (the primary one at least) has these arrowheads.
fn arrows_are(app: &CraftApp, start: Arrow, end: Arrow) -> bool {
    selected_connector(app).is_some_and(|c| c.start_arrow == start && c.end_arrow == end)
}

/// Line: dash pattern, arrows (connectors), corner rounding, cap and transparency.
fn line_menu() -> Vec<MenuEntry> {
    vec![
        style_entry!("Solid", json!({"dash": "solid"}), |s| s.dash == Dash::Solid),
        style_entry!("Dashed", json!({"dash": "dashed"}), |s| s.dash == Dash::Dashed),
        style_entry!("Dotted", json!({"dash": "dotted"}), |s| s.dash == Dash::Dotted),
        style_entry!("Dash Dot", json!({"dash": "dashDot"}), |s| s.dash == Dash::DashDot),
        style_entry!("Long Dash", json!({"dash": "longDash"}), |s| s.dash == Dash::LongDash),
        MenuEntry::new("No Arrows", "connector.setArrows", json!({"start": "none", "end": "none"})).checked(|a| arrows_are(a, Arrow::None, Arrow::None)),
        MenuEntry::new("Arrow at End", "connector.setArrows", json!({"start": "none", "end": "triangle"}))
            .checked(|a| arrows_are(a, Arrow::None, Arrow::Triangle)),
        MenuEntry::new("Arrows at Both Ends", "connector.setArrows", json!({"start": "triangle", "end": "triangle"}))
            .checked(|a| arrows_are(a, Arrow::Triangle, Arrow::Triangle)),
        style_entry!("Square Corners", json!({"cornerRadius": 0}), |s| near(s.corner_radius, 0.0)),
        style_entry!("Rounded Corners", json!({"cornerRadius": 8}), |s| near(s.corner_radius, 8.0)),
        style_entry!("Very Rounded Corners", json!({"cornerRadius": 20}), |s| near(s.corner_radius, 20.0)),
        style_entry!("Round Line Caps", json!({"lineCap": "round"}), |s| s.line_cap == LineCap::Round),
        style_entry!("Square Line Caps", json!({"lineCap": "square"}), |s| s.line_cap == LineCap::Square),
        style_entry!("Flat Line Caps", json!({"lineCap": "butt"}), |s| s.line_cap == LineCap::Butt),
        style_entry!("Line Opaque", json!({"lineTransparency": 0}), |s| near(s.line_transparency, 0.0)),
        style_entry!("Line 50% Transparent", json!({"lineTransparency": 0.5}), |s| near(s.line_transparency, 0.5)),
    ]
}

/// Effects: shadow, soft edges and glow.
fn effects_menu() -> Vec<MenuEntry> {
    vec![
        style_entry!("No Shadow", json!({"shadow": false}), |s| s.shadow.is_none()),
        style_entry!("Offset Shadow", json!({"shadow": {"dx": 3, "dy": 3, "blur": 2}}), |s| s
            .shadow
            .is_some_and(|d| near(d.dx, 3.0) && near(d.dy, 3.0) && near(d.blur, 2.0))),
        style_entry!("Soft Shadow", json!({"shadow": {"dx": 0, "dy": 4, "blur": 10, "color": "#00000040"}}), |s| s
            .shadow
            .is_some_and(|d| near(d.dx, 0.0) && near(d.dy, 4.0) && near(d.blur, 10.0))),
        style_entry!("No Soft Edges", json!({"softEdges": 0}), |s| near(s.soft_edges, 0.0)),
        style_entry!("Soft Edges 2.5", json!({"softEdges": 2.5}), |s| near(s.soft_edges, 2.5)),
        style_entry!("Soft Edges 5", json!({"softEdges": 5}), |s| near(s.soft_edges, 5.0)),
        style_entry!("Soft Edges 10", json!({"softEdges": 10}), |s| near(s.soft_edges, 10.0)),
        style_entry!("No Glow", json!({"glow": false}), |s| s.glow.is_none()),
        style_entry!("Small Glow", json!({"glow": 4}), |s| s.glow.is_some_and(|g| near(g.radius, 4.0))),
        style_entry!("Large Glow", json!({"glow": 10}), |s| s.glow.is_some_and(|g| near(g.radius, 10.0))),
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
