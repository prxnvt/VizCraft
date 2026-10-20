//! The checks of the `idef0` pack (FIPS 183 function models) behind [`Check::Idef0`].
//!
//! A diagram is a page: the context page (A-0) has the viewpoint `idef0.Context`, decomposition
//! pages `idef0.Decomposition`. A decomposition page keeps its node number in
//! `Page.ext["idef0.node"]` and the box it details in `Page.ext["idef0.parent"]`. Box ports are
//! the builder's `I1`.., `C1`.., `O1`.., `M1`.. and `call`; an arrow's ends name them. Messages
//! state only the finding.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::json;
use vizcraft_analysis::mech::{number, text};
use vizcraft_diagram::Document;
use vizcraft_model::issue::Fix;
use vizcraft_model::{Element, Issue, Relationship};

use crate::Catalog;
use crate::infer::Facts;
use crate::mech_rules::{finding, label, views};
use crate::schema::RuleDecl;

/// The checks of `Check::Idef0`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Idef0Test {
    /// A context page without exactly one box, numbered 0.
    ContextOneBox,
    /// A decomposition page with fewer than three boxes.
    BoxCount,
    /// Decomposition box numbers that are not 1 to n, or not in staircase order.
    BoxNumbers,
    /// A box with no control arrow in, or no output arrow out.
    ControlAndOutput,
    /// A box with more than one call arrow.
    OneCall,
    /// A boundary arrow whose ICOM code does not match an arrow of the parent box.
    IcomCodes,
    /// A feedback arrow not routed up and over (control) or down and under (input, mechanism).
    FeedbackRouting,
    /// A test this build does not know (from a newer pack); skipped.
    #[default]
    #[serde(other)]
    Unknown,
}

impl Idef0Test {
    /// Whether this build knows the test.
    pub fn known(self) -> bool {
        self != Idef0Test::Unknown
    }
}

/// The activity box type.
pub const ACTIVITY: &str = "idef0.Activity";
/// A boundary arrow end.
pub const BOUNDARY: &str = "idef0.Boundary";
/// An arrow.
pub const ARROW: &str = "idef0.Arrow";
/// A call arrow.
pub const CALL: &str = "idef0.Call";
/// The context viewpoint (A-0).
pub const CONTEXT: &str = "idef0.Context";
/// The decomposition viewpoint.
pub const DECOMPOSITION: &str = "idef0.Decomposition";
/// The page key holding the node number.
pub const NODE_KEY: &str = "idef0.node";
/// The page key holding the parent box's element id.
pub const PARENT_KEY: &str = "idef0.parent";

/// The box number as text (`0`, `3`): its `number` property, a number or text.
pub fn box_number(e: &Element) -> Option<u32> {
    number(&e.properties, "number").filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n < 1000.0).map(|n| n as u32)
}

/// The ICOM letter and number of a code (`I1` gives `('I', 1)`).
pub fn icom(code: &str) -> Option<(char, u32)> {
    let mut chars = code.trim().chars();
    let letter = chars.next().filter(|c| matches!(c, 'I' | 'C' | 'O' | 'M'))?;
    let n: u32 = chars.as_str().parse().ok().filter(|n| (1..100).contains(n))?;
    Some((letter, n))
}

/// The side letter of a port id (`C2` gives `C`); `None` for no port or the call port.
pub fn port_letter(port: Option<&str>) -> Option<char> {
    port.and_then(|p| p.chars().next()).filter(|c| matches!(c, 'I' | 'C' | 'O' | 'M'))
}

/// The code a boundary carries: its name, else its tag.
pub fn code_of(e: &Element) -> String {
    let n = e.name.trim();
    if n.is_empty() { e.tag.trim().to_string() } else { n.to_string() }
}

/// Whether an arrow is tunnelled at either end.
pub fn tunnelled(r: &Relationship) -> bool {
    matches!(text(&r.properties, "tunnel"), "start" | "end")
}

