//! Project tag formats (`identity.*`): the identifier scopes of every loaded pack as this document
//! reads them, and the project's own format or `numberBy` for a scope (`Model.idFormats`).
//!
//! A pack declares how tags are built (`{letters}-{owner:process.InstrumentationLoopFunction.tag}`);
//! a project may change a scope's format or the property its numbers count within. The identity
//! registry reads every scope through these overrides, so new numbers, duplicates and the live
//! process tags all follow the project's choice. Setting a format is one undo step.

use serde_json::{Value, json};
use vizcraft_model::identity::{IdFormat, IdOverride, MAX_OVERRIDES, is_number_by};
use vizcraft_model::{IdScope, Registry};

use super::model::{cat, check_len};
use super::short;
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return the identity commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("identity.formats", "Tag Formats", "Notation > Tags", formats).params(
            r#"{"pack"?: pack id} -> {"scopes": [{"id", "types", "format", "numberBy", "unique", "paired", "autoNumber", "perOwner", "packFormat", "packNumberBy", "overridden"}]}; every identifier scope of the loaded packs (or one pack's) as this document reads it: the pack's format and numberBy with the project's override applied; read-only"#,
        ),
        CommandSpec::new("identity.setFormat", "Set Tag Format", "Notation > Tags", set_format).params(
            r#"{"scope": scope id, "format"?: format string or "" or null (the pack's), "numberBy"?: property key, "" (one count) or null (the pack's)} -> {"scope", "format", "numberBy", "overridden"}; the project's own format or numbering group for the scope, one undo step; a format must parse and an auto-numbered scope's must hold {n}; at most 64 overridden scopes"#,
        ),
    ]
}

/// A scope as the commands report it.
pub(crate) fn scope_json(pack: &IdScope, now: &IdScope, overridden: bool) -> Value {
    json!({
        "id": now.id,
        "types": now.types,
        "format": now.format,
        "numberBy": now.number_by,
        "unique": now.unique,
        "paired": now.paired,
        "autoNumber": now.auto_number,
        "perOwner": now.per_owner,
        "packFormat": pack.format,
        "packNumberBy": pack.number_by,
        "overridden": overridden,
    })
}

fn formats(s: &mut Session, v: &Value) -> CmdResult {
    let pack = super::opt_str(v, "pack")?;
    if let Some(pk) = pack
        && cat().pack(pk).is_none()
    {
        return Err(EngineError::Params(format!("unknown pack `{}`", short(pk))));
    }
    let scopes = cat().scopes();
    let reg = Registry::new(&s.doc.model, &scopes);
    let out: Vec<Value> = scopes
        .iter()
        .zip(reg.effective())
        .filter(|(sc, _)| pack.is_none_or(|pk| vizcraft_model::pack_of(&sc.id) == pk))
        .map(|(sc, now)| scope_json(sc, now, s.doc.model.id_formats.contains_key(&sc.id)))
        .collect();
    Ok(json!({"scopes": out}))
}

/// A param that may be absent (keep), null (the pack's) or text.
fn tri(v: &Value, k: &str) -> Result<Option<Option<String>>, EngineError> {
    match v.get(k) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) => Ok(Some(Some(check_len(k, t)?.to_string()))),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be text or null"))),
    }
}

fn set_format(s: &mut Session, v: &Value) -> CmdResult {
    let id = check_len("scope", p::req_str(v, "scope")?)?.to_string();
    let scopes = cat().scopes();
    let pack_scope = scopes.iter().find(|sc| sc.id == id).ok_or_else(|| EngineError::Params(format!("no identifier scope `{}`", short(&id))))?;
    let format = tri(v, "format")?;
    let number_by = tri(v, "numberBy")?;
    if format.is_none() && number_by.is_none() {
        return Err(EngineError::Params("give `format`, `numberBy` or both".into()));
    }
    let mut next = s.doc.model.id_formats.get(&id).cloned().unwrap_or_default();
    if let Some(f) = format {
        let f = f.unwrap_or_default();
        if !f.is_empty() {
            let parsed = IdFormat::parse(&f).map_err(|e| EngineError::Params(e.to_string()))?;
            if pack_scope.auto_number && !parsed.has_number() {
                return Err(EngineError::Params(format!("`{}` numbers automatically, so its format needs {{n}}", short(&id))));
            }
        }
        next.format = if f == pack_scope.format { String::new() } else { f };
    }
    if let Some(by) = number_by {
        if let Some(b) = &by
            && !is_number_by(b)
        {
            return Err(EngineError::Params("`numberBy` must be a property key (letters, digits and `_`) or \"\"".into()));
        }
        next.number_by = by.filter(|b| *b != pack_scope.number_by);
    }
    let map = &mut s.doc.model.id_formats;
    if next.is_empty() {
        map.remove(&id);
    } else {
        if !map.contains_key(&id) && map.len() >= MAX_OVERRIDES {
            return Err(EngineError::Params(format!("a project overrides at most {MAX_OVERRIDES} scopes")));
        }
        map.insert(id.clone(), next);
    }
    let now = s.doc.model.id_formats.get(&id).map_or_else(|| pack_scope.clone(), |o: &IdOverride| o.apply(pack_scope));
    Ok(json!({"scope": id, "format": now.format, "numberBy": now.number_by, "overridden": s.doc.model.id_formats.contains_key(&id)}))
}
