# Control protocol

The control channel lets a script, a test or an agent drive the running app: run commands,
inspect state, inject pointer and keyboard input, and take screenshots of the window. It is the
same path the menus and ribbon use (everything dispatches by command id), so what a script does is
what a person clicking would get.

Start the app with a port:

```sh
cargo run --release -p protocraft -- --sample --control 7990     # or PROTOCRAFT_CONTROL_PORT=7990
```

The server listens on `127.0.0.1` only. A client sends one JSON object per line and gets one
reply line per request, in order. Several clients may connect; each connection is served on its
own thread and requests are handled on the UI thread one frame at a time.

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
| `{"id": ..., "ok": false, "error": "timeout"}` | the UI did not answer within 60 seconds (window hidden or blocked) |

A failed `engine.execute` leaves the document as it was: `Session::run` restores it, including
after a caught panic. An unknown command id is an error, not a crash.

## Methods

| Method | Params | Result |
|---|---|---|
| `engine.execute` | `command` (an id), `params?` (object, default `{}`) | the command's result; engine commands and `ui.*` commands alike |
| `engine.commands` | none | array of `{id, label, location, shortcut, params, enabled, reason?}` for every command, UI commands included |
| `ui.inspect` | none | the UI state, see below |
| `ui.set` | `path` (dotted path into `UiState`, e.g. `dark`, `panes.navPane`, `ribbonCollapsed`), `value` | the updated `UiState` |
| `ui.click` | `x`, `y` (window points), `button?` (`left` default, `right`), `count?` (1 to 3), `shift?`, `cmd?`, `alt?` | `{"queued": true}` |
| `ui.move` | `x`, `y` | `{"queued": true}` |
| `ui.drag` | `x`, `y`, `toX`, `toY`, `steps?` (default 8), `shift?`, `cmd?`, `alt?` | `{"queued": true}` |
| `ui.key` | `key` (`A`, `Enter`, `Escape`, `Left`, `F2`, `Space`, `Tab`, egui key names), `shift?`, `alt?`, `cmd?` | `{"queued": true}` |
| `ui.text` | `text` | `{"queued": true}` |
| `ui.screenshot` | `path?` | `{"path", "width", "height"}`; see below |
| `app.quit` | none | `{}`; quits every window without asking about unsaved changes |

Any other method name is treated as a command id with `params` as its params, so
`{"method": "edit.undo"}` is the short form of `engine.execute`.

`cmd` means the platform's primary modifier: Command on macOS, Control elsewhere, the same
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
| `ui` | the `UiState` as it would be persisted: `tab`, `ribbonCollapsed`, `dark`, `recent`, `author`, `panes`, `window`, `inWindowMenus` |
| `view` | the engine `ViewState`: zoom, dark mode, pane visibility flags |
| `dialog` | the open dialog, or `null` |
| `window` | `[width, height]` in points |
| `title` | the window title stem (document name) |
| `dirty` | whether the document has unsaved changes |
| `rev` | the document revision, bumped on every change |
| `undoLabel` | the label of the next undo step, or `null` |
| `status` | the current status-bar message, or `null` |
| `perf` | `{"frameMs": ...}` |

A craft adds its own fields (canvas rectangle, selection, caret, hit regions) next to these and
documents them in its own copy of this file. `ui.set` is the write side of `ui`: it accepts any
path into `UiState` and replies with the whole state after the change, so a test can switch to
dark mode, collapse the ribbon or hide a pane and then screenshot.

### `ui.screenshot`

`ui.screenshot {"path": "/tmp/shot.png"}` captures the window and writes a PNG. The capture is
not taken on the frame the request arrives: the app waits for at least three frames and for any
queued input to be processed, then asks the viewport for a screenshot, and replies once the image
has been received and written. The reply, `{"path": ..., "width": ..., "height": ...}`, therefore
arrives a few frames later than other replies; keep it last in a pipelined batch, or send it on
its own. Without `path` the image is written to `/tmp/protocraft-screenshot.png`. The size is in
physical pixels, so on a Retina display it is twice the window size in points.

With several windows open the control channel belongs to the app, not a window, and acts on the
window in front; `ui.screenshot` captures that window. Because eframe draws additional viewports
inside the root window's frame, a screenshot while a non-root window is in front fails with an
error rather than returning the wrong image. Screenshots work with the screen locked and do not
steal focus, which is what makes them usable from an agent running in the background.

## Examples with nc

Open the sample, zoom in, then screenshot:

```sh
printf '%s\n' \
  '{"id":1,"method":"engine.execute","params":{"command":"view.zoomIn","params":{}}}' \
  '{"id":2,"method":"ui.inspect"}' \
  '{"id":3,"method":"ui.screenshot","params":{"path":"/tmp/zoomed.png"}}' \
  | nc 127.0.0.1 7990
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

## Conventions for a craft that extends the protocol

- Keep the reply shape. New methods return `ok` and `result` or `ok: false` and `error`.
- Name methods by subject: `document.inspect`, `ui.clickText`, `sheet.read`. Methods that act on
  the document should also exist as commands, so the CLI and the menus get them for free; the
  method is then only a convenience.
- Document every method here, in the table, with its params and its result.
