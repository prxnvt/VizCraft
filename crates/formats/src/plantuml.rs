//! PlantUML text from a UML page (writing only), from the public PlantUML language reference.
//!
//! The page's viewpoint picks the diagram: class, component, package, deployment and use case
//! pages are written as declarations (with nested blocks for owners) and links; activity pages as
//! the structured activity syntax (`start`, `:action;`, `if`, `fork`, `stop`); state machine
//! pages as `state` declarations and transitions, with `[*]` for initial and final states.
//! Element ids become aliases, so names may hold any text (quotes and line breaks are replaced).
//! Anything the text form cannot hold comes back as a warning: an activity edge outside the
//! structured flow, a junction written as a choice. Output is capped at [`MAX_OUTPUT`] bytes.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::Document;
use vizcraft_notation::uml_text::{Shown, format_member};

use crate::FormatError;
use crate::uml::{self, Family, UmlElement, UmlPage, UmlRelationship, alias, one_line};

/// Largest text written, in bytes.
pub const MAX_OUTPUT: usize = 2 * 1024 * 1024;

/// Longest name written, in bytes.
const MAX_NAME: usize = 256;

/// Deepest nesting or flow walked.
const MAX_DEPTH: usize = 64;

/// Write page `page` of `doc` as PlantUML: the text and the warnings.
pub fn write(doc: &Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    let p = uml::read(doc, page)?;
    let mut w = Writer::new(&p);
    w.line(0, "@startuml");
    match p.family() {
        Family::Structure => w.structure(),
        Family::Activity => w.activity(),
        Family::State => w.state(),
    }
    w.line(0, "@enduml");
    if w.out.len() > MAX_OUTPUT {
        return Err(FormatError::Encode(format!("the PlantUML text is larger than {} MiB", MAX_OUTPUT / (1024 * 1024))));
    }
    Ok((w.out, w.warnings))
}

/// A name in double quotes, on one line, with the quote character replaced.
fn quoted(name: &str) -> String {
    format!("\"{}\"", one_line(name, MAX_NAME).replace('"', "'"))
}

/// Text after a `:` (labels and members), on one line.
fn label(text: &str) -> String {
    one_line(text, MAX_NAME)
}

struct Writer<'a> {
    page: &'a UmlPage,
    aliases: HashMap<&'a str, String>,
    out: String,
    warnings: Vec<String>,
}

impl<'a> Writer<'a> {
    fn new(page: &'a UmlPage) -> Self {
        let mut used = HashSet::new();
        let aliases = page.elements.iter().map(|e| (e.id.as_str(), alias(&e.id, &mut used))).collect();
        Writer { page, aliases, out: String::new(), warnings: Vec::new() }
    }

