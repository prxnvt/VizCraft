//! The checks of the process pack, and the generic net test it uses. [`check`] is the rules
//! engine's one dispatch arm for them.
//!
//! - `netCheck` with test `unconnectedPin`: a connection point of a shape of the listed types with
//!   nothing glued to it and no other shape's connection point on it (a cap or a flange set
//!   against it closes it): ISO 10628-1 asks every nozzle and valve port to be connected.
//! - `instrumentTag`: functional letters that do not read as ANSI/ISA-5.1 identification letters,
//!   or whose first letter is not the loop's measured variable.
//! - `loopComplete`: a control loop without a measuring element, or with no final element
//!   reachable from it along signal lines.
//! - `loopSheet`: an ISA-5.4 loop diagram showing more than one loop, missing one of its loop's
//!   instruments, with a terminal landed by a single wire, or an I/O channel used twice.
//! - `streamsNumbered`: a process flow diagram stream without a number.
//! - `offPageContinuity`: off-page connector partners on one sheet, or on different lines.

use std::collections::{BTreeMap, HashMap, HashSet};

use vizcraft_diagram::{Document, End, Point};
use vizcraft_model::{Element, Issue, PropValue};

use crate::Catalog;
use crate::infer::Facts;
use crate::rules::{cut, message};
use crate::schema::{Check, Pack, RuleDecl};

/// Longest name or tag quoted in a message, in bytes.
const MAX_LABEL: usize = 60;
/// How close, in page units, two connection points must be to count as joined.
const TOUCH: f64 = 2.0;
/// The loop element type.
pub const LOOP: &str = "process.InstrumentationLoopFunction";
/// The instrument function type.
pub const INSTRUMENT: &str = "process.ProcessInstrumentationFunction";
/// The inline primary element type.
pub const PRIMARY: &str = "process.ProcessSignalGeneratingFunction";
/// The piping component supertype (valves and fittings).
pub const COMPONENT: &str = "process.PipingComponent";
/// The signal relationship type.
pub const SIGNAL: &str = "process.Signal";
/// The off-page continuation relationship type.
pub const CONTINUES: &str = "process.Continues";
/// The terminal type of loop diagrams.
pub const TERMINAL: &str = "process.Terminal";
/// The I/O channel type of loop diagrams.
pub const IO_CHANNEL: &str = "process.IOChannel";

/// ISA-5.1 first letters (measured or initiating variable), the user's-choice ones left out.
pub const FIRST: &str = "ABEFHIJKLPQRSTUVWXYZ";
/// ISA-5.1 first letters left to the user's choice; valid only when a project defines them.
pub const USER_CHOICE: &str = "CDGMNO";
/// ISA-5.1 variable modifiers.
pub const MODIFIERS: &str = "DFKQSXYZ";
/// ISA-5.1 readout or passive function letters.
pub const READOUT: &str = "AEGIJLOPRUWX";
/// ISA-5.1 output or active function letters.
pub const OUTPUT: &str = "CKSTUVXYZ";
/// ISA-5.1 function modifiers.
pub const FUNCTION_MODIFIERS: &str = "CDHLMORS";

/// Run one of the checks of this module; other checks are ignored.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let facts = Facts::new(&doc.model, cat);
    let views = views(doc);
    let ctx = Ctx { doc, pack, facts: &facts, views: &views, rule };
    match &rule.check {
        Check::NetCheck { test, types, .. } if test == "unconnectedPin" => ctx.unconnected(types, out),
        Check::InstrumentTag { types, allowed } => ctx.instrument_tags(types, allowed, out),
        Check::LoopComplete { types } => ctx.loops_complete(types, out),
        Check::LoopSheet { viewpoints } => ctx.loop_sheets(viewpoints, out),
        Check::StreamsNumbered { viewpoints, relationship } => ctx.streams(viewpoints, relationship, out),
        Check::OffPageContinuity { types } => ctx.continuity(types, out),
        _ => {}
    }
}

/// Shape views by element id: (page index, shape id).
fn views(doc: &Document) -> HashMap<&str, Vec<(usize, &str)>> {
    let mut m: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            if let Some(e) = s.element.as_deref() {
                m.entry(e).or_default().push((pi, s.id.as_str()));
            }
        }
    }
    m
}

