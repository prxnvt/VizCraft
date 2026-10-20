//! Mechanical analyses over the `mechanical` pack's model: the Kutzbach mobility of a kinematic
//! diagram ([`mobility`]), the speed ratio along a power transmission ([`ratio`]), Mason's gain
//! formula over a control block diagram ([`mason`]) and the sequential causality assignment of a
//! bond graph ([`assign_causality`]). The notation rules call the same functions, so a rule and
//! the analysis it explains never disagree.
//!
//! Every algorithm is iterative and capped: walks keep their own stacks, loop and path searches
//! stop at [`MAX_LOOPS`], [`MAX_PATHS`] and a step budget and then say `truncated`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};
use vizcraft_model::{Element, Model, PropValue, Relationship};

use crate::{Analysis, AnalysisError, AnalysisResult, Context, Group, Mark, ResultTable, TableRow, Types};

/// Element and relationship types of the `mechanical` pack.
pub mod types {
    /// A kinematic pair.
    pub const JOINT: &str = "mechanical.Joint";
    /// The frame (link 1); every ground symbol is the same body.
    pub const GROUND: &str = "mechanical.Ground";
    /// A ternary or higher link drawn as a plate.
    pub const PLATE: &str = "mechanical.Plate";
    /// A cam body.
    pub const CAM: &str = "mechanical.Cam";
    /// A follower body.
    pub const FOLLOWER: &str = "mechanical.Follower";
    /// A binary link between two joints.
    pub const LINK: &str = "mechanical.Link";
    /// Components on one shaft turn together.
    pub const SHAFT: &str = "mechanical.Shaft";
    /// Gears or friction wheels in contact.
    pub const MESH: &str = "mechanical.Mesh";
    /// A belt between pulleys.
    pub const BELT: &str = "mechanical.Belt";
    /// A chain between sprockets.
    pub const CHAIN: &str = "mechanical.Chain";
    /// A gear.
    pub const GEAR: &str = "mechanical.Gear";
    /// A pulley.
    pub const PULLEY: &str = "mechanical.Pulley";
    /// A sprocket.
    pub const SPROCKET: &str = "mechanical.Sprocket";
    /// A friction wheel.
    pub const FRICTION_WHEEL: &str = "mechanical.FrictionWheel";
    /// A clutch.
    pub const CLUTCH: &str = "mechanical.Clutch";
    /// A motor.
    pub const MOTOR: &str = "mechanical.Motor";
    /// A driven load.
    pub const LOAD: &str = "mechanical.Load";
    /// A bearing.
    pub const BEARING: &str = "mechanical.Bearing";
    /// A bond.
    pub const BOND: &str = "mechanical.Bond";
    /// A signal line.
    pub const SIGNAL: &str = "mechanical.Signal";
    /// A summing junction.
    pub const SUMMING: &str = "mechanical.SummingJunction";
    /// A branch point.
    pub const BRANCH: &str = "mechanical.BranchPoint";
    /// The loop's input.
    pub const INPUT: &str = "mechanical.Input";
    /// The loop's output.
    pub const OUTPUT: &str = "mechanical.Output";
    /// A transfer function block.
    pub const TRANSFER: &str = "mechanical.TransferBlock";
    /// A gain triangle.
    pub const GAIN: &str = "mechanical.Gain";
    /// An integrator.
    pub const INTEGRATOR: &str = "mechanical.Integrator";
    /// A time delay.
    pub const DELAY: &str = "mechanical.Delay";
    /// A saturation.
    pub const SATURATION: &str = "mechanical.Saturation";
}

use types::*;

/// Most loops [`mason`] and the algebraic loop rule look for.
pub const MAX_LOOPS: usize = 256;

/// Most forward paths [`mason`] lists.
pub const MAX_PATHS: usize = 64;

/// Highest order of non-touching loop groups [`mason`] combines.
pub const MAX_ORDER: usize = 4;

/// Steps a path or loop search may take before it gives up and says `truncated`.
const STEP_BUDGET: usize = 200_000;

/// Most non-touching groups [`mason`] keeps.
const MAX_GROUPS: usize = 10_000;

// ---------------------------------------------------------------- shared helpers

/// The element ids an analysis is limited to (the shapes drawn on a page); `None`: the whole
/// model.
pub type Scope = Option<HashSet<String>>;

/// Whether `id` is in `scope`.
pub fn in_scope(scope: &Scope, id: &str) -> bool {
    scope.as_ref().is_none_or(|s| s.contains(id))
}

/// The `scope` param: an array of element ids (at most the model's element cap), or absent.
pub fn scope_param(v: &Value) -> Result<Scope, AnalysisError> {
    match v.get("scope") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= vizcraft_model::MAX_ELEMENTS => a
            .iter()
            .map(|x| x.as_str().map(str::to_string).ok_or_else(|| AnalysisError::Params("`scope` must be element ids".into())))
            .collect::<Result<HashSet<_>, _>>()
            .map(Some),
        Some(_) => Err(AnalysisError::Params(format!("`scope` must be an array of at most {} element ids", vizcraft_model::MAX_ELEMENTS))),
    }
}

/// A number property: a finite number, or text that reads as one.
pub fn number(props: &BTreeMap<String, PropValue>, key: &str) -> Option<f64> {
    match props.get(key)? {
        PropValue::Number(n) if n.is_finite() => Some(*n),
        PropValue::Text(t) => t.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

/// A text property, trimmed; empty when unset or not text.
pub fn text<'a>(props: &'a BTreeMap<String, PropValue>, key: &str) -> &'a str {
    props.get(key).and_then(PropValue::as_str).map(str::trim).unwrap_or_default()
}

/// A flag property: a bool, or text reading `true` or `false`.
pub fn flag(props: &BTreeMap<String, PropValue>, key: &str) -> Option<bool> {
    match props.get(key)? {
        PropValue::Bool(b) => Some(*b),
        PropValue::Text(t) => match t.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        PropValue::Number(_) => None,
    }
}

/// What a result calls an element: its name, else its tag, else its id.
pub fn name_of(e: &Element) -> String {
    let n = e.name.trim();
    let t = e.tag.trim();
    let s = if !n.is_empty() {
        n
    } else if !t.is_empty() {
        t
    } else {
        e.id.as_str()
    };
    s.chars().take(60).collect()
}

/// The elements of `model` in scope whose type is (or extends) one of `want`, in model order.
fn of<'m>(model: &'m Model, ty: &dyn Types, scope: &Scope, want: &[&str]) -> Vec<&'m Element> {
    model.elements.iter().filter(|e| in_scope(scope, &e.id) && want.iter().any(|w| ty.is_a(&e.type_id, w))).collect()
}

/// The relationships of type `want` between two elements in scope, in model order.
fn rels<'m>(model: &'m Model, ty: &dyn Types, scope: &Scope, want: &str) -> Vec<&'m Relationship> {
    model.relationships.iter().filter(|r| ty.is_a(&r.type_id, want) && in_scope(scope, &r.source) && in_scope(scope, &r.target)).collect()
}

fn params_err(t: impl Into<String>) -> AnalysisError {
    AnalysisError::Params(t.into())
}

/// The optional string param `k`.
fn opt_id<'v>(v: &'v Value, k: &str) -> Result<Option<&'v str>, AnalysisError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if !t.is_empty() && t.len() <= vizcraft_model::MAX_KEY => Ok(Some(t)),
        Some(_) => Err(params_err(format!("`{k}` must be an element id"))),
    }
}

