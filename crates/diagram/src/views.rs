//! Diagram items as views of the semantic model: which shapes and connectors draw which model
//! items, and keeping those references valid.

use crate::model::Document;

/// Where one model item is drawn on one page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewItems {
    /// Index of the page in `Document::pages`.
    pub page: usize,
    /// Page id.
    pub page_id: String,
    /// Shape or connector ids on that page that reference the item, in z order.
    pub items: Vec<String>,
}

/// Clear every `Shape.element` that names no element of `doc.model` and every
/// `Connector.relationship` that names no relationship, so a damaged or hand-edited file loads
/// with those items as free drawing.
pub fn sanitize_refs(doc: &mut Document) {
    // Contract stub; the c4a-refs piece implements the repair.
    let _ = doc;
}

/// Every page that draws element or relationship `id`, with the items drawing it, in page order;
/// pages that do not draw it are left out.
pub fn views_of(doc: &Document, id: &str) -> Vec<ViewItems> {
    // Contract stub; the c4a-refs piece implements the scan.
    let _ = (doc, id);
    Vec::new()
}

/// The element a shape draws, when `shape` names a bound shape on any page.
pub fn element_of_shape<'a>(doc: &'a Document, shape: &str) -> Option<&'a str> {
    doc.shape(shape).and_then(|s| s.element.as_deref())
}

/// Ids of the shapes and connectors on any page that reference any of `ids` (elements or
/// relationships), in page then z order: the view items to remove when those model items are
/// deleted from the model.
pub fn items_referencing(doc: &Document, ids: &[String]) -> Vec<String> {
    // Contract stub; the c4a-refs piece implements the scan.
    let _ = (doc, ids);
    Vec::new()
}