/// Whether `letters` read as ISA-5.1 identification letters: a first letter (a user's-choice one
/// only when `allowed` lists it), an optional variable modifier, then at least one succeeding
/// letter, readout letters before output letters before function modifiers.
pub fn letters_read(letters: &str, allowed: &[String]) -> bool {
    let chars: Vec<char> = letters.trim().chars().collect();
    if !(2..=8).contains(&chars.len()) || !chars.iter().all(char::is_ascii_uppercase) {
        return false;
    }
    let Some((first, rest)) = chars.split_first() else { return false };
    let user = USER_CHOICE.contains(*first) && allowed.iter().any(|a| a.trim() == first.to_string());
    if !FIRST.contains(*first) && !user {
        return false;
    }
    let with_modifier = rest.split_first().is_some_and(|(m, tail)| MODIFIERS.contains(*m) && succeeding(tail));
    with_modifier || succeeding(rest)
}

/// Whether `letters` are succeeding letters in class order (readout, output, function modifier):
/// each letter takes the lowest class at or after the last one that holds it.
fn succeeding(letters: &[char]) -> bool {
    if letters.is_empty() {
        return false;
    }
    let classes = [READOUT, OUTPUT, FUNCTION_MODIFIERS];
    let mut level = 0usize;
    for c in letters {
        match classes.iter().enumerate().skip(level).find(|(_, set)| set.contains(*c)) {
            Some((i, _)) => level = i,
            None => return false,
        }
    }
    true
}

/// The succeeding letters of a tag's letters: after the first and a variable modifier, when the
/// rest reads without it only as a modifier would.
fn functions(letters: &str) -> String {
    let t = letters.trim();
    t.chars().skip(1).collect()
}

/// One run's view of the document for these checks.
struct Ctx<'a> {
    doc: &'a Document,
    pack: &'a Pack,
    facts: &'a Facts<'a>,
    views: &'a HashMap<&'a str, Vec<(usize, &'a str)>>,
    rule: &'a RuleDecl,
}

impl<'a> Ctx<'a> {
    fn covers(&self, types: &[String], type_id: &str) -> bool {
        if types.is_empty() { self.facts.of_pack(type_id, self.pack) } else { self.facts.lists(self.pack, types, type_id) }
    }

    fn label(&self, e: &Element) -> String {
        let ty = self
            .facts
            .element_type(&e.type_id)
            .map(|t| t.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| vizcraft_model::local_of(&e.type_id).to_string());
        let handle = if !e.tag.trim().is_empty() {
            e.tag.trim().to_string()
        } else if !e.name.trim().is_empty() {
            format!("\"{}\"", e.name.trim())
        } else {
            e.id.clone()
        };
        format!("{ty} {}", cut(&handle.replace(['\n', '\r'], " "), MAX_LABEL))
    }

    fn issue(&self, what: &str, subjects: Vec<String>, items: Vec<(usize, &str)>) -> Issue {
        let page = match items.first() {
            Some((p, _)) if items.iter().all(|(q, _)| q == p) => self.doc.pages.get(*p).map(|pg| pg.id.clone()),
            _ => None,
        };
        let mut seen = HashSet::new();
        let items: Vec<String> = items.into_iter().map(|(_, id)| id.to_string()).filter(|id| seen.insert(id.clone())).collect();
        let mut i =
            Issue::new(self.rule.id.clone(), self.rule.severity, message(&self.rule.description, what)).with_subjects(subjects).with_items(items);
        i.page = page;
        i
    }

    fn views_of(&self, id: &str) -> Vec<(usize, &'a str)> {
        self.views.get(id).cloned().unwrap_or_default()
    }

