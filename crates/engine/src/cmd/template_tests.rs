//! Tests of the document type and example commands: every shipped type and example builds
//! cleanly, a type draws no content and sets up its viewpoint and rules, the hooks run or are
//! skipped by registration, the variant follows the choice and the project's standards, and a
//! failing example changes nothing.

use serde_json::{Value, json};
use vizcraft_diagram::Document;

use super::*;
use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn blank() -> Session {
    Session::new(Document::new())
}

/// A pack built in code with IEC and JIC variants, a document type and examples that use every
/// engineering field (the `engtest` convention of `cmd/notation.rs`), loaded into the catalog
/// once.
fn engtest() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let pack = json!({
            "id": "engtest", "version": "1.0.0", "name": "Engineering test", "audience": ["engineering"],
            "glyphs": [
                {"id": "engtest.coil", "label": "Coil", "category": "T", "outline": "ellipse", "w": 40, "h": 40, "fill": "#ffffff", "line": "#000000", "elementType": "engtest.Coil"},
                {"id": "engtest.coilJic", "label": "Coil (JIC)", "category": "T", "outline": "rectangle", "w": 40, "h": 40, "fill": "#ffffff", "line": "#000000"}
            ],
            "elementTypes": [{"id": "engtest.Coil", "glyph": "engtest.coil"}],
            "viewpoints": [{"id": "engtest.Schematic", "label": "Schematic", "types": ["*"]}],
            "variants": [
                {"id": "engtest.iec", "label": "IEC"},
                {"id": "engtest.jic", "label": "JIC", "glyphs": {"engtest.Coil": "engtest.coilJic"}}
            ],
            "templates": [
                {
                    "id": "engtest.sheet", "label": "Schematic Sheet", "description": "An empty schematic.", "discipline": "engineering",
                    "viewpoint": "engtest.Schematic", "variant": "engtest.iec", "sheet": "iso.a3", "module": "iso2_5mm",
                    "key": true, "titleBlock": true, "liveChecks": false
                }
            ],
            "examples": [
                {
                    "id": "engtest.schematic", "label": "Schematic", "description": "One coil.",
                    "viewpoint": "engtest.Schematic", "variant": "engtest.iec", "sheet": "iso.a3", "module": "iso2_5mm",
                    "key": true, "titleBlock": true,
                    "seed": [
                        {"command": "shape.add", "as": "k1", "params": {"stencil": "engtest.coil", "x": 0, "y": 0, "text": "K1"}},
                        {"command": "engtest.later", "optional": true}
                    ]
                },
                {"id": "engtest.plain", "label": "Plain", "seed": [{"command": "shape.add", "params": {"stencil": "engtest.coil", "x": 0, "y": 0, "text": "K2"}}]},
                {"id": "engtest.broken", "label": "Broken", "seed": [
                    {"command": "shape.add", "params": {"stencil": "engtest.coil", "x": 0, "y": 0}},
                    {"command": "relationship.create", "params": {"from": "nothing", "to": "nothing"}}
                ]}
            ]
        });
        let p = vizcraft_notation::loader::load_json(&pack.to_string()).unwrap();
        vizcraft_notation::catalog::add_pack(p).unwrap();
    });
}

