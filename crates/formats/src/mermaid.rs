//! Mermaid import: flowchart, sequence, class, state and ER text become shapes and connectors on
//! one page, placed automatically.
//!
//! `vizcraft-mermaid` parses the text. Flowcharts, state, class and ER diagrams go through the
//! parser's uniform graph view ([`vizcraft_mermaid::Diagram::to_graph`]) and are placed with the
//! layered layout of `vizcraft-layout`, one nested layout per group so a subgraph's rectangle
//! holds its members and nothing else. Sequence diagrams are placed by their own rules: a row of
//! participants, a lifeline under each and the messages in order down the page.
//!
//! Node shapes map onto the closest Basic or Flowchart master; anything the model cannot show
//! exactly (a dotted line, a crow's foot, the original Mermaid shape name) is kept in the item's
//! `ext` map under `mermaid.*` keys, and anything dropped comes back as a warning.

mod common;
mod graph;
mod sequence;
#[cfg(test)]
mod tests;

use vizcraft_mermaid::Diagram;

use crate::{FormatError, Imported};

/// Read Mermaid text as one laid-out page; skipped statements come back as warnings.
pub fn import(text: &str) -> Result<Imported, FormatError> {
    let parsed = vizcraft_mermaid::parse(text).map_err(|e| FormatError::Import(format!("Mermaid text could not be read: {e}")))?;
    let mut warnings = common::Warnings::default();
    for w in &parsed.warnings {
        warnings.push(w.to_string());
    }
    let page = match &parsed.diagram {
        Diagram::Sequence(seq) => sequence::build(seq, &mut warnings),
        other => graph::build(&other.to_graph(), &mut warnings)?,
    };
    Ok(Imported { pages: vec![page], warnings: warnings.finish() })
}
