//! The built-in packs, packs loaded from disk, and lookups across every loaded pack.
//!
//! [`catalog`] is the catalog every session reads. It starts as the built-in packs plus every
//! pack file in the folder named by [`PACKS_DIR_ENV`] (or set with [`use_packs_dir`], which the
//! app points at the `packs` folder next to its preferences), and is replaced when a pack is
//! loaded with [`add_pack`] (the `notation.load` command). Each replacement is a new catalog
//! that lives for the rest of the process, so references handed out earlier stay valid; the
//! number of replacements is capped by [`MAX_INSTALLS`] so the memory this keeps is bounded.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{PoisonError, RwLock};

use vizcraft_model::{IdScope, PackRef};

use crate::loader::{MAX_PACK, NotationError};
use crate::schema::{ElementType, Glyph, Pack, RelationshipType, Ruleset};

/// The built-in packs as shipped: file name and text, in Shapes pane order. `drawing` has no
/// glyphs, only rules any page can be held to.
pub const BUILTIN: &[(&str, &str)] = &[
    ("basic.json", include_str!("../packs/basic.json")),
    ("flowchart.json", include_str!("../packs/flowchart.json")),
    ("drawing.json", include_str!("../packs/drawing.json")),
    ("sheet.json", include_str!("../packs/sheet.json")),
    ("cloud.json", include_str!("../packs/cloud.json")),
    ("network.json", include_str!("../packs/network.json")),
    ("dfd.json", include_str!("../packs/dfd.json")),
    ("uml.json", include_str!("../packs/uml.json")),
    ("er.json", include_str!("../packs/er.json")),
    ("electrical.json", include_str!("../packs/electrical.json")),
];

/// The environment variable naming a folder of pack files (`*.json`, `*.toml`) loaded with the
/// built-in packs when the catalog is first built.
pub const PACKS_DIR_ENV: &str = "VIZCRAFT_PACKS_DIR";

/// Most packs loaded from disk into one catalog.
pub const MAX_DISK_PACKS: usize = 64;

