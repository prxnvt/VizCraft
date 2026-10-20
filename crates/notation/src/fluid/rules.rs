//! The fluid power checks (ISO 1219-2 circuit rules): open ports, the pressure path, a relief
//! valve after every fixed pump, a reservoir under every hydraulic pump, valves drawn at rest,
//! port labels matching the medium, pilot and drain lines on the right ports, component codes,
//! and the displacement-step chart. [`check`] is the rules engine's one dispatch arm for them;
//! the circuit ([`super::Circuit`]) is built once per call.

use serde_json::json;
use vizcraft_diagram::Document;
use vizcraft_diagram::net::pin_of;
use vizcraft_model::issue::Fix;
use vizcraft_model::{Element, Issue, PropValue};

use super::{ACTUATOR, Circuit, Part, Positions, Role, STEP_CHART, Walk, is_a, paths, steps};
use crate::Catalog;
use crate::rules::{MAX_MESSAGE, cut};
use crate::schema::{Check, Pack, RuleDecl};

/// The `netCheck` tests this module runs.
pub const NET_TESTS: &[&str] = &["openPort"];

/// The relationship type of pilot lines.
pub const PILOT_LINE: &str = "fluidpower.PilotLine";
/// The relationship type of drain lines.
pub const DRAIN_LINE: &str = "fluidpower.DrainLine";

/// The code forms when the page's variant names none: ISO 1219-2 letters, and the numeric form.
pub const LETTER_FORMAT: &str = "{installation}{circuit}{letter}{n}";
/// See [`LETTER_FORMAT`].
pub const NUMERIC_FORMAT: &str = "{installation}{circuit}.{n}";

/// Longest name quoted in a message, in bytes.
const MAX_LABEL: usize = 60;

/// Run one of the fluid power checks; other checks are ignored.
pub fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let c = Circuit::build(doc, cat, None);
    let cx = Cx { doc, cat, pack, rule, c: &c };
    match &rule.check {
        Check::NetCheck { test, types, .. } if test == "openPort" => cx.open_ports(types, out),
        Check::FluidPressurePath => cx.pressure_path(out),
        Check::FluidRelief => cx.relief(out),
        Check::FluidSupply => cx.supply(out),
        Check::FluidRestPosition => cx.rest(out),
        Check::FluidPortSet => cx.port_set(out),
        Check::FluidLineEnds => cx.line_ends(out),
        Check::FluidCodes => cx.codes(out),
        Check::FluidSteps { idle } => cx.steps(*idle, out),
        _ => {}
    }
}

struct Cx<'a> {
    doc: &'a Document,
    cat: &'a Catalog,
    pack: &'a Pack,
    rule: &'a RuleDecl,
    c: &'a Circuit,
}

/// How a message names a component: its code, else its quoted name, else its type's label.
pub fn label(cat: &Catalog, e: Option<&Element>, type_id: &str) -> String {
    match e {
        Some(e) if !e.tag.trim().is_empty() => cut(e.tag.trim(), MAX_LABEL),
        Some(e) if !e.name.trim().is_empty() => format!("\"{}\"", cut(&e.name.trim().replace(['\n', '\r'], " "), MAX_LABEL)),
        _ => cat.element_type(type_id).map(|(_, t)| t.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| type_id.to_string()),
    }
}