/// A session whose registry also holds fake hook commands that leave a mark in the status.
fn with_hooks(ids: &[&'static str]) -> Session {
    let mut s = blank();
    let mut all: Vec<CommandSpec> = s.registry.all().to_vec();
    for id in ids {
        all.push(CommandSpec::new(id, "Fake hook", "Test", |s, v| {
            let page = super::super::page_mut(s)?;
            page.name = format!("{}+", page.name);
            Ok(json!({"seen": v}))
        }));
    }
    all.push(CommandSpec::new("sheet.presets", "Fake presets", "Test", |_, _| {
        Ok(json!({"presets": [{"id": "iso.a3.landscape"}, {"id": "asme.d.landscape"}]}))
    }));
    s.registry = crate::Registry::new(all);
    s
}

/// Every shipped document type, then every shipped example.
fn shipped() -> Vec<String> {
    let mut ids = types();
    for pack in cat().packs().iter().filter(|p| !p.id.ends_with("test")) {
        ids.extend(pack.examples.iter().map(|t| t.id.clone()));
    }
    ids
}

/// Every shipped document type.
fn types() -> Vec<String> {
    cat().packs().iter().filter(|p| !p.id.ends_with("test")).flat_map(|p| p.templates.iter().map(|t| t.id.clone())).collect()
}

#[test]
fn every_shipped_template_builds_with_registered_commands_and_no_error_issue() {
    let ids = shipped();
    for want in ["basic.blank", "flowchart.flowchart", "flowchart.decision", "flowchart.ciPipeline", "er.diagram", "uml.classDiagram", "uml.class"] {
        assert!(ids.iter().any(|i| i == want), "{want} is shipped");
    }
    let reg = crate::cmd::registry();
    for id in &ids {
        let (_, t) = lookup(id).unwrap();
        for step in t.seed.iter().filter(|st| !st.optional) {
            assert!(reg.get(&step.command).is_some(), "{id}: {} is registered", step.command);
        }
        let mut s = blank();
        let r = run(&mut s, "template.new", json!({"template": id}));
        assert_eq!(r["template"], json!(id));
        let issues = run(&mut s, "validate.run", json!({}));
        let errors: Vec<&Value> = issues["issues"].as_array().unwrap().iter().filter(|i| i["severity"] == "error").collect();
        assert!(errors.is_empty(), "{id}: {errors:?}");
        if !t.seed.is_empty() {
            assert!(r["primary"].is_string(), "{id}: the first element's shape is selected");
        }
    }
}

#[test]
fn decision_template_draws_and_lays_out_its_flow() {
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "flowchart.decision"}));
    let page = s.doc.page().unwrap();
    assert_eq!(page.shapes.len(), 6);
    assert_eq!(page.connectors.len(), 6);
    assert_eq!(page.view.as_ref().map(|v| v.viewpoint.as_str()), Some("flowchart.Flowchart"));
    assert_eq!(r["skipped"], json!([]));
    let primary = r["primary"].as_str().unwrap();
    assert_eq!(page.shape(primary).unwrap().text, "Start");
    assert_no_overlap(&s);
    let labels: Vec<&str> = page.connectors.iter().map(|c| c.text.as_str()).filter(|t| !t.is_empty()).collect();
    assert!(labels.contains(&"Yes") && labels.contains(&"No"), "{labels:?}");
}

fn assert_no_overlap(s: &Session) {
    let page = s.doc.page().unwrap();
    let boxes: Vec<_> = page.shapes.iter().filter(|sh| sh.parent.is_none()).map(|sh| (sh.id.clone(), sh.x, sh.y, sh.x + sh.w, sh.y + sh.h)).collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(i + 1) {
            let overlap = a.1 < b.3 && b.1 < a.3 && a.2 < b.4 && b.2 < a.4;
            assert!(!overlap, "{} overlaps {}", a.0, b.0);
        }
    }
}

#[test]
fn ci_pipeline_records_one_pattern_instance_and_nothing_overlaps() {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "flowchart.ciPipeline"}));
    assert_eq!(s.doc.model.patterns.len(), 1);
    assert_eq!(s.doc.model.patterns[0].pattern, "workflow.parallelSplit");
    assert_no_overlap(&s);
    let names: Vec<&str> = s.doc.model.elements.iter().map(|e| e.name.as_str()).collect();
    for want in ["Change pushed", "Build", "Unit tests", "Lint", "Integration tests", "Deploy", "Released"] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    // The source flows into the build and the deploy into the release.
    let flows = |from: &str, to: &str| {
        let id = |n: &str| s.doc.model.elements.iter().find(|e| e.name == n).unwrap().id.clone();
        let (f, t) = (id(from), id(to));
        s.doc.model.relationships.iter().any(|r| r.source == f && r.target == t)
    };
    assert!(flows("Change pushed", "Build") && flows("Deploy", "Released"));
}

