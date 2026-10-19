//! Tests of the architecture mapping: read, write and read again keep ids, types, groups, sides
//! and arrows; hostile text is an error, never a panic; the written text matches its golden.

use vizcraft_diagram::{Document, End, Page};

use super::{ICON_EXT, write};
use crate::csv::TYPE_KEY;
use crate::mermaid::import;

const SRC: &str = "architecture-beta
    group edge(cloud)[Edge]
    group net(cloud)[Network] in edge
    service web(internet)[Web]
    service db(database)[Orders] in net
    service app(server)[App] in net
    service files(disk)[Files] in net
    service q(logos:queue-thing)[Jobs]
    junction j in net
    web:R --> L:app
    app:B -- T:db
    app:R <--> L:files
    files:B -- T:j
    q{group}:L <-- R:db{group}
";

fn page_of(text: &str) -> (Page, Vec<String>) {
    let mut i = import(text).unwrap();
    (i.pages.remove(0), i.warnings)
}

fn doc(page: Page) -> Document {
    let mut d = Document::new();
    d.pages = vec![page];
    d
}

/// Ids, types, parents, and per connector its glued shapes and point indexes plus arrows.
fn summary(p: &Page) -> (Vec<(String, String, Option<String>)>, Vec<String>) {
    let mut shapes: Vec<_> = p.shapes.iter().map(|s| (s.id.clone(), s.ext.get(TYPE_KEY).cloned().unwrap_or_default(), s.parent.clone())).collect();
    shapes.sort();
    let end = |e: &End| match e {
        End::Glued { shape, point, .. } => format!("{shape}@{point:?}"),
        End::Free { .. } => "free".into(),
    };
    let mut lines: Vec<String> =
        p.connectors.iter().map(|c| format!("{} {:?} {:?} {}", end(&c.start), c.start_arrow, c.end_arrow, end(&c.end))).collect();
    lines.sort();
    (shapes, lines)
}

#[test]
fn read_maps_icons_groups_and_sides() {
    let (p, warnings) = page_of(SRC);
    let get = |id: &str| p.shapes.iter().find(|s| s.id == id).unwrap();
    assert_eq!(get("db").ext.get(TYPE_KEY).map(String::as_str), Some("cloud.Database"));
    assert_eq!(get("files").stencil, "cloud.bucket");
    assert_eq!(get("j").stencil, "cloud.junction");
    assert_eq!(get("edge").ext.get(TYPE_KEY).map(String::as_str), Some("cloud.Cloud"));
    assert!(get("net").container.is_some(), "a group is a container shape");
    assert_eq!(get("db").parent.as_deref(), Some("net"));
    assert_eq!(get("net").parent.as_deref(), Some("edge"));
    assert_eq!(get("q").ext.get(ICON_EXT).map(String::as_str), Some("logos:queue-thing"));
    assert!(warnings.iter().any(|w| w.contains("logos:queue-thing")), "{warnings:?}");
    let web_app = p.connectors.iter().find(|c| c.start.shape() == Some("web")).unwrap();
    assert!(matches!(&web_app.start, End::Glued { point: Some(1), .. }), "R is the right point");
    assert!(matches!(&web_app.end, End::Glued { point: Some(3), .. }), "L is the left point");
    let grouped = p.connectors.iter().find(|c| c.end.shape() == Some("net")).unwrap();
    assert_eq!(grouped.start.shape(), Some("q"), "q has no group, so its own shape");
}

#[test]
fn read_write_read_keeps_ids_types_groups_sides_and_arrows() {
    let (p, _) = page_of(SRC);
    let (text, warnings) = write(&doc(p.clone()), 0).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (again, _) = page_of(&text);
    assert_eq!(summary(&again), summary(&p), "{text}");
    let (text2, _) = write(&doc(again), 0).unwrap();
    assert_eq!(text2, text, "writing is stable");
    golden("round_trip.mmd", &text);
}

#[test]
fn types_the_icons_cannot_carry_come_back_from_comments() {
    let src = "architecture-beta\n    %% type: lb cloud.LoadBalancer\n    %% type: g cloud.Region\n    group g(cloud)[Region]\n    service lb(server)[Front] in g\n";
    let (p, _) = page_of(src);
    let get = |id: &str| p.shapes.iter().find(|s| s.id == id).unwrap();
    assert_eq!(get("lb").ext.get(TYPE_KEY).map(String::as_str), Some("cloud.LoadBalancer"));
    assert_eq!(get("g").ext.get(TYPE_KEY).map(String::as_str), Some("cloud.Region"));
    let (text, _) = write(&doc(p), 0).unwrap();
    assert!(text.contains("%% type: lb cloud.LoadBalancer") && text.contains("%% type: g cloud.Region"), "{text}");
    golden("comments.mmd", &text);
}

#[test]
fn labels_and_steps_the_grammar_cannot_hold_are_warnings() {
    let (mut p, _) = page_of("architecture-beta\n    service a(server)[A]\n    service b(database)[B]\n    a:R --> L:b\n");
    p.connectors[0].text = "query".into();
    p.connectors[0].step = 2;
    let (_, warnings) = write(&doc(p), 0).unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(write(&Document::new(), 3).is_err());
}

#[test]
fn hostile_text_is_an_error_not_a_panic() {
    let many: String = (0..100_000).map(|i| format!("group g{i}(cloud)[G]\n")).collect();
    assert!(import(&format!("architecture-beta\n{many}")).is_err());
    let deep: String = (0..100).map(|i| format!("group g{i}(cloud)[G] in g{}\n", i + 1)).collect();
    assert!(import(&format!("architecture-beta\n{deep}group g100(cloud)[G]\n")).is_err());
    assert!(import("architecture-beta\ngroup a(cloud)[A] in b\ngroup b(cloud)[B] in a\n").is_err());
    for junk in ["architecture-beta\nservice", "architecture-beta\nservice (x)[y]\n", "architecture-beta\na:R -- \n", "architecture-beta\n:: -- ::\n"]
    {
        let _ = import(junk);
    }
}

/// Compare `text` with `goldens/<name>`; `VIZCRAFT_BLESS=1` rewrites it.
fn golden(name: &str, text: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mermaid/architecture/goldens").join(name);
    if std::env::var("VIZCRAFT_BLESS").is_ok() {
        std::fs::write(&path, text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(text == want, "{name} differs from its golden; run with VIZCRAFT_BLESS=1 after checking it");
}
