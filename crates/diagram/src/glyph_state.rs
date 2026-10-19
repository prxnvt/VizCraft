//! The state of a shape drawn by a parametric glyph builder (`vizcraft_stencils::builders`).
//!
//! A builder draws a symbol (a contact, a valve, a weld symbol) from parameters. The shape keeps
//! which builder drew it, its own parameter overrides, the port ids the builder gave its
//! connection points (point `i` is port `ports[i]`) and the text slots the builder placed
//! (terminal numbers, the letters of an instrument bubble). The outline itself is the shape's
//! freeform path. Everything here is read from files, so it is sanitized and capped.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use vizcraft_model::PropValue;

use crate::geom::finite;
use crate::model::truncate;
use crate::serde_util::{lenient, lenient_vec, num};

/// What a builder drew on a shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GlyphState {
    /// The builder's name (`contact`).
    #[serde(deserialize_with = "lenient")]
    pub builder: String,
    /// The shape's own parameter overrides, at most [`GlyphState::MAX_PARAMS`]. They win over
    /// the glyph's defaults; the element's properties win over them.
    #[serde(deserialize_with = "lenient_params", skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, PropValue>,
    /// Port ids in connection point order, at most [`GlyphState::MAX_PORTS`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    /// Text slots, at most [`GlyphState::MAX_LABELS`], drawn upright over the outline.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<GlyphLabel>,
}

/// One text slot of a built symbol.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GlyphLabel {
    /// Slot id within the builder (`tag`, `t13`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// The text, at most [`GlyphState::MAX_LABEL`] bytes.
    #[serde(deserialize_with = "lenient")]
    pub text: String,
    /// Anchor across the frame, `0` (left) to `1` (right); a slot may sit a little outside.
    #[serde(deserialize_with = "num")]
    pub x: f64,
    /// Baseline middle down the frame, `0` (top) to `1` (bottom).
    #[serde(deserialize_with = "num")]
    pub y: f64,
    /// Text height in page units.
    #[serde(deserialize_with = "num")]
    pub size: f64,
    /// Horizontal anchor: `start`, `middle` or `end`.
    #[serde(deserialize_with = "lenient")]
    pub anchor: String,
}

impl GlyphState {
    /// Most parameter overrides kept.
    pub const MAX_PARAMS: usize = 64;
    /// Most ports kept.
    pub const MAX_PORTS: usize = 256;
    /// Most text slots kept.
    pub const MAX_LABELS: usize = 64;
    /// Longest label text, port id, parameter key or builder name kept, in bytes.
    pub const MAX_LABEL: usize = 256;
    /// Longest parameter text value kept, in bytes.
    pub const MAX_PARAM_TEXT: usize = 1024;

    /// Cap every list and string and keep every number finite and in range.
    pub fn sanitize(&mut self) {
        truncate(&mut self.builder, Self::MAX_LABEL);
        if self.params.len() > Self::MAX_PARAMS || self.params.keys().any(|k| k.is_empty() || k.len() > Self::MAX_LABEL) {
            let kept: BTreeMap<String, PropValue> = std::mem::take(&mut self.params)
                .into_iter()
                .filter(|(k, _)| !k.is_empty() && k.len() <= Self::MAX_LABEL)
                .take(Self::MAX_PARAMS)
                .collect();
            self.params = kept;
        }
        for v in self.params.values_mut() {
            match v {
                PropValue::Text(t) => truncate(t, Self::MAX_PARAM_TEXT),
                PropValue::Number(n) if !n.is_finite() => *n = 0.0,
                _ => {}
            }
        }
        self.ports.truncate(Self::MAX_PORTS);
        self.ports.iter_mut().for_each(|p| truncate(p, Self::MAX_LABEL));
        self.labels.truncate(Self::MAX_LABELS);
        for l in &mut self.labels {
            truncate(&mut l.id, Self::MAX_LABEL);
            truncate(&mut l.text, Self::MAX_LABEL);
            // A slot may hang a little outside the frame (a tag beside a contact), never far.
            l.x = finite(l.x, 0.5).clamp(-4.0, 5.0);
            l.y = finite(l.y, 0.5).clamp(-4.0, 5.0);
            l.size = finite(l.size, 10.0).clamp(0.5, 500.0);
            if !matches!(l.anchor.as_str(), "start" | "middle" | "end") {
                l.anchor = "middle".into();
            }
        }
    }
}

/// Parameter overrides that keep the entries whose value is a flag, a number or text and drop
/// the rest; a non-map reads as empty.
fn lenient_params<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, PropValue>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Value(PropValue),
        Other(serde::de::IgnoredAny),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Map {
        Value(BTreeMap<String, Entry>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Map::deserialize(d)? {
        Map::Value(m) => m
            .into_iter()
            .filter_map(|(k, v)| match v {
                Entry::Value(v) => Some((k, v)),
                Entry::Other(_) => None,
            })
            .collect(),
        Map::Other(_) => BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_state_is_capped_and_junk_entries_are_dropped() {
        let json = serde_json::json!({
            "builder": 5,
            "params": {"poles": 3, "bad": [1, 2], "form": "nc", "on": true, "nothing": null},
            "ports": ["13", 7, "14"],
            "labels": [{"id": "tag", "text": "K1", "x": 1e300, "y": "x", "size": -3, "anchor": "sideways"}, 9]
        });
        let mut g: GlyphState = serde_json::from_value(json).unwrap();
        g.sanitize();
        assert_eq!(g.builder, "");
        assert_eq!(g.params.keys().collect::<Vec<_>>(), ["form", "on", "poles"]);
        assert_eq!(g.ports, ["13", "14"]);
        let l = &g.labels[0];
        assert_eq!((l.x, l.y, l.size, l.anchor.as_str()), (5.0, 0.0, 0.5, "middle"));
        let mut big = GlyphState { ports: vec!["p".repeat(1000); 1000], ..GlyphState::default() };
        for i in 0..100 {
            big.params.insert(format!("k{i}"), PropValue::Text("t".repeat(5000)));
        }
        big.sanitize();
        assert_eq!((big.ports.len(), big.ports[0].len(), big.params.len()), (GlyphState::MAX_PORTS, GlyphState::MAX_LABEL, GlyphState::MAX_PARAMS));
        assert!(big.params.values().all(|v| v.as_str().is_some_and(|t| t.len() == GlyphState::MAX_PARAM_TEXT)));
    }
}
