//! The UML pack through the real binary: its test cases validate, and a class diagram made from
//! the template exports as PlantUML.

use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vizcraft-cli")).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn the_uml_cases_validate() {
    let cases = format!("{}/../../crates/notation/packs/tests/uml", env!("CARGO_MANIFEST_DIR"));
    let o = cli(&["validate", &cases]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("12 cases: 12 passed"), "{}", text(&o));
}

#[test]
fn a_class_diagram_from_the_template_exports_as_plantuml() {
    let d = std::env::temp_dir().join(format!("vizcraft-cli-uml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let doc = d.join("class.vizcraft").to_string_lossy().into_owned();
    let puml = d.join("class.puml").to_string_lossy().into_owned();
    let o = cli(&["new", "--template", "uml.classDiagram", &doc]);
    assert!(o.status.success(), "{}", text(&o));
    let o = cli(&["run", "--open", &doc, "--cmd", &format!("file.export={}", serde_json::json!({"path": puml}))]);
    assert!(o.status.success(), "{}", text(&o));
    let out = std::fs::read_to_string(&puml).unwrap();
    assert!(out.starts_with("@startuml\n") && out.ends_with("@enduml\n"), "{out}");
    assert!(out.contains("abstract class \"Party\"") && out.contains("..|>") && out.contains("*--"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}
