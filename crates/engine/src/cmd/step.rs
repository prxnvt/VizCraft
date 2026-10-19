//! Numbered steps on connectors (`connector.setStep`, `connector.renumberSteps`): the order of
//! requests on a topology, drawn as a badge a quarter of the way along each connector
//! (`vizcraft_diagram::step_badge`). Later work (C4 dynamic views) extends these commands.

use serde_json::{Value, json};
use vizcraft_diagram::step_badge::MAX_STEP;

use super::model::page_param;
use super::{has_connectors, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Return the step commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector.setStep", "Set Step", "Notation > Steps", set_step)
            .params(r#"{"id": connector or relationship id, "step": 1..999 | null} -> {"id", "step"}; a relationship id sets the step on every connector drawing it; null clears it"#)
            .when(has_connectors),
        CommandSpec::new("connector.renumberSteps", "Renumber Steps", "Notation > Steps", renumber)
            .params(r#"{"page"?: page} -> {"renumbered"}; the numbered connectors become 1 to n in their current order, ties broken by page order"#)
            .when(has_connectors),
    ]
}

/// The `step` param: an integer 1 to [`MAX_STEP`], or null for none (0).
fn step_param(v: &Value) -> Result<u16, EngineError> {
    let bad = || EngineError::Params(format!("`step` must be a whole number from 1 to {MAX_STEP}, or null"));
    match v.get("step") {
        Some(Value::Null) => Ok(0),
        Some(n) => {
            let n = n.as_u64().ok_or_else(bad)?;
            u16::try_from(n).ok().filter(|n| (1..=MAX_STEP).contains(n)).ok_or_else(bad)
        }
        None => Err(EngineError::Params("`step` (a number or null) is required".into())),
    }
}

fn set_step(s: &mut Session, v: &Value) -> CmdResult {
    let id = crate::p::req_str(v, "id")?.to_string();
    let step = step_param(v)?;
    let mut found = 0usize;
    for page in &mut s.doc.pages {
        for c in page.connectors.iter_mut().filter(|c| c.id == id || c.relationship.as_deref() == Some(id.as_str())) {
            c.step = step;
            found += 1;
        }
    }
    if found == 0 {
        return Err(EngineError::Params(format!("`{}` is not a connector or a drawn relationship", short(&id))));
    }
    Ok(json!({"id": id, "step": if step == 0 { Value::Null } else { json!(step) }}))
}

fn renumber(s: &mut Session, v: &Value) -> CmdResult {
    let i = page_param(&s.doc, v)?;
    let page = s.doc.pages.get_mut(i).ok_or_else(|| EngineError::Params("no such page".into()))?;
    let mut order: Vec<(u16, usize)> = page.connectors.iter().enumerate().filter(|(_, c)| c.step > 0).map(|(k, c)| (c.step, k)).collect();
    order.sort();
    let mut changed = 0usize;
    for (n, (_, k)) in order.iter().enumerate() {
        let want = u16::try_from(n.saturating_add(1)).unwrap_or(MAX_STEP).min(MAX_STEP);
        if let Some(c) = page.connectors.get_mut(*k)
            && c.step != want
        {
            c.step = want;
            changed += 1;
        }
    }
    Ok(json!({"renumbered": changed, "steps": order.len()}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vizcraft_diagram::{Connector, Document, End, Point};

    use crate::Session;

    fn session() -> Session {
        let mut d = Document::new();
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            let y = i as f64 * 50.0;
            d.pages[0].connectors.push(Connector::new(*id, End::free(Point::new(0.0, y)), End::free(Point::new(100.0, y))));
        }
        Session::new(d)
    }

    #[test]
    fn set_and_renumber_steps() {
        let mut s = session();
        assert_eq!(s.run("connector.setStep", &json!({"id": "b", "step": 7})).unwrap()["step"], 7);
        s.run("connector.setStep", &json!({"id": "c", "step": 3})).unwrap();
        s.run("connector.setStep", &json!({"id": "a", "step": 3})).unwrap();
        for bad in [
            json!({"id": "a", "step": -1}),
            json!({"id": "a", "step": 1e9}),
            json!({"id": "a", "step": {}}),
            json!({"id": "a", "step": 0}),
            json!({"id": "zz", "step": 1}),
        ] {
            assert!(s.run("connector.setStep", &bad).is_err(), "{bad}");
        }
        let r = s.run("connector.renumberSteps", &json!({})).unwrap();
        assert_eq!(r["steps"], 3);
        let steps: Vec<u16> = s.doc.pages[0].connectors.iter().map(|c| c.step).collect();
        assert_eq!(steps, [1, 3, 2], "3 and 3 by page order, then 7");
        s.run("connector.setStep", &json!({"id": "a", "step": null})).unwrap();
        assert_eq!(s.doc.pages[0].connectors[0].step, 0);
        assert_eq!(s.run("connector.renumberSteps", &json!({})).unwrap()["renumbered"], 2);
    }
}
