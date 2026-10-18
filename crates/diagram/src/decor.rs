//! Page decorations: backgrounds (solid, gradient, subtle grid) and borders with a title block,
//! drawn on a generated background page.
//!
//! A decorated page gets its own background page, marked with [`EXT_FOR`] in its `ext` (the id of
//! the page it decorates) and holding the chosen kinds in [`EXT_BACKGROUND`], [`EXT_COLOR`] and
//! [`EXT_BORDER`]. Its shapes are generated, never edited by hand: [`refresh`] rebuilds them
//! from those settings, the page size, the theme and the titles, and runs after every document
//! change, so the title block follows the document title and the page name, and the frame follows
//! the page size. A page that already had a background page keeps it, under the generated one.
//! Removing both decorations removes the generated page and restores the old background.
//!
//! SVG and PNG export and the canvas draw background pages already, so decorations need nothing
//! of their own there.

use crate::kind::ShapeKind;
use crate::model::{Color, Document, Page, Shape, Style};
use crate::style_fields::{Gradient, GradientKind, GradientStop, HAlign, VAlign};
use crate::theme::{accents, background_color, mix, shade, tint};

/// `ext` key on a generated background page: the id of the page it decorates.
pub const EXT_FOR: &str = "vizcraft.design.for";
/// `ext` key: the background kind's name.
pub const EXT_BACKGROUND: &str = "vizcraft.design.background";
/// `ext` key: the background colour (`#rrggbb`); absent means the theme's.
pub const EXT_COLOR: &str = "vizcraft.design.color";
/// `ext` key: the border kind's name.
pub const EXT_BORDER: &str = "vizcraft.design.border";

/// A page background.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BackgroundKind {
    /// No background. Unknown names load as this.
    #[default]
    None,
    /// One colour.
    Solid,
    /// The colour fading lighter towards the top.
    Gradient,
    /// The colour with a faint grid of lines.
    Pattern,
}

crate::serde_util::named_enum!(BackgroundKind, BackgroundKind::None, {
    BackgroundKind::None => "none",
    BackgroundKind::Solid => "solid",
    BackgroundKind::Gradient => "gradient",
    BackgroundKind::Pattern => "pattern",
});

/// A page border with a title block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderKind {
    /// No border. Unknown names load as this.
    #[default]
    None,
    /// A thin frame inset from the edge, with a boxed title block in the lower right corner.
    Frame,
    /// A coloured band across the top holding the title and page name, and a thin band at the
    /// bottom.
    Banner,
    /// A coloured band down the left edge, with the title and page name in the lower right corner
    /// over a short rule.
    Margin,
}

crate::serde_util::named_enum!(BorderKind, BorderKind::None, {
    BorderKind::None => "none",
    BorderKind::Frame => "frame",
    BorderKind::Banner => "banner",
    BorderKind::Margin => "margin",
});

/// What decorates one page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decoration {
    /// The background.
    pub background: BackgroundKind,
    /// The background colour; `None` takes the theme's.
    pub color: Option<Color>,
    /// The border and title block.
    pub border: BorderKind,
}

impl Decoration {
    fn is_none(&self) -> bool {
        self.background == BackgroundKind::None && self.border == BorderKind::None
    }
    fn of(deco_page: &Page) -> Decoration {
        let get = |k: &str| deco_page.ext.get(k).map(String::as_str).unwrap_or_default();
        Decoration {
            background: BackgroundKind::from_name(get(EXT_BACKGROUND)).unwrap_or_default(),
            color: deco_page.ext.get(EXT_COLOR).and_then(|c| Color::parse(c)),
            border: BorderKind::from_name(get(EXT_BORDER)).unwrap_or_default(),
        }
    }
    fn store(&self, deco_page: &mut Page) {
        deco_page.ext.insert(EXT_BACKGROUND.into(), self.background.name().into());
        deco_page.ext.insert(EXT_BORDER.into(), self.border.name().into());
        match self.color {
            Some(c) => deco_page.ext.insert(EXT_COLOR.into(), c.to_hex()),
            None => deco_page.ext.remove(EXT_COLOR),
        };
    }
}

/// The index of the generated background page decorating the page with id `page_id`.
pub fn generated_page(doc: &Document, page_id: &str) -> Option<usize> {
    doc.pages.iter().position(|p| p.is_background && p.ext.get(EXT_FOR).is_some_and(|f| f == page_id))
}

/// Whether page `p` is a generated decoration page.
pub fn is_generated(p: &Page) -> bool {
    p.is_background && p.ext.contains_key(EXT_FOR)
}

