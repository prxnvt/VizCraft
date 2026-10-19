//! PlantUML sequence diagram text from a page with an interaction (writing only), from the
//! public PlantUML language reference: `participant` and `actor` declarations, `->` for calls,
//! `->>` for asynchronous messages, `-->` for replies, `create` and `destroy`, the `alt` /
//! `else`, `opt`, `loop`, `par`, `break`, `critical` and `group` blocks closed by `end`,
//! `ref over`, `activate` and `deactivate`, and `note`. Lost and found messages are written
//! with PlantUML's incoming and outgoing message ends (`[->`, `->]`).
//!
//! In `auto` mode the bars are written as `activate` after each call and `deactivate` after the
//! reply that closes it (in a forking fragment, in its last operand, where the drawn bar ends).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::Document;
use vizcraft_diagram::sequence::{Activations, Event, Operator, Part, Side, Sort};

use crate::FormatError;
use crate::plantuml::MAX_OUTPUT;
use crate::uml::alias;

/// Text on one line for PlantUML: line breaks as `\n`.
fn line(t: &str) -> String {
    t.trim().replace("\r\n", "\\n").replace('\n', "\\n")
}

/// A name in double quotes on one line.
fn quoted(t: &str) -> String {
    format!("\"{}\"", line(t).replace('"', "'"))
}

