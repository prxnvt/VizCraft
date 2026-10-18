//! Custom masters: the document's own stencil (Visio's Document Stencil).
//!
//! A [`CustomMaster`] is a snapshot of some shapes, the connectors among them with their glue, and
//! the model items they draw, moved so its bounds start at `(0, 0)`. The engine's `master.*`
//! commands make one from the selection and insert copies with fresh ids; this module holds the
//! data, its caps, and the `.vizcraft-stencil` file form that `master.export` and `master.import`
//! write and read.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::geom::Rect;
use crate::model::truncate;
use crate::serde_util::{lenient, lenient_vec, num};
use crate::vizcraft_model::{Element, Relationship};
use crate::{Connector, End, Shape};

/// The file extension of an exported stencil.
pub const STENCIL_EXTENSION: &str = "vizcraft-stencil";

/// The marker an exported stencil carries in its `vizcraft` key.
pub const STENCIL_MARKER: &str = "stencil";

/// The stencil file version this build writes and reads.
pub const STENCIL_VERSION: u64 = 1;

/// One master of the document's own stencil.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomMaster {
    /// Unique id among the document's masters (`m1`).
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// The name shown in the Document Stencil pane, at most [`CustomMaster::MAX_NAME`] bytes.
    #[serde(deserialize_with = "lenient")]
    pub name: String,
    /// Width of the snapshot's bounds.
    #[serde(deserialize_with = "num")]
    pub w: f64,
    /// Height of the snapshot's bounds.
    #[serde(deserialize_with = "num")]
    pub h: f64,
    /// The shapes, in page order, placed so the bounds start at `(0, 0)`.
    #[serde(deserialize_with = "lenient_vec")]
    pub shapes: Vec<Shape>,
    /// The connectors among the shapes; an end glued to a shape outside the master is free.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<Connector>,
    /// The model elements the shapes draw, each once.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<Element>,
    /// The model relationships the connectors draw, each once.
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
}

impl CustomMaster {
    /// Most masters one document keeps.
    pub const MAX_MASTERS: usize = 256;
    /// Most shapes, and separately connectors, one master keeps.
    pub const MAX_ITEMS: usize = 1000;
    /// Longest name kept, in bytes.
    pub const MAX_NAME: usize = 256;

    /// A master named `name` from `shapes` and `connectors` (already a self-contained piece: ends
    /// glued outside it freed), moved so their bounds start at `(0, 0)`. `None` when there are no
    /// shapes.
    pub fn new(id: String, name: &str, shapes: Vec<Shape>, connectors: Vec<Connector>) -> Option<CustomMaster> {
        let bounds = shapes.iter().map(Shape::bounds).reduce(|a, b| a.union(&b))?;
        let mut m = CustomMaster { id, name: name.to_string(), w: bounds.width(), h: bounds.height(), shapes, connectors, ..Default::default() };
        let (dx, dy) = (-bounds.x0, -bounds.y0);
        for s in &mut m.shapes {
            s.x += dx;
            s.y += dy;
        }
        for c in &mut m.connectors {
            for e in [&mut c.start, &mut c.end] {
                if let End::Free { x, y } = e {
                    *x += dx;
                    *y += dy;
                }
            }
            for p in &mut c.waypoints {
                p.x += dx;
                p.y += dy;
            }
        }
        m.sanitize();
        Some(m)
    }

    /// The bounds the shapes were moved into, `(0, 0)` to `(w, h)`.
    pub fn bounds(&self) -> Rect {
        Rect::from_xywh(0.0, 0.0, self.w, self.h)
    }

    /// Cap the name, the item counts and every shape and connector; the size is kept finite and
    /// non-negative.
    pub fn sanitize(&mut self) {
        truncate(&mut self.id, crate::model::MAX_ID);
        truncate(&mut self.name, Self::MAX_NAME);
        self.w = crate::geom::finite(self.w, 0.0).clamp(0.0, crate::MAX_EXTENT);
        self.h = crate::geom::finite(self.h, 0.0).clamp(0.0, crate::MAX_EXTENT);
        self.shapes.truncate(Self::MAX_ITEMS);
        self.shapes.iter_mut().for_each(Shape::sanitize);
        self.connectors.truncate(Self::MAX_ITEMS);
        self.connectors.iter_mut().for_each(Connector::sanitize);
        self.elements.truncate(Self::MAX_ITEMS);
        self.relationships.truncate(Self::MAX_ITEMS);
    }
}