/// Most times the catalog can be replaced in one process (pack loads and folder changes).
pub const MAX_INSTALLS: usize = 256;

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
            match crate::loader::load_shipped(name, text) {
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
    /// The stencil master an element type's views are drawn with on a page that uses rendering
    /// variant `variant` (a qualified variant id; empty for the default drawing). A variant of
    /// another pack, an unknown one, or one that does not list the type gives the default glyph.
    pub fn master_for_type_in(&self, type_id: &str, variant: &str) -> Option<&str> {
        let (pack, _) = self.element_type(type_id)?;
        pack.variant(variant)
            .and_then(|v| v.glyphs.get(type_id))
            .map(String::as_str)
            .filter(|g| pack.glyph(g).is_some())
            .or_else(|| self.master_for_type(type_id))
    }
    /// The connector type a relationship type is drawn with under rendering variant `variant`
    /// (empty for the default): the variant's choice when it lists the type, else the first
    /// connector type of any pack that names the relationship type.
    pub fn connector_type_for_relationship_in(&self, rel_type: &str, variant: &str) -> Option<&str> {
        let chosen = self
            .relationship_type(rel_type)
            .and_then(|(pack, _)| pack.variant(variant).and_then(|v| v.connectors.get(rel_type)).filter(|k| pack.connector_type(k).is_some()));
        if let Some(k) = chosen {
            return Some(k.as_str());
        }
        self.packs.iter().flat_map(|p| p.connector_types.iter()).find(|k| k.relationship_type.as_deref() == Some(rel_type)).map(|k| k.id.as_str())
    }
    /// The compartments views of `type_id` carry on a page drawn with rendering variant
    /// `variant` (empty for the default): the variant's list for the type when it has one (an
    /// empty list: none), else the type's own declarations.
    pub fn compartments_in(&self, type_id: &str, variant: &str) -> &[crate::schema::CompartmentDecl] {
        let Some((pack, t)) = self.element_type(type_id) else { return &[] };
        match pack.variant(variant).and_then(|v| v.compartments.get(type_id)) {
            Some(list) => list,
            None => &t.compartments,
        }
    }
    /// Whether some declaration of `type_id` (its own, or any variant's of its pack) has the
    /// compartment id `id`: such a compartment is the pack's, not the user's.
    pub fn declares_compartment(&self, type_id: &str, id: &str) -> bool {
        let Some((pack, t)) = self.element_type(type_id) else { return false };
        t.compartments.iter().any(|d| d.id == id) || pack.variants.iter().filter_map(|v| v.compartments.get(type_id)).flatten().any(|d| d.id == id)
    }
    /// The rendering variant with this qualified id, with its pack.
    pub fn variant(&self, id: &str) -> Option<(&Pack, &crate::schema::Variant)> {
        let pack = self.pack(vizcraft_model::pack_of(id))?;
        pack.variant(id).map(|v| (pack, v))
    }
    /// The element type a shape placed from master `master_id` becomes, if any.
    pub fn type_for_master(&self, master_id: &str) -> Option<&str> {
        self.glyph(master_id).map(|(_, g)| g.element_type.as_str()).filter(|t| !t.is_empty())
    }
    /// Identifier scopes of every pack, for the identity registry.
    pub fn scopes(&self) -> Vec<IdScope> {
        self.packs.iter().flat_map(|p| p.identifiers.iter().cloned()).collect()
    }
    /// The ruleset with this qualified id, from the pack its prefix names.
    pub fn ruleset(&self, id: &str) -> Option<&Ruleset> {
        self.pack(vizcraft_model::pack_of(id))?.ruleset(id)
    }
    /// Every ruleset of every pack, with its pack, in catalog order.
    pub fn rulesets(&self) -> Vec<(&Pack, &Ruleset)> {
        self.packs.iter().flat_map(|p| p.rulesets.iter().map(move |r| (p, r))).collect()
    }
    /// The rule with this qualified id, with its pack.
    pub fn rule(&self, id: &str) -> Option<(&Pack, &crate::schema::RuleDecl)> {
        let pack = self.pack(vizcraft_model::pack_of(id))?;
        pack.rule(id).map(|r| (pack, r))
    }
    /// Whether `id` is the id of a built-in pack.
    pub fn is_builtin(id: &str) -> bool {
        BUILTIN.iter().any(|(name, _)| name.strip_suffix(".json") == Some(id))
    }
    /// This catalog with `pack` added, or replacing an earlier disk pack with the same id. A
    /// built-in pack cannot be replaced, and at most [`MAX_DISK_PACKS`] disk packs are held.
    pub fn with_pack(&self, pack: Pack) -> Result<Catalog, String> {
        if Catalog::is_builtin(&pack.id) {
            return Err(format!("pack `{}` is built in and cannot be replaced", pack.id));
        }
        let mut c = self.clone();
        if let Some(slot) = c.packs.iter_mut().find(|p| p.id == pack.id) {
            *slot = pack;
            return Ok(c);
        }
        let disk = c.packs.iter().filter(|p| !Catalog::is_builtin(&p.id)).count();
        if disk >= MAX_DISK_PACKS {
            return Err(format!("at most {MAX_DISK_PACKS} packs can be loaded from disk"));
        }
        c.packs.push(pack);
        Ok(c)
    }
    /// Load every `*.json` and `*.toml` pack file directly in `dir`, in file name order, at most
    /// [`MAX_DISK_PACKS`]; a file that fails to load is noted in [`Catalog::errors`] and skipped.
    pub fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                self.errors.push(format!("{}: {e}", dir.display()));
                return;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file() && p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "json" | "toml"))
            })
            .collect();
        files.sort();
        for path in files.into_iter().take(MAX_DISK_PACKS) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match read_pack(&path).map_err(|e| e.to_string()).and_then(|p| self.with_pack(p)) {
                Ok(c) => *self = Catalog { errors: std::mem::take(&mut self.errors), ..c },
                Err(e) => self.errors.push(format!("{name}: {e}")),
            }
        }
    }
}

