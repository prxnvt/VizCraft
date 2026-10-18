//! In-place text editing of a shape or connector label: starting, the editor drawn over the
//! item, and the commit that runs `shape.setText` or `connector.setText`.
//!
//! A shape with compartments edits per band: a double-click on a compartment edits that band
//! (a paragraph's text, or a list's rows one per line) in the band's own area, style and
//! alignment, and the commit runs `compartment.set`; a double-click on the main band, a slot or
//! a title edits the shape's text as usual.
//!
//! A shape's editor sits in its text block, in the shape's font, size, format and alignment, and
//! shows the stored text: field placeholders stay raw (`{{pageName}}`), and drawn letter case and
//! bullets are left off so the caret matches the characters typed. On a rotated shape (or a
//! rotated text block, or vertical text) egui cannot turn a text field, so the field runs
//! invisibly and the editor paints its text, selection and caret turned with the shape; a click
//! in the turned text places the caret where it lands.

use std::sync::Arc;

use egui::epaint::TextShape;
use egui::text::{CCursor, CCursorRange};
use egui::{Align, Color32, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Stroke, Ui, UiBuilder, vec2};
use serde_json::json;
use vizcraft_diagram::compartment::{self as cp, CompartmentKind};
use vizcraft_diagram::{self as dg, Hit, Style};

use super::run;
use crate::CraftApp;
use crate::canvas::Viewport2D;
use crate::canvas::draw::text::{Block, band_block, block, block_job};
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
    /// The caret as a character index, from the last frame.
    pub caret: Option<usize>,
    /// Put the caret here (a character index) on the next frame, focusing the field.
    pub(crate) place_caret: Option<usize>,
    /// The item was made for this edit (a text box clicked onto the page): left empty, it goes.
    pub created: bool,
    /// The compartment of the shape being edited (its text, or its rows one per line); `None`
    /// is the shape's own text.
    pub compartment: Option<String>,
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
    app.canvas.editing =
        Some(TextEditing { target, original: text.clone(), text, focus: true, caret: None, place_caret: None, created: false, compartment: None });
}

/// Start editing `target` where it was double-clicked (`at`, world space): a shape's compartment
/// band under the point edits that compartment (see the module doc), anything else the item's
/// own text.
pub fn start_editing_at(app: &mut CraftApp, target: Hit, at: dg::Point) {
    let band = match (&target, app.session.doc.page()) {
        (Hit::Shape(id), Some(page)) => page.shape(id).and_then(|s| {
            let hit = dg::hit::band_of(s, at, 0.0)?;
            let c = s.compartments.iter().find(|c| Some(&c.id) == hit.compartment.as_ref())?;
            // A title or a slot has no body text here: those edit the shape's own text.
            let on_title = cp::layout(s).iter().any(|b| {
                b.compartment.and_then(|i| s.compartments.get(i)).is_some_and(|x| x.id == c.id)
                    && b.title.is_some_and(|r| s.transform().inverse().is_some_and(|inv| r.contains(inv.apply(at))))
            });
            (c.kind != CompartmentKind::Slot && !on_title).then(|| (c.id.clone(), c.body_text()))
        }),
        _ => None,
    };
    start_editing(app, target);
    if let (Some((cid, text)), Some(ed)) = (band, app.canvas.editing.as_mut()) {
        ed.compartment = Some(cid);
        ed.original = text.clone();
        ed.text = text;
    }
}

/// Start editing shape `id`, which was just made for it (a new text box): left empty, the commit
/// deletes it.
pub fn start_editing_new(app: &mut CraftApp, id: &str) {
    start_editing(app, Hit::Shape(id.to_string()));
    if let Some(ed) = app.canvas.editing.as_mut() {
        ed.created = true;
    }
}

