//! Parametric glyphs: symbols drawn in code from parameters.
//!
//! A pack glyph with outline `builder` names one of the builders here and its parameter
//! defaults. The builder draws the symbol from the element's properties and the page's standard
//! on the symbol grid: every length is in modules (2.5 mm for the ISO and IEC standards, 0.1 inch
//! for the American ones, or the page's own module), the origin is the top left, and every port
//! sits on whole module coordinates, so connectors drawn on the module grid meet the pins.
//!
//! - [`specs`] lists the builders with their parameters ([`params::ParamSpec`]), the standards
//!   they draw and where each drawing came from.
//! - [`build`] runs one: it never fails on a bad value (each is clamped or defaulted with a
//!   warning, see [`params::Params::resolve`]); only an unknown builder is an error. Output is
//!   capped at [`MAX_ELS`] path commands, truncated with a warning.
//! - [`apply`] puts a [`Built`] on a shape and resolves the parameter layers; [`sync`] redraws
//!   shapes whose inputs changed, after every command.
//!
//! Every symbol is drawn from the written rules of the public notation (proportions, which
//! strokes make up a symbol, where the terminals are), never from another tool's rendering or a
//! figure of the standard. Each builder module states its provenance in its first doc lines and
//! its spec's `sources` repeats it.
#![deny(clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use vizcraft_diagram::geom::{Path, PathEl, Point};
use vizcraft_diagram::vizcraft_model::PropValue;

pub mod apply;
pub mod params;
pub mod sync;

mod bubble;
mod coil;
mod contact;
mod cylinder;
mod fcf;
mod gate;
mod process_valve;
mod pump;
mod texture;
mod valve;
mod weld;

pub use bubble::split_tag;
pub use params::{Def, ParamKind, ParamSpec, Params};

/// Most path commands one build emits (fills and lines together).
pub const MAX_ELS: usize = 2048;

/// Page units per inch.
pub const UNITS_PER_INCH: f64 = 100.0;

/// The ISO and IEC module, 2.5 mm, in page units.
pub const MODULE_METRIC: f64 = 2.5 / 25.4 * UNITS_PER_INCH;

/// The American module, 0.1 inch, in page units.
pub const MODULE_INCH: f64 = 0.1 * UNITS_PER_INCH;

/// Smallest and largest module accepted, in page units.
pub const MODULE_RANGE: (f64, f64) = (0.5, 1000.0);

/// The default module of a standard, in page units: 0.1 inch for the American standards, 2.5 mm
/// for the rest.
pub fn default_module(standard: &str) -> f64 {
    match standard {
        "jic" | "ieee315" | "ieee91" | "ansi" | "aws" | "asmeY14_5" | "asmeY14_36" | "isa5_1" => MODULE_INCH,
        _ => MODULE_METRIC,
    }
}

/// A builder: its name, parameters and drawing function.
#[derive(Clone, Copy)]
pub struct BuilderSpec {
    /// Name a glyph's `builder` field uses (`contact`).
    pub name: &'static str,
    /// Name shown in the Inspector.
    pub label: &'static str,
    /// One paragraph for agents.
    pub description: &'static str,
    /// The parameters, in Inspector order.
    pub params: &'static [ParamSpec],
    /// `(standard, provenance)` for each standard drawn: where the drawing came from.
    pub sources: &'static [(&'static str, &'static str)],
    /// Draw the symbol from resolved parameters.
    pub build: fn(&Params, &mut Draw),
}

impl std::fmt::Debug for BuilderSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuilderSpec").field("name", &self.name).finish_non_exhaustive()
    }
}

impl BuilderSpec {
    /// The parameter with this key.
    pub fn param(&self, key: &str) -> Option<&'static ParamSpec> {
        self.params.iter().find(|p| p.key == key)
    }
}

static SPECS: [BuilderSpec; 11] = [
    contact::SPEC,
    coil::SPEC,
    bubble::SPEC,
    valve::SPEC,
    cylinder::SPEC,
    pump::SPEC,
    gate::SPEC,
    fcf::SPEC,
    weld::SPEC,
    texture::SPEC,
    process_valve::SPEC,
];

/// Every builder, in palette order.
pub fn specs() -> &'static [BuilderSpec] {
    &SPECS
}

