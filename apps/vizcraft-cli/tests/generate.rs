//! `vizcraft-cli generate`: a run, a clean check, a stale check after the model changes, a spec
//! beside the document and usage errors, through the real binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vizcraft-cli")).arg("generate").args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vizcraft-cli-generate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A document with two flowchart elements named `first` and `second`.
fn write_doc(path: &Path, second: &str) {
    let model = json!({"schemaVersion": 1, "elements": [
        {"id": "e1", "type": "flowchart.Process", "name": "first"},
        {"id": "e2", "type": "flowchart.Decision", "name": second},
    ]});
    let doc = json!({"format": "vizcraft", "version": 4, "model": model, "document": {"pages": [{"id": "p1", "name": "P"}]}});
    std::fs::write(path, doc.to_string()).unwrap();
}

#[test]
fn check_exits_1_when_the_output_is_stale() {
    let d = dir("stale");
    let doc = d.join("model.vizcraft");
    write_doc(&doc, "second");
    let out = d.join("gen");
    let (doc_s, out_s) = (doc.to_string_lossy().into_owned(), out.to_string_lossy().into_owned());
    let o = cli(&[&doc_s, "--target", "markdown-catalog", "--output", &out_s]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("wrote     catalog.md"), "{}", text(&o));
    let o = cli(&[&doc_s, "--target", "markdown-catalog", "--output", &out_s, "--check"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    write_doc(&doc, "renamed");
    let o = cli(&[&doc_s, "--target", "markdown-catalog", "--output", &out_s, "--check", "--json"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let r: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["stale"], json!(["catalog.md"]));
    let o = cli(&[&doc_s, "--target", "markdown-catalog", "--output", &out_s, "--preview"]);
    assert!(o.status.success() && text(&o).contains("+| second |  | renamed |"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_spec_beside_the_document_is_used_and_a_conflict_exits_1() {
    let d = dir("spec");
    let doc = d.join("model.vizcraft");
    write_doc(&doc, "second");
    std::fs::write(d.join(".vizcraft-gen.toml"), "target = \"markdown-catalog\"\noutput = \"docs\"\n").unwrap();
    let doc_s = doc.to_string_lossy().into_owned();
    let o = cli(&[&doc_s]);
    assert!(o.status.success(), "{}", text(&o));
    let file = d.join("docs/catalog.md");
    let edited = std::fs::read_to_string(&file).unwrap().replace("| second |  | second |", "| second |  | edited by hand |");
    std::fs::write(&file, &edited).unwrap();
    write_doc(&doc, "renamed");
    let o = cli(&[&doc_s]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("codegen.conflict"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn usage_mistakes_exit_2() {
    for args in [&[][..], &["x.vizcraft", "--target", "markdown-catalog"][..], &["x.vizcraft", "--check", "--preview"][..], &["x", "--bogus"][..]] {
        assert_eq!(cli(args).status.code(), Some(2), "{args:?}");
    }
    assert_eq!(cli(&["/no/such/file.vizcraft", "--target", "markdown-catalog", "--output", "/tmp/x"]).status.code(), Some(1));
}
