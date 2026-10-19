//! The rebuild pass: after every command that changed the document, redraw the builder shapes
//! whose inputs changed.
//!
//! A shape is rebuilt when it is new, its stencil or its own parameters changed, its element's
//! type, properties or tag changed, or its page's variant or module changed. Values are written
//! only when they differ, so a second run changes nothing. The engine runs this right before the
//! model sync (`vizcraft_notation::bind::sync`), so binding sees the new ports.

use std::collections::HashMap;

use vizcraft_diagram::vizcraft_model::Element;
use vizcraft_diagram::{Document, Page, Shape};
use vizcraft_notation::Catalog;

use super::{BuildError, apply};

/// The page's symbol module in page units, `None` for the standard's own.
///
/// The page module (`View.module`, `snap::module_step`) comes with the substrate branch; until
/// then every page uses the standard's module. This is the one place that reads it.
pub fn page_module(_page: &Page) -> Option<f64> {
    None
}

/// The page's rendering variant, empty for the default drawing.
pub fn page_variant(page: &Page) -> &str {
    page.view.as_ref().map(|v| v.variant.as_str()).unwrap_or_default()
}

/// Whether a builder may draw `shape`: it carries builder state, or its stencil names a builder
/// glyph.
fn is_built(shape: &Shape, cat: &Catalog) -> bool {
    shape.glyph.as_ref().is_some_and(|g| !g.builder.is_empty())
        || cat.glyph(&shape.stencil).is_some_and(|(_, g)| g.outline == vizcraft_notation::loader::BUILDER_OUTLINE)
}

/// Rebuild `shape` from its current inputs: the element it draws, the catalog, the page's
/// variant and module. When the page has a module the shape moves so its first port lands on
/// the grid. Returns whether anything changed; an unknown builder is an error and changes
/// nothing.
pub fn redraw(shape: &mut Shape, element: Option<&Element>, cat: &Catalog, variant: &str, module: Option<f64>) -> Result<bool, BuildError> {
    let Some(r) = apply::resolve(shape, element, cat, variant) else {
        let name = shape.glyph.as_ref().map(|g| g.builder.clone()).unwrap_or_default();
        return Err(BuildError::Unknown(name));
    };
    let built = super::build(r.spec.name, &r.values(), module)?;
    let mut next = shape.clone();
    if let Some(g) = r.glyph.filter(|g| next.stencil != g.id) {
        next.stencil = g.id.clone();
    }
    apply::apply(&mut next, r.spec.name, &built);
    if let Some(m) = module {
        apply::snap_first_port(&mut next, m);
    }
    if next == *shape {
        return Ok(false);
    }
    *shape = next;
    Ok(true)
}

/// Redraw every builder shape of `doc` whose inputs differ from `before` (see the module docs).
/// Returns the ids of the shapes that changed.
pub fn rebuild(doc: &mut Document, before: &Document, cat: &Catalog) -> Vec<String> {
    let old_shapes: HashMap<&str, &Shape> = before.pages.iter().flat_map(|p| p.shapes.iter()).map(|s| (s.id.as_str(), s)).collect();
    if doc.pages.iter().flat_map(|p| p.shapes.iter()).all(|s| !is_built(s, cat)) {
        return Vec::new();
    }
    let old_pages: HashMap<&str, &Page> = before.pages.iter().map(|p| (p.id.as_str(), p)).collect();
    let old_elements: HashMap<&str, &Element> = before.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let elements: HashMap<&str, &Element> = doc.model.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut changed = Vec::new();
    for page in &mut doc.pages {
        let variant = page_variant(page).to_string();
        let module = page_module(page);
        let page_changed = old_pages.get(page.id.as_str()).is_none_or(|old| page_variant(old) != variant || page_module(old) != module);
        for shape in &mut page.shapes {
            if !is_built(shape, cat) {
                continue;
            }
            let element = shape.element.as_deref().and_then(|e| elements.get(e)).copied();
            let stale = page_changed
                || match old_shapes.get(shape.id.as_str()) {
                    None => true,
                    Some(old) => {
                        old.stencil != shape.stencil
                            || old.glyph.as_ref().map(|g| &g.params) != shape.glyph.as_ref().map(|g| &g.params)
                            || old.element != shape.element
                    }
                }
                || element.is_some_and(|e| {
                    old_elements.get(e.id.as_str()).is_none_or(|o| o.type_id != e.type_id || o.properties != e.properties || o.tag != e.tag)
                });
            if stale && matches!(redraw(shape, element, cat, &variant, module), Ok(true)) {
                changed.push(shape.id.clone());
            }
        }
    }
    changed
}
