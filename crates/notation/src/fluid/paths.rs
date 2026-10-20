//! The directional valve path grammar, port sets and defaults: what the `valve` glyph builder
//! draws and what the circuit model joins, in one place.
//!
//! A valve's `paths` text names the flow in each switching position, positions separated by `;`,
//! left to right: `P-A` is a flow from P to A, `P|` blocks P and `X` blocks every port. Ports are
//! named by ISO 9461 letters (P, A, B, T; R and S for pneumatic exhausts) or ISO 5599 numbers
//! (1 supply, 2 and 4 outputs, 3 and 5 exhausts); empty text is the usual paths for the port count
//! and positions ([`default_paths`]).
//!
//! Pilot ports sit at the operator sides: `X` (left) and `Y` (right) with ISO 9461 letters, or
//! `1n` with ISO 5599 numbers, where `n` is the output joined to the supply in that side's square
//! (`12`, `14`; `10` when the square joins the supply to nothing). A valve's ports come fluid ports
//! first, then pilots, then electrical (`el`, `er`) and mechanical (`ml`, `mr`) ports, so adding
//! an operator never shifts a glued fluid port.

/// The default number of switching positions.
pub const DEFAULT_POSITIONS: i64 = 2;
/// The default number of ports.
pub const DEFAULT_PORTS: i64 = 4;
/// The default operator at the left side.
pub const DEFAULT_LEFT: &str = "solenoid";
/// The default operator at the right side.
pub const DEFAULT_RIGHT: &str = "none";
/// The default return.
pub const DEFAULT_RETURN: &str = "spring";
/// The default rest position (from the left).
pub const DEFAULT_REST: i64 = 1;
/// The default port labels.
pub const DEFAULT_LABELS: &str = "iso9461";
/// The default medium.
pub const DEFAULT_MEDIUM: &str = "hydraulic";

/// The operators a valve side may have.
pub const ACTUATORS: &[&str] = &["none", "lever", "button", "pedal", "roller", "plunger", "solenoid", "pilot"];

/// Most tokens read from one position's group.
pub const MAX_TOKENS: usize = 16;

/// Port ids that carry no fluid: electrical and mechanical connections, shafts and rods. An open
/// one is not an open fluid port.
pub const NON_FLUID: &[&str] = &["el", "er", "ml", "mr", "shaft", "rod"];

/// A port of a directional valve: its letter, its number, which side of the square and how far
/// across, and whether it is an exhaust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValvePort {
    /// ISO 9461 letter.
    pub letter: &'static str,
    /// ISO 5599 number.
    pub number: &'static str,
    /// On the top edge (outputs) rather than the bottom (supply and exhausts).
    pub top: bool,
    /// Modules across the square from its left edge.
    pub off: f64,
    /// An exhaust (T, R, S; 3, 5).
    pub exhaust: bool,
}

impl ValvePort {
    /// The port's id under the port labels `numbers` (ISO 5599) or letters (ISO 9461).
    pub fn id(&self, numbers: bool) -> &'static str {
        if numbers { self.number } else { self.letter }
    }
}

/// The ports of a valve with `n` ports (2 to 5; anything else draws four), pneumatic exhausts
/// lettered R and S.
pub fn port_set(n: i64, pneumatic: bool) -> Vec<ValvePort> {
    let vp = |letter, number, top, off, exhaust| ValvePort { letter, number, top, off, exhaust };
    let t = if pneumatic { "R" } else { "T" };
    match n {
        2 => vec![vp("P", "1", false, 2.0, false), vp("A", "2", true, 2.0, false)],
        3 => vec![vp("P", "1", false, 1.0, false), vp("A", "2", true, 2.0, false), vp(t, "3", false, 3.0, true)],
        5 => vec![
            vp("P", "1", false, 2.0, false),
            vp("A", "2", true, 1.0, false),
            vp("B", "4", true, 3.0, false),
            vp("R", "3", false, 1.0, true),
            vp("S", "5", false, 3.0, true),
        ],
        _ => vec![vp("P", "1", false, 1.0, false), vp("A", "2", true, 1.0, false), vp("B", "4", true, 3.0, false), vp(t, "3", false, 3.0, true)],
    }
}

/// The usual paths for a valve with `ports` ports and `positions` positions.
pub fn default_paths(ports: i64, positions: i64) -> &'static str {
    match (ports, positions) {
        (2, 3) => "P-A; P| A|; A-P",
        (2, _) => "P-A; P| A|",
        (3, 3) => "P-A T|; X; A-T P|",
        (3, _) => "P-A T|; A-T P|",
        (5, 3) => "P-A B-S; X; P-B A-R",
        (5, _) => "P-A B-S; P-B A-R",
        (_, 3) => "P-A B-T; X; P-B A-T",
        _ => "P-A B-T; P-B A-T",
    }
}