/// The builder with this name.
pub fn spec(name: &str) -> Option<&'static BuilderSpec> {
    SPECS.iter().find(|s| s.name == name)
}

/// Why a build did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// No builder has this name.
    Unknown(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Unknown(n) => write!(f, "no glyph builder is named `{n}`"),
        }
    }
}

impl std::error::Error for BuildError {}

/// A port a builder placed, in module units.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltPort {
    /// Port id (`13`, `A1`, `P`).
    pub id: String,
    /// Label shown beside the port mark.
    pub label: String,
    /// Across, in modules from the left (a whole number).
    pub x: f64,
    /// Down, in modules from the top (a whole number).
    pub y: f64,
    /// Which way a wire leaves it: `left`, `right`, `up` or `down`.
    pub dir: &'static str,
}

/// A text slot a builder placed, in module units.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltLabel {
    /// Slot id (`tag`, `letters`).
    pub id: String,
    /// The text.
    pub text: String,
    /// Anchor across, in modules.
    pub x: f64,
    /// Middle of the text height down, in modules.
    pub y: f64,
    /// Text height in modules.
    pub size: f64,
    /// `start`, `middle` or `end`.
    pub anchor: &'static str,
}

/// A drawn symbol in module units, origin top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    /// Closed subpaths, filled even-odd with the shape's fill.
    pub fills: Path,
    /// Open subpaths, stroked only.
    pub lines: Path,
    /// Width in modules.
    pub w: f64,
    /// Height in modules.
    pub h: f64,
    /// Ports, in connection point order.
    pub ports: Vec<BuiltPort>,
    /// Text slots.
    pub labels: Vec<BuiltLabel>,
    /// What was clamped, defaulted, ignored or cut.
    pub warnings: Vec<String>,
    /// The standard the symbol was drawn to (`iec60617`, `jic`).
    pub standard: &'static str,
    /// The module the symbol is placed at, in page units.
    pub module: f64,
}

impl Built {
    /// Fills then lines, in page units.
    pub fn path(&self) -> Path {
        let mut p = scaled(&self.fills, self.module);
        p.els.extend(scaled(&self.lines, self.module).els);
        p
    }
    /// Width in page units.
    pub fn width(&self) -> f64 {
        self.w * self.module
    }
    /// Height in page units.
    pub fn height(&self) -> f64 {
        self.h * self.module
    }
}

/// `p` scaled by `k` about the origin.
pub fn scaled(p: &Path, k: f64) -> Path {
    p.transformed(&vizcraft_diagram::geom::Transform::scale(k, k))
}

/// Run builder `name` with the parameters in `raw` at `module` page units (`None`: the default
/// module of the standard drawn, see [`default_module`]).
pub fn build(name: &str, raw: &BTreeMap<String, PropValue>, module: Option<f64>) -> Result<Built, BuildError> {
    let spec = spec(name).ok_or_else(|| BuildError::Unknown(name.chars().take(64).collect()))?;
    let mut d = Draw::default();
    let params = Params::resolve(spec.params, raw, &mut d.warnings);
    (spec.build)(&params, &mut d);
    let mut built = d.finish();
    let given = module.filter(|m| m.is_finite() && *m > 0.0);
    if module.is_some() && given.is_none() {
        params::warn(&mut built.warnings, "the module must be a positive number; the standard's module is used".into());
    }
    built.module = given.map(|m| m.clamp(MODULE_RANGE.0, MODULE_RANGE.1)).unwrap_or_else(|| default_module(built.standard));
    Ok(built)
}

/// The pen builders draw with: collects fills, lines, ports and slots in module units and keeps
/// the command count under [`MAX_ELS`].
#[derive(Debug)]
pub struct Draw {
    fills: Path,
    lines: Path,
    ports: Vec<BuiltPort>,
    labels: Vec<BuiltLabel>,
    warnings: Vec<String>,
    w: f64,
    h: f64,
    standard: &'static str,
    full: bool,
}

impl Default for Draw {
    fn default() -> Self {
        Draw {
            fills: Path::new(),
            lines: Path::new(),
            ports: Vec::new(),
            labels: Vec::new(),
            warnings: Vec::new(),
            w: 1.0,
            h: 1.0,
            standard: "",
            full: false,
        }
    }
}

