//! Drawing sheets: a page's paper standard, size, frame, zones (the grid reference system),
//! sheet number and title block kind, as plain data in `Page::sheet`.
//!
//! Two standards are offered and ISO is the default everywhere:
//!
//! | Standard | Sizes | Margins | Zones |
//! |---|---|---|---|
//! | ISO 5457 | A0 to A4 (mm) | left 20 mm, others 10 mm | 50 mm fields counted out from the centring axes; letters run down, numbers across |
//! | ASME Y14.1 | A to E (inches) | [`ASME_MARGINS_IN`] | [`ASME_ZONES`]; numbers run from the right, letters from the bottom |
//!
//! The geometry ([`geometry`]) is in world units (100 per inch) with the trimmed sheet at the page
//! origin, so a page whose size was set by `sheet.set` and its sheet agree. The frame, zone ticks,
//! zone labels, centring marks and title block are drawn on the page's generated decoration page
//! (`crate::decor`), never edited by hand. Everything is clamped: zone counts, numbers and sizes
//! read from a file cannot make the frame allocate without bound.

use serde::{Deserialize, Serialize};

use crate::geom::{Point, Rect};
use crate::model::{Color, Document, Page, Shape, Style};
use crate::page_fields::Orientation;
use crate::serde_util::{is_default, lenient};
use crate::style_fields::{HAlign, VAlign};

/// World units per millimetre (100 per inch).
pub const WORLD_PER_MM: f64 = 100.0 / 25.4;
/// World units per inch.
pub const WORLD_PER_IN: f64 = 100.0;

/// Most zone columns a sheet keeps.
pub const MAX_COLS: u32 = 99;
/// Most zone rows a sheet keeps: the letters without I and O.
pub const MAX_ROWS: u32 = ROW_LETTERS.len() as u32;
/// Largest sheet number kept.
pub const MAX_NUMBER: u32 = 9999;

/// Zone row letters: the alphabet without I and O, which read as 1 and 0.
pub const ROW_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

/// The drawing ink of frames and title blocks: drawings are printed on white paper in one dark
/// colour, whatever the app theme.
pub const INK: Color = Color::rgb(0x1e, 0x26, 0x30);

/// A sheet standard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SheetStandard {
    /// ISO 5457: A sizes in millimetres. Unknown names load as this.
    #[default]
    Iso5457,
    /// ASME Y14.1: A to E sizes in inches.
    AsmeY14_1,
}

crate::serde_util::named_enum!(SheetStandard, SheetStandard::Iso5457, {
    SheetStandard::Iso5457 => "iso5457",
    SheetStandard::AsmeY14_1 => "asmeY14_1",
});

impl SheetStandard {
    /// The short name a preset id uses (`iso`, `asme`).
    pub fn short(self) -> &'static str {
        match self {
            SheetStandard::Iso5457 => "iso",
            SheetStandard::AsmeY14_1 => "asme",
        }
    }
    /// The standard for a short or full name, ignoring case.
    pub fn parse(t: &str) -> Option<SheetStandard> {
        match t.trim().to_ascii_lowercase().as_str() {
            "iso" | "iso5457" | "iso 5457" => Some(SheetStandard::Iso5457),
            "asme" | "asmey14_1" | "asmey14.1" | "asme y14.1" | "ansi" => Some(SheetStandard::AsmeY14_1),
            _ => None,
        }
    }
    /// The display name.
    pub fn label(self) -> &'static str {
        match self {
            SheetStandard::Iso5457 => "ISO 5457",
            SheetStandard::AsmeY14_1 => "ASME Y14.1",
        }
    }
}

/// A sheet size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SheetSize {
    /// ISO A0, 1189 x 841 mm.
    IsoA0,
    /// ISO A1, 841 x 594 mm.
    IsoA1,
    /// ISO A2, 594 x 420 mm.
    IsoA2,
    /// ISO A3, 420 x 297 mm. Unknown names load as this.
    #[default]
    IsoA3,
    /// ISO A4, 297 x 210 mm.
    IsoA4,
    /// ANSI A, 11 x 8.5 in.
    AnsiA,
    /// ANSI B, 17 x 11 in.
    AnsiB,
    /// ANSI C, 22 x 17 in.
    AnsiC,
    /// ANSI D, 34 x 22 in.
    AnsiD,
    /// ANSI E, 44 x 34 in.
    AnsiE,
}

crate::serde_util::named_enum!(SheetSize, SheetSize::IsoA3, {
    SheetSize::IsoA0 => "a0",
    SheetSize::IsoA1 => "a1",
    SheetSize::IsoA2 => "a2",
    SheetSize::IsoA3 => "a3",
    SheetSize::IsoA4 => "a4",
    SheetSize::AnsiA => "a",
    SheetSize::AnsiB => "b",
    SheetSize::AnsiC => "c",
    SheetSize::AnsiD => "d",
    SheetSize::AnsiE => "e",
});

/// One paper size: its dimensions, the standard it belongs to and its default zone counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperSize {
    /// The size.
    pub size: SheetSize,
    /// Display name (`A3`, `ANSI D`); `page.setup {size}` takes it or the size's id.
    pub name: &'static str,
    /// The standard the size belongs to.
    pub standard: SheetStandard,
    /// Long side, in `unit`.
    pub long: f64,
    /// Short side, in `unit`.
    pub short: f64,
    /// Whether the sides are inches (else millimetres).
    pub inches: bool,
    /// Default zone counts, long side by short side.
    pub zones: (u32, u32),
}

impl PaperSize {
    /// Long and short sides in world units.
    pub fn world(&self) -> (f64, f64) {
        let k = if self.inches { WORLD_PER_IN } else { WORLD_PER_MM };
        (self.long * k, self.short * k)
    }
}

