//! Tests of the generator framework through the reference Markdown target, on real folders.

use std::path::PathBuf;

use serde_json::json;
use vizcraft_model::{Element, Model, PropValue, Relationship};

use crate::writer::{self, Action, FsDisk, NoDisk};
use crate::{CodegenError, Manifest, Spec, names, regions};

/// A fresh empty folder under the system temp folder.
fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vizcraft-codegen-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Three flowchart elements, one owned by another, and a flow between two of them.
fn model() -> Model {
    let mut m = Model::new();
    let mut a = Element::new("e1", "flowchart.Process");
    a.name = "Gather input".into();
    a.tag = "P1".into();
    let mut b = Element::new("e2", "flowchart.Decision");
    b.name = "Valid?".into();
    let mut c = Element::new("e3", "flowchart.Process");
    c.name = "Save diagram".into();
    c.properties.insert("note".into(), PropValue::Text("x".into()));
    m.add_element(a).unwrap();
    m.add_element(b).unwrap();
    m.add_element(c).unwrap();
    let mut r = Relationship::new("r1", "flowchart.Flow", "e1", "e2");
    r.name = "next".into();
    m.add_relationship(r).unwrap();
    m
}

fn spec() -> Spec {
    Spec { name: "test".into(), target: "markdown-catalog".into(), output: "out".into(), ..Default::default() }
}

fn run(spec: &Spec, m: &Model, root: &std::path::Path, renames: bool) -> crate::Report {
    let plan = writer::plan(spec, m, vizcraft_notation::catalog(), renames, &FsDisk::new(root)).unwrap();
    writer::apply(&plan, root).unwrap()
}

fn catalog(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("catalog.md")).unwrap()
}

