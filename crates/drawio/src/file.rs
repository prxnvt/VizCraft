//! The file container: `<mxfile>` with one `<diagram>` per page, or a bare `<mxGraphModel>`.
//!
//! Pages keep their model as decoded XML text rather than a parsed [`GraphModel`]: listing and
//! fetching pages (the MCP `list_pages` and `get_page` workflows) then never needs a parse, a
//! broken page does not stop the others from opening, and an untouched page is written back
//! exactly as it was read.

use serde::{Deserialize, Serialize};

use crate::attrs::Attributes;
use crate::encoding::{compress_page, decompress_page};
use crate::model::{GraphModel, read_model, write_model};
use crate::xml::{self, Element, write_attr, write_attrs};
use crate::{DrawioError, Result, limits};

/// How the file was stored on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Container {
    /// `<mxfile>` with `<diagram>` pages.
    MxFile,
    /// A bare `<mxGraphModel>` document: one page, no file wrapper.
    BareModel,
}

/// A whole draw.io file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawioFile {
    /// How the file was stored. A bare model with more than one page is written as `<mxfile>`.
    pub container: Container,
    /// The `<mxfile>` attributes (`host`, `modified`, `agent`, `version`, `etag`, `type`, ...)
    /// in document order.
    pub attributes: Attributes,
    /// Pages in order.
    pub pages: Vec<Page>,
}

/// One `<diagram>` page.
///
/// Fields are private so that every change goes through a setter that forgets the original
/// source text; an unchanged page is written back byte for byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    id: Option<String>,
    name: Option<String>,
    attributes: Attributes,
    compressed: bool,
    xml: String,
    /// The original `<diagram>` element text, while the page is unmodified.
    #[serde(skip)]
    source: Option<String>,
}

impl PartialEq for Page {
    // The source text is a cache of the other fields, not part of the page's identity.
    fn eq(&self, other: &Self) -> bool {
        // An empty page has no body to compress, so its flag cannot survive a round trip.
        let flag = |p: &Page| p.compressed && !p.xml.is_empty();
        self.id == other.id && self.name == other.name && self.attributes == other.attributes && flag(self) == flag(other) && self.xml == other.xml
    }
}

impl Page {
    /// A new uncompressed page holding `xml` (an `<mxGraphModel>` document, or empty).
    pub fn new(id: Option<String>, name: Option<String>, xml: String) -> Self {
        Page { id, name, attributes: Attributes::new(), compressed: false, xml, source: None }
    }

    /// The page id.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// The page name (its tab label).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Other `<diagram>` attributes, in order.
    pub fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    /// Whether the page was stored compressed.
    pub fn compressed(&self) -> bool {
        self.compressed
    }

    /// The page's model XML (`<mxGraphModel>...</mxGraphModel>`), or an empty string for an empty
    /// page.
    pub fn xml(&self) -> &str {
        &self.xml
    }

    /// Parses the page's model.
    pub fn model(&self) -> Result<GraphModel> {
        read_model(&self.xml)
    }

    /// Renames the page.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
        self.source = None;
    }

    /// Changes the page id.
    pub fn set_id(&mut self, id: Option<String>) {
        self.id = id;
        self.source = None;
    }

    /// Mutable access to the other `<diagram>` attributes.
    pub fn attributes_mut(&mut self) -> &mut Attributes {
        self.source = None;
        &mut self.attributes
    }

    /// Chooses whether the page is written compressed.
    pub fn set_compressed(&mut self, compressed: bool) {
        if self.compressed != compressed {
            self.compressed = compressed;
            self.source = None;
        }
    }

    /// Replaces the model from XML after checking it parses as an `<mxGraphModel>`. The text is
    /// kept as given (minus a BOM, the XML declaration and surrounding whitespace), so nothing
    /// the IR does not model is lost.
    pub fn set_xml(&mut self, xml: &str) -> Result<()> {
        self.xml = normalize_model_xml(xml)?;
        self.source = None;
        Ok(())
    }

    /// Replaces the model with a serialised [`GraphModel`].
    pub fn set_model(&mut self, model: &GraphModel) {
        self.xml = write_model(model);
        self.source = None;
    }
}

/// A row of [`DrawioFile::list_pages`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    /// Zero-based position.
    pub index: usize,
    /// Page id.
    pub id: Option<String>,
    /// Page name.
    pub name: Option<String>,
    /// Length of the decoded model XML in bytes: an approximate size of the page's content.
    pub xml_bytes: usize,
    /// Whether the page is stored compressed.
    pub compressed: bool,
}

