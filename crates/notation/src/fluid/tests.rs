//! Tests of the circuit model: the path grammar, the pack loading, and each check over the
//! case files in `packs/tests/fluidpower/`.

use super::paths::{self, Token};

#[test]
fn the_path_grammar_reads_flows_blocks_and_x() {
    assert_eq!(paths::tokens("P-A B| X junk"), [Token::Flow("P", "A"), Token::Block("B"), Token::All, Token::Bad("junk")]);
    let set = paths::port_set(4, false);
    let f = paths::flows(&set, paths::default_paths(4, 3), 3);
    assert_eq!(f, vec![vec![(0, 1), (2, 3)], vec![], vec![(0, 2), (1, 3)]]);
    // T and R name the same exhaust below five ports; numbers work too; a self loop is dropped.
    assert_eq!(paths::flows(&set, "1-2 R-4; P-P", 2), vec![vec![(0, 1), (3, 2)], vec![]]);
    assert_eq!(paths::groups("a;b;;c").len(), 4);
    assert_eq!(paths::effective(" ", 5, 2), "P-A B-S; P-B A-R");
    assert!(paths::tokens(&"P-A ".repeat(100)).len() <= paths::MAX_TOKENS);
}

#[test]
fn pilot_ports_follow_the_port_labels_and_the_square() {
    let set = paths::port_set(5, true);
    let f = paths::flows(&set, paths::default_paths(5, 2), 2);
    assert_eq!(paths::pilot_id(&set, &f[0], true, true), "12");
    assert_eq!(paths::pilot_id(&set, &f[1], true, false), "14");
    assert_eq!(paths::pilot_id(&set, &[], true, false), "10");
    assert_eq!(paths::pilot_id(&set, &f[0], false, true), "X");
    assert_eq!(paths::pilot_id(&set, &f[1], false, false), "Y");
    assert!(paths::is_pilot("14") && paths::is_pilot("X") && !paths::is_pilot("1") && !paths::is_pilot("P"));
    assert!(paths::is_drain("L") && !paths::is_fluid("shaft") && paths::is_fluid("P"));
}

#[test]
fn the_rest_position_follows_the_spring() {
    assert_eq!(paths::expected_rest(2, "spring", "solenoid", "none"), Some(1));
    assert_eq!(paths::expected_rest(2, "spring", "none", "lever"), Some(0));
    assert_eq!(paths::expected_rest(3, "spring", "solenoid", "solenoid"), Some(1));
    assert_eq!(paths::expected_rest(2, "detent", "lever", "none"), None);
}

/// The document of a case file in `packs/tests/fluidpower/`.
fn case_doc(name: &str) -> vizcraft_diagram::Document {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/tests/fluidpower").join(name);
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    crate::cases::document(&v["document"]).unwrap()
}

#[test]
fn the_circuit_graph_joins_ports_through_lines_and_parts() {
    use super::{Circuit, Positions, Role, Walk};
    let cat = crate::catalog();
    let doc = case_doc("clean_circuit.json");
    let c = Circuit::build(&doc, cat, None);
    let role = |r: Role| c.parts.iter().filter(|p| p.role == r).count();
    assert_eq!((role(Role::Valve), role(Role::Pump), role(Role::Cylinder), role(Role::Tank)), (1, 1, 1, 1));
    let pump = c.parts.iter().position(|p| p.role == Role::Pump).unwrap();
    let cyl = c.parts.iter().position(|p| p.role == Role::Cylinder).unwrap();
    let out = c.outlets(cat, pump);
    assert_eq!(out.len(), 1);
    assert!(c.trace(&out, &Walk::pressure(Positions::Any)).parts.contains(&cyl));
    // At rest the spring-centred valve blocks P: the pressure stops at the valve and the relief.
    let rest = c.trace(&out, &Walk::pressure(Positions::Rest));
    assert!(!rest.parts.contains(&cyl));
    // A one-way path is not walked backwards: from the reservoir nothing reaches the pressure line.
    let tank = c.parts.iter().position(|p| p.role == Role::Tank).unwrap();
    let t1 = c.parts[tank].port(&c, "T1").unwrap();
    let back = c.trace(&[t1], &Walk { directed: true, ..Walk::lines() });
    assert!(!back.parts.contains(&cyl));
    // Junk start ports are ignored.
    assert!(c.trace(&[usize::MAX], &Walk::lines()).ports.is_empty());
}
