//! Pattern commands (`pattern.*`): list the patterns with their parameter schemas, insert one as
//! model elements laid out by its layout primitive, regenerate an instance with other arguments,
//! list the instances, forget one, and sync a table-driven one with its table ([`sync`]).
//!
//! An insertion is one command, so one undo step: the elements (tags from the identity registry,
//! or the pattern's tag prefix), their shapes, the relationships with glued connectors, the
//! regions drawn as containers, the layout, the move to the insertion point clear of the shapes
//! already there, the group gathering the shapes, and the [`PatternInstance`] in `Model.patterns`
//! recording the pattern, its arguments, the item playing each role and the names generated.
//! Regenerating keeps the items of roles both generations share, and keeps a name a person
//! changed (one that differs from the name generated last time).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{Value, json};
use vizcraft_diagram::autolayout::{self, Options};
use vizcraft_diagram::{Arrow, Connector, Document, End, Point, Rect, Route, arrange as ar, container};
use vizcraft_model::{MAX_TEXT, PatternInstance, Registry as IdRegistry, Relationship};
use vizcraft_patterns::{Emission, Pattern, PatternError};

use super::model::{NewElement, cat, create_element, cut, model_err, page_param, place_shape, rename_element, rename_relationship};
use super::{delete_on_page, num, opt_str, prune_selection, select, short, with_sel};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The pattern library, for layers above the engine (the Patterns pane and form read schemas).
pub use vizcraft_patterns as patterns;

pub mod sync;

/// Space kept between an inserted instance and the shapes already on the page.
pub const GAP: f64 = 40.0;

/// The pattern commands, in menu order.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pattern.list", "Patterns", "Insert > Patterns", list).params(
            r#"{"pack"?: pack id, "audience"?: string} -> {"patterns": [{"id", "name", "description", "pack", "packName", "audience", "params": [{"key", "label", "kind": "count|text|labels|choice|tagPrefix|table", "min"?, "max"?, "values"?, "columns"?, "default"}]}]}"#,
        ),
        CommandSpec::new("pattern.insert", "Insert Pattern", "Insert > Patterns", insert).params(
            r#"{"pattern": pattern id, "args"?: {key: value}, "page"?: page, "x"?: n, "y"?: n} -> {"instance", "pattern", "group", "members": {role: id}, "args"}; x, y: the top-left corner (default: centred on the view); moved clear of the shapes already there; one undo step"#,
        ),
        CommandSpec::new("pattern.reparameterize", "Change Pattern", "Insert > Patterns", reparameterize)
            .params(r#"{"instance": instance id (or an element or shape of it), "args": {key: value}} -> {"instance", "group", "members", "added": [role], "removed": [role], "args"}; args are merged over the current ones; names edited by hand are kept"#)
            .when(has_instances),
        CommandSpec::new("pattern.instances", "Pattern Instances", "Insert > Patterns", instances)
            .params(r#"{"page"?: page} -> {"instances": [{"id", "pattern", "name", "args", "members", "generated", "page", "group", "edited": [role]}]}"#),
        CommandSpec::new("pattern.sync", "Sync Pattern Table", "Insert > Patterns", sync::sync)
            .params(r#"{"instance": instance id (or an element or shape of it), "direction": "fromTable"|"toTable", "table"?: {"columns"?: [name], "rows": [[cell]]}, "path"?: csv or xlsx file} -> fromTable: the pattern.reparameterize result; toTable: {"instance", "direction", "table": {"columns", "rows"}}; for patterns with a table param; an ambiguous model (a step in two lanes, a node with two parents, a cell with two relationships) is refused"#)
            .when(has_instances),
        CommandSpec::new("pattern.forget", "Forget Pattern", "Insert > Patterns", forget)
            .params(r#"{"instance": instance id (or an element or shape of it)} -> {"forgotten"}; the elements and shapes stay"#)
            .when(has_instances),
    ]
}

/// Enabled when the model records at least one pattern instance.
pub fn has_instances(s: &Session) -> Option<&'static str> {
    if s.doc.model.patterns.is_empty() { Some("no pattern has been inserted") } else { None }
}

fn pattern_err(e: PatternError) -> EngineError {
    match e {
        PatternError::Unknown(_) | PatternError::Args(_) | PatternError::Pack(_) => EngineError::Params(e.to_string()),
        PatternError::Pattern(_) => EngineError::Failed(e.to_string()),
    }
}

/// The pattern named by `id`, among those the catalog offers.
fn offered(id: &str) -> Result<&'static Pattern, EngineError> {
    vizcraft_patterns::available(cat()).into_iter().find(|p| p.id == id).ok_or_else(|| EngineError::Params(format!("no pattern `{}`", short(id))))
}

