# File formats

What VizCraft reads and writes, when each format lands, and the public source each one is built
from. The native `.vizcraft` file, SVG and PNG export, `.drawio` read and write, Mermaid import and CSV
import are built; the rest is planned ([ROADMAP.md](../ROADMAP.md)).

| Format | Direction | Cascade | Public source |
|---|---|---|---|
| `.vizcraft` (native JSON) | read, write (built; version 3 adds the model section in 4A, version 4 the Block 1 document fields; Block 4 adds the optional freeform `path`, `name`, `language` and `dataGraphic` on shapes and `masters`, `protection`, `language`, `ignoredWords`, `dataGraphics` and `dataLinks` on the document, and `image.adjust {brightness, contrast, transparency, recolor}`; Block 5 adds `startLabel` and `endLabel` on connectors, `variant` on a page view and freeform subpaths that mix closed and open, all lenient on read; also reads `.craft.json`) | 1, 4A, B1 | Our own; documented below |
| `.drawio` (mxGraph XML) | read, write (built: every page; import into a page) | 3 | JGraph's draw.io documentation; the Apache-2.0 mxGraph and draw.io format documentation |
| Mermaid | import (built: flowchart, sequence, class, state, ER; node ids become element ids and a tag comes from a `%% tag: <id> <tag>` comment or a label ending in ` #<tag>`, so re-import keeps ids and geometry) | 3, B5 | Mermaid syntax documentation |
| CSV | import (built; an `id` column gives the shape id and `tag`, `type` and the other columns are kept, so `file.import {merge: true}` and `file.reimport` match rows to existing elements) | 3, B5 | draw.io documentation on CSV import |
| XLSX and Markdown tables | write (built: `table.export` and `report.run` with `format: xlsx` or `markdown`; the workbook is a stored-entry ZIP with the five smallest SpreadsheetML parts and inline strings, written by our own code with no new dependency, so the web build writes it too; `table::read` reads it back) | B5 | ECMA-376 Office Open XML, SpreadsheetML |
| Table (CSV or XLSX) for data links | read (built: `datalink.*`; capped at 10 000 rows, 256 columns and 4 KB per cell; XLSX through `calamine`, not in the web build) | B4 | CSV through the `csv` crate; XLSX from ECMA-376 SpreadsheetML (test workbooks are assembled from hand-written XML parts) |
| `.vizcraft-stencil` (custom masters, JSON) | read, write (built: `master.export`, `master.import`) | B4 | Our own: a named list of `Fragment` snapshots |
| SVG | export (built: one page; sequence message labels sit above their line, right of a self-message loop, clear of passed stems and bars; freeforms with more than one closed ring fill even-odd, open subpaths draw unfilled, connector end labels as text beside each end, typed ports as `<g class="ports">`; pictures as `<g class="picture">` with an `<image href="data:...">`, a `vc-cN` clip path for the crop and a `vc-pN` colour-matrix filter for the adjustments, so PNG and PDF follow) | 1, B4 | W3C SVG 1.1 |
| Pictures (PNG, JPEG, SVG) | import (built: `picture.insert`; PNG and JPEG headers read for size, SVG kept as text and rasterised by `resvg` at the drawn size; assets named `img-<hash>` and stored once) | B4 | W3C PNG, ITU-T T.81 JPEG headers, W3C SVG 1.1 |
| PNG | export (built: one page or several, one file each) | 2, built in 3 | W3C PNG specification (written by `resvg`) |
| PDF | export (built: one vector page per document page, foreground pages by default, built from the SVG export through the krilla and krilla-svg crates; text is real PDF text in embedded subsets of the bundled Inter faces; blur effects are rasterised inside the PDF; 100 page units are one inch (0.72 pt per unit), pages capped at 200 inches; printing uses the same writer with paper layout, tiling or fit, and non-printing shapes left out) | 2, B2 | ISO 32000 (PDF) |
| `.vsdx` | import | 4 | Microsoft Open Specifications [MS-VSDX]; ECMA-376 Open Packaging Conventions |
| Mermaid class, state, ER, sequence and architecture-beta into and out of the model | read and write (built: `file.import {format: "mermaid", merge: true}` reads class and state diagrams into `uml` elements, sequence diagrams into the sequence substrate, architecture-beta into `cloud` elements; `file.export` to `.mmd` writes the page by its viewpoint; architecture-beta from the mermaid.js.org page read 2026-10-19, erDiagram into `er` entities with owned columns) | B6 wave 2 | Mermaid syntax documentation |
| PlantUML | write (built: `file.export` to `.puml` for the class, component, package, deployment, use case, activity, state machine and sequence viewpoints; unstructured activity edges come back as warnings) | B6 wave 2 | PlantUML language reference |
| DBML | read and write (built: `file.import {format: "dbml"}` with merge and re-import, `er.write {format: "dbml"}`; tables, columns and settings, refs including composite, enums, table groups, notes and indexes; sticky notes and table partials come back as warnings; capped at 2 MB, 5 000 tables, 256 columns) | B6 wave 2 | DBML documentation (dbml.dbdiagram.io) |
| VizCraft netlist (`vizcraft-netlist` v1, JSON and text) | write (built: `net.export {format: json or text, page?, path?}`; the JSON form is read back) | B6E wave 4 | Our own; documented in `crates/formats/src/netlist.rs` |
| Wire list and nets table (`table.export {subject: nets or connections}`) to CSV, XLSX and Markdown | write (built; rows carry numbers and device:pin names) | B6E wave 4 | existing table writers |
| DEXPI class mapping (`formats::dexpi_map`) | mapping only (built: `plant.dexpiMap`; 40 process types mapped, 29 listed as unmapped; no RDL URIs and no Proteus XML export yet) | B6E wave 4 | DEXPI P&ID Specification 1.4 class lists (CC BY 4.0) |
| Process lists (`plant.list`, `table.export {view: "pack"}`) to CSV, XLSX and Markdown | write (built; line list golden) | B6E wave 4 | existing table writers |
| Pack check kinds for engineering (`netCheck` tests `unconnectedPin`, `singlePin`, `unnumbered`, `duplicateNumber`, `reservedColour`, `pinFanIn`, `drivers`, `undriven`, `property`, `allowedPeers`, `openPort`; `fluidPressurePath`, `fluidRelief`, `fluidSupply`, `fluidRestPosition`, `fluidPortSet`, `fluidLineEnds`, `fluidCodes`, `fluidSteps {idle}`; the process, electrical and mechanical checks) and the variant field `aliases` | read (built; part of the pack JSON schema in `crates/notation/src/schema.rs`) | B6E wave 4 | Our own |
| Structurizr DSL | read and write (planned) | B6 wave 4 | Structurizr DSL language reference |
| D2 | write (planned) | B6 wave 4 | D2 language documentation |

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
carrying a `format` tag, a `version` number, the semantic `model` and the `document`, so later
versions can read older files.

