//! Ribbon widgets: big and small command buttons, toggles, split buttons, menu buttons, combo
//! fields, groups and tooltips. Each button runs a command by id, so the ribbon never has
//! behaviour of its own; labels, shortcuts, locations and enablement come from the registry (or
//! [`UI_COMMANDS`] for `ui.*` and `app.*` commands).
//!
//! Every widget registers accessibility info (`WidgetInfo`), so tests find it by its label.

use egui::{Align2, Atom, CornerRadius, Id, Pos2, Rect, Response, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use serde_json::{Value, json};

use crate::theme::{Tokens, regular, semibold};
use crate::{CraftApp, UI_COMMANDS, icons};

/// Height of the ribbon content area (without group labels).
pub const CONTENT_H: f32 = 66.0;
/// Height of the group label row under the content.
pub const LABEL_H: f32 = 16.0;
/// Vertical padding inside the ribbon panel (top and bottom together).
pub const RIBBON_PAD_V: f32 = 10.0;
/// Height of the ribbon body panel.
pub const BODY_H: f32 = CONTENT_H + LABEL_H + RIBBON_PAD_V;
/// Big button: icon size and centre line, label font size, first label line and line step,
/// minimum width. Two label lines end at 63, just inside `CONTENT_H`.
pub const BIG_ICON: f32 = 32.0;
pub const BIG_ICON_CY: f32 = 18.0;
pub const BIG_LABEL_Y: f32 = 37.0;
pub const BIG_FONT: f32 = 11.5;
pub const BIG_LINE_H: f32 = 13.0;
pub const BIG_MIN_W: f32 = 44.0;
/// Horizontal padding around a big button's label.
pub const BIG_PAD_X: f32 = 12.0;
/// Small button: height, icon size, icon cell width (icon plus padding), label padding.
pub const SMALL_H: f32 = 22.0;
pub const SMALL_ICON: f32 = 16.0;
pub const SMALL_ICON_W: f32 = 24.0;
pub const SMALL_FONT: f32 = 11.5;
pub const SMALL_PAD_X: f32 = 6.0;
/// Width of the arrow part of a split or menu button (small) and the dropdown glyph.
pub const ARROW_W: f32 = 12.0;
pub const ARROW_GLYPH: f32 = 9.0;
/// Corner radius of button backgrounds.
pub const BUTTON_RADIUS: u8 = 4;
/// Space between a group's content and the dividing rule, and after the rule.
pub const GROUP_GAP: f32 = 6.0;
/// Width of a vertical gap control inside a group.
pub const GAP_W: f32 = 6.0;
/// Default minimum width of a dropdown menu.
pub const MENU_MIN_W: f32 = 200.0;
/// Width of the column at the right edge of a menu row that holds the check mark of the current
/// choice. Every row of a menu that has a current value reserves it, so labels and shortcuts line up.
pub const CHECK_COL_W: f32 = 16.0;

/// A check mark drawn with two strokes (no glyph, so every font renders it the same), centred on `c`.
/// The window menu bar paints it at the left of a row, the ribbon's menus at the right.
pub fn check_mark(ui: &Ui, c: Pos2, color: egui::Color32) {
    let s = Stroke::new(1.6, color);
    ui.painter().line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.0)], s);
    ui.painter().line_segment([c + vec2(-1.5, 3.0), c + vec2(4.5, -3.5)], s);
}

/// The mark a gallery cell shows when it is the current choice: a small accent disc with a check
/// in the cell's top right corner. `cell` is the cell's rect.
pub fn check_badge(ui: &Ui, cell: Rect) {
    let t = Tokens::get(ui.ctx());
    let c = pos2(cell.max.x - 7.0, cell.min.y + 7.0);
    ui.painter().circle_filled(c, 5.5, t.accent);
    let s = Stroke::new(1.3, t.on_accent);
    ui.painter().line_segment([c + vec2(-2.8, 0.0), c + vec2(-0.8, 2.0)], s);
    ui.painter().line_segment([c + vec2(-0.8, 2.0), c + vec2(2.8, -2.2)], s);
}

