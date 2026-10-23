# Ultra review 1: VizCraft main at ca9982d

23 October 2026. Read-only correctness review of the whole repository. No file was changed and no build was run. The same findings are published as an interactive page at https://claude.ai/artifact/Dt58r6FLMPdjhJ7uuUEyKj.

## Method

A finding is a defect confirmed by tracing the call path in the source. Each entry names the file and line where the defect is anchored, states the defect in one sentence, and gives a concrete input or state that produces the wrong result. Findings that two reviewers reported from different sides are merged into one entry that names both anchors.

Severity has three levels. High means data loss, a crash, or an unbounded hang reachable from user, file or command input. Medium means a wrong result, a protection bypass, or two paths that should agree and do not, in a case a user reaches in ordinary work. Low means a wrong result in a rare case, a cosmetic inconsistency, or a rule violation with no user-visible effect yet.

Nine reviewers covered the repository: one reviewed the last wave merged into main (the diff from 5efda7c to HEAD, 30 files) and eight reviewed one crate group each. Each read its scope line by line, verified every candidate against the source and discarded what it could not confirm. None built or ran the code.

## Summary

Total 66 findings: 8 high, 30 medium, 28 low.

Table 1. Findings by area.

| Area | Scope | High | Medium | Low | Total |
|---|---|---|---|---|---|
| Last wave (transform, open policy) | git diff 5efda7c..HEAD | 0 | 6 | 9 | 15 |
| Engine commands | crates/engine/src/cmd | 2 | 6 | 7 | 15 |
| Engine core | crates/engine except cmd | 1 | 1 | 2 | 4 |
| Canvas | crates/ui-egui/src/canvas | 1 | 3 | 2 | 6 |
| Chrome, dock, panes, control | crates/ui-egui except canvas | 1 | 4 | 3 | 8 |
| Diagram, routing, layout | crates/diagram, routing, layout | 0 | 1 | 1 | 2 |
| Formats, drawio, mermaid, stencils | crates/formats, drawio, mermaid, stencils | 2 | 4 | 1 | 7 |
| Notation, analysis, codegen | crates/notation, analysis, codegen, patterns | 1 | 2 | 1 | 4 |
| Model, MCP, apps | crates/model, mcp, macos-open, apps | 0 | 3 | 2 | 5 |

Table 2. Findings in rank order.

