//! The fluid power circuit model: ports, lines and the paths inside components, built from the
//! drawing, and a trace along them.
//!
//! [`Circuit::build`] reads every view of a fluid power component (an element whose type extends
//! `fluidpower.Component`): its ports come from the built glyph (`Shape.glyph.ports`), else from
//! the type's declared ports; the lines between them come from the net index
//! (`vizcraft_diagram::net::NetIndex`). Each component joins some of its ports inside, by what it
//! is:
//!
//! - a directional valve (the `valve` builder) joins the ports its `paths` text joins in each
//!   switching position ([`paths`]); a walk takes the rest position or any position;
//! - a pump (`pump`, one flow direction) draws from its suction port S into its pressure port P;
//! - a pressure valve (`pressureValve`) passes P to T (relief) or P to A (reducing, sequence);
//! - a check valve passes 1 to 2 only; filters, coolers, throttles, flow controls, shut-off
//!   valves and service units pass 1 and 2 both ways; a reservoir joins its ports;
//! - an actuator joins nothing (A and B are the two sides of the piston).
//!
//! The values a component is drawn with come in layers, as the glyph builders read them: the
//! element's property, the shape's own parameter, the type's glyph params under the page's
//! variant, then the builder's default ([`param`]). [`Circuit::trace`] walks from ports through
//! lines and the components a [`Walk`] lets it pass.
//!
//! [`rules`] holds the pack's checks, [`steps`] the displacement-step chart.

pub mod paths;
pub mod rules;
pub mod steps;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use vizcraft_diagram::net::{NetIndex, pin_of};
use vizcraft_diagram::{Document, Page, Shape};
use vizcraft_model::{Element, PropValue};

use crate::Catalog;

/// The pack id.
pub const PACK: &str = "fluidpower";

/// The supertype of every fluid power component.
pub const COMPONENT: &str = "fluidpower.Component";
/// Pumps, compressors and pressure sources.
pub const SOURCE: &str = "fluidpower.Source";
/// Accumulators: they store pressure and give it back.
pub const STORAGE: &str = "fluidpower.Storage";
/// Cylinders and motors.
pub const ACTUATOR: &str = "fluidpower.Actuator";
/// Reservoirs.
pub const TANK: &str = "fluidpower.Tank";
/// Directional control valves.
pub const DIRECTIONAL: &str = "fluidpower.DirectionalValve";
/// The displacement-step chart.
pub const STEP_CHART: &str = "fluidpower.StepChart";

/// Most components one circuit holds; a bigger drawing is read up to it.
pub const MAX_PARTS: usize = 20_000;

/// Most ports one component contributes.
pub const MAX_PORTS: usize = 64;

/// What a component does to the flow, from its builder and parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A directional valve: joins ports by switching position.
    Valve,
    /// A pump or compressor (one flow direction): S into P.
    Pump,
    /// A pump with two flow directions or a motor.
    Rotary,
    /// A cylinder: joins nothing.
    Cylinder,
    /// A pressure valve: P to T or P to A, one way.
    PressureValve,
    /// Passes flow between its two ports (filter, cooler, throttle, shut-off, service unit), or
    /// one way for a check valve.
    PassThrough,
    /// A reservoir: its ports are one volume.
    Tank,
    /// A one-port end: a source, an accumulator, a gauge, a silencer, an exhaust.
    End,
    /// Anything else: no inside paths.
    Other,
}

/// A port of a component view.
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    /// Index of the component in [`Circuit::parts`].
    pub part: usize,
    /// Connection point index on the shape.
    pub point: usize,
    /// The port id (`P`, `14`, `shaft`).
    pub id: String,
    /// The net its line belongs to, if any.
    pub net: Option<String>,
    /// Some connector is glued to it.
    pub glued: bool,
    /// An exhaust that may stay open (pneumatic valve exhausts, a compressor's intake).
    pub exhaust: bool,
}

/// A path inside a component, between two of its ports (indices into [`Circuit::ports`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// One end.
    pub a: usize,
    /// The other end.
    pub b: usize,
    /// Flow goes from `a` to `b` only.
    pub directed: bool,
}

