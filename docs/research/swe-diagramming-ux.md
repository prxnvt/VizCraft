# Diagramming for software and systems engineers: UX review, standards digest and a template plan

*Research date: 2026-10-19, against main at a056cfe (Block 5 briefs; Block 5 wave 1 running in
its own worktrees). This report reviews the workbench as it is, digests the notations software
and systems engineers draw (UML, entity-relationship, C4, cloud and network topology, data flow),
and proposes a block of work, "Block 6: templates and the engineering workspace", that makes
VizCraft the most direct way for an engineer to get a correct diagram. Recommendations marked
"(synthesis)" are this report's design proposals; every other claim links its source. The
sequencing proposal at the end changes Block 7's enterprise track. Approved by the user on
2026-10-19, sequencing change included; the Block 6 briefs are written once Block 5 is merged.*

## Summary

The workbench already has the bones of a model-backed engineering tool: a page bound to a
notation viewpoint shows a scoped Palette, a contextual notation tab, a typed Inspector with
auto-numbered tags, a Model Tree, live rule checking in the Problems dock, hover insertion
handles, a type-to-insert picker and a command palette over the model (screenshots below). What
it does not yet have is anything a software engineer would recognise as *their* starting point:
the New page offers "Blank drawing" and "Flowchart", the sample document is a free drawing that
hides the model features, the Home tab is an office ribbon of fonts and paragraph tools, the
canvas opens with inch rulers and a print page, and there is no text form of a diagram, no
diagram key, no sequence, class, entity or architecture notation.

The research says what those engineers draw and how they want to draw it. The notations are
stable and well specified: UML 2.5.1's fourteen diagrams, four entity-relationship notations,
the C4 model's four levels plus deployment and dynamic views, vendor cloud topology conventions
(nested groups, labelled unidirectional lines, numbered steps, a key), physical and logical
network views, and data flow diagrams with trust boundaries. The habits are newer: engineers keep
diagrams next to code, so Mermaid (rendered by GitHub, GitLab, Notion and Obsidian), DBML,
Structurizr DSL, PlantUML and D2 are the forms a diagram must be able to take; they reach for
Excalidraw when a sketch is enough, and for a C4 modeller when the drawing must mean something.
No tool the research found combines a semantic model, two-way text sync, validation, agent
access and a native desktop canvas. VizCraft can.

**(synthesis)** The proposal: a template system (a template is a pack viewpoint plus a seeded
skeleton, a layout, a key and a code form), an engineering workspace preset, a Code pane that
keeps Mermaid, DBML or Structurizr DSL text and the canvas in sync, an Analyze group of graph
checks over the model, and the first engineering packs (C4, UML class, sequence, state, activity,
component and deployment, entity-relationship in four notations, generic cloud topology, network
topology, data flow with trust boundaries). The packs pull the C4, ER and UML work forward from
Block 7's enterprise track; Block 7 keeps BPMN, API and events, DDL generation and the patterns.

## The workbench today

Screenshots rendered headlessly from this branch with `ui_shot` at 1440 by 920 points. The PNGs
and the three `ui_shot` scripts that produced them are in the gitignored folder
`log/ux-review-2026-10-19/` of the worktree; retake them with
`VIZCRAFT_NO_PREFS=1 cargo run --release -p vizcraft-ui-egui --example ui_shot -- --size 1440x920 --scale 1 <script>`.

| View | What it shows | Finding |
|---|---|---|
| `01-home.png` | The sample flowchart with the Home tab | The sample is six free shapes glued by connectors. No viewpoint is set, so the Flowchart tab is hidden, the Palette shows "Flowchart" collapsed and the Basic polygons open, the Model Tree pane is collapsed to a strip at the bottom left and the Inspector offers shape data rows. A first-time user never sees the model workbench. |
| `02-insert.png` | Insert tab | Basic Shapes and Flowchart galleries of outlines, Diagram Parts (connector, text, container, callout, swimlanes, compartment), Links, Text. Nothing names a software notation. |
| `03-design.png` | Design tab | Page setup, four themes with eight variants, backgrounds, Re-Layout and connector style. Layout is one menu with four algorithms and no notation awareness. |
| `04-data.png`, `05-process.png`, `07-review.png`, `08-view.png`, `09-developer.png` | Data, Process, Review, View, Developer tabs | Faithful to the office product: external data links, data graphics, subprocesses, comments, ink, spelling, shape reports, window tiling, macros, boolean operations. Process > Validate duplicates the notation tab's Check. View > Show has rulers, grid, guides and page breaks on by default. |
| `06-model.png` | Model tab | Three small buttons (Model Tree pane, Show in All Views, Move in Model) and two deletes. The tab exists but has nothing to say yet. |
| `11-selected-inspector-palette.png` | A free shape selected | Font and style controls light up; the Inspector shows "No shape data yet" and a key-value form. Position fields are in world units with no unit label. |
| `12-dock-problems.png` | Problems tab after `validate.run` | Toolbar with Check, Rules, Live Checking, Previous, Next, severity filters. On a free document checking is off and the tab says so with a Turn On button. |
| `13-insert-picker.png` | Double-click picker | A type-to-filter list of the page's types with a pack badge and the glyph outline. Good; it lists shapes only, never relationships or patterns. |
| `14-command-palette.png` | Command palette, query "dec" | Decision and Process as Flowchart types with an Insert action. Good. |
| `15-backstage.png`, `16-backstage-new.png` | File page and its New page | New offers two cards: Blank drawing and Flowchart under "General". No preview, no use case, no search, no recent templates. |
| `17-dark.png` | Dark mode | Complete and consistent; the sample's fills are light tints on a dark canvas, which reads well. |
| `18-notation-doc.png` | A document created with `file.new` then `model.setViewpoint flowchart.Flowchart`, four elements, three relationships, Expert Inspector, Problems open | This is the real product: the Flowchart tab (Insert per palette group, Viewpoint, Validate), the scoped Palette with "Only this viewpoint", typed Identity (type, name, tag S1 with Auto, id, owner, tag format) and Properties (automated, owner) in the Inspector, live checking on, undo label "New Relationship". Rulers are now in inches at 153 percent zoom and the page frame shows as a white rectangle on the grid. |

