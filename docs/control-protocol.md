# Control protocol

The control channel lets a script, a test or an agent drive the running app: run commands,
inspect state, inject pointer and keyboard input, and take screenshots of the window. It is the
same path the menus and ribbon use (everything dispatches by command id), so what a script does is
what a person clicking would get. The MCP server's `Remote` backend speaks this protocol
to the running app ([mcp.md](mcp.md)).

Start the app with a port:

```sh
cargo run --release -p vizcraft -- --sample --control 7990     # or VIZCRAFT_CONTROL_PORT=7990
```

The app prints `vizcraft: control server listening on 127.0.0.1:7990` on stderr once the port
is bound. `--control 0` asks the OS for a free port and prints the one it got, so a script that
cannot know which ports are free parses that line. A port that is already taken is reported
(`control server failed to bind ...`) and the app runs without control.

The server listens on `127.0.0.1` only. A client sends one JSON object per line and gets one
reply line per request, in order. Several clients may connect; each connection is served on its
own thread and requests are handled on the UI thread one frame at a time. A client that
disconnects mid-request is dropped; the app does not care. Request lines longer than 16 MB are
refused.

The CLI is the one-shot client, so scripts do not need `nc`:

```sh
vizcraft-cli control 7990 ui.inspect                          # host:port or just the port
vizcraft-cli control 127.0.0.1:7990 engine.execute '{"command":"view.zoomIn"}'
printf '%s\n' '{"id":1,"method":"ui.inspect"}' | vizcraft-cli control 7990 -    # requests from stdin
```

It prints the reply line and exits 0 when `ok` is true, 1 otherwise (or when the app cannot be
reached), 2 for a usage mistake.

```json
{"id": 1, "method": "engine.execute", "params": {"command": "file.new", "params": {}}}
{"id": 1, "ok": true, "result": {}}
```

`id` is anything the client likes (number, string, or absent); the reply carries it back
unchanged so a client can match replies when it pipelines requests. `params` may be omitted and
defaults to `{}`.

## Reply shape

| Reply | Meaning |
|---|---|
| `{"id": ..., "ok": true, "result": <any>}` | the method ran; `result` is method-specific |
| `{"id": ..., "ok": false, "error": "<message>"}` | the method failed; the document is unchanged |
| `{"ok": false, "error": "bad JSON: ..."}` | the line did not parse; no `id` because none was read |
| `{"id": ..., "ok": false, "error": "bad request: ..."}` | the line parsed but was not an object with a string `method` |
| `{"id": ..., "ok": false, "error": "unknown method ..."}` | the method is neither a protocol method nor a command id |
| `{"id": ..., "ok": false, "error": "timeout"}` | the UI did not answer within 60 seconds (window hidden or blocked) |

A failed `engine.execute` leaves the document as it was: `Session::run` restores it, including
after a caught panic. An unknown command id is an error, not a crash.

## Methods

