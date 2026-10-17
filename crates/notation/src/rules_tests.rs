use vizcraft_diagram::{Document, End, Point};
use vizcraft_model::{Issue, PropValue, Relationship, Severity, View};

use super::test_doc::{Doc, free_shape, glued};
use super::*;
use crate::{Pack, catalog};

fn rules(issues: &[Issue]) -> Vec<&str> {
    issues.iter().map(|i| i.rule.as_str()).collect()
}

fn only<'a>(issues: &'a [Issue], rule: &str) -> Vec<&'a Issue> {
    issues.iter().filter(|i| i.rule == rule).collect()
}

fn text(s: &str) -> PropValue {
    PropValue::Text(s.into())
}

#[test]
fn free_drawing_has_no_issues() {
    let mut d = Document::new();
    d.pages[0].shapes.push(free_shape("s1", 0.0));
    d.pages[0].shapes.push(free_shape("s2", 200.0));
    d.pages[0].connectors.push(Connector::new("c1", End::dynamic("s1"), End::free(Point::new(5.0, 5.0))));
    assert!(live(&d, catalog()).is_empty());
    assert!(batch(&d, catalog(), &Scope::All).is_empty());
}

use vizcraft_diagram::Connector;

#[test]
fn database_to_database_flow_is_illegal() {
    let d = Doc::new().el("e1", "Database", "Orders").el("e2", "Database", "Archive").flow("r1", "e1", "e2").build();
    let issues = live(&d, catalog());
    let ill = only(&issues, "flowchart.illegalFlow");
    assert_eq!(ill.len(), 1, "{issues:?}");
    let i = ill[0];
    assert_eq!(i.severity, Severity::Error);
    assert_eq!(i.subjects, ["r1", "e1", "e2"]);
    assert_eq!(i.items, ["c_r1", "s_e1", "s_e2"]);
    assert_eq!(i.page.as_deref(), Some("p1"));
    assert!(i.message.contains("\"Orders\"") && i.message.contains("\"Archive\"") && i.message.contains("Database"), "{}", i.message);
    assert!(i.message.starts_with("Flows follow the connection table"), "{}", i.message);

    // Legal flows raise nothing.
    let d = Doc::new().el("e1", "Database", "Orders").el("e2", "Process", "Load").flow("r1", "e1", "e2").build();
    assert!(only(&live(&d, catalog()), "flowchart.illegalFlow").is_empty());
}

#[test]
fn unknown_relationship_and_element_types_are_illegal() {
    let mut b = Doc::new();
    b.el("e1", "Process", "A").el("e2", "Process", "B").el("e3", "flowchart.Ghost", "G");
    b.doc.model.add_relationship(Relationship::new("r1", "flowchart.Jump", "e1", "e2")).unwrap();
    b.rel("r2", "e1", "e3");
    let d = b.build();
    let ill: Vec<Vec<String>> = only(&live(&d, catalog()), "flowchart.illegalFlow").iter().map(|i| i.subjects.clone()).collect();
    assert_eq!(ill, [vec!["r1", "e1", "e2"], vec!["r2", "e1", "e3"]]);
}

#[test]
fn process_with_two_exits_trips_multiplicity() {
    let d = Doc::new()
        .el("e1", "Process", "Check")
        .el("e2", "Process", "A")
        .el("e3", "Decision", "B")
        .flow("r1", "e1", "e2")
        .flow("r2", "e1", "e3")
        .build();
    let issues = live(&d, catalog());
    let m = only(&issues, "flowchart.singleExit");
    assert_eq!(m.len(), 1, "{issues:?}");
    assert_eq!(m[0].severity, Severity::Warning);
    assert_eq!(m[0].subjects, ["e1", "r1", "r2"]);
    assert!(m[0].message.contains("2 outgoing") && m[0].message.contains("\"Check\""), "{}", m[0].message);

    // One step and one data symbol: the row counts targets it lists only.
    let d = Doc::new()
        .el("e1", "Process", "Check")
        .el("e2", "Process", "A")
        .el("e3", "Database", "B")
        .flow("r1", "e1", "e2")
        .flow("r2", "e1", "e3")
        .build();
    assert!(only(&live(&d, catalog()), "flowchart.singleExit").is_empty());
}