/// The placeholder atom that reserves a menu row's check column; [`paint_check_column`] draws into it.
fn check_column_id() -> Id {
    Id::new("menu_check_column")
}

fn check_column() -> Atom<'static> {
    Atom::custom(check_column_id(), vec2(CHECK_COL_W, 12.0))
}

/// Paint the check mark into the column [`check_column`] reserved in `layout`, when `on`.
fn paint_check_column(ui: &Ui, layout: &egui::AtomLayoutResponse, on: bool, enabled: bool) {
    if !on {
        return;
    }
    if let Some(r) = layout.rect(check_column_id()) {
        let t = Tokens::get(ui.ctx());
        check_mark(ui, r.center(), if enabled { t.accent } else { t.text_disabled });
    }
}

/// What `ui.inspect` reports about the dropdown that is open: its title and the rows drawn in the
/// last frame. An open menu is egui popup state, so the widgets record themselves as they draw.
#[derive(Clone, Default)]
struct MenuRecord {
    frame: u64,
    title: String,
    rows: Vec<Value>,
}

fn record_id() -> Id {
    Id::new("open_menu_record")
}

/// Start the record for the dropdown `title`, which is being drawn this frame.
fn begin_menu(ui: &Ui, title: &str) {
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx().data_mut(|d| d.insert_temp(record_id(), MenuRecord { frame, title: title.to_string(), rows: Vec::new() }));
}

/// Add a row to the open dropdown's record.
fn record_row(ui: &Ui, row: Value) {
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx().data_mut(|d| {
        let rec = d.get_temp_mut_or_insert_with(record_id(), MenuRecord::default);
        if rec.frame != frame {
            *rec = MenuRecord { frame, ..MenuRecord::default() };
        }
        rec.rows.push(row);
    });
}

/// The dropdown that was drawn in the last frame, as `{title, rows: [{label, id, enabled,
/// checked}]}`, or `None` when no dropdown is open. `checked` is null for a row with no current
/// value to show.
pub fn open_menu(ctx: &egui::Context) -> Option<Value> {
    let frame = ctx.cumulative_frame_nr();
    let rec = ctx.data(|d| d.get_temp::<MenuRecord>(record_id()))?;
    (rec.frame + 1 >= frame && !rec.rows.is_empty()).then(|| json!({"title": rec.title, "rows": rec.rows}))
}

/// The portable shortcut string for a command (engine or UI), first alternative only.
pub fn shortcut_of(app: &CraftApp, id: &str) -> &'static str {
    let sc = match app.session.registry.get(id) {
        Some(spec) => spec.shortcut,
        None => UI_COMMANDS.iter().find(|(i, ..)| *i == id).map(|(_, _, sc, _)| *sc).unwrap_or(""),
    };
    sc.split(" / ").next().unwrap_or("")
}

/// Shortcut text for a command as the platform shows it (`Cmd S` glyphs on Apple macOS, `Ctrl+S`
/// elsewhere).
pub fn shortcut_text(app: &CraftApp, id: &str) -> String {
    let sc = shortcut_of(app, id);
    if sc.is_empty() {
        return String::new();
    }
    if cfg!(target_os = "macos") {
        sc.replace("Mod+", "\u{2318}").replace("Shift+", "\u{21e7}").replace("Alt+", "\u{2325}").replace("Ctrl+", "\u{2303}")
    } else {
        sc.replace("Mod+", "Ctrl+")
    }
}

/// The label a command is registered with (engine or UI). Empty for an unknown id.
pub fn label_of(app: &CraftApp, id: &str) -> &'static str {
    match app.session.registry.get(id) {
        Some(spec) => spec.label,
        None => UI_COMMANDS.iter().find(|(i, ..)| *i == id).map(|(_, l, ..)| *l).unwrap_or(""),
    }
}