| Id | Severity | Location | Summary |
|---|---|---|---|
| F01 | high | crates/ui-egui/src/canvas/interact/crop.rs:58 | A crop handle drag calls f64::clamp with min greater than max, which panics outside Session::run and brings the app down. |
| F02 | high | crates/ui-egui/src/lib.rs:1192 | On a single-window host (the web build) File > New, Mod+N and a dropped or picked .vizcraft file replace an edited document with no prompt and no undo, while the gallery's Create on the same host refuses to do so. |
| F03 | high | crates/engine/src/cmd/template.rs:494 | template.new and example.open replace the document before a step that can still fail, and Session::run's error path restores only the document and selection, so a failed run leaves the user's restored document marked clean with no undo history and no file path. |
| F04 | high | crates/engine/src/cmd/arrange.rs:144 | Every arrange.* command (align, distribute, autoSpace, flip, rotate, setBounds, group, ungroup) moves, resizes or rotates shapes without any shape lock, document protection or layer lock check, while the equivalent shape.* commands refuse. |
| F05 | high | crates/engine/src/session.rs:696 | macro.run is not in is_transaction, so one macro run records two undo steps and two log entries, and the sync passes run twice over the whole macro. |
| F06 | high | crates/mermaid/src/flowchart.rs:475 | A flowchart linkStyle, and the class statement, multiply two unbounded comma lists from one statement, so a 2 MB input can allocate terabytes or spin for hours. |
| F07 | high | crates/formats/src/table.rs:199 | The XLSX zip-bomb guard trusts the sizes the file declares in its central directory, and nothing downstream bounds the real inflated size, so a small workbook can inflate without limit. |
| F08 | high | crates/analysis/src/reliability.rs:1022 | outcomes enumerates every path of the event tree with no visited set, so a Success/Failure loop between branch points makes the walk exponential in the element count and analyze.outcomes never returns. |
| F09 | medium | crates/ui-egui/src/dock/activity.rs:45 | The Activity tab's mapping from log rows to undo steps is wrong after an undo followed by a new command, so "Undo to before this" undoes the wrong number of steps and the reachable and undone marks are off. |
| F10 | medium | crates/ui-egui/src/panes/model_tree.rs:505 | In-place rename in the Model Tree ends by itself two frames after it starts whenever the element's row and the shape row that draws it are both on screen, because both rows draw a rename editor for the same id and fight over focus. |
| F11 | medium | crates/ui-egui/src/spelling_dialog.rs:233 | Change All with a replacement equal to the flagged word, the default when the checker has no suggestions, runs proof.check over the whole document 2000 times before stopping and freezes the UI. |
| F12 | medium | crates/ui-egui/src/control.rs:145 | ui.set of any single field discards every transient dock state (Problems check, Search hits and query, Tables view, Nets selection, Analysis options and Re-run, Generate fields and log) because it round-trips the whole UiState through serde and DockLayout.state is serde(skip). |
| F13 | medium | crates/engine/src/cmd/shape.rs:144 | shape.move, shape.resize, shape.rotate, shape.delete and lane.remove check locks only on the named ids but act on every descendant of a group, while shape.transform checks the descendants and refuses; a multi-selection holding a group with one locked member therefore previews the whole move and is refused on release, whereas the same group dragged alone moves the locked member silently. |
| F14 | medium | crates/engine/src/cmd/edit.rs:230 | edit.pasteSpecial {as: "text"} writes shape and connector text directly, bypassing the Text lock and document protection that shape.setText, connector.setText and the text.* commands enforce through put_text. |
| F15 | medium | crates/engine/src/cmd/page.rs:341 | page.duplicate renames shape and connector ids and remaps parents and glued ends, but leaves compartment slot members and callout targets pointing at the ids on the source page, unlike Fragment::insert which remaps both. |
| F16 | medium | crates/engine/src/cmd/page.rs:296 | page.delete deletes a background page and clears every page's reference to it without the backgrounds protection check, and deletes every shape on a page regardless of delete locks or shape protection. |
| F17 | medium | crates/engine/src/cmd/layer.rs:251 | layer.assign and layer.unassign fall back to the selection only when the target page index is 0, not when it is the active page, so with no ids on any other active page they fail with "select a shape first". |
| F18 | medium | crates/engine/src/cmd/connector.rs:98 | The glue checks are applied inconsistently across the paths that glue a connector end to a shape: connector.glue checks both the shape's noGlue behaviour and the layer's glue flag, connector.add checks only the layer, wire.draw checks only noGlue, and suggest.insertNext, suggest.insertConnect, relationship.create, table.toggle and sheet.balloon check neither. |
| F19 | medium | crates/engine/src/session.rs:499 | After file.open the page auto-size and autoroute refresh still run, comparing the freshly opened document against the previous session document, so the opened document can be silently re-routed or its pages grown while dirty stays false; the next save writes routes the user never changed. |
| F20 | medium | crates/diagram/src/arrange/rigid.rs:203 | A selected connector with one end glued to a shape outside the moved set has its waypoints cleared, and a net wire (avoid = false) is never re-routed by the routing pass, so its hand-placed bends are replaced by an obstacle-blind orthogonal elbow; shape.move left waypoints untouched. |
| F21 | medium | crates/ui-egui/src/canvas/interact/membership.rs:17 | commit_move picks shape.transform on ids.len() > 1, an id count rather than "anything glued at both ends moves", so one selected group whose members are joined by a bent wire is moved by shape.move and the bends stay behind, while selecting the two members directly carries them. |
| F22 | medium | crates/ui-egui/src/gallery.rs:563 | Examples go through CraftApp::open_document, so on a multi-window host the gallery's Replace and Save First buttons never replace the drawing, while the create tail treats the deferred result as an open in this window and turns on the Shape Data pane here. |
| F23 | medium | crates/diagram/src/arrange/rigid.rs:161 | On a module page a multi-item move always re-snaps each symbol's first pin and every carried bend and free end to the module while plain shapes get the raw delta, so a symbol and a plain shape dragged together drift apart by up to half a module whenever the delta is off-module; shape.move applied no such snap. |
| F24 | medium | crates/ui-egui/src/lib.rs:1043 | In the in-place branch open_document closes the File page only on Ok, but the status bar is drawn only outside the backstage, so a failed open from Recent or the Open dialog in an untouched or single-window app leaves the File page open with no visible error. |
| F25 | medium | crates/ui-egui/src/lib.rs:1186 | Several files dropped in one frame onto a window that already holds a document each call open_document, which overwrites the single Option<WindowRequest>, so only the last file opens in a new window and the others are silently lost after the status bar announced each of them. |
| F26 | medium | crates/ui-egui/src/canvas/interact/drag.rs:256 | A resize-handle drag on an aspect-locked shape previews an exact-ratio frame but commits w and h rounded independently, which the engine's aspect check refuses, so the release does nothing and reports an error. |
| F27 | medium | crates/ui-egui/src/canvas/interact/edit_text.rs:315 | Escape in the editor of a text box that was just created for the edit leaves an invisible empty shape on the page as a committed undo step instead of removing it. |
| F28 | medium | crates/ui-egui/src/canvas/interact/ink.rs:89 | The eraser marks strokes on a locked layer as about to be erased, then the single ink.erase for the sweep is refused, so nothing in the sweep is erased. |
| F29 | medium | crates/diagram/src/container.rs:1048 | A lane set whose heading position is Left or Right has its lanes placed below the whole set and the set grows by its lanes' total thickness on every refit, so each move or resize on the page makes it taller until MAX_EXTENT. |
| F30 | medium | crates/drawio/src/attrs.rs:29 | Attribute insertion is a linear scan per attribute, so one element with millions of attributes in a 50 MB .drawio file makes parsing quadratic and hangs the open. |
| F31 | medium | crates/formats/src/drawio.rs:133 | drawio::import never restores the nets a VizCraft export carried in vizcraftNets, so importing a VizCraft-written .drawio silently drops every wire's net membership and the nets' numbers and names. |
| F32 | medium | crates/formats/src/dbml.rs:702 | A one-line indexes { ... } block swallows its own closing brace, so the table's } is taken as the end of indexes and the following statements are parsed as columns of that table. |
| F33 | medium | crates/formats/src/pdf.rs:168 | Print layout draws the SVG into a box clamped to 14400 units, so a page or overhanging canvas wider than 14400 units prints scaled down and its tiles no longer line up with the plan. |
| F34 | medium | crates/notation/src/electrical_rules.rs:513 | electrical.mixedSymbols assumes IEC for any builder shape that names no standard, so a block-builder symbol on an ANSI or JIC page is reported as a symbol of another standard; the page variant the user chose is never consulted. |
| F35 | medium | crates/codegen/src/regions.rs:120 | carry silently drops the body of a protected region whose id appears twice in the file on disk, although the module promises every hand-written block is placed or appended with an issue. |
| F36 | medium | crates/mcp/src/diagram.rs:77 | The tools/list schema for open_xml, open_mermaid and open_csv says page defaults to the first page, but file.import fills the active page, and docs/mcp.md says active. |
| F37 | medium | crates/mcp/src/tools.rs:150 | The screenshot tool schema says page defaults to the first page, but a page render goes through file.export, which renders the active page; docs/mcp.md says active. |
| F38 | medium | apps/vizcraft-cli/src/main.rs:110 | std::env::args() panics on a command-line argument that is not valid Unicode, so a non-UTF-8 file path crashes the CLI and the desktop app before any subcommand runs. |
| F39 | low | crates/engine/src/session.rs:368 | mark_saved suppresses the undo step only for a directly run command; inside a batch the transaction compares documents itself and records the same change as an undo step, so page.setActive, link.go, navigation and file.save behave differently in the two paths. |
| F40 | low | crates/engine/src/session.rs:682 | Script recording pairs edit.redo with the last undone group by stack position, not by step count, so an undo and redo across a document-changing but non-recordable step (file.import) re-adds a recorded group that is not applied, and the saved script replays extra commands. |
| F41 | low | crates/engine/src/cmd/relationship.rs:133 | relationship.add, and the table link path, draw the new connector on the first page that shows both elements rather than the active page, and set the selection only when that page is index 0, so the selection can name a connector on a page the canvas is not showing. |
| F42 | low | crates/engine/src/cmd/file.rs:402 | file.import and page.set select the imported items only when the target page index is 0, although the target defaults to the active page, so an import into any other active page leaves nothing selected and an import into page 0 from another page clears the selection. |
| F43 | low | crates/engine/src/cmd/layer.rs:172 | layer.delete {deleteShapes: true} removes shapes through delete_on_page without the delete-lock and protection check and without re-anchoring comments, so delete-locked shapes go and comment anchors keep naming removed shapes. |
| F44 | low | crates/engine/src/cmd/page.rs:648 | page.fitToDrawing shifts every shape, free end, waypoint and guide on the page without checking position locks, document protection or locked layers. |
| F45 | low | crates/engine/src/cmd/compartment.rs:98 | Compartment edits grow a shape through fit_shape and write compartment text with only a layer check, bypassing the height lock and the Text lock that shape.resize and shape.setText enforce. |
| F46 | low | crates/engine/src/cmd/ladder.rs:283 | ladder.place subtracts the ladder's first rung number from an i64 made by a saturating float cast of the rung parameter without checked arithmetic, so rung -1e300 overflows: a panic in debug builds, caught as a command failure; a silent wrap in release. |
| F47 | low | crates/engine/src/cmd/datagraphic.rs:296 | Rebuilding a data graphic legend deletes every connector that touches a legend shape at either end, so a user's connector glued at one end to a legend box is removed outright, unlike every other removal path, which frees the glued end and keeps the connector. |
| F48 | low | crates/ui-egui/src/canvas/interact/keys.rs:87 | Arrow-key nudging of a multi-selection still runs shape.move (shapes and free ends only), while a pointer drag of the same selection commits shape.transform and carries the wire bends, so nudging and dragging the same selection end in different documents. |
| F49 | low | apps/vizcraft/src/main.rs:195 | WindowRequest::RunInNew opens a blank window before the open is attempted; when file.open fails the error lands in that new empty window, which stays open, while the requesting window has already closed its File page and reports "Opening X in a new window". |
| F50 | low | crates/ui-egui/src/canvas/interact/drag.rs:139 | A multi-item Drag::Move is previewed through rigid::apply even when the rounded delta is (0, 0), so on a module page every off-module symbol pin, routed waypoint and free end is drawn snapped onto the module, then finish commits nothing for a zero delta and the page snaps back. |
| F51 | low | crates/ui-egui/src/canvas/interact/drag.rs:189 | For a routed connector with hand-placed bends glued at both ends to shapes in a group move, resize or rotate, the preview transforms the bends and then autoroute::preview discards them, while the commit keeps the transformed bends, so the bends vanish during the drag and reappear on release. |
| F52 | low | crates/engine/src/cmd/transform.rs:82 | shape.transform has no identity guard and normalises the angle with rem_euclid(360), so a no-op request still runs rigid::apply: on a module page it snaps every symbol pin, bend and free end to the module; on any page coordinates drift by an ulp; a tiny negative angle becomes exactly 360 and a full turn moves far-away centres enough to trip the lock tolerance. |
| F53 | low | crates/ui-egui/src/canvas/interact/group.rs:211 | run_named wraps the lone shape.transform in edit.batch only to name the undo step, so an engine refusal reaches the status bar as "commands[0] (shape.transform) failed: bad parameters: ...; nothing was changed"; neither group::allowed nor drag::move_allowed consults layer locks or descendants, so the handles stay visible and the drag previews before that wrapped refusal. |
| F54 | low | crates/ui-egui/src/canvas/interact/group.rs:86 | allowed() hides all eight group resize handles when any scaling shape has a width or a height lock, but protect::refuses only rejects a Size change on the locked side and the single-shape path keeps the handles that change the free side, so a height-only lock needlessly removes the left and right edge handles from the group frame. |
| F55 | low | crates/ui-egui/src/menus.rs:613 | The open-beside policy is reachable only from gestures and has no ui.* command id, so the control channel's advertised Recent entry file.open {path}, dispatched by id, replaces the front document while the same click opens a new window; AGENTS.md states that a thing that can be clicked but not run by id is not finished. |
| F56 | low | crates/ui-egui/src/canvas/interact | Twelve further items verified in the last-wave review and cut by its cap: duplicated helpers and small edge cases in the multi-selection transform. |
| F57 | low | crates/ui-egui/src/canvas/interact/tools.rs:54 | A press at the position of a resize or rotate handle that the overlay hides because of locks or document protection starts a dead Resize or Rotate drag instead of moving the shape. |
| F58 | low | crates/ui-egui/src/canvas/interact/mod.rs:152 | Pressing the pan modifier after a move or resize drag has begun pans the view and continues the shape drag at the same time, and the release commits the shape change. |
| F59 | low | crates/ui-egui/src/panes/inspector.rs:629 | The Inspector's X, Y, Width and Height drag values, the Format tab's line width and font size drag values and its colour pickers run a document command on every frame of a drag, producing one undo step per frame instead of one per gesture. |
| F60 | low | crates/ui-egui/src/dock/generate.rs:164 | Dock code that calls session.run directly leaves the engine's status text in session.status, which the next unrelated command then shows in the status bar. |
| F61 | low | crates/ui-egui/src/lib.rs:881 | ui.tab stores any string as the ribbon tab, so ui.inspect and the saved preferences report a tab that is not drawn. |
| F62 | low | crates/diagram/src/container.rs:536 | content_rect takes min(width, height) of the heading band as the band's thickness, which is too small whenever the frame is narrower than the band is thick, so the content area disagrees with heading_rect. |
| F63 | low | crates/mermaid/src/architecture.rs:213 | Edge validation is a linear search over all services and junctions for every edge end, twice, so a diagram at the node and edge caps costs about 1e10 string comparisons. |
| F64 | low | crates/codegen/src/names.rs:182 | The uniqueness counter is appended after sanitising, so a de-duplicated identifier can exceed the target's max_len and bypass its reserved-word check. |
| F65 | low | crates/model/src/types.rs:469 | Model::remove leaves Model.patterns untouched, so after a delete a PatternInstance.members entry names an element or relationship that no longer exists, contradicting the invariant pattern.rs documents. |
| F66 | low | apps/vizcraft/src/main.rs:521 | A --control value that is not a port number is silently dropped and also discards the port from VIZCRAFT_CONTROL_PORT, so the app starts with no control channel and no message. |

## Findings

### F01. crates/ui-egui/src/canvas/interact/crop.rs:58

Severity: high. Area: Canvas.

A crop handle drag calls f64::clamp with min greater than max, which panics outside Session::run and brings the app down.

Failure scenario. cropped() computes l = to.x.clamp(fx0, r - 1.0) and r = to.x.clamp(l + 1.0, fx1) where (fx0, fx1) come from the current crop. Rust's f64::clamp asserts min <= max. Two routes reach it. (1) picture.crop {crop: {x: 0.99995, w: 0.00005}} is accepted by the engine (picture.rs lines 216 to 224 refuse only w <= 0); on a picture narrower than 20 units fx1 < 1.0, so a Crop-tool press near a right-side handle and any drag runs clamp(1.0, 0.95). (2) A loaded file with a picture of w < 1 or h < 1 (Shape::sanitize has no floor) runs clamp(0.0, -1.0) from the top-left handle. Path: crop::pointer in interact/mod.rs line 130 to cropped at line 104.

### F02. crates/ui-egui/src/lib.rs:1192

Severity: high. Area: Chrome, dock, panes, control.

On a single-window host (the web build) File > New, Mod+N and a dropped or picked .vizcraft file replace an edited document with no prompt and no undo, while the gallery's Create on the same host refuses to do so.

Failure scenario. The web host sets multi_window = false (apps/vizcraft-web/src/web.rs line 42). With session.dirty = true, Mod+N or File > New goes through menus::run_item to CraftApp::new_window (lib.rs lines 1192 to 1198), which with !multi_window runs file.new; the engine's new calls set_document, which clears history and redo and sets dirty = false. A document dropped on the page or picked from the web picker reaches drain_inbox (lines 1322 to 1328), which calls session.set_document unconditionally. gallery::create (gallery.rs lines 531 to 535) refuses to replace a dirty document and shows Save First / Replace / Cancel, so the two New paths disagree.

