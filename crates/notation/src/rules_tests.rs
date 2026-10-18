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

// ---------------------------------------------------------------- conformance profile

use vizcraft_diagram::{Conformance, RuleOverride};

/// A catalog of the built-in packs plus one test pack written as JSON.
fn with_pack(v: serde_json::Value) -> Catalog {
    let pack: Pack = serde_json::from_value(v).unwrap();
    assert_eq!(crate::validate::check(&pack), Vec::<String>::new());
    let mut packs = Catalog::builtin().packs().to_vec();
    packs.push(pack);
    Catalog::new(packs)
}

fn set_profile(d: &mut Document, enabled: bool, rulesets: &[&str]) {
    d.conformance = Conformance { enabled, user_set: true, rulesets: rulesets.iter().map(|s| s.to_string()).collect(), ..Conformance::default() };
}

fn two_illegal_flows() -> Document {
    Doc::new().el("a", "Database", "A").el("b", "Database", "B").el("c", "Database", "C").flow("r1", "a", "b").flow("r2", "b", "c").build()
}

#[test]
fn a_profile_switched_off_runs_nothing() {
    let mut d = two_illegal_flows();
    assert_eq!(only(&live(&d, catalog()), "flowchart.illegalFlow").len(), 2);
    set_profile(&mut d, false, &[]);
    assert!(live(&d, catalog()).is_empty());
    assert!(batch(&d, catalog(), &Scope::All).is_empty());
    // Enabled with no rulesets: every rule of the packs in use, as before profiles existed.
    set_profile(&mut d, true, &[]);
    assert_eq!(only(&live(&d, catalog()), "flowchart.illegalFlow").len(), 2);
}

#[test]
fn rulesets_choose_the_rules() {
    let mut d = two_illegal_flows();
    set_profile(&mut d, true, &["drawing.tidy"]);
    assert!(live(&d, catalog()).iter().all(|i| i.rule.starts_with("drawing.")), "{:?}", live(&d, catalog()));
    set_profile(&mut d, true, &["flowchart.iso5807"]);
    assert_eq!(only(&live(&d, catalog()), "flowchart.illegalFlow").len(), 2);
    // A pack id stands for every rule of the pack; unknown ids are skipped.
    set_profile(&mut d, true, &["flowchart", "nope.nothing"]);
    assert_eq!(only(&live(&d, catalog()), "flowchart.illegalFlow").len(), 2);
    let p = d.conformance.clone();
    let sel: Vec<&str> = selected(&d, catalog(), &p).iter().map(|(_, r)| r.id.as_str()).collect();
    assert_eq!(sel.len(), 12);
}

#[test]
fn overrides_change_severity_or_switch_a_rule_off() {
    let mut d = two_illegal_flows();
    d.conformance.overrides.push(RuleOverride { rule: "flowchart.illegalFlow".into(), severity: Some(Severity::Info), disabled: false });
    let ill = live(&d, catalog());
    assert!(only(&ill, "flowchart.illegalFlow").iter().all(|i| i.severity == Severity::Info), "{ill:?}");
    d.conformance.overrides[0].disabled = true;
    assert!(only(&live(&d, catalog()), "flowchart.illegalFlow").is_empty());
}

#[test]
fn a_suppression_hides_exactly_one_issue() {
    let mut d = two_illegal_flows();
    let before = only(&live(&d, catalog()), "flowchart.illegalFlow").into_iter().cloned().collect::<Vec<_>>();
    assert_eq!(before.len(), 2);
    assert!(d.conformance.suppress(&before[0].key(), "the archive copy is intended"));
    let after = live(&d, catalog());
    let ill = only(&after, "flowchart.illegalFlow");
    assert_eq!(ill.len(), 1);
    assert_eq!(ill[0].key(), before[1].key());
}

