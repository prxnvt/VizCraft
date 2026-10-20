//! The page tab strip along the bottom of the canvas: one tab per page in document order, the
//! active one drawn as a sheet joined to the canvas, and a plus button that adds a page.
//!
//! A click switches page (`page.setActive`), a double-click renames in place (`page.rename`),
//! and the right-click menu inserts, duplicates, renames and deletes pages and assigns a
//! background page (`page.add`, `page.duplicate`, `page.delete`, `page.setBackground`). Every
//! action runs a command by id; the strip holds no document logic. The rename in progress lives
//! in egui's memory, not in the app, since it is pure UI state.
//!
//! Generated decoration pages (a drawing sheet's frame and title block, a page border; see
//! `vizcraft_diagram::decor`) get no tab: they are drawn under the page they decorate and are
//! rebuilt from it, so there is nothing to edit there. They stay reachable by command
//! (`page.setActive`), and the tab shows while such a page is the active one.

use egui::{Id, Rect, Sense, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::{Value, json};

use crate::CraftApp;
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

/// Height of the strip.
pub const STRIP_H: f32 = 26.0;

/// Horizontal padding inside a tab.
const TAB_PAD: f32 = 12.0;

/// One page as the strip shows it.
struct TabInfo {
    /// The page's index in the document.
    index: usize,
    id: String,
    name: String,
    background: bool,
    assigned: Option<String>,
}

/// The rename in progress: `(page id, text being typed, focus requested yet)`.
type Rename = (String, String, bool);

fn rename_id() -> Id {
    Id::new("page_tab_rename")
}

/// Start renaming page `id` in place.
pub fn start_rename(ui: &Ui, app: &CraftApp, id: &str) {
    let name = app.session.doc.pages.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
    ui.data_mut(|d| d.insert_temp::<Rename>(rename_id(), (id.to_string(), name, false)));
}

/// Run a command for the strip; a refusal shows in the status bar.
fn run(app: &mut CraftApp, id: &str, params: Value) {
    if let Err(e) = app.run(id, params) {
        app.status(e);
    }
}

/// Draw the strip in `rect` (the bottom edge of the canvas area).
pub fn show(app: &mut CraftApp, ui: &mut Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, t.tab_strip);
    ui.painter().hline(rect.x_range(), rect.min.y + 0.5, egui::Stroke::new(1.0, t.border));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(6.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Min)));
    let doc = &app.session.doc;
    let active = doc.active_index();
    let tabs: Vec<TabInfo> = doc
        .pages
        .iter()
        .enumerate()
        .filter(|(i, p)| *i == active || !vizcraft_diagram::decor::is_generated(p))
        .map(|(index, p)| TabInfo { index, id: p.id.clone(), name: p.name.clone(), background: p.is_background, assigned: p.background.clone() })
        .collect();
    let foreground = tabs.iter().filter(|p| !p.background).count();
    egui::ScrollArea::horizontal().id_salt("page_tabs_scroll").max_height(STRIP_H).show(&mut child, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
            for tab in &tabs {
                page_tab(app, ui, &tabs, tab.index, tab, tab.index == active, foreground, &t);
            }
            ui.add_space(4.0);
            ui.allocate_ui(vec2(28.0, STRIP_H), |ui| {
                ui.add_space(1.0);
                if widgets::icon_button(ui, "plus", "Add Page", 14.0, true) {
                    run(app, "page.add", json!({}));
                }
            });
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn page_tab(app: &mut CraftApp, ui: &mut Ui, tabs: &[TabInfo], i: usize, tab: &TabInfo, active: bool, foreground: usize, t: &Tokens) {
    let renaming: Option<Rename> = ui.data(|d| d.get_temp::<Rename>(rename_id())).filter(|r| r.0 == tab.id);
    if let Some((id, mut text, focused)) = renaming {
        let r = ui.add_sized(vec2(140.0, STRIP_H - 4.0), egui::TextEdit::singleline(&mut text).font(regular(12.0)));
        if !focused {
            r.request_focus();
        }
        let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if escape {
            ui.data_mut(|d| d.remove::<Rename>(rename_id()));
        } else if enter || (focused && r.lost_focus()) {
            ui.data_mut(|d| d.remove::<Rename>(rename_id()));
            if text.trim() != tab.name {
                run(app, "page.rename", json!({"page": id, "name": text}));
            }
        } else {
            ui.data_mut(|d| d.insert_temp::<Rename>(rename_id(), (id, text, true)));
        }
        return;
    }
    let font = if active { semibold(12.0) } else { regular(12.0) };
    let color = if active { t.text } else { t.text_dim };
    let galley = ui.painter().layout_no_wrap(tab.name.clone(), font, color);
    let size = vec2(galley.size().x + 2.0 * TAB_PAD, STRIP_H - 3.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, active, format!("Page {}", tab.name)));
    // The active tab is a sheet hanging from the canvas: page-coloured, with side and bottom rules.
    if active {
        ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 4, se: 4 }, t.page);
        let stroke = egui::Stroke::new(1.0, t.border_strong);
        ui.painter().vline(r.min.x + 0.5, r.y_range(), stroke);
        ui.painter().vline(r.max.x - 0.5, r.y_range(), stroke);
        ui.painter().hline(r.x_range(), r.max.y - 0.5, stroke);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, t.hover);
    }
    let text_pos = pos2(r.min.x + TAB_PAD, r.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, color);
    if tab.background {
        // A background page is marked with a short rule under its name.
        let y = r.max.y - 5.0;
        ui.painter().hline((r.min.x + TAB_PAD)..=(r.max.x - TAB_PAD), y, egui::Stroke::new(1.0, t.text_disabled));
    }
    let tip = if tab.background { format!("{} (background page)", tab.name) } else { tab.name.clone() };
    let resp = resp.on_hover_text(tip);
    if resp.double_clicked() {
        start_rename(ui, app, &tab.id);
    } else if resp.clicked() && !active {
        run(app, "page.setActive", json!({"page": tab.id}));
    }
    resp.context_menu(|ui| context_menu(app, ui, tabs, i, tab, foreground));
}

