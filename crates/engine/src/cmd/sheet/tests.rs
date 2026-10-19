//! Sheet, document property and cross-reference command tests, with the frame goldens.

use serde_json::json;
use vizcraft_diagram::sheet::{Sheet, SheetSize, SheetStandard, sheet_geometry};
use vizcraft_diagram::text_fields::{FieldCtx, expand_shape};
use vizcraft_diagram::{Document, Orientation, decor};
use vizcraft_model::identity::{ComposeCtx, IdFormat};
use vizcraft_model::{Element, IdScope, Model, PropValue, Registry};
use vizcraft_notation::sheet_pass::EXT_XREF_FOR;

use crate::Session;
use crate::cmd::model::fixture::{blank, refused, run};

/// The generated page of page `i`.
fn generated(s: &Session, i: usize) -> &vizcraft_diagram::Page {
    let id = &s.doc.pages[i].id;
    &s.doc.pages[decor::generated_page(&s.doc, id).expect("a generated frame page")]
}

/// The expanded text of the generated shape whose stored text is `stored`, as drawn over page `i`.
fn shown(s: &Session, i: usize, stored: &str) -> String {
    let sh = generated(s, i).shapes.iter().find(|x| x.text == stored).unwrap_or_else(|| panic!("no slot {stored}"));
    expand_shape(sh, &FieldCtx::new(&s.doc, i, None)).text.clone()
}

/// A point in zone (`col`, `row`) indexes (0 at the left and top) of an A3 sheet.
fn in_zone(col: usize, row: usize) -> (f64, f64) {
    let g = sheet_geometry(&Sheet::default());
    ((g.col_edges[col] + g.col_edges[col + 1]) / 2.0, (g.row_edges[row] + g.row_edges[row + 1]) / 2.0)
}

#[test]
fn set_get_list_and_off() {
    let mut s = blank();
    let r = run(&mut s, "sheet.set", json!({}));
    assert_eq!(r["presetId"], "iso.a3.landscape");
    let p = &s.doc.pages[0];
    assert!((p.width - 420.0 * 100.0 / 25.4).abs() < 1e-9 && (p.height - 297.0 * 100.0 / 25.4).abs() < 1e-9);
    assert_eq!((p.board, p.units), (vizcraft_diagram::Board::Fixed, vizcraft_diagram::Units::Millimeters));
    let g = run(&mut s, "sheet.get", json!({}));
    assert_eq!(g["zones"].as_array().unwrap().len(), 48);
    assert_eq!(g["zones"][0]["zone"], "A1");
    let r = run(&mut s, "sheet.set", json!({"sheet": "asme.d.landscape", "number": 4}));
    assert_eq!((r["presetId"].as_str(), r["number"].as_u64()), (Some("asme.d.landscape"), Some(4)));
    assert_eq!((s.doc.pages[0].width, s.doc.pages[0].height), (3400.0, 2200.0));
    assert_eq!(s.doc.pages[0].units, vizcraft_diagram::Units::Inches);
    let z = run(&mut s, "sheet.zoneAt", json!({"x": 3300, "y": 2100}));
    assert_eq!(z["zone"], "A1", "ASME counts from the bottom right");
    assert_eq!(run(&mut s, "sheet.zoneAt", json!({"x": 1, "y": 1}))["zone"], json!(null));
    run(&mut s, "sheet.set", json!({"standard": "iso", "size": "a4", "orientation": "portrait", "zones": {"cols": 2}}));
    let sh = s.doc.pages[0].sheet.unwrap();
    assert_eq!((sh.size, sh.orientation, sh.zones.cols, sh.zones.rows), (SheetSize::IsoA4, Orientation::Portrait, 2, 6));
    let list = run(&mut s, "sheet.list", json!({}));
    assert_eq!(list["sheets"][0]["size"], "a4");
    let presets = run(&mut s, "sheet.presets", json!({}));
    assert_eq!(presets["presets"][0], "iso.a3.landscape");
    assert_eq!(presets["standards"][1]["sizes"][3]["zones"]["cols"], 8);
    run(&mut s, "sheet.titleBlock", json!({"kind": "none"}));
    assert!(!generated(&s, 0).shapes.iter().any(|x| x.text.contains("{{doc:")));
    run(&mut s, "sheet.titleBlock", json!({}));
    assert!(generated(&s, 0).shapes.iter().any(|x| x.text == "{{doc:title}}"));
    assert_eq!(run(&mut s, "sheet.frames", json!({"all": true}))["pages"], 1);
    run(&mut s, "sheet.set", json!({"off": true}));
    assert!(s.doc.pages[0].sheet.is_none());
    assert_eq!(s.doc.pages.len(), 1, "the frame page goes with the sheet");
    for bad in [
        json!({"sheet": "iso.a9.landscape"}),
        json!({"standard": "din"}),
        json!({"size": 4}),
        json!({"zones": {"cols": 0}}),
        json!({"zones": {"rows": 25}}),
        json!({"zones": 5}),
        json!({"number": -1}),
        json!({"number": 1.5}),
        json!({"titleBlock": "fancy"}),
        json!({"page": "nope"}),
    ] {
        refused(&mut s, "sheet.set", bad);
    }
    refused(&mut s, "sheet.titleBlock", json!({}));
}

