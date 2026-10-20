//! The electrical sync pass: designations, contact links, rungs, terminal wires and coil
//! mirrors, kept in step with the drawing after every change. [`run`] is called from
//! `bind::sync` right before the sheet pass and does nothing in a document with no element of
//! the `electrical` pack.
//!
//! In order:
//!
//! 1. **Designations.** The document property `tagFormat` (`iec81346`, the default, `jic` or
//!    `y1444`) picks each device's class code (its `iecCode`, `jicCode` or `y1444Code`, which the
//!    pack defaults per type and the person may edit per element) and prefix (IEC 81346: the
//!    function `=` and location `+` it sits in, then `-`; Y14.44: the unit and assembly chain;
//!    JIC: none). Both are written to the element's `code` and `prefix`. A tag that reads as the
//!    old prefix, code and a number is recomposed at once with the same number, as EPLAN does
//!    (`K1` becomes `CR1` under JIC and back); a new device whose recomposed tag is taken gets the
//!    next free number of its code. Other tags are the person's and stay.
//! 2. **Contacts.** A coil retagged takes its linked contacts' tags with it; a contact with no tag
//!    and a link takes its coil's tag; then each contact is linked (`ContactOf`) to the coil whose
//!    tag it carries, and unlinked from any other.
//! 3. **Rungs.** A device drawn inside a ladder frame gets the `rung` of the band it sits in; one
//!    moved out of every frame loses it.
//! 4. **Terminals.** A terminal's `netIn` and `netOut` are the numbers (or names) of the nets on
//!    its two sides.
//! 5. **Mirrors.** Each view of a coil with linked contacts carries a mirror: on an IEC page a
//!    list under the coil, one row per contact (`13-14 /2.C3`); on an ANSI or JIC page the rungs
//!    of its make contacts to the right of the coil, and the rungs of its break contacts in a
//!    second, underlined label. A mirror the person moved stays where it was put.
//!
//! Running it again changes nothing.

use std::collections::{HashMap, HashSet};

use vizcraft_diagram::net::NetIndex;
use vizcraft_diagram::{Color, Document, Shape, ShapeKind, Style};
use vizcraft_model::{Element, PropValue, Relationship};

use crate::Catalog;
use crate::sheet_pass;

/// The pack this pass serves.
pub const PACK: &str = "electrical";

/// The identifier scope of devices.
pub const DEVICE_SCOPE: &str = "electrical.device";

/// The relationship from a contact to its coil.
pub const CONTACT_OF: &str = "electrical.ContactOf";

/// The ladder frame type.
pub const LADDER: &str = "electrical.Ladder";

/// The terminal type.
pub const TERMINAL: &str = "electrical.Terminal";

/// The function (`=`) and location (`+`) container types.
pub const FUNCTION: &str = "electrical.Function";
/// See [`FUNCTION`].
pub const LOCATION: &str = "electrical.Location";

/// The document property holding the designation format.
pub const TAG_FORMAT: &str = "tagFormat";

/// The designation formats `tagFormat` takes, the default first.
pub const TAG_FORMATS: &[&str] = &["iec81346", "jic", "y1444"];

/// `ext` key naming the coil view a mirror label belongs to.
pub const EXT_MIRROR_FOR: &str = "vizcraft.mirror.for";

/// `ext` key naming which mirror label it is: `list` (IEC), `no` or `nc` (ANSI and JIC).
pub const EXT_MIRROR_PART: &str = "vizcraft.mirror.part";

/// Font size of mirror labels.
const MIRROR_FONT: f64 = 8.0;

/// Most contacts one mirror lists.
const MAX_MIRROR_ROWS: usize = 32;

/// The project's designation format (`tagFormat`), the default when unset or unknown.
pub fn tag_format(doc: &Document) -> &'static str {
    let v = doc.properties.fields.get(TAG_FORMAT).map(String::as_str).unwrap_or_default();
    TAG_FORMATS.iter().copied().find(|f| *f == v).unwrap_or("iec81346")
}

/// The property holding the class code for a format.
pub fn code_key(format: &str) -> &'static str {
    match format {
        "jic" => "jicCode",
        "y1444" => "y1444Code",
        _ => "iecCode",
    }
}

