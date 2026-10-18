//! Validation commands: batch rules over a scope, the live issue list, the document's
//! conformance profile, rule explanations and issue navigation.
//!
//! Live issues are recomputed by the session after every change (`Session::issues`); batch rules
//! (reachability, dangling decisions, free connector ends, unmatched pairs, crossings) run only
//! when asked. Which rules run at all is the document's conformance profile
//! (`Document::conformance`), read inside the rules engine: `validate.setProfile` and
//! `validate.suppress` change it (an undo step each); the other commands only read the document
//! and never add an undo step.

use std::collections::HashSet;

use serde_json::{Value, json};
use vizcraft_diagram::conformance::{MAX_KEY, MAX_OVERRIDES, MAX_REASON, MAX_RULESETS, MAX_SUPPRESSIONS};
use vizcraft_diagram::{Conformance, RuleOverride, Suppression};
use vizcraft_model::{Issue, Severity};
use vizcraft_notation::rules::{self, Scope};

use super::{ids_param, opt_bool, opt_str, select, short};
use crate::{CmdResult, CommandSpec, EngineError, Session, p};

/// Return all validate commands: `validate.run`, `validate.list`, `validate.setProfile`,
/// `validate.suppress`, `validate.explain`, `validate.next`, `validate.previous`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("validate.run", "Validate", "Model > Validate", run).params(
            r#"{"scope"?: "all" (default) | "page" | "selection", "page"?: page id, name or index (page scope; default the active page), "ids"?: [string] (selection scope; default the selection)} -> {"issues": [{"rule", "severity", "message", "subjects", "items", "page"?}], "live": n, "batch": n}; live and batch rules in the scope; shape and connector ids stand for their elements and relationships"#,
        ),
        CommandSpec::new("validate.list", "Issues", "Model > Validate", list).params(
            r#"{"severity"?: "error" | "warning" | "info", "rule"?: string} -> {"issues": [{"rule", "severity", "message", "subjects", "items", "page"?, "fixes"?}]}; the live issues, filtered"#,
        ),
        CommandSpec::new("validate.setProfile", "Conformance Profile", "Process > Validate", set_profile).params(
            r#"{"enabled"?: bool, "rulesets"?: [ruleset id or pack id], "overrides"?: [{"rule", "severity"?: "error"|"warning"|"info", "disabled"?: bool}], "suppressions"?: [{"key", "reason"?}]} -> {"profile": {"enabled", "userSet", "rulesets", "overrides", "suppressions"}}; the document's conformance profile; each given list replaces the stored one; enabled or rulesets marks the profile as chosen, so choosing a notation no longer changes it; with no params, the profile in effect"#,
        ),
        CommandSpec::new("validate.suppress", "Suppress Issue", "Process > Validate", suppress)
            .params(r#"{"key": issue key (rule|subjects|items|page), "reason"?: string} -> {"key", "added": bool}; the issue no longer reports"#),
        CommandSpec::new("validate.explain", "Explain Rule", "Process > Validate", explain).params(
            r#"{"rule": rule id} -> {"rule", "description", "severity", "mode", "reference", "ruleset", "pack", "check", "selected", "override"?}; severity is the one in effect; ruleset is the first ruleset listing the rule (one in the profile when there is one); selected says whether the profile runs it"#,
        ),
        CommandSpec::new("validate.next", "Next Issue", "Process > Validate", next)
            .params(r#"{} -> {"index", "issue", "count"}; selects the next issue's items (live and batch, in order) on their page, wrapping around; index and issue are null when no issue points at a drawn item"#),
        CommandSpec::new("validate.previous", "Previous Issue", "Process > Validate", previous)
            .params(r#"{} -> {"index", "issue", "count"}; selects the previous issue's items on their page, wrapping around"#),
    ]
}

/// Longest string param looked at; rule ids, page names and item ids are far shorter.
const MAX_PARAM: usize = 256;

/// An optional string param of at most [`MAX_PARAM`] bytes.
fn short_str<'a>(v: &'a Value, k: &str) -> Result<Option<&'a str>, EngineError> {
    match opt_str(v, k)? {
        Some(t) if t.len() > MAX_PARAM => Err(EngineError::Params(format!("`{k}` is longer than {MAX_PARAM} bytes"))),
        t => Ok(t),
    }
}

fn to_json(issues: &[Issue]) -> CmdResult {
    serde_json::to_value(issues).map_err(|e| EngineError::Failed(e.to_string()))
}

