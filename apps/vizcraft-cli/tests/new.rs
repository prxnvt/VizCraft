//! `vizcraft-cli new`: a document from a template that `inspect` reads, a preview, the list, an
//! unknown template and usage errors, through the real binary.

use std::path::PathBuf;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vizcraft-cli")).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vizcraft-cli-new-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_template_writes_a_document_inspect_reads_and_a_preview() {
    let d = dir("decision");
    let out = d.join("decision.vizcraft").to_string_lossy().into_owned();
    let svg = d.join("decision.svg").to_string_lossy().into_owned();
    let o = cli(&["new", "--template", "flowchart.decision", "--svg", &svg, &out]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("wrote") && text(&o).contains("6 shapes"), "{}", text(&o));
    let o = cli(&["inspect", &out]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("6 shapes, 6 connectors"), "{}", text(&o));
    let preview = std::fs::read_to_string(&svg).unwrap();
    assert!(preview.starts_with("<svg") && preview.contains("Inputs complete?"));
    let o = cli(&["new", "--template", "flowchart.ciPipeline", "--json", &out]);
    let r: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["template"], "flowchart.ciPipeline");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn list_filters_by_audience() {
    let o = cli(&["new", "--list"]);
    assert!(o.status.success() && text(&o).contains("flowchart.decision") && text(&o).contains("basic.blank"), "{}", text(&o));
    let o = cli(&["new", "--list", "--audience", "enterprise", "--json"]);
    let r: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let ids: Vec<&str> = r["templates"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"flowchart.decision") && !ids.contains(&"basic.blank"), "{ids:?}");
}

#[test]
fn an_unknown_template_exits_1_and_usage_mistakes_exit_2() {
    let d = dir("errors");
    let out = d.join("x.vizcraft").to_string_lossy().into_owned();
    let o = cli(&["new", "--template", "nope.nothing", &out]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("unknown template"), "{}", text(&o));
    assert!(!d.join("x.vizcraft").exists());
    for args in [
        &["new", "--template", "flowchart.decision"][..],
        &["new", &out][..],
        &["new", "--list", "--template", "flowchart.decision"][..],
        &["new", "--template"][..],
        &["new", "--frobnicate", &out][..],
    ] {
        let o = cli(args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o));
    }
    let _ = std::fs::remove_dir_all(&d);
}
