//! Mermaid state diagrams as OMG UML state machines: read into `uml.*` shapes with typed data, and
//! written back.
//!
//! Reading lays the diagram out like any Mermaid graph, then draws each state with the UML master
//! of its kind and stores the type in `ext` for a merging import. `[*]` becomes an initial or a
//! final state with a plain id: `initial` and `final` at the top level, `<parent>_initial` inside
//! a composite state (`<parent>_region<n>_initial` inside one of its concurrent regions).
//! Composite states and regions become containers that own their states. Description lines
//! `entry / x`, `do / x` and `exit / x` become the state's behaviours; a transition label is its
//! name, read as `trigger [guard] / effect` into the transition's properties.
//!
//! Writing uses `state "<name>" as <id>` so ids survive a round trip; initial and final states
//! are written `[*]` in the block of the state that owns them.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use vizcraft_diagram::{Document, Page};
use vizcraft_mermaid::state::{StateDiagram, StateKind};

use super::class::{make_container, restyle, set_prop, type_connector};
use super::common::Warnings;
use crate::FormatError;
use crate::csv::TYPE_KEY;
use crate::uml::{self, UmlElement, UmlPage, UmlRelationship, alias, one_line};

/// Longest name or label written, in bytes.
const MAX_NAME: usize = 256;

/// Deepest nesting written.
const MAX_DEPTH: usize = 64;

/// The plain id for a generated `[*]` id (`P/region1/[*]start` is `P_region1_initial`).
fn plain_star(id: &str, start: bool) -> String {
    let suffix = if start { "initial" } else { "final" };
    let scope = id.rsplit_once('/').map(|(s, _)| s.replace('/', "_")).unwrap_or_default();
    if scope.is_empty() { suffix.to_string() } else { format!("{scope}_{suffix}") }
}

/// A transition label read as `trigger [guard] / effect`.
fn label_parts(label: &str) -> (String, String, String) {
    let (head, effect) = label.split_once('/').map_or((label, ""), |(h, e)| (h, e));
    let (trigger, guard) = match (head.find('['), head.rfind(']')) {
        (Some(o), Some(c)) if c > o => (
            format!("{}{}", head.get(..o).unwrap_or_default(), head.get(c + 1..).unwrap_or_default()),
            head.get(o + 1..c).unwrap_or_default().to_string(),
        ),
        _ => (head.to_string(), String::new()),
    };
    (trigger.trim().to_string(), guard.trim().to_string(), effect.trim().to_string())
}

/// Rename shape `old` to `new` on `page`: its id, parents naming it and connector ends.
fn rename(page: &mut Page, old: &str, new: &str) {
    for s in &mut page.shapes {
        if s.id == old {
            s.id = new.to_string();
        }
        if s.parent.as_deref() == Some(old) {
            s.parent = Some(new.to_string());
        }
    }
    for c in &mut page.connectors {
        for end in [&mut c.start, &mut c.end] {
            if end.shape() == Some(old) {
                *end = vizcraft_diagram::End::dynamic(new);
            }
        }
    }
}

