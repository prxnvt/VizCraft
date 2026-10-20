//! Document type and example commands. `template.*` lists the document types the notation packs
//! offer (empty documents set up for a kind of work, see `vizcraft_notation::template`), starts a
//! document or adds a page of one, previews one as SVG, and reads or sets the project's standard
//! choices (symbol family and sheet preset, `Document.standards`). `example.*` lists and opens
//! the filled-in examples (Help > Examples). For scripts written before document types,
//! `template.new`, `template.addPage` and `template.preview` also take an example id.
//!
//! A document type or example becomes commands (the pipeline, see [`plan`]): the viewpoint, the
//! rules (the conformance profile, when the type names rulesets or live checking), the sheet and
//! module hooks, the seed (examples only), the rendering variant (after the seed, so the seeded
//! shapes are redrawn in it), the layout (when the seed drew something), then the title block and
//! key hooks. A hook is a command another part of the app provides (`sheet.set`, `page.setModule`,
//! `sheet.titleBlock`, `key.insert`); when it is not registered the step is skipped and named in
//! the result's `skipped`, so templates work before those commands exist.
//!
//! The pipeline first runs in a scratch session, step by step, which resolves the seed's
//! `${name.path}` references from earlier results and proves the template works. The resolved
//! steps then run again in the user's session as one transaction: one undo step, and nothing
//! changed when any step fails. Ids come out the same both times because both runs start from
//! the same document.

use std::collections::HashMap;

use serde_json::{Map, Value, json};
use vizcraft_diagram::standards::{DEFAULT_SYMBOLS, MAX_STANDARD, Standards};
use vizcraft_notation::Pack;
use vizcraft_notation::template::{self as tpl, Module, Template};

use super::model::cat;
use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, Document, EngineError, Session};

/// Each optional pipeline step and the command that performs it, agreed with the branches that
/// provide them. A step whose command is not registered is skipped.
pub const HOOKS: &[(&str, &str)] = &[("sheet", "sheet.set"), ("module", "page.setModule"), ("titleBlock", "sheet.titleBlock"), ("key", "key.insert")];

/// The command that lists the sheet presets, when one is registered.
pub const SHEET_PRESETS: &str = "sheet.presets";

/// The symbol families `template.standards` offers, IEC first.
pub const SYMBOL_CHOICES: &[&str] = &["iec", "ansi", "jic", "ieee", "isa", "iso"];

/// The sheet presets offered when no `sheet.presets` command is registered, ISO A3 first.
pub const SHEET_CHOICES: &[&str] = &["iso.a3.landscape", "asme.b.landscape"];

/// The orientation a two-part sheet preset id (`iso.a3`) is completed with.
const DEFAULT_ORIENTATION: &str = "landscape";

/// Largest preview `template.preview` returns.
pub const MAX_PREVIEW: usize = 2 * 1024 * 1024;

/// Longest output path `template.new` takes, in bytes.
const MAX_PATH: usize = 4096;

/// Longest search text `template.list` takes, in bytes.
const MAX_QUERY: usize = 256;