/// A pattern as `pattern.list` reports it.
pub fn pattern_json(p: &Pattern) -> Value {
    let mut v = p.to_json();
    if let Some(o) = v.as_object_mut() {
        o.insert("packName".into(), json!(cat().pack(&p.pack).map(|k| k.name.clone()).unwrap_or_default()));
    }
    v
}

fn list(_s: &mut Session, v: &Value) -> CmdResult {
    let pack = opt_str(v, "pack")?;
    let audience = opt_str(v, "audience")?;
    let patterns: Vec<Value> = vizcraft_patterns::list(cat(), pack, audience).into_iter().map(pattern_json).collect();
    Ok(json!({"patterns": patterns}))
}

/// The instance a param names: an instance id, or an element, relationship or shape of one.
pub fn instance_param(s: &Session, v: &Value) -> Result<String, EngineError> {
    let r = opt_str(v, "instance")?.ok_or_else(|| EngineError::Params("`instance` is required".into()))?;
    let m = &s.doc.model;
    if let Some(p) = m.pattern(r) {
        return Ok(p.id.clone());
    }
    let item = vizcraft_diagram::views::element_of_shape(&s.doc, r).unwrap_or(r);
    if let Some(p) = m.pattern_of(item) {
        return Ok(p.id.clone());
    }
    if let Some(p) = m.patterns.iter().find(|p| p.group == r || p.frames.values().any(|f| f == r)) {
        return Ok(p.id.clone());
    }
    Err(EngineError::Params(format!("`{}` is no pattern instance", short(r))))
}

/// The instance a selection belongs to: the first selected shape that draws a member, is the
/// instance's group or one of its regions.
pub fn instance_of_selection(s: &Session) -> Option<String> {
    s.sel.items.iter().find_map(|id| instance_param(s, &json!({"instance": id})).ok())
}

/// Tags with a prefix: `<prefix><n>` counting up past every tag already used.
struct Tags {
    prefix: String,
    next: u64,
    taken: HashSet<String>,
}

impl Tags {
    fn new(doc: &Document, prefix: &str) -> Tags {
        Tags { prefix: prefix.to_string(), next: 1, taken: doc.model.elements.iter().map(|e| e.tag.clone()).collect() }
    }
    /// The next tag for an element of `type_id`, or `None` to let the registry number it (no
    /// prefix, or a type whose pack numbers no tags).
    fn for_type(&mut self, doc: &Document, type_id: &str) -> Option<String> {
        if self.prefix.is_empty() {
            return None;
        }
        let scopes = cat().scopes();
        IdRegistry::new(&doc.model, &scopes).scope_for(type_id)?;
        // Each try either returns or skips a taken tag, so `taken.len() + 1` tries always find one.
        for _ in 0..=self.taken.len() {
            let t = format!("{}{}", self.prefix, self.next);
            self.next = self.next.saturating_add(1);
            if self.taken.insert(t.clone()) {
                return Some(t);
            }
        }
        None
    }
}

/// The ids playing each role after [`realize`], and the shape drawing each role on the page.
#[derive(Default)]
struct Realized {
    members: BTreeMap<String, String>,
    frames: BTreeMap<String, String>,
    shapes: BTreeMap<String, String>,
}

/// The first shape on page `page` drawing element `id`.
fn shape_of(doc: &Document, page: usize, id: &str) -> Option<String> {
    doc.pages.get(page)?.shapes.iter().find(|sh| sh.element.as_deref() == Some(id)).map(|sh| sh.id.clone())
}

