//! Round-trip, page editing, style, label and CSV tests over hand-written fixtures.

use std::io::Write;

use base64::Engine;
use vizcraft_drawio::*;

/// Two inline pages: shapes with HTML labels, a UserObject cell, an edge with waypoints and a
/// collapsed container with alternate bounds.
const MULTI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="Electron" version="24.1.0" agent="test-agent" type="device">
  <diagram id="page-a" name="Overview">
    <mxGraphModel dx="1200" dy="800" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0" background="#FFFFFF" customThing="kept">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
        <mxCell id="a" value="Start &lt;b&gt;here&lt;/b&gt;" style="ellipse;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1">
          <mxGeometry x="40" y="60.5" width="120" height="80" as="geometry" />
        </mxCell>
        <UserObject label="Server&lt;br&gt;rack 2" tooltip="Primary" link="https://example.org/rack" placeholders="1" owner="ops" id="b">
          <mxCell style="shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;" vertex="1" parent="1">
            <mxGeometry x="300" y="60" width="60" height="80" as="geometry" />
          </mxCell>
        </UserObject>
        <mxCell id="e1" value="calls" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;endArrow=block;exitX=1;exitY=0.5;entryX=0;entryY=0.5;" edge="1" parent="1" source="a" target="b">
          <mxGeometry relative="1" as="geometry">
            <mxPoint x="10" y="20" as="sourcePoint" />
            <mxPoint x="400" y="20" as="targetPoint" />
            <Array as="points">
              <mxPoint x="200" y="100" />
              <mxPoint x="200" y="40" />
              <mxPoint x="250" y="-15.25" />
            </Array>
            <mxPoint x="-5" y="3" as="offset" />
          </mxGeometry>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
  <diagram id="page-b" name="Details &amp; notes">
    <mxGraphModel dx="0" dy="0" grid="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" value="Layer one" parent="0" visible="0" />
        <object label="Group" kind="team" id="g">
          <mxCell style="swimlane;" vertex="1" collapsed="1" connectable="0" parent="1">
            <mxGeometry x="10" y="10" width="200" height="30" as="geometry">
              <mxRectangle x="10" y="10" width="200" height="150" as="alternateBounds" />
            </mxGeometry>
          </mxCell>
        </object>
        <mxCell id="t" value="line one&#xa;line two" style="text;html=0;" vertex="1" parent="g">
          <mxGeometry x="5" y="5" width="80" height="40" as="geometry" />
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"##;

const BARE: &str = "\u{feff}  <mxGraphModel grid=\"1\"><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"x\" value=\"Hi\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"10\" height=\"10\" as=\"geometry\"/></mxCell></root></mxGraphModel>\n\n";

const ORG_CSV: &str = r##"## An org chart of a small made-up studio
# label: %name%<br><i>%role%</i>
# style: rounded=1;whiteSpace=wrap;html=1;fillColor=%fill%;
# stylename: kind
# styles: {"lead": "shape=hexagon;fillColor=%fill%;", "contractor": "dashed=1;"}
# parentstyle: swimlane;html=1;
# identity: id
# parent: team
# namespace: org-
# connect: {"from": "manager", "to": "id", "invert": true, "label": "manages", "style": "endArrow=blockThin;"}
# connect: {"from": "peers", "to": "id", "style": "dashed=1;endArrow=none;"}
# width: auto
# height: @h
# padding: 8
# ignore: fill,h
# link: url
# left: x
# top: y
# nodespacing: 40
# levelspacing: 60
# edgespacing: 20
# layout: verticaltree
# flavour: vanilla
id,name,role,manager,peers,kind,fill,h,url,team,x,y
studio,Studio,,,,,#eeeeee,200,,,0,0
ana,Ana Ruiz,Director,,,lead,#ffe6cc,60,https://example.org/ana,studio,10,20
ben,"Ben ""Benji"" Okoro","Engineer, tools",ana,cara,,#dae8fc,auto,,studio,,
cara,Cara Li,"Designer
(part time)",ana,,contractor,#d5e8d4,50,,studio,30,40
dev,Dev Patel,Intern,"ana,ghost",,,#f8cecc,x,,nowhere,,
"##;

fn compressed_independently(xml: &str) -> String {
    let encoded = percent_encode(xml);
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(encoded.as_bytes()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap())
}

