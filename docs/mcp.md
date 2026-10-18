# MCP server

VizCraft's [Model Context Protocol](https://modelcontextprotocol.io) server lets agents create,
edit and look at diagrams. It lands in two steps: a minimal server with the generic command tools
in Cascade 1 (**built**), and the diagram tools that match draw.io's MCP server feature for
feature in Cascade 3 (**built**, with the Headless backend, offscreen page screenshots and the
page, shape and stencil resources; [ROADMAP.md](../ROADMAP.md)). Cascade 4A adds the
notation schema resources and the issues reported with each command call (**built**). Each
section below says which.

## Design

| Piece | State |
|---|---|
| Crate | Built: `crates/mcp` (`vizcraft-mcp`), between the engine and the apps; it depends on `vizcraft-engine` (for the Headless backend), `serde_json` and `log`, never on `ui-egui` ([architecture.md](architecture.md)) |
| Transport | Built: newline-delimited JSON-RPC 2.0 over stdio, written by hand; no async runtime. Stdout carries only protocol messages; the CLI's own messages go to stderr |
| Launch | Built: `vizcraft-cli mcp [--connect ADDR \| --headless]` |
| Backends | Built: a `Backend` trait with `Remote` and `Headless` (below) |
| Origin | The protocol core (server loop, version negotiation, `Backend`, `Remote`, logging) is ported from VectorCraft's MCP crate, which has the same author and licence. The tool and resource lists are VizCraft's own |

### Protocol

| Method | Behaviour |
|---|---|
| `initialize` | Answers with the client's `protocolVersion` when it is one we speak (`2026-07-28`, `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`), otherwise `2025-06-18`. Capabilities: `tools`, `resources` (the command catalog, the stencil catalog, pages, shapes and the notation schemas; `listChanged: false`), `logging`. `instructions` explain the command model, point at the notation resources and the `issues` keys, and name the backend |
| `ping` | `{}` |
| `tools/list`, `tools/call` | The tools below. An unknown tool name or an argument the tool's schema does not list is a JSON-RPC error (`-32602`); a tool that runs and fails returns `isError: true` with the reason as text |
| `resources/list`, `resources/read` | The resources below. The list holds `vizcraft://commands`, `vizcraft://stencils`, `vizcraft://notation` with one entry per notation pack (`notation.list`), and one `vizcraft://page/{id}` per page of the open document (`page.list`); entries the backend cannot answer are left out. An unknown URI or a missing page, shape, pack or type is `-32002`; a backend failure on a page, shape or catalog read is `-32603` with the reason |
| `resources/templates/list` | `vizcraft://page/{id}`, `vizcraft://shape/{id}`, `vizcraft://notation/{pack}` and `vizcraft://notation/{pack}/{type}`, each with a name, title, description and `application/json` |
| `logging/setLevel` | Turns `notifications/message` log records on at an RFC 5424 level (`null` turns them off) |
| anything else | `-32601` method not found |

Errors use the JSON-RPC codes: `-32700` parse error (bad JSON or not UTF-8), `-32600` invalid
request (not an object, no `method`, an `id` that is neither string nor number, an oversized line
or batch), `-32601`, `-32602`, `-32603` (a bug caught while answering; the server keeps going).
JSON-RPC batch arrays from older clients are answered, up to 64 messages.

The server reads one line, answers it, and reads the next. While a tool call runs it neither
reads nor writes, so progress notifications, cancellation, subscriptions and server-to-client
requests are out of scope until the transport gains a reader thread.

### Backends

| Backend | How it starts | What works |
|---|---|---|
| `Remote` (Cascade 1, built) | `vizcraft-cli mcp --connect 127.0.0.1:7990`, with the app started as `vizcraft --control 7990` | Everything. Each tool call becomes one or more requests on the app's [control channel](control-protocol.md), so the diagram changes live in the window |
| `Headless` (Cascade 3, built) | `vizcraft-cli mcp --headless`, or `vizcraft-cli mcp` with no flags when no app listens | An engine `Session` in the server's own process, starting with a blank document and no window. It answers `engine.execute`, `engine.commands` and the document part of `ui.inspect` (title, path, dirty, rev, selection, undo and redo, view, status, pages, a `diagram` page summary, and `headless: true`). Commands that would open a dialog return their request in the result's `warnings`. Window methods (`ui.click`, `ui.screenshot` and the other `ui.*` input, `app.quit`) and `ui.*` command ids are errors that say there is no window. `screenshot` renders a page offscreen instead (below) |

