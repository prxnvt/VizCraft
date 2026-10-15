//! Several document windows in one process (File > New Window, Cmd+N), so the app has one Dock
//! icon and one Quit. The first window is eframe's root viewport; each further window is an
//! immediate viewport with its own [`CraftApp`].
//!
//! Closing the root window while others are open hands the root to the next window (it moves to
//! where that window was), because closing eframe's root viewport ends the app. Quit asks each
//! window with unsaved changes in turn; Cancel on any of them stops quitting.

use egui::{Pos2, Rect, Vec2, ViewportBuilder, ViewportCommand, ViewportId, vec2};
use protocraft_ui_egui::CraftApp;

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

    /// Open `app` in a new window shaped like `base`, cascaded from the window in front.
    pub fn open(&mut self, ctx: &egui::Context, mut app: CraftApp, base: ViewportBuilder) {
        app.multi_window = true;
        self.next_id += 1;
        let id = ViewportId::from_hash_of(("protocraft-window", self.next_id));
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
    pub fn close_front(&self, ctx: &egui::Context) {
        let id = self.focused.filter(|f| self.extras.iter().any(|e| e.id == *f && !e.closed)).unwrap_or(ViewportId::ROOT);
        ctx.send_viewport_cmd_to(id, ViewportCommand::Close);
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
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        let root_closing = ctx.input(|i| i.viewport().close_requested()) && !root.has_unsaved();
        if root_closing && self.extras.iter().any(|e| !e.closed) {
            // Ending the root viewport would end the app: keep it, and (unless quitting) give it
            // the next window's document, where that window was.
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if !self.quitting
                && let Some(i) = self.extras.iter().position(|e| !e.closed)
            {
                let e = self.extras.remove(i);
                if let Some(r) = e.outer {
                    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(r.min));
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(r.size()));
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
                ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
                self.asked = Some(id);
            }
            QuitStep::Finish => {
                // Everything saved or discarded: end the app.
                self.extras.clear();
                root.quit_confirmed = true;
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
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
                    ctx.send_viewport_cmd(ViewportCommand::Close);
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
