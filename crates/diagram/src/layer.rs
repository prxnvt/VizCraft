//! Layer rules: what a shape's layers allow (drawing, selecting, printing, snapping, glue) and
//! the colour a layer paints its shapes with.
//!
//! A shape on no layer is visible, selectable, printable, snappable and gluable. A shape on
//! several layers is visible when any of them is visible and prints when any visible one prints
//! (showing and printing are generous: one layer that shows the shape is enough), but it is
//! locked when any of them is locked and refuses snap or glue when any of them does (restrictions
//! are strict: one layer that forbids is enough). A layer id that names no layer on the page is
//! ignored, as `Page::sanitize` would drop it. The shape's own `behavior` flags (hidden,
//! non-printing, no glue, no snap) count too, so one function answers each question.

use std::borrow::Cow;

use crate::model::{Color, Page, Shape};
use crate::page_fields::Layer;

/// The page layers `shape` names that exist, in the shape's order.
pub fn layers_of<'a>(page: &'a Page, shape: &'a Shape) -> impl Iterator<Item = &'a Layer> + 'a {
    shape.layers.iter().filter_map(|id| page.layer(id))
}

/// Whether `shape` is drawn and can be hit: not hidden by its own flag, and on no layer or on at
/// least one visible layer (the same rule SVG and PNG export use).
pub fn visible(page: &Page, shape: &Shape) -> bool {
    page.shape_visible(shape)
}

/// Whether any of `shape`'s layers is locked.
pub fn locked(page: &Page, shape: &Shape) -> bool {
    layers_of(page, shape).any(|l| l.locked)
}

/// Whether `shape` can be selected, moved, changed or deleted: visible and not locked.
pub fn selectable(page: &Page, shape: &Shape) -> bool {
    visible(page, shape) && !locked(page, shape)
}

/// Whether `shape` goes to the printer: visible, and on no layer or on a visible layer that
/// prints. Export to print leaves out every shape for which this is false.
pub fn printable(page: &Page, shape: &Shape) -> bool {
    if !visible(page, shape) || shape.behavior.non_printing {
        return false;
    }
    let mut layers = layers_of(page, shape).peekable();
    layers.peek().is_none() || layers.any(|l| l.visible && l.print)
}

/// Whether other shapes snap to `shape`: visible and on no layer that turns snapping off.
pub fn snappable(page: &Page, shape: &Shape) -> bool {
    visible(page, shape) && !shape.behavior.no_snap && layers_of(page, shape).all(|l| l.snap)
}

/// Whether connectors may glue to `shape`: visible and on no layer that turns glue off.
pub fn gluable(page: &Page, shape: &Shape) -> bool {
    visible(page, shape) && !shape.behavior.no_glue && layers_of(page, shape).all(|l| l.glue)
}

/// Ids of the shapes on `page` that printing skips (hidden, or on no printing layer), in z order.
pub fn unprinted(page: &Page) -> Vec<String> {
    page.shapes.iter().filter(|s| !printable(page, s)).map(|s| s.id.clone()).collect()
}

/// The colour override for `shape`: the first of its layers (in the shape's order) with a colour.
pub fn color(page: &Page, shape: &Shape) -> Option<Color> {
    layers_of(page, shape).find_map(|l| l.color)
}

/// Mix `a` toward `b` by `t` (0 keeps `a`, 1 is `b`), keeping `a`'s alpha.
fn mix(a: Color, b: Color, t: f64) -> Color {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let ch = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color { r: ch(a.r, b.r), g: ch(a.g, b.g), b: ch(a.b, b.b), a: a.a }
}

/// How far a layer colour pulls a shape's fill toward it; line and text take the colour outright.
pub const FILL_TINT: f64 = 0.5;

