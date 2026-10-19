//! Mermaid `sequenceDiagram` text from a page with an interaction (see
//! `vizcraft_diagram::sequence`), the reverse of [`super::sequence`].
//!
//! Lifelines are declared left to right (`actor` for actor lifelines, created ones at their
//! create message), messages are written in event order with the arrow of their sort (or the
//! token kept from an import when it reads as the same sort), fragments as their blocks with
//! `else`, `and` or `option` between operands, notes as `Note` lines and boxes as `box` blocks.
//! In `auto` mode the activation bars are written as `+` and `-` on calls and replies, so a
//! renderer that follows Mermaid draws the same bars: a reply in an operand of a forking
//! fragment closes its bar only in the last operand, where the drawn bar ends. Explicit
//! activation events directly after a message fold into its `+` or `-`.
//!
//! Lossy, each with a warning: lost and found messages (a comment), `ref` (a note over the
//! lifelines), `neg`, `strict` and `seq` (written as `critical` with the operator in the guard),
//! and separators in fragments whose Mermaid block has no branch keyword.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::sequence::{Activations, Event, Operator, Part, Side, Sort};
use vizcraft_diagram::{Page, Shape};

use super::sequence::{ARROW_KEY, BOX_KEY, RECT_KEY, canonical_arrow};

/// Largest text written, in bytes.
pub const MAX_OUTPUT: usize = 2 * 1024 * 1024;

/// The sort an arrow token reads as, if it is one.
fn token_sort(t: &str) -> Option<Sort> {
    Some(match t {
        "->>" | "->" | "<<->>" | "<<-->>" => Sort::Sync,
        "-->>" | "-->" => Sort::Reply,
        "-)" | "--)" | "-x" | "--x" => Sort::Async,
        _ => return None,
    })
}

/// Text on one line, safe after a `:`.
fn line(t: &str) -> String {
    t.trim().replace("\r\n", "<br/>").replace('\n', "<br/>").replace(';', "#59;")
}

/// Mermaid participant aliases for the lifelines: the shape id when it reads as one, else a
/// cleaned copy, unique.
fn aliases(lifelines: &[&Shape]) -> HashMap<String, String> {
    let mut used = HashSet::new();
    let mut out = HashMap::new();
    for s in lifelines {
        let mut base: String = s.id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
        if base.is_empty() || base.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            base = format!("p_{base}");
        }
        // Words Mermaid reads as keywords cannot be ids.
        if matches!(base.as_str(), "end" | "box" | "loop" | "alt" | "else" | "opt" | "par" | "and" | "rect" | "note" | "participant" | "actor") {
            base = format!("{base}_");
        }
        let mut id = base.clone();
        let mut n = 1usize;
        while !used.insert(id.clone()) {
            n += 1;
            id = format!("{base}_{n}");
        }
        out.insert(s.id.clone(), id);
    }
    out
}

/// An open fragment while writing: whether its operands fork, the operand being written, how
/// many operands it has, and how many bars were open when it started.
struct Frag {
    forks: bool,
    operand: usize,
    operands: usize,
    /// Bars open at the start, per lifeline.
    before: HashMap<String, usize>,
}