/// Selects a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRef<'a> {
    /// By zero-based position.
    Index(usize),
    /// By name.
    Name(&'a str),
    /// By id.
    Id(&'a str),
}

impl<'a> PageRef<'a> {
    /// Interprets free text the way the MCP tools do: an exact id first, then an exact name, then
    /// a zero-based index. Use [`DrawioFile::find_page`] to resolve it.
    pub fn lookup(file: &DrawioFile, text: &'a str) -> Option<usize> {
        file.find_page(PageRef::Id(text))
            .or_else(|| file.find_page(PageRef::Name(text)))
            .or_else(|| text.trim().parse::<usize>().ok().and_then(|i| file.find_page(PageRef::Index(i))))
    }
}

/// Options for [`write_file_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// `None` writes each page as recorded; `Some(true)` compresses every page; `Some(false)`
    /// writes every page as inline XML.
    pub compress: Option<bool>,
}

impl DrawioFile {
    /// An empty `<mxfile>` with no pages.
    pub fn new() -> Self {
        DrawioFile { container: Container::MxFile, attributes: Attributes::new(), pages: Vec::new() }
    }

    /// One row per page, for page listings.
    pub fn list_pages(&self) -> Vec<PageInfo> {
        self.pages
            .iter()
            .enumerate()
            .map(|(index, p)| PageInfo { index, id: p.id.clone(), name: p.name.clone(), xml_bytes: p.xml.len(), compressed: p.compressed })
            .collect()
    }

    /// The index of the selected page.
    pub fn find_page(&self, sel: PageRef<'_>) -> Option<usize> {
        match sel {
            PageRef::Index(i) => (i < self.pages.len()).then_some(i),
            PageRef::Name(n) => self.pages.iter().position(|p| p.name.as_deref() == Some(n)),
            PageRef::Id(id) => self.pages.iter().position(|p| p.id.as_deref() == Some(id)),
        }
    }

    /// The selected page.
    pub fn page(&self, sel: PageRef<'_>) -> Option<&Page> {
        self.find_page(sel).and_then(|i| self.pages.get(i))
    }

    /// The selected page's model XML.
    pub fn get_page_xml(&self, sel: PageRef<'_>) -> Result<&str> {
        self.page(sel).map(Page::xml).ok_or_else(|| DrawioError::PageNotFound(format!("{sel:?}")))
    }

    /// Replaces the selected page's model from XML. Other pages are untouched and are written
    /// back byte for byte.
    pub fn set_page_xml(&mut self, sel: PageRef<'_>, xml: &str) -> Result<()> {
        let idx = self.find_page(sel).ok_or_else(|| DrawioError::PageNotFound(format!("{sel:?}")))?;
        let page = self.pages.get_mut(idx).ok_or_else(|| DrawioError::PageNotFound(format!("{sel:?}")))?;
        page.set_xml(xml)
    }

    /// Appends a new uncompressed page and returns its index.
    pub fn add_page(&mut self, id: Option<String>, name: Option<String>, xml: &str) -> Result<usize> {
        if self.pages.len() >= limits::MAX_PAGES {
            return Err(DrawioError::Limit(format!("more than {} pages", limits::MAX_PAGES)));
        }
        let xml = normalize_model_xml(xml)?;
        self.pages.push(Page::new(id, name, xml));
        Ok(self.pages.len() - 1)
    }
}

impl Default for DrawioFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Checks that `xml` is a single `<mxGraphModel>` document (or empty) and returns just that
/// element's text.
fn normalize_model_xml(xml: &str) -> Result<String> {
    let xml = xml.trim_start_matches('\u{feff}');
    if xml.trim().is_empty() {
        return Ok(String::new());
    }
    if xml.len() > limits::MAX_PAGE_BYTES {
        return Err(DrawioError::Limit(format!("model larger than {} bytes", limits::MAX_PAGE_BYTES)));
    }
    let root = xml::parse(xml)?;
    crate::model::model_from_element(&root)?;
    xml.get(root.span.0..root.span.1).map(str::to_string).ok_or_else(|| DrawioError::Xml("element span outside the document".into()))
}

