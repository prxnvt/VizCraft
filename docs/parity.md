# Feature parity catalog

This catalog lists the Visio-class features VizCraft aims for, grouped by area, with the cascade
in [ROADMAP.md](../ROADMAP.md) that schedules each and its state. It names features only: this is
a clean-room project, so descriptions are our own words and never another product's wording (see
[AGENTS.md](../AGENTS.md)). The MCP tools that mirror draw.io's MCP server are listed under Agent
integration.

When a feature lands, change its State in the same commit as the code. States:

| State | Meaning |
|---|---|
| `planned` | Scheduled in a cascade, not built |
| `partial` | Some of it works; Notes say what is missing |
| `done` | Built, tested and reachable by command id |
| `done (chrome)` | Provided by the application chrome that exists today |

## Application chrome

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Undo and redo | 0 | done (chrome) | Engine undo stacks; diagram commands plug into them |
| Zoom | 0 | done (chrome) | `view.zoom`, `view.zoomIn`, `view.zoomOut` |
| Pan | 0 | done (chrome) | `view.pan`, `view.resetPan`; middle-drag, Cmd+drag (Ctrl off macOS) or Space+drag |
| Multiple windows | 0 | done (chrome) | One process, one window per document |
| Side panes | 0 | done (chrome) | Left and right panes, resizable and reorderable; diagram panes are added per cascade |
| Ribbon | 0 | done (chrome) | Tabs, groups, contextual tabs; diagram tabs are added per cascade |
| Command palette | 0 | done (chrome) | Search over every command id and label |
| Control channel | 0 | done (chrome) | JSON lines over localhost; see [docs/control-protocol.md](control-protocol.md) |

## Shapes and stencils

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Basic shapes | 1 | planned | Rectangle, rounded rectangle, ellipse, diamond, triangle, parallelogram, hexagon, cylinder, document, cloud |
| Select, move, resize, rotate | 1 | planned | Handles on the canvas and `shape.*` commands |
| Multi-select and marquee | 1 | planned | Uses the existing `Selection` |
| Shapes pane | 1 | planned | Stencil browser; click or drag to insert |
| Basic and Flowchart stencils | 1 | planned | Original shapes defined in code |
| Connection points | 1 | planned | Per-shape points connectors glue to |
| Stencil search | 3 | planned | Keyword index over every stencil |
| Custom stencils | Future | planned | Plugin stencil sets loaded from files |

## Connectors and glue

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Straight, orthogonal and curved connectors | 1 | planned | Route style per connector |
| Dynamic glue | 1 | planned | Glued to a shape; the end moves to the best point as shapes move |
| Point glue | 1 | planned | Glued to one connection point |
| Arrowheads | 1 | planned | Per end, several styles |
| Connector labels | 1 | planned | Text on the connector |
| Waypoints | 2 | planned | User-placed bends |
| Line jumps | 2 | planned | Hops where connectors cross |
| Obstacle-avoiding rerouting | 2 | planned | Orthogonal routes that go around shapes |

## Text

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Text in shapes | 1 | planned | In-place editing on the canvas |
| Font, size, colour, bold, italic | 1 | planned | Home tab font group |
| Alignment and wrapping in the shape | 2 | planned | Horizontal and vertical alignment, margins |
| Text blocks | 2 | planned | Free-standing text shapes |

## Pages

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Multiple pages and page tabs | 2 | planned | `page.*` commands |
| Background pages | 2 | planned | A page shown behind others |
| Page setup | 2 | planned | Size, orientation, units |
| Auto-size page to content | 2 | planned | Page grows with the drawing |

## Layers

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Layers with visible, lock and print flags | 2 | planned | `layer.*` commands |
| Layers pane | 2 | planned | Add, rename, toggle flags, assign shapes |
| Layer colour | 2 | planned | Tint shapes by layer |

## Arrange and layout

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Align | 2 | planned | Left, centre, right, top, middle, bottom |
| Distribute | 2 | planned | Even horizontal and vertical spacing |
| Order | 2 | planned | Bring forward, send backward, to front, to back |
| Group and ungroup | 2 | planned | `arrange.*` commands |
| Rotate and flip | 2 | planned | Rotate by angle, flip horizontal and vertical |
| Size and position | 2 | planned | Numeric x, y, width, height, angle |
| Auto-layout | 3 | planned | Layered, tree, radial, force; whole page or selection |

