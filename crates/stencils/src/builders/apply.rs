//! Putting a built symbol on a shape, and the parameter layers a shape is built from.
//!
//! The values a shape is drawn with come in layers, later ones winning:
//!
//! 1. the builder's defaults;
//! 2. the pack glyph's `params` (a variant switches the glyph, and with it the standard);
//! 3. the shape's own overrides (`Shape.glyph.params`, set by `glyph.setParams`);
//! 4. the element's properties whose keys the builder declares, and the element's tag for a
//!    builder with a `tag` parameter.

use std::collections::BTreeMap;

use vizcraft_diagram::geom::Point;
use vizcraft_diagram::glyph_state::{GlyphLabel, GlyphState};
use vizcraft_diagram::vizcraft_model::{Element, PropValue};
use vizcraft_diagram::{Freeform, Shape, ShapeKind};
use vizcraft_notation::{Catalog, Glyph};

use super::{BuilderSpec, Built};

/// Where a parameter value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The builder's default.
    Default,
    /// The pack glyph's params.
    Glyph,
    /// The shape's own override.
    Shape,
    /// The element's property (or tag).
    Element,
}

impl Layer {
    /// `default`, `glyph`, `shape` or `element`.
    pub fn name(self) -> &'static str {
        match self {
            Layer::Default => "default",
            Layer::Glyph => "glyph",
            Layer::Shape => "shape",
            Layer::Element => "element",
        }
    }
}

/// What a shape is built from: the builder, the pack glyph (when one names it) and every
/// declared parameter with its winning value and layer.
#[derive(Clone, Debug)]
pub struct Resolved<'a> {
    /// The builder.
    pub spec: &'static BuilderSpec,
    /// The pack glyph naming the builder, if any.
    pub glyph: Option<&'a Glyph>,
    /// `(key, value, layer)` for every declared parameter, in spec order.
    pub layers: Vec<(&'static str, PropValue, Layer)>,
}

impl Resolved<'_> {
    /// The winning values, ready for [`super::build`].
    pub fn values(&self) -> BTreeMap<String, PropValue> {
        self.layers.iter().map(|(k, v, _)| ((*k).to_string(), v.clone())).collect()
    }
}

/// The pack glyph that draws `shape`: for an element's view the glyph of the element's type
/// under `variant`, else the glyph its stencil names.
pub fn glyph_of<'a>(shape: &Shape, element: Option<&Element>, cat: &'a Catalog, variant: &str) -> Option<&'a Glyph> {
    let by_type = element.and_then(|e| cat.master_for_type_in(&e.type_id, variant)).and_then(|id| cat.glyph(id)).map(|(_, g)| g);
    let builder = |g: &&Glyph| g.outline == vizcraft_notation::loader::BUILDER_OUTLINE;
    by_type.filter(builder).or_else(|| cat.glyph(&shape.stencil).map(|(_, g)| g).filter(builder))
}

/// The builder and parameter layers of `shape` (see the module docs), or `None` when nothing
/// names a known builder.
pub fn resolve<'a>(shape: &Shape, element: Option<&Element>, cat: &'a Catalog, variant: &str) -> Option<Resolved<'a>> {
    let glyph = glyph_of(shape, element, cat, variant);
    let name = glyph.map(|g| g.builder.as_str()).or_else(|| shape.glyph.as_ref().map(|g| g.builder.as_str()))?;
    let spec = super::spec(name)?;
    let own = shape.glyph.as_ref().map(|g| &g.params);
    let layers = spec
        .params
        .iter()
        .map(|p| {
            let mut out = (p.key, p.default.value(), Layer::Default);
            if let Some(v) = glyph.and_then(|g| g.params.get(p.key)) {
                out = (p.key, v.clone(), Layer::Glyph);
            }
            if let Some(v) = own.and_then(|o| o.get(p.key)) {
                out = (p.key, v.clone(), Layer::Shape);
            }
            if let Some(e) = element {
                if let Some(v) = e.properties.get(p.key) {
                    out = (p.key, v.clone(), Layer::Element);
                } else if p.key == "tag" && !e.tag.trim().is_empty() {
                    out = (p.key, PropValue::Text(e.tag.clone()), Layer::Element);
                }
            }
            out
        })
        .collect();
    Some(Resolved { spec, glyph, layers })
}

/// The values `shape` is built with (see [`resolve`]); empty when no builder draws it.
pub fn effective(shape: &Shape, element: Option<&Element>, cat: &Catalog, variant: &str) -> BTreeMap<String, PropValue> {
    resolve(shape, element, cat, variant).map(|r| r.values()).unwrap_or_default()
}

/// The text slots of `b` normalized to its frame, sizes in page units.
pub fn labels_of(b: &Built) -> Vec<GlyphLabel> {
    b.labels
        .iter()
        .map(|l| GlyphLabel {
            id: l.id.clone(),
            text: l.text.clone(),
            x: l.x / b.w,
            y: l.y / b.h,
            size: l.size * b.module,
            anchor: l.anchor.to_string(),
        })
        .collect()
}

/// The ports of `b` as normalized connection points.
pub fn points_of(b: &Built) -> Vec<Point> {
    b.ports.iter().map(|p| Point::new(p.x / b.w, p.y / b.h)).collect()
}

/// Draw `shape` as `built` from builder `builder`: a freeform at the built size (the top left
/// kept), the ports as connection points, and the port ids and slots in `Shape.glyph`. The
/// shape's own parameter overrides are kept. Only fields that differ are written; returns
/// whether anything changed.
pub fn apply(shape: &mut Shape, builder: &str, built: &Built) -> bool {
    let mut next = shape.clone();
    let (w, h) = (built.width(), built.height());
    next.kind = ShapeKind::Freeform;
    next.w = w;
    next.h = h;
    next.path = Some(Freeform::in_frame(&built.path(), !built.fills.is_empty(), w, h));
    next.connection_points = points_of(built);
    let params = shape.glyph.as_ref().map(|g| g.params.clone()).unwrap_or_default();
    next.glyph = Some(GlyphState {
        builder: builder.to_string(),
        params,
        ports: built.ports.iter().map(|p| p.id.clone()).collect(),
        labels: labels_of(built),
    });
    next.sanitize();
    if next == *shape {
        return false;
    }
    *shape = next;
    true
}

/// Move `shape` so its first port lands on the `module` grid (page units), when it has a port.
pub fn snap_first_port(shape: &mut Shape, module: f64) {
    if !(module.is_finite() && module > 0.0) {
        return;
    }
    let Some(p) = shape.connection_points.first().copied() else { return };
    // Through the transform, so a turned or mirrored glyph snaps the pin where it is drawn.
    let at = shape.transform().apply(Point::new(p.x * shape.w, p.y * shape.h));
    let dx = (at.x / module).round() * module - at.x;
    let dy = (at.y / module).round() * module - at.y;
    if dx.is_finite() && dy.is_finite() {
        shape.x += dx;
        shape.y += dy;
    }
}