/// Where a command lives (`Home > Clipboard`), for tooltips. UI commands have no location.
pub fn location_of(app: &CraftApp, id: &str) -> &'static str {
    app.session.registry.get(id).map(|s| s.location).unwrap_or("")
}

/// Whether `id` names a command at all (engine or UI).
pub fn exists(app: &CraftApp, id: &str) -> bool {
    app.session.registry.get(id).is_some() || UI_COMMANDS.iter().any(|(i, ..)| *i == id)
}

/// Whether a command exists and is enabled. UI commands are always enabled.
pub fn enabled(app: &CraftApp, id: &str) -> bool {
    match app.session.registry.get(id) {
        Some(s) => (s.enabled)(&app.session).is_none(),
        None => UI_COMMANDS.iter().any(|(i, ..)| *i == id),
    }
}

/// Why a command is disabled, if it is.
pub fn disabled_reason(app: &CraftApp, id: &str) -> Option<&'static str> {
    app.session.registry.get(id).and_then(|s| (s.enabled)(&app.session))
}

/// A big button's label: the registry label, broken onto two lines at the space nearest the
/// middle when it is long enough to want it (`Select All` becomes `Select` over `All`).
pub fn big_label(label: &str) -> String {
    if label.chars().count() < 9 || !label.contains(' ') {
        return label.to_string();
    }
    let mid = label.len() / 2;
    let best = label.match_indices(' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match best {
        Some(i) => format!("{}\n{}", &label[..i], &label[i + 1..]),
        None => label.to_string(),
    }
}

/// A tooltip with the label, shortcut, location and (when disabled) why it is not available.
pub fn tooltip(app: &CraftApp, resp: Response, label: &str, id: &str) -> Response {
    let sc = shortcut_text(app, id);
    let desc = location_of(app, id);
    let on = enabled(app, id);
    let reason = disabled_reason(app, id);
    resp.on_hover_ui(|ui| {
        ui.set_max_width(260.0);
        let head = if sc.is_empty() { label.to_string() } else { format!("{label} ({sc})") };
        ui.label(egui::RichText::new(head).font(semibold(12.0)));
        if !desc.is_empty() {
            ui.label(egui::RichText::new(desc).small().weak());
        }
        if !on {
            let why = match reason {
                Some(why) => format!("Not available right now: {why}"),
                None => "Not available right now".to_string(),
            };
            ui.label(egui::RichText::new(why).small().weak());
        }
    })
}

/// Hover, pressed and checked backgrounds shared by every button.
fn bg(ui: &Ui, r: Rect, resp: &Response, checked: bool, on: bool, t: &Tokens) {
    let fill = if !on {
        return;
    } else if resp.is_pointer_button_down_on() {
        t.pressed
    } else if checked {
        t.checked
    } else if resp.hovered() {
        t.hover
    } else {
        return;
    };
    ui.painter().rect_filled(r, CornerRadius::same(BUTTON_RADIUS), fill);
    if checked {
        ui.painter().rect_stroke(r, CornerRadius::same(BUTTON_RADIUS), Stroke::new(1.0, t.accent.linear_multiply(0.5)), egui::StrokeKind::Inside);
    }
}

fn text_width(ui: &Ui, s: &str, font: egui::FontId) -> f32 {
    let t = Tokens::get(ui.ctx());
    ui.ctx().fonts_mut(|f| f.layout_no_wrap(s.to_string(), font, t.text).size().x)
}

/// Icon and text colours for an enabled or disabled control.
fn colors(t: &Tokens, on: bool) -> (egui::Color32, egui::Color32, egui::Color32) {
    if on { (t.icon, t.accent, t.text) } else { (t.text_disabled, t.text_disabled, t.text_disabled) }
}