/// One drawn component.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Page index.
    pub page: usize,
    /// The shape id.
    pub shape: String,
    /// The element id.
    pub element: String,
    /// The element type.
    pub type_id: String,
    /// The element's tag (its component code).
    pub tag: String,
    /// The glyph builder that draws it (empty for none).
    pub builder: String,
    /// What it does to the flow.
    pub role: Role,
    /// `hydraulic` or `pneumatic`.
    pub medium: String,
    /// Its ports (indices into [`Circuit::ports`]), in connection point order.
    pub ports: Vec<usize>,
    /// Paths that do not depend on a switching position.
    pub edges: Vec<Edge>,
    /// A directional valve's paths per position, left to right.
    pub positions: Vec<Vec<Edge>>,
    /// A directional valve's rest position.
    pub rest: usize,
    /// The positions a valve can be put in: the rest, an end square with an operator, the
    /// spring-centred middle.
    pub selectable: Vec<usize>,
    /// A directional valve's operators and port labels.
    pub valve: Option<ValveInfo>,
    /// A pressure valve's function or an accessory's kind (`relief`, `reservoir`); empty else.
    pub kind: String,
    /// The centre of the view.
    pub at: (f64, f64),
}

/// What a directional valve is drawn with, beyond its paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValveInfo {
    /// Switching positions.
    pub positions: i64,
    /// The operator at the left side.
    pub left: String,
    /// The operator at the right side.
    pub right: String,
    /// The return (`spring`, `detent`, `none`).
    pub ret: String,
    /// Ports labelled with ISO 5599 numbers rather than ISO 9461 letters.
    pub numbers: bool,
    /// The supply port (P or 1), an index into [`Circuit::ports`].
    pub supply: Option<usize>,
    /// The exhaust ports (T, R, S; 3, 5).
    pub exhausts: Vec<usize>,
}

impl Part {
    /// The port with this id.
    pub fn port(&self, c: &Circuit, id: &str) -> Option<usize> {
        self.ports.iter().copied().find(|&p| c.ports.get(p).is_some_and(|x| x.id == id))
    }
}

/// Which positions a walk takes through directional valves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Positions {
    /// The rest position only.
    Rest,
    /// Any position.
    #[default]
    Any,
}

/// What a trace may pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    /// Positions taken through directional valves.
    pub positions: Positions,
    /// Pass through directional valves.
    pub valves: bool,
    /// Pass through pressure valves.
    pub pressure_valves: bool,
    /// Pass through reservoirs.
    pub tanks: bool,
    /// Follow one-way paths only their way.
    pub directed: bool,
}

impl Walk {
    /// Everything a pressure trace passes: every valve in any position (or the rest position),
    /// one way paths their way, never through a reservoir.
    pub fn pressure(positions: Positions) -> Self {
        Walk { positions, valves: true, pressure_valves: true, tanks: false, directed: true }
    }

    /// Lines and pass-through parts only.
    pub fn lines() -> Self {
        Walk::default()
    }
}

/// What a trace reached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reached {
    /// Ports (indices into [`Circuit::ports`]).
    pub ports: BTreeSet<usize>,
    /// Components (indices into [`Circuit::parts`]).
    pub parts: BTreeSet<usize>,
    /// Nets.
    pub nets: BTreeSet<String>,
}

/// A circuit: the components drawn, their ports and the nets joining them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Circuit {
    /// Every component view, pages in order, shapes in z order.
    pub parts: Vec<Part>,
    /// Every port of those views.
    pub ports: Vec<Port>,
    /// The ports on each net.
    pub nets: BTreeMap<String, Vec<usize>>,
    /// The segments (connector ids) of each net.
    pub segments: BTreeMap<String, Vec<String>>,
}

/// The value of builder parameter `key` for `shape` drawing `element` on a page with `variant`:
/// the element's property, else the shape's own parameter, else the type's glyph params.
pub fn param<'a>(cat: &'a Catalog, shape: &'a Shape, element: Option<&'a Element>, variant: &str, key: &str) -> Option<&'a PropValue> {
    if let Some(v) = element.and_then(|e| e.properties.get(key)) {
        return Some(v);
    }
    if let Some(v) = shape.glyph.as_ref().and_then(|g| g.params.get(key)) {
        return Some(v);
    }
    let glyph =
        element.and_then(|e| cat.master_for_type_in(&e.type_id, variant)).or(Some(shape.stencil.as_str())).and_then(|g| cat.glyph(g)).map(|(_, g)| g);
    glyph.and_then(|g| g.params.get(key))
}

