//! PNG export of one page: the [`svg`](crate::svg) export rasterised with resvg on the CPU.
//! Several pages are several files; the engine names them (`file.export`).
//!
//! Choices:
//! - **One drawing path.** The PNG is the SVG export drawn into pixels, so the two exports never
//!   disagree and there is no second renderer to keep in step with the canvas. Gradients,
//!   shadows, glow and soft edges come through as resvg draws the SVG gradients and blur filters.
//! - **Fonts.** The repository's Inter faces are compiled in and `sans-serif` maps to Inter, so
//!   text looks the same on every machine and no system font is ever read (the web build has none).
//! - **Background.** The page is white, like a printed page and like the SVG export.
//! - **Size.** `scale` is clamped to [`MIN_SCALE`]..[`MAX_SCALE`]; when a side would pass
//!   [`MAX_SIDE`] pixels the scale is lowered so the whole page still fits. The encoded file is
//!   refused above [`MAX_PNG_BYTES`].

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia::{Color, Pixmap, Transform};
use resvg::usvg::{self, fontdb};
use vizcraft_diagram::Document;

use crate::FormatError;
use crate::svg::SvgOptions;

/// Smallest scale accepted; smaller requests are raised to this.
pub const MIN_SCALE: f64 = 0.25;
/// Largest scale accepted; larger requests are lowered to this.
pub const MAX_SCALE: f64 = 4.0;
/// Longest side of the image in pixels.
pub const MAX_SIDE: u32 = 8192;
/// Largest encoded PNG returned, in bytes.
pub const MAX_PNG_BYTES: usize = 48 * 1024 * 1024;

/// The fonts the SVG export names, built once: parsing the faces is cheap but not free, and every
/// render needs the same set.
pub(crate) fn fonts() -> Arc<fontdb::Database> {
    static DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_font_data(include_bytes!("../../../assets/fonts/Inter-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../../../assets/fonts/Inter-Medium.ttf").to_vec());
        db.load_font_data(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf").to_vec());
        db.set_sans_serif_family("Inter");
        Arc::new(db)
    })
    .clone()
}

/// The scale actually used for a page of `w` by `h` units when `scale` was asked for: clamped to
/// the accepted range (a non-finite request means 1), then lowered until both sides fit.
pub fn effective_scale(w: f64, h: f64, scale: f64) -> f64 {
    let s = if scale.is_finite() { scale.clamp(MIN_SCALE, MAX_SCALE) } else { 1.0 };
    let longest = w.max(h);
    if longest.is_finite() && longest > 0.0 && longest * s > f64::from(MAX_SIDE) { f64::from(MAX_SIDE) / longest } else { s }
}

/// Render page `page_index` (0 based) of `doc` at `scale` pixels per page unit. Returns the PNG
/// bytes with the image width and height in pixels.
pub fn render_page(doc: &Document, page_index: usize, scale: f64) -> Result<(Vec<u8>, u32, u32), FormatError> {
    render_page_with(doc, page_index, scale, &SvgOptions::default())
}

/// [`render_page`] with export options (printing, the date fields expand to).
pub fn render_page_with(doc: &Document, page_index: usize, scale: f64, opts: &SvgOptions) -> Result<(Vec<u8>, u32, u32), FormatError> {
    let svg = crate::svg::export_page_with(doc, page_index, opts)?;
    render_svg(&svg, scale)
}

/// Parse SVG bytes from the [`crate::svg`] export with the bundled fonts.
pub(crate) fn parse_svg(svg: &[u8]) -> Result<usvg::Tree, FormatError> {
    let opt = usvg::Options { font_family: "Inter".into(), fontdb: fonts(), ..usvg::Options::default() };
    usvg::Tree::from_data(svg, &opt).map_err(|e| FormatError::Encode(format!("cannot read the page drawing: {e}")))
}

