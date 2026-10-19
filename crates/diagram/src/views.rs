//! Diagram items as views of the semantic model: which shapes and connectors draw which model
//! items, and keeping those references valid.
//!
//! Shapes reference elements (`Shape.element`) and connectors reference relationships
//! (`Connector.relationship`). Elements and relationships share the document's one id space, so a
//! single id names at most one model item and the scans below can match both kinds at once.
//! Within a page, z order is the shapes back to front followed by the connectors back to front,
//! because connectors are drawn above shapes.

use std::collections::HashSet;

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

/// Clear every `Shape.element` that names no element of `doc.model`, every
/// `Connector.relationship` that names no relationship and every `Connector.net` that names no
/// net, so a damaged or hand-edited file loads with those items as free drawing.
pub fn sanitize_refs(doc: &mut Document) {
    // Build the id sets once: the model lookups are linear scans, and a large page would make
    // a per-item lookup quadratic.
    let elements: HashSet<&str> = doc.model.elements.iter().map(|e| e.id.as_str()).collect();
    let relationships: HashSet<&str> = doc.model.relationships.iter().map(|r| r.id.as_str()).collect();
    let nets: HashSet<&str> = doc.model.nets.iter().map(|n| n.id.as_str()).collect();
    for page in &mut doc.pages {
        for shape in &mut page.shapes {
            if shape.element.as_deref().is_some_and(|e| !elements.contains(e)) {
                shape.element = None;
            }
        }
        for c in &mut page.connectors {
            if c.relationship.as_deref().is_some_and(|r| !relationships.contains(r)) {
                c.relationship = None;
            }
            if c.net.as_deref().is_some_and(|n| !nets.contains(n)) {
                c.net = None;
            }
        }
    }
}

/// Every page that draws element or relationship `id`, with the items drawing it, in page order;
/// pages that do not draw it are left out.
pub fn views_of(doc: &Document, id: &str) -> Vec<ViewItems> {
    doc.pages
        .iter()
        .enumerate()
        .filter_map(|(index, page)| {
            let items: Vec<String> = page
                .shapes
                .iter()
                .filter(|s| s.element.as_deref() == Some(id))
                .map(|s| s.id.clone())
                .chain(page.connectors.iter().filter(|c| c.relationship.as_deref() == Some(id)).map(|c| c.id.clone()))
                .collect();
            (!items.is_empty()).then(|| ViewItems { page: index, page_id: page.id.clone(), items })
        })
        .collect()
}

/// The element a shape draws, when `shape` names a bound shape on any page.
pub fn element_of_shape<'a>(doc: &'a Document, shape: &str) -> Option<&'a str> {
    doc.shape(shape).and_then(|s| s.element.as_deref())
}

/// Ids of the shapes and connectors on any page that reference any of `ids` (elements or
/// relationships), in page then z order: the view items to remove when those model items are
/// deleted from the model.
pub fn items_referencing(doc: &Document, ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let hit = |r: Option<&str>| r.is_some_and(|r| wanted.contains(r));
    doc.pages
        .iter()
        .flat_map(|page| {
            page.shapes
                .iter()
                .filter(move |s| hit(s.element.as_deref()))
                .map(|s| s.id.clone())
                .chain(page.connectors.iter().filter(move |c| hit(c.relationship.as_deref())).map(|c| c.id.clone()))
        })
        .collect()
}

/// Element ids drawn on page `page_index`, each once, in the z order of their first shape; empty
/// when the page does not exist.
pub fn elements_on_page(doc: &Document, page_index: usize) -> Vec<&str> {
    let Some(page) = doc.pages.get(page_index) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    page.shapes.iter().filter_map(|s| s.element.as_deref()).filter(|e| seen.insert(*e)).collect()
}