/// The port a token names: by letter or number, with T and R the same exhaust below five ports.
pub fn find<'a>(set: &'a [ValvePort], name: &str) -> Option<&'a ValvePort> {
    let name = name.trim().to_ascii_uppercase();
    set.iter()
        .find(|p| p.letter == name || p.number == name)
        .or_else(|| if set.len() < 5 && matches!(name.as_str(), "T" | "R") { set.iter().find(|p| p.exhaust) } else { None })
}

/// One token of a position's group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// A flow between two named ports (`P-A`).
    Flow(&'a str, &'a str),
    /// A blocked port (`P|`).
    Block(&'a str),
    /// Every port blocked (`X`).
    All,
    /// Anything else.
    Bad(&'a str),
}

/// The tokens of one position's group, at most [`MAX_TOKENS`].
pub fn tokens(group: &str) -> Vec<Token<'_>> {
    group
        .split_whitespace()
        .take(MAX_TOKENS)
        .map(|t| {
            if t.eq_ignore_ascii_case("x") {
                Token::All
            } else if let Some(name) = t.strip_suffix('|') {
                Token::Block(name)
            } else if let Some((a, b)) = t.split_once('-') {
                Token::Flow(a, b)
            } else {
                Token::Bad(t)
            }
        })
        .collect()
}

/// The text the valve draws: `paths`, or the usual paths when it is blank.
pub fn effective(paths: &str, ports: i64, positions: i64) -> &str {
    if paths.trim().is_empty() { default_paths(ports, positions) } else { paths }
}

/// The groups of `text`, one per position (there may be more or fewer than the positions).
pub fn groups(text: &str) -> Vec<&str> {
    text.split(';').collect()
}

/// The flows in each of the first `positions` positions of `text`, as index pairs into `set`;
/// tokens naming no port are skipped.
pub fn flows(set: &[ValvePort], text: &str, positions: usize) -> Vec<Vec<(usize, usize)>> {
    let mut out = vec![Vec::new(); positions];
    for (i, g) in groups(text).into_iter().take(positions).enumerate() {
        for t in tokens(g) {
            if let Token::Flow(a, b) = t {
                let ia = set.iter().position(|p| find(set, a) == Some(p));
                let ib = set.iter().position(|p| find(set, b) == Some(p));
                if let (Some(ia), Some(ib), Some(slot)) = (ia, ib, out.get_mut(i))
                    && ia != ib
                {
                    slot.push((ia, ib));
                }
            }
        }
    }
    out
}

/// The pilot port id at one side: `X` or `Y` with letters; with numbers `1n`, `n` the output the
/// supply joins in that side's square, `10` when it joins none.
pub fn pilot_id(set: &[ValvePort], square: &[(usize, usize)], numbers: bool, left: bool) -> String {
    if !numbers {
        return if left { "X" } else { "Y" }.to_string();
    }
    let supply = set.iter().position(|p| p.number == "1");
    let out = square.iter().find_map(|&(a, b)| match supply {
        Some(s) if a == s => set.get(b),
        Some(s) if b == s => set.get(a),
        _ => None,
    });
    match out.filter(|p| !p.exhaust) {
        Some(p) => format!("1{}", p.number),
        None => "10".to_string(),
    }
}

/// Whether a port id is a pilot port: `X`, `Y`, or an ISO 5599 pilot number (`10`, `12`, `14`).
pub fn is_pilot(id: &str) -> bool {
    matches!(id, "X" | "Y") || (id.len() == 2 && id.starts_with('1') && id.chars().all(|c| c.is_ascii_digit()))
}

/// Whether a port id is a drain port (`L`).
pub fn is_drain(id: &str) -> bool {
    id == "L"
}

/// Whether a port id carries fluid (see [`NON_FLUID`]).
pub fn is_fluid(id: &str) -> bool {
    !NON_FLUID.contains(&id)
}

/// The kind of port an operator gives its side: a pilot, an electrical or a mechanical port, or
/// none.
pub fn operator_port(kind: &str) -> Option<&'static str> {
    match kind {
        "pilot" => Some("pilot"),
        "solenoid" => Some("el"),
        "roller" | "plunger" => Some("ml"),
        _ => None,
    }
}

/// The square a return acts on: the right one (`true`) when the right side has no operator, the
/// left when only the left is free, the right when both have operators.
pub fn return_on_right(left: &str, right: &str) -> bool {
    !(right != "none" && left == "none")
}

/// The square a valve should rest in, when it has a return: the middle of three, else the square
/// on the return's side. `None` with no return (a detent or nothing keeps the last position).
pub fn expected_rest(positions: i64, ret: &str, left: &str, right: &str) -> Option<i64> {
    if ret != "spring" {
        return None;
    }
    if positions >= 3 {
        return Some(1);
    }
    Some(if return_on_right(left, right) { positions.saturating_sub(1).max(0) } else { 0 })
}