`ADDR` is `HOST:PORT` or just `PORT`; only loopback addresses are accepted. `--headless` and
`--connect` exclude each other (a usage error).

| Flags | Backend |
|---|---|
| `--headless` | Headless, always |
| `--connect ADDR` | Remote at `ADDR`. The server starts even when no app is listening (MCP clients launch their servers up front): it says so on stderr, every tool call until the app is up fails with a message naming the address and the `vizcraft --control PORT` command to start one, and the first call after the app starts connects |
| none | Remote at the port in `VIZCRAFT_CONTROL_PORT`, otherwise `127.0.0.1:7990`, when an app answers there at startup; otherwise Headless, with a line on stderr saying so. The choice is made once: an app started later is not picked up, so pin `--connect` when the app will come up after the server |

`Remote` keeps one TCP connection, sends one `{"id","method","params"}` line per call and reads
one reply line (at most 64 MB, 90 s). A broken connection is reopened once per call; a timeout is
not retried, because the command may already have run. A reply whose `id` does not match resets
the connection.

The trait is small: call one method with JSON params and get a JSON result or an error message,
report whether a real UI is attached, and describe the backend for logs.

## Tools

### Generic tools (Cascade 1, built)

Every user-visible action in VizCraft is a command with an id, so these five tools already reach
every feature, including ones added after the server ships.

| Tool | Params | Result | Control methods |
|---|---|---|---|
| `command_list` | `prefix?` (string): keep ids that start with it; `brief?` (boolean): only `{id, label}` per command | JSON text `{count, commands}`; each command is `{id, label, location, shortcut, params, enabled, reason?}` | `engine.commands` |
| `command_run` | `id` (string), `params?` (object, default `{}`) | JSON text: the command's result, plus `issues` and `openIssues` (see [Issues on each call](#issues-on-each-call-cascade-4a-built)). A failing command gives `isError: true` and `<id> failed: <reason>` | `engine.execute`; `validate.list` before and after |
| `command_batch` | `commands`: 1 to 256 entries of `{id, params?}` | JSON text `{completed, results: [{id, result}], issues?, openIssues?}`. Every entry is checked before any runs (a bad entry runs nothing and names its index). The list runs as one undo step labelled Agent Batch (`edit.batch`): on a failure nothing changes, and the result has `isError: true` and `{completed: 0, failedIndex, failedId, error, results: [], rolledBack: true, issues?, openIssues?}`. A list containing `edit.undo`, `edit.redo`, `ui.*` or `app.*`, or sent to an app without `edit.batch`, runs each command as its own undo step and stops at the first failure with the earlier commands applied | one `engine.execute` of `edit.batch` (or one per entry in the fallback); `validate.list` before the first and after the last that ran |
| `describe` | `detail?` (`structure`, the default, `layout` or `full`), `format?` (`json`, the default, or `text`), `pages?` (ids, names or indexes; empty means all) | JSON text `{detail, format, pages}` or, with `format: text`, the indented outline as plain text. Structure is ids, kinds and stencils, texts and names, element and relationship ids, glue with connection point indexes, containment and shape data, no coordinates and no style; layout adds integer geometry and route style; full adds style, ext, waypoints and connection points. Read this before `get_page` | `engine.execute` `doc.describe` |
| `doc_inspect` | `page?`: a page id, name or index; `detail?`: as for `describe`, replaces `pages` in the report with the described pages | JSON text: the `ui.inspect` report (title, path, dirty, rev, selection, undo and redo labels, view, dialog, status; on Headless the document part with `headless: true`). With `page`, that page's `page.list` entry `{index, id, name, width, height, shapes, connectors}` is added as `page`; an unknown page is `isError: true` | `ui.inspect`, plus `engine.execute` `page.list` with `page` |
| `screenshot` | `page?` (id, name or index), `window?` (boolean), `scale?` (0.25 to 4) | An `image` block (`image/png`, base64), then a text block `{width, height, bytes, page?}` in pixels. With a window and no arguments: the front window. Given `page` or `scale`, `window: false`, or on Headless: one page (the active one unless `page` says) rendered offscreen on white at `scale` pixels per point (default 1). `window: true` with no window, or together with `page` or `scale`, is `isError: true` | Window: `ui.screenshot`. Page: `engine.execute` `file.export {format: "png", path, page?, scale?}`. Either way to a file in the system temporary directory, read back (at most 48 MB, must be a PNG) and deleted |

