//! Sequence > Activations: whether bars follow calls and replies (Auto, the default) or only
//! explicit activate and deactivate events (Explicit), and on the selected message whether it
//! activates its receiver or deactivates its sender (`sequence.setActivations`,
//! `sequence.setActivation`).

use serde_json::json;
use vizcraft_diagram::sequence::{Activations, Event};

use super::{events, selected_message, with_message};
use crate::CraftApp;
use crate::ribbon::Control::*;
use crate::ribbon::{Group, Size};

fn mode(app: &CraftApp) -> Activations {
    app.session.doc.page().and_then(|p| p.sequence.as_ref()).map(|q| q.activations).unwrap_or_default()
}

/// Whether the selected message is followed by `Activate(receiver)` (`activate`) or
/// `Deactivate(sender)` in the run of activation events after it.
fn marked(app: &CraftApp, activate: bool) -> bool {
    let (Some(id), Some(page)) = (selected_message(app), app.session.doc.page()) else { return false };
    let Some(c) = page.connector(&id) else { return false };
    let ev = events(app);
    let Some(at) = ev.iter().position(|e| matches!(e, Event::Message(m) if *m == id)) else { return false };
    let want = if activate { c.end.shape().map(|l| Event::Activate(l.into())) } else { c.start.shape().map(|l| Event::Deactivate(l.into())) };
    ev.iter().skip(at + 1).take_while(|e| matches!(e, Event::Activate(_) | Event::Deactivate(_))).any(|e| Some(e) == want.as_ref())
}

/// The Sequence tab's Activations group.
pub(in crate::ribbon) fn group() -> Group {
    Group::new(
        "Activations",
        vec![
            Stack(vec![
                Button {
                    id: "sequence.setActivations",
                    icon: "activation",
                    label: "Auto",
                    size: Size::Small,
                    params: |_| json!({"mode": "auto"}),
                    checked: Some(|a| mode(a) == Activations::Auto),
                },
                Button {
                    id: "sequence.setActivations",
                    icon: "activation",
                    label: "Explicit",
                    size: Size::Small,
                    params: |_| json!({"mode": "explicit"}),
                    checked: Some(|a| mode(a) == Activations::Explicit),
                },
            ]),
            Stack(vec![
                Button {
                    id: "sequence.setActivation",
                    icon: "plus",
                    label: "Activate",
                    size: Size::Small,
                    params: |a| with_message(a, json!({"activate": !marked(a, true)})),
                    checked: Some(|a| marked(a, true)),
                },
                Button {
                    id: "sequence.setActivation",
                    icon: "minus",
                    label: "Deactivate",
                    size: Size::Small,
                    params: |a| with_message(a, json!({"deactivate": !marked(a, false)})),
                    checked: Some(|a| marked(a, false)),
                },
            ]),
        ],
    )
}