#[test]
fn the_profile_follows_the_notation_until_the_user_sets_it() {
    let mut d = Document::new();
    d.pages[0].shapes.push(free_shape("s1", 0.0));
    assert!(!profile(&d, catalog()).enabled);
    auto_enable(&mut d, catalog());
    assert!(d.conformance.is_default(), "free drawing stays off");
    // A viewpoint on a page brings its pack's ruleset.
    d.pages[0].view = Some(View { viewpoint: "flowchart.Flowchart".into(), layout: String::new() });
    auto_enable(&mut d, catalog());
    assert!(d.conformance.enabled && !d.conformance.user_set);
    assert_eq!(d.conformance.rulesets, ["flowchart.iso5807"]);
    // Once set by the user, the sync leaves it alone.
    let mut d = Doc::new().el("p", "PredefinedProcess", "").build();
    set_profile(&mut d, false, &[]);
    auto_enable(&mut d, catalog());
    assert!(!d.conformance.enabled);
    assert!(live(&d, catalog()).is_empty());
}

#[test]
fn free_drawing_is_quiet_until_the_drawing_ruleset_is_enabled() {
    let mut d = Document::new();
    let page = &mut d.pages[0];
    page.shapes.push(free_shape("a", 0.0));
    page.shapes.push(free_shape("b", 200.0));
    page.shapes.push(free_shape("c", 400.0));
    let mut through = Connector::new("ac", End::point("a", 1), End::point("c", 3));
    through.route = vizcraft_diagram::Route::Straight;
    page.connectors.push(through);
    page.connectors.push(Connector::new("loose", End::dynamic("c"), End::free(Point::new(450.0, 200.0))));
    assert!(live(&d, catalog()).is_empty() && batch(&d, catalog(), &Scope::All).is_empty());
    set_profile(&mut d, true, &["drawing.tidy"]);
    let crossing = batch(&d, catalog(), &Scope::All);
    let c = only(&crossing, "drawing.connectorCrossesShape");
    assert_eq!(c.len(), 1, "{crossing:?}");
    assert_eq!((c[0].items.clone(), c[0].subjects.len(), c[0].page.as_deref()), (vec!["ac".to_string()], 0, Some("p1")));
    assert_eq!(c[0].fixes[0].command, "connector.reroute");
    assert_eq!(c[0].fixes[0].params, serde_json::json!({"ids": ["ac"], "avoid": true}));
    let lv = live(&d, catalog());
    let u = only(&lv, "drawing.ungluedEnd");
    assert_eq!(u.len(), 1, "{lv:?}");
    assert_eq!(u[0].items, ["loose"]);
    // The free end at (450, 200) is nearest the bottom point of c, index 2.
    let f = &u[0].fixes[0];
    assert_eq!(f.command, "connector.glue");
    assert_eq!(f.params, serde_json::json!({"id": "loose", "end": "end", "shape": "c", "point": 2}));
}

// ---------------------------------------------------------------- the reasoner in checks

fn hierarchy_pack() -> Catalog {
    with_pack(serde_json::json!({
        "id": "plant", "version": "1.0.0",
        "elementTypes": [
            {"id": "plant.Equipment", "properties": [{"key": "service", "required": true, "default": "water"}]},
            {"id": "plant.Pump", "extends": "plant.Equipment"},
            {"id": "plant.Pipe"}
        ],
        "relationshipTypes": [{"id": "plant.feeds"}, {"id": "plant.fedBy", "inverseOf": "plant.feeds"}],
        "connections": [{"source": ["plant.Equipment"], "relationship": "plant.feeds", "target": ["plant.Equipment"]}],
        "rules": [
            {"id": "plant.illegal", "severity": "error", "check": {"kind": "illegalRelationship"}},
            {"id": "plant.service", "check": {"kind": "requiredProperty", "types": ["plant.Equipment"]}},
            {"id": "plant.loop", "severity": "error", "mode": "batch", "check": {"kind": "forbiddenCycle", "relationship": "plant.feeds"}}
        ]
    }))
}