/// The live issues, filtered by severity and rule.
fn list(s: &mut Session, v: &Value) -> CmdResult {
    let severity = match short_str(v, "severity")? {
        None => None,
        Some(name) => Some(
            Severity::from_name(name)
                .ok_or_else(|| EngineError::Params(format!("`severity` must be \"error\", \"warning\" or \"info\", not `{}`", short(name))))?,
        ),
    };
    let rule = short_str(v, "rule")?;
    let issues: Vec<Issue> =
        s.issues.iter().filter(|i| severity.is_none_or(|sv| i.severity == sv) && rule.is_none_or(|r| i.rule == r)).cloned().collect();
    Ok(json!({"issues": to_json(&issues)?}))
}

/// The scope a `validate.run` call names.
fn scope_param(s: &Session, v: &Value) -> Result<Scope, EngineError> {
    let page = short_str(v, "page")?;
    let ids = ids_param(v)?;
    if let Some(ids) = &ids
        && let Some(long) = ids.iter().find(|id| id.len() > MAX_PARAM)
    {
        return Err(EngineError::Params(format!("`ids` entry `{}` is longer than {MAX_PARAM} bytes", short(long))));
    }
    // With no explicit scope, a `page` or `ids` param says which one is meant.
    let name = match short_str(v, "scope")? {
        Some(n) => n,
        None if page.is_some() => "page",
        None if ids.is_some() => "selection",
        None => "all",
    };
    match name {
        "all" | "page" | "selection" => {}
        other => return Err(EngineError::Params(format!("`scope` must be \"all\", \"page\" or \"selection\", not `{}`", short(other)))),
    }
    if page.is_some() && name != "page" {
        return Err(EngineError::Params("`page` goes with the \"page\" scope".into()));
    }
    if ids.is_some() && name != "selection" {
        return Err(EngineError::Params("`ids` goes with the \"selection\" scope".into()));
    }
    Ok(match name {
        "page" => match page {
            None => Scope::Page(0),
            Some(r) => Scope::Page(s.doc.find_page(r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(r))))?),
        },
        "selection" => Scope::Items(ids.unwrap_or_else(|| s.sel.items.clone())),
        _ => Scope::All,
    })
}

/// Live issues in the scope (recomputed, so the answer does not depend on when the session last
/// refreshed) plus the batch rules for the scope, in the stable rules order.
fn run(s: &mut Session, v: &Value) -> CmdResult {
    let scope = scope_param(s, v)?;
    let cat = vizcraft_notation::catalog();
    let live = rules::in_scope(&s.doc, &scope, rules::live(&s.doc, cat));
    let batch = rules::batch(&s.doc, cat, &scope);
    let (live_n, batch_n) = (live.len(), batch.len());
    let mut issues = live;
    issues.extend(batch);
    rules::sort(&mut issues);
    Ok(json!({"issues": to_json(&issues)?, "live": live_n, "batch": batch_n}))
}

// ---------------------------------------------------------------- the profile

/// Most bytes of a ruleset or rule id param.
const MAX_ID_PARAM: usize = 256;

/// A param that is a list (absent or null: `None`) of at most `max` entries.
fn list_param<'a>(v: &'a Value, k: &str, max: usize) -> Result<Option<&'a Vec<Value>>, EngineError> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) if a.len() <= max => Ok(Some(a)),
        Some(Value::Array(_)) => Err(EngineError::Params(format!("`{k}` has more than {max} entries"))),
        Some(_) => Err(EngineError::Params(format!("`{k}` must be an array"))),
    }
}

/// A string inside a list entry, at most `max` bytes.
fn entry_str<'a>(e: &'a Value, k: &str, list: &str, max: usize) -> Result<Option<&'a str>, EngineError> {
    match e.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if t.len() <= max => Ok(Some(t)),
        Some(Value::String(_)) => Err(EngineError::Params(format!("`{list}`: `{k}` is longer than {max} bytes"))),
        Some(_) => Err(EngineError::Params(format!("`{list}`: `{k}` must be a string"))),
    }
}

fn severity_name(name: &str) -> Result<Severity, EngineError> {
    Severity::from_name(name)
        .ok_or_else(|| EngineError::Params(format!("severity must be \"error\", \"warning\" or \"info\", not `{}`", short(name))))
}

fn profile_json(p: &Conformance) -> CmdResult {
    let mut v = serde_json::to_value(p).map_err(|e| EngineError::Failed(e.to_string()))?;
    // Always show the switches, even at their defaults, so a reader need not know the defaults.
    if let Some(o) = v.as_object_mut() {
        o.insert("enabled".into(), json!(p.enabled));
        o.insert("userSet".into(), json!(p.user_set));
        o.entry("rulesets").or_insert(json!([]));
    }
    Ok(v)
}

