//! The menu tree, shared by the in-window menu bar (egui) and the desktop app's native Apple
//! macOS menu bar (muda, built by the host from [`menu_tree`]). Items are command ids, so both bars
//! show the same labels, shortcuts and enablement, and the control channel can press any of them.
//!
//! What the native builder needs:
//! - [`menu_tree`] for the structure, [`item_label`] for each item's text (dynamic for Undo and
//!   Redo), [`entry`] for shortcut text, enablement and check state by id;
//! - [`structure_key`] to know when to rebuild (it changes when the tree's shape or any label
//!   changes: a recent file, a different undo step); between rebuilds only enablement and check
//!   marks move;
//! - [`PLACEHOLDERS`]: ids that exist as commands so the palette and the control channel see
//!   them, but are shown disabled in menus because this craft has nothing behind them yet.
//!   `window.minimize` and `window.zoom` carry [`MenuRole::Window`] so the native layer puts the
//!   system items in their place.
//!
//! The in-window bar ([`menu_bar`]) is drawn by hand so it behaves like a menu bar rather than a
//! row of popup buttons: one click opens a menu, moving to another title switches to it, Escape
//! or a click anywhere else closes, arrows and Enter walk the items. Its colours all come from
//! [`crate::theme::Tokens`].

use std::hash::{DefaultHasher, Hash, Hasher};

use egui::{Align2, Id, Key, Modifiers, Order, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::theme::{Tokens, regular};
use crate::{CraftApp, panes, widgets};

/// One entry of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A command: label, id, params.
    Cmd(&'static str, &'static str, Value),
    /// A submenu.
    Sub(&'static str, Vec<Item>),
    Sep,
    /// A non-interactive heading (recent files, say).
    Header(&'static str),
    /// A command shown with a check mark when [`entry`] says it is checked.
    Check(&'static str, &'static str, Value),
    /// A recent file: opens it. Owned, because the path comes from the preferences.
    Recent(String),
}

/// What a top-level menu is for; the native bar places `App`, `Window` and `Help` specially.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuRole {
    Normal,
    App,
    Window,
    Help,
}

/// A top-level menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: &'static str,
    pub role: MenuRole,
    pub items: Vec<Item>,
}

/// How a command should look in a menu right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    /// Platform shortcut text (`Cmd S` glyphs or `Ctrl+S`); empty when there is none.
    pub shortcut: String,
    pub enabled: bool,
    pub checked: bool,
}

/// Commands that are listed (so every front sees them) but have nothing behind them yet: menus
/// show them disabled. A craft removes an id from here when it implements the command.
pub const PLACEHOLDERS: &[&str] = &["window.minimize", "window.zoom"];

/// The label of the Exit item on platforms whose File menu has one.
const EXIT_LABEL: &str = if cfg!(target_os = "windows") { "Exit" } else { "Quit" };