| Method | Params | Result |
|---|---|---|
| `engine.execute` | `command` (an id), `params?` (object, default `{}`), `origin?` (`mcp` logs the transaction as an MCP call; the MCP Remote backend sends it) | the command's result; engine commands and `ui.*` commands alike |
| `engine.commands` | none | array of `{id, label, location, shortcut, params, enabled, reason?}` for every command, UI commands included |
| `ui.inspect` | none | the UI state, see below |
| `ui.set` | `path` (dotted path or JSON pointer into `UiState`: `dark`, `panes.leftWidth`, `/ribbonCollapsed`), `value` | the updated `UiState`; unknown paths and wrong types are errors |
| `ui.menus` | none | the menu tree, see below |
| `ui.click` | `x`, `y` (window points), `button?` (`left` default, `right`), `count?` (1 to 3), `shift?`, `cmd?`, `alt?` | `{"queued": true}` |
| `ui.move` | `x`, `y` | `{"queued": true}` |
| `ui.drag` | `x`, `y`, `toX`, `toY`, `steps?` (default 8), `shift?`, `cmd?`, `alt?` | `{"queued": true}` |
| `ui.key` | `key` (`A`, `Enter`, `Escape`, `Left`, `F2`, `Space`, `Tab`, egui key names), `shift?`, `alt?`, `cmd?` | `{"queued": true}` |
| `ui.text` | `text` | `{"queued": true}` |
| `ui.wait` | `frames?` (default 1, at most 10000) | `{"waited": true}`, sent after that many frames have run |
| `ui.screenshot` | `path?` | `{"path", "width", "height"}`; see below |
| `ui.resize` | `width`, `height` (points, clamped to the window's minimum size) | `{"width", "height"}`; asks the window to resize |
| `ui.focus` | none | `{}`; asks the window for focus (the desktop app only) |
| `app.quit` | none | `{}`; quits every window without asking about unsaved changes |

A method name that is a command id (engine or `ui.*`) runs that command with `params` as its
params, so `{"method": "edit.undo"}` is the short form of `engine.execute`. Any other name is
`unknown method`, never a silent no-op.

Pipelined requests are separate undo steps, and a failure in the middle leaves the earlier ones
applied. To make several commands one undo step, all or nothing, send them as one `edit.batch`:
`{"method": "edit.batch", "params": {"label": "Add pair", "commands": [{"command": "shape.add",
"params": {...}}, ...]}}`. It runs the commands in order inside one engine transaction; when one
fails it rolls the whole batch back and replies `ok: false` with `commands[i] (id) failed: ...`.
`edit.undo`, `edit.redo`, `ui.*` and `app.*` commands cannot run inside it. `script.run
{script}` replays a recorded script the same way. See
[architecture.md](architecture.md#transactions-and-scripts).

Commands run through the app's `CraftApp::run`, so they behave as they do from a menu: after a
successful `file.open`, or a `file.import` with `replace: true`, the app runs `view.zoomFit` when
the canvas cuts the page off at the current zoom (the MCP `open_*` tools get this too). A caller
that sets the view itself passes `"fit": false` in the command's params to skip it.

Every param reader tolerates junk: a wrong type is treated as missing (and reported when the
param is required), numbers are clamped, nothing panics. The UI tests feed every method hostile
params to keep it that way.

`cmd` means the platform's primary modifier: Command on Apple macOS, Control elsewhere, the same
meaning as `Mod` in shortcut strings. Coordinates are in window points (logical pixels, as egui
sees them), with the origin at the top left of the window content; on macOS with the integrated
title bar the content starts at the very top.

### Input methods are queued

`ui.click`, `ui.move`, `ui.drag`, `ui.key` and `ui.text` reply `{"queued": true}` as soon as the
synthetic events are added to the next frame's input. The reply does not mean the UI has reacted;
it means egui will see the events on the next frame. To observe the effect, follow with
`ui.inspect` or `ui.screenshot`, which run after the queued input has been processed. Shortcuts
apply to `ui.key`, so `{"key": "S", "cmd": true}` saves exactly as pressing Cmd+S would.

### `ui.inspect`

Returns one JSON object with everything a script needs to check its work without a screenshot:

| Field | Content |
|---|---|
| `ui` | the `UiState` as it would be persisted: `tab`, `ribbonCollapsed`, `dark`, `recent`, `author`, `panes`, `window`, `inWindowMenus`, `inspector`, `dock` |
| `view` | the engine `ViewState`: `zoom`, `pan` (the world point at the canvas centre), `darkMode`, `navPane`, `propsPane` |
| `selection` | the active selection: `primary`, `items` (ids), `range` |
| `dialog` | the open dialog as `{"dialog": "<name>", ...its fields}`, or `null` |
| `backstage` | whether the File page is showing |
| `window` | `[width, height]` in points |
| `title` | the window title stem (document name) |
| `path` | the document's file, or `null` |
| `dirty` | whether the document has unsaved changes |
| `rev` | the document revision, bumped on every change |
| `canUndo`, `canRedo` | whether Undo and Redo are available |
| `undoLabel`, `redoLabel` | the label of the next undo or redo step, or `null` |
| `status` | the current status-bar message, or `null` |
| `perf` | `{"frameMs": ...}`, how long the last layout took |
| `tool` | the canvas tool: `pointer`, `connector`, `text`, `wire`, `pipe` or `ladder` (set with `ui.tool`; `ui.tool {tool: "connector", kind}` also picks the connector kind; `ui.tool {tool: "wire" or "pipe", line}` arms a connector type id for the wire and pipe tools, and the reply adds `line`; with no line armed, W and P use the active pack's first connector type with a matching net) |
| `tables` | the Tables dock view: `view` (`list`, `matrix` or `pack`), `subject` (`nets` or `connections` for the net tables, empty for the model), `kind`, and `pack` (a pack list such as `process.line`) (set with `ui.tables {subject?, kind?, pack?}`); `ui.dialog {name: "tagFormats"}` opens the Tag Formats dialog and `ui.loopTrace` traces the selected loop |
| `tree` | the Model Tree pane: `mode` (`model` or `context`), `branch`, `expanded`, `collapsed`, `selected`, `relationships` (the relationship rows shown) (set with `ui.tree`) |
| `palette` | the Palette pane: its query, groups and the rows shown (set with `ui.palette`) |
| `inspector` | the Inspector pane: `open`, `mode` (`standard` or `expert`), `tab` (`properties` or `format`), and `fields` with the `symbol`, `sheet`, `zone` and `sequence` (`{activations, row, columnGap, selected}`), `relationship`, `members`, `columns` and `electrical` (designation parts, the coil's contacts, the terminal grid, a cable's cores), `line`, `loop`, `nozzles` and `stepChart` (`{element, rows: [{code, states}], findings: [{message, idle}]}`) groups when shown (set with `ui.inspector`) |
| `dock` | the bottom dock: `open`, `height`, `tab` (`problems`, `search`, `activity`, `tables`, `generate`, `nets`, `analysis`) and the `rows` the tab shows (set with `ui.dock`; `ui.search` fills the Search tab) |
| `menu` | the open ribbon dropdown as `{"title", "rows": [{"label", "id", "enabled", "checked"}]}`, or `null` |
| `gallery` | the New gallery: `open`, `mode` (`new`, `page` or `examples`), `viewpoint`, `query`, `audience`, `selected`, `shown`, `pending` and `failed` previews, `notice`, `workspace`, `recent`, and click rects `[x0, y0, x1, y1]` for `chips`, `cards`, `recentCards` and `buttons` (set with `ui.gallery`; `ui.preset` reads or sets the workspace preset) |
| `analysis` | the stored analysis as `{kind, summary, count, sections, rev, stale, truncated, highlighted, ghosts, selected, options, last}` (`highlighted` and `ghosts` count what the active page draws), or `null` with no result (set with `ui.analysis`; `ui.compareFile` picks a file and runs `model.diff`) |
| `nets` | the Nets dock tab: the nets listed and the selected one (set with `ui.nets`) |
| `diagram` | a compact summary of the page and the canvas, see below |

`diagram` has these fields:

| Field | Content |
|---|---|
| `page` | `{"id", "width", "height", "board"}` of the page shown, or `null`; `board` is `infinite` or `fixed`, and on an infinite board `width` and `height` are the frame's (the drawing bounds plus a margin) |
| `shapes`, `connectors` | how many of each are on the page |
| `selected` | the selected shape and connector ids, in selection order |
| `hover` | the id of the shape or connector under the pointer, or `null` |
| `editing` | the id whose text is being edited in place, or `null` |
| `dragging` | the canvas drag in progress (`marquee`, `move`, `resize`, `rotate`, `connect`, `reglue`, `wire`, `ladder`, and on a Sequence page `reorder`, `column`, `message`, `separator`), or `null` |
| `sequence` | on a Sequence page, `{active, guard, drop}`: the drag in progress, the guard being edited and the drop row, or `null` |
| `net`, `zone` | the net under the pointer or held by a click (number and pin count), and the sheet zone under the pointer, or `null` |
| `canvas` | the canvas rectangle in window points, `[x, y, width, height]`, or `null` before the first frame |

To click a shape, map its world position to window points: the canvas centre shows world point
`view.pan` and one world unit is `view.zoom` points, so a world point `(x, y)` is at
`canvas.x + canvas.width / 2 + (x - pan[0]) * zoom` (and the same for y). The full document is
in the saved file or `vizcraft-cli`. `ui.set` is the write side of `ui`: it accepts any path into
`UiState`, dotted (`panes.leftWidth`, snake_case segments are accepted too) or as a JSON pointer
(`/panes/leftWidth`), and replies with the whole state after the change, so a test can switch to
dark mode, collapse the ribbon or hide a pane and then screenshot. The whole state is validated:
an unknown path or a value of the wrong type is an error and leaves the state as it was. Pane
widths are clamped to the layout's limits and a window geometry that cannot be real is dropped,
as when the preferences file is read. Setting `panes.leftWidth` or `panes.rightWidth` applies to
open side panels live.

### `ui.menus`

Returns the menu tree the in-window bar and the native macOS bar are built from, so a script can
find out what the menus offer and how each entry looks right now:

```json
[{"title": "File", "role": "normal", "items": [
   {"label": "New", "id": "file.new", "params": {}, "shortcut": "Cmd N", "enabled": true, "checked": false, "check": false},
   {"label": "Open Recent", "items": [{"recent": "/tmp/Plans.vizcraft", "id": "file.open", "params": {"path": "..."}}]},
   {"sep": true},
   {"header": "..."}
 ]}, ...]
```

`role` is `normal`, `app`, `window` or `help` (where the native bar places the menu). `check`
marks entries drawn with a check box; `checked` is their state. `shortcut` is the platform text
(glyphs on macOS, `Ctrl+S` elsewhere); the portable `Mod+S` form is in `engine.commands`. Press
any entry with `engine.execute` and its `id` and `params`.

### `ui.wait`

`ui.wait {"frames": 10}` replies once ten more frames have run. Queued input lands one pointer
event per frame (so a drag with eight steps takes about ten frames), and a command's effect on
the layout shows one frame later; `ui.wait` is the way to let that happen before `ui.inspect`
without guessing with sleeps. The app repaints while a wait is pending, so it does not stall on a
window that would otherwise sit idle.

### `ui.screenshot`

`ui.screenshot {"path": "/tmp/shot.png"}` captures the window and writes a PNG. The capture is
not taken on the frame the request arrives: the app waits for at least three frames and for any
queued input to be processed, then asks the viewport for a screenshot, and replies once the image
has been received and written. The reply, `{"path": ..., "width": ..., "height": ...}`, therefore
arrives a few frames later than other replies; keep it last in a pipelined batch, or send it on
its own. Without `path` the image is written to `vizcraft-screenshot.png` in the system
temporary directory (`$TMPDIR` on macOS, `/tmp` on Linux); the reply says where. The size is in
physical pixels, so on a Retina display it is twice the window size in points.

With several windows open the control channel belongs to the app, not a window, and acts on the
window in front; `ui.screenshot` captures that window. Because eframe draws additional viewports
inside the root window's frame, a screenshot while a non-root window is in front fails with an
error rather than returning the wrong image. Screenshots work with the screen locked and do not
steal focus, which is what makes them usable from an agent running in the background. When a
screenshot is all that is needed, the headless `ui_shot` example renders the window without
opening one at all (`docs/testing.md`).

## Examples

With the CLI, one request at a time:

```sh
vizcraft-cli control 7990 engine.execute '{"command":"view.zoomIn","params":{}}'
vizcraft-cli control 7990 ui.set '{"path":"dark","value":true}'
vizcraft-cli control 7990 ui.screenshot '{"path":"/tmp/zoomed.png"}'
vizcraft-cli control 7990 ui.menus | jq '.result[] | .title'
```

With `nc`, several pipelined. Open the sample, zoom in, then screenshot:

```sh
printf '%s\n' \
  '{"id":1,"method":"engine.execute","params":{"command":"view.zoomIn","params":{}}}' \
  '{"id":2,"method":"ui.inspect"}' \
  '{"id":3,"method":"ui.screenshot","params":{"path":"/tmp/zoomed.png"}}' \
  | nc 127.0.0.1 7990
```

Add two shapes and connect them as one undo step (all or nothing):

```sh
vizcraft-cli control 7990 edit.batch '{"label":"Add pair","commands":[
  {"command":"shape.add","params":{"stencil":"basic.rectangle","x":0,"y":0,"id":"a"}},
  {"command":"shape.add","params":{"stencil":"basic.rectangle","x":0,"y":200,"id":"b"}},
  {"command":"connector.add","params":{"from":{"shape":"a"},"to":{"shape":"b"}}}]}'
```

List the commands and keep the enabled ones:

```sh
echo '{"id":1,"method":"engine.commands"}' | nc 127.0.0.1 7990 | jq '.result[] | select(.enabled) | .id'
```

Switch to dark mode, collapse the ribbon, then screenshot:

```sh
printf '%s\n' \
  '{"id":1,"method":"ui.set","params":{"path":"dark","value":true}}' \
  '{"id":2,"method":"ui.set","params":{"path":"ribbonCollapsed","value":true}}' \
  '{"id":3,"method":"ui.screenshot","params":{"path":"/tmp/dark.png"}}' \
  | nc 127.0.0.1 7990
```

Click the first ribbon tab, type into the document, save with the shortcut:

```sh
printf '%s\n' \
  '{"id":1,"method":"ui.click","params":{"x":60,"y":52}}' \
  '{"id":2,"method":"ui.text","params":{"text":"Hello"}}' \
  '{"id":3,"method":"ui.key","params":{"key":"S","cmd":true}}' \
  '{"id":4,"method":"ui.inspect"}' \
  | nc 127.0.0.1 7990
```

(`file.save` on an untitled document asks the UI for a Save As dialog through `ui_requests`; a
script that wants no dialog runs `file.saveAs` with a `path` instead.)

Quit:

```sh
echo '{"id":9,"method":"app.quit"}' | nc 127.0.0.1 7990
```

`nc` keeps the connection open until the server closes it or stdin ends; with `printf` piped in
it ends after the replies. On macOS the system `nc` works as shown; GNU netcat wants `-q 1` to
wait for the replies before exiting.

## Extending the protocol

VizCraft's own methods follow these rules:


- Keep the reply shape. New methods return `ok` and `result` or `ok: false` and `error`.
- Name methods by subject: `document.inspect`, `page.inspect`, `ui.clickShape`. Methods that act on
  the document should also exist as commands, so the CLI and the menus get them for free; the
  method is then only a convenience.
- Document every method here, in the table, with its params and its result.

## Trademarks

Apple, macOS and Retina are trademarks of Apple Inc.; Linux is the registered trademark of Linus
Torvalds. The names are used only to identify platforms; VizCraft is not affiliated with or
endorsed by their owners.
