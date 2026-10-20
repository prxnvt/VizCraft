//! Annotation commands (`annotation.*`): switch the tolerancing, welding and texture
//! conventions of the symbols drawn on a page between ISO and ASME (AWS for welds) in one undo
//! step, or clear them back to the page's variant. The builders redraw each symbol in place.

use serde_json::{Value, json};
use vizcraft_model::PropValue;
use vizcraft_notation::annotation_rules::{FCF, TEXTURE, WELD};
use vizcraft_notation::infer::Facts;

use super::model::{cat, page_param};
use super::{opt_str, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// The annotation commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("annotation.setStandard", "Annotation Standard", "Sheet > Annotate", set_standard).params(
            r#"{"page"?, "standard"?: "iso"|"asme"|"" (all three families), "gdt"?: "iso"|"asme"|"", "weld"?: "iso"|"asme"|"", "texture"?: "iso"|"asme"|""} -> {"changed": [element id]}; sets the standard property of every feature control frame, welding symbol and surface texture symbol drawn on the page ("asme" gives ASME Y14.5, AWS A2.4 and ASME Y14.36; "" clears it so the page's variant decides), in one undo step"#,
        ),
    ]
}

/// The standard value for `family` (`gdt`, `weld`, `texture`) in convention `choice`.
fn value(family: &str, choice: &str) -> Option<&'static str> {
    Some(match (family, choice) {
        ("gdt", "iso") => "iso1101",
        ("gdt", "asme") => "asmeY14_5",
        ("weld", "iso") => "iso2553",
        ("weld", "asme") => "aws",
        ("texture", "iso") => "iso21920",
        ("texture", "asme") => "asmeY14_36",
        _ => return None,
    })
}

fn set_standard(s: &mut Session, v: &Value) -> CmdResult {
    let all = opt_str(v, "standard")?;
    let mut want: Vec<(&str, &str, Option<&str>)> = Vec::new();
    for (family, ty) in [("gdt", FCF), ("weld", WELD), ("texture", TEXTURE)] {
        let Some(choice) = opt_str(v, family)?.or(all) else { continue };
        let val = match choice {
            "" => None,
            c => Some(value(family, c).ok_or_else(|| EngineError::Params(format!("`{family}` must be iso, asme or \"\", not `{}`", short(c))))?),
        };
        want.push((family, ty, val));
    }
    if want.is_empty() {
        return Err(EngineError::Params("give `standard`, or one of `gdt`, `weld` and `texture`".into()));
    }
    let pi = page_param(&s.doc, v)?;
    let drawn: Vec<String> = s.doc.pages.get(pi).map(|p| p.shapes.iter().filter_map(|sh| sh.element.clone()).collect()).unwrap_or_default();
    let targets: Vec<(String, Option<&str>)> = {
        let f = Facts::new(&s.doc.model, cat());
        s.doc
            .model
            .elements
            .iter()
            .filter(|e| drawn.contains(&e.id))
            .filter_map(|e| want.iter().find(|(_, ty, _)| f.is_a(&e.type_id, ty)).map(|(_, _, val)| (e.id.clone(), *val)))
            .collect()
    };
    let mut changed = Vec::new();
    for (id, val) in targets {
        let Some(e) = s.doc.model.element_mut(&id) else { continue };
        let before = e.properties.get("standard").cloned();
        match val {
            Some(x) => {
                e.properties.insert("standard".into(), PropValue::Text(x.into()));
            }
            None => {
                e.properties.remove("standard");
            }
        }
        if e.properties.get("standard") != before.as_ref() {
            changed.push(id);
        }
    }
    s.status = format!("Standard set on {} symbols", changed.len());
    Ok(json!({"changed": changed}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Document, Session};

    #[test]
    fn the_bracket_switches_to_asme_in_one_step_and_back() {
        let mut s = Session::new(Document::new());
        s.run("example.open", &json!({"example": "annotation.bracket"})).unwrap();
        let positions = |s: &Session| s.doc.page().unwrap().shapes.iter().map(|sh| (sh.id.clone(), sh.x, sh.y)).collect::<Vec<_>>();
        let before = positions(&s);
        let issues = |s: &Session| s.issues.iter().filter(|i| i.rule == "annotation.retiredCharacteristic").count();
        // A concentricity frame is allowed under ISO and retired under ASME.
        let f2 = s.doc.model.elements.iter().find(|e| e.name == "Base flatness").unwrap().id.clone();
        s.run("element.setProperty", &json!({"id": f2, "key": "characteristic", "value": "concentricity"})).unwrap();
        s.run("element.setProperty", &json!({"id": f2, "key": "datums", "value": "A"})).unwrap();
        assert_eq!(issues(&s), 0);
        let r = s.run("annotation.setStandard", &json!({"standard": "asme"})).unwrap();
        assert_eq!(r["changed"].as_array().unwrap().len(), 4, "{r}");
        assert_eq!(issues(&s), 1);
        let after = positions(&s);
        for ((a, ax, ay), (b, bx, by)) in before.iter().zip(&after) {
            assert_eq!(a, b);
            assert!((ax - bx).abs() < 1e-6 && (ay - by).abs() < 1e-6, "{a} moved");
        }
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(issues(&s), 0);
        assert!(s.run("annotation.setStandard", &json!({"gdt": "din"})).is_err());
        assert!(s.run("annotation.setStandard", &json!({})).is_err());
    }
}