## Containers, swimlanes and groups

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Groups | 2 | planned | Move and resize as one |
| Containers | 2 | planned | Members move with the container; container fits its members |
| Swimlanes | 2 | planned | Lanes in a cross-functional flowchart |
| Pools and lanes | 4 | planned | BPMN variant of swimlanes |

## Themes and styles

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Themes | 2 | planned | Our own colour and font sets |
| Quick styles | 2 | planned | Preset shape styles within a theme |
| Fill, line, shadow, gradient | 2 | planned | Per shape and connector |
| Format painter | 2 | planned | Copy style from one shape to others |
| Light and dark app theme | 0 | done (chrome) | Window chrome only; diagram colours follow the document theme |

## Snap, grid, rulers and guides

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Grid | 2 | planned | Shown and hidden from the View tab |
| Snap to grid, shapes and points | 2 | planned | Toggles per snap target |
| Rulers | 2 | planned | In page units |
| Guides | 2 | planned | Dragged from the rulers |
| Dynamic alignment guides | 2 | planned | Shown while dragging near other shapes |
| Pan and Zoom pane | 2 | planned | Page overview with the visible area as a frame |

## Data

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Shape data | 2 | planned | Key and value pairs on shapes; Shape Data pane |
| Data graphics | 4 | planned | Callouts, icon sets, colour by value |
| Import from CSV | 3 | planned | draw.io-style CSV; org charts from CSV in Cascade 4 |

## Diagram types and templates

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Flowchart | 1 | planned | Flowchart stencil set |
| Cross-functional flowchart | 4 | planned | Template on swimlanes from Cascade 2 |
| Org chart | 4 | planned | Stencils and a CSV-driven template |
| UML | 4 | planned | Class, sequence, state, activity, use case |
| BPMN | 4 | planned | Events, activities, gateways, pools and lanes |
| ER | 4 | planned | Entities and crow's foot relationships |
| Network | 4 | planned | Generic devices drawn originally; no vendor brand icons |
| Timeline | 4 | planned | Timeline shapes and template |

## Review

| Feature | Cascade | State | Notes |
|---|---|---|---|
| Comments | 4 | planned | On shapes and pages |
| Hyperlinks | 4 | planned | To pages, shapes and URLs |
| Validation | 4 | planned | Rule sets with an issues pane |
| Find and replace | 2 | planned | Shape text and shape data |

## Import and export

| Feature | Cascade | State | Notes |
|---|---|---|---|
| `.vizcraft` native format | 1 | planned | Versioned JSON; see [docs/formats.md](formats.md) |
| SVG export | 1 | planned | Current page |
| PNG export | 2 | planned | Page or all pages |
| PDF export | 2 | planned | Page or all pages |
| .drawio read and write | 3 | planned | Multi-page, compressed pages |
| Mermaid import | 3 | planned | Flowchart, sequence, class, state, ER |
| CSV import | 3 | planned | draw.io-style CSV |
| .vsdx import | 4 | planned | From the public MS-VSDX specification |
| Export and import command slots | 0 | done (chrome) | `file.export` and `file.import` exist and await formats |

## Agent integration

| Feature | Cascade | State | Notes |
|---|---|---|---|
| `command_list` | 1 | planned | Every command id, label and params doc |
| `command_run` | 1 | planned | Run one command by id |
| `command_batch` | 1 | planned | Run several commands in order |
| `doc_inspect` | 1 | planned | Document and UI state as JSON |
| `screenshot` | 1 | planned | PNG of the window |
| `open_xml` | 3 | planned | Load a diagram from .drawio XML |
| `open_mermaid` | 3 | planned | Load a diagram from Mermaid text |
| `open_csv` | 3 | planned | Load a diagram from CSV |
| `search_shapes` | 3 | planned | Keyword search over stencils |
| `get_reference` | 3 | planned | Format and style reference for agents |
| `list_pages` | 3 | planned | Pages of the open document |
| `get_page` | 3 | planned | One page as structured data |
| `set_page` | 3 | planned | Replace or update one page |
| `layout` and `routing` options | 3 | planned | Auto-layout and connector routing applied on open |
| Resources for pages, shapes, commands, stencils | 3 | planned | Read-only MCP resources |
| Remote backend | 1 | planned | Drives a running app over the control channel |
| Headless backend | 3 | planned | Hosts a `Session` in process, no window |

Tool shapes, resources and launch details are in [docs/mcp.md](mcp.md).