What works, and the design should keep:

- Everything is a command, so a template can be a script and an agent can do anything the UI does.
- Packs are data: glyphs, element and relationship types, the connection table, conditional
  styles, palette groups, viewpoints, identifier formats, rules with standard references and
  rulesets. A notation lands as JSON plus drawing support, not as a crate of widgets.
- Insertion on the canvas: hover handles that run `insert.next` with the legal next type, the
  magic connector that asks `suggest.connect`, keyboard chaining with Alt+Shift+Arrow, the
  double-click picker and the command palette. These are the fastest ways to draw a graph that
  any tool in the research offers, and they are already model-aware.
- The Inspector's Identity block with live tag checking and auto-numbering, and the Problems
  dock with fixes on issues.
- Breadcrumbs for drill-down views (`<element id>/<viewpoint>` pages).
- The dark theme, the infinite board default and the obstacle-avoiding router with line jumps.

What hurts a software or systems engineer, in the order they meet it:

1. **The first screen is a drawing of boxes.** The sample is not model-bound; the Model Tree is
   collapsed; the Palette opens on polygons. The user's own direction ("not for kids") is not
   what the app shows first.
2. **No way to start from a use case.** "Flowchart" is the only notation. There is no "System
   context", "Service map", "Database schema", "Sequence of a request", "State machine of a
   device".
3. **The canvas looks like a printed page.** Inch rulers, a page frame, print-oriented page
   setup, page tabs. Engineers expect an infinite dotted canvas measured in nothing, with a page
   only when they export to PDF.
4. **The Home tab is an office ribbon.** Clipboard, Font, Paragraph and Shape Styles take two
   thirds of it; the tools an engineer uses every minute (insert next, connect, relayout, check,
   code form, key) are on four other tabs. At 1280 points the tab already overflows.
5. **No text form.** The research is unanimous that engineers want the diagram as text in the
   repository. Mermaid import exists; export, DBML, Structurizr DSL, PlantUML and D2 do not, and
   nothing stays in sync while drawing.
6. **No key, no title block, no numbered steps.** Every architecture diagramming guide asks for
   them; the app has a Borders and Titles background and a data graphics legend, neither of which
   knows the notation.
7. **No analysis.** Rules check well-formedness. Nothing answers "what depends on this", "is
   there a cycle", "what is unreachable", "what changed since the last version".
8. **The notations are missing.** Class, sequence, state, activity, component, deployment, use
   case, entity-relationship, C4, cloud and network topology, data flow: none exist yet. The
   roadmap puts them in Blocks 7 and 8 behind the industrial and embedded packs.

## Standards digest

What each notation must draw and check, condensed to what a pack needs. Primary sources are the
specifications and the maintainers' documentation; vendor blogs were used only where no primary
source states the practice.

### UML 2.5.1