/// A number written for a formula: whole numbers without a point, others to four significant
/// places.
pub fn fmt(n: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{n:.0}");
    }
    let s = format!("{n:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---------------------------------------------------------------- mobility

/// Degrees of freedom of a kinematic pair kind (ISO 3952-1 pairs and the planar half joints).
pub fn joint_dof(kind: &str) -> u32 {
    match kind {
        "cylindrical" | "pinInSlot" | "camContact" => 2,
        "spherical" | "planar" => 3,
        // revolute, prismatic, screw, rolling and anything unknown.
        _ => 1,
    }
}

/// Whether a pair kind moves in a plane (the others belong to spatial mechanisms).
pub fn planar_kind(kind: &str) -> bool {
    matches!(kind, "" | "revolute" | "prismatic" | "pinInSlot" | "camContact" | "rolling")
}

/// One joint as the mobility count sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JointCount {
    /// The joint element.
    pub id: String,
    /// Its pair kind.
    pub kind: String,
    /// Bodies it joins.
    pub bodies: usize,
    /// Its degrees of freedom.
    pub dof: u32,
}

/// The Kutzbach count of a kinematic diagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mobility {
    /// Bodies, the frame included.
    pub n: usize,
    /// Full joints (one degree of freedom), a joint of k bodies counting k - 1 times.
    pub j1: u32,
    /// Half joints (two degrees of freedom).
    pub j2: u32,
    /// The mobility.
    pub m: i64,
    /// Whether a frame (ground) is drawn.
    pub grounded: bool,
    /// Every joint.
    pub joints: Vec<JointCount>,
    /// The bodies: for each, the link relationships and body elements it is made of.
    pub bodies: Vec<Vec<String>>,
    /// The formula with the numbers in it.
    pub formula: String,
}

/// Union-find over body handles, iterative.
struct Bodies {
    parent: Vec<usize>,
}

impl Bodies {
    fn find(&mut self, mut a: usize) -> usize {
        let mut root = a;
        while let Some(&p) = self.parent.get(root) {
            if p == root {
                break;
            }
            root = p;
        }
        while let Some(&p) = self.parent.get(a) {
            if p == root {
                break;
            }
            if let Some(slot) = self.parent.get_mut(a) {
                *slot = root;
            }
            a = p;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb
            && let Some(slot) = self.parent.get_mut(rb)
        {
            *slot = ra;
        }
    }
}

/// The Kutzbach mobility of the kinematic elements in scope. Bodies are the frame (every
/// `Ground`, as one body), each `Plate`, `Cam` and `Follower`, and each `Link` that touches none of
/// those; a link touching one is part of that body. A joint touching k bodies counts k - 1
/// times. Planar: `3(n - 1) - sum (3 - f)`; spatial: `6(n - 1) - sum (6 - f)`, each over the
/// joints' degrees of freedom f.
pub fn mobility(model: &Model, ty: &dyn Types, scope: &Scope, spatial: bool) -> Mobility {
    let bodies_el = of(model, ty, scope, &[GROUND, PLATE, CAM, FOLLOWER]);
    let joints = of(model, ty, scope, &[JOINT]);
    let links = rels(model, ty, scope, LINK);
    // Handles: 0 is the frame, then body elements, then links.
    let mut handle: HashMap<&str, usize> = HashMap::new();
    let grounded = bodies_el.iter().any(|e| ty.is_a(&e.type_id, GROUND));
    let mut next = 1usize;
    for e in &bodies_el {
        if ty.is_a(&e.type_id, GROUND) {
            handle.insert(e.id.as_str(), 0);
        } else {
            handle.insert(e.id.as_str(), next);
            next += 1;
        }
    }
    let first_link = next;
    let mut uf = Bodies { parent: (0..first_link + links.len()).collect() };
    let joint_ids: HashSet<&str> = joints.iter().map(|j| j.id.as_str()).collect();
    // Per joint: the handles of the bodies it touches.
    let mut touching: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in links.iter().enumerate() {
        let h = first_link + i;
        for end in [r.source.as_str(), r.target.as_str()] {
            if let Some(&b) = handle.get(end) {
                uf.union(b, h);
            }
        }
        for end in [r.source.as_str(), r.target.as_str()] {
            if joint_ids.contains(end) {
                touching.entry(end).or_default().push(h);
            }
        }
    }
    // Members of each body, in handle order.
    let mut members: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    if grounded {
        members.entry(uf.find(0)).or_default().extend(bodies_el.iter().filter(|e| ty.is_a(&e.type_id, GROUND)).map(|e| e.id.clone()));
    }
    for e in bodies_el.iter().filter(|e| !ty.is_a(&e.type_id, GROUND)) {
        if let Some(&h) = handle.get(e.id.as_str()) {
            let root = uf.find(h);
            members.entry(root).or_default().push(e.id.clone());
        }
    }
    for (i, r) in links.iter().enumerate() {
        let root = uf.find(first_link + i);
        members.entry(root).or_default().push(r.id.clone());
    }
    let n = members.len();
    let mut out_joints = Vec::new();
    let (mut j1, mut j2, mut constraint) = (0u32, 0u32, 0i64);
    let per = if spatial { 6i64 } else { 3 };
    for j in &joints {
        let kind = text(&j.properties, "kind");
        let dof = joint_dof(kind);
        let mut roots: Vec<usize> = touching.get(j.id.as_str()).map(|hs| hs.iter().map(|h| uf.find(*h)).collect()).unwrap_or_default();
        roots.sort_unstable();
        roots.dedup();
        let k = roots.len();
        let times = u32::try_from(k.saturating_sub(1)).unwrap_or(u32::MAX);
        match dof {
            1 => j1 = j1.saturating_add(times),
            2 => j2 = j2.saturating_add(times),
            _ => {}
        }
        constraint = constraint.saturating_add(i64::from(times).saturating_mul((per - i64::from(dof)).max(0)));
        out_joints.push(JointCount { id: j.id.clone(), kind: if kind.is_empty() { "revolute".into() } else { kind.to_string() }, bodies: k, dof });
    }
    let n_i = i64::try_from(n).unwrap_or(i64::MAX);
    let m = per.saturating_mul(n_i.saturating_sub(1)).saturating_sub(constraint);
    let others = out_joints.iter().any(|j| j.dof > 2 && j.bodies > 1);
    let formula = if spatial {
        format!("M = 6(n - 1) - sum(6 - f) = 6({n} - 1) - {constraint} = {m}")
    } else if others {
        format!("M = 3(n - 1) - sum(3 - f) = 3({n} - 1) - {constraint} = {m}")
    } else {
        format!("M = 3(n - 1) - 2J1 - J2 = 3({n} - 1) - 2·{j1} - {j2} = {m}")
    };
    Mobility { n, j1, j2, m, grounded, joints: out_joints, bodies: members.into_values().collect(), formula }
}

/// `mobility`: the Kutzbach count of a kinematic diagram.
pub struct MobilityAnalysis;

impl Analysis for MobilityAnalysis {
    fn id(&self) -> &'static str {
        "mobility"
    }
    fn label(&self) -> &'static str {
        "Mobility"
    }
    fn params(&self) -> &'static str {
        r#"{"space"?: "planar"|"spatial", "scope"?: [element id]} -> {"n", "j1", "j2", "m", "formula", "grounded"} plus a table of joints; planar counts 3(n - 1) - 2J1 - J2, spatial 6(n - 1) - sum(6 - f); a joint joining k bodies counts k - 1 times"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let spatial = match v.get("space") {
            None | Some(Value::Null) => false,
            Some(Value::String(t)) if t == "planar" => false,
            Some(Value::String(t)) if t == "spatial" => true,
            Some(_) => return Err(params_err("`space` must be planar or spatial")),
        };
        let scope = scope_param(v)?;
        let mob = mobility(cx.model, cx.types, &scope, spatial);
        let mut r = AnalysisResult::new(self.id());
        let names: HashMap<&str, &Element> = cx.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let label = |id: &str| names.get(id).map_or_else(|| id.to_string(), |e| name_of(e));
        r.set_elements(mob.joints.iter().map(|j| j.id.clone()).collect(), Some(Mark::Focus));
        for (i, b) in mob.bodies.iter().enumerate() {
            let (els, rs): (Vec<String>, Vec<String>) = b.iter().cloned().partition(|id| names.contains_key(id.as_str()));
            r.push_group(Group { label: format!("Body {}", i + 1), elements: els, relationships: rs });
        }
        r.table = Some(ResultTable {
            columns: vec!["Joint".into(), "Kind".into(), "Bodies".into(), "Freedom".into()],
            rows: mob
                .joints
                .iter()
                .map(|j| TableRow { id: j.id.clone(), cells: vec![json!(label(&j.id)), json!(j.kind), json!(j.bodies), json!(j.dof)] })
                .collect(),
        });
        let moves = match mob.m {
            m if m < 0 => format!("over-constrained by {}", -m),
            0 => "a structure: it cannot move".to_string(),
            1 => "one input drives it".to_string(),
            m => format!("{m} inputs are needed to drive it"),
        };
        r.summary = format!("M = {} ({moves}); {}", mob.m, mob.formula);
        if !mob.grounded {
            r.summary.push_str("; no frame (ground) is drawn");
        }
        r.detail = json!({"n": mob.n, "j1": mob.j1, "j2": mob.j2, "m": mob.m, "formula": mob.formula, "grounded": mob.grounded, "space": if spatial { "spatial" } else { "planar" }});
        Ok(r)
    }
}

