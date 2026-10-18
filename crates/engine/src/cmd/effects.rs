//! The line and fill effect keys of `shape.setStyle`: dash, line cap, corner rounding, gradient,
//! fill and line transparency, shadow, soft edges and glow. Parsed and checked before anything
//! changes; numbers are clamped to the model's ranges rather than refused.

use serde_json::Value;
use vizcraft_diagram::theme::tint;
use vizcraft_diagram::{Color, Dash, Glow, Gradient, GradientKind, GradientStop, LineCap, Shadow, Style};

use super::{num, opt_str, short};
use crate::EngineError;

/// How a gradient key changes the fill.
#[derive(Clone, Debug, PartialEq)]
pub enum GradientChange {
    /// Remove the gradient.
    Clear,
    /// A gradient of this kind built from each shape's own fill colour.
    FromFill(GradientKind),
    /// Exactly this gradient.
    Set(Gradient),
}

/// How a shadow or glow key changes the effect.
#[derive(Clone, Debug, PartialEq)]
pub enum EffectChange<T> {
    /// Remove the effect.
    Clear,
    /// Set the effect.
    Set(T),
}

/// The effect keys given to `shape.setStyle`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectPatch {
    dash: Option<Dash>,
    line_cap: Option<LineCap>,
    corner_radius: Option<f64>,
    gradient: Option<GradientChange>,
    fill_transparency: Option<f64>,
    line_transparency: Option<f64>,
    shadow: Option<EffectChange<Shadow>>,
    soft_edges: Option<f64>,
    glow: Option<EffectChange<Glow>>,
}

/// The keys this patch reads, for error messages and the params doc.
pub const KEYS: &str = "dash, lineCap, cornerRadius, gradient, fillTransparency, lineTransparency, shadow, softEdges, glow";

fn bad(k: &str, what: &str) -> EngineError {
    EngineError::Params(format!("`{k}` must be {what}"))
}

fn color(v: &Value, k: &str) -> Result<Option<Color>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => Color::parse(t).map(Some).ok_or_else(|| EngineError::Params(format!("`{k}`: `{}` is not a #rrggbb colour", short(t)))),
    }
}

/// A named enum key: absent is `None`, an unknown name is an error listing the names.
fn named<T>(v: &Value, k: &str, from: fn(&str) -> Option<T>, names: &str) -> Result<Option<T>, EngineError> {
    match opt_str(v, k)? {
        None => Ok(None),
        Some(t) => from(t).map(Some).ok_or_else(|| bad(k, &format!("one of {names}"))),
    }
}

/// `false` or `"none"` clears the effect; anything else goes to `parse`.
fn is_clear(x: &Value) -> bool {
    matches!(x, Value::Bool(false)) || x.as_str() == Some("none")
}

impl EffectPatch {
    /// Read the effect keys from `v`.
    pub fn parse(v: &Value) -> Result<Self, EngineError> {
        let unit = |k: &str| -> Result<Option<f64>, EngineError> { Ok(num(v, k)?.map(|f| f.clamp(0.0, 1.0))) };
        Ok(EffectPatch {
            dash: named(v, "dash", Dash::from_name, "solid, dashed, dotted, dashDot, longDash")?,
            line_cap: named(v, "lineCap", LineCap::from_name, "butt, round, square")?,
            corner_radius: num(v, "cornerRadius")?.map(|f| f.clamp(0.0, Style::MAX_CORNER_RADIUS)),
            gradient: Self::gradient(v)?,
            fill_transparency: unit("fillTransparency")?,
            line_transparency: unit("lineTransparency")?,
            shadow: Self::shadow(v)?,
            soft_edges: num(v, "softEdges")?.map(|f| f.clamp(0.0, Style::MAX_EFFECT)),
            glow: Self::glow(v)?,
        })
    }

    /// Whether no effect key was given.
    pub fn is_empty(&self) -> bool {
        *self == EffectPatch::default()
    }

    fn gradient(v: &Value) -> Result<Option<GradientChange>, EngineError> {
        const WHAT: &str = "false, \"none\", true, \"linear\", \"radial\" or {kind?, angle?, stops: [{offset, color}]} with 2 to 16 stops";
        let Some(g) = v.get("gradient").filter(|g| !g.is_null()) else { return Ok(None) };
        if is_clear(g) {
            return Ok(Some(GradientChange::Clear));
        }
        match g {
            Value::Bool(true) => Ok(Some(GradientChange::FromFill(GradientKind::Linear))),
            Value::String(t) => GradientKind::from_name(t).map(|k| Some(GradientChange::FromFill(k))).ok_or_else(|| bad("gradient", WHAT)),
            Value::Object(_) => {
                let kind = named(g, "kind", GradientKind::from_name, "linear, radial")?.unwrap_or_default();
                let angle_deg = num(g, "angle")?.unwrap_or(90.0);
                let stops = g.get("stops").and_then(Value::as_array).ok_or_else(|| bad("gradient", WHAT))?;
                if stops.len() < 2 || stops.len() > Gradient::MAX_STOPS {
                    return Err(bad("gradient", WHAT));
                }
                let mut out = Vec::with_capacity(stops.len());
                for s in stops {
                    let offset = num(s, "offset")?.ok_or_else(|| bad("gradient", WHAT))?;
                    let color = color(s, "color")?.ok_or_else(|| bad("gradient", WHAT))?;
                    out.push(GradientStop { offset, color });
                }
                let mut grad = Gradient { kind, angle_deg, stops: out };
                grad.sanitize();
                Ok(Some(GradientChange::Set(grad)))
            }
            _ => Err(bad("gradient", WHAT)),
        }
    }

