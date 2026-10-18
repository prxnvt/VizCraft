# Part 1: Microsoft Visio 2019 (Windows Desktop) Ribbon Reference
---

## 1. File Tab (Backstage View)
Unlike standard tabs that display toolbars, clicking **File** opens the Backstage view for file management and global application settings:
* **Info:** View document properties, protect the diagram, inspect the file for hidden metadata, or manage versions.
* **New:** Create a new drawing from blank pages or built-in/online templates.
* **Open:** Open existing Visio files from local storage, OneDrive, SharePoint, or recent file lists.
* **Save / Save As:** Save the active drawing locally or to the cloud in various formats (`.vsdx`, `.vsdm`, `.pdf`, `.svg`, etc.).
* **Print:** Preview the diagram, select printers, and configure print scaling or page setup.
* **Share:** Send the drawing via email, present online, or save to cloud storage for collaboration.
* **Export:** Create a PDF/XPS document, change the file type, or export a video/graphic.
* **Close:** Exit the current drawing without closing the application.
* **Account:** Manage Office themes, background styles, license activation, and Microsoft account connections.
* **Options:** Open the Visio Options dialog box to configure general settings, proofing, save paths, Quick Access Toolbar, and Ribbon customization.

## 2. Home Tab
Houses the most frequently used tools for formatting text, drawing basic geometries, styling shapes, and arranging objects on the canvas.

### Clipboard Group
* **Paste:** Insert clipboard contents onto the drawing page (includes options for *Paste* and *Paste Special*).
* **Cut:** Remove the selected shape or text and place it on the clipboard.
* **Copy:** Duplicate the selected shape or text to the clipboard.
* **Format Painter:** Copy visual formatting (fill, line, font) from one shape and apply it to another.

### Font Group
* **Font & Font Size:** Select the typeface and point size for selected text.
* **Increase / Decrease Font Size:** Step the text size up or down incrementally.
* **Bold / Italic / Underline / Strikethrough:** Apply standard typographic emphasis to shape text.
* **Text Case:** Switch selected text between sentence case, lowercase, uppercase, or toggle case.
* **Font Color:** Change the color of the selected text.
* **Text Block Background:** Apply a highlight or background fill color specifically behind the text block.

### Paragraph Group
* **Top / Middle / Bottom Align:** Vertically align text inside a shape's text block.
* **Align Left / Center / Align Right / Justify:** Horizontally align text within its bounding box.
* **Bullets:** Create a bulleted list inside a shape or text box.
* **Decrease / Increase Indent:** Shift paragraph indentation closer to or further from the text block margin.
* **Direction:** Rotate text orientation (Horizontal vs. Vertical/Stacked text).

### Tools Group
* **Pointer Tool:** The default cursor mode used to select, move, and resize shapes.
* **Connector:** Draw dynamic connector lines between shapes that stay attached when shapes move.
* **Text:** Create standalone text boxes or click existing shapes to edit their text block.
* **Drawing Tools Gallery (Rectangle / Ellipse / Line / Freeform / Arc / Pencil):** Draw custom geometric shapes or freehand lines directly on the canvas.
* **Connection Point:** Add, move, or delete connection points (`X` markers) on a shape where connectors can snap.
* **Text Block:** Move, resize, or rotate a shape's text area independently of the shape itself.
* **Crop Tool:** Trim the visible edges of an inserted picture or object.

### Shape Styles Group
* **Quick Styles Gallery:** Apply preset color and effect combinations derived from the active theme.
* **Fill:** Change a shape's interior color, gradient, or pattern, or configure its drop shadow.
* **Line:** Modify the outline color, weight (thickness), dash pattern, arrow style, and corner rounding.
* **Effects:** Apply 3D rotations, bevels, glows, reflections, shadows, or soft edges to shapes.

### Arrange Group
* **Align:** Align multiple selected shapes relative to each other or the page (Left, Center, Right, Top, Middle, Bottom) or run *Auto Align*.
* **Position:** Distribute shapes evenly (horizontally or vertically) or run *Auto Space*.
* **Bring to Front / Send to Back:** Adjust the Z-order (layer stacking) of overlapping shapes.
* **Group:** Combine multiple shapes into a single unit, or ungroup existing groups.
* **Rotate:** Rotate shapes 90 degrees left/right, flip them vertically/horizontally, or rotate page orientation.