/// Write the page's interaction as Mermaid text, with warnings about what the text cannot hold.
pub fn write(page: &Page) -> (String, Vec<String>) {
    let mut out = String::from("sequenceDiagram\n");
    let mut warnings: Vec<String> = Vec::new();
    let Some(seq) = &page.sequence else {
        return (out, vec!["the page has no interaction".into()]);
    };
    if seq.autonumber {
        out.push_str("  autonumber\n");
    }
    let mut lifelines: Vec<&Shape> = page.shapes.iter().filter(|s| matches!(s.sequence, Some(Part::Lifeline { .. }))).collect();
    lifelines.sort_by(|a, b| (a.x + a.w / 2.0).total_cmp(&(b.x + b.w / 2.0)));
    let alias = aliases(&lifelines);
    let name = |id: &str| alias.get(id).cloned().unwrap_or_else(|| "unknown".into());
    let created: HashSet<&str> = seq
        .events
        .iter()
        .filter_map(|e| if let Event::Message(m) = e { page.connector(m) } else { None })
        .filter(|c| c.message.as_ref().is_some_and(|m| m.sort == Sort::Create))
        .filter_map(|c| c.end.shape())
        .collect();
    let declare = |s: &Shape| {
        let kw = if s.kind == vizcraft_diagram::ShapeKind::ActorLifeline { "actor" } else { "participant" };
        let a = name(&s.id);
        let label = line(&s.text);
        if label.is_empty() || label == a { format!("{kw} {a}") } else { format!("{kw} {a} as {label}") }
    };
    // Boxes: background rectangles from an import, around consecutive lifelines.
    let boxes: Vec<&Shape> = page.shapes.iter().filter(|s| s.ext.contains_key(BOX_KEY)).collect();
    let in_box = |s: &Shape| boxes.iter().position(|b| s.x >= b.x - 1e-6 && s.x + s.w <= b.x + b.w + 1e-6);
    let mut open_box: Option<usize> = None;
    for s in lifelines.iter().filter(|s| !created.contains(s.id.as_str())) {
        let b = in_box(s);
        if b != open_box {
            if open_box.is_some() {
                out.push_str("  end\n");
            }
            if let Some(bi) = b
                && let Some(bs) = boxes.get(bi)
            {
                let color = bs.ext.get(BOX_KEY).cloned().unwrap_or_default();
                let words = [color, line(&bs.text)].into_iter().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
                let _ = writeln!(out, "  box {words}");
            }
            open_box = b;
        }
        let indent = if open_box.is_some() { "    " } else { "  " };
        let _ = writeln!(out, "{indent}{}", declare(s));
    }
    if open_box.is_some() {
        out.push_str("  end\n");
    }

    let auto = seq.activations == Activations::Auto;
    // Open bars per lifeline in auto mode: the caller of each.
    let mut bars: HashMap<String, Vec<String>> = HashMap::new();
    let mut frags: Vec<Frag> = Vec::new();
    let mut depth = 1usize;
    let mut skip_next = false;
    let ind = |d: usize| "  ".repeat(d.min(64));
    for (k, e) in seq.events.iter().enumerate() {
        if out.len() > MAX_OUTPUT {
            warnings.push("the text was cut at 2 MiB".into());
            break;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        match e {
            Event::Message(id) => {
                let Some(c) = page.connector(id) else { continue };
                let sort = c.message.as_ref().map(|m| m.sort).unwrap_or_default();
                let (a, b) = (c.start.shape(), c.end.shape());
                if matches!(sort, Sort::Lost | Sort::Found) {
                    let who = a.or(b).map(name).unwrap_or_default();
                    let _ = writeln!(out, "{}%% {} message at {who}: {}", ind(depth), sort.name(), line(&c.text));
                    warnings.push(format!("Mermaid has no {} messages; `{}` is written as a comment", sort.name(), id));
                    continue;
                }
                let (Some(a), Some(b)) = (a, b) else { continue };
                if sort == Sort::Create {
                    if let Some(t) = lifelines.iter().find(|s| s.id == b) {
                        let _ = writeln!(out, "{}create {}", ind(depth), declare(t));
                    }
                } else if sort == Sort::Destroy {
                    let _ = writeln!(out, "{}destroy {}", ind(depth), name(b));
                }
                let token = c.ext.get(ARROW_KEY).filter(|t| token_sort(t) == Some(sort)).map(String::as_str).unwrap_or(canonical_arrow(sort));
                let mut mark = "";
                if auto {
                    match sort {
                        Sort::Sync => {
                            bars.entry(b.to_string()).or_default().push(a.to_string());
                            mark = "+";
                        }
                        Sort::Reply => {
                            let stack = bars.entry(a.to_string()).or_default();
                            if let Some(pos) = stack.iter().rposition(|caller| caller == b) {
                                // In an earlier operand of a forking fragment a bar opened before
                                // it stays open: the drawn bar ends in the last operand.
                                let held = frags.iter().any(|f| f.forks && f.operand + 1 < f.operands && f.before.get(a).copied().unwrap_or(0) > pos);
                                if !held && pos + 1 == stack.len() {
                                    stack.pop();
                                    mark = "-";
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    match seq.events.get(k + 1) {
                        Some(Event::Activate(l)) if l == b => {
                            mark = "+";
                            skip_next = true;
                        }
                        Some(Event::Deactivate(l)) if l == a => {
                            mark = "-";
                            skip_next = true;
                        }
                        _ => {}
                    }
                }
                let _ = writeln!(out, "{}{}{token}{mark}{}: {}", ind(depth), name(a), name(b), line(&c.text));
            }
            Event::Activate(l) => {
                let _ = writeln!(out, "{}activate {}", ind(depth), name(l));
            }
            Event::Deactivate(l) => {
                let _ = writeln!(out, "{}deactivate {}", ind(depth), name(l));
            }
            Event::Note(id) => {
                let Some(s) = page.shape(id) else { continue };
                let Some(Part::Note { over, side }) = &s.sequence else { continue };
                let who: Vec<String> = over.iter().map(|o| name(o)).collect();
                let place = match side {
                    Side::Left => format!("left of {}", who.first().cloned().unwrap_or_default()),
                    Side::Right => format!("right of {}", who.first().cloned().unwrap_or_default()),
                    Side::Over => format!("over {}", who.join(",")),
                };
                let _ = writeln!(out, "{}Note {place}: {}", ind(depth), line(&s.text));
            }
            Event::Open(f) => {
                let Some(s) = page.shape(f) else { continue };
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
                    Operator::Region => format!("rect {}", s.ext.get(RECT_KEY).cloned().unwrap_or_else(|| "rgba(0, 0, 0, 0.08)".into())),
                    Operator::Ref => {
                        // A reference is a note across the lifelines it covers.
                        let who: Vec<String> = if covers.is_empty() {
                            lifelines.first().map(|l| name(&l.id)).into_iter().collect()
                        } else {
                            covers.iter().take(2).map(|c| name(c)).collect()
                        };
                        warnings.push(format!("Mermaid has no ref fragments; `{f}` is written as a note"));
                        let _ = writeln!(out, "{}Note over {}: ref {guard}", ind(depth), who.join(","));
                        "rect rgba(0, 0, 0, 0.04)".to_string()
                    }
                    Operator::Neg | Operator::Strict | Operator::Seq => {
                        warnings.push(format!("Mermaid has no {} fragments; `{f}` is written as critical", operator.name()));
                        format!("critical {} {guard}", operator.name()).trim_end().to_string()
                    }
                    op => format!("{} {guard}", op.name()).trim_end().to_string(),
                };
                let _ = writeln!(out, "{}{head}", ind(depth));
                let before = bars.iter().map(|(l, v)| (l.clone(), v.len())).collect();
                frags.push(Frag { forks: operator.forks(), operand: 0, operands: n, before });
                depth += 1;
            }
            Event::Else(f) => {
                let Some(s) = page.shape(f) else { continue };
                let Some(Part::Fragment { operator, operands, .. }) = &s.sequence else { continue };
                let k_op = frags.last().map_or(1, |x| x.operand + 1);
                if let Some(fr) = frags.last_mut() {
                    fr.operand = k_op;
                }
                let guard = operands.get(k_op).map(|o| line(&o.guard)).unwrap_or_default();
                let kw = match operator {
                    Operator::Alt => "else",
                    Operator::Par => "and",
                    Operator::Critical | Operator::Neg | Operator::Strict | Operator::Seq => "option",
                    _ => {
                        warnings.push(format!("Mermaid's {} block has one branch; a separator of `{f}` is left out", operator.name()));
                        continue;
                    }
                };
                let _ = writeln!(out, "{}{} {guard}", ind(depth.saturating_sub(1)), kw);
            }
            Event::Close(_) => {
                frags.pop();
                depth = depth.saturating_sub(1).max(1);
                let _ = writeln!(out, "{}end", ind(depth));
            }
        }
    }
    // Trailing spaces from empty guards.
    let out = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n";
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use vizcraft_diagram::sequence::Part;

    use super::super::import;
    use super::*;

    fn page(src: &str) -> Page {
        import(src).unwrap().pages.remove(0)
    }

    /// What a round trip must keep: events (as kinds and labels), sorts, guards and bars.
    fn essence(p: &Page) -> (Vec<String>, Vec<String>, Vec<(i64, i64, i64, i64)>) {
        let seq = p.sequence.as_ref().unwrap();
        let events = seq
            .events
            .iter()
            .map(|e| match e {
                Event::Message(m) => {
                    let c = p.connector(m).unwrap();
                    format!("message {} {}", c.message.as_ref().unwrap().sort.name(), c.text)
                }
                Event::Note(n) => format!("note {}", p.shape(n).unwrap().text),
                other => format!("{} {}", other.kind(), p.shape(other.id()).map(|s| s.text.as_str()).unwrap_or_default()),
            })
            .collect();
        let guards = p
            .shapes
            .iter()
            .filter_map(|s| if let Some(Part::Fragment { operator, operands, .. }) = &s.sequence { Some((operator, operands)) } else { None })
            .map(|(o, ops)| format!("{} {}", o.name(), ops.iter().map(|x| x.guard.as_str()).collect::<Vec<_>>().join("|")))
            .collect();
        let bars = p
            .shapes
            .iter()
            .filter(|s| matches!(s.sequence, Some(Part::Activation { .. })))
            .map(|s| (s.x.round() as i64, s.y.round() as i64, s.w.round() as i64, s.h.round() as i64))
            .collect();
        (events, guards, bars)
    }

    #[test]
    fn read_write_read_keeps_events_sorts_labels_guards_and_bars() {
        let src = "sequenceDiagram\n  autonumber\n  participant A as Alice\n  actor B\n  participant C\n  A->>+B: hello\n  B->>+C: query\n  C-->>-B: rows\n  Note right of C: cached\n  alt found\n    B-->>A: 200\n  else missing\n    B-->>A: 404\n  end\n  deactivate B\n  loop every minute\n    A-)C: ping\n  end\n  par one\n    A->>A: self\n  and two\n    C--)A: event\n  end\n";
        let first = page(src);
        let (text, warnings) = write(&first);
        assert!(warnings.is_empty(), "{warnings:?}");
        let second = page(&text);
        assert_eq!(essence(&second), essence(&first), "{text}");
        let (again, _) = write(&second);
        assert_eq!(again, text, "the canonical text is a fixed point");
        assert!(text.contains("  A->>+B: hello\n") && text.contains("  C-->>-B: rows\n") && text.contains("  else missing\n"), "{text}");
    }

    #[test]
    fn auto_bars_are_written_as_shorthands_and_lossy_parts_warn() {
        let src = "sequenceDiagram\n  participant A\n  participant B\n  A->>B: call\n  alt ok\n    B-->>A: yes\n  else\n    B-->>A: no\n  end\n";
        let mut p = page(src);
        assert_eq!(p.sequence.as_ref().unwrap().activations, Activations::Auto);
        let (text, _) = write(&p);
        assert!(text.contains("A->>+B: call\n") && text.contains("B-->>A: yes\n") && text.contains("B-->>-A: no\n"), "{text}");
        // A lost message and a ref fragment are lossy.
        for c in &mut p.connectors {
            if c.text == "yes" {
                c.message = Some(vizcraft_diagram::sequence::Message { sort: Sort::Lost });
            }
        }
        for s in &mut p.shapes {
            if let Some(Part::Fragment { operator, .. }) = &mut s.sequence {
                *operator = Operator::Ref;
            }
        }
        let (text, warnings) = write(&p);
        assert!(text.contains("%% lost message at B: yes"), "{text}");
        assert!(warnings.iter().any(|w| w.contains("lost")) && warnings.iter().any(|w| w.contains("ref")), "{warnings:?}");
    }
}
