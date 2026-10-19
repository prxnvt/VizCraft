//! The contextual Sequence tab, shown while the active page has an interaction (`Page.sequence`):
//! lifelines and notes, the message sorts as Connector tool settings, fragments, the order moves,
//! activations and the layout. Everything runs a `sequence.*` command or arms the Connector tool,
//! so each click is one undo step.

use serde_json::{Value, json};
use vizcraft_diagram::sequence::{self as seq, Event, Part};

use super::Tab;
use crate::CraftApp;

fn has_interaction(app: &CraftApp) -> bool {
    app.session.doc.page().is_some_and(|p| p.sequence.is_some())
}

/// The selected item of the given kind on the active page: a message connector, a lifeline, or
/// a block (message, note or fragment) of the order.
fn selected(app: &CraftApp, want: fn(&vizcraft_diagram::Page, &str) -> bool) -> Option<String> {
    let page = app.session.doc.page()?;
    app.session.sel.primary.iter().chain(&app.session.sel.items).find(|id| want(page, id)).cloned()
}

fn is_message(page: &vizcraft_diagram::Page, id: &str) -> bool {
    page.connector(id).is_some_and(|c| c.message.is_some())
}

fn is_lifeline(page: &vizcraft_diagram::Page, id: &str) -> bool {
    page.shape(id).is_some_and(|s| matches!(s.sequence, Some(Part::Lifeline { .. })))
}

fn is_block(page: &vizcraft_diagram::Page, id: &str) -> bool {
    page.sequence.as_ref().is_some_and(|q| seq::block_of(&q.events, id).is_some())
}

/// The selected message.
fn selected_message(app: &CraftApp) -> Option<String> {
    selected(app, is_message)
}

/// The selected lifeline.
fn selected_lifeline(app: &CraftApp) -> Option<String> {
    selected(app, is_lifeline)
}

/// The selected blocks of the order (messages, notes and fragments), in selection order.
fn selected_blocks(app: &CraftApp) -> Vec<String> {
    let Some(page) = app.session.doc.page() else { return Vec::new() };
    app.session.sel.items.iter().filter(|id| is_block(page, id)).cloned().collect()
}

/// The page's events, if it has an interaction.
fn events(app: &CraftApp) -> Vec<Event> {
    app.session.doc.page().and_then(|p| p.sequence.as_ref()).map(|q| q.events.clone()).unwrap_or_default()
}

/// `{"id": <selected message>}` merged into `v`, or `v` alone.
fn with_message(app: &CraftApp, mut v: Value) -> Value {
    if let (Some(id), Some(o)) = (selected_message(app), v.as_object_mut()) {
        o.insert("id".into(), json!(id));
    }
    v
}

groups! {
    lifelines,
    messages,
    fragments,
    order,
    activations,
    layout,
}

/// The Sequence tab.
pub(super) fn tab() -> Tab {
    Tab::contextual("Sequence", has_interaction, groups())
}