// ---------------------------------------------------------------- ratio

/// A speed ratio: an exact fraction while every number on the path is whole (or has at most
/// three decimals), and the value as a float always.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ratio {
    /// Numerator and denominator, reduced, the sign on the numerator; `None` once inexact.
    pub exact: Option<(i128, i128)>,
    /// The value.
    pub value: f64,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.checked_abs().unwrap_or(1);
    b = b.checked_abs().unwrap_or(1);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Ratio {
    /// One to one.
    pub const ONE: Ratio = Ratio { exact: Some((1, 1)), value: 1.0 };

    /// `num / den` from two property values; exact when each has at most three decimals.
    pub fn of(num: f64, den: f64) -> Option<Ratio> {
        if !num.is_finite() || !den.is_finite() || den == 0.0 || num == 0.0 {
            return None;
        }
        let scaled = |x: f64| {
            let s = (x * 1000.0).round();
            ((s - x * 1000.0).abs() < 1e-6 && s.abs() < 1e15).then_some(s as i128)
        };
        let exact = match (scaled(num), scaled(den)) {
            (Some(a), Some(b)) if b != 0 => {
                let g = gcd(a, b);
                let (a, b) = (a / g, b / g);
                Some(if b < 0 { (-a, -b) } else { (a, b) })
            }
            _ => None,
        };
        Some(Ratio { exact, value: num / den })
    }

    /// The product of two ratios.
    pub fn times(self, o: Ratio) -> Ratio {
        let exact = match (self.exact, o.exact) {
            (Some((a, b)), Some((c, d))) => {
                let (g1, g2) = (gcd(a, d), gcd(c, b));
                match ((a / g1).checked_mul(c / g2), (b / g2).checked_mul(d / g1)) {
                    (Some(n), Some(m)) if m != 0 => Some((n, m)),
                    _ => None,
                }
            }
            _ => None,
        };
        Ratio { exact, value: self.value * o.value }
    }

    /// The reciprocal.
    pub fn inverse(self) -> Ratio {
        let exact = self.exact.filter(|(n, _)| *n != 0).map(|(n, d)| if n < 0 { (-d, -n) } else { (d, n) });
        Ratio { exact, value: if self.value == 0.0 { 0.0 } else { 1.0 / self.value } }
    }

    /// Whether two ratios are the same speed.
    pub fn same(self, o: Ratio) -> bool {
        match (self.exact, o.exact) {
            (Some(a), Some(b)) => a == b,
            _ => (self.value - o.value).abs() <= 1e-9 * self.value.abs().max(o.value.abs()).max(1.0),
        }
    }

    /// `a:b` without the sign: the exact fraction, else the value to four places.
    pub fn text(self) -> String {
        match self.exact {
            Some((n, d)) => format!("{}:{}", n.unsigned_abs(), d.unsigned_abs()),
            None => format!("{}:1", fmt((self.value.abs() * 10_000.0).round() / 10_000.0)),
        }
    }

    /// 1 when both turn the same way, -1 when they turn opposite ways.
    pub fn sign(self) -> i8 {
        if self.value < 0.0 { -1 } else { 1 }
    }
}

/// A drive step whose ratio disagrees with one already found by another way round a loop.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// The relationship whose ratio disagrees.
    pub relationship: String,
    /// The element whose speed it would change.
    pub element: String,
    /// The speed found first, as a ratio of the start's.
    pub found: Ratio,
    /// The speed this step gives.
    pub other: Ratio,
}

/// The speed of `b` per unit speed of `a` across one drive relationship `r`, or `None` when it
/// does not turn `b` or lacks the numbers.
fn step(ty: &dyn Types, r: &Relationship, a: &Element, b: &Element) -> Option<Ratio> {
    let teeth = |e: &Element| number(&e.properties, "teeth");
    let dia = |e: &Element| number(&e.properties, "diameter");
    fn kind(e: &Element) -> &str {
        text(&e.properties, "kind")
    }
    if ty.is_a(&r.type_id, SHAFT) {
        return Some(Ratio::ONE);
    }
    if ty.is_a(&r.type_id, MESH) {
        if kind(a) == "rack" || kind(b) == "rack" {
            return None;
        }
        if kind(a) == "worm" {
            let s = number(&a.properties, "starts").unwrap_or(1.0);
            return Ratio::of(s, teeth(b)?);
        }
        if kind(b) == "worm" {
            let s = number(&b.properties, "starts").unwrap_or(1.0);
            return Ratio::of(teeth(a)?, s);
        }
        let wheels = ty.is_a(&a.type_id, FRICTION_WHEEL) || ty.is_a(&b.type_id, FRICTION_WHEEL);
        let (x, y) = if wheels { (dia(a)?, dia(b)?) } else { (teeth(a)?, teeth(b)?) };
        let internal = kind(a) == "internal" || kind(b) == "internal";
        return Ratio::of(if internal { x } else { -x }, y);
    }
    if ty.is_a(&r.type_id, BELT) {
        let crossed = flag(&r.properties, "crossed").unwrap_or(false);
        let x = dia(a)?;
        return Ratio::of(if crossed { -x } else { x }, dia(b)?);
    }
    if ty.is_a(&r.type_id, CHAIN) {
        return Ratio::of(teeth(a)?, teeth(b)?);
    }
    None
}