/// Return the template commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("template.list", "Document Types", "File > New", list).params(
            r#"{"audience"?: string, "pack"?: pack id, "query"?: string} -> {"templates": [{"id", "label", "description", "pack", "packName", "discipline", "audience", "keywords", "viewpoint", "viewpoints": [{"id", "label", "rulesets"}], "rulesets": [{"id", "label"}], "liveChecks"?, "codeForms", "key", "titleBlock", "variant", "variants": [{"id", "label", "family"}], "sheet", "module"}]}; the document types, in catalog order; rulesets are the ones the default viewpoint holds the document to (the pack's first when the type names none); the query matches label, keywords, id and description, ignoring case"#,
        ),
        CommandSpec::new("template.new", "New Document", "File > New", new).params(
            r#"{"template": document type id (an example id draws the example, as example.open), "path"?: file to save to, "viewpoint"?: one of the type's viewpoints, "variant"?: variant id, symbol family ("jic") or "" for the default drawing, "sheet"?: "<standard>.<size>[.<orientation>]" or "" for none, "standards"?: {"symbols"?: symbol family, "sheet"?: sheet preset id}} -> {"template", "page", "viewpoint", "variant", "sheet", "rulesets", "applied": [command], "skipped": [command], "primary", "items", "path"?}; replaces the document with an empty one of the type (one undo step back to a blank page): its viewpoint, rules, variant, sheet, title block, module and key, and records the project's standards: the given `standards`, else the variant's family when it is one of template.standards' symbolChoices and the sheet; with `standards` the variant follows its family when the pack draws it and the sheet its sheet when the type has one; variant default: the type's, else the pack's IEC variant, else the default drawing"#,
        ),
        CommandSpec::new("template.addPage", "New Page of a Document Type", "Insert > Pages", add_page).params(
            r#"{"template": document type id (or example id), "viewpoint"?: one of its viewpoints, "variant"?: variant id or family, "sheet"?: sheet preset id or "", "name"?: page name} -> {"template", "page", "viewpoint", "variant", "sheet", "rulesets", "applied", "skipped", "primary", "items"}; adds a page of the type in one undo step; its rulesets join the ones the document is held to; variant and sheet default to the project's standards"#,
        ),
        CommandSpec::new("example.list", "Examples", "Help > Examples", list_examples).params(
            r#"{"audience"?: string, "pack"?: pack id, "query"?: string} -> {"examples": [rows as template.list]}; the filled-in examples the packs offer, in catalog order"#,
        ),
        CommandSpec::new("example.open", "Open Example", "Help > Examples", open_example).params(
            r#"{"example": example id, "path"?, "viewpoint"?, "variant"?, "sheet"?, "standards"?} -> as template.new; replaces the document with a new one drawn from the example (one undo step back to a blank page), with the same choices as template.new"#,
        ),
        CommandSpec::new("template.standards", "Project Standards", "Design > Standards", standards).params(
            r#"{"symbols"?: symbol family, "sheet"?: sheet preset id} -> {"symbols", "sheet", "symbolChoices", "sheetChoices"}; with neither it reads them; setting is one undo step; "" restores the default (IEC, ISO A3 landscape); sheetChoices come from sheet.presets when that command is registered"#,
        ),
        CommandSpec::new("template.preview", "Preview Template", "File > New", preview).params(
            r#"{"template": document type or example id, "viewpoint"?, "variant"?: variant id or family, "sheet"?: sheet preset id or "", "standards"?: {"symbols"?, "sheet"?}} -> {"template", "viewpoint", "variant", "sheet", "svg", "width", "height"}; draws the type (or example) in a scratch document, choosing as template.new does, and returns its first page as SVG text; changes nothing"#,
        ),
    ]
}

/// What a caller chose for a document type or example: `None` takes the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    /// One of the type's viewpoints.
    pub viewpoint: Option<String>,
    /// A variant id, a symbol family, or "" for the default drawing.
    pub variant: Option<String>,
    /// A sheet preset id, or "" for no sheet.
    pub sheet: Option<String>,
}

/// The document type with this id and its pack, or, for callers that predate document types, the
/// example with this id.
pub fn lookup(id: &str) -> Result<(&'static Pack, &'static Template), EngineError> {
    let unknown = || EngineError::Params(format!("unknown document type `{}`; template.list lists them", short(id)));
    let pack = cat().pack(vizcraft_model::pack_of(id)).ok_or_else(unknown)?;
    let t = tpl::find(pack, id).or_else(|| tpl::find_example(pack, id)).ok_or_else(unknown)?;
    Ok((pack, t))
}

/// The example with this id and its pack.
pub fn lookup_example(id: &str) -> Result<(&'static Pack, &'static Template), EngineError> {
    let unknown = || EngineError::Params(format!("unknown example `{}`; example.list lists them", short(id)));
    let pack = cat().pack(vizcraft_model::pack_of(id)).ok_or_else(unknown)?;
    let t = tpl::find_example(pack, id).ok_or_else(unknown)?;
    Ok((pack, t))
}

/// Draw `template` (a document type or an example) in a new document with `choices`, as
/// `template.new` would, and return it. For callers that want the document itself (a gallery
/// drawing previews without SVG).
pub fn build(template: &Template, choices: &Choices) -> Result<Document, EngineError> {
    let (pack, t) = lookup(&template.id)?;
    if t != template {
        return Err(EngineError::Params(format!("`{}` is not the document type the catalog holds", short(&template.id))));
    }
    let base = Session::new(Document::new());
    let (variant, sheet) = resolve(pack, t, choices, None)?;
    let viewpoint = viewpoint_of(t, choices)?;
    Ok(plan(&base, Document::new(), false, t, &viewpoint, &variant, &sheet, Vec::new())?.doc)
}

/// The variant and sheet `template.new` would draw document type (or example) `id` with, given
/// `choices` and, when the caller passes them, the project's `standards` (as `template.new`'s
/// `standards` param).
pub fn resolved(id: &str, choices: &Choices, project: Option<&Standards>) -> Result<(String, String), EngineError> {
    let (pack, t) = lookup(id)?;
    resolve(pack, t, choices, project)
}