/// The arrows at each ICOM letter of `box_id`, in model order: inputs, controls and mechanisms
/// coming in, outputs going out.
pub fn arrows_by_side<'m>(rels: &'m [Relationship], facts: &Facts<'_>, box_id: &str) -> BTreeMap<char, Vec<&'m Relationship>> {
    let mut out: BTreeMap<char, Vec<&Relationship>> = BTreeMap::new();
    for r in rels.iter().filter(|r| facts.is_a(&r.type_id, ARROW)) {
        if r.target == box_id
            && let Some(l) = port_letter(r.target_port.as_deref()).filter(|l| *l != 'O')
        {
            out.entry(l).or_default().push(r);
        }
        if r.source == box_id && port_letter(r.source_port.as_deref()) == Some('O') {
            out.entry('O').or_default().push(r);
        }
    }
    out
}

pub(crate) fn check(doc: &Document, cat: &Catalog, rule: &RuleDecl, test: Idef0Test, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let views = views(doc);
    let model = &doc.model;
    let push =
        |out: &mut Vec<Issue>, what: String, subjects: Vec<String>, on: Option<usize>| out.push(finding(doc, &views, rule, &what, subjects, on));
    let viewpoint = |pi: usize| doc.pages.get(pi).and_then(|p| p.view.as_ref()).map(|v| v.viewpoint.as_str()).unwrap_or_default();
    // Boxes drawn on each page, with their shapes.
    let boxes_on = |pi: usize| -> Vec<(&Element, &vizcraft_diagram::Shape)> {
        let Some(p) = doc.pages.get(pi) else { return Vec::new() };
        p.shapes
            .iter()
            .filter_map(|s| s.element.as_deref().and_then(|e| facts.element(e)).filter(|e| facts.is_a(&e.type_id, ACTIVITY)).map(|e| (e, s)))
            .collect()
    };
    match test {
        Idef0Test::ContextOneBox => {
            for pi in (0..doc.pages.len()).filter(|pi| viewpoint(*pi) == CONTEXT) {
                let boxes = boxes_on(pi);
                if boxes.len() != 1 {
                    let what = format!("The context diagram has {} boxes; A-0 has exactly one, numbered 0.", boxes.len());
                    push(out, what, boxes.iter().map(|(e, _)| e.id.clone()).collect(), Some(pi));
                } else if let Some((e, _)) = boxes.first()
                    && box_number(e) != Some(0)
                {
                    let mut i = finding(
                        doc,
                        &views,
                        rule,
                        &format!("{} on the context diagram is numbered {}; A-0's box is 0.", label(cat, e), shown(e)),
                        vec![e.id.clone()],
                        Some(pi),
                    );
                    i = i.with_fix(Fix::new("Renumber the boxes", "idef0.renumber", json!({"page": doc.pages.get(pi).map(|p| p.id.clone())})));
                    out.push(i);
                }
            }
        }
        Idef0Test::BoxCount => {
            for pi in (0..doc.pages.len()).filter(|pi| viewpoint(*pi) == DECOMPOSITION) {
                let boxes = boxes_on(pi);
                if (1..3).contains(&boxes.len()) {
                    let node = doc.pages.get(pi).and_then(|p| p.ext.get(NODE_KEY)).cloned().unwrap_or_default();
                    let what = format!(
                        "Diagram {} has {} box(es); a decomposition has 3 to 6.",
                        if node.is_empty() { "here".into() } else { node },
                        boxes.len()
                    );
                    push(out, what, boxes.iter().map(|(e, _)| e.id.clone()).collect(), Some(pi));
                }
            }
        }
        Idef0Test::BoxNumbers => {
            for pi in (0..doc.pages.len()).filter(|pi| viewpoint(*pi) == DECOMPOSITION) {
                let boxes = boxes_on(pi);
                if boxes.is_empty() {
                    continue;
                }
                let mut numbers: Vec<Option<u32>> = boxes.iter().map(|(e, _)| box_number(e)).collect();
                numbers.sort_unstable();
                let want: Vec<Option<u32>> = (1..=u32::try_from(boxes.len()).unwrap_or(0)).map(Some).collect();
                let fix = Fix::new("Renumber the boxes", "idef0.renumber", json!({"page": doc.pages.get(pi).map(|p| p.id.clone())}));
                let all: Vec<String> = boxes.iter().map(|(e, _)| e.id.clone()).collect();
                if numbers != want {
                    let what = format!("The boxes are not numbered 1 to {}.", boxes.len());
                    out.push(finding(doc, &views, rule, &what, all, Some(pi)).with_fix(fix));
                    continue;
                }
                let mut by_number: Vec<(u32, f64, f64)> = boxes
                    .iter()
                    .filter_map(|(e, s)| {
                        let r = s.bounds();
                        box_number(e).map(|n| (n, (r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0))
                    })
                    .collect();
                by_number.sort_by_key(|(n, _, _)| *n);
                let staircase = by_number.windows(2).all(|w| match w {
                    [(_, x0, y0), (_, x1, y1)] => x1 > x0 && *y1 >= *y0 - 1.0,
                    _ => true,
                });
                if !staircase {
                    out.push(
                        finding(doc, &views, rule, "The boxes do not step down to the right in number order (staircase).", all, Some(pi))
                            .with_fix(fix),
                    );
                }
            }
        }
        Idef0Test::ControlAndOutput => {
            for e in model.elements.iter().filter(|e| facts.is_a(&e.type_id, ACTIVITY) && views.contains_key(e.id.as_str())) {
                let sides = arrows_by_side(&model.relationships, &facts, &e.id);
                let missing: Vec<&str> =
                    [('C', "control"), ('O', "output")].iter().filter(|(l, _)| !sides.contains_key(l)).map(|(_, w)| *w).collect();
                if !missing.is_empty() {
                    let what =
                        format!("{} has no {} arrow; every box takes at least one control and one output.", label(cat, e), missing.join(" or "));
                    push(out, what, vec![e.id.clone()], None);
                }
            }
        }
        Idef0Test::OneCall => {
            for e in model.elements.iter().filter(|e| facts.is_a(&e.type_id, ACTIVITY)) {
                let calls = model.relationships.iter().filter(|r| r.source == e.id && facts.is_a(&r.type_id, CALL)).count();
                if calls > 1 {
                    push(out, format!("{} has {calls} call arrows; a box has at most one.", label(cat, e)), vec![e.id.clone()], None);
                }
            }
        }
        Idef0Test::IcomCodes => icom_codes(doc, cat, &facts, &views, rule, out),
        Idef0Test::FeedbackRouting => {
            for (pi, page) in doc.pages.iter().enumerate() {
                for c in &page.connectors {
                    let Some(r) = c.relationship.as_deref().and_then(|r| model.relationship(r)).filter(|r| facts.is_a(&r.type_id, ARROW)) else {
                        continue;
                    };
                    let (Some(src), Some(tgt)) = (facts.element(&r.source), facts.element(&r.target)) else { continue };
                    let (Some(a), Some(b)) = (box_number(src), box_number(tgt)) else { continue };
                    if !facts.is_a(&src.type_id, ACTIVITY) || !facts.is_a(&tgt.type_id, ACTIVITY) || a <= b {
                        continue;
                    }
                    let (Some(sa), Some(sb)) = (
                        page.shapes.iter().find(|s| s.element.as_deref() == Some(src.id.as_str())),
                        page.shapes.iter().find(|s| s.element.as_deref() == Some(tgt.id.as_str())),
                    ) else {
                        continue;
                    };
                    let Some(route) = vizcraft_diagram::route_path(page, c).bounds() else { continue };
                    let (ra, rb) = (sa.bounds(), sb.bounds());
                    let (over, under) = (route.y0 < ra.y0.min(rb.y0) - 1.0, route.y1 > ra.y1.max(rb.y1) + 1.0);
                    let what = match port_letter(r.target_port.as_deref()) {
                        Some('C') if !over => Some("goes back to a control without going up and over the boxes"),
                        Some('I' | 'M') if !under => Some("goes back to an input or mechanism without going down and under the boxes"),
                        _ => None,
                    };
                    if let Some(what) = what {
                        let name = if r.name.trim().is_empty() {
                            "A feedback arrow".to_string()
                        } else {
                            format!("Feedback arrow \"{}\"", crate::rules::cut(r.name.trim(), 40))
                        };
                        push(out, format!("{name} from box {a} to box {b} {what}."), vec![r.id.clone()], Some(pi));
                    }
                }
            }
        }
        Idef0Test::Unknown => {}
    }
}

fn shown(e: &Element) -> String {
    box_number(e).map_or_else(|| "nothing".into(), |n| n.to_string())
}

/// Boundary codes on each decomposition page against the arrows of the box it details.
fn icom_codes(doc: &Document, cat: &Catalog, facts: &Facts<'_>, views: &HashMap<&str, Vec<(usize, &str)>>, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let model = &doc.model;
    for (pi, page) in doc.pages.iter().enumerate() {
        let Some(parent) = page.ext.get(PARENT_KEY).and_then(|id| facts.element(id)) else { continue };
        let sides = arrows_by_side(&model.relationships, facts, &parent.id);
        let fix = Fix::new("Recompute the ICOM codes", "idef0.icom", json!({"page": page.id}));
        let mut used: HashMap<(char, u32), Vec<String>> = HashMap::new();
        for s in &page.shapes {
            let Some(b) = s.element.as_deref().and_then(|e| facts.element(e)).filter(|e| facts.is_a(&e.type_id, BOUNDARY)) else { continue };
            let arrows: Vec<&Relationship> =
                model.relationships.iter().filter(|r| (r.source == b.id || r.target == b.id) && facts.is_a(&r.type_id, ARROW)).collect();
            if arrows.iter().any(|r| tunnelled(r)) {
                continue;
            }
            let code = code_of(b);
            let Some((letter, n)) = icom(&code) else {
                let what = format!("Boundary arrow end \"{}\" has no ICOM code (I, C, O or M and a number).", crate::rules::cut(&code, 20));
                out.push(finding(doc, views, rule, &what, vec![b.id.clone()], Some(pi)).with_fix(fix.clone()));
                continue;
            };
            used.entry((letter, n)).or_default().push(b.id.clone());
            let have = sides.get(&letter).map_or(0, Vec::len);
            if n as usize > have {
                let what = format!("Boundary {code} has no match: {} has {have} {} arrow(s).", label(cat, parent), side_word(letter));
                out.push(finding(doc, views, rule, &what, vec![b.id.clone()], Some(pi)).with_fix(fix.clone()));
            }
        }
        let mut twice: Vec<(&(char, u32), &Vec<String>)> = used.iter().filter(|(_, v)| v.len() > 1).collect();
        twice.sort();
        for ((l, n), ids) in twice {
            out.push(
                finding(doc, views, rule, &format!("ICOM code {l}{n} is used by {} boundary arrows.", ids.len()), ids.clone(), Some(pi))
                    .with_fix(fix.clone()),
            );
        }
        for (letter, arrows) in &sides {
            for (k, r) in arrows.iter().enumerate() {
                let n = u32::try_from(k + 1).unwrap_or(u32::MAX);
                if !used.contains_key(&(*letter, n)) && !tunnelled(r) {
                    let what = format!(
                        "{} has {} arrow {letter}{n} that this diagram does not show; draw it or tunnel it.",
                        label(cat, parent),
                        side_word(*letter)
                    );
                    let mut subjects = vec![r.id.clone()];
                    subjects.extend(
                        page.shapes
                            .iter()
                            .filter_map(|s| s.element.clone())
                            .filter(|e| facts.element(e).is_some_and(|x| facts.is_a(&x.type_id, ACTIVITY)))
                            .take(1),
                    );
                    out.push(finding(doc, views, rule, &what, subjects, Some(pi)));
                }
            }
        }
    }
}

fn side_word(letter: char) -> &'static str {
    match letter {
        'I' => "input",
        'C' => "control",
        'O' => "output",
        _ => "mechanism",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icom_codes_read() {
        assert_eq!(icom("I1"), Some(('I', 1)));
        assert_eq!(icom(" C12 "), Some(('C', 12)));
        assert_eq!(icom("X1"), None);
        assert_eq!(icom("I0"), None);
        assert_eq!(icom("O"), None);
        assert_eq!(port_letter(Some("M2")), Some('M'));
        assert_eq!(port_letter(Some("call")), None);
    }
}