/// [`param`] as text, else `default`.
pub fn text_param<'a>(cat: &'a Catalog, shape: &'a Shape, element: Option<&'a Element>, variant: &str, key: &str, default: &'a str) -> &'a str {
    param(cat, shape, element, variant, key).and_then(PropValue::as_str).unwrap_or(default)
}

/// [`param`] as a whole number, else `default`.
pub fn int_param(cat: &Catalog, shape: &Shape, element: Option<&Element>, variant: &str, key: &str, default: i64) -> i64 {
    param(cat, shape, element, variant, key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok())))
        .filter(|n| n.is_finite())
        .map(|n| n.round().clamp(-1e9, 1e9) as i64)
        .unwrap_or(default)
}

/// [`param`] as a flag, else `default`.
pub fn flag_param(cat: &Catalog, shape: &Shape, element: Option<&Element>, variant: &str, key: &str, default: bool) -> bool {
    param(cat, shape, element, variant, key)
        .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|t| t.trim().eq_ignore_ascii_case("true"))))
        .unwrap_or(default)
}

/// The page's rendering variant, empty for the default drawing.
pub fn page_variant(page: &Page) -> &str {
    page.view.as_ref().map(|v| v.variant.as_str()).unwrap_or_default()
}

/// Whether `type_id` is or extends `of`.
pub fn is_a(cat: &Catalog, type_id: &str, of: &str) -> bool {
    crate::infer::chain_of(cat, type_id).contains(&of)
}

/// A property of `e`, else the default the nearest type in its chain declares for it.
pub fn prop_or_default<'a>(cat: &'a Catalog, e: &'a Element, key: &str) -> Option<&'a PropValue> {
    if let Some(v) = e.properties.get(key) {
        return Some(v);
    }
    crate::infer::chain_of(cat, &e.type_id)
        .into_iter()
        .find_map(|t| cat.element_type(t).and_then(|(_, ty)| ty.property(key)).and_then(|p| p.default.as_ref()))
}

/// The ISO 1219-2 letter of a component (`P`, `A`, `V`, `S`, `M`, `Z`): its `letter` property or
/// its type's default.
pub fn letter_of(cat: &Catalog, e: &Element) -> String {
    prop_or_default(cat, e, "letter").and_then(PropValue::as_str).map(str::trim).unwrap_or("Z").chars().take(4).collect()
}

/// The circuit number of a component (`circuit`, default 1; 0 is the power unit), clamped to
/// `0..=999`.
pub fn circuit_of(cat: &Catalog, e: &Element) -> i64 {
    prop_or_default(cat, e, "circuit")
        .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok())))
        .filter(|n| n.is_finite())
        .map(|n| n.round().clamp(0.0, 999.0) as i64)
        .unwrap_or(1)
}

/// The installation of a component (`installation`, default empty), at most 16 characters.
pub fn installation_of(cat: &Catalog, e: &Element) -> String {
    prop_or_default(cat, e, "installation").and_then(PropValue::as_str).map(str::trim).unwrap_or("").chars().take(16).collect()
}

/// The ports a component has plugged (`plugged`, space or comma separated).
pub fn plugged_of(e: &Element) -> Vec<String> {
    e.prop("plugged")
        .and_then(PropValue::as_str)
        .map(|t| t.split([' ', ',']).filter(|x| !x.is_empty()).take(MAX_PORTS).map(str::to_string).collect())
        .unwrap_or_default()
}