/// `shape` as the canvas draws it: unchanged when no layer has a colour, else with the line and
/// text in the layer colour and the fill tinted toward it (a transparent fill stays transparent).
pub fn tinted<'a>(page: &Page, shape: &'a Shape) -> Cow<'a, Shape> {
    let Some(c) = color(page, shape) else { return Cow::Borrowed(shape) };
    let mut s = shape.clone();
    let opaque = Color { a: 255, ..c };
    s.style.line = Color { a: s.style.line.a, ..opaque };
    s.style.font_color = Color { a: s.style.font_color.a, ..opaque };
    s.style.fill = mix(s.style.fill, opaque, FILL_TINT);
    Cow::Owned(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ShapeKind;

    fn page() -> Page {
        let mut p = Page::new("p1", "Page-1");
        p.layers = vec![
            Layer { id: "open".into(), ..Layer::default() },
            Layer { id: "hidden".into(), visible: false, ..Layer::default() },
            Layer { id: "locked".into(), locked: true, ..Layer::default() },
            Layer { id: "noprint".into(), print: false, ..Layer::default() },
            Layer { id: "nosnap".into(), snap: false, glue: false, ..Layer::default() },
            Layer { id: "red".into(), color: Some(Color::rgb(255, 0, 0)), ..Layer::default() },
        ];
        p
    }

    fn on(layers: &[&str]) -> Shape {
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
        s.layers = layers.iter().map(|l| l.to_string()).collect();
        s
    }

    #[test]
    fn no_layer_allows_everything() {
        let p = page();
        let s = on(&[]);
        assert!(visible(&p, &s) && selectable(&p, &s) && printable(&p, &s) && snappable(&p, &s) && gluable(&p, &s));
        assert!(!locked(&p, &s));
        assert_eq!(color(&p, &s), None);
        assert!(matches!(tinted(&p, &s), Cow::Borrowed(_)));
    }

    #[test]
    fn several_layers_show_generously_and_restrict_strictly() {
        let p = page();
        assert!(!visible(&p, &on(&["hidden"])));
        assert!(visible(&p, &on(&["hidden", "open"])), "visible when any layer is");
        assert!(selectable(&p, &on(&["hidden", "open"])));
        assert!(locked(&p, &on(&["open", "locked"])), "locked when any layer is");
        assert!(!selectable(&p, &on(&["open", "locked"])));
        assert!(!selectable(&p, &on(&["hidden"])), "a hidden shape cannot be selected");
        assert!(!printable(&p, &on(&["noprint"])));
        assert!(printable(&p, &on(&["noprint", "open"])));
        assert!(!printable(&p, &on(&["hidden"])));
        assert!(!printable(&p, &on(&["noprint", "hidden"])), "a hidden layer's print flag does not count");
        assert!(!gluable(&p, &on(&["open", "nosnap"])));
        assert!(!snappable(&p, &on(&["nosnap"])));
        assert!(!gluable(&p, &on(&["hidden"])));
        assert!(visible(&p, &on(&["gone"])), "unknown layer ids are ignored");
        let mut s = on(&[]);
        s.behavior.no_glue = true;
        s.behavior.no_snap = true;
        s.behavior.non_printing = true;
        assert!(!gluable(&p, &s) && !snappable(&p, &s) && !printable(&p, &s), "the shape's own flags count");
    }

    #[test]
    fn unprinted_lists_what_print_skips() {
        let mut p = page();
        p.shapes = vec![on(&["noprint"]), on(&["open"]), on(&["hidden"])];
        p.shapes[1].id = "s2".into();
        p.shapes[2].id = "s3".into();
        assert_eq!(unprinted(&p), vec!["s1".to_string(), "s3".to_string()]);
    }

    #[test]
    fn layer_colour_tints_the_shape() {
        let p = page();
        let mut s = on(&["open", "red"]);
        s.style.fill = Color::WHITE;
        s.style.line = Color::BLACK;
        let t = tinted(&p, &s);
        assert_eq!(t.style.line, Color::rgb(255, 0, 0));
        assert_eq!(t.style.font_color, Color::rgb(255, 0, 0));
        assert_eq!(t.style.fill, Color::rgb(255, 128, 128));
        s.style.fill = Color::TRANSPARENT;
        assert_eq!(tinted(&p, &s).style.fill.a, 0, "no fill stays no fill");
        assert_eq!(mix(Color::BLACK, Color::WHITE, f64::NAN), Color::BLACK);
    }
}