/// Change the document's conformance profile; with no params, report the profile in effect.
fn set_profile(s: &mut Session, v: &Value) -> CmdResult {
    let cat = vizcraft_notation::catalog();
    let enabled = opt_bool(v, "enabled")?;
    let rulesets = match list_param(v, "rulesets", MAX_RULESETS)? {
        None => None,
        Some(list) => {
            let mut out = Vec::new();
            for r in list {
                let id = r.as_str().ok_or_else(|| EngineError::Params("`rulesets` must be an array of strings".into()))?;
                if id.len() > MAX_ID_PARAM || (cat.ruleset(id).is_none() && cat.pack(id).is_none()) {
                    return Err(EngineError::Params(format!("no ruleset or pack `{}`; notation.rulesets lists them", short(id))));
                }
                if !out.iter().any(|x: &String| x == id) {
                    out.push(id.to_string());
                }
            }
            Some(out)
        }
    };
    let overrides = match list_param(v, "overrides", MAX_OVERRIDES)? {
        None => None,
        Some(list) => {
            let mut out: Vec<RuleOverride> = Vec::new();
            for o in list {
                let rule =
                    entry_str(o, "rule", "overrides", MAX_ID_PARAM)?.ok_or_else(|| EngineError::Params("`overrides`: each needs a `rule`".into()))?;
                if cat.rule(rule).is_none() {
                    return Err(EngineError::Params(format!("`overrides`: no rule `{}`", short(rule))));
                }
                let severity = entry_str(o, "severity", "overrides", MAX_ID_PARAM)?.map(severity_name).transpose()?;
                let disabled = match o.get("disabled") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => return Err(EngineError::Params("`overrides`: `disabled` must be true or false".into())),
                };
                out.retain(|x| x.rule != rule);
                out.push(RuleOverride { rule: rule.to_string(), severity, disabled });
            }
            Some(out)
        }
    };
    let suppressions = match list_param(v, "suppressions", MAX_SUPPRESSIONS)? {
        None => None,
        Some(list) => {
            let mut out: Vec<Suppression> = Vec::new();
            for e in list {
                let key = entry_str(e, "key", "suppressions", MAX_KEY)?.filter(|k| !k.is_empty());
                let key = key.ok_or_else(|| EngineError::Params("`suppressions`: each needs a `key`".into()))?;
                let reason = entry_str(e, "reason", "suppressions", MAX_REASON)?.unwrap_or_default();
                out.push(Suppression { key: key.to_string(), reason: reason.to_string() });
            }
            Some(out)
        }
    };
    if enabled.is_none() && rulesets.is_none() && overrides.is_none() && suppressions.is_none() {
        return Ok(json!({"profile": profile_json(&rules::profile(&s.doc, cat))?}));
    }
    let mut p = s.doc.conformance.clone();
    if enabled.is_some() || rulesets.is_some() {
        // Choosing starts from what was in effect, so turning the profile off and on again keeps
        // the notation's rulesets.
        let effective = rules::profile(&s.doc, cat);
        p.enabled = effective.enabled;
        p.rulesets = effective.rulesets;
        p.user_set = true;
        if let Some(e) = enabled {
            p.enabled = e;
        }
        if let Some(r) = rulesets {
            p.rulesets = r;
            // Naming rulesets means holding the document to them.
            if enabled.is_none() {
                p.enabled = true;
            }
        }
    }
    if let Some(o) = overrides {
        p.overrides = o;
    }
    if let Some(sp) = suppressions {
        p.suppressions = sp;
    }
    p.sanitize();
    s.doc.conformance = p;
    Ok(json!({"profile": profile_json(&rules::profile(&s.doc, cat))?}))
}

/// Silence one issue by key.
fn suppress(s: &mut Session, v: &Value) -> CmdResult {
    let key = p::req_str(v, "key")?;
    if key.is_empty() || key.len() > MAX_KEY {
        return Err(EngineError::Params(format!("`key` must be an issue key of 1 to {MAX_KEY} bytes")));
    }
    let reason = opt_str(v, "reason")?.unwrap_or_default();
    if reason.len() > MAX_REASON {
        return Err(EngineError::Params(format!("`reason` is longer than {MAX_REASON} bytes")));
    }
    if s.doc.conformance.suppressions.len() >= MAX_SUPPRESSIONS && !s.doc.conformance.is_suppressed(key) {
        return Err(EngineError::Failed(format!("the document already holds {MAX_SUPPRESSIONS} suppressions")));
    }
    let added = s.doc.conformance.suppress(key, reason);
    Ok(json!({"key": key, "added": added}))
}

