//! Port marks: where an element type's ports sit on a shape that draws it, for the canvas and
//! the exports to mark.
//!
//! A type's port `i` is the shape's connection point `i` (see `vizcraft_notation::bind`), so a
//! typed shape is given its type's port positions as connection points when it is placed
//! ([`port_points`]), and [`port_marks`] draws a small square at each: filled for `in`, hollow
//! for `out`, half filled for `inOut`, sized by the shape's line width.

use vizcraft_diagram::geom::{Point, Rect};
use vizcraft_diagram::{MAX_EXTENT, Shape};
pub use vizcraft_notation::PortDirection;
use vizcraft_notation::{Catalog, Port};

/// Most ports marked on one shape; a pack with more draws the first ones.
pub const MAX_MARKS: usize = 256;

/// One port drawn on a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct PortMark {
    /// The square, in world coordinates, centred on the connection point.
    pub rect: Rect,
    /// The port's label (its id when it has none).
    pub label: String,
    /// Which way relationships run through it.
    pub direction: PortDirection,
}

/// The normalized connection points of `ports`, in port order, each clamped into the frame.
pub fn points_of(ports: &[Port]) -> Vec<Point> {
    let unit = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 };
    ports.iter().take(MAX_MARKS).map(|p| Point::new(unit(p.x), unit(p.y))).collect()
}

/// The connection points a shape of element type `type_id` gets from the type's ports, or
/// `None` when the type is unknown or declares no ports (the shape keeps its kind's points).
pub fn port_points(type_id: &str) -> Option<Vec<Point>> {
    port_points_in(vizcraft_notation::catalog(), type_id)
}

/// [`port_points`] against catalog `cat`.
pub fn port_points_in(cat: &Catalog, type_id: &str) -> Option<Vec<Point>> {
    let (_, t) = cat.element_type(type_id)?;
    (!t.ports.is_empty()).then(|| points_of(&t.ports))
}

/// The side of a port square for a line `line_width` wide.
fn side(line_width: f64) -> f64 {
    let lw = if line_width.is_finite() { line_width.clamp(0.0, 20.0) } else { 1.0 };
    6.0 + 2.0 * lw
}

/// The ports of element type `type_id` as drawn on `shape`: one mark per port at the shape's
/// connection point of the same index. Empty for an unknown type, a type without ports, or a
/// port whose point the shape does not have.
pub fn port_marks(shape: &Shape, type_id: &str) -> Vec<PortMark> {
    port_marks_in(vizcraft_notation::catalog(), shape, type_id)
}

/// [`port_marks`] against catalog `cat`.
pub fn port_marks_in(cat: &Catalog, shape: &Shape, type_id: &str) -> Vec<PortMark> {
    let Some((_, t)) = cat.element_type(type_id) else { return Vec::new() };
    let half = side(shape.style.line_width) / 2.0;
    t.ports
        .iter()
        .take(MAX_MARKS)
        .enumerate()
        .filter_map(|(i, p)| {
            let c = shape.connection_point_world(i)?;
            if !c.is_finite() || c.x.abs() > MAX_EXTENT || c.y.abs() > MAX_EXTENT {
                return None;
            }
            let label = if p.label.is_empty() { p.id.clone() } else { p.label.clone() };
            Some(PortMark { rect: Rect::new(c.x - half, c.y - half, c.x + half, c.y + half), label, direction: p.direction })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in catalog plus a pack whose `stencilports.Block` declares three ports. Kept out
    /// of the global catalog so the other tests here still see only the built-in sets.
    pub(crate) fn ported() -> Catalog {
        use vizcraft_notation::{ElementType, Glyph, Pack};
        let port = |id: &str, label: &str, direction: PortDirection, x: f64, y: f64| Port { id: id.into(), label: label.into(), direction, x, y };
        let pack = Pack {
            schema_version: 1,
            id: "stencilports".into(),
            version: "1.0.0".into(),
            name: "Ports".into(),
            glyphs: vec![Glyph {
                id: "stencilports.block".into(),
                label: "Block".into(),
                category: "Blocks".into(),
                outline: "rectangle".into(),
                w: 100.0,
                h: 60.0,
                element_type: "stencilports.Block".into(),
                ..Glyph::default()
            }],
            element_types: vec![ElementType {
                id: "stencilports.Block".into(),
                glyph: "stencilports.block".into(),
                ports: vec![
                    port("in", "", PortDirection::In, 0.0, 0.5),
                    port("out", "Out", PortDirection::Out, 1.0, 0.5),
                    port("io", "", PortDirection::InOut, 0.5, 7.0),
                ],
                ..ElementType::default()
            }],
            ..Pack::default()
        };
        Catalog::builtin().with_pack(pack).unwrap()
    }

    #[test]
    fn a_typed_master_takes_its_ports_as_connection_points_and_marks_them() {
        let cat = ported();
        let t = "stencilports.Block";
        let want = [Point::new(0.0, 0.5), Point::new(1.0, 0.5), Point::new(0.5, 1.0)];
        assert_eq!(port_points_in(&cat, t).unwrap(), want);
        assert!(port_points_in(&cat, "flowchart.Process").is_none(), "no ports: the kind's points stay");
        let sets = crate::build(&cat);
        let m = sets.iter().flat_map(|s| s.masters.iter()).find(|m| m.id == "stencilports.block").unwrap();
        assert_eq!(m.ports, want);
        let s = m.instantiate("s1", 100.0, 100.0, None, None);
        assert_eq!(s.connection_points, want);
        let marks = port_marks_in(&cat, &s, t);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].rect.center(), Point::new(100.0, 130.0));
        assert_eq!((marks[1].label.as_str(), marks[1].direction), ("Out", PortDirection::Out));
        assert_eq!((marks[2].label.as_str(), marks[2].direction), ("io", PortDirection::InOut));
        assert!(port_marks_in(&cat, &s, "nope.Nothing").is_empty());
        // A shape with fewer points than ports marks only the ports it has.
        let mut few = s.clone();
        few.connection_points.truncate(1);
        assert_eq!(port_marks_in(&cat, &few, t).len(), 1);
    }
}
