//! Several document windows in one process (File > New Window, Cmd+N), so the app has one Dock
//! icon and one Quit. The first window is eframe's root viewport; each further window is an
//! immediate viewport with its own [`CraftApp`].
//!
//! Closing the root window while others are open hands the root to the next window (it moves to
//! where that window was), because closing eframe's root viewport ends the app. Quit asks each
//! window with unsaved changes in turn; Cancel on any of them stops quitting.

use egui::{Pos2, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportId, pos2, vec2};
use vizcraft_ui_egui::{CraftApp, ViewportCmdExt, WindowRequest};

/// How far a new window sits from the one in front, in points, right and down.
pub const CASCADE: f32 = 28.0;

/// A document window other than the root one.
struct Extra {
    id: ViewportId,
    app: CraftApp,
    builder: ViewportBuilder,
    /// Outer rectangle last frame (to place the root window when it takes this one's place).
    outer: Option<Rect>,
    closed: bool,
}

/// The windows besides the root one, which window is in front, and the state of a quit in progress.
#[derive(Default)]
pub struct Windows {
    extras: Vec<Extra>,
    next_id: u64,
    /// The window in front (menu commands go to it); `None` for the root.
    focused: Option<ViewportId>,
    quitting: bool,
    /// The window whose Save / Don't Save / Cancel prompt Quit opened last.
    asked: Option<ViewportId>,
}

/// Where a new window goes: cascaded from the front window's outer rectangle, with its size.
/// `None` when the front window's place is unknown (the builder's defaults are used).
pub fn cascade_from(front: Option<Rect>) -> Option<(Pos2, Vec2)> {
    front.map(|r| (r.min + vec2(CASCADE, CASCADE), r.size()))
}

/// Room the menu bar takes at the top of a monitor, in points; tiled windows start below it.
const MENU_BAR: f32 = 28.0;
/// Room left at the bottom of a monitor for a dock or task bar, in points.
const DOCK_ROOM: f32 = 8.0;
/// Smallest size a cascaded window gets, in points.
const MIN_CASCADE: Vec2 = vec2(640.0, 440.0);

/// Tile `n` windows over `area`: as square a grid as fits (two windows side by side, five in two
/// rows of three and two), the last row's windows wider to fill it. Empty for no windows.
pub fn tile(n: usize, area: Rect) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let cols = (1..=n).find(|c| c * c >= n).unwrap_or(1);
    let rows = n.div_ceil(cols);
    let height = area.height() / rows as f32;
    (0..n)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            let in_row = cols.min(n - row * cols);
            let width = area.width() / in_row as f32;
            Rect::from_min_size(pos2(area.min.x + col as f32 * width, area.min.y + row as f32 * height), vec2(width, height))
        })
        .collect()
}

/// Stack `n` windows over `area`, each [`CASCADE`] right and down of the one before, all the same
/// size (two thirds of the area, never below [`MIN_CASCADE`] or above the area). When the stair
/// would run off the area it starts again at the top left.
pub fn cascade(n: usize, area: Rect) -> Vec<Rect> {
    let size = (area.size() * (2.0 / 3.0)).max(MIN_CASCADE).min(area.size());
    let room = area.size() - size;
    let steps = (room.x.min(room.y) / CASCADE).floor().max(0.0) as usize + 1;
    (0..n).map(|i| Rect::from_min_size(area.min + vec2(CASCADE, CASCADE) * (i % steps) as f32, size)).collect()
}

/// The part of the monitor windows are placed on: below the menu bar, above the dock. The
/// monitor's own origin is not known to egui, so it is taken to be the nearest multiple of the
/// monitor's size below the front window's corner, which is right for monitors of one size laid
/// side by side and the primary monitor (the origin) in every case.
pub fn work_area(monitor: Vec2, front: Option<Rect>) -> Rect {
    let origin = match front {
        Some(r) if monitor.x > 0.0 && monitor.y > 0.0 => pos2((r.min.x / monitor.x).floor() * monitor.x, (r.min.y / monitor.y).floor() * monitor.y),
        _ => Pos2::ZERO,
    };
    Rect::from_min_size(origin + vec2(0.0, MENU_BAR), (monitor - vec2(0.0, MENU_BAR + DOCK_ROOM)).max(vec2(200.0, 200.0)))
}