/// The viewpoint a document of `t` starts on: the caller's choice when the type offers it, else
/// the type's own.
pub fn viewpoint_of(t: &Template, choices: &Choices) -> Result<String, EngineError> {
    match choices.viewpoint.as_deref() {
        None | Some("") => Ok(t.viewpoint.clone()),
        Some(v) if t.offers_viewpoint(v) => Ok(v.to_string()),
        Some(v) => {
            Err(EngineError::Params(format!("`{}` is not a viewpoint {} offers; it offers {}", short(v), t.label, t.viewpoint_choices().join(", "))))
        }
    }
}

/// The template id param (a document type, or an example for older callers), checked against
/// the catalog.
fn template_param(v: &Value) -> Result<(&'static Pack, &'static Template), EngineError> {
    let id = opt_str(v, "template")?.ok_or_else(|| EngineError::Params("`template` (a document type id from template.list) is required".into()))?;
    if id.len() > tpl::MAX_ID {
        return Err(EngineError::Params(format!("`template` is longer than {} bytes", tpl::MAX_ID)));
    }
    lookup(id)
}

/// The example id param, checked against the catalog.
fn example_param(v: &Value) -> Result<(&'static Pack, &'static Template), EngineError> {
    let id = opt_str(v, "example")?.ok_or_else(|| EngineError::Params("`example` (an example id from example.list) is required".into()))?;
    if id.len() > tpl::MAX_ID {
        return Err(EngineError::Params(format!("`example` is longer than {} bytes", tpl::MAX_ID)));
    }
    lookup_example(id)
}

/// The `viewpoint`, `variant` and `sheet` params, checked for shape and length.
fn choices_param(v: &Value) -> Result<Choices, EngineError> {
    let mut c = Choices::default();
    for (k, slot) in [("viewpoint", &mut c.viewpoint), ("variant", &mut c.variant), ("sheet", &mut c.sheet)] {
        if let Some(t) = opt_str(v, k)? {
            if t.len() > tpl::MAX_ID {
                return Err(EngineError::Params(format!("`{k}` is longer than {} bytes", tpl::MAX_ID)));
            }
            *slot = Some(t.to_string());
        }
    }
    Ok(c)
}

/// A sheet preset id with its orientation: `iso.a3` becomes `iso.a3.landscape`. `ansi.` is
/// read as `asme.`, since the ANSI sheet sizes are the ASME Y14.1 ones and people use both names.
fn full_sheet(sheet: &str) -> Result<String, EngineError> {
    if !tpl::is_sheet_id(sheet) {
        return Err(EngineError::Params(format!("`{}` is not a sheet preset id (`<standard>.<size>[.<orientation>]`)", short(sheet))));
    }
    let sheet = match sheet.strip_prefix("ansi.") {
        Some(rest) => format!("asme.{rest}"),
        None => sheet.to_string(),
    };
    Ok(if sheet.split('.').count() == 2 { format!("{sheet}.{DEFAULT_ORIENTATION}") } else { sheet })
}

/// The variant and sheet a template is drawn with. Variant: the caller's choice (an id of the
/// pack, a family, or "" for the default drawing), else the project's family when `project` is
/// given and set, else the template's own, else the pack's variant for the default family, else
/// the default drawing. Sheet: the caller's choice, else (when the template has a sheet) the
/// project's sheet when given and set, else the template's; "" means no sheet.
fn resolve(pack: &Pack, t: &Template, c: &Choices, project: Option<&Standards>) -> Result<(String, String), EngineError> {
    let variant = match c.variant.as_deref() {
        Some("") => String::new(),
        Some(v) if v.contains('.') => {
            pack.variant(v).ok_or_else(|| EngineError::Params(format!("`{}` is not a variant of {}", short(v), pack.name)))?.id.clone()
        }
        Some(family) => {
            tpl::variant_for(pack, family).ok_or_else(|| EngineError::Params(format!("{} has no `{}` variant", pack.name, short(family))))?.id.clone()
        }
        None => project
            .and_then(|p| tpl::variant_for(pack, &p.symbols))
            .map(|x| x.id.clone())
            .or_else(|| (!t.variant.is_empty()).then(|| t.variant.clone()))
            .or_else(|| tpl::variant_for(pack, DEFAULT_SYMBOLS).map(|x| x.id.clone()))
            .unwrap_or_default(),
    };
    let sheet = match c.sheet.as_deref() {
        Some("") => String::new(),
        Some(s) => full_sheet(s)?,
        None if t.sheet.is_empty() => String::new(),
        None => match project.map(|p| p.sheet.as_str()).filter(|s| !s.is_empty()) {
            Some(s) => full_sheet(s)?,
            None => full_sheet(&t.sheet)?,
        },
    };
    Ok((variant, sheet))
}

