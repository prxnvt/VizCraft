//! The net and electrical checks: [`Check::NetCheck`] with its generic tests over nets and the
//! pins they join (unconnected pins, single-pin nets, numbering, reserved colours, fan-in,
//! drivers, required net properties, allowed peers), and the electrical pack's own checks
//! (contact complements, outputs per rung, mixed symbol sets, device function numbers, cable
//! cores, placement on a viewpoint). [`check`] is the rules engine's one dispatch arm for them.
//!
//! Connectivity comes from `vizcraft_diagram::net::NetIndex`, the same index the canvas and the
//! net commands read, so a check sees exactly the nets the sync pass made. Messages state only
//! the problem.

use std::collections::{BTreeMap, HashMap, HashSet};

use vizcraft_diagram::net::{NetIndex, pin_of};
use vizcraft_diagram::{Document, Shape};
use vizcraft_model::{Element, Issue, Net, PropValue};

use crate::Catalog;
use crate::infer::Facts;
use crate::rules::cut;
use crate::schema::{Check, Pack, PortDirection, RuleDecl};

/// Longest name quoted in a message, in bytes.
const MAX_LABEL: usize = 60;

/// Most pins named in one message.
const MAX_LISTED: usize = 8;

/// Run one of the checks of this module; other checks are ignored.
pub(crate) fn check(doc: &Document, cat: &Catalog, pack: &Pack, rule: &RuleDecl, out: &mut Vec<Issue>) {
    let cx = Cx { doc, cat, pack, rule, facts: Facts::new(&doc.model, cat) };
    match &rule.check {
        Check::NetCheck { test, types, kinds, connector_type, property, peers, max } => {
            let ix = NetIndex::build(doc);
            let t = NetTest { types, kinds, connector_type, property, peers, max: *max };
            match test.as_str() {
                "unconnectedPin" => cx.unconnected(&ix, &t, out),
                "singlePin" => cx.single_pin(&ix, &t, out),
                "unnumbered" => cx.unnumbered(&ix, &t, out),
                "duplicateNumber" => cx.duplicate_numbers(&ix, &t, out),
                "reservedColour" => cx.reserved_colour(&ix, &t, out),
                "pinFanIn" => cx.fan_in(&t, out),
                "drivers" => cx.drivers(&ix, &t, false, out),
                "undriven" => cx.drivers(&ix, &t, true, out),
                "property" => cx.net_property(&ix, &t, out),
                "allowedPeers" => cx.allowed_peers(&ix, &t, out),
                _ => {}
            }
        }
        Check::ContactComplement { types, relationship } => cx.complement(types, relationship, out),
        Check::RungOutputs { types, max, at_right } => cx.rung_outputs(types, *max, *at_right, out),
        Check::MixedSymbols { types } => cx.mixed_symbols(types, out),
        Check::DeviceNumber { types, property } => cx.device_numbers(types, property, out),
        Check::CableCores { types, cores } => cx.cable_cores(types, cores, out),
        Check::PlacedOn { types, viewpoint, min, max } => cx.placed_on(types, viewpoint, *min, *max, out),
        _ => {}
    }
}

/// The parameters of one net test.
struct NetTest<'a> {
    types: &'a [String],
    kinds: &'a [String],
    connector_type: &'a str,
    property: &'a str,
    peers: &'a [String],
    max: u32,
}

/// One run's context.
struct Cx<'a> {
    doc: &'a Document,
    cat: &'a Catalog,
    pack: &'a Pack,
    rule: &'a RuleDecl,
    facts: Facts<'a>,
}

/// A pin as a check sees it: the element whose shape it is on and the port id.
struct Pin<'a> {
    element: &'a Element,
    port: String,
}

impl<'a> Cx<'a> {
    fn covers(&self, types: &[String], type_id: &str) -> bool {
        if types.is_empty() { self.facts.of_pack(type_id, self.pack) } else { self.facts.lists(self.pack, types, type_id) }
    }