/// Paint the body of a big button: icon over label lines; returns the rect of the last line.
/// With `arrow`, the last line moves left to leave room for a dropdown glyph beside it.
fn paint_big(ui: &Ui, r: Rect, icon: &str, label: &str, on: bool, arrow: bool, t: &Tokens) -> Rect {
    let (ic, ac, tc) = colors(t, on);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.min.y + BIG_ICON_CY), vec2(BIG_ICON, BIG_ICON)), icon, ic, ac);
    let mut y = r.min.y + BIG_LABEL_Y;
    let mut last = Rect::NOTHING;
    let lines: Vec<&str> = label.split('\n').collect();
    for (i, l) in lines.iter().enumerate() {
        let shift = if arrow && i + 1 == lines.len() { (ARROW_GLYPH + 4.0) / 2.0 } else { 0.0 };
        last = ui.painter().text(pos2(r.center().x - shift, y), Align2::CENTER_TOP, l, regular(BIG_FONT), tc);
        y += BIG_LINE_H;
    }
    last
}

/// Paint the body of a small button: icon at the left, optional label.
fn paint_small(ui: &Ui, r: Rect, icon: &str, label: Option<&str>, on: bool, t: &Tokens) {
    let (ic, ac, tc) = colors(t, on);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + SMALL_ICON_W / 2.0, r.center().y), vec2(SMALL_ICON, SMALL_ICON)), icon, ic, ac);
    if let Some(l) = label {
        ui.painter().text(pos2(r.min.x + SMALL_ICON_W, r.center().y), Align2::LEFT_CENTER, l, regular(SMALL_FONT), tc);
    }
}

fn big_width(ui: &Ui, label: &str, extra: f32) -> f32 {
    let galley_w = label.split('\n').map(|l| text_width(ui, l, regular(BIG_FONT))).fold(0.0, f32::max);
    (galley_w + BIG_PAD_X + extra).max(BIG_MIN_W)
}

fn small_width(ui: &Ui, label: Option<&str>) -> f32 {
    SMALL_ICON_W + label.map(|l| text_width(ui, l, regular(SMALL_FONT)) + SMALL_PAD_X).unwrap_or(0.0)
}

fn run_if(app: &mut CraftApp, resp: &Response, on: bool, id: &str, params: Value) {
    if resp.clicked() && on {
        let _ = app.run(id, params);
    }
}

/// A big ribbon button: 32 px icon over a (possibly two-line) label. Runs `id` with `params`.
pub fn big(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: &str, id: &str, params: Value) -> Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(big_width(ui, label, 0.0), CONTENT_H), Sense::click());
    let on = enabled(app, id);
    let name = label.replace('\n', " ");
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, &name));
    bg(ui, r, &resp, false, on, &t);
    paint_big(ui, r, icon, label, on, false, &t);
    let resp = tooltip(app, resp, &name, id);
    run_if(app, &resp, on, id, params);
    resp
}

/// A small button: 16 px icon with an optional label. Runs `id` with `params`; `tip` names it.
pub fn small(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: Option<&str>, tip: &str, id: &str, params: Value) -> Response {
    small_checked(ui, app, icon, label, tip, id, params, false)
}

/// A small button drawn in the toggled state when `checked` (pane toggles, dark mode).
pub fn toggle(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: Option<&str>, tip: &str, id: &str, params: Value, checked: bool) -> Response {
    small_checked(ui, app, icon, label, tip, id, params, checked)
}

fn small_checked(ui: &mut Ui, app: &mut CraftApp, icon: &str, label: Option<&str>, tip: &str, id: &str, params: Value, checked: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(small_width(ui, label), SMALL_H), Sense::click());
    let on = enabled(app, id);
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, on, checked, tip));
    bg(ui, r, &resp, checked, on, &t);
    paint_small(ui, r, icon, label, on, &t);
    let resp = tooltip(app, resp, tip, id);
    run_if(app, &resp, on, id, params);
    resp
}