/// One window as the quit stepper sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowState {
    pub id: ViewportId,
    /// Unsaved changes closing would lose.
    pub unsaved: bool,
    /// Its Save / Don't Save / Cancel prompt is showing.
    pub asking: bool,
}

/// What the quit stepper decided this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitStep {
    /// A prompt is open: wait for the answer.
    Wait,
    /// The last prompt was cancelled (or its Save As cancelled): stop quitting.
    Cancel,
    /// Open this window's prompt and bring it to the front.
    Ask(ViewportId),
    /// Nothing unsaved anywhere: end the app.
    Finish,
}

/// The pure core of quitting: given the open windows (root first) and the window asked last,
/// what to do next. Windows already closed are not passed in.
pub fn quit_step(windows: &[WindowState], asked: Option<ViewportId>) -> QuitStep {
    if windows.iter().any(|w| w.asking) {
        return QuitStep::Wait;
    }
    // The last prompt closed but its window still has unsaved changes: Cancel, or a cancelled
    // Save As. Stop quitting.
    if let Some(asked) = asked
        && windows.iter().any(|w| w.id == asked && w.unsaved)
    {
        return QuitStep::Cancel;
    }
    match windows.iter().find(|w| w.unsaved) {
        Some(w) => QuitStep::Ask(w.id),
        None => QuitStep::Finish,
    }
}

impl Windows {
    /// The app of the window in front (the root's when that is in front or unknown).
    pub fn front<'a>(&'a mut self, root: &'a mut CraftApp) -> &'a mut CraftApp {
        let focused = self.focused;
        match self.extras.iter_mut().find(|e| Some(e.id) == focused) {
            Some(e) => &mut e.app,
            None => root,
        }
    }

    /// The app of window `id` (`None` is the root's).
    pub fn app_of<'a>(&'a mut self, root: &'a mut CraftApp, id: Option<ViewportId>) -> &'a mut CraftApp {
        match id.and_then(|id| self.extras.iter_mut().find(|e| e.id == id)) {
            Some(e) => &mut e.app,
            None => root,
        }
    }

    /// Give every window the list of window titles, root first, for its Switch Windows menu.
    pub fn sync_titles(&mut self, root: &mut CraftApp) {
        let mut titles = vec![root.title_stem()];
        titles.extend(self.extras.iter().filter(|e| !e.closed).map(|e| e.app.title_stem()));
        for app in self.extras.iter_mut().map(|e| &mut e.app).chain(std::iter::once(root)) {
            if app.window_titles != titles {
                app.window_titles = titles.clone();
            }
        }
    }

    /// Window commands the windows made during the last frame, with the window each came from
    /// (`None` is the root).
    pub fn take_window_requests(&mut self) -> Vec<(ViewportId, WindowRequest)> {
        self.extras.iter_mut().filter_map(|e| e.app.window_request.take().map(|r| (e.id, r))).collect()
    }

