# MCP server

VizCraft's [Model Context Protocol](https://modelcontextprotocol.io) server lets agents create,
edit and look at diagrams. It lands in two steps: a minimal server with the generic command tools
in Cascade 1 (**built**), and the diagram tools that match draw.io's MCP server feature for
feature in Cascade 3 (**planned**, [ROADMAP.md](../ROADMAP.md)). Each section below says which.

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
| `initialize` | Answers with the client's `protocolVersion` when it is one we speak (`2026-07-28`, `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`), otherwise `2025-06-18`. Capabilities: `tools`, `resources`, `logging`. `instructions` explain the command model and name the backend |
| `ping` | `{}` |
| `tools/list`, `tools/call` | The tools below. An unknown tool name or an argument the tool's schema does not list is a JSON-RPC error (`-32602`); a tool that runs and fails returns `isError: true` with the reason as text |
| `resources/list`, `resources/read` | The resources below; an unknown URI is `-32002` |
| `resources/templates/list` | An empty list in Cascade 1 |
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
| `command_run` | `id` (string), `params?` (object, default `{}`) | JSON text: the command's result. A failing command gives `isError: true` and `<id> failed: <reason>` | `engine.execute` |
| `command_batch` | `commands`: 1 to 256 entries of `{id, params?}` | JSON text `{completed, results: [{id, result}]}`. Every entry is checked before any runs (a bad entry runs nothing and names its index). At the first failing command it stops with `isError: true` and `{completed, failedIndex, failedId, error, results}`; the commands before it stay applied, each its own undo step | `engine.execute`, once per entry |
| `doc_inspect` | none | JSON text: the `ui.inspect` report (title, path, dirty, rev, selection, undo and redo labels, view, dialog, status) | `ui.inspect` |
| `screenshot` | none | An `image` block (`image/png`, base64) of the front window, then a text block `{width, height, bytes}` in physical pixels | `ui.screenshot` to a file in the system temporary directory, read back (at most 48 MB, must be a PNG) and deleted |

To build a diagram in one `command_batch`, name the items: `shape.add` and `connector.add`
take an optional `id` (letters, digits, `_`, `-`, `.`, up to 64 characters, not already used),
so later entries in the same batch can glue connectors to shapes added earlier. Without `id` the
engine picks one and returns it. `doc_inspect` already includes a `diagram` summary of the page
(size, shape and connector counts, selection).

Planned for Cascade 3: `doc_inspect {page?}` to report a chosen page, and
`screenshot {page?, window?, scale?}` to render one page offscreen instead of the window.

### Diagram tools (Cascade 3, planned)

These cover the same features as the tools in draw.io's MCP server and reuse its tool names as
feature names; what they do is described here in our own terms. The page tools act on the
active document, or on a `.drawio` or `.vizcraft` file when a `path` is given.

| Tool | Purpose | Key params | Cascade |
|---|---|---|---|
| `open_xml` | Load a draw.io `mxGraphModel` XML diagram into a page, new or existing | `xml`, `page?`, `layout?`, `routing?` | 3 |
| `open_mermaid` | Turn Mermaid source (flowchart, sequence, class, state, ER) into shapes and connectors on a page | `source`, `page?`, `layout?`, `routing?` | 3 |
| `open_csv` | Build a diagram from CSV text with a directive header, as described in [formats.md](formats.md#csv) | `csv`, `page?`, `layout?`, `routing?` | 3 |
| `search_shapes` | Search VizCraft's own stencil library by keyword and return matching masters with their style strings | `query`, `limit?` | 3 |
| `get_reference` | Return a reference for agents that generate diagrams: the XML subset we accept, style keys, stencil naming, layout and routing options | `topic?` | 3 |
| `list_pages` | List the pages of a document with ids, names and sizes | `path?` | 3 |
| `get_page` | Return one page as draw.io XML (or VizCraft JSON) | `page`, `path?`, `format?` | 3 |
| `set_page` | Replace one page's content from draw.io XML, creating the page if needed | `page`, `xml`, `path?`, `layout?`, `routing?` | 3 |

Options shared by the tools that create content:

| Option | Values | Effect |
|---|---|---|
| `layout` | `layered`, `tree`, `radial`, `force` | Run the auto-layout engine over the imported shapes before they are placed |
| `routing` | `orthogonal` | Reroute every connector as an orthogonal path that avoids shapes |

Each tool call is one undo step in the document it changed.

## Resources

| URI | Content | Cascade |
|---|---|---|
| `vizcraft://page/{id}` | One page as JSON: shapes, connectors, layers, size | 3 |
| `vizcraft://shape/{id}` | One shape as JSON: geometry, text, style, shape data, connections | 3 |
| `vizcraft://commands` | The command catalog as a JSON array, the same list as `command_list` (built) | 1 |
| `vizcraft://stencils` | The stencil catalog: categories and masters with keywords | 3 |

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