/// Finish in-place editing: run `shape.setText` / `connector.setText` when the text changed, or
/// delete a new text box left empty. The caret is kept in `CanvasState::last_caret`, so the
/// Symbol dialog can insert where typing stopped.
pub fn commit_editing(app: &mut CraftApp) {
    let Some(ed) = app.canvas.editing.take() else { return };
    app.canvas.last_caret = ed.caret.map(|c| (ed.id().to_string(), c));
    if ed.created && ed.text.trim().is_empty() {
        run(app, "shape.delete", json!({"ids": [ed.id()]}));
        return;
    }
    if ed.text == ed.original {
        return;
    }
    if let Some(cid) = &ed.compartment {
        let list = app
            .session
            .doc
            .page()
            .and_then(|p| p.shape(ed.id()))
            .and_then(|s| s.compartments.iter().find(|c| &c.id == cid))
            .map(|c| c.kind == CompartmentKind::List);
        let params = match list {
            Some(true) => json!({"id": ed.id(), "compartment": cid, "rows": ed.text.lines().collect::<Vec<_>>()}),
            Some(false) => json!({"id": ed.id(), "compartment": cid, "text": ed.text}),
            // The compartment went away under the editor: nothing to set.
            None => return,
        };
        run(app, "compartment.set", params);
        return;
    }
    let cmd = match ed.target {
        Hit::Shape(_) => "shape.setText",
        Hit::Connector(_) => "connector.setText",
    };
    run(app, cmd, json!({"id": ed.id(), "text": ed.text}));
}

/// Insert `piece` at the editor's caret (the end when unknown). Returns false when nothing is
/// being edited.
pub fn insert_at_caret(app: &mut CraftApp, piece: &str) -> bool {
    let Some(ed) = app.canvas.editing.as_mut() else { return false };
    let chars = ed.text.chars().count();
    let at = ed.caret.unwrap_or(chars).min(chars);
    let byte = ed.text.char_indices().nth(at).map_or(ed.text.len(), |(i, _)| i);
    ed.text.insert_str(byte, piece);
    let caret = at + piece.chars().count();
    ed.caret = Some(caret);
    ed.place_caret = Some(caret);
    true
}

/// Where the editor goes and how it lays out text.
struct Placement {
    /// The text block (for a connector: a box around the label position).
    block: Block,
    style: Style,
    font: egui::FontId,
}

fn placement(app: &CraftApp, vp: &Viewport2D) -> Option<Placement> {
    let page = app.session.doc.page()?;
    let ed = app.canvas.editing.as_ref()?;
    // Never smaller than half size while editing, so the text stays readable.
    let zoom = vp.zoom.max(0.5);
    match &ed.target {
        Hit::Shape(id) => {
            let s = page.shape(id)?;
            if let Some(cid) = &ed.compartment {
                let bands = cp::layout(s);
                let (band, c) = bands.iter().find_map(|b| Some((b, s.compartments.get(b.compartment?).filter(|c| &c.id == cid)?)))?;
                let style = c.text_style(&s.style);
                let font = draw::style_font(&style, zoom).unwrap_or_else(|| crate::theme::regular(12.0));
                return Some(Placement { block: band_block(vp, s, band.body, c.align, dg::VAlign::Top), style, font });
            }
            let font = draw::style_font(&s.style, zoom).unwrap_or_else(|| crate::theme::regular(12.0));
            Some(Placement { block: block(vp, s), style: s.style.clone(), font })
        }
        Hit::Connector(id) => {
            let c = page.connector(id)?;
            let font = draw::style_font(&c.style, zoom).unwrap_or_else(|| crate::theme::regular(12.0));
            let half = vec2(80.0 * vp.zoom, font.size * 0.7);
            let centre = vp.world_to_screen(pos(dg::connector_geometry(page, c).label_pos));
            let block = Block {
                centre,
                angle: 0.0,
                inner: Rect::from_min_max((-half).to_pos2(), half.to_pos2()),
                h_align: dg::HAlign::Center,
                v_align: dg::VAlign::Middle,
            };
            Some(Placement { block, style: c.style.clone(), font })
        }
    }
}

