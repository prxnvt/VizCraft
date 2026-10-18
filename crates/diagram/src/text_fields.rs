//! Text helpers shared by the engine, the canvas and the exports: fields (dynamic text inside
//! shape text), letter case conversion of the stored text, and the stencil of standalone text
//! boxes.
//!
//! **Fields.** A field is a placeholder in the stored text, `{{name}}`, expanded when the text is
//! drawn or exported and kept raw while it is edited, so the stored text never goes stale:
//!
//! | Placeholder | Expands to |
//! | --- | --- |
//! | `{{date}}` | Today, `YYYY-MM-DD` (UTC) |
//! | `{{time}}` | Now, `HH:MM` (UTC) |
//! | `{{pageName}}` | The page's name |
//! | `{{pageNumber}}` | The page's position among the foreground pages, from 1 |
//! | `{{pageCount}}` | How many foreground pages there are |
//! | `{{title}}` | The document title (the title property, else the file title, else "Untitled") |
//! | `{{width}}`, `{{height}}` | The shape's size in the page's units, drawing scale applied |
//! | `{{data:key}}` | The shape's data value for `key`; empty when the shape has none |
//!
//! Names are case sensitive; anything else between braces is left as typed. On a background
//! page the page fields name the foreground page being drawn over it, so a footer on a background
//! page numbers every page. Without a clock (the web build has no `SystemTime`) the date and time
//! fall back to the document's modified stamp.

use std::borrow::Cow;

use crate::MAX_TEXT;
use crate::decor::document_title;
use crate::model::{Document, Shape, truncate};
use crate::snap::unit_size;

/// Stencil of a standalone text box: a rectangle with no fill and no line, holding only text.
pub const TEXT_BOX_STENCIL: &str = "vizcraft.text";

/// Prefix of the `ext` keys that hold a shape's own data values (`vizcraft.data.cost`), read by
/// `{{data:key}}` when the shape is not a view of a model element that has the property.
pub const DATA_EXT_PREFIX: &str = "vizcraft.data.";

/// Placeholder opening and closing marks.
const OPEN: &str = "{{";
const CLOSE: &str = "}}";
/// Longest placeholder looked at, braces included; a longer run is plain text.
const MAX_PLACEHOLDER: usize = 200;

/// A field kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// Today's date.
    Date,
    /// The time now.
    Time,
    /// The page name.
    PageName,
    /// The page number.
    PageNumber,
    /// The number of pages.
    PageCount,
    /// The document title.
    Title,
    /// The shape width.
    Width,
    /// The shape height.
    Height,
    /// A shape data value by key.
    Data(String),
}

/// Every field name with its label, in menu order (`data` takes a key).
pub const FIELDS: &[(&str, &str)] = &[
    ("date", "Date"),
    ("time", "Time"),
    ("pageName", "Page Name"),
    ("pageNumber", "Page Number"),
    ("pageCount", "Page Count"),
    ("title", "Document Title"),
    ("width", "Shape Width"),
    ("height", "Shape Height"),
    ("data", "Shape Data"),
];

impl Field {
    /// The field named `name`, with `key` for `data` (which needs a non-empty key without braces).
    pub fn parse(name: &str, key: Option<&str>) -> Option<Field> {
        Some(match name {
            "date" => Field::Date,
            "time" => Field::Time,
            "pageName" => Field::PageName,
            "pageNumber" => Field::PageNumber,
            "pageCount" => Field::PageCount,
            "title" => Field::Title,
            "width" => Field::Width,
            "height" => Field::Height,
            "data" => {
                let k = key?.trim();
                if k.is_empty() || k.contains('{') || k.contains('}') || k.len() > MAX_PLACEHOLDER - 10 {
                    return None;
                }
                Field::Data(k.to_string())
            }
            _ => return None,
        })
    }

    /// The field inside a placeholder's braces (`pageName`, `data:cost`).
    fn from_inner(inner: &str) -> Option<Field> {
        match inner.split_once(':') {
            Some(("data", key)) => Field::parse("data", Some(key)),
            Some(_) => None,
            None => Field::parse(inner, None),
        }
    }

    /// The placeholder that stands for this field in stored text.
    pub fn placeholder(&self) -> String {
        let name = match self {
            Field::Date => "date",
            Field::Time => "time",
            Field::PageName => "pageName",
            Field::PageNumber => "pageNumber",
            Field::PageCount => "pageCount",
            Field::Title => "title",
            Field::Width => "width",
            Field::Height => "height",
            Field::Data(k) => return format!("{OPEN}data:{k}{CLOSE}"),
        };
        format!("{OPEN}{name}{CLOSE}")
    }
}