fn models(file: &DrawioFile) -> Vec<GraphModel> {
    file.pages.iter().map(|p| p.model().unwrap()).collect()
}

#[test]
fn reads_multi_page_file() {
    let f = read_file(MULTI.as_bytes()).unwrap();
    assert_eq!(f.container, Container::MxFile);
    assert_eq!(f.attributes.get("host"), Some("Electron"));
    assert_eq!(f.attributes.get("version"), Some("24.1.0"));
    assert_eq!(f.pages.len(), 2);
    let pages = f.list_pages();
    assert_eq!(pages[1].name.as_deref(), Some("Details & notes"));
    assert_eq!(pages[0].id.as_deref(), Some("page-a"));
    assert!(pages[0].xml_bytes > 100);

    let m = f.pages[0].model().unwrap();
    assert_eq!(m.dx(), Some(1200.0));
    assert_eq!(m.grid(), Some(true));
    assert_eq!(m.page_width(), Some(850.0));
    assert_eq!(m.math(), Some(false));
    assert_eq!(m.background(), Some("#FFFFFF"));
    assert_eq!(m.attributes.get("customThing"), Some("kept"));
    assert_eq!(m.cells.len(), 5);

    let a = m.cell("a").unwrap();
    assert!(a.vertex);
    assert_eq!(a.plain_label(), "Start here");
    let s = a.style.as_ref().unwrap();
    assert!(s.has_name("ellipse"));
    assert_eq!(s.fill_color(), Some("#dae8fc"));
    assert_eq!(parse_color(s.stroke_color().unwrap()), Some(Rgb { r: 0x6c, g: 0x8e, b: 0xbf }));
    let g = a.geometry.as_ref().unwrap();
    assert_eq!((g.x, g.y, g.width, g.height), (40.0, 60.5, 120.0, 80.0));
    assert_eq!(g.as_.as_deref(), Some("geometry"));

    let b = m.cell("b").unwrap();
    let w = b.wrapper.as_ref().unwrap();
    assert_eq!(w.kind, WrapperKind::UserObject);
    let keys: Vec<&str> = w.attributes.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["label", "tooltip", "link", "placeholders", "owner"]);
    assert_eq!(b.custom("tooltip"), Some("Primary"));
    assert_eq!(b.plain_label(), "Server\nrack 2");
    assert_eq!(b.style.as_ref().unwrap().shape(), Some("cylinder3"));
    assert_eq!(b.parent.as_deref(), Some("1"));

    let e = m.cell("e1").unwrap();
    assert!(e.edge);
    assert_eq!((e.source.as_deref(), e.target.as_deref()), (Some("a"), Some("b")));
    let es = e.style.as_ref().unwrap();
    assert_eq!(es.edge_style(), Some("orthogonalEdgeStyle"));
    assert_eq!(es.end_arrow(), Some("block"));
    assert_eq!(es.rounded(), Some(false));
    assert_eq!((es.exit_x(), es.exit_y(), es.entry_x(), es.entry_y()), (Some(1.0), Some(0.5), Some(0.0), Some(0.5)));
    let eg = e.geometry.as_ref().unwrap();
    assert!(eg.relative);
    assert_eq!(eg.points, vec![Point { x: 200.0, y: 100.0 }, Point { x: 200.0, y: 40.0 }, Point { x: 250.0, y: -15.25 }]);
    assert_eq!(eg.source_point, Some(Point { x: 10.0, y: 20.0 }));
    assert_eq!(eg.target_point, Some(Point { x: 400.0, y: 20.0 }));
    assert_eq!(eg.offset, Some(Point { x: -5.0, y: 3.0 }));

    let m2 = f.pages[1].model().unwrap();
    assert_eq!(m2.cell("1").unwrap().visible, Some(false));
    let grp = m2.cell("g").unwrap();
    assert_eq!(grp.wrapper.as_ref().unwrap().kind, WrapperKind::Object);
    assert!(grp.collapsed);
    assert_eq!(grp.connectable, Some(false));
    assert_eq!(grp.geometry.as_ref().unwrap().alternate_bounds, Some(Rect { x: 10.0, y: 10.0, width: 200.0, height: 150.0 }));
    let t = m2.cell("t").unwrap();
    assert_eq!(t.label(), Some("line one\nline two"));
    assert_eq!(t.plain_label(), "line one\nline two");
}