/// Keep the document's masters bounded: each sanitised, with a plain unique id and at least one
/// shape, at most [`CustomMaster::MAX_MASTERS`] of them.
pub(crate) fn sanitize_masters(masters: &mut Vec<CustomMaster>) {
    masters.truncate(CustomMaster::MAX_MASTERS);
    let mut seen = HashSet::new();
    masters.retain_mut(|m| {
        m.sanitize();
        crate::is_plain_id(&m.id) && !m.shapes.is_empty() && seen.insert(m.id.clone())
    });
}

/// The `.vizcraft-stencil` file form of `masters`: `{"vizcraft": "stencil", "version": 1,
/// "masters": [...]}`, pretty-printed.
pub fn to_stencil_json(masters: &[CustomMaster]) -> Result<String, String> {
    let v = json!({"vizcraft": STENCIL_MARKER, "version": STENCIL_VERSION, "masters": masters});
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Read a `.vizcraft-stencil` file. Masters that do not parse are skipped and the rest sanitised
/// and capped; the ids are kept as written (the importer gives them fresh ones). A file that is
/// not a stencil, or is from a newer version, is an error.
pub fn from_stencil_json(text: &str) -> Result<Vec<CustomMaster>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not a stencil file: {e}"))?;
    if v.get("vizcraft").and_then(Value::as_str) != Some(STENCIL_MARKER) {
        return Err("not a VizCraft stencil file".into());
    }
    let version = v.get("version").and_then(Value::as_u64).unwrap_or(STENCIL_VERSION);
    if version > STENCIL_VERSION {
        return Err(format!("the stencil is version {version}; this VizCraft reads version {STENCIL_VERSION}"));
    }
    let list = v.get("masters").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<CustomMaster> = list
        .into_iter()
        .take(CustomMaster::MAX_MASTERS)
        .filter_map(|m| serde_json::from_value::<CustomMaster>(m).ok())
        .map(|mut m| {
            m.sanitize();
            m
        })
        .filter(|m| !m.shapes.is_empty())
        .collect();
    out.truncate(CustomMaster::MAX_MASTERS);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShapeKind;

    fn two() -> CustomMaster {
        let a = Shape::new("a", ShapeKind::Rectangle, 100.0, 50.0, 40.0, 20.0);
        let b = Shape::new("b", ShapeKind::Ellipse, 200.0, 80.0, 30.0, 30.0);
        let c = Connector::new("c", End::dynamic("a"), End::dynamic("b"));
        CustomMaster::new("m1".into(), "Pair", vec![a, b], vec![c]).unwrap()
    }

    #[test]
    fn a_master_starts_at_the_origin() {
        let m = two();
        assert_eq!((m.w, m.h), (130.0, 60.0));
        assert_eq!((m.shapes[0].x, m.shapes[0].y), (0.0, 0.0));
        assert_eq!((m.shapes[1].x, m.shapes[1].y), (100.0, 30.0));
        assert!(CustomMaster::new("m2".into(), "None", Vec::new(), Vec::new()).is_none());
    }

    #[test]
    fn stencil_files_round_trip_and_junk_is_refused() {
        let m = two();
        let text = to_stencil_json(std::slice::from_ref(&m)).unwrap();
        assert_eq!(from_stencil_json(&text).unwrap(), vec![m]);
        assert!(from_stencil_json("[]").is_err());
        assert!(from_stencil_json(r#"{"vizcraft": "clip"}"#).is_err());
        assert!(from_stencil_json(r#"{"vizcraft": "stencil", "version": 99, "masters": []}"#).is_err());
        let junk = from_stencil_json(r#"{"vizcraft": "stencil", "masters": [1, {"name": 5}, {"shapes": [{"w": "x"}], "w": -1}]}"#).unwrap();
        assert_eq!(junk.len(), 1);
        assert_eq!(junk[0].w, 0.0);
    }

    #[test]
    fn document_masters_are_capped_and_unique() {
        let mut list = vec![two(), two(), CustomMaster { id: "bad id".into(), ..two() }];
        sanitize_masters(&mut list);
        assert_eq!(list.len(), 1);
    }
}