### F03. crates/engine/src/cmd/template.rs:494

Severity: high. Area: Engine commands.

template.new and example.open replace the document before a step that can still fail, and Session::run's error path restores only the document and selection, so a failed run leaves the user's restored document marked clean with no undo history and no file path.

Failure scenario. Also anchored at crates/engine/src/session.rs line 525. Open a file, edit it (dirty = true), run template.new {template, path: "/no/such/dir/x.vizcraft"}. create() passes the dry run, calls s.set_document(Document::new()) (line 494), which clears history and redo and sets dirty = false (session.rs lines 349 to 362), and sets s.path = None (line 495). The transaction succeeds, then s.run("file.save", {path}) (line 501) fails on std::fs::write and the error propagates. Session::run's Err branch (session.rs lines 523 to 527) sets self.doc = before_doc and self.sel = before_sel only. The old document is back, dirty is false, history is empty and path is None, so Close will not prompt and the edits are lost. example.open (template.rs lines 73 and 451 to 453) follows the same path. file.new and file.open cannot fail after set_document, so template.new and example.open are the reachable triggers.

### F04. crates/engine/src/cmd/arrange.rs:144

Severity: high. Area: Engine commands.

Every arrange.* command (align, distribute, autoSpace, flip, rotate, setBounds, group, ungroup) moves, resizes or rotates shapes without any shape lock, document protection or layer lock check, while the equivalent shape.* commands refuse.

Failure scenario. shape.setLocks {ids: [a], position: true, width: true, rotation: true} or doc.protect {shapes: true}, then arrange.setBounds {id: a, x: 0, y: 0, w: 10, h: 10, angle: 45}: set_bounds (lines 144 to 165) calls ar::set_frame and ar::set_angle with no layer::ensure_unlocked or protect::check; the file imports neither. shape.resize checks Lock::Size at shape.rs line 189 and shape.rotate checks Lock::Rotation at line 215. arrange.align, distribute, autoSpace, flip and rotate (lines 71 to 142) likewise translate or turn locked shapes and shapes on locked layers. protect.rs lines 5 to 9 document that document protection refuses moving, sizing, turning and deleting every shape.

### F05. crates/engine/src/session.rs:696

Severity: high. Area: Engine core.

macro.run is not in is_transaction, so one macro run records two undo steps and two log entries, and the sync passes run twice over the whole macro.

Failure scenario. macro.rs line 194 calls s.transaction(&m.name, &steps) from inside the command body. txn.depth is 0, so the transaction is outer and commits with self.record(label, ...) at session.rs lines 631 to 638 (origin Script). Back in Session::run, manages_undo is false for "macro.run" (line 437; is_transaction at line 696 lists only edit.batch, script.run, template.new, template.addPage, example.open, fluid.powerUnit), changed is true, so lines 507 to 513 call self.record("Run Macro", ...) a second time and the sequence, glyph, bind and autoroute passes at lines 452 to 506 run again. Undo once restores the pre-macro document; undo again is a no-op step. macro.rs line 71 documents one transaction, one undo step; the test at lines 253 to 256 checks the document after one undo but not undo_depth. Severity is high because every macro user hits it on every run.

### F06. crates/mermaid/src/flowchart.rs:475

Severity: high. Area: Formats, drawio, mermaid, stencils.

A flowchart linkStyle, and the class statement, multiply two unbounded comma lists from one statement, so a 2 MB input can allocate terabytes or spin for hours.

Failure scenario. Builder::finish (lines 472 to 488) extends every edge's style with every prop for linkStyle default, and per index for LinkTarget::Indices. Neither the edge count (MAX_EDGES = 50,000), the index list (targets_word.split(','), line 308, uncapped; "0,0,0,..." is 2 bytes per index) nor the prop list (parse_style_props, no cap) is bounded against the others. 50,000 statements a-->b (300 KB) followed by linkStyle default with 1.5 MB of a:b, gives about 1.9e10 cloned StyleProps. statement lines 234 to 238 nest ids.split(',') inside classes.split(',') for the class statement: class a,a,... (500k) c,c,... (500k) runs 2.5e11 push_unique calls. MAX_INPUT_BYTES (2 MB) and the one-statement budget do not stop either.

### F07. crates/formats/src/table.rs:199

Severity: high. Area: Formats, drawio, mermaid, stencils.

The XLSX zip-bomb guard trusts the sizes the file declares in its central directory, and nothing downstream bounds the real inflated size, so a small workbook can inflate without limit.

Failure scenario. zip_inflated_size (lines 199 to 221) sums the uncompressed-size field of each central-directory header, and xlsx_grid (lines 229 to 237) refuses only when that sum exceeds MAX_INFLATED. A hostile file declares 1 KB per entry while its deflate stream for xl/worksheets/sheet1.xml inflates to gigabytes of row elements. calamine 0.36.1 opens the entry through zip 8.6.0, whose reader takes compressed_size bytes of input and never caps the decompressed output against uncompressed_size; calamine has no size check. The sheet is parsed into a Range in memory before the row cap at line 261 sees anything. The module comment at lines 6 and 7 promises the opposite.

### F08. crates/analysis/src/reliability.rs:1022

Severity: high. Area: Notation, analysis, codegen.

outcomes enumerates every path of the event tree with no visited set, so a Success/Failure loop between branch points makes the walk exponential in the element count and analyze.outcomes never returns.

Failure scenario. Initiator I with Success to branch point B, and B with both Success and Failure back to B (or B1 and B2 two ways; both satisfy twoBranches, and reliability.acyclic is a forbiddenCycle rule on reliability.Input only, so no rule objects). Lines 1017 to 1056: the stack DFS pops (at, f, depth); depth_cap = els.len() + 1 (line 1020) is the only bound; paths increments only when an Outcome is reached (lines 1027 to 1029); every branch point pushes all of out_by[at] (lines 1046 to 1054). Each pop of B at depth d pushes two B at d + 1 until depth_cap, so with 40 elements in scope the walk does about 2^42 pops. junk_fault_trees_never_panic uses at most 12 elements.

### F09. crates/ui-egui/src/dock/activity.rs:45

Severity: medium. Area: Chrome, dock, panes, control.

The Activity tab's mapping from log rows to undo steps is wrong after an undo followed by a new command, so "Undo to before this" undoes the wrong number of steps and the reachable and undone marks are off.

Failure scenario. Lines 43 to 51 assume the undone entries are the last redo entries of the log: top = len - redo, floor = top - undo; the button undoes top - i steps (lines 65 and 79 to 85). Session::record (session.rs lines 545 to 557) pushes a new log entry and clears redo without removing the undone entries, and undo and redo never touch the log. Run A, run B, undo (log [A, B], history [A], redo [B]), run C (log [A, B, C], history [A, C]). Now top = 3, floor = 1: row A is drawn unreachable although it is on the undo stack, row B is drawn reachable although it is not, and the button on B runs edit.undo twice, undoing C and then A.

### F10. crates/ui-egui/src/panes/model_tree.rs:505

Severity: medium. Area: Chrome, dock, panes, control.

In-place rename in the Model Tree ends by itself two frames after it starts whenever the element's row and the shape row that draws it are both on screen, because both rows draw a rename editor for the same id and fight over focus.

Failure scenario. draw_row computes item as the element id for an Element row and for a Shape row bound to that element (lines 478 to 484), and renaming is true for every row whose item equals the rename id (line 505), so both rows call draw_rename (lines 693 to 714) in one frame. Frame 1: both request focus, the second wins. Frame 2: the first editor requests focus again; the second now reports lost_focus, so line 706 commits element.rename with the untouched name and returns true, and line 510 clears the rename. The Model branch, the Diagrams branch and the page rows start open, so a small model-backed document shows both rows by default and double-click rename never works there.

### F11. crates/ui-egui/src/spelling_dialog.rs:233

Severity: medium. Area: Chrome, dock, panes, control.

Change All with a replacement equal to the flagged word, the default when the checker has no suggestions, runs proof.check over the whole document 2000 times before stopping and freezes the UI.

Failure scenario. show_current sets replacement = item.word when suggestions are empty (line 126). Change All (lines 229 to 247) finds the item, change_item splices the identical word back and runs shape.setText with unchanged text; the engine records nothing and returns Ok. refresh (lines 102 to 118) runs proof.check {all: true} again, finds the same word, and the loop repeats until MAX_CHANGE_ALL = 2000: 2000 full-document checks and 2000 setText commands from one click, then the report "Changed 2000 occurrence(s)". The loop should stop when the replacement equals the word or the document did not change.

### F12. crates/ui-egui/src/control.rs:145

Severity: medium. Area: Chrome, dock, panes, control.

ui.set of any single field discards every transient dock state (Problems check, Search hits and query, Tables view, Nets selection, Analysis options and Re-run, Generate fields and log) because it round-trips the whole UiState through serde and DockLayout.state is serde(skip).

Failure scenario. control::set (lines 143 to 154) serialises app.prefs(), replaces one pointer, deserialises a fresh UiState and calls apply_prefs, which assigns self.ui = ui (lib.rs lines 662 to 674). DockLayout's state field is #[serde(skip)] (dock/mod.rs lines 110 to 112), so the rebuilt layout carries DockState::default(). ui.analysis {live: true, depth: 3} followed by ui.set {path: "panes.leftWidth", value: 300} leaves analysis.live false, depth None and last None, so Re-run is disabled and live re-runs stop; Problems, Search and Generate state is gone likewise.

### F13. crates/engine/src/cmd/shape.rs:144

Severity: medium. Area: Engine commands.

shape.move, shape.resize, shape.rotate, shape.delete and lane.remove check locks only on the named ids but act on every descendant of a group, while shape.transform checks the descendants and refuses; a multi-selection holding a group with one locked member therefore previews the whole move and is refused on release, whereas the same group dragged alone moves the locked member silently.

Failure scenario. Also anchored at crates/engine/src/cmd/transform.rs line 92. Group a and b, then shape.setLocks {ids: [b], position: true, delete: true}. shape.move {id: g, x: 100, y: 100}: ensure_unlocked and protect::check see [g] only (shape.rs lines 144 and 145), then arrange::translate expands with_descendants and moves b. shape.delete {ids: [g]}: Lock::Delete is checked on [g] (line 133) while delete_on_page expands descendants and removes b. In the canvas, move_allowed([G, b]) at drag.rs line 60 reads only the named ids, the preview shows everything moving, and on release shape.transform's protect::check per descendant fails with "commands[0] (shape.transform) failed: bad parameters: m1 is locked against moving; nothing was changed".

