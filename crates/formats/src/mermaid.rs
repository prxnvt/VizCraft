//! Mermaid import: flowchart, sequence, class, state, ER and architecture text become shapes and connectors on
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
//!
//! Identity for re-import: node ids are kept as shape ids (a merging import makes them the
//! element ids), and a node's tag comes from a comment line `%% tag: <node id> <tag>` or from a
//! label that ends in ` #<tag>` where the tag holds a digit (`Pump #P-101`); the suffix is taken
//! off the label. The tag is kept in the shape's `ext` under [`crate::csv::TAG_KEY`].

pub mod architecture;
mod class;
mod common;
pub mod er;
mod graph;
pub mod sequence;
pub mod sequence_write;
mod state;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use vizcraft_mermaid::Diagram;

use crate::{FormatError, Imported};

/// Read Mermaid text as one laid-out page; skipped statements come back as warnings.
pub fn import(text: &str) -> Result<Imported, FormatError> {
    let parsed = vizcraft_mermaid::parse(text).map_err(|e| FormatError::Import(format!("Mermaid text could not be read: {e}")))?;
    let mut warnings = common::Warnings::default();
    for w in &parsed.warnings {
        warnings.push(w.to_string());
    }
    let mut page = match &parsed.diagram {
        Diagram::Sequence(seq) => sequence::build(seq, &mut warnings),
        Diagram::Architecture(a) => architecture::build(a, text, &mut warnings)?,
        Diagram::Class(d) => class::build(d, &mut warnings)?,
        Diagram::State(d) => state::build(d, &mut warnings)?,
        Diagram::Er(d) => er::build(d, &mut warnings)?,
        other => graph::build(&other.to_graph(), &mut warnings)?,
    };
    carry_tags(text, &mut page);
    Ok(Imported { pages: vec![page], warnings: warnings.finish() })
}

/// Write page `page` of `doc` as Mermaid text, picked by the page's viewpoint (or, with none,
/// the kinds of element it draws): UML class and state machine pages, and ER pages. Returns the text and the
/// warnings about what the text cannot hold.
pub fn export(doc: &vizcraft_diagram::Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    if let Some(p) = doc.pages.get(page).filter(|p| p.sequence.is_some()) {
        return Ok(sequence_write::write(p));
    }
    let p = crate::uml::read(doc, page)?;
    match p.viewpoint.as_str() {
        "uml.ClassDiagram" => class::write(doc, page),
        "uml.StateMachineDiagram" => state::write(doc, page),
        "er.Conceptual" | "er.Logical" | "er.Physical" => er::write_page(doc, page),
        _ if p.has_any(crate::uml::STATE_TYPES) => state::write(doc, page),
        _ if p.has_any(&["Class", "Interface", "Enumeration"]) => class::write(doc, page),
        _ => Err(FormatError::Encode("Mermaid export writes UML class and state machine pages; this page is neither".into())),
    }
}

/// Longest tag read from a comment or a label suffix, in bytes.
const MAX_TAG: usize = 256;

/// Whether `t` reads as a tag: 1 to [`MAX_TAG`] letters, digits, `-`, `_` and `.`, with a digit.
fn is_tag(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= MAX_TAG
        && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && t.chars().any(|c| c.is_ascii_digit())
}

/// The `%% tag: <id> <tag>` comments of `text`, node id to tag (a later line wins).
fn tag_comments(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("%%").map(str::trim_start) else { continue };
        let Some(rest) = rest.strip_prefix("tag:") else { continue };
        let mut words = rest.split_whitespace().filter(|w| *w != "=");
        if let (Some(id), Some(tag)) = (words.next(), words.next())
            && tag.len() <= MAX_TAG
        {
            out.insert(id.to_string(), tag.to_string());
        }
    }
    out
}

/// Put each shape's tag in its `ext`: from a `%% tag:` comment naming its id, else from a ` #tag`
/// suffix on its label's last line (taken off the label).
fn carry_tags(text: &str, page: &mut vizcraft_diagram::Page) {
    let comments = tag_comments(text);
    for s in &mut page.shapes {
        let mut tag = comments.get(&s.id).cloned();
        let last = s.text.rsplit('\n').next().unwrap_or_default();
        if let Some((head, t)) = last.rsplit_once(" #")
            && is_tag(t.trim_end())
            && !head.trim().is_empty()
        {
            tag.get_or_insert_with(|| t.trim_end().to_string());
            let keep = s.text.len().saturating_sub(last.len()).saturating_add(head.trim_end().len());
            if s.text.is_char_boundary(keep) {
                s.text.truncate(keep);
            }
        }
        if let Some(t) = tag {
            s.ext.insert(crate::csv::TAG_KEY.into(), t);
            s.sanitize();
        }
    }
}

#[cfg(test)]
mod tag_tests {
    use super::import;
    use crate::csv::TAG_KEY;

    #[test]
    fn node_ids_stay_and_tags_come_from_comments_or_label_suffixes() {
        let src = "flowchart TD\n%% tag: A = T-100\n%% tag: B T-200\nA[Start] --> B[Pump #P-101]\nB --> C[Step #1a]\nC --> D[Release #notatag]\nD --> E[#9]\n";
        let page = import(src).unwrap().pages.remove(0);
        let get = |id: &str| page.shapes.iter().find(|s| s.id == id).unwrap();
        assert_eq!(get("A").ext.get(TAG_KEY).map(String::as_str), Some("T-100"));
        assert_eq!(get("B").ext.get(TAG_KEY).map(String::as_str), Some("T-200"), "a comment wins over the suffix");
        assert_eq!(get("B").text, "Pump");
        assert_eq!((get("C").text.as_str(), get("C").ext.get(TAG_KEY).map(String::as_str)), ("Step", Some("1a")));
        assert_eq!(get("D").text, "Release #notatag", "a suffix with no digit is part of the label");
        assert!(!get("D").ext.contains_key(TAG_KEY));
        assert_eq!(get("E").text, "#9", "a label that is only a suffix stays");
    }
}