To build a diagram in one `command_batch`, name the items: `shape.add` and `connector.add`
take an optional `id` (letters, digits, `_`, `-`, `.`, up to 64 characters, not already used),
so later entries in the same batch can glue connectors to shapes added earlier. Without `id` the
engine picks one and returns it. `doc_inspect` already includes a `diagram` summary of the page
(size, shape and connector counts, selection).

### Issues on each call (Cascade 4A, built)

When the document carries a notation model, live rules check it after every change and the
engine keeps a list of structured issues. So that an agent sees a problem in the reply to the call
that caused it, `command_run` and `command_batch` report them:

| Key | Value |
|---|---|
| `issues` | The issues this call raised: the live issues after the call that were not live before it, in the order the app lists them. Each is `{rule, severity, message, subjects, items, page?}`: `severity` is `error`, `warning` or `info`, `subjects` are element and relationship ids, `items` the shapes and connectors that draw them |
| `openIssues` | How many issues are live after the call, counting older ones |

The server reads the live list with the engine command `validate.list {}` once before the call
and once after it (for a batch, before the first command and after the last one that ran,
including a batch that stopped at a failure; a batch rejected before running reads nothing).
Two issues are the same when rule, subjects, items and page match (the engine's `Issue::key`); a
reworded message does not make an issue new, and an issue fixed by the call simply drops out of
`openIssues`.

Placement: for `command_batch` the keys sit in the result object beside `completed`. For
`command_run` they are added to the command's result object; when that result is not an object,
or already has its own `issues` or `openIssues` key (such as `validate.run` or `validate.list`
run by hand), the result is left exactly as the command returned it and a second text block holds
`{issues, openIssues}`. A failing `command_run` stays a plain text error.

An app without `validate.list` (built before Cascade 4A), a broken connection or a reply without
an `issues` array leaves both keys out; the call itself never fails for it. When the reading
before the call fails, no reading is attempted after it.

A page screenshot is the page's SVG export rasterised on the CPU ([formats.md](formats.md)), so
it needs no window and no GPU and looks the same on both backends; it is the whole page, not the
part in view.

### Diagram tools (Cascade 3, built)

These cover the same workflows as the tools in draw.io's MCP server and reuse its tool names as
feature names; what they do is described here in our own terms. Each one except `get_reference`
is exactly one engine command sent through `engine.execute`, so it behaves the same over the
control channel, in the CLI and in undo. Its result is the command's JSON result as text, the
same as `command_run`; a command that fails gives `isError: true` and `<command> failed:
<reason>`.

Arguments are checked against the tool's input schema before anything is sent: a missing
required argument, an unknown one, a wrong type, a value outside an enum, an empty string where
text is required, or a number out of range is a JSON-RPC error `-32602` whose message names the
argument (and, for enums, the allowed values). `null` counts as not given.

| Tool | Params | Command sent | Result |
|---|---|---|---|
| `open_xml` | `xml` (string), `page?`, `layout?`, `direction?`, `routing?` | `file.import {text: xml, format: "drawio", replace: true, page?, layout?, direction?, routing?}` | `{page, ids, warnings}` |
| `open_mermaid` | `source` (string), `page?`, `layout?`, `direction?`, `routing?` | `file.import {text: source, format: "mermaid", replace: true, ...}` | `{page, ids, warnings}` |
| `open_csv` | `csv` (string), `page?`, `layout?`, `direction?`, `routing?` | `file.import {text: csv, format: "csv", replace: true, ...}` | `{page, ids, warnings}` |
| `list_pages` | `path?` | `page.list {path?}` | `{pages: [{index, id, name, width, height, shapes, connectors}]}` |
| `get_page` | `page`, `path?`, `format?` (`drawio`, the default, or `vizcraft`) | `page.get {page, path?, format?}` | `{page, format, content}`: draw.io `mxGraphModel` XML or VizCraft JSON as a string |
| `set_page` | `page`, `xml` (string), `path?`, `layout?`, `direction?`, `routing?` | `page.set {page, xml, path?, layout?, direction?, routing?}` | `{page, ids, warnings}` |
| `search_shapes` | `query` (string, up to 1000 characters; empty lists every master), `limit?` (1 to 200, default 20) | `shape.search {query, limit?}` | `{masters: [{id, label, category, keywords, w, h, style}]}`; `style` is a draw.io style string, including `vizcraft=<id>`, that opens as that master |
| `get_reference` | `topic?`: `xml`, `styles`, `shapes`, `layout`, `routing`, `mermaid`, `csv` or `model` | none: static text in the server | Plain text (Markdown). Without `topic`, an index of the topics followed by all of them |

Shared params:

| Param | Values | Effect |
|---|---|---|
| `page` | A page id or name (string, 1 to 256 characters) or a zero-based index (integer, 0 to 100000) | The page to read or fill. The open_* tools default to the active page; a name that matches no page adds a page with that name |
| `path` | A file path (string, 1 to 4096 characters), `.drawio` or `.vizcraft` | Read or edit that file on disk instead of the open document. `set_page` with `path` loads the file, replaces or adds the page, and saves it; the open document is untouched |
| `layout` | `layered`, `tree`, `radial`, `force` | Place the imported shapes automatically. XML keeps its own coordinates unless this is given; Mermaid and CSV content is always laid out |
| `direction` | `TB`, `BT`, `LR`, `RL` | Flow direction for the layered and tree layouts |
| `routing` | `orthogonal` | Mark every imported connector to route around shapes; it is rerouted whenever shapes move |

The open_* tools always replace the target page's content (`replace: true`); to add to a page
instead, run `file.import` through `command_run` without `replace`. Imports keep source ids when
they are plain and free, and rename the rest with the glue rewritten. When the app cuts the
imported page off, it zooms to fit it (`view.zoomFit`), so a `screenshot` that follows shows the
whole diagram. Each tool call that changes
the open document is one undo step. The tool annotations mark `list_pages`, `get_page`,
`search_shapes` and `get_reference` read-only, and the open_* tools and `set_page` destructive.

`get_reference` holds our own notes on what the importers accept: the draw.io XML subset (root
and layer cells, vertex and edge cells, geometry with bend points and loose end points, edge
label cells, and what is flattened), the style mapping table, master ids and the `vizcraft=<id>`
style key, the layout and routing options, the five Mermaid kinds and the CSV header directives;
the `model` topic covers model-backed diagrams: notation discovery (the resources below and
`notation.list`, `notation.describe`, `notation.legal`), `element.create`, `relationship.create`,
`validate.run`, `validate.list` and the `issues` keys.
An unknown topic is `-32602` with the list of topics.

The engine command `shape.search` is new with these tools: it reads the stencil library only,
is always enabled and never changes the document. Results are ranked: an exact label or id
first, then word-prefix matches, then keyword matches, then substring matches; ties keep catalog
order. Every word of the query must match the master's id, label, category, set name or keywords.

## Resources

Every resource is read-only JSON (`mimeType: application/json`, returned as `text`) and is read
through engine commands, so it shows the same state over the control channel and headless.

| URI | Content | Cascade |
|---|---|---|
| `vizcraft://commands` | The command catalog as a JSON array, the same list as `command_list`; read through `engine.commands` (built) | 1 |
| `vizcraft://stencils` | `{sets: [{id, name, categories, masters: [{id, label, category, keywords, w, h, style}]}]}`; `style` is the draw.io style string that opens as the master; read through `shape.catalog` (built) | 3 |
| `vizcraft://page/{id}` | One page at the `structure` detail level by default (see `describe`); `?detail=layout` or `?detail=full` for more, any other query is `-32602`; read through `page.describe {page: id, detail}` (built; listed once per page, and a template) | 3, 2C |
| `vizcraft://shape/{id}` | One shape at the `structure` detail level by default, with its `connections: [{connector, end, from, to}]` (`end` says which end of the connector is glued to this shape: `start`, `end` or `both`) and `children`; `?detail=layout` or `?detail=full` adds geometry, then style, ext and connection points; every page is searched; read through `shape.describe {id, detail}` (built; a template) | 3, 2C |
| `vizcraft://notation` | The notation packs the app knows: the result of `notation.list {}`, `{packs: [{id, version, name, audience, description, types}]}` (built) | 4A |
| `vizcraft://notation/{pack}` | One pack's schema: the result of `notation.describe {pack}` (built; listed once per pack, and a template) | 4A |
| `vizcraft://notation/{pack}/{type}` | One element or relationship type with its properties, ports and legal relationships: the result of `notation.describe {pack, type}`. Type ids are pack-qualified, such as `vizcraft://notation/flowchart/flowchart.process` (built; a template) | 4A |

For pages and shapes, `{id}` is percent-decoded (UTF-8) and at most 256 bytes once decoded;
listed page URIs encode every character outside `A-Z a-z 0-9 - . _ ~`. A page URI also accepts
what `page.get` does (a page name or index), though the list always uses ids. A bad escape, an
empty or overlong id, an unknown URI, or a page or shape that does not exist is `-32002`. When the
backend fails (no app running, a timeout), reading is `-32603` with the reason and the list leaves
out the entries it could not get. The engine commands `shape.get {id, page?}` and
`shape.catalog {}` are new with these resources: both are always enabled and never change the
document.

The notation resources are `application/json` text, read live from the app through
`engine.execute`, so they always match the packs it has loaded. A URI is at most 300 characters;
pack and type ids are letters, digits and `_`, with `.` only between two names, at most 100
characters each. Any other form (an empty segment, a trailing slash, a query, percent escapes,
extra segments) is `-32002` without asking the app. A pack or type the app does not know, or an
app without the notation commands, is `-32002` too, with the app's reason in the message.

## Registering with a client

Build the CLI, then point the client's MCP configuration at it:

```json
{
  "mcpServers": {
    "vizcraft": {
      "command": "vizcraft-cli",
      "args": ["mcp"]
    }
  }
}
```

Use an absolute path to the binary when it is not on `PATH`, and add
`"--connect", "127.0.0.1:7990"` to `args` to pin the address, or `"--headless"` to always work
without a window. For a live session, start the app with `vizcraft --control 7990`.

To try it by hand, pipe JSON-RPC lines in:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"command_list","arguments":{"prefix":"view."}}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"screenshot","arguments":{}}}' \
  | vizcraft-cli mcp --connect 7990
```

## Safety

- Loopback only. `Remote` connects to `127.0.0.1`; the control channel it talks to listens on
  loopback and nothing else. The server opens no network port of its own. `Headless` opens no
  port at all; its file commands (`file.open`, `file.export`, `path` params) read and write the
  local disk with the server's own permissions, the same as `vizcraft-cli run`.
- Every input is hostile: JSON-RPC messages, tool params, XML, Mermaid, CSV and file paths alike.
  Sizes are bounded, numbers are clamped, and parsers are fed truncated and garbage input in
  tests. Built limits: 4 MB per incoming line (longer lines are read to their end, dropped and
  answered with `-32600`), 256 commands per `command_batch`, 64 messages per JSON-RPC batch,
  200 characters per command id, 64 MB per reply from the app, 48 MB per screenshot, 8192 pixels
  a side and a scale of 0.25 to 4 per page screenshot, 256 bytes per resource id. Node
  counts and image dimensions for the Cascade 3 importers are planned.
- No panics. Errors come back as JSON-RPC errors or as tool results flagged as errors; the
  crate root carries the same `deny` list as every production crate ([AGENTS.md](../AGENTS.md)).
- A malformed line gets a parse error and the server keeps reading; it never exits because of
  what a client sent.
