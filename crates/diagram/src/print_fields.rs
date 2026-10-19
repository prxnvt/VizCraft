//! Fields in header and footer text: `{page}`, `{pages}`, `{pageName}`, `{title}`, `{date}`,
//! `{sheet}` and `{sheets}`, expanded when a page is exported or printed.
//!
//! Field names are matched without regard to case and may carry spaces inside the braces
//! (`{ Page Name }`). `{{` and `}}` write a literal brace. An unknown name, or a brace that is not
//! closed, is left as written, so ordinary text with braces survives. One pass, no recursion: a
//! page name that itself contains `{page}` is not expanded again.

use crate::model::{Document, Page};

/// Longest expansion returned, in bytes; the rest is cut at a character boundary.
pub const MAX_EXPANDED: usize = 4096;

/// The values fields expand to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldValues {
    /// The page's name (`{pageName}`, also `{name}`).
    pub page_name: String,
    /// The page's number, 1 based, among the pages that are not background pages (`{page}`).
    pub page_number: usize,
    /// How many pages are not background pages (`{pages}`, also `{pageCount}`).
    pub page_count: usize,
    /// The document title: the title property, else the document's title (`{title}`).
    pub title: String,
    /// The date, as the caller formats it (`{date}`).
    pub date: String,
    /// The page's sheet number (`{sheet}`; the page number on a page without a sheet).
    pub sheet: u32,
    /// How many sheets the document has (`{sheets}`).
    pub sheets: u32,
}

impl FieldValues {
    /// The values for page `page_index` of `doc`, with `date` as given. A background page counts
    /// its own position among the background pages, so its number is never 0.
    pub fn for_page(doc: &Document, page_index: usize, date: &str) -> Self {
        let page = doc.pages.get(page_index);
        let background = page.is_some_and(|p| p.is_background);
        let same_kind = |p: &Page| p.is_background == background;
        let page_number = doc.pages.iter().take(page_index.saturating_add(1)).filter(|p| same_kind(p)).count().max(1);
        let page_count = doc.pages.iter().filter(|p| same_kind(p)).count().max(1);
        let title = if doc.properties.title.trim().is_empty() { doc.title.clone() } else { doc.properties.title.clone() };
        FieldValues {
            page_name: page.map(|p| p.name.clone()).unwrap_or_default(),
            page_number,
            page_count,
            title,
            date: date.to_string(),
            sheet: crate::sheet::sheet_number(doc, page_index),
            sheets: crate::sheet::sheet_count(doc),
        }
    }

    /// The value of field `name` (already trimmed and lowercased, spaces removed), or `None`.
    fn get(&self, name: &str) -> Option<String> {
        Some(match name {
            "page" | "pagenumber" => self.page_number.to_string(),
            "pages" | "pagecount" | "numpages" => self.page_count.to_string(),
            "pagename" | "name" => self.page_name.clone(),
            "title" => self.title.clone(),
            "date" => self.date.clone(),
            "sheet" => self.sheet.to_string(),
            "sheets" => self.sheets.to_string(),
            _ => return None,
        })
    }
}