/// A pipeline run: the scratch document it left, the resolved steps to replay, and which
/// optional steps ran and which were skipped.
struct Plan {
    doc: Document,
    steps: Vec<(String, Value)>,
    applied: Vec<String>,
    skipped: Vec<String>,
}

/// Replace every `${name.path}` string inside `v` by that value of a named result.
fn bind(v: &Value, names: &HashMap<String, Value>) -> Result<Value, String> {
    Ok(match v {
        Value::String(t) => match tpl::reference(t) {
            Some((name, path)) => {
                let mut at = names.get(name).ok_or_else(|| format!("`{}` names no earlier result", short(name)))?;
                for key in path.split('.').filter(|k| !k.is_empty()) {
                    at = match at {
                        Value::Object(o) => o.get(key),
                        Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
                        _ => None,
                    }
                    .ok_or_else(|| format!("`{}` names nothing in that result", short(t)))?;
                }
                at.clone()
            }
            None => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(|x| bind(x, names)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), bind(x, names)?))).collect::<Result<Map<_, _>, String>>()?),
        _ => v.clone(),
    })
}

/// The `validate.setProfile` params that hold a document of `t` on `viewpoint` to its rules, when
/// the type names rulesets or live checking: its rulesets join the ones `doc` is already held to
/// (a page added to a document keeps the other notations' rules), and checking a person switched
/// off stays off unless the type says. `None` when the type leaves the rules to the notation.
fn rules_step(doc: &Document, t: &Template, viewpoint: &str) -> Option<Value> {
    let own = t.rulesets_for(viewpoint);
    if own.is_empty() && t.live_checks.is_none() {
        return None;
    }
    let before = vizcraft_notation::rules::profile(doc, cat());
    let mut p = Map::new();
    if !own.is_empty() {
        let mut all: Vec<String> = before.rulesets.clone();
        for r in own {
            if !all.contains(r) {
                all.push(r.clone());
            }
        }
        p.insert("rulesets".into(), json!(all));
    }
    let enabled = t.live_checks.or((before.user_set && !before.enabled).then_some(false));
    if let Some(e) = enabled {
        p.insert("enabled".into(), json!(e));
    }
    Some(Value::Object(p))
}

