use serde_json::json;
use vizcraft_diagram::{Arrow, Color, Connector, Document, End, Route};

use crate::{FormatError, native, svg};

/// A small flowchart built from the catalog: three shapes (one rotated, one with multi-line
/// text and markup characters), two glued connectors with different heads and one label.
pub(crate) fn sample() -> Document {
    let mut doc = Document::new();
    doc.title = "Formats test".into();
    doc.core.author = "Ada".into();
    let a = doc.next_id("s");
    let b = doc.next_id("s");
    let c = doc.next_id("s");
    let mut sa = vizcraft_stencils::new_shape("flowchart.terminator", a.clone(), 100.0, 40.0, None, None).unwrap();
    sa.text = "Start".into();
    let mut sb = vizcraft_stencils::new_shape("flowchart.decision", b.clone(), 100.0, 160.0, None, None).unwrap();
    sb.text = "x < 1 & \"ok\"\nsecond line".into();
    sb.style.bold = true;
    sb.style.italic = true;
    let mut sc = vizcraft_stencils::new_shape("basic.cylinder", c.clone(), 320.0, 160.0, None, None).unwrap();
    sc.angle_deg = 30.0;
    sc.text = "Store".into();
    sc.style.fill = Color { r: 10, g: 20, b: 30, a: 128 };
    let page = doc.page_mut().unwrap();
    page.shapes.extend([sa, sb, sc]);
    let c1 = doc.next_id("c");
    let c2 = doc.next_id("c");
    let mut k1 = Connector::new(c1, End::point(a, 2), End::point(b.clone(), 0));
    k1.route = Route::Straight;
    let mut k2 = Connector::new(c2, End::dynamic(b), End::dynamic(c));
    k2.start_arrow = Arrow::Diamond;
    k2.end_arrow = Arrow::Open;
    k2.text = "Yes".into();
    let page = doc.page_mut().unwrap();
    page.connectors.extend([k1, k2]);
    doc
}

#[test]
fn native_round_trip() {
    let doc = sample();
    let bytes = native::save(&doc).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("\"format\": \"vizcraft\""), "{text}");
    assert!(text.contains("\"version\": 2"));
    assert_eq!(native::load(&bytes).unwrap(), doc);
    // Saving the loaded document again gives the same bytes.
    assert_eq!(native::save(&native::load(&bytes).unwrap()).unwrap(), bytes);
}

#[test]
fn version_1_placeholder_loads_with_one_blank_page() {
    let v1 = json!({
        "format": "vizcraft",
        "version": 1,
        "document": {"title": "Old plans", "core": {"author": "Ada", "lastModifiedBy": "Bo", "created": "2026-01-01T00:00:00Z", "modified": "2026-01-02T00:00:00Z", "revision": 3}}
    });
    let doc = native::load(v1.to_string().as_bytes()).unwrap();
    assert_eq!(doc.title, "Old plans");
    assert_eq!(doc.core.author, "Ada");
    assert_eq!(doc.core.last_modified_by, "Bo");
    assert_eq!(doc.core.revision, 3);
    assert_eq!(doc.pages.len(), 1);
    assert!(doc.pages[0].shapes.is_empty() && doc.pages[0].connectors.is_empty());
}

#[test]
fn bare_document_loads() {
    let doc = sample();
    let bare = serde_json::to_vec(&doc).unwrap();
    assert_eq!(native::load(&bare).unwrap(), doc);
    assert_eq!(native::load(b"{}").unwrap().pages.len(), 1);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let good = native::save(&sample()).unwrap();
    for cut in [0, 1, 10, good.len() / 2, good.len() - 2] {
        assert!(native::load(&good[..cut]).is_err(), "truncated at {cut}");
    }
    for junk in [&b"\xff\xfe\x00garbage"[..], b"not json", b"[]", b"42", b"\"text\"", b"null", b"{\"format\": \"vizcraft\", \"version\": 2}"] {
        assert!(matches!(native::load(junk), Err(FormatError::Invalid(_))), "{:?}", String::from_utf8_lossy(junk));
    }
    let doc_not_object = json!({"format": "vizcraft", "version": 2, "document": [1, 2]});
    assert!(native::load(doc_not_object.to_string().as_bytes()).is_err());
    let bad_version = json!({"format": "vizcraft", "version": "two", "document": {}});
    assert!(native::load(bad_version.to_string().as_bytes()).is_err());
    let deep = "[".repeat(100_000);
    assert!(native::load(deep.as_bytes()).is_err());
    let big = vec![b' '; native::MAX_INPUT + 1];
    assert!(matches!(native::load(&big), Err(FormatError::TooLarge { .. })));
}