#[test]
fn subtypes_match_their_supertypes_lists() {
    let cat = hierarchy_pack();
    let mut b = Doc::new();
    b.el("p1", "plant.Pump", "P-101").el("p2", "plant.Pump", "P-102").el("x", "plant.Pipe", "L1");
    b.doc.model.add_relationship(Relationship::new("r1", "plant.feeds", "p1", "p2")).unwrap();
    b.doc.model.add_relationship(Relationship::new("r2", "plant.feeds", "p1", "x")).unwrap();
    let d = b.build();
    let issues = live(&d, &cat);
    // A Pump is an Equipment: Pump to Pump is legal, Pump to Pipe is not.
    let ill: Vec<&str> = only(&issues, "plant.illegal").iter().map(|i| i.subjects[0].as_str()).collect();
    assert_eq!(ill, ["r2"]);
    // The inherited required property is checked, and the fix sets its default.
    let req = only(&issues, "plant.service");
    assert_eq!(req.iter().map(|i| i.subjects[0].as_str()).collect::<Vec<_>>(), ["p1", "p2"]);
    let fix = &req[0].fixes[0];
    assert_eq!((fix.command.as_str(), fix.label.as_str()), ("element.setProperty", "Set service to \"water\""));
    assert_eq!(fix.params, serde_json::json!({"id": "p1", "key": "service", "value": "water"}));
}

#[test]
fn forbidden_cycle_follows_inverses() {
    let cat = hierarchy_pack();
    let mut b = Doc::new();
    b.el("a", "plant.Pump", "A").el("b", "plant.Pump", "B").el("c", "plant.Pump", "C").el("d", "plant.Pump", "D");
    b.doc.model.add_relationship(Relationship::new("r1", "plant.feeds", "a", "b")).unwrap();
    // A chain with no way back is not a cycle.
    b.doc.model.add_relationship(Relationship::new("r2", "plant.feeds", "c", "d")).unwrap();
    let d = b.build();
    assert!(only(&batch(&d, &cat, &Scope::All), "plant.loop").is_empty());
    let mut b = Doc::new();
    b.el("a", "plant.Pump", "A").el("b", "plant.Pump", "B").el("c", "plant.Pump", "C");
    b.doc.model.add_relationship(Relationship::new("r1", "plant.feeds", "a", "b")).unwrap();
    b.doc.model.add_relationship(Relationship::new("r2", "plant.feeds", "b", "c")).unwrap();
    // a fedBy c: c feeds a, which closes a, b, c.
    b.doc.model.add_relationship(Relationship::new("r3", "plant.fedBy", "a", "c")).unwrap();
    let d = b.build();
    let cyc = batch(&d, &cat, &Scope::All);
    let l = only(&cyc, "plant.loop");
    assert_eq!(l.len(), 1, "{cyc:?}");
    assert_eq!(l[0].subjects, ["a", "b", "c", "r1", "r2", "r3"]);
    assert!(l[0].message.contains("form a cycle of feeds relationships"), "{}", l[0].message);
    assert!(l[0].items.contains(&"s_a".to_string()));
}

/// A catalog with one pack `g` holding these rules over types `g.A`, `g.B` and `g.Box`.
fn graph_pack(rules: serde_json::Value) -> Catalog {
    with_pack(serde_json::json!({
        "id": "g", "version": "1.0.0",
        "elementTypes": [
            {"id": "g.A", "properties": [{"key": "lo", "kind": "number"}, {"key": "hi", "kind": "number"}, {"key": "code"}],
             "ports": [{"id": "i", "direction": "in"}, {"id": "o", "direction": "out"}, {"id": "io"}]},
            {"id": "g.B"},
            {"id": "g.Box", "container": true}
        ],
        "relationshipTypes": [{"id": "g.L"}],
        "rules": rules
    }))
}

fn model_doc(elements: &[(&str, &str)], rels: &[(&str, &str, &str)]) -> Document {
    let mut b = Doc::new();
    for (id, ty) in elements {
        b.el(id, ty, &id.to_uppercase());
    }
    for (id, s, t) in rels {
        b.doc.model.add_relationship(Relationship::new(*id, "g.L", *s, *t)).unwrap();
    }
    b.build()
}

