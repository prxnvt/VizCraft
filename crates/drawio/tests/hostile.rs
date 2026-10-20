//! Hostile input: truncated fixtures and pseudo-random bytes must give errors or values, never
//! panics or hangs.

use vizcraft_drawio::*;

/// A small xorshift generator so the inputs are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn fixture_file() -> String {
    let mut f = DrawioFile::new();
    f.attributes.set("host", "test");
    let xml = r##"<mxGraphModel dx="10" grid="1"><root><mxCell id="0"/><mxCell id="1" parent="0"/>
<mxCell id="a" value="A &lt;br&gt; b" style="ellipse;html=1;fillColor=#fff;" vertex="1" parent="1"><mxGeometry x="1" y="2" width="3" height="4" as="geometry"/></mxCell>
<UserObject label="U" tooltip="t" id="u"><mxCell style="shape=cube;" vertex="1" parent="1"><mxGeometry width="5" height="5" as="geometry"/></mxCell></UserObject>
<mxCell id="e" edge="1" source="a" target="u" parent="1"><mxGeometry relative="1" as="geometry"><Array as="points"><mxPoint x="5" y="6"/></Array><mxPoint as="sourcePoint"/></mxGeometry></mxCell>
</root></mxGraphModel>"##;
    f.add_page(Some("p1".into()), Some("One".into()), xml).unwrap();
    f.add_page(Some("p2".into()), Some("Two".into()), xml).unwrap();
    f.pages[1].set_compressed(true);
    String::from_utf8(write_file(&f)).unwrap()
}

const CSV: &str = "## c\n# label: %name%\n# style: fillColor=%fill%;\n# identity: id\n# parent: p\n# connect: {\"from\": \"m\", \"to\": \"id\", \"invert\": true}\n# width: @w\nid,name,fill,m,p,w\na,\"A, \"\"x\"\"\",#fff,,,10\nb,B\n,#000,a,a,auto\n";

/// Exercises every entry point on one input.
fn poke(bytes: &[u8]) {
    if let Ok(f) = read_file(bytes) {
        let _ = f.list_pages();
        for p in &f.pages {
            if let Ok(m) = p.model() {
                for c in &m.cells {
                    let _ = c.plain_label();
                    if let Some(s) = &c.style {
                        let _ = (s.fill_color().map(parse_color), s.font_size(), s.rounded(), s.exit_x());
                    }
                }
                let _ = read_model(&write_model(&m));
            }
        }
        let _ = write_file_with(&f, WriteOptions { compress: Some(true) });
    }
    let text = String::from_utf8_lossy(bytes);
    let _ = read_model(&text);
    let _ = parse_csv(&text);
    let _ = Style::parse(&text).to_style_string();
    let _ = html_to_text(&text);
    let _ = text_to_html(&text);
    let _ = decompress_page(&text);
    let _ = percent_decode(&text);
}

#[test]
fn truncated_fixtures_never_panic() {
    let file = fixture_file();
    for fixture in [file.as_str(), CSV] {
        let bytes = fixture.as_bytes();
        for end in 0..=bytes.len() {
            poke(&bytes[..end]);
        }
    }
}

#[test]
fn random_inputs_never_panic() {
    let file = fixture_file();
    let csv = CSV.as_bytes();
    let alphabet: &[u8] = b"<>/=\"' &#;%:,\n\r\tabcxyz019-_.mxCellGraphModelrootdiagramUserObjectArraymxPoint";
    let tokens: &[&str] = &[
        "<mxfile>",
        "</mxfile>",
        "<diagram name=\"x\">",
        "</diagram>",
        "<mxGraphModel>",
        "</mxGraphModel>",
        "<root>",
        "</root>",
        "<mxCell id=\"1\" parent=\"0\" vertex=\"1\" style=\"a;b=c\">",
        "</mxCell>",
        "<mxGeometry x=\"NaN\" width=\"1e999\" as=\"geometry\">",
        "</mxGeometry>",
        "<Array as=\"points\">",
        "<mxPoint x=\"-0\"/>",
        "</Array>",
        "<UserObject label=\"&amp;nbsp;\" id=\"u\">",
        "</UserObject>",
        "&#xFFFFFFFF;",
        "&bogus;",
        "<![CDATA[x]]>",
        "<!-- c -->",
        "\u{feff}",
        "%E9%",
        "# connect: {\"from\":\"a\",\"to\":\"a\"}\n",
        "# label: %a%\n",
        "a,b\n",
        "\"q\"\"\n",
        "<br>",
        "<div>",
        "&lt;",
    ];
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    for i in 0..3000 {
        let input: Vec<u8> = match i % 4 {
            // Random bytes from an XML-flavoured alphabet.
            0 => (0..rng.below(300)).map(|_| alphabet[rng.below(alphabet.len())]).collect(),
            // Concatenated tokens.
            1 => (0..rng.below(40)).flat_map(|_| tokens[rng.below(tokens.len())].bytes()).collect(),
            // The file fixture with random byte flips.
            2 => {
                let mut b = file.as_bytes().to_vec();
                for _ in 0..1 + rng.below(8) {
                    let at = rng.below(b.len());
                    b[at] = (rng.next() & 0xff) as u8;
                }
                b
            }
            // The CSV fixture with random insertions.
            _ => {
                let mut b = csv.to_vec();
                for _ in 0..1 + rng.below(8) {
                    let at = rng.below(b.len() + 1);
                    b.insert(at, alphabet[rng.below(alphabet.len())]);
                }
                b
            }
        };
        poke(&input);
    }
}

#[test]
fn limits_are_enforced() {
    // Deep nesting.
    let deep = format!("<mxGraphModel>{}{}</mxGraphModel>", "<a>".repeat(200), "</a>".repeat(200));
    assert!(matches!(read_file(deep.as_bytes()), Err(DrawioError::Limit(_))));

    // A deflate bomb: a small compressed body that inflates past the page cap.
    use std::io::Write;
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![b'a'; 1 << 20];
    for _ in 0..(limits::MAX_PAGE_BYTES >> 20) + 2 {
        enc.write_all(&chunk).unwrap();
    }
    use base64::Engine;
    let body = base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap());
    let doc = format!("<mxfile><diagram>{body}</diagram></mxfile>");
    assert!(matches!(read_file(doc.as_bytes()), Err(DrawioError::Limit(_))));

    // Too many CSV columns.
    let wide = format!("{}\n", vec!["c"; limits::MAX_CSV_COLUMNS + 1].join(","));
    assert!(matches!(parse_csv(&wide), Err(DrawioError::Limit(_))));

    // Short rows under a wide header cannot be padded into a huge table.
    let header = vec!["c"; limits::MAX_CSV_COLUMNS].join(",");
    let tall = format!("{header}\n{}", "x\n".repeat(limits::MAX_CSV_CELLS / limits::MAX_CSV_COLUMNS + 10));
    assert!(matches!(parse_csv(&tall), Err(DrawioError::Limit(_))));

    // Not a draw.io (JGraph) document, not UTF-8, empty.
    assert!(matches!(read_file(b"<html/>"), Err(DrawioError::NotDrawio(_))));
    assert!(matches!(read_file(&[0xff, 0xfe, 0x00]), Err(DrawioError::Encoding(_))));
    assert!(read_file(b"   ").is_err());
}