/// ASME Y14.1 zone counts (long side by short side), A to E.
///
/// Secondary-source data, to verify against MIL-STD-100G (and the NASA GSFC drafting guide)
/// before the goldens freeze.
pub const ASME_ZONES: [(SheetSize, (u32, u32)); 5] =
    [(SheetSize::AnsiA, (2, 2)), (SheetSize::AnsiB, (4, 2)), (SheetSize::AnsiC, (4, 4)), (SheetSize::AnsiD, (8, 4)), (SheetSize::AnsiE, (8, 8))];

/// ASME Y14.1 margins in inches, (left and right, top and bottom), A to E.
///
/// Secondary-source data, to verify against ASME Y14.1 and MIL-STD-100G before the goldens
/// freeze.
pub const ASME_MARGINS_IN: [(SheetSize, (f64, f64)); 5] = [
    (SheetSize::AnsiA, (0.25, 0.38)),
    (SheetSize::AnsiB, (0.38, 0.25)),
    (SheetSize::AnsiC, (0.5, 0.5)),
    (SheetSize::AnsiD, (0.5, 0.5)),
    (SheetSize::AnsiE, (0.5, 0.5)),
];

/// Every paper size, ISO first.
pub const PAPER_SIZES: [PaperSize; 10] = [
    PaperSize { size: SheetSize::IsoA0, name: "A0", standard: SheetStandard::Iso5457, long: 1189.0, short: 841.0, inches: false, zones: (24, 16) },
    PaperSize { size: SheetSize::IsoA1, name: "A1", standard: SheetStandard::Iso5457, long: 841.0, short: 594.0, inches: false, zones: (16, 12) },
    PaperSize { size: SheetSize::IsoA2, name: "A2", standard: SheetStandard::Iso5457, long: 594.0, short: 420.0, inches: false, zones: (12, 8) },
    PaperSize { size: SheetSize::IsoA3, name: "A3", standard: SheetStandard::Iso5457, long: 420.0, short: 297.0, inches: false, zones: (8, 6) },
    PaperSize { size: SheetSize::IsoA4, name: "A4", standard: SheetStandard::Iso5457, long: 297.0, short: 210.0, inches: false, zones: (6, 4) },
    PaperSize {
        size: SheetSize::AnsiA,
        name: "ANSI A",
        standard: SheetStandard::AsmeY14_1,
        long: 11.0,
        short: 8.5,
        inches: true,
        zones: ASME_ZONES[0].1,
    },
    PaperSize {
        size: SheetSize::AnsiB,
        name: "ANSI B",
        standard: SheetStandard::AsmeY14_1,
        long: 17.0,
        short: 11.0,
        inches: true,
        zones: ASME_ZONES[1].1,
    },
    PaperSize {
        size: SheetSize::AnsiC,
        name: "ANSI C",
        standard: SheetStandard::AsmeY14_1,
        long: 22.0,
        short: 17.0,
        inches: true,
        zones: ASME_ZONES[2].1,
    },
    PaperSize {
        size: SheetSize::AnsiD,
        name: "ANSI D",
        standard: SheetStandard::AsmeY14_1,
        long: 34.0,
        short: 22.0,
        inches: true,
        zones: ASME_ZONES[3].1,
    },
    PaperSize {
        size: SheetSize::AnsiE,
        name: "ANSI E",
        standard: SheetStandard::AsmeY14_1,
        long: 44.0,
        short: 34.0,
        inches: true,
        zones: ASME_ZONES[4].1,
    },
];

impl SheetSize {
    /// The paper this size is.
    pub fn paper(self) -> &'static PaperSize {
        // Every size has a row; the fallback is never taken.
        PAPER_SIZES.iter().find(|p| p.size == self).unwrap_or(&PAPER_SIZES[3])
    }
    /// The size for an id (`a3`, `d`) or a display name (`A3`, `ANSI D`), ignoring case.
    pub fn parse(t: &str) -> Option<SheetSize> {
        let t = t.trim().to_ascii_lowercase();
        let t = t.strip_prefix("ansi ").or_else(|| t.strip_prefix("ansi-")).unwrap_or(&t);
        SheetSize::from_name(t)
    }
}

/// Where zone row letters start counting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LettersFrom {
    /// A at the top (ISO 5457). Unknown names load as this.
    #[default]
    Top,
    /// A at the bottom (ASME, MIL-STD-100G).
    Bottom,
}

crate::serde_util::named_enum!(LettersFrom, LettersFrom::Top, {
    LettersFrom::Top => "top",
    LettersFrom::Bottom => "bottom",
});

/// Where zone column numbers start counting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumbersFrom {
    /// 1 at the left (ISO 5457). Unknown names load as this.
    #[default]
    Left,
    /// 1 at the right (ASME, MIL-STD-100G).
    Right,
}

crate::serde_util::named_enum!(NumbersFrom, NumbersFrom::Left, {
    NumbersFrom::Left => "left",
    NumbersFrom::Right => "right",
});

/// The zone grid: how many columns and rows and which way they count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ZoneScheme {
    /// Columns (numbered), 1 to [`MAX_COLS`].
    #[serde(deserialize_with = "lenient")]
    pub cols: u32,
    /// Rows (lettered), 1 to [`MAX_ROWS`].
    #[serde(deserialize_with = "lenient")]
    pub rows: u32,
    /// Where the letters start.
    #[serde(deserialize_with = "lenient")]
    pub letters_from: LettersFrom,
    /// Where the numbers start.
    #[serde(deserialize_with = "lenient")]
    pub numbers_from: NumbersFrom,
}