```json
{
  "format": "vizcraft",
  "version": 4,
  "model": {"schemaVersion": 1, "packs": [{"id": "flowchart", "version": "1.0.0"}], "elements": [...], "relationships": [...]},
  "document": {"title": "", "core": {...}, "pages": [{"id": "p1", "view": {...}, "shapes": [...], "connectors": [...]}], "idSeq": 13}
}
```

- **Written:** always version 4, keys in the order `format`, `version`, `model`, `document`. The
  `model` section is `vizcraft_model::Model` (camelCase, its own `schemaVersion`; elements with
  `id`, `type`, `name`, `tag`, `owner`, `properties`; relationships with `id`, `type`, `source`,
  `target`, ports, `name`, `properties`; property values are plain JSON booleans, numbers or
  strings). It is left out when the model is empty, so free-drawing files carry no model. The
  `document` is the diagram as serialized by `vizcraft-diagram` (camelCase fields, colours as
  `#rrggbb`, glue ends as `{"shape", "point"?}` or `{"x", "y"}`), with optional fields left out
  when empty: connector `waypoints`, `avoid` and `jumps`, an `ext` map on shapes, connectors and
  pages, and since version 3 `element` on shapes, `relationship` on connectors and `view`
  (`{"viewpoint", "layout"}`) on pages. Version 4 keeps that layout and adds optional fields,
  each left out at its default: style (`fontFamily`, `underline`, `strikethrough`, `textCase`,
  `highlight`, `hAlign`, `vAlign`, `bullets`, `indent`, `textDirection`, `textMargins`, `dash`,
  `lineCap`, `cornerRadius`, `gradient`, `fillTransparency`, `lineTransparency`, `shadow`,
  `softEdges`, `glow`); shapes (`layers`, `parent`, `textBlock`, `links`, `screenTip`, `locks`,
  `behavior`, `image`); pages (`orientation`, `units`, `drawingScale`, `autoSize`, `background`,
  `isBackground`, `header`, `footer`, `guides`, `layers`); and the document (`properties`,
  `comments`, `images` with base64 `bytes`, capped at 16 MiB each, 48 MiB in all and 1024
  assets), plus `activePage` (the page id shown on open; a missing or unknown id opens the first
  page). Block 2C adds, still at version 4: page `board` (`"infinite"` or `"fixed"`; a page
  without the key loads fixed, so older files keep their sheet, while new pages are infinite),
  connector `kind` (a connector kind id such as `basic.dependency`, left out when empty, cut at
  128 bytes), and eight more arrowhead names beside the first five (`hollowTriangle`,
  `hollowDiamond`, `bar`, `openCircle`, `crowsFoot`, `crowsFootBar`, `crowsFootCircle`,
  `barCircle`; unknown names still load as `none`), and the document `conformance` profile
  (`enabled`, `userSet`, `rulesets`, `overrides: [{rule, severity?, disabled}]`, `suppressions:
  [{key, reason}]`; left out at its default, and junk loads as the default). Also from Block 2C:
  shape `compartments` (`[{id, kind: paragraph | list | slot, title?, text, rows, align, height:
  "auto" | n, fontSize?, bold?, fill?, fontColor?}]`, at most 32 per shape and 256 rows), a glue
  end may name a compartment port instead of a point (`{"shape": "s1", "port": "attrs.left"}`,
  the band's left or right midpoint), page `routing` (`{avoid: "always" | "never",
  straightFallback: "detour" | "keep"}`, left out at the default of always and detour, so older
  files route every connector), and the `waypoints` of a routed connector now include its end
  points, which is how a dynamically glued end remembers the connection point the router chose.
  In draw.io, compartments travel as the wrapper attribute `vizcraftCompartments` (JSON) and come
  back from `ext` key `drawio.attr.vizcraftCompartments`.
- **Read:** version 4; version 3 loads unchanged; version 2 and version 1 (the Cascade 0 placeholder: title and metadata
  only, which loads with that title and metadata and one blank page) load as free drawing with an
  empty model, ignoring any `model` key; a wrapper without a `version` (read as 1); and a bare
  document without the wrapper (empty model).
- **Refused with an error:** input over 64 MiB, anything that is not a JSON object, a wrapper
  whose `format` is not `vizcraft`, a `version` newer than 4 ("saved by a newer VizCraft"), a
  model `schemaVersion` newer than this build reads (same error), a wrapper with no `document`
  object. Nesting depth is bounded by the JSON parser.
- **Tolerated:** a `model` that is not an object loads as an empty model; inside the model and
  the document the loaders are tolerant: missing or wrong-typed fields take defaults, unreadable
  list items are dropped, numbers and text are clamped, and every document has at least one page.
  A shape `element` or connector `relationship` that names no model item is cleared, so that item
  loads as free drawing.

File names: `io::EXTENSION` is `vizcraft`. The engine reads and writes native files for names
ending in `.vizcraft`, `.craft.json` or `.json` (so paths from Cascade 0 keep working), draw.io
files for names ending in `.drawio` or `.drawio.xml`, and refuses other names.

- **Block 6 wave 2 fields** (optional, serde defaults): on a page, `sequence` (the event list,
  activation mode, autonumber, row and column spacing); on a shape, `sequence` (lifeline,
  activation, destruction, fragment with operands and guards, or note part); on a connector,
  `message` (sort) and `step` (badge number, 0 none). Packs gain `sections`, element and
  relationship `keyword`, relationship `endLabels`, compartment `rowsFrom`, element
  `allowedParents`, glyph `container` looks and variant `options`. ER adds connector `halves`
  (per-half dash and double line) and `labelFrame` (a Chen diamond), glyph `textPlace` (`inside` or `below`; also a native shape field, left out when `inside`), rule check `flowBalance {types, relationship, expect: fanOut or fanIn}`, style `compound` (double
  outline), connector type `ends` (ends read from relationship properties), variant `satellites`
  and `compartments`, and table query `owned`, `filter` and `values`. draw.io: the double bar
  writes and reads as `ERmandOne`, the dot end writes as `oval` and reads back as a circle.
- **Blocks 6 and 6E fields** (all optional, serde defaults, left out when default, so a file
  from before them loads and saves back byte for byte; `formats/testdata/pre-6e-sample.vizcraft`
  checks it): in the model, `nets` (`id`, `kind` wire, potential, pipe, fluid or signal, `name`,
  `number`, `label` once, each or off, `properties`; a model holding only nets is still
  written); on the document, `standards {symbols, sheet}` (the project's chosen symbol family and
  sheet preset) and `properties.fields` (the ISO 7200 and ASME title block values by key); on a
  page, `view.module` (`iso2_5mm`, `inch0_1` or a number of page units) and `sheet` (`standard`,
  `size`, `orientation`, `zones`, `number`, `titleBlock`, `revisionScheme`); on a shape, `orient
  {turns, mirror}` (quarter turns and a mirror, canonical form) and `glyph` (a parametric glyph's
  `builder`, parameter overrides, port ids and text slots), and on a freeform `frame` (the unit box
  its path is drawn in); on a connector, `net` (the net id) and `marker` (`doubleSlash`, `cross`,
  `l`, `circle`, `wave`); on a list compartment, `columns`; on an identifier scope, `perOwner`.
  Packs gain `templates`, `module`, `documentProperties`, glyph `builder`, `params` and `source`,
  connector type `net` and `marker`, relationship type `kind` (`crossReference`) and compartment
  `query`.

## .drawio

draw.io's file is an `mxfile` XML element with one `<diagram>` element per page. A page holds an
`mxGraphModel` whose cells are shapes and edges with geometry and a semicolon-separated style
string. A page's content may be stored as plain XML or compressed: raw deflate, then base64.

The codec is the leaf crate `crates/drawio`; the mapping onto the model is
`vizcraft_formats::drawio` (`load`, `save`, `import`, `page_xml`, `master_style`). The engine
reads and writes names ending in `.drawio` or `.drawio.xml` as draw.io files, every page;
`file.export {format: "drawio"}` writes one too, and `file.import`, `page.list`, `page.get` and
`page.set` work with draw.io pages (see [mcp.md](mcp.md) for the tools that call them).

- **Read:** both page encodings, an `mxfile` or a bare `mxGraphModel`. Page id and name come from
  the `<diagram>` element, the size from `pageWidth`/`pageHeight`. Draw.io ids are only unique per
  page, so opening a file renames any id that clashes or is not plain (letters, digits, `_`, `-`,
  `.`) and rewrites glue to match; importing keeps plain free ids, so an agent can address the
  cells it wrote.
- **Written:** one uncompressed `<diagram>` per page holding the root cell `0`, one layer `1`, the
  shapes, then the connectors. Every shape carries `vizcraft=<master id>` (which wins when read
  back) beside the draw.io keys that draw its kind, the colours and the font. Labels are written
  with `html=1` and escaped HTML when reading that back gives the same text; text HTML would
  change (runs of spaces, tabs) is written plain, newlines kept.
- **Style mapping:** the fixed table below, both ways. Any style key not in it stays on the item
  in `ext["drawio.style"]`, in source order, and is written back after our own keys.

| draw.io | VizCraft |
|---|---|
| Default, `shape=rect` | rectangle (`basic.rectangle`) |
| `rounded=1` | rounded rectangle |
| `ellipse` | ellipse |
| `rhombus` | diamond (`flowchart.decision`) |
| `triangle` | triangle (written with `direction=north`, since ours points up) |
| `shape=parallelogram`, `hexagon`, `cylinder3` or `cylinder`, `document`, `cloud`, `process`, `manualInput`, `trapezoid`, `offPageConnector` | parallelogram, hexagon, cylinder, document, cloud, predefined process, manual input, trapezoid, off-page reference |
| `vizcraft=<id>` | that master's kind and default style; wins over the keys above |
| `fillColor`, `strokeColor`, `fontColor` (`none` is transparent), `strokeWidth`, `fontSize`, `fontStyle` (1 bold, 2 italic; other bits kept in `ext`), `rotation` | the matching `Style` fields and `angle_deg` |
| `edgeStyle=orthogonalEdgeStyle` or `elbowEdgeStyle` | orthogonal route; `curved=1` gives curved; anything else straight |
| `startArrow`, `endArrow`: `block`, `classic` (and unknown names) triangle; `open`; `diamond`; `oval` circle; `none` | the matching `Arrow`; a missing `endArrow` is draw.io's default arrow |
| `startArrow`, `endArrow` with `startFill=0` or `endFill=0`: `block`, `diamond`, `oval` | hollow triangle, hollow diamond, open circle |
| `startArrow`, `endArrow`: `ERone`, `ERmany`, `ERoneToMany`, `ERzeroToMany`, `ERzeroToOne` | bar, crow's foot, crow's foot with bar, crow's foot with circle, bar with circle |
| `vizcraftStartLabel`, `vizcraftEndLabel` on an edge | the connector's end labels; the `slash` end is written as draw.io's `dash` |
| `vizcraftKind=<id>` on an edge | the connector's `kind` (the edge twin of `vizcraft=<id>`) |
| `exitX`/`exitY`, `entryX`/`entryY` | point glue at the nearest connection point; without them dynamic glue |
| `sourcePoint`, `targetPoint` | free ends |
| `mxGeometry` points | `waypoints` |
| `jumpStyle` (any value but `none`) | `jumps` (written as `arc`) |
| Edge label child cells | connector text (joined with newlines) |
| `UserObject` / `object` wrapper attributes | kept in `ext` (`drawio.wrapper`, `drawio.attr.<name>`) and written back |
| Other `mxGraphModel` attributes (grid, background) | kept in the page's `ext` (`drawio.model.<name>`) |

- **Defaults:** a draw.io style that leaves a colour out gets draw.io's look (white fill, black
  line, 12 pt black text); a shape with `vizcraft=<id>` gets that master's style instead. An
  imported page is a fixed board at the draw.io page size, so saving and reopening keeps it;
  an infinite board is written at its frame size.
- **Flattened, with a warning:** more than one layer (all land on the page); vertices inside
  other vertices (groups, containers, swimlanes) get absolute positions from their parents'
  offsets; group cells themselves draw nothing.
- **Skipped, with a warning:** images (`shape=image`), cells whose parent is missing, vertices
  without geometry, duplicate ids. An edge ending on a skipped cell keeps a free end at that
  cell's centre. Custom connection points of a VizCraft shape are not written.
- **Limits:** the codec's caps (50 MiB file, 64 MiB decoded page, 256 MiB decoded in total,
  200,000 cells, 1,000 pages, nesting depth 64); the engine reads at most 32 MiB for an import.
  Truncated and garbage input is an error; a broken page in a multi-page file becomes an empty
  page with a warning.
- **Tests:** a fixture typed by hand from the documented two-vertex model
  (`crates/formats/testdata/drawio-docs-example.drawio`) and a two-page fixture written by our own
  codec with its second page compressed, each loaded, saved and reloaded with equal pages.

- **Blocks 6 and 6E keys:** a shape's orientation is written as `rotation` (its angle plus 90 per
  quarter turn) and `flipH`, with a `vizcraftOrient` attribute that is read back first; a foreign
  file's `flipH` and `flipV` read as orientation. A junction carries `vizcraftJunction`; a wire
  carries `vizcraftNet` and `vizcraftMarker`; the page model carries the nets as JSON in
  `vizcraftNets`. Other readers ignore these.

## Mermaid

Import of Mermaid text (`vizcraft_formats::mermaid::import`, the `file.import` command with
`format: "mermaid"` and the `open_mermaid` MCP tool) for five diagram kinds: flowchart, state,
class, entity-relationship and sequence. The syntax comes from the Mermaid documentation and is
parsed by `crates/mermaid`; the result is one page.

- Flowchart, state, class and ER diagrams use the layered auto-layout in the Mermaid direction.
  Each subgraph, composite state, region or namespace is laid out on its own and becomes a
  background rectangle behind its members, titled with the group label.
- Node shapes become the closest Basic or Flowchart master (diamond to decision, stadium to
  terminator, subroutine to predefined process, cylinder to database, hexagon to preparation,
  and so on); unknown shapes become rectangles with a warning. Start and end states are small
  filled circles, fork and join are thin bars. Shapes are sized from their text: class members,
  ER attributes and state descriptions are extra lines, stereotypes a `<<name>>` first line.
- Mermaid ids are kept as shape ids when they are plain and free. Edges become connectors glued
  to both shapes, with arrowheads and labels; class and ER cardinalities are written into the
  label (`1 places 0..*`). Thick links are drawn wider; invisible links only guide the layout.
- `classDef`, `class`, `style` and `linkStyle` set fill, line colour, line width, text colour,
  font size, bold and italic.
- Sequence diagrams are placed directly: participants in a row, a lifeline under each, messages
  as horizontal connectors in order, notes as note shapes, boxes as background rectangles, and
  `autonumber` as numbered labels. Blocks, activations and create/destroy are not drawn yet.
- What the model cannot show yet (the Mermaid shape name, dotted or dashed lines, crow's foot
  markers, links and tooltips) is kept in `ext` under `mermaid.*` keys. Everything skipped is
  reported as a warning, with its line number when it comes from the parser; syntax errors fail
  the import with a line and column.

## CSV

Import of draw.io-style CSV, in `vizcraft_formats::csv::import`: a header of `#` comment lines
carrying directives (label template, style, which columns hold identifiers and links between
rows, sizes, spacing, layout), followed by ordinary CSV rows, one shape per row. The parser is the
leaf crate `crates/drawio`; it caps rows, columns, fields and edges.

