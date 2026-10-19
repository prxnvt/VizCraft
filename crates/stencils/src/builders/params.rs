//! Builder parameters: what each builder declares ([`ParamSpec`]) and the values it is drawn
//! with ([`Params`]).
//!
//! Values come from packs, documents and agents, so they are hostile. [`Params::resolve`] never
//! fails: a value of the wrong kind, out of range or too long is clamped or replaced by the
//! default, and a warning says so. Unknown keys are ignored with a warning.

use std::collections::BTreeMap;

use vizcraft_diagram::vizcraft_model::PropValue;

/// Most warnings one resolution or build reports; the rest are counted in the last one.
pub const MAX_WARNINGS: usize = 32;

/// What values a parameter takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    /// One of these names.
    Choice(&'static [&'static str]),
    /// A whole number in `min..=max`.
    Int {
        /// Smallest value.
        min: i64,
        /// Largest value.
        max: i64,
    },
    /// A number in `min..=max`.
    Number {
        /// Smallest value.
        min: f64,
        /// Largest value.
        max: f64,
    },
    /// A flag.
    Bool,
    /// Text of at most `max` characters.
    Text {
        /// Longest text kept, in characters.
        max: usize,
    },
}

impl ParamKind {
    /// The kind's name for schemas: `choice`, `int`, `number`, `bool` or `text`.
    pub fn name(self) -> &'static str {
        match self {
            ParamKind::Choice(_) => "choice",
            ParamKind::Int { .. } => "int",
            ParamKind::Number { .. } => "number",
            ParamKind::Bool => "bool",
            ParamKind::Text { .. } => "text",
        }
    }
}

/// A default value that can live in a `static`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Def {
    /// Text or a choice name.
    Text(&'static str),
    /// A whole number.
    Int(i64),
    /// A number.
    Num(f64),
    /// A flag.
    Bool(bool),
}

impl Def {
    /// The default as a property value.
    pub fn value(self) -> PropValue {
        match self {
            Def::Text(t) => PropValue::Text(t.to_string()),
            Def::Int(i) => PropValue::Number(i as f64),
            Def::Num(n) => PropValue::Number(n),
            Def::Bool(b) => PropValue::Bool(b),
        }
    }
}

/// One parameter a builder declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// Key in params and element properties (`poles`).
    pub key: &'static str,
    /// Name shown in the Inspector.
    pub label: &'static str,
    /// The values it takes.
    pub kind: ParamKind,
    /// The value when none is given or the given one is unusable.
    pub default: Def,
    /// One sentence for the Inspector and agents.
    pub description: &'static str,
}

impl ParamSpec {
    /// `value` made valid for this parameter, and why it changed when it did. `None` as the
    /// value means "use the default"; a value that cannot be used at all also gives the default.
    pub fn check(&self, value: &PropValue) -> (PropValue, Option<String>) {
        let fallback = |why: &str| (self.default.value(), Some(format!("`{}`: {why}; the default is used", self.key)));
        match self.kind {
            ParamKind::Choice(names) => {
                let Some(t) = value.as_str() else { return fallback("must be one of the listed names") };
                if let Some(n) = names.iter().find(|n| **n == t) {
                    return (PropValue::Text((*n).to_string()), None);
                }
                // A different case is accepted quietly; anything else is not.
                match names.iter().find(|n| n.eq_ignore_ascii_case(t.trim())) {
                    Some(n) => (PropValue::Text((*n).to_string()), None),
                    None => fallback(&format!("`{}` is not one of {}", short(t), names.join(", "))),
                }
            }
            ParamKind::Int { min, max } => {
                let n = match value {
                    PropValue::Number(n) => Some(*n),
                    PropValue::Text(t) => t.trim().parse::<f64>().ok(),
                    PropValue::Bool(_) => None,
                };
                let Some(n) = n.filter(|n| n.is_finite()) else { return fallback("must be a whole number") };
                let r = n.round().clamp(min as f64, max as f64);
                let why = (r != n).then(|| format!("`{}`: {n} is kept to {r} (whole numbers {min} to {max})", self.key));
                (PropValue::Number(r), why)
            }
            ParamKind::Number { min, max } => {
                let n = match value {
                    PropValue::Number(n) => Some(*n),
                    PropValue::Text(t) => t.trim().parse::<f64>().ok(),
                    PropValue::Bool(_) => None,
                };
                let Some(n) = n.filter(|n| n.is_finite()) else { return fallback("must be a number") };
                let r = n.clamp(min, max);
                let why = (r != n).then(|| format!("`{}`: {n} is kept to {r} ({min} to {max})", self.key));
                (PropValue::Number(r), why)
            }
            ParamKind::Bool => match value {
                PropValue::Bool(b) => (PropValue::Bool(*b), None),
                PropValue::Text(t) if matches!(t.trim(), "true" | "yes" | "1") => (PropValue::Bool(true), None),
                PropValue::Text(t) if matches!(t.trim(), "false" | "no" | "0" | "") => (PropValue::Bool(false), None),
                PropValue::Number(n) if *n == 0.0 || *n == 1.0 => (PropValue::Bool(*n == 1.0), None),
                _ => fallback("must be true or false"),
            },
            ParamKind::Text { max } => {
                let t = match value {
                    PropValue::Text(t) => t.clone(),
                    PropValue::Number(n) if n.is_finite() => fmt_num(*n),
                    PropValue::Bool(b) => b.to_string(),
                    PropValue::Number(_) => return fallback("must be text"),
                };
                // Control characters never reach a drawing.
                let clean: String = t.chars().filter(|c| !c.is_control()).collect();
                let cut: String = clean.chars().take(max).collect();
                let why = (cut.chars().count() < t.chars().count()).then(|| format!("`{}`: cut to {max} characters", self.key));
                (PropValue::Text(cut), why)
            }
        }
    }
}