### Editing Group
* **Change Shape:** Swap the currently selected shape(s) with a different shape from open stencils while preserving text, connections, and formatting.
* **Layers:** Assign shapes to specific layers or open the *Layer Properties* dialog to lock, hide, or color-code layers.
* **Select:** Choose selection modes (*Select All*, *Select by Type*, *Lasso Select*, or *Area Select*).
* **Find:** Search for specific text strings across the drawing or replace text.

## 3. Insert Tab
Contains buttons for adding new pages, external graphics, structural containers, annotations, and metadata.

### Pages Group
* **New Page:** Insert a new Blank Page, Background Page, or duplicate the current page.

### Illustrations Group
* **Pictures:** Insert an image file stored on your local computer or network.
* **Online Pictures:** Search and insert royalty-free images via Bing.
* **Chart:** Embed a Microsoft Excel chart object directly onto the canvas.
* **CAD Drawing:** Insert an AutoCAD file (`.dwg` or `.dxf`) onto the page for floor plans or engineering layouts.

### Diagram Parts Group
* **Container:** Insert a visual boundary box that logically groups and holds shapes together without locking them into a rigid group.
* **Callout:** Attach a visual annotation or note balloon to a selected shape.
* **Connector:** Insert standard dynamic connectors.

### Links Group
* **Link (Hyperlink):** Attach a URL, local file path, or jump-to-page link to the selected shape.

### Text Group
* **Text Box:** Draw a horizontal or vertical standalone text container.
* **Header & Footer:** Configure text, page numbers, and dates that appear at the margins of printed pages.
* **Field:** Insert dynamic, auto-updating text variables into a shape (e.g., creation date, page name, shape data properties, or geometry measurements).
* **Symbol:** Insert special characters or Unicode symbols into text.
* **Object:** Embed or link an external OLE object (such as a Word document, PDF, or Excel worksheet).
* **ScreenTip:** Add custom hover text that appears when a user rests their mouse pointer over a shape.

## 4. Design Tab
Controls page setup, visual themes, background watermarks, and automated diagram layouts.

### Page Setup Group
* **Orientation:** Toggle the drawing page between Portrait and Landscape.
* **Size:** Set the physical paper dimensions for printing and canvas boundaries, or select *Fit to Drawing*.
* **Auto Size:** Toggle automatic expansion of the drawing page whenever shapes are dragged past the page edge.
* *(Dialog Box Launcher):* Opens the full *Page Setup* window (Print Setup, Page Size, Drawing Scale, Page Properties, Layout and Routing).

### Themes Group
* **Themes Gallery:** Apply an overarching color, font, and effect scheme across the entire diagram.
* **Variants Gallery:** Select one of four color/style variations based on the currently active Theme.

### Backgrounds Group
* **Backgrounds:** Apply a decorative background style (gradients, solids, world maps) via an automatic background page.
* **Borders & Titles:** Add a standardized outer border frame and title block header/footer to the drawing.

### Layout Group
* **Re-Layout Page:** Automatically rearrange the entire diagram into a clean hierarchy, radial, circular, or compact tree layout.
* **Connectors:** Globally switch connector routing styles across the page (Right-Angle, Straight, Curved) or toggle line jumps (bridges/gaps where lines cross).

## 5. Data Tab
Manages shape metadata (Shape Data), external database connections, and data-driven visual indicators.

### External Data Group [Pro]
* **Quick Import:** Automatically link and import an Excel workbook into the drawing in one click.
* **Custom Import:** Launch the *Data Selector Wizard* to connect shapes to Excel, Access, SharePoint lists, SQL Server, or OLEDB/ODBC sources.
* **Refresh All:** Pull the latest data updates from linked external data sources into the diagram.

### Display Data Group
* **Data Graphics [Pro]:** Apply visual overlays (text callouts, data bars, icon sets, or color-by-value rules) directly onto shapes based on their underlying data fields.
* **Insert Legend:** Add a key to the page explaining the icons, colors, and bars used by Data Graphics.
* **External Data Window [Pro]:** Toggle the bottom dockable pane showing rows from linked external databases.
* **Shape Data Window:** Toggle the dockable pane used to view and edit custom properties attached to the selected shape.

### Create Group
* **PivotDiagram [Pro]:** Generate an interactive hierarchical tree diagram that aggregates and summarizes numerical data from external sources.

## 6. Process Tab [Pro]
Dedicated to Business Process Management (BPM), multi-page process breakdowns, and rule validation.