#[test]
fn a_page_without_a_sheet_is_untouched() {
    let mut s = Session::new(crate::sample::sample_document());
    let before = s.doc.clone();
    run(&mut s, "sheet.list", json!({}));
    run(&mut s, "doc.properties", json!({}));
    run(&mut s, "xref.list", json!({}));
    assert_eq!(s.doc, before);
}

#[test]
fn the_title_block_follows_set_properties_and_refresh_is_idempotent() {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({}));
    assert_eq!(shown(&s, 0, "{{doc:title}}"), "");
    run(&mut s, "doc.setProperty", json!({"key": "title", "value": "Pump skid"}));
    run(&mut s, "doc.setProperty", json!({"key": "legalOwner", "value": "Works Ltd"}));
    run(&mut s, "doc.setProperties", json!({"author": "Ada"}));
    assert_eq!(shown(&s, 0, "{{doc:title}}"), "Pump skid");
    assert_eq!(shown(&s, 0, "{{doc:legalOwner}}"), "Works Ltd");
    assert_eq!(shown(&s, 0, "{{doc:creator}}"), "Ada", "the creator falls back to the author");
    assert_eq!(shown(&s, 0, "{{sheet}}/{{sheets}}"), "1/1");
    let settled = s.doc.clone();
    run(&mut s, "sheet.frames", json!({}));
    assert_eq!(s.doc, settled, "regenerating changes nothing");
    let r = run(&mut s, "doc.setProperty", json!({"key": "identificationNumber", "value": "DWG-0000-1111-22222"}));
    assert!(r["issue"].as_str().unwrap().contains("at most 16"), "{r}");
    assert_eq!(s.doc.properties.fields["identificationNumber"], "DWG-0000-1111-22222", "kept over the limit");
    let props = run(&mut s, "doc.properties", json!({}));
    let row = |k: &str| props["properties"].as_array().unwrap().iter().find(|p| p["key"] == k).cloned().unwrap();
    assert_eq!(row("approvalPerson")["issue"], "required");
    assert_eq!(row("title")["value"], "Pump skid");
    assert_eq!(row("xrefFormat")["group"], "Cross references");
    // The mandatory fields rule reports the empty ones.
    assert!(s.issues.iter().any(|i| i.rule == "sheet.mandatoryFields"));
    assert!(s.issues.iter().any(|i| i.rule == "sheet.fieldLength"));
    run(&mut s, "doc.setProperty", json!({"key": "legalOwner", "value": null}));
    assert!(!s.doc.properties.fields.contains_key("legalOwner"));
    refused(&mut s, "doc.setProperty", json!({"key": "nope", "value": "x"}));
    refused(&mut s, "doc.setProperty", json!({"key": "title", "value": 5}));
    refused(&mut s, "doc.setProperty", json!({"key": "documentType", "value": "poem"}));
    refused(&mut s, "doc.setProperty", json!({"key": "title"}));
}