/// The port ids of `shape` drawing an element of `type_id`: the built glyph's, else the type's
/// declared ports (from its chain).
pub fn port_ids(cat: &Catalog, shape: &Shape, type_id: &str) -> Vec<String> {
    if let Some(g) = shape.glyph.as_ref().filter(|g| !g.ports.is_empty()) {
        return g.ports.iter().take(MAX_PORTS).cloned().collect();
    }
    crate::infer::chain_of(cat, type_id)
        .into_iter()
        .filter_map(|t| cat.element_type(t).map(|(_, ty)| ty))
        .find(|ty| !ty.ports.is_empty())
        .map(|ty| ty.ports.iter().take(MAX_PORTS).map(|p| p.id.clone()).collect())
        .unwrap_or_default()
}

/// The builder that draws `shape`: its built state's, else the glyph of its stencil or type.
fn builder_of<'a>(cat: &'a Catalog, shape: &'a Shape, e: &Element, variant: &str) -> &'a str {
    if let Some(g) = shape.glyph.as_ref().filter(|g| !g.builder.is_empty()) {
        return &g.builder;
    }
    cat.master_for_type_in(&e.type_id, variant).and_then(|g| cat.glyph(g)).map(|(_, g)| g.builder.as_str()).unwrap_or_default()
}