/// What fields are expanded against: the document, the page being drawn (its index in
/// `doc.pages`) and the time.
#[derive(Clone, Copy)]
pub struct FieldCtx<'a> {
    /// The document.
    pub doc: &'a Document,
    /// The page the text is drawn on, or the foreground page a background page is drawn under.
    pub page_index: usize,
    /// Seconds since 1970 (UTC), or `None` without a clock.
    pub now_secs: Option<u64>,
}

impl<'a> FieldCtx<'a> {
    /// A context for page `page_index` of `doc` at `now_secs`.
    pub fn new(doc: &'a Document, page_index: usize, now_secs: Option<u64>) -> Self {
        FieldCtx { doc, page_index, now_secs }
    }
}

/// The system clock in seconds since 1970, or `None` where there is none (wasm, a clock before
/// 1970).
pub fn system_now_secs() -> Option<u64> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Whether `text` may hold a field (a cheap check before expanding).
pub fn has_fields(text: &str) -> bool {
    text.contains(OPEN)
}

/// Split `text` into literal runs and recognised placeholders, in order: `(run, Some(field))` for
/// a placeholder, `(run, None)` for literal text.
fn segments(text: &str) -> Vec<(&str, Option<Field>)> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut literal_start = 0usize;
    let mut pos = 0usize;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        let found = after.find(CLOSE).filter(|j| *j + OPEN.len() + CLOSE.len() <= MAX_PLACEHOLDER).and_then(|j| {
            let inner = &after[..j];
            if inner.contains('\n') { None } else { Field::from_inner(inner).map(|f| (j, f)) }
        });
        match found {
            Some((j, field)) => {
                let start = pos + i;
                let end = start + OPEN.len() + j + CLOSE.len();
                if start > literal_start {
                    out.push((&text[literal_start..start], None));
                }
                out.push((&text[start..end], Some(field)));
                literal_start = end;
                pos = end;
                rest = &text[end..];
            }
            None => {
                // Not a field: step past the first brace and look again.
                pos += i + 1;
                rest = &text[pos..];
            }
        }
    }
    if literal_start < text.len() {
        out.push((&text[literal_start..], None));
    }
    out
}

