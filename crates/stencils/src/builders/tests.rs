//! Builder tests: golden SVGs, ports on the module grid, determinism, hostile parameters,
//! provenance, and shipped packs agreeing with their builders' port ids.

use std::collections::BTreeMap;

use vizcraft_diagram::vizcraft_model::PropValue;

use super::*;

/// Parameters from `key=value` pairs: numbers and flags as such, the rest as text.
fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, PropValue> {
    pairs
        .iter()
        .map(|(k, v)| {
            let value = match *v {
                "true" => PropValue::Bool(true),
                "false" => PropValue::Bool(false),
                t => t.parse::<f64>().map(PropValue::Number).unwrap_or_else(|_| PropValue::Text(t.to_string())),
            };
            ((*k).to_string(), value)
        })
        .collect()
}

type Case = (&'static str, &'static str, &'static [(&'static str, &'static str)]);

const CASES: &[Case] = &[
    ("contact-iec-no", "contact", &[]),
    ("contact-iec-nc-3pole-ondelay", "contact", &[("form", "nc"), ("poles", "3"), ("timing", "onDelay"), ("tag", "K1")]),
    ("contact-jic-no", "contact", &[("standard", "jic")]),
    ("contact-jic-nc-2pole-offdelay", "contact", &[("standard", "jic"), ("form", "nc"), ("poles", "2"), ("timing", "offDelay")]),
    ("contact-ieee315-no", "contact", &[("standard", "ieee315"), ("tag", "CR1")]),
    ("coil-iec-relay", "coil", &[("tag", "K1")]),
    ("coil-iec-timer", "coil", &[("kind", "timer")]),
    ("coil-jic-contactor", "coil", &[("standard", "jic"), ("kind", "contactor"), ("tag", "M1")]),
    ("bubble-field", "bubble", &[]),
    ("bubble-shared-panel", "bubble", &[("system", "sharedDisplay"), ("location", "panelFront"), ("letters", "TIC"), ("number", "101")]),
    ("bubble-computer-rear", "bubble", &[("system", "computer"), ("location", "panelRear"), ("letters", "FY")]),
    ("bubble-safety-local", "bubble", &[("system", "safety"), ("location", "localFront"), ("letters", "PSHH"), ("number", "2001-A")]),
    ("valve-4-2", "valve", &[]),
    ("valve-4-3-spring", "valve", &[("positions", "3"), ("actuatorRight", "solenoid"), ("return", "spring")]),
    ("valve-5-2-pneumatic", "valve", &[("ports", "5"), ("medium", "pneumatic"), ("actuatorLeft", "pilot"), ("portLabels", "iso5599")]),
    ("valve-3-2-lever-detent", "valve", &[("ports", "3"), ("actuatorLeft", "lever"), ("return", "detent")]),
    ("cylinder-double", "cylinder", &[]),
    ("cylinder-single-spring-pneumatic", "cylinder", &[("acting", "single"), ("springReturn", "true"), ("medium", "pneumatic")]),
    ("cylinder-through-cushions", "cylinder", &[("rod", "through"), ("cushion", "both")]),
    ("pump-fixed", "pump", &[]),
    ("pump-motor-bi-variable-drain", "pump", &[("role", "motor"), ("flow", "bi"), ("variable", "true"), ("drain", "true")]),
    ("pump-pneumatic", "pump", &[("medium", "pneumatic")]),
    ("gate-and-rectangular", "gate", &[]),
    ("gate-nand-distinctive-3", "gate", &[("function", "nand"), ("inputs", "3"), ("shapes", "distinctive")]),
    ("gate-xor-distinctive-inverted", "gate", &[("function", "xor"), ("invert", "10"), ("shapes", "distinctive")]),
    ("gate-not-rectangular", "gate", &[("function", "not"), ("inputs", "1")]),
    ("gate-or-distinctive-6", "gate", &[("function", "or"), ("inputs", "6"), ("shapes", "distinctive")]),
    ("fcf-position", "fcf", &[("diameter", "true"), ("tolerance", "0.05"), ("modifier", "mmc"), ("datums", "A|B|C")]),
    ("fcf-flatness", "fcf", &[("characteristic", "flatness"), ("tolerance", "0.02")]),
    ("fcf-runout-asme", "fcf", &[("characteristic", "totalRunout"), ("datums", "A-B"), ("standard", "asmeY14_5")]),
    ("weld-iso-fillet", "weld", &[("otherSide", "fillet"), ("size", "a5"), ("length", "100")]),
    (
        "weld-aws-v",
        "weld",
        &[("standard", "aws"), ("arrowSide", "vButt"), ("otherSide", "squareButt"), ("allAround", "true"), ("field", "true"), ("tail", "GTAW")],
    ),
    ("weld-iso-spot", "weld", &[("arrowSide", "spot"), ("size", "5")]),
    ("texture-iso-required", "texture", &[("removal", "required"), ("value", "Ra 1.6"), ("process", "ground")]),
    ("texture-asme-prohibited", "texture", &[("standard", "asmeY14_36"), ("removal", "prohibited"), ("value", "63")]),
    ("texture-iso-lay", "texture", &[("lay", "perpendicular"), ("value", "Rz 6.3")]),
];

#[test]
fn golden_svgs() {
    let dir = format!("{}/testdata/builders", env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("VIZCRAFT_UPDATE_GOLDEN").is_some();
    if update {
        std::fs::create_dir_all(&dir).unwrap();
    }
    for (name, builder, pairs) in CASES {
        let b = build(builder, &params(pairs), None).unwrap();
        assert!(b.warnings.is_empty(), "{name}: {:?}", b.warnings);
        let got = svg(&b);
        let path = format!("{dir}/{name}.svg");
        if update {
            std::fs::write(&path, &got).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is missing (run with VIZCRAFT_UPDATE_GOLDEN=1)"));
        assert_eq!(got, want, "{name} changed; refresh the golden files if that was intended");
    }
    for spec in specs() {
        assert!(CASES.iter().filter(|c| c.1 == spec.name).count() >= 3, "{} needs three golden cases", spec.name);
    }
}

/// Every value of every choice parameter that picks a standard, one build per value.
fn standard_builds(spec: &BuilderSpec) -> Vec<(String, Built)> {
    let mut out = vec![("defaults".to_string(), build(spec.name, &BTreeMap::new(), None).unwrap())];
    for p in spec.params.iter().filter(|p| matches!(p.key, "standard" | "shapes" | "medium" | "form" | "kind" | "system" | "role")) {
        if let ParamKind::Choice(names) = p.kind {
            for n in names {
                let raw = params(&[(p.key, n)]);
                out.push((format!("{}={n}", p.key), build(spec.name, &raw, None).unwrap()));
            }
        }
    }
    out
}

#[test]
fn ports_sit_on_whole_modules_inside_the_frame() {
    for spec in specs() {
        for (what, b) in standard_builds(spec) {
            assert!(!b.ports.is_empty(), "{} {what}: no ports", spec.name);
            assert!(b.w.fract() == 0.0 && b.h.fract() == 0.0, "{} {what}: frame {} by {}", spec.name, b.w, b.h);
            for p in &b.ports {
                assert!(p.x.fract() == 0.0 && p.y.fract() == 0.0, "{} {what}: port {} at {}, {}", spec.name, p.id, p.x, p.y);
                assert!((0.0..=b.w).contains(&p.x) && (0.0..=b.h).contains(&p.y), "{} {what}: port {} outside", spec.name, p.id);
            }
            let mut ids: Vec<&str> = b.ports.iter().map(|p| p.id.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), b.ports.len(), "{} {what}: port ids repeat", spec.name);
            assert!(b.module > 0.0);
        }
    }
    // The module follows the standard, and a given module wins.
    let iec = build("contact", &BTreeMap::new(), None).unwrap();
    let jic = build("contact", &params(&[("standard", "jic")]), None).unwrap();
    assert_eq!((iec.module, jic.module), (MODULE_METRIC, MODULE_INCH));
    assert!((MODULE_METRIC - 9.8425).abs() < 1e-4);
    assert_eq!(build("contact", &BTreeMap::new(), Some(20.0)).unwrap().module, 20.0);
    let junk = build("contact", &BTreeMap::new(), Some(f64::NAN)).unwrap();
    assert_eq!((junk.module, junk.warnings.len()), (MODULE_METRIC, 1));
}

#[test]
fn builds_are_byte_identical_twice() {
    for (name, builder, pairs) in CASES {
        let a = svg(&build(builder, &params(pairs), None).unwrap());
        let b = svg(&build(builder, &params(pairs), None).unwrap());
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn contact_ports_follow_poles_and_form() {
    let ids = |pairs: &[(&str, &str)]| build("contact", &params(pairs), None).unwrap().ports.into_iter().map(|p| p.id).collect::<Vec<_>>();
    assert_eq!(ids(&[]), ["13", "14"]);
    assert_eq!(ids(&[("poles", "3")]), ["13", "14", "23", "24", "33", "34"]);
    assert_eq!(ids(&[("form", "nc"), ("standard", "jic")]), ["11", "12"]);
    let b = build("valve", &params(&[("ports", "5"), ("portLabels", "iso5599")]), None).unwrap();
    assert_eq!(b.ports.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["1", "2", "4", "3", "5"]);
    // Bad values are clamped or defaulted with a warning, never an error.
    let b = build("contact", &params(&[("poles", "99"), ("form", "sideways"), ("ghost", "1")]), None).unwrap();
    assert_eq!((b.ports.len(), b.warnings.len()), (12, 3), "{:?}", b.warnings);
    let v = build("valve", &params(&[("paths", "P-Q; Z; P|"), ("rest", "2")]), None).unwrap();
    assert!(v.warnings.len() >= 3, "{:?}", v.warnings);
    assert_eq!(build("nope", &BTreeMap::new(), None), Err(BuildError::Unknown("nope".into())));
}

/// A small deterministic generator, so the junk loop is the same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}

#[test]
fn two_thousand_junk_parameter_maps_never_panic() {
    let mut rng = Lcg(7);
    let texts = ["", "x", "nc", "jic", "P-A B-T; X; P-B A-T", "P|; X", "--;;--", "A|B|C|D|E", "1111111111111111", "\u{0}\u{202e}", "Ø≥1"];
    let numbers = [0.0, -1.0, 1.0, 3.0, 6.5, 1e308, -1e308, f64::NAN, f64::INFINITY, 0.5];
    for i in 0..2000 {
        let spec = rng.pick(specs());
        let mut raw = BTreeMap::new();
        for _ in 0..(rng.next() % 8) {
            let key = if rng.next().is_multiple_of(5) { "junk".to_string() } else { rng.pick(spec.params).key.to_string() };
            let value = match rng.next() % 4 {
                0 => PropValue::Text(rng.pick(&texts).to_string()),
                1 => PropValue::Number(*rng.pick(&numbers)),
                2 => PropValue::Bool(rng.next().is_multiple_of(2)),
                _ => PropValue::Text("long".repeat((rng.next() % 600) as usize)),
            };
            raw.insert(key, value);
        }
        let module = match rng.next() % 4 {
            0 => None,
            1 => Some(*rng.pick(&numbers)),
            _ => Some(MODULE_INCH),
        };
        let b = build(spec.name, &raw, module).unwrap_or_else(|e| panic!("{i}: {e}"));
        assert!(b.fills.els.len() + b.lines.els.len() <= MAX_ELS);
        assert!(b.module.is_finite() && b.module > 0.0 && b.w >= 1.0 && b.h >= 1.0, "{i}");
        let _ = svg(&b);
    }
    for name in ["", "\u{0}", "x".repeat(10_000).as_str()] {
        assert!(build(name, &BTreeMap::new(), None).is_err());
    }
}

#[test]
fn the_command_cap_cuts_with_a_warning() {
    let mut d = Draw::default();
    for i in 0..3000 {
        d.line(&[(0.0, i as f64), (1.0, i as f64)]);
    }
    let b = d.finish();
    assert!(b.lines.els.len() <= MAX_ELS);
    assert_eq!(b.warnings.len(), 1);
}

/// Words a provenance note may not contain (it may not claim, or deny, a tracing or a vendor).
pub const FORBIDDEN: &[&str] = &["traced", "copied", "visio", "microsoft", "autocad", "autodesk", "adobe", "avid"];

/// Whether `source` is an acceptable provenance note.
fn good_source(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    (lower.starts_with("own design") || lower.starts_with("drawn from")) && !FORBIDDEN.iter().any(|w| lower.contains(w))
}

#[test]
fn every_builder_states_its_provenance_in_its_module_docs() {
    let files: &[(&str, &str)] = &[
        ("contact", include_str!("contact.rs")),
        ("coil", include_str!("coil.rs")),
        ("bubble", include_str!("bubble.rs")),
        ("valve", include_str!("valve.rs")),
        ("cylinder", include_str!("cylinder.rs")),
        ("pump", include_str!("pump.rs")),
        ("gate", include_str!("gate.rs")),
        ("fcf", include_str!("fcf.rs")),
        ("weld", include_str!("weld.rs")),
        ("texture", include_str!("texture.rs")),
    ];
    assert_eq!(files.len(), specs().len());
    for spec in specs() {
        let text = files.iter().find(|(n, _)| *n == spec.name).map(|(_, t)| *t).unwrap();
        let docs: Vec<&str> = text.lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim()).collect();
        let docs = docs.join(" ");
        assert!(!spec.sources.is_empty(), "{}", spec.name);
        for (standard, source) in spec.sources {
            assert!(good_source(source), "{}: {standard}: {source}", spec.name);
            assert!(docs.contains(&format!("Provenance: {source}")), "{}: the module docs must state `Provenance: {source}`", spec.name);
        }
    }
    assert!(good_source("drawn from the written rules of ISO 1219-1"));
    assert!(!good_source("drawn from a Visio stencil"));
    assert!(!good_source("traced from a figure"));
    assert!(!good_source(""));
}

#[test]
fn shipped_builder_glyphs_name_known_builders_and_their_types_declare_the_builders_port_ids() {
    let cat = vizcraft_notation::Catalog::builtin();
    assert!(cat.errors().is_empty(), "{:?}", cat.errors());
    for pack in cat.packs() {
        for g in pack.glyphs.iter().filter(|g| g.outline == vizcraft_notation::loader::BUILDER_OUTLINE) {
            let b = build(&g.builder, &g.params, None).unwrap_or_else(|e| panic!("{}: {e}", g.id));
            let Some(t) = pack.element_type(&g.element_type) else { continue };
            if t.ports.is_empty() {
                continue;
            }
            let want: Vec<&str> = t.ports.iter().map(|p| p.id.as_str()).collect();
            let got: Vec<&str> = b.ports.iter().map(|p| p.id.as_str()).collect();
            assert_eq!(got, want, "{}: the type must declare the builder's port ids in order", g.id);
        }
    }
}
