# MCP server

VizCraft's [Model Context Protocol](https://modelcontextprotocol.io) server lets agents create,
edit and look at diagrams. It lands in two steps: a minimal server with the generic command tools
in Cascade 1 (**built**), and the diagram tools that match draw.io's MCP server feature for
feature in Cascade 3 (**built**; the Headless backend, the page and shape resources and
`screenshot {page?}` are still **planned**, [ROADMAP.md](../ROADMAP.md)). Cascade 4A adds the
notation schema resources and the issues reported with each command call (**built**). Each
section below says which.

## Design

| Piece | State |
|---|---|
| Crate | Built: `crates/mcp` (`vizcraft-mcp`), between the engine and the apps; it depends on `serde_json` and `log` only, never on `ui-egui` ([architecture.md](architecture.md)) |
| Transport | Built: newline-delimited JSON-RPC 2.0 over stdio, written by hand; no async runtime. Stdout carries only protocol messages; the CLI's own messages go to stderr |
| Launch | Built: `vizcraft-cli mcp [--connect ADDR]`. `--headless` is planned (Cascade 3) and is a usage error until then |
| Backends | A `Backend` trait; `Remote` is built, `Headless` is planned (below) |
| Origin | The protocol core (server loop, version negotiation, `Backend`, `Remote`, logging) is ported from VectorCraft's MCP crate, which has the same author and licence. The tool and resource lists are VizCraft's own |

### Protocol

| Method | Behaviour |
|---|---|
| `initialize` | Answers with the client's `protocolVersion` when it is one we speak (`2026-07-28`, `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`), otherwise `2025-06-18`. Capabilities: `tools`, `resources` (the command catalog and the notation schemas; `listChanged: false`), `logging`. `instructions` explain the command model, point at the notation resources and the `issues` keys, and name the backend |
| `ping` | `{}` |
| `tools/list`, `tools/call` | The tools below. An unknown tool name or an argument the tool's schema does not list is a JSON-RPC error (`-32602`); a tool that runs and fails returns `isError: true` with the reason as text |
| `resources/list`, `resources/read` | The resources below; an unknown URI is `-32002`. The list asks the app for its notation packs (`notation.list`) and adds one entry per pack; when the app cannot answer, only the fixed entries are listed |
| `resources/templates/list` | `vizcraft://notation/{pack}` and `vizcraft://notation/{pack}/{type}`; the page and shape templates are planned |
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
| `Headless` (Cascade 3, planned) | `vizcraft-cli mcp --headless` | An in-process engine `Session` that answers the same method names itself, with screenshots rendered offscreen. UI-only methods (`ui.*` input, `ui.inspect`) are unavailable and say so |

`ADDR` is `HOST:PORT` or just `PORT`. Without `--connect` the server uses the port in
`VIZCRAFT_CONTROL_PORT` when it is set, otherwise `127.0.0.1:7990`. Only loopback addresses are
accepted. The server starts even when no app is listening (MCP clients launch their servers up
front): it says so on stderr, and every tool call until the app is up fails with a message naming
the address and the `vizcraft --control PORT` command to start one. The first call after the app
starts connects. Once Headless exists, a missing app will fall back to it instead.

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
| `command_list` | `prefix?` (string): keep ids that start with it | JSON text `{count, commands}`; each command is `{id, label, location, shortcut, params, enabled, reason?}` | `engine.commands` |
| `command_run` | `id` (string), `params?` (object, default `{}`) | JSON text: the command's result, plus `issues` and `openIssues` (see [Issues on each call](#issues-on-each-call-cascade-4a-built)). A failing command gives `isError: true` and `<id> failed: <reason>` | `engine.execute`; `validate.list` before and after |
| `command_batch` | `commands`: 1 to 256 entries of `{id, params?}` | JSON text `{completed, results: [{id, result}], issues?, openIssues?}`. Every entry is checked before any runs (a bad entry runs nothing and names its index). At the first failing command it stops with `isError: true` and `{completed, failedIndex, failedId, error, results, issues?, openIssues?}`; the commands before it stay applied, each its own undo step | `engine.execute`, once per entry; `validate.list` before the first and after the last that ran |
| `doc_inspect` | none | JSON text: the `ui.inspect` report (title, path, dirty, rev, selection, undo and redo labels, view, dialog, status) | `ui.inspect` |
| `screenshot` | none | An `image` block (`image/png`, base64) of the front window, then a text block `{width, height, bytes}` in physical pixels | `ui.screenshot` to a file in the system temporary directory, read back (at most 48 MB, must be a PNG) and deleted |

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