/// The drive graph: per element, `(neighbour, relationship, speed of the neighbour per unit speed
/// of the element)`. A disengaged clutch passes nothing on.
fn drive<'m>(model: &'m Model, ty: &dyn Types, scope: &Scope) -> (HashMap<&'m str, &'m Element>, HashMap<&'m str, Vec<(&'m str, &'m str, Ratio)>>) {
    let els: HashMap<&str, &Element> = model.elements.iter().filter(|e| in_scope(scope, &e.id)).map(|e| (e.id.as_str(), e)).collect();
    let mut adj: HashMap<&str, Vec<(&str, &str, Ratio)>> = HashMap::new();
    for r in &model.relationships {
        let (Some(a), Some(b)) = (els.get(r.source.as_str()), els.get(r.target.as_str())) else { continue };
        let Some(f) = step(ty, r, a, b) else { continue };
        adj.entry(a.id.as_str()).or_default().push((b.id.as_str(), r.id.as_str(), f));
        adj.entry(b.id.as_str()).or_default().push((a.id.as_str(), r.id.as_str(), f.inverse()));
    }
    (els, adj)
}

fn passes(ty: &dyn Types, e: &Element) -> bool {
    !(ty.is_a(&e.type_id, CLUTCH) && flag(&e.properties, "engaged") == Some(false))
}

/// What [`ratio`] found.
#[derive(Clone, Debug, PartialEq)]
pub struct RatioResult {
    /// Speed of `to` per unit speed of `from`.
    pub ratio: Ratio,
    /// Elements from `from` to `to`.
    pub path: Vec<String>,
    /// Relationships along that path.
    pub path_relationships: Vec<String>,
    /// Steps around loops that disagree.
    pub conflicts: Vec<Conflict>,
}

/// Speeds of every element reached from `start`, breadth first, with the parent step of each and
/// the conflicts found on the way.
#[allow(clippy::type_complexity)]
fn speeds<'m>(
    ty: &dyn Types,
    els: &HashMap<&'m str, &'m Element>,
    adj: &HashMap<&'m str, Vec<(&'m str, &'m str, Ratio)>>,
    start: &'m str,
) -> (HashMap<&'m str, (Ratio, Option<(&'m str, &'m str)>)>, Vec<Conflict>) {
    let mut seen: HashMap<&str, (Ratio, Option<(&str, &str)>)> = HashMap::new();
    let mut conflicts = Vec::new();
    let mut flagged: HashSet<&str> = HashSet::new();
    seen.insert(start, (Ratio::ONE, None));
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        let Some(&(w, came)) = seen.get(at) else { continue };
        // A disengaged clutch is reached but passes nothing on (unless it is where the walk began).
        if at != start && els.get(at).is_some_and(|e| !passes(ty, e)) {
            continue;
        }
        for &(nb, rid, f) in adj.get(at).map_or(&[][..], Vec::as_slice) {
            let speed = w.times(f);
            match seen.get(nb) {
                None => {
                    seen.insert(nb, (speed, Some((at, rid))));
                    queue.push_back(nb);
                }
                Some(&(old, parent)) => {
                    let own_step = parent.is_some_and(|(p, r)| r == rid && p == at) || came.is_some_and(|(p, r)| r == rid && p == nb);
                    if !own_step && !old.same(speed) && flagged.insert(rid) {
                        conflicts.push(Conflict { relationship: rid.to_string(), element: nb.to_string(), found: old, other: speed });
                    }
                }
            }
        }
    }
    (seen, conflicts)
}

/// The speed ratio from `from` to `to` along shafts, meshes, belts and chains. External meshes
/// turn the other way, internal ones the same way; a worm turns its wheel by starts over teeth; a
/// crossed belt turns the other way; a disengaged clutch cuts the path.
pub fn ratio(model: &Model, ty: &dyn Types, scope: &Scope, from: &str, to: &str) -> Result<RatioResult, AnalysisError> {
    let (els, adj) = drive(model, ty, scope);
    let from_k = *els.keys().find(|k| **k == from).ok_or_else(|| AnalysisError::Unknown(from.chars().take(64).collect()))?;
    let to_k = *els.keys().find(|k| **k == to).ok_or_else(|| AnalysisError::Unknown(to.chars().take(64).collect()))?;
    let (seen, conflicts) = speeds(ty, &els, &adj, from_k);
    let Some(&(ratio, _)) = seen.get(to_k) else {
        let name = |k: &str| els.get(k).map_or_else(|| k.to_string(), |e| name_of(e));
        return Err(params_err(format!(
            "no drive joins {} to {} (a disengaged clutch, a rack or a missing tooth count cuts it)",
            name(from_k),
            name(to_k)
        )));
    };
    let mut path = vec![to_k.to_string()];
    let mut path_rels = Vec::new();
    let mut at = to_k;
    // The parent chain is a tree, so it ends at the start; the bound is the element count.
    for _ in 0..els.len() {
        let Some(&(_, Some((p, r)))) = seen.get(at) else { break };
        path.push(p.to_string());
        path_rels.push(r.to_string());
        at = p;
    }
    path.reverse();
    path_rels.reverse();
    Ok(RatioResult { ratio, path, path_relationships: path_rels, conflicts })
}

/// Every conflicting step in the drives in scope, each loop reported once.
pub fn ratio_conflicts(model: &Model, ty: &dyn Types, scope: &Scope) -> Vec<Conflict> {
    let (els, adj) = drive(model, ty, scope);
    let mut done: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for e in &model.elements {
        let Some(&k) = els.keys().find(|k| **k == e.id.as_str()) else { continue };
        if done.contains(k) || !adj.contains_key(k) {
            continue;
        }
        let (seen, conflicts) = speeds(ty, &els, &adj, k);
        done.extend(seen.keys().copied());
        out.extend(conflicts);
    }
    out
}

/// `ratio`: the speed ratio between two transmission components.
pub struct RatioAnalysis;