/// What a rule checks, where it comes from and how it runs for this document.
fn explain(s: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "rule")?;
    if id.len() > MAX_ID_PARAM {
        return Err(EngineError::Params(format!("`rule` is longer than {MAX_ID_PARAM} bytes")));
    }
    let cat = vizcraft_notation::catalog();
    let (pack, rule) = cat.rule(id).ok_or_else(|| EngineError::Params(format!("no rule `{}`", short(id))))?;
    let profile = rules::profile(&s.doc, cat);
    let listing: Vec<&str> = cat.rulesets().into_iter().filter(|(_, rs)| rs.rules.iter().any(|r| r == id)).map(|(_, rs)| rs.id.as_str()).collect();
    let ruleset = listing.iter().find(|r| profile.rulesets.iter().any(|p| p == *r)).or(listing.first()).copied();
    let selected = rules::selected(&s.doc, cat, &profile).iter().any(|(_, r)| r.id == id) && !profile.is_disabled(id);
    let mode = match rule.mode {
        vizcraft_notation::RuleMode::Live => "live",
        vizcraft_notation::RuleMode::Batch => "batch",
    };
    let check = serde_json::to_value(&rule.check).map_err(|e| EngineError::Failed(e.to_string()))?;
    let mut out = json!({
        "rule": rule.id,
        "description": rule.description,
        "severity": profile.severity_of(id, rule.severity).name(),
        "declaredSeverity": rule.severity.name(),
        "mode": mode,
        "reference": rule.reference,
        "ruleset": ruleset,
        "pack": pack.id,
        "check": check,
        "selected": selected,
    });
    if let (Some(o), Some(ov)) = (out.as_object_mut(), profile.override_for(id)) {
        o.insert("override".into(), serde_json::to_value(ov).map_err(|e| EngineError::Failed(e.to_string()))?);
    }
    Ok(out)
}

// ---------------------------------------------------------------- navigation

/// Every issue, live and batch over the whole document, in the stable order.
fn all_issues(s: &Session) -> Vec<Issue> {
    let cat = vizcraft_notation::catalog();
    let mut issues = rules::live(&s.doc, cat);
    issues.extend(rules::batch(&s.doc, cat, &Scope::All));
    rules::sort(&mut issues);
    issues
}

/// Where an issue can be shown: the page index and the issue's items drawn on it. `None` when
/// none of its items is drawn.
fn target(s: &Session, issue: &Issue) -> Option<(usize, Vec<String>)> {
    let doc = &s.doc;
    let page = match issue.page.as_deref().and_then(|id| doc.pages.iter().position(|p| p.id == id)) {
        Some(i) => i,
        None => doc.pages.iter().position(|p| issue.items.iter().any(|it| p.contains_id(it)))?,
    };
    let pg = doc.pages.get(page)?;
    let items: Vec<String> = issue.items.iter().filter(|it| pg.contains_id(it)).cloned().collect();
    (!items.is_empty()).then_some((page, items))
}