#[test]
fn required_path_from_each_start_to_a_goal() {
    let cat =
        graph_pack(serde_json::json!([{"id": "g.path", "check": {"kind": "requiredPath", "from": ["g.A"], "to": ["g.B"], "relationship": "g.L"}}]));
    let d = model_doc(&[("a1", "g.A"), ("a2", "g.A"), ("a3", "g.A"), ("b", "g.B")], &[("r1", "a1", "a2"), ("r2", "a2", "b")]);
    let issues = live(&d, &cat);
    assert_eq!(issues.iter().map(|i| i.subjects[0].as_str()).collect::<Vec<_>>(), ["a3"]);
    assert!(issues[0].message.contains("no path of L relationships to a B"), "{}", issues[0].message);
}

#[test]
fn connected_reports_islands_but_the_largest() {
    let cat = graph_pack(serde_json::json!([{"id": "g.one", "check": {"kind": "connected", "types": [], "relationship": "g.L"}}]));
    let d =
        model_doc(&[("a", "g.A"), ("b", "g.B"), ("c", "g.A"), ("x", "g.A"), ("y", "g.B")], &[("r1", "a", "b"), ("r2", "c", "b"), ("r3", "y", "x")]);
    let issues = live(&d, &cat);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].subjects, ["x", "y"]);
    let d = model_doc(&[("a", "g.A"), ("b", "g.B")], &[("r1", "b", "a")]);
    assert!(live(&d, &cat).is_empty());
}

#[test]
fn containment_says_which_types_may_own_which() {
    let cat = graph_pack(serde_json::json!([{"id": "g.own", "check": {"kind": "containment", "owners": ["g.Box"], "children": ["g.A"]}}]));
    let mut d = model_doc(&[("box", "g.Box"), ("a", "g.A"), ("b", "g.B")], &[]);
    d.model.set_owner("a", Some("box")).unwrap();
    assert!(live(&d, &cat).is_empty());
    d.model.set_owner("b", Some("box")).unwrap();
    let issues = live(&d, &cat);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].subjects, ["b", "box"]);
    assert!(issues[0].message.contains("Box \"BOX\" may not contain B \"B\""), "{}", issues[0].message);
}

#[test]
fn port_direction_runs_out_to_in() {
    let cat = graph_pack(serde_json::json!([{"id": "g.dir", "severity": "error", "check": {"kind": "portDirection", "relationship": "g.L"}}]));
    let mut d = model_doc(&[("a", "g.A"), ("b", "g.A")], &[("ok", "a", "b"), ("bad", "a", "b"), ("ghost", "a", "b"), ("both", "a", "b")]);
    let ports = [("ok", "o", "i"), ("bad", "i", "o"), ("ghost", "o", "zz"), ("both", "io", "io")];
    for (id, s, t) in ports {
        let r = d.model.relationship_mut(id).unwrap();
        r.source_port = Some(s.into());
        r.target_port = Some(t.into());
    }
    let issues = live(&d, &cat);
    let ids: Vec<&str> = issues.iter().map(|i| i.subjects[0].as_str()).collect();
    assert_eq!(ids, ["bad", "ghost"], "{issues:?}");
    let bad = issues.iter().find(|i| i.subjects[0] == "bad").unwrap();
    assert!(bad.message.contains("starts at the input port") && bad.message.contains("ends at the output port"), "{}", bad.message);
}

#[test]
fn port_fan_in_limits_drivers() {
    let cat = graph_pack(serde_json::json!([{"id": "g.fan", "check": {"kind": "portFanIn", "relationship": "g.L", "max": 1}}]));
    let mut d = model_doc(&[("a", "g.A"), ("b", "g.A"), ("c", "g.A")], &[("r1", "a", "c"), ("r2", "b", "c"), ("r3", "a", "b")]);
    for id in ["r1", "r2", "r3"] {
        d.model.relationship_mut(id).unwrap().target_port = Some("i".into());
    }
    let issues = live(&d, &cat);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].subjects, ["c", "r1", "r2"]);
    assert!(issues[0].message.contains("2 drivers; at most 1"), "{}", issues[0].message);
}