/// The element types in the device scope, the contact types and the coil types (sources and
/// targets of `ContactOf` in the connection table).
struct Kinds {
    devices: HashSet<String>,
    contacts: HashSet<String>,
    coils: HashSet<String>,
}

fn kinds(cat: &Catalog) -> Option<Kinds> {
    let pack = cat.pack(PACK)?;
    let devices = pack.identifiers.iter().find(|s| s.id == DEVICE_SCOPE).map(|s| s.types.iter().cloned().collect()).unwrap_or_default();
    let mut contacts = HashSet::new();
    let mut coils = HashSet::new();
    for c in pack.connections.iter().filter(|c| c.relationship == CONTACT_OF) {
        contacts.extend(c.source.iter().cloned());
        coils.extend(c.target.iter().cloned());
    }
    Some(Kinds { devices, contacts, coils })
}

/// Run the pass (see the module docs).
pub fn run(doc: &mut Document, before: &Document, cat: &Catalog) {
    if !doc.model.elements.iter().any(|e| e.pack() == PACK) {
        return;
    }
    let Some(k) = kinds(cat) else { return };
    designations(doc, before, cat, &k);
    contacts(doc, before, &k);
    tag_labels(doc);
    rungs(doc);
    terminals(doc);
    mirrors(doc, &k);
}

/// A property's value as text, else the schema default of the element's type (or a supertype).
fn prop_or_default(cat: &Catalog, e: &Element, key: &str) -> String {
    if let Some(v) = e.prop(key) {
        return v.display();
    }
    crate::infer::chain_of(cat, &e.type_id)
        .iter()
        .find_map(|t| cat.element_type(t).and_then(|(_, et)| et.property(key)).and_then(|p| p.default.as_ref()))
        .map(PropValue::display)
        .unwrap_or_default()
}

/// The designation prefix of `e` in `format`.
fn prefix_of(doc: &Document, cat: &Catalog, e: &Element, format: &str) -> String {
    if format == "jic" {
        return String::new();
    }
    let chain: Vec<&Element> = doc.model.ancestors(&e.id).iter().filter_map(|a| doc.model.element(a)).collect();
    let is = |a: &Element, t: &str| crate::infer::chain_of(cat, &a.type_id).contains(&t);
    if format == "y1444" {
        // Outermost first: unit numeral, then assemblies, each without its aspect sign.
        let parts: Vec<String> = chain
            .iter()
            .rev()
            .filter(|a| is(a, FUNCTION) || is(a, LOCATION))
            .map(|a| a.tag.trim_start_matches(['=', '+', '-']).to_string())
            .filter(|t| !t.is_empty())
            .collect();
        return parts.concat();
    }
    let function = chain.iter().find(|a| is(a, FUNCTION)).map(|a| a.tag.clone()).unwrap_or_default();
    let location = chain.iter().find(|a| is(a, LOCATION)).map(|a| a.tag.clone()).unwrap_or_default();
    if function.is_empty() && location.is_empty() { String::new() } else { format!("{function}{location}-") }
}