/// Read and check one pack file, refusing files larger than the loader's cap before reading
/// them whole.
pub fn read_pack(path: &Path) -> Result<Pack, NotationError> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let file = std::fs::File::open(path).map_err(|e| NotationError::Parse(format!("cannot read {}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_PACK).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| NotationError::Parse(format!("cannot read {}: {e}", path.display())))?;
    crate::loader::load(&name, &bytes)
}

/// The current catalog; `None` until first asked for.
static CURRENT: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// Catalogs installed so far.
static INSTALLS: AtomicUsize = AtomicUsize::new(0);

/// The catalog when the process starts: the built-in packs and the packs in [`PACKS_DIR_ENV`].
fn initial() -> Catalog {
    let mut c = Catalog::builtin();
    if let Some(dir) = std::env::var_os(PACKS_DIR_ENV).filter(|d| !d.is_empty()) {
        c.scan_dir(Path::new(&dir));
    }
    c
}

/// The catalog every session uses: the built-in packs, the packs of the packs folder, and packs
/// loaded since (see the module docs).
pub fn catalog() -> &'static Catalog {
    if let Some(c) = *CURRENT.read().unwrap_or_else(PoisonError::into_inner) {
        return c;
    }
    let mut slot = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
    match *slot {
        Some(c) => c,
        None => {
            let c: &'static Catalog = Box::leak(Box::new(initial()));
            *slot = Some(c);
            c
        }
    }
}

/// Replace the current catalog with the result of `f` on it, holding the lock so two loads
/// cannot lose each other's packs.
fn replace(f: impl FnOnce(&Catalog) -> Result<Catalog, String>) -> Result<&'static Catalog, String> {
    let now = catalog();
    let mut slot = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
    let base = slot.unwrap_or(now);
    if INSTALLS.load(Ordering::Relaxed) >= MAX_INSTALLS {
        return Err(format!("the catalog was replaced {MAX_INSTALLS} times in this run; restart to load more packs"));
    }
    let next = f(base)?;
    INSTALLS.fetch_add(1, Ordering::Relaxed);
    let c: &'static Catalog = Box::leak(Box::new(next));
    *slot = Some(c);
    Ok(c)
}

/// Add `pack` to the catalog every session uses (or replace the disk pack with its id).
pub fn add_pack(pack: Pack) -> Result<&'static Catalog, String> {
    replace(|c| c.with_pack(pack))
}