/// Most ports and slots one build places.
const MAX_PORTS: usize = 256;
const MAX_LABELS: usize = 64;

/// The cubic handle length for a quarter circle of radius 1.
const KAPPA: f64 = 0.552_284_749_830_793_4;

fn pt(p: (f64, f64)) -> Point {
    Point::new(p.0, p.1)
}

impl Draw {
    /// Set the symbol's extent in modules (whole numbers keep the frame on the grid).
    pub fn size(&mut self, w: f64, h: f64) {
        self.w = w.max(1.0);
        self.h = h.max(1.0);
    }

    /// Set the standard drawn (it picks the default module).
    pub fn standard(&mut self, standard: &'static str) {
        self.standard = standard;
    }

    /// Note something about the parameters.
    pub fn warn(&mut self, w: impl Into<String>) {
        params::warn(&mut self.warnings, w.into());
    }

    fn room(&mut self, n: usize) -> bool {
        if self.fills.els.len() + self.lines.els.len() + n <= MAX_ELS {
            return true;
        }
        if !self.full {
            self.full = true;
            self.warn(format!("the drawing is cut at {MAX_ELS} path commands"));
        }
        false
    }

    /// An open polyline through `pts`, stroked only.
    pub fn line(&mut self, pts: &[(f64, f64)]) {
        if pts.len() < 2 || !self.room(pts.len()) {
            return;
        }
        for (i, p) in pts.iter().enumerate() {
            self.lines.els.push(if i == 0 { PathEl::MoveTo(pt(*p)) } else { PathEl::LineTo(pt(*p)) });
        }
    }

    /// A closed polygon through `pts`, filled and stroked.
    pub fn shape(&mut self, pts: &[(f64, f64)]) {
        if pts.len() < 3 || !self.room(pts.len() + 1) {
            return;
        }
        for (i, p) in pts.iter().enumerate() {
            self.fills.els.push(if i == 0 { PathEl::MoveTo(pt(*p)) } else { PathEl::LineTo(pt(*p)) });
        }
        self.fills.els.push(PathEl::Close);
    }

    /// A closed rectangle from `(x0, y0)` to `(x1, y1)`.
    pub fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.shape(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    }