#[test]
fn file_round_trip_is_byte_stable_and_ir_equal() {
    let f = read_file(MULTI.as_bytes()).unwrap();
    let out = write_file(&f);
    let f2 = read_file(&out).unwrap();
    assert_eq!(f, f2);
    assert_eq!(models(&f), models(&f2));
    // Unchanged pages are copied verbatim, so a second write is identical to the first.
    assert_eq!(out, write_file(&f2));
}

#[test]
fn model_round_trip_through_writer() {
    let f = read_file(MULTI.as_bytes()).unwrap();
    for page in &f.pages {
        let m = page.model().unwrap();
        let xml = write_model(&m);
        let m2 = read_model(&xml).unwrap();
        assert_eq!(m, m2);
        assert_eq!(xml, write_model(&m2));
    }
}

#[test]
fn compressed_pages_round_trip() {
    let f = read_file(MULTI.as_bytes()).unwrap();
    let packed = write_file_with(&f, WriteOptions { compress: Some(true) });
    let text = String::from_utf8(packed.clone()).unwrap();
    assert!(!text.contains("<mxGraphModel"));
    let f2 = read_file(&packed).unwrap();
    assert!(f2.pages.iter().all(Page::compressed));
    assert_eq!(models(&f), models(&f2));
    // Recorded compression is kept on a plain write.
    let again = read_file(&write_file(&f2)).unwrap();
    assert!(again.pages.iter().all(Page::compressed));
    // Forcing plain XML brings the inline form back.
    let plain = read_file(&write_file_with(&f2, WriteOptions { compress: Some(false) })).unwrap();
    assert!(plain.pages.iter().all(|p| !p.compressed()));
    assert_eq!(models(&f), models(&plain));
}

#[test]
fn reads_independently_compressed_page() {
    let xml = read_file(MULTI.as_bytes()).unwrap().pages[0].xml().to_string();
    let body = compressed_independently(&xml);
    let doc = format!("<mxfile host=\"test\"><diagram name=\"Packed\" id=\"p1\">\n  {body}\n</diagram></mxfile>");
    let f = read_file(doc.as_bytes()).unwrap();
    assert!(f.pages[0].compressed());
    assert_eq!(f.pages[0].xml(), xml);
    assert_eq!(f.pages[0].model().unwrap().cells.len(), 5);
    assert_eq!(decompress_page(&compress_page(&xml)).unwrap(), xml);
}

#[test]
fn percent_coding() {
    let s = "a b<c>&d=%é\u{1F600}-_.!~*'()";
    let e = percent_encode(s);
    assert!(e.is_ascii());
    assert!(e.starts_with("a%20b%3Cc%3E%26d%3D%25"));
    assert!(e.ends_with("-_.!~*'()"));
    assert_eq!(percent_decode(&e).unwrap(), s);
    assert_eq!(percent_decode("100% sure %zz").unwrap(), "100% sure %zz");
    assert!(percent_decode("%FF").is_err());
}

#[test]
fn reads_bare_model() {
    let f = read_file(BARE.as_bytes()).unwrap();
    assert_eq!(f.container, Container::BareModel);
    assert_eq!(f.pages.len(), 1);
    let m = f.pages[0].model().unwrap();
    assert_eq!(m.cell("x").unwrap().label(), Some("Hi"));
    let f2 = read_file(&write_file(&f)).unwrap();
    assert_eq!(f, f2);
}