- **Shapes:** each row's resolved style goes through the `.drawio` style mapping above; the label
  is the shape's text (HTML labels as plain text). The size comes from `width`/`height` when they
  give a number, otherwise it is estimated from the label (at least 120 by 60).
- **Connectors:** each `connect` reference becomes a connector glued (dynamic glue) to both rows,
  with the directive's style and label.
- **Placement:** with `left`/`top` columns the rows go there. Otherwise the auto-layout places
  them: `verticaltree` and `orgchart` as a top-down tree, `horizontaltree` as a left-right tree,
  `verticalflow` and `horizontalflow` layered top-down or left-right, `organic` force-directed,
  `circle` radial, and `auto`, `none` or no directive layered top-down. `nodespacing` and
  `levelspacing` set the gaps. Other layouts are reported and laid out layered.
- **Warned:** container rows (`parent`) stay ordinary shapes and their members are not grouped;
  images are drawn as rectangles; the parser's own warnings pass through.

## SVG

Export of one page as a standalone SVG 1.1 document, in `vizcraft_formats::svg::export_page`;
`file.export {path, format?, page?}` writes one page, the first unless `page` (an id, name or
index) says otherwise (format `svg`, or taken from a `.svg` extension). Export only; SVG import is not planned. Choices:

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
- **Engineering drawing (Blocks 6 and 6E):** junctions are ordinary shapes (a dot); wire numbers
  and names are drawn upright beside their segments and line markers repeat along the line;
  parametric glyph text slots are drawn upright after the outline; a closed ring that crosses
  itself fills `evenodd`; sheet frames, zone labels, centring marks and title blocks are generated
  shapes on the decoration page; table compartments draw a header row and column rules.