/// A split button: the main part runs `id`, the arrow part opens `menu`. Big: icon over the
/// label with the arrow under it; small: icon and label with the arrow at the right.
pub fn split(
    ui: &mut Ui,
    app: &mut CraftApp,
    icon: &str,
    label: &str,
    id: &str,
    params: Value,
    big_btn: bool,
    menu: impl FnOnce(&mut Ui, &mut CraftApp),
) -> Response {
    let t = Tokens::get(ui.ctx());
    let on = enabled(app, id);
    let name = label.replace('\n', " ");
    let (main, arrow) = if big_btn {
        let w = big_width(ui, label, 0.0);
        // The arrow lives in a strip under the label; the main part is everything above.
        let lines = label.split('\n').count() as f32;
        let main_h = BIG_LABEL_Y + BIG_LINE_H * lines;
        let (r, resp) = ui.allocate_exact_size(vec2(w, CONTENT_H), Sense::hover());
        let main_r = Rect::from_min_size(r.min, vec2(w, main_h));
        let arrow_r = Rect::from_min_max(pos2(r.min.x, r.min.y + main_h), r.max);
        let main = ui.interact(main_r, resp.id.with("main"), Sense::click());
        let arrow = ui.interact(arrow_r, resp.id.with("arrow"), Sense::click());
        bg(ui, main_r, &main, false, on, &t);
        bg(ui, arrow_r, &arrow, false, true, &t);
        paint_big(ui, r, icon, label, on, false, &t);
        let glyph = Rect::from_center_size(pos2(r.center().x, arrow_r.center().y - 1.0), vec2(ARROW_GLYPH + 3.0, ARROW_GLYPH + 3.0));
        icons::paint(ui.painter(), glyph, "dropdown", t.icon, t.accent);
        (main, arrow)
    } else {
        let w = small_width(ui, Some(label));
        let (r, main) = ui.allocate_exact_size(vec2(w, SMALL_H), Sense::click());
        let (ar, arrow) = ui.allocate_exact_size(vec2(ARROW_W, SMALL_H), Sense::click());
        bg(ui, r, &main, false, on, &t);
        bg(ui, ar, &arrow, false, true, &t);
        paint_small(ui, r, icon, Some(label), on, &t);
        icons::paint(ui.painter(), Rect::from_center_size(ar.center(), vec2(ARROW_GLYPH, ARROW_GLYPH)), "dropdown", t.icon, t.accent);
        (main, arrow)
    };
    main.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, &name));
    arrow.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{name} options")));
    let main = tooltip(app, main, &name, id);
    run_if(app, &main, on, id, params);
    let arrow = arrow.on_hover_text(format!("{name} options"));
    egui::Popup::menu(&arrow).show(|ui| {
        ui.set_min_width(MENU_MIN_W);
        begin_menu(ui, &name);
        menu(ui, app);
    });
    main
}

/// A button that only opens a menu (icon, label and a dropdown arrow). `tip` is its name.
pub fn menu_button(
    ui: &mut Ui,
    app: &mut CraftApp,
    icon: &str,
    label: &str,
    tip: &str,
    big_btn: bool,
    menu: impl FnOnce(&mut Ui, &mut CraftApp),
) -> Response {
    menu_button_if(ui, app, icon, label, tip, big_btn, true, menu)
}