#[test]
fn new_is_one_undo_step_back_to_a_blank_page() {
    let mut s = Session::new(crate::sample::sample_document());
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    run(&mut s, "template.new", json!({"template": "flowchart.decision"}));
    assert_eq!(s.undo_depth(), 1);
    assert_eq!(s.undo_label(), Some("New from Decision Flow"));
    assert!(s.path.is_none());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.pages.len(), 1);
    assert!(s.doc.pages[0].shapes.is_empty() && s.doc.model.elements.is_empty());
    assert!(!s.can_undo());
}

#[test]
fn a_failing_template_or_bad_params_leave_the_document_and_history_alone() {
    engtest();
    let mut s = Session::new(crate::sample::sample_document());
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let before = s.doc.clone();
    let depth = s.undo_depth();
    let e = s.run("template.new", &json!({"template": "engtest.broken"})).unwrap_err();
    assert!(e.to_string().contains("relationship.create failed"), "{e}");
    for bad in [
        json!({"template": "flowchart.decision", "variant": 5}),
        json!({"template": "flowchart.decision", "sheet": "A3"}),
        json!({"template": "flowchart.decision", "variant": "jic"}),
        json!({"template": "flowchart.decision", "path": {}}),
        json!({"template": "nope.nope"}),
        json!({}),
    ] {
        assert!(s.run("template.new", &bad).is_err(), "{bad}");
        if bad.get("path").is_none() {
            assert!(s.run("template.addPage", &bad).is_err(), "{bad}");
        }
    }
    assert_eq!(s.doc, before);
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn hooks_run_when_registered_and_are_skipped_otherwise() {
    engtest();
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "engtest.schematic"}));
    // Every hook is a real command now (b6e-sheets, b6e-substrate, b6-topology).
    assert_eq!(r["applied"], json!(["sheet.set", "page.setModule", "sheet.titleBlock", "key.insert"]));
    assert_eq!(r["skipped"], json!(["engtest.later"]));
    assert_eq!(r["sheet"], "iso.a3.landscape");

    let mut s = with_hooks(&["key.insert", "sheet.titleBlock"]);
    let r = run(&mut s, "template.new", json!({"template": "engtest.schematic"}));
    assert_eq!(r["applied"], json!(["sheet.set", "page.setModule", "sheet.titleBlock", "key.insert"]));
    assert_eq!(r["skipped"], json!(["engtest.later"]));
    assert_eq!(s.doc.page().unwrap().name, "Page-1++", "both hooks ran once, in the real run");
    assert_eq!(s.undo_depth(), 1);

    // Faking sheet.set means no sheet is made, so the title block hook is faked with it.
    let mut s = with_hooks(&["sheet.set", "page.setModule", "sheet.titleBlock"]);
    let r = run(&mut s, "template.new", json!({"template": "engtest.schematic", "sheet": "asme.d.landscape"}));
    assert_eq!(r["applied"], json!(["sheet.set", "page.setModule", "sheet.titleBlock", "key.insert"]));
    assert_eq!(s.doc.standards.sheet, "asme.d.landscape");
    let txn = s.transactions().back().unwrap();
    assert!(txn.commands.iter().any(|c| c == "page.setModule"), "{:?}", txn.commands);
}