/// Reads a `.drawio` file: `<mxfile>` with pages, or a bare `<mxGraphModel>`. Leading and
/// trailing whitespace and a UTF-8 byte order mark are accepted.
pub fn read_file(bytes: &[u8]) -> Result<DrawioFile> {
    if bytes.len() > limits::MAX_FILE_BYTES {
        return Err(DrawioError::Limit(format!("file larger than {} bytes", limits::MAX_FILE_BYTES)));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| DrawioError::Encoding(format!("file is not UTF-8: {e}")))?;
    let text = text.trim_start_matches('\u{feff}');
    let root = xml::parse(text)?;
    match root.name.as_str() {
        "mxfile" => read_mxfile(text, &root),
        "mxGraphModel" => {
            crate::model::model_from_element(&root)?;
            let xml = slice(text, root.span)?.to_string();
            Ok(DrawioFile { container: Container::BareModel, attributes: Attributes::new(), pages: vec![Page::new(None, None, xml)] })
        }
        other => Err(DrawioError::NotDrawio(format!("root element is <{other}>, expected <mxfile> or <mxGraphModel>"))),
    }
}

fn slice(text: &str, span: (usize, usize)) -> Result<&str> {
    text.get(span.0..span.1).ok_or_else(|| DrawioError::Xml("element span outside the document".into()))
}

fn read_mxfile(text: &str, root: &Element) -> Result<DrawioFile> {
    let mut pages = Vec::new();
    // Each page is capped on its own; the total is capped too so many small compressed pages
    // cannot add up to an unbounded allocation.
    let mut decoded_total: usize = 0;
    for d in root.children.iter().filter(|c| c.name == "diagram") {
        if pages.len() >= limits::MAX_PAGES {
            return Err(DrawioError::Limit(format!("more than {} pages", limits::MAX_PAGES)));
        }
        let mut attributes = d.attrs.clone();
        let id = attributes.remove("id");
        let name = attributes.remove("name");
        let (compressed, xml) = match d.child("mxGraphModel") {
            Some(model) => (false, slice(text, model.span)?.to_string()),
            None if d.text.trim().is_empty() => (false, String::new()),
            // The model itself is parsed only on demand, as for inline pages, so one broken page
            // does not stop the rest of the file from opening.
            None => {
                let xml = decompress_page(&d.text)?;
                decoded_total = decoded_total.saturating_add(xml.len());
                if decoded_total > limits::MAX_TOTAL_DECODED_BYTES {
                    return Err(DrawioError::Limit(format!("decompressed pages larger than {} bytes in total", limits::MAX_TOTAL_DECODED_BYTES)));
                }
                (true, xml)
            }
        };
        let source = Some(slice(text, d.span)?.to_string());
        pages.push(Page { id, name, attributes, compressed, xml, source });
    }
    Ok(DrawioFile { container: Container::MxFile, attributes: root.attrs.clone(), pages })
}

/// Writes the file, each page compressed or not as recorded.
pub fn write_file(file: &DrawioFile) -> Vec<u8> {
    write_file_with(file, WriteOptions::default())
}

/// Writes the file with options.
pub fn write_file_with(file: &DrawioFile, options: WriteOptions) -> Vec<u8> {
    let mut out = String::new();
    if file.container == Container::BareModel
        && file.pages.len() == 1
        && options.compress != Some(true)
        && let Some(page) = file.pages.first()
    {
        out.push_str(&page.xml);
        out.push('\n');
        return out.into_bytes();
    }
    out.push_str("<mxfile");
    write_attrs(&mut out, &file.attributes);
    out.push_str(">\n");
    for page in &file.pages {
        let compressed = options.compress.unwrap_or(page.compressed);
        out.push_str("  ");
        match &page.source {
            Some(src) if compressed == page.compressed => out.push_str(src),
            _ => write_page(&mut out, page, compressed),
        }
        out.push('\n');
    }
    out.push_str("</mxfile>\n");
    out.into_bytes()
}

fn write_page(out: &mut String, page: &Page, compressed: bool) {
    out.push_str("<diagram");
    if let Some(id) = &page.id {
        write_attr(out, "id", id);
    }
    if let Some(name) = &page.name {
        write_attr(out, "name", name);
    }
    write_attrs(out, &page.attributes);
    out.push('>');
    if compressed {
        if !page.xml.is_empty() {
            out.push_str(&compress_page(&page.xml));
        }
    } else {
        out.push_str(&page.xml);
    }
    out.push_str("</diagram>");
}
