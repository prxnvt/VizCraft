//! Consistency checks on a loaded pack.
//!
//! The loader runs [`check`] on every pack, so the rest of the workbench can trust that a loaded
//! pack's references resolve and its values are usable. A reference qualified with another pack's
//! prefix cannot be resolved from one pack alone; it is checked for shape only and left to the
//! catalog that holds both packs.

use std::collections::HashSet;

use vizcraft_diagram::{Arrow, Color, Dash, Route, ShapeKind};
use vizcraft_model::{IdFormat, PropValue};

use crate::schema::{Check, Pack, PropKind, PropertySchema};

/// Most entries walked in any one list of a pack (glyphs, types, rows, the types of one row...).
/// A list longer than this is reported and only its first entries are checked, so a hostile
/// pack near the loader's size cap cannot make the check slow.
pub const MAX_LIST: usize = 10_000;

/// Most problems reported for one pack; past this the report says how many more there were.
pub const MAX_PROBLEMS: usize = 100;

/// Longest id accepted (pack id, qualified type id, glyph id, property key), in bytes.
pub const MAX_ID_LEN: usize = 128;

/// Longest rule reference (the standard and clause), in bytes.
pub const MAX_REFERENCE: usize = 1_000;

/// Longest `keyword` of a type, and longest variant option key or value, in bytes.
pub const MAX_KEYWORD: usize = 64;

/// Most options one variant may set.
pub const MAX_VARIANT_OPTIONS: usize = 16;

/// Problems with `pack`, in plain words; empty when it is consistent. Checks that every id is
/// qualified with the pack's own prefix (or another pack's, for references), that ids are unique,
/// that every reference (glyph, type, relationship, scope, viewpoint) resolves, that colours parse,
/// outlines and arrows are known names, sizes are positive and finite, formats parse, and that
/// rules use known checks.
pub fn check(pack: &Pack) -> Vec<String> {
    let mut c = Checker::new(pack);
    c.run();
    c.finish()
}

/// What a reference names, for the lookup and the message.
#[derive(Clone, Copy)]
enum Kind {
    ElementType,
    Relationship,
    Glyph,
    Scope,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Kind::ElementType => "element type",
            Kind::Relationship => "relationship type",
            Kind::Glyph => "glyph",
            Kind::Scope => "identifier scope",
        }
    }
}

struct Checker<'a> {
    pack: &'a Pack,
    /// Whether the pack id itself is usable; prefix checks are skipped when it is not, so one bad
    /// id does not produce a problem for every entry.
    id_ok: bool,
    types: HashSet<&'a str>,
    rels: HashSet<&'a str>,
    glyphs: HashSet<&'a str>,
    scopes: HashSet<&'a str>,
    out: Vec<String>,
    dropped: usize,
}

/// The first [`MAX_LIST`] entries of a list.
fn capped<T>(list: &[T]) -> &[T] {
    list.get(..MAX_LIST).unwrap_or(list)
}

/// An id cut to a readable length for a message (a hostile pack may hold huge strings).
fn short(t: &str) -> String {
    let mut out: String = t.chars().take(64).collect();
    if out.len() < t.len() {
        out.push_str("...");
    }
    out
}