#[test]
fn the_variant_follows_the_choice_and_the_project_standards() {
    engtest();
    let variant = |s: &Session| s.doc.page().unwrap().view.as_ref().map(|v| v.variant.clone()).unwrap_or_default();
    let stencil = |s: &Session| s.doc.page().unwrap().shapes[0].stencil.clone();
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "engtest.plain"}));
    assert_eq!((r["variant"].as_str(), variant(&s).as_str()), (Some("engtest.iec"), "engtest.iec"), "IEC by default");
    assert_eq!(s.doc.standards.symbols, "iec");
    let r = run(&mut s, "template.new", json!({"template": "engtest.schematic", "variant": "engtest.jic"}));
    assert_eq!(r["variant"], "engtest.jic");
    assert_eq!(stencil(&s), "engtest.coilJic");
    assert_eq!((s.doc.standards.symbols.as_str(), s.doc.standards.sheet.as_str()), ("jic", "iso.a3.landscape"));
    // A new page takes the project's family and sheet.
    let r = run(&mut s, "template.addPage", json!({"template": "engtest.plain", "name": "Second"}));
    assert_eq!((r["variant"].as_str(), r["page"].as_str()), (Some("engtest.jic"), s.doc.page().map(|p| p.id.as_str())));
    assert_eq!(s.doc.page().unwrap().name, "Second");
    assert_eq!(stencil(&s), "engtest.coilJic");
    assert_eq!(s.undo_label(), Some("New Page from Plain"));
    run(&mut s, "template.standards", json!({"symbols": "iec", "sheet": "ansi.b"}));
    let r = run(&mut s, "template.addPage", json!({"template": "engtest.schematic"}));
    assert_eq!((r["variant"].as_str(), r["sheet"].as_str()), (Some("engtest.iec"), Some("asme.b.landscape")));
    // A family name works as the choice, and "" is the default drawing.
    let r = run(&mut s, "template.addPage", json!({"template": "engtest.plain", "variant": "jic"}));
    assert_eq!(r["variant"], "engtest.jic");
    let r = run(&mut s, "template.addPage", json!({"template": "engtest.plain", "variant": ""}));
    assert_eq!(r["variant"], "");
    // Foreground pages only: a page with a sheet also gets its generated frame page.
    let drawn = |s: &Session| s.doc.pages.iter().filter(|p| !p.is_background).count();
    assert_eq!(drawn(&s), 5);
    // Each added page was one step.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(drawn(&s), 4);
}

#[test]
fn the_same_template_can_be_added_twice() {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "flowchart.decision"}));
    run(&mut s, "template.addPage", json!({"template": "flowchart.decision"}));
    run(&mut s, "template.addPage", json!({"template": "flowchart.ciPipeline"}));
    assert_eq!(s.doc.pages.len(), 3);
    assert_eq!(s.doc.model.elements.len(), 6 + 6 + 7);
}

#[test]
fn standards_read_set_and_offer_choices() {
    let mut s = blank();
    let r = run(&mut s, "template.standards", json!({}));
    assert_eq!(r["symbols"], "iec");
    assert_eq!(r["sheet"], "iso.a3.landscape");
    assert_eq!(r["symbolChoices"][0], "iec");
    assert_eq!(r["sheetChoices"][0], "iso.a3.landscape", "ISO A3 first");
    assert!(r["sheetChoices"].as_array().unwrap().contains(&json!("asme.b.landscape")), "ASME offered too");
    assert!(!s.can_undo(), "reading is not a step");
    run(&mut s, "template.standards", json!({"symbols": "ansi", "sheet": "ansi.b.landscape"}));
    assert_eq!(s.undo_label(), Some("Project Standards"));
    assert_eq!((s.doc.standards.symbols.as_str(), s.doc.standards.sheet.as_str()), ("ansi", "asme.b.landscape"), "ansi. is read as asme.");
    run(&mut s, "template.standards", json!({"symbols": "", "sheet": ""}));
    assert_eq!(s.doc.standards, Standards::default());
    for bad in [json!({"symbols": "IEC 60617"}), json!({"sheet": "A4"}), json!({"symbols": 3})] {
        assert!(s.run("template.standards", &bad).is_err(), "{bad}");
    }
    // With sheet.presets registered, its list is offered and enforced.
    let mut s = with_hooks(&[]);
    let r = run(&mut s, "template.standards", json!({}));
    assert_eq!(r["sheetChoices"], json!(["iso.a3.landscape", "asme.d.landscape"]));
    assert!(s.run("template.standards", &json!({"sheet": "ansi.b.landscape"})).is_err());
    run(&mut s, "template.standards", json!({"sheet": "asme.d"}));
    assert_eq!(s.doc.standards.sheet, "asme.d.landscape");
}