Planned for Cascade 3: `doc_inspect {page?}` to report a chosen page, and
`screenshot {page?, window?, scale?}` to render one page offscreen instead of the window.

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
| `page` | A page id or name (string, 1 to 256 characters) or a zero-based index (integer, 0 to 100000) | The page to read or fill. The open_* tools default to the first page; a name that matches no page adds a page with that name |
| `path` | A file path (string, 1 to 4096 characters), `.drawio` or `.vizcraft` | Read or edit that file on disk instead of the open document. `set_page` with `path` loads the file, replaces or adds the page, and saves it; the open document is untouched |
| `layout` | `layered`, `tree`, `radial`, `force` | Place the imported shapes automatically. XML keeps its own coordinates unless this is given; Mermaid and CSV content is always laid out |
| `direction` | `TB`, `BT`, `LR`, `RL` | Flow direction for the layered and tree layouts |
| `routing` | `orthogonal` | Mark every imported connector to route around shapes; it is rerouted whenever shapes move |

The open_* tools always replace the target page's content (`replace: true`); to add to a page
instead, run `file.import` through `command_run` without `replace`. Imports keep source ids when
they are plain and free, and rename the rest with the glue rewritten. Each tool call that changes
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
is always enabled and never changes the document.

## Resources

| URI | Content | Cascade |
|---|---|---|
| `vizcraft://page/{id}` | One page as JSON: shapes, connectors, layers, size | 3 |
| `vizcraft://shape/{id}` | One shape as JSON: geometry, text, style, shape data, connections | 3 |
| `vizcraft://commands` | The command catalog as a JSON array, the same list as `command_list` (built) | 1 |
| `vizcraft://stencils` | The stencil catalog: categories and masters with keywords | 3 |
| `vizcraft://notation` | The notation packs the app knows: the result of `notation.list {}`, `{packs: [{id, version, name, audience, description, types}]}` (built) | 4A |
| `vizcraft://notation/{pack}` | One pack's schema: the result of `notation.describe {pack}` (built; listed once per pack, and a template) | 4A |
| `vizcraft://notation/{pack}/{type}` | One element or relationship type with its properties, ports and legal relationships: the result of `notation.describe {pack, type}`. Type ids are pack-qualified, such as `vizcraft://notation/flowchart/flowchart.process` (built; a template) | 4A |

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
`"--connect", "127.0.0.1:7990"` to `args` to pin the address (`"--headless"` once Cascade 3
lands). For a live session, start the app with `vizcraft --control 7990`.

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
  loopback and nothing else. The server opens no network port of its own.
- Every input is hostile: JSON-RPC messages, tool params, XML, Mermaid, CSV and file paths alike.
  Sizes are bounded, numbers are clamped, and parsers are fed truncated and garbage input in
  tests. Built limits: 4 MB per incoming line (longer lines are read to their end, dropped and
  answered with `-32600`), 256 commands per `command_batch`, 64 messages per JSON-RPC batch,
  200 characters per command id, 64 MB per reply from the app, 48 MB per screenshot. Node
  counts and image dimensions for the Cascade 3 importers are planned.
- No panics. Errors come back as JSON-RPC errors or as tool results flagged as errors; the
  crate root carries the same `deny` list as every production crate ([AGENTS.md](../AGENTS.md)).
- A malformed line gets a parse error and the server keeps reading; it never exits because of
  what a client sent.