/// The in-place text editor over the edited shape or connector label. Enter commits,
/// Shift+Enter starts a new line, Escape cancels, clicking elsewhere commits.
pub(in crate::canvas) fn text_overlay(app: &mut CraftApp, ui: &mut Ui, vp: &Viewport2D, t: &Tokens) {
    let Some(Placement { block: b, style, font }) = placement(app, vp) else {
        // The edited compartment went away (undo, an agent): the edit ends.
        if app.canvas.editing.as_ref().is_some_and(|e| e.compartment.is_some()) {
            app.canvas.editing = None;
        }
        return;
    };
    // A dialog over the canvas (Insert Symbol) takes the keys and the focus; the edit waits.
    let dialog_open = app.dialog.is_some();
    if !dialog_open {
        let (escape, enter) = ui.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Escape), i.consume_key(Modifiers::NONE, Key::Enter)));
        if escape {
            app.canvas.editing = None;
            return;
        }
        if enter {
            commit_editing(app);
            return;
        }
    }
    let Some(ed) = app.canvas.editing.as_mut() else { return };
    let rotated = b.angle.abs() > 1e-4;
    let align = b.row_align();
    // Measure the text first, so the field grows to hold it.
    let wrap = b.inner.width().max(40.0);
    let measure = ui.painter().layout_job(editor_job(&ed.text, &style, font.clone(), &b, wrap));
    let height = b.inner.height().max(measure.rect.height() + 8.0).max(font.size * 1.4 + 8.0);
    let unrotated = Rect::from_center_size(b.centre + b.inner.center().to_vec2(), vec2(wrap + 8.0, height));
    let (field_rect, min_size) = if rotated {
        // The invisible field covers the turned editor, so a click on it lands in the field.
        let corners = [unrotated.left_top(), unrotated.right_top(), unrotated.right_bottom(), unrotated.left_bottom()];
        let turned: Vec<Pos2> = corners.iter().map(|p| b.to_screen(*p - b.centre)).collect();
        let bbox = Rect::from_points(&turned);
        (bbox, bbox.size())
    } else {
        (unrotated, unrotated.size())
    };
    let frame = if rotated {
        egui::Frame::NONE.inner_margin(egui::Margin::same(4))
    } else {
        egui::Frame::NONE.fill(draw::PAGE_FILL).stroke(Stroke::new(1.0, t.accent)).inner_margin(egui::Margin::same(4)).corner_radius(2)
    };
    let job_style = style.clone();
    let job_font = font.clone();
    let job_block = b;
    let mut layouter = move |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| -> Arc<egui::Galley> {
        let mut job = editor_job(text.as_str(), &job_style, job_font.clone(), &job_block, wrap_width);
        job.halign = align;
        ui.painter().layout_job(job)
    };
    let edit = egui::TextEdit::multiline(&mut ed.text)
        .id(egui::Id::new("canvas-text-edit"))
        .font(font.clone())
        .text_color(color(style.font_color))
        .horizontal_align(align)
        .vertical_align(match b.v_align {
            dg::VAlign::Top => Align::Min,
            dg::VAlign::Middle => Align::Center,
            dg::VAlign::Bottom => Align::Max,
        })
        .frame(frame)
        .desired_width(wrap)
        .min_size(min_size)
        .desired_rows(1)
        .layouter(&mut layouter)
        .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter));
    let mut child = ui.new_child(UiBuilder::new().max_rect(field_rect));
    if rotated {
        // Interactive but not painted: the turned editor is painted below.
        child.set_opacity(0.0);
    }
    let out = edit.show(&mut child);
    let resp = out.response.response.clone();
    let id = resp.id;
    let mut state = out.state.clone();
    let mut range = out.cursor_range;
    if rotated {
        // A click in the turned text: the caret goes to the character under it.
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let off = b.galley_offset(&out.galley);
            let rel = egui::emath::Rot2::from_angle(-b.angle) * (p - b.centre) - off;
            let c = out.galley.cursor_from_pos(rel);
            range = Some(CCursorRange::one(c));
            state.cursor.set_char_range(range);
            state.clone().store(ui.ctx(), id);
        }
        paint_turned(ui, &b, &out.galley, range, &unrotated, t, color(style.font_color));
    }
    if let Some(r) = range {
        ed.caret = Some(r.primary.index.into());
    }
    if ed.focus || ed.place_caret.is_some() {
        resp.request_focus();
        let chars = ed.text.chars().count();
        let sel = match ed.place_caret.take() {
            Some(c) => CCursorRange::one(CCursor::new(c.min(chars))),
            // Start with the whole text selected, so typing replaces it and arrows keep it.
            None => CCursorRange::two(CCursor::new(0), CCursor::new(chars)),
        };
        state.cursor.set_char_range(Some(sel));
        state.store(ui.ctx(), id);
        ed.focus = false;
    } else if dialog_open {
        // Back to the field, caret where it was, once the dialog closes.
        ed.place_caret = ed.caret;
    } else if resp.lost_focus() {
        commit_editing(app);
    }
}