    /// Tile or cascade every window on the monitor of the window in front.
    pub fn arrange(&mut self, ctx: &egui::Context, tile_them: bool) {
        let front = match self.extras.iter().find(|e| Some(e.id) == self.focused) {
            Some(e) => e.outer,
            None => ctx.input(|i| i.viewport().outer_rect),
        };
        let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1440.0, 900.0));
        let area = work_area(monitor, front);
        let ids: Vec<ViewportId> = std::iter::once(ViewportId::ROOT).chain(self.extras.iter().filter(|e| !e.closed).map(|e| e.id)).collect();
        let rects = if tile_them { tile(ids.len(), area) } else { cascade(ids.len(), area) };
        for (id, r) in ids.into_iter().zip(rects) {
            ctx.viewport_cmd_to(id, ViewportCommand::OuterPosition(r.min));
            ctx.viewport_cmd_to(id, ViewportCommand::InnerSize(r.size()));
            ctx.viewport_cmd_to(id, ViewportCommand::Focus);
        }
    }

    /// Bring window `index` (0 is the root) to the front; an index past the last does nothing.
    pub fn switch(&mut self, ctx: &egui::Context, index: usize) {
        let id = match index {
            0 => Some(ViewportId::ROOT),
            n => self.extras.iter().filter(|e| !e.closed).nth(n - 1).map(|e| e.id),
        };
        if let Some(id) = id {
            ctx.viewport_cmd_to(id, ViewportCommand::Focus);
            self.focused = (id != ViewportId::ROOT).then_some(id);
        }
    }

    /// Every window's app besides the root's (a preference that applies to the whole app is
    /// copied into each).
    pub fn apps_mut(&mut self) -> impl Iterator<Item = &mut CraftApp> {
        self.extras.iter_mut().map(|e| &mut e.app)
    }

    /// Open `app` in a new window shaped like `base`, cascaded from the window in front.
    pub fn open(&mut self, ctx: &egui::Context, mut app: CraftApp, base: ViewportBuilder) {
        app.multi_window = true;
        self.next_id += 1;
        let id = ViewportId::from_hash_of(("vizcraft-window", self.next_id));
        let front = match self.extras.iter().find(|e| Some(e.id) == self.focused) {
            Some(e) => e.outer,
            None => ctx.input(|i| i.viewport().outer_rect),
        };
        let mut builder = base;
        if let Some((pos, size)) = cascade_from(front) {
            builder = builder.with_position(pos).with_inner_size(size);
        }
        self.focused = Some(id);
        self.extras.push(Extra { id, app, builder, outer: None, closed: false });
    }

    /// Close the window in front the way its close button does, so unsaved changes are asked about.
    /// Only the native Apple macOS menu bar dispatches File > Close here; the in-window bar closes
    /// through `menus::run_item`, so on other platforms this would be dead code.
    #[cfg(target_os = "macos")]
    pub fn close_front(&self, ctx: &egui::Context) {
        let id = self.focused.filter(|f| self.extras.iter().any(|e| e.id == *f && !e.closed)).unwrap_or(ViewportId::ROOT);
        ctx.viewport_cmd_to(id, ViewportCommand::Close);
    }

    /// Requests the other windows made during the last frame: (new window templates, quit wanted).
    pub fn take_requests(&mut self) -> (Vec<String>, bool) {
        let mut new = Vec::new();
        let mut quit = false;
        for e in &mut self.extras {
            new.extend(e.app.new_window_request.take());
            quit |= std::mem::take(&mut e.app.quit_all_request);
        }
        (new, quit)
    }

    /// Start quitting: every window closes, asking about unsaved changes one window at a time.
    pub fn quit(&mut self) {
        self.quitting = true;
        self.asked = None;
    }

    /// Per-frame bookkeeping for the root window, after the root app's `logic`: focus, the root
    /// window closing, and quitting. Call from the root viewport.
    pub fn root_logic(&mut self, ctx: &egui::Context, root: &mut CraftApp) {
        if ctx.input(|i| i.viewport().focused) == Some(true) {
            self.focused = None;
        }
        if root.quit_requested {
            // Already confirmed: the control channel's `app.quit`, which ends the app. Otherwise
            // File > Close of a saved (or discarded) document: close this window.
            if root.quit_confirmed {
                self.extras.clear();
            }
            root.quit_requested = false;
            ctx.viewport_cmd(ViewportCommand::Close);
        }
        let root_closing = ctx.input(|i| i.viewport().close_requested()) && !root.has_unsaved();
        if root_closing && self.extras.iter().any(|e| !e.closed) {
            // Ending the root viewport would end the app: keep it, and (unless quitting) give it
            // the next window's document, where that window was.
            ctx.viewport_cmd(ViewportCommand::CancelClose);
            if !self.quitting
                && let Some(i) = self.extras.iter().position(|e| !e.closed)
            {
                let e = self.extras.remove(i);
                if let Some(r) = e.outer {
                    ctx.viewport_cmd(ViewportCommand::OuterPosition(r.min));
                    ctx.viewport_cmd(ViewportCommand::InnerSize(r.size()));
                }
                // The control channel belongs to the app, not to a window: keep it.
                let control = root.take_control();
                *root = match control {
                    Some(rx) => e.app.with_control(rx),
                    None => e.app,
                };
                root.quit_confirmed = false;
                self.focused = None;
            }
        }
        if self.quitting {
            self.step_quit(ctx, root);
        }
    }

    fn step_quit(&mut self, ctx: &egui::Context, root: &mut CraftApp) {
        for e in &mut self.extras {
            if !e.app.has_unsaved() {
                e.closed = true;
            }
        }
        let mut states = vec![WindowState { id: ViewportId::ROOT, unsaved: root.has_unsaved(), asking: root.asking_before_closing() }];
        states.extend(self.extras.iter().filter(|e| !e.closed).map(|e| WindowState {
            id: e.id,
            unsaved: e.app.has_unsaved(),
            asking: e.app.asking_before_closing(),
        }));
        let step = quit_step(&states, self.asked);
        if step != QuitStep::Wait {
            // The answer to the last prompt has been seen; a later step must not read it again.
            self.asked = None;
        }
        match step {
            QuitStep::Wait => {}
            QuitStep::Cancel => self.quitting = false,
            QuitStep::Ask(id) => {
                if id == ViewportId::ROOT {
                    root.ask_before_closing();
                } else if let Some(e) = self.extras.iter_mut().find(|e| e.id == id) {
                    e.app.ask_before_closing();
                }
                ctx.viewport_cmd_to(id, ViewportCommand::Focus);
                self.asked = Some(id);
            }
            QuitStep::Finish => {
                // Everything saved or discarded: end the app.
                self.extras.clear();
                root.quit_confirmed = true;
                ctx.viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            }
        }
    }

    /// Draw the other windows. Call once per frame from the root viewport's `ui`.
    pub fn show(&mut self, ctx: &egui::Context) {
        self.extras.retain(|e| !e.closed);
        let mut focused = None;
        for e in &mut self.extras {
            let builder = e.builder.clone();
            ctx.show_viewport_immediate(e.id, builder, |ui, _class| {
                let ctx = ui.ctx().clone();
                e.app.logic(&ctx);
                if e.app.quit_requested {
                    e.app.quit_requested = false;
                    ctx.viewport_cmd(ViewportCommand::Close);
                }
                e.app.ui(ui);
                let (has_focus, outer, close) =
                    ctx.input(|i| (i.viewport().focused == Some(true), i.viewport().outer_rect, i.viewport().close_requested()));
                if has_focus {
                    focused = Some(e.id);
                }
                e.outer = outer.or(e.outer);
                // `logic` cancelled the close and opened the prompt if there are unsaved changes.
                if close && !e.app.has_unsaved() {
                    e.closed = true;
                }
            });
        }
        if focused.is_some() {
            self.focused = focused;
        }
        if self.focused.is_some_and(|f| !self.extras.iter().any(|e| e.id == f && !e.closed)) {
            self.focused = None;
        }
        self.extras.retain(|e| !e.closed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn id(n: u64) -> ViewportId {
        ViewportId::from_hash_of(("test-window", n))
    }

    #[test]
    fn new_window_cascades_from_the_front_one_with_its_size() {
        let front = Rect::from_min_size(pos2(100.0, 50.0), vec2(1200.0, 800.0));
        let (pos, size) = cascade_from(Some(front)).unwrap();
        assert_eq!(pos, pos2(128.0, 78.0));
        assert_eq!(size, vec2(1200.0, 800.0));
        assert_eq!(cascade_from(None), None, "unknown front window: use the builder's defaults");
    }

    #[test]
    fn tiling_two_windows_splits_the_area_side_by_side() {
        let area = Rect::from_min_size(pos2(0.0, 28.0), vec2(1000.0, 600.0));
        let t = tile(2, area);
        assert_eq!(t, [Rect::from_min_size(pos2(0.0, 28.0), vec2(500.0, 600.0)), Rect::from_min_size(pos2(500.0, 28.0), vec2(500.0, 600.0))]);
        assert_eq!(tile(1, area), [area]);
        assert!(tile(0, area).is_empty());
    }

    #[test]
    fn tiling_five_windows_makes_three_over_two_and_covers_the_area() {
        let area = Rect::from_min_size(pos2(100.0, 40.0), vec2(1200.0, 800.0));
        let t = tile(5, area);
        assert_eq!(t.len(), 5);
        // Three across the top row, two wider ones below.
        assert!(t[..3].iter().all(|r| r.min.y == 40.0 && (r.width() - 400.0).abs() < 1e-3 && (r.height() - 400.0).abs() < 1e-3));
        assert!(t[3..].iter().all(|r| r.min.y == 440.0 && (r.width() - 600.0).abs() < 1e-3));
        assert_eq!((t[3].min.x, t[4].min.x), (100.0, 700.0));
        let covered: f32 = t.iter().map(|r| r.area()).sum();
        assert!((covered - area.area()).abs() < 1.0, "no gaps and no overlaps");
        assert!(t.iter().all(|r| area.expand(0.01).contains_rect(*r)));
    }

    #[test]
    fn cascading_steps_down_and_right_then_starts_again() {
        let area = Rect::from_min_size(pos2(0.0, 28.0), vec2(1200.0, 800.0));
        let c = cascade(2, area);
        assert_eq!(c[1].min - c[0].min, vec2(CASCADE, CASCADE));
        assert_eq!(c[0].size(), c[1].size());
        // A small area runs out of room and wraps instead of leaving the screen.
        let c = cascade(40, Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 500.0)));
        assert!(c.iter().all(|r| Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 500.0)).expand(0.01).contains_rect(*r)));
        assert!(cascade(0, area).is_empty());
    }

    #[test]
    fn the_work_area_sits_below_the_menu_bar_on_the_monitor_of_the_front_window() {
        let a = work_area(vec2(1920.0, 1080.0), None);
        assert_eq!(a.min, pos2(0.0, MENU_BAR));
        let front = Rect::from_min_size(pos2(-2500.0, 100.0), vec2(800.0, 600.0));
        let a = work_area(vec2(1920.0, 1080.0), Some(front));
        assert_eq!(a.min, pos2(-3840.0, MENU_BAR));
        assert!(work_area(vec2(0.0, 0.0), Some(front)).size().x >= 200.0, "a monitor of unknown size is not a panic");
    }

    #[test]
    fn quit_asks_each_unsaved_window_in_turn_then_finishes() {
        let w = |n: u64, unsaved: bool| WindowState { id: id(n), unsaved, asking: false };
        let root = WindowState { id: ViewportId::ROOT, unsaved: false, asking: false };
        // Two dirty extras, a clean root: the first dirty window is asked first.
        let mut windows = vec![root, w(1, true), w(2, true)];
        assert_eq!(quit_step(&windows, None), QuitStep::Ask(id(1)));
        // While its prompt is open, wait.
        windows[1].asking = true;
        assert_eq!(quit_step(&windows, Some(id(1))), QuitStep::Wait);
        // Saved (or discarded): on to the next.
        windows[1] = w(1, false);
        assert_eq!(quit_step(&windows, Some(id(1))), QuitStep::Ask(id(2)));
        windows[2] = w(2, false);
        assert_eq!(quit_step(&windows, Some(id(2))), QuitStep::Finish);
        // Nothing unsaved at all: finish at once.
        assert_eq!(quit_step(&[root], None), QuitStep::Finish);
    }

    #[test]
    fn cancel_on_any_prompt_stops_quitting() {
        let root = WindowState { id: ViewportId::ROOT, unsaved: true, asking: false };
        let extra = WindowState { id: id(1), unsaved: true, asking: false };
        assert_eq!(quit_step(&[root, extra], None), QuitStep::Ask(ViewportId::ROOT));
        // The root's prompt closed and it is still unsaved: the user pressed Cancel.
        assert_eq!(quit_step(&[root, extra], Some(ViewportId::ROOT)), QuitStep::Cancel);
        // A window asked earlier that has since closed (Don't Save) is simply gone.
        assert_eq!(quit_step(&[extra], Some(ViewportId::ROOT)), QuitStep::Ask(id(1)));
    }
}