impl Analysis for RatioAnalysis {
    fn id(&self) -> &'static str {
        "ratio"
    }
    fn label(&self) -> &'static str {
        "Ratio"
    }
    fn params(&self) -> &'static str {
        r#"{"from": element id, "to": element id, "speed"?: number (the speed of from), "scope"?: [element id]} -> {"ratio": "1:6" (speed of to : speed of from, exact), "value": signed number, "sign": "same"|"opposite", "path", "conflicts": [{"relationship", "element", "found", "other"}], "speed"?: the speed of to}"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let scope = scope_param(v)?;
        let from = opt_id(v, "from")?.ok_or_else(|| params_err("`from` (element id) is required"))?;
        let to = opt_id(v, "to")?.ok_or_else(|| params_err("`to` (element id) is required"))?;
        let speed = match v.get("speed") {
            None | Some(Value::Null) => None,
            Some(x) => Some(x.as_f64().filter(|n| n.is_finite() && n.abs() <= 1e12).ok_or_else(|| params_err("`speed` must be a number"))?),
        };
        let found = ratio(cx.model, cx.types, &scope, from, to)?;
        let mut r = AnalysisResult::new(self.id());
        let names: HashMap<&str, &Element> = cx.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
        let label = |id: &str| names.get(id).map_or_else(|| id.to_string(), |e| name_of(e));
        r.set_elements(found.path.clone(), Some(Mark::Path));
        r.marks.insert(from.to_string(), Mark::Focus);
        r.set_relationships(found.path_relationships.clone(), Some(Mark::Path));
        let sign = if found.ratio.sign() < 0 { "opposite" } else { "same" };
        let conflicts: Vec<Value> = found
            .conflicts
            .iter()
            .map(|c| json!({"relationship": c.relationship, "element": c.element, "found": c.found.text(), "other": c.other.text()}))
            .collect();
        for c in &found.conflicts {
            r.marks.insert(c.relationship.clone(), Mark::Cycle);
        }
        r.summary = format!("{} to {}: {} ({} direction)", label(from), label(to), found.ratio.text(), sign);
        if !found.conflicts.is_empty() {
            r.summary.push_str(&format!("; {} loop(s) give a different ratio", found.conflicts.len()));
        }
        let mut detail = json!({
            "ratio": found.ratio.text(),
            "value": found.ratio.value,
            "sign": sign,
            "path": found.path,
            "conflicts": conflicts,
        });
        if let (Some(s), Some(o)) = (speed, detail.as_object_mut()) {
            o.insert("speed".into(), json!(s * found.ratio.value));
            r.summary.push_str(&format!("; {} turns at {}", label(to), fmt(s * found.ratio.value)));
        }
        let (els, adj) = drive(cx.model, cx.types, &scope);
        let speeds_from = els.keys().find(|k| **k == from).map(|k| speeds(cx.types, &els, &adj, k).0).unwrap_or_default();
        r.table = Some(ResultTable {
            columns: vec!["Element".into(), "Speed".into()],
            rows: found
                .path
                .iter()
                .filter_map(|id| {
                    let w = speeds_from.get(id.as_str())?.0;
                    Some(TableRow { id: id.clone(), cells: vec![json!(label(id)), json!(fmt(w.value))] })
                })
                .collect(),
        });
        r.detail = detail;
        Ok(r)
    }
}

// ---------------------------------------------------------------- signal flow and Mason

/// One product term of gains: a sign and the factors multiplied, in path order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Term {
    /// 1 or -1.
    pub sign: i8,
    /// Gain factors other than 1.
    pub factors: Vec<String>,
}

impl Term {
    fn one() -> Term {
        Term { sign: 1, factors: Vec::new() }
    }
    fn times(&self, o: &Term) -> Term {
        let mut f = self.factors.clone();
        f.extend(o.factors.iter().cloned());
        Term { sign: self.sign * o.sign, factors: f }
    }
    fn negated(&self) -> Term {
        Term { sign: -self.sign, factors: self.factors.clone() }
    }
    /// The factors joined by a dot, `1` when there are none.
    pub fn body(&self) -> String {
        if self.factors.is_empty() { "1".into() } else { self.factors.join("·") }
    }
    /// The value when every factor reads as a number.
    pub fn value(&self) -> Option<f64> {
        let mut v = f64::from(self.sign);
        for f in &self.factors {
            v *= f.trim().parse::<f64>().ok().filter(|n| n.is_finite())?;
        }
        Some(v)
    }
}

/// The product of `terms`, in one pass (a long path must not copy its factors once per step).
fn product<'a>(terms: impl Iterator<Item = &'a Term>) -> Term {
    let mut out = Term::one();
    for t in terms {
        out.sign *= t.sign;
        out.factors.extend(t.factors.iter().cloned());
    }
    out
}

/// A sum of terms as text: `1 + G·H`, `-G`.
pub fn sum_text(terms: &[Term]) -> String {
    let mut out = String::new();
    for (i, t) in terms.iter().enumerate() {
        match (i, t.sign < 0) {
            (0, true) => out.push('-'),
            (0, false) => {}
            (_, true) => out.push_str(" - "),
            (_, false) => out.push_str(" + "),
        }
        out.push_str(&t.body());
    }
    if out.is_empty() { "0".into() } else { out }
}

fn sum_value(terms: &[Term]) -> Option<f64> {
    terms.iter().map(Term::value).sum()
}

/// The signal flow graph of the control blocks in scope: nodes are elements, edge gains are the
/// source block's gain with the signal's sign.
pub struct SignalGraph {
    /// Node ids, in model order.
    pub ids: Vec<String>,
    /// Per node: `(target node, relationship id, gain term)`.
    pub out: Vec<Vec<(usize, String, Term)>>,
    /// Whether each node holds state (an integrator, a delay, a dynamic transfer block), so a
    /// loop through it is not algebraic.
    pub dynamic: Vec<bool>,
}

/// The gain a block puts on the signals leaving it.
fn block_gain(ty: &dyn Types, e: &Element) -> Option<String> {
    let own = text(&e.properties, "gain");
    let default = if ty.is_a(&e.type_id, TRANSFER) || ty.is_a(&e.type_id, GAIN) {
        let n = e.name.trim();
        if n.is_empty() { "G".to_string() } else { n.chars().take(40).collect() }
    } else if ty.is_a(&e.type_id, INTEGRATOR) {
        "1/s".into()
    } else if ty.is_a(&e.type_id, DELAY) {
        "e^-sT".into()
    } else {
        return None;
    };
    Some(if own.is_empty() { default } else { own.chars().take(40).collect() })
}

/// Build the signal flow graph of the control blocks in scope.
pub fn signal_graph(model: &Model, ty: &dyn Types, scope: &Scope) -> SignalGraph {
    let nodes = of(model, ty, scope, &[TRANSFER, GAIN, INTEGRATOR, DELAY, SATURATION, SUMMING, BRANCH, INPUT, OUTPUT]);
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut out = vec![Vec::new(); nodes.len()];
    for r in rels(model, ty, scope, SIGNAL) {
        let (Some(&a), Some(&b)) = (index.get(r.source.as_str()), index.get(r.target.as_str())) else { continue };
        let Some(src) = nodes.get(a) else { continue };
        let mut t = Term::one();
        if let Some(g) = block_gain(ty, src).filter(|g| g != "1") {
            t.factors.push(g);
        }
        if text(&r.properties, "sign") == "-" {
            t.sign = -1;
        }
        if let Some(o) = out.get_mut(a) {
            o.push((b, r.id.clone(), t));
        }
    }
    let dynamic = nodes
        .iter()
        .map(|e| {
            ty.is_a(&e.type_id, INTEGRATOR)
                || ty.is_a(&e.type_id, DELAY)
                || (ty.is_a(&e.type_id, TRANSFER) && flag(&e.properties, "dynamic").unwrap_or(false))
        })
        .collect();
    SignalGraph { ids: nodes.iter().map(|e| e.id.clone()).collect(), out, dynamic }
}

/// A path or loop: nodes in order, relationships in order, and its gain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walk {
    /// Node indices.
    pub nodes: Vec<usize>,
    /// Relationship ids.
    pub relationships: Vec<String>,
    /// Product of the edge gains.
    pub gain: Term,
}