/// The editor's layout job: the raw text in the style's format, wrapped to `wrap`.
fn editor_job(text: &str, st: &Style, font: egui::FontId, b: &Block, wrap: f32) -> egui::text::LayoutJob {
    let mut job = block_job(text.to_string(), st, font, b);
    job.wrap.max_width = wrap.max(1.0);
    job
}

/// The editor of a turned text block: background, border, text, selection and caret, all turned
/// about the block centre.
fn paint_turned(ui: &Ui, b: &Block, galley: &Arc<egui::Galley>, range: Option<CCursorRange>, field: &Rect, t: &Tokens, fallback: Color32) {
    let painter = ui.painter();
    let quad = |r: Rect| -> Vec<Pos2> {
        [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].iter().map(|p| b.to_screen(*p - b.centre)).collect()
    };
    painter.add(egui::Shape::convex_polygon(quad(*field), draw::PAGE_FILL, Stroke::new(1.0, t.accent)));
    let off = b.galley_offset(galley);
    // Galley coordinates to screen.
    let at = |p: Pos2| b.to_screen(p.to_vec2() + off);
    let sel_fill = t.accent.gamma_multiply(0.3);
    if let Some(r) = range {
        let (a, z) = if r.primary.index <= r.secondary.index { (r.primary, r.secondary) } else { (r.secondary, r.primary) };
        if a.index != z.index {
            let (ra, rz) = (galley.layout_from_cursor(a).row, galley.layout_from_cursor(z).row);
            let (pa, pz) = (galley.pos_from_cursor(a), galley.pos_from_cursor(z));
            for (i, row) in galley.rows.iter().enumerate().take(rz.saturating_add(1)).skip(ra) {
                let rr = row.rect();
                let x0 = if i == ra { pa.left() } else { rr.left() };
                let x1 = if i == rz { pz.left() } else { rr.right() };
                if x1 > x0 {
                    let r = Rect::from_min_max(egui::pos2(x0, rr.top()), egui::pos2(x1, rr.bottom()));
                    let pts: Vec<Pos2> = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].iter().map(|p| at(*p)).collect();
                    painter.add(egui::Shape::convex_polygon(pts, sel_fill, Stroke::NONE));
                }
            }
        }
    }
    let text = TextShape::new(at(Pos2::ZERO), galley.clone(), fallback).with_angle(b.angle);
    painter.add(text);
    if let Some(r) = range {
        let c = galley.pos_from_cursor(r.primary);
        painter.line_segment([at(c.left_top()), at(c.left_bottom())], Stroke::new(1.5, t.accent));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_editing(text: &str) -> CraftApp {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let id = app.session.doc.page().unwrap().shapes[0].id.clone();
        start_editing(&mut app, Hit::Shape(id));
        app.canvas.editing.as_mut().unwrap().text = text.into();
        app
    }

    #[test]
    fn symbols_go_in_at_the_caret() {
        let mut app = app_editing("ab");
        app.canvas.editing.as_mut().unwrap().caret = Some(1);
        assert!(insert_at_caret(&mut app, "\u{3a9}"));
        let ed = app.canvas.editing.as_ref().unwrap();
        assert_eq!((ed.text.as_str(), ed.caret, ed.place_caret), ("a\u{3a9}b", Some(2), Some(2)));
        // No caret yet: at the end.
        app.canvas.editing.as_mut().unwrap().caret = None;
        insert_at_caret(&mut app, "!");
        assert_eq!(app.canvas.editing.as_ref().unwrap().text, "a\u{3a9}b!");
        app.canvas.editing = None;
        assert!(!insert_at_caret(&mut app, "x"));
    }

    #[test]
    fn a_double_click_on_a_band_edits_that_compartment() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let id = app.session.doc.page().unwrap().shapes[0].id.clone();
        app.run("compartment.add", json!({"id": id, "kind": "list", "compartmentId": "attrs", "rows": ["a", "b"]})).unwrap();
        let s = app.session.doc.page().unwrap().shape(&id).unwrap().clone();
        let bands = cp::layout(&s);
        let row = s.transform().apply(bands[1].rows[1].center());
        start_editing_at(&mut app, Hit::Shape(id.clone()), row);
        let ed = app.canvas.editing.as_mut().unwrap();
        assert_eq!((ed.compartment.as_deref(), ed.text.as_str()), (Some("attrs"), "a\nb"));
        ed.text = "a\nb\nc".into();
        commit_editing(&mut app);
        let s = app.session.doc.page().unwrap().shape(&id).unwrap();
        assert_eq!(s.compartments[0].rows, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let text = s.text.clone();
        // The main band edits the shape's own text.
        let main = s.transform().apply(cp::layout(s)[0].rect.center());
        start_editing_at(&mut app, Hit::Shape(id.clone()), main);
        let ed = app.canvas.editing.as_ref().unwrap();
        assert_eq!((ed.compartment.as_deref(), ed.text.as_str()), (None, text.as_str()));
        // Placement follows the band; a compartment removed under the editor ends the edit.
        app.canvas.editing = None;
        start_editing_at(&mut app, Hit::Shape(id.clone()), row);
        let vp = Viewport2D::new(Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(800.0, 600.0)), [0.0, 0.0], 1.0);
        assert!(placement(&app, &vp).is_some());
        app.run("compartment.remove", json!({"id": id, "compartment": "attrs"})).unwrap();
        assert!(placement(&app, &vp).is_none());
        commit_editing(&mut app);
    }

    #[test]
    fn a_new_text_box_left_empty_is_deleted_on_commit() {
        let mut app = CraftApp::new(vizcraft_engine::Session::new(vizcraft_engine::sample::sample_document()), crate::Services::default());
        let id = app.run("shape.addTextBox", json!({"x": 10, "y": 10})).unwrap()["id"].as_str().unwrap().to_string();
        start_editing_new(&mut app, &id);
        app.canvas.editing.as_mut().unwrap().caret = Some(0);
        commit_editing(&mut app);
        assert!(app.session.doc.page().unwrap().shape(&id).is_none());
        assert_eq!(app.canvas.last_caret, Some((id, 0)));
        // With text it stays and takes the text.
        let id = app.run("shape.addTextBox", json!({"x": 10, "y": 10})).unwrap()["id"].as_str().unwrap().to_string();
        start_editing_new(&mut app, &id);
        app.canvas.editing.as_mut().unwrap().text = "Note".into();
        commit_editing(&mut app);
        assert_eq!(app.session.doc.page().unwrap().shape(&id).map(|s| s.text.as_str()), Some("Note"));
    }
}