    fn shadow(v: &Value) -> Result<Option<EffectChange<Shadow>>, EngineError> {
        const WHAT: &str = "false, \"none\", true or {dx?, dy?, blur?, color?}";
        let Some(x) = v.get("shadow").filter(|x| !x.is_null()) else { return Ok(None) };
        if is_clear(x) {
            return Ok(Some(EffectChange::Clear));
        }
        let mut sh = Shadow::default();
        match x {
            Value::Bool(true) => {}
            Value::Object(_) => {
                sh.dx = num(x, "dx")?.unwrap_or(sh.dx);
                sh.dy = num(x, "dy")?.unwrap_or(sh.dy);
                sh.blur = num(x, "blur")?.unwrap_or(sh.blur);
                sh.color = color(x, "color")?.unwrap_or(sh.color);
            }
            _ => return Err(bad("shadow", WHAT)),
        }
        sh.sanitize();
        Ok(Some(EffectChange::Set(sh)))
    }

    fn glow(v: &Value) -> Result<Option<EffectChange<Glow>>, EngineError> {
        const WHAT: &str = "false, \"none\", true, a radius or {radius?, color?}";
        let Some(x) = v.get("glow").filter(|x| !x.is_null()) else { return Ok(None) };
        if is_clear(x) {
            return Ok(Some(EffectChange::Clear));
        }
        let mut g = Glow::default();
        match x {
            Value::Bool(true) => {}
            Value::Number(_) => g.radius = num(v, "glow")?.unwrap_or(g.radius),
            Value::Object(_) => {
                g.radius = num(x, "radius")?.unwrap_or(g.radius);
                g.color = color(x, "color")?.unwrap_or(g.color);
            }
            _ => return Err(bad("glow", WHAT)),
        }
        g.sanitize();
        Ok(Some(EffectChange::Set(g)))
    }

    /// Apply to a style. A connector (`with_fill` false) takes the line keys only: dash, line
    /// cap, corner rounding of its bends and line transparency.
    pub fn apply(&self, st: &mut Style, with_fill: bool) {
        if let Some(d) = self.dash {
            st.dash = d;
        }
        if let Some(c) = self.line_cap {
            st.line_cap = c;
        }
        if let Some(r) = self.corner_radius {
            st.corner_radius = r;
        }
        if let Some(t) = self.line_transparency {
            st.line_transparency = t;
        }
        if !with_fill {
            return;
        }
        match &self.gradient {
            None => {}
            Some(GradientChange::Clear) => st.gradient = None,
            Some(GradientChange::FromFill(kind)) => st.gradient = Some(from_fill(st.fill, *kind)),
            Some(GradientChange::Set(g)) => st.gradient = Some(g.clone()),
        }
        if let Some(t) = self.fill_transparency {
            st.fill_transparency = t;
        }
        match &self.shadow {
            None => {}
            Some(EffectChange::Clear) => st.shadow = None,
            Some(EffectChange::Set(s)) => st.shadow = Some(*s),
        }
        if let Some(s) = self.soft_edges {
            st.soft_edges = s;
        }
        match &self.glow {
            None => {}
            Some(EffectChange::Clear) => st.glow = None,
            Some(EffectChange::Set(g)) => st.glow = Some(*g),
        }
    }
}

