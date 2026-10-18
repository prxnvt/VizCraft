//! In-place text editing of a shape or connector label: starting, the editor drawn over the
//! item, and the commit that runs `shape.setText` or `connector.setText`.

use egui::{Align, Key, KeyboardShortcut, Modifiers, Rect, Stroke, Ui, UiBuilder, vec2};
use serde_json::json;
use vizcraft_diagram::{self as dg, Hit};

use super::run;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::draw::{self, color, pos};
use crate::theme::Tokens;

/// In-place text editing of a shape or connector.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditing {
    /// What is being edited.
    pub target: Hit,
    /// The text as typed so far.
    pub text: String,
    /// The text before editing, to skip a command when nothing changed.
    pub original: String,
    /// Focus the field on its first frame.
    pub(crate) focus: bool,
}

impl TextEditing {
    /// The id of the edited item.
    pub fn id(&self) -> &str {
        match &self.target {
            Hit::Shape(id) | Hit::Connector(id) => id,
        }
    }
}

/// Start editing the text of `target`.
pub fn start_editing(app: &mut CraftApp, target: Hit) {
    let Some(page) = app.session.doc.page() else { return };
    let text = match &target {
        Hit::Shape(id) => page.shape(id).map(|s| s.text.clone()),
        Hit::Connector(id) => page.connector(id).map(|c| c.text.clone()),
    };
    let Some(text) = text else { return };
    app.canvas.drag = None;
    app.canvas.editing = Some(TextEditing { target, original: text.clone(), text, focus: true });
}

/// Finish in-place editing: run `shape.setText` / `connector.setText` when the text changed.
pub fn commit_editing(app: &mut CraftApp) {
    let Some(ed) = app.canvas.editing.take() else { return };
    if ed.text == ed.original {
        return;
    }
    let cmd = match ed.target {
        Hit::Shape(_) => "shape.setText",
        Hit::Connector(_) => "connector.setText",
    };
    run(app, cmd, json!({"id": ed.id(), "text": ed.text}));
}

/// The in-place text editor over the edited shape or connector label. Enter commits,
/// Shift+Enter starts a new line, Escape cancels, clicking elsewhere commits.
pub(in crate::canvas) fn text_overlay(app: &mut CraftApp, ui: &mut Ui, vp: &Viewport2D, t: &Tokens) {
    let Some(page) = app.session.doc.page() else { return };
    let Some(ed) = app.canvas.editing.as_ref() else { return };
    let (centre, width, style) = match &ed.target {
        Hit::Shape(id) => match page.shape(id) {
            // Over the text block (a container's heading band), or the frame.
            Some(s) => {
                let (block, _) = s.text_rect();
                (s.transform().apply(block.center()), block.width() as f32 * vp.zoom, s.style.clone())
            }
            None => return,
        },
        Hit::Connector(id) => match page.connector(id) {
            Some(c) => (dg::connector_geometry(page, c).label_pos, 160.0 * vp.zoom, c.style.clone()),
            None => return,
        },
    };
    let font = draw::font_for(style.font_size, style.bold, vp.zoom.max(0.5)).unwrap_or_else(|| crate::theme::regular(12.0));
    let lines = ed.text.lines().count().max(1) + usize::from(ed.text.ends_with('\n'));
    let height = font.size * 1.35 * lines as f32 + 10.0;
    let width = width.clamp(80.0, 1200.0);
    let r = Rect::from_center_size(vp.world_to_screen(pos(centre)), vec2(width, height));

    let (escape, enter) = ui.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Escape), i.consume_key(Modifiers::NONE, Key::Enter)));
    if escape {
        app.canvas.editing = None;
        return;
    }
    if enter {
        commit_editing(app);
        return;
    }
    let Some(ed) = app.canvas.editing.as_mut() else { return };
    let frame = egui::Frame::NONE.fill(draw::PAGE_FILL).stroke(Stroke::new(1.0, t.accent)).inner_margin(egui::Margin::same(4)).corner_radius(2);
    let edit = egui::TextEdit::multiline(&mut ed.text)
        .id(egui::Id::new("canvas-text-edit"))
        .font(font)
        .text_color(color(style.font_color))
        .horizontal_align(Align::Center)
        .frame(frame)
        .desired_width(width - 8.0)
        .desired_rows(1)
        .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter));
    let mut child = ui.new_child(UiBuilder::new().max_rect(r));
    let resp = child.put(r, edit);
    if ed.focus {
        resp.request_focus();
        // Start with the whole text selected, so typing replaces it and arrows keep it.
        let id = resp.id;
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(ed.text.chars().count()));
        state.cursor.set_char_range(Some(all));
        state.store(ui.ctx(), id);
        ed.focus = false;
    } else if resp.lost_focus() {
        commit_editing(app);
    }
}