/// A number as short text: no trailing `.0`.
pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// The first 32 chars of `t`, for messages about hostile text.
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(32).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// The values one build uses: every declared parameter, valid for its spec.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params {
    values: BTreeMap<&'static str, PropValue>,
}

impl Params {
    /// Every parameter of `specs` from `raw`, each checked against its spec (see
    /// [`ParamSpec::check`]); missing ones take their defaults. Warnings for unusable values and
    /// unknown keys go to `warnings`.
    pub fn resolve(specs: &[ParamSpec], raw: &BTreeMap<String, PropValue>, warnings: &mut Vec<String>) -> Params {
        let mut values = BTreeMap::new();
        for spec in specs {
            let (v, why) = match raw.get(spec.key) {
                Some(v) => spec.check(v),
                None => (spec.default.value(), None),
            };
            if let Some(w) = why {
                warn(warnings, w);
            }
            values.insert(spec.key, v);
        }
        for k in raw.keys().filter(|k| !specs.iter().any(|s| s.key == k.as_str())).take(MAX_WARNINGS) {
            warn(warnings, format!("`{}` is not a parameter of this builder and is ignored", short(k)));
        }
        Params { values }
    }

    /// The text of a choice or text parameter; empty when it is not text.
    pub fn text(&self, key: &str) -> &str {
        self.values.get(key).and_then(PropValue::as_str).unwrap_or_default()
    }

    /// A whole-number parameter, `0` when absent.
    pub fn int(&self, key: &str) -> i64 {
        self.values.get(key).and_then(PropValue::as_f64).map(|n| n as i64).unwrap_or(0)
    }

    /// A number parameter, `0` when absent.
    pub fn num(&self, key: &str) -> f64 {
        self.values.get(key).and_then(PropValue::as_f64).unwrap_or(0.0)
    }

    /// A flag parameter, `false` when absent.
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.values.get(key), Some(PropValue::Bool(true)))
    }

    /// Every resolved value by key.
    pub fn values(&self) -> &BTreeMap<&'static str, PropValue> {
        &self.values
    }
}

/// Push a warning, keeping at most [`MAX_WARNINGS`].
pub fn warn(warnings: &mut Vec<String>, w: String) {
    if warnings.len() < MAX_WARNINGS {
        warnings.push(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECS: &[ParamSpec] = &[
        ParamSpec { key: "form", label: "Form", kind: ParamKind::Choice(&["no", "nc"]), default: Def::Text("no"), description: "" },
        ParamSpec { key: "poles", label: "Poles", kind: ParamKind::Int { min: 1, max: 6 }, default: Def::Int(1), description: "" },
        ParamSpec { key: "gap", label: "Gap", kind: ParamKind::Number { min: 0.0, max: 2.0 }, default: Def::Num(0.5), description: "" },
        ParamSpec { key: "on", label: "On", kind: ParamKind::Bool, default: Def::Bool(false), description: "" },
        ParamSpec { key: "tag", label: "Tag", kind: ParamKind::Text { max: 4 }, default: Def::Text(""), description: "" },
    ];

    #[test]
    fn values_are_clamped_or_defaulted_with_a_warning() {
        let raw: BTreeMap<String, PropValue> = [
            ("form", PropValue::Text("NC".into())),
            ("poles", PropValue::Number(9.6)),
            ("gap", PropValue::Text("1.5".into())),
            ("on", PropValue::Text("maybe".into())),
            ("tag", PropValue::Text("K1\u{0}23456".into())),
            ("ghost", PropValue::Bool(true)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let mut w = Vec::new();
        let p = Params::resolve(SPECS, &raw, &mut w);
        assert_eq!((p.text("form"), p.int("poles"), p.num("gap"), p.flag("on"), p.text("tag")), ("nc", 6, 1.5, false, "K123"));
        assert_eq!(w.len(), 4, "{w:?}");
        assert!(w.iter().any(|m| m.contains("ghost")));
        let none = Params::resolve(SPECS, &BTreeMap::new(), &mut Vec::new());
        assert_eq!((none.text("form"), none.int("poles"), none.num("gap")), ("no", 1, 0.5));
        assert_eq!(none.text("nothing"), "");
    }

    #[test]
    fn junk_never_panics() {
        for v in [PropValue::Number(f64::NAN), PropValue::Number(1e308), PropValue::Text("x".repeat(100_000)), PropValue::Bool(true)] {
            for s in SPECS {
                let _ = s.check(&v);
            }
        }
    }
}