/// Read a Mermaid state diagram as a laid-out page of UML shapes.
pub(super) fn build(src: &StateDiagram, warnings: &mut Warnings) -> Result<Page, FormatError> {
    let mut d = src.clone();
    let mut ids: HashMap<String, String> = HashMap::new();
    for s in &d.states {
        match s.kind {
            StateKind::Start => ids.insert(s.id.clone(), plain_star(&s.id, true)),
            StateKind::End => ids.insert(s.id.clone(), plain_star(&s.id, false)),
            _ => None,
        };
    }
    let map = |id: &mut String| {
        if let Some(n) = ids.get(id.as_str()) {
            *id = n.clone();
        }
    };
    for s in &mut d.states {
        map(&mut s.id);
    }
    for t in &mut d.transitions {
        map(&mut t.from);
        map(&mut t.to);
    }
    let mut g = d.to_graph();
    let n = d.transitions.len();
    for (i, e) in g.edges.iter_mut().enumerate().take(n) {
        e.id = Some(format!("t{}", i.saturating_add(1)));
    }
    let mut page = super::graph::build(&g, warnings)?;
    let regions: Vec<String> = page.shapes.iter().filter(|s| s.id.contains("/region")).map(|s| s.id.clone()).collect();
    for r in regions {
        rename(&mut page, &r, &r.replace('/', "_"));
    }
    let regions_of: HashMap<&str, usize> = d.states.iter().map(|s| (s.id.as_str(), s.regions)).collect();
    let group = |parent: &str, region: usize| {
        if regions_of.get(parent).copied().unwrap_or(1) > 1 { format!("{parent}_region{region}") } else { parent.to_string() }
    };
    let index: HashMap<String, usize> = page.shapes.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
    // Regions: containers inside their composite state.
    for s in d.states.iter().filter(|s| s.kind == StateKind::Composite && s.regions > 1) {
        for r in 0..s.regions {
            let Some(sh) = index.get(&format!("{}_region{r}", s.id)).and_then(|&i| page.shapes.get_mut(i)) else { continue };
            restyle(sh, "uml.region", false);
            make_container(sh);
            sh.ext.insert(TYPE_KEY.into(), "uml.Region".into());
            sh.ext.remove("mermaid.group");
            sh.parent = Some(s.id.clone());
            sh.sanitize();
        }
    }
    let mut dropped = 0usize;
    for s in &d.states {
        let Some(sh) = index.get(&s.id).and_then(|&i| page.shapes.get_mut(i)) else { continue };
        let (ty, master, resize) = match s.kind {
            StateKind::Simple | StateKind::Composite => ("uml.State", "uml.state", false),
            StateKind::Start => ("uml.Initial", "uml.initial", true),
            StateKind::End => ("uml.Final", "uml.final", true),
            StateKind::Choice => ("uml.Choice", "uml.choice", true),
            StateKind::Fork => ("uml.Fork", "uml.fork", true),
            StateKind::Join => ("uml.Join", "uml.join", true),
        };
        restyle(sh, master, resize);
        sh.ext.insert(TYPE_KEY.into(), ty.into());
        if s.kind == StateKind::Composite {
            make_container(sh);
        }
        let mut name = s.label.clone();
        for line in &s.descriptions {
            let t = line.trim();
            let key = ["entry", "do", "exit"].into_iter().find(|k| t.strip_prefix(k).is_some_and(|r| r.trim_start().starts_with('/')));
            match key {
                Some(k) => {
                    let v = t.get(k.len()..).unwrap_or_default().trim_start().trim_start_matches('/');
                    set_prop(&mut sh.ext, k, v);
                }
                None if name.is_none() => name = Some(t.to_string()),
                None => dropped += 1,
            }
        }
        sh.text = match s.kind {
            StateKind::Simple | StateKind::Composite => name.unwrap_or_else(|| s.id.clone()),
            _ => String::new(),
        };
        sh.parent = s.parent.as_deref().map(|p| group(p, s.region)).filter(|p| index.contains_key(p));
        sh.sanitize();
    }
    if dropped > 0 {
        warnings.add(format!("{dropped} state description lines other than entry, do and exit were left out"));
    }
    let transitions: HashMap<String, usize> = (0..n).map(|i| (format!("t{}", i.saturating_add(1)), i)).collect();
    for c in &mut page.connectors {
        let Some(t) = transitions.get(&c.id).and_then(|&i| d.transitions.get(i)) else { continue };
        type_connector(c, "uml.Transition");
        let label = t.label.clone().unwrap_or_default();
        let (trigger, guard, effect) = label_parts(&label);
        set_prop(&mut c.ext, "trigger", &trigger);
        set_prop(&mut c.ext, "guard", &guard);
        set_prop(&mut c.ext, "effect", &effect);
        c.text = label.trim().to_string();
        c.sanitize();
    }
    Ok(page)
}

/// Write page `page` of `doc` as a Mermaid state diagram.
pub(super) fn write(doc: &Document, page: usize) -> Result<(String, Vec<String>), FormatError> {
    let p = uml::read(doc, page)?;
    Ok(Writer::new(&p).run())
}

struct Writer<'a> {
    page: &'a UmlPage,
    ids: HashMap<&'a str, String>,
    out: String,
    warnings: Vec<String>,
}

impl<'a> Writer<'a> {
    fn new(page: &'a UmlPage) -> Self {
        let mut used = HashSet::new();
        let ids = page.elements.iter().map(|e| (e.id.as_str(), alias(&e.id, &mut used))).collect();
        Writer { page, ids, out: String::from("stateDiagram-v2\n"), warnings: Vec::new() }
    }

    fn id(&self, x: &str) -> String {
        self.ids.get(x).cloned().unwrap_or_else(|| x.to_string())
    }

    fn is_star(e: &UmlElement) -> bool {
        matches!(e.local(), "Initial" | "Final")
    }