/// Every file under `root` with its bytes, sorted, for byte-identical comparisons.
fn snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push((p.strip_prefix(base).unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn the_catalogue_generates_under_a_manifest_and_a_second_run_is_byte_identical() {
    let root = tmp("first");
    let r = run(&spec(), &model(), &root, false);
    assert_eq!(r.written, ["catalog.md"]);
    let text = catalog(&root);
    let row = text.lines().find(|l| l.starts_with("| p1 | P1 | Gather input |")).unwrap_or_else(|| panic!("{text}"));
    assert!(row.ends_with("|<!-- vizcraft:id=e1 -->"), "{row}");
    assert!(text.contains("## Process (Flowchart)"), "{text}");
    assert!(text.contains("<!-- vizcraft:id=r1 -->"));
    assert!(text.contains("<!-- vizcraft:begin e2 -->"));
    let m = Manifest::parse(&std::fs::read_to_string(root.join(".vizcraft-gen/manifest.json")).unwrap()).unwrap();
    assert_eq!(m.files.len(), 1);
    assert_eq!(m.files[0].sha256, crate::sha256_hex(text.as_bytes()));
    assert_eq!(m.renames.get("e1").map(String::as_str), Some("p1"));
    assert_eq!(std::fs::read_to_string(root.join(".vizcraft-gen/last/catalog.md")).unwrap(), text);

    let before = snapshot(&root);
    let r = run(&spec(), &model(), &root, false);
    assert_eq!((r.written.len(), r.unchanged.as_slice()), (0, &["catalog.md".to_string()][..]));
    assert_eq!(snapshot(&root), before, "a second run changes no byte");
    let plan = writer::plan(&spec(), &model(), vizcraft_notation::catalog(), false, &FsDisk::new(&root)).unwrap();
    assert!(plan.is_current());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_hand_edit_outside_the_changed_lines_merges_and_one_on_them_stops_the_run() {
    let root = tmp("merge");
    run(&spec(), &model(), &root, false);
    let text = catalog(&root);
    // An edit to the intro line, away from the rows a rename changes.
    let edited = text.replacen("# Model catalogue\n", "# Model catalogue\n\nWritten by hand.\n", 1);
    std::fs::write(root.join("catalog.md"), &edited).unwrap();
    let mut m = model();
    m.element_mut("e3").unwrap().name = "Store diagram".into();
    let r = run(&spec(), &m, &root, false);
    assert_eq!(r.merged, ["catalog.md"]);
    assert!(!r.stopped);
    let merged = catalog(&root);
    assert!(merged.contains("Written by hand.") && merged.contains("Store diagram"), "{merged}");

    // Now edit the row the next model change rewrites.
    let clash = merged.replace("| Store diagram |", "| Stored by hand |");
    std::fs::write(root.join("catalog.md"), &clash).unwrap();
    m.element_mut("e3").unwrap().name = "Keep diagram".into();
    let plan = writer::plan(&spec(), &m, vizcraft_notation::catalog(), false, &FsDisk::new(&root)).unwrap();
    assert!(plan.stopped);
    let f = &plan.files[0];
    assert_eq!(f.action, Action::Conflict);
    assert!(f.diff.contains("Stored by hand"), "{}", f.diff);
    assert!(plan.issues.iter().any(|i| i.rule == "codegen.conflict" && i.message.contains("catalog.md")));
    let r = writer::apply(&plan, &root).unwrap();
    assert!(r.stopped);
    assert_eq!(catalog(&root), clash, "a stopped run writes nothing");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_renamed_element_keeps_its_identifier_until_renames_is_asked_for() {
    let root = tmp("rename");
    run(&spec(), &model(), &root, false);
    let mut m = model();
    m.element_mut("e2").unwrap().name = "Is it valid".into();
    let plan = writer::plan(&spec(), &m, vizcraft_notation::catalog(), false, &FsDisk::new(&root)).unwrap();
    assert!(plan.renames.is_empty());
    assert_eq!(plan.manifest.renames.get("e2").map(String::as_str), Some("valid"));
    writer::apply(&plan, &root).unwrap();
    assert!(catalog(&root).contains("| valid |  | Is it valid |"), "{}", catalog(&root));

    let plan = writer::plan(&spec(), &m, vizcraft_notation::catalog(), true, &FsDisk::new(&root)).unwrap();
    assert_eq!(plan.renames, [names::Rename { id: "e2".into(), from: "valid".into(), to: "is-it-valid".into() }]);
    writer::apply(&plan, &root).unwrap();
    assert!(catalog(&root).contains("| is-it-valid |"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_protected_region_survives_and_an_orphan_is_kept_at_the_end() {
    let root = tmp("regions");
    run(&spec(), &model(), &root, false);
    let text = catalog(&root);
    let with_notes = text
        .replace("<!-- vizcraft:begin e1 -->\n", "<!-- vizcraft:begin e1 -->\nAsk for the file name.\n")
        .replace("<!-- vizcraft:begin e3 -->\n", "<!-- vizcraft:begin e3 -->\nSaved twice a day.\n");
    std::fs::write(root.join("catalog.md"), &with_notes).unwrap();
    let mut m = model();
    m.element_mut("e1").unwrap().name = "Collect input".into();
    m.remove(&["e3".to_string()]);
    let r = run(&spec(), &m, &root, false);
    assert!(!r.stopped, "{:?}", r.issues);
    let out = catalog(&root);
    assert!(out.contains("<!-- vizcraft:begin e1 -->\nAsk for the file name.\n<!-- vizcraft:end e1 -->"), "{out}");
    assert!(out.contains("Collect input"));
    assert!(out.trim_end().ends_with("<!-- vizcraft:begin e3 -->\nSaved twice a day.\n<!-- vizcraft:end e3 -->"), "{out}");
    assert!(r.issues.iter().any(|i| i.rule == "codegen.orphanRegion" && i.subjects == ["e3"]));
    // Stable: the next run leaves it alone.
    let before = snapshot(&root);
    run(&spec(), &m, &root, false);
    assert_eq!(snapshot(&root), before);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn paths_outside_the_output_folder_are_refused() {
    for bad in ["", "/etc/passwd", "../x.md", "a/../../x", "a//b", "./a", "a\\b", "c:x", "a\0b", &"x".repeat(600)] {
        assert!(matches!(writer::relative_path(bad), Err(CodegenError::Path(_))), "{bad:?}");
    }
    assert!(writer::relative_path("docs/model/catalog.md").is_ok());
    let mut s = spec();
    s.options.insert("file".into(), json!("../escape.md"));
    let e = writer::plan(&s, &model(), vizcraft_notation::catalog(), false, &NoDisk).unwrap_err();
    assert!(matches!(e, CodegenError::Path(_)), "{e}");
    s.options.insert("file".into(), json!(".vizcraft-gen/manifest.json"));
    assert!(writer::plan(&s, &model(), vizcraft_notation::catalog(), false, &NoDisk).is_err());
    // A spec's output folder is relative and never climbs out.
    let s = Spec { output: "../../elsewhere".into(), ..spec() };
    assert!(s.output_dir(std::path::Path::new("/repo")).is_err());
    // A hand-edited manifest naming a file outside the folder loses that row.
    let m = Manifest::parse(r#"{"files": [{"path": "../../etc/hosts", "sha256": "x"}, {"path": "ok.md", "sha256": "y"}]}"#).unwrap();
    assert_eq!(m.files.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_inside_the_output_is_never_followed() {
    let root = tmp("link");
    let outside = tmp("link-target");
    std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();
    let mut s = spec();
    s.options.insert("file".into(), json!("sub/catalog.md"));
    let e = writer::plan(&s, &model(), vizcraft_notation::catalog(), false, &FsDisk::new(&root)).unwrap_err();
    assert!(matches!(e, CodegenError::Path(_)), "{e}");
    let plan = writer::plan(&s, &model(), vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    assert!(writer::apply(&plan, &root).is_err());
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none(), "nothing written through the link");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn a_file_the_generator_does_not_own_is_a_conflict_and_a_dropped_file_is_deleted() {
    let root = tmp("owned");
    std::fs::write(root.join("catalog.md"), "mine\n").unwrap();
    let plan = writer::plan(&spec(), &model(), vizcraft_notation::catalog(), false, &FsDisk::new(&root)).unwrap();
    assert!(plan.stopped && plan.files[0].action == Action::Conflict);
    std::fs::remove_file(root.join("catalog.md")).unwrap();
    run(&spec(), &model(), &root, false);
    let mut s = spec();
    s.options.insert("file".into(), json!("model.md"));
    let r = run(&s, &model(), &root, false);
    assert_eq!((r.written.as_slice(), r.deleted.as_slice()), (&["model.md".to_string()][..], &["catalog.md".to_string()][..]));
    assert!(!root.join("catalog.md").exists() && !root.join(".vizcraft-gen/last/catalog.md").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unsupported_and_flattened_constructs_raise_issues() {
    let mut m = model();
    m.relationships[0].source_port = Some("out".into());
    let plan = writer::plan(&spec(), &m, vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    assert!(plan.issues.iter().any(|i| i.rule == "codegen.flattened" && i.subjects == ["r1"]), "{:?}", plan.issues);
    let text = plan.files[0].write.clone().unwrap();
    assert!(text.contains("| p1 | valid |"), "ports left out without the opt-in");
    let mut s = spec();
    s.options.insert("flatten".into(), json!(true));
    let plan = writer::plan(&s, &m, vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    assert!(plan.issues.is_empty());
    assert!(plan.files[0].write.as_deref().unwrap().contains("| p1.out | valid |"));
}

#[test]
fn scope_and_order_are_stable() {
    let mut s = spec();
    s.scope.types = vec!["flowchart.Process".into()];
    let plan = writer::plan(&s, &model(), vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    let text = plan.files[0].write.clone().unwrap();
    assert!(!text.contains("Valid?") && !text.contains("## Relationships"));
    // The same model built in another order generates the same text.
    let mut m = model();
    m.elements.reverse();
    let a = writer::plan(&spec(), &model(), vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    let b = writer::plan(&spec(), &m, vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    assert_eq!(a.files[0].write, b.files[0].write);
}

#[test]
fn specs_read_from_toml_and_json_and_junk_is_an_error() {
    let s = Spec::from_toml(
        "name = \"c\"\ntarget = \"markdown-catalog\"\noutput = \"docs\"\n[options]\nnotes = false\n[scope]\npacks = [\"flowchart\"]\n",
    )
    .unwrap();
    assert_eq!((s.output.as_str(), s.scope.packs.as_slice()), ("docs", &["flowchart".to_string()][..]));
    assert_eq!(s.options.get("notes"), Some(&json!(false)));
    let j = Spec::from_json(
        r#"{"name": "c", "target": "markdown-catalog", "output": "docs", "options": {"notes": false}, "scope": {"packs": ["flowchart"]}}"#,
    )
    .unwrap();
    assert_eq!(s, j);
    assert_eq!(s.checksum(), j.checksum());
    for junk in ["[[[", "name = 5", "\u{0}"] {
        assert!(Spec::from_toml(junk).is_err(), "{junk:?}");
    }
    assert!(Spec::from_value(&json!({"scope": {"packs": vec!["x"; 20_000]}})).is_err());
    assert!(Spec::from_value(&json!({"target": "x".repeat(1000)})).is_err());
    assert!(Spec::read(std::path::Path::new("/no/such/spec.toml")).is_err());
    let unknown = Spec { target: "nope".into(), ..spec() };
    assert!(matches!(writer::plan(&unknown, &model(), vizcraft_notation::catalog(), false, &NoDisk), Err(CodegenError::Spec(_))));
}

#[test]
fn identifiers_follow_the_target_rules_and_are_unique() {
    use names::{Case, NameRules};
    let r = |case| NameRules { case, max_len: 20, reserved: &["type"], digit_prefix: Some("_"), fallback: "item" };
    assert_eq!(names::sanitize("Gather input!", &r(Case::Kebab)), "gather-input");
    assert_eq!(names::sanitize("gatherInput", &r(Case::Snake)), "gather_input");
    assert_eq!(names::sanitize("gather input", &r(Case::Pascal)), "GatherInput");
    assert_eq!(names::sanitize("gather input", &r(Case::Camel)), "gatherInput");
    assert_eq!(names::sanitize("gather input", &r(Case::ScreamingSnake)), "GATHER_INPUT");
    assert_eq!(names::sanitize("2 pumps", &r(Case::Snake)), "_2_pumps");
    assert_eq!(names::sanitize("Type", &r(Case::Snake)), "type_");
    assert_eq!(names::sanitize("\u{1F600}", &r(Case::Snake)), "item");
    assert!(names::sanitize(&"long ".repeat(100), &r(Case::Snake)).len() <= 21);
    let mut m = Model::new();
    for i in 0..5 {
        let mut e = Element::new(format!("e{i}"), "flowchart.Process");
        e.name = "Same".into();
        m.add_element(e).unwrap();
    }
    let view = crate::ModelView::new(&m, vizcraft_notation::catalog(), &Default::default());
    let (n, _) = crate::Names::resolve(&view, &r(Case::Kebab), &Default::default(), false);
    let mut all: Vec<&str> = n.map().values().map(String::as_str).collect();
    all.sort();
    assert_eq!(all, ["same", "same-2", "same-3", "same-4", "same-5"]);
}

#[test]
fn regions_parse_only_matched_blocks() {
    let text = "<!-- vizcraft:begin a -->\nx\n<!-- vizcraft:end a -->\n<!-- vizcraft:begin b -->\nno end\n<!-- vizcraft:beginc -->\n";
    let b = regions::blocks(text, "<!--", "-->");
    assert_eq!(b, [regions::Block { id: "a".into(), body: "x\n".into() }]);
    let (out, issues) = regions::carry("f", "<!-- vizcraft:begin a -->\n<!-- vizcraft:end a -->\n", text, "<!--", "-->");
    assert_eq!(out, "<!-- vizcraft:begin a -->\nx\n<!-- vizcraft:end a -->\n");
    assert!(issues.is_empty());
}

#[test]
fn hostile_models_never_panic() {
    let mut m = Model::new();
    for id in ["", " ", "--><script>", "\u{0}", "|", &"x".repeat(5000), "e\nnew line"] {
        let mut e = Element::new(id, "nope");
        e.name = id.to_string();
        e.tag = id.to_string();
        e.owner = Some(id.to_string());
        let _ = m.add_element(e);
    }
    let plan = writer::plan(&spec(), &m, vizcraft_notation::catalog(), false, &NoDisk).unwrap();
    let text = plan.files[0].write.clone().unwrap();
    assert!(!text.contains("<script>"));
    for line in text.lines().filter(|l| l.contains("vizcraft:")) {
        assert_eq!(line.matches("-->").count(), 1, "{line}");
    }
}
