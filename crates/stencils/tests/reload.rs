//! A pack loaded at run time shows up in the sets and the search without a restart. Its own test
//! binary because it replaces the process-wide catalog.

use vizcraft_notation::{Glyph, Pack};

#[test]
fn a_loaded_pack_appears_in_sets_and_search() {
    let before = vizcraft_stencils::sets().len();
    assert!(vizcraft_stencils::search("zeppelin").is_empty());
    let glyph = Glyph {
        id: "airship.zeppelin".into(),
        label: "Zeppelin".into(),
        category: "Craft".into(),
        outline: "ellipse".into(),
        w: 120.0,
        h: 50.0,
        fill: "#ffffff".into(),
        line: "#000000".into(),
        keywords: vec!["blimp".into()],
        ..Glyph::default()
    };
    let pack = Pack { id: "airship".into(), version: "1.0.0".into(), name: "Airships".into(), glyphs: vec![glyph], ..Pack::default() };
    vizcraft_notation::catalog::add_pack(pack).unwrap();
    assert_eq!(vizcraft_stencils::sets().len(), before + 1);
    assert!(vizcraft_stencils::master("airship.zeppelin").is_some());
    assert_eq!(vizcraft_stencils::search("blimp").iter().map(|m| m.id).collect::<Vec<_>>(), ["airship.zeppelin"]);
}
