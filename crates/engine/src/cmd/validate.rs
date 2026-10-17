//! Validation commands: batch rules over a scope and the live issue list.
//!
//! Live issues are recomputed by the session after every change (`Session::issues`); batch rules
//! (reachability, dangling decisions, free connector ends, unmatched pairs) run only when asked.
//! Both commands only read, so they are always enabled and never add an undo step.

use serde_json::{Value, json};
use vizcraft_model::{Issue, Severity};
use vizcraft_notation::rules::{self, Scope};

use super::{ids_param, opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return all validate commands: `validate.run`, `validate.list`.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("validate.run", "Validate", "Model > Validate", run).params(
            r#"{"scope"?: "all" (default) | "page" | "selection", "page"?: page id, name or index (page scope; default the active page), "ids"?: [string] (selection scope; default the selection)} -> {"issues": [{"rule", "severity", "message", "subjects", "items", "page"?}], "live": n, "batch": n}; live and batch rules in the scope; shape and connector ids stand for their elements and relationships"#,
        ),
        CommandSpec::new("validate.list", "Issues", "Model > Validate", list).params(
            r#"{"severity"?: "error" | "warning" | "info", "rule"?: string} -> {"issues": [{"rule", "severity", "message", "subjects", "items", "page"?}]}; the live issues, filtered"#,
        ),
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