#[test]
fn revise_letters_follow_the_asme_scheme() {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({"sheet": "asme.b.landscape"}));
    let revs: Vec<String> = (0..9)
        .map(|i| {
            run(&mut s, "sheet.revise", json!({"description": format!("change {i}"), "date": "2026-10-19"}))["rev"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(revs, ["A", "B", "C", "D", "E", "F", "G", "H", "J"], "I is skipped");
    assert_eq!(s.doc.properties.fields["revisionIndex"], "J");
    let mut y = Element::new("ey", "sheet.Revision");
    y.properties.insert("rev".into(), PropValue::Text("Y".into()));
    s.doc.model.elements.push(y);
    assert_eq!(run(&mut s, "sheet.revise", json!({"description": "after Y"}))["rev"], "AA");
    assert!(s.issues.iter().any(|i| i.rule == "sheet.revisionSequence"), "Y after J skips letters");
    refused(&mut s, "sheet.revise", json!({}));
}

#[test]
fn a_parts_list_fills_from_three_parts_and_a_balloon_shows_its_item() {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({}));
    for (i, (n, d)) in [("P-100", "Housing"), ("P-200", "Shaft"), ("P-300", "Seal")].iter().enumerate() {
        run(
            &mut s,
            "element.create",
            json!({"type": "sheet.Part", "id": format!("part{i}"), "x": 200 + 200 * i, "y": 300, "properties": {"partNumber": n, "description": d}}),
        );
    }
    let list = run(&mut s, "element.create", json!({"type": "sheet.PartsList", "id": "pl", "x": 600, "y": 700}));
    let shape = list["shape"].as_str().unwrap().to_string();
    let rows = s.doc.pages[0].shape(&shape).unwrap().compartments.iter().find(|c| c.id == "rows").unwrap().clone();
    assert_eq!(rows.rows, vec!["1\t1\tP-100\tHousing\t", "2\t1\tP-200\tShaft\t", "3\t1\tP-300\tSeal\t"]);
    assert_eq!(rows.columns.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["ITEM", "QTY", "PART NUMBER", "DESCRIPTION", "MATERIAL"]);
    let b = run(&mut s, "sheet.balloon", json!({"part": "part1"}));
    let bshape = b["shape"].as_str().unwrap();
    assert_eq!(s.doc.pages[0].shape(bshape).unwrap().text, "2");
    assert_eq!(s.doc.model.element(b["balloon"].as_str().unwrap()).unwrap().name, "2");
    let c = s.doc.pages[0].connector(b["connector"].as_str().unwrap()).unwrap();
    assert_eq!(c.end.shape(), s.doc.pages[0].shapes.iter().find(|x| x.element.as_deref() == Some("part1")).map(|x| x.id.as_str()));
    // Two parts have no balloon yet.
    assert_eq!(s.issues.iter().filter(|i| i.rule == "sheet.balloonRows").count(), 2);
    refused(&mut s, "sheet.balloon", json!({"part": "pl"}));
    refused(&mut s, "sheet.balloon", json!({"part": "nope"}));
}

#[test]
fn composed_tags_follow_the_owner_chain_and_per_owner_numbering() {
    let mut m = Model::new();
    let el =
        |id: &str, ty: &str, owner: Option<&str>, tag: &str| Element { owner: owner.map(str::to_string), tag: tag.into(), ..Element::new(id, ty) };
    m.elements.push(el("f1", "x.Function", None, "=F1"));
    m.elements.push(el("a2", "x.Location", Some("f1"), "=F1+A2"));
    m.elements.push(el("a3", "x.Location", Some("f1"), "=F1+A3"));
    let scopes = vec![IdScope {
        id: "x.device".into(),
        types: vec!["x.Device".into()],
        format: "{owner.tag}-K{n}".into(),
        auto_number: true,
        per_owner: true,
        ..IdScope::default()
    }];
    let k = el("", "x.Device", Some("a2"), "");
    assert_eq!(Registry::new(&m, &scopes).next_tag(&k).unwrap().as_deref(), Some("=F1+A2-K1"));
    m.elements.push(el("k1", "x.Device", Some("a2"), "=F1+A2-K1"));
    m.elements.push(el("k2", "x.Device", Some("a2"), "=F1+A2-K2"));
    assert_eq!(Registry::new(&m, &scopes).next_tag(&k).unwrap().as_deref(), Some("=F1+A2-K3"));
    let other = el("", "x.Device", Some("a3"), "");
    assert_eq!(Registry::new(&m, &scopes).next_tag(&other).unwrap().as_deref(), Some("=F1+A3-K1"), "each owner numbers from 1");
    m.elements.push(el("k9", "x.Device", Some("a3"), "=F1+A2-K1"));
    assert!(Registry::new(&m, &scopes).duplicates().is_empty(), "uniqueness counts within each owner");
    // The nearest ancestor of a type, and context parts.
    let f = IdFormat::parse("{owner:x.Function.tag}.{page}{rung:3}").unwrap();
    let ctx = ComposeCtx { page: "2".into(), zone: "B4".into(), rung: "3".into() };
    let k1 = m.element("k1").unwrap().clone();
    assert_eq!(f.compose_in(&m, &k1, 0, &ctx), "=F1.2003");
    let cr = IdFormat::parse("CR-{page}-{n}").unwrap();
    let ctx4 = ComposeCtx { page: "4".into(), ..ComposeCtx::default() };
    assert_eq!(cr.compose_in(&m, &k1, 25, &ctx4), "CR-4-25");
    // A moved element keeps its number: the loose match reads it whatever the page was.
    let mut moved = k1.clone();
    moved.tag = "CR-4-25".into();
    assert_eq!(cr.number_in(&moved.tag, &moved), Some(25));
    let ctx7 = ComposeCtx { page: "7".into(), ..ComposeCtx::default() };
    assert_eq!(vizcraft_notation::sheet_pass::recomposed(&m, &cr, &moved, &ctx7).as_deref(), Some("CR-7-25"));
    let z = IdFormat::parse("{zone}/{n:2}").unwrap();
    assert_eq!(z.number_in("AB12/07", &moved), Some(7));
    assert!(IdFormat::parse("{owner:.tag}").is_err());
    assert!(IdFormat::parse("{page:99}").is_err());
}

#[test]
fn cross_references_label_both_ends_and_follow_moves() {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({}));
    let p2 = run(&mut s, "page.add", json!({}))["page"].as_str().unwrap().to_string();
    run(&mut s, "sheet.set", json!({"page": p2}));
    // Contact on sheet 1 in B4, coil on sheet 2 in C3.
    let (x1, y1) = in_zone(3, 1);
    let (x2, y2) = in_zone(2, 2);
    run(&mut s, "page.setActive", json!({"page": 0}));
    let a = run(
        &mut s,
        "element.create",
        json!({"type": "flowchart.Process", "id": "contact", "name": "K1 contact", "x": x1 - 20.0, "y": y1 - 10.0, "w": 40, "h": 20, "page": "Page-1"}),
    );
    let b = run(
        &mut s,
        "element.create",
        json!({"type": "flowchart.Process", "id": "coil", "name": "K1 coil", "x": x2 - 20.0, "y": y2 - 10.0, "w": 40, "h": 20, "page": p2}),
    );
    run(&mut s, "relationship.create", json!({"from": "contact", "to": "coil", "type": "sheet.References"}));
    let label_of = |s: &Session, view: &str| {
        s.doc.pages.iter().flat_map(|p| &p.shapes).find(|x| x.ext.get(EXT_XREF_FOR).map(String::as_str) == Some(view)).cloned()
    };
    let (va, vb) = (a["shape"].as_str().unwrap().to_string(), b["shape"].as_str().unwrap().to_string());
    let la = label_of(&s, &va).expect("the contact is labelled");
    assert_eq!(la.text, "/2.C3");
    assert_eq!(la.parent.as_deref(), Some(va.as_str()));
    assert_eq!(label_of(&s, &vb).unwrap().text, "/1.B4");
    let list = run(&mut s, "xref.list", json!({}));
    assert_eq!(list["xrefs"].as_array().unwrap().len(), 2);
    assert_eq!(list["xrefs"][0]["to"]["zone"], "C3");
    // Moving the coil one zone right updates the contact's label; the coil's label moves with it.
    let before = label_of(&s, &vb).unwrap();
    run(&mut s, "page.setActive", json!({"page": p2}));
    run(&mut s, "shape.move", json!({"ids": [vb], "dx": 50.0 * 100.0 / 25.4, "dy": 0}));
    assert_eq!(label_of(&s, &va).unwrap().text, "/2.C4");
    assert!((label_of(&s, &vb).unwrap().x - before.x - 50.0 * 100.0 / 25.4).abs() < 1e-6, "a label moves with its view");
    // The label is selected on its own, not with its view.
    run(&mut s, "edit.select", json!({"ids": [before.id]}));
    assert_eq!(s.sel.items, vec![before.id.clone()]);
    // xref.go lands on the partner, without an undo step.
    let steps = s.undo_label().map(str::to_string);
    let r = run(&mut s, "xref.go", json!({"id": vb}));
    assert_eq!(r["element"], "contact");
    assert_eq!(s.doc.page().unwrap().id, s.doc.pages[0].id);
    assert_eq!(s.sel.items, vec![va.clone()]);
    assert_eq!(s.undo_label().map(str::to_string), steps);
    // A deleted label comes back unless the element asks for none.
    let la_id = label_of(&s, &va).unwrap().id;
    run(&mut s, "edit.select", json!({"ids": [la_id]}));
    run(&mut s, "edit.delete", json!({}));
    assert!(s.doc.pages[0].shape(&va).is_some(), "only the label was deleted");
    assert!(label_of(&s, &va).is_some(), "the label comes back");
    run(&mut s, "element.setProperty", json!({"id": "contact", "key": "xrefLabel", "value": "off"}));
    assert!(label_of(&s, &va).is_none());
    refused(&mut s, "xref.go", json!({"id": "nope"}));
}

