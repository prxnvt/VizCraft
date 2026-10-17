//! The text behind the `get_reference` tool: what an agent needs to know to write diagrams that
//! VizCraft opens well. It is our own description of our own importers, kept here as static
//! text so the tool works with no app attached.

/// Every topic, in the order the index lists them: name, one-line summary, body.
pub(crate) const TOPICS: &[(&str, &str, &str)] = &[
    ("xml", "The draw.io XML subset open_xml and set_page accept", XML),
    ("styles", "How draw.io style keys map onto VizCraft shapes, lines and text", STYLES),
    ("shapes", "Stencil master ids and the `vizcraft=<id>` style key", SHAPES),
    ("layout", "The auto-layout algorithms and directions", LAYOUT),
    ("routing", "Connectors that avoid shapes, and line jumps", ROUTING),
    ("mermaid", "The Mermaid diagram kinds open_mermaid understands", MERMAID),
    ("csv", "The CSV header directives open_csv understands", CSV),
    ("model", "Model-backed diagrams: notation packs, elements, relationships and issues", MODEL),
];

/// The names of every topic.
pub(crate) fn topic_names() -> Vec<&'static str> {
    TOPICS.iter().map(|(name, _, _)| *name).collect()
}

/// One topic's text, or `None` when there is no such topic.
pub(crate) fn topic(name: &str) -> Option<&'static str> {
    TOPICS.iter().find(|(n, _, _)| *n == name).map(|(_, _, body)| *body)
}

/// The index followed by every topic, for a call with no topic.
pub(crate) fn everything() -> String {
    let mut out = String::from("# VizCraft diagram reference\n\nTopics (pass one as `topic` to get it alone):\n\n");
    for (name, summary, _) in TOPICS {
        out.push_str(&format!("- `{name}`: {summary}\n"));
    }
    for (_, _, body) in TOPICS {
        out.push('\n');
        out.push_str(body);
    }
    out
}

const XML: &str = "\
## XML

open_xml and set_page take one draw.io diagram as XML: either a whole `<mxfile>` (each
`<diagram>` is a page) or a bare `<mxGraphModel>`. Compressed diagram bodies are accepted too.

Structure VizCraft reads:

- `<mxGraphModel>` holds `<root>`, which holds the cells in order.
- The first cell (`id=\"0\"`) is the root; cells whose parent is the root are layers. Write one
  root cell and one layer cell (`id=\"1\" parent=\"0\"`) and give every other cell `parent=\"1\"`.
- A shape is `<mxCell vertex=\"1\" id=... value=... style=... parent=\"1\">` with a child
  `<mxGeometry x y width height as=\"geometry\"/>`. `value` is the label text.
- A connector is `<mxCell edge=\"1\" source=... target=... style=... parent=\"1\">` with a child
  `<mxGeometry relative=\"1\" as=\"geometry\">`. Inside the geometry:
  - `<Array as=\"points\">` with `<mxPoint x y/>` children gives the bend points, in order.
  - `<mxPoint as=\"sourcePoint\"/>` and `<mxPoint as=\"targetPoint\"/>` place an end that is not
    attached to a shape.
- A label on a connector can be the edge's own `value`, or a child cell (`vertex=\"1\"`,
  `connectable=\"0\"`, parent set to the edge id); either becomes the connector's text.

What is simplified:

- Groups, containers and extra layers are flattened: their children land on the page at their
  absolute position, and a warning says so.
- HTML labels (`html=1`) become plain text, with line breaks kept.
- Style keys VizCraft has no use for are kept with the item and written back on export, so a
  round trip does not lose them.
- Ids are kept when they are simple (letters, digits, `_`, `-`, `.`) and not taken; others are
  renamed and the connections follow. The tool result lists the final ids.
";

const STYLES: &str = "\
## Styles

A style is a list of `key=value` pairs separated by `;`. A bare word with no `=` at the start
names a shape, as in `ellipse;fillColor=#ffffff`.

Shape outline:

| draw.io style | VizCraft shape |
|---|---|
| no shape key, or `shape=rect` | `basic.rectangle` |
| `rounded=1` | rounded rectangle |
| `ellipse` | ellipse |
| `rhombus` | diamond (`flowchart.decision`) |
| `triangle` | triangle |
| `shape=parallelogram` | parallelogram |
| `shape=hexagon` | hexagon |
| `shape=cylinder3` or `shape=cylinder` | cylinder |
| `shape=document` | document |
| `shape=cloud` or `ellipse;shape=cloud` | cloud |
| `shape=process` | predefined process |
| `shape=manualInput` | manual input |
| `shape=trapezoid` | trapezoid |
| `shape=offPageConnector` | off-page reference |
| `vizcraft=<master id>` | exactly that master; this key wins over all the others |