#[test]
fn page_lookup_and_replace_keeps_others_intact() {
    let mut f = read_file(MULTI.as_bytes()).unwrap();
    let packed = read_file(&write_file_with(&f, WriteOptions { compress: Some(true) })).unwrap();
    f.pages[1] = packed.pages[1].clone();
    let before = String::from_utf8(write_file(&f)).unwrap();

    assert_eq!(f.find_page(PageRef::Name("Overview")), Some(0));
    assert_eq!(f.find_page(PageRef::Id("page-b")), Some(1));
    assert_eq!(f.find_page(PageRef::Index(2)), None);
    assert_eq!(PageRef::lookup(&f, "Details & notes"), Some(1));
    assert_eq!(PageRef::lookup(&f, "0"), Some(0));
    assert!(f.get_page_xml(PageRef::Index(0)).unwrap().starts_with("<mxGraphModel"));
    assert!(matches!(f.get_page_xml(PageRef::Name("nope")), Err(DrawioError::PageNotFound(_))));

    let new_xml = "<?xml version=\"1.0\"?>\n<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"n\" value=\"New\" vertex=\"1\" parent=\"1\"/></root></mxGraphModel>";
    f.set_page_xml(PageRef::Id("page-a"), new_xml).unwrap();
    assert!(f.set_page_xml(PageRef::Index(0), "<notAModel/>").is_err());
    assert!(f.set_page_xml(PageRef::Index(9), new_xml).is_err());

    let after = String::from_utf8(write_file(&f)).unwrap();
    // The second page's element text is unchanged byte for byte.
    let page_b = |s: &str| s[s.find("<diagram id=\"page-b\"").unwrap()..].split("</diagram>").next().unwrap().to_string();
    assert_eq!(page_b(&before), page_b(&after));
    let reread = read_file(after.as_bytes()).unwrap();
    assert_eq!(reread.pages[0].model().unwrap().cell("n").unwrap().label(), Some("New"));
    assert_eq!(reread.pages[0].name(), Some("Overview"));
    assert_eq!(reread.pages[1].model().unwrap(), packed.pages[1].model().unwrap());

    let idx = f.add_page(Some("page-c".into()), Some("Third".into()), new_xml).unwrap();
    assert_eq!(idx, 2);
    f.pages[2].set_compressed(true);
    let r = read_file(&write_file(&f)).unwrap();
    assert_eq!(r.pages.len(), 3);
    assert!(r.pages[2].compressed());
}

#[test]
fn style_parsing() {
    let s = Style::parse(
        "ellipse;shape=cylinder3;whiteSpace=wrap;fillColor=#FFF;fillColor=none;;empty=;fontSize=abc;rounded=yes;image=data:image/png,AAA=",
    );
    assert_eq!(s.names().collect::<Vec<_>>(), ["ellipse"]);
    assert_eq!(s.shape(), Some("cylinder3"));
    assert_eq!(s.fill_color(), Some("none"));
    assert_eq!(s.get("empty"), Some(""));
    assert_eq!(s.font_size(), None);
    assert_eq!(s.rounded(), None);
    assert_eq!(s.get("image"), Some("data:image/png,AAA="));
    assert!(!s.trailing_semicolon);
    assert_eq!(Style::parse(&s.to_style_string()), s);

    let mut t = Style::parse("a=1;b=2;a=3;");
    assert_eq!(t.get("a"), Some("3"));
    t.set("a", "4");
    assert_eq!(t.to_style_string(), "b=2;a=4;");
    t.set_f64("fontSize", 12.5);
    assert_eq!(t.font_size(), Some(12.5));
    assert_eq!(t.remove("b").as_deref(), Some("2"));
    assert_eq!(t.to_string(), "a=4;fontSize=12.5;");
    assert!(Style::parse("").is_empty());
    assert_eq!(Style::parse(";;").to_style_string(), "");
    assert_eq!(Style::parse(";;"), Style::parse(""));
    assert_eq!(parse_color("#abc"), Some(Rgb { r: 0xaa, g: 0xbb, b: 0xcc }));
    assert_eq!(parse_color("#12345g"), None);
    assert_eq!(parse_color("#é12"), None);
    assert_eq!(parse_color("none"), None);
}

#[test]
fn html_labels() {
    assert_eq!(html_to_text("Hello&nbsp;<b>World</b>"), "Hello World");
    assert_eq!(html_to_text("one<br>two<br/>three"), "one\ntwo\nthree");
    assert_eq!(html_to_text("<div>a</div><div>b</div>"), "a\nb");
    assert_eq!(html_to_text("a<div><br></div><div>b</div>"), "a\n\nb");
    assert_eq!(html_to_text("<p>x &amp; y &lt;z&gt; &#65;&#x42; &copy; &unknown; &</p>"), "x & y <z> AB \u{a9} &unknown; &");
    assert_eq!(html_to_text("  spaced \n  out  "), "spaced out");
    assert_eq!(html_to_text("a<script>bad()</script>b<style>p{}</style>c<!-- note -->d"), "abcd");
    assert_eq!(html_to_text("1 < 2"), "1 < 2");
    assert_eq!(html_to_text("&#1114112;"), "\u{fffd}");
    assert_eq!(text_to_html("a & b\n<c>\r\n\"d\""), "a &amp; b<br>&lt;c&gt;<br>&quot;d&quot;");
    let plain = "Tom & Jerry\nsay \"hi\" <loudly>";
    assert_eq!(html_to_text(&text_to_html(plain)), plain);
}

