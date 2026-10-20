//! The end pass of the sync pass (`crate::bind::sync`, after the styles step): a connector whose
//! kind declares `ends` (`ConnectorType::ends`) draws the ends its relationship's properties call
//! for: the line ends, the labels beside them, the dash and double line of each half, and the
//! kind's label frame. ER cardinality is the case it was made for: the four notations read the
//! same `sourceCardinality` and `targetCardinality` and draw them differently.
//!
//! The look starts from the kind's own (its arrows, plain halves, no end labels, its label frame)
//! and each matching rule overwrites the fields it names, so a value with no rule shows the kind's
//! default. A field changes only when it differs, so running the pass twice changes nothing,
//! and `variant.set` (which applies the new kind) and the pass agree.
//!
//! End labels: a relationship type that declares `endLabels` (OMG UML multiplicities) has its end
//! labels written by the end labels step of the sync pass, which also writes edits back to the
//! properties; this pass leaves those labels alone so the two never write the same field. Both
//! exist because they answer different questions: `endLabels` shows property text as it is,
//! while `ends` maps property values to a notation's symbols.

use std::collections::HashMap;

use vizcraft_diagram::connector_look::{Half, Halves, LabelFrame};
use vizcraft_diagram::{Arrow, Dash, Document};
use vizcraft_model::{PropValue, Relationship};

use crate::Catalog;
use crate::schema::{ConnectorType, EndRule, RelationshipType};

/// The look the pass gives one connector.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    start_arrow: Arrow,
    end_arrow: Arrow,
    start_label: String,
    end_label: String,
    halves: Halves,
    frame: LabelFrame,
}

/// The value of `key` on `r` as text, else its schema default.
fn value(r: &Relationship, t: &RelationshipType, key: &str) -> Option<String> {
    r.prop(key).or_else(|| t.property(key).and_then(|p| p.default.as_ref())).map(PropValue::display)
}

/// Apply `rules` to one end of `look`.
fn apply(rules: &[EndRule], r: &Relationship, t: &RelationshipType, arrow: &mut Arrow, label: &mut String, half: &mut Half) {
    for rule in rules.iter().take(crate::loader::MAX_END_RULES) {
        if let Some(w) = &rule.when
            && !value(r, t, &w.property).is_some_and(|v| w.any.contains(&v))
        {
            continue;
        }
        let Some(look) = value(r, t, &rule.property).and_then(|v| rule.values.get(&v)) else { continue };
        if let Some(a) = look.arrow.as_deref().and_then(Arrow::from_name) {
            *arrow = a;
        }
        if let Some(l) = &look.label {
            *label = l.chars().take(crate::loader::MAX_END_LABEL).collect();
        }
        if let Some(d) = look.dash.as_deref().and_then(Dash::from_name) {
            half.dash = Some(d);
        }
        if let Some(d) = look.double {
            half.double = d;
        }
    }
}

/// The look of a connector of kind `k` drawing `r`.
fn look_for(k: &ConnectorType, r: &Relationship, t: &RelationshipType) -> Option<Look> {
    let ends = k.ends.as_ref()?;
    let mut look = Look {
        start_arrow: Arrow::from_name(&k.start_arrow).unwrap_or_default(),
        end_arrow: Arrow::from_name(&k.end_arrow).unwrap_or_default(),
        start_label: String::new(),
        end_label: String::new(),
        halves: Halves::default(),
        frame: LabelFrame::from_name(&k.label_frame).unwrap_or_default(),
    };
    apply(&ends.start, r, t, &mut look.start_arrow, &mut look.start_label, &mut look.halves.start);
    apply(&ends.end, r, t, &mut look.end_arrow, &mut look.end_label, &mut look.halves.end);
    // A half dashed the way the whole line already is says nothing extra.
    let line = Dash::from_name(&k.dash).unwrap_or_default();
    for h in [&mut look.halves.start, &mut look.halves.end] {
        if h.dash == Some(line) {
            h.dash = None;
        }
    }
    Some(look)
}

fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

/// Run the end pass over `doc`.
pub fn ends(doc: &mut Document, cat: &Catalog) {
    let kinds: HashMap<&str, &ConnectorType> =
        cat.packs().iter().flat_map(|p| p.connector_types.iter()).filter(|k| k.ends.is_some()).map(|k| (k.id.as_str(), k)).collect();
    if kinds.is_empty() {
        return;
    }
    let rels: HashMap<&str, &Relationship> = doc.model.relationships.iter().map(|r| (r.id.as_str(), r)).collect();
    // Wanted looks by (page, connector index), with whether labels are this pass's to write.
    let mut wanted: Vec<(usize, usize, Look, bool)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for (ci, c) in page.connectors.iter().enumerate() {
            let Some(k) = kinds.get(c.kind.as_str()) else { continue };
            let Some(r) = c.relationship.as_deref().and_then(|id| rels.get(id)) else { continue };
            let Some((_, t)) = cat.relationship_type(&r.type_id) else { continue };
            if let Some(look) = look_for(k, r, t) {
                wanted.push((pi, ci, look, t.end_labels.is_empty()));
            }
        }
    }
    for (pi, ci, look, labels) in wanted {
        let Some(c) = doc.pages.get_mut(pi).and_then(|p| p.connectors.get_mut(ci)) else { continue };
        set_if_changed(&mut c.start_arrow, look.start_arrow);
        set_if_changed(&mut c.end_arrow, look.end_arrow);
        set_if_changed(&mut c.halves, look.halves);
        set_if_changed(&mut c.label_frame, look.frame);
        if labels {
            set_if_changed(&mut c.start_label, look.start_label);
            set_if_changed(&mut c.end_label, look.end_label);
        }
    }
}