impl Default for ZoneScheme {
    fn default() -> Self {
        ZoneScheme { cols: 8, rows: 6, letters_from: LettersFrom::Top, numbers_from: NumbersFrom::Left }
    }
}

/// Which title block the frame carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TitleBlockKind {
    /// The ISO 7200 field set in our own layout. Unknown names load as this.
    #[default]
    Iso7200,
    /// The ASME Y14.1 field set in our own layout.
    Asme,
    /// No title block.
    None,
}

crate::serde_util::named_enum!(TitleBlockKind, TitleBlockKind::Iso7200, {
    TitleBlockKind::Iso7200 => "iso7200",
    TitleBlockKind::Asme => "asme",
    TitleBlockKind::None => "none",
});

/// How revisions are lettered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RevisionScheme {
    /// ASME Y14.35: initial issue `-`, then A to Y without I, O, Q, S, X and Z, then AA.
    AsmeY14_35,
    /// ISO 7200 revision index: A to Z without I and O, then AA. Unknown names load as this.
    #[default]
    Iso7200,
}

crate::serde_util::named_enum!(RevisionScheme, RevisionScheme::Iso7200, {
    RevisionScheme::AsmeY14_35 => "asmeY14_35",
    RevisionScheme::Iso7200 => "iso7200",
});

/// A page's drawing sheet.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sheet {
    /// The standard: margins, zone directions, centring marks.
    #[serde(deserialize_with = "lenient")]
    pub standard: SheetStandard,
    /// The paper size.
    #[serde(deserialize_with = "lenient")]
    pub size: SheetSize,
    /// Landscape (long side across) or portrait.
    #[serde(deserialize_with = "lenient")]
    pub orientation: Orientation,
    /// The zone grid.
    #[serde(deserialize_with = "lenient")]
    pub zones: ZoneScheme,
    /// The sheet number; `None` counts it among the pages with sheets.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// The title block.
    #[serde(deserialize_with = "lenient")]
    pub title_block: TitleBlockKind,
    /// How revisions are lettered.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "is_default")]
    pub revision_scheme: RevisionScheme,
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet::preset(SheetStandard::Iso5457, SheetSize::IsoA3, Orientation::Landscape)
    }
}

impl Sheet {
    /// A sheet of `size` under `standard`, with the standard's zone directions, title block and
    /// revision scheme and the size's default zone counts.
    pub fn preset(standard: SheetStandard, size: SheetSize, orientation: Orientation) -> Sheet {
        let (long, short) = size.paper().zones;
        let (cols, rows) = match orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        };
        let (letters_from, numbers_from, title_block, revision_scheme) = match standard {
            SheetStandard::Iso5457 => (LettersFrom::Top, NumbersFrom::Left, TitleBlockKind::Iso7200, RevisionScheme::Iso7200),
            // Directions per MIL-STD-100G, a secondary source for ASME Y14.1: verify.
            SheetStandard::AsmeY14_1 => (LettersFrom::Bottom, NumbersFrom::Right, TitleBlockKind::Asme, RevisionScheme::AsmeY14_35),
        };
        Sheet {
            standard,
            size,
            orientation,
            zones: ZoneScheme { cols, rows, letters_from, numbers_from },
            number: None,
            title_block,
            revision_scheme,
        }
    }
    /// The preset id `<standard>.<size>.<orientation>` (`iso.a3.landscape`).
    pub fn preset_id(&self) -> String {
        format!("{}.{}.{}", self.standard.short(), self.size.name(), self.orientation.name())
    }
    /// Clamp zone counts and the number into range.
    pub fn sanitize(&mut self) {
        self.zones.cols = self.zones.cols.clamp(1, MAX_COLS);
        self.zones.rows = self.zones.rows.clamp(1, MAX_ROWS);
        self.number = self.number.map(|n| n.clamp(1, MAX_NUMBER));
    }
    /// The trimmed sheet's width and height in world units.
    pub fn world_size(&self) -> (f64, f64) {
        let (long, short) = self.size.paper().world();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }
    /// The page units the sheet is measured in: millimetres for ISO, inches for ASME.
    pub fn units(&self) -> crate::Units {
        match self.standard {
            SheetStandard::Iso5457 => crate::Units::Millimeters,
            SheetStandard::AsmeY14_1 => crate::Units::Inches,
        }
    }
}

/// Parse a preset id `<standard>.<size>.<orientation>` (`iso.a3.landscape`, `asme.d.landscape`).
pub fn parse_preset(id: &str) -> Option<(SheetStandard, SheetSize, Orientation)> {
    let mut parts = id.trim().splitn(3, '.');
    let standard = SheetStandard::parse(parts.next()?)?;
    let size = SheetSize::parse(parts.next()?)?;
    let orientation = Orientation::from_name(&parts.next()?.to_ascii_lowercase())?;
    Some((standard, size, orientation))
}