#[test]
fn in_max_is_checked_too() {
    let pack: Pack = serde_json::from_value(serde_json::json!({
        "id": "t", "version": "1.0.0",
        "elementTypes": [{"id": "t.A"}],
        "relationshipTypes": [{"id": "t.L"}],
        "connections": [{"source": ["*"], "relationship": "t.L", "target": ["*"], "inMax": 1}],
        "rules": [{"id": "t.mult", "severity": "error", "check": {"kind": "multiplicity"}}]
    }))
    .unwrap();
    let cat = Catalog::new(vec![pack]);
    let mut d = Document::new();
    for id in ["a", "b", "c"] {
        d.model.add_element(vizcraft_model::Element::new(id, "t.A")).unwrap();
    }
    d.model.add_relationship(Relationship::new("r1", "t.L", "a", "c")).unwrap();
    d.model.add_relationship(Relationship::new("r2", "t.L", "b", "c")).unwrap();
    let issues = live(&d, &cat);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].subjects, ["c", "r1", "r2"]);
    assert!(issues[0].message.contains("incoming"), "{}", issues[0].message);
    assert!(issues[0].items.is_empty() && issues[0].page.is_none());
}

#[test]
fn terminator_with_in_and_out_flows() {
    let d =
        Doc::new().el("t", "Terminator", "Middle").el("a", "Process", "A").el("b", "Process", "B").flow("r1", "a", "t").flow("r2", "t", "b").build();
    let issues = live(&d, catalog());
    let t = only(&issues, "flowchart.terminatorDirection");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].severity, Severity::Error);
    assert_eq!(t[0].subjects, ["t", "r1", "r2"]);
    let d = Doc::new().el("t", "Terminator", "Start").el("a", "Process", "A").flow("r1", "t", "a").build();
    assert!(only(&live(&d, catalog()), "flowchart.terminatorDirection").is_empty());
}

#[test]
fn duplicate_step_tag_is_one_issue_with_every_element() {
    let d = Doc::new().el("a", "Process", "A").el("b", "Decision", "B").el("c", "Process", "").tag("a", "S1").tag("b", "S1").tag("c", "S1").build();
    let issues = live(&d, catalog());
    let dup = only(&issues, "flowchart.duplicateStep");
    assert_eq!(dup.len(), 1, "{issues:?}");
    assert_eq!(dup[0].subjects, ["a", "b", "c"]);
    assert_eq!(dup[0].items, ["s_a", "s_b", "s_c"]);
    // Unnamed elements are named by tag.
    assert!(dup[0].message.contains("\"S1\"") && dup[0].message.contains("Process S1"), "{}", dup[0].message);
    let d = Doc::new().el("a", "Process", "A").el("b", "Process", "B").tag("a", "S1").tag("b", "S2").build();
    assert!(only(&live(&d, catalog()), "flowchart.duplicateStep").is_empty());
}

#[test]
fn predefined_process_needs_a_reference() {
    let d = Doc::new().el("p", "PredefinedProcess", "Call").build();
    let issues = live(&d, catalog());
    let m = only(&issues, "flowchart.missingReference");
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].severity, m[0].subjects.clone()), (Severity::Warning, vec!["p".to_string()]));
    assert!(m[0].message.contains("Reference"), "{}", m[0].message);
    let d = Doc::new().el("p", "PredefinedProcess", "Call").prop("p", "reference", text("  ")).build();
    assert_eq!(only(&live(&d, catalog()), "flowchart.missingReference").len(), 1);
    let d = Doc::new().el("p", "PredefinedProcess", "Call").prop("p", "reference", text("Billing")).build();
    assert!(only(&live(&d, catalog()), "flowchart.missingReference").is_empty());
}