/// The number in `tag` when it reads as `prefix`, `code` and digits.
fn number_in(tag: &str, prefix: &str, code: &str) -> Option<u64> {
    let digits = tag.strip_prefix(prefix)?.strip_prefix(code)?;
    if digits.is_empty() || digits.len() > 12 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn set_text(e: &mut Element, key: &str, value: &str) {
    if e.prop(key).map(PropValue::display).as_deref() != Some(value) {
        if value.is_empty() {
            e.properties.remove(key);
        } else if e.properties.contains_key(key) || e.properties.len() < vizcraft_model::MAX_PROPERTIES {
            e.properties.insert(key.to_string(), PropValue::Text(value.to_string()));
        }
    }
}

/// Step 1.
fn designations(doc: &mut Document, before: &Document, cat: &Catalog, k: &Kinds) {
    let format = tag_format(doc);
    let key = code_key(format);
    let existed: HashSet<&str> = before.model.elements.iter().map(|e| e.id.as_str()).collect();
    // (index, new code, new prefix, new tag)
    let mut plan: Vec<(usize, String, String, Option<String>)> = Vec::new();
    let mut taken: HashSet<String> = doc.model.elements.iter().filter(|e| k.devices.contains(&e.type_id)).map(|e| e.tag.clone()).collect();
    for (i, e) in doc.model.elements.iter().enumerate() {
        if !k.devices.contains(&e.type_id) {
            continue;
        }
        let code = prop_or_default(cat, e, key);
        let prefix = prefix_of(doc, cat, e, format);
        let old_code = prop_or_default(cat, e, "code");
        let old_prefix = e.prop("prefix").map(PropValue::display).unwrap_or_default();
        // The number the tag carries: read with the old code and prefix (or the new ones, for an
        // element whose code was never written), else with none at all.
        let n = number_in(&e.tag, &old_prefix, &old_code).or_else(|| number_in(&e.tag, &prefix, &code));
        let mut tag = n.map(|n| format!("{prefix}{code}{n}"));
        if let Some(t) = &tag
            && *t != e.tag
            && !existed.contains(e.id.as_str())
            && taken.contains(t)
        {
            // A new device: the next free number of its code.
            let next = doc
                .model
                .elements
                .iter()
                .filter(|o| k.devices.contains(&o.type_id))
                .filter_map(|o| number_in(&o.tag, &prefix, &code))
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            tag = Some(format!("{prefix}{code}{next}"));
        }
        if let Some(t) = &tag {
            if *t == e.tag {
                tag = None;
            } else {
                taken.insert(t.clone());
            }
        }
        if code != old_code || prefix != old_prefix || e.prop("code").is_none() || tag.is_some() {
            plan.push((i, code, prefix, tag));
        }
    }
    for (i, code, prefix, tag) in plan {
        let Some(e) = doc.model.elements.get_mut(i) else { continue };
        set_text(e, "code", &code);
        set_text(e, "prefix", &prefix);
        if let Some(t) = tag {
            e.tag = t.chars().take(vizcraft_model::MAX_TEXT).collect();
        }
    }
}

/// The tag slots builder symbols draw follow the tags this pass wrote: the rebuild pass ran
/// before it in this command and sees the new tags only in the next.
fn tag_labels(doc: &mut Document) {
    let tags: HashMap<String, String> =
        doc.model.elements.iter().filter(|e| e.pack() == PACK && !e.tag.is_empty()).map(|e| (e.id.clone(), e.tag.clone())).collect();
    for s in doc.pages.iter_mut().flat_map(|p| p.shapes.iter_mut()) {
        let Some(tag) = s.element.as_deref().and_then(|e| tags.get(e)) else { continue };
        let Some(g) = s.glyph.as_mut() else { continue };
        for l in g.labels.iter_mut().filter(|l| l.id == "tag" && l.text != *tag) {
            l.text = tag.clone();
        }
    }
}

/// Step 2.
fn contacts(doc: &mut Document, before: &Document, k: &Kinds) {
    // a. A retagged coil takes its contacts along.
    let old_tags: HashMap<&str, &str> = before.model.elements.iter().map(|e| (e.id.as_str(), e.tag.as_str())).collect();
    let retagged: HashMap<String, String> = doc
        .model
        .elements
        .iter()
        .filter(|e| k.coils.contains(&e.type_id) && old_tags.get(e.id.as_str()).is_some_and(|t| *t != e.tag))
        .map(|e| (e.id.clone(), e.tag.clone()))
        .collect();
    let links: Vec<(String, String)> =
        doc.model.relationships.iter().filter(|r| r.type_id == CONTACT_OF).map(|r| (r.source.clone(), r.target.clone())).collect();
    let tag_of: HashMap<String, String> = doc.model.elements.iter().map(|e| (e.id.clone(), e.tag.clone())).collect();
    for (contact, coil) in &links {
        let new_tag = match retagged.get(coil) {
            Some(t) => Some(t.clone()),
            // c. A linked contact with no tag of its own takes its coil's.
            None if tag_of.get(contact).is_some_and(String::is_empty) => tag_of.get(coil).cloned(),
            None => None,
        };
        if let Some(t) = new_tag
            && let Some(e) = doc.model.element_mut(contact)
            && k.contacts.contains(&e.type_id)
            && e.tag != t
        {
            e.tag = t;
        }
    }
    // b. Each contact is linked to the coil carrying its tag, and to no other.
    let mut coil_by_tag: HashMap<&str, &str> = HashMap::new();
    for e in doc.model.elements.iter().filter(|e| k.coils.contains(&e.type_id) && !e.tag.is_empty()) {
        coil_by_tag.entry(e.tag.as_str()).or_insert(e.id.as_str());
    }
    let wanted: HashMap<String, Option<String>> = doc
        .model
        .elements
        .iter()
        .filter(|e| k.contacts.contains(&e.type_id))
        .map(|e| (e.id.clone(), coil_by_tag.get(e.tag.as_str()).map(|c| (*c).to_string())))
        .collect();
    let mut have: HashSet<(String, String)> = HashSet::new();
    doc.model.relationships.retain(|r| {
        if r.type_id != CONTACT_OF {
            return true;
        }
        match wanted.get(&r.source) {
            Some(w) => w.as_deref() == Some(r.target.as_str()) && have.insert((r.source.clone(), r.target.clone())),
            // A link from something that is not a contact any more is left for the rules.
            None => true,
        }
    });
    let mut add: Vec<(String, String)> = wanted.into_iter().filter_map(|(c, w)| w.map(|w| (c, w))).filter(|pair| !have.contains(pair)).collect();
    add.sort();
    for (contact, coil) in add {
        let id = doc.next_id("r");
        let _ = doc.model.add_relationship(Relationship::new(id, CONTACT_OF, contact, coil));
    }
}

/// Step 3: rungs from the ladder frames.
fn rungs(doc: &mut Document) {
    /// A ladder frame's band on its page.
    struct Frame {
        page: usize,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        first: i64,
        inc: i64,
        rungs: i64,
    }
    let num = |e: &Element, k: &str, d: i64| e.prop(k).and_then(PropValue::as_f64).filter(|n| n.is_finite()).map(|n| n.round() as i64).unwrap_or(d);
    let mut frames: Vec<Frame> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            if e.type_id != LADDER {
                continue;
            }
            frames.push(Frame {
                page: pi,
                x0: s.x,
                x1: s.x + s.w,
                y0: s.y,
                y1: s.y + s.h,
                first: num(e, "first", 1),
                inc: num(e, "increment", 1).max(1),
                rungs: num(e, "rungs", 10).clamp(1, 200),
            });
        }
    }
    let mut wanted: HashMap<String, Option<i64>> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            if e.pack() != PACK || e.type_id == LADDER || wanted.get(&e.id).is_some_and(Option::is_some) {
                continue;
            }
            let c = s.center();
            let rung = frames.iter().filter(|f| f.page == pi && c.x >= f.x0 && c.x <= f.x1 && c.y >= f.y0 && c.y <= f.y1).find_map(|f| {
                let step = (f.y1 - f.y0) / f.rungs as f64;
                if !(step.is_finite() && step > 0.0) {
                    return None;
                }
                let i = (((c.y - f.y0) / step).floor() as i64).clamp(0, f.rungs - 1);
                Some(f.first.saturating_add(f.inc.saturating_mul(i)))
            });
            wanted.insert(e.id.clone(), rung);
        }
    }
    for e in doc.model.elements.iter_mut() {
        let Some(w) = wanted.get(&e.id) else { continue };
        match w {
            Some(r) => {
                let v = PropValue::Number(*r as f64);
                if e.prop("rung") != Some(&v) && (e.properties.contains_key("rung") || e.properties.len() < vizcraft_model::MAX_PROPERTIES) {
                    e.properties.insert("rung".into(), v);
                }
            }
            None => {
                e.properties.remove("rung");
            }
        }
    }
}