fn item(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    let clicked = ui.add_enabled(enabled, egui::Button::new(label).min_size(vec2(160.0, 0.0))).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn context_menu(app: &mut CraftApp, ui: &mut Ui, tabs: &[TabInfo], i: usize, tab: &TabInfo, foreground: usize) {
    if item(ui, "Insert Page", true) {
        run(app, "page.add", json!({"index": i + 1}));
    }
    if item(ui, "Insert Page from Document Type...", true) {
        run(app, "ui.gallery", json!({"mode": "page"}));
    }
    if item(ui, "Duplicate", true) {
        run(app, "page.duplicate", json!({"page": tab.id}));
    }
    if item(ui, "Rename", true) {
        start_rename(ui, app, &tab.id);
    }
    let can_delete = tabs.len() > 1 && (tab.background || foreground > 1);
    if item(ui, "Delete", can_delete) {
        run(app, "page.delete", json!({"page": tab.id}));
    }
    ui.separator();
    ui.menu_button("Background", |ui| {
        let none = tab.assigned.is_none();
        if ui.add(egui::Button::selectable(none, "None").min_size(vec2(140.0, 0.0))).clicked() {
            run(app, "page.setBackground", json!({"page": tab.id, "background": null}));
            ui.close();
        }
        for bg in tabs.iter().filter(|p| p.background && p.id != tab.id) {
            let on = tab.assigned.as_deref() == Some(bg.id.as_str());
            if ui.add(egui::Button::selectable(on, bg.name.as_str()).min_size(vec2(140.0, 0.0))).clicked() {
                run(app, "page.setBackground", json!({"page": tab.id, "background": bg.id}));
                ui.close();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable as _;
    use serde_json::json;

    use crate::{CraftApp, Services};

    type Harness = egui_kittest::Harness<'static, CraftApp>;

    fn harness() -> Harness {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), Services::default());
        app.run("page.add", json!({"name": "Second"})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 700.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, app: &mut CraftApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.run_steps(5);
        h
    }

    #[test]
    fn a_click_switches_page_and_the_plus_button_adds_one() {
        let mut h = harness();
        assert_eq!(h.state().session.doc.active_index(), 1);
        let first = h.state().session.doc.pages[0].name.clone();
        h.get_by_label(&format!("Page {first}")).click();
        h.run_steps(3);
        assert_eq!(h.state().session.doc.active_index(), 0);
        h.get_by_label("Add Page").click();
        h.run_steps(3);
        let doc = &h.state().session.doc;
        assert_eq!(doc.pages.len(), 3);
        assert_eq!(doc.active_index(), 1, "the new page goes after the active one");
    }

    #[test]
    fn a_sheets_generated_frame_page_has_no_tab_but_stays_reachable() {
        let mut h = harness();
        h.state_mut().run("sheet.set", json!({})).unwrap();
        h.run_steps(3);
        let doc = &h.state().session.doc;
        let frame = doc.pages.iter().find(|p| vizcraft_diagram::decor::is_generated(p)).expect("the sheet's frame page").name.clone();
        assert!(h.query_by_label(&format!("Page {frame}")).is_none(), "no tab for {frame}");
        assert!(h.query_by_label("Page Second").is_some());
        let id = h.state().session.doc.pages.iter().find(|p| p.name == frame).unwrap().id.clone();
        h.state_mut().run("page.setActive", json!({"page": id})).unwrap();
        h.run_steps(3);
        assert!(h.query_by_label(&format!("Page {frame}")).is_some(), "the active frame page shows its tab");
    }

    #[test]
    fn a_double_click_renames_in_place() {
        let mut h = harness();
        let p = h.get_by_label("Page Second").rect().center();
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(1);
        for _ in 0..2 {
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
                h.run_steps(1);
            }
        }
        h.run_steps(3);
        h.event(egui::Event::Key { key: egui::Key::End, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        h.event(egui::Event::Text("x".into()));
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().session.doc.pages[1].name, "Secondx");
    }
}
