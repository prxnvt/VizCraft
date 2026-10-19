//! `vizcraft-cli analyze`: cycles as JSON, `--check` failing on a cycle, a compare of two files
//! and usage errors, through the real binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vizcraft-cli")).arg("analyze").args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vizcraft-cli-analyze-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A flowchart model a -> b -> c, with c -> b when `cycle`, and `b` named `b_name`.
fn write_doc(path: &Path, cycle: bool, b_name: &str) {
    let mut rels = vec![
        json!({"id": "r1", "type": "flowchart.Flow", "source": "a", "target": "b"}),
        json!({"id": "r2", "type": "flowchart.Flow", "source": "b", "target": "c"}),
    ];
    if cycle {
        rels.push(json!({"id": "r3", "type": "flowchart.Flow", "source": "c", "target": "b"}));
    }
    let model = json!({"schemaVersion": 1, "elements": [
        {"id": "a", "type": "flowchart.Process", "name": "A"},
        {"id": "b", "type": "flowchart.Process", "name": b_name},
        {"id": "c", "type": "flowchart.Process", "name": "C"},
    ], "relationships": rels});
    let doc = json!({"format": "vizcraft", "version": 4, "model": model, "document": {"pages": [{"id": "p1", "name": "P"}]}});
    std::fs::write(path, doc.to_string()).unwrap();
}

#[test]
fn cycles_as_json_and_check_exits_1_on_a_cycle() {
    let d = dir("cycles");
    let doc = d.join("m.vizcraft");
    write_doc(&doc, true, "B");
    let doc_s = doc.to_string_lossy().into_owned();
    let o = cli(&[&doc_s, "--kind", "cycles", "--json"]);
    assert!(o.status.success(), "{}", text(&o));
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["elements"], json!(["b", "c"]));
    assert_eq!(r["groups"][0]["relationships"], json!(["r2", "r3"]));
    let o = cli(&[&doc_s, "--kind", "cycles", "--check"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("1 cycle through 2 elements"), "{}", text(&o));
    write_doc(&doc, false, "B");
    let o = cli(&[&doc_s, "--kind", "cycles", "--check"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = cli(&[&doc_s, "--kind", "dependencies", "--id", "a", "--depth", "1"]);
    assert!(o.status.success() && text(&o).contains("  b  B"), "{}", text(&o));
    let o = cli(&[&doc_s, "--kind", "path", "--from", "a", "--to", "c", "--json"]);
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["length"], 2);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn diff_of_two_files_reports_the_rename() {
    let d = dir("diff");
    let (before, after) = (d.join("before.vizcraft"), d.join("after.vizcraft"));
    write_doc(&before, false, "B");
    write_doc(&after, true, "Bee");
    let (b, a) = (before.to_string_lossy().into_owned(), after.to_string_lossy().into_owned());
    let o = cli(&[&a, "--kind", "diff", "--against", &b, "--json"]);
    assert!(o.status.success(), "{}", text(&o));
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["elements"]["changed"], json!([{"id": "b", "fields": [{"field": "name", "before": "B", "after": "Bee"}]}]));
    assert_eq!(r["relationships"]["added"], json!(["r3"]));
    let o = cli(&[&a, "--kind", "diff", "--against", &b, "--check"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("changed  b (name)"), "{}", text(&o));
    let o = cli(&[&a, "--kind", "diff", "--against", &a, "--check"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        vec![],
        vec!["x.vizcraft"],
        vec!["x.vizcraft", "--kind", "nope"],
        vec!["x.vizcraft", "--kind", "diff"],
        vec!["x.vizcraft", "--kind", "cycles", "--depth", "many"],
        vec!["x.vizcraft", "--kind", "metrics", "--check"],
        vec!["x.vizcraft", "--kind", "cycles", "--bogus"],
    ] {
        let o = cli(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o));
    }
    let o = cli(&["/nonexistent/x.vizcraft", "--kind", "cycles"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
}