### F14. crates/engine/src/cmd/edit.rs:230

Severity: medium. Area: Engine commands.

edit.pasteSpecial {as: "text"} writes shape and connector text directly, bypassing the Text lock and document protection that shape.setText, connector.setText and the text.* commands enforce through put_text.

Failure scenario. shape.setLocks {ids: [a], text: true} or doc.protect {shapes: true}, edit.copy of a shape with text, then edit.pasteSpecial {as: "text", ids: [a]}: paste_special calls only layer::ensure_unlocked (line 219) and sets sh.text = t.clone() (lines 230 to 233). shape.setText on the same shape goes through text::put_text, which starts with protect::check(s, [id], Lock::Text) (text.rs line 230) and is refused.

### F15. crates/engine/src/cmd/page.rs:341

Severity: medium. Area: Engine commands.

page.duplicate renames shape and connector ids and remaps parents and glued ends, but leaves compartment slot members and callout targets pointing at the ids on the source page, unlike Fragment::insert which remaps both.

Failure scenario. A page with shape s1 holding a slot compartment whose members are [s2] and a callout whose container.target is s1. page.duplicate replaces ids (lines 341 to 345), remaps sh.parent (lines 346 to 348) and connector ends (lines 354 to 360), but never touches sh.compartments[].members or sh.container.target, so the copy's slot lists s2 on another page and the copied callout points at the original s1. Fragment::insert in engine/src/fragment.rs remaps both fields, so paste and page duplication produce different documents from the same content; later compartment.set or compartment.remove on the copy act on ids not on its page.

### F16. crates/engine/src/cmd/page.rs:296

Severity: medium. Area: Engine commands.

page.delete deletes a background page and clears every page's reference to it without the backgrounds protection check, and deletes every shape on a page regardless of delete locks or shape protection.

Failure scenario. doc.protect {backgrounds: true}, then page.delete {page: "Background-1"}: delete() (lines 296 to 330) has no protect::backgrounds call (the only caller is set_background at line 564) and sets p.background = None on every page that used it (lines 308 to 312), the very change set_background refuses. With shape.setLocks {delete: true} on a shape or doc.protect {shapes: true}, page.delete removes the whole page (line 307) with no protect::check, while shape.delete of the same shape is refused.

### F17. crates/engine/src/cmd/layer.rs:251

Severity: medium. Area: Engine commands.

layer.assign and layer.unassign fall back to the selection only when the target page index is 0, not when it is the active page, so with no ids on any other active page they fail with "select a shape first".

Failure scenario. page.add, select a shape on the new page (index 1), layer.add {name: "L"}, then layer.assign {layer: "L"}: page_index returns active_index() = 1 (lines 90 to 93), shape_ids matches None if i == 0 (line 251) only for index 0, so out stays empty and the command errors at lines 254 and 255 although the doc comment at lines 235 and 236 says the selection is used when the page is the active one.

### F18. crates/engine/src/cmd/connector.rs:98

Severity: medium. Area: Engine commands.

The glue checks are applied inconsistently across the paths that glue a connector end to a shape: connector.glue checks both the shape's noGlue behaviour and the layer's glue flag, connector.add checks only the layer, wire.draw checks only noGlue, and suggest.insertNext, suggest.insertConnect, relationship.create, table.toggle and sheet.balloon check neither.

Failure scenario. shape.setBehavior {ids: [a], noGlue: true}: connector.glue {id: c, end: "end", shape: a} is refused by protect::check(Lock::Glue) (connector.rs line 182), but connector.add {from: {shape: a}, to: {x, y}} succeeds because parse_end calls only layer::ensure_gluable (line 53) and add() never calls protect::check (lines 96 to 135). Conversely layer.set {layer: L, glue: false} with a on L: connector.add and connector.glue refuse, but wire.draw glues anyway since end_net checks only Lock::Glue (wire.rs lines 91 to 98). suggest.insertNext and insertConnect (suggest.rs lines 152 to 154), relationship.create (line 117), table.toggle (line 374) and sheet.balloon (line 404) build the glued end with no check.

### F19. crates/engine/src/session.rs:499

Severity: medium. Area: Engine core.

After file.open the page auto-size and autoroute refresh still run, comparing the freshly opened document against the previous session document, so the opened document can be silently re-routed or its pages grown while dirty stays false; the next save writes routes the user never changed.

Failure scenario. file.open calls set_document (file.rs line 74), which sets no_undo. The sync pass at line 448 honours no_undo, but the second changed test at lines 495 to 498 compares the new document with the old session document, and lines 499 to 506 run page::auto_size and autoroute::refresh(&mut self.doc, &before_doc) regardless. In autoroute.rs lines 477 to 489, stale() marks every routed connector stale when the old document has no page of that id or a different shape count, and keep_of re-routes every connector whose waypoints look router-made. Neither touch() nor dirty is set, so the session no longer equals the file on disk, close discards the difference without a prompt. The round-trip test at tests.rs lines 361 to 379 opens into a blank session whose sample has no waypoints.

### F20. crates/diagram/src/arrange/rigid.rs:203

Severity: medium. Area: Last wave (transform, open policy).

A selected connector with one end glued to a shape outside the moved set has its waypoints cleared, and a net wire (avoid = false) is never re-routed by the routing pass, so its hand-placed bends are replaced by an obstacle-blind orthogonal elbow; shape.move left waypoints untouched.

Failure scenario. Control Circuit sample: a marquee that touches contact K1 and crosses the first segment of its bent wire selects [K1, wire]. Dragging K1 now runs shape.transform (ids.len() > 1); rigid::apply takes the one-end-inside branch for the wire, clears c.waypoints and lists it in rerouted; routing_policy::routes returns avoid = false for a net wire drawn with points, so autoroute::refresh skips it and path_through draws orthogonal_points, an elbow that ignores shapes and other wires. Before the change shape.move moved K1 and kept the bends.

### F21. crates/ui-egui/src/canvas/interact/membership.rs:17

Severity: medium. Area: Last wave (transform, open policy).

commit_move picks shape.transform on ids.len() > 1, an id count rather than "anything glued at both ends moves", so one selected group whose members are joined by a bent wire is moved by shape.move and the bends stay behind, while selecting the two members directly carries them.

Failure scenario. Shapes s3 and s4 joined by wire w with a bend at (150, 40). Select s3 and s4, drag by (100, 50): rigid::apply moves the bend to (250, 90). Group them, drag the group by (100, 50): shape.move via arrange::translate moves the members, shape.rs touches only End::Free ends, the bend stays at (150, 40) and the wire doubles back through it. The same divergence hits shape.move from MCP, ladder.place and container.drop batches. The root fix is for arrange::translate to carry the bends of connectors glued at both ends into the moved set, which removes the ids.len() > 1 forks in membership.rs, drag.rs line 139 and group.rs line 170.

### F22. crates/ui-egui/src/gallery.rs:563

Severity: medium. Area: Last wave (transform, open policy).

Examples go through CraftApp::open_document, so on a multi-window host the gallery's Replace and Save First buttons never replace the drawing, while the create tail treats the deferred result as an open in this window and turns on the Shape Data pane here.

Failure scenario. Desktop app, edited unsaved drawing, Help > Examples, pick a card: the notice offers Save First, Replace or Cancel. Replace calls create(app, id, true), which skips the gallery's dirty check (line 531); line 563 calls open_document, lib.rs line 1033 computes untouched = false, queues WindowRequest::RunInNew and returns Ok({requested: true}); the example opens in a second window and the dirty drawing stays. Save First saves and then also goes to a new window. Lines 568 to 574 then close the gallery and enable the Shape Data pane in the window whose document did not change. Mode::New templates still run template.new in place, so the two gallery modes behave differently behind an identical notice. The kittest harness has multi_window = false, so no test sees it.

### F23. crates/diagram/src/arrange/rigid.rs:161

Severity: medium. Area: Last wave (transform, open policy).

On a module page a multi-item move always re-snaps each symbol's first pin and every carried bend and free end to the module while plain shapes get the raw delta, so a symbol and a plain shape dragged together drift apart by up to half a module whenever the delta is off-module; shape.move applied no such snap.

Failure scenario. Control Circuit sample (module iso2_5mm). Select contact K1 and a plain text label. Alt-drag by step * 0.4: snap_move returns early (drag.rs line 359), rigid::apply moves both centres by the raw delta, then snap_first_pin shifts K1 back onto the module while the label keeps the raw delta, so their offset changes by step * 0.4. With Alt off, snap_targets sets grid, guides, rects and points but never a module target, so the move snaps to the zoom grid or a guide. K1 dragged alone lands off-module; K1 plus any second item lands on it. group::preview and shape.transform both pass module_step with no way to pass None.

### F24. crates/ui-egui/src/lib.rs:1043

Severity: medium. Area: Last wave (transform, open policy).

In the in-place branch open_document closes the File page only on Ok, but the status bar is drawn only outside the backstage, so a failed open from Recent or the Open dialog in an untouched or single-window app leaves the File page open with no visible error.

Failure scenario. New untouched window (or the web build), File page open, click a Recent entry whose file was deleted: self.run fails, backstage stays true (lines 1042 to 1045), the error goes to status_msg with a 6 s lifetime, which backstage::show never draws (lib.rs lines 1257 to 1275 draw the status bar only in the else branch). The user sees nothing happen and the message has expired by the time they leave the File page. The doc comment says "The File page closes either way". Before the change backstage.rs closed the page unconditionally.

### F25. crates/ui-egui/src/lib.rs:1186

Severity: medium. Area: Last wave (transform, open policy).

Several files dropped in one frame onto a window that already holds a document each call open_document, which overwrites the single Option<WindowRequest>, so only the last file opens in a new window and the others are silently lost after the status bar announced each of them.

Failure scenario. Window W has a saved document. Drop a.vizcraft and b.vizcraft together: the loop at lines 1185 and 1186 calls open_document twice; the first sets window_request = Some(RunInNew a), the second replaces it with b. The host takes the slot once per frame (windows.rs line 165, main.rs line 299): one new window with b, and a never opens. Command-line open_files (main.rs line 230) opens each file in its own window, so the two routes disagree.