#[test]
fn property_values_must_fit_their_schema() {
    let d = Doc::new().el("p", "Process", "P").prop("p", "owner", PropValue::Number(3.0)).prop("p", "automated", PropValue::Bool(true)).build();
    let issues = live(&d, catalog());
    let m = only(&issues, "flowchart.propertyValue");
    assert_eq!(m.len(), 1, "{issues:?}");
    assert!(m[0].message.contains("Owner should be text"), "{}", m[0].message);
    // Relationship properties are checked too (an empty type list covers relationship types).
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("b", "Process", "B").flow("r1", "a", "b");
    b.doc.model.relationship_mut("r1").unwrap().properties.insert("condition".into(), PropValue::Bool(true));
    assert_eq!(only(&live(&b.build(), catalog()), "flowchart.propertyValue")[0].subjects, ["r1"]);

    // Enums, integers and booleans in a pack of our own.
    let pack: Pack = serde_json::from_value(serde_json::json!({
        "id": "t", "version": "1.0.0",
        "elementTypes": [{"id": "t.A", "properties": [
            {"key": "mode", "kind": "enum", "values": ["open", "closed"]},
            {"key": "count", "kind": "integer"},
            {"key": "on", "kind": "bool"}
        ]}],
        "rules": [{"id": "t.types", "check": {"kind": "propertyType"}}]
    }))
    .unwrap();
    let cat = Catalog::new(vec![pack]);
    let mut d = Document::new();
    let mut e = vizcraft_model::Element::new("a", "t.A");
    e.properties.insert("mode".into(), text("ajar"));
    e.properties.insert("count".into(), PropValue::Number(2.5));
    e.properties.insert("on".into(), text("yes"));
    d.model.add_element(e).unwrap();
    let issues = live(&d, &cat);
    // One issue per item, naming every property that does not fit.
    assert_eq!(issues.len(), 1, "{issues:?}");
    let m = &issues[0].message;
    assert!(m.contains("mode is \"ajar\", which is not one of \"open\", \"closed\""), "{m}");
    assert!(m.contains("count should be a whole number") && m.contains("on should be true or false"), "{m}");
    let mut d = Document::new();
    let mut e = vizcraft_model::Element::new("a", "t.A");
    e.properties.insert("mode".into(), text("open"));
    e.properties.insert("count".into(), PropValue::Number(2.0));
    d.model.add_element(e).unwrap();
    assert!(live(&d, &cat).is_empty());
}

#[test]
fn viewpoint_allows_only_its_types() {
    let mut b = Doc::new();
    b.el("a", "Process", "A").el("x", "other.Pump", "Pump");
    b.doc.pages[0].view = Some(View { viewpoint: "flowchart.Flowchart".into(), layout: String::new() });
    let d = b.build();
    let issues = live(&d, catalog());
    let v = only(&issues, "flowchart.viewpoint");
    assert_eq!(v.len(), 1, "{issues:?}");
    assert_eq!((v[0].subjects.clone(), v[0].items.clone(), v[0].page.as_deref()), (vec!["x".to_string()], vec!["s_x".to_string()], Some("p1")));
    assert!(v[0].message.contains("a Flowchart page \"Page-1\""), "{}", v[0].message);
    // Without a viewpoint any type may be drawn.
    let mut d = d;
    d.pages[0].view = None;
    assert!(only(&live(&d, catalog()), "flowchart.viewpoint").is_empty());
}

#[test]
fn dangling_connector_is_a_batch_finding() {
    let mut b = Doc::new();
    b.el("a", "Process", "A");
    b.doc.pages[0].connectors.push(Connector::new("c1", End::dynamic("s_a"), End::free(Point::new(500.0, 0.0))));
    let mut c = Connector::new("c2", End::free(Point::new(0.0, 300.0)), End::free(Point::new(10.0, 300.0)));
    c.text = "loose".into();
    b.doc.pages[0].connectors.push(c);
    // A page with no flowchart elements is not checked.
    b.doc.pages.push(vizcraft_diagram::Page::new("p2", "Free"));
    b.doc.pages[1].connectors.push(Connector::new("c3", End::free(Point::new(0.0, 0.0)), End::free(Point::new(1.0, 1.0))));
    let d = b.build();
    assert!(only(&live(&d, catalog()), "flowchart.unconnectedConnector").is_empty());
    let issues = batch(&d, catalog(), &Scope::All);
    let dc = only(&issues, "flowchart.unconnectedConnector");
    let items: Vec<&str> = dc.iter().map(|i| i.items[0].as_str()).collect();
    assert_eq!(items, ["c1", "c2"]);
    assert!(dc.iter().all(|i| i.subjects.is_empty() && i.page.as_deref() == Some("p1")));
    assert!(dc[1].message.contains("\"loose\" has both ends free"), "{}", dc[1].message);
}