#[test]
fn csv_org_chart() {
    let d = parse_csv(ORG_CSV).unwrap();
    assert_eq!(d.columns[0], "id");
    assert_eq!(d.nodes.len(), 5);
    assert_eq!(d.layout, Some(CsvLayout::VerticalTree));
    assert_eq!(d.spacing, Spacing { node: Some(40.0), level: Some(60.0), edge: Some(20.0) });
    assert_eq!(d.directives.padding, Some(8.0));
    assert_eq!(d.directives.connect.len(), 2);

    let studio = &d.nodes[0];
    assert_eq!(studio.id, "org-studio");
    assert!(studio.is_container);
    assert_eq!(studio.style, "swimlane;html=1;");
    assert_eq!(studio.height, Some(NodeSize::Fixed(200.0)));

    let ana = &d.nodes[1];
    assert_eq!(ana.label, "Ana Ruiz<br><i>Director</i>");
    assert_eq!(ana.style, "shape=hexagon;fillColor=#ffe6cc;");
    assert_eq!(ana.parent.as_deref(), Some("org-studio"));
    assert_eq!(ana.link.as_deref(), Some("https://example.org/ana"));
    assert_eq!((ana.left, ana.top), (Some(10.0), Some(20.0)));
    assert_eq!(ana.width, Some(NodeSize::Auto));
    assert!(ana.attributes.iter().all(|(k, _)| k != "fill" && k != "h"));
    assert!(ana.attributes.iter().any(|(k, v)| k == "role" && v == "Director"));

    let ben = &d.nodes[2];
    assert_eq!(ben.label, "Ben \"Benji\" Okoro<br><i>Engineer, tools</i>");
    assert_eq!(ben.style, "rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;");
    assert_eq!(ben.height, Some(NodeSize::Auto));
    let cara = &d.nodes[3];
    assert_eq!(cara.label, "Cara Li<br><i>Designer\n(part time)</i>");
    assert_eq!(cara.style, "dashed=1;");

    let manages: Vec<(&str, &str)> = d.edges.iter().filter(|e| e.connect == 0).map(|e| (e.source.as_str(), e.target.as_str())).collect();
    assert_eq!(manages, [("org-ana", "org-ben"), ("org-ana", "org-cara"), ("org-ana", "org-dev")]);
    assert!(d.edges.iter().filter(|e| e.connect == 0).all(|e| e.label.as_deref() == Some("manages")));
    let peers: Vec<(&str, &str)> = d.edges.iter().filter(|e| e.connect == 1).map(|e| (e.source.as_str(), e.target.as_str())).collect();
    assert_eq!(peers, [("org-ben", "org-cara")]);
    assert_eq!(d.edges[3].style.as_deref(), Some("dashed=1;endArrow=none;"));

    let w = d.warnings.join("\n");
    assert!(w.contains("unknown directive 'flavour'"), "{w}");
    assert!(w.contains("'ghost'"), "{w}");
    assert!(w.contains("parent 'nowhere'"), "{w}");
    assert!(w.contains("height 'x'"), "{w}");
}

#[test]
fn csv_edge_cases() {
    let d = parse_csv("# label: %a% and %missing% 100%\r\na,b\r\n\r\n1,2,3\r\n\"x\"\"y\",\n4").unwrap();
    assert_eq!(d.nodes.len(), 3);
    assert_eq!(d.nodes[0].label, "1 and %missing% 100%");
    assert_eq!(d.nodes[1].label, "x\"y and %missing% 100%");
    assert_eq!(d.nodes[2].attributes, vec![("a".to_string(), "4".to_string()), ("b".to_string(), String::new())]);
    assert!(d.warnings.iter().any(|w| w.contains("extra fields")));
    assert_eq!(d.nodes[0].id, "0");

    let empty = parse_csv("## nothing\n# layout: -\n").unwrap();
    assert!(empty.nodes.is_empty());
    assert_eq!(empty.layout, None);

    let bad = parse_csv("# connect: {nope\n# styles: [1]\n# width: wide\n# nonsense\na\n\"open").unwrap();
    assert_eq!(bad.nodes.len(), 1);
    assert!(bad.warnings.len() >= 4, "{:?}", bad.warnings);
}