/// Every preset id, ISO A3 landscape first, then each standard's own sizes in both orientations.
pub fn preset_ids() -> Vec<String> {
    let mut out = vec!["iso.a3.landscape".to_string()];
    for p in &PAPER_SIZES {
        for o in [Orientation::Landscape, Orientation::Portrait] {
            let id = format!("{}.{}.{}", p.standard.short(), p.size.name(), o.name());
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

/// A sheet's geometry in world units, the trimmed sheet at the origin.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetGeometry {
    /// The trimmed sheet.
    pub trimmed: Rect,
    /// The drawing frame.
    pub frame: Rect,
    /// Zone column edges, left to right, from the frame's left to its right (`cols + 1` values).
    pub col_edges: Vec<f64>,
    /// Zone row edges, top to bottom, from the frame's top to its bottom (`rows + 1` values).
    pub row_edges: Vec<f64>,
}

/// Margins (left, top, right, bottom) in world units.
fn margins(sheet: &Sheet) -> (f64, f64, f64, f64) {
    match sheet.standard {
        SheetStandard::Iso5457 => {
            let m = 10.0 * WORLD_PER_MM;
            (20.0 * WORLD_PER_MM, m, m, m)
        }
        SheetStandard::AsmeY14_1 => {
            let (h, v) = ASME_MARGINS_IN.iter().find(|(s, _)| *s == sheet.size).map_or((0.5, 0.5), |(_, m)| *m);
            (h * WORLD_PER_IN, v * WORLD_PER_IN, h * WORLD_PER_IN, v * WORLD_PER_IN)
        }
    }
}

/// ISO 5457 field edges along one side: 50 mm fields counted out from the centring axis of the
/// trimmed sheet (`0..len`), the remainder going to the corner fields, clipped to the frame
/// (`lo..hi`). `None` when that layout does not give `n` fields (custom counts divide evenly).
fn iso_edges(len: f64, lo: f64, hi: f64, n: u32) -> Option<Vec<f64>> {
    let step = 50.0 * WORLD_PER_MM;
    let half = len / 2.0;
    let per_side = (half / step).round().max(1.0) as u32;
    if per_side.checked_mul(2)? != n {
        return None;
    }
    let mut edges = vec![lo];
    for k in 1..per_side {
        edges.push(half - f64::from(per_side - k) * step);
    }
    edges.push(half);
    for k in 1..per_side {
        edges.push(half + f64::from(k) * step);
    }
    edges.push(hi);
    // Every inner edge must fall inside the frame for the fields to make sense.
    if edges.windows(2).all(|w| w.get(1).zip(w.first()).is_some_and(|(b, a)| b > a)) { Some(edges) } else { None }
}

fn even_edges(lo: f64, hi: f64, n: u32) -> Vec<f64> {
    let n = n.max(1);
    (0..=n).map(|i| lo + (hi - lo) * f64::from(i) / f64::from(n)).collect()
}

/// The geometry of `sheet`.
pub fn sheet_geometry(sheet: &Sheet) -> SheetGeometry {
    let mut sheet = *sheet;
    sheet.sanitize();
    let (w, h) = sheet.world_size();
    let (l, t, r, b) = margins(&sheet);
    let frame = Rect::new(l, t, w - r, h - b);
    let z = sheet.zones;
    let iso = sheet.standard == SheetStandard::Iso5457 && sheet.size.paper().standard == SheetStandard::Iso5457;
    let cols = iso.then(|| iso_edges(w, frame.x0, frame.x1, z.cols)).flatten().unwrap_or_else(|| even_edges(frame.x0, frame.x1, z.cols));
    let rows = iso.then(|| iso_edges(h, frame.y0, frame.y1, z.rows)).flatten().unwrap_or_else(|| even_edges(frame.y0, frame.y1, z.rows));
    SheetGeometry { trimmed: Rect::new(0.0, 0.0, w, h), frame, col_edges: cols, row_edges: rows }
}

/// The geometry of page `page`'s sheet, or `None` when it has none.
pub fn geometry(page: &Page) -> Option<SheetGeometry> {
    page.sheet.as_ref().map(sheet_geometry)
}

/// A zone: row letter and column number, shown as `B4`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Zone {
    /// The row letter.
    pub row: String,
    /// The column number, from 1.
    pub col: u32,
}

impl std::fmt::Display for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.row, self.col)
    }
}

/// The letter of row `j` (0 at the top) of `sheet`.
pub fn row_label(sheet: &Sheet, j: u32) -> String {
    let rows = sheet.zones.rows.clamp(1, MAX_ROWS);
    let k = match sheet.zones.letters_from {
        LettersFrom::Top => j,
        LettersFrom::Bottom => rows.saturating_sub(1).saturating_sub(j),
    };
    ROW_LETTERS.get(k as usize).map(|c| char::from(*c).to_string()).unwrap_or_default()
}

/// The number of column `i` (0 at the left) of `sheet`.
pub fn col_label(sheet: &Sheet, i: u32) -> u32 {
    let cols = sheet.zones.cols.clamp(1, MAX_COLS);
    match sheet.zones.numbers_from {
        NumbersFrom::Left => i.saturating_add(1),
        NumbersFrom::Right => cols.saturating_sub(i),
    }
}

/// The index of the band of `edges` holding `v`; the last band includes its far edge.
fn band(edges: &[f64], v: f64) -> Option<u32> {
    let (first, last) = (*edges.first()?, *edges.last()?);
    if !(v >= first && v <= last) {
        return None;
    }
    let n = edges.len().saturating_sub(1);
    let i = edges.iter().skip(1).position(|e| v < *e).unwrap_or(n.saturating_sub(1));
    u32::try_from(i).ok()
}

/// The zone of `sheet` at world point `p`, or `None` outside the frame.
pub fn zone_in(sheet: &Sheet, p: Point) -> Option<Zone> {
    let g = sheet_geometry(sheet);
    let (i, j) = (band(&g.col_edges, p.x)?, band(&g.row_edges, p.y)?);
    Some(Zone { row: row_label(sheet, j), col: col_label(sheet, i) })
}

/// The zone of `page` at world point `p`: `None` outside the frame or with no sheet.
pub fn zone_of(page: &Page, p: Point) -> Option<Zone> {
    zone_in(page.sheet.as_ref()?, p)
}