Look and text: `fillColor`, `strokeColor`, `fontColor` (`#rrggbb`), `strokeWidth`, `fontSize`,
`fontStyle` (a bit mask: 1 bold, 2 italic, 3 both) and `rotation` (degrees).

Connectors:

| draw.io style | VizCraft connector |
|---|---|
| `edgeStyle=orthogonalEdgeStyle` or `edgeStyle=elbowEdgeStyle` | right-angle route |
| `curved=1` | curved route |
| any other edge | straight route |
| `startArrow`, `endArrow`: `block` or `classic` | filled triangle |
| `open` | open arrow |
| `diamond` | diamond |
| `oval` | circle |
| `none` | no arrow head |
| `exitX`, `exitY` (source), `entryX`, `entryY` (target), 0 to 1 across the shape | glue at the nearest connection point; without them the end slides to the closest side |
| `jumpStyle=arc` | draw hops where this connector crosses earlier ones |
";

const SHAPES: &str = "\
## Shapes

Every VizCraft shape comes from a stencil master with an id of the form `<set>.<name>`, such as
`basic.rectangle`, `basic.ellipse` or `flowchart.decision`. To get a master's look exactly in
XML, put `vizcraft=<master id>` in the cell's style: it overrides every other shape key, and the
other keys (colours, font) still apply on top.

Use search_shapes with a few words (`decision`, `database`, `rounded`) to find masters. Each
result carries its id, label, category, keywords, default width and height, and a ready-made
`style` string to paste into a cell. Unknown ids fall back to a rectangle with a warning.
";

const LAYOUT: &str = "\
## Layout

The `layout` option of open_xml, open_mermaid, open_csv and set_page places the imported shapes
automatically. Without it, XML keeps its own coordinates, Mermaid content is laid out in its own
direction with subgraph boxes around their members, and CSV content follows its `layout`
directive (layered top to bottom by default). Giving `layout` re-lays the imported shapes out
with that algorithm and `direction`, which suits flat graphs best.

| `layout` | Result |
|---|---|
| `layered` | Rows (or columns) of shapes with connectors flowing one way; best for flowcharts, pipelines and dependency graphs |
| `tree` | A tidy tree with each parent centred over its children; graphs that are not trees fall back to layered |
| `radial` | The tree on concentric rings around the root |
| `force` | A spring layout for networks with no natural direction |

`direction` goes with `layout` and sets the flow for layered and tree layouts: `TB` (top to
bottom, the default), `BT`, `LR` (left to right) or `RL`. Shapes are kept from overlapping,
unconnected groups of shapes are packed side by side, and the page grows to fit.

Afterwards, three commands (run with command_run) rearrange a page:

| Command | Effect |
|---|---|
| `layout.full {algorithm?, direction?, ids?}` | Rearranges everything: the named shapes, the selection, or the whole page |
| `layout.incremental {ids, algorithm?, direction?}` | Places only the named (new) shapes next to the shapes they connect to; every other shape stays put |
| `layout.route {ids?}` | Routes connectors around shapes and never moves a shape |
";

const ROUTING: &str = "\
## Routing

`routing: \"orthogonal\"` marks every imported connector to avoid shapes: it is drawn as
horizontal and vertical segments that go around the shapes in its way, and parallel segments are
spread apart. Such connectors are rerouted automatically whenever a shape moves.

Without the option, connectors keep the bend points from the source (or none). The
`layout.route` command does the same later, for chosen connectors or all of them,
`connector.reroute {ids?, avoid: false}` turns avoidance off again, and
`connector.setJumps` turns line jumps on or off: a connector with jumps draws a small hop where
it crosses a connector drawn before it. In XML, `jumpStyle=arc` turns jumps on.
";

const MERMAID: &str = "\
## Mermaid

open_mermaid reads these five kinds, detected from the first line:

| First line | Becomes |
|---|---|
| `flowchart` or `graph`, with a direction (`TD`, `TB`, `BT`, `LR`, `RL`) | Shapes by node bracket (rectangle, rounded, decision, stadium as terminator, cylinder, hexagon, subroutine as predefined process, and so on), arrows with their labels, thick lines (dashed and dotted lines are recorded but drawn solid for now); subgraphs become a background box behind their members |
| `sequenceDiagram` | Participants in a row with lifelines, messages as horizontal arrows from top to bottom in order |
| `classDiagram` | One box per class with its members as text, relations as connectors |
| `stateDiagram` or `stateDiagram-v2` | States and transitions; composite states become a background box |
| `erDiagram` | One box per entity with its attributes as text, relationships as labelled connectors |

