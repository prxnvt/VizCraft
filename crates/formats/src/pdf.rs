//! PDF export and print layout: the [`svg`](crate::svg) export of each page drawn as vector PDF
//! pages through krilla and krilla-svg.
//!
//! Choices:
//! - **One drawing path.** Like PNG, the PDF is built from the SVG export (parsed by usvg with the
//!   bundled Inter faces), so SVG, PNG and PDF never disagree. Paths stay vectors; blur filters
//!   (shadows, glow, soft edges) are rasterised by krilla-svg inside the PDF.
//! - **Fonts.** Text is real PDF text in subsets of the bundled Inter faces, embedded in the file;
//!   no system font is read, so the PDF looks the same on every machine and in the web build.
//! - **Units.** A page unit is a hundredth of an inch and a PDF point is a seventy-second, so a
//!   unit is 0.72 points: the default 1100 by 850 page is a US Letter landscape sheet. A page
//!   larger than [`MAX_SIDE_PT`] (the PDF limit of 200 inches) is scaled down to fit it.
//! - **Export** ([`export_pages`]): one PDF page per document page, each the size of the SVG
//!   canvas (the page, grown when content reaches past it).
//! - **Print** ([`print_pages`]): every document page is laid out on paper sheets by [`plan`]:
//!   at full size and tiled across as many sheets as it needs (the default), shrunk or grown to
//!   one sheet (`fit`), or spread over a given grid of sheets (`tiles`). The drawing is centred in
//!   the grid. Printing leaves out what does not print, and the header and footer are drawn once,
//!   where they fall on the page, not on every sheet.
//! - **Limits.** At most [`MAX_TILES`] sheets across or down a page and [`MAX_SHEETS`] in one
//!   file; a layout that needs more is scaled down to the cap. The file is refused above
//!   [`MAX_PDF_BYTES`].

use krilla::Document as PdfDocument;
use krilla::geom::{Size, Transform};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla_svg::{SurfaceExt, SvgSettings};
use vizcraft_diagram::Document;

use crate::FormatError;
use crate::svg::{SvgOptions, canvas_rect_with};

/// Points per page unit (72 points per inch, 100 units per inch).
pub const PT_PER_UNIT: f64 = 0.72;
/// Longest page side written, in points (the PDF 1.7 limit, 200 inches).
pub const MAX_SIDE_PT: f64 = 14_400.0;
/// Most sheets across, or down, one page when tiling.
pub const MAX_TILES: u32 = 16;
/// Most PDF pages in one file.
pub const MAX_SHEETS: usize = 512;
/// Largest PDF returned, in bytes.
pub const MAX_PDF_BYTES: usize = 128 * 1024 * 1024;

/// How pages are laid out on paper for printing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrintLayout {
    /// Paper width and height in page units; `None` uses each page's own size.
    pub paper: Option<(f64, f64)>,
    /// Shrink or grow each page's drawing to one sheet.
    pub fit: bool,
    /// Spread each page's drawing over this many sheets across and down (ignored with `fit`).
    pub tiles: Option<(u32, u32)>,
}

/// Where one page's drawing goes on paper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Sheets across.
    pub cols: u32,
    /// Sheets down.
    pub rows: u32,
    /// Drawing scale (1 is full size).
    pub scale: f64,
    /// Paper width in page units.
    pub paper_w: f64,
    /// Paper height in page units.
    pub paper_h: f64,
    /// Drawing width in page units, before scaling (the canvas: page plus overhanging content).
    pub drawing_w: f64,
    /// Drawing height in page units, before scaling.
    pub drawing_h: f64,
}

impl Plan {
    /// Sheets this page prints on.
    pub fn sheets(&self) -> usize {
        (self.cols as usize).saturating_mul(self.rows as usize)
    }
}

/// A positive finite side, else `fallback`, capped at the model's largest extent.
fn side(v: f64, fallback: f64) -> f64 {
    if v.is_finite() && v >= 1.0 { v.min(vizcraft_diagram::MAX_EXTENT) } else { fallback }
}

/// How page `page_index` of `doc` lies on paper under `layout`, counting only printable shapes
/// when `print` is set.
pub fn plan(doc: &Document, page_index: usize, layout: &PrintLayout, print: bool) -> Result<Plan, FormatError> {
    let page = doc.pages.get(page_index).ok_or(FormatError::NoPage(page_index))?;
    let r = canvas_rect_with(page, print);
    let (w, h) = (side(r.width(), 1.0), side(r.height(), 1.0));
    let (pw, ph) = layout.paper.unwrap_or((page.width, page.height));
    let (pw, ph) = (side(pw, side(page.width, 1100.0)), side(ph, side(page.height, 850.0)));
    let fit_into = |cols: u32, rows: u32| (f64::from(cols) * pw / w).min(f64::from(rows) * ph / h);
    let (cols, rows, scale) = if layout.fit {
        (1, 1, fit_into(1, 1))
    } else if let Some((c, r)) = layout.tiles {
        let (c, r) = (c.clamp(1, MAX_TILES), r.clamp(1, MAX_TILES));
        (c, r, fit_into(c, r))
    } else {
        // Full size; a hair of slack so float noise on an exact fit does not add a sheet.
        let need = |d: f64, p: f64| ((d / p) - 1e-6).ceil().max(1.0);
        let (c, r) = (need(w, pw), need(h, ph));
        let cap = f64::from(MAX_TILES);
        if c > cap || r > cap {
            (MAX_TILES.min(c as u32), MAX_TILES.min(r as u32), fit_into(MAX_TILES, MAX_TILES).min(1.0))
        } else {
            (c as u32, r as u32, 1.0)
        }
    };
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    Ok(Plan { cols, rows, scale, paper_w: pw, paper_h: ph, drawing_w: w, drawing_h: h })
}

