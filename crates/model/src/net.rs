//! Nets and the symbol grid module.
//!
//! A [`Net`] is one electrical potential, one pipe run or one signal: the set of wire segments
//! (connectors with `Connector.net` set, in `vizcraft-diagram`) that glue joins through junctions
//! and pins. The model holds the nets themselves (name, number, kind and properties); which
//! segments belong to which net is stored on the connectors and kept consistent by the net pass
//! of the sync step (`vizcraft-notation`). Nets share the document's one id space with elements,
//! relationships, shapes, connectors and pages (`n7`).
//!
//! A [`Module`] is the symbol grid a page snaps ports to: 2.5 mm (IEC 81714-2 and ISO 128
//! drawings), 0.1 inch (ANSI and IEEE practice) or a custom step. It is a choice on the page's
//! view, never imposed: the default is none.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::repair::truncate;
use crate::serde_util::{lenient, lenient_map};
use crate::{MAX_ID, MAX_TAG, Model, PropValue};

/// Most nets a model holds; a file with more keeps the first ones.
pub const MAX_NETS: usize = 100_000;

/// What a net carries. The kind decides how it is drawn (a pipe tee has no dot) and how it is
/// numbered; it never changes how glue joins segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetKind {
    /// An electrical wire (the default).
    #[default]
    Wire,
    /// A named potential spread over many wires (`L1`, `0V`, `PE`).
    Potential,
    /// A pipe line (process and instrumentation diagrams).
    Pipe,
    /// A fluid line of a hydraulic or pneumatic circuit.
    Fluid,
    /// A signal or data line.
    Signal,
}

impl NetKind {
    /// Every kind, in documentation order.
    pub const ALL: [NetKind; 5] = [NetKind::Wire, NetKind::Potential, NetKind::Pipe, NetKind::Fluid, NetKind::Signal];
    /// The name used on disk and in command params.
    pub fn name(self) -> &'static str {
        match self {
            NetKind::Wire => "wire",
            NetKind::Potential => "potential",
            NetKind::Pipe => "pipe",
            NetKind::Fluid => "fluid",
            NetKind::Signal => "signal",
        }
    }
    /// The kind for a name, or `None` for an unknown name.
    pub fn from_name(s: &str) -> Option<Self> {
        NetKind::ALL.into_iter().find(|k| k.name() == s)
    }
    /// Whether a junction of this kind is drawn as a filled dot (wires, signals and fluid power
    /// lines, which ISO 1219 joins with a dot) rather than a plain tee (pipes).
    pub fn dotted(self) -> bool {
        matches!(self, NetKind::Wire | NetKind::Potential | NetKind::Signal | NetKind::Fluid)
    }
}

/// Where a net's number (or name) is written along its segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetLabel {
    /// Once, on the net's longest segment (the default).
    #[default]
    Once,
    /// On every segment.
    Each,
    /// Not written.
    Off,
}

impl NetLabel {
    /// The name used on disk and in command params.
    pub fn name(self) -> &'static str {
        match self {
            NetLabel::Once => "once",
            NetLabel::Each => "each",
            NetLabel::Off => "off",
        }
    }
    /// The mode for a name, or `None` for an unknown name.
    pub fn from_name(s: &str) -> Option<Self> {
        [NetLabel::Once, NetLabel::Each, NetLabel::Off].into_iter().find(|k| k.name() == s)
    }
}

/// Property keys with an agreed meaning on a net; any other key is allowed too.
pub mod keys {
    /// Insulation or line colour code (`BK`, `BU`, `GNYE`).
    pub const COLOUR: &str = "colour";
    /// Conductor cross-section or gauge (`1.5mm2`, `18AWG`).
    pub const GAUGE: &str = "gauge";
    /// Nominal pipe size (`DN50`, `2in`).
    pub const SIZE: &str = "size";
    /// The service or medium (`CW`, `steam`).
    pub const SERVICE: &str = "service";
    /// Pipe class or specification.
    pub const CLASS: &str = "class";
}

/// One net: an electrical potential, a pipe run or a signal.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Net {
    /// Stable id in the document's shared id space (`n7`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// What it carries.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "crate::net::is_default")]
    pub kind: NetKind,
    /// Its name (`L1`, `24V`, `CW-101`); empty is unnamed. A named net may span pages and
    /// unconnected wires, which is how potentials and off-page continuation work.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Its wire or line number (`2003`); empty for none. Written beside the line before the name.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub number: String,
    /// Properties such as [`keys::COLOUR`] and [`keys::GAUGE`].
    #[serde(deserialize_with = "lenient_map", skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, PropValue>,
    /// Where its number or name is written.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "crate::net::is_default")]
    pub label: NetLabel,
}

/// Whether a value equals its default, so default fields stay out of files.
pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl Net {
    /// An unnamed net of `kind`.
    pub fn new(id: impl Into<String>, kind: NetKind) -> Self {
        Net { id: id.into(), kind, ..Net::default() }
    }
    /// Whether the net has a name or a number. Named nets are kept when they lose their
    /// segments, are never merged with another named net and may span unconnected wires.
    pub fn named(&self) -> bool {
        !self.name.is_empty() || !self.number.is_empty()
    }
    /// The text written beside its segments: the number, else the name.
    pub fn label_text(&self) -> &str {
        if self.number.is_empty() { &self.name } else { &self.number }
    }
}