/// The menus: File, Edit, View, Window, Help (the app menu is the host's, on macOS).
pub fn menu_tree(app: &CraftApp) -> Vec<Menu> {
    let mut recent: Vec<Item> = app.ui.recent.iter().map(|p| Item::Recent(p.clone())).collect();
    if recent.is_empty() {
        recent.push(Item::Header("No recent files"));
    } else {
        recent.push(Item::Sep);
        recent.push(Item::Cmd("Clear Recent", "ui.clearRecent", json!({})));
    }
    let mut file = vec![
        Item::Cmd("New", "file.new", json!({})),
        Item::Cmd("New Document...", "ui.gallery", json!({"mode": "new"})),
        Item::Cmd("New Window", "ui.newWindow", json!({})),
        Item::Cmd("Open...", "file.open", json!({})),
        Item::Sub("Open Recent", recent),
        Item::Sep,
        Item::Cmd("Save", "file.save", json!({})),
        Item::Cmd("Save As...", "file.saveAs", json!({})),
        Item::Cmd("Export...", "file.export", json!({})),
        Item::Cmd("Import...", "file.import", json!({})),
        Item::Cmd("Close", "file.close", json!({})),
        Item::Sep,
        Item::Cmd("Print...", "ui.print", json!({})),
    ];
    if !cfg!(target_os = "macos") {
        // On macOS these live in the app menu.
        file.extend([Item::Sep, Item::Cmd("Preferences...", "app.preferences", json!({})), Item::Cmd(EXIT_LABEL, "app.quit", json!({}))]);
    }
    let mut view = vec![
        Item::Cmd("Zoom In", "view.zoomIn", json!({})),
        Item::Cmd("Zoom Out", "view.zoomOut", json!({})),
        Item::Cmd("Actual Size", "view.zoom100", json!({})),
        Item::Cmd("Fit to Window", "view.zoomFit", json!({})),
        Item::Cmd("Zoom...", "view.zoom", json!({})),
        Item::Cmd("Reset Pan", "view.resetPan", json!({})),
        Item::Sep,
        Item::Check("Dark Mode", "view.dark", json!({})),
        Item::Check("Collapse the Ribbon", "ui.collapseRibbon", json!({})),
    ];
    if cfg!(target_os = "macos") {
        view.push(Item::Check("Menu Bar in Window", "ui.inWindowMenus", json!({})));
    }
    view.extend([
        Item::Sep,
        Item::Header("Panes"),
        Item::Check("Navigation Pane", "view.navigationPane", json!({})),
        Item::Check("Properties Pane", "view.propertiesPane", json!({})),
    ]);
    // The panes, one check item each (`ui.pane.<key>`), then the layout reset. The native bar
    // appends the system Minimize and Zoom and the list of open windows after these.
    let mut window = vec![
        Item::Cmd("New Window", "ui.newWindow", json!({})),
        Item::Sep,
        Item::Cmd("Minimize", "window.minimize", json!({})),
        Item::Cmd("Zoom", "window.zoom", json!({})),
        Item::Sep,
        Item::Header("Panes"),
    ];
    window.extend(panes::window_menu_items(app).into_iter().map(|(title, id, _)| Item::Check(title, id, json!({}))));
    window.extend([Item::Sep, Item::Cmd("Reset Panes", "ui.panesReset", json!({}))]);
    vec![
        Menu { title: "File", role: MenuRole::Normal, items: file },
        Menu {
            title: "Edit",
            role: MenuRole::Normal,
            items: vec![
                Item::Cmd("Undo", "edit.undo", json!({})),
                Item::Cmd("Redo", "edit.redo", json!({})),
                Item::Sep,
                Item::Cmd("Cut", "edit.cut", json!({})),
                Item::Cmd("Copy", "edit.copy", json!({})),
                Item::Cmd("Paste", "edit.paste", json!({})),
                Item::Cmd("Select All", "edit.selectAll", json!({})),
                Item::Sep,
                Item::Cmd("Find...", "ui.find", json!({})),
                Item::Sep,
                Item::Cmd("Valve...", "ui.valve", json!({})),
                Item::Cmd("Rebuild Symbols", "glyph.rebuild", json!({})),
                Item::Cmd("Renumber Fluid Power Components", "fluid.renumber", json!({})),
                Item::Cmd("Trace Pressure", "fluid.trace", json!({})),
            ],
        },
        Menu { title: "View", role: MenuRole::Normal, items: view },
        Menu { title: "Window", role: MenuRole::Window, items: window },
        Menu {
            title: "Help",
            role: MenuRole::Help,
            items: vec![
                Item::Cmd("Keyboard Shortcuts...", "ui.shortcuts", json!({})),
                Item::Cmd("Command Palette", "ui.commands", json!({})),
                Item::Cmd("Examples...", "ui.gallery", json!({"mode": "examples"})),
                Item::Sep,
                Item::Cmd("About VizCraft", "help.about", json!({})),
            ],
        },
    ]
}

/// The text an item shows: its own label, except Undo and Redo, which name the step
/// (`Undo Typing`). Recent files show their file name.
pub fn item_label(app: &CraftApp, item: &Item) -> String {
    match item {
        Item::Cmd(label, id, _) | Item::Check(label, id, _) => dynamic_label(app, id).unwrap_or_else(|| (*label).to_string()),
        Item::Sub(label, _) | Item::Header(label) => (*label).to_string(),
        Item::Sep => String::new(),
        Item::Recent(path) => file_name(path),
    }
}