### F26. crates/ui-egui/src/canvas/interact/drag.rs:256

Severity: medium. Area: Canvas.

A resize-handle drag on an aspect-locked shape previews an exact-ratio frame but commits w and h rounded independently, which the engine's aspect check refuses, so the release does nothing and reports an error.

Failure scenario. resize_frame (lines 72 to 100) scales w and h by one factor when locks.aspect is set, and preview_page draws that unrounded frame. finish (line 256) sends shape.resize with round(w), round(h). protect.rs lines 82 to 88 refuse when |w/h - sh.w/sh.h| > 1e-3 times the ratio. A 30 by 20 shape with the aspect lock, corner dragged so k = 1.047: preview 31.4 by 20.93, commit 31 by 21, ratio 1.476 versus 1.5, refused with "is locked against resizing". Most drags land on a non-exact pair.

### F27. crates/ui-egui/src/canvas/interact/edit_text.rs:315

Severity: medium. Area: Canvas.

Escape in the editor of a text box that was just created for the edit leaves an invisible empty shape on the page as a committed undo step instead of removing it.

Failure scenario. Text tool click on empty page: select.rs line 211 to tools::add_text_box runs shape.addTextBox (fill.a = 0 and line.a = 0) and start_editing_new sets created = true (line 177). Escape takes text_overlay lines 314 to 318: editing = None and return; the created-and-empty branch of commit_editing (lines 187 to 190, which runs shape.delete) is never reached. keys.rs line 26 returns early while editing is Some. Each Text-tool click followed by Escape adds one invisible shape and an undo entry; the module doc at line 56 says "left empty, it goes".

### F28. crates/ui-egui/src/canvas/interact/ink.rs:89

Severity: medium. Area: Canvas.

The eraser marks strokes on a locked layer as about to be erased, then the single ink.erase for the sweep is refused, so nothing in the sweep is erased.

Failure scenario. strokes_at (lines 84 to 93) filters only on is_stroke and shape_visible, not on layer lock, so a locked-layer stroke is collected into the erase set and drawn in red by the preview (lines 194 to 200). commit (line 172) runs one ink.erase with all ids; engine ink.rs line 93 calls layer::ensure_unlocked on the whole list and errors, so the unlocked strokes swept in the same drag are not erased either.

### F29. crates/diagram/src/container.rs:1048

Severity: medium. Area: Diagram, routing, layout.

A lane set whose heading position is Left or Right has its lanes placed below the whole set and the set grows by its lanes' total thickness on every refit, so each move or resize on the page makes it taller until MAX_EXTENT.

Failure scenario. File: horizontal lane set (w 760, h 172, role laneSet, headingPosition left, headingSize 32) with one lane of h 140. heading_rect (lines 498 to 512) returns Rect::new(0, 0, t, h) for a side heading, whose height() is the full set height. lanes_origin (lines 1047 to 1051) takes that height as the title band, so oy = set.y + 172. layout_lanes (lines 1056 to 1095) places the lane at oy, below the set, and sets s.h = 312. layout_lanes runs from container::refit, which arrange::refit_groups calls after every translate, rotate, flip, set_frame, group and add_lane, so the next refit sees 312 and sets 452. The engine's container.heading refuses headingPosition on lane sets, but the model loads the field leniently and Container::sanitize (lines 239 to 252) does not correct it. phase_positions and after_set_frame read the same height.

### F30. crates/drawio/src/attrs.rs:29

Severity: medium. Area: Formats, drawio, mermaid, stencils.

Attribute insertion is a linear scan per attribute, so one element with millions of attributes in a 50 MB .drawio file makes parsing quadratic and hangs the open.

Failure scenario. xml::element_from (xml.rs lines 154 to 170) calls attrs.push(key, value) for every attribute; push is set (attrs.rs lines 29 to 37), which scans all earlier pairs. A file under MAX_FILE_BYTES holding one mxCell with five million empty attributes performs about 1.25e13 string comparisons before any cell cap is reached, since it is one element. with_checks(false) (line 157) also disables quick-xml's own duplicate check.

### F31. crates/formats/src/drawio.rs:133

Severity: medium. Area: Formats, drawio, mermaid, stencils.

drawio::import never restores the nets a VizCraft export carried in vizcraftNets, so importing a VizCraft-written .drawio silently drops every wire's net membership and the nets' numbers and names.

Failure scenario. save writes the nets as JSON in the page attribute vizcraftNets and each segment's net as vizcraftNet. load (lines 77 to 100) calls take_nets (lines 105 to 115) to put them back, but import (lines 133 to 154), which file.import uses (engine/src/io.rs line 113), only calls read::page_from_model, leaving the attribute inert in page.ext and doc.model.nets untouched. Document::sanitize then clears every Connector::net that names an unknown net, which the crate's test foreign_flips_read_as_orientations documents. The next save discards the stale attribute, so the nets are gone for good.

### F32. crates/formats/src/dbml.rs:702

Severity: medium. Area: Formats, drawio, mermaid, stencils.

A one-line indexes { ... } block swallows its own closing brace, so the table's } is taken as the end of indexes and the following statements are parsed as columns of that table.

Failure scenario. indexes (lines 687 to 724) reads each entry with p.skip_line() (line 702), which stops only at a newline, unlike the column path which stops at }. For Table a { id int; indexes { (id) [unique] } } followed by Table b { x int }, the entry line consumes "(id) [unique] }", the table's } then closes indexes, and table() continues: Table becomes a column named Table with type b, x int becomes a column of a, and Table b is never created. The rendered index line also carries the stray brace, so the file re-exports in the same broken shape.

### F33. crates/formats/src/pdf.rs:168

Severity: medium. Area: Formats, drawio, mermaid, stencils.

Print layout draws the SVG into a box clamped to 14400 units, so a page or overhanging canvas wider than 14400 units prints scaled down and its tiles no longer line up with the plan.

Failure scenario. print_pages computes tree_size = page_size(tree width, tree height) (line 168), and page_size (lines 211 to 214) clamps each side to 1.0..=MAX_SIDE_PT (14400). The tree size is in page units (up to MAX_EXTENT 1e6), not points, so a 20,000-unit drawing is handed to draw_svg as a 14400-wide box and drawn at 0.72 of its size, while the scale and centring offsets at lines 172 to 176 assume the full width. export_pages (lines 134 to 138) scales by k before calling page_size.

### F34. crates/notation/src/electrical_rules.rs:513

Severity: medium. Area: Notation, analysis, codegen.

electrical.mixedSymbols assumes IEC for any builder shape that names no standard, so a block-builder symbol on an ANSI or JIC page is reported as a symbol of another standard; the page variant the user chose is never consulted.

Failure scenario. A page in variant electrical.ansi or electrical.jic with contacts and coils whose glyphs carry standard ieee315 or jic, plus one electrical.ioModule, ic, flipFlop or plug (the four block-builder glyphs with no standard param). symbol_family (lines 502 to 514) reads only params and falls to (None, _) => Some(true), meaning IEC. In mixed_symbols (lines 394 to 416) both sets are non-empty and lines 412 to 415 push "1 symbols on this page are drawn to another standard than the American symbols around them". The rule is in every electrical ruleset, so io and panel pages always get the false warning.

### F35. crates/codegen/src/regions.rs:120

Severity: medium. Area: Notation, analysis, codegen.

carry silently drops the body of a protected region whose id appears twice in the file on disk, although the module promises every hand-written block is placed or appended with an issue.

Failure scenario. catalog.md on disk holds the begin and end markers for region e7 twice with different text (a copy-pasted Notes entry, or a merge that duplicated a section). blocks returns both. Line 93 bodies.entry(b.id).or_insert keeps only the first body; the placement loop (lines 108 to 113) places it once and inserts e7 into placed; the orphan loop at line 120 skips every block whose id is in placed, so the second body is neither written nor appended and no codegen.orphanRegion issue is raised. decide then writes the generated text (writer.rs line 713) and the second hand-written text is gone.

### F36. crates/mcp/src/diagram.rs:77

Severity: medium. Area: Model, MCP, apps.

The tools/list schema for open_xml, open_mermaid and open_csv says page defaults to the first page, but file.import fills the active page, and docs/mcp.md says active.

Failure scenario. A multi-page document with page 1 active. An agent reads the schema at line 77 ("default: the first page"), calls open_mermaid without page expecting page 0 to be replaced, and replace: true wipes the active page instead: engine file.rs lines 325 and 326 use doc.active_index() when page is None. docs/mcp.md line 187 states the active-page behaviour, so the schema is the wrong one of the two.

### F37. crates/mcp/src/tools.rs:150

Severity: medium. Area: Model, MCP, apps.

The screenshot tool schema says page defaults to the first page, but a page render goes through file.export, which renders the active page; docs/mcp.md says active.

Failure scenario. Headless or window: false with no page: tools.rs lines 515 to 525 send file.export {format: png} with no page; engine file.rs lines 127 and 128 resolve pages with the default "active" for non-PDF formats, and the engine test at lines 495 to 498 asserts it. After page.setActive {page: 1} an agent that trusted the schema gets page 1, not page 0.

### F38. apps/vizcraft-cli/src/main.rs:110

Severity: medium. Area: Model, MCP, apps.

std::env::args() panics on a command-line argument that is not valid Unicode, so a non-UTF-8 file path crashes the CLI and the desktop app before any subcommand runs.

Failure scenario. vizcraft-cli inspect with an argument containing invalid UTF-8 bytes (possible on Linux, and on Windows with unpaired surrogates): line 110 std::env::args().skip(1).collect() panics on the first non-Unicode argument; nothing catches it, so the process aborts with a panic message instead of exit code 2. apps/vizcraft/src/main.rs line 518 has the same call, so the desktop app crashes at startup too. std::env::args_os() with to_string_lossy or an explicit usage error is the non-panicking path.

### F39. crates/engine/src/session.rs:368

Severity: low. Area: Engine core.

mark_saved suppresses the undo step only for a directly run command; inside a batch the transaction compares documents itself and records the same change as an undo step, so page.setActive, link.go, navigation and file.save behave differently in the two paths.