/// The sheet number of page `index`: its `Sheet::number`, else its position among the foreground
/// pages that have a sheet. A page without a sheet takes its page number.
pub fn sheet_number(doc: &Document, index: usize) -> u32 {
    let Some(page) = doc.pages.get(index) else { return 1 };
    let position = |pred: &dyn Fn(&Page) -> bool| {
        let n = doc.pages.iter().take(index.saturating_add(1)).filter(|p| !p.is_background && pred(p)).count();
        u32::try_from(n.max(1)).unwrap_or(u32::MAX)
    };
    match &page.sheet {
        Some(s) => s.number.unwrap_or_else(|| position(&|p: &Page| p.sheet.is_some())),
        None => position(&|_: &Page| true),
    }
}

/// How many sheets the document has: the foreground pages with a sheet, else the page count.
pub fn sheet_count(doc: &Document) -> u32 {
    let sheets = doc.pages.iter().filter(|p| !p.is_background && p.sheet.is_some()).count();
    let n = if sheets > 0 { sheets } else { doc.pages.iter().filter(|p| !p.is_background).count() };
    u32::try_from(n.max(1)).unwrap_or(u32::MAX)
}

/// ASME Y14.35 revision letters.
const ASME_REV: &[u8] = b"ABCDEFGHJKLMNPRTUVWY";
/// ISO 7200 revision index letters.
const ISO_REV: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

/// The revision after `prev` under `scheme`: from the initial issue (`-` or empty) to `A`, then
/// through the scheme's letters, then two letters (`AA` after the last single one). `None` past
/// the last two-letter revision or when `prev` is not a revision of the scheme.
pub fn next_revision(prev: &str, scheme: RevisionScheme) -> Option<String> {
    let letters = match scheme {
        RevisionScheme::AsmeY14_35 => ASME_REV,
        RevisionScheme::Iso7200 => ISO_REV,
    };
    let prev = prev.trim();
    if prev.is_empty() || prev == "-" {
        return letters.first().map(|c| char::from(*c).to_string());
    }
    let digits: Vec<usize> = prev.bytes().map(|b| letters.iter().position(|l| *l == b.to_ascii_uppercase())).collect::<Option<_>>()?;
    let n = letters.len();
    match digits.as_slice() {
        [a] if a + 1 < n => letters.get(a + 1).map(|c| char::from(*c).to_string()),
        [_] => letters.first().map(|c| format!("{0}{0}", char::from(*c))),
        [a, b] if b + 1 < n => Some(format!("{}{}", char::from(*letters.get(*a)?), char::from(*letters.get(b + 1)?))),
        [a, _] if a + 1 < n => Some(format!("{}{}", char::from(*letters.get(a + 1)?), char::from(*letters.first()?))),
        _ => None,
    }
}

/// The position of a revision in its scheme's order (`-` is 0, `A` is 1), for sorting and the
/// sequence check; `None` when it is not a revision of the scheme.
pub fn revision_rank(rev: &str, scheme: RevisionScheme) -> Option<usize> {
    let letters = match scheme {
        RevisionScheme::AsmeY14_35 => ASME_REV,
        RevisionScheme::Iso7200 => ISO_REV,
    };
    let rev = rev.trim();
    if rev.is_empty() || rev == "-" {
        return Some(0);
    }
    let digits: Vec<usize> = rev.bytes().map(|b| letters.iter().position(|l| *l == b.to_ascii_uppercase())).collect::<Option<_>>()?;
    let n = letters.len();
    match digits.as_slice() {
        [a] => Some(a + 1),
        [a, b] => Some(n + 1 + a * n + b),
        _ => None,
    }
}

/// The context a composed tag reads: the sheet number of the element's first view, the zone of
/// that view's centre and the element's `rung` property. Empty parts when the element is not
/// drawn on a sheet.
pub fn compose_ctx(doc: &Document, element: &str) -> vizcraft_model::identity::ComposeCtx {
    let mut ctx = vizcraft_model::identity::ComposeCtx::default();
    if let Some(e) = doc.model.element(element)
        && let Some(r) = e.prop("rung")
    {
        ctx.rung = r.display();
    }
    if let Some((i, c)) = first_views(doc).remove(element) {
        fill_ctx(doc, i, c, &mut ctx);
    }
    ctx
}

/// [`compose_ctx`] for every element, from one walk over the pages.
pub fn compose_ctxs(doc: &Document) -> std::collections::HashMap<String, vizcraft_model::identity::ComposeCtx> {
    let views = first_views(doc);
    doc.model
        .elements
        .iter()
        .map(|e| {
            let mut ctx =
                vizcraft_model::identity::ComposeCtx { rung: e.prop("rung").map(|r| r.display()).unwrap_or_default(), ..Default::default() };
            if let Some((i, c)) = views.get(e.id.as_str()) {
                fill_ctx(doc, *i, *c, &mut ctx);
            }
            (e.id.clone(), ctx)
        })
        .collect()
}

fn fill_ctx(doc: &Document, page_index: usize, centre: Point, ctx: &mut vizcraft_model::identity::ComposeCtx) {
    ctx.page = sheet_number(doc, page_index).to_string();
    ctx.zone = doc.pages.get(page_index).and_then(|p| zone_of(p, centre)).map(|z| z.to_string()).unwrap_or_default();
}

/// The first view of every element on a foreground page: page index and the view's centre.
pub fn first_views(doc: &Document) -> std::collections::HashMap<&str, (usize, Point)> {
    let mut out = std::collections::HashMap::new();
    for (i, p) in doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
        for s in &p.shapes {
            if let Some(e) = s.element.as_deref() {
                out.entry(e).or_insert((i, s.center()));
            }
        }
    }
    out
}

/// Line widths and character heights of the frame drawing, in millimetres (ISO 5457 values; the
/// ASME frame uses the same).
const FRAME_LINE_MM: f64 = 0.7;
const GRID_LINE_MM: f64 = 0.35;
const CHAR_MM: f64 = 3.5;
/// How far a zone tick runs out from the frame line, and a centring mark into the frame.
const TICK_MM: f64 = 5.0;