    /// An open rectangle outline, stroked only.
    pub fn rect_line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.line(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]);
    }

    /// The four quarter cubics of a circle, starting at its right.
    fn circle_els(cx: f64, cy: f64, r: f64) -> [PathEl; 5] {
        let k = r * KAPPA;
        [
            PathEl::MoveTo(Point::new(cx + r, cy)),
            PathEl::CubicTo(Point::new(cx + r, cy + k), Point::new(cx + k, cy + r), Point::new(cx, cy + r)),
            PathEl::CubicTo(Point::new(cx - k, cy + r), Point::new(cx - r, cy + k), Point::new(cx - r, cy)),
            PathEl::CubicTo(Point::new(cx - r, cy - k), Point::new(cx - k, cy - r), Point::new(cx, cy - r)),
            PathEl::CubicTo(Point::new(cx + k, cy - r), Point::new(cx + r, cy - k), Point::new(cx + r, cy)),
        ]
    }

    /// A closed circle, filled and stroked.
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64) {
        if !self.room(6) {
            return;
        }
        self.fills.els.extend(Self::circle_els(cx, cy, r));
        self.fills.els.push(PathEl::Close);
    }

    /// A closed ellipse with radii `rx` across and `ry` down, filled and stroked.
    pub fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        if !self.room(6) {
            return;
        }
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        self.fills.els.extend([
            PathEl::MoveTo(Point::new(cx + rx, cy)),
            PathEl::CubicTo(Point::new(cx + rx, cy + ky), Point::new(cx + kx, cy + ry), Point::new(cx, cy + ry)),
            PathEl::CubicTo(Point::new(cx - kx, cy + ry), Point::new(cx - rx, cy + ky), Point::new(cx - rx, cy)),
            PathEl::CubicTo(Point::new(cx - rx, cy - ky), Point::new(cx - kx, cy - ry), Point::new(cx, cy - ry)),
            PathEl::CubicTo(Point::new(cx + kx, cy - ry), Point::new(cx + rx, cy - ky), Point::new(cx + rx, cy)),
            PathEl::Close,
        ]);
    }

    /// A circle stroked only (inside a filled body, so even-odd leaves no hole).
    pub fn ring(&mut self, cx: f64, cy: f64, r: f64) {
        if !self.room(5) {
            return;
        }
        self.lines.els.extend(Self::circle_els(cx, cy, r));
    }

    /// An open arc of radius `r` about `(cx, cy)` from angle `a0` to `a1` in degrees (0 points
    /// right, 90 points down), stroked only.
    pub fn arc(&mut self, cx: f64, cy: f64, r: f64, a0: f64, a1: f64) {
        let sweep = (a1 - a0).clamp(-360.0, 360.0);
        let n = (sweep.abs() / 90.0).ceil().clamp(1.0, 4.0) as usize;
        if !self.room(n + 1) {
            return;
        }
        let step = sweep / n as f64;
        let at = |a: f64| Point::new(cx + r * a.to_radians().cos(), cy + r * a.to_radians().sin());
        self.lines.els.push(PathEl::MoveTo(at(a0)));
        for i in 0..n {
            let (s, e) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
            let k = 4.0 / 3.0 * ((e - s).to_radians() / 4.0).tan() * r;
            let (p0, p3) = (at(s), at(e));
            let (sr, er) = (s.to_radians(), e.to_radians());
            let c1 = Point::new(p0.x - k * sr.sin(), p0.y + k * sr.cos());
            let c2 = Point::new(p3.x + k * er.sin(), p3.y - k * er.cos());
            self.lines.els.push(PathEl::CubicTo(c1, c2, p3));
        }
    }

    /// A dashed line from `a` to `b`: dashes of `dash` with gaps of `gap`, in modules.
    pub fn dashed(&mut self, a: (f64, f64), b: (f64, f64), dash: f64, gap: f64) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len < 1e-9 || dash <= 0.0 || gap < 0.0 {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let mut t = 0.0;
        // At most 256 dashes: a hostile length cannot run away.
        for _ in 0..256 {
            if t >= len {
                break;
            }
            let e = (t + dash).min(len);
            self.line(&[(a.0 + ux * t, a.1 + uy * t), (a.0 + ux * e, a.1 + uy * e)]);
            t = e + gap;
        }
    }

    /// An arrowhead with its tip at `tip`, pointing away from `from`: a closed triangle `len`
    /// long and `2 * half` wide. With `solid`, hatched across so it reads as filled.
    pub fn head(&mut self, from: (f64, f64), tip: (f64, f64), len: f64, half: f64, solid: bool) {
        let d = ((tip.0 - from.0).powi(2) + (tip.1 - from.1).powi(2)).sqrt();
        if d < 1e-9 {
            return;
        }
        let (ux, uy) = ((tip.0 - from.0) / d, (tip.1 - from.1) / d);
        let base = (tip.0 - ux * len, tip.1 - uy * len);
        let (nx, ny) = (-uy * half, ux * half);
        let l = (base.0 + nx, base.1 + ny);
        let r = (base.0 - nx, base.1 - ny);
        self.shape(&[tip, l, r]);
        if solid {
            for f in [0.25, 0.5, 0.75] {
                let a = (tip.0 + (l.0 - tip.0) * f, tip.1 + (l.1 - tip.1) * f);
                let b = (tip.0 + (r.0 - tip.0) * f, tip.1 + (r.1 - tip.1) * f);
                self.line(&[a, b]);
            }
            self.line(&[l, r]);
            self.line(&[tip, base]);
        }
    }

    /// A port at `(x, y)` (whole modules), with its label the same as its id.
    pub fn port(&mut self, id: &str, x: f64, y: f64, dir: &'static str) {
        if self.ports.len() < MAX_PORTS {
            self.ports.push(BuiltPort { id: id.to_string(), label: id.to_string(), x, y, dir });
        }
    }

    /// A text slot; empty text places nothing.
    pub fn label(&mut self, id: &str, text: &str, x: f64, y: f64, size: f64, anchor: &'static str) {
        if !text.trim().is_empty() && self.labels.len() < MAX_LABELS {
            self.labels.push(BuiltLabel { id: id.to_string(), text: text.trim().to_string(), x, y, size, anchor });
        }
    }

    fn finish(self) -> Built {
        Built {
            fills: self.fills,
            lines: self.lines,
            w: self.w,
            h: self.h,
            ports: self.ports,
            labels: self.labels,
            warnings: self.warnings,
            standard: self.standard,
            module: default_module(self.standard),
        }
    }
}