/// The decoration of page `index` (none when it has no generated page).
pub fn decoration(doc: &Document, index: usize) -> Decoration {
    let Some(page) = doc.pages.get(index) else { return Decoration::default() };
    generated_page(doc, &page.id).and_then(|g| doc.pages.get(g)).map(Decoration::of).unwrap_or_default()
}

/// Why a page cannot be decorated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecorError {
    /// No page at that index.
    NoPage,
    /// The page is itself a background page.
    BackgroundPage,
}

impl std::fmt::Display for DecorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DecorError::NoPage => "no such page",
            DecorError::BackgroundPage => "a background page cannot be decorated; choose a foreground page",
        })
    }
}

/// Set page `index`'s decoration: create, update or remove its generated background page and
/// rebuild its shapes.
pub fn set(doc: &mut Document, index: usize, deco: Decoration) -> Result<(), DecorError> {
    let page = doc.pages.get(index).ok_or(DecorError::NoPage)?;
    if page.is_background {
        return Err(DecorError::BackgroundPage);
    }
    let page_id = page.id.clone();
    match (generated_page(doc, &page_id), deco.is_none()) {
        (None, true) => {}
        (Some(g), true) => remove(doc, g),
        (Some(g), false) => {
            if let Some(deco_page) = doc.pages.get_mut(g) {
                deco.store(deco_page);
            }
        }
        (None, false) => {
            let id = doc.next_id("p");
            let name = doc.pages.get(index).map(|p| format!("{} Background", p.name)).unwrap_or_default();
            let mut deco_page = Page::new(id.clone(), name);
            deco_page.is_background = true;
            deco_page.ext.insert(EXT_FOR.into(), page_id);
            deco.store(&mut deco_page);
            if let Some(page) = doc.pages.get_mut(index) {
                // The page's own background (if any) goes under the generated one.
                deco_page.background = page.background.replace(id);
            }
            doc.pages.push(deco_page);
        }
    }
    refresh(doc);
    Ok(())
}

/// Remove generated page `g`, handing its own background back to every page that used it.
fn remove(doc: &mut Document, g: usize) {
    if g >= doc.pages.len() {
        return;
    }
    let deco_page = doc.pages.remove(g);
    for p in &mut doc.pages {
        if p.background.as_deref() == Some(deco_page.id.as_str()) {
            p.background = deco_page.background.clone().filter(|b| *b != p.id);
        }
    }
}

/// Rebuild the shapes of every generated background page from its settings, its page's size and
/// name, the document title and the theme. A generated page whose page is gone is removed. Leaves
/// the document unchanged when everything is already up to date.
pub fn refresh(doc: &mut Document) {
    if !doc.pages.iter().any(is_generated) {
        return;
    }
    let mut i = 0;
    while i < doc.pages.len() {
        let Some(deco_page) = doc.pages.get(i).filter(|p| is_generated(p)) else {
            i += 1;
            continue;
        };
        let for_id = deco_page.ext.get(EXT_FOR).cloned().unwrap_or_default();
        let Some(fg) = doc.pages.iter().find(|p| p.id == for_id && !p.is_background) else {
            remove(doc, i);
            continue;
        };
        let (w, h, name) = (fg.width, fg.height, fg.name.clone());
        let shapes = build(doc, deco_page, w, h, &name);
        if let Some(deco_page) = doc.pages.get_mut(i) {
            if deco_page.width != w || deco_page.height != h {
                deco_page.width = w;
                deco_page.height = h;
            }
            if deco_page.shapes != shapes {
                deco_page.shapes = shapes;
            }
            if !deco_page.connectors.is_empty() {
                deco_page.connectors.clear();
            }
        }
        i += 1;
    }
}

/// The title the title block shows: the title property, else the document title, else
/// "Untitled".
pub fn document_title(doc: &Document) -> &str {
    if !doc.properties.title.trim().is_empty() {
        &doc.properties.title
    } else if !doc.title.trim().is_empty() {
        &doc.title
    } else {
        "Untitled"
    }
}

/// Builds a generated page's shapes with ids unique in the document.
struct Builder<'a> {
    doc: &'a Document,
    gen_id: &'a str,
    shapes: Vec<Shape>,
}