### Diagram Validation Group
* **Check Diagram:** Validate the drawing against built-in topological rules (e.g., BPMN 2.0 or Flowchart rules) to detect unconnected shapes or logic errors, and toggle the *Issues Window*.

### Subprocess Group
* **Create New:** Generate a new page and automatically link the currently selected process step to that new page for detailed breakdown.
* **Link to Existing:** Connect the selected shape to an existing page within the same document or an external Visio file.

### SharePoint Workflow Group
* **Import / Export:** Import or export workflow definitions (`.vwi` files) compatible with Microsoft SharePoint Designer.

## 7. Review Tab
Provides proofing tools, document comparison, markup, and user commenting.

### Proofing Group
* **Spelling:** Check text across the drawing for spelling errors.
* **Thesaurus:** Look up synonyms and related terms in the Research task pane.

### Language Group
* **Translate:** Translate selected shape text into another language using Microsoft Translator.
* **Language:** Set the proofing dictionary language for the selected text.

### Comments Group
* **New Comment:** Attach a threaded review comment to a specific shape or the page itself.
* **Delete:** Remove the selected comment thread.
* **Previous / Next:** Cycle sequentially through all comments in the document.
* **Show Comments:** Toggle the visibility of comment indicators on the drawing canvas and open the *Comments Pane*.

### Ink Group
* **Start Inking:** Open the contextual **Ink Tools** tab to draw, highlight, or annotate freehand using a stylus, touch screen, or mouse.

### Reporting Group
* **Shape Reports:** Run customizable inventory reports that extract shape text, dimensions, and metadata, exporting them to Excel, HTML, Word, or an embedded Visio table.

## 8. View Tab
Controls how the workspace, grids, rulers, zoom levels, and auxiliary task panes are displayed.

### Views Group
* **Presentation Mode:** Display the diagram full-screen without toolbars or menus.
* **Page Breaks:** Show dashed lines indicating where the drawing will split across physical pages when printed.

### Show Group
* **Ruler:** Toggle horizontal and vertical measurement rulers along the top and left edges of the canvas.
* **Grid:** Toggle the background alignment grid on the drawing page.
* **Guides:** Toggle draggable horizontal and vertical reference alignment lines.
* **Connection Points:** Toggle visibility of the attachment nodes on shapes.
* **Task Panes:** Open or close floating/docked utility panes (*Shapes*, *Size & Position*, *Pan & Zoom*, *Navigation*).

### Visual Aids Group
* **AutoConnect:** Toggle the blue directional arrows that appear around selected shapes for one-click shape addition and connection.
* *(Dialog Box Launcher):* Opens the *Snap & Glue* settings dialog to customize what objects snap to (grid, guides, geometry, vertices) and how dynamic glue behaves.

### Zoom Group
* **Zoom:** Open a dialog to set a custom magnification percentage (e.g., 50%, 100%, 200%).
* **Fit to Window:** Scale the entire drawing page so it fits completely inside the current workspace window.
* **Page Width:** Zoom the canvas so the horizontal width of the page fills the screen width.

### Window Group
* **New Window:** Open a second window view of the current document so you can work on two different areas or pages simultaneously.
* **Arrange All / Cascade:** Tile or stack all open Visio windows across the screen.
* **Switch Windows:** Quickly jump between currently open Visio drawing files.

### Macros Group
* **Macros:** View, run, or delete VBA macros, or toggle the macro recorder.
* **Add-Ons:** Run built-in Visio utilities (such as *Number Shapes*, *Move Shapes*, *Array Shapes*, or *Database Wizard*).

## 9. Developer Tab
Used by advanced users and template creators to build custom shapes, edit ShapeSheets, lock behaviors, and write VBA code.

### Code Group
* **Visual Basic:** Launch the Visual Basic for Applications (VBA) Editor (`Alt + F11`).
* **Macros / Record Macro / Pause Recording:** Create and manage automated VBA scripts.
* **Macro Security:** Configure Trust Center settings for macro execution.
* **COM Add-ins:** Manage installed COM add-ins and extensions.

### Controls Group
* **Insert Controls:** Embed ActiveX controls (buttons, checkboxes, text boxes, combo boxes) onto the drawing page.
* **Design Mode:** Toggle between editing ActiveX controls and interacting with them live.
* **Properties / View Code:** Configure properties or write event-handling code for a selected ActiveX control.