/// `text` with every field expanded for `shape` in `ctx`; borrowed when it holds none. The result
/// is at most [`MAX_TEXT`] bytes.
pub fn expand<'t>(text: &'t str, shape: &Shape, ctx: &FieldCtx<'_>) -> Cow<'t, str> {
    if !has_fields(text) {
        return Cow::Borrowed(text);
    }
    let segs = segments(text);
    if segs.iter().all(|(_, f)| f.is_none()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for (run, field) in segs {
        match field {
            Some(f) => out.push_str(&value(&f, shape, ctx)),
            None => out.push_str(run),
        }
        if out.len() > MAX_TEXT {
            break;
        }
    }
    truncate(&mut out, MAX_TEXT);
    Cow::Owned(out)
}

/// `shape` with its text's fields expanded; borrowed when it has none.
pub fn expand_shape<'s>(shape: &'s Shape, ctx: &FieldCtx<'_>) -> Cow<'s, Shape> {
    match expand(&shape.text, shape, ctx) {
        Cow::Borrowed(_) => Cow::Borrowed(shape),
        Cow::Owned(t) => {
            let mut s = shape.clone();
            s.text = t;
            Cow::Owned(s)
        }
    }
}

/// The value one field shows.
fn value(f: &Field, shape: &Shape, ctx: &FieldCtx<'_>) -> String {
    let doc = ctx.doc;
    let page = doc.pages.get(ctx.page_index);
    match f {
        Field::Date | Field::Time => {
            let stamp = match ctx.now_secs {
                Some(secs) => iso_from_secs(secs),
                // No clock: the stored modified stamp (ISO 8601), when there is one.
                None => doc.core.modified.clone(),
            };
            let (date, time) = (stamp.get(0..10).unwrap_or(""), stamp.get(11..16).unwrap_or(""));
            if *f == Field::Date { date.to_string() } else { time.to_string() }
        }
        Field::PageName => page.map(|p| p.name.clone()).unwrap_or_default(),
        Field::PageNumber => {
            let n = doc.pages.iter().take(ctx.page_index.saturating_add(1)).filter(|p| !p.is_background).count();
            n.max(1).to_string()
        }
        Field::PageCount => doc.pages.iter().filter(|p| !p.is_background).count().max(1).to_string(),
        Field::Title => document_title(doc).to_string(),
        Field::Width | Field::Height => {
            let world = if *f == Field::Width { shape.w } else { shape.h };
            let Some(p) = page else { return number(world) };
            let scale = p.drawing_scale;
            let ratio = scale.drawing / scale.page;
            // The default scale (1 in = 1 in) means no scale at all, whatever the page units.
            let unscaled = scale == crate::DrawingScale::ONE_TO_ONE || ((ratio - 1.0).abs() < 1e-12 && scale.units == p.units);
            let units = if unscaled { p.units } else { scale.units };
            let v = world / unit_size(p.units).max(1e-9) * ratio;
            format!("{} {}", number(v), units_name(units))
        }
        Field::Data(key) => data_value(doc, shape, key).unwrap_or_default(),
    }
}

/// A shape's data value for `key`: the property of the model element it draws, else its Shape Data
/// row, else its own `ext["vizcraft.data.<key>"]`. `None` when none has the key.
pub fn data_value(doc: &Document, shape: &Shape, key: &str) -> Option<String> {
    use vizcraft_model::PropValue;
    let from_element = shape.element.as_deref().and_then(|e| doc.model.element(e)).and_then(|e| e.properties.get(key)).map(|v| match v {
        PropValue::Bool(b) => b.to_string(),
        PropValue::Number(n) => number(*n),
        PropValue::Text(t) => t.clone(),
    });
    // Then the shape's typed data rows (Shape Data), then data kept from other formats.
    from_element
        .or_else(|| shape.data.rows.iter().find(|r| r.key == key && !r.value.is_empty()).map(|r| r.value.clone()))
        .or_else(|| shape.ext.get(&format!("{DATA_EXT_PREFIX}{key}")).cloned())
}

/// A number with at most two decimals and no trailing zeros.
fn number(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn units_name(u: crate::Units) -> &'static str {
    use crate::Units;
    match u {
        Units::Inches => "in",
        Units::Feet => "ft",
        Units::Millimeters => "mm",
        Units::Centimeters => "cm",
        Units::Meters => "m",
        Units::Points => "pt",
        Units::Pixels => "px",
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for seconds since 1970.
pub fn iso_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 to (year, month, day): Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A change of letter case applied to the stored text (unlike `TextCase`, which only changes how
/// text is drawn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseChange {
    /// Lower case with the first letter of every sentence (and line) upper case.
    Sentence,
    /// Every letter lower case.
    Lower,
    /// Every letter upper case.
    Upper,
    /// Upper case letters become lower case and lower case letters upper case.
    Toggle,
    /// The first letter of every word upper case, the rest lower case.
    Title,
}

impl CaseChange {
    /// The change named `name` (`sentence`, `lower`, `upper`, `toggle`, `title`).
    pub fn parse(name: &str) -> Option<CaseChange> {
        Some(match name {
            "sentence" => CaseChange::Sentence,
            "lower" => CaseChange::Lower,
            "upper" => CaseChange::Upper,
            "toggle" => CaseChange::Toggle,
            "title" => CaseChange::Title,
            _ => return None,
        })
    }
}

/// `text` with `change` applied. Field placeholders are kept exactly, so `{{pageName}}` still
/// expands afterwards.
pub fn change_case(text: &str, change: CaseChange) -> String {
    let mut out = String::with_capacity(text.len());
    // Sentence and title state carried across runs: the next letter starts a sentence or word.
    let mut cap_next = true;
    let mut after_stop = false;
    for (run, field) in segments(text) {
        if field.is_some() {
            out.push_str(run);
            cap_next = false;
            after_stop = false;
            continue;
        }
        for ch in run.chars() {
            match change {
                CaseChange::Lower => out.extend(ch.to_lowercase()),
                CaseChange::Upper => out.extend(ch.to_uppercase()),
                CaseChange::Toggle => {
                    if ch.is_uppercase() {
                        out.extend(ch.to_lowercase());
                    } else if ch.is_lowercase() {
                        out.extend(ch.to_uppercase());
                    } else {
                        out.push(ch);
                    }
                }
                CaseChange::Title => {
                    if cap_next && ch.is_alphabetic() {
                        out.extend(ch.to_uppercase());
                    } else {
                        out.extend(ch.to_lowercase());
                    }
                    cap_next = ch.is_whitespace();
                }
                CaseChange::Sentence => {
                    if cap_next && ch.is_alphabetic() {
                        out.extend(ch.to_uppercase());
                        cap_next = false;
                    } else {
                        out.extend(ch.to_lowercase());
                    }
                    if matches!(ch, '.' | '!' | '?') {
                        after_stop = true;
                    } else if ch == '\n' || (ch.is_whitespace() && after_stop) {
                        cap_next = true;
                        after_stop = false;
                    } else if !ch.is_whitespace() {
                        after_stop = false;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Page;

    fn doc() -> Document {
        Document { pages: vec![Page::new("p1", "Plant"), Page::new("p2", "Office")], title: "Site".into(), ..Document::default() }
    }

    fn shape(text: &str) -> Shape {
        Shape { text: text.into(), w: 250.0, h: 50.0, ..Shape::default() }
    }

    #[test]
    fn page_fields_follow_the_page_and_renames() {
        let mut d = doc();
        let s = shape("{{pageName}} {{pageNumber}} of {{pageCount}} in {{title}}");
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 1, None)), "Office 2 of 2 in Site");
        d.pages[1].name = "Annex".into();
        d.properties.title = "Campus".into();
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 1, None)), "Annex 2 of 2 in Campus");
        // A background page is not counted.
        d.pages[0].is_background = true;
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 1, None)), "Annex 1 of 1 in Campus");
    }

    #[test]
    fn date_time_and_size_fields() {
        let mut d = doc();
        let s = shape("{{date}} {{time}} {{width}} x {{height}}");
        // 2024-03-05 14:07:09 UTC.
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 0, Some(1_709_647_629))), "2024-03-05 14:07 2.5 in x 0.5 in");
        d.core.modified = "2020-01-02T03:04:05Z".into();
        d.pages[0].units = crate::Units::Pixels;
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 0, None)), "2020-01-02 03:04 250 px x 50 px");
        d.pages[0].units = crate::Units::Inches;
        d.pages[0].drawing_scale = crate::DrawingScale { page: 1.0, drawing: 10.0, units: crate::Units::Feet };
        assert_eq!(expand("{{width}}", &s, &FieldCtx::new(&d, 0, None)), "25 ft");
    }

    #[test]
    fn data_fields_read_the_element_then_the_shape_and_a_missing_key_is_empty() {
        let d = doc();
        let mut s = shape("Cost: {{data:cost}}; owner: {{data:owner}}.");
        s.ext.insert(format!("{DATA_EXT_PREFIX}cost"), "40".into());
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 0, None)), "Cost: 40; owner: .");
        assert_eq!(data_value(&d, &s, "missing"), None);
    }

    #[test]
    fn unknown_and_broken_placeholders_stay_as_typed() {
        let d = doc();
        let s = shape("{{nope}} {{pageName {{ {{data:}} }} {{pageName}}");
        assert_eq!(expand(&s.text, &s, &FieldCtx::new(&d, 0, None)), "{{nope}} {{pageName {{ {{data:}} }} Plant");
        assert!(matches!(expand("plain", &s, &FieldCtx::new(&d, 0, None)), Cow::Borrowed(_)));
        // Hostile: a page index past the end and multibyte text around braces.
        assert_eq!(expand("\u{e9}{{pageName}}\u{e9}{", &s, &FieldCtx::new(&d, 99, None)), "\u{e9}\u{e9}{");
    }

    #[test]
    fn placeholders_round_trip_through_parse() {
        for (name, _) in FIELDS {
            let f = Field::parse(name, Some("k")).unwrap();
            assert_eq!(segments(&f.placeholder()), vec![(f.placeholder().as_str(), Some(f.clone()))]);
        }
        assert_eq!(Field::parse("data", None), None);
        assert_eq!(Field::parse("data", Some("a}b")), None);
    }

    #[test]
    fn case_conversions() {
        let t = "hello WORLD. this is {{pageName}}!\nnext LINE";
        assert_eq!(change_case(t, CaseChange::Upper), "HELLO WORLD. THIS IS {{pageName}}!\nNEXT LINE");
        assert_eq!(change_case(t, CaseChange::Lower), "hello world. this is {{pageName}}!\nnext line");
        assert_eq!(change_case(t, CaseChange::Sentence), "Hello world. This is {{pageName}}!\nNext line");
        assert_eq!(change_case(t, CaseChange::Title), "Hello World. This Is {{pageName}}!\nNext Line");
        assert_eq!(change_case("aBc {{date}} D", CaseChange::Toggle), "AbC {{date}} d");
        assert_eq!(CaseChange::parse("toggle"), Some(CaseChange::Toggle));
        assert_eq!(CaseChange::parse("Upper"), None);
    }
}