/// Run the pipeline for document type (or example) `t` on a scratch session holding `doc` (with
/// `base`'s registry and view), after the `prelude` steps, and return what it left and the
/// resolved steps. With `keep_sel` (when `doc` is `base`'s own document) the scratch also takes
/// `base`'s selection, so the replay starts from the same state. Fails with the step that failed.
#[allow(clippy::too_many_arguments)]
fn plan(
    base: &Session,
    doc: Document,
    keep_sel: bool,
    t: &Template,
    viewpoint: &str,
    variant: &str,
    sheet: &str,
    prelude: Vec<(String, Value)>,
) -> Result<Plan, EngineError> {
    let rules = rules_step(&doc, t, viewpoint);
    let mut s = Session::new(doc);
    s.registry = base.registry.clone();
    s.view = base.view.clone();
    if keep_sel {
        s.sel = base.sel.clone();
    }
    let mut out = Plan { doc: Document::new(), steps: Vec::new(), applied: Vec::new(), skipped: Vec::new() };
    let fail = |id: &str, e: EngineError| EngineError::Failed(format!("template `{}`: {} failed: {e}", short(&t.id), short(id)));
    let step = |s: &mut Session, out: &mut Plan, id: &str, params: Value| -> Result<Value, EngineError> {
        let r = s.run(id, &params).map_err(|e| fail(id, e))?;
        out.steps.push((id.to_string(), params));
        Ok(r)
    };
    let hook = |s: &Session, out: &mut Plan, flag: &str| -> Option<String> {
        let id = HOOKS.iter().find(|(f, _)| *f == flag).map(|(_, id)| *id)?;
        if s.registry.get(id).is_some() {
            out.applied.push(id.to_string());
            Some(id.to_string())
        } else {
            out.skipped.push(id.to_string());
            None
        }
    };
    for (id, params) in prelude {
        step(&mut s, &mut out, &id, params)?;
    }
    if !viewpoint.is_empty() {
        step(&mut s, &mut out, "model.setViewpoint", json!({"viewpoint": viewpoint}))?;
    }
    if let Some(p) = rules {
        step(&mut s, &mut out, "validate.setProfile", p)?;
    }
    if !sheet.is_empty()
        && let Some(id) = hook(&s, &mut out, "sheet")
    {
        step(&mut s, &mut out, &id, json!({"sheet": sheet}))?;
    }
    if let Some(m) = &t.module
        && let Some(id) = hook(&s, &mut out, "module")
    {
        let module = match m {
            Module::Named(n) => json!(n),
            Module::Units(u) => json!(u),
        };
        step(&mut s, &mut out, &id, json!({"module": module}))?;
    }
    let page = active_id(&s.doc);
    let before: usize = s.doc.page().map(|p| p.shapes.len()).unwrap_or(0);
    let mut names: HashMap<String, Value> = HashMap::new();
    for (i, st) in t.seed.iter().enumerate() {
        if st.optional && s.registry.get(&st.command).is_none() {
            out.skipped.push(st.command.clone());
            continue;
        }
        let params = if st.params.is_null() { json!({}) } else { st.params.clone() };
        let params = bind(&params, &names).map_err(|e| fail(&format!("seed[{i}] {}", st.command), EngineError::Params(e)))?;
        let r = step(&mut s, &mut out, &st.command, params)?;
        if !st.name.is_empty() {
            names.insert(st.name.clone(), r);
        }
    }
    // After the seed, so the shapes it drew with a pack's own glyphs are redrawn in the variant,
    // and before the layout, which needs their final sizes.
    if !variant.is_empty() {
        step(&mut s, &mut out, "variant.set", json!({"variant": variant}))?;
    }
    // An empty page cannot be laid out; a seed that drew nothing keeps its layout to itself.
    let drew = s.doc.pages.iter().find(|p| p.id == page).is_some_and(|p| p.shapes.len() > before);
    if let Some(l) = t.layout.as_ref().filter(|_| drew) {
        // The layouts act on the selection when two or more shapes are selected; the whole page
        // is wanted.
        step(&mut s, &mut out, "edit.selectNone", json!({}))?;
        let mut p = Map::new();
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                p.insert(k.to_string(), v);
            }
        };
        put("direction", l.direction.as_ref().map(|d| json!(d)));
        put("nodeSpacing", l.node_spacing.map(|n| json!(n)));
        put("layerSpacing", l.layer_spacing.map(|n| json!(n)));
        match &l.primitive {
            Some(prim) => {
                put("primitive", Some(json!(prim)));
                if !l.roles.is_null() {
                    put("roles", Some(bind(&l.roles, &names).map_err(|e| fail("layout.primitive", EngineError::Params(e)))?));
                }
                step(&mut s, &mut out, "layout.primitive", Value::Object(p))?;
            }
            None => {
                put("algorithm", l.algorithm.as_ref().map(|a| json!(a)));
                step(&mut s, &mut out, "layout.full", Value::Object(p))?;
            }
        }
    }
    // A title block needs a sheet: with the sheet chosen away (`"sheet": ""`) there is none.
    if t.title_block
        && !sheet.is_empty()
        && let Some(id) = hook(&s, &mut out, "titleBlock")
    {
        step(&mut s, &mut out, &id, json!({}))?;
    }
    if t.key
        && let Some(id) = hook(&s, &mut out, "key")
    {
        step(&mut s, &mut out, &id, json!({}))?;
    }
    out.doc = std::mem::take(&mut s.doc);
    Ok(out)
}

/// The id of the document's active page.
fn active_id(doc: &Document) -> String {
    doc.page().map(|p| p.id.clone()).unwrap_or_default()
}

/// Select the shape of the first model element drawn on the active page (none when it draws
/// none).
fn select_first_element(s: &mut Session) {
    let Some(page) = s.doc.page() else { return };
    let first = s.doc.model.elements.iter().find_map(|e| page.shapes.iter().find(|sh| sh.element.as_deref() == Some(e.id.as_str())));
    let ids = first.map(|sh| vec![sh.id.clone()]).unwrap_or_default();
    super::select(s, ids);
}

/// The result `template.new`, `template.addPage` and `example.open` share.
fn outcome(s: &Session, t: &Template, viewpoint: &str, variant: &str, sheet: &str, p: &Plan) -> Value {
    let profile = vizcraft_notation::rules::profile(&s.doc, cat());
    let rulesets: Vec<String> = if profile.enabled { profile.rulesets } else { Vec::new() };
    json!({
        "template": t.id,
        "page": active_id(&s.doc),
        "viewpoint": viewpoint,
        "rulesets": rulesets,
        "variant": variant,
        "sheet": sheet,
        "applied": p.applied,
        "skipped": p.skipped,
        "primary": s.sel.primary,
        "items": s.sel.items,
    })
}

fn new(s: &mut Session, v: &Value) -> CmdResult {
    let (pack, t) = template_param(v)?;
    create(s, v, pack, t)
}

