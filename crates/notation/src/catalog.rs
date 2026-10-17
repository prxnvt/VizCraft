//! The built-in packs and lookups across every loaded pack.

use std::sync::OnceLock;

use vizcraft_model::{IdScope, PackRef};

use crate::schema::{ElementType, Glyph, Pack, RelationshipType};

/// The built-in packs as shipped: file name and text, in Shapes pane order.
pub const BUILTIN: &[(&str, &str)] =
    &[("basic.json", include_str!("../packs/basic.json")), ("flowchart.json", include_str!("../packs/flowchart.json"))];

/// A set of loaded packs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    packs: Vec<Pack>,
    errors: Vec<String>,
}

impl Catalog {
    /// A catalog of these packs (a later pack with an id already present is dropped and noted).
    pub fn new(packs: Vec<Pack>) -> Self {
        let mut c = Catalog::default();
        for p in packs {
            if c.pack(&p.id).is_some() {
                c.errors.push(format!("pack `{}` is loaded twice; the second copy is ignored", p.id));
            } else {
                c.packs.push(p);
            }
        }
        c
    }
    /// The catalog of the built-in packs. A pack that fails to load is left out and its error
    /// kept in [`Catalog::errors`].
    pub fn builtin() -> Self {
        let mut packs = Vec::new();
        let mut errors = Vec::new();
        for (name, text) in BUILTIN {
            match crate::loader::load_json(text) {
                Ok(p) => packs.push(p),
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        let mut c = Catalog::new(packs);
        c.errors.extend(errors);
        c
    }
    /// Every pack, in load order.
    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }
    /// Problems found while building the catalog.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
    /// The pack with this id.
    pub fn pack(&self, id: &str) -> Option<&Pack> {
        self.packs.iter().find(|p| p.id == id)
    }
    /// The pack reference (id and version) to record in a model that uses pack `id`.
    pub fn pack_ref(&self, id: &str) -> Option<PackRef> {
        self.pack(id).map(|p| PackRef { id: p.id.clone(), version: p.version.clone() })
    }
    /// The element type with this qualified id, with its pack.
    pub fn element_type(&self, id: &str) -> Option<(&Pack, &ElementType)> {
        let pack = self.pack(vizcraft_model::pack_of(id))?;
        pack.element_type(id).map(|t| (pack, t))
    }
    /// The relationship type with this qualified id, with its pack.
    pub fn relationship_type(&self, id: &str) -> Option<(&Pack, &RelationshipType)> {
        let pack = self.pack(vizcraft_model::pack_of(id))?;
        pack.relationship_type(id).map(|t| (pack, t))
    }
    /// The glyph with this id (a stencil master id such as `flowchart.decision`), with its pack.
    pub fn glyph(&self, id: &str) -> Option<(&Pack, &Glyph)> {
        let pack = self.pack(vizcraft_model::pack_of(id))?;
        pack.glyph(id).map(|g| (pack, g))
    }
    /// The stencil master an element type's views are drawn with.
    pub fn master_for_type(&self, type_id: &str) -> Option<&str> {
        self.element_type(type_id).map(|(_, t)| t.glyph.as_str()).filter(|g| !g.is_empty())
    }
    /// The element type a shape placed from master `master_id` becomes, if any.
    pub fn type_for_master(&self, master_id: &str) -> Option<&str> {
        self.glyph(master_id).map(|(_, g)| g.element_type.as_str()).filter(|t| !t.is_empty())
    }
    /// Identifier scopes of every pack, for the identity registry.
    pub fn scopes(&self) -> Vec<IdScope> {
        self.packs.iter().flat_map(|p| p.identifiers.iter().cloned()).collect()
    }
}

/// The catalog every session uses: the built-in packs, loaded once.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(Catalog::builtin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Check;

    #[test]
    fn builtin_packs_load_with_every_section() {
        let c = Catalog::builtin();
        assert!(c.errors().is_empty(), "{:?}", c.errors());
        let ids: Vec<_> = c.packs().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["basic", "flowchart"]);
        let basic = c.pack("basic").unwrap();
        assert_eq!((basic.glyphs.len(), basic.element_types.len()), (10, 0));
        let f = c.pack("flowchart").unwrap();
        assert_eq!((f.glyphs.len(), f.element_types.len(), f.relationship_types.len()), (12, 12, 1));
        assert_eq!((f.connections.len(), f.identifiers.len(), f.rules.len()), (3, 3, 12));
        assert!(f.rules.iter().all(|r| r.check != Check::Unknown), "every flowchart rule uses a known check");
        assert_eq!(c.type_for_master("flowchart.decision"), Some("flowchart.Decision"));
        assert_eq!(c.master_for_type("flowchart.Terminator"), Some("flowchart.terminator"));
        assert_eq!(c.type_for_master("basic.rectangle"), None);
        assert_eq!(c.scopes().len(), 3);
    }
}