#[test]
fn preview_returns_svg_with_the_seeded_labels_and_changes_nothing() {
    let mut s = Session::new(crate::sample::sample_document());
    let rev = s.rev();
    let before = s.doc.clone();
    let r = run(&mut s, "template.preview", json!({"template": "flowchart.decision"}));
    let svg = r["svg"].as_str().unwrap();
    assert!(svg.starts_with("<svg"), "{}", &svg[..svg.len().min(80)]);
    for label in ["Start", "Inputs complete?", "Done"] {
        assert!(svg.contains(label), "{label}");
    }
    assert!(r["width"].as_f64().unwrap() > 0.0 && r["height"].as_f64().unwrap() > 0.0);
    assert_eq!(s.rev(), rev);
    assert_eq!(s.doc, before);
    assert!(!s.can_undo());
}

#[test]
fn build_returns_the_document_template_new_draws() {
    let (_, t) = lookup("flowchart.decision").unwrap();
    let doc = build(t, &Choices::default()).unwrap();
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "flowchart.decision"}));
    assert_eq!(doc.pages[0].shapes.len(), s.doc.pages[0].shapes.len());
    assert_eq!(doc.model.elements, s.doc.model.elements);
    let mut other = t.clone();
    other.label = "Changed".into();
    assert!(build(&other, &Choices::default()).is_err());
}