fn open_example(s: &mut Session, v: &Value) -> CmdResult {
    let (pack, t) = example_param(v)?;
    create(s, v, pack, t)
}

/// `template.new` and `example.open`: replace the document with a new one of `t`.
fn create(s: &mut Session, v: &Value, pack: &Pack, t: &Template) -> CmdResult {
    // Every param is checked before the document is touched, so junk never clears history.
    let choices = choices_param(v)?;
    let viewpoint = viewpoint_of(t, &choices)?;
    let path = opt_str(v, "path")?.map(str::to_string);
    if path.as_ref().is_some_and(|p| p.is_empty() || p.len() > MAX_PATH) {
        return Err(EngineError::Params(format!("`path` must be 1 to {MAX_PATH} bytes")));
    }
    let given = standards_param(s, v)?;
    let project = given.as_ref().map(as_project);
    let (variant, sheet) = resolve(pack, t, &choices, project.as_ref())?;
    // The project's standards are the first step, so they are part of the one undo step. Given
    // ones are written as given; otherwise the variant's family, but only a symbol family the
    // standards offer (a notation such as `ie` or `chen` is not one), and the sheet drawn.
    let (given_symbols, given_sheet) = given.unwrap_or_default();
    let mut std = Map::new();
    let family = tpl::family_of(&variant);
    match given_symbols {
        Some(sym) => {
            std.insert("symbols".into(), json!(sym));
        }
        None if !variant.is_empty() && SYMBOL_CHOICES.contains(&family) => {
            std.insert("symbols".into(), json!(family));
        }
        None => {}
    }
    match given_sheet {
        Some(sh) => {
            std.insert("sheet".into(), json!(sh));
        }
        None if !sheet.is_empty() => {
            std.insert("sheet".into(), json!(sheet));
        }
        None => {}
    }
    let prelude = if std.is_empty() { Vec::new() } else { vec![("template.standards".to_string(), Value::Object(std))] };
    // A dry run first: a template that fails leaves the user's document and history alone.
    let p = plan(s, Document::new(), false, t, &viewpoint, &variant, &sheet, prelude)?;
    s.set_document(Document::new());
    s.path = None;
    s.transaction(&format!("New from {}", t.label), &p.steps)?;
    select_first_element(s);
    let mut out = outcome(s, t, &viewpoint, &variant, &sheet, &p);
    if let Some(path) = path {
        let r = s.run("file.save", &json!({"path": path}))?;
        out["path"] = r.get("path").cloned().unwrap_or(Value::Null);
    }
    Ok(out)
}

fn add_page(s: &mut Session, v: &Value) -> CmdResult {
    let (pack, t) = template_param(v)?;
    let choices = choices_param(v)?;
    let viewpoint = viewpoint_of(t, &choices)?;
    let name = opt_str(v, "name")?;
    let (variant, sheet) = resolve(pack, t, &choices, Some(&s.doc.standards))?;
    let mut add = Map::new();
    if let Some(n) = name {
        add.insert("name".into(), json!(n));
    }
    let p = plan(s, s.doc.clone(), true, t, &viewpoint, &variant, &sheet, vec![("page.add".to_string(), Value::Object(add))])?;
    s.transaction(&format!("New Page from {}", t.label), &p.steps)?;
    select_first_element(s);
    Ok(outcome(s, t, &viewpoint, &variant, &sheet, &p))
}

fn preview(s: &mut Session, v: &Value) -> CmdResult {
    let (pack, t) = template_param(v)?;
    let choices = choices_param(v)?;
    let viewpoint = viewpoint_of(t, &choices)?;
    let project = standards_param(s, v)?.as_ref().map(as_project);
    let (variant, sheet) = resolve(pack, t, &choices, project.as_ref())?;
    let p = plan(s, Document::new(), false, t, &viewpoint, &variant, &sheet, Vec::new())?;
    let index = p.doc.active_index();
    let bytes = vizcraft_formats::svg::export_page(&p.doc, index).map_err(|e| EngineError::Failed(e.to_string()))?;
    if bytes.len() > MAX_PREVIEW {
        return Err(EngineError::Failed(format!("the preview is larger than {MAX_PREVIEW} bytes")));
    }
    let svg = String::from_utf8(bytes).map_err(|e| EngineError::Failed(e.to_string()))?;
    // The text is for embedding, so it starts at the root element, without the XML declaration.
    let svg = svg.find("<svg").and_then(|i| svg.get(i..)).unwrap_or(&svg).to_string();
    let (width, height) = svg_size(&svg);
    Ok(json!({"template": t.id, "viewpoint": viewpoint, "variant": variant, "sheet": sheet, "svg": svg, "width": width, "height": height}))
}