#[test]
fn decision_with_one_exit_is_dangling() {
    let d = Doc::new()
        .el("t", "Terminator", "Start")
        .el("d", "Decision", "Ok?")
        .el("a", "Process", "A")
        .flow("r1", "t", "d")
        .flow("r2", "d", "a")
        .build();
    assert!(only(&live(&d, catalog()), "flowchart.danglingDecision").is_empty());
    let issues = batch(&d, catalog(), &Scope::All);
    let dd = only(&issues, "flowchart.danglingDecision");
    assert_eq!(dd.len(), 1);
    assert_eq!(dd[0].subjects, ["d"]);
    assert!(dd[0].message.contains("1 outgoing Flow relationships; at least 2"), "{}", dd[0].message);
}

#[test]
fn off_page_references_pair_by_tag() {
    let d = Doc::new()
        .el("o1", "OffPageReference", "")
        .el("o2", "OffPageReference", "")
        .el("o3", "OffPageReference", "")
        .tag("o1", "1")
        .tag("o2", "1")
        .tag("o3", "2")
        .build();
    let issues = batch(&d, catalog(), &Scope::All);
    let u = only(&issues, "flowchart.unmatchedOffPage");
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].subjects, ["o3"]);
    assert!(u[0].message.contains("tag \"2\""), "{}", u[0].message);
    assert!(only(&issues, "flowchart.unmatchedOnPage").is_empty());
}

#[test]
fn unreachable_elements_from_a_start_terminator() {
    let d = Doc::new()
        .el("t", "Terminator", "Start")
        .el("a", "Process", "A")
        .el("b", "Process", "B")
        .el("x", "Process", "Island")
        .el("y", "Process", "Island 2")
        .flow("r1", "t", "a")
        .flow("r2", "a", "b")
        .flow("r3", "x", "y")
        .build();
    let issues = batch(&d, catalog(), &Scope::All);
    let u = only(&issues, "flowchart.unreachable");
    let subjects: Vec<&str> = u.iter().map(|i| i.subjects[0].as_str()).collect();
    assert_eq!(subjects, ["x", "y"]);
    assert!(u.iter().all(|i| i.severity == Severity::Info));
    assert!(u[0].message.contains("starting Terminator"), "{}", u[0].message);
}

#[test]
fn batch_scopes_filter_findings() {
    let mut b = Doc::new();
    b.el("t", "Terminator", "Start").el_on(1, "x", "Process", "Lost").el_on(1, "d", "Decision", "D");
    let d = b.build();
    let all = batch(&d, catalog(), &Scope::All);
    let subjects = |v: &[Issue]| -> Vec<String> { v.iter().map(|i| i.subjects.join(",")).collect() };
    assert!(subjects(&all).contains(&"x".to_string()));
    // Page 2 shows x and d only.
    let p1 = batch(&d, catalog(), &Scope::Page(1));
    assert!(!p1.is_empty() && p1.iter().all(|i| i.items.iter().any(|it| it == "s_x" || it == "s_d")), "{p1:?}");
    assert!(batch(&d, catalog(), &Scope::Page(0)).iter().all(|i| i.subjects.iter().all(|s| s == "t")));
    assert!(batch(&d, catalog(), &Scope::Page(9)).is_empty());
    // A shape id stands for its element; element ids work directly.
    let by_shape = batch(&d, catalog(), &Scope::Items(vec!["s_x".into()]));
    assert!(!by_shape.is_empty() && by_shape.iter().all(|i| i.subjects.contains(&"x".to_string())));
    assert_eq!(by_shape, batch(&d, catalog(), &Scope::Items(vec!["x".into()])));
    assert!(batch(&d, catalog(), &Scope::Items(vec!["nothing".into()])).is_empty());
    // A connector id stands for itself (a free connector has no relationship).
    let mut d2 = d.clone();
    d2.pages[1].connectors.push(Connector::new("cf", End::dynamic("s_x"), End::free(Point::new(0.0, 0.0))));
    let by_conn = batch(&d2, catalog(), &Scope::Items(vec!["cf".into()]));
    assert_eq!(rules(&by_conn), ["flowchart.unconnectedConnector"]);
}

#[test]
fn connector_ids_stand_for_their_relationship_in_scope() {
    let d = Doc::new().el("a", "Database", "A").el("b", "Database", "B").flow("r1", "a", "b").build();
    let all = live(&d, catalog());
    let scoped = in_scope(&d, &Scope::Items(vec!["c_r1".into()]), all.clone());
    assert_eq!(rules(&scoped), ["flowchart.illegalFlow"]);
}