The diagram's own direction is used unless `layout` is given. Node ids are kept, so a node `A`
can be addressed as `A` afterwards. Statements that are understood but not drawn come back as
warnings rather than errors; a real syntax error names its line and column.
";

const CSV: &str = "\
## CSV

open_csv takes a header of lines starting with `#`, then an ordinary CSV table whose first row
names the columns. Each data row becomes one shape.

- `## anything` is a comment.
- `# key: value` is a directive; a value of `-` means not set.

Directives:

| Directive | Meaning |
|---|---|
| `label` | Label template; `%column%` is replaced by that column's value |
| `style` | Style template for every shape, with the same placeholders (see the `styles` topic) |
| `stylename`, `styles` | A column whose value picks a style from a JSON object of named styles |
| `identity` | Column holding each row's id; rows are numbered when it is missing |
| `namespace` | Prefix added to every id |
| `parent`, `parentstyle` | Column naming a container row, and the style rows named as parents get |
| `connect` | JSON `{\"from\": column, \"to\": column, \"invert\"?, \"label\"?, \"style\"?}`: each value in `from` (comma separated) links the row to the row whose `to` column matches. Repeat the directive for more links |
| `width`, `height` | A number, `auto`, or `@column` to read it from a column |
| `padding` | Extra space around auto-sized labels |
| `ignore` | Columns not kept as shape data |
| `link` | Column holding a link for the shape |
| `left`, `top` | Columns holding fixed positions, used with `layout: none` |
| `nodespacing`, `levelspacing`, `edgespacing` | Spacing for the layout |
| `layout` | `auto`, `none`, `verticaltree`, `horizontaltree`, `verticalflow`, `horizontalflow`, `organic`, `circle` or `orgchart` |

The tool's own `layout` and `direction` options override the `layout` directive. Problems that
do not stop the import (unknown directives, references to missing rows) come back as warnings.

Example:

```
# label: %name%<br>%role%
# style: rounded=1;fillColor=#dae8fc
# identity: id
# connect: {\"from\": \"manager\", \"to\": \"id\", \"invert\": true}
# layout: verticaltree
id,name,role,manager
1,Ada,Director,
2,Ben,Lead,1
3,Cy,Engineer,2
```
";

const MODEL: &str = "\
## Model

A diagram can be backed by a model: elements (a process step, a data store) and the
relationships between them (a flow), each with a type from a notation pack, a name, an optional
tag and typed properties. Shapes and connectors are views of them, so one element can appear on
several pages. Shapes made from a pack master (every Flowchart master, for instance) create
their element automatically; Basic shapes stay free drawing.

Discover a pack before building with it:

| Ask | How |
|---|---|
| Which packs exist | Read the resource `vizcraft://notation`, or run `notation.list {}` |
| One pack's types, properties, legal connections and rules | Read `vizcraft://notation/{pack}`, or run `notation.describe {pack}` |
| One type | Read `vizcraft://notation/{pack}/{type}` (type ids are pack-qualified, such as `flowchart.process`), or run `notation.describe {pack, type}` |
| What may connect to what | `notation.legal {from, to?}` with element or type ids |

Build with command_run or command_batch:

| Command | Effect |
|---|---|
| `element.create {type, name?, tag?, owner?, properties?, id?, x?, y?, w?, h?, page?}` | Adds an element; with `x` and `y` it is also drawn on the page. Returns `{id, tag, shape?}` |
| `relationship.create {from, to, type?, name?, properties?, id?, strict?}` | Connects two elements (element or shape ids). Without `type` the pack's default is used; `strict: true` refuses a connection the pack does not allow instead of recording it. Returns `{id, connector?}` |
| `validate.run {scope?, ids?}` | Runs every rule, including the slower batch checks (unreachable steps, unmatched references), over the selection, the page or everything (`all`) |
| `validate.list {severity?, rule?}` | The live issues, optionally filtered |

Live rules run after every change. Each issue is `{rule, severity, message, subjects, items,
page?}`: `subjects` are element and relationship ids, `items` the shapes and connectors that draw
them. The results of command_run and command_batch carry `issues` (the issues that call raised)
and `openIssues` (how many are open in all), so an agent sees an illegal connection or a
duplicate tag in the reply to the call that made it. An app that does not check models leaves
both keys out.
";