#[test]
fn an_unmatched_off_sheet_reference_raises_an_issue() {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({}));
    run(&mut s, "element.create", json!({"type": "sheet.OffSheet", "id": "o1", "tag": "7", "x": 300, "y": 300}));
    assert!(s.issues.iter().any(|i| i.rule == "sheet.unmatchedOffSheet"), "{:?}", s.issues);
    run(&mut s, "element.create", json!({"type": "sheet.OffSheet", "id": "o2", "tag": "7", "x": 900, "y": 300}));
    assert!(!s.issues.iter().any(|i| i.rule == "sheet.unmatchedOffSheet"));
    // Paired partners label each other on the same sheet with the zone alone.
    let labels: Vec<String> = s.doc.pages[0].shapes.iter().filter(|x| x.ext.contains_key(EXT_XREF_FOR)).map(|x| x.text.clone()).collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().all(|t| !t.starts_with('/')), "{labels:?}");
}

#[test]
fn page_setup_takes_a_paper_size() {
    let mut s = blank();
    let r = run(&mut s, "page.setup", json!({"size": "ANSI B", "board": "fixed"}));
    assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(1700.0), Some(1100.0)));
    let r = run(&mut s, "page.setup", json!({"size": "a4", "orientation": "portrait"}));
    assert!((r["height"].as_f64().unwrap() - 297.0 * 100.0 / 25.4).abs() < 1e-9);
    refused(&mut s, "page.setup", json!({"size": "A9"}));
    refused(&mut s, "page.setup", json!({"size": "A4", "width": 100}));
}

