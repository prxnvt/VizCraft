//! `vizcraft-cli validate`: documents, test cases and packs from `VIZCRAFT_PACKS_DIR`, run as a
//! child process so the catalog is built fresh with the environment variable set.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn cli(args: &[&str], packs: Option<&Path>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_vizcraft-cli"));
    c.arg("validate").args(args);
    match packs {
        Some(p) => c.env("VIZCRAFT_PACKS_DIR", p),
        None => c.env_remove("VIZCRAFT_PACKS_DIR"),
    };
    c.output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// A folder holding the pack `clitest` (one error rule: node names start with N), one test case
/// for it, and two documents: one breaking the rule and one keeping it.
fn packs_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vizcraft-cli-validate-{}", std::process::id()));
    let cases = dir.join("tests").join("clitest");
    std::fs::create_dir_all(&cases).unwrap();
    let pack = json!({
        "id": "clitest", "version": "1.0.0", "name": "CLI test",
        "elementTypes": [{"id": "clitest.Node"}],
        "rules": [{"id": "clitest.named", "severity": "error", "check": {"kind": "nameFormat", "types": ["clitest.Node"], "pattern": "N*"}}],
        "rulesets": [{"id": "clitest.strict", "rules": ["clitest.named"]}]
    });
    std::fs::write(dir.join("clitest.json"), pack.to_string()).unwrap();
    let doc = |name: &str| {
        let model = json!({"schemaVersion": 1, "elements": [{"id": "e1", "type": "clitest.Node", "name": name}]});
        json!({"format": "vizcraft", "version": 4, "model": model, "document": {"pages": [{"id": "p1", "name": "P"}]}})
    };
    std::fs::write(cases.join("bad_name.json"), json!({"document": doc("bad"), "expect": ["clitest.named|e1||"]}).to_string()).unwrap();
    std::fs::write(dir.join("bad.vizcraft"), doc("bad").to_string()).unwrap();
    std::fs::write(dir.join("good.vizcraft"), doc("N1").to_string()).unwrap();
    dir
}

#[test]
fn a_disk_pack_loads_through_the_environment_and_its_cases_run() {
    let dir = packs_dir();
    let cases = dir.join("tests").join("clitest");
    let cases = cases.to_str().unwrap();
    let o = cli(&[cases], Some(&dir));
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("pass bad_name.json"), "{}", text(&o));
    // Without the folder the pack is unknown, its rule never runs and the case fails.
    let o = cli(&[cases], None);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("FAIL bad_name.json"));
    // `--packs` names the folder too.
    assert!(cli(&[cases, "--packs", dir.to_str().unwrap()], None).status.success());

    // A document with an error issue fails; one without passes; JSON output reads back.
    let bad = dir.join("bad.vizcraft");
    let o = cli(&[bad.to_str().unwrap(), "--format", "json"], Some(&dir));
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((v["errors"].as_u64(), v["issues"][0]["rule"].as_str()), (Some(1), Some("clitest.named")));
    assert_eq!(v["profile"]["rulesets"], json!(["clitest.strict"]));
    let good = dir.join("good.vizcraft");
    assert!(cli(&[good.to_str().unwrap()], Some(&dir)).status.success());
    // Held to another ruleset only, the bad document has no error.
    assert!(cli(&[bad.to_str().unwrap(), "--ruleset", "drawing.tidy"], Some(&dir)).status.success());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn builtin_cases_pass_and_usage_mistakes_exit_two() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/notation/packs/tests");
    for pack in ["flowchart", "drawing"] {
        let o = cli(&[root.join(pack).to_str().unwrap()], None);
        assert!(o.status.success(), "{pack}: {}", text(&o));
    }
    for bad in [&[][..], &["--format", "xml", "x"][..], &["a", "b"][..], &["--ruleset"][..], &["--what", "x"][..]] {
        assert_eq!(cli(bad, None).status.code(), Some(2), "{bad:?}");
    }
    assert_eq!(cli(&["/nonexistent/file.vizcraft"], None).status.code(), Some(1));
}