/// Rasterise SVG bytes from [`crate::svg::export_page`] onto a white image.
fn render_svg(svg: &[u8], scale: f64) -> Result<(Vec<u8>, u32, u32), FormatError> {
    let tree = parse_svg(svg)?;
    let size = tree.size();
    let (w, h) = (f64::from(size.width()), f64::from(size.height()));
    let s = effective_scale(w, h, scale);
    // Rounded up so the right and bottom edges are not cut, and at least one pixel each way.
    let px = |v: f64| (v * s).ceil().clamp(1.0, f64::from(MAX_SIDE)) as u32;
    let (pw, ph) = (px(w), px(h));
    let mut pixmap = Pixmap::new(pw, ph).ok_or_else(|| FormatError::Encode(format!("cannot make a {pw} by {ph} image")))?;
    pixmap.fill(Color::WHITE);
    resvg::render(&tree, Transform::from_scale(s as f32, s as f32), &mut pixmap.as_mut());
    let png = pixmap.encode_png().map_err(|e| FormatError::Encode(format!("cannot encode the PNG: {e}")))?;
    if png.len() > MAX_PNG_BYTES {
        return Err(FormatError::TooLarge { len: png.len(), max: MAX_PNG_BYTES });
    }
    Ok((png, pw, ph))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vizcraft_diagram::Page;

    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn header_size(png: &[u8]) -> (u32, u32) {
        let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        (be(16), be(20))
    }

    fn doc_with_page(w: f64, h: f64) -> Document {
        let mut doc = Document::new();
        let mut page = Page::new("p1", "Page-1");
        page.board = vizcraft_diagram::Board::Fixed;
        page.width = w;
        page.height = h;
        doc.pages = vec![page];
        doc
    }

    #[test]
    fn renders_a_page_with_its_size_and_scale() {
        let doc = crate::tests::sample();
        let (png, w, h) = render_page(&doc, 0, 1.0).unwrap();
        assert!(png.starts_with(SIGNATURE));
        assert_eq!(header_size(&png), (w, h));
        assert!(w > 10 && h > 10, "{w} x {h}");
        let (_, w2, h2) = render_page(&doc, 0, 2.0).unwrap();
        assert!(w2.abs_diff(w * 2) <= 2 && h2.abs_diff(h * 2) <= 2, "{w2} x {h2} against {w} x {h}");
        // Not all white: the shapes and their text are drawn.
        let (png, _, _) = render_page(&doc, 0, 1.0).unwrap();
        assert!(png.len() > 2000, "{} bytes looks like an empty page", png.len());
    }

    #[test]
    fn scale_is_clamped_and_sides_are_bounded() {
        assert_eq!(effective_scale(100.0, 100.0, 100.0), MAX_SCALE);
        assert_eq!(effective_scale(100.0, 100.0, 0.0), MIN_SCALE);
        assert_eq!(effective_scale(100.0, 100.0, -3.0), MIN_SCALE);
        assert_eq!(effective_scale(100.0, 100.0, f64::NAN), 1.0);
        assert_eq!(effective_scale(100.0, 100.0, f64::INFINITY), 1.0);
        assert_eq!(effective_scale(20_000.0, 10.0, 1.0), f64::from(MAX_SIDE) / 20_000.0);

        let (png, w, h) = render_page(&doc_with_page(40.0, 20.0), 0, 0.001).unwrap();
        assert_eq!((w, h), (10, 5));
        assert_eq!(header_size(&png), (10, 5));
        let (_, w, h) = render_page(&doc_with_page(100_000.0, 50.0), 0, 4.0).unwrap();
        assert!(w <= MAX_SIDE && h >= 1, "{w} x {h}");
    }

    #[test]
    fn gradient_shadow_dash_and_effects_reach_the_pixels() {
        use vizcraft_diagram::{Dash, Glow, Gradient, GradientStop, HeaderFooter, Shadow, Shape, ShapeKind};
        let mut doc = doc_with_page(200.0, 120.0);
        let mut s = Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0);
        let stops = vec![
            GradientStop { offset: 0.0, color: vizcraft_diagram::Color::WHITE },
            GradientStop { offset: 1.0, color: vizcraft_diagram::Color::rgb(0, 0, 255) },
        ];
        s.style.gradient = Some(Gradient { angle_deg: 90.0, stops, ..Gradient::default() });
        s.style.shadow = Some(Shadow { dx: 30.0, dy: 0.0, blur: 0.0, color: vizcraft_diagram::Color::BLACK });
        s.style.dash = Dash::Dashed;
        s.style.corner_radius = 6.0;
        s.text = "Hi".into();
        s.style.underline = true;
        let mut t = Shape::new("s2", ShapeKind::Ellipse, 150.0, 90.0, 20.0, 20.0);
        t.style.glow = Some(Glow::default());
        t.style.soft_edges = 3.0;
        let page = &mut doc.pages[0];
        page.shapes.extend([s, t]);
        page.footer = HeaderFooter { center: "Footer".into(), ..HeaderFooter::default() };
        let (png, w, _) = render_page(&doc, 0, 1.0).unwrap();
        let px = Pixmap::decode_png(&png).unwrap();
        let at = |x: u32, y: u32| px.pixel(x, y).map(|p| (p.red(), p.green(), p.blue())).unwrap();
        assert!(w >= 200);
        // Right of the shape but inside the hard shadow: black.
        assert_eq!(at(125, 50), (0, 0, 0));
        // The gradient runs from white at the top to blue at the bottom.
        let (top, bottom) = (at(60, 24), at(60, 76));
        assert!(top.2 > 200 && top.0 > 150, "{top:?}");
        assert!(bottom.0 < 60 && bottom.2 > 200, "{bottom:?}");
    }

    #[test]
    fn hostile_pages_are_errors_not_panics() {
        let doc = doc_with_page(100.0, 100.0);
        assert!(matches!(render_page(&doc, 1, 1.0), Err(FormatError::NoPage(1))));
        assert!(render_page(&doc, usize::MAX, 1.0).is_err());
        assert!(render_page(&Document { pages: vec![], ..Document::new() }, 0, 1.0).is_err());
        for (w, h) in [(0.0, 0.0), (-5.0, 10.0), (f64::NAN, f64::INFINITY), (1e300, 1e300)] {
            let r = render_page(&doc_with_page(w, h), 0, 1.0);
            if let Ok((png, pw, ph)) = r {
                assert!(png.starts_with(SIGNATURE) && pw >= 1 && ph >= 1 && pw <= MAX_SIDE && ph <= MAX_SIDE);
            }
        }
        assert!(render_svg(b"not svg", 1.0).is_err());
    }
    #[test]
    fn the_same_drawing_renders_at_the_sheet_size_on_a_fixed_board_and_the_frame_size_on_an_infinite_one() {
        use vizcraft_diagram::{Board, Shape, ShapeKind};
        let doc = |board: Board| {
            let mut doc = doc_with_page(200.0, 120.0);
            doc.pages[0].board = board;
            doc.pages[0].shapes.push(Shape::new("s1", ShapeKind::Rectangle, 10.0, 20.0, 100.0, 60.0));
            doc
        };
        let (fixed, fw, fh) = render_page(&doc(Board::Fixed), 0, 1.0).unwrap();
        assert_eq!((fw, fh, header_size(&fixed)), (200, 120, (200, 120)));
        // The shape spans 100 by 60; the frame adds the 25 unit fit margin on every side.
        let (infinite, iw, ih) = render_page(&doc(Board::Infinite), 0, 1.0).unwrap();
        assert_eq!((iw, ih, header_size(&infinite)), (150, 110, (150, 110)));
        assert_ne!(fixed, infinite);
    }
}