- **Not yet:** the selection only, all pages, embedded fonts, layers as groups.

## PNG

Raster export of one page, in `vizcraft_formats::png::render_page` (built in Cascade 3 for the
headless MCP screenshot). `file.export {path, format?: "png", page?, scale?}` writes it (format
from a `.png` extension too); the MCP `screenshot` tool uses the same command. Choices:

- **Rendering:** the page's SVG export (above) rasterised on the CPU by `resvg`, so a PNG looks
  like the SVG, needs no window or GPU, and builds for wasm.
- **Fonts:** the bundled Inter faces from `assets/fonts` only; no system fonts are read, so the
  output is the same on every machine.
- **Background:** white.
- **Scale:** pixels per page unit, default 1, clamped to 0.25 to 4; zero, negative or non-numeric
  scale is a params error. A side past 8192 pixels lowers the scale so the whole page fits, and
  the file is at most 48 MB.
- **Not yet:** a transparent background, the selection only, a crop to the content.

## PDF

Vector PDF export of one or all pages at the page size. Cascade 2. PDF output follows ISO 32000;
fonts are embedded from `assets/fonts`.

## .vsdx

Import of Microsoft Visio drawings from the public specification. A `.vsdx` is an Open Packaging
Conventions zip package: a content-types part, relationship parts, and XML parts for the
document, pages, masters and themes. We read pages, shapes with their geometry sections, text,
connections and basic formatting, and map master shapes onto our own stencil shapes where they
match. The zip reader caps entry count and decompressed size, and rejects paths that escape the
package. Cascade 4. Writing `.vsdx` is not planned.

## Trademarks

Microsoft, Visio and Office are trademarks of the Microsoft group of companies; draw.io is a
trademark of JGraph Ltd; dbdiagram.io of Holistics Software. The names are used only to identify
products and formats; VizCraft is not affiliated with or endorsed by their owners.