#[test]
fn list_filters_by_audience_pack_and_query() {
    engtest();
    let mut s = blank();
    let ids = |r: &Value| r["templates"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let all = ids(&run(&mut s, "template.list", json!({})));
    assert!(all.starts_with(&["basic.blank".to_string()]), "catalog order: {all:?}");
    let flow = ids(&run(&mut s, "template.list", json!({"pack": "flowchart"})));
    assert_eq!(flow, ["flowchart.flowchart"], "document types only");
    let eng = ids(&run(&mut s, "template.list", json!({"audience": "Engineering"})));
    assert!(eng.contains(&"engtest.sheet".to_string()) && !eng.contains(&"basic.blank".to_string()), "{eng:?}");
    let examples = |r: &Value| r["examples"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let flow = examples(&run(&mut s, "example.list", json!({"pack": "flowchart"})));
    assert_eq!(flow, ["flowchart.decision", "flowchart.ciPipeline"]);
    let q = examples(&run(&mut s, "example.list", json!({"query": "DEPLOY", "pack": "flowchart"})));
    assert_eq!(q, ["flowchart.ciPipeline"]);
    let q = examples(&run(&mut s, "example.list", json!({"query": "branches on one question"})));
    assert_eq!(q, ["flowchart.decision"]);
    assert!(s.run("example.list", &json!({"query": "x".repeat(300)})).is_err());
    let r = run(&mut s, "template.list", json!({"pack": "engtest"}));
    let t = &r["templates"][0];
    assert_eq!(
        t["variants"],
        json!([{"id": "engtest.iec", "label": "IEC", "family": "iec"}, {"id": "engtest.jic", "label": "JIC", "family": "jic"}])
    );
    assert_eq!((t["module"].as_str(), t["titleBlock"].as_bool(), t["packName"].as_str()), (Some("iso2_5mm"), Some(true), Some("Engineering test")));
    assert_eq!((t["discipline"].as_str(), t["liveChecks"].as_bool()), (Some("engineering"), Some(false)));
    let er = &run(&mut s, "template.list", json!({"pack": "er"}))["templates"][0];
    let vps: Vec<&str> = er["viewpoints"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
    assert_eq!(vps, ["er.Physical", "er.Logical", "er.Conceptual"]);
    assert_eq!(er["viewpoints"][2]["rulesets"][0]["id"], "er.conceptual");
    assert_eq!(er["rulesets"], json!([{"id": "er.physical", "label": "Physical data model"}]));
    assert_eq!(er["discipline"], "data");
    assert!(s.run("template.list", &json!({"query": "x".repeat(300)})).is_err());
}

#[test]
fn the_layout_name_lists_match_the_layout_commands() {
    use vizcraft_notation::template::{ALGORITHMS, DIRECTIONS, PRIMITIVES};
    let reg = crate::cmd::registry();
    let doc = |id: &str| reg.get(id).unwrap().params;
    let prim = doc("layout.primitive");
    let listed = prim.split("\"primitive\": ").nth(1).unwrap().split(", ").next().unwrap();
    let names: Vec<&str> = listed.split('|').map(|n| n.trim_matches('"')).collect();
    assert_eq!(names, PRIMITIVES, "layout.primitive lists the same primitives");
    for p in PRIMITIVES {
        let parsed = serde_json::from_value::<vizcraft_diagram::autolayout::Primitive<String>>(json!({"primitive": p, "roles": {}}));
        assert!(parsed.as_ref().err().is_none_or(|e| !e.to_string().contains("unknown variant")), "{p}: {parsed:?}");
    }
    let full = doc("layout.full");
    let listed = full.split("\"algorithm\"?: ").nth(1).unwrap().split(" (").next().unwrap();
    let names: Vec<&str> = listed.split('|').map(|n| n.trim_matches('"')).collect();
    assert_eq!(names, ALGORITHMS);
    for d in DIRECTIONS {
        assert!(full.contains(&format!("\"{d}\"")), "{d}");
    }
    // And each one runs.
    for a in ALGORITHMS {
        let mut s = blank();
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
        run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
        run(&mut s, "layout.full", json!({"algorithm": a}));
    }
}

#[test]
fn template_new_saves_when_given_a_path_and_is_not_recorded() {
    let dir = std::env::temp_dir().join(format!("vizcraft-template-new-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("decision.vizcraft");
    let mut s = blank();
    s.record_start();
    let r = run(&mut s, "template.new", json!({"template": "flowchart.decision", "path": path.to_string_lossy()}));
    assert_eq!(r["path"], json!(path.to_string_lossy()));
    assert!(!s.dirty && s.path.as_deref() == Some(path.as_path()));
    run(&mut s, "template.addPage", json!({"template": "flowchart.flowchart"}));
    let script = s.record_stop().unwrap();
    assert_eq!(script.iter().map(|x| x["command"].as_str().unwrap()).collect::<Vec<_>>(), ["template.addPage"]);
    let mut t = blank();
    run(&mut t, "file.open", json!({"path": path.to_string_lossy()}));
    assert_eq!(t.doc.model.elements.len(), 6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_takes_the_project_standards_and_writes_them_in_the_one_step() {
    let mut s = Session::new(crate::sample::sample_document());
    let r = run(&mut s, "template.new", json!({"template": "flowchart.decision", "standards": {"symbols": "jic", "sheet": "ansi.b"}}));
    assert_eq!((s.doc.standards.symbols.as_str(), s.doc.standards.sheet.as_str()), ("jic", "asme.b.landscape"));
    assert_eq!(r["sheet"], "", "the template has no sheet, so the page has none");
    assert!(s.doc.page().unwrap().sheet.is_none());
    assert_eq!(s.undo_depth(), 1);

    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "network.physical", "standards": {"sheet": "asme.b"}}));
    assert_eq!(r["sheet"], "asme.b.landscape");
    let sheet = s.doc.pages.iter().find_map(|p| p.sheet).unwrap();
    assert_eq!(sheet.standard, vizcraft_diagram::sheet::SheetStandard::AsmeY14_1);

    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "er.schema", "standards": {"symbols": "iec"}}));
    assert_eq!(r["variant"], "er.ie", "ER has no IEC drawing, so the template's own notation");
    assert_eq!(s.doc.standards.symbols, "iec");
}

#[test]
fn new_without_standards_records_only_a_symbol_family() {
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "er.schema"}));
    assert_eq!(r["variant"], "er.ie");
    assert_eq!(s.doc.standards.symbols, "", "`ie` is a notation, not a symbol family");
    run(&mut s, "template.new", json!({"template": "er.conceptual"}));
    assert_eq!(s.doc.standards.symbols, "");
    engtest();
    run(&mut s, "template.new", json!({"template": "engtest.plain", "variant": "jic"}));
    assert_eq!(s.doc.standards.symbols, "jic");
}