    fn issue(&self, what: String, subjects: Vec<String>, items: Vec<String>, page: Option<usize>) -> Issue {
        let mut i = Issue::new(self.rule.id.clone(), self.rule.severity, cut(&what, 400)).with_subjects(subjects).with_items(items);
        i.page = page.and_then(|p| self.doc.pages.get(p)).map(|p| p.id.clone());
        i
    }

    /// The pin at `(shape, point)` on page `page`, when the shape draws an element.
    fn pin(&self, page: usize, shape: &str, point: usize) -> Option<Pin<'a>> {
        let s = self.doc.pages.get(page)?.shape(shape)?;
        let element = self.doc.model.element(s.element.as_deref()?)?;
        let port = port_id(self.cat, s, element, point).unwrap_or_else(|| point.to_string());
        Some(Pin { element, port })
    }

    /// The direction of a pin's port on its element type or a supertype.
    fn direction(&self, pin: &Pin<'_>) -> PortDirection {
        self.facts
            .chain(&pin.element.type_id)
            .iter()
            .filter_map(|t| self.cat.element_type(t).map(|(_, et)| et))
            .find_map(|et| et.ports.iter().find(|p| p.id == pin.port))
            .map(|p| p.direction)
            .unwrap_or_default()
    }

    /// Whether `net` is one the test looks at: its kind listed (or none listed), and a segment of
    /// the connector type when one is named.
    fn net_wanted(&self, ix: &NetIndex, net: &Net, t: &NetTest<'_>) -> bool {
        if !t.kinds.is_empty() && !t.kinds.iter().any(|k| k == net.kind.name()) {
            return false;
        }
        if t.connector_type.is_empty() {
            return true;
        }
        let Some(entry) = ix.nets.get(&net.id) else { return false };
        entry.segments.iter().any(|c| self.doc.pages.iter().any(|p| p.connector(c).is_some_and(|c| c.kind == t.connector_type)))
    }

    /// The pins of `net`, with the page each is on (pins are listed per page by the index).
    fn pins_of(&self, ix: &NetIndex, net: &str) -> Vec<Pin<'a>> {
        let Some(entry) = ix.nets.get(net) else { return Vec::new() };
        let mut out = Vec::new();
        for (shape, point) in &entry.pins {
            let page = self.doc.pages.iter().position(|p| p.shape(shape).is_some());
            if let Some(pin) = page.and_then(|pi| self.pin(pi, shape, *point)) {
                out.push(pin);
            }
        }
        out
    }

    fn net_label(net: &Net) -> String {
        let n = if !net.number.is_empty() {
            &net.number
        } else if !net.name.is_empty() {
            &net.name
        } else {
            &net.id
        };
        cut(n, MAX_LABEL)
    }

    fn net_page(&self, ix: &NetIndex, net: &str) -> Option<usize> {
        let first = ix.nets.get(net)?.pages.first()?;
        self.doc.pages.iter().position(|p| p.id == *first)
    }

    fn segments(ix: &NetIndex, net: &str) -> Vec<String> {
        ix.nets.get(net).map(|e| e.segments.clone()).unwrap_or_default()
    }

    /// Pins of covered types on no net: one issue per shape.
    fn unconnected(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
            for s in &page.shapes {
                let Some(e) = s.element.as_deref().and_then(|e| self.doc.model.element(e)) else { continue };
                if !self.covers(t.types, &e.type_id) || !has_ports(self.cat, &self.facts, s, e) {
                    continue;
                }
                let free: Vec<String> = (0..s.connection_points.len())
                    .filter(|i| !ix.of_pin.contains_key(&(s.id.clone(), *i)))
                    .map(|i| port_id(self.cat, s, e, i).unwrap_or_else(|| i.to_string()))
                    .collect();
                if free.is_empty() {
                    continue;
                }
                let listed: Vec<String> = free.iter().take(MAX_LISTED).cloned().collect();
                let what = if free.len() == 1 {
                    format!("Pin {} of {} is not connected.", listed.join(", "), label(e))
                } else {
                    format!("Pins {} of {} are not connected.", listed.join(", "), label(e))
                };
                out.push(self.issue(what, vec![e.id.clone()], vec![s.id.clone()], Some(pi)));
            }
        }
    }

    /// Unnamed nets that join one pin.
    fn single_pin(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        for net in &self.doc.model.nets {
            if net.named() || !self.net_wanted(ix, net, t) {
                continue;
            }
            let Some(entry) = ix.nets.get(&net.id) else { continue };
            if entry.segments.is_empty() || entry.pins.len() != 1 {
                continue;
            }
            let what = format!("Net {} joins only one pin.", Self::net_label(net));
            out.push(self.issue(what, vec![net.id.clone()], entry.segments.clone(), self.net_page(ix, &net.id)));
        }
    }

    /// Nets with segments and neither a number nor a name.
    fn unnumbered(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        for net in &self.doc.model.nets {
            if net.named() || !self.net_wanted(ix, net, t) || Self::segments(ix, &net.id).is_empty() {
                continue;
            }
            let what = format!("Net {} has no wire number.", net.id);
            out.push(self.issue(what, vec![net.id.clone()], Self::segments(ix, &net.id), self.net_page(ix, &net.id)));
        }
    }

    /// Two nets with one number.
    fn duplicate_numbers(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        let mut by: BTreeMap<&str, Vec<&Net>> = BTreeMap::new();
        for net in self.doc.model.nets.iter().filter(|n| !n.number.is_empty() && self.net_wanted(ix, n, t)) {
            by.entry(net.number.as_str()).or_default().push(net);
        }
        for (number, nets) in by.into_iter().filter(|(_, v)| v.len() > 1) {
            let what = format!("Wire number {} is used by {} nets.", cut(number, MAX_LABEL), nets.len());
            let items = nets.iter().flat_map(|n| Self::segments(ix, &n.id)).collect();
            out.push(self.issue(what, nets.iter().map(|n| n.id.clone()).collect(), items, None));
        }
    }

    /// Green-yellow on a net that is not protective earth.
    fn reserved_colour(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        let key = if t.property.is_empty() { "colour" } else { t.property };
        for net in self.doc.model.nets.iter().filter(|n| self.net_wanted(ix, n, t)) {
            let Some(colour) = net.properties.get(key).map(PropValue::display) else { continue };
            if !is_green_yellow(&colour) {
                continue;
            }
            let pe = |s: &str| matches!(s.trim().to_ascii_uppercase().as_str(), "PE" | "PEN");
            let on_earth = self.pins_of(ix, &net.id).iter().any(|p| p.port == "PE");
            if pe(&net.name) || pe(&net.number) || on_earth {
                continue;
            }
            let what = format!("Net {} is green-yellow, which is kept for protective earth.", Self::net_label(net));
            out.push(self.issue(what, vec![net.id.clone()], Self::segments(ix, &net.id), self.net_page(ix, &net.id)));
        }
    }

    /// Pins of covered types with more than `max` segments on them.
    fn fan_in(&self, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        let max = t.max.max(1) as usize;
        for (pi, page) in self.doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
            let mut count: HashMap<(String, usize), Vec<String>> = HashMap::new();
            for c in page.connectors.iter().filter(|c| c.net.is_some()) {
                for e in [&c.start, &c.end] {
                    if let Some(pin) = pin_of(page, e) {
                        count.entry(pin).or_default().push(c.id.clone());
                    }
                }
            }
            let mut over: Vec<_> = count.into_iter().filter(|(_, v)| v.len() > max).collect();
            over.sort();
            for ((shape, point), segs) in over {
                let Some(pin) = self.pin(pi, &shape, point) else { continue };
                if !self.covers(t.types, &pin.element.type_id) {
                    continue;
                }
                let what = format!("Pin {} of {} has {} wires; at most {max}.", pin.port, label(pin.element), segs.len());
                out.push(self.issue(what, vec![pin.element.id.clone()], segs, Some(pi)));
            }
        }
    }

    /// Nets with more than `max` output pins (`undriven`: input pins and no output pin).
    fn drivers(&self, ix: &NetIndex, t: &NetTest<'_>, undriven: bool, out: &mut Vec<Issue>) {
        let max = t.max.max(1) as usize;
        for net in self.doc.model.nets.iter().filter(|n| self.net_wanted(ix, n, t)) {
            let pins = self.pins_of(ix, &net.id);
            let checked: Vec<&Pin<'_>> = pins.iter().filter(|p| self.covers(t.types, &p.element.type_id)).collect();
            let outs = checked.iter().filter(|p| self.direction(p) == PortDirection::Out).count();
            let ins = checked.iter().filter(|p| self.direction(p) == PortDirection::In).count();
            let what = if undriven {
                let any_out = pins.iter().any(|p| self.direction(p) == PortDirection::Out);
                if ins == 0 || any_out {
                    continue;
                }
                format!("Net {} feeds {ins} inputs and nothing drives it.", Self::net_label(net))
            } else {
                if outs <= max {
                    continue;
                }
                format!("Net {} is driven by {outs} outputs; at most {max}.", Self::net_label(net))
            };
            let subjects = std::iter::once(net.id.clone()).chain(checked.iter().map(|p| p.element.id.clone())).collect();
            out.push(self.issue(what, subjects, Self::segments(ix, &net.id), self.net_page(ix, &net.id)));
        }
    }

    /// Nets (of the connector type) without the property.
    fn net_property(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        if t.property.is_empty() {
            return;
        }
        for net in self.doc.model.nets.iter().filter(|n| self.net_wanted(ix, n, t)) {
            if net.properties.get(t.property).is_some_and(|v| !v.display().trim().is_empty()) || Self::segments(ix, &net.id).is_empty() {
                continue;
            }
            let what = format!("Net {} has no {}.", Self::net_label(net), t.property);
            out.push(self.issue(what, vec![net.id.clone()], Self::segments(ix, &net.id), self.net_page(ix, &net.id)));
        }
    }

    /// Nets joining a checked pin (of the ports listed in `property`, all when empty) to a pin
    /// whose type is not in `peers`.
    fn allowed_peers(&self, ix: &NetIndex, t: &NetTest<'_>, out: &mut Vec<Issue>) {
        let ports: Vec<&str> = t.property.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
        for net in self.doc.model.nets.iter().filter(|n| self.net_wanted(ix, n, t)) {
            let pins = self.pins_of(ix, &net.id);
            let checked: Vec<&Pin<'_>> =
                pins.iter().filter(|p| self.covers(t.types, &p.element.type_id) && (ports.is_empty() || ports.contains(&p.port.as_str()))).collect();
            let Some(first) = checked.first() else { continue };
            let own: HashSet<&str> = checked.iter().map(|p| p.element.id.as_str()).collect();
            let bad: Vec<&Pin<'_>> =
                pins.iter().filter(|p| !own.contains(p.element.id.as_str()) && !self.facts.lists(self.pack, t.peers, &p.element.type_id)).collect();
            if bad.is_empty() {
                continue;
            }
            let names: Vec<String> = bad.iter().take(MAX_LISTED).map(|p| label(p.element)).collect();
            let what = format!("Net {} joins {} to {}, which may not share it.", Self::net_label(net), label(first.element), names.join(", "));
            let subjects = own.iter().map(|s| s.to_string()).chain(bad.iter().map(|p| p.element.id.clone())).collect();
            out.push(self.issue(what, subjects, Self::segments(ix, &net.id), self.net_page(ix, &net.id)));
        }
    }

    /// Coils with more contacts of a form than their complement allows.
    fn complement(&self, types: &[String], relationship: &str, out: &mut Vec<Issue>) {
        let mut linked: HashMap<&str, Vec<&Element>> = HashMap::new();
        for r in self.doc.model.relationships.iter().filter(|r| r.type_id == relationship) {
            if let Some(c) = self.doc.model.element(&r.source) {
                linked.entry(r.target.as_str()).or_default().push(c);
            }
        }
        for coil in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id)) {
            let contacts = linked.get(coil.id.as_str()).map(Vec::as_slice).unwrap_or_default();
            let form = |c: &Element| c.prop("form").map(PropValue::display).unwrap_or_else(|| "no".into());
            for (key, want, name) in [("complementNo", "no", "make"), ("complementNc", "nc", "break")] {
                let Some(limit) = coil.prop(key).and_then(number) else { continue };
                let n = contacts.iter().filter(|c| form(c) == want || form(c) == "co").count();
                if (n as f64) > limit {
                    let what = format!("{} has {n} {name} contacts; its complement is {limit}.", label(coil));
                    let subjects = std::iter::once(coil.id.clone()).chain(contacts.iter().map(|c| c.id.clone())).collect();
                    out.push(self.issue(what, subjects, self.views(&coil.id), None));
                }
            }
        }
    }

    /// The shapes drawing element `id`.
    fn views(&self, id: &str) -> Vec<String> {
        self.doc.pages.iter().flat_map(|p| p.shapes.iter()).filter(|s| s.element.as_deref() == Some(id)).map(|s| s.id.clone()).collect()
    }

    /// Rungs with too many outputs, and outputs with a device to their right on the rung.
    fn rung_outputs(&self, types: &[String], max: u32, at_right: bool, out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
            // (rung, element, shape) of every device of this pack on the page with a rung.
            let mut on: BTreeMap<String, Vec<(&Element, &Shape)>> = BTreeMap::new();
            for s in &page.shapes {
                let Some(e) = s.element.as_deref().and_then(|e| self.doc.model.element(e)) else { continue };
                let Some(rung) = e.prop("rung").map(PropValue::display).filter(|r| !r.is_empty()) else { continue };
                if self.facts.of_pack(&e.type_id, self.pack) {
                    on.entry(rung).or_default().push((e, s));
                }
            }
            for (rung, devices) in on {
                let outputs: Vec<&(&Element, &Shape)> = devices.iter().filter(|(e, _)| self.covers(types, &e.type_id)).collect();
                if max > 0 && outputs.len() > max as usize {
                    let what = format!("Rung {} has {} outputs; at most {max}.", cut(&rung, MAX_LABEL), outputs.len());
                    out.push(self.issue(
                        what,
                        outputs.iter().map(|(e, _)| e.id.clone()).collect(),
                        outputs.iter().map(|(_, s)| s.id.clone()).collect(),
                        Some(pi),
                    ));
                }
                if at_right {
                    for (e, s) in &outputs {
                        let right = devices.iter().any(|(o, os)| o.id != e.id && !self.covers(types, &o.type_id) && os.center().x > s.center().x);
                        if right {
                            let what = format!("{} is not the last device on rung {}.", label(e), cut(&rung, MAX_LABEL));
                            out.push(self.issue(what, vec![e.id.clone()], vec![s.id.clone()], Some(pi)));
                        }
                    }
                }
            }
        }
    }

    /// Pages whose builder symbols mix the IEC and the American symbol sets.
    fn mixed_symbols(&self, types: &[String], out: &mut Vec<Issue>) {
        for (pi, page) in self.doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
            let mut iec: Vec<&Shape> = Vec::new();
            let mut american: Vec<&Shape> = Vec::new();
            for s in &page.shapes {
                let Some(e) = s.element.as_deref().and_then(|e| self.doc.model.element(e)) else { continue };
                if !self.covers(types, &e.type_id) {
                    continue;
                }
                match symbol_family(self.cat, s, e) {
                    Some(true) => iec.push(s),
                    Some(false) => american.push(s),
                    None => {}
                }
            }
            if iec.is_empty() || american.is_empty() {
                continue;
            }
            let (few, many) = if iec.len() < american.len() { (&iec, "American") } else { (&american, "IEC") };
            let what = format!("{} symbols on this page are drawn to another standard than the {many} symbols around them.", few.len());
            let subjects = few.iter().filter_map(|s| s.element.clone()).collect();
            out.push(self.issue(what, subjects, few.iter().map(|s| s.id.clone()).collect(), Some(pi)));
        }
    }

    /// Relays whose function number does not read as an IEEE C37.2 device number.
    fn device_numbers(&self, types: &[String], property: &str, out: &mut Vec<Issue>) {
        for e in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id)) {
            let Some(v) = e.prop(property).map(PropValue::display).filter(|v| !v.trim().is_empty()) else { continue };
            if is_device_number(&v) {
                continue;
            }
            let what = format!("{} has function {}, which is not an IEEE C37.2 device number.", label(e), cut(&v, MAX_LABEL));
            out.push(self.issue(what, vec![e.id.clone()], self.views(&e.id), None));
        }
    }

    /// Cables whose cores do not match their core count, or repeat a number.
    fn cable_cores(&self, types: &[String], cores: &[String], out: &mut Vec<Issue>) {
        for cable in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id)) {
            let owned: Vec<&Element> = self
                .doc
                .model
                .elements
                .iter()
                .filter(|c| c.owner.as_deref() == Some(cable.id.as_str()) && (cores.is_empty() || self.facts.lists(self.pack, cores, &c.type_id)))
                .collect();
            if let Some(want) = cable.prop("cores").and_then(number)
                && (owned.len() as f64 - want).abs() > f64::EPSILON
            {
                let what = format!("{} has {} cores drawn; its core count is {want}.", label(cable), owned.len());
                out.push(self.issue(what, vec![cable.id.clone()], self.views(&cable.id), None));
            }
            let mut seen: HashSet<String> = HashSet::new();
            for c in &owned {
                let n = c.prop("number").map(PropValue::display).unwrap_or_default();
                if !n.is_empty() && !seen.insert(n.clone()) {
                    let what = format!("{} has two cores numbered {}.", label(cable), cut(&n, MAX_LABEL));
                    out.push(self.issue(what, vec![cable.id.clone(), c.id.clone()], self.views(&cable.id), None));
                }
            }
        }
    }

    /// Elements drawn on too few or too many pages of a viewpoint.
    fn placed_on(&self, types: &[String], viewpoint: &str, min: u32, max: u32, out: &mut Vec<Issue>) {
        let pages: Vec<usize> =
            self.doc.pages.iter().enumerate().filter(|(_, p)| p.view.as_ref().is_some_and(|v| v.viewpoint == viewpoint)).map(|(i, _)| i).collect();
        if pages.is_empty() {
            return;
        }
        let vp = self.pack.viewpoint(viewpoint).map(|v| v.label.clone()).unwrap_or_else(|| viewpoint.to_string());
        for e in self.doc.model.elements.iter().filter(|e| self.covers(types, &e.type_id)) {
            let n = pages
                .iter()
                .filter_map(|&i| self.doc.pages.get(i))
                .filter(|p| p.shapes.iter().any(|s| s.element.as_deref() == Some(e.id.as_str())))
                .count();
            let what = if (n as u32) < min {
                format!("{} is not placed on a {vp} page.", label(e))
            } else if max > 0 && n as u32 > max {
                format!("{} is placed on {n} {vp} pages; at most {max}.", label(e))
            } else {
                continue;
            };
            out.push(self.issue(what, vec![e.id.clone()], self.views(&e.id), None));
        }
    }
}