/// [`menu_button`] that can be switched off: while `on` is false it draws dimmed and neither
/// reacts to the pointer nor opens its menu (a menu whose rows need a selection).
pub fn menu_button_if(
    ui: &mut Ui,
    app: &mut CraftApp,
    icon: &str,
    label: &str,
    tip: &str,
    big_btn: bool,
    on: bool,
    menu: impl FnOnce(&mut Ui, &mut CraftApp),
) -> Response {
    let t = Tokens::get(ui.ctx());
    let sense = if on { Sense::click() } else { Sense::hover() };
    let resp = if big_btn {
        let (r, resp) = ui.allocate_exact_size(vec2(big_width(ui, label, ARROW_GLYPH + 4.0), CONTENT_H), sense);
        bg(ui, r, &resp, false, on, &t);
        let last = paint_big(ui, r, icon, label, on, true, &t);
        // The arrow follows the last line, which moved left to make room for it.
        icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(last.max.x + 6.0, last.center().y), vec2(ARROW_GLYPH, ARROW_GLYPH)),
            "dropdown",
            if on { t.icon } else { t.text_disabled },
            if on { t.accent } else { t.text_disabled },
        );
        resp
    } else {
        let (r, resp) = ui.allocate_exact_size(vec2(small_width(ui, Some(label)) + ARROW_W, SMALL_H), sense);
        bg(ui, r, &resp, false, on, &t);
        paint_small(ui, r, icon, Some(label), on, &t);
        icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(r.max.x - ARROW_W / 2.0, r.center().y), vec2(ARROW_GLYPH, ARROW_GLYPH)),
            "dropdown",
            if on { t.icon } else { t.text_disabled },
            if on { t.accent } else { t.text_disabled },
        );
        resp
    };
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, on, tip));
    let resp = resp.on_hover_text(tip);
    if on {
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(MENU_MIN_W);
            begin_menu(ui, tip);
            menu(ui, app);
        });
    }
    resp
}

/// A menu item that runs a command and closes the menu. Shows the shortcut; disabled when the
/// command is. `checked` is the item's current-choice state: `None` for an item with no current
/// value, `Some(on)` for one that has (the row then reserves a check column at the right, after
/// the shortcut, and paints the mark there while `on`; its accessibility info says selected).
pub fn menu_item(ui: &mut Ui, app: &mut CraftApp, label: &str, id: &str, params: Value, checked: Option<bool>) {
    let sc = shortcut_text(app, id);
    let on = enabled(app, id);
    let mut button = egui::Button::new(label).min_size(vec2(MENU_MIN_W, 0.0));
    button = match (sc.is_empty(), checked.is_some()) {
        (true, false) => button,
        (false, false) => button.shortcut_text(sc),
        (true, true) => button.right_text(check_column()),
        (false, true) => button.shortcut_text((sc, check_column())),
    };
    let out = ui.add_enabled_ui(on, |ui| button.atom_ui(ui)).inner;
    let resp = out.response.clone();
    paint_check_column(ui, &out, checked == Some(true), on);
    if let Some(c) = checked {
        resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, on, c, label));
    }
    record_row(ui, json!({"label": label, "id": id, "enabled": on, "checked": checked}));
    if resp.clicked() {
        let _ = app.run(id, params);
        ui.close();
    }
}

/// A ribbon-style text field with a dropdown list of `items`; `current` shows when the field is
/// not being edited. Returns a value the user chose from the list or typed and confirmed with
/// Enter. `id` keeps the edit state apart from other combos.
///
/// The current item in the list carries the same check mark as a checked menu row.
pub fn combo(ui: &mut Ui, id: &str, width: f32, current: &str, items: &[String]) -> Option<String> {
    let mut out = None;
    let edit_id = ui.id().with(("combo_text", id));
    let mut text = ui.data_mut(|d| d.get_temp::<String>(edit_id)).unwrap_or_else(|| current.to_string());
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let te = egui::TextEdit::singleline(&mut text).desired_width(width - ARROW_W - 8.0).font(regular(12.0)).margin(vec2(4.0, 3.0));
        let resp = ui.add(te);
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(edit_id, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(edit_id));
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            out = Some(text.clone());
        }
        let (ar, aresp) = ui.allocate_exact_size(vec2(ARROW_W + 4.0, resp.rect.height()), Sense::click());
        aresp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{id} options")));
        ui.painter().rect(ar, 0.0, if aresp.hovered() { t.hover } else { t.input }, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
        icons::paint(ui.painter(), Rect::from_center_size(ar.center(), vec2(ARROW_GLYPH, ARROW_GLYPH)), "dropdown", t.icon, t.accent);
        egui::Popup::menu(&aresp).show(|ui| {
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                ui.set_min_width(width + 40.0);
                begin_menu(ui, id);
                for it in items {
                    let on = it == current;
                    let button = egui::Button::new(it.as_str()).min_size(vec2(width + 40.0, 0.0)).right_text(check_column());
                    let row = button.atom_ui(ui);
                    paint_check_column(ui, &row, on, true);
                    row.response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, on, it));
                    record_row(ui, json!({"label": it, "id": Value::Null, "enabled": true, "checked": on}));
                    if row.response.clicked() {
                        out = Some(it.clone());
                        ui.close();
                    }
                }
            });
        });
    });
    out
}