impl Cx<'_> {
    fn element(&self, id: &str) -> Option<&Element> {
        self.doc.model.element(id)
    }

    fn name(&self, p: &Part) -> String {
        label(self.cat, self.element(&p.element), &p.type_id)
    }

    fn issue(&self, what: &str, subject: &str, items: Vec<String>, page: Option<usize>) -> Issue {
        // Problems messages state only the problem; the rule's description stays with the rule.
        let mut i =
            Issue::new(self.rule.id.clone(), self.rule.severity, cut(what, MAX_MESSAGE)).with_subjects(vec![subject.to_string()]).with_items(items);
        i.page = page.and_then(|p| self.doc.pages.get(p)).map(|p| p.id.clone());
        i
    }

    fn part_issue(&self, p: &Part, what: &str) -> Issue {
        self.issue(what, &p.element, vec![p.shape.clone()], Some(p.page))
    }

    fn covers(&self, types: &[String], type_id: &str) -> bool {
        if types.is_empty() {
            crate::infer::chain_of(self.cat, type_id).iter().any(|t| vizcraft_model::pack_of(t) == self.pack.id)
        } else {
            let chain = crate::infer::chain_of(self.cat, type_id);
            types.iter().any(|t| t == "*" || chain.contains(&t.as_str()))
        }
    }

    /// Ports of a component on no line, not plugged and not an exhaust that may stay open.
    fn open_ports(&self, types: &[String], out: &mut Vec<Issue>) {
        for p in self.c.parts.iter().filter(|p| self.covers(types, &p.type_id)) {
            let plugged = self.element(&p.element).map(super::plugged_of).unwrap_or_default();
            let open: Vec<&str> = p
                .ports
                .iter()
                .filter_map(|&i| self.c.ports.get(i))
                .filter(|x| paths::is_fluid(&x.id) && !x.glued && x.net.is_none() && !x.exhaust && !plugged.contains(&x.id))
                .map(|x| x.id.as_str())
                .collect();
            if open.is_empty() {
                continue;
            }
            let what = format!("{} has {}: {}.", self.name(p), if open.len() == 1 { "an open port" } else { "open ports" }, open.join(", "));
            let mut i = self.part_issue(p, &what);
            i.fixes.push(Fix::new("Plug the open ports", "fluid.plug", json!({"id": p.element, "ports": open})));
            out.push(i);
        }
    }

    /// Actuators no pressure source reaches through lines and any valve position.
    fn pressure_path(&self, out: &mut Vec<Issue>) {
        let outlets: Vec<usize> = (0..self.c.parts.len()).flat_map(|i| self.c.outlets(self.cat, i)).collect();
        let reached = self.c.trace(&outlets, &Walk::pressure(Positions::Any));
        for p in self.c.parts.iter().filter(|p| is_a(self.cat, &p.type_id, ACTUATOR)) {
            let fed = p.ports.iter().any(|&i| self.c.is_fluid(i) && reached.ports.contains(&i));
            if !fed {
                out.push(self.part_issue(p, &format!("{} is not driven by any pump, compressor or pressure source.", self.name(p))));
            }
        }
    }

    /// Fixed displacement hydraulic pumps whose pressure port reaches no relief valve.
    fn relief(&self, out: &mut Vec<Issue>) {
        for (i, p) in self.c.parts.iter().enumerate() {
            if p.role != Role::Pump || p.medium == "pneumatic" || self.variable(p) {
                continue;
            }
            let Some(start) = p.port(self.c, "P") else { continue };
            let walk = Walk { directed: true, ..Walk::lines() };
            let r = self.c.trace(&[start], &walk);
            let relieved =
                r.ports.iter().filter_map(|&q| self.c.ports.get(q)).any(|q| {
                    q.id == "P" && q.part != i && self.c.parts.get(q.part).is_some_and(|x| x.role == Role::PressureValve && x.kind == "relief")
                });
            if !relieved {
                out.push(self.part_issue(p, &format!("{} is a fixed displacement pump with no relief valve on its pressure line.", self.name(p))));
            }
        }
    }

    fn variable(&self, p: &Part) -> bool {
        let (Some(page), Some(e)) = (self.doc.pages.get(p.page), self.element(&p.element)) else { return false };
        let Some(shape) = page.shape(&p.shape) else { return false };
        super::flag_param(self.cat, shape, Some(e), super::page_variant(page), "variable", false)
    }

    /// Hydraulic pumps whose suction reaches no reservoir.
    fn supply(&self, out: &mut Vec<Issue>) {
        for p in self.c.parts.iter().filter(|p| p.role == Role::Pump && p.medium != "pneumatic") {
            let Some(start) = p.port(self.c, "S") else { continue };
            let r = self.c.trace(&[start], &Walk::lines());
            if !r.parts.iter().any(|&q| self.c.parts.get(q).is_some_and(|x| x.role == Role::Tank)) {
                out.push(self.part_issue(p, &format!("{} draws from no reservoir.", self.name(p))));
            }
        }
    }

    /// Valves with a return spring not drawn in the position the spring holds.
    fn rest(&self, out: &mut Vec<Issue>) {
        for p in &self.c.parts {
            let Some(v) = &p.valve else { continue };
            let Some(want) = paths::expected_rest(v.positions, &v.ret, &v.left, &v.right) else { continue };
            if want as usize == p.rest {
                continue;
            }
            let side =
                if v.positions >= 3 { "the middle square".to_string() } else { format!("square {} from the left, on the spring side", want + 1) };
            let mut i = self.part_issue(p, &format!("{} is drawn in square {}; at rest it is in {side}.", self.name(p), p.rest + 1));
            i.fixes.push(Fix::new("Draw it at rest", "element.setProperty", json!({"id": p.element, "key": "rest", "value": want})));
            out.push(i);
        }
    }

    /// Valves whose port labels are not their medium's set.
    fn port_set(&self, out: &mut Vec<Issue>) {
        for p in &self.c.parts {
            let Some(v) = &p.valve else { continue };
            let pneumatic = p.medium == "pneumatic";
            if pneumatic == v.numbers {
                continue;
            }
            let (set, what) = if pneumatic {
                ("iso5599", format!("{} is pneumatic, so its ports are numbered by ISO 5599 (1, 2, 4, 3, 5).", self.name(p)))
            } else {
                ("iso9461", format!("{} is hydraulic, so its ports are lettered by ISO 9461 (P, A, B, T).", self.name(p)))
            };
            let mut i = self.part_issue(p, &what);
            i.fixes.push(Fix::new("Use the medium's port labels", "fluid.setPortSet", json!({"ids": [p.shape], "set": set, "page": p.page})));
            out.push(i);
        }
    }

    /// The line type a connector is drawn as: its kind's relationship type, else its
    /// relationship's type (the kind is what the drawing shows).
    fn line_type(&self, c: &vizcraft_diagram::Connector) -> Option<String> {
        let by_kind = (!c.kind.is_empty()).then(|| self.cat.packs().iter().find_map(|p| p.connector_type(&c.kind))).flatten();
        if let Some(t) = by_kind.and_then(|k| k.relationship_type.clone()) {
            return Some(t);
        }
        c.relationship.as_deref().and_then(|r| self.doc.model.relationship(r)).map(|r| r.type_id.clone())
    }

    /// Pilot lines with no end on a pilot port, drain lines with no end on a drain port or a
    /// reservoir.
    fn line_ends(&self, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate() {
            for con in &page.connectors {
                let Some(t) = self.line_type(con) else { continue };
                let pilot = t == PILOT_LINE;
                if !pilot && t != DRAIN_LINE {
                    continue;
                }
                let ends: Vec<usize> =
                    [&con.start, &con.end].into_iter().filter_map(|e| pin_of(page, e)).filter_map(|(s, i)| self.c.port_at(&s, i)).collect();
                if ends.is_empty() {
                    continue;
                }
                let right = ends.iter().filter_map(|&i| self.c.ports.get(i)).any(|q| {
                    if pilot {
                        paths::is_pilot(&q.id)
                    } else {
                        paths::is_drain(&q.id) || self.c.parts.get(q.part).is_some_and(|x| x.role == Role::Tank)
                    }
                });
                if right {
                    continue;
                }
                let at: Vec<String> = ends
                    .iter()
                    .filter_map(|&i| self.c.ports.get(i))
                    .filter_map(|q| self.c.parts.get(q.part).map(|x| format!("{} of {}", q.id, self.name(x))))
                    .collect();
                let what = if pilot {
                    format!("A pilot line ends at {}, not at a pilot port (X, Y, 12, 14).", at.join(" and "))
                } else {
                    format!("A drain line ends at {}, not at a drain port (L) or a reservoir.", at.join(" and "))
                };
                let subject = con.relationship.clone().unwrap_or_else(|| con.id.clone());
                out.push(self.issue(&what, &subject, vec![con.id.clone()], Some(pi)));
            }
        }
    }

    /// Component codes that read as neither code form for the component's circuit and class.
    fn codes(&self, out: &mut Vec<Issue>) {
        let mut seen = std::collections::HashSet::new();
        for p in &self.c.parts {
            if !seen.insert(p.element.as_str()) {
                continue;
            }
            let Some(e) = self.element(&p.element) else { continue };
            let tag = e.tag.trim();
            if tag.is_empty() {
                continue;
            }
            let variant = self.doc.pages.get(p.page).map(super::page_variant).unwrap_or_default();
            let (letters, numeric) = formats(self.cat, variant);
            let fits = |f: &str| matches_format(&compose_prefix(self.cat, e, f), tag);
            if fits(&letters) || fits(&numeric) {
                continue;
            }
            let want = compose(self.cat, e, &letters, None);
            let what = format!(
                "Code {} does not read as circuit {} with letter {} (such as {want}).",
                cut(tag, MAX_LABEL),
                super::circuit_of(self.cat, e),
                super::letter_of(self.cat, e)
            );
            let mut i = self.part_issue(p, &what);
            i.fixes.push(Fix::new("Renumber the components", "fluid.renumber", json!({"page": p.page})));
            out.push(i);
        }
    }

    /// Step chart rows naming no actuator and steps no valve position drives; with `idle`, steps
    /// in which no actuator moves.
    fn steps(&self, idle: bool, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate() {
            for shape in &page.shapes {
                let Some(e) = shape.element.as_deref().and_then(|id| self.element(id)) else { continue };
                if !is_a(self.cat, &e.type_id, STEP_CHART) {
                    continue;
                }
                let chart = steps::parse(&chart_text(self.cat, shape, e, super::page_variant(page)));
                for f in steps::findings(self.c, self.cat, &self.doc.model, &chart) {
                    if f.idle != idle {
                        continue;
                    }
                    let mut items = vec![shape.id.clone()];
                    items.extend(f.shapes.iter().cloned());
                    out.push(self.issue(&f.message, &e.id, items, Some(pi)));
                }
            }
        }
    }
}

