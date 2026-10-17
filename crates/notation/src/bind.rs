//! The sync pass: keeps the model and its views in step after every document change.
//!
//! The engine calls [`sync`] after each command that changed the document, inside the same undo
//! step, before connector rerouting. It only touches items that are already bound or that join
//! bound items, so free drawing (and every Cascade 1 to 3 document) is left exactly as it was.

use vizcraft_diagram::Document;

use crate::Catalog;

/// Bring views and model into agreement after a command turned `before` into `doc`, in this order:
///
/// 1. A shape whose `element` is missing from the model, and a connector whose `relationship` is
///    missing, become free drawing (the reference is cleared).
/// 2. A second shape for the same element on one page is a copy: it gets a new element (fresh id
///    from `Document::next_id("e")`, same type and properties, name kept, tag re-issued by the
///    identity registry when the scope numbers automatically, else cleared).
/// 3. A connector glued at both ends to bound shapes gets a relationship when it has none: type
///    from [`crate::legal::relationship_for`] (an illegal pair still binds, so the rules engine
///    can report it), id from `next_id("r")`, the connector's text as its name. A bound
///    connector's relationship follows its glue (source and target from the glued shapes'
///    elements, ports from glued connection-point indexes when the type declares ports); a bound
///    connector with a free end keeps its relationship as it was.
/// 4. A relationship that some connector drew in `before` and no connector draws now is removed
///    from the model: deleting the last connector view deletes the relationship. Relationships
///    that never had a view (made by `relationship.create` between elements not on one page) stay.
/// 5. Every bound shape's text becomes its element's name, and every bound connector's text its
///    relationship's name.
/// 6. Style fields controlled by conditional styles (see `ConditionalStyle`) are set on bound
///    shapes.
/// 7. Packs of every element and relationship type in use are recorded in `doc.model.packs`.
pub fn sync(doc: &mut Document, before: &Document, cat: &Catalog) {
    // Contract stub; the c4a-rules piece implements the steps above.
    let _ = (doc, before, cat);
}