/// Relationship ids drawn on page `page_index`, each once, in the z order of their first
/// connector; empty when the page does not exist.
pub fn relationships_on_page(doc: &Document, page_index: usize) -> Vec<&str> {
    let Some(page) = doc.pages.get(page_index) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    page.connectors.iter().filter_map(|c| c.relationship.as_deref()).filter(|r| seen.insert(*r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Connector, End, Page, Shape};
    use vizcraft_model::{Element, Relationship};

    fn shape(id: &str, element: Option<&str>) -> Shape {
        Shape { id: id.into(), element: element.map(str::to_string), ..Shape::default() }
    }

    fn connector(id: &str, from: &str, to: &str, relationship: Option<&str>) -> Connector {
        Connector {
            id: id.into(),
            start: End::dynamic(from),
            end: End::dynamic(to),
            relationship: relationship.map(str::to_string),
            ..Connector::default()
        }
    }

    /// Two pages; e1 is drawn on both (twice on p1), e2 on p1 only, r1 on p1.
    fn doc() -> Document {
        let mut d = Document::new();
        d.model.elements.push(Element::new("e1", "flowchart.Process"));
        d.model.elements.push(Element::new("e2", "flowchart.Decision"));
        d.model.relationships.push(Relationship::new("r1", "flowchart.Flow", "e1", "e2"));
        let mut p1 = Page::new("p1", "One");
        p1.shapes = vec![shape("s1", Some("e1")), shape("s2", None), shape("s3", Some("e2")), shape("s4", Some("e1"))];
        p1.connectors = vec![connector("c1", "s1", "s3", Some("r1")), connector("c2", "s2", "s3", None)];
        let mut p2 = Page::new("p2", "Two");
        p2.shapes = vec![shape("s5", Some("e1"))];
        d.pages = vec![p1, p2];
        d
    }

    #[test]
    fn sanitize_refs_clears_only_dangling_references() {
        let mut d = doc();
        if let Some(p) = d.pages.get_mut(1) {
            p.shapes.push(shape("s6", Some("gone")));
            p.shapes.push(shape("s7", Some("r1")));
            p.connectors.push(connector("c3", "s5", "s6", Some("ghost")));
            p.connectors.push(connector("c4", "s5", "s6", Some("e1")));
        }
        sanitize_refs(&mut d);
        assert_eq!(element_of_shape(&d, "s1"), Some("e1"));
        assert_eq!(element_of_shape(&d, "s6"), None);
        // A shape naming a relationship is not a view of an element.
        assert_eq!(element_of_shape(&d, "s7"), None);
        assert_eq!(d.connector("c1").and_then(|c| c.relationship.as_deref()), Some("r1"));
        assert_eq!(d.connector("c3").and_then(|c| c.relationship.as_deref()), None);
        assert_eq!(d.connector("c4").and_then(|c| c.relationship.as_deref()), None);
        let once = d.clone();
        sanitize_refs(&mut d);
        assert_eq!(d, once, "idempotent");
    }

    #[test]
    fn document_sanitize_repairs_refs_when_the_model_loses_items() {
        let mut d = doc();
        d.model.relationships.clear();
        d.model.elements.retain(|e| e.id != "e2");
        d.sanitize();
        assert_eq!(element_of_shape(&d, "s3"), None);
        assert_eq!(element_of_shape(&d, "s1"), Some("e1"));
        assert_eq!(d.connector("c1").and_then(|c| c.relationship.as_deref()), None);
    }

    #[test]
    fn views_of_lists_pages_and_items_in_order() {
        let d = doc();
        assert_eq!(
            views_of(&d, "e1"),
            vec![
                ViewItems { page: 0, page_id: "p1".into(), items: vec!["s1".into(), "s4".into()] },
                ViewItems { page: 1, page_id: "p2".into(), items: vec!["s5".into()] },
            ]
        );
        assert_eq!(views_of(&d, "r1"), vec![ViewItems { page: 0, page_id: "p1".into(), items: vec!["c1".into()] }]);
        assert!(views_of(&d, "nothing").is_empty());
        assert!(views_of(&d, "").is_empty());
    }

    #[test]
    fn items_referencing_covers_elements_and_relationships() {
        let d = doc();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(items_referencing(&d, &ids(&["e1"])), ids(&["s1", "s4", "s5"]));
        assert_eq!(items_referencing(&d, &ids(&["r1", "e2", "zzz"])), ids(&["s3", "c1"]));
        assert_eq!(items_referencing(&d, &ids(&["e1", "e2", "r1"])), ids(&["s1", "s3", "s4", "c1", "s5"]));
        assert!(items_referencing(&d, &[]).is_empty());
    }

    #[test]
    fn page_queries_list_each_model_item_once() {
        let d = doc();
        assert_eq!(elements_on_page(&d, 0), vec!["e1", "e2"]);
        assert_eq!(elements_on_page(&d, 1), vec!["e1"]);
        assert!(elements_on_page(&d, 9).is_empty());
        assert_eq!(relationships_on_page(&d, 0), vec!["r1"]);
        assert!(relationships_on_page(&d, 1).is_empty());
        assert!(relationships_on_page(&d, usize::MAX).is_empty());
    }
}