#[test]
fn junk_standards_are_rejected_before_anything_changes() {
    let mut s = Session::new(crate::sample::sample_document());
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 0, "y": 0}));
    let (before, depth, rev) = (s.doc.clone(), s.undo_depth(), s.rev());
    for bad in [json!(5), json!({"symbols": "A B"}), json!({"sheet": "x"}), json!({"sheet": "iso.z9"}), json!({"symbols": 5, "sheet": []})] {
        let p = json!({"template": "flowchart.decision", "standards": bad});
        assert!(s.run("template.new", &p).is_err(), "{p}");
        assert!(s.run("template.preview", &p).is_err(), "{p}");
    }
    assert_eq!(s.doc, before);
    assert_eq!((s.undo_depth(), s.rev()), (depth, rev));
}

#[test]
fn preview_reports_its_choices_and_follows_the_standards() {
    let mut s = blank();
    let r = run(&mut s, "template.preview", json!({"template": "network.physical", "standards": {"sheet": "asme.c"}}));
    assert_eq!((r["variant"].as_str(), r["sheet"].as_str()), (Some(""), Some("asme.c.landscape")));
    let r = run(&mut s, "template.preview", json!({"template": "er.conceptual"}));
    assert_eq!(r["variant"], "er.chen");
}

#[test]
fn resolved_matches_what_template_new_draws_for_every_template() {
    for standards in [None, Some(Standards { symbols: "jic".into(), sheet: "asme.b.landscape".into() })] {
        for id in shipped() {
            let mut s = blank();
            let mut p = json!({"template": id});
            if let Some(st) = &standards {
                p["standards"] = json!({"symbols": st.symbols, "sheet": st.sheet});
            }
            let r = run(&mut s, "template.new", p);
            let (variant, sheet) = resolved(&id, &Choices::default(), standards.as_ref()).unwrap();
            assert_eq!((r["variant"].as_str(), r["sheet"].as_str()), (Some(variant.as_str()), Some(sheet.as_str())), "{id}");
        }
    }
    assert!(resolved("nope.nope", &Choices::default(), None).is_err());
}

#[test]
fn listing_is_what_template_list_returns() {
    let mut s = blank();
    for (audience, pack, query) in [(None, None, None), (Some("software"), None, Some(" er ")), (None, Some("flowchart"), Some("deploy"))] {
        let mut p = json!({});
        for (k, v) in [("audience", audience), ("pack", pack), ("query", query)] {
            if let Some(v) = v {
                p[k] = json!(v);
            }
        }
        let r = run(&mut s, "template.list", p);
        assert_eq!(r["templates"], json!(listing(audience, pack, query)));
    }
}

#[test]
fn every_document_type_is_empty_and_sets_up_its_viewpoint_and_rules() {
    let ids = types();
    assert!(ids.len() >= 16, "{ids:?}");
    for id in &ids {
        let (pack, t) = lookup(id).unwrap();
        assert!(t.seed.is_empty(), "{id}");
        let mut s = blank();
        let r = run(&mut s, "template.new", json!({"template": id}));
        assert!(s.doc.model.elements.is_empty() && s.doc.model.relationships.is_empty(), "{id} draws no content");
        let page = s.doc.page().unwrap();
        assert!(page.shapes.iter().all(|sh| sh.element.is_none()), "{id}: only setup shapes (frame, title block)");
        let vp = page.view.as_ref().map(|v| v.viewpoint.clone()).unwrap_or_default();
        assert_eq!(vp, t.viewpoint, "{id}");
        assert_eq!(r["viewpoint"], json!(t.viewpoint));
        let profile = vizcraft_notation::rules::profile(&s.doc, cat());
        for want in t.rulesets_for(&t.viewpoint) {
            assert!(profile.enabled && profile.rulesets.contains(want), "{id}: {want} in {:?}", profile.rulesets);
        }
        // The Blank Drawing sets nothing up, so there is nothing to undo.
        assert_eq!(s.undo_depth(), usize::from(id != "basic.blank"), "{id}");
        if id != "basic.blank" {
            assert_eq!(s.undo_label().map(str::to_string), Some(format!("New from {}", t.label)));
        }
        assert!(!t.discipline().is_empty() && pack.id != "drawing");
    }
}