/// Select the issue `step` places from the one the selection shows (forwards or backwards).
fn step(s: &mut Session, forward: bool) -> CmdResult {
    let shown: Vec<(Issue, usize, Vec<String>)> = all_issues(s).into_iter().filter_map(|i| target(s, &i).map(|(pg, items)| (i, pg, items))).collect();
    let n = shown.len();
    if n == 0 {
        return Ok(json!({"index": null, "issue": null, "count": 0}));
    }
    let active = s.doc.active_index();
    let selected: HashSet<&str> = s.sel.items.iter().map(String::as_str).collect();
    let current =
        shown.iter().position(|(_, pg, items)| *pg == active && items.len() == selected.len() && items.iter().all(|i| selected.contains(i.as_str())));
    let at = match (current, forward) {
        (Some(c), true) => (c + 1) % n,
        (Some(c), false) => (c + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    let Some((issue, page, items)) = shown.get(at).cloned() else { return Ok(json!({"index": null, "issue": null, "count": n})) };
    if page != active || s.doc.active_page.is_empty() {
        s.doc.set_active_index(page);
        // Going to another page to show an issue is not an edit, as with `page.setActive`.
        let dirty = s.dirty;
        s.mark_saved();
        s.dirty = dirty;
    }
    select(s, items);
    Ok(json!({"index": at, "issue": to_json(std::slice::from_ref(&issue))?.get(0).cloned().unwrap_or(Value::Null), "count": n}))
}

fn next(s: &mut Session, _v: &Value) -> CmdResult {
    step(s, true)
}

fn previous(s: &mut Session, _v: &Value) -> CmdResult {
    step(s, false)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vizcraft_diagram::{Connector, Document, End};
    use vizcraft_model::{Element, Relationship};

    use crate::Session;

    /// A document with a bound shape `s_<id>` for each element.
    fn doc(elements: &[(&str, &str, &str)]) -> Document {
        let mut d = Document::new();
        for (i, (id, ty, name)) in elements.iter().enumerate() {
            let mut e = Element::new(*id, format!("flowchart.{ty}"));
            e.name = name.to_string();
            d.model.add_element(e).unwrap();
            let master = vizcraft_notation::catalog().master_for_type(&format!("flowchart.{ty}")).unwrap();
            let mut s = vizcraft_stencils::new_shape(master, format!("s_{id}"), i as f64 * 200.0, 0.0, None, None).unwrap();
            s.element = Some(id.to_string());
            s.text = name.to_string();
            d.pages[0].shapes.push(s);
        }
        d.id_seq = 100;
        d
    }

    fn rules_of(v: &Value) -> Vec<String> {
        v["issues"].as_array().unwrap().iter().map(|i| i["rule"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn connecting_two_databases_raises_an_illegal_flow_live() {
        let mut s = Session::new(doc(&[("e1", "Database", "Orders"), ("e2", "Database", "Archive")]));
        assert!(s.issues.is_empty());
        let r = s.run("connector.add", &json!({"from": {"shape": "s_e1"}, "to": {"shape": "s_e2"}})).unwrap();
        let cid = r["id"].as_str().unwrap().to_string();
        let rid = s.doc.connector(&cid).unwrap().relationship.clone().expect("the connector is bound");
        let rel = s.doc.model.relationship(&rid).unwrap();
        assert_eq!((rel.type_id.as_str(), rel.source.as_str(), rel.target.as_str()), ("flowchart.Flow", "e1", "e2"));
        assert_eq!(s.issues.len(), 1, "{:?}", s.issues);
        let i = &s.issues[0];
        assert_eq!((i.rule.as_str(), i.severity), ("flowchart.illegalFlow", vizcraft_model::Severity::Error));
        assert!(i.message.contains("Orders") && i.message.contains("Archive"), "{}", i.message);
        assert!(i.subjects.contains(&"e1".to_string()) && i.subjects.contains(&"e2".to_string()));
        assert!(i.items.contains(&cid));

        // The live list reads the same, in camelCase serde form.
        let l = s.run("validate.list", &json!({})).unwrap();
        assert_eq!(rules_of(&l), ["flowchart.illegalFlow"]);
        assert_eq!(l["issues"][0]["severity"], "error");
        assert!(l["issues"][0]["subjects"].is_array() && l["issues"][0]["page"] == "p1");
        assert_eq!(rules_of(&s.run("validate.list", &json!({"severity": "warning"})).unwrap()).len(), 0);
        assert_eq!(rules_of(&s.run("validate.list", &json!({"rule": "flowchart.illegalFlow", "severity": "error"})).unwrap()).len(), 1);
        assert_eq!(rules_of(&s.run("validate.list", &json!({"rule": "flowchart.other"})).unwrap()).len(), 0);

        // One undo step takes away the connector, the relationship and the issue.
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.issues.is_empty());
        assert!(s.doc.connector(&cid).is_none() && s.doc.model.relationships.is_empty());
    }

    #[test]
    fn run_reports_a_dangling_decision_in_each_scope() {
        let mut d = doc(&[("t", "Terminator", "Start"), ("d", "Decision", "Ok?"), ("a", "Process", "Do")]);
        for (rid, from, to) in [("r1", "t", "d"), ("r2", "d", "a")] {
            d.model.add_relationship(Relationship::new(rid, "flowchart.Flow", from, to)).unwrap();
            let mut c = Connector::new(format!("c_{rid}"), End::dynamic(format!("s_{from}")), End::dynamic(format!("s_{to}")));
            c.relationship = Some(rid.into());
            d.pages[0].connectors.push(c);
        }
        let mut s = Session::new(d);
        let before = s.doc.clone();
        let r = s.run("validate.run", &json!({})).unwrap();
        assert_eq!(rules_of(&r), ["flowchart.danglingDecision"]);
        assert_eq!((r["live"].as_u64(), r["batch"].as_u64()), (Some(0), Some(1)));
        assert_eq!(r["issues"][0]["subjects"], json!(["d"]));
        assert_eq!(rules_of(&s.run("validate.run", &json!({"scope": "page"})).unwrap()), ["flowchart.danglingDecision"]);
        assert_eq!(rules_of(&s.run("validate.run", &json!({"scope": "page", "page": "Page-1"})).unwrap()), ["flowchart.danglingDecision"]);
        assert_eq!(rules_of(&s.run("validate.run", &json!({"page": "0"})).unwrap()).len(), 1);
        assert_eq!(rules_of(&s.run("validate.run", &json!({"scope": "selection", "ids": ["s_d"]})).unwrap()).len(), 1);
        assert_eq!(rules_of(&s.run("validate.run", &json!({"ids": ["s_a"]})).unwrap()).len(), 0);
        // Selection scope with no ids reads the selection.
        s.sel.select_single("s_d");
        assert_eq!(rules_of(&s.run("validate.run", &json!({"scope": "selection"})).unwrap()).len(), 1);
        s.sel.clear();
        assert_eq!(rules_of(&s.run("validate.run", &json!({"scope": "selection"})).unwrap()).len(), 0);
        // Read-only.
        assert_eq!(s.doc, before);
        assert!(!s.can_undo());
    }

    #[test]
    fn run_merges_live_and_batch_in_order() {
        let mut d = doc(&[("x", "Database", "X"), ("y", "Database", "Y")]);
        d.model.add_relationship(Relationship::new("r1", "flowchart.Flow", "x", "y")).unwrap();
        let mut c = Connector::new("c1", End::dynamic("s_x"), End::dynamic("s_y"));
        c.relationship = Some("r1".into());
        d.pages[0].connectors.push(c);
        let mut s = Session::new(d);
        let r = s.run("validate.run", &json!({"scope": "all"})).unwrap();
        // Error first, then the info findings of unreachable symbols (no terminator).
        assert_eq!(rules_of(&r), ["flowchart.illegalFlow", "flowchart.unreachable", "flowchart.unreachable"]);
        assert_eq!((r["live"].as_u64(), r["batch"].as_u64()), (Some(1), Some(2)));
    }

    /// Three free shapes in a row, a straight connector from the first to the third through the
    /// second, and a connector with a loose end below the third.
    fn free_drawing() -> Session {
        let mut s = Session::new(Document::new());
        // Routing avoids shapes by default, which would detour the crossing this sketch plants;
        // the drawing rule is for pages that route by hand.
        s.run("page.setRouting", &json!({"avoid": "never"})).unwrap();
        for (id, x) in [("a", 100), ("b", 300), ("c", 500)] {
            s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": x, "y": 100, "text": id.to_uppercase(), "id": id})).unwrap();
        }
        s.run("connector.add", &json!({"from": {"shape": "a", "point": 1}, "to": {"shape": "c", "point": 3}, "route": "straight", "id": "ac"}))
            .unwrap();
        s.run("connector.add", &json!({"from": {"shape": "c", "point": 2}, "to": {"x": 560, "y": 260}, "id": "loose"})).unwrap();
        s
    }

    #[test]
    fn free_drawing_is_quiet_until_the_drawing_ruleset_then_fixes_repair() {
        let mut s = free_drawing();
        assert!(rules_of(&s.run("validate.run", &json!({})).unwrap()).is_empty());
        let p = s.run("validate.setProfile", &json!({})).unwrap();
        assert_eq!((p["profile"]["enabled"].as_bool(), p["profile"]["userSet"].as_bool()), (Some(false), Some(false)));
        let p = s.run("validate.setProfile", &json!({"rulesets": ["drawing.tidy"]})).unwrap();
        assert_eq!(p["profile"]["rulesets"], json!(["drawing.tidy"]));
        assert_eq!((p["profile"]["enabled"].as_bool(), p["profile"]["userSet"].as_bool()), (Some(true), Some(true)));
        // The live unglued end shows at once; the crossing is a batch finding.
        assert_eq!(rules_of(&s.run("validate.list", &json!({})).unwrap()), ["drawing.ungluedEnd"]);
        let r = s.run("validate.run", &json!({})).unwrap();
        assert_eq!(rules_of(&r), ["drawing.connectorCrossesShape", "drawing.ungluedEnd"]);
        let issues = r["issues"].as_array().unwrap();
        for i in issues {
            assert_eq!(i["subjects"], json!([]));
            let fix = &i["fixes"][0];
            // Every fix is a command that runs as given and removes its issue.
            let mut t = Session::new(s.doc.clone());
            t.run(fix["command"].as_str().unwrap(), &fix["params"]).unwrap();
            let after = t.run("validate.run", &json!({})).unwrap();
            assert!(!rules_of(&after).contains(&i["rule"].as_str().unwrap().to_string()), "{fix}: {after}");
        }
        assert_eq!(issues[1]["fixes"][0]["params"], json!({"id": "loose", "end": "end", "shape": "c", "point": 2}));
        // Switching the profile off silences everything; one undo brings it back.
        s.run("validate.setProfile", &json!({"enabled": false})).unwrap();
        assert!(rules_of(&s.run("validate.run", &json!({})).unwrap()).is_empty());
        assert!(s.issues.is_empty());
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(rules_of(&s.run("validate.run", &json!({})).unwrap()).len(), 2);
    }

    #[test]
    fn suppressions_and_overrides() {
        let mut s = free_drawing();
        s.run("validate.setProfile", &json!({"rulesets": ["drawing.tidy"]})).unwrap();
        s.run("connector.add", &json!({"from": {"shape": "a", "point": 2}, "to": {"x": 150, "y": 300}, "id": "loose2"})).unwrap();
        let l = s.run("validate.list", &json!({"rule": "drawing.ungluedEnd"})).unwrap();
        let keys: Vec<String> =
            l["issues"].as_array().unwrap().iter().map(|i| format!("drawing.ungluedEnd||{}|p1", i["items"][0].as_str().unwrap())).collect();
        assert_eq!(keys.len(), 2);
        let r = s.run("validate.suppress", &json!({"key": keys[0], "reason": "left loose on purpose"})).unwrap();
        assert_eq!(r["added"], true);
        assert_eq!(s.run("validate.suppress", &json!({"key": keys[0]})).unwrap()["added"], false);
        let l = s.run("validate.list", &json!({"rule": "drawing.ungluedEnd"})).unwrap();
        assert_eq!(l["issues"].as_array().unwrap().len(), 1, "exactly one issue is hidden");
        assert_eq!(s.doc.conformance.suppressions[0].reason, "left loose on purpose");
        // An override downgrades a severity.
        s.run("validate.setProfile", &json!({"overrides": [{"rule": "drawing.ungluedEnd", "severity": "info"}]})).unwrap();
        let l = s.run("validate.list", &json!({"rule": "drawing.ungluedEnd"})).unwrap();
        assert_eq!(l["issues"][0]["severity"], "info");
        let e = s.run("validate.explain", &json!({"rule": "drawing.ungluedEnd"})).unwrap();
        assert_eq!((e["severity"].as_str(), e["declaredSeverity"].as_str()), (Some("info"), Some("warning")));
        assert_eq!(e["override"]["severity"], "info");
        s.run("validate.setProfile", &json!({"overrides": [{"rule": "drawing.ungluedEnd", "disabled": true}]})).unwrap();
        assert!(rules_of(&s.run("validate.list", &json!({})).unwrap()).is_empty());
    }

    #[test]
    fn explain_returns_the_clause_and_the_ruleset() {
        let mut s = Session::new(doc(&[("a", "Process", "A")]));
        let e = s.run("validate.explain", &json!({"rule": "flowchart.danglingDecision"})).unwrap();
        assert_eq!(e["reference"], "ISO 5807:1985, decision symbol: one entry, alternative exits");
        assert_eq!((e["ruleset"].as_str(), e["pack"].as_str(), e["mode"].as_str()), (Some("flowchart.iso5807"), Some("flowchart"), Some("batch")));
        assert_eq!((e["severity"].as_str(), e["selected"].as_bool()), (Some("warning"), Some(true)));
        assert_eq!(e["check"]["kind"], "minOutgoing");
        let d = s.run("validate.explain", &json!({"rule": "drawing.overlap"})).unwrap();
        assert_eq!((d["ruleset"].as_str(), d["selected"].as_bool()), (Some("drawing.tidy"), Some(false)));
        assert!(s.run("validate.explain", &json!({"rule": "nope.rule"})).is_err());
        assert!(!s.can_undo());
    }

    #[test]
    fn next_and_previous_select_each_issue_on_its_page() {
        let mut s = free_drawing();
        s.run("page.add", &json!({"name": "Two"})).unwrap();
        // A fixed sheet, so a shape far to the right is off it (an infinite board has no edge).
        s.run("page.setup", &json!({"board": "fixed"})).unwrap();
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 2000, "y": 100, "text": "Far", "id": "far"})).unwrap();
        s.run("page.setActive", &json!({"page": 0})).unwrap();
        s.run("validate.setProfile", &json!({"rulesets": ["drawing.tidy"]})).unwrap();
        let undo_before = s.can_undo();
        let r = s.run("validate.next", &json!({})).unwrap();
        // Order: the warnings by rule (crossing, offBoard, ungluedEnd).
        assert_eq!((r["index"].as_u64(), r["count"].as_u64()), (Some(0), Some(3)));
        assert_eq!(r["issue"]["rule"], "drawing.connectorCrossesShape");
        assert_eq!(s.sel.items, ["ac"]);
        let r = s.run("validate.next", &json!({})).unwrap();
        assert_eq!((r["index"].as_u64(), r["issue"]["rule"].as_str()), (Some(1), Some("drawing.offBoard")));
        assert_eq!((s.sel.items.clone(), s.doc.active_index()), (vec!["far".to_string()], 1));
        let r = s.run("validate.next", &json!({})).unwrap();
        assert_eq!((r["index"].as_u64(), s.doc.active_index()), (Some(2), 0));
        assert_eq!(s.sel.items, ["loose"]);
        // Wraps around, both ways.
        assert_eq!(s.run("validate.next", &json!({})).unwrap()["index"], 0);
        assert_eq!(s.run("validate.previous", &json!({})).unwrap()["index"], 2);
        s.sel.clear();
        assert_eq!(s.run("validate.previous", &json!({})).unwrap()["index"], 2);
        // Showing issues is not an edit.
        assert_eq!(s.can_undo(), undo_before);
        s.run("validate.setProfile", &json!({"enabled": false})).unwrap();
        let r = s.run("validate.next", &json!({})).unwrap();
        assert_eq!((r["index"].is_null(), r["count"].as_u64()), (true, Some(0)));
    }

    #[test]
    fn choosing_a_notation_turns_its_ruleset_on() {
        let mut s = Session::new(Document::new());
        s.run("shape.add", &json!({"stencil": "basic.rectangle", "x": 0, "y": 0})).unwrap();
        assert!(s.doc.conformance.is_default());
        s.run("shape.add", &json!({"stencil": "flowchart.database", "x": 200, "y": 0, "text": "A"})).unwrap();
        assert!(s.doc.conformance.enabled && !s.doc.conformance.user_set);
        assert_eq!(s.doc.conformance.rulesets, ["flowchart.iso5807"]);
        let rs = s.run("notation.rulesets", &json!({})).unwrap();
        let active: Vec<&str> =
            rs["rulesets"].as_array().unwrap().iter().filter(|r| r["active"] == true).map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(active, ["flowchart.iso5807"]);
    }

    #[test]
    fn profile_params_are_checked() {
        let mut s = free_drawing();
        let long = "x".repeat(100_000);
        for bad in [
            json!({"enabled": "yes"}),
            json!({"rulesets": "drawing.tidy"}),
            json!({"rulesets": ["nope.set"]}),
            json!({"rulesets": [3]}),
            json!({"rulesets": vec!["drawing.tidy"; 1000]}),
            json!({"overrides": [{"rule": "nope.rule"}]}),
            json!({"overrides": [{"rule": "drawing.overlap", "severity": "fatal"}]}),
            json!({"overrides": [{"rule": "drawing.overlap", "disabled": 1}]}),
            json!({"overrides": [{}]}),
            json!({"suppressions": [{"reason": "x"}]}),
            json!({"suppressions": [{"key": long}]}),
        ] {
            assert!(s.run("validate.setProfile", &bad).is_err(), "{bad}");
        }
        for bad in [json!({}), json!({"key": ""}), json!({"key": long}), json!({"key": "k", "reason": 3})] {
            assert!(s.run("validate.suppress", &bad).is_err(), "{bad}");
        }
        for bad in [json!({}), json!({"rule": 1}), json!({"rule": long})] {
            assert!(s.run("validate.explain", &bad).is_err(), "{bad}");
        }
        assert!(s.doc.conformance.is_default(), "refused params change nothing");
    }

    #[test]
    fn hostile_params_are_errors() {
        let mut s = Session::new(doc(&[("a", "Process", "A")]));
        let long = "x".repeat(100_000);
        for bad in [json!({"severity": "fatal"}), json!({"severity": 3}), json!({"rule": []}), json!({"rule": long}), json!({"severity": long})] {
            assert!(s.run("validate.list", &bad).is_err(), "{bad}");
        }
        for bad in [
            json!({"scope": "everything"}),
            json!({"scope": 1}),
            json!({"scope": "page", "page": "Nope"}),
            json!({"scope": "page", "page": 99}),
            json!({"scope": "all", "page": "Page-1"}),
            json!({"scope": "page", "ids": ["a"]}),
            json!({"ids": "a"}),
            json!({"ids": [1]}),
            json!({"ids": [long]}),
            json!({"page": long}),
            json!({"ids": vec!["a"; 20_000]}),
        ] {
            assert!(s.run("validate.run", &bad).is_err(), "{bad}");
        }
        // Junk that is not an object at all reads as no params.
        assert!(s.run("validate.run", &json!(null)).is_ok());
        assert!(s.run("validate.list", &json!([1, 2])).is_ok());
    }
}