/// Simple paths from `from` to `to` (at most `max`), depth first with an explicit stack; the
/// flag says the search stopped early.
fn simple_paths(g: &SignalGraph, from: usize, to: usize, max: usize, budget: &mut usize) -> (Vec<Walk>, bool) {
    let mut out = Vec::new();
    let mut on: Vec<bool> = vec![false; g.ids.len()];
    // Each frame: node, the next edge index to try.
    let mut stack: Vec<(usize, usize)> = vec![(from, 0)];
    let mut edges: Vec<(String, Term)> = Vec::new();
    if let Some(x) = on.get_mut(from) {
        *x = true;
    }
    let mut truncated = false;
    while let Some(&mut (node, ref mut next)) = stack.last_mut() {
        if *budget == 0 || out.len() >= max {
            truncated = true;
            break;
        }
        *budget -= 1;
        let list = g.out.get(node).map_or(&[][..], Vec::as_slice);
        if let Some((t, rid, gain)) = list.get(*next) {
            *next += 1;
            if *t == to {
                let mut nodes: Vec<usize> = stack.iter().map(|(n, _)| *n).collect();
                nodes.push(*t);
                let mut rs: Vec<String> = edges.iter().map(|(r, _)| r.clone()).collect();
                rs.push(rid.clone());
                let gain = product(edges.iter().map(|(_, g)| g).chain(std::iter::once(gain)));
                out.push(Walk { nodes, relationships: rs, gain });
            } else if !on.get(*t).copied().unwrap_or(true) {
                if let Some(x) = on.get_mut(*t) {
                    *x = true;
                }
                edges.push((rid.clone(), gain.clone()));
                stack.push((*t, 0));
            }
        } else {
            if let Some(x) = on.get_mut(node) {
                *x = false;
            }
            stack.pop();
            edges.pop();
        }
    }
    (out, truncated)
}

/// Every elementary loop (at most `max`): for each start node, the cycles back to it through
/// higher-numbered nodes only, so each loop is found once, from its lowest node.
pub fn loops(g: &SignalGraph, max: usize) -> (Vec<Walk>, bool) {
    let mut out = Vec::new();
    let mut budget = STEP_BUDGET;
    let mut truncated = false;
    // Every pop clears its node's flag, so one array serves every start.
    let mut on = vec![false; g.ids.len()];
    for s in 0..g.ids.len() {
        let mut stack: Vec<(usize, usize)> = vec![(s, 0)];
        let mut edges: Vec<(String, Term)> = Vec::new();
        if let Some(x) = on.get_mut(s) {
            *x = true;
        }
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if budget == 0 || out.len() >= max {
                truncated = true;
                break;
            }
            budget -= 1;
            let list = g.out.get(node).map_or(&[][..], Vec::as_slice);
            if let Some((t, rid, gain)) = list.get(*next) {
                *next += 1;
                if *t == s {
                    let nodes: Vec<usize> = stack.iter().map(|(n, _)| *n).collect();
                    let mut rs: Vec<String> = edges.iter().map(|(r, _)| r.clone()).collect();
                    rs.push(rid.clone());
                    let gain = product(edges.iter().map(|(_, g)| g).chain(std::iter::once(gain)));
                    out.push(Walk { nodes, relationships: rs, gain });
                } else if *t > s && !on.get(*t).copied().unwrap_or(true) {
                    if let Some(x) = on.get_mut(*t) {
                        *x = true;
                    }
                    edges.push((rid.clone(), gain.clone()));
                    stack.push((*t, 0));
                }
            } else {
                if let Some(x) = on.get_mut(node) {
                    *x = false;
                }
                stack.pop();
                edges.pop();
            }
        }
        if truncated {
            break;
        }
    }
    (out, truncated)
}

fn touches(a: &[usize], b: &[usize]) -> bool {
    a.iter().any(|x| b.contains(x))
}

/// Groups of 1 to [`MAX_ORDER`] mutually non-touching loops among `ids` (indices into `loops`),
/// by order; the flag says the list was cut.
fn groups(loops: &[Walk], ids: &[usize]) -> (Vec<Vec<Vec<usize>>>, bool) {
    let mut by_order: Vec<Vec<Vec<usize>>> = vec![Vec::new(); MAX_ORDER];
    let mut count = 0usize;
    let mut truncated = false;
    // Depth-first over increasing loop positions, explicit stack of chosen positions.
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0usize;
    loop {
        if count >= MAX_GROUPS {
            truncated = true;
            break;
        }
        if next < ids.len() && stack.len() < MAX_ORDER {
            let cand = ids.get(next).copied().unwrap_or(0);
            let fits = stack.iter().all(|&p| {
                let other = ids.get(p).copied().unwrap_or(0);
                match (loops.get(cand), loops.get(other)) {
                    (Some(a), Some(b)) => !touches(&a.nodes, &b.nodes),
                    _ => false,
                }
            });
            if fits {
                stack.push(next);
                if let Some(slot) = by_order.get_mut(stack.len() - 1) {
                    slot.push(stack.iter().filter_map(|p| ids.get(*p).copied()).collect());
                    count += 1;
                }
            }
            next += 1;
        } else {
            let Some(last) = stack.pop() else { break };
            next = last + 1;
        }
    }
    (by_order, truncated)
}

/// Δ over the loops `ids`: 1, minus each loop gain, plus each pair's product, and so on.
fn delta(loops: &[Walk], ids: &[usize]) -> (Vec<Term>, bool) {
    let (by_order, truncated) = groups(loops, ids);
    let mut terms = vec![Term::one()];
    for (k, list) in by_order.iter().enumerate() {
        for grp in list {
            let mut t = grp.iter().filter_map(|i| loops.get(*i)).fold(Term::one(), |a, w| a.times(&w.gain));
            if k % 2 == 0 {
                t = t.negated();
            }
            terms.push(t);
        }
    }
    (terms, truncated)
}

/// What [`mason`] found.
#[derive(Clone, Debug, PartialEq)]
pub struct MasonResult {
    /// The input and output node ids.
    pub input: String,
    /// The output.
    pub output: String,
    /// Forward paths with their gains, and each one's cofactor.
    pub paths: Vec<(Walk, Vec<Term>)>,
    /// Loops with their gains.
    pub loops: Vec<Walk>,
    /// Non-touching groups of each order (loop indices), first order first.
    pub groups: Vec<Vec<Vec<usize>>>,
    /// Δ.
    pub delta: Vec<Term>,
    /// The transfer function as text.
    pub formula: String,
    /// Its value when every gain is a number.
    pub value: Option<f64>,
    /// A search stopped at a cap.
    pub truncated: bool,
    /// Node ids, for the walks' node indices.
    pub ids: Vec<String>,
}

fn product_text(terms: &[Term]) -> String {
    if terms.len() > 1 { format!("({})", sum_text(terms)) } else { sum_text(terms) }
}