/// A two-stop gradient of `fill`: lighter at the top (linear) or in the middle (radial). A
/// transparent fill uses white to grey so the gradient still shows.
pub fn from_fill(fill: Color, kind: GradientKind) -> Gradient {
    let base = if fill.a == 0 { Color::rgb(0xc8, 0xcf, 0xd8) } else { Color { a: 255, ..fill } };
    let light = tint(base, 0.65);
    Gradient { kind, angle_deg: 90.0, stops: vec![GradientStop { offset: 0.0, color: light }, GradientStop { offset: 1.0, color: base }] }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Dash, GradientKind, LineCap, Style};

    use crate::Session;

    fn first_shape(s: &Session) -> String {
        s.doc.page().unwrap().shapes[0].id.clone()
    }

    fn style_of(s: &Session, id: &str) -> Style {
        s.doc.page().unwrap().shape(id).unwrap().style.clone()
    }

    #[test]
    fn set_style_takes_every_effect_key() {
        let mut s = Session::new(crate::sample::sample_document());
        let id = first_shape(&s);
        s.run(
            "shape.setStyle",
            &json!({"ids": [id], "dash": "dashDot", "lineCap": "round", "cornerRadius": 8, "gradient": "radial",
                    "fillTransparency": 0.25, "lineTransparency": 0.5, "shadow": {"dx": 4, "blur": 3, "color": "#00000080"},
                    "softEdges": 5, "glow": 7}),
        )
        .unwrap();
        let st = style_of(&s, &id);
        assert_eq!(st.dash, Dash::DashDot);
        assert_eq!(st.line_cap, LineCap::Round);
        assert_eq!(st.corner_radius, 8.0);
        assert_eq!(st.gradient.as_ref().map(|g| g.kind), Some(GradientKind::Radial));
        assert_eq!(st.gradient.as_ref().map(|g| g.stops.last().unwrap().color), Some(st.fill));
        assert_eq!((st.fill_transparency, st.line_transparency), (0.25, 0.5));
        let sh = st.shadow.unwrap();
        assert_eq!((sh.dx, sh.dy, sh.blur, sh.color.a), (4.0, 3.0, 3.0, 0x80));
        assert_eq!(st.soft_edges, 5.0);
        assert_eq!(st.glow.unwrap().radius, 7.0);
        // Clearing.
        s.run("shape.setStyle", &json!({"ids": [id], "gradient": false, "shadow": "none", "glow": false})).unwrap();
        let st = style_of(&s, &id);
        assert!(st.gradient.is_none() && st.shadow.is_none() && st.glow.is_none());
    }

    #[test]
    fn set_style_clamps_numbers_and_refuses_junk_without_changing_anything() {
        let mut s = Session::new(crate::sample::sample_document());
        let id = first_shape(&s);
        s.run(
            "shape.setStyle",
            &json!({"ids": [id], "cornerRadius": 1e300, "fillTransparency": 7, "lineTransparency": -3, "softEdges": 1e9,
                    "glow": {"radius": -5}, "shadow": {"dx": 1e9, "dy": -1e9, "blur": -1}}),
        )
        .unwrap();
        let st = style_of(&s, &id);
        assert_eq!(st.corner_radius, Style::MAX_CORNER_RADIUS);
        assert_eq!((st.fill_transparency, st.line_transparency), (1.0, 0.0));
        assert_eq!(st.soft_edges, Style::MAX_EFFECT);
        assert_eq!(st.glow.unwrap().radius, 0.0);
        let sh = st.shadow.unwrap();
        assert_eq!((sh.dx, sh.dy, sh.blur), (1000.0, -1000.0, 0.0));
        let before = s.doc.clone();
        for bad in [
            json!({"ids": [id], "dash": "wavy"}),
            json!({"ids": [id], "lineCap": 3}),
            json!({"ids": [id], "cornerRadius": "big"}),
            json!({"ids": [id], "gradient": {"stops": [{"offset": 0, "color": "#fff"}]}}),
            json!({"ids": [id], "gradient": {"stops": [{"offset": 0, "color": "#fff"}, {"offset": 1, "color": "nope"}]}}),
            json!({"ids": [id], "gradient": 5}),
            json!({"ids": [id], "shadow": "big"}),
            json!({"ids": [id], "glow": [1]}),
            json!({"ids": [id], "fill": "#fff", "softEdges": "x"}),
        ] {
            assert!(s.run("shape.setStyle", &bad).is_err(), "{bad}");
        }
        assert_eq!(s.doc, before);
    }

    #[test]
    fn explicit_gradients_are_sorted_and_connectors_take_line_keys_only() {
        let mut s = Session::new(crate::sample::sample_document());
        let id = first_shape(&s);
        let stops = json!([{"offset": 1.5, "color": "#000000"}, {"offset": 0, "color": "#ffffff"}]);
        s.run("shape.setStyle", &json!({"ids": [id], "gradient": {"kind": "linear", "angle": 405, "stops": stops}})).unwrap();
        let g = style_of(&s, &id).gradient.unwrap();
        assert_eq!(g.angle_deg, 45.0);
        assert_eq!((g.stops[0].offset, g.stops[1].offset), (0.0, 1.0));
        let c = s.doc.page().unwrap().connectors[0].id.clone();
        s.run("shape.setStyle", &json!({"ids": [c], "dash": "dotted", "shadow": true, "glow": true, "lineTransparency": 0.3})).unwrap();
        let st = &s.doc.page().unwrap().connector(&c).unwrap().style;
        assert_eq!(st.dash, Dash::Dotted);
        assert_eq!(st.line_transparency, 0.3);
        assert!(st.shadow.is_none() && st.glow.is_none());
    }
}