/// The PDF of `pages` (0-based indices, in order) of `doc`, one PDF page per document page.
pub fn export_pages(doc: &Document, pages: &[usize], opts: &SvgOptions) -> Result<Vec<u8>, FormatError> {
    check_count(pages.len())?;
    let mut pdf = new_pdf(doc);
    for &i in pages {
        let tree = crate::png::parse_svg(&crate::svg::export_page_with(doc, i, opts)?)?;
        let (w, h) = (f64::from(tree.size().width()), f64::from(tree.size().height()));
        let k = PT_PER_UNIT.min(MAX_SIDE_PT / w.max(h).max(1.0));
        let size = page_size(w * k, h * k)?;
        let mut page = pdf.start_page_with(PageSettings::new(size));
        let mut surface = page.surface();
        surface.draw_svg(&tree, size, SvgSettings::default());
        surface.finish();
        page.finish();
    }
    finish(pdf)
}

/// What [`print_pages`] made: the PDF and, for each document page printed, its plan.
#[derive(Clone, Debug)]
pub struct Printed {
    /// The print-ready PDF.
    pub pdf: Vec<u8>,
    /// (page index, plan) for every page, in order; the PDF holds their sheets in that order,
    /// row by row.
    pub plans: Vec<(usize, Plan)>,
}

impl Printed {
    /// Paper sheets in the PDF.
    pub fn sheets(&self) -> usize {
        self.plans.iter().map(|(_, p)| p.sheets()).fold(0usize, usize::saturating_add)
    }
}

/// The print-ready PDF of `pages` of `doc` laid out by `layout`. `opts.print` is normally set so
/// non-printing shapes are left out.
pub fn print_pages(doc: &Document, pages: &[usize], opts: &SvgOptions, layout: &PrintLayout) -> Result<Printed, FormatError> {
    let mut plans = Vec::with_capacity(pages.len().min(MAX_SHEETS));
    let mut total = 0usize;
    for &i in pages {
        let p = plan(doc, i, layout, opts.print)?;
        total = total.saturating_add(p.sheets());
        check_count(total)?;
        plans.push((i, p));
    }
    let mut pdf = new_pdf(doc);
    for &(i, p) in &plans {
        let tree = crate::png::parse_svg(&crate::svg::export_page_with(doc, i, opts)?)?;
        let k = PT_PER_UNIT.min(MAX_SIDE_PT / p.paper_w.max(p.paper_h));
        let size = page_size(p.paper_w * k, p.paper_h * k)?;
        let tree_size = page_size(f64::from(tree.size().width()), f64::from(tree.size().height()))?;
        // The drawing centred in the grid of sheets.
        let ox = (f64::from(p.cols) * p.paper_w - p.drawing_w * p.scale) / 2.0;
        let oy = (f64::from(p.rows) * p.paper_h - p.drawing_h * p.scale) / 2.0;
        for row in 0..p.rows {
            for col in 0..p.cols {
                let tx = (ox - f64::from(col) * p.paper_w) * k;
                let ty = (oy - f64::from(row) * p.paper_h) * k;
                let s = (p.scale * k) as f32;
                let mut page = pdf.start_page_with(PageSettings::new(size));
                let mut surface = page.surface();
                surface.push_transform(&Transform::from_row(s, 0.0, 0.0, s, tx as f32, ty as f32));
                surface.draw_svg(&tree, tree_size, SvgSettings::default());
                surface.pop();
                surface.finish();
                page.finish();
            }
        }
    }
    Ok(Printed { pdf: finish(pdf)?, plans })
}

/// An empty PDF with the document's title and VizCraft as the creator.
fn new_pdf(doc: &Document) -> PdfDocument {
    let mut pdf = PdfDocument::new();
    let title = if doc.properties.title.trim().is_empty() { doc.title.trim() } else { doc.properties.title.trim() };
    let mut meta = Metadata::new().creator("VizCraft".into());
    if !title.is_empty() {
        meta = meta.title(title.to_string());
    }
    if !doc.properties.author.trim().is_empty() {
        meta = meta.authors(vec![doc.properties.author.trim().to_string()]);
    }
    pdf.set_metadata(meta);
    pdf
}

/// A page size in points, clamped to what PDF allows.
fn page_size(w: f64, h: f64) -> Result<Size, FormatError> {
    let c = |v: f64| if v.is_finite() { v.clamp(1.0, MAX_SIDE_PT) as f32 } else { 1.0 };
    Size::from_wh(c(w), c(h)).ok_or_else(|| FormatError::Encode(format!("cannot make a {w} by {h} PDF page")))
}

/// Refuse no pages, and more than [`MAX_SHEETS`].
fn check_count(n: usize) -> Result<(), FormatError> {
    if n == 0 {
        return Err(FormatError::Encode("no pages to write".into()));
    }
    if n > MAX_SHEETS {
        return Err(FormatError::Encode(format!("{n} PDF pages is more than the {MAX_SHEETS} one file may hold")));
    }
    Ok(())
}

fn finish(pdf: PdfDocument) -> Result<Vec<u8>, FormatError> {
    let bytes = pdf.finish().map_err(|e| FormatError::Encode(format!("cannot write the PDF: {e:?}")))?;
    if bytes.len() > MAX_PDF_BYTES {
        return Err(FormatError::TooLarge { len: bytes.len(), max: MAX_PDF_BYTES });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