/// A ribbon group: content, a centred label below and, when `rule` is set, a dividing rule on
/// the right. Returns the group's rect.
pub fn group(ui: &mut Ui, title: &str, rule: bool, app: &mut CraftApp, add: impl FnOnce(&mut Ui, &mut CraftApp)) -> Rect {
    let t = Tokens::get(ui.ctx());
    let label_w = text_width(ui, title, regular(11.0));
    let start = ui.cursor().min;
    let inner = ui.vertical(|ui| {
        ui.set_min_width(label_w + 10.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), CONTENT_H), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            add(ui, app);
        });
    });
    let r = Rect::from_min_max(start, pos2(inner.response.rect.max.x, start.y + CONTENT_H + LABEL_H));
    ui.painter().text(pos2(r.center().x, r.max.y - 8.0), Align2::CENTER_CENTER, title, regular(11.0), t.group_label);
    if rule {
        ui.add_space(GROUP_GAP - 1.0);
        let x = ui.cursor().min.x;
        ui.painter().line_segment([pos2(x, start.y + 4.0), pos2(x, start.y + CONTENT_H + LABEL_H - 4.0)], Stroke::new(1.0, t.border));
        ui.add_space(GROUP_GAP);
    }
    r
}

/// A vertical stack of small controls inside a group.
pub fn stack(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        add(ui);
    });
}

/// A plain icon button (title bar, status bar). `tip` is its name. Returns true when clicked.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str, size: f32, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size + 8.0, size + 6.0), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tip));
    if resp.hovered() && enabled {
        ui.painter().rect_filled(r, BUTTON_RADIUS as f32, t.hover);
    }
    let c = if enabled { t.icon } else { t.text_disabled };
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(size, size)), icon, c, if enabled { t.accent } else { c });
    resp.on_hover_text(tip).clicked() && enabled
}

/// A horizontal slider `width` points wide that reads on every background: egui paints the rail
/// in the text-field colour, which disappears on a white dialog and on the dark status bar, and
/// an unbounded slider in a dialog stretches the dialog towards the window's height, so the rail
/// takes the strong border colour and the slider gets one row.
pub fn slider(ui: &mut Ui, slider: egui::Slider<'_>, width: f32) -> Response {
    let t = Tokens::get(ui.ctx());
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = t.border_strong;
        ui.allocate_ui_with_layout(vec2(width, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| ui.add(slider)).inner
    })
    .inner
}

/// A filled accent button for the primary action of a dialog.
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Button::new(egui::RichText::new(label).color(t.on_accent)).fill(t.accent).min_size(vec2(72.0, 24.0)))
}

#[cfg(test)]
mod tests {
    use super::big_label;

    #[test]
    fn big_labels_break_long_names_at_the_middle_space() {
        assert_eq!(big_label("Undo"), "Undo");
        assert_eq!(big_label("Zoom In"), "Zoom In");
        assert_eq!(big_label("Select All"), "Select\nAll");
        assert_eq!(big_label("Navigation Pane"), "Navigation\nPane");
        assert_eq!(big_label("Search Commands"), "Search\nCommands");
        assert_eq!(big_label("Preferences"), "Preferences");
    }
}