/// Labels that change with the session: Undo and Redo name their step.
fn dynamic_label(app: &CraftApp, id: &str) -> Option<String> {
    match id {
        "edit.undo" => Some(app.session.undo_label().map_or_else(|| "Undo".to_string(), |l| format!("Undo {l}"))),
        "edit.redo" => Some(app.session.redo_label().map_or_else(|| "Redo".to_string(), |l| format!("Redo {l}"))),
        _ => None,
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

/// The first item in the tree that runs `id`.
fn find_item(items: &[Item], id: &str) -> Option<Item> {
    for it in items {
        match it {
            Item::Cmd(_, i, _) | Item::Check(_, i, _) if *i == id => return Some(it.clone()),
            Item::Sub(_, sub) => {
                if let Some(found) = find_item(sub, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// How to show the command `id` right now: its menu label (the tree's, dynamic for Undo and
/// Redo, else the registry's), shortcut, enablement and check state.
pub fn entry(app: &CraftApp, id: &str) -> Entry {
    let label = menu_tree(app)
        .iter()
        .find_map(|m| find_item(&m.items, id))
        .map(|it| item_label(app, &it))
        .or_else(|| dynamic_label(app, id))
        .unwrap_or_else(|| widgets::label_of(app, id).to_string());
    entry_with_label(app, id, label)
}

fn entry_with_label(app: &CraftApp, id: &str, label: String) -> Entry {
    let checked = match id {
        "view.navigationPane" => app.session.view.nav_pane,
        "view.propertiesPane" => app.session.view.props_pane,
        "view.dark" => app.session.view.dark_mode,
        "ui.collapseRibbon" => app.ui.ribbon_collapsed,
        "ui.inWindowMenus" => app.ui.in_window_menus,
        _ => id.strip_prefix("ui.pane.").is_some_and(|key| app.ui.panes.is_open(key)),
    };
    let enabled = !PLACEHOLDERS.contains(&id) && widgets::enabled(app, id);
    Entry { label, shortcut: widgets::shortcut_text(app, id), enabled, checked }
}

/// [`entry`] for a tree item (no tree search); `None` for separators, headers and submenus.
pub fn item_entry(app: &CraftApp, item: &Item) -> Option<Entry> {
    match item {
        Item::Cmd(_, id, _) | Item::Check(_, id, _) => Some(entry_with_label(app, id, item_label(app, item))),
        Item::Recent(path) => Some(Entry { label: file_name(path), shortcut: String::new(), enabled: true, checked: false }),
        _ => None,
    }
}

/// What running an item does: the command and params, or `None` for items that run nothing.
pub fn item_command(item: &Item) -> Option<(String, Value)> {
    match item {
        Item::Cmd(_, id, p) | Item::Check(_, id, p) => Some(((*id).to_string(), p.clone())),
        Item::Recent(path) => Some(("file.open".to_string(), json!({"path": path}))),
        _ => None,
    }
}

/// A hash of everything that changes the shape or the text of the menus (titles, labels, ids,
/// shortcuts, recent files), but not their enablement or check marks. A native menu rebuilds
/// when it changes and only updates item states otherwise.
pub fn structure_key(app: &CraftApp) -> u64 {
    fn walk(app: &CraftApp, h: &mut DefaultHasher, items: &[Item]) {
        for it in items {
            match it {
                Item::Cmd(_, id, _) | Item::Check(_, id, _) => {
                    0u8.hash(h);
                    id.hash(h);
                    item_label(app, it).hash(h);
                    widgets::shortcut_of(app, id).hash(h);
                }
                Item::Sub(label, sub) => {
                    1u8.hash(h);
                    label.hash(h);
                    walk(app, h, sub);
                }
                Item::Sep => 2u8.hash(h),
                Item::Header(label) => {
                    3u8.hash(h);
                    label.hash(h);
                }
                Item::Recent(path) => {
                    4u8.hash(h);
                    path.hash(h);
                }
            }
        }
    }
    let mut h = DefaultHasher::new();
    for m in menu_tree(app) {
        m.title.hash(&mut h);
        m.role.hash(&mut h);
        walk(app, &mut h, &m.items);
        u8::MAX.hash(&mut h);
    }
    h.finish()
}

/// Whether the menu bar is drawn inside the window: always off macOS (nothing else has one),
/// and on macOS when the preference says so.
pub fn in_window_menus(app: &CraftApp) -> bool {
    !cfg!(target_os = "macos") || app.ui.in_window_menus
}

/// Height of the in-window menu bar.
pub const BAR_H: f32 = 26.0;
/// Height of one menu row.
const ROW_H: f32 = 24.0;
/// Narrowest a menu popup gets.
const MIN_MENU_W: f32 = 230.0;

/// What is open in the in-window bar; kept in egui's frame-to-frame memory.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BarState {
    /// Index of the open top-level menu.
    open: Option<usize>,
    /// Highlighted row of the open menu (keyboard or hover).
    hi: Option<usize>,
    /// Row of the open menu whose submenu is showing.
    sub: Option<usize>,
    /// Highlighted row of the submenu.
    sub_hi: Option<usize>,
}

fn state_id() -> Id {
    Id::new("craft_menu_bar_state")
}

fn load(ctx: &egui::Context) -> BarState {
    ctx.data(|d| d.get_temp(state_id())).unwrap_or_default()
}

fn store(ctx: &egui::Context, s: BarState) {
    ctx.data_mut(|d| d.insert_temp(state_id(), s));
}

/// Whether an in-window menu is open (shortcuts stand down while it is).
pub fn is_open(ctx: &egui::Context) -> bool {
    load(ctx).open.is_some()
}

/// Close any open in-window menu.
pub fn close(ctx: &egui::Context) {
    store(ctx, BarState::default());
}

/// Open the top-level menu whose title is `title` (tests and automation).
pub fn open_menu(app: &CraftApp, ctx: &egui::Context, title: &str) -> bool {
    match menu_tree(app).iter().position(|m| m.title == title) {
        Some(i) => {
            store(ctx, BarState { open: Some(i), ..BarState::default() });
            ctx.request_repaint();
            true
        }
        None => false,
    }
}

/// One drawn row of a popup: where it is and how the pointer met it this frame.
struct Row {
    rect: Rect,
    index: usize,
    hovered: bool,
    clicked: bool,
}

/// The in-window menu bar, from [`menu_tree`].
pub fn menu_bar(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let tree = menu_tree(app);
    let mut st = load(&ctx);
    if let Some(i) = st.open
        && i >= tree.len()
    {
        st = BarState::default();
    }
    let mut title_rects: Vec<Rect> = Vec::with_capacity(tree.len());
    let mut next = st;
    egui::Panel::top("menu_bar").exact_size(BAR_H).frame(egui::Frame::NONE.fill(t.tab_strip).inner_margin(egui::Margin::symmetric(6, 0))).show(
        ui,
        |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                for (i, menu) in tree.iter().enumerate() {
                    let w = text_width(ui, menu.title, regular(12.5)) + 20.0;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, BAR_H - 4.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, menu.title));
                    title_rects.push(r);
                    let open = st.open == Some(i);
                    if open {
                        ui.painter().rect_filled(r, 4.0, t.pressed);
                    } else if resp.hovered() && st.open.is_none() {
                        ui.painter().rect_filled(r, 4.0, t.hover);
                    }
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, menu.title, regular(12.5), t.text);
                    // With no menu open the title is an ordinary button. While one is open, the
                    // backdrop below takes the pointer and the titles are hit-tested by hand.
                    if st.open.is_none() && resp.clicked() {
                        next = BarState { open: Some(i), ..BarState::default() };
                    }
                }
            });
        },
    );
    let Some(open) = st.open else {
        if next != st {
            store(&ctx, next);
            ctx.request_repaint();
        }
        return;
    };
    let Some(menu) = tree.get(open) else {
        close(&ctx);
        return;
    };
    let Some(title_rect) = title_rects.get(open).copied() else {
        close(&ctx);
        return;
    };

    // The backdrop: a full-window layer under the popups that swallows the click that closes the
    // menu, so the ribbon under the pointer does not also act on it.
    let screen = ctx.content_rect();
    let backdrop = egui::Area::new(Id::new("craft_menu_backdrop"))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(&ctx, |ui| ui.allocate_rect(screen, Sense::click()));
    let pointer = ctx.pointer_hover_pos();
    let moving = ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
    let pressed_outside = backdrop.inner.clicked() || backdrop.inner.secondary_clicked();

    // Hovering another title switches to it; clicking the open title closes.
    if let Some(p) = pointer
        && let Some(i) = title_rects.iter().position(|r| r.contains(p))
    {
        if i != open && moving {
            next = BarState { open: Some(i), ..BarState::default() };
        } else if i == open && pressed_outside {
            next = BarState::default();
        }
    }

    // Keys: Escape closes (the submenu first), arrows move, Enter runs, Left/Right change menus.
    let mut run: Option<(String, Value)> = None;
    let keys = [Key::Escape, Key::ArrowDown, Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight, Key::Enter, Key::Space];
    let pressed: Vec<Key> = ctx.input_mut(|i| keys.iter().copied().filter(|k| i.consume_key(Modifiers::NONE, *k)).collect());
    let sub_items: Option<&Vec<Item>> = st.sub.and_then(|s| match menu.items.get(s) {
        Some(Item::Sub(_, items)) => Some(items),
        _ => None,
    });
    for k in pressed {
        match k {
            Key::Escape => {
                next = if st.sub.is_some() { BarState { sub: None, sub_hi: None, ..st } } else { BarState::default() };
            }
            Key::ArrowDown | Key::ArrowUp => {
                let down = k == Key::ArrowDown;
                match sub_items {
                    Some(items) => next.sub_hi = step(items, st.sub_hi, down),
                    None => next.hi = step(&menu.items, st.hi, down),
                }
            }
            Key::ArrowRight => {
                if sub_items.is_none() && matches!(st.hi.and_then(|h| menu.items.get(h)), Some(Item::Sub(..))) {
                    next.sub = st.hi;
                    next.sub_hi = None;
                } else {
                    next = BarState { open: Some((open + 1) % tree.len()), ..BarState::default() };
                }
            }
            Key::ArrowLeft => {
                if sub_items.is_some() {
                    next.sub = None;
                    next.sub_hi = None;
                } else {
                    next = BarState { open: Some((open + tree.len() - 1) % tree.len()), ..BarState::default() };
                }
            }
            Key::Enter | Key::Space => {
                let target = match sub_items {
                    Some(items) => st.sub_hi.and_then(|h| items.get(h)),
                    None => st.hi.and_then(|h| menu.items.get(h)),
                };
                match target {
                    Some(Item::Sub(..)) => {
                        next.sub = st.hi;
                        next.sub_hi = None;
                    }
                    Some(it) if item_entry(app, it).is_some_and(|e| e.enabled) => run = item_command(it),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // Keyboard highlight and hover highlight share one field; a resting pointer leaves the
    // keyboard's choice alone, a moving one takes over.
    let hover_takes = moving;

    // The top-level popup.
    let popup_id = Id::new("craft_menu_popup");
    let mut hovered_row: Option<usize> = None;
    let mut clicked_row: Option<usize> = None;
    let mut sub_anchor: Option<Rect> = None;
    let popup = popup_area(&ctx, popup_id, title_rect.left_bottom() + vec2(0.0, 2.0), |ui| {
        for row in draw_items(app, ui, &menu.items, st.hi, &t) {
            if row.hovered && hover_takes {
                hovered_row = Some(row.index);
            }
            if row.clicked {
                clicked_row = Some(row.index);
            }
            if st.sub == Some(row.index) {
                sub_anchor = Some(row.rect);
            }
        }
    });
    if let Some(h) = hovered_row {
        next.hi = Some(h);
        // Hovering a submenu row opens it; hovering any other row closes the submenu.
        match menu.items.get(h) {
            Some(Item::Sub(..)) => {
                if st.sub != Some(h) {
                    next.sub = Some(h);
                    next.sub_hi = None;
                }
            }
            _ => {
                next.sub = None;
                next.sub_hi = None;
            }
        }
    }
    if let Some(c) = clicked_row {
        match menu.items.get(c) {
            Some(Item::Sub(..)) => {
                next.sub = Some(c);
                next.sub_hi = None;
            }
            Some(it) if item_entry(app, it).is_some_and(|e| e.enabled) => run = item_command(it),
            _ => {}
        }
    }

    // The submenu, to the right of its row.
    let mut sub_rect: Option<Rect> = None;
    if let (Some(items), Some(anchor)) = (sub_items, sub_anchor) {
        let sub_id = Id::new("craft_menu_submenu");
        let mut hovered_sub: Option<usize> = None;
        let mut clicked_sub: Option<usize> = None;
        let at = pos2(anchor.max.x - 2.0, anchor.min.y - 5.0);
        let resp = popup_area(&ctx, sub_id, at, |ui| {
            for row in draw_items(app, ui, items, st.sub_hi, &t) {
                if row.hovered && hover_takes {
                    hovered_sub = Some(row.index);
                }
                if row.clicked {
                    clicked_sub = Some(row.index);
                }
            }
        });
        sub_rect = Some(resp);
        if let Some(h) = hovered_sub {
            next.sub_hi = Some(h);
            // The pointer is in the submenu: keep its parent row highlighted.
            next.hi = st.sub;
            next.sub = st.sub;
        }
        if let Some(c) = clicked_sub
            && let Some(it) = items.get(c)
            && item_entry(app, it).is_some_and(|e| e.enabled)
        {
            run = item_command(it);
        }
    }

    // A click that landed on nothing of ours closes the menu.
    if pressed_outside
        && let Some(p) = ctx.pointer_interact_pos()
        && !popup.contains(p)
        && !sub_rect.is_some_and(|r| r.contains(p))
        && !title_rects.iter().any(|r| r.contains(p))
    {
        next = BarState::default();
    }

    if let Some((id, params)) = run {
        next = BarState::default();
        store(&ctx, next);
        run_item(app, &ctx, &id, params);
        ctx.request_repaint();
        return;
    }
    if next != st {
        ctx.request_repaint();
    }
    store(&ctx, next);
}

/// Run a menu item's command the way a person expects: New opens a fresh window where the host
/// has several (and a fresh document otherwise), and Copy and Cut hand their text to the system
/// clipboard. Shortcuts and the in-window bar both come through here.
pub fn run_item(app: &mut CraftApp, ctx: &egui::Context, id: &str, params: Value) {
    if id == "file.new" {
        app.new_window(params.get("template").and_then(Value::as_str).unwrap_or("blank"));
        return;
    }
    if let Ok(v) = app.run(id, params)
        && matches!(id, "edit.copy" | "edit.cut")
        && let Some(text) = v.get("text").and_then(Value::as_str)
        && !text.is_empty()
    {
        ctx.copy_text(text.to_string());
    }
}

/// The next selectable row after `from` in `items` (wrapping), skipping separators and headers.
fn step(items: &[Item], from: Option<usize>, down: bool) -> Option<usize> {
    let n = items.len();
    if n == 0 {
        return None;
    }
    let selectable = |i: usize| !matches!(items.get(i), Some(Item::Sep | Item::Header(_)) | None);
    let mut i = match (from, down) {
        (Some(f), true) => (f + 1) % n,
        (Some(f), false) => (f + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    for _ in 0..n {
        if selectable(i) {
            return Some(i);
        }
        i = if down { (i + 1) % n } else { (i + n - 1) % n };
    }
    None
}

/// A popup frame at `at`, on top of everything. Returns its rectangle.
fn popup_area(ctx: &egui::Context, id: Id, at: Pos2, add: impl FnOnce(&mut Ui)) -> Rect {
    let t = Tokens::get(ctx);
    let shadow = ctx.global_style().visuals.popup_shadow;
    let frame = egui::Frame::NONE
        .fill(t.menu)
        .stroke(Stroke::new(1.0, t.border_strong))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(4))
        .shadow(shadow);
    // No fade: a menu appears at once, like a native one.
    let resp = egui::Area::new(id).order(Order::Foreground).fixed_pos(at).interactable(true).constrain(true).fade_in(false).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_min_width(MIN_MENU_W);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            add(ui);
        });
    });
    ctx.move_to_top(resp.response.layer_id);
    resp.response.rect
}

fn text_width(ui: &Ui, s: &str, font: egui::FontId) -> f32 {
    ui.ctx().fonts_mut(|f| f.layout_no_wrap(s.to_string(), font, egui::Color32::WHITE).size().x)
}

/// Draw the rows of one popup; returns the interactive rows and their rectangles.
fn draw_items(app: &CraftApp, ui: &mut Ui, items: &[Item], hi: Option<usize>, t: &Tokens) -> Vec<Row> {
    // Wide enough for the longest label plus the longest shortcut.
    let font = regular(12.5);
    let mut w: f32 = MIN_MENU_W;
    for it in items {
        if let Some(e) = item_entry(app, it) {
            let lw = text_width(ui, &e.label, font.clone());
            let sw = if e.shortcut.is_empty() { 0.0 } else { text_width(ui, &e.shortcut, font.clone()) + 24.0 };
            w = w.max(28.0 + lw + sw + 16.0);
        } else if let Item::Sub(label, _) = it {
            w = w.max(28.0 + text_width(ui, label, font.clone()) + 40.0);
        }
    }
    let mut rows = Vec::new();
    for (index, it) in items.iter().enumerate() {
        match it {
            Item::Sep => {
                let (r, _) = ui.allocate_exact_size(vec2(w, 7.0), Sense::hover());
                ui.painter().hline(r.min.x + 8.0..=r.max.x - 8.0, r.center().y, Stroke::new(1.0, t.border));
            }
            Item::Header(label) => {
                let (r, _) = ui.allocate_exact_size(vec2(w, 20.0), Sense::hover());
                ui.painter().text(pos2(r.min.x + 28.0, r.center().y), Align2::LEFT_CENTER, *label, regular(11.0), t.text_dim);
            }
            Item::Sub(label, _) => {
                let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *label));
                if hi == Some(index) {
                    ui.painter().rect_filled(r, 4.0, t.hover);
                }
                ui.painter().text(pos2(r.min.x + 28.0, r.center().y), Align2::LEFT_CENTER, *label, font.clone(), t.text);
                chevron(ui, pos2(r.max.x - 14.0, r.center().y), t.text_dim);
                rows.push(Row { rect: r, index, hovered: resp.hovered(), clicked: resp.clicked() });
            }
            _ => {
                let Some(e) = item_entry(app, it) else { continue };
                let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, e.enabled, e.checked, e.label.as_str()));
                if hi == Some(index) && e.enabled {
                    ui.painter().rect_filled(r, 4.0, t.hover);
                }
                let color = if e.enabled { t.text } else { t.text_disabled };
                if e.checked {
                    widgets::check_mark(ui, pos2(r.min.x + 14.0, r.center().y), if e.enabled { t.accent } else { t.text_disabled });
                }
                ui.painter().text(pos2(r.min.x + 28.0, r.center().y), Align2::LEFT_CENTER, &e.label, font.clone(), color);
                if !e.shortcut.is_empty() {
                    let sc_color = if e.enabled { t.text_dim } else { t.text_disabled };
                    ui.painter().text(pos2(r.max.x - 12.0, r.center().y), Align2::RIGHT_CENTER, &e.shortcut, font.clone(), sc_color);
                }
                let (hovered, clicked) = (resp.hovered(), resp.clicked());
                if let Item::Recent(path) = it {
                    resp.on_hover_text(path);
                }
                rows.push(Row { rect: r, index, hovered, clicked });
            }
        }
    }
    rows
}