### Shape Design Group
* **Operations:** Perform Boolean geometry operations on overlapping shapes (*Union*, *Combine*, *Fragment*, *Intersect*, *Subtract*, *Offset*, *Join*, *Trim*).
* **Behavior:** Configure how a shape responds to double-clicks, grouping, selection highlighting, and layer membership.
* **Protection:** Lock specific attributes of a shape (width, height, aspect ratio, rotation, deletion, or selection).
* **Shape Name:** View or modify the internal Master ID and universal name of a shape.

### Show/Hide Group
* **Document Stencil:** Display the local cache of master shapes used specifically inside the active document.
* **Drawing Explorer:** Open a hierarchical tree view showing all pages, foreground/background layers, styles, shapes, and masters in the file.
* **Show ShapeSheet:** Open the spreadsheet-like interface that defines every mathematical formula, coordinate, and behavior governing a shape, page, or document.

## 10. Key Contextual Tabs
These tabs appear on the Ribbon only when specific objects are selected or specific diagram templates are active:

* **Picture Tools (Format):** Appears when a bitmap image is selected. Includes **Brightness**, **Contrast**, **Recolor**, **Compress Pictures**, **Change Picture**, **Reset Picture**, **Picture Border**, **Transparency**, and **Crop**.
* **Container Tools (Format):** Appears when a Container is selected. Includes **Container Styles**, **Insert Heading**, **Margins**, **Lock Container**, **Select Contents**, and **Disband Container**.
* **Callout Tools (Format):** Appears when a Callout is selected. Includes **Callout Styles**, **Callout Line**, and **Target Orientation**.
* **Ink Tools (Pens):** Appears when inking is active. Includes **Pen**, **Highlighter**, **Eraser**, **Lasso Select**, **Color**, **Thickness**, and **Convert Ink to Shapes/Text**.
* **CAD Drawing Tools (Format):** Appears when an imported AutoCAD drawing is selected. Includes **Convert** (converts CAD lines into native Visio shapes), **Properties** (toggle CAD layer visibility/colors), and **Lock** (prevents accidental movement).
* **Template-Specific Tabs:** Specialized diagram templates activate dedicated workflow tabs—such as **Org Chart** (for arranging subordinates, changing hierarchy layout, and importing pictures), **Timeline** (for configuring intervals, milestones, and time scales), **Cross-Functional Flowchart** (for adding swimlanes, phases, and flipping orientation), and **PivotDiagram** (for adding category nodes and sorting data totals).

# Part 2: Enterprise & Industry Diagramming Standards

Enterprise and industry diagramming standards are governed by consortiums like the **Object Management Group (OMG)**, **The Open Group**, **ISO/IEC**, and **IEEE** to ensure unambiguous communication across software architecture, business operations, systems engineering, and physical infrastructure.

## 1. Software & System Architecture Standards
These standards model code structures, software components, distributed services, and system-level abstractions.

* **UML (Unified Modeling Language — ISO/IEC 19505 / OMG):** The foundational visual modeling specification for object-oriented software design. UML 2.5 defines 14 diagram types split into two categories:
  * *Structural Diagrams:* **Class**, **Component**, **Deployment**, **Object**, **Package**, **Profile**, and **Composite Structure** diagrams.
  * *Behavioral Diagrams:* **Use Case**, **Sequence**, **Activity**, **State Machine (Statechart)**, **Communication**, **Interaction Overview**, and **Timing** diagrams.
* **C4 Model (Context, Containers, Components, Code):** Created by Simon Brown, C4 is a hierarchical, zoom-based approach to software architecture. Rather than enforcing rigid geometric shapes, it standardizes four abstraction levels: **Level 1 (System Context)** for high-level boundaries and users, **Level 2 (Containers)** for deployable applications/databases/APIs, **Level 3 (Components)** for internal modules inside a container, and **Level 4 (Code)**, which delegates to UML Class or Entity-Relationship diagrams.
* **SysML (Systems Modeling Language — ISO/IEC 19514 / OMG):** An extension of a subset of UML tailored for **Model-Based Systems Engineering (MBSE)** across hardware, software, information, and physical processes. It replaces software-centric UML diagrams with **Block Definition Diagrams (BDD)**, **Internal Block Diagrams (IBD)**, **Parametric Diagrams** (for mathematical/engineering constraints), and **Requirements Diagrams**.
* **OPM (Object-Process Methodology — ISO/PAS 19450):** A holistic systems modeling paradigm that integrates system structure and behavior within a single diagram view using only two primary building blocks: stateful **Objects** (things that exist) and **Processes** (things that transform objects), accompanied by an auto-generated natural language subset (OPL).
* **FMC (Fundamental Modeling Concepts):** A semi-formal graphical notation focused on communicating software-intensive system structures via three core perspectives: **Compositional Structures** (agents and storages), **Dynamic Structures** (Petri-net-style behavior), and **Value Range Structures** (data structures).
* **AADL (Architecture Analysis & Design Language — SAE AS5506):** Standardized by SAE International for modeling embedded, real-time, and safety-critical systems (aerospace, automotive, autonomous robotics), mapping software threads and processes directly onto execution hardware.