/// Whether the shape or element type has ports to connect.
fn has_ports(cat: &Catalog, facts: &Facts<'_>, s: &Shape, e: &Element) -> bool {
    s.glyph.as_ref().is_some_and(|g| !g.ports.is_empty())
        || facts.chain(&e.type_id).iter().any(|t| cat.element_type(t).is_some_and(|(_, et)| !et.ports.is_empty()))
}

/// The port id at connection point `point` of `s`: the builder's port there, else the element
/// type's port `point`.
pub fn port_id(cat: &Catalog, s: &Shape, e: &Element, point: usize) -> Option<String> {
    if let Some(id) = s.glyph.as_ref().and_then(|g| g.ports.get(point)) {
        return Some(id.clone());
    }
    crate::infer::chain_of(cat, &e.type_id).iter().find_map(|t| cat.element_type(t).and_then(|(_, et)| et.ports.get(point)).map(|p| p.id.clone()))
}

/// Whether a builder shape draws to the IEC set (`Some(true)`), the American one
/// (`Some(false)`), or neither says (`None`): its `standard` (or a gate's `shapes`) from the
/// shape's own overrides, else its pack glyph.
fn symbol_family(cat: &Catalog, s: &Shape, e: &Element) -> Option<bool> {
    let g = s.glyph.as_ref()?;
    if g.builder.is_empty() {
        return None;
    }
    let glyph = cat.glyph(&s.stencil).map(|(_, g)| g);
    let read = |k: &str| g.params.get(k).or_else(|| e.properties.get(k)).or_else(|| glyph.and_then(|gl| gl.params.get(k))).map(PropValue::display);
    match (read("standard"), read("shapes")) {
        (_, Some(sh)) if g.builder == "gate" => Some(sh != "distinctive"),
        (Some(st), _) => Some(st == "iec60617"),
        // No standard named anywhere: the builder's default, the IEC set.
        (None, _) => Some(true),
    }
}