#[test]
fn a_viewpoint_choice_brings_its_own_rules_and_a_bad_one_changes_nothing() {
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "er.diagram", "viewpoint": "er.Conceptual", "variant": "er.chen"}));
    assert_eq!((r["viewpoint"].as_str(), r["variant"].as_str()), (Some("er.Conceptual"), Some("er.chen")));
    assert_eq!(r["rulesets"], json!(["er.conceptual"]), "the conceptual rules only");
    let view = s.doc.page().unwrap().view.clone().unwrap();
    assert_eq!((view.viewpoint.as_str(), view.variant.as_str()), ("er.Conceptual", "er.chen"));
    let (before, depth) = (s.doc.clone(), s.undo_depth());
    for bad in [json!({"template": "er.diagram", "viewpoint": "uml.Sequence"}), json!({"template": "er.diagram", "viewpoint": 5})] {
        assert!(s.run("template.new", &bad).is_err(), "{bad}");
        assert!(s.run("template.addPage", &bad).is_err(), "{bad}");
        assert!(s.run("template.preview", &bad).is_err(), "{bad}");
    }
    assert_eq!((s.doc == before, s.undo_depth()), (true, depth));
    // The default viewpoint is the physical one, with its rules.
    let r = run(&mut s, "template.new", json!({"template": "er.diagram"}));
    assert_eq!((r["viewpoint"].as_str(), r["variant"].as_str()), (Some("er.Physical"), Some("er.ie")));
    assert_eq!(r["rulesets"], json!(["er.physical"]));
}

#[test]
fn a_page_of_another_type_adds_its_rules_and_live_checks_can_start_off() {
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "er.diagram"}));
    let r = run(&mut s, "template.addPage", json!({"template": "flowchart.flowchart"}));
    assert_eq!(r["rulesets"], json!(["er.physical", "flowchart.iso5807"]));
    assert_eq!(s.undo_depth(), 2);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(vizcraft_notation::rules::profile(&s.doc, cat()).rulesets, ["er.physical"]);
    engtest();
    let mut s = blank();
    let r = run(&mut s, "template.new", json!({"template": "engtest.sheet"}));
    assert_eq!(r["rulesets"], json!([]));
    assert!(!vizcraft_notation::rules::profile(&s.doc, cat()).enabled, "the type starts with checking off");
    // Checking the person switched off stays off when a page of a type with rules is added.
    let mut s = blank();
    run(&mut s, "template.new", json!({"template": "flowchart.flowchart"}));
    run(&mut s, "validate.setProfile", json!({"enabled": false}));
    run(&mut s, "template.addPage", json!({"template": "er.diagram"}));
    let p = vizcraft_notation::rules::profile(&s.doc, cat());
    assert!(!p.enabled && p.rulesets.contains(&"er.physical".to_string()), "{p:?}");
}

#[test]
fn examples_open_filled_in_and_types_do_not_open_as_examples() {
    let mut s = Session::new(crate::sample::sample_document());
    let r = run(&mut s, "example.open", json!({"example": "flowchart.decision", "standards": {"symbols": "jic"}}));
    assert_eq!(r["template"], "flowchart.decision");
    assert_eq!(s.doc.page().unwrap().shapes.len(), 6);
    assert_eq!((s.undo_depth(), s.undo_label()), (1, Some("New from Decision Flow")));
    assert_eq!(s.doc.standards.symbols, "jic");
    let (before, depth) = (s.doc.clone(), s.undo_depth());
    for bad in [json!({"example": "flowchart.flowchart"}), json!({"example": "nope.nope"}), json!({"template": "flowchart.decision"}), json!({})] {
        assert!(s.run("example.open", &bad).is_err(), "{bad}");
    }
    assert_eq!((s.doc == before, s.undo_depth()), (true, depth));
    // A script written before document types still opens an example through template.new.
    let mut t = blank();
    run(&mut t, "template.new", json!({"template": "flowchart.decision"}));
    assert_eq!(t.doc.model.elements, s.doc.model.elements);
}

#[test]
fn ids_are_unique_across_document_types_and_examples() {
    let mut seen = std::collections::HashSet::new();
    for id in shipped() {
        assert!(seen.insert(id.clone()), "{id} twice");
    }
}