#[test]
fn unique_property_within_an_owner() {
    let cat = graph_pack(
        serde_json::json!([{"id": "g.uniq", "check": {"kind": "uniqueProperty", "types": ["g.A"], "property": "code", "within": ["g.Box"]}}]),
    );
    let mut d = model_doc(&[("box1", "g.Box"), ("box2", "g.Box"), ("mid", "g.B"), ("a1", "g.A"), ("a2", "g.A"), ("a3", "g.A")], &[]);
    // a1 sits in box1 through mid; a2 in box1 directly; a3 in box2: only a1 and a2 clash.
    d.model.set_owner("mid", Some("box1")).unwrap();
    d.model.set_owner("a1", Some("mid")).unwrap();
    d.model.set_owner("a2", Some("box1")).unwrap();
    d.model.set_owner("a3", Some("box2")).unwrap();
    for id in ["a1", "a2", "a3"] {
        d.model.element_mut(id).unwrap().properties.insert("code".into(), text("K1"));
    }
    let issues = live(&d, &cat);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].subjects, ["a1", "a2"]);
    assert!(issues[0].message.contains("share code \"K1\" in Box \"BOX1\""), "{}", issues[0].message);
}

#[test]
fn property_compare_against_a_property_or_a_constant() {
    let cat = graph_pack(serde_json::json!([
        {"id": "g.range", "check": {"kind": "propertyCompare", "types": ["g.A"], "property": "hi", "op": "ge", "other": "lo"}},
        {"id": "g.cap", "check": {"kind": "propertyCompare", "types": ["g.A"], "property": "hi", "op": "lt", "other": 100}}
    ]));
    let mut d = model_doc(&[("ok", "g.A"), ("bad", "g.A"), ("big", "g.A"), ("none", "g.A")], &[]);
    let set = |d: &mut Document, id: &str, lo: f64, hi: f64| {
        let e = d.model.element_mut(id).unwrap();
        e.properties.insert("lo".into(), PropValue::Number(lo));
        e.properties.insert("hi".into(), PropValue::Number(hi));
    };
    set(&mut d, "ok", 1.0, 5.0);
    set(&mut d, "bad", 7.0, 5.0);
    set(&mut d, "big", 1.0, 500.0);
    let issues = live(&d, &cat);
    let found: Vec<(&str, &str)> = issues.iter().map(|i| (i.rule.as_str(), i.subjects[0].as_str())).collect();
    assert_eq!(found, [("g.cap", "big"), ("g.range", "bad")]);
    let bad = &only(&issues, "g.range")[0].message;
    assert!(bad.contains("hi is \"5\", which should be at least lo (7)"), "{bad}");
}

#[test]
fn name_format_matches_a_glob() {
    let cat = graph_pack(serde_json::json!([{"id": "g.name", "check": {"kind": "nameFormat", "types": ["g.A"], "pattern": "P-[0-9][0-9]*"}}]));
    let mut d = model_doc(&[("a", "g.A"), ("b", "g.A"), ("c", "g.A"), ("x", "g.B")], &[]);
    d.model.element_mut("a").unwrap().name = "P-101".into();
    d.model.element_mut("b").unwrap().name = "pump".into();
    d.model.element_mut("c").unwrap().name = "  ".into();
    let issues = live(&d, &cat);
    assert_eq!(issues.iter().map(|i| i.subjects[0].as_str()).collect::<Vec<_>>(), ["b"]);
    assert!(issues[0].message.contains("name pattern \"P-[0-9][0-9]*\""), "{}", issues[0].message);
}

// ---------------------------------------------------------------- drawing checks