Failure scenario. edit.batch {commands: [{command: "page.setActive", params: {page: 1}}]}: the inner run sets no_undo via mark_saved, so run() skips record. The transaction commit at lines 631 to 643 checks only txn.replaced and self.doc != before_doc, which is true because active_page changed, so record("Batch") pushes an undo step. Directly, page.setActive makes no step (page.rs test at lines 880 to 894). edit.batch [file.save] turns the metadata stamp into an undo step whose undo marks the saved document dirty.

### F40. crates/engine/src/session.rs:682

Severity: low. Area: Engine core.

Script recording pairs edit.redo with the last undone group by stack position, not by step count, so an undo and redo across a document-changing but non-recordable step (file.import) re-adds a recorded group that is not applied, and the saved script replays extra commands.

Failure scenario. script.recordStart; shape.add (entry n = 1); file.import of a Mermaid text (changes the document so a step is recorded, steps = 2, but recordable is false at line 702 so no entry); shape.add (entry n = 3). edit.undo: steps = 2, note() splits off entry 3 into undone. edit.undo again: steps = 1, nothing pushed. edit.redo: steps = 2, lines 683 to 685 pop the shape.add entry back although it is not applied. A further shape.add clears undone. script.recordStop returns [A, B, C] while the document holds A, import, C.

### F41. crates/engine/src/cmd/relationship.rs:133

Severity: low. Area: Engine commands.

relationship.add, and the table link path, draw the new connector on the first page that shows both elements rather than the active page, and set the selection only when that page is index 0, so the selection can name a connector on a page the canvas is not showing.

Failure scenario. Elements A and B each have views on page 0 and page 2; page 2 is active. relationship.add scans pages from 0 (lines 106 to 111), pushes the connector onto page 0 (lines 130 and 131) and, because page == 0, selects it (lines 132 to 134): the selection holds an id absent from the active page, and the connector is not on the page shown. table.rs lines 370 to 386 use the same rule.

### F42. crates/engine/src/cmd/file.rs:402

Severity: low. Area: Engine commands.

file.import and page.set select the imported items only when the target page index is 0, although the target defaults to the active page, so an import into any other active page leaves nothing selected and an import into page 0 from another page clears the selection.

Failure scenario. With page 2 active, file.import {text: drawio xml}: place() uses active_index() = 2 (line 326), then import_into_session checks placed.index == 0 (line 402) and falls to prune_selection, contrary to the comment at lines 389 and 390. With page 2 active and page: 0, the imported ids are filtered by the active page (line 403) and select(s, []) empties the selection.

### F43. crates/engine/src/cmd/layer.rs:172

Severity: low. Area: Engine commands.

layer.delete {deleteShapes: true} removes shapes through delete_on_page without the delete-lock and protection check and without re-anchoring comments, so delete-locked shapes go and comment anchors keep naming removed shapes.

Failure scenario. shape.setLocks {ids: [a], delete: true}, comment.add pinned to a, assign a to layer L, then layer.delete {layer: "L", deleteShapes: true}: delete() checks only layer::locked (lines 174 to 176) and calls delete_on_page directly (line 178), skipping protect::check and the comment re-anchor that delete_items_heal performs (mod.rs lines 328 to 341).

### F44. crates/engine/src/cmd/page.rs:648

Severity: low. Area: Engine commands.

page.fitToDrawing shifts every shape, free end, waypoint and guide on the page without checking position locks, document protection or locked layers.

Failure scenario. shape.setLocks {ids: [a], position: true}, or layer.set {locked: true} on a's layer, then page.fitToDrawing: fit_to_drawing (lines 638 to 674) adds dx, dy to each shape (lines 648 to 651) with no protect::check or layer::ensure_unlocked, while shape.move of a is refused.

### F45. crates/engine/src/cmd/compartment.rs:98

Severity: low. Area: Engine commands.

Compartment edits grow a shape through fit_shape and write compartment text with only a layer check, bypassing the height lock and the Text lock that shape.resize and shape.setText enforce.

Failure scenario. shape.setLocks {ids: [a], height: true, text: true}, then compartment.add {id: a, text: long} or text.insertSymbol {ids: [a], compartment: c1, symbol: x}: the_shape and each() call only layer::ensure_unlocked (lines 60 and 112); fit_shape calls arrange::set_frame with a larger height (lines 98 to 104) with no Lock::Size check, and each() writes c.text (text.rs lines 443 to 446) with no Lock::Text check.

### F46. crates/engine/src/cmd/ladder.rs:283

Severity: low. Area: Engine commands.

ladder.place subtracts the ladder's first rung number from an i64 made by a saturating float cast of the rung parameter without checked arithmetic, so rung -1e300 overflows: a panic in debug builds, caught as a command failure; a silent wrap in release.

Failure scenario. ladder.place {shapes: [s], rung: -1e300}: line 264 computes n.round() as i64, which saturates to i64::MIN; line 283 evaluates (r - frame.settings.first) with first >= 0, so the subtraction overflows before div_euclid. The release profile sets no overflow checks, so release wraps to i64::MAX and the range check at line 284 then rejects it; debug and test builds panic. hostile_params_never_panic tries only rung 1e20, which saturates to i64::MAX.

### F47. crates/engine/src/cmd/datagraphic.rs:296

Severity: low. Area: Engine commands.

Rebuilding a data graphic legend deletes every connector that touches a legend shape at either end, so a user's connector glued at one end to a legend box is removed outright, unlike every other removal path, which frees the glued end and keeps the connector.

Failure scenario. A page has a legend group for graphic G; the user draws connector c from their own shape to a legend box. Any command that calls remove_legends(page, G) collects the legend root and its descendants (lines 286 to 294) and then retains only connectors touching none of them (line 296), dropping c. delete_on_page (mod.rs lines 299 to 312) keeps a connector with one glued end on a deleted shape and detaches it. key.rs lines 141 to 146 (clear) remove key shapes without detaching connectors glued to them.

### F48. crates/ui-egui/src/canvas/interact/keys.rs:87

Severity: low. Area: Last wave (transform, open policy).

Arrow-key nudging of a multi-selection still runs shape.move (shapes and free ends only), while a pointer drag of the same selection commits shape.transform and carries the wire bends, so nudging and dragging the same selection end in different documents.

Failure scenario. The circuit() fixture of tests/group_transform.rs: wire a to b with bends at (100, 10) and (100, 110); select a, b and the wire. Shift+Right ten times runs shape.move {dx: 10}: a and b move, both bends stay at x = 100 and the wire gets a jog. Dragging the same selection 100 right moves the bends to x = 200. commit_move is the only call site that picks shape.transform.

### F49. apps/vizcraft/src/main.rs:195

Severity: low. Area: Last wave (transform, open policy).

WindowRequest::RunInNew opens a blank window before the open is attempted; when file.open fails the error lands in that new empty window, which stays open, while the requesting window has already closed its File page and reports "Opening X in a new window".

Failure scenario. Window W holds a saved document. File > Open Recent > a deleted file: open_document sets the status, closes the backstage, queues RunInNew and returns Ok. Next host frame: open_window(ctx, "blank") pushes window N and focuses it; run_in_front runs file.open in N, which fails (file.rs line 72), and N shows the error; nothing closes N. Result: an untitled empty window in front showing the error, W behind it claiming success. Before the change the error showed in W and no window was created.

### F50. crates/ui-egui/src/canvas/interact/drag.rs:139

Severity: low. Area: Last wave (transform, open policy).

A multi-item Drag::Move is previewed through rigid::apply even when the rounded delta is (0, 0), so on a module page every off-module symbol pin, routed waypoint and free end is drawn snapped onto the module, then finish commits nothing for a zero delta and the page snaps back.

Failure scenario. Module page, two symbols selected, one earlier nudged by shape.move dx = 3, with a wire whose autoroute polyline is stored in waypoints. Press on a selected symbol and hold still: after egui's 0.8 s max click duration the drag starts with from = to; preview_page takes the ids.len() > 1 arm, rigid_of returns Rigid::moved(0, 0), group::preview snaps the symbol 3 units and rounds the bends on screen. Release without moving: drag.rs line 246 finds d = 0, nothing commits, the items jump back.

### F51. crates/ui-egui/src/canvas/interact/drag.rs:189

Severity: low. Area: Last wave (transform, open policy).

For a routed connector with hand-placed bends glued at both ends to shapes in a group move, resize or rotate, the preview transforms the bends and then autoroute::preview discards them, while the commit keeps the transformed bends, so the bends vanish during the drag and reappear on release.

Failure scenario. Flowchart page with routing.avoid = Always, A and B joined by an orthogonal connector whose waypoints the user dragged into a detour. Select A and B and drag 200 right: rigid::apply translates the waypoints; preview_page passes shapes plus carried plus rerouted as moving; autoroute.rs lines 569 to 575 exclude the connector from keep because both ends are glued to moving shapes, and the detour is drawn as the router's route. On release shape.transform translates the bends and refresh's keep_of puts the connector in keep, so the detour jumps back, translated. Net wires are unaffected.

### F52. crates/engine/src/cmd/transform.rs:82

Severity: low. Area: Last wave (transform, open policy).

shape.transform has no identity guard and normalises the angle with rem_euclid(360), so a no-op request still runs rigid::apply: on a module page it snaps every symbol pin, bend and free end to the module; on any page coordinates drift by an ulp; a tiny negative angle becomes exactly 360 and a full turn moves far-away centres enough to trip the lock tolerance.

Failure scenario. Page with module 10, a symbol previously nudged by shape.move dx = 3 and a connector with autoroute-stored waypoints: shape.transform {ids, move: {dx: 0, dy: 0}} moves the symbol onto the module, rounds the waypoints and adds an undo step. Without a module, x = 0.3 with w = 37.7 becomes 0.3000000000000007 and an undo step is recorded for no visible change. rotate {angle: -1e-20} on a position-locked shape at x = 1e7 gives 360.0, sin(2 pi) moves the centre by 2.4e-9 > 1e-9, and protect::check refuses a full turn. The canvas filters identity in group::finish, so this is the MCP and control path.

### F53. crates/ui-egui/src/canvas/interact/group.rs:211

Severity: low. Area: Last wave (transform, open policy).

run_named wraps the lone shape.transform in edit.batch only to name the undo step, so an engine refusal reaches the status bar as "commands[0] (shape.transform) failed: bad parameters: ...; nothing was changed"; neither group::allowed nor drag::move_allowed consults layer locks or descendants, so the handles stay visible and the drag previews before that wrapped refusal.