/// Whether text names green-yellow (`GNYE`, `GN/YE`, `green-yellow`).
fn is_green_yellow(c: &str) -> bool {
    let t: String = c.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    matches!(t.as_str(), "gnye" | "greenyellow" | "yellowgreen" | "yegn")
}

fn number(v: &PropValue) -> Option<f64> {
    match v {
        PropValue::Number(n) if n.is_finite() => Some(*n),
        PropValue::Text(t) => t.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

fn label(e: &Element) -> String {
    let t = if !e.tag.trim().is_empty() {
        e.tag.trim()
    } else if !e.name.trim().is_empty() {
        e.name.trim()
    } else {
        e.id.as_str()
    };
    cut(&t.replace(['\n', '\r'], " "), MAX_LABEL)
}

/// The suffixes IEEE C37.2 adds to a device function number, as secondary sources list them
/// (neutral, ground, transformer, phase, breaker failure, and the common letter suffixes).
const SUFFIXES: &[&str] = &["", "N", "G", "T", "P", "BF", "Q", "V", "L", "B", "M", "R", "S", "X", "Y", "Z", "GS", "TN", "TG", "TD", "TS"];

/// The acronyms IEEE C37.2-2022 adds beside the numbers (secondary sources).
const ACRONYMS: &[&str] = &["PMU", "DFR", "RAS", "AFD", "SER", "CLK", "LGC", "TCM", "TCU", "DDR", "HMI", "RIO"];

/// Whether `v` reads as device function numbers joined by `/` (`50/51`, `51G`, `87T`, `50BF`):
/// each part a number from 1 to 99 (the C37.2 table; secondary sources) and an optional suffix,
/// or one of the C37.2 acronyms.
pub fn is_device_number(v: &str) -> bool {
    let v = v.trim();
    if v.is_empty() || v.len() > 32 {
        return false;
    }
    v.split('/').all(|part| {
        let part = part.trim();
        if ACRONYMS.contains(&part) {
            return true;
        }
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        let suffix = part.get(digits.len()..).unwrap_or("");
        let n: u32 = digits.parse().unwrap_or(0);
        (1..=99).contains(&n) && digits.len() <= 2 && SUFFIXES.contains(&suffix)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_numbers_follow_the_c37_2_grammar() {
        for ok in ["50", "50/51", "51G", "87T", "50BF", "52", "PMU", "86", "27/59"] {
            assert!(is_device_number(ok), "{ok}");
        }
        for bad in ["", "0", "100", "51Q7", "51-G", "abc", "5/", "x50", "999"] {
            assert!(!is_device_number(bad), "{bad}");
        }
    }

    #[test]
    fn green_yellow_is_read_in_several_spellings() {
        for c in ["GNYE", "gn/ye", "Green-Yellow", "GN-YE"] {
            assert!(is_green_yellow(c), "{c}");
        }
        assert!(!is_green_yellow("BK") && !is_green_yellow("green"));
    }
}