    fn line(&mut self, depth: usize, text: &str) {
        if self.out.len() > MAX_OUTPUT {
            return;
        }
        for _ in 0..depth.min(MAX_DEPTH) {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn alias(&self, id: &str) -> String {
        self.aliases.get(id).cloned().unwrap_or_else(|| alias(id, &mut HashSet::new()))
    }

    // ------------------------------------------------------------ structure diagrams

    /// The PlantUML keyword declaring an element, with a stereotype to add.
    fn keyword(e: &UmlElement) -> (&'static str, Option<&'static str>) {
        match e.local() {
            "Class" if e.prop("isAbstract") == "true" => ("abstract class", None),
            "Class" => ("class", None),
            "Interface" => ("interface", None),
            "Enumeration" => ("enum", None),
            "Component" => ("component", None),
            "Package" => ("package", None),
            "Node" => ("node", None),
            "Device" => ("node", Some("device")),
            "ExecutionEnvironment" => ("node", Some("executionEnvironment")),
            "Artifact" => ("artifact", None),
            "Actor" => ("actor", None),
            "UseCase" => ("usecase", None),
            "Subject" => ("rectangle", None),
            _ => ("rectangle", None),
        }
    }

    fn structure(&mut self) {
        let page = self.page;
        let mut done = HashSet::new();
        for e in page.children(None) {
            self.declare(e, 0, &mut done);
        }
        for r in &page.relationships {
            self.link(r);
        }
    }

    fn declare(&mut self, e: &'a UmlElement, depth: usize, done: &mut HashSet<&'a str>) {
        if depth > MAX_DEPTH || !done.insert(e.id.as_str()) {
            return;
        }
        let a = self.alias(&e.id);
        if e.local() == "Note" {
            self.line(depth, &format!("note {} as {a}", quoted(&e.name)));
            return;
        }
        let (kw, stereo) = Self::keyword(e);
        let mut head = format!("{kw} {} as {a}", quoted(&e.name));
        let stereotype = e.prop("stereotype");
        if !stereotype.is_empty() {
            let _ = write!(head, " <<{}>>", label(stereotype));
        } else if let Some(s) = stereo {
            let _ = write!(head, " <<{s}>>");
        }
        let rows: Vec<String> = match e.local() {
            "Class" | "Interface" => {
                e.members("attributes").iter().chain(e.members("operations").iter()).map(|m| label(&format_member(m, Shown::Symbols))).collect()
            }
            "Enumeration" => e.members("literals").iter().map(|m| label(&format_member(m, Shown::Hidden))).collect(),
            _ => Vec::new(),
        };
        let children: Vec<&'a UmlElement> = self.page.children(Some(e.id.as_str())).collect();
        if rows.is_empty() && children.is_empty() {
            self.line(depth, &head);
            return;
        }
        head.push_str(" {");
        self.line(depth, &head);
        for r in rows {
            self.line(depth + 1, &r);
        }
        for c in children {
            self.declare(c, depth + 1, done);
        }
        self.line(depth, "}");
    }

    fn link(&mut self, r: &UmlRelationship) {
        let (a, b) = (self.alias(&r.source), self.alias(&r.target));
        let navigable = r.prop("navigability");
        let arrow = match r.local() {
            "Generalization" => "--|>",
            "Realization" => "..|>",
            "Composition" => "*--",
            "Aggregation" => "o--",
            "Association" => match navigable {
                "target" => "-->",
                "source" => "<--",
                "both" => "<-->",
                _ => "--",
            },
            "CommunicationPath" => "--",
            "Anchor" => "..",
            _ => "..>",
        };
        let keyword = match r.local() {
            "Usage" => Some("use"),
            "PackageImport" => Some("import"),
            "PackageMerge" => Some("merge"),
            "Include" => Some("include"),
            "Extend" => Some("extend"),
            "Deploy" => Some("deploy"),
            "Manifest" => Some("manifest"),
            _ => None,
        };
        let end_label = |role: &str, mult: &str| {
            let parts: Vec<&str> = [r.prop(role), r.prop(mult)].into_iter().filter(|p| !p.is_empty()).collect();
            (!parts.is_empty()).then(|| quoted(&parts.join(" ")))
        };
        let words: Vec<String> =
            [Some(a), end_label("sourceRole", "sourceMultiplicity"), Some(arrow.to_string()), end_label("targetRole", "targetMultiplicity"), Some(b)]
                .into_iter()
                .flatten()
                .collect();
        let mut text = words.join(" ");
        let mut tail: Vec<String> = Vec::new();
        if let Some(k) = keyword {
            tail.push(format!("<<{k}>>"));
        }
        if !r.name.trim().is_empty() {
            tail.push(label(r.name.trim()));
        }
        if !tail.is_empty() {
            let _ = write!(text, " : {}", tail.join(" "));
        }
        self.line(0, &text);
    }

    // ------------------------------------------------------------ activity diagrams

    fn activity(&mut self) {
        let page = self.page;
        let out: HashMap<&str, Vec<&UmlRelationship>> = page.relationships.iter().fold(HashMap::new(), |mut m, r| {
            m.entry(r.source.as_str()).or_insert_with(Vec::new).push(r);
            m
        });
        let incoming: HashMap<&str, usize> = page.relationships.iter().fold(HashMap::new(), |mut m, r| {
            *m.entry(r.target.as_str()).or_insert(0) += 1;
            m
        });
        let mut flow = Flow { page, out, incoming, visited: HashSet::new(), written: HashSet::new(), lines: Vec::new(), warnings: Vec::new() };
        let starts: Vec<&UmlElement> = page.elements.iter().filter(|e| e.local() == "InitialNode").collect();
        if starts.is_empty() {
            flow.warnings.push("the activity has no initial node, so no flow was written".into());
        }
        for s in starts {
            flow.lines.push((0, "start".into()));
            flow.walk(s.id.as_str(), None, 0, 0);
        }
        let left = page.relationships.iter().filter(|r| !flow.written.contains(r.id.as_str())).count();
        if left > 0 {
            flow.warnings.push(format!("{left} control flows are outside the structured flow and were left out"));
        }
        let unvisited = page.elements.iter().filter(|e| uml::ACTIVITY_TYPES.contains(&e.local()) && !flow.visited.contains(e.id.as_str())).count();
        if unvisited > 0 {
            flow.warnings.push(format!("{unvisited} activity nodes are not reached from an initial node and were left out"));
        }
        let Flow { lines, warnings, .. } = flow;
        for (d, l) in lines {
            self.line(d, &l);
        }
        self.warnings.extend(warnings);
    }

    // ------------------------------------------------------------ state machine diagrams

    /// Whether `e` is written as `[*]`.
    fn is_star(e: &UmlElement) -> bool {
        matches!(e.local(), "Initial" | "Final")
    }

    /// How a transition end is written: `[*]` for initial and final states.
    fn state_ref(&self, id: &str) -> String {
        match self.page.element(id) {
            Some(e) if Self::is_star(e) => "[*]".into(),
            _ => self.alias(id),
        }
    }

    /// The scope a transition is written in: the owner of its initial or final end, else the
    /// top level.
    fn scope_of(&self, r: &UmlRelationship) -> Option<String> {
        [&r.source, &r.target].into_iter().filter_map(|id| self.page.element(id)).find(|e| Self::is_star(e)).and_then(|e| self.state_owner(e))
    }

    /// The owner of `e` that is written as a block (state machines are the diagram itself).
    fn state_owner(&self, e: &UmlElement) -> Option<String> {
        let mut at = e.owner.as_deref();
        for _ in 0..MAX_DEPTH {
            let o = self.page.element(at?)?;
            if o.local() != "StateMachine" {
                return Some(o.id.clone());
            }
            at = o.owner.as_deref();
        }
        None
    }

    fn transition_label(r: &UmlRelationship) -> String {
        if !r.name.trim().is_empty() {
            return label(r.name.trim());
        }
        let mut t = r.prop("trigger").to_string();
        if !r.prop("guard").is_empty() {
            let _ = write!(t, " [{}]", r.prop("guard"));
        }
        if !r.prop("effect").is_empty() {
            let _ = write!(t, " / {}", r.prop("effect"));
        }
        label(t.trim())
    }

    fn state(&mut self) {
        let page = self.page;
        let mut by_scope: HashMap<Option<String>, Vec<&UmlRelationship>> = HashMap::new();
        for r in &page.relationships {
            by_scope.entry(self.scope_of(r)).or_default().push(r);
        }
        let tops: Vec<&UmlElement> = page.elements.iter().filter(|e| self.state_owner(e).is_none() && e.local() != "StateMachine").collect();
        let mut done = HashSet::new();
        for e in tops {
            self.state_decl(e, 0, &mut done, &by_scope);
        }
        for r in by_scope.get(&None).cloned().unwrap_or_default() {
            self.transition(r, 0);
        }
    }

    fn state_decl(
        &mut self,
        e: &'a UmlElement,
        depth: usize,
        done: &mut HashSet<&'a str>,
        by_scope: &HashMap<Option<String>, Vec<&'a UmlRelationship>>,
    ) {
        if depth > MAX_DEPTH || !done.insert(e.id.as_str()) || Self::is_star(e) {
            return;
        }
        let a = self.alias(&e.id);
        let stereo = match e.local() {
            "Choice" => Some("choice"),
            "Junction" => {
                self.warnings.push(format!("junction {} was written as a choice", quoted(&e.name)));
                Some("choice")
            }
            "Fork" => Some("fork"),
            "Join" => Some("join"),
            "ShallowHistory" => Some("history"),
            "DeepHistory" => Some("history*"),
            "Terminate" => Some("end"),
            _ => None,
        };
        if let Some(s) = stereo {
            self.line(depth, &format!("state {a} <<{s}>>"));
            return;
        }
        let children: Vec<&'a UmlElement> =
            self.page.elements.iter().filter(|c| self.state_owner(c).as_deref() == Some(e.id.as_str()) && c.local() != "StateMachine").collect();
        let head = format!("state {} as {a}", quoted(if e.name.trim().is_empty() { &e.id } else { &e.name }));
        let inner: Vec<&'a UmlRelationship> = by_scope.get(&Some(e.id.clone())).cloned().unwrap_or_default();
        if children.is_empty() && inner.is_empty() {
            self.line(depth, &head);
        } else {
            self.line(depth, &format!("{head} {{"));
            for c in children {
                self.state_decl(c, depth + 1, done, by_scope);
            }
            for r in inner {
                self.transition(r, depth + 1);
            }
            self.line(depth, "}");
        }
        for key in ["entry", "do", "exit"] {
            let v = e.prop(key);
            if !v.trim().is_empty() {
                self.line(depth, &format!("{a} : {key} / {}", label(v.trim())));
            }
        }
    }

    fn transition(&mut self, r: &UmlRelationship, depth: usize) {
        let mut t = format!("{} --> {}", self.state_ref(&r.source), self.state_ref(&r.target));
        let l = Self::transition_label(r);
        if !l.is_empty() {
            let _ = write!(t, " : {l}");
        }
        self.line(depth, &t);
    }
}

/// The structured walk over an activity's control flows.
struct Flow<'a> {
    page: &'a UmlPage,
    out: HashMap<&'a str, Vec<&'a UmlRelationship>>,
    incoming: HashMap<&'a str, usize>,
    visited: HashSet<&'a str>,
    written: HashSet<&'a str>,
    lines: Vec<(usize, String)>,
    warnings: Vec<String>,
}

impl<'a> Flow<'a> {
    fn local(&self, id: &str) -> &'a str {
        self.page.element(id).map(UmlElement::local).unwrap_or_default()
    }