impl Circuit {
    /// The circuit drawn in `doc` (on page `page` only, when given).
    pub fn build(doc: &Document, cat: &Catalog, page: Option<usize>) -> Circuit {
        let ix = NetIndex::build(doc);
        let elements: HashMap<&str, &Element> = doc.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut c = Circuit::default();
        for (pi, pg) in doc.pages.iter().enumerate() {
            if page.is_some_and(|p| p != pi) {
                continue;
            }
            let variant = page_variant(pg);
            let mut glued: HashSet<(String, usize)> = HashSet::new();
            for con in &pg.connectors {
                for end in [&con.start, &con.end] {
                    if let Some(pin) = pin_of(pg, end) {
                        glued.insert(pin);
                    }
                }
            }
            for shape in &pg.shapes {
                if c.parts.len() >= MAX_PARTS {
                    break;
                }
                let Some(e) = shape.element.as_deref().and_then(|id| elements.get(id)).copied() else { continue };
                if !is_a(cat, &e.type_id, COMPONENT) {
                    continue;
                }
                c.add_part(cat, &ix, &glued, pi, shape, e, variant);
            }
        }
        for (i, p) in c.ports.iter().enumerate() {
            if let Some(n) = &p.net {
                c.nets.entry(n.clone()).or_default().push(i);
            }
        }
        for (n, entry) in &ix.nets {
            if c.nets.contains_key(n) {
                c.segments.insert(n.clone(), entry.segments.clone());
            }
        }
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn add_part(&mut self, cat: &Catalog, ix: &NetIndex, glued: &HashSet<(String, usize)>, pi: usize, shape: &Shape, e: &Element, variant: &str) {
        let builder = builder_of(cat, shape, e, variant).to_string();
        let medium = text_param(cat, shape, Some(e), variant, "medium", paths::DEFAULT_MEDIUM).to_string();
        let pneumatic = medium == "pneumatic";
        let ids = port_ids(cat, shape, &e.type_id);
        let index = self.parts.len();
        let first = self.ports.len();
        // Exhausts that may stay open: a pneumatic valve's exhausts, a compressor's intake.
        let mut exhausts: HashSet<String> = HashSet::new();
        let mut valve: Option<(Vec<paths::ValvePort>, bool, String, i64, i64)> = None;
        if builder == "valve" {
            let ports = int_param(cat, shape, Some(e), variant, "ports", paths::DEFAULT_PORTS);
            let positions = int_param(cat, shape, Some(e), variant, "positions", paths::DEFAULT_POSITIONS).clamp(2, 3);
            let numbers = text_param(cat, shape, Some(e), variant, "portLabels", paths::DEFAULT_LABELS) == "iso5599";
            let set = paths::port_set(ports, pneumatic);
            if pneumatic {
                exhausts.extend(set.iter().filter(|p| p.exhaust).map(|p| p.id(numbers).to_string()));
            }
            let text = paths::effective(text_param(cat, shape, Some(e), variant, "paths", ""), ports, positions).to_string();
            valve = Some((set, numbers, text, ports, positions));
        }
        if builder == "pump" && pneumatic {
            exhausts.insert("S".into());
        }
        for (point, id) in ids.iter().enumerate() {
            self.ports.push(Port {
                part: index,
                point,
                id: id.clone(),
                net: ix.of_pin.get(&(shape.id.clone(), point)).cloned(),
                glued: glued.contains(&(shape.id.clone(), point)),
                exhaust: exhausts.contains(id),
            });
        }
        let ports: Vec<usize> = (first..self.ports.len()).collect();
        let at = |id: &str| ports.iter().copied().find(|&p| self.ports.get(p).is_some_and(|x| x.id == id));
        let edge = |a: Option<usize>, b: Option<usize>, directed: bool| Some(Edge { a: a?, b: b?, directed });
        let mut part = Part {
            page: pi,
            shape: shape.id.clone(),
            element: e.id.clone(),
            type_id: e.type_id.clone(),
            tag: e.tag.clone(),
            builder: builder.clone(),
            role: Role::Other,
            medium,
            ports: ports.clone(),
            edges: Vec::new(),
            positions: Vec::new(),
            rest: 0,
            selectable: Vec::new(),
            valve: None,
            kind: String::new(),
            at: (shape.x + shape.w / 2.0, shape.y + shape.h / 2.0),
        };
        match builder.as_str() {
            "valve" => {
                part.role = Role::Valve;
                if let Some((set, numbers, text, _, positions)) = valve {
                    let n = positions as usize;
                    for square in paths::flows(&set, &text, n) {
                        let edges = square
                            .iter()
                            .filter_map(|&(a, b)| {
                                let pa = set.get(a).map(|p| p.id(numbers)).and_then(at);
                                let pb = set.get(b).map(|p| p.id(numbers)).and_then(at);
                                edge(pa, pb, false)
                            })
                            .collect();
                        part.positions.push(edges);
                    }
                    let rest = int_param(cat, shape, Some(e), variant, "rest", paths::DEFAULT_REST).clamp(0, positions - 1) as usize;
                    let left = text_param(cat, shape, Some(e), variant, "actuatorLeft", paths::DEFAULT_LEFT);
                    let right = text_param(cat, shape, Some(e), variant, "actuatorRight", paths::DEFAULT_RIGHT);
                    let ret = text_param(cat, shape, Some(e), variant, "return", paths::DEFAULT_RETURN);
                    part.rest = rest;
                    let mut sel = vec![rest];
                    if left != "none" {
                        sel.push(0);
                    }
                    if right != "none" {
                        sel.push(n.saturating_sub(1));
                    }
                    if n == 3 && ret == "spring" {
                        sel.push(1);
                    }
                    sel.sort_unstable();
                    sel.dedup();
                    part.selectable = sel;
                    part.valve = Some(ValveInfo {
                        positions,
                        left: left.to_string(),
                        right: right.to_string(),
                        ret: ret.to_string(),
                        numbers,
                        supply: set.iter().find(|p| p.number == "1").map(|p| p.id(numbers)).and_then(at),
                        exhausts: set.iter().filter(|p| p.exhaust).filter_map(|p| at(p.id(numbers))).collect(),
                    });
                }
            }
            "pump" => {
                let pump = text_param(cat, shape, Some(e), variant, "role", "pump") != "motor";
                let bi = text_param(cat, shape, Some(e), variant, "flow", "uni") == "bi";
                if pump && !bi {
                    part.role = Role::Pump;
                    part.edges.extend(edge(at("S"), at("P"), true));
                } else {
                    part.role = Role::Rotary;
                }
            }
            "cylinder" => part.role = Role::Cylinder,
            "pressureValve" => {
                part.role = Role::PressureValve;
                part.kind = text_param(cat, shape, Some(e), variant, "function", "relief").to_string();
                let out = if part.kind == "relief" { "T" } else { "A" };
                part.edges.extend(edge(at("P"), at(out), true));
            }
            "fluidPart" => {
                let kind = text_param(cat, shape, Some(e), variant, "kind", "filter");
                part.kind = kind.to_string();
                match kind {
                    "reservoir" => {
                        part.role = Role::Tank;
                        for w in ports.windows(2) {
                            if let [a, b] = w {
                                part.edges.push(Edge { a: *a, b: *b, directed: false });
                            }
                        }
                    }
                    "checkValve" => {
                        part.role = Role::PassThrough;
                        part.edges.extend(edge(at("1"), at("2"), true));
                    }
                    "filter" | "cooler" | "throttle" | "flowControl" | "shutOff" | "serviceUnit" => {
                        part.role = Role::PassThrough;
                        part.edges.extend(edge(at("1"), at("2"), false));
                    }
                    _ => part.role = Role::End,
                }
            }
            _ => {}
        }
        if is_a(cat, &e.type_id, TANK) {
            part.role = Role::Tank;
        }
        self.parts.push(part);
    }

    /// The component drawn by shape `shape`.
    pub fn part_of_shape(&self, shape: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.shape == shape)
    }