    /// The owner written as a block: not a state machine (the diagram itself) or a region (its
    /// parent's block, split by `--`).
    fn block_of(&self, e: &UmlElement) -> Option<&'a UmlElement> {
        let mut at = e.owner.as_deref();
        for _ in 0..MAX_DEPTH {
            let o = self.page.element(at?)?;
            if o.local() == "State" {
                return Some(o);
            }
            at = o.owner.as_deref();
        }
        None
    }

    /// The region `e` sits in directly, if any.
    fn region_of(&self, e: &UmlElement) -> Option<&'a str> {
        let o = self.page.element(e.owner.as_deref()?)?;
        (o.local() == "Region").then_some(o.id.as_str())
    }

    fn end(&self, id: &str) -> String {
        match self.page.element(id) {
            Some(e) if Self::is_star(e) => "[*]".into(),
            _ => self.id(id),
        }
    }

    fn scope_of(&self, r: &UmlRelationship) -> Option<String> {
        [&r.source, &r.target]
            .into_iter()
            .filter_map(|x| self.page.element(x))
            .find(|e| Self::is_star(e))
            .and_then(|e| self.block_of(e))
            .map(|b| b.id.clone())
    }

    /// The `[*]` group a transition belongs to inside its block: its star end's region.
    fn region_scope(&self, r: &UmlRelationship) -> Option<String> {
        [&r.source, &r.target]
            .into_iter()
            .filter_map(|x| self.page.element(x))
            .find(|e| Self::is_star(e))
            .and_then(|e| self.region_of(e))
            .map(str::to_string)
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..=depth.min(MAX_DEPTH) {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn run(mut self) -> (String, Vec<String>) {
        let page = self.page;
        let mut by_scope: HashMap<Option<String>, Vec<&'a UmlRelationship>> = HashMap::new();
        for r in &page.relationships {
            if r.local() == "Transition" {
                by_scope.entry(self.scope_of(r)).or_default().push(r);
            }
        }
        let tops: Vec<&'a UmlElement> =
            page.elements.iter().filter(|e| self.block_of(e).is_none() && !matches!(e.local(), "StateMachine" | "Region")).collect();
        let mut done = HashSet::new();
        for e in tops {
            self.decl(e, 0, &mut done, &by_scope);
        }
        for r in by_scope.get(&None).cloned().unwrap_or_default() {
            self.transition(r, 0);
        }
        let other = page.relationships.iter().filter(|r| r.local() != "Transition").count();
        if other > 0 {
            self.warnings.push(format!("{other} relationships that are not transitions were left out"));
        }
        (self.out, self.warnings)
    }

    fn decl(&mut self, e: &'a UmlElement, depth: usize, done: &mut HashSet<&'a str>, by_scope: &HashMap<Option<String>, Vec<&'a UmlRelationship>>) {
        if depth > MAX_DEPTH || !done.insert(e.id.as_str()) || Self::is_star(e) || e.local() == "Note" {
            return;
        }
        let id = self.id(&e.id);
        let stereo = match e.local() {
            "Choice" => Some("choice"),
            "Fork" => Some("fork"),
            "Join" => Some("join"),
            "State" => None,
            other => {
                self.warnings.push(format!("a {other} has no Mermaid form and was written as a plain state"));
                None
            }
        };
        if let Some(s) = stereo {
            self.line(depth, &format!("state {id} <<{s}>>"));
            return;
        }
        let page = self.page;
        let children: Vec<&'a UmlElement> =
            page.elements.iter().filter(|c| self.block_of(c).map(|b| b.id.as_str()) == Some(e.id.as_str()) && c.local() != "Region").collect();
        let inner: Vec<&'a UmlRelationship> = by_scope.get(&Some(e.id.clone())).cloned().unwrap_or_default();
        let name = one_line(e.name.trim(), MAX_NAME).replace('"', "'");
        let head = if name.is_empty() { format!("state {id}") } else { format!("state \"{name}\" as {id}") };
        if children.is_empty() && inner.is_empty() {
            self.line(depth, &head);
        } else {
            self.line(depth, &format!("{head} {{"));
            // Children and star transitions grouped by region, regions split by `--`.
            let mut regions: Vec<Option<&'a str>> = Vec::new();
            for c in &children {
                let r = self.region_of(c);
                if !regions.contains(&r) {
                    regions.push(r);
                }
            }
            for r in &inner {
                let reg = self.region_scope(r);
                let reg = reg.as_deref().and_then(|x| page.element(x)).map(|x| x.id.as_str());
                if !regions.contains(&reg) {
                    regions.push(reg);
                }
            }
            for (i, reg) in regions.iter().enumerate() {
                if i > 0 {
                    self.line(depth + 1, "--");
                }
                let here: Vec<&'a UmlElement> = children.iter().copied().filter(|c| self.region_of(c) == *reg).collect();
                for c in here {
                    self.decl(c, depth + 1, done, by_scope);
                }
                let moves: Vec<&'a UmlRelationship> = inner.iter().copied().filter(|r| self.region_scope(r).as_deref() == *reg).collect();
                for r in moves {
                    self.transition(r, depth + 1);
                }
            }
            self.line(depth, "}");
        }
        for key in ["entry", "do", "exit"] {
            let v = e.prop(key).trim().to_string();
            if !v.is_empty() {
                self.line(depth, &format!("{id} : {key} / {}", one_line(&v, MAX_NAME)));
            }
        }
    }

    fn transition(&mut self, r: &UmlRelationship, depth: usize) {
        let mut t = format!("{} --> {}", self.end(&r.source), self.end(&r.target));
        let mut label = r.name.trim().to_string();
        if label.is_empty() {
            label = r.prop("trigger").to_string();
            if !r.prop("guard").is_empty() {
                let _ = write!(label, " [{}]", r.prop("guard"));
            }
            if !r.prop("effect").is_empty() {
                let _ = write!(label, " / {}", r.prop("effect"));
            }
        }
        let label = one_line(label.trim(), MAX_NAME);
        if !label.is_empty() {
            let _ = write!(t, " : {label}");
        }
        self.line(depth, &t);
    }
}
