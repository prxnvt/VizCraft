//! The sequence page checks: [`Check::MessageOrder`] and [`Check::RepliesMatched`]. The logic is
//! `vizcraft_diagram::sequence::check_order` and `check_replies`; this module turns their
//! findings into issues on the page, with the model items the connectors and shapes draw as
//! subjects.

use vizcraft_diagram::Document;
use vizcraft_diagram::sequence::{Finding, check_order, check_replies};
use vizcraft_model::Issue;

use crate::rules::{MAX_PER_RULE, message};
use crate::schema::{Check, RuleDecl};

/// Run one of the sequence checks over every page with an interaction; other checks are
/// ignored.
pub(crate) fn check(doc: &Document, rule: &RuleDecl, out: &mut Vec<Issue>) {
    for page in doc.pages.iter().filter(|p| p.sequence.is_some()) {
        let findings = match &rule.check {
            Check::MessageOrder => check_order(page),
            Check::RepliesMatched { require_replies } => check_replies(page, *require_replies),
            _ => return,
        };
        for Finding { what, items } in findings {
            if out.len() >= MAX_PER_RULE {
                return;
            }
            let subjects: Vec<String> = items
                .iter()
                .filter_map(|id| page.connector(id).and_then(|c| c.relationship.clone()).or_else(|| page.shape(id).and_then(|s| s.element.clone())))
                .collect();
            out.push(
                Issue::new(rule.id.clone(), rule.severity, message(&rule.description, &format!("{what}.")))
                    .with_subjects(subjects)
                    .with_items(items)
                    .on_page(page.id.clone()),
            );
        }
    }
}