/// The text of a step chart: its element's `rows`, else its shape's or glyph's.
pub fn chart_text(cat: &Catalog, shape: &vizcraft_diagram::Shape, e: &Element, variant: &str) -> String {
    super::param(cat, shape, Some(e), variant, "rows").and_then(PropValue::as_str).unwrap_or_default().to_string()
}

/// The letter and numeric code forms of `variant` (its `letterFormat` and `numericFormat`
/// options), else the defaults.
pub fn formats(cat: &Catalog, variant: &str) -> (String, String) {
    let opts = cat.variant(variant).map(|(_, v)| &v.options);
    let get = |k: &str, d: &str| opts.and_then(|o| o.get(k)).filter(|f| f.contains("{n}")).cloned().unwrap_or_else(|| d.to_string());
    (get("letterFormat", LETTER_FORMAT), get("numericFormat", NUMERIC_FORMAT))
}

/// The code form `variant` gives by default: `letters` or `numeric` (its `codes` option).
pub fn default_codes(cat: &Catalog, variant: &str) -> &'static str {
    match cat.variant(variant).and_then(|(_, v)| v.options.get("codes")).map(String::as_str) {
        Some("numeric") => "numeric",
        _ => "letters",
    }
}

/// `format` with `e`'s installation, circuit and letter put in and `{n}` as `n` (kept as `{n}`
/// when `None`).
pub fn compose(cat: &Catalog, e: &Element, format: &str, n: Option<usize>) -> String {
    let base = compose_prefix(cat, e, format);
    match n {
        Some(n) => base.replacen("{n}", &n.to_string(), 1),
        None => base.replacen("{n}", "1", 1),
    }
}

/// `format` with everything but `{n}` put in.
fn compose_prefix(cat: &Catalog, e: &Element, format: &str) -> String {
    format
        .replace("{installation}", &super::installation_of(cat, e))
        .replace("{circuit}", &super::circuit_of(cat, e).to_string())
        .replace("{letter}", &super::letter_of(cat, e))
}

/// Whether `tag` is `pattern` with its `{n}` a whole number.
fn matches_format(pattern: &str, tag: &str) -> bool {
    let Some((head, tail)) = pattern.split_once("{n}") else { return pattern == tag };
    tag.strip_prefix(head).and_then(|r| r.strip_suffix(tail)).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}
