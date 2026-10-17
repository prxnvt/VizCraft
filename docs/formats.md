# File formats

What VizCraft reads and writes, when each format lands, and the public source each one is built
from. The native `.vizcraft` file and SVG export are built; everything else is planned
([ROADMAP.md](../ROADMAP.md)).

| Format | Direction | Cascade | Public source |
|---|---|---|---|
| `.vizcraft` (native JSON) | read, write (built; also reads `.craft.json`) | 1 | Our own; documented below |
| `.drawio` (mxGraph XML) | read, write | 3 | draw.io documentation; the Apache-2.0 mxGraph and draw.io format documentation |
| Mermaid | import | 3 | Mermaid syntax documentation |
| CSV | import | 3 | draw.io documentation on CSV import |
| SVG | export (built: one page) | 1 | W3C SVG 1.1 |
| PNG | export | 2 | W3C PNG specification |
| PDF | export | 2 | ISO 32000 (PDF) |
| `.vsdx` | import | 4 | Microsoft Open Specifications [MS-VSDX]; ECMA-376 Open Packaging Conventions |

## Rules for every format

- Format code lives in `crates/formats` (`vizcraft-formats`), which knows the diagram model and
  nothing about the UI or the engine ([architecture.md](architecture.md)). The engine's
  `crates/engine/src/io.rs` only picks a format from the file name and calls it. The parsers
  that do not need the model are leaf crates: `crates/drawio` (draw.io files and CSV) and
  `crates/mermaid` (Mermaid text) produce faithful intermediate forms, and `crates/formats`
  maps those onto VizCraft documents.
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

The native format, in `vizcraft_formats::native`: pretty-printed JSON with a wrapper object
carrying a `format` tag, a `version` number and the `document`, so later versions can read older
files.

```json
{
  "format": "vizcraft",
  "version": 2,
  "document": {"title": "", "core": {...}, "pages": [{"id": "p1", "shapes": [...], "connectors": [...]}], "idSeq": 13}
}
```

- **Written:** always version 2, the diagram model as serialized by `vizcraft-diagram`
  (camelCase fields, colours as `#rrggbb`, glue ends as `{"shape", "point"?}` or `{"x", "y"}`).
- **Read:** version 2; version 1 (the Cascade 0 placeholder: title and metadata only, which
  loads with that title and metadata and one blank page); a wrapper without a `version` (read
  as 1); and a bare document without the wrapper.
- **Refused with an error:** input over 64 MiB, anything that is not a JSON object, a wrapper
  whose `format` is not `vizcraft`, a `version` newer than 2 ("saved by a newer VizCraft"), a
  wrapper with no `document` object. Nesting depth is bounded by the JSON parser.
- **Inside the document** the model's loader is tolerant: missing or wrong-typed fields take
  defaults, unreadable list items are dropped, numbers and text are clamped, and every document
  has at least one page.

File names: `io::EXTENSION` is `vizcraft`. The engine reads and writes native files for names
ending in `.vizcraft`, `.craft.json` or `.json` (so paths from Cascade 0 keep working) and
refuses other names.

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

Export of one page as a standalone SVG 1.1 document, in `vizcraft_formats::svg::export_page`;
`file.export {path, format?}` writes the first page (format `svg`, or taken from a `.svg`
extension; anything else is an error). Export only; SVG import is not planned. Choices:

- **Size:** the canvas is the page (`viewBox="0 0 width height"`, width and height in pixels).
  When content reaches past the page edge the canvas grows to the content bounds plus a 20 px
  margin, keeping world coordinates, so nothing is cut off.
- **Background:** the whole canvas is filled white.
- **Shapes:** a `<g id>` per shape holding a `<path>` of the world outline (rotation applied)
  with fill, stroke and stroke width (`none` for transparent colours, an opacity attribute for
  translucent ones), plus an unfilled path for interior detail lines (cylinder rim, process bars).
- **Text:** centred `<text>` with a `<tspan>` per line, rotated with its shape, `font-family`
  `Inter, sans-serif`, with size, colour, bold and italic. The font is named, not embedded.
- **Connectors:** the line from the model's connector geometry (shortened under filled heads),
  arrowheads as filled or open paths in the line colour, and the label centred on a white box
  at the label position (box width estimated from the character count).
- **Escaping:** text and attribute values are XML-escaped; control characters XML forbids are
  dropped. Numbers have at most three decimals and never print as NaN or infinity.
- **Not yet:** the selection only, all pages, embedded fonts, layers as groups.

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