/// Step 4: a terminal's nets.
fn terminals(doc: &mut Document) {
    if !doc.model.elements.iter().any(|e| e.type_id == TERMINAL) {
        return;
    }
    let ix = NetIndex::build(doc);
    let net_text = |id: &str| doc.model.net(id).map(|n| if n.number.is_empty() { n.name.clone() } else { n.number.clone() }).unwrap_or_default();
    let mut plan: Vec<(String, String, String)> = Vec::new();
    for page in &doc.pages {
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            if e.type_id != TERMINAL {
                continue;
            }
            let side = |i: usize| ix.of_pin.get(&(s.id.clone(), i)).map(|n| net_text(n)).unwrap_or_default();
            plan.push((e.id.clone(), side(0), side(1)));
        }
    }
    for (id, a, b) in plan {
        if let Some(e) = doc.model.element_mut(&id) {
            set_text(e, "netIn", &a);
            set_text(e, "netOut", &b);
        }
    }
}

/// The text of each mirror label one coil view wants: `(part, text, underlined)`.
fn mirror_texts(doc: &Document, pi: usize, contacts: &[&Element], ansi: bool) -> Vec<(&'static str, String, bool)> {
    if ansi {
        let rung_of = |c: &Element| {
            c.prop("rung")
                .map(PropValue::display)
                .filter(|r| !r.is_empty())
                .or_else(|| sheet_pass::location(doc, &c.id).map(|l| l.zone))
                .unwrap_or_default()
        };
        let list = |nc: bool| -> String {
            let mut rs: Vec<String> = contacts
                .iter()
                .filter(|c| (c.prop("form").map(PropValue::display).as_deref() == Some("nc")) == nc)
                .map(|c| rung_of(c))
                .filter(|r| !r.is_empty())
                .take(MAX_MIRROR_ROWS)
                .collect();
            rs.dedup();
            rs.join(", ")
        };
        let mut out = Vec::new();
        let (no, nc) = (list(false), list(true));
        if !no.is_empty() {
            out.push(("no", no, false));
        }
        if !nc.is_empty() {
            out.push(("nc", nc, true));
        }
        return out;
    }
    let rows: Vec<String> = contacts
        .iter()
        .take(MAX_MIRROR_ROWS)
        .filter_map(|c| {
            let at = sheet_pass::location(doc, &c.id)?;
            let view = doc.pages.get(at.page_index)?.shape(&at.view)?;
            let ports: Vec<&str> = view.glyph.as_ref().map(|g| g.ports.iter().take(2).map(String::as_str).collect()).unwrap_or_default();
            let pins = if ports.is_empty() { "--".to_string() } else { ports.join("-") };
            Some(format!("{pins} {}", sheet_pass::location_text(doc, pi, &at, &c.tag)))
        })
        .collect();
    if rows.is_empty() { Vec::new() } else { vec![("list", rows.join("\n"), false)] }
}