/// A free-drawing document held to the drawing ruleset.
fn drawing_doc(shapes: Vec<vizcraft_diagram::Shape>, connectors: Vec<Connector>) -> Document {
    let mut d = Document::new();
    d.pages[0].shapes = shapes;
    d.pages[0].connectors = connectors;
    set_profile(&mut d, true, &["drawing.tidy"]);
    d
}

fn drawing_issues(d: &Document) -> Vec<Issue> {
    let mut all = live(d, catalog());
    all.extend(batch(d, catalog(), &Scope::All));
    sort(&mut all);
    all
}

#[test]
fn crosses_shape_names_the_connector() {
    let mut c = Connector::new("c1", End::point("a", 1), End::point("c", 3));
    c.route = vizcraft_diagram::Route::Straight;
    let d = drawing_doc(vec![free_shape("a", 0.0), free_shape("b", 200.0), free_shape("c", 400.0)], vec![c, glued("ok", "a", "b")]);
    let all = drawing_issues(&d);
    assert_eq!(rules(&all), ["drawing.connectorCrossesShape"], "{all:?}");
    assert!(all[0].message.contains("Connector c1 runs through a shape"), "{}", all[0].message);
}

#[test]
fn unglued_end_offers_glue_to_the_nearest_point() {
    let c = Connector::new("c1", End::free(Point::new(-30.0, 30.0)), End::free(Point::new(2000.0, 2000.0)));
    let d = drawing_doc(vec![free_shape("a", 0.0)], vec![c, glued("g", "a", "a")]);
    let u: Vec<Issue> = drawing_issues(&d).into_iter().filter(|i| i.rule == "drawing.ungluedEnd").collect();
    assert_eq!(u.len(), 1);
    assert!(u[0].message.contains("both ends free"), "{}", u[0].message);
    // Only the start is near a shape: its left point (0, 30), index 3.
    assert_eq!(u[0].fixes.len(), 1);
    assert_eq!(u[0].fixes[0].params, serde_json::json!({"id": "c1", "end": "start", "shape": "a", "point": 3}));
}

#[test]
fn overlap_skips_groups_and_containers() {
    let mut member = free_shape("m", 500.0);
    member.parent = Some("box".into());
    let mut boxed = vizcraft_diagram::container::new_container("box", 480.0, -20.0, 200.0, 120.0, "Box");
    boxed.parent = None;
    let touching = free_shape("t", 100.0);
    let d = drawing_doc(vec![free_shape("a", 0.0), free_shape("b", 50.0), touching, boxed, member], vec![]);
    let all = drawing_issues(&d);
    let o = only(&all, "drawing.overlap");
    // a (0..100) overlaps b (50..150), b overlaps t (100..200); the member sits in its container.
    let pairs: Vec<Vec<String>> = o.iter().map(|i| i.items.clone()).collect();
    assert_eq!(pairs, [vec!["a".to_string(), "b".to_string()], vec!["b".to_string(), "t".to_string()]], "{all:?}");
    let edge = drawing_doc(vec![free_shape("a", 0.0), free_shape("b", 100.0)], vec![]);
    assert!(only(&drawing_issues(&edge), "drawing.overlap").is_empty(), "edge to edge is not an overlap");
}

#[test]
fn off_board_and_text_overflow() {
    let mut long = free_shape("long", 300.0);
    long.text = "word ".repeat(80);
    let mut fits = free_shape("fits", 500.0);
    fits.text = "Short".into();
    let d = drawing_doc(vec![free_shape("off", -50.0), free_shape("gone", 5000.0), long, fits], vec![]);
    let all = drawing_issues(&d);
    let off = only(&all, "drawing.offBoard");
    assert_eq!(off.iter().map(|i| i.items[0].as_str()).collect::<Vec<_>>(), ["gone", "off"]);
    assert!(off.iter().any(|i| i.message.contains("is off the page")) && off.iter().any(|i| i.message.contains("runs over the page edge")));
    assert_eq!(off[0].fixes[0].command, "page.fitToDrawing");
    let t = only(&all, "drawing.textOverflow");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].items, ["long"]);
}