    fn outs(&self, id: &str) -> Vec<&'a UmlRelationship> {
        self.out.get(id).cloned().unwrap_or_default()
    }

    /// The node where the branches from `from` come together: the first node on the first
    /// branch, walking single successors, of local type `kind` (or any node with two or more
    /// incoming flows when `kind` is empty).
    fn meeting(&self, from: &str, kind: &str) -> Option<&'a str> {
        let first = self.outs(from).first().map(|r| r.target.as_str())?;
        let mut at: &'a str = first;
        let mut seen = HashSet::new();
        for _ in 0..1024 {
            if !seen.insert(at) {
                return None;
            }
            let l = self.local(at);
            let joins = if kind.is_empty() { self.incoming.get(at).copied().unwrap_or(0) >= 2 } else { l == kind };
            if joins {
                return Some(at);
            }
            at = self.out.get(at).and_then(|v| v.first()).map(|r| r.target.as_str())?;
        }
        None
    }

    /// Write the flow from `id` until `stop` (exclusive).
    fn walk(&mut self, id: &'a str, stop: Option<&'a str>, depth: usize, guard: usize) {
        let mut at = id;
        for _ in 0..10_000 {
            if Some(at) == stop || guard > MAX_DEPTH {
                return;
            }
            if !self.visited.insert(at) {
                self.warnings.push("a loop in the activity was cut where it returns".into());
                return;
            }
            let Some(e) = self.page.element(at) else { return };
            let name = label(e.name.trim());
            match e.local() {
                "InitialNode" => {}
                "ActivityFinal" => {
                    self.lines.push((depth, "stop".into()));
                    return;
                }
                "FlowFinal" => {
                    self.lines.push((depth, "end".into()));
                    return;
                }
                "SendSignal" => self.lines.push((depth, format!(":{name}>"))),
                "AcceptEvent" | "TimeEvent" => self.lines.push((depth, format!(":{name}<"))),
                "ObjectNode" => self.lines.push((depth, format!(":{name}]"))),
                "ForkNode" => {
                    let join = self.meeting(at, "JoinNode");
                    let branches = self.outs(at);
                    for (i, r) in branches.iter().enumerate() {
                        self.lines.push((depth, if i == 0 { "fork".into() } else { "fork again".into() }));
                        self.written.insert(r.id.as_str());
                        self.walk(r.target.as_str(), join, depth + 1, guard + 1);
                        if let Some(j) = join {
                            self.mark_into(j);
                        }
                    }
                    self.lines.push((depth, "end fork".into()));
                    match join {
                        Some(j) => {
                            self.visited.insert(j);
                            match self.next(j) {
                                Some(n) => at = n,
                                None => return,
                            }
                            continue;
                        }
                        None => return,
                    }
                }
                "Decision" if self.outs(at).len() > 1 => {
                    let merge = self.meeting(at, "");
                    let branches = self.outs(at);
                    let question = if name.is_empty() { "?".to_string() } else { name.clone() };
                    for (i, r) in branches.iter().enumerate() {
                        let g = label(r.prop("guard").trim().trim_start_matches('[').trim_end_matches(']'));
                        let g = if g.is_empty() { label(r.name.trim()) } else { g };
                        let line = match i {
                            0 => format!("if ({question}) then ({g})"),
                            _ if i + 1 == branches.len() => format!("else ({g})"),
                            _ => format!("elseif ({question}) then ({g})"),
                        };
                        self.lines.push((depth, line));
                        self.written.insert(r.id.as_str());
                        self.walk(r.target.as_str(), merge, depth + 1, guard + 1);
                        if let Some(m) = merge {
                            self.mark_into(m);
                        }
                    }
                    self.lines.push((depth, "endif".into()));
                    match merge {
                        Some(m) => {
                            // A merge is only a meeting point: the walk goes on from it.
                            at = m;
                            continue;
                        }
                        None => return,
                    }
                }
                "Decision" | "JoinNode" => {}
                _ => self.lines.push((depth, format!(":{name};"))),
            }
            match self.next(at) {
                Some(n) => at = n,
                None => return,
            }
        }
    }

    /// Mark the flows into `node` written (the branches that reach it).
    fn mark_into(&mut self, node: &str) {
        for r in self.page.relationships.iter().filter(|r| r.target == node) {
            if self.visited.contains(r.source.as_str()) {
                self.written.insert(r.id.as_str());
            }
        }
    }

    /// The single next node after `id`, marking the flow written; more than one is a warning.
    fn next(&mut self, id: &str) -> Option<&'a str> {
        let outs = self.outs(id);
        if outs.len() > 1 {
            self.warnings.push(format!("{} flows leave a node that is not a decision or fork; only the first was written", outs.len()));
        }
        let r = outs.first()?;
        self.written.insert(r.id.as_str());
        Some(r.target.as_str())
    }
}