impl Builder<'_> {
    /// `<page>-<role>`, or with a numbered suffix when another page already uses that id.
    fn id(&self, role: &str) -> Option<String> {
        let taken =
            |id: &str| self.doc.pages.iter().any(|p| p.id != self.gen_id && (p.id == id || p.contains_id(id))) || self.doc.model.contains_id(id);
        let base = format!("{}-{role}", self.gen_id);
        std::iter::once(base.clone()).chain((1..8).map(|n| format!("{base}-{n}"))).find(|id| !taken(id))
    }
    fn rect(&mut self, role: &str, (x, y, w, h): (f64, f64, f64, f64), style: Style) -> Option<&mut Shape> {
        let id = self.id(role)?;
        let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, w.max(1.0), h.max(1.0));
        s.style = style;
        s.sanitize();
        self.shapes.push(s);
        self.shapes.last_mut()
    }
    fn fill(&mut self, role: &str, r: (f64, f64, f64, f64), fill: Color) -> Option<&mut Shape> {
        self.rect(role, r, Style { fill, line: Color::TRANSPARENT, line_width: 0.0, ..Style::DEFAULT })
    }
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, role: &str, r: (f64, f64, f64, f64), text: &str, size: f64, bold: bool, color: Color, align: HAlign) {
        let style = Style {
            fill: Color::TRANSPARENT,
            line: Color::TRANSPARENT,
            line_width: 0.0,
            font_size: size,
            font_color: color,
            bold,
            h_align: align,
            v_align: VAlign::Middle,
            ..Style::DEFAULT
        };
        if let Some(s) = self.rect(role, r, style) {
            s.text = text.to_string();
            s.sanitize();
        }
    }
}

/// Most grid lines a pattern background draws each way.
const MAX_GRID_LINES: f64 = 200.0;

