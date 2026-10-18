//! The path bar above the canvas: the document title, the owner element when the page is a
//! drill-down view of one, and the page name. A click on a crumb switches page (`page.setActive`).
//!
//! A drill-down page is named `<element id>/<viewpoint>`: the element is the one the page opens
//! into, the viewpoint is what it shows. Its crumbs are the title, the element's name (a click
//! goes to a page that draws the element) and the viewpoint's name. Any other page has two
//! crumbs, the title and the page name.

use egui::{Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::CraftApp;
use crate::theme::{Tokens, medium, regular};

/// Height of the bar.
pub const BAR_H: f32 = 22.0;

/// One step of the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    /// Shown text.
    pub label: String,
    /// The page a click switches to; `None` for the last crumb, which is where you are.
    pub page: Option<String>,
}

/// The crumbs for the active page of `app`.
pub fn crumbs(app: &CraftApp) -> Vec<Crumb> {
    let doc = &app.session.doc;
    let Some(page) = doc.page() else { return Vec::new() };
    let first = doc.pages.first().filter(|p| p.id != page.id).map(|p| p.id.clone());
    let mut out = vec![Crumb { label: app.title_stem(), page: first }];
    let owner = page.name.split_once('/').and_then(|(id, vp)| doc.model.element(id).map(|e| (e, vp)));
    if let Some((e, vp)) = owner {
        let elsewhere = vizcraft_diagram::views::views_of(doc, &e.id).into_iter().find(|v| v.page_id != page.id).map(|v| v.page_id);
        let name = if e.name.is_empty() { e.id.clone() } else { e.name.clone() };
        out.push(Crumb { label: name, page: elsewhere });
        let label = vizcraft_notation::catalog()
            .pack(vp.split_once('.').map_or("", |(p, _)| p))
            .and_then(|p| p.viewpoint(vp))
            .map_or_else(|| vp.to_string(), |v| v.label.clone());
        out.push(Crumb { label, page: None });
    } else {
        out.push(Crumb { label: page.name.clone(), page: None });
    }
    out
}

/// Draw the bar at the top of `avail` and return what is left below it. The bar takes its height
/// from the layout, so the canvas below starts under it.
pub fn show(app: &mut CraftApp, ui: &mut Ui, avail: Rect) -> Rect {
    let h = BAR_H.min(avail.height());
    let (bar, _) = ui.allocate_exact_size(vec2(avail.width(), h), Sense::hover());
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(bar, 0.0, t.tab_strip);
    ui.painter().line_segment([bar.left_bottom(), bar.right_bottom()], egui::Stroke::new(1.0, t.border));
    let mut x = bar.left() + 10.0;
    let list = crumbs(app);
    let n = list.len();
    for (i, c) in list.into_iter().enumerate() {
        let last = i + 1 == n;
        let font = if last { medium(11.5) } else { regular(11.5) };
        let galley = ui.painter().layout_no_wrap(c.label.clone(), font, if last { t.text } else { t.text_dim });
        let r = Rect::from_min_size(pos2(x, bar.top()), vec2(galley.size().x + 8.0, h));
        if r.right() > bar.right() {
            break;
        }
        let id = ui.id().with(("crumb", i));
        let resp = ui.interact(r, id, if c.page.is_some() { Sense::click() } else { Sense::hover() });
        let label = c.label.clone();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, c.page.is_some(), format!("Crumb {label}")));
        if resp.hovered() && c.page.is_some() {
            ui.painter().rect_filled(r.shrink2(vec2(0.0, 2.0)), 3.0, t.hover);
        }
        let color = if last || resp.hovered() { t.text } else { t.text_dim };
        ui.painter().galley(pos2(r.left() + 4.0, bar.center().y - galley.size().y / 2.0), galley.clone(), color);
        if resp.clicked()
            && let Some(page) = c.page
        {
            let _ = app.run("page.setActive", json!({"page": page}));
        }
        x = r.right();
        if !last {
            ui.painter().text(pos2(x + 1.0, bar.center().y), egui::Align2::LEFT_CENTER, ">", regular(11.5), t.text_dim);
            x += 12.0;
        }
    }
    ui.available_rect_before_wrap()
}