/// The `width` and `height` of an SVG's root element (0 when absent).
fn svg_size(svg: &str) -> (f64, f64) {
    let head = svg.find("<svg").and_then(|i| svg.get(i..)).and_then(|h| h.split('>').next()).unwrap_or_default();
    let attr = |k: &str| {
        head.split_once(&format!(" {k}=\""))
            .and_then(|(_, rest)| rest.split('"').next())
            .and_then(|n| n.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .unwrap_or(0.0)
    };
    (attr("width"), attr("height"))
}

/// The sheet presets `sheet.presets` lists, when it is registered and answers with a list of ids
/// (as an array, or under `presets` or `sheets`, each entry an id or an object with an `id`).
fn sheet_presets(s: &mut Session) -> Option<Vec<String>> {
    s.registry.get(SHEET_PRESETS)?;
    let r = s.run(SHEET_PRESETS, &json!({})).ok()?;
    let list = match &r {
        Value::Array(a) => a,
        Value::Object(o) => o.get("presets").or_else(|| o.get("sheets")).and_then(Value::as_array)?,
        _ => return None,
    };
    let ids: Vec<String> = list
        .iter()
        .filter_map(|x| x.as_str().or_else(|| x.get("id").and_then(Value::as_str)))
        .filter(|id| id.len() <= MAX_STANDARD)
        .map(str::to_string)
        .collect();
    (!ids.is_empty()).then_some(ids)
}

/// The `symbols` and `sheet` of `v`, checked: a family name of lowercase letters, digits and `_`,
/// and a sheet preset id (completed with its orientation, and one of `presets` when given). ""
/// is kept, meaning the default. Absent stays `None`.
fn checked_standards(v: &Value, presets: Option<&[String]>) -> Result<(Option<String>, Option<String>), EngineError> {
    let symbols = opt_str(v, "symbols")?;
    let sheet = opt_str(v, "sheet")?;
    if let Some(sym) = symbols
        && !sym.is_empty()
        && (sym.len() > MAX_STANDARD || !sym.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    {
        return Err(EngineError::Params(format!("`symbols` must be a family name of at most {MAX_STANDARD} lowercase letters, digits or `_`")));
    }
    let sheet = match sheet {
        Some("") => Some(String::new()),
        Some(sh) => {
            let full = full_sheet(sh)?;
            if presets.is_some_and(|p| !p.contains(&full)) {
                return Err(EngineError::Params(format!("`{}` is not a sheet preset; sheet.presets lists them", short(sh))));
            }
            Some(full)
        }
        None => None,
    };
    Ok((symbols.map(str::to_string), sheet))
}

/// The `standards` param of `template.new` and `template.preview`, checked as
/// `template.standards` checks its own: `None` when absent, else the given symbols and sheet.
fn standards_param(s: &mut Session, v: &Value) -> Result<Option<(Option<String>, Option<String>)>, EngineError> {
    match v.get("standards") {
        None | Some(Value::Null) => Ok(None),
        Some(o @ Value::Object(_)) => {
            let presets = sheet_presets(s);
            checked_standards(o, presets.as_deref()).map(Some)
        }
        Some(_) => Err(EngineError::Params("`standards` must be an object {\"symbols\"?, \"sheet\"?}".into())),
    }
}

/// The project standards a `standards` param stands for (an absent field reads as unset).
fn as_project(given: &(Option<String>, Option<String>)) -> Standards {
    Standards { symbols: given.0.clone().unwrap_or_default(), sheet: given.1.clone().unwrap_or_default() }
}

fn standards(s: &mut Session, v: &Value) -> CmdResult {
    let presets = sheet_presets(s);
    let (symbols, sheet) = checked_standards(v, presets.as_deref())?;
    if let Some(sym) = symbols {
        s.doc.standards.symbols = sym;
    }
    if let Some(sh) = sheet {
        s.doc.standards.sheet = sh;
    }
    let sheet_choices: Vec<String> = presets.unwrap_or_else(|| SHEET_CHOICES.iter().map(|x| x.to_string()).collect());
    let st = &s.doc.standards;
    Ok(json!({"symbols": st.symbols(), "sheet": st.sheet(), "symbolChoices": SYMBOL_CHOICES, "sheetChoices": sheet_choices}))
}

/// Whether `t` of `pack` matches the `template.list` filters.
fn matches(pack: &Pack, t: &Template, audience: Option<&str>, query: Option<&str>) -> bool {
    let audience_ok = audience.is_none_or(|a| {
        let list = if t.audience.is_empty() { &pack.audience } else { &t.audience };
        list.iter().any(|x| x.eq_ignore_ascii_case(a))
    });
    let query_ok = query.is_none_or(|q| {
        let q = q.to_lowercase();
        [&t.label, &t.id, &t.description].iter().any(|f| f.to_lowercase().contains(&q)) || t.keywords.iter().any(|k| k.to_lowercase().contains(&q))
    });
    audience_ok && query_ok
}

/// The `audience`, `pack` and `query` filters of `template.list` and `example.list`.
fn filters(v: &Value) -> Result<(Option<&str>, Option<&str>, Option<&str>), EngineError> {
    let audience = opt_str(v, "audience")?;
    let pack_id = opt_str(v, "pack")?;
    let query = opt_str(v, "query")?;
    if query.is_some_and(|q| q.trim().len() > MAX_QUERY) {
        return Err(EngineError::Params(format!("`query` is longer than {MAX_QUERY} bytes")));
    }
    Ok((audience, pack_id, query))
}

fn list(_s: &mut Session, v: &Value) -> CmdResult {
    let (audience, pack_id, query) = filters(v)?;
    Ok(json!({"templates": listing(audience, pack_id, query)}))
}

fn list_examples(_s: &mut Session, v: &Value) -> CmdResult {
    let (audience, pack_id, query) = filters(v)?;
    Ok(json!({"examples": example_listing(audience, pack_id, query)}))
}

/// The document types `template.list` lists for these filters, in catalog order, each as its
/// row. For callers that filter as the person types (a gallery), so nothing is recorded into a
/// macro. A blank query matches everything.
pub fn listing(audience: Option<&str>, pack_id: Option<&str>, query: Option<&str>) -> Vec<Value> {
    rows(|p| &p.templates, audience, pack_id, query)
}

/// The examples `example.list` lists for these filters, as [`listing`] does for document types.
pub fn example_listing(audience: Option<&str>, pack_id: Option<&str>, query: Option<&str>) -> Vec<Value> {
    rows(|p| &p.examples, audience, pack_id, query)
}

/// A ruleset id with its label, for the rows (the id when the catalog does not know it).
fn ruleset_row(id: &str) -> Value {
    let label = cat().ruleset(id).map(|r| r.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| id.to_string());
    json!({"id": id, "label": label})
}

/// The rulesets a document of `t` on `viewpoint` is held to: the type's for that viewpoint, else
/// the pack's first (what the notation brings), else none.
fn rulesets_shown(pack: &Pack, t: &Template, viewpoint: &str) -> Vec<Value> {
    let own = t.rulesets_for(viewpoint);
    if own.is_empty() {
        pack.rulesets.first().filter(|_| !viewpoint.is_empty()).map(|r| vec![ruleset_row(&r.id)]).unwrap_or_default()
    } else {
        own.iter().map(|r| ruleset_row(r)).collect()
    }
}

fn rows(list: impl Fn(&'static Pack) -> &'static Vec<Template>, audience: Option<&str>, pack_id: Option<&str>, query: Option<&str>) -> Vec<Value> {
    let query = query.map(str::trim).filter(|q| !q.is_empty());
    let mut out = Vec::new();
    for pack in cat().packs().iter().filter(|p| pack_id.is_none_or(|id| p.id == id)) {
        let variants: Vec<Value> = pack.variants.iter().map(|x| json!({"id": x.id, "label": x.label, "family": tpl::family_of(&x.id)})).collect();
        for t in list(pack).iter().filter(|t| matches(pack, t, audience, query)) {
            let audience = if t.audience.is_empty() { &pack.audience } else { &t.audience };
            let viewpoints: Vec<Value> = t
                .viewpoint_choices()
                .into_iter()
                .map(|vp| {
                    let label = pack.viewpoint(vp).map(|x| x.label.clone()).unwrap_or_default();
                    json!({"id": vp, "label": label, "rulesets": rulesets_shown(pack, t, vp)})
                })
                .collect();
            let mut row = json!({
                "id": t.id,
                "label": t.label,
                "description": t.description,
                "pack": pack.id,
                "packName": pack.name,
                "discipline": t.discipline(),
                "audience": audience,
                "keywords": t.keywords,
                "viewpoint": t.viewpoint,
                "viewpoints": viewpoints,
                "rulesets": rulesets_shown(pack, t, &t.viewpoint),
                "codeForms": t.code_forms,
                "key": t.key,
                "titleBlock": t.title_block,
                "variant": t.variant,
                "variants": variants,
                "sheet": t.sheet,
                "module": t.module,
            });
            if let (Some(o), Some(l)) = (row.as_object_mut(), t.live_checks) {
                o.insert("liveChecks".into(), json!(l));
            }
            out.push(row);
        }
    }
    out
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;