## 2. Enterprise Architecture (EA) Frameworks & Languages
Used by enterprise architects to align corporate strategy, business capabilities, application portfolios, and underlying IT infrastructure.

* **ArchiMate (The Open Group):** An open, visual modeling language specifically designed to complement **TOGAF** (The Open Group Architecture Framework). ArchiMate models enterprise dependencies across three core color-coded layers (**Business**, **Application**, and **Technology/Physical**) intersected by three aspects (**Active Structure**, **Behavior**, and **Passive Structure**), plus extensions for Strategy, Motivation, and Implementation.
* **UAF (Unified Architecture Framework — OMG) / DoDAF / MODAF:** A standardized enterprise architecture framework built atop SysML to model complex systems-of-systems, originally rooted in defense and government standards (**DoDAF** in the US, **MODAF** in the UK, **NAF** in NATO) and now widely applied in commercial aerospace, telecom, and critical infrastructure.
* **Zachman Framework:** An enterprise ontology represented as a $6 \times 6$ matrix matching six communication interrogatives (*What, How, Where, Who, When, Why*) against six stakeholder perspectives (*Executive, Business Management, Architect, Engineer, Technician, Enterprise*), defining which specialized diagram standard belongs in each cell.

## 3. Business Process, Decision & Workflow Standards
Standards that bridge operational business analysis and executable workflow automation.

* **BPMN 2.0 (Business Process Model and Notation — ISO/IEC 19510 / OMG):** The global standard for business process modeling. It uses **Flow Objects** (Events, Activities, Gateways), **Connecting Objects** (Sequence, Message, and Association flows), **Swimlanes** (Pools and Lanes for organizational roles), and **Artifacts**. BPMN 2.0 models can be serialized directly into XML and executed by workflow engines (e.g., Camunda, Flowable).
* **DMN (Decision Model and Notation — OMG):** A companion standard to BPMN used to decouple complex business rules from process flows. It combines **Decision Requirements Diagrams (DRD)** with standardized **Decision Tables** and the **FEEL** (Friendly Enough Expression Language) syntax.
* **CMMN (Case Management Model and Notation — OMG):** Designed for unstructured, ad-hoc, or knowledge-worker-driven workflows (such as medical diagnosis, legal case handling, or insurance claims) where steps cannot be predetermined in a strict BPMN sequence.
* **IDEF (Integration Definition — NIST / IEEE 1320):** A family of federal and industrial modeling standards:
  * **IDEF0:** Functional modeling using **ICOM** boxes (Inputs, Controls, Outputs, Mechanisms) to decompose complex manufacturing or operational systems.
  * **IDEF1X:** Semantic data and relational database modeling.
  * **IDEF3:** Process description capture showing precedence and causality relationships.
* **EPC (Event-driven Process Chain):** Developed within the **ARIS** (Architecture of Integrated Information Systems) framework and heavily utilized in **SAP** ERP environments. It alternates between passive **Events** (triggers/states) and active **Functions** (tasks) linked by logical `AND`, `OR`, and `XOR` connectors.
* **VSM (Value Stream Mapping — ISO 22468):** Originating from the Toyota Production System and Lean manufacturing, VSM uses standardized symbols to track material and information flows across a supply chain or software delivery pipeline (DevOps), isolating cycle times, wait times, and operational bottlenecks.
* **ISO 5807 (Standard Information Processing Flowcharts):** The classic international standard that formalized flowchart symbols—ovals for terminators, rectangles for processes, diamonds for decisions, parallelograms for I/O, and cylinders for magnetic disk/database storage.

## 4. Data, Information & Pipeline Modeling
Standards dedicated to database schemas, information lineage, and state transitions.