#[test]
fn ordering_is_stable_and_by_severity_rule_and_subject() {
    let build = |reverse: bool| {
        let mut b = Doc::new();
        let mut steps =
            vec![("e1", "Database", "A"), ("e2", "Database", "B"), ("e3", "PredefinedProcess", "C"), ("e4", "Process", "D"), ("e5", "Process", "E")];
        if reverse {
            steps.reverse();
        }
        for (id, ty, name) in steps {
            b.el(id, ty, name);
        }
        b.tag("e4", "S1").tag("e5", "S1").flow("r1", "e1", "e2");
        b.build()
    };
    let a = live(&build(false), catalog());
    assert_eq!(a, live(&build(false), catalog()));
    let r = rules(&a);
    assert_eq!(r, ["flowchart.duplicateStep", "flowchart.illegalFlow", "flowchart.missingReference"]);
    let sev: Vec<Severity> = a.iter().map(|i| i.severity).collect();
    let mut sorted = sev.clone();
    sorted.sort();
    assert_eq!(sev, sorted);
    // Model order changes items but not which issues come in which order.
    assert_eq!(rules(&live(&build(true), catalog())), r);
}

#[test]
fn rules_run_for_listed_packs_and_types_in_use() {
    // A flowchart element alone pulls in the Flowchart rules.
    let d = Doc::new().el("p", "PredefinedProcess", "").build();
    assert_eq!(rules(&live(&d, catalog())), ["flowchart.missingReference"]);
    // Unknown packs are skipped without trouble.
    let mut d = Document::new();
    d.model.add_element(vizcraft_model::Element::new("z", "zzz.Thing")).unwrap();
    d.model.use_pack(vizcraft_model::PackRef { id: "nope".into(), version: "1.0.0".into() });
    assert!(live(&d, catalog()).is_empty());
    assert!(batch(&d, catalog(), &Scope::All).is_empty());
}

#[test]
fn messages_are_capped_and_on_one_line() {
    let long = format!("{}\nsecond line", "x".repeat(10_000));
    let d = Doc::new().el("a", "Database", &long).el("b", "Database", &"y".repeat(10_000)).flow("r1", "a", "b").build();
    let issues = live(&d, catalog());
    for i in &issues {
        assert!(i.message.len() <= MAX_MESSAGE, "{}", i.message.len());
        assert!(!i.message.contains('\n'));
    }
    assert_eq!(cut("\u{e9}\u{e9}\u{e9}\u{e9}", 6), "\u{e9}...");
}

#[test]
fn connectors_drawn_on_two_pages_leave_page_unset() {
    let mut b = Doc::new();
    b.el("a", "Database", "A").el("b", "Database", "B").flow("r1", "a", "b");
    b.doc.pages.push(vizcraft_diagram::Page::new("p2", "Two"));
    let mut c = glued("c9", "s_a", "s_b");
    c.relationship = Some("r1".into());
    b.doc.pages[1].connectors.push(c);
    let i = &only(&live(&b.build(), catalog()), "flowchart.illegalFlow")[0].clone();
    assert_eq!(i.items, ["c_r1", "c9", "s_a", "s_b"]);
    assert_eq!(i.page, None);
}

#[test]
fn large_models_check_quickly() {
    // A long chain of steps with a duplicate tag at the end: linear indexes keep this fast.
    let mut b = Doc::new();
    let n = 5000;
    for i in 0..n {
        b.el(&format!("e{i}"), "Process", "");
        b.tag(&format!("e{i}"), &format!("S{i}"));
    }
    for i in 1..n {
        b.flow(&format!("r{i}"), &format!("e{}", i - 1), &format!("e{i}"));
    }
    b.tag("e1", "S0");
    let d = b.build();
    let t = std::time::Instant::now();
    let issues = live(&d, catalog());
    let all = batch(&d, catalog(), &Scope::All);
    assert!(t.elapsed().as_secs() < 10, "{:?}", t.elapsed());
    assert_eq!(rules(&issues), ["flowchart.duplicateStep"]);
    // Every step is unreachable (there is no terminator) and nothing else is wrong.
    assert_eq!(all.len(), n);
}