    /// The components drawing element `element`.
    pub fn parts_of_element<'a>(&'a self, element: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.parts.iter().enumerate().filter(move |(_, p)| p.element == element).map(|(i, _)| i)
    }

    /// The port at connection point `point` of shape `shape`.
    pub fn port_at(&self, shape: &str, point: usize) -> Option<usize> {
        let part = self.parts.get(self.part_of_shape(shape)?)?;
        part.ports.iter().copied().find(|&p| self.ports.get(p).is_some_and(|x| x.point == point))
    }

    /// The paths inside part `i` a walk may take.
    fn inside(&self, i: usize, walk: &Walk) -> Vec<Edge> {
        let Some(p) = self.parts.get(i) else { return Vec::new() };
        match p.role {
            Role::Valve if !walk.valves => Vec::new(),
            Role::Valve => match walk.positions {
                Positions::Rest => p.positions.get(p.rest).cloned().unwrap_or_default(),
                Positions::Any => p.positions.iter().flatten().copied().collect(),
            },
            Role::PressureValve if !walk.pressure_valves => Vec::new(),
            Role::Tank if !walk.tanks => Vec::new(),
            _ => p.edges.clone(),
        }
    }

    /// Everything reached from `from` (port indices) by `walk`: the ports on the same lines, and
    /// through the components it may pass, the ports on their far side, and so on.
    pub fn trace(&self, from: &[usize], walk: &Walk) -> Reached {
        let mut r = Reached::default();
        let mut queue: VecDeque<usize> = VecDeque::new();
        for &p in from {
            if p < self.ports.len() && r.ports.insert(p) {
                queue.push_back(p);
            }
        }
        // Each port is queued once, so the walk is linear in ports, lines and paths.
        while let Some(p) = queue.pop_front() {
            let Some(port) = self.ports.get(p) else { continue };
            r.parts.insert(port.part);
            let mut next: Vec<usize> = Vec::new();
            if let Some(n) = &port.net
                && r.nets.insert(n.clone())
            {
                next.extend(self.nets.get(n).into_iter().flatten().copied());
            }
            for e in self.inside(port.part, walk) {
                if e.a == p {
                    next.push(e.b);
                } else if e.b == p && (!e.directed || !walk.directed) {
                    next.push(e.a);
                }
            }
            for q in next {
                if r.ports.insert(q) {
                    queue.push_back(q);
                }
            }
        }
        r
    }

    /// The ports a component sends pressure out of: a pump's P (both sides of a two-way pump),
    /// a source's or an accumulator's port.
    pub fn outlets(&self, cat: &Catalog, i: usize) -> Vec<usize> {
        let Some(p) = self.parts.get(i) else { return Vec::new() };
        match p.role {
            Role::Pump => p.port(self, "P").into_iter().collect(),
            Role::Rotary if is_a(cat, &p.type_id, SOURCE) => p.ports.iter().copied().filter(|&q| self.is_fluid(q)).collect(),
            _ if is_a(cat, &p.type_id, SOURCE) || is_a(cat, &p.type_id, STORAGE) => p.ports.iter().copied().filter(|&q| self.is_fluid(q)).collect(),
            _ => Vec::new(),
        }
    }

    /// Whether port `i` carries fluid.
    pub fn is_fluid(&self, i: usize) -> bool {
        self.ports.get(i).is_some_and(|p| paths::is_fluid(&p.id))
    }
}