/// Text width estimate in modules for `chars` characters `size` modules high.
pub fn text_width(chars: usize, size: f64) -> f64 {
    chars as f64 * size * 0.6
}

/// A number for SVG and path data: at most three decimals, no trailing zeros.
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let mut s = format!("{r:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// SVG path data for `p` scaled by `k`.
pub fn path_data(p: &Path, k: f64) -> String {
    let mut out = String::new();
    for e in &p.els {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = match *e {
            PathEl::MoveTo(a) => write!(out, "M{} {}", num(a.x * k), num(a.y * k)),
            PathEl::LineTo(a) => write!(out, "L{} {}", num(a.x * k), num(a.y * k)),
            PathEl::CubicTo(a, b, c) => {
                write!(out, "C{} {} {} {} {} {}", num(a.x * k), num(a.y * k), num(b.x * k), num(b.y * k), num(c.x * k), num(c.y * k))
            }
            PathEl::Close => write!(out, "Z"),
        };
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A standalone SVG of `b` at its module, in page units, framed to hold the outline and every
/// slot: the outline, the slots upright and a small circle at each port.
pub fn svg(b: &Built) -> String {
    let k = b.module;
    let (mut x0, mut y0, mut x1, mut y1) = (0.0f64, 0.0f64, b.w, b.h);
    if let Some(r) = b.fills.bounds() {
        (x0, y0, x1, y1) = (x0.min(r.x0), y0.min(r.y0), x1.max(r.x1), y1.max(r.y1));
    }
    if let Some(r) = b.lines.bounds() {
        (x0, y0, x1, y1) = (x0.min(r.x0), y0.min(r.y0), x1.max(r.x1), y1.max(r.y1));
    }
    for l in &b.labels {
        let tw = text_width(l.text.chars().count(), l.size);
        let left = match l.anchor {
            "start" => l.x,
            "end" => l.x - tw,
            _ => l.x - tw / 2.0,
        };
        (x0, x1) = (x0.min(left), x1.max(left + tw));
        (y0, y1) = (y0.min(l.y - l.size * 0.6), y1.max(l.y + l.size * 0.6));
    }
    let m = 0.5;
    let (vx, vy, vw, vh) = ((x0 - m) * k, (y0 - m) * k, (x1 - x0 + 2.0 * m) * k, (y1 - y0 + 2.0 * m) * k);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
        num(vw),
        num(vh),
        num(vx),
        num(vy),
        num(vw),
        num(vh)
    );
    if !b.fills.is_empty() {
        let _ = writeln!(
            out,
            "  <path d=\"{}\" fill=\"#ffffff\" fill-rule=\"evenodd\" stroke=\"#1f2933\" stroke-width=\"1\" stroke-linejoin=\"round\"/>",
            path_data(&b.fills, k)
        );
    }
    if !b.lines.is_empty() {
        let _ = writeln!(
            out,
            "  <path d=\"{}\" fill=\"none\" stroke=\"#1f2933\" stroke-width=\"1\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            path_data(&b.lines, k)
        );
    }
    for l in &b.labels {
        let size = l.size * k;
        let _ = writeln!(
            out,
            "  <text x=\"{}\" y=\"{}\" font-family=\"Inter, sans-serif\" font-size=\"{}\" text-anchor=\"{}\" fill=\"#1f2933\">{}</text>",
            num(l.x * k),
            num(l.y * k + size * 0.35),
            num(size),
            l.anchor,
            escape(&l.text)
        );
    }
    if !b.ports.is_empty() {
        out.push_str("  <g class=\"ports\" fill=\"#c2410c\">\n");
        for p in &b.ports {
            let _ = writeln!(
                out,
                "    <circle cx=\"{}\" cy=\"{}\" r=\"{}\"><title>{}</title></circle>",
                num(p.x * k),
                num(p.y * k),
                num(k * 0.2),
                escape(&p.id)
            );
        }
        out.push_str("  </g>\n");
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests;