/// Step 5: coil mirrors.
fn mirrors(doc: &mut Document, k: &Kinds) {
    let has_labels = doc.pages.iter().any(|p| p.shapes.iter().any(|s| s.ext.contains_key(EXT_MIRROR_FOR)));
    let mut by_coil: HashMap<&str, Vec<&Element>> = HashMap::new();
    for r in doc.model.relationships.iter().filter(|r| r.type_id == CONTACT_OF) {
        if let Some(c) = doc.model.element(&r.source) {
            by_coil.entry(r.target.as_str()).or_default().push(c);
        }
    }
    if by_coil.is_empty() && !has_labels {
        return;
    }
    // Wanted labels per (coil view, part): text and underline.
    let mut wanted: HashMap<(String, String), (String, bool)> = HashMap::new();
    let mut anchors: HashMap<String, (usize, bool)> = HashMap::new();
    for (pi, page) in doc.pages.iter().enumerate().filter(|(_, p)| !p.is_background) {
        let family = page.view.as_ref().map(|v| crate::template::family_of(&v.variant)).unwrap_or_default();
        let ansi = matches!(family, "ansi" | "jic");
        for s in &page.shapes {
            let Some(e) = s.element.as_deref().and_then(|e| doc.model.element(e)) else { continue };
            if !k.coils.contains(&e.type_id) {
                continue;
            }
            let Some(list) = by_coil.get(e.id.as_str()) else { continue };
            for (part, text, under) in mirror_texts(doc, pi, list, ansi) {
                wanted.insert((s.id.clone(), part.to_string()), (text, under));
            }
            anchors.insert(s.id.clone(), (pi, ansi));
        }
    }
    let key_of = |s: &Shape| -> Option<(String, String)> { Some((s.ext.get(EXT_MIRROR_FOR)?.clone(), s.ext.get(EXT_MIRROR_PART)?.clone())) };
    // New labels: (page, coil view, part) with ids handed out first.
    let mut missing: Vec<(usize, String, String, String)> = Vec::new();
    for (pi, page) in doc.pages.iter().enumerate() {
        let present: HashSet<(String, String)> = page.shapes.iter().filter_map(key_of).collect();
        let mut here: Vec<&(String, String)> =
            wanted.keys().filter(|(v, _)| anchors.get(v).is_some_and(|(p, _)| *p == pi) && page.shape(v).is_some()).collect();
        here.sort();
        for key in here {
            if !present.contains(key) {
                missing.push((pi, key.0.clone(), key.1.clone(), String::new()));
            }
        }
    }
    for m in &mut missing {
        m.3 = doc.next_id("s");
    }
    for page in doc.pages.iter_mut() {
        // One label per coil view and part: a pasted copy of a label is dropped.
        let mut seen: HashSet<(String, String)> = HashSet::new();
        page.shapes.retain(|s| match key_of(s) {
            Some(k) => wanted.contains_key(&k) && seen.insert(k),
            None => !s.ext.contains_key(EXT_MIRROR_FOR),
        });
        for s in page.shapes.iter_mut() {
            let Some(key) = key_of(s) else { continue };
            let Some((text, under)) = wanted.get(&key) else { continue };
            if s.text != *text || s.style.underline != *under {
                s.text = text.clone();
                s.style.underline = *under;
                let need = text.lines().count().max(1) as f64 * MIRROR_FONT * 1.3 + 4.0;
                if s.h < need {
                    s.h = need;
                }
                s.sanitize();
            }
        }
    }
    // Ladder frames per page: on an ANSI or JIC ladder the rung list goes right of the right rail.
    let ladders: Vec<(usize, vizcraft_diagram::Rect)> = doc
        .pages
        .iter()
        .enumerate()
        .flat_map(|(pi, p)| {
            p.shapes
                .iter()
                .filter(|s| s.element.as_deref().and_then(|e| doc.model.element(e)).is_some_and(|e| e.type_id == LADDER))
                .map(move |s| (pi, vizcraft_diagram::Rect::new(s.x, s.y, s.x + s.w, s.y + s.h)))
        })
        .collect();
    for (pi, view, part, id) in missing {
        let Some(page) = doc.pages.get_mut(pi) else { continue };
        let Some(v) = page.shape(&view) else { continue };
        let Some((text, under)) = wanted.get(&(view.clone(), part.clone())) else { continue };
        let ansi = anchors.get(&view).is_some_and(|(_, a)| *a);
        let c = v.center();
        let frame = ladders.iter().find(|(p, r)| *p == pi && c.x >= r.x0 && c.x <= r.x1 && c.y >= r.y0 && c.y <= r.y1).map(|(_, r)| r.x1);
        let label = new_mirror(id, v, &part, text, *under, ansi, frame);
        page.shapes.push(label);
    }
}