/// Make the document hold `em` on page `page`, reusing what `old` recorded where the role is
/// still played by an item of the right kind: kept names follow the new generation unless they
/// were edited by hand, missing shapes and connectors are drawn, and everything else is created
/// at `at`.
fn realize(doc: &mut Document, page: usize, em: &Emission, old: Option<&PatternInstance>, at: Point) -> Result<Realized, EngineError> {
    let empty = BTreeMap::new();
    let (old_members, old_generated, old_frames) = old.map_or((&empty, &empty, &empty), |o| (&o.members, &o.generated, &o.frames));
    let unedited = |role: &str, current: &str| old_generated.get(role).is_some_and(|g| g == current);
    let mut out = Realized::default();
    let mut tags = Tags::new(doc, &em.tag_prefix);
    for e in &em.elements {
        let kept = old_members.get(&e.role).filter(|id| doc.model.element(id).is_some_and(|x| x.type_id == e.type_id)).cloned();
        let id = match kept {
            Some(id) => {
                let current = doc.model.element(&id).map(|x| x.name.clone()).unwrap_or_default();
                if current != e.name && unedited(&e.role, &current) {
                    rename_element(doc, &id, &e.name)?;
                }
                id
            }
            None => {
                let owner = e.owner.as_ref().and_then(|o| out.members.get(o)).cloned();
                let tag = if e.tagged { tags.for_type(doc, &e.type_id) } else { None };
                create_element(doc, NewElement { type_id: e.type_id.clone(), name: e.name.clone(), tag, owner, ..NewElement::default() })?.0
            }
        };
        let shape = match shape_of(doc, page, &id) {
            Some(sh) => sh,
            None => place_shape(doc, &id, page, at.x, at.y, None, None)?,
        };
        out.members.insert(e.role.clone(), id);
        out.shapes.insert(e.role.clone(), shape);
    }
    for g in &em.regions {
        let members: Vec<String> = g.members.iter().filter_map(|m| out.shapes.get(m)).cloned().collect();
        let pg = doc.pages.get(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
        let kept = old_frames.get(&g.role).filter(|f| pg.shape(f).is_some_and(container::is_container)).cloned();
        let frame = match kept {
            Some(f) => {
                let joining: Vec<String> = members
                    .iter()
                    .filter(|m| doc.pages.get(page).and_then(|p| p.shape(m)).is_some_and(|sh| sh.parent.as_deref() != Some(f.as_str())))
                    .cloned()
                    .collect();
                if !joining.is_empty() {
                    let pg = doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
                    container::add_members(pg, &f, &joining).map_err(|e| EngineError::Failed(e.to_string()))?;
                }
                f
            }
            None => {
                let id = doc.next_id("s");
                let c = container::new_container(id.clone(), at.x, at.y, 200.0, 120.0, &cut(&g.label, MAX_TEXT));
                let pg = doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
                container::wrap(pg, c, &members).map_err(|e| EngineError::Failed(e.to_string()))?;
                id
            }
        };
        out.frames.insert(g.role.clone(), frame.clone());
        out.shapes.insert(g.role.clone(), frame);
    }
    for r in &em.relationships {
        let (Some(from), Some(to)) = (out.members.get(&r.from).cloned(), out.members.get(&r.to).cloned()) else {
            return Err(EngineError::Failed(format!("{}: {} joins a role with no element", em.pattern, r.role)));
        };
        let kept = old_members
            .get(&r.role)
            .filter(|id| doc.model.relationship(id).is_some_and(|x| x.type_id == r.type_id && x.source == from && x.target == to))
            .cloned();
        let id = match kept {
            Some(id) => {
                let current = doc.model.relationship(&id).map(|x| x.name.clone()).unwrap_or_default();
                if current != r.name && unedited(&r.role, &current) {
                    rename_relationship(doc, &id, &r.name)?;
                }
                id
            }
            None => {
                let id = doc.next_id("r");
                let mut rel = Relationship::new(id.clone(), r.type_id.clone(), from, to);
                rel.name = cut(&r.name, MAX_TEXT);
                if let Some((_, t)) = cat().relationship_type(&r.type_id) {
                    super::model::fill_defaults(&mut rel.properties, &t.properties);
                }
                let pack = cat().pack_ref(rel.pack());
                doc.model.add_relationship(rel).map_err(model_err)?;
                if let Some(p) = pack {
                    doc.model.use_pack(p);
                }
                id
            }
        };
        let drawn = doc.pages.get(page).is_some_and(|p| p.connectors.iter().any(|c| c.relationship.as_deref() == Some(id.as_str())));
        if !drawn && let (Some(a), Some(b)) = (out.shapes.get(&r.from).cloned(), out.shapes.get(&r.to).cloned()) {
            let cid = doc.next_id("c");
            let mut c = Connector::new(cid, End::dynamic(a), End::dynamic(b));
            c.route = Route::Orthogonal;
            if let Some(arrow) = cat().relationship_type(&r.type_id).and_then(|(_, t)| Arrow::from_name(&t.end_arrow)) {
                c.end_arrow = arrow;
            }
            c.text = doc.model.relationship(&id).map(|x| x.name.clone()).unwrap_or_default();
            c.relationship = Some(id.clone());
            c.sanitize();
            doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?.connectors.push(c);
        }
        out.members.insert(r.role.clone(), id);
    }
    Ok(out)
}

/// Where an instance goes: its top-left corner, or (for a missing coordinate) centred on `centre`.
struct Anchor {
    x: Option<f64>,
    y: Option<f64>,
    centre: Point,
}

/// Lay the instance's shapes out with the emission's primitive, move them to `anchor`, then push
/// them across the flow until they overlap no other shape. Returns the top-level shape ids.
fn arrange(doc: &mut Document, page: usize, em: &Emission, r: &Realized, anchor: &Anchor) -> Result<Vec<String>, EngineError> {
    let primitive = em.layout.clone().try_map(&mut |role: String| {
        r.shapes.get(&role).cloned().ok_or_else(|| EngineError::Failed(format!("{}: the layout names no shape for `{role}`", em.pattern)))
    })?;
    let ids: Vec<String> = r.shapes.values().cloned().collect();
    let opts = Options { direction: em.direction, ..Options::default() };
    let pg = doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
    autolayout::layout_primitive(pg, Some(&ids), primitive, &opts).map_err(EngineError::Failed)?;
    let top = ar::top_level(pg, &ids);
    let Some(b) = ar::bounds_of(pg, &top) else { return Ok(top) };
    let tx = anchor.x.unwrap_or(anchor.centre.x - b.width() / 2.0);
    let ty = anchor.y.unwrap_or(anchor.centre.y - b.height() / 2.0);
    super::arrange::move_shapes(pg, &top, tx - b.x0, ty - b.y0);
    // Clear of the shapes already there, the way `layout.incremental` places: across the flow.
    let mine: HashSet<String> = ar::with_descendants(pg, &top).into_iter().collect();
    let others: Vec<Rect> = pg
        .shapes
        .iter()
        .filter(|s| s.parent.is_none() && !mine.contains(&s.id))
        .map(|s| s.bounds())
        .filter(|r| r.x0.is_finite() && r.x1.is_finite() && r.y0.is_finite() && r.y1.is_finite())
        .collect();
    let across_x = !em.direction.is_horizontal();
    for _ in 0..=others.len() {
        let Some(b) = ar::bounds_of(pg, &top) else { break };
        let area = b.inflate(GAP / 2.0);
        let hit: Vec<&Rect> = others.iter().filter(|o| o.intersects(&area)).collect();
        if hit.is_empty() {
            break;
        }
        let (dx, dy) = if across_x {
            (hit.iter().map(|o| o.x1).fold(f64::MIN, f64::max) + GAP - b.x0, 0.0)
        } else {
            (0.0, hit.iter().map(|o| o.y1).fold(f64::MIN, f64::max) + GAP - b.y0)
        };
        super::arrange::move_shapes(pg, &top, dx, dy);
    }
    Ok(top)
}

/// Gather `top` in a group, `wanted` as its id when that id is free. Returns the group id, or
/// empty when the shapes cannot be grouped (they sit in different containers).
fn gather(doc: &mut Document, page: usize, top: &[String], wanted: Option<&str>) -> String {
    let gid = match wanted.filter(|g| !g.is_empty() && !doc.contains_id(g)) {
        Some(g) => g.to_string(),
        None => doc.next_id("g"),
    };
    let Some(pg) = doc.pages.get_mut(page) else { return String::new() };
    if top.is_empty() || ar::group(pg, top, &gid).is_err() { String::new() } else { gid }
}

/// A fresh instance id, unique among the document's ids and the recorded instances.
fn instance_id(doc: &mut Document) -> String {
    loop {
        let id = doc.next_id("pat");
        if doc.model.pattern(&id).is_none() {
            return id;
        }
    }
}

/// The point a missing coordinate centres the instance on: the view's centre with a canvas,
/// else the page frame's centre.
fn view_centre(s: &Session, page: usize) -> Point {
    if s.view.canvas.is_some() {
        return Point::new(f64::from(s.view.pan[0]), f64::from(s.view.pan[1]));
    }
    s.doc.pages.get(page).map(|p| p.frame().center()).unwrap_or(Point::new(0.0, 0.0))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let id = opt_str(v, "pattern")?.ok_or_else(|| EngineError::Params("`pattern` (a pattern id from pattern.list) is required".into()))?;
    let p = offered(id)?;
    let args = p.check(v.get("args").unwrap_or(&Value::Null)).map_err(pattern_err)?;
    let em = vizcraft_patterns::generate(p, &args, cat()).map_err(pattern_err)?;
    if s.doc.model.patterns.len() >= vizcraft_model::pattern::MAX_INSTANCES {
        return Err(EngineError::Params("the model already records the most pattern instances it can".into()));
    }
    let page = page_param(&s.doc, v)?;
    let clamp = |n: Option<f64>| n.map(super::shape::coord);
    let anchor = Anchor { x: clamp(num(v, "x")?), y: clamp(num(v, "y")?), centre: view_centre(s, page) };
    let start = Point::new(anchor.x.unwrap_or(anchor.centre.x), anchor.y.unwrap_or(anchor.centre.y));
    let mut r = realize(&mut s.doc, page, &em, None, start)?;
    let top = arrange(&mut s.doc, page, &em, &r, &anchor)?;
    let top = sync::after_arrange(&mut s.doc, page, &p.id, &args, &mut r, top)?;
    let group = gather(&mut s.doc, page, &top, None);
    let instance = instance_id(&mut s.doc);
    let page_id = s.doc.pages.get(page).map(|p| p.id.clone()).unwrap_or_default();
    s.doc.model.patterns.push(PatternInstance {
        id: instance.clone(),
        pattern: p.id.clone(),
        args: args.to_value(),
        members: r.members.clone(),
        generated: em.generated(),
        page: page_id,
        group: group.clone(),
        frames: r.frames,
    });
    if page == s.doc.active_index() {
        select(s, vec![if group.is_empty() { top.first().cloned().unwrap_or_default() } else { group.clone() }]);
        prune_selection(s);
    }
    with_sel(s, json!({"instance": instance, "pattern": p.id, "group": group, "members": r.members, "args": args.to_value()}))
}

fn reparameterize(s: &mut Session, v: &Value) -> CmdResult {
    let iid = instance_param(s, v)?;
    let old = s.doc.model.pattern(&iid).cloned().ok_or_else(|| EngineError::Failed("the instance went missing".into()))?;
    let p = offered(&old.pattern)?;
    let mut merged = match &old.args {
        Value::Object(o) => o.clone(),
        _ => serde_json::Map::new(),
    };
    match v.get("args") {
        Some(Value::Object(o)) => merged.extend(o.iter().map(|(k, v)| (k.clone(), v.clone()))),
        None | Some(Value::Null) => {}
        Some(_) => return Err(EngineError::Params("`args` must be an object".into())),
    }
    let args = p.check(&Value::Object(merged)).map_err(pattern_err)?;
    let em = vizcraft_patterns::generate(p, &args, cat()).map_err(pattern_err)?;
    let page = s.doc.find_page(&old.page).unwrap_or_else(|| s.doc.active_index());

    // Where the instance is now, so it stays there.
    let old_shapes: Vec<String> = old.members.values().filter_map(|id| shape_of(&s.doc, page, id)).chain(old.frames.values().cloned()).collect();
    let pg = s.doc.pages.get_mut(page).ok_or_else(|| EngineError::Failed("the page went missing".into()))?;
    let corner = ar::bounds_of(pg, &old_shapes).map(|b| Point::new(b.x0, b.y0));
    if !old.group.is_empty() && pg.shape(&old.group).is_some_and(ar::is_group) {
        ar::ungroup(pg, std::slice::from_ref(&old.group));
    }

    // Roles the new generation drops, or plays with another type, go with their views.
    let roles: BTreeSet<&str> = em.elements.iter().map(|e| e.role.as_str()).chain(em.relationships.iter().map(|r| r.role.as_str())).collect();
    let mut gone: Vec<String> = Vec::new();
    let mut removed_roles: Vec<String> = Vec::new();
    for (role, id) in &old.members {
        let kept = roles.contains(role.as_str())
            && match em.element_by_role(role) {
                Some(e) => s.doc.model.element(id).is_some_and(|x| x.type_id == e.type_id),
                None => true,
            };
        if !kept {
            removed_roles.push(role.clone());
            gone.push(id.clone());
        }
    }
    let removed = s.doc.model.remove(&gone);
    let ids: Vec<String> = removed.elements.into_iter().chain(removed.relationships).collect();
    let doomed = vizcraft_diagram::views::items_referencing(&s.doc, &ids);
    if !doomed.is_empty() {
        for pg in &mut s.doc.pages {
            delete_on_page(pg, &doomed);
        }
    }
    for (role, frame) in &old.frames {
        if !em.regions.iter().any(|g| &g.role == role)
            && let Some(pg) = s.doc.pages.get_mut(page)
            && pg.shape(frame).is_some_and(container::is_container)
        {
            let _ = container::disband(pg, frame);
            removed_roles.push(role.clone());
        }
    }

    let centre = view_centre(s, page);
    let at = corner.unwrap_or(centre);
    let mut r = realize(&mut s.doc, page, &em, Some(&old), at)?;
    let anchor = Anchor { x: Some(at.x), y: Some(at.y), centre };
    let top = arrange(&mut s.doc, page, &em, &r, &anchor)?;
    let top = sync::after_arrange(&mut s.doc, page, &p.id, &args, &mut r, top)?;
    let group = gather(&mut s.doc, page, &top, Some(&old.group));
    let added: Vec<&String> = r.members.keys().filter(|role| old.members.get(*role).is_none_or(|id| r.members.get(*role) != Some(id))).collect();
    let added: Vec<String> = added.into_iter().cloned().collect();
    let page_id = s.doc.pages.get(page).map(|p| p.id.clone()).unwrap_or_default();
    let rec = s.doc.model.pattern_mut(&iid).ok_or_else(|| EngineError::Failed("the instance went missing".into()))?;
    rec.args = args.to_value();
    rec.members = r.members.clone();
    rec.generated = em.generated();
    rec.page = page_id;
    rec.group = group.clone();
    rec.frames = r.frames;
    if page == s.doc.active_index() && !group.is_empty() {
        select(s, vec![group.clone()]);
    }
    prune_selection(s);
    with_sel(s, json!({"instance": iid, "group": group, "members": r.members, "added": added, "removed": removed_roles, "args": args.to_value()}))
}

/// Roles whose current name differs from the one generated for them: edited by hand.
pub fn edited_roles(doc: &Document, p: &PatternInstance) -> Vec<String> {
    p.members
        .iter()
        .filter(|(role, id)| {
            let current = doc.model.element(id).map(|e| e.name.as_str()).or_else(|| doc.model.relationship(id).map(|r| r.name.as_str()));
            current.is_some_and(|c| p.generated.get(*role).is_some_and(|g| g != c))
        })
        .map(|(role, _)| role.clone())
        .collect()
}

/// One instance as `pattern.instances` reports it.
pub fn instance_json(doc: &Document, p: &PatternInstance) -> Value {
    let name = vizcraft_patterns::find(&p.pattern).map(|x| x.name.clone()).unwrap_or_else(|| p.pattern.clone());
    json!({
        "id": p.id,
        "pattern": p.pattern,
        "name": name,
        "args": p.args,
        "members": p.members,
        "generated": p.generated,
        "page": p.page,
        "group": p.group,
        "edited": edited_roles(doc, p),
    })
}

fn instances(s: &mut Session, v: &Value) -> CmdResult {
    let page = match opt_str(v, "page")? {
        None => None,
        Some(_) => s.doc.pages.get(page_param(&s.doc, v)?).map(|p| p.id.clone()),
    };
    let list: Vec<Value> =
        s.doc.model.patterns.iter().filter(|p| page.as_ref().is_none_or(|pg| &p.page == pg)).map(|p| instance_json(&s.doc, p)).collect();
    Ok(json!({"instances": list}))
}

fn forget(s: &mut Session, v: &Value) -> CmdResult {
    let iid = instance_param(s, v)?;
    s.doc.model.patterns.retain(|p| p.id != iid);
    Ok(json!({"forgotten": iid}))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};

    use super::super::model::fixture::{blank, refused, run};
    use crate::Session;

    /// The member ids of an insert or reparameterize result.
    fn members(r: &Value) -> BTreeMap<String, String> {
        serde_json::from_value(r["members"].clone()).unwrap()
    }

    /// The shapes on the active page drawing elements, with their bounds.
    fn element_shapes(s: &Session) -> Vec<(String, vizcraft_diagram::Rect)> {
        s.doc.page().unwrap().shapes.iter().filter(|sh| sh.element.is_some()).map(|sh| (sh.id.clone(), sh.bounds())).collect()
    }

    fn no_overlaps(s: &Session, what: &str) {
        let shapes = element_shapes(s);
        for (i, (a, ra)) in shapes.iter().enumerate() {
            for (b, rb) in shapes.iter().skip(i + 1) {
                assert!(!ra.inflate(-0.5).intersects(&rb.inflate(-0.5)), "{what}: {a} {ra:?} overlaps {b} {rb:?}");
            }
        }
    }

    /// Issues at warning or error level.
    fn serious(s: &mut Session) -> Vec<Value> {
        let r = run(s, "validate.run", json!({}));
        r["issues"].as_array().unwrap().iter().filter(|i| i["severity"] != "info").cloned().collect()
    }

    #[test]
    fn each_starter_pattern_inserts_bound_elements_that_validate_clean_without_overlaps_in_one_undo_step() {
        let list = run(&mut blank(), "pattern.list", json!({}));
        let ids: Vec<String> = list["patterns"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids.len(), 8, "the five starter patterns and the three table-driven ones");
        for id in ids {
            let mut s = blank();
            let r = run(&mut s, "pattern.insert", json!({"pattern": id, "x": 100, "y": 80}));
            let m = members(&r);
            for (role, item) in &m {
                if let Some(e) = s.doc.model.element(item) {
                    assert!(e.type_id.starts_with("flowchart."), "{id} {role}");
                    let shapes = vizcraft_diagram::views::views_of(&s.doc, item);
                    assert_eq!(shapes.len(), 1, "{id} {role} is drawn once");
                } else {
                    let rel = s.doc.model.relationship(item).unwrap_or_else(|| panic!("{id} {role} names nothing"));
                    let c = s.doc.page().unwrap().connectors.iter().find(|c| c.relationship.as_deref() == Some(item)).unwrap();
                    let ends = (
                        c.start.shape().and_then(|x| s.doc.shape(x)).and_then(|x| x.element.clone()),
                        c.end.shape().and_then(|x| s.doc.shape(x)).and_then(|x| x.element.clone()),
                    );
                    assert_eq!(ends, (Some(rel.source.clone()), Some(rel.target.clone())), "{id} {role} is glued at both ends");
                }
            }
            // A tree drawn with Flowchart's types has steps with several flows out, which the
            // single-exit rule flags; the defaults are only the types the Flowchart pack has.
            if id != "table.tree" {
                assert_eq!(serious(&mut s), Vec::<Value>::new(), "{id} validates clean");
            }
            no_overlaps(&s, &id);
            let group = r["group"].as_str().unwrap();
            assert!(s.doc.page().unwrap().shape(group).is_some_and(vizcraft_diagram::arrange::is_group), "{id} is grouped");
            let b = vizcraft_diagram::arrange::bounds_of(s.doc.page().unwrap(), &[group.to_string()]).unwrap();
            assert!((b.x0 - 100.0).abs() < 1.0 && (b.y0 - 80.0).abs() < 1.0, "{id} lands at the insertion point: {b:?}");
            assert_eq!(s.doc.model.patterns.len(), 1);
            run(&mut s, "edit.undo", json!({}));
            assert!(
                s.doc.model.elements.is_empty() && s.doc.model.patterns.is_empty() && s.doc.page().unwrap().shapes.is_empty(),
                "{id}: one undo step"
            );
        }
    }

    #[test]
    fn a_parallel_split_of_four_grows_to_six_keeping_a_hand_edit_and_shrinks_to_three() {
        let mut s = blank();
        let r = run(&mut s, "pattern.insert", json!({"pattern": "workflow.parallelSplit", "args": {"branches": 4}}));
        let inst = r["instance"].as_str().unwrap().to_string();
        let m = members(&r);
        let b2 = m["branch.2"].clone();
        run(&mut s, "element.rename", json!({"id": b2, "name": "Check stock"}));
        let r = run(&mut s, "pattern.reparameterize", json!({"instance": inst, "args": {"branches": 6}}));
        let m6 = members(&r);
        assert_eq!(m6["branch.2"], b2, "a kept role keeps its element");
        assert_eq!(s.doc.model.element(&b2).unwrap().name, "Check stock", "the hand edit is kept");
        assert_eq!(s.doc.model.element(&m6["branch.6"]).unwrap().name, "Branch 6");
        assert_eq!(r["added"].as_array().unwrap().len(), 6, "two branches and their two flows each");
        assert_eq!(s.doc.model.elements.len(), 8);
        assert_eq!(element_shapes(&s).len(), 8);
        assert_eq!(serious(&mut s), Vec::<Value>::new());
        no_overlaps(&s, "six");
        let listed = run(&mut s, "pattern.instances", json!({}));
        assert_eq!(listed["instances"][0]["edited"], json!(["branch.2"]));
        assert_eq!(listed["instances"][0]["args"]["branches"], 6);

        // Back to three: the extra branches go with their shapes and flows.
        let gone: Vec<String> = ["branch.4", "branch.5", "branch.6"].iter().map(|r| m6[*r].clone()).collect();
        let r = run(&mut s, "pattern.reparameterize", json!({"instance": b2, "args": {"branches": 3}}));
        assert_eq!(r["removed"].as_array().unwrap().len(), 9);
        assert_eq!(s.doc.model.elements.len(), 5);
        assert_eq!(element_shapes(&s).len(), 5);
        assert!(gone.iter().all(|id| s.doc.model.element(id).is_none() && vizcraft_diagram::views::views_of(&s.doc, id).is_empty()));
        assert_eq!(s.doc.model.relationships.len(), 6);
        assert_eq!(s.doc.page().unwrap().connectors.len(), 6);
        assert_eq!(s.doc.model.element(&b2).unwrap().name, "Check stock");
        // Each change was one step.
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.model.elements.len(), 8);
    }

    #[test]
    fn bad_patterns_and_args_are_refused() {
        let mut s = blank();
        for bad in [
            json!({}),
            json!({"pattern": "nope"}),
            json!({"pattern": 5}),
            json!({"pattern": "workflow.parallelSplit", "args": {"branches": 1}}),
            json!({"pattern": "workflow.parallelSplit", "args": {"branches": 99}}),
            json!({"pattern": "workflow.parallelSplit", "args": {"nope": 1}}),
            json!({"pattern": "workflow.parallelSplit", "args": {"labels": "x"}}),
            json!({"pattern": "workflow.parallelSplit", "args": [1]}),
            json!({"pattern": "workflow.nOfMJoin", "args": {"branches": 2, "required": 3}}),
            json!({"pattern": "workflow.parallelSplit", "x": "a"}),
            json!({"pattern": "workflow.parallelSplit", "page": "nope"}),
        ] {
            refused(&mut s, "pattern.insert", bad);
        }
        let r = run(&mut s, "pattern.insert", json!({"pattern": "workflow.exclusiveChoice"}));
        let inst = r["instance"].as_str().unwrap();
        refused(&mut s, "pattern.reparameterize", json!({"instance": inst, "args": {"branches": 0}}));
        refused(&mut s, "pattern.reparameterize", json!({"instance": "nope", "args": {}}));
        refused(&mut s, "pattern.forget", json!({"instance": "nope"}));
    }

    #[test]
    fn list_carries_the_schemas_and_filters() {
        let mut s = blank();
        let all = run(&mut s, "pattern.list", json!({}));
        let p = all["patterns"].as_array().unwrap().iter().find(|p| p["id"] == "workflow.parallelSplit").unwrap();
        assert_eq!(p["packName"], "Flowchart");
        assert_eq!(p["params"][0], json!({"key": "branches", "label": "Branches", "kind": "count", "min": 2, "max": 32, "default": 3}));
        assert!(run(&mut s, "pattern.list", json!({"pack": "basic"}))["patterns"].as_array().unwrap().is_empty());
        assert_eq!(run(&mut s, "pattern.list", json!({"audience": "enterprise"}))["patterns"].as_array().unwrap().len(), 5);
        let hits = run(&mut s, "suggest.search", json!({"query": "parallel"}));
        assert!(hits["hits"].as_array().unwrap().iter().any(|h| h["kind"] == "pattern" && h["id"] == "workflow.parallelSplit"), "{hits}");
    }

    #[test]
    fn a_second_instance_lands_clear_of_the_first_and_forget_keeps_the_elements() {
        let mut s = blank();
        run(&mut s, "pattern.insert", json!({"pattern": "workflow.multipleInstances", "x": 0, "y": 0}));
        let r = run(&mut s, "pattern.insert", json!({"pattern": "workflow.cancelRegion", "args": {"steps": 3}, "x": 0, "y": 0}));
        no_overlaps(&s, "two instances");
        assert_eq!(s.doc.model.patterns.len(), 2);
        let region = s.doc.model.patterns[1].frames["region"].clone();
        let page = s.doc.page().unwrap();
        let members: Vec<_> = page.shapes.iter().filter(|sh| sh.parent.as_deref() == Some(region.as_str())).collect();
        assert_eq!(members.len(), 4, "three steps and the decision sit in the region");
        let inst = r["instance"].as_str().unwrap().to_string();
        run(&mut s, "pattern.reparameterize", json!({"instance": region, "args": {"steps": 1}}));
        assert_eq!(s.doc.page().unwrap().shapes.iter().filter(|sh| sh.parent.as_deref() == Some(region.as_str())).count(), 2);
        let n = s.doc.model.elements.len();
        run(&mut s, "pattern.forget", json!({"instance": inst}));
        assert_eq!((s.doc.model.patterns.len(), s.doc.model.elements.len()), (1, n));
    }

    #[test]
    fn a_tag_prefix_numbers_the_steps() {
        let mut s = blank();
        let r = run(&mut s, "pattern.insert", json!({"pattern": "workflow.parallelSplit", "args": {"branches": 2, "tagPrefix": "PAY-"}}));
        let m = members(&r);
        let tags: Vec<&str> = ["split", "branch.1", "branch.2", "join"].iter().map(|r| s.doc.model.element(&m[*r]).unwrap().tag.as_str()).collect();
        assert_eq!(tags, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4"]);
        let r = run(&mut s, "pattern.insert", json!({"pattern": "workflow.parallelSplit"}));
        let split = members(&r)["split"].clone();
        assert_eq!(s.doc.model.element(&split).unwrap().tag, "S1", "no prefix: the pack numbers them");
    }
}
