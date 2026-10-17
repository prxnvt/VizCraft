# File formats

What VizCraft reads and writes, when each format lands, and the public source each one is built
from. Only the native JSON file exists today; everything else is planned
([ROADMAP.md](../ROADMAP.md)).

| Format | Direction | Cascade | Public source |
|---|---|---|---|
| `.vizcraft` (native JSON) | read, write | 1 (today: `.craft.json`) | Our own; documented below |
| `.drawio` (mxGraph XML) | read, write | 3 | draw.io documentation; the Apache-2.0 mxGraph and draw.io format documentation |
| Mermaid | import | 3 | Mermaid syntax documentation |
| CSV | import | 3 | draw.io documentation on CSV import |
| SVG | export | 1 | W3C SVG 1.1 and SVG 2 |
| PNG | export | 2 | W3C PNG specification |
| PDF | export | 2 | ISO 32000 (PDF) |
| `.vsdx` | import | 4 | Microsoft Open Specifications [MS-VSDX]; ECMA-376 Open Packaging Conventions |

## Rules for every format

- Format code lands in `crates/formats`, which knows the diagram model and nothing about the UI
  ([architecture.md](architecture.md)). Today's native file lives in `crates/engine/src/io.rs`.
- Bytes in, bytes out: readers take a byte slice, writers return a byte vector, so the desktop,
  the CLI, the MCP server and the web build share one code path.
- Every file is untrusted. Bound every size (file length, decompressed length, element count,
  nesting depth, string length, image dimensions), check arithmetic, and return an error rather
  than allocate without limit.
- Never panic. Each reader has a test that feeds it truncated and garbage input and expects an
  error, in addition to round-trip tests.
- Implement from the public specification or documentation named above, never from another
  implementation's code ([AGENTS.md](../AGENTS.md)). Test files are made by us.

## .vizcraft

The native format: pretty-printed JSON with a wrapper object carrying a `format` tag, a
`version` number and the `document`, so later versions can read older files.

Today the engine saves exactly this wrapper (`format: "vizcraft"`, `version: 1`) under the
extension `.craft.json`, from `crates/engine/src/io.rs`; a bare document without the wrapper is
accepted on load. The document inside is still the trivial sample model. In Cascade 1 the
extension becomes `.vizcraft`, the document becomes the diagram model (pages, layers, shapes,
connectors, styles, shape data), and the code moves to `crates/formats`. Files saved as
`.craft.json` stay readable.

## .drawio

draw.io's file is an `mxfile` XML element with one `<diagram>` element per page. A page holds an
`mxGraphModel` whose cells are shapes and edges with geometry and a semicolon-separated style
string. A page's content may be stored as plain XML or compressed: raw deflate, then base64.

We read both page encodings and write uncompressed XML by default. Shapes, edges, labels, groups,
containers, layers and the common style keys map onto the VizCraft model; style keys we do not
understand are kept on the shape so a round trip does not lose them. Decompressed size and cell
count are capped. Landing in Cascade 3, alongside the `open_xml`, `get_page` and `set_page` MCP
tools ([mcp.md](mcp.md)).

## Mermaid

Import of Mermaid text for five diagram kinds: flowchart, sequence, class, state and
entity-relationship. Each becomes shapes and connectors on a page, placed by the auto-layout
engine (layered by default). Unsupported statements are reported, not guessed at. Cascade 3,
with the `open_mermaid` MCP tool.

## CSV

Import of draw.io-style CSV: a header of comment lines carrying directives (label template,
style, which columns hold identifiers and links between rows, layout choice), followed by
ordinary CSV rows, one shape per row. Rows that reference each other become connectors. This is
how org charts and similar data-driven diagrams are built. Row count and field length are
capped. Cascade 3, with the `open_csv` MCP tool.

## SVG

Export of a page, or the selection, as a standalone SVG document: shapes as paths, text as text
with the font named, connectors with arrowheads. Cascade 1. Export only; SVG import is not
planned.

## PNG and PDF

Raster export of a page at a chosen scale and background, and vector PDF export of one or all
pages at the page size. Cascade 2. PDF output follows ISO 32000; fonts are embedded from
`assets/fonts`.

## .vsdx

Import of Visio drawing files from the public specification. A `.vsdx` is an Open Packaging
Conventions zip package: a content-types part, relationship parts, and XML parts for the
document, pages, masters and themes. We read pages, shapes with their geometry sections, text,
connections and basic formatting, and map master shapes onto our own stencil shapes where they
match. The zip reader caps entry count and decompressed size, and rejects paths that escape the
package. Cascade 4. Writing `.vsdx` is not planned.