    /// The loop that owns `e`, directly or through other owners.
    fn loop_of(&self, e: &Element) -> Option<&'a Element> {
        self.facts.ancestors(&e.id).into_iter().filter_map(|a| self.facts.element(a)).find(|a| self.facts.is_a(&a.type_id, LOOP))
    }

    // ------------------------------------------------------------ unconnected pins

    fn unconnected(&self, types: &[String], out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let candidates: Vec<&vizcraft_diagram::Shape> = page
                .shapes
                .iter()
                .filter(|s| s.element.as_deref().and_then(|e| self.facts.element(e)).is_some_and(|e| self.covers(types, &e.type_id)))
                .collect();
            if candidates.is_empty() {
                continue;
            }
            // Glued points, and dynamic glue resolved to the nearest point where the route ends.
            let mut glued: HashSet<(&str, usize)> = HashSet::new();
            for c in &page.connectors {
                let poly = vizcraft_diagram::route_polyline(page, c);
                for (end, at) in [(&c.start, poly.first()), (&c.end, poly.last())] {
                    let End::Glued { shape, point, .. } = end else { continue };
                    match point {
                        Some(i) => {
                            glued.insert((shape.as_str(), *i));
                        }
                        None => {
                            let Some(sh) = page.shape(shape) else { continue };
                            let Some(at) = at else { continue };
                            let nearest = sh
                                .connection_points_world()
                                .iter()
                                .enumerate()
                                .min_by(|a, b| a.1.distance(*at).total_cmp(&b.1.distance(*at)))
                                .map(|(i, _)| i);
                            if let Some(i) = nearest {
                                glued.insert((shape.as_str(), i));
                            }
                        }
                    }
                }
            }
            let all_points: Vec<(&str, Point)> =
                page.shapes.iter().flat_map(|s| s.connection_points_world().into_iter().map(move |p| (s.id.as_str(), p))).collect();
            for s in candidates {
                let Some(e) = s.element.as_deref().and_then(|e| self.facts.element(e)) else { continue };
                let ports: Vec<String> = s.glyph.as_ref().map(|g| g.ports.clone()).unwrap_or_default();
                for (i, p) in s.connection_points_world().into_iter().enumerate() {
                    if glued.contains(&(s.id.as_str(), i)) {
                        continue;
                    }
                    if all_points.iter().any(|(other, q)| *other != s.id && q.distance(p) <= TOUCH) {
                        continue;
                    }
                    let name = ports.get(i).cloned().unwrap_or_else(|| (i + 1).to_string());
                    let what = format!("{} has port {name} with nothing connected.", self.label(e));
                    out.push(self.issue(&what, vec![e.id.clone()], vec![(pi, s.id.as_str())]));
                }
            }
        }
    }

    // ------------------------------------------------------------ instrument letters

    fn instrument_tags(&self, types: &[String], allowed: &[String], out: &mut Vec<Issue>) {
        for e in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id)) {
            let letters = e.prop("letters").map(PropValue::display).unwrap_or_default();
            let letters = letters.trim();
            if letters.is_empty() {
                continue;
            }
            let items = self.views_of(&e.id);
            if !letters_read(letters, allowed) {
                let what = format!("{} has letters {} that do not read as identification letters.", self.label(e), cut(letters, MAX_LABEL));
                out.push(self.issue(&what, vec![e.id.clone()], items));
                continue;
            }
            let Some(lp) = self.loop_of(e) else { continue };
            let variable = lp.prop("variable").map(PropValue::display).unwrap_or_default();
            let variable = variable.trim();
            if !variable.is_empty() && !letters.starts_with(variable) {
                let what = format!(
                    "{} starts with {} but loop {} measures {}.",
                    self.label(e),
                    letters.chars().next().map(String::from).unwrap_or_default(),
                    cut(&lp.tag, MAX_LABEL),
                    cut(variable, MAX_LABEL)
                );
                out.push(self.issue(&what, vec![e.id.clone(), lp.id.clone()], items));
            }
        }
    }

    // ------------------------------------------------------------ loops

    /// Whether `e` measures: an inline primary element, or an instrument whose succeeding
    /// letters hold a primary element (E) or a transmitter (T).
    fn measures(&self, e: &Element) -> bool {
        if self.facts.is_a(&e.type_id, PRIMARY) {
            return true;
        }
        self.facts.is_a(&e.type_id, INSTRUMENT) && functions(&e.prop("letters").map(PropValue::display).unwrap_or_default()).contains(['E', 'T'])
    }

    /// Whether `e` is a final element: a valve or fitting, or an instrument whose succeeding
    /// letters hold a valve (V).
    fn is_final(&self, e: &Element) -> bool {
        self.facts.is_a(&e.type_id, COMPONENT)
            || (self.facts.is_a(&e.type_id, INSTRUMENT) && functions(&e.prop("letters").map(PropValue::display).unwrap_or_default()).contains('V'))
    }

    fn loops_complete(&self, types: &[String], out: &mut Vec<Issue>) {
        let loop_types: Vec<String> = if types.is_empty() { vec![LOOP.to_string()] } else { types.to_vec() };
        // Signal lines and off-page pairs, both ways.
        let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
        for r in self.doc.model.relationships.iter().filter(|r| r.type_id == SIGNAL || r.type_id == CONTINUES) {
            graph.entry(r.source.as_str()).or_default().push(r.target.as_str());
            graph.entry(r.target.as_str()).or_default().push(r.source.as_str());
        }
        for lp in self.doc.model.elements.iter().filter(|e| self.facts.lists(self.pack, &loop_types, &e.type_id)) {
            let kind = lp.prop("loopType").map(PropValue::display).unwrap_or_else(|| "control".into());
            if kind != "control" {
                continue;
            }
            let members: Vec<&Element> = self.doc.model.descendants(&lp.id).iter().filter_map(|id| self.facts.element(id)).collect();
            let mut subjects = vec![lp.id.clone()];
            subjects.extend(members.iter().map(|m| m.id.clone()));
            let items: Vec<(usize, &str)> = members.iter().flat_map(|m| self.views_of(&m.id)).collect();
            let starts: Vec<&str> = members.iter().filter(|m| self.measures(m)).map(|m| m.id.as_str()).collect();
            if starts.is_empty() {
                let what = format!("Loop {} has no measuring element (a primary element or a transmitter).", cut(&lp.tag, MAX_LABEL));
                out.push(self.issue(&what, subjects, items));
                continue;
            }
            let mut seen: HashSet<&str> = starts.iter().copied().collect();
            let mut stack: Vec<&str> = starts.clone();
            let mut reached = false;
            while let Some(n) = stack.pop() {
                if self.facts.element(n).is_some_and(|e| self.is_final(e)) && !starts.contains(&n) {
                    reached = true;
                    break;
                }
                for next in graph.get(n).map(Vec::as_slice).unwrap_or_default() {
                    if seen.len() < 100_000 && seen.insert(next) {
                        stack.push(next);
                    }
                }
            }
            if !reached {
                let what = format!("Loop {} reaches no final element along its signal lines.", cut(&lp.tag, MAX_LABEL));
                out.push(self.issue(&what, subjects, items));
            }
        }
    }

    fn loop_sheets(&self, viewpoints: &[String], out: &mut Vec<Issue>) {
        let mut channels: BTreeMap<String, Vec<(String, (usize, &str))>> = BTreeMap::new();
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let Some(view) = &page.view else { continue };
            if !viewpoints.is_empty() && !viewpoints.contains(&view.viewpoint) {
                continue;
            }
            let mut loops: Vec<&Element> = Vec::new();
            let mut drawn: HashSet<&str> = HashSet::new();
            for s in &page.shapes {
                let Some(e) = s.element.as_deref().and_then(|e| self.facts.element(e)) else { continue };
                drawn.insert(e.id.as_str());
                if let Some(lp) = self.loop_of(e)
                    && !loops.iter().any(|l| l.id == lp.id)
                {
                    loops.push(lp);
                }
                if self.facts.is_a(&e.type_id, IO_CHANNEL) && !e.tag.is_empty() {
                    channels.entry(e.tag.clone()).or_default().push((e.id.clone(), (pi, s.id.as_str())));
                }
                if self.facts.is_a(&e.type_id, TERMINAL) {
                    let wires = page.connectors.iter().filter(|c| c.touches(&s.id)).count();
                    if wires == 1 {
                        let what = format!("{} is landed by a single wire.", self.label(e));
                        out.push(self.issue(&what, vec![e.id.clone()], vec![(pi, s.id.as_str())]));
                    }
                }
            }
            if loops.len() > 1 {
                let tags: Vec<String> = loops.iter().map(|l| cut(&l.tag, MAX_LABEL)).collect();
                let items: Vec<(usize, &str)> = page.shapes.iter().map(|s| (pi, s.id.as_str())).take(64).collect();
                let what = format!("Sheet {} shows loops {}; a loop diagram holds one loop.", cut(&page.name, MAX_LABEL), tags.join(", "));
                out.push(self.issue(&what, loops.iter().map(|l| l.id.clone()).collect(), items));
            } else if let Some(lp) = loops.first() {
                for m in self.doc.model.descendants(&lp.id).iter().filter_map(|id| self.facts.element(id)) {
                    let tagged = self.facts.is_a(&m.type_id, INSTRUMENT) || (self.facts.is_a(&m.type_id, COMPONENT) && !m.tag.is_empty());
                    if tagged && !drawn.contains(m.id.as_str()) {
                        let what =
                            format!("{} of loop {} is missing from sheet {}.", self.label(m), cut(&lp.tag, MAX_LABEL), cut(&page.name, MAX_LABEL));
                        out.push(self.issue(&what, vec![m.id.clone(), lp.id.clone()], Vec::new()));
                    }
                }
            }
        }
        for (tag, uses) in channels {
            let ids: HashSet<&str> = uses.iter().map(|(id, _)| id.as_str()).collect();
            if ids.len() > 1 {
                let what = format!("I/O channel {} is used {} times.", cut(&tag, MAX_LABEL), ids.len());
                let mut subjects: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
                subjects.sort();
                out.push(self.issue(&what, subjects, uses.iter().map(|(_, it)| *it).collect()));
            }
        }
    }

    // ------------------------------------------------------------ streams

    fn streams(&self, viewpoints: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let rel = if relationship.is_empty() { "process.Stream" } else { relationship };
        for (pi, page) in self.doc.pages.iter().enumerate() {
            let Some(view) = &page.view else { continue };
            if !viewpoints.is_empty() && !viewpoints.contains(&view.viewpoint) {
                continue;
            }
            for c in &page.connectors {
                let Some(r) = c.relationship.as_deref().and_then(|r| self.doc.model.relationship(r)) else { continue };
                if r.type_id != rel || !r.name.trim().is_empty() {
                    continue;
                }
                let from = self.facts.element(&r.source).map(|e| self.label(e)).unwrap_or_default();
                let to = self.facts.element(&r.target).map(|e| self.label(e)).unwrap_or_default();
                let what = format!("The stream from {from} to {to} has no number.");
                out.push(self.issue(&what, vec![r.id.clone()], vec![(pi, c.id.as_str())]));
            }
        }
    }

    // ------------------------------------------------------------ off-page continuity

    /// The lines of the pipe segments glued to the views of `e`.
    fn lines_at(&self, e: &str) -> HashSet<&'a str> {
        let mut out = HashSet::new();
        for (pi, sid) in self.views_of(e) {
            let Some(page) = self.doc.pages.get(pi) else { continue };
            for c in page.connectors.iter().filter(|c| c.touches(sid)) {
                if let Some(l) = c.ext.get(crate::process_pass::EXT_LINE) {
                    out.insert(l.as_str());
                }
            }
        }
        out
    }

    fn continuity(&self, types: &[String], out: &mut Vec<Issue>) {
        let mut by_tag: BTreeMap<(&str, &str), Vec<&Element>> = BTreeMap::new();
        for e in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id) && !e.tag.trim().is_empty()) {
            by_tag.entry((e.type_id.as_str(), e.tag.as_str())).or_default().push(e);
        }
        for ((_, tag), group) in by_tag {
            let [a, b] = group.as_slice() else { continue };
            let (va, vb) = (self.views_of(&a.id), self.views_of(&b.id));
            let mut items = va.clone();
            items.extend(vb.iter().copied());
            let subjects = vec![a.id.clone(), b.id.clone()];
            if va.iter().any(|(p, _)| vb.iter().any(|(q, _)| p == q)) {
                let what = format!("Off-page connectors {} are drawn on the same sheet.", cut(tag, MAX_LABEL));
                out.push(self.issue(&what, subjects, items));
                continue;
            }
            let (la, lb) = (self.lines_at(&a.id), self.lines_at(&b.id));
            if !la.is_empty() && !lb.is_empty() && la.is_disjoint(&lb) {
                let what = format!("Off-page connectors {} join different lines.", cut(tag, MAX_LABEL));
                out.push(self.issue(&what, subjects, items));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isa_letters_read_by_class_order() {
        for ok in ["LIC", "FT", "TT", "LV", "PSV", "PSHH", "LAHH", "FQI", "PDT", "FE", "ZSC", "HS", "FCV", "TIC", "LG", "FO", "XV"] {
            assert!(letters_read(ok, &[]), "{ok}");
        }
        for bad in ["", "L", "lic", "LQB", "CIC", "QQQQQQQQQ", "L-IC", "FHI", "Ø"] {
            assert!(!letters_read(bad, &[]), "{bad}");
        }
        assert!(letters_read("CIC", &["C".to_string()]), "a user's-choice letter the project defines");
    }
}