/// Mason's gain formula from `input` to `output` (the first Input and Output when not given).
pub fn mason(model: &Model, ty: &dyn Types, scope: &Scope, input: Option<&str>, output: Option<&str>) -> Result<MasonResult, AnalysisError> {
    let g = signal_graph(model, ty, scope);
    let pick = |want: Option<&str>, t: &str, k: &str| -> Result<usize, AnalysisError> {
        match want {
            Some(id) => g.ids.iter().position(|x| x == id).ok_or_else(|| AnalysisError::Unknown(id.chars().take(64).collect())),
            None => model
                .elements
                .iter()
                .find(|e| in_scope(scope, &e.id) && ty.is_a(&e.type_id, t))
                .and_then(|e| g.ids.iter().position(|x| *x == e.id))
                .ok_or_else(|| params_err(format!("`{k}`: the diagram has no {}", t.trim_start_matches("mechanical.")))),
        }
    };
    let (from, to) = (pick(input, INPUT, "input")?, pick(output, OUTPUT, "output")?);
    let mut budget = STEP_BUDGET;
    let (paths, t1) = simple_paths(&g, from, to, MAX_PATHS, &mut budget);
    let (lps, t2) = loops(&g, MAX_LOOPS);
    let all: Vec<usize> = (0..lps.len()).collect();
    let (d, t3) = delta(&lps, &all);
    let (grps, _) = groups(&lps, &all);
    let mut numerator: Vec<Term> = Vec::new();
    let mut with_cofactors = Vec::new();
    let mut t4 = false;
    for p in paths {
        let free: Vec<usize> = all.iter().copied().filter(|i| lps.get(*i).is_some_and(|l| !touches(&l.nodes, &p.nodes))).collect();
        let (dk, t) = delta(&lps, &free);
        t4 |= t;
        for term in &dk {
            numerator.push(p.gain.times(term));
        }
        with_cofactors.push((p, dk));
    }
    let formula = if numerator.is_empty() {
        "0".to_string()
    } else if d.len() == 1 && d.first().is_some_and(|t| t.factors.is_empty() && t.sign > 0) {
        sum_text(&numerator)
    } else {
        format!("{} / {}", product_text(&numerator), product_text(&d))
    };
    let value = match (sum_value(&numerator), sum_value(&d)) {
        (Some(n), Some(den)) if den != 0.0 => Some(n / den),
        _ => None,
    };
    Ok(MasonResult {
        input: g.ids.get(from).cloned().unwrap_or_default(),
        output: g.ids.get(to).cloned().unwrap_or_default(),
        paths: with_cofactors,
        loops: lps,
        groups: grps,
        delta: d,
        formula,
        value,
        truncated: t1 || t2 || t3 || t4,
        ids: g.ids,
    })
}

/// The loops of the signal graph that hold no state: no integrator, delay or dynamic block on
/// them, so the signal answers itself in no time.
pub fn algebraic_loops(model: &Model, ty: &dyn Types, scope: &Scope) -> Vec<(Vec<String>, Vec<String>)> {
    let g = signal_graph(model, ty, scope);
    let (lps, _) = loops(&g, MAX_LOOPS);
    lps.into_iter()
        .filter(|l| !l.nodes.iter().any(|n| g.dynamic.get(*n).copied().unwrap_or(false)))
        .map(|l| (l.nodes.iter().filter_map(|n| g.ids.get(*n).cloned()).collect(), l.relationships))
        .collect()
}

/// `mason`: the transfer function of a control loop by Mason's gain formula.
pub struct MasonAnalysis;

impl Analysis for MasonAnalysis {
    fn id(&self) -> &'static str {
        "mason"
    }
    fn label(&self) -> &'static str {
        "Mason's Gain"
    }
    fn params(&self) -> &'static str {
        r#"{"input"?: element id (default: the first Input), "output"?: element id (default: the first Output), "scope"?: [element id]} -> {"formula", "value"?, "paths": [{"elements", "gain", "cofactor"}], "loops": [{"elements", "gain"}], "nonTouching": [[[loop index]]], "delta"}; at most 64 paths, 256 loops and groups of four"#
    }
    fn run(&self, cx: &Context, v: &Value) -> Result<AnalysisResult, AnalysisError> {
        let scope = scope_param(v)?;
        let m = mason(cx.model, cx.types, &scope, opt_id(v, "input")?, opt_id(v, "output")?)?;
        let ids = |w: &Walk| w.nodes.iter().filter_map(|n| m.ids.get(*n).cloned()).collect::<Vec<_>>();
        let mut r = AnalysisResult::new(self.id());
        let mut els: Vec<String> = Vec::new();
        let mut rs: Vec<String> = Vec::new();
        for (p, _) in &m.paths {
            els.extend(ids(p));
            rs.extend(p.relationships.iter().cloned());
        }
        for l in &m.loops {
            els.extend(ids(l));
            rs.extend(l.relationships.iter().cloned());
        }
        let mut seen = HashSet::new();
        els.retain(|x| seen.insert(x.clone()));
        rs.retain(|x| seen.insert(x.clone()));
        r.set_elements(els, Some(Mark::Path));
        r.set_relationships(rs, Some(Mark::Path));
        for (i, l) in m.loops.iter().enumerate() {
            r.push_group(Group { label: format!("Loop {}", i + 1), elements: ids(l), relationships: l.relationships.clone() });
            for rid in &l.relationships {
                r.marks.insert(rid.clone(), Mark::Cycle);
            }
        }
        let mut rows = Vec::new();
        for (i, (p, dk)) in m.paths.iter().enumerate() {
            rows.push(TableRow {
                id: m.ids.get(p.nodes.first().copied().unwrap_or(0)).cloned().unwrap_or_default(),
                cells: vec![json!(format!("Path {}", i + 1)), json!(sum_text(std::slice::from_ref(&p.gain))), json!(sum_text(dk))],
            });
        }
        for (i, l) in m.loops.iter().enumerate() {
            rows.push(TableRow {
                id: m.ids.get(l.nodes.first().copied().unwrap_or(0)).cloned().unwrap_or_default(),
                cells: vec![json!(format!("Loop {}", i + 1)), json!(sum_text(std::slice::from_ref(&l.gain))), json!("")],
            });
        }
        r.table = Some(ResultTable { columns: vec!["Term".into(), "Gain".into(), "Cofactor".into()], rows });
        r.truncated |= m.truncated;
        r.summary = format!(
            "T = {}{}; {} forward path(s), {} loop(s)",
            m.formula,
            m.value.map(|x| format!(" = {}", fmt(x))).unwrap_or_default(),
            m.paths.len(),
            m.loops.len()
        );
        r.detail = json!({
            "formula": m.formula,
            "value": m.value,
            "delta": sum_text(&m.delta),
            "paths": m.paths.iter().map(|(p, dk)| json!({"elements": ids(p), "gain": sum_text(std::slice::from_ref(&p.gain)), "cofactor": sum_text(dk)})).collect::<Vec<_>>(),
            "loops": m.loops.iter().map(|l| json!({"elements": ids(l), "gain": sum_text(std::slice::from_ref(&l.gain))})).collect::<Vec<_>>(),
            "nonTouching": m.groups.iter().skip(1).collect::<Vec<_>>(),
            "input": m.input,
            "output": m.output,
        });
        Ok(r)
    }
}

// ---------------------------------------------------------------- bond graph causality

/// What a bond graph element does to causality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BondRole {
    /// An effort source (Se, MSe): the stroke is away from it.
    Effort,
    /// A flow source (Sf, MSf): the stroke is at it.
    Flow,
    /// A resistor: either way.
    R,
    /// A capacitor: integral causality has the stroke away from it.
    C,
    /// An inertia: integral causality has the stroke at it.
    I,
    /// A transformer: one stroke at it, one away.
    Tf,
    /// A gyrator: both strokes at it or both away.
    Gy,
    /// A 0-junction: exactly one stroke at it.
    J0,
    /// A 1-junction: exactly one stroke away from it.
    J1,
}

