//! `.drawio` files: the mapping between the `vizcraft-drawio` cell IR and the diagram model.
//!
//! The style mapping is fixed in the Cascade 3 contract (see docs/formats.md). Style keys
//! VizCraft does not model are kept in the items' `ext` maps so a round trip loses nothing:
//!
//! | `ext` key | Holds |
//! |---|---|
//! | `drawio.style` | the style items VizCraft did not map, in source order |
//! | `drawio.wrapper` | `UserObject` or `object` when the cell used that form |
//! | `drawio.attr.<name>` | the wrapper's custom attributes (tooltip, link, user data) |
//! | `drawio.fontStyle` | `fontStyle` bits beyond bold and italic (underline, strike) |
//! | `drawio.model.<name>` | page-level `mxGraphModel` attributes (grid, background, ...) |

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use vizcraft_diagram::{Document, Page};
use vizcraft_stencils::Master;

use crate::{FormatError, Imported};

pub(crate) use read::{edge_look, vertex_look};

/// Most warnings one import keeps, so hostile input cannot produce one per cell.
const MAX_WARNINGS: usize = 200;

/// Warnings with duplicates dropped and a cap, shared by the draw.io and CSV importers.
#[derive(Default)]
pub(crate) struct Warnings {
    list: Vec<String>,
    seen: HashSet<String>,
}

impl Warnings {
    /// Add `w` unless it was already reported or the cap is reached.
    pub(crate) fn push(&mut self, w: impl Into<String>) {
        let w = w.into();
        if self.list.len() > MAX_WARNINGS || self.seen.contains(&w) {
            return;
        }
        if self.list.len() == MAX_WARNINGS {
            self.list.push("further warnings suppressed".into());
            return;
        }
        self.seen.insert(w.clone());
        self.list.push(w);
    }
    /// The warnings in the order they were found.
    pub(crate) fn into_vec(self) -> Vec<String> {
        self.list
    }
}

/// Text cut to a readable length for a warning (a hostile file may hold megabytes in one value).
pub(crate) fn short(t: &str) -> String {
    let mut out: String = t.chars().take(48).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

fn import_error(e: vizcraft_drawio::DrawioError) -> FormatError {
    FormatError::Import(format!("draw.io: {e}"))
}

/// Read a `.drawio` file (an `mxfile` with one or more pages, or a bare `mxGraphModel`) as a
/// document with one page per diagram.
///
/// Ids are made unique across the document (draw.io ids are only unique per page): clashing or
/// unusual ids get fresh ones and glue follows. Page ids are kept when they are plain and free.
pub fn load(bytes: &[u8]) -> Result<Document, FormatError> {
    let imported = import(bytes)?;
    let mut doc = Document::new();
    for (i, src) in imported.pages.into_iter().enumerate() {
        let index = if i == 0 { 0 } else { doc.add_page(&src.name) };
        let (id, name, width, height) = (src.id.clone(), src.name.clone(), src.width, src.height);
        doc.adopt(index, src, true);
        let free = vizcraft_diagram::is_plain_id(&id) && !doc.contains_id(&id);
        if let Some(page) = doc.pages.get_mut(index) {
            if free {
                page.id = id;
            }
            if !name.is_empty() {
                page.name = name;
            }
            page.width = width;
            page.height = height;
        }
    }
    doc.sanitize();
    Ok(doc)
}

/// Write a document as a `.drawio` file, one uncompressed diagram per page.
pub fn save(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let mut file = vizcraft_drawio::DrawioFile::new();
    file.attributes.set("host", "VizCraft");
    for page in &doc.pages {
        let xml = page_xml(page)?;
        file.pages.push(vizcraft_drawio::Page::new(Some(page.id.clone()), Some(page.name.clone()), xml));
    }
    Ok(vizcraft_drawio::write_file_with(&file, vizcraft_drawio::WriteOptions { compress: Some(false) }))
}

/// Read draw.io XML (an `mxfile` or a bare `mxGraphModel`) for import into an existing document.
///
/// Every page comes back with its cells' ids as the file named them; the engine's
/// `Document::adopt` renames any that clash. A page that cannot be parsed is an error when it is
/// the only page and a warning (with an empty page in its place) otherwise.
pub fn import(bytes: &[u8]) -> Result<Imported, FormatError> {
    let file = vizcraft_drawio::read_file(bytes).map_err(import_error)?;
    if file.pages.is_empty() {
        return Err(FormatError::Import("draw.io: the file has no pages".into()));
    }
    let single = file.pages.len() == 1;
    let mut warnings = Warnings::default();
    let mut pages = Vec::with_capacity(file.pages.len());
    for (i, p) in file.pages.iter().enumerate() {
        let id = p.id().unwrap_or_default().to_string();
        let name = p.name().map(str::to_string).unwrap_or_else(|| format!("Page-{}", i.saturating_add(1)));
        match p.model() {
            Ok(model) => pages.push(read::page_from_model(&model, id, name, &mut warnings)),
            Err(e) if single => return Err(import_error(e)),
            Err(e) => {
                warnings.push(format!("page `{name}` could not be read and is left empty: {e}"));
                pages.push(Page::new(id, name));
            }
        }
    }
    Ok(Imported { pages, warnings: warnings.into_vec() })
}

/// One page as a bare `mxGraphModel` XML string.
pub fn page_xml(page: &Page) -> Result<String, FormatError> {
    Ok(vizcraft_drawio::write_model(&write::model_from_page(page)))
}

/// The draw.io style string for a stencil master, so an agent can write XML that opens as that
/// master.
pub fn master_style(master: &Master) -> String {
    let shape = master.instantiate("m", 0.0, 0.0, None, None);
    write::shape_style(&shape).to_style_string()
}