* **ERD (Entity-Relationship Diagrams):** Formulated by Peter Chen and refined into industry notations for relational database design:
  * **Crow's Foot (Information Engineering / IE Notation):** The most widely used industry standard for visualizing tables, primary/foreign keys, and cardinality (`0..1`, `1..1`, `0..*`, `1..*`) via pronged line endings.
  * **Chen Notation:** Uses rectangles for entities, diamonds for relationships, and ovals for attributes; primarily used in conceptual database design.
  * **Barker's Notation:** Popularized by Oracle CASE tools, using soft-cornered boxes, split solid/dashed relationship lines, and hash/circle symbols for mandatory vs. optional attributes.
* **DFD (Data Flow Diagrams — Yourdon/DeMarco & Gane/Sarson):** Models how data moves through an information system without showing control flow or timing. Built from four elements: **External Entities**, **Processes**, **Data Stores**, and **Data Flows**.
* **Petri Nets (ISO/IEC 15909):** A mathematical and graphical modeling standard for distributed, concurrent, and asynchronous systems. It uses bipartite directed graphs consisting of **Places** (circles representing states/conditions), **Transitions** (bars representing events), and **Tokens** that fire across arcs.
* **SDL (Specification and Description Language — ITU-T Z.100):** Standardized by the International Telecommunication Union for unambiguous specification of reactive, distributed, and real-time telecommunication protocols and state machines.

## 5. Cloud, Network & Industrial Engineering Standards
Domain-specific visual standards for physical networks, cloud topologies, and industrial control systems.

* **Cloud Provider Architecture Icons (AWS, Azure, Google Cloud, CNCF):** De facto industry standards governing cloud infrastructure diagrams. They use vendor-standardized icon sets inside hierarchical bounding boxes representing **VPCs**, **Regions**, **Availability Zones**, **Subnets**, **Security Groups**, and **Kubernetes Clusters**.
* **Network Topology Diagrams (Cisco / TIA-606):** Standardized physical (Layer 1/2) and logical (Layer 3) network diagramming conventions using recognized iconography for routers, L2/L3 switches, firewalls, VLAN trunks, DMZs, and BGP/OSPF routing domains.
* **P&ID (Piping and Instrumentation Diagrams — ISA-5.1 / ISO 14617):** The universal industrial engineering standard for process industries (chemical, oil & gas, power generation), depicting physical piping, valves, vessels, sensors, and automated control loops (PLC/SCADA interfaces).
* **IEC 61131-3 (PLC Programming & Control Diagrams):** The international standard for industrial automation and programmable logic controllers, defining three graphical modeling languages: **LD (Ladder Diagram)**, **FBD (Function Block Diagram)**, and **SFC (Sequential Function Chart)** (derived from French **GRAFCET** / IEC 60848).
* **SADT (Structured Analysis and Design Technique) & Ward-Mellor Notation:** Real-time extensions to structured analysis that add control flows, state-transition triggers, and continuous data flows for hardware/software co-design.

## Summary Comparison Matrix

| Standard | Governing Body | Primary Domain | Core Abstraction / Focus | Executable / Formal Semantics |
| :--- | :--- | :--- | :--- | :--- |
| **UML 2.5** | OMG / ISO 19505 | Software Engineering | Object-oriented classes, components, sequences, and states | Partial (Foundational UML / fUML) |
| **C4 Model** | Open / De Facto | Software Architecture | 4 hierarchical zoom levels (Context, Container, Component, Code) | No (Notation-independent) |
| **SysML** | OMG / ISO 19514 | Systems Engineering (MBSE) | Hardware/software blocks, requirements, and physical parametrics | Yes (SysML v2 API & formal semantics) |
| **ArchiMate** | The Open Group | Enterprise Architecture | Cross-layer mapping (Strategy, Business, Application, Technology) | Formal metamodel (XML Exchange) |
| **BPMN 2.0** | OMG / ISO 19510 | Business Operations | End-to-end business workflows, roles, events, and gateways | Yes (Direct XML execution in BPM engines) |
| **DMN** | OMG | Business Rules & Logic | Decision requirements graphs, decision tables, and FEEL logic | Yes (Executable rule engines) |
| **ERD (IE)** | De Facto / ANSI | Database Architecture | Entities, relational tables, keys, and cardinality constraints | Yes (Compiles to SQL DDL schemas) |
| **IDEF0** | NIST / IEEE 1320 | Industrial & Systems Ops | Hierarchical functional decomposition (Inputs, Controls, Outputs, Mechanisms) | Formal syntax, non-executable |
| **Petri Nets** | ISO/IEC 15909 | Concurrent Systems | Asynchronous state transitions, concurrency, and deadlock verification | Yes (Strict mathematical formalism) |