/// The bond graph role of element type `t`.
pub fn bond_role(ty: &dyn Types, t: &str) -> Option<BondRole> {
    let is = |x: &str| ty.is_a(t, &format!("mechanical.{x}"));
    Some(if is("Se") || is("MSe") {
        BondRole::Effort
    } else if is("Sf") || is("MSf") {
        BondRole::Flow
    } else if is("R") {
        BondRole::R
    } else if is("C") {
        BondRole::C
    } else if is("I") {
        BondRole::I
    } else if is("TF") {
        BondRole::Tf
    } else if is("GY") {
        BondRole::Gy
    } else if is("J0") {
        BondRole::J0
    } else if is("J1") {
        BondRole::J1
    } else {
        return None;
    })
}

/// Where a bond's causal stroke is, from its `causal` property: `Some(true)` at its target end,
/// `Some(false)` at its source end, `None` when unassigned.
pub fn stroke_of(r: &Relationship) -> Option<bool> {
    match text(&r.properties, "causal") {
        "end" => Some(true),
        "start" => Some(false),
        _ => None,
    }
}

/// What [`assign_causality`] decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Causality {
    /// Every bond with the end its stroke goes to (`start` or `end`), in model order.
    pub assigned: Vec<(String, &'static str)>,
    /// Elements whose rule could not be met, each once, in the order found.
    pub conflicts: Vec<String>,
}

/// The working state of [`assign_causality`].
struct Assign<'m> {
    bonds: Vec<&'m Relationship>,
    roles: HashMap<&'m str, BondRole>,
    /// Bond indices at each element.
    at: HashMap<&'m str, Vec<usize>>,
    /// Per bond: `Some(true)` with the stroke at its target end.
    stroke: Vec<Option<bool>>,
    conflicts: Vec<String>,
    queue: Vec<&'m str>,
}

impl<'m> Assign<'m> {
    /// Whether stroke value `s` puts bond `i`'s stroke at element `x`.
    fn at_x(&self, i: usize, x: &str, s: bool) -> bool {
        self.bonds.get(i).is_some_and(|b| (b.target == x) == s)
    }

    /// The stroke value that puts bond `i`'s stroke at `x` (`at_it`) or away from it.
    fn value_for(&self, i: usize, x: &str, at_it: bool) -> bool {
        self.bonds.get(i).is_some_and(|b| (b.target == x) == at_it)
    }

    fn conflict(&mut self, who: &str) {
        if !self.conflicts.iter().any(|c| c == who) {
            self.conflicts.push(who.to_string());
        }
    }

    /// Decide bond `i`, noting a conflict at `who` when it was decided the other way.
    fn set(&mut self, i: usize, v: bool, who: &str) {
        match self.stroke.get(i).copied().flatten() {
            Some(old) if old != v => self.conflict(who),
            Some(_) => {}
            None => {
                if let Some(slot) = self.stroke.get_mut(i) {
                    *slot = Some(v);
                }
                if let Some(b) = self.bonds.get(i) {
                    self.queue.push(b.source.as_str());
                    self.queue.push(b.target.as_str());
                }
            }
        }
    }

    /// Carry decisions through junctions, transformers and gyrators until nothing changes. Every
    /// element pushed comes from a bond decided once, so the queue empties.
    fn propagate(&mut self) {
        let mut guard = self.bonds.len().saturating_mul(4).saturating_add(16);
        while let Some(x) = self.queue.pop() {
            if guard == 0 {
                break;
            }
            guard -= 1;
            let Some(role) = self.roles.get(x).copied() else { continue };
            let list = self.at.get(x).cloned().unwrap_or_default();
            let known: Vec<(usize, bool)> =
                list.iter().filter_map(|&i| self.stroke.get(i).copied().flatten().map(|s| (i, self.at_x(i, x, s)))).collect();
            let unknown: Vec<usize> = list.iter().copied().filter(|&i| self.stroke.get(i).copied().flatten().is_none()).collect();
            match role {
                BondRole::J0 | BondRole::J1 => {
                    // At a 0-junction exactly one stroke is at it; at a 1-junction exactly one is away.
                    let special = role == BondRole::J0;
                    let specials = known.iter().filter(|(_, a)| *a == special).count();
                    if specials > 1 {
                        self.conflict(x);
                    } else if specials == 1 {
                        for i in unknown {
                            let v = self.value_for(i, x, !special);
                            self.set(i, v, x);
                        }
                    } else if unknown.len() == 1 && !known.is_empty() {
                        if let Some(&i) = unknown.first() {
                            let v = self.value_for(i, x, special);
                            self.set(i, v, x);
                        }
                    } else if unknown.is_empty() && !known.is_empty() {
                        self.conflict(x);
                    }
                }
                BondRole::Tf | BondRole::Gy => {
                    if list.len() == 2
                        && let (Some(&(_, a)), Some(&i)) = (known.first(), unknown.first())
                    {
                        let want = if role == BondRole::Tf { !a } else { a };
                        let v = self.value_for(i, x, want);
                        self.set(i, v, x);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The sequential causality assignment procedure: sources first, then integral causality on C
/// and I, then R, then any bond left, each choice carried through the junctions, transformers and
/// gyrators before the next. Reads no existing strokes: it decides every bond afresh.
pub fn assign_causality(model: &Model, ty: &dyn Types, scope: &Scope) -> Causality {
    let bonds = rels(model, ty, scope, BOND);
    let roles: HashMap<&str, BondRole> =
        model.elements.iter().filter(|e| in_scope(scope, &e.id)).filter_map(|e| bond_role(ty, &e.type_id).map(|r| (e.id.as_str(), r))).collect();
    let mut at: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, b) in bonds.iter().enumerate() {
        at.entry(b.source.as_str()).or_default().push(i);
        if b.target != b.source {
            at.entry(b.target.as_str()).or_default().push(i);
        }
    }
    let order: Vec<&str> = model.elements.iter().filter(|e| roles.contains_key(e.id.as_str())).map(|e| e.id.as_str()).collect();
    let n = bonds.len();
    let mut a = Assign { bonds, roles, at, stroke: vec![None; n], conflicts: Vec::new(), queue: Vec::new() };
    // 1: sources.
    for &x in &order {
        let want = match a.roles.get(x) {
            Some(BondRole::Effort) => false,
            Some(BondRole::Flow) => true,
            _ => continue,
        };
        for i in a.at.get(x).cloned().unwrap_or_default() {
            let v = a.value_for(i, x, want);
            a.set(i, v, x);
        }
        a.propagate();
    }
    // 2: integral causality on storage, 3: resistors, 4: anything left.
    for pass in 0..3 {
        for &x in &order {
            let want = match (pass, a.roles.get(x)) {
                (0, Some(BondRole::C)) => false,
                (0, Some(BondRole::I)) => true,
                (1, Some(BondRole::R)) => true,
                (2, _) => true,
                _ => continue,
            };
            for i in a.at.get(x).cloned().unwrap_or_default() {
                if a.stroke.get(i).copied().flatten().is_none() {
                    let v = a.value_for(i, x, want);
                    a.set(i, v, x);
                    a.propagate();
                }
            }
        }
    }
    let assigned = a.bonds.iter().zip(&a.stroke).map(|(b, s)| (b.id.clone(), if s.unwrap_or(true) { "end" } else { "start" })).collect();
    Causality { assigned, conflicts: a.conflicts }
}

#[cfg(test)]
#[path = "mech_tests.rs"]
mod tests;