/// A frame golden: the SVG of a sheet page with its title block, compared with the file under
/// `goldens/`; `VIZCRAFT_BLESS=1` rewrites it. The PDF drawn from it has the sheet's paper size.
fn golden(name: &str, preset: &str, media: &str) {
    let mut s = blank();
    run(&mut s, "sheet.set", json!({"sheet": preset}));
    run(&mut s, "doc.setProperty", json!({"key": "title", "value": "Golden"}));
    let svg = String::from_utf8(vizcraft_formats::svg::export_page(&s.doc, 0).unwrap()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cmd/sheet/goldens").join(name);
    if std::env::var("VIZCRAFT_BLESS").is_ok() {
        std::fs::write(&path, &svg).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(svg == want, "{name} differs from its golden; run with VIZCRAFT_BLESS=1 after checking it");
    assert!(svg.contains(">Golden</tspan>"), "the title block shows the title");
    let pdf = vizcraft_formats::pdf::export_pages(&s.doc, &[0], &vizcraft_formats::svg::SvgOptions::default()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains(media), "{name}: no {media} in the PDF");
}

#[test]
fn golden_iso_a3_frame() {
    golden("iso-a3.svg", "iso.a3.landscape", "1190.55");
    let a3 = Sheet::default();
    assert_eq!((a3.standard, a3.size), (SheetStandard::Iso5457, SheetSize::IsoA3));
}

#[test]
fn golden_asme_d_frame() {
    golden("asme-d.svg", "asme.d.landscape", "2448");
}

#[test]
fn a_new_document_from_before_sheets_is_unchanged_by_the_pass() {
    let doc = Document::new();
    let mut s = Session::new(doc.clone());
    run(&mut s, "shape.add", json!({"stencil": "basic.rectangle", "x": 10, "y": 10}));
    assert!(s.doc.pages.iter().all(|p| p.sheet.is_none()));
    assert_eq!(s.doc.pages.len(), doc.pages.len());
}