Failure scenario. Group g (members m, n) with m on a locked layer, plus shape b. The frame shows every handle. Drag and release: move_allowed([g, b]) passes, commit_move runs edit.batch with shape.transform, ensure_unlocked over descendants refuses on m with the wrapped message. Session::run records spec.label (a static str), so this is the third single-command batch used as a label workaround (relationship.rs line 155, problems.rs line 246); a label parameter honoured by Session::run removes all three.

### F54. crates/ui-egui/src/canvas/interact/group.rs:86

Severity: low. Area: Last wave (transform, open policy).

allowed() hides all eight group resize handles when any scaling shape has a width or a height lock, but protect::refuses only rejects a Size change on the locked side and the single-shape path keeps the handles that change the free side, so a height-only lock needlessly removes the left and right edge handles from the group frame.

Failure scenario. Shape a with locks.height = true selected together with b: the engine accepts shape.transform {scale: {sx: 2, sy: 1}} (dh = 0; protect.rs lines 81 to 87 do not refuse), a selected alone shows its left and right handles (effective_handle sets hy to 0.5 and keeps hx), but the multi-selection frame shows no resize handles at all.

### F55. crates/ui-egui/src/menus.rs:613

Severity: low. Area: Last wave (transform, open policy).

The open-beside policy is reachable only from gestures and has no ui.* command id, so the control channel's advertised Recent entry file.open {path}, dispatched by id, replaces the front document while the same click opens a new window; AGENTS.md states that a thing that can be clicked but not run by id is not finished.

Failure scenario. Desktop app with a dirty document in front. Clicking File > Open Recent > X runs open_document and opens X beside. A control client reads ui.menus, gets {id: "file.open", params: {path: X}} (control.rs line 177), sends engine.execute with it: control.rs lines 248 to 256 call app.run, replacing the dirty document. The same untouched test is written at lib.rs line 1033, main.rs line 222 and menus.rs line 613. One ui.open {path} command dispatched by every caller would make the policy single and scriptable. The author documents the split as intentional, so this is a rule-versus-design call for the owner.

### F56. crates/ui-egui/src/canvas/interact

Severity: low. Area: Last wave (transform, open policy).

Twelve further items verified in the last-wave review and cut by its cap: duplicated helpers and small edge cases in the multi-selection transform.

