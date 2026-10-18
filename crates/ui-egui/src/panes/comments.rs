//! The Comments pane: the review threads of the active page, each with its replies and a reply
//! field, a box for a new comment on the selected shape (or the page), and Previous and Next to
//! walk the anchors. Every button runs a `comment.*` command. While the pane is open the canvas
//! also draws a marker at each comment's anchor (`canvas::draw::links`).

use egui::{Id, Ui};
use serde_json::json;
use vizcraft_diagram::Comment;

use super::{PaneSpec, Side};
use crate::CraftApp;
use crate::theme::Tokens;

/// The Comments pane. It starts closed: Review > Comments opens it.
pub(super) const SPEC: PaneSpec = PaneSpec::new("comments", "Comments", Side::Right, 300.0, comments);

/// A time stamp as people read it: the date and the minute.
fn when(iso: &str) -> String {
    iso.chars().take(16).map(|c| if c == 'T' { ' ' } else { c }).collect()
}

/// A draft kept between frames in egui's memory, keyed by `id`.
fn draft(ui: &mut Ui, id: Id) -> String {
    ui.memory_mut(|m| m.data.get_temp::<String>(id)).unwrap_or_default()
}

fn set_draft(ui: &mut Ui, id: Id, text: String) {
    ui.memory_mut(|m| m.data.insert_temp(id, text));
}

/// What a comment is pinned to, in words.
fn anchor_label(app: &CraftApp, c: &Comment) -> String {
    match c.anchor.shape.as_deref().and_then(|id| app.session.doc.shape(id)) {
        Some(sh) if !sh.text.trim().is_empty() => format!("on {}", sh.text.replace('\n', " ")),
        Some(sh) => format!("on {}", sh.id),
        None => "on the page".into(),
    }
}

fn comments(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let any = !app.session.doc.comments.is_empty();
        if ui.add_enabled(any, egui::Button::new("Previous")).clicked() {
            let _ = app.run("comment.previous", json!({}));
        }
        if ui.add_enabled(any, egui::Button::new("Next")).clicked() {
            let _ = app.run("comment.next", json!({}));
        }
    });
    ui.add_space(6.0);

    // The new comment goes on the selected shape of this page, else on the page itself.
    let target = app.session.doc.page().and_then(|p| app.session.sel.primary.as_ref().and_then(|id| p.shape(id)));
    let target_text = target.map(|sh| if sh.text.trim().is_empty() { sh.id.clone() } else { sh.text.replace('\n', " ") });
    ui.label(egui::RichText::new(match &target_text {
        Some(n) => format!("New comment on {n}"),
        None => "New comment on the page".to_string(),
    })
    .small()
    .color(t.text_dim));
    let new_id = Id::new("comments_new_draft");
    let mut text = draft(ui, new_id);
    let field = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(f32::INFINITY).hint_text("Write a comment"));
    // Focus the box when the pane has just opened (it was not drawn in the last few frames).
    let seen_id = Id::new("comments_seen_pass");
    let pass = ui.ctx().cumulative_pass_nr();
    let last = ui.memory_mut(|m| m.data.get_temp::<u64>(seen_id)).unwrap_or(0);
    ui.memory_mut(|m| m.data.insert_temp(seen_id, pass));
    if pass.saturating_sub(last) > 3 {
        field.request_focus();
    }
    let post = ui.add_enabled(!text.trim().is_empty(), egui::Button::new("Post")).clicked();
    if post {
        let params = match target {
            Some(sh) => json!({"text": text, "shape": sh.id}),
            None => json!({"text": text, "shape": null}),
        };
        if app.run("comment.add", params).is_ok() {
            text.clear();
        }
    }
    set_draft(ui, new_id, text);
    ui.add_space(8.0);
    ui.separator();

    let page_id = app.session.doc.page().map(|p| p.id.clone()).unwrap_or_default();
    let threads: Vec<Comment> = app.session.doc.comments.iter().filter(|c| c.anchor.page == page_id).cloned().collect();
    if threads.is_empty() {
        ui.label(egui::RichText::new("No comments on this page").color(t.text_dim));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for c in &threads {
            thread(app, ui, c);
            ui.add_space(6.0);
        }
    });
}

/// One thread: header, text, replies, a reply field and the row of buttons.
fn thread(app: &mut CraftApp, ui: &mut Ui, c: &Comment) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE
        .fill(t.input)
        .stroke(egui::Stroke::new(1.0, t.border))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(8));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&c.author).strong().color(t.text));
            ui.label(egui::RichText::new(when(&c.time)).small().color(t.text_dim));
        });
        ui.label(egui::RichText::new(anchor_label(app, c)).small().color(t.text_dim));
        ui.label(egui::RichText::new(&c.text).color(t.text));
        for r in &c.replies {
            ui.add_space(4.0);
            ui.indent(("reply", &c.id), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(&r.author).strong().color(t.text));
                    ui.label(egui::RichText::new(when(&r.time)).small().color(t.text_dim));
                });
                ui.label(egui::RichText::new(&r.text).color(t.text));
            });
        }
        ui.add_space(4.0);
        let rid = Id::new(("comments_reply_draft", &c.id));
        let mut text = draft(ui, rid);
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY).hint_text("Reply"));
        ui.horizontal(|ui| {
            if ui.add_enabled(!text.trim().is_empty(), egui::Button::new("Reply")).clicked()
                && app.run("comment.reply", json!({"id": c.id, "text": text})).is_ok()
            {
                text.clear();
            }
            if ui.button("Go to").clicked() {
                let _ = app.run("comment.show", json!({"id": c.id}));
            }
            if ui.button("Delete").clicked() {
                let _ = app.run("comment.delete", json!({"id": c.id}));
            }
        });
        set_draft(ui, rid, text);
    });
}