/// `text` with every known field replaced by its value.
pub fn expand(text: &str, values: &FieldValues) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        let (before, from) = rest.split_at(i);
        out.push_str(before);
        if let Some(after) = from.strip_prefix("{{") {
            out.push('{');
            rest = after;
        } else if let Some(after) = from.strip_prefix("}}") {
            out.push('}');
            rest = after;
        } else if let Some(body) = from.strip_prefix('{') {
            // Field names are short; a far-away brace is not a field.
            match body.find('}').filter(|&j| j <= 32) {
                Some(j) => {
                    let raw = body.get(..j).unwrap_or_default();
                    let key: String = raw.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
                    match values.get(&key) {
                        Some(v) => out.push_str(&v),
                        None => {
                            out.push('{');
                            out.push_str(raw);
                            out.push('}');
                        }
                    }
                    rest = body.get(j + 1..).unwrap_or_default();
                }
                None => {
                    out.push('{');
                    rest = body;
                }
            }
        } else {
            // A lone `}`.
            out.push('}');
            rest = from.get(1..).unwrap_or_default();
        }
        if out.len() > MAX_EXPANDED {
            break;
        }
    }
    out.push_str(rest);
    if out.len() > MAX_EXPANDED {
        let mut end = MAX_EXPANDED;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> FieldValues {
        FieldValues {
            page_name: "Plant".into(),
            page_number: 2,
            page_count: 5,
            title: "Works".into(),
            date: "2026-10-17".into(),
            sheet: 3,
            sheets: 4,
        }
    }

    #[test]
    fn sheet_fields_expand() {
        assert_eq!(expand("Sheet {sheet} of {Sheets}", &values()), "Sheet 3 of 4");
    }

    #[test]
    fn fields_expand_in_any_case_with_spaces() {
        let v = values();
        assert_eq!(expand("Page {page} of {pages}", &v), "Page 2 of 5");
        assert_eq!(expand("{ Page Name } / {TITLE} / {date}", &v), "Plant / Works / 2026-10-17");
        assert_eq!(expand("{pageNumber}/{pageCount}/{name}", &v), "2/5/Plant");
    }

    #[test]
    fn unknown_fields_escapes_and_stray_braces_are_kept() {
        let v = values();
        assert_eq!(expand("{nope} {{page}} } {", &v), "{nope} {page} } {");
        assert_eq!(expand("{page", &v), "{page");
        assert_eq!(expand("", &v), "");
        assert_eq!(expand("caf\u{e9} {page}", &v), "caf\u{e9} 2");
        // A value holding a field is not expanded again.
        let tricky = FieldValues { page_name: "{page}".into(), ..values() };
        assert_eq!(expand("{pageName}", &tricky), "{page}");
    }

    #[test]
    fn expansion_is_capped() {
        let big = FieldValues { title: "x".repeat(3000), ..values() };
        let out = expand(&"{title}".repeat(100), &big);
        assert!(out.len() <= MAX_EXPANDED);
        let wide = expand(&"\u{e9}{title}".repeat(100), &big);
        assert!(wide.len() <= MAX_EXPANDED);
    }

    #[test]
    fn numbers_skip_background_pages() {
        let mut doc = Document::new();
        let mut bg = Page::new("bg", "Back");
        bg.is_background = true;
        doc.pages.insert(0, bg);
        doc.pages.push(Page::new("p2", "Two"));
        doc.title = "Doc".into();
        let v = FieldValues::for_page(&doc, 2, "d");
        assert_eq!((v.page_number, v.page_count, v.page_name.as_str(), v.title.as_str()), (2, 2, "Two", "Doc"));
        let b = FieldValues::for_page(&doc, 0, "d");
        assert_eq!((b.page_number, b.page_count), (1, 1));
        doc.properties.title = "Prop".into();
        assert_eq!(FieldValues::for_page(&doc, usize::MAX, "").title, "Prop");
    }

    #[test]
    fn header_margin_round_trips_and_is_sanitised() {
        use crate::HeaderFooter;
        let hf = HeaderFooter { center: "{page}".into(), margin: Some(40.0), ..HeaderFooter::default() };
        let text = serde_json::to_string(&hf).unwrap();
        assert_eq!(serde_json::from_str::<HeaderFooter>(&text).unwrap(), hf);
        assert!(!serde_json::to_string(&HeaderFooter::default()).unwrap().contains("margin"));
        let mut junk: HeaderFooter = serde_json::from_str(r#"{"margin": "wide"}"#).unwrap();
        assert_eq!(junk.margin, None);
        junk.margin = Some(f64::NAN);
        junk.sanitize();
        assert_eq!((junk.margin, junk.margin()), (None, HeaderFooter::DEFAULT_MARGIN));
        junk.margin = Some(1e9);
        junk.sanitize();
        assert_eq!(junk.margin, Some(HeaderFooter::MAX_MARGIN));
    }
}