Failure scenario. resize_rigid can emit a factor in (1e-9, 1e-6) on a frame over 1e6 units wide that the commit then refuses (needs a wheel pan mid-drag). A control-channel edit.undo or page.select during a held group drag makes finish scale or turn about the stale press-time bounds. snap_first_pin is a third copy of wire::snap_first_port and the stencils apply::snap_first_port. group::bounds_of and transform::items_bounds duplicate each other (the UI copy's is_finite filter is dead). resize_cursor was extracted but interact/mod.rs line 261 still inlines the same chain. group_handle and group_cursor are the same hit test twice. rotate_deg duplicates rotate_angle and resize_rigid duplicates resize_frame with different rounding. number and pivot duplicate req_num and wire::coord. one_step is the seventh copy in engine tests and Lcg the second. group::selection and allowed run twice per frame with with_descendants and a route_polyline per selected connector, and allowed_aspect is recomputed every frame during a drag. transform() clones the page and routes connectors before the routing pass replaces those routes. Commit 0cd04c5 changes docs/architecture.md without saying so in its message. transform_tests.rs line 216 would panic on an empty page (test only).

### F57. crates/ui-egui/src/canvas/interact/tools.rs:54

Severity: low. Area: Canvas.

A press at the position of a resize or rotate handle that the overlay hides because of locks or document protection starts a dead Resize or Rotate drag instead of moving the shape.

Failure scenario. decide (lines 51 to 56) picks a handle by position only; overlays.rs lines 322 and 328 hide handles for which rotate_allowed or handle_allowed is false or the document is protected. For a shape with width and aspect locked, effective_handle returns None, the user sees no handles, presses on the corner expecting a move, and gets Drag::Resize whose frame is None in both preview and finish: no move, no marquee, no feedback. The sequence page handles the case for frame_fixed shapes only.

### F58. crates/ui-egui/src/canvas/interact/mod.rs:152

Severity: low. Area: Canvas.

Pressing the pan modifier after a move or resize drag has begun pans the view and continues the shape drag at the same time, and the release commits the shape change.

Failure scenario. The pan modifier is tested only at drag start (line 152); the per-frame update at lines 172 to 199 keeps following the pointer regardless of modifiers, while handle_pan_and_zoom (canvas/mod.rs lines 146 to 148) re-evaluates the modifier every frame and pans on a primary drag. Start a move, hold the modifier mid-drag: the page scrolls while the preview keeps moving the shape, and drag_stopped commits shape.move with the accumulated world delta.

### F59. crates/ui-egui/src/panes/inspector.rs:629

Severity: low. Area: Chrome, dock, panes, control.

The Inspector's X, Y, Width and Height drag values, the Format tab's line width and font size drag values and its colour pickers run a document command on every frame of a drag, producing one undo step per frame instead of one per gesture.

Failure scenario. shape_section pushes arrange.setBounds whenever the drag value reports changed() (lines 622 to 632); format_tab does the same for shape.setStyle lineWidth (lines 1077 to 1082), text.setFormat size (lines 1093 to 1098) and the colour buttons. egui's DragValue reports changed() each frame the pointer moves, and Session::run records a step for every command that changes the document. Dragging Width by 40 points leaves about 40 undo steps. param_editor in the same file commits only on drag_stopped (lines 916 to 918).

### F60. crates/ui-egui/src/dock/generate.rs:164

Severity: low. Area: Chrome, dock, panes, control.

Dock code that calls session.run directly leaves the engine's status text in session.status, which the next unrelated command then shows in the status bar.

Failure scenario. generate::act runs app.session.run(id, p) (line 164) instead of app.run; the engine's generate.run sets s.status (engine generate.rs line 193). Only CraftApp::after_command drains session.status (lib.rs lines 750 to 753). Click Generate, then Zoom In: the status bar says "Generate: wrote ..." as if zooming had done it. The same holds for errors from problems::run_check (problems.rs line 56) and tables::refresh (tables.rs line 203).

### F61. crates/ui-egui/src/lib.rs:881

Severity: low. Area: Chrome, dock, panes, control.

ui.tab stores any string as the ribbon tab, so ui.inspect and the saved preferences report a tab that is not drawn.

Failure scenario. ui.tab {tab: "Nope"} (lines 881 to 890) sets self.ui.tab and replies with it; the ribbon falls back to the first visible tab (ribbon/draw.rs lines 51 to 53) and draws Home, while ui.inspect and the saved preferences carry "Nope". A saved junk value survives restarts since nothing sanitises it.

### F62. crates/diagram/src/container.rs:536

Severity: low. Area: Diagram, routing, layout.

content_rect takes min(width, height) of the heading band as the band's thickness, which is too small whenever the frame is narrower than the band is thick, so the content area disagrees with heading_rect.

Failure scenario. Container with w 20, h 200, heading Top, headingSize 28. heading_rect returns a band of 20 by 28. content_rect (line 536) takes min(20, 28) = 20, so the content rect starts 8 units inside the heading band. Callers inherit the error: autolayout.rs line 655 room_in, lines 420 to 425 with_box_insets, and engine pattern/sync.rs lines 81 and 91. The same happens for a Left or Right heading when h < headingSize.

### F63. crates/mermaid/src/architecture.rs:213

Severity: low. Area: Formats, drawio, mermaid, stencils.

Edge validation is a linear search over all services and junctions for every edge end, twice, so a diagram at the node and edge caps costs about 1e10 string comparisons.

Failure scenario. Lines 213 to 220 check each edge end with a linear any() over services chained with junctions, then retain edges with the same scan. With MAX_NODES = MAX_EDGES = 50,000, both fitting in the 2 MB input, the cost is 2 times 2 times 50,000 times 50,000 comparisons. The ids HashMap built at line 185 is already available.

### F64. crates/codegen/src/names.rs:182

Severity: low. Area: Notation, analysis, codegen.

The uniqueness counter is appended after sanitising, so a de-duplicated identifier can exceed the target's max_len and bypass its reserved-word check.

Failure scenario. Two elements whose tag sanitises to the same 64-byte base under markdown-catalog (max_len 64). sanitize truncates at line 102, but line 182 builds base plus separator plus n without re-sanitising, giving a 66-byte identifier. The rename map stores it and a later run remakes it and reports it as a rename, since sanitize(prev) == prev at line 176 fails. The one shipped target has no reserved words, so the reserved-word part is latent.

### F65. crates/model/src/types.rs:469

Severity: low. Area: Model, MCP, apps.

Model::remove leaves Model.patterns untouched, so after a delete a PatternInstance.members entry names an element or relationship that no longer exists, contradicting the invariant pattern.rs documents.

Failure scenario. A model with patterns [{id: p1, members: {split: e1}}]; call model.remove(["e1"]). Lines 469 to 510 remove e1 and its relationships but never touch self.patterns. pattern.rs line 7 promises that ids that no longer exist are dropped by sanitize, yet pattern::sanitize runs only from Model::sanitize at load time; no engine code calls it after a delete. Until reload, pattern_of("e1") still returns p1 and role_of reports a role for a missing id.

### F66. apps/vizcraft/src/main.rs:521

Severity: low. Area: Model, MCP, apps.

A --control value that is not a port number is silently dropped and also discards the port from VIZCRAFT_CONTROL_PORT, so the app starts with no control channel and no message.

Failure scenario. VIZCRAFT_CONTROL_PORT=7990 vizcraft --control 799O, or vizcraft --control followed by a file name: line 521 turns the parse failure into None, overwriting the environment value read at line 512, and the next argument is consumed as the port instead of being opened as a file (line 534). The first sign is every CLI control call and MCP Remote call failing with "no VizCraft app is listening". The CLI's mcp_backend treats the same mistake as a usage error.

## Recurring patterns

Four patterns account for most of the medium findings. A fix at the shared point removes several entries at once.

Pattern 1. Locks and protection are checked at the command, not at the mutation. The checks for shape locks, layer locks and document protection live in a few handlers (shape.move, shape.resize, shape.rotate, shape.delete, shape.setText, connector.glue, shape.transform). Every other handler that moves, sizes, turns, deletes or writes text reaches the model through the arrange, text and delete helpers without those checks. Entries F04, F13, F14, F16, F18, F43, F44 and F45 are instances. The second engine pass observed the same gap, not separately listed, in model.delete, doc.removeHidden, glyph.setParams and datalink apply.

Pattern 2. The first page is treated as the active page. Several handlers test index == 0 where the intended condition is "this is the active page". Entries F17, F41 and F42 are instances; ladder.insert has the same assumption.

Pattern 3. Single-item and multi-item editing diverge. shape.transform (the multi-selection path) carries wire bends, snaps symbols to the module and checks descendants; shape.move (the single-item path, also used by arrow-key nudging, groups, MCP and ladder placement) does none of these. Entries F13, F20, F21, F23, F48, F50, F51, F53 and F54 are instances.

Pattern 4. Input caps bound one dimension but not the product. The parsers cap input bytes, node counts and edge counts separately. A construct that multiplies two capped lists, or a size the file declares for itself, escapes the cap. Entries F06, F07, F30 and F63 are instances.

## Coverage

Table 3. Regions each reviewer did not read line by line. These regions carry no finding because they were not read, not because they are known to be clean.

| Scope | Not read line by line |
|---|---|
| Last wave diff | Complete. Twelve further verified low items were cut by the reviewer cap and are summarised under F56. |
| crates/engine/src/cmd (two passes) | plant.rs outside the loop-sheet section, electrical.rs after line 440, validate.rs and describe.rs bodies, sequence.rs layout and move commands. |
| crates/engine except cmd | The frame and stall bookkeeping in trace.rs lines 560 to 930 (skimmed). |
| crates/ui-egui/src/canvas | Wire self-split preview in wire.rs and guide versus canvas drag ordering in rulers.rs were traced but not confirmable from the code alone. |
| crates/ui-egui except canvas | icons.rs (drawing code, constant indexes only) and theme.rs. |
| crates/diagram, routing, layout | sequence.rs event-list editing, data_graphic.rs, ink.rs beyond simplify, the force layout cell loop. The routing and layout crates traced clean. |
| crates/formats, drawio, mermaid, stencils | mermaid class, state and er parsers and their mappers, the PlantUML activity walker, individual stencil builders, svg step badges and glyph labels (grepped only). |
| crates/notation, analysis, codegen, patterns | bind.rs, bind_nets.rs, electrical_pass.rs and process_pass.rs beyond their loops, template.rs, loader.rs, validate.rs, schema.rs, catalog.rs bodies, the large process fixtures. |
| crates/model, mcp, macos-open, apps | Complete. |
## Fix blocks

A block is a set of findings that one agent fixes on one branch, chosen so that the files it edits do not overlap another block running at the same time. Finding ids refer to the tables above.

Table 4. Fix blocks.

| Block | Theme | Findings | Files edited | Runs in parallel with |
|---|---|---|---|---|
| A | Crashes and hangs from input | F01, F06, F07, F08, F11, F30, F38, F46, F63 | crop.rs, mermaid flowchart.rs and architecture.rs, formats table.rs, analysis reliability.rs, spelling_dialog.rs, drawio attrs.rs, both main.rs, ladder.rs | all others |
| B | Session and undo contract | F03, F05, F09, F19, F39, F40 | session.rs, macro.rs, dock/activity.rs | A, C, D, F, G, H |
| C | Locks checked at the mutation point | F04, F13 (engine side), F14, F16, F18, F43, F44, F45, and the four unlisted bypasses in model.delete, doc.removeHidden, glyph.setParams, datalink apply | engine cmd: arrange.rs, shape.rs, edit.rs, page.rs, layer.rs, connector.rs, compartment.rs, text.rs, mod.rs, wire.rs, suggest.rs, relationship.rs, table.rs, sheet.rs | A, B, D, F, G, H |
| D | One move path for single and multi selection | F13 (UI side), F20, F21, F23, F26, F27, F28, F48, F50, F51, F52, F53, F54, F56, F57, F58 | diagram arrange.rs and arrange/rigid.rs, engine transform.rs, canvas interact (drag, group, membership, keys, edit_text, ink, tools, mod) | A, B, C, F, G, H |
| E | Page index and id remapping | F15, F17, F41, F42, F47, F65, ladder.insert | page.rs, layer.rs, relationship.rs, file.rs, table.rs, datagraphic.rs, model types.rs | after C merges (same files) |
| F | Windows, open policy and the control channel | F02, F12, F22, F24, F25, F49, F55, F60, F61, F66 | ui-egui lib.rs, gallery.rs, menus.rs, control.rs, dock/generate.rs, problems.rs, tables.rs, apps/vizcraft main.rs | A, B, C, D, G, H |
| G | Panes | F10, F59 | panes/model_tree.rs, panes/inspector.rs | all others |
| H | Format, rule and schema correctness | F29, F31, F32, F33, F34, F35, F36, F37, F62, F64 | diagram container.rs, formats drawio.rs, dbml.rs, pdf.rs, notation electrical_rules.rs, codegen regions.rs and names.rs, mcp diagram.rs and tools.rs | all others |

### Block A. Crashes and hangs

Each fix is local and independent. The crop drag orders its clamp bounds and refuses a crop whose free span is under one unit. The Mermaid builder caps the product of targets and props and the product of ids and classes against one budget, and the architecture validator uses the id map it already has. The XLSX reader inflates each entry through a counting reader with a hard cap before handing bytes to calamine, instead of trusting the central directory. The event-tree walk carries a visited set per path, or refuses when a branch point is reachable from itself. Change All stops when the replacement equals the word or when the document did not change. Attribute insertion keeps an index. Both hosts read arguments with args_os and reject non-Unicode paths as a usage error. ladder.place uses checked_sub. One hostile-input test per fix, added to the existing never-panic suites.

### Block B. Session and undo contract

Session::run snapshots history, redo, path and dirty alongside the document and restores all of them on error; that single change fixes F03 for template.new, example.open and any future command that replaces the document. macro.run joins is_transaction. file.open stops the auto-size and autoroute passes from comparing against the previous session document. mark_saved marks the transaction as well, so a batch holding page.setActive or file.save records no step. Script recording pairs redo with the step count. For the Activity tab the engine's transaction listing carries a live flag per entry, so the tab stops inferring undo positions from log order. Tests assert undo_depth, dirty and path after each case.

### Block C. Locks at the mutation point

The lock, layer and protection checks move from the individual handlers into the three helpers every mutation passes through: the engine wrappers around arrange translate, set_frame and set_angle, delete_on_page, and put_text. Each helper expands with_descendants before checking, which closes F13 on the engine side. A single glue helper applies both the noGlue behaviour and the layer glue flag, and every path that glues an end calls it. page.delete calls the backgrounds check. After this block the arrange.*, page.*, layer.*, compartment.*, edit.pasteSpecial, model.delete, doc.removeHidden, glyph.setParams and datalink paths need no handler-level check. One test per handler that bypasses a lock today, each asserting the refusal and that nothing changed.

### Block D. One move path

arrange::translate carries the bends of connectors glued at both ends into the moved set, and leaves the bends of a connector glued at one end untouched instead of clearing them. shape.move, arrow nudging, groups, MCP and ladder placement then behave as shape.transform does, and the three ids.len() greater than 1 forks in the canvas go. shape.transform gains an identity guard and a normalised angle range of (-180, 180]. The preview and the commit share one routing decision for connectors with own bends. The aspect-locked resize rounds one side and derives the other. Escape on a created empty text box deletes it. The eraser filters locked layers before previewing. A press on a hidden handle starts a move. The pan modifier is read once per drag. The duplicate helpers listed under F56 collapse. Module snapping (F23) needs an owner decision, recorded below. Snapshot PNGs change on this branch, so it merges with ours taken for snapshots, as earlier UI branches did.

### Block E. Page index and remapping

A helper answers whether a page index is the active page, and layer.assign, layer.unassign, relationship.add, file.import, page.set, table link and ladder.insert use it instead of index 0. page.duplicate reuses the id remap from Fragment::insert, which covers compartment members and callout targets. Legend rebuilds free glued ends instead of deleting connectors. Model::remove drops pattern members. This block edits page.rs and layer.rs, which Block C also edits, so it starts after C merges.

### Block F. Windows and open policy

A single untouched-or-prompt decision lives in one function that File > New, Mod+N, drop, picker, Recent, Open and the Examples gallery all call, on every host; the web build gets the Save First / Replace / Cancel notice that the gallery already has. Window requests become a queue so a multi-file drop opens each file. A failed open in a new window closes that window and reports in the requesting one, and the File page shows the status line so a failed open is visible. ui.set changes one field in place instead of round-tripping UiState. Dock code calls app.run so status is drained. ui.tab validates against the tab list. A bad --control value is a usage error. Whether the open-beside policy gets a ui.open command id (F55) is an owner decision, recorded below.

### Block G. Panes

The Model Tree draws one rename editor per id. Inspector drag values commit on drag_stopped like param_editor does. Two files, one short task.

### Block H. Format, rule and schema correctness

Ten independent fixes: Container::sanitize forces Top on lane sets; content_rect takes the band thickness from the heading side; drawio::import restores nets; the DBML indexes parser stops at a closing brace; print layout scales before clamping as export_pages does; mixedSymbols reads the page variant before assuming IEC; codegen carry places or appends every duplicate region with an orphanRegion issue; the uniqueness suffix is re-sanitised; the two MCP schemas say "active page". Each gets a fixture or unit test.

### Owner decisions

- F23 module snapping. Under optionality-first, snapping a moved symbol to the module is a page or user setting, not a fixed behaviour of the multi-item path. Block D needs to know whether shape.move should snap too when the setting is on, or neither path should.
- F55 ui.open command id. The author documented the gesture-only policy as intentional. AGENTS.md says a clickable thing must be runnable by id. Block F adds the command unless the split is kept.
- F19 file.open re-routing. Block B assumes an opened file is never changed by open. If some auto-size on open is wanted, it must mark the document dirty.

### Order

A, B, C, D, F, G and H start together on seven branches, each with its own target directory. E starts when C merges. Merge order on return: A, H, G, B, C, E, F, D, with D last because it changes the most snapshots.