/// Rebuild the catalog from the built-in packs and the pack files in `dir` (the app's packs
/// folder), dropping packs loaded since. Problems are in the new catalog's [`Catalog::errors`].
pub fn use_packs_dir(dir: &Path) -> Result<&'static Catalog, String> {
    replace(|_| {
        let mut c = Catalog::builtin();
        if dir.is_dir() {
            c.scan_dir(dir);
        }
        Ok(c)
    })
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
        // The packs every build ships, in order; the engineering packs follow them.
        assert_eq!(ids.get(..9), Some(&["basic", "flowchart", "drawing", "sheet", "cloud", "network", "dfd", "uml", "er"][..]));
        assert!(ids.contains(&"electrical"));
        let basic = c.pack("basic").unwrap();
        assert_eq!((basic.glyphs.len(), basic.element_types.len()), (10, 0));
        let f = c.pack("flowchart").unwrap();
        assert_eq!((f.glyphs.len(), f.element_types.len(), f.relationship_types.len()), (12, 12, 1));
        assert_eq!((f.connections.len(), f.identifiers.len(), f.rules.len()), (3, 3, 12));
        assert!(f.rules.iter().all(|r| r.check != Check::Unknown), "every flowchart rule uses a known check");
        assert_eq!(c.type_for_master("flowchart.decision"), Some("flowchart.Decision"));
        assert_eq!(c.master_for_type("flowchart.Terminator"), Some("flowchart.terminator"));
        assert_eq!(c.type_for_master("basic.rectangle"), None);
        let base = ["basic", "flowchart", "drawing", "sheet", "cloud", "network", "dfd", "uml", "er"];
        assert_eq!(c.scopes().iter().filter(|s| base.contains(&vizcraft_model::pack_of(&s.id))).count(), 7);
        let d = c.pack("drawing").unwrap();
        assert_eq!((d.glyphs.len(), d.element_types.len(), d.rules.len()), (0, 0, 5));
        assert_eq!(c.ruleset("drawing.tidy").unwrap().rules.len(), 5);
        let iso = c.ruleset("flowchart.iso5807").unwrap();
        assert_eq!(iso.rules.len(), f.rules.len(), "the ISO ruleset lists every Flowchart rule");
        assert!(f.rules.iter().all(|r| !r.reference.is_empty()), "every Flowchart rule names its clause");
        assert_eq!(c.rule("flowchart.singleExit").map(|(p, _)| p.id.as_str()), Some("flowchart"));
        assert!(Catalog::is_builtin("drawing") && !Catalog::is_builtin("mine"));
    }

    fn tiny(id: &str) -> Pack {
        serde_json::from_value(serde_json::json!({"id": id, "version": "1.0.0", "name": id})).unwrap()
    }

    #[test]
    fn variants_choose_the_glyph_and_connector_type_and_fall_back_to_the_default() {
        let pack: Pack = serde_json::from_value(serde_json::json!({
            "id": "vt", "version": "1.0.0", "name": "Variant test",
            "glyphs": [{"id": "vt.box", "outline": "rectangle", "w": 10, "h": 10}, {"id": "vt.round", "outline": "ellipse", "w": 10, "h": 10}],
            "elementTypes": [{"id": "vt.A", "glyph": "vt.box"}, {"id": "vt.B", "glyph": "vt.box"}],
            "relationshipTypes": [{"id": "vt.R"}],
            "connectorTypes": [{"id": "vt.plain", "relationshipType": "vt.R"}, {"id": "vt.dashed", "dash": "dashed"}],
            "variants": [{"id": "vt.alt", "glyphs": {"vt.A": "vt.round"}, "connectors": {"vt.R": "vt.dashed"}}]
        }))
        .unwrap();
        let c = Catalog::builtin().with_pack(pack).unwrap();
        assert_eq!(c.master_for_type_in("vt.A", "vt.alt"), Some("vt.round"));
        assert_eq!(c.master_for_type_in("vt.B", "vt.alt"), Some("vt.box"), "a type the variant leaves alone");
        assert_eq!(c.master_for_type_in("vt.A", ""), Some("vt.box"));
        assert_eq!(c.master_for_type_in("vt.A", "flowchart.nothing"), Some("vt.box"));
        assert_eq!(c.connector_type_for_relationship_in("vt.R", "vt.alt"), Some("vt.dashed"));
        assert_eq!(c.connector_type_for_relationship_in("vt.R", ""), Some("vt.plain"));
        assert_eq!(c.connector_type_for_relationship_in("flowchart.Flow", "vt.alt"), Some("flowchart.flow"));
        assert!(c.variant("vt.alt").is_some() && c.variant("vt.none").is_none());
    }

    #[test]
    fn disk_packs_add_replace_and_never_shadow_builtin_ones() {
        let c = Catalog::builtin();
        assert!(c.with_pack(tiny("flowchart")).is_err());
        let c2 = c.with_pack(tiny("mine")).unwrap();
        assert_eq!(c2.packs().len(), c.packs().len() + 1);
        let mut again = tiny("mine");
        again.name = "Mine two".into();
        let c3 = c2.with_pack(again).unwrap();
        assert_eq!((c3.packs().len(), c3.pack("mine").unwrap().name.as_str()), (c2.packs().len(), "Mine two"));
        let mut full = c.clone();
        for i in 0..MAX_DISK_PACKS {
            full = full.with_pack(tiny(&format!("p{i}"))).unwrap();
        }
        assert!(full.with_pack(tiny("one_more")).is_err());
    }

    #[test]
    fn a_folder_of_packs_loads_with_errors_noted() {
        let dir = std::env::temp_dir().join(format!("vizcraft-packs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b_mine.json"), serde_json::to_string(&tiny("mine")).unwrap()).unwrap();
        std::fs::write(dir.join("a_bad.json"), "{ not json").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let mut c = Catalog::builtin();
        c.scan_dir(&dir);
        assert!(c.pack("mine").is_some());
        assert_eq!(c.errors().len(), 1, "{:?}", c.errors());
        assert!(c.errors()[0].starts_with("a_bad.json"));
        let mut missing = Catalog::builtin();
        missing.scan_dir(&dir.join("nope"));
        assert_eq!(missing.errors().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