#[test]
fn foreign_tag_and_future_version_are_refused() {
    let other = json!({"format": "drawio", "version": 1, "document": {}});
    assert_eq!(native::load(other.to_string().as_bytes()), Err(FormatError::WrongFormat("drawio".into())));
    let not_string = json!({"format": 7, "document": {}});
    assert!(matches!(native::load(not_string.to_string().as_bytes()), Err(FormatError::WrongFormat(_))));
    let future = json!({"format": "vizcraft", "version": 3, "document": {}});
    let err = native::load(future.to_string().as_bytes()).unwrap_err();
    assert_eq!(err, FormatError::TooNew { found: 3, supported: 2 });
    assert!(err.to_string().contains("newer VizCraft"));
}

/// Minimal well-formedness check: every tag opened is closed in order, every `&` starts an
/// entity, and nothing follows the root element.
fn assert_well_formed(svg: &str) {
    let body = svg.strip_prefix("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n").unwrap();
    assert!(body.starts_with("<svg "));
    let mut stack: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find('<') {
        let text = &rest[..i];
        for (j, _) in text.match_indices('&') {
            let ent = &text[j..];
            assert!(["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"].iter().any(|e| ent.starts_with(e)), "bare & in {text:?}");
        }
        if !text.trim().is_empty() {
            assert!(!stack.is_empty(), "text outside the root: {text:?}");
        }
        let end = rest[i..].find('>').unwrap() + i;
        let tag = &rest[i + 1..end];
        assert!(!tag.contains('<'), "unescaped < in {tag:?}");
        assert_eq!(tag.matches('"').count() % 2, 0, "unbalanced quotes in {tag:?}");
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(stack.pop().as_deref(), Some(name));
            if stack.is_empty() {
                assert!(rest[end + 1..].trim().is_empty(), "content after the root");
            }
        } else if !tag.ends_with('/') {
            stack.push(tag.split_whitespace().next().unwrap().to_string());
        }
        rest = &rest[end + 1..];
    }
    assert!(stack.is_empty(), "unclosed: {stack:?}");
}

#[test]
fn svg_is_well_formed_and_complete() {
    let doc = sample();
    let bytes = svg::export_page(&doc, 0).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_well_formed(&text);
    let page = doc.page().unwrap();
    // One outline per shape, one detail path for the cylinder, and per connector a line plus a
    // path per head (k1: triangle end; k2: diamond start and open end).
    assert_eq!(text.matches("<path ").count(), 3 + 1 + (1 + 1) + (1 + 2));
    for s in &page.shapes {
        assert!(text.contains(&format!("<g id=\"{}\">", s.id)));
    }
    assert!(text.contains("width=\"1100\" height=\"850\" viewBox=\"0 0 1100 850\""), "the page fits, so the canvas is the page");
    assert!(text.contains("fill=\"#ffffff\""));
    assert!(text.contains("x &lt; 1 &amp; &quot;ok&quot;"));
    assert!(text.contains(">second line</tspan>"));
    assert!(text.contains("font-family=\"Inter, sans-serif\""));
    assert!(text.contains("font-weight=\"bold\"") && text.contains("font-style=\"italic\""));
    assert!(text.contains("transform=\"rotate(30 "), "rotated shape text turns with it");
    assert!(text.contains("fill-opacity=\"0.502\""));
    assert!(text.contains(">Yes</tspan>"));
    assert!(text.contains("<title>Formats test</title>"));
    assert!(!text.contains("NaN") && !text.contains("inf"));
}

#[test]
fn svg_grows_past_the_page_and_escapes_ids() {
    let mut doc = sample();
    let page = doc.page_mut().unwrap();
    page.shapes[0].x = -300.0;
    page.shapes[0].id = "a<&\"b".into();
    page.shapes[1].text = "\u{1}ctl\u{0}".into();
    let text = String::from_utf8(svg::export_page(&doc, 0).unwrap()).unwrap();
    assert_well_formed(&text);
    assert!(text.contains("viewBox=\"-320 "), "{}", &text[..300]);
    assert!(text.contains("id=\"a&lt;&amp;&quot;b\""));
    assert!(text.contains(">ctl</tspan>"));
}

#[test]
fn svg_of_a_missing_page_is_an_error() {
    assert_eq!(svg::export_page(&Document::new(), 3), Err(FormatError::NoPage(3)));
    let text = String::from_utf8(svg::export_page(&Document::new(), 0).unwrap()).unwrap();
    assert_well_formed(&text);
    assert_eq!(text.matches("<path ").count(), 0);
}

#[test]
fn numbers_are_compact_and_finite() {
    assert_eq!(svg::num(1.0), "1");
    assert_eq!(svg::num(0.1 + 0.2), "0.3");
    assert_eq!(svg::num(-0.0001), "0");
    assert_eq!(svg::num(f64::NAN), "0");
    assert_eq!(svg::num(f64::INFINITY), "0");
    assert_eq!(svg::num(-12.3456), "-12.346");
    assert_eq!(svg::num(1e7), "10000000");
}