/// The font size whose capitals are `mm` tall, in world units (capitals are about 0.7 em).
pub fn font_for_char_height(mm: f64) -> f64 {
    mm * WORLD_PER_MM / 0.7
}

/// The shapes of `sheet`'s frame: the frame line, zone ticks and labels in the border, the
/// centring marks (ISO) and the title block. `id(role)` hands out each shape's id; a role it
/// refuses is left out.
pub fn frame_shapes(sheet: &Sheet, id: &dyn Fn(&str) -> Option<String>) -> Vec<Shape> {
    let g = sheet_geometry(sheet);
    let mut out = Vec::new();
    let line = |role: &str, r: Rect, out: &mut Vec<Shape>| {
        if let Some(id) = id(role) {
            let mut s = Shape::new(id, crate::ShapeKind::Rectangle, r.x0, r.y0, r.width().max(0.5), r.height().max(0.5));
            s.style = Style { fill: INK, line: Color::TRANSPARENT, line_width: 0.0, ..Style::DEFAULT };
            s.sanitize();
            out.push(s);
        }
    };
    let label = |role: &str, r: Rect, text: String, out: &mut Vec<Shape>| {
        if let Some(id) = id(role) {
            let mut s = Shape::new(id, crate::ShapeKind::Rectangle, r.x0, r.y0, r.width().max(1.0), r.height().max(1.0));
            s.style = Style {
                fill: Color::TRANSPARENT,
                line: Color::TRANSPARENT,
                line_width: 0.0,
                font_size: font_for_char_height(CHAR_MM),
                font_color: INK,
                h_align: HAlign::Center,
                v_align: VAlign::Middle,
                ..Style::DEFAULT
            };
            s.text = text;
            s.sanitize();
            out.push(s);
        }
    };
    let f = g.frame;
    if let Some(fid) = id("frame") {
        let mut s = Shape::new(fid, crate::ShapeKind::Rectangle, f.x0, f.y0, f.width().max(1.0), f.height().max(1.0));
        s.style = Style { fill: Color::TRANSPARENT, line: INK, line_width: FRAME_LINE_MM * WORLD_PER_MM, ..Style::DEFAULT };
        s.sanitize();
        out.push(s);
    }
    let t = g.trimmed;
    let grid = GRID_LINE_MM * WORLD_PER_MM;
    let tick = TICK_MM * WORLD_PER_MM;
    // ISO 5457 labels an A4 sheet on the top and right only.
    let a4 = sheet.standard == SheetStandard::Iso5457 && sheet.size == SheetSize::IsoA4;
    let (top_strip, bottom_strip) = (Rect::new(f.x0, t.y0, f.x1, f.y0), Rect::new(f.x0, f.y1, f.x1, t.y1));
    let (left_strip, right_strip) = (Rect::new(t.x0, f.y0, f.x0, f.y1), Rect::new(f.x1, f.y0, t.x1, f.y1));
    let inner_cols = g.col_edges.get(1..g.col_edges.len().saturating_sub(1)).unwrap_or_default();
    for (k, x) in inner_cols.iter().enumerate() {
        line(&format!("tick-t{k}"), Rect::new(x - grid / 2.0, (f.y0 - tick).max(t.y0), x + grid / 2.0, f.y0), &mut out);
        if !a4 {
            line(&format!("tick-b{k}"), Rect::new(x - grid / 2.0, f.y1, x + grid / 2.0, (f.y1 + tick).min(t.y1)), &mut out);
        }
    }
    let inner_rows = g.row_edges.get(1..g.row_edges.len().saturating_sub(1)).unwrap_or_default();
    for (k, y) in inner_rows.iter().enumerate() {
        if !a4 {
            line(&format!("tick-l{k}"), Rect::new((f.x0 - tick).max(t.x0), y - grid / 2.0, f.x0, y + grid / 2.0), &mut out);
        }
        line(&format!("tick-r{k}"), Rect::new(f.x1, y - grid / 2.0, (f.x1 + tick).min(t.x1), y + grid / 2.0), &mut out);
    }
    for (i, w) in g.col_edges.windows(2).enumerate() {
        let (Some(x0), Some(x1)) = (w.first(), w.get(1)) else { continue };
        let n = col_label(sheet, u32::try_from(i).unwrap_or(u32::MAX)).to_string();
        label(&format!("col-t{i}"), Rect::new(*x0, top_strip.y0, *x1, top_strip.y1), n.clone(), &mut out);
        if !a4 {
            label(&format!("col-b{i}"), Rect::new(*x0, bottom_strip.y0, *x1, bottom_strip.y1), n, &mut out);
        }
    }
    for (j, w) in g.row_edges.windows(2).enumerate() {
        let (Some(y0), Some(y1)) = (w.first(), w.get(1)) else { continue };
        let l = row_label(sheet, u32::try_from(j).unwrap_or(u32::MAX));
        // The left strip is wider (filing margin): the letter sits next to the frame.
        let left = Rect::new((left_strip.x1 - (right_strip.width())).max(left_strip.x0), *y0, left_strip.x1, *y1);
        if !a4 {
            label(&format!("row-l{j}"), left, l.clone(), &mut out);
        }
        label(&format!("row-r{j}"), Rect::new(right_strip.x0, *y0, right_strip.x1, *y1), l, &mut out);
    }
    if sheet.standard == SheetStandard::Iso5457 {
        let w = FRAME_LINE_MM * WORLD_PER_MM;
        let (cx, cy) = (t.center().x, t.center().y);
        line("centre-t", Rect::new(cx - w / 2.0, t.y0, cx + w / 2.0, f.y0 + tick), &mut out);
        line("centre-b", Rect::new(cx - w / 2.0, f.y1 - tick, cx + w / 2.0, t.y1), &mut out);
        line("centre-l", Rect::new(t.x0, cy - w / 2.0, f.x0 + tick, cy + w / 2.0), &mut out);
        line("centre-r", Rect::new(f.x1 - tick, cy - w / 2.0, t.x1, cy + w / 2.0), &mut out);
    }
    if let Some(block) = crate::title_block::for_kind(sheet.title_block) {
        let origin = Point::new(f.x1 - block.w_mm * WORLD_PER_MM, f.y1 - block.h_mm * WORLD_PER_MM);
        out.extend(crate::title_block::shapes(&block, origin, WORLD_PER_MM, |role| id(&format!("tb-{role}"))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(sheet: &Sheet) -> (Vec<u32>, Vec<String>) {
        let cols = (0..sheet.zones.cols).map(|i| col_label(sheet, i)).collect();
        let rows = (0..sheet.zones.rows).map(|j| row_label(sheet, j)).collect();
        (cols, rows)
    }

    #[test]
    fn iso_a3_and_a4_zones_count_out_from_the_centre_without_i_or_o() {
        let a3 = Sheet::default();
        assert_eq!((a3.zones.cols, a3.zones.rows), (8, 6));
        let g = sheet_geometry(&a3);
        assert_eq!((g.col_edges.len(), g.row_edges.len()), (9, 7));
        // The centring axis is a field edge, and inner fields are 50 mm.
        let mm = |v: f64| (v / WORLD_PER_MM * 1000.0).round() / 1000.0;
        assert_eq!(mm(g.col_edges[4]), 210.0);
        assert_eq!(mm(g.col_edges[5] - g.col_edges[4]), 50.0);
        assert_eq!(mm(g.frame.x0), 20.0, "the filing margin is on the left");
        assert_eq!(labels(&a3), ((1..=8).collect(), ["A", "B", "C", "D", "E", "F"].map(String::from).to_vec()));
        let a4 = Sheet::preset(SheetStandard::Iso5457, SheetSize::IsoA4, Orientation::Portrait);
        assert_eq!((a4.zones.cols, a4.zones.rows), (4, 6));
        let g = sheet_geometry(&a4);
        assert_eq!((g.col_edges.len(), g.row_edges.len()), (5, 7));
        let a0 = Sheet::preset(SheetStandard::Iso5457, SheetSize::IsoA0, Orientation::Landscape);
        let (_, rows) = labels(&a0);
        assert_eq!(rows.len(), 16);
        assert!(!rows.iter().any(|r| r == "I" || r == "O"), "{rows:?}");
        assert_eq!(rows.last().map(String::as_str), Some("R"));
        // Zones: top left is A1, the point right of the centre one row down is B5.
        let page = Page { sheet: Some(a3), ..Page::default() };
        let g = sheet_geometry(&a3);
        let at = |x: f64, y: f64| zone_of(&page, Point::new(x, y)).map(|z| z.to_string());
        assert_eq!(at(g.frame.x0 + 1.0, g.frame.y0 + 1.0).as_deref(), Some("A1"));
        assert_eq!(at(g.col_edges[4] + 1.0, g.row_edges[1] + 1.0).as_deref(), Some("B5"));
        assert_eq!(at(g.frame.x1, g.frame.y1).as_deref(), Some("F8"), "the far corner is inside");
        assert_eq!(at(1.0, 1.0), None, "the border is outside the frame");
        assert_eq!(zone_of(&Page::default(), Point::new(10.0, 10.0)), None, "no sheet, no zone");
    }

    #[test]
    fn asme_zones_count_from_the_bottom_right() {
        for (size, (long, short)) in ASME_ZONES {
            let s = Sheet::preset(SheetStandard::AsmeY14_1, size, Orientation::Landscape);
            assert_eq!((s.zones.cols, s.zones.rows), (long, short), "{size:?}");
        }
        let d = Sheet::preset(SheetStandard::AsmeY14_1, SheetSize::AnsiD, Orientation::Landscape);
        assert_eq!(d.world_size(), (3400.0, 2200.0));
        let (cols, rows) = labels(&d);
        assert_eq!(cols, vec![8, 7, 6, 5, 4, 3, 2, 1], "numbers run from the right");
        assert_eq!(rows, vec!["D", "C", "B", "A"], "letters run from the bottom");
        let g = sheet_geometry(&d);
        assert_eq!(zone_in(&d, Point::new(g.frame.x1 - 1.0, g.frame.y1 - 1.0)).map(|z| z.to_string()).as_deref(), Some("A1"));
        assert_eq!(zone_in(&d, Point::new(g.frame.x0 + 1.0, g.frame.y0 + 1.0)).map(|z| z.to_string()).as_deref(), Some("D8"));
    }

    #[test]
    fn hostile_zone_counts_are_clamped() {
        let mut s = Sheet::default();
        s.zones.cols = u32::MAX;
        s.zones.rows = 0;
        s.number = Some(0);
        s.sanitize();
        assert_eq!((s.zones.cols, s.zones.rows, s.number), (MAX_COLS, 1, Some(1)));
        let mut wild = Sheet::default();
        wild.zones.rows = 1000;
        let g = sheet_geometry(&wild);
        assert_eq!(g.row_edges.len() as u32, MAX_ROWS + 1);
        assert!(frame_shapes(&wild, &|r| Some(r.to_string())).len() < 1000);
        let back: Sheet = serde_json::from_value(serde_json::json!({"size": "zz", "zones": {"cols": "x", "rows": -4}, "number": "nine"})).unwrap();
        assert_eq!(back.size, SheetSize::IsoA3);
    }

    #[test]
    fn revisions_follow_their_scheme() {
        let mut rev = "-".to_string();
        let mut seen = Vec::new();
        for _ in 0..9 {
            rev = next_revision(&rev, RevisionScheme::AsmeY14_35).unwrap();
            seen.push(rev.clone());
        }
        assert_eq!(seen, ["A", "B", "C", "D", "E", "F", "G", "H", "J"], "I is skipped");
        assert_eq!(next_revision("P", RevisionScheme::AsmeY14_35).as_deref(), Some("R"), "Q and S are skipped");
        assert_eq!(next_revision("Y", RevisionScheme::AsmeY14_35).as_deref(), Some("AA"));
        assert_eq!(next_revision("AY", RevisionScheme::AsmeY14_35).as_deref(), Some("BA"));
        assert_eq!(next_revision("YY", RevisionScheme::AsmeY14_35), None, "two characters at most");
        assert_eq!(next_revision("H", RevisionScheme::Iso7200).as_deref(), Some("J"));
        assert_eq!(next_revision("Z", RevisionScheme::Iso7200).as_deref(), Some("AA"));
        assert_eq!(next_revision("I", RevisionScheme::Iso7200), None);
        assert!(revision_rank("AA", RevisionScheme::AsmeY14_35) > revision_rank("Y", RevisionScheme::AsmeY14_35));
    }

    #[test]
    fn presets_parse_and_list_iso_a3_first() {
        assert_eq!(parse_preset("iso.a3.landscape"), Some((SheetStandard::Iso5457, SheetSize::IsoA3, Orientation::Landscape)));
        assert_eq!(parse_preset("asme.d.portrait"), Some((SheetStandard::AsmeY14_1, SheetSize::AnsiD, Orientation::Portrait)));
        assert_eq!(parse_preset("iso.a9.landscape"), None);
        assert_eq!(parse_preset("x"), None);
        let ids = preset_ids();
        assert_eq!(ids.first().map(String::as_str), Some("iso.a3.landscape"));
        assert_eq!(ids.len(), 20);
        assert_eq!(Sheet::default().preset_id(), "iso.a3.landscape");
    }

    #[test]
    fn sheet_numbers_count_sheet_pages_and_fall_back_to_page_numbers() {
        let mut doc = Document::new();
        doc.add_page("Two");
        doc.add_page("Three");
        assert_eq!((sheet_number(&doc, 2), sheet_count(&doc)), (3, 3), "no sheets: page numbers");
        doc.pages[1].sheet = Some(Sheet::default());
        doc.pages[2].sheet = Some(Sheet::default());
        assert_eq!((sheet_number(&doc, 1), sheet_number(&doc, 2), sheet_count(&doc)), (1, 2, 2));
        doc.pages[2].sheet = Some(Sheet { number: Some(7), ..Sheet::default() });
        assert_eq!(sheet_number(&doc, 2), 7);
        assert_eq!(sheet_number(&doc, 0), 1, "a page without a sheet keeps its page number");
    }

    #[test]
    fn a_sheet_keeps_a_generated_frame_page_whose_title_block_shows_the_properties() {
        use crate::decor;
        use crate::text_fields::{FieldCtx, expand_shape};
        let mut doc = Document::new();
        let before_plain = doc.clone();
        decor::refresh(&mut doc);
        assert_eq!(doc, before_plain, "no sheet, no decoration: nothing changes");
        doc.pages[0].sheet = Some(Sheet::default());
        decor::set(&mut doc, 0, decor::Decoration::default()).unwrap();
        let g = decor::generated_page(&doc, "p1").expect("the sheet keeps a generated page");
        let gid = doc.pages[g].id.clone();
        assert!(doc.pages[g].shape(&format!("{gid}-frame")).is_some());
        assert!(doc.pages[g].shape(&format!("{gid}-centre-t")).is_some(), "ISO centring marks");
        let title = doc.pages[g].shapes.iter().find(|s| s.text == "{{doc:title}}").cloned().expect("a title slot");
        doc.properties.title = "Pump skid".into();
        doc.properties.fields.insert("legalOwner".into(), "Works Ltd".into());
        let ctx = FieldCtx::new(&doc, 0, None);
        assert_eq!(expand_shape(&title, &ctx).text, "Pump skid");
        let owner = doc.pages[g].shapes.iter().find(|s| s.text == "{{doc:legalOwner}}").cloned().unwrap();
        assert_eq!(expand_shape(&owner, &ctx).text, "Works Ltd");
        let settled = doc.clone();
        decor::refresh(&mut doc);
        assert_eq!(doc, settled, "refresh is idempotent");
        // Turning the sheet off with no decoration removes the generated page.
        doc.pages[0].sheet = None;
        decor::refresh(&mut doc);
        assert!(decor::generated_page(&doc, "p1").is_none());
        assert_eq!(doc.pages.len(), 1);
    }

    #[test]
    fn zone_sheet_and_doc_fields_expand() {
        use crate::text_fields::{FieldCtx, expand_shape};
        let mut doc = Document::new();
        doc.pages[0].sheet = Some(Sheet::default());
        let g = sheet_geometry(&Sheet::default());
        let x = g.col_edges[2] + 5.0;
        let y = g.row_edges[1] + 5.0;
        let mut s = Shape::new("s", crate::ShapeKind::Rectangle, x - 5.0, y - 5.0, 10.0, 10.0);
        s.text = "{{zone}} {{sheet}}/{{sheets}} {{doc:revisionIndex}} {{doc:paperSize}}".into();
        let ctx = FieldCtx::new(&doc, 0, None);
        assert_eq!(expand_shape(&s, &ctx).text, "B3 1/1  A3");
    }
}