/// A mirror label for coil view `v`: under it (IEC) or to its right (ANSI and JIC; the break
/// list under the make list), right of the right rail `rail` when the coil is on a ladder.
fn new_mirror(id: String, v: &Shape, part: &str, text: &str, underline: bool, ansi: bool, rail: Option<f64>) -> Shape {
    let h = text.lines().count().max(1) as f64 * MIRROR_FONT * 1.3 + 4.0;
    let (x, y, w) = if ansi {
        let dy = if part == "nc" { MIRROR_FONT * 1.6 } else { 0.0 };
        // Beyond the right rail on a ladder, else clear of the symbol's own tag.
        let x = rail.map(|r| r + 10.0).unwrap_or(v.x + v.w + 36.0);
        (x, v.y + v.h / 2.0 - MIRROR_FONT * 1.2 + dy, 80.0)
    } else {
        (v.x, v.y + v.h + 14.0, v.w.max(70.0))
    };
    let mut s = Shape::new(id, ShapeKind::Rectangle, x, y, w, h);
    s.stencil = "vizcraft.text".to_string();
    s.style = Style { fill: Color::TRANSPARENT, line: Color::TRANSPARENT, line_width: 0.0, font_size: MIRROR_FONT, ..Style::DEFAULT };
    s.style.h_align = vizcraft_diagram::HAlign::Left;
    s.style.v_align = vizcraft_diagram::VAlign::Top;
    s.style.underline = underline;
    s.text = text.to_string();
    s.parent = Some(v.id.clone());
    s.layers = v.layers.clone();
    s.ext.insert(EXT_MIRROR_FOR.into(), v.id.clone());
    s.ext.insert(EXT_MIRROR_PART.into(), part.to_string());
    s.sanitize();
    s
}