/// Write page `page` of `doc` as PlantUML: the text and the warnings.
pub fn write(doc: &Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    let p = doc.pages.get(page).ok_or_else(|| FormatError::Encode(format!("there is no page {page}")))?;
    let seq = p.sequence.as_ref().ok_or_else(|| FormatError::Encode("the page has no interaction".into()))?;
    let mut out = String::from("@startuml\n");
    let mut warnings = Vec::new();
    if seq.autonumber {
        out.push_str("autonumber\n");
    }
    let mut lifelines: Vec<&vizcraft_diagram::Shape> = p.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. }))).collect();
    lifelines.sort_by(|a, b| (a.x + a.w / 2.0).total_cmp(&(b.x + b.w / 2.0)));
    let mut used = HashSet::new();
    let names: HashMap<&str, String> = lifelines.iter().map(|s| (s.id.as_str(), alias(&s.id, &mut used))).collect();
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| "unknown".into());
    let created: HashSet<&str> = seq
        .events
        .iter()
        .filter_map(|e| if let Event::Message(m) = e { p.connector(m) } else { None })
        .filter(|c| c.message.as_ref().is_some_and(|m| m.sort == Sort::Create))
        .filter_map(|c| c.end.shape())
        .collect();
    let declare = |s: &vizcraft_diagram::Shape| {
        let kw = if s.kind == vizcraft_diagram::ShapeKind::ActorLifeline { "actor" } else { "participant" };
        format!("{kw} {} as {}", quoted(&s.text), name(&s.id))
    };
    for s in lifelines.iter().filter(|s| !created.contains(s.id.as_str())) {
        let _ = writeln!(out, "{}", declare(s));
    }
    let auto = seq.activations == Activations::Auto;
    let mut bars: HashMap<String, Vec<String>> = HashMap::new();
    // Open fragments: forks, operand being written, operand count, bars open at the start.
    let mut frags: Vec<(bool, usize, usize, HashMap<String, usize>)> = Vec::new();
    let mut depth = 0usize;
    let ind = |d: usize| "  ".repeat(d.min(64));
    for (k, e) in seq.events.iter().enumerate() {
        if out.len() > MAX_OUTPUT {
            return Err(FormatError::Encode(format!("the PlantUML text is larger than {} MiB", MAX_OUTPUT / (1024 * 1024))));
        }
        match e {
            Event::Message(id) => {
                let Some(c) = p.connector(id) else { continue };
                let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
                let label = if c.text.trim().is_empty() { String::new() } else { format!(" : {}", line(&c.text)) };
                match (sort, c.start.shape(), c.end.shape()) {
                    (Sort::Lost, Some(a), _) => {
                        let _ = writeln!(out, "{}{} ->] {}", ind(depth), name(a), label.trim_start());
                        continue;
                    }
                    (Sort::Found, _, Some(b)) => {
                        let _ = writeln!(out, "{}[-> {}{label}", ind(depth), name(b));
                        continue;
                    }
                    _ => {}
                }
                let (Some(a), Some(b)) = (c.start.shape(), c.end.shape()) else { continue };
                if sort == Sort::Create
                    && let Some(t) = lifelines.iter().find(|s| s.id == b)
                {
                    let _ = writeln!(out, "{}create {}", ind(depth), declare(t));
                }
                let arrow = match sort {
                    Sort::Reply => "-->",
                    Sort::Async => "->>",
                    _ => "->",
                };
                let _ = writeln!(out, "{}{} {arrow} {}{label}", ind(depth), name(a), name(b));
                if sort == Sort::Destroy {
                    let _ = writeln!(out, "{}destroy {}", ind(depth), name(b));
                }
                if auto {
                    match sort {
                        Sort::Sync => {
                            bars.entry(b.to_string()).or_default().push(a.to_string());
                            let _ = writeln!(out, "{}activate {}", ind(depth), name(b));
                        }
                        Sort::Reply => {
                            let stack = bars.entry(a.to_string()).or_default();
                            if let Some(pos) = stack.iter().rposition(|caller| caller == b) {
                                let held =
                                    frags.iter().any(|(forks, op, n, before)| *forks && op + 1 < *n && before.get(a).copied().unwrap_or(0) > pos);
                                if !held {
                                    stack.truncate(pos);
                                    let _ = writeln!(out, "{}deactivate {}", ind(depth), name(a));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Activate(l) => {
                let _ = writeln!(out, "{}activate {}", ind(depth), name(l));
            }
            Event::Deactivate(l) => {
                let _ = writeln!(out, "{}deactivate {}", ind(depth), name(l));
            }
            Event::Note(id) => {
                let Some(s) = p.shape(id) else { continue };
                let Some(Part::Note { over, side }) = &s.sequence else { continue };
                let who: Vec<String> = over.iter().map(|o| name(o)).collect();
                let place = match side {
                    Side::Left => format!("left of {}", who.first().cloned().unwrap_or_default()),
                    Side::Right => format!("right of {}", who.first().cloned().unwrap_or_default()),
                    Side::Over => format!("over {}", who.join(", ")),
                };
                let _ = writeln!(out, "{}note {place} : {}", ind(depth), line(&s.text));
            }
            Event::Open(f) => {
                let Some(s) = p.shape(f) else { continue };
                let Some(Part::Fragment { operator, operands, covers }) = &s.sequence else { continue };
                let guard = operands.first().map(|o| line(&o.guard)).unwrap_or_default();
                let n = seq
                    .events
                    .iter()
                    .skip(k)
                    .take_while(|x| !matches!(x, Event::Close(c) if c == f))
                    .filter(|x| matches!(x, Event::Else(c) if c == f))
                    .count()
                    + 1;
                let head = match operator {
                    Operator::Ref => {
                        let who: Vec<String> = if covers.is_empty() {
                            lifelines.iter().map(|l| name(&l.id)).collect()
                        } else {
                            covers.iter().map(|c| name(c)).collect()
                        };
                        let _ = writeln!(out, "{}ref over {} : {guard}", ind(depth), who.join(", "));
                        "group ref".to_string()
                    }
                    Operator::Neg | Operator::Strict | Operator::Seq | Operator::Region => {
                        format!("group {} {guard}", if *operator == Operator::Region { "" } else { operator.name() }).replace("  ", " ")
                    }
                    op => format!("{} {guard}", op.name()),
                };
                if *operator == Operator::Ref {
                    warnings.push(format!("the content of ref fragment `{f}` is written in a group under its ref"));
                }
                let _ = writeln!(out, "{}{}", ind(depth), head.trim_end());
                let before = bars.iter().map(|(l, v)| (l.clone(), v.len())).collect();
                frags.push((operator.forks(), 0, n, before));
                depth += 1;
            }
            Event::Else(f) => {
                let k_op = frags.last().map_or(1, |x| x.1 + 1);
                if let Some(fr) = frags.last_mut() {
                    fr.1 = k_op;
                }
                let guard = match p.shape(f).and_then(|s| s.sequence.as_ref()) {
                    Some(Part::Fragment { operands, .. }) => operands.get(k_op).map(|o| line(&o.guard)).unwrap_or_default(),
                    _ => String::new(),
                };
                let _ = writeln!(out, "{}{}", ind(depth.saturating_sub(1)), format!("else {guard}").trim_end());
            }
            Event::Close(_) => {
                frags.pop();
                depth = depth.saturating_sub(1);
                let _ = writeln!(out, "{}end", ind(depth));
            }
        }
    }
    out.push_str("@enduml\n");
    Ok((out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_without_an_interaction_is_an_error() {
        let doc = Document::new();
        assert!(write(&doc, 0).is_err());
        assert!(write(&doc, 9).is_err());
    }

    #[test]
    fn messages_blocks_bars_and_notes_are_written() {
        let src = "sequenceDiagram\n  actor U as User\n  participant S as Shop\n  U->>S: buy\n  Note over U,S: paying\n  opt card\n    S-)U: receipt\n  end\n  S-->>U: done\n";
        let mut doc = Document::new();
        doc.pages = crate::mermaid::import(src).unwrap().pages;
        let (text, warnings) = write(&doc, 0).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let want = "@startuml\nactor \"User\" as U\nparticipant \"Shop\" as S\nU -> S : buy\nactivate S\nnote over U, S : paying\nopt card\n  S ->> U : receipt\nend\nS --> U : done\ndeactivate S\n@enduml\n";
        assert_eq!(text, want);
    }
}