/// Whether `id` is a usable pack id: lowercase ASCII letters, digits and `_`.
pub fn is_pack_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Whether `id` is `<pack>.<name>` with a valid pack id and a name of ASCII letters, digits, `_`
/// and `-`.
fn is_qualified(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && id
            .split_once('.')
            .is_some_and(|(p, n)| is_pack_id(p) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

/// Whether `key` is a usable property key: ASCII letters, digits and `_`.
fn is_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_ID_LEN && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether `v` looks like `major.minor.patch`.
fn is_version(v: &str) -> bool {
    let parts: Vec<&str> = v.splitn(4, '.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

impl<'a> Checker<'a> {
    fn new(pack: &'a Pack) -> Self {
        let set = |ids: Vec<&'a str>| ids.into_iter().collect::<HashSet<_>>();
        Checker {
            pack,
            id_ok: is_pack_id(&pack.id),
            types: set(capped(&pack.element_types).iter().map(|t| t.id.as_str()).collect()),
            rels: set(capped(&pack.relationship_types).iter().map(|t| t.id.as_str()).collect()),
            glyphs: set(capped(&pack.glyphs).iter().map(|g| g.id.as_str()).collect()),
            scopes: set(capped(&pack.identifiers).iter().map(|s| s.id.as_str()).collect()),
            out: Vec::new(),
            dropped: 0,
        }
    }

    fn problem(&mut self, msg: String) {
        if self.out.len() < MAX_PROBLEMS {
            self.out.push(msg);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    fn finish(mut self) -> Vec<String> {
        if self.dropped > 0 {
            self.out.push(format!("and {} more problems", self.dropped));
        }
        self.out
    }

    /// Report a list longer than [`MAX_LIST`] and return the part that is checked.
    fn list<'b, T>(&mut self, what: &str, list: &'b [T]) -> &'b [T] {
        if list.len() > MAX_LIST {
            self.problem(format!("{what} has {} entries; the limit is {MAX_LIST}", list.len()));
        }
        capped(list)
    }

    fn own_prefix(&self, id: &str) -> bool {
        vizcraft_model::pack_of(id) == self.pack.id
    }

    /// A declared id: qualified with this pack's prefix and not seen before in its kind.
    fn declared(&mut self, what: &str, id: &'a str, seen: &mut HashSet<&'a str>) {
        if !is_qualified(id) {
            self.problem(format!("{what} id `{}` is not `<pack>.<name>`", short(id)));
        } else if self.id_ok && !self.own_prefix(id) {
            self.problem(format!("{what} id `{}` does not start with `{}.`", short(id), self.pack.id));
        }
        if !seen.insert(id) {
            self.problem(format!("{what} id `{}` is declared twice", short(id)));
        }
    }

    /// A reference from `ctx` to an id of `kind`: it must resolve in this pack when it carries
    /// this pack's prefix, and be well formed when it names another pack.
    fn reference(&mut self, ctx: &str, kind: Kind, id: &str) {
        let known = match kind {
            Kind::ElementType => self.types.contains(id),
            Kind::Relationship => self.rels.contains(id),
            Kind::Glyph => self.glyphs.contains(id),
            Kind::Scope => self.scopes.contains(id),
        };
        if known {
            return;
        }
        if !is_qualified(id) {
            self.problem(format!("{ctx}: {} `{}` is not `<pack>.<name>`", kind.noun(), short(id)));
        } else if self.own_prefix(id) {
            self.problem(format!("{ctx}: unknown {} `{}`", kind.noun(), short(id)));
        }
    }

    /// A list of element types, where `*` stands for every element type of this pack.
    fn type_list(&mut self, ctx: &str, list: &[String]) {
        for t in self.list(ctx, list) {
            if t != "*" {
                self.reference(ctx, Kind::ElementType, t);
            }
        }
    }

    fn colour(&mut self, ctx: &str, what: &str, value: &str) {
        if Color::parse(value).is_none() {
            self.problem(format!("{ctx}: {what} `{}` is not a colour like #rrggbb", short(value)));
        }
    }

    fn run(&mut self) {
        let pack = self.pack;
        if pack.id.is_empty() {
            self.problem("the pack has no id".to_string());
        } else if !self.id_ok {
            self.problem(format!("pack id `{}` must be lowercase letters, digits and `_`", short(&pack.id)));
        }
        if !is_version(&pack.version) {
            self.problem(format!("pack version `{}` is not `major.minor.patch`", short(&pack.version)));
        }
        self.glyphs();
        self.connector_types();
        self.element_types();
        self.relationship_types();
        self.connections();
        self.conditional_styles();
        self.palette();
        self.viewpoints();
        self.identifiers();
        self.rules();
        self.rulesets();
        self.modules();
        self.variant_options();
    }

    fn glyphs(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for g in self.list("glyphs", &pack.glyphs) {
            self.declared("glyph", &g.id, &mut seen);
            let ctx = format!("glyph `{}`", short(&g.id));
            if ShapeKind::from_name(&g.outline).is_none() {
                self.problem(format!("{ctx}: unknown outline `{}`", short(&g.outline)));
            }
            if !positive(g.w) || !positive(g.h) {
                self.problem(format!("{ctx}: width and height must be positive numbers"));
            }
            self.colour(&ctx, "fill", &g.fill);
            self.colour(&ctx, "line", &g.line);
            if !g.element_type.is_empty() {
                self.reference(&ctx, Kind::ElementType, &g.element_type);
            }
        }
    }

    fn connector_types(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for t in self.list("connector types", &pack.connector_types) {
            self.declared("connector type", &t.id, &mut seen);
            let ctx = format!("connector type `{}`", short(&t.id));
            if Route::from_name(&t.route).is_none() {
                self.problem(format!("{ctx}: unknown route `{}`", short(&t.route)));
            }
            for (what, name) in [("start arrow", &t.start_arrow), ("end arrow", &t.end_arrow)] {
                if Arrow::from_name(name).is_none() {
                    self.problem(format!("{ctx}: unknown {what} `{}`", short(name)));
                }
            }
            if Dash::from_name(&t.dash).is_none() {
                self.problem(format!("{ctx}: unknown dash `{}`", short(&t.dash)));
            }
            if !t.line_width.is_finite() || t.line_width < 0.0 {
                self.problem(format!("{ctx}: line width must be a number, zero or more"));
            }
            self.colour(&ctx, "line", &t.line);
            if let Some(r) = &t.relationship_type {
                self.reference(&ctx, Kind::Relationship, r);
            }
            if let Some(n) = &t.net
                && vizcraft_model::NetKind::from_name(n).is_none()
            {
                self.problem(format!("{ctx}: unknown net kind `{}`", short(n)));
            }
            if !t.marker.is_empty() && vizcraft_diagram::LineMarker::from_name(&t.marker).is_none() {
                self.problem(format!("{ctx}: unknown line marker `{}`", short(&t.marker)));
            }
        }
    }

    /// Every port of a typed glyph must sit a whole number of modules from the type's first
    /// port (within 0.05 of a module), on every module the pack and its variants name, so
    /// symbols placed on the module grid put their pins on it too.
    fn modules(&mut self) {
        let pack = self.pack;
        // (module, glyph override per type) for the pack and each variant with a module.
        let mut checks: Vec<(String, f64, Option<&std::collections::BTreeMap<String, String>>)> = Vec::new();
        if let Some(step) = pack.module.step() {
            checks.push(("the pack's module".to_string(), step, None));
        }
        for v in capped(&pack.variants) {
            if let Some(step) = v.module.step().or(pack.module.step()) {
                checks.push((format!("variant `{}`", short(&v.id)), step, Some(&v.glyphs)));
            }
        }
        if checks.is_empty() {
            return;
        }
        for (what, step, glyphs) in checks {
            for t in capped(&pack.element_types) {
                let Some(first) = t.ports.first() else { continue };
                let glyph_id = glyphs.and_then(|g| g.get(&t.id)).unwrap_or(&t.glyph);
                let Some(g) = pack.glyph(glyph_id) else { continue };
                for p in capped(&t.ports).iter().skip(1) {
                    let off = [(p.x - first.x) * g.w, (p.y - first.y) * g.h];
                    let on = off.iter().all(|d| {
                        let m = d / step;
                        m.is_finite() && (m - m.round()).abs() <= 0.05
                    });
                    if !on {
                        self.problem(format!(
                            "element type `{}`: port `{}` of glyph `{}` is not a whole number of modules from port `{}` on {what}",
                            short(&t.id),
                            short(&p.id),
                            short(glyph_id),
                            short(&first.id)
                        ));
                    }
                }
            }
        }
    }

    fn properties(&mut self, ctx: &str, props: &[PropertySchema]) {
        let mut keys = HashSet::new();
        for p in self.list(ctx, props) {
            if !is_key(&p.key) {
                self.problem(format!("{ctx}: property key `{}` must be letters, digits and `_`", short(&p.key)));
            }
            if !keys.insert(p.key.as_str()) {
                self.problem(format!("{ctx}: property `{}` is declared twice", short(&p.key)));
            }
            let pctx = format!("{ctx}, property `{}`", short(&p.key));
            if p.kind == PropKind::Enum {
                if p.values.is_empty() {
                    self.problem(format!("{pctx}: an enum needs values"));
                }
                let n = self.list(&pctx, &p.values).len();
                let values = capped(&p.values);
                let distinct: HashSet<&String> = values.iter().collect();
                if distinct.len() < n {
                    self.problem(format!("{pctx}: an enum value is listed twice"));
                }
            }
            if let Some(d) = &p.default {
                let fits = match (p.kind, d) {
                    (PropKind::Text, PropValue::Text(_)) | (PropKind::Bool, PropValue::Bool(_)) => true,
                    (PropKind::Number, PropValue::Number(n)) => n.is_finite(),
                    (PropKind::Integer, PropValue::Number(n)) => n.is_finite() && n.fract() == 0.0,
                    (PropKind::Enum, PropValue::Text(t)) => capped(&p.values).contains(t),
                    _ => false,
                };
                if !fits {
                    let what = if p.kind == PropKind::Enum { "is not one of its values" } else { "does not fit its kind" };
                    self.problem(format!("{pctx}: the default {what}"));
                }
            }
        }
    }

    fn element_types(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for t in self.list("element types", &pack.element_types) {
            self.declared("element type", &t.id, &mut seen);
            let ctx = format!("element type `{}`", short(&t.id));
            if !t.glyph.is_empty() {
                self.reference(&ctx, Kind::Glyph, &t.glyph);
            }
            if let Some(sup) = &t.extends {
                if sup == &t.id {
                    self.problem(format!("{ctx}: a type cannot extend itself"));
                } else {
                    self.reference(&ctx, Kind::ElementType, sup);
                }
            }
            self.properties(&ctx, &t.properties);
            self.keyword(&ctx, &t.keyword);
            for c in capped(&t.compartments) {
                if !c.rows_from.is_empty() && !is_key(&c.rows_from) {
                    self.problem(format!("{ctx}: compartment `{}`: `rowsFrom` must be a property key", short(&c.id)));
                }
            }
            let mut ports = HashSet::new();
            for p in self.list(&ctx, &t.ports) {
                if p.id.is_empty() || !ports.insert(p.id.as_str()) {
                    self.problem(format!("{ctx}: port id `{}` is empty or declared twice", short(&p.id)));
                }
                if !(0.0..=1.0).contains(&p.x) || !(0.0..=1.0).contains(&p.y) {
                    self.problem(format!("{ctx}: port `{}` must sit within the frame (0 to 1)", short(&p.id)));
                }
            }
        }
    }

    fn relationship_types(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for t in self.list("relationship types", &pack.relationship_types) {
            self.declared("relationship type", &t.id, &mut seen);
            let ctx = format!("relationship type `{}`", short(&t.id));
            if Arrow::from_name(&t.end_arrow).is_none() {
                self.problem(format!("{ctx}: unknown end arrow `{}`", short(&t.end_arrow)));
            }
            if let Some(inv) = &t.inverse_of {
                if inv == &t.id {
                    self.problem(format!("{ctx}: a relationship type cannot be its own inverse"));
                } else {
                    self.reference(&ctx, Kind::Relationship, inv);
                }
            }
            self.properties(&ctx, &t.properties);
            self.keyword(&ctx, &t.keyword);
            for k in capped(&t.end_labels.start).iter().chain(capped(&t.end_labels.end)) {
                self.key(&ctx, k);
            }
        }
    }

    /// A `keyword`: at most [`MAX_KEYWORD`] bytes on one line.
    fn keyword(&mut self, ctx: &str, keyword: &str) {
        if keyword.len() > MAX_KEYWORD || keyword.chars().any(char::is_control) {
            self.problem(format!("{ctx}: the keyword must be one line of at most {MAX_KEYWORD} bytes"));
        }
    }

    /// Variant options: few, short, one line each.
    fn variant_options(&mut self) {
        let pack = self.pack;
        for v in capped(&pack.variants) {
            if v.options.len() > MAX_VARIANT_OPTIONS {
                self.problem(format!("variant `{}`: at most {MAX_VARIANT_OPTIONS} options", short(&v.id)));
            }
            for (k, val) in v.options.iter().take(MAX_VARIANT_OPTIONS + 1) {
                if !is_key(k) || k.len() > MAX_KEYWORD || val.len() > MAX_KEYWORD || val.chars().any(char::is_control) {
                    self.problem(format!("variant `{}`: option `{}` needs a key and value of at most {MAX_KEYWORD} bytes", short(&v.id), short(k)));
                }
            }
        }
    }

    fn connections(&mut self) {
        let pack = self.pack;
        for (i, row) in self.list("connections", &pack.connections).iter().enumerate() {
            let ctx = format!("connection {}", i.saturating_add(1));
            if row.source.is_empty() || row.target.is_empty() {
                self.problem(format!("{ctx}: needs source and target types"));
            }
            self.type_list(&ctx, &row.source);
            self.type_list(&ctx, &row.target);
            self.reference(&ctx, Kind::Relationship, &row.relationship);
        }
    }

    fn conditional_styles(&mut self) {
        let pack = self.pack;
        for (i, cs) in self.list("conditional styles", &pack.conditional_styles).iter().enumerate() {
            let ctx = format!("conditional style {}", i.saturating_add(1));
            self.type_list(&ctx, &cs.types);
            if !is_key(&cs.property) {
                self.problem(format!("{ctx}: property key `{}` must be letters, digits and `_`", short(&cs.property)));
            } else {
                // Each listed type of this pack must declare the property the style compares.
                for t in capped(&cs.types) {
                    if let Some(et) = pack.element_type(t)
                        && et.property(&cs.property).is_none()
                    {
                        self.problem(format!("{ctx}: type `{}` has no property `{}`", short(t), short(&cs.property)));
                    }
                }
            }
            for (what, v) in [("fill", &cs.fill), ("line", &cs.line), ("font colour", &cs.font_color)] {
                if let Some(v) = v {
                    self.colour(&ctx, what, v);
                }
            }
            if cs.line_width.is_some_and(|w| !positive(w)) {
                self.problem(format!("{ctx}: line width must be a positive number"));
            }
        }
    }

    fn palette(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for g in self.list("palette", &pack.palette) {
            self.declared("palette group", &g.id, &mut seen);
            let ctx = format!("palette group `{}`", short(&g.id));
            self.type_list(&ctx, &g.types);
            for gl in self.list(&ctx, &g.glyphs) {
                self.reference(&ctx, Kind::Glyph, gl);
            }
        }
    }

    fn viewpoints(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for v in self.list("viewpoints", &pack.viewpoints) {
            self.declared("viewpoint", &v.id, &mut seen);
            let ctx = format!("viewpoint `{}`", short(&v.id));
            self.type_list(&ctx, &v.types);
            for r in self.list(&ctx, &v.relationships) {
                self.reference(&ctx, Kind::Relationship, r);
            }
        }
    }

    fn identifiers(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for s in self.list("identifiers", &pack.identifiers) {
            self.declared("identifier scope", &s.id, &mut seen);
            let ctx = format!("identifier scope `{}`", short(&s.id));
            self.type_list(&ctx, &s.types);
            if let Err(e) = IdFormat::parse(&s.format) {
                self.problem(format!("{ctx}: format: {e}"));
            }
        }
    }

    fn rules(&mut self) {
        let pack = self.pack;
        let mut seen = HashSet::new();
        for r in self.list("rules", &pack.rules) {
            self.declared("rule", &r.id, &mut seen);
            let ctx = format!("rule `{}`", short(&r.id));
            if r.reference.len() > MAX_REFERENCE {
                self.problem(format!("{ctx}: the reference is longer than {MAX_REFERENCE} bytes"));
            }
            match &r.check {
                Check::IllegalRelationship | Check::Multiplicity | Check::ViewpointType | Check::DanglingConnector => {}
                Check::RequiredProperty { types } | Check::PropertyType { types } => self.type_list(&ctx, types),
                Check::DuplicateIdentifier { scope } | Check::UnmatchedPair { scope } => self.reference(&ctx, Kind::Scope, scope),
                Check::MinOutgoing { types, relationship, .. }
                | Check::MinIncoming { types, relationship, .. }
                | Check::ExclusiveDirections { types, relationship } => {
                    self.type_list(&ctx, types);
                    self.reference(&ctx, Kind::Relationship, relationship);
                }
                Check::Unreachable { from, relationship, types } => {
                    self.type_list(&ctx, from);
                    self.type_list(&ctx, types);
                    self.any_relationship(&ctx, relationship);
                }
                Check::ForbiddenCycle { relationship } | Check::PortDirection { relationship } | Check::PortFanIn { relationship, .. } => {
                    self.any_relationship(&ctx, relationship)
                }
                Check::RequiredPath { from, to, relationship } => {
                    self.type_list(&ctx, from);
                    self.type_list(&ctx, to);
                    self.any_relationship(&ctx, relationship);
                }
                Check::Connected { types, relationship } => {
                    self.type_list(&ctx, types);
                    self.any_relationship(&ctx, relationship);
                }
                Check::Containment { owners, children } => {
                    self.type_list(&ctx, owners);
                    self.type_list(&ctx, children);
                }
                Check::UniqueProperty { types, property, within } => {
                    self.type_list(&ctx, types);
                    self.type_list(&ctx, within);
                    self.key(&ctx, property);
                }
                Check::PropertyCompare { types, property, other, .. } => {
                    self.type_list(&ctx, types);
                    self.key(&ctx, property);
                    if other.is_none() {
                        self.problem(format!("{ctx}: a comparison needs `other`, a property key or a constant"));
                    }
                }
                Check::NameFormat { types, pattern } => {
                    self.type_list(&ctx, types);
                    if crate::glob::Glob::parse(pattern).is_none() {
                        self.problem(format!("{ctx}: `{}` is not a usable name pattern", short(pattern)));
                    }
                }
                Check::CrossesShape | Check::UngluedEnd | Check::Overlap | Check::OffBoard | Check::TextOverflow => {}
                Check::DocumentFields { .. } | Check::RevisionSequence | Check::BalloonRows | Check::CrossReferences | Check::StaleTag => {}
                Check::EndMarkerMatchesProperty { relationship, property, markers } => {
                    self.reference(&ctx, Kind::Relationship, relationship);
                    self.key(&ctx, property);
                    for m in self.list(&ctx, markers) {
                        for a in [&m.start, &m.end].into_iter().flatten() {
                            if Arrow::from_name(a).is_none() {
                                self.problem(format!("{ctx}: unknown arrow `{}`", short(a)));
                            }
                        }
                    }
                }
                Check::MultiplicitySyntax { types, properties, members } => {
                    for t in self.list(&ctx, types) {
                        if t != "*" && !self.rels.contains(t.as_str()) {
                            self.reference(&ctx, Kind::ElementType, t);
                        }
                    }
                    for k in self.list(&ctx, properties).iter().chain(capped(members)) {
                        self.key(&ctx, k);
                    }
                }
                Check::MaxPerOwner { types, within, .. } => {
                    self.type_list(&ctx, types);
                    self.type_list(&ctx, within);
                }
                Check::UniqueMembers { types, property } => {
                    self.type_list(&ctx, types);
                    self.key(&ctx, property);
                }
                Check::Unknown => self.problem(format!("{ctx}: unknown check kind (the pack needs a newer VizCraft)")),
            }
        }
    }

    /// A relationship type a check follows; empty means every relationship.
    fn any_relationship(&mut self, ctx: &str, id: &str) {
        if !id.is_empty() {
            self.reference(ctx, Kind::Relationship, id);
        }
    }

    fn key(&mut self, ctx: &str, key: &str) {
        if !is_key(key) {
            self.problem(format!("{ctx}: property key `{}` must be letters, digits and `_`", short(key)));
        }
    }

    /// Ruleset ids are declared like rules; each listed rule of this pack must exist, and a rule
    /// of another pack must at least be well formed.
    fn rulesets(&mut self) {
        let pack = self.pack;
        let rules: HashSet<&str> = capped(&pack.rules).iter().map(|r| r.id.as_str()).collect();
        let mut seen = HashSet::new();
        for rs in self.list("rulesets", &pack.rulesets) {
            self.declared("ruleset", &rs.id, &mut seen);
            let ctx = format!("ruleset `{}`", short(&rs.id));
            for r in self.list(&ctx, &rs.rules) {
                if rules.contains(r.as_str()) {
                    continue;
                }
                if !is_qualified(r) {
                    self.problem(format!("{ctx}: rule `{}` is not `<pack>.<name>`", short(r)));
                } else if self.own_prefix(r) {
                    self.problem(format!("{ctx}: unknown rule `{}`", short(r)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::BUILTIN;
    use crate::schema::{Connection, ElementType, Glyph, RelationshipType, RuleDecl};

    fn flowchart() -> Pack {
        serde_json::from_str(BUILTIN.iter().find(|(n, _)| *n == "flowchart.json").unwrap().1).unwrap()
    }

    fn has(problems: &[String], needle: &str) -> bool {
        problems.iter().any(|p| p.contains(needle))
    }

    #[test]
    fn builtin_packs_are_consistent() {
        for (name, text) in BUILTIN {
            let p: Pack = serde_json::from_str(text).unwrap();
            assert_eq!(check(&p), Vec::<String>::new(), "{name}");
        }
    }

    #[test]
    fn ids_must_be_qualified_unique_and_lowercase_pack() {
        let mut p = flowchart();
        p.id = "Flow Chart".into();
        assert!(has(&check(&p), "lowercase"));
        let mut p = flowchart();
        p.element_types[1].id = "other.Process".into();
        let probs = check(&p);
        assert!(has(&probs, "does not start with `flowchart.`"), "{probs:?}");
        let mut p = flowchart();
        p.element_types[1].id = "Process".into();
        assert!(has(&check(&p), "is not `<pack>.<name>`"));
        let mut p = flowchart();
        let dup = p.glyphs[0].clone();
        p.glyphs.push(dup);
        assert!(has(&check(&p), "declared twice"));
        let mut p = flowchart();
        p.version = "1.0".into();
        assert!(has(&check(&p), "major.minor.patch"));
    }

    #[test]
    fn references_must_resolve() {
        let mut p = flowchart();
        p.element_types[0].glyph = "flowchart.nope".into();
        p.glyphs[0].element_type = "flowchart.Nope".into();
        p.connections[0].relationship = "flowchart.Jump".into();
        p.connections[1].target.push("flowchart.Ghost".into());
        p.viewpoints[0].relationships.push("flowchart.Link".into());
        p.identifiers[0].types.push("flowchart.Phantom".into());
        p.palette[0].types.push("flowchart.Spirit".into());
        p.conditional_styles[0].types.push("flowchart.Shade".into());
        let probs = check(&p);
        for needle in [
            "unknown glyph `flowchart.nope`",
            "unknown element type `flowchart.Nope`",
            "unknown relationship type `flowchart.Jump`",
            "flowchart.Ghost",
            "flowchart.Link",
            "flowchart.Phantom",
            "flowchart.Spirit",
            "flowchart.Shade",
        ] {
            assert!(has(&probs, needle), "{needle}: {probs:?}");
        }
        // Another pack's types cannot be resolved here, and `*` stands for every type.
        let mut p = flowchart();
        p.connections[0].target = vec!["*".into(), "bpmn.Task".into()];
        assert_eq!(check(&p), Vec::<String>::new());
    }

    #[test]
    fn rule_references_and_unknown_checks() {
        let mut p = flowchart();
        p.rules.push(RuleDecl {
            id: "flowchart.x".into(),
            check: Check::DuplicateIdentifier { scope: "flowchart.none".into() },
            ..RuleDecl::default()
        });
        p.rules.push(RuleDecl {
            id: "flowchart.y".into(),
            check: Check::MinOutgoing { types: vec!["flowchart.Ghost".into()], relationship: "flowchart.Wire".into(), min: 1 },
            ..RuleDecl::default()
        });
        p.rules.push(RuleDecl { id: "flowchart.z".into(), ..RuleDecl::default() });
        let probs = check(&p);
        assert!(has(&probs, "unknown identifier scope `flowchart.none`"), "{probs:?}");
        assert!(has(&probs, "flowchart.Ghost") && has(&probs, "flowchart.Wire"));
        assert!(has(&probs, "rule `flowchart.z`: unknown check kind"));
    }

    #[test]
    fn new_fields_and_checks_are_checked() {
        let mut p = flowchart();
        p.element_types[0].extends = Some("flowchart.Nope".into());
        p.element_types[1].extends = Some(p.element_types[1].id.clone());
        p.relationship_types[0].inverse_of = Some("flowchart.Flow".into());
        p.rulesets[0].rules.push("flowchart.missingRule".into());
        p.rulesets[0].rules.push("not qualified".into());
        p.rulesets.push(p.rulesets[0].clone());
        p.rules[0].reference = "x".repeat(MAX_REFERENCE + 1);
        p.rules.push(RuleDecl {
            id: "flowchart.n1".into(),
            check: Check::NameFormat { types: vec![], pattern: "[abc".into() },
            ..RuleDecl::default()
        });
        p.rules.push(RuleDecl {
            id: "flowchart.n2".into(),
            check: Check::PropertyCompare {
                types: vec!["flowchart.Ghost".into()],
                property: "bad key".into(),
                op: crate::CompareOp::Ge,
                other: None,
            },
            ..RuleDecl::default()
        });
        p.rules.push(RuleDecl {
            id: "flowchart.n3".into(),
            check: Check::ForbiddenCycle { relationship: "flowchart.Wire".into() },
            ..RuleDecl::default()
        });
        // An empty relationship means every relationship.
        p.rules.push(RuleDecl {
            id: "flowchart.n4".into(),
            check: Check::Connected { types: vec![], relationship: String::new() },
            ..RuleDecl::default()
        });
        let probs = check(&p);
        for needle in [
            "unknown element type `flowchart.Nope`",
            "cannot extend itself",
            "its own inverse",
            "unknown rule `flowchart.missingRule`",
            "`not qualified` is not",
            "ruleset id `flowchart.iso5807` is declared twice",
            "reference is longer",
            "not a usable name pattern",
            "flowchart.Ghost",
            "property key `bad key`",
            "needs `other`",
            "flowchart.Wire",
        ] {
            assert!(has(&probs, needle), "{needle}: {probs:?}");
        }
        assert!(!has(&probs, "flowchart.n4"), "{probs:?}");
    }

    #[test]
    fn values_must_be_usable() {
        let mut p = flowchart();
        p.glyphs[0].fill = "green".into();
        p.glyphs[1].outline = "blob".into();
        p.glyphs[2].w = f64::NAN;
        p.glyphs[3].h = -1.0;
        p.relationship_types[0].end_arrow = "harpoon".into();
        p.identifiers[0].format = "S{n".into();
        p.conditional_styles[0].line_width = Some(0.0);
        p.conditional_styles[0].property = "missing".into();
        let probs = check(&p);
        for needle in [
            "`green` is not a colour",
            "unknown outline `blob`",
            "positive numbers",
            "unknown end arrow",
            "format:",
            "line width",
            "no property `missing`",
        ] {
            assert!(has(&probs, needle), "{needle}: {probs:?}");
        }
    }

    #[test]
    fn connector_types_are_checked_like_glyphs() {
        let mut p = flowchart();
        let good = p.connector_types[0].clone();
        assert_eq!(good.id, "flowchart.flow");
        let mut dup = good.clone();
        dup.route = "zigzag".into();
        dup.start_arrow = "harpoon".into();
        dup.end_arrow = "".into();
        dup.dash = "wavy".into();
        dup.line = "green".into();
        dup.line_width = f64::NAN;
        dup.relationship_type = Some("flowchart.Jump".into());
        p.connector_types.push(dup);
        p.connector_types.push(vizcraft_connector("other.flow"));
        p.connector_types.push(vizcraft_connector("flow"));
        let probs = check(&p);
        for needle in [
            "connector type id `flowchart.flow` is declared twice",
            "unknown route `zigzag`",
            "unknown start arrow `harpoon`",
            "unknown end arrow ``",
            "unknown dash `wavy`",
            "`green` is not a colour",
            "line width must be a number",
            "unknown relationship type `flowchart.Jump`",
            "connector type id `other.flow` does not start with `flowchart.`",
            "connector type id `flow` is not `<pack>.<name>`",
        ] {
            assert!(has(&probs, needle), "{needle}: {probs:?}");
        }
    }

    fn vizcraft_connector(id: &str) -> crate::schema::ConnectorType {
        crate::schema::ConnectorType { id: id.into(), ..crate::schema::ConnectorType::default() }
    }

    #[test]
    fn enum_properties_need_values_and_a_listed_default() {
        let mut p = flowchart();
        let et = &mut p.element_types[0];
        et.properties.push(PropertySchema { key: "mode".into(), kind: PropKind::Enum, ..PropertySchema::default() });
        et.properties.push(PropertySchema {
            key: "level".into(),
            kind: PropKind::Enum,
            values: vec!["low".into(), "high".into()],
            default: Some(PropValue::Text("mid".into())),
            ..PropertySchema::default()
        });
        et.properties.push(PropertySchema {
            key: "count".into(),
            kind: PropKind::Integer,
            default: Some(PropValue::Number(1.5)),
            ..PropertySchema::default()
        });
        let probs = check(&p);
        assert!(has(&probs, "an enum needs values"), "{probs:?}");
        assert!(has(&probs, "is not one of its values"));
        assert!(has(&probs, "does not fit its kind"));
    }

    #[test]
    fn huge_packs_are_bounded() {
        let mut p = Pack { id: "big".into(), version: "1.0.0".into(), ..Pack::default() };
        p.relationship_types.push(RelationshipType { id: "big.R".into(), ..RelationshipType::default() });
        for i in 0..20_000 {
            p.element_types.push(ElementType { id: format!("big.T{i}"), ..ElementType::default() });
            p.glyphs.push(Glyph { id: format!("big.g{i}"), ..Glyph::default() });
        }
        p.connections.push(Connection {
            source: vec!["big.Missing".into(); 50_000],
            relationship: "big.R".into(),
            target: vec!["*".into()],
            ..Connection::default()
        });
        let start = std::time::Instant::now();
        let probs = check(&p);
        assert!(start.elapsed().as_secs_f64() < 2.0, "{:?}", start.elapsed());
        assert!(probs.len() <= MAX_PROBLEMS + 1);
        assert!(has(&probs, "the limit is 10000"));
        assert!(probs.last().unwrap().starts_with("and "));
    }

    #[test]
    fn ports_off_the_module_are_reported_for_the_pack_and_each_variant() {
        let pack = |module: &str, variant_module: &str, w: f64| {
            let json = format!(
                r##"{{"id": "modtest", "version": "1.0.0", "name": "M", "module": {module},
                "glyphs": [{{"id": "modtest.r", "label": "R", "category": "C", "outline": "rectangle", "w": {w}, "h": 19.685, "fill": "#ffffff", "line": "#000000"}},
                           {{"id": "modtest.r2", "label": "R", "category": "C", "outline": "rectangle", "w": 40, "h": 20, "fill": "#ffffff", "line": "#000000"}}],
                "elementTypes": [{{"id": "modtest.R", "glyph": "modtest.r", "ports": [{{"id": "a", "x": 0, "y": 0.5}}, {{"id": "b", "x": 1, "y": 0.5}}]}}],
                "variants": [{{"id": "modtest.ansi", "label": "ANSI", "module": {variant_module}, "glyphs": {{"modtest.R": "modtest.r2"}}}}]}}"##
            );
            serde_json::from_str::<Pack>(&json).unwrap()
        };
        // Four 2.5 mm modules wide, and 40 units is four 0.1 inch modules: clean.
        let ok = check(&pack("\"iso2_5mm\"", "\"inch0_1\"", 4.0 * vizcraft_model::Module::ISO_STEP));
        assert!(ok.iter().all(|p| !p.contains("module")), "{ok:?}");
        // 45 units is not a whole number of 2.5 mm modules.
        let off = check(&pack("\"iso2_5mm\"", "\"none\"", 45.0));
        assert!(has(&off, "port `b` of glyph `modtest.r` is not a whole number of modules"), "{off:?}");
        // The variant draws with its own glyph on the pack's module: 40 is not 2.5 mm modules.
        assert!(has(&off, "variant `modtest.ansi`"), "{off:?}");
        // No module: nothing to check.
        assert!(check(&pack("\"none\"", "\"none\"", 45.0)).iter().all(|p| !p.contains("module")));
    }
}