UML 2.5.1 is OMG formal/17-12-05 ([OMG](https://www.omg.org/spec/UML/About-UML/)). Its taxonomy
has fourteen diagram kinds in two families ([uml-diagrams.org](https://uml-diagrams.org/uml-25-diagrams.html)):

| Family | Diagrams | Engineers draw it | Pack treatment (synthesis) |
|---|---|---|---|
| Structure | Class, Object, Package, Component, Composite Structure, Deployment, Profile | Class and Component daily; Package and Deployment often; Object rarely; Composite Structure and Profile seldom | Class, Component, Package, Deployment as viewpoints in Block 6; Object as a Class variant; Composite Structure and Profile drawable from the same types, no rules |
| Behaviour | Use Case, Activity, State Machine, Sequence, Communication, Interaction Overview, Timing | Sequence daily; State Machine and Activity often; Use Case for requirements; the other three seldom | Sequence (own substrate), State Machine (shares the statechart model of the systems track), Activity, Use Case in Block 6; Communication, Interaction Overview and Timing later (Timing belongs with the embedded WaveJSON work) |

Notation essentials per diagram, from the specification and the tutorial corpus
([Visual Paradigm](https://guides.visual-paradigm.com/understanding-sequence-diagram-notation-in-uml/),
[IBM pseudostates](https://www.ibm.com/docs/zh-tw/SS8PJ7_9.6.1/com.ibm.xtools.modeler.doc/topics/cpseud.html),
[uml-diagrams.org activities](https://www.uml-diagrams.org/activity-diagrams-reference.html),
[Herong Yang class notation](https://www.herongyang.com/UML/Class-Diagram-Aggregation-Composition-Notation.html)):

| Diagram | Elements | Lines and ends | Rules worth checking |
|---|---|---|---|
| Class | Class with name, attributes and operations compartments; visibility `+ - # ~`; `«interface»`, `«enumeration»`, abstract in italics; static underlined | Association (solid, optional reading-direction triangle, role names and multiplicities `0..1`, `1`, `*`, `1..*` at ends), aggregation (hollow diamond at the whole), composition (filled diamond), generalization (hollow triangle at the parent), realization (dashed, hollow triangle), dependency (dashed, open arrow), interface ball and socket | No generalization cycles; a realization only targets an interface; a composition part has at most one whole; multiplicity syntax; unique attribute names in a class |
| Component | Component (box with the component tab glyph), provided and required interfaces (ball, socket), ports, artifacts | Assembly connector (ball meets socket), dependency, delegation | Every required interface is satisfied; port direction |
| Package | Package (tabbed folder), nested packages | Import (dashed `«import»`), merge, dependency | No dependency cycles between packages |
| Deployment | Node (3-D box), device, execution environment, artifact, deployment specification | Communication path (solid), deploy (dashed `«deploy»`), manifest | Artifact deployed to a node; nodes connected |
| Use Case | Actor (stick figure), use case (ellipse), system boundary (box) | Association, include (dashed `«include»`), extend (dashed `«extend»`), actor generalization | Use case inside a boundary; every use case has an actor path |
| Sequence | Lifeline (head box over a dashed line), activation bar, combined fragment frame with operator (`alt`, `opt`, `loop`, `par`, `break`, `critical`, `neg`, `ref`, `seq`, `strict`) and guards `[..]`, interaction use `ref`, gate, lost and found messages, destruction cross | Synchronous (filled arrow), asynchronous (open arrow), reply (dashed, open), create (dashed to the head), self message | Messages ordered top to bottom; a reply matches a call; an activation ends after its last message; a fragment covers the lifelines it spans; a created lifeline starts at its create message |
| State Machine | Initial (filled circle), final (bullseye), state with name and `entry / exit / do` internal actions, composite state, orthogonal regions (dashed divider), shallow history `H`, deep history `H*`, entry and exit points, choice (diamond), junction (filled circle), fork and join (bars), terminate (X), submachine state | Transition `trigger [guard] / effect` | One initial per region; every state reachable; no dead ends except final; events known; guards on choice branches |
| Activity | Initial, activity final (bullseye), flow final (circle with X), action (rounded box), object node, pins, decision and merge (diamond), fork and join (bar), partition (swimlane), interruptible region (dashed rounded box), send and accept signal (pentagons), time event (hourglass) | Control flow, object flow (through pins), interrupting edge (lightning) | Decision outgoing flows carry guards; a join has every inflow; an action has a path from initial |

### Entity-relationship

Four notations are in use and teams argue about them
([DbSchema](https://dbschema.com/blog/design/choosing-an-er-notation-your-team-can-read/),
[OpenDSA](https://opendsa-server.cs.vt.edu/ODSA/RST/en/Database/ERDNotations.rst),
[Barker on Wikipedia](https://en.wikipedia.org/wiki/Barker%27s_notation),
[Gleek](https://gleek.io/blog/er-symbols-notations)). They draw the same model (entities,
attributes, keys, relationships with cardinality and optionality), so one ER pack with four
variants (the Block 5 variant machinery exists for this) is the right shape:

| Notation | Entities and attributes | Relationship line ends | Already drawable | Still needed |
|---|---|---|---|---|
| Information Engineering (crow's foot) | Box with name band and attribute rows; `PK` and `FK` markers | Mandatory one `||`, optional one `o|`, mandatory many `|<`, optional many `o<` | `Bar`, `BarCircle`, `CrowsFoot`, `CrowsFootBar`, `CrowsFootCircle`, `OpenCircle` ends | A double-bar end (one and only one) |
| Chen | Rectangle entity, double rectangle weak entity, oval attribute, underlined key, diamond relationship, double diamond identifying relationship | No ends: `1`, `N`, `M` labels beside each end; double line for total participation | Ellipse and diamond outlines; end labels from Block 5 (`connector.setEndLabels`) | Double-outline shape style; a double-line connector style |
| Barker | Soft box; attributes marked `#` (identifier), `*` (mandatory), `o` (optional); subtypes as nested boxes | Each half of the line solid (mandatory) or dashed (optional); crow's foot for many; UID bar across an end | Rounded box, crow's foot, bar | A per-end dash on a connector (two halves styled differently); nested subtype boxes (containers do this) |
| IDEF1X | Box, rounded corners for an identifier-dependent entity; attributes above and below the key line | Solid line identifying, dashed non-identifying; filled dot at the child, hollow diamond at an optional parent; `P`, `Z`, `n` cardinality letters; categorisation circle with one or two underlines | `Circle`, `HollowDiamond`, dashed lines | Cardinality letter near the end (end labels), the categorisation glyph |

Physical level adds data types per dialect, indexes, constraints and schema names, which is what
DBML carries: tables, columns with settings in brackets (`pk`, `unique`, `not null`, `note`),
`Ref:` lines with `<`, `>`, `-`, `<>`, `Enum` blocks, `Indexes {}` blocks and schemas
([DBML docs](https://dbml.dbdiagram.io/docs/)). Mermaid's `erDiagram` is the lighter form the
parser already reads.

### C4

The C4 model prescribes abstractions, not shapes, and asks every diagram to be self-describing
([c4model.com notation](https://c4model.com/diagrams/notation),
[Structurizr notation](https://docs.structurizr.com/ui/diagrams/notation),
[InfoQ](https://www.infoq.com/articles/c4-architecture-model/)):

| Aspect | Convention |
|---|---|
| Levels | System Context (people, the system, neighbouring systems), Container (deployable units: applications, services, databases, file systems), Component (modules inside a container), Code (hands off to class or ER diagrams); supplementary System Landscape, Dynamic (numbered interactions for one scenario) and Deployment (deployment nodes, infrastructure nodes, instances) |
| Every diagram | A title stating type and scope; a key explaining shapes, colours, borders, lines and arrowheads; acronyms explained |
| Every element | Its type stated on the box (Person, Software System, Container, Component); a short description of responsibilities; technology for containers and components |
| Every line | Unidirectional, labelled with intent (never "Uses" alone); container-to-container lines carry technology or protocol; bidirectional arrows are ambiguous and Structurizr refuses them |
| Colour and shape | Not prescribed; consistent within and across diagrams; must survive black-and-white print and colour blindness |
| Structurizr defaults | Boxes with 2 px stroke, dashed relationships, description shown; shapes Box, RoundedBox, Diamond, Ellipse, Circle, Hexagon, Person, Robot, Component, Folder, Cylinder, Pipe, WebBrowser, Window, MobileDevicePortrait, MobileDeviceLandscape; an automatically generated key; views landscape, context, container, component, dynamic, deployment |
| Text form | Structurizr DSL: a `model` block of `person`, `softwareSystem`, `container`, `component`, relationships with `->`, `deploymentEnvironment` with `deploymentNode`, `containerInstance`, `infrastructureNode`; a `views` block with `systemContext`, `container`, `component`, `dynamic`, `deployment`, `include *`, `autolayout`; names unique within their parent ([language reference](https://docs.structurizr.com/dsl/language)) |

### Cloud and network topology

The vendor icon sets are off limits (AGENTS.md: no vendor product or brand icons), and the
research confirms the useful part is not the icons but the conventions around them. AWS releases
its icon package quarterly and moved to a flat palette with accessibility-tested line colours in
2023; its decks say groups are told apart by colour and line, some groups span others (auto
scaling), others nest (a VPC holds subnets), lines are 2 pt throughout and every icon carries a
label ([AWS icons page](https://aws.amazon.com/architecture/icons/),
[2023 deck](https://www.slideshare.net/slideshow/awsarchitectureiconsdeckforlightbg04282023pdf/259900325),
[2024 deck](https://www.scribd.com/presentation/731525811/AWS-Architecture-Icons-Deck-For-Light-BG-02062024)).
Azure's guidance is the same in spirit: name every icon, never crop, flip, rotate or distort it
([Microsoft Learn](https://learn.microsoft.com/azure/architecture/icons)). Reference
architectures add numbered step badges with a steps legend and a left-to-right request flow
([Ilograph](https://www.ilograph.com/blog/posts/better-aws-architecture-diagrams-vod/)).
Mermaid's `architecture-beta` grammar is the text form engineers now write: `group`, `service`,
`junction`, edges with a side letter on each end (`db:R -- L:server`), `{group}` edges to a
boundary, `align row` and `align column`, five built-in icons (cloud, database, disk, internet,
server) ([Mermaid docs](https://mermaid.js.org/syntax/architecture)).

**(synthesis)** A generic cloud topology pack therefore has:

| Kind | Members |
|---|---|
| Boundary groups (containers with nesting rules) | Cloud, Account or project, Region, Availability zone, Virtual network, Public subnet, Private subnet, Security boundary, Auto-scaling group, Cluster, Namespace, On-premises site, Generic group |
| Node glyphs (original, vendor-neutral) | Compute instance, Container, Serverless function, Relational database, Key-value store, Cache, Object storage, Block storage, Queue, Topic or stream, Load balancer, API gateway, DNS, Content delivery, Firewall, Identity, Secrets, Monitoring, Browser, Mobile client, User, Internet, External service |
| Lines | Request (solid, labelled with protocol), Event (dashed), Optional (dotted), Replication (double), Peering |
| Decorations | Numbered step badges on connectors with a steps legend; the key |
| Rules | A subnet sits in a virtual network in a region; a public subnet needs an internet gateway; a security boundary spans members; every node is labelled; every line is labelled; a single node that every path crosses is reported as a single point of failure |

Network topology keeps two views over one model, physical (devices, ports, cables, racks, sites)
and logical (subnets, VLAN ids, routing domains, DMZ and internal zones), with address tables
beside the drawing ([Domotz](https://blog.domotz.com/?p=27852),
[Graphical Networks](https://graphicalnetworks.com/blog-what-is-a-logical-network-diagram/)).
Device glyphs (router, L2 and L3 switch, firewall, access point, server, storage, workstation)
are drawn originally.

### Data flow and threat models

Yourdon and DeMarco draw processes as circles, Gane and Sarson as rounded boxes with an id band;
both use open-ended rectangles for stores, boxes for external entities and labelled arrows for
flows; a level-0 context diagram holds one process. Threat modelling adds dashed trust boundaries
and a STRIDE list per element crossing a boundary, which the roadmap already assigns to the
enterprise pack. Mermaid has no DFD grammar; the text form is our own CSV or the pytm export.

### SysML v2

OMG adopted SysML v2.0 in 2025 with both textual and graphical notation
([Wikipedia](https://en.wikipedia.org/wiki/Systems_modeling_language),
[release repository](https://github.com/systems-modeling/sysml-v2-release)). It stays in Block 8;
the Block 6 UML component and deployment types and the statechart model are what it will extend.

### How engineers draw now

The 2026 comparisons are vendor blogs, not surveys, but they agree on the shape of the market
([InfraSketch](https://infrasketch.net/blog/best-diagram-as-code-tools-2026),
[8starlabs](https://www.8starlabs.com/blogs/architecture-diagramming-tools-2026),
[Catio](https://www.catio.tech/blog/software-architecture-tools)): Mermaid is the default text
form because GitHub, GitLab, Notion and Obsidian render it; D2 and PlantUML are code-first with
auto-layout; Excalidraw is the sketch tool with files in JSON but "no notion of what each box
represents"; Eraser mixes documents and diagrams; IcePanel models C4. Visio and draw.io remain
the stencil tools. Keyboard-first creation (an arrow key or Tab spawning a connected box, Enter to
type) is what users ask of whiteboard tools
([ClickUp request](https://feedback.clickup.com/feature-requests/p/whiteboard-flowdiagram-keyboard-shortcuts)),
and tldraw and Excalidraw document single-key tools and Enter-to-edit
([tldraw](https://tldraw.dev/examples/ui/keyboard-shortcuts),
[Excalidraw hotkeys via Amplenote](https://assets.amplenote.com/help/handwriting_drawing_in_notes)).

## Principles for an intuitive engineering diagrammer (synthesis)

1. **Start from a use case, not a stencil.** A template is a pack viewpoint, a seeded skeleton of
   typed elements, a layout, a key and a code form. "System context", "Service map", "Database
   schema", "Request sequence", "Device state machine" are the entries, not "Blank drawing".
2. **Text and canvas are one model.** The Code pane shows the page as Mermaid, DBML or
   Structurizr DSL; typing there edits the model, drawing updates the text, ids stay stable so the
   file diffs cleanly in git. Export is "Copy as Mermaid" for a Markdown file.
3. **Keyboard-first creation.** Tab adds the legal next element in the flow direction, Enter adds
   a sibling, typing names it, arrows move the selection along relationships, Alt+Shift+Arrow keeps
   its meaning; the picker accepts relationship and pattern names too.
4. **Layout by default, per notation.** Architecture and C4 lay out left to right; flows top to
   bottom; sequences as lifeline columns and message rows; ER orthogonally with ends never
   overlapping; cloud groups as nested containers. Re-Layout respects pinned shapes.
5. **Every diagram self-describing.** A title block, a key generated from the types and line
   kinds in use, numbered steps on a scenario. Generated, so they never go stale.
6. **Lint, never block.** The Problems dock reports; nothing refuses a stroke. Rules carry the
   standard clause they come from.
7. **Levels are pages.** C4 drill-down, subprocesses and submachines are child pages reached by
   double-click and the breadcrumbs, with parent and child views of the same elements.
8. **Import what exists, export what the team reads.** DDL, DBML, Structurizr DSL, Mermaid,
   OpenAPI in; Mermaid, PlantUML, D2, DBML, Structurizr DSL, SVG and PNG out; later Terraform and
   Compose.
9. **Analysis over the model.** Dependencies, cycles, reachability, single points of failure,
   metrics, and a diff against the last saved or committed version.
10. **Do not look like an office suite by default.** An engineering workspace preset: no rulers,
    a dot grid, no page frame on an infinite board, Model Tree open, the Code pane a click away.
    The office features stay for the people who need to print.

## Template catalogue (synthesis)

A template is data in a pack: `Pack.templates[]` with id, label, description, audience, the
viewpoint, a seed (a command script run inside one transaction, the same form as
`script.run`), the layout algorithm and direction, whether a key and title block are placed, the
code forms it can round-trip, and keywords. The New page renders a preview from the seed. Each row
is one template; packs are the ones proposed in the next section.

| Template | Pack and viewpoint | Seed | Layout | Code forms | Checks that matter |
|---|---|---|---|---|---|
| System context | C4 / Context | One software system, one person, one external system, three labelled relationships, title, key | Layered LR | Structurizr DSL, Mermaid C4, PlantUML | Types stated, every line labelled, no bidirectional lines |
| Container diagram | C4 / Container | A system boundary with web app, API, database; a person; relationships with technology | Layered LR inside the boundary | Structurizr DSL, Mermaid C4 | Technology on every container and container-to-container line |
| Component diagram | C4 / Component | A container boundary with three components and one external container | Layered LR | Structurizr DSL | Components inside one container |
| Deployment diagram | C4 / Deployment | Two deployment nodes (server, database server) with container instances; an infrastructure node | Nested containers | Structurizr DSL | Every instance in a node |
| Dynamic scenario | C4 / Dynamic | Three elements and numbered steps 1 to 3 with a steps legend | Layered LR | Structurizr DSL, Mermaid sequence | Steps contiguous and unique |
| Service map | C4 / Container, microservice style | Gateway, two services, a broker, a database per service (from the `pattern.insert` microservice topology once Block 5 lands) | Layered LR | Mermaid flowchart, Structurizr DSL | Database per service, no shared store |
| Event-driven system | C4 / Container, pub-sub style | Producer, topic, two consumers, event lines dashed | Hub and spoke | Mermaid, AsyncAPI later | Topic has at least one consumer |
| Cloud topology | Cloud / Topology | Cloud, region, virtual network, public and private subnet, load balancer, two compute instances, database, user and internet, numbered request steps, key | Nested containers, request flow LR | Mermaid architecture-beta | Nesting rules, labelled lines, single point of failure |
| Cluster topology | Cloud / Cluster | Cluster with two namespaces, services, pods as containers, ingress | Nested containers | Mermaid architecture-beta | Ingress reaches a service |
| Network topology (logical) | Network / Logical | Internet, firewall, DMZ zone and internal zone, router, two switches, VLAN ids, subnet table | Layered TB | CSV address table | Every subnet has a gateway; DMZ separated by a firewall |
| Network topology (physical) | Network / Physical | Two racks, devices with ports, cables | Grid | CSV device and cable lists | Every cable has two ports |
| Pipeline (CI/CD) | Flowchart / Pipeline style | Stages source, build, test, deploy as a chain with gates | Pipeline | Mermaid flowchart | Gates have a yes and a no |
| Data pipeline | Flowchart / Pipeline style | Sources, bronze, silver, gold, consumers (medallion pattern once Block 5 lands) | Pipeline | Mermaid flowchart | No store without a reader |
| Class diagram | UML / Class | Two classes with attributes and operations, an interface, an association with multiplicities, a generalization | Layered TB | Mermaid class, PlantUML | Generalization cycles, realization targets |
| Sequence diagram | UML / Sequence | Three lifelines, a synchronous call with reply, an `alt` fragment with two guards | Sequence (lifeline columns) | Mermaid sequence, PlantUML | Replies matched, fragments span their lifelines |
| State machine | UML / State Machine (statechart model) | Initial, two states with entry actions, a composite state, a final, transitions with triggers and guards | Layered LR | Mermaid state, SCXML later | Reachability, dead ends, unknown events |
| Activity diagram | UML / Activity | Initial, two partitions, three actions, a decision with guards, fork and join, final | Layered TB in lanes | Mermaid flowchart (lossy), PlantUML | Guards on decisions, joins complete |
| Use case diagram | UML / Use Case | A boundary, two actors, three use cases, one include | Radial around the boundary | PlantUML | Every use case reachable from an actor |
| Component diagram | UML / Component | Two components, a provided and a required interface joined, a dependency | Layered LR | PlantUML | Required interfaces satisfied |
| Deployment diagram | UML / Deployment | Two nodes, an execution environment, two artifacts, a communication path | Nested containers | PlantUML | Artifacts deployed |
| Package diagram | UML / Package | Three packages with two imports | Layered TB | PlantUML | Import cycles |
| Database schema | ER / Information Engineering | Three tables with typed columns, primary and foreign keys, two crow's-foot relationships | Orthogonal ER | DBML, Mermaid ER, SQL DDL (Block 7) | Every table has a key; FK types match; orphan tables |
| Conceptual data model | ER / Chen | Two entities, attributes as ovals, one relationship diamond with 1 and N labels | Force then orthogonal | Mermaid ER (lossy) | Keys underlined, total participation |
| Logical data model | ER / Barker or IDEF1X | Same model, the chosen variant's drawing | Orthogonal ER | DBML | As IE |
| Data flow (context) | DFD / Context | One process, two external entities, four flows | Radial | CSV | Every flow labelled |
| Data flow with trust boundaries | DFD / Threat | Two processes, a store, an external entity, one dashed boundary | Layered LR | pytm (Block 7) | Flows crossing a boundary listed |
| Flowchart, swimlane process, decision tree, incident runbook | Flowchart (exists) | Terminator, steps, decisions; lanes for the swimlane; a decision tree seed | Layered TB | Mermaid flowchart | ISO 5807 ruleset |

The embedded and industrial templates (statechart with code targets, register and memory maps,
timing, P&ID loops) follow the same schema when Block 7's tracks land; nothing here is specific to
software.

## Workspace changes (synthesis)

### New page

A gallery instead of two cards: a search box, audience chips (Software, Systems, Data, Process,
Infrastructure, All), a Recent row, then one card per template with a drawn preview (the seed
rendered through the existing offscreen page renderer into a texture at first show, cached), name,
one-line description and the code forms it round-trips. "Blank" lives at the end as one card per
pack ("Blank C4 page"). Opening a template runs `template.new {template}`: `file.new`,
`model.setViewpoint`, the seed inside one transaction, the layout, the key and title block, then
selects the first element and opens the Inspector. The same command serves agents and the CLI
(`vizcraft-cli new --template c4.context out.vizcraft`).

### Engineering workspace preset

`ui.preset {name: engineering | office}` stored in preferences and chosen on first run from the
New page's audience: rulers off, dot grid, page frame hidden on infinite boards, page tabs shown
only when a document has more than one page, Model Tree open on the left above the Palette, the
Inspector on the right, the Code pane as a right-dock tab, Home tab in its engineering
arrangement (below). The office preset is today's layout. Every toggle remains individually
settable; the preset only sets defaults.

### Ribbon

| Tab | Change |
|---|---|
| Home (engineering preset) | Groups: Clipboard, Text (one font row and one alignment row, the rest behind a dialog launcher), Insert (Next, Connect, Picker, Pattern), Layout (Re-Layout with the notation's default, Route, Pin), Check (Validate, Problems), Code (Code pane, Copy as Mermaid), Arrange (Align, Position, Group, Order), Styles (quick styles only). The office arrangement stays as it is for the office preset. |
| Notation tab (exists per pack) | Add groups: Layout (the pack's algorithms and directions), Key (Insert key, Insert title block, Insert step numbers), Levels (Drill down, Up, Show in parent), Code (the pack's code forms: import, export, open pane), Analyze (below). |
| Model | Grows into the Analyze home: Dependencies (Upstream, Downstream, Both, with a depth), Cycles, Unreachable, Single points of failure, Metrics (fan-in, fan-out, depth), Compare (against the saved file, a chosen file or the git HEAD copy), Clear highlights. Results highlight on the canvas and list in a Analysis dock tab with Select and Explain. |
| Design | Layout menu becomes notation-aware (the active pack's defaults first); page setup unchanged. |
| Process | Validate and Problems move to the notation tab and Home; Subprocess stays. |

### Side panes and dock

- **Palette**: the template's groups open, Connectors as relationship types with their line
  preview (exists), a Patterns row once Block 5 lands.
- **Model Tree**: relationship rows under each element (a Block 3 follow-up), a Views branch
  listing the pages each element appears on, drag to re-own (exists).
- **Inspector**: relationship end editing (multiplicity, role, label, dash per end for Barker),
  compartment rows as typed lists (class attributes as `visibility name: type`, table columns as
  `name type [pk fk]`), a Levels row (parent view, child views), a Key row (include in key).
- **Code pane** (right dock tab or side pane): a text editor over the active page in the chosen
  grammar, with a grammar menu limited to what the pack round-trips, parse errors as Problems
  rows, Apply and Auto-sync, Copy, Save as file; a diff badge when the text and canvas diverge.
- **Analysis dock tab**: the results of the last analysis with Select, Explain and Clear.
- **Key** is not a pane: it is a generated group of shapes on the page (like the data graphics
  legend) that `key.refresh` rebuilds; the title block is a shape with fields.

### Canvas

- Tab and Enter insertion as in the principles; Escape returns to the pointer; `/` on an empty
  canvas opens the picker; the picker lists relationship types when a shape is selected and
  patterns once they exist.
- Sequence editing mode for Sequence pages: lifelines are columns that cannot move vertically,
  messages are rows that drag to reorder and snap to the row grid, fragments are containers that
  span lifelines and resize with their rows, activations grow with their messages.
- ER editing: double-click a column row to edit it in place as text; dragging from a column to
  another table creates a foreign key relationship with the end markers of the page variant.
- Cloud groups: dropping a node into a group re-owns it; a group's border and tint come from its
  kind; a numbered step badge is a click on a connector in the Steps tool.

## Engine and analysis (synthesis)

| Area | What to build | Where |
|---|---|---|
| Templates | `Pack.templates`, `template.list {audience?, pack?}`, `template.new {template, path?}`, preview rendering, Recent templates in preferences | `vizcraft-notation` schema and loader, `engine/cmd/template.rs`, `ui-egui/backstage.rs` |
| Code forms | A `CodeForm` trait in `vizcraft-formats` (grammar id, read to model elements, write from the page with stable ids, diff) with Mermaid (flowchart, sequence, class, state, ER, C4, architecture-beta), DBML, Structurizr DSL, PlantUML (write), D2 (write); `code.read {grammar, text, merge?}`, `code.write {grammar, page?}`; the re-import contract from `b5-reimport` keeps ids and geometry | new `crates/codeforms` (model layer; the Mermaid parser is reused), `engine/cmd/code.rs`, `ui-egui/dock/code.rs` |
| Analysis | `vizcraft-analysis` crate over the model graph: Tarjan strongly connected components, reachability with depth, articulation points and bridges, longest path, fan-in and fan-out, containment depth; `analyze.dependencies`, `analyze.cycles`, `analyze.unreachable`, `analyze.spof`, `analyze.metrics`, `analyze.path {from, to}` returning element ids and highlighting; `model.diff {against: saved, file or path}` returning added, removed and changed elements, relationships and properties, with a Compare page option that draws the diff in colours | new `crates/analysis`, `engine/cmd/analyze.rs`, `ui-egui/dock/analysis.rs` |
| Sequence substrate | Shape kinds for lifeline, activation, message (a connector kind with time order), fragment (a container with operands); a sequence layout primitive (columns by lifeline, rows by message order); reorder interaction; Mermaid sequence read and write | `vizcraft-diagram` (kinds), `vizcraft-layout` (primitive), `ui-egui/canvas/interact/sequence.rs`, the UML pack |
| ER substrate | Double-bar end; per-end dash; double-outline style; cardinality end labels (Block 5); typed column rows in list compartments; the ER layout primitive (orthogonal with end spacing) | `vizcraft-diagram` (arrows, style), `vizcraft-layout`, the ER pack |
| Cloud substrate | Container kinds with nesting rules in the pack (`ElementType.container` plus a new `allowedParents`), group border styles per type, step badges (`connector.setStep`), the key generator (`key.insert`, `key.refresh`) and the title block shape with fields | `vizcraft-notation` schema, `engine/cmd/key.rs`, `vizcraft-diagram` |
| Rules | The checks in the standards tables, declared in the packs with the existing vocabulary plus three new checks: `endMarkerMatchesProperty` (an end drawn from the relationship's multiplicity), `messageOrder` (sequence) and `nestingAllowed` (cloud and deployment containers) | `vizcraft-notation` rules, the packs |
| Formats list | Add Mermaid architecture and C4 grammars, DBML, Structurizr DSL, PlantUML and D2 to the Formats list in AGENTS.md and docs/formats.md when the block starts, each implemented from its public documentation only | AGENTS.md, docs/formats.md (main session) |

What exists to build on: `Pack` with glyphs, types, connections, palette, viewpoints,
identifiers, rules and rulesets; `bind::sync`; `element.create`, `relationship.create`,
`insert.next`, `insert.connect`, `suggest.*`; containers, lanes, compartments, connector kinds
with thirteen ends; `layout.full`, `layout.incremental`, `layout.route` and the Block 5 layout
primitives; `Session::transaction` and `script.run`; the Mermaid parser for five grammars; the
dock with Problems, Search, Activity, Tables and Generate; breadcrumbs; `describe` for agents;
the variant machinery, end labels and path glyphs from Block 5.

## Proposed Block 6: templates and the engineering workspace

Block numbering already skips 6, so this block fills the gap between Block 5 (generators and
glyphs) and Block 7 (first deep packs). It needs Block 5 merged: path glyphs (actor, pseudostate
and device glyphs), ports, variants, end labels, the layout primitives (nested containers,
pipeline, hub) and patterns (template seeds call `pattern.insert`), and the re-import contract
(the Code pane's merge).

Sequencing change for the user to approve: C4, ER and UML move here from Block 7's enterprise
track, and the generic cloud and network topology packs move here from Block 8. Block 7's
enterprise track keeps BPMN, API and events, DDL generation and its re-import, DFD threat lists
and the enterprise patterns; its systems and industrial tracks are unchanged. The three tracks
then start against packs that already have class, state, component and deployment types.

| Branch (port) | Model | Scope | Own (new unless noted) | Starts |
|---|---|---|---|---|
| `b6-templates` (8061) | Opus | Template schema, `template.*`, the New gallery with previews, Recent, the engineering preset `ui.preset`, Home tab arrangement per preset | `notation/src/schema.rs` templates hunk, `engine/src/cmd/template.rs`, `ui-egui/src/backstage.rs`, `ui-egui/src/preset.rs`, `ribbon/home/mod.rs` groups per preset | wave 1 |
| `b6-analysis` (8062) | Opus | `vizcraft-analysis`, `analyze.*`, `model.diff`, the Analyze groups on the Model tab, canvas highlights, the Analysis dock tab | `crates/analysis`, `engine/src/cmd/analyze.rs`, `ribbon/model/analyze.rs`, `dock/analysis.rs`, `canvas/draw/highlights.rs` | wave 1 |
| `b6-uml` (8063) | Opus | UML pack: class, component, package, deployment, use case, activity, state machine viewpoints; compartments for attributes and operations; the ends and rules in the standards table; PlantUML writer for these; Mermaid class and state into the model | `notation/packs/uml.json`, `formats/src/plantuml.rs`, `formats/src/mermaid.rs` class and state hunks, `notation/src/drawing.rs` UML hunks | wave 1 |
| `b6-sequence` (8064) | Opus | Sequence substrate and the UML Sequence viewpoint (adds to `uml.json` under its own `sequence` key to avoid conflicts), sequence layout primitive, reorder interaction, Mermaid sequence read and write | `diagram/src/sequence.rs`, `layout/src/sequence.rs`, `canvas/interact/sequence.rs`, `canvas/draw/sequence.rs`, `formats/src/mermaid.rs` sequence hunk, the `sequence` section of `uml.json` | wave 1 |
| `b6-er` (8065) | Opus | ER pack with IE, Chen, Barker and IDEF1X variants; double-bar end, per-end dash, double outline; typed column rows; ER layout primitive; DBML read and write; Mermaid ER into the model | `notation/packs/er.json`, `diagram/src/arrows.rs` end hunk, `diagram/src/style.rs` per-end dash, `layout/src/er.rs`, `formats/src/dbml.rs`, `formats/src/mermaid.rs` ER hunk | wave 1 |
| `b6-c4` (8066) | Opus | C4 pack with landscape, context, container, component, dynamic and deployment viewpoints; implied relationships; levels (drill-down pages); step badges; Structurizr DSL read and write; Mermaid C4 read and write | `notation/packs/c4.json`, `formats/src/structurizr.rs`, `formats/src/mermaid.rs` C4 hunk, `engine/src/cmd/level.rs`, `engine/src/cmd/step.rs` | wave 2 (needs the key and container nesting from `b6-topology`) |
| `b6-topology` (8067) | Opus | Generic cloud topology pack and network topology pack; `allowedParents` nesting in the schema; group border styles; the key generator and title block; Mermaid architecture-beta read and write; DFD pack with trust boundaries (drawing and rules only, STRIDE stays in Block 7) | `notation/packs/{cloud,network,dfd}.json`, `notation/src/schema.rs` nesting hunk, `engine/src/cmd/key.rs`, `formats/src/mermaid.rs` architecture hunk, `ribbon/notation/key.rs` | wave 1 |
| `b6-code` (8068) | Opus | The `CodeForm` trait, `code.read`, `code.write`, the Code pane with auto-sync and diff badge, D2 writer, "Copy as Mermaid"; adopts the writers the pack branches put behind the trait | `crates/codeforms`, `engine/src/cmd/code.rs`, `dock/code.rs`, `ribbon/notation/code.rs`, `formats/src/d2.rs` | wave 2 (after the three Mermaid-hunk branches merge) |
| `b6-ux` (8069) | Sonnet | Tab and Enter insertion, `/` picker, relationship and pattern rows in the picker, double-click connector label editing, Model Tree relationship rows, Inspector relationship ends and compartment row editing, notation-aware Layout menu, Process tab trim | `canvas/interact/keys.rs`, `insert_picker.rs`, `panes/model_tree.rs` relationship hunk, `panes/inspector.rs` ends and rows hunks, `ribbon/design/layout.rs`, `ribbon/process/` | wave 1 |

Nine branches in two waves of at most five agents: wave 1 is templates, analysis, uml, sequence,
er, topology, ux with five running and the next starting as one reports; wave 2 is c4 and code.
Merge order: ux, templates, analysis, topology, uml, sequence, er, c4, code; snapshots regenerated
after er and after code. Shared files follow the Block 5 rules: `formats/src/mermaid.rs` is split
by grammar hunks, `notation/packs/uml.json` by top-level key, registries are append-only, the main
session merges and writes the ROADMAP, parity, mcp and formats rows. Every pack ships its
`tests/<pack>/` cases for `vizcraft-cli validate` and a template preview snapshot.

Done when for the block: from the New page an engineer picks "Container diagram", gets a laid-out
C4 page with a key and title, presses Tab twice to add two containers that the rules accept, sees
the page as Structurizr DSL in the Code pane, edits a relationship label there and watches the
canvas follow, runs Dependencies on the database and sees its callers highlighted, and exports
Mermaid that GitHub renders; the same for "Database schema" with DBML and "Sequence diagram" with
Mermaid; `cargo xtask ci` is green.

## Decisions needed before the block starts

| Question | Recommendation |
|---|---|
| Move C4, ER, UML, cloud and network packs ahead of the industrial and embedded packs | Yes: they share the substrate every later pack needs (containers with nesting, sequences, typed rows, code forms), and they are what the stated audiences draw first |
| Vendor names anywhere in the cloud pack | None in labels or ids; the template's keywords may carry generic words only ("virtual network", not a product name), so searching for a vendor finds nothing by design |
| Engineering preset as the default for new installs | Yes, chosen from the first New page's audience chips; the office preset is one click away |
| Code pane grammar when a page has several | The pack's first code form; Mermaid where the pack has one, since it renders in the most places |
| Key as generated shapes or as an overlay | Generated shapes, so they export and print; `key.refresh` keeps them current and the drawing rules ignore them |
| Sequence pages: own board or a page like any other | A page like any other with a sequence editing mode, so breadcrumbs, comments, pictures and export work unchanged |