/// Cut lengths and counts, drop nets with an empty id or an id another model item or net
/// already holds. Runs after the element and relationship repair.
pub(crate) fn sanitize(model: &mut Model) {
    model.nets.truncate(MAX_NETS);
    let mut ids: HashSet<String> = model.elements.iter().map(|e| e.id.clone()).chain(model.relationships.iter().map(|r| r.id.clone())).collect();
    model.nets.retain_mut(|n| {
        truncate(&mut n.id, MAX_ID);
        truncate(&mut n.name, MAX_TAG);
        truncate(&mut n.number, MAX_TAG);
        crate::repair::sanitize_props(&mut n.properties);
        !n.id.is_empty() && ids.insert(n.id.clone())
    });
}

/// The symbol grid module of a page: the step ports and wires snap to. On disk it is a name
/// (`"iso2_5mm"`, `"inch0_1"`) or a number of page units (100 per inch); `"none"` and anything
/// unreadable are no module.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Module {
    /// No symbol grid: the zoom grid only (the default).
    #[default]
    None,
    /// 2.5 mm, the IEC 81714-2 module (9.8425 page units).
    Iso2_5mm,
    /// 0.1 inch, the ANSI and IEEE practice (10 page units).
    Inch0_1,
    /// A custom step in page units.
    Custom(f64),
}

impl Module {
    /// Smallest custom step kept, in page units.
    pub const MIN_STEP: f64 = 0.5;
    /// Largest custom step kept, in page units.
    pub const MAX_STEP: f64 = 10_000.0;
    /// The step of [`Module::Iso2_5mm`]: 2.5 mm at 100 units per inch.
    pub const ISO_STEP: f64 = 2.5 * 100.0 / 25.4;

    /// The step in page units, `None` for no module.
    pub fn step(self) -> Option<f64> {
        match self {
            Module::None => None,
            Module::Iso2_5mm => Some(Self::ISO_STEP),
            Module::Inch0_1 => Some(10.0),
            Module::Custom(u) => (u.is_finite() && u >= Self::MIN_STEP).then_some(u.min(Self::MAX_STEP)),
        }
    }
    /// The name of a named module (`none`, `iso2_5mm`, `inch0_1`); `None` for a custom step.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Module::None => Some("none"),
            Module::Iso2_5mm => Some("iso2_5mm"),
            Module::Inch0_1 => Some("inch0_1"),
            Module::Custom(_) => None,
        }
    }
    /// The module a name stands for; `None` for an unknown name.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "none" | "" => Some(Module::None),
            "iso2_5mm" => Some(Module::Iso2_5mm),
            "inch0_1" => Some(Module::Inch0_1),
            _ => None,
        }
    }
    /// The module a JSON value stands for: a name, or a number of page units within
    /// [`Module::MIN_STEP`] to [`Module::MAX_STEP`]. Anything else is `None`.
    pub fn from_value(v: &serde_json::Value) -> Option<Self> {
        match v {
            serde_json::Value::String(s) => Module::from_name(s),
            serde_json::Value::Number(n) => n.as_f64().filter(|u| u.is_finite() && (Self::MIN_STEP..=Self::MAX_STEP).contains(u)).map(Module::Custom),
            _ => None,
        }
    }
    /// The JSON form: the name, or the custom step as a number.
    pub fn to_value(self) -> serde_json::Value {
        match (self.name(), self) {
            (Some(n), _) => serde_json::Value::from(n),
            (None, Module::Custom(u)) => serde_json::Value::from(u),
            (None, _) => serde_json::Value::from("none"),
        }
    }
    /// Whether this is no module.
    pub fn is_none(&self) -> bool {
        self.step().is_none()
    }
}

impl Serialize for Module {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Module {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: serde_json::Value = lenient(d)?;
        Ok(Module::from_value(&v).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nets_round_trip_with_defaults_left_out() {
        let n = Net::new("n1", NetKind::Wire);
        assert_eq!(serde_json::to_value(&n).unwrap(), json!({"id": "n1"}));
        let mut p = Net { name: "L1".into(), kind: NetKind::Potential, label: NetLabel::Each, ..Net::new("n2", NetKind::Wire) };
        p.properties.insert(keys::COLOUR.into(), PropValue::Text("BK".into()));
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v, json!({"id": "n2", "kind": "potential", "name": "L1", "properties": {"colour": "BK"}, "label": "each"}));
        assert_eq!(serde_json::from_value::<Net>(v).unwrap(), p);
        assert!(p.named() && !n.named());
        assert_eq!(Net { number: "2003".into(), ..p.clone() }.label_text(), "2003");
    }

    #[test]
    fn junk_nets_read_as_defaults() {
        let n: Net = serde_json::from_value(json!({"id": 4, "kind": "plasma", "label": [], "properties": {"a": {}, "b": 2}})).unwrap();
        assert_eq!((n.id.as_str(), n.kind, n.label), ("", NetKind::Wire, NetLabel::Once));
        assert_eq!(n.properties.len(), 1);
    }

    #[test]
    fn modules_read_names_and_numbers() {
        for (v, m) in [
            (json!("iso2_5mm"), Module::Iso2_5mm),
            (json!("inch0_1"), Module::Inch0_1),
            (json!("none"), Module::None),
            (json!(12.5), Module::Custom(12.5)),
            (json!("parsec"), Module::None),
            (json!(-3), Module::None),
            (json!(1e9), Module::None),
            (json!({}), Module::None),
        ] {
            assert_eq!(serde_json::from_value::<Module>(v.clone()).unwrap(), m, "{v}");
        }
        assert_eq!(serde_json::to_value(Module::Custom(12.5)).unwrap(), json!(12.5));
        assert_eq!(serde_json::to_value(Module::Iso2_5mm).unwrap(), json!("iso2_5mm"));
        assert!((Module::Iso2_5mm.step().unwrap() - 9.8425).abs() < 1e-4);
        assert_eq!(Module::Inch0_1.step(), Some(10.0));
        assert!(Module::None.is_none() && Module::Custom(f64::NAN).is_none());
    }
}