fn build(doc: &Document, deco_page: &Page, w: f64, h: f64, page_name: &str) -> Vec<Shape> {
    let deco = Decoration::of(deco_page);
    let mut b = Builder { doc, gen_id: &deco_page.id, shapes: Vec::new() };
    let bg = deco.color.unwrap_or_else(|| background_color(&doc.theme));
    let page = (0.0, 0.0, w, h);
    match deco.background {
        BackgroundKind::None => {}
        BackgroundKind::Solid => {
            b.fill("fill", page, bg);
        }
        BackgroundKind::Gradient => {
            if let Some(s) = b.fill("fill", page, bg) {
                s.style.gradient = Some(Gradient {
                    kind: GradientKind::Linear,
                    angle_deg: 90.0,
                    stops: vec![GradientStop { offset: 0.0, color: tint(bg, 0.7) }, GradientStop { offset: 1.0, color: bg }],
                });
            }
        }
        BackgroundKind::Pattern => {
            b.fill("fill", page, bg);
            let ink = shade(bg, 0.08);
            let step_x = (w / MAX_GRID_LINES).max(40.0);
            let step_y = (h / MAX_GRID_LINES).max(40.0);
            let mut x = step_x;
            let mut n = 0;
            while x < w {
                b.fill(&format!("grid-v{n}"), (x, 0.0, 1.0, h), ink);
                x += step_x;
                n += 1;
            }
            let mut y = step_y;
            n = 0;
            while y < h {
                b.fill(&format!("grid-h{n}"), (0.0, y, w, 1.0), ink);
                y += step_y;
                n += 1;
            }
        }
    }
    let (acc, ink) = accents(&doc.theme);
    let accent = acc.first().copied().unwrap_or(ink);
    let title = document_title(doc).to_string();
    match deco.border {
        BorderKind::None => {}
        BorderKind::Frame => {
            let m = 18.0;
            let line = shade(accent, 0.2);
            b.rect("frame", (m, m, w - 2.0 * m, h - 2.0 * m), Style { fill: Color::TRANSPARENT, line, line_width: 1.5, ..Style::DEFAULT });
            let (bw, bh) = ((w * 0.4).min(300.0), 56.0);
            let (bx, by) = (w - m - bw, h - m - bh);
            b.rect("titleblock", (bx, by, bw, bh), Style { fill: Color::WHITE, line, line_width: 1.5, ..Style::DEFAULT });
            b.text("title", (bx + 8.0, by + 4.0, bw - 16.0, 28.0), &title, 14.0, true, ink, HAlign::Left);
            b.text("pagename", (bx + 8.0, by + 32.0, bw - 16.0, 20.0), page_name, 10.0, false, mix(ink, Color::WHITE, 0.3), HAlign::Left);
        }
        BorderKind::Banner => {
            let band = 54.0;
            b.fill("banner", (0.0, 0.0, w, band), accent);
            b.fill("footer", (0.0, h - 8.0, w, 8.0), accent);
            b.text("title", (24.0, 0.0, w * 0.6, band), &title, 20.0, true, Color::WHITE, HAlign::Left);
            b.text("pagename", (w * 0.6, 0.0, w * 0.4 - 24.0, band), page_name, 12.0, false, tint(accent, 0.8), HAlign::Right);
        }
        BorderKind::Margin => {
            b.fill("margin", (0.0, 0.0, 36.0, h), accent);
            let bw = (w * 0.4).min(300.0);
            let (bx, by) = (w - 24.0 - bw, h - 70.0);
            b.text("title", (bx, by, bw, 26.0), &title, 14.0, true, ink, HAlign::Right);
            b.fill("rule", (bx + bw - 80.0, by + 28.0, 80.0, 2.0), accent);
            b.text("pagename", (bx, by + 32.0, bw, 20.0), page_name, 10.0, false, mix(ink, Color::WHITE, 0.3), HAlign::Right);
        }
    }
    b.shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deco(background: BackgroundKind, border: BorderKind) -> Decoration {
        Decoration { background, color: None, border }
    }

    #[test]
    fn decorating_adds_one_generated_page_and_removing_restores_the_old_background() {
        let mut doc = Document::new();
        let old = doc.add_page("Old background");
        doc.pages[old].is_background = true;
        let old_id = doc.pages[old].id.clone();
        doc.pages[0].background = Some(old_id.clone());
        set(&mut doc, 0, deco(BackgroundKind::Solid, BorderKind::None)).unwrap();
        let g = generated_page(&doc, "p1").unwrap();
        assert_eq!(doc.pages[0].background.as_deref(), Some(doc.pages[g].id.as_str()));
        assert_eq!(doc.pages[g].background.as_deref(), Some(old_id.as_str()));
        assert_eq!(doc.pages[g].shapes.len(), 1);
        set(&mut doc, 0, deco(BackgroundKind::Solid, BorderKind::Frame)).unwrap();
        assert_eq!(doc.pages.len(), 3, "the same generated page is reused");
        assert_eq!(decoration(&doc, 0), deco(BackgroundKind::Solid, BorderKind::Frame));
        set(&mut doc, 0, Decoration::default()).unwrap();
        assert_eq!(doc.pages.len(), 2);
        assert_eq!(doc.pages[0].background.as_deref(), Some(old_id.as_str()));
        assert_eq!(set(&mut doc, old, deco(BackgroundKind::Solid, BorderKind::None)), Err(DecorError::BackgroundPage));
        assert_eq!(set(&mut doc, 99, deco(BackgroundKind::Solid, BorderKind::None)), Err(DecorError::NoPage));
    }

    #[test]
    fn the_title_block_follows_titles_and_the_frame_follows_the_page_size() {
        let mut doc = Document::new();
        set(&mut doc, 0, deco(BackgroundKind::Pattern, BorderKind::Frame)).unwrap();
        let g = generated_page(&doc, "p1").unwrap();
        let text_of = |doc: &Document, role: &str| doc.pages[g].shape(&format!("{}-{role}", doc.pages[g].id)).map(|s| s.text.clone());
        assert_eq!(text_of(&doc, "title").as_deref(), Some("Untitled"));
        doc.properties.title = "Network plan".into();
        doc.pages[0].name = "Floor 2".into();
        doc.pages[0].width = 2000.0;
        refresh(&mut doc);
        assert_eq!(text_of(&doc, "title").as_deref(), Some("Network plan"));
        assert_eq!(text_of(&doc, "pagename").as_deref(), Some("Floor 2"));
        assert_eq!(doc.pages[g].width, 2000.0);
        let settled = doc.clone();
        refresh(&mut doc);
        assert_eq!(doc, settled, "refresh is idempotent");
        // Deleting the decorated page removes its generated page.
        doc.pages.remove(0);
        doc.pages.insert(0, Page::new("p9", "Other"));
        refresh(&mut doc);
        assert!(!doc.pages.iter().any(is_generated));
    }

    #[test]
    fn grid_lines_are_capped_on_huge_pages() {
        let mut doc = Document::new();
        doc.pages[0].width = crate::MAX_EXTENT;
        doc.pages[0].height = crate::MAX_EXTENT;
        set(&mut doc, 0, deco(BackgroundKind::Pattern, BorderKind::Banner)).unwrap();
        let g = generated_page(&doc, "p1").unwrap();
        assert!(doc.pages[g].shapes.len() < 2 * MAX_GRID_LINES as usize + 10);
    }
}