/// A right-pointing chevron for submenu rows.
fn chevron(ui: &Ui, c: Pos2, color: egui::Color32) {
    let s = Stroke::new(1.4, color);
    ui.painter().line_segment([c + vec2(-2.0, -4.0), c + vec2(2.0, 0.0)], s);
    ui.painter().line_segment([c + vec2(2.0, 0.0), c + vec2(-2.0, 4.0)], s);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> CraftApp {
        CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::Document::new()), crate::Services::default())
    }

    #[test]
    fn every_menu_item_is_a_known_command() {
        let app = app();
        fn walk(app: &CraftApp, items: &[Item]) {
            for i in items {
                match i {
                    Item::Cmd(_, id, _) | Item::Check(_, id, _) => {
                        assert!(
                            app.session.registry.get(id).is_some() || crate::UI_COMMANDS.iter().any(|(c, ..)| c == id),
                            "menu item {id} is not a command"
                        );
                        assert!(!entry(app, id).label.is_empty(), "{id} has no label");
                    }
                    Item::Sub(_, sub) => walk(app, sub),
                    _ => {}
                }
            }
        }
        for m in menu_tree(&app) {
            walk(&app, &m.items);
        }
        assert_eq!(entry(&app, "view.navigationPane").checked, app.session.view.nav_pane);
        assert!(!entry(&app, "edit.undo").enabled);
        assert!(entry(&app, "ui.print").enabled, "Print opens Print Preview");
        assert!(entry(&app, "ui.find").enabled, "Find opens the Find dialog");
        assert!(!entry(&app, "window.minimize").enabled, "placeholders show disabled");
    }

    #[test]
    fn labels_follow_the_tree_and_the_undo_stack() {
        let mut a = app();
        assert_eq!(entry(&a, "file.open").label, "Open...");
        assert_eq!(entry(&a, "edit.undo").label, "Undo");
        a.run("edit.setTitle", json!({"title": "hello"})).unwrap();
        let e = entry(&a, "edit.undo");
        assert!(e.enabled);
        assert!(e.label.starts_with("Undo "), "{}", e.label);
        // An id outside the tree still gets its registry label.
        assert_eq!(entry(&a, "edit.setTitle").label, "Document Title");
    }

    #[test]
    fn structure_key_tracks_shape_and_labels_only() {
        let mut a = app();
        let k0 = structure_key(&a);
        a.session.view.dark_mode = true;
        assert_eq!(structure_key(&a), k0, "a check mark is state, not structure");
        a.run("edit.setTitle", json!({"title": "hello"})).unwrap();
        let k1 = structure_key(&a);
        assert_ne!(k1, k0, "the undo label changed");
        a.ui.recent.push("/tmp/a.craft.json".into());
        assert_ne!(structure_key(&a), k1, "a recent file changed the tree");
    }

    #[test]
    fn window_menu_lists_every_pane_with_its_state() {
        let mut a = app();
        let window = menu_tree(&a).into_iter().find(|m| m.role == MenuRole::Window).unwrap();
        let checks: Vec<(&str, &str)> =
            window.items.iter().filter_map(|i| if let Item::Check(l, id, _) = i { Some((*l, *id)) } else { None }).collect();
        let expected: Vec<(&str, &str)> = panes::panes().iter().map(|p| (p.title, panes::command_id(p.key).unwrap())).collect();
        assert_eq!(checks, expected, "one check item per pane, in pane order");
        assert!(find_item(&window.items, "ui.panesReset").is_some(), "Reset Panes is on the Window menu");
        assert!(entry(&a, "ui.pane.history").checked, "open panes are checked");
        a.run("ui.pane.history", json!({})).unwrap();
        assert!(!entry(&a, "ui.pane.history").checked);
        let k = structure_key(&a);
        a.run("ui.pane.history", json!({})).unwrap();
        assert_eq!(structure_key(&a), k, "a pane's check mark is state, not structure");
    }

    #[test]
    fn stepping_skips_separators_and_wraps() {
        let items = vec![Item::Header("h"), Item::Cmd("a", "x", json!({})), Item::Sep, Item::Cmd("b", "y", json!({}))];
        assert_eq!(step(&items, None, true), Some(1));
        assert_eq!(step(&items, Some(1), true), Some(3));
        assert_eq!(step(&items, Some(3), true), Some(1));
        assert_eq!(step(&items, Some(1), false), Some(3));
        assert_eq!(step(&[], None, true), None);
    }
}
