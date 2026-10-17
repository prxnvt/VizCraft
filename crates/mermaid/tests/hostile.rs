//! Hostile input: truncated fixtures and deterministic pseudo-random text must never panic,
//! and every error must carry a usable position.

mod support;

use vizcraft_mermaid::parse;

/// A small linear congruential generator, so the test needs no extra dependency and every run
/// sees the same inputs.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Parse, and when it succeeds also build the graph view; check error positions.
fn exercise(text: &str) {
    match parse(text) {
        Ok(p) => {
            let g = p.diagram.to_graph();
            assert!(g.edges.iter().all(|e| e.length >= 1));
            assert!(p.warnings.iter().all(|w| w.line >= 1));
        }
        Err(e) => {
            assert!(e.line >= 1 && e.column >= 1, "{e:?} for {text:?}");
            assert!(!e.message.is_empty());
        }
    }
}

#[test]
fn truncated_fixtures_never_panic() {
    for fixture in support::ALL {
        for (i, _) in fixture.char_indices() {
            exercise(&fixture[..i]);
            exercise(&fixture[i..]);
        }
        exercise(fixture);
    }
}

/// Characters that matter to the grammars, weighted toward syntax.
const ALPHABET: &[&str] = &[
    "A",
    "B",
    "x",
    "o",
    "-",
    "-",
    "-",
    ".",
    "=",
    "~",
    ">",
    "<",
    "|",
    "&",
    ":",
    ":::",
    ";",
    "\"",
    "`",
    "#",
    "#35;",
    "[",
    "]",
    "(",
    ")",
    "{",
    "}",
    "/",
    "\\",
    "*",
    "@{",
    "}",
    "[*]",
    "<<",
    ">>",
    "+",
    "$",
    "%%",
    "\n",
    "\n",
    " ",
    " ",
    " ",
    "subgraph ",
    "end",
    "state ",
    "class ",
    "note ",
    "namespace ",
    "loop ",
    "alt ",
    "else ",
    "-->",
    "--o",
    "||--o{",
    "}|..|{",
    "<|--",
    "..>",
    "->>",
    "--",
    "é",
    "\u{2665}",
    "\t",
    "direction LR",
    "classDef c fill:#f00",
    "style A fill:#0f0",
    "linkStyle 0 stroke:red",
    "click A \"u\"",
    "box ",
    "Note over A,B: n",
    "PK",
    "~T~",
];

const HEADERS: &[&str] = &["flowchart LR\n", "graph TD;", "stateDiagram-v2\n", "erDiagram\n", "classDiagram\n", "sequenceDiagram\n", ""];

#[test]
fn pseudo_random_text_never_panics() {
    let mut rng = Lcg(0x5eed_1234_abcd_0042);
    for round in 0..3000 {
        let header = HEADERS[round % HEADERS.len()];
        let mut text = String::from(header);
        let pieces = rng.below(60);
        for _ in 0..pieces {
            text.push_str(ALPHABET[rng.below(ALPHABET.len())]);
        }
        exercise(&text);
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Lcg(7);
    for round in 0..2000 {
        let header = HEADERS[round % HEADERS.len()];
        let len = rng.below(200);
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
        let text = format!("{header}{}", String::from_utf8_lossy(&bytes));
        exercise(&text);
    }
}

#[test]
fn mutated_fixtures_never_panic() {
    let mut rng = Lcg(99);
    for fixture in support::ALL {
        let chars: Vec<char> = fixture.chars().collect();
        for _ in 0..400 {
            let mut c = chars.clone();
            for _ in 0..(1 + rng.below(6)) {
                if c.is_empty() {
                    break;
                }
                let at = rng.below(c.len());
                match rng.below(3) {
                    0 => {
                        c.remove(at);
                    }
                    1 => c.insert(at, ALPHABET[rng.below(ALPHABET.len())].chars().next().unwrap_or('x')),
                    _ => {
                        let other = rng.below(c.len());
                        c.swap(at, other);
                    }
                }
            }
            exercise(&c.into_iter().collect::<String>());
        }
    }
}

#[test]
fn pathological_shapes_terminate() {
    let long_dashes = format!("flowchart LR\nA{}B", "-".repeat(100_000));
    let long_text = format!("flowchart LR\nA-- {} B", "x ".repeat(50_000));
    let many_quotes = format!("flowchart LR\n{}", "\"".repeat(100_001));
    let many_brackets = format!("flowchart LR\nA{}", "[".repeat(100_000));
    let many_tildes = format!("classDiagram\nclass A{}", "~".repeat(100_000));
    let many_entities = format!("flowchart LR\nA[\"{}\"]", "#9829;".repeat(50_000));
    let deep_generic = format!("classDiagram\nA : +f({}x{}) int", "List~".repeat(10_000), "~".repeat(10_000));
    for src in [long_dashes, long_text, many_quotes, many_brackets, many_tildes, many_entities, deep_generic] {
        exercise(&src);
    }
}
