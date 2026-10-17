# MCP server (planned)

None of this is built yet. This page is the design for VizCraft's
[Model Context Protocol](https://modelcontextprotocol.io) server, so agents can create, edit and
look at diagrams. It lands in two steps: a minimal server with the generic command tools in
Cascade 1, and the diagram tools that match draw.io's MCP server feature for feature in
Cascade 3 ([ROADMAP.md](../ROADMAP.md)). Until then, agents drive the running app through the
[control protocol](control-protocol.md) directly.

## Design

| Piece | Plan |
|---|---|
| Crate | `crates/mcp` (`vizcraft-mcp`), in the tooling layer next to the apps ([architecture.md](architecture.md)) |
| Transport | Newline-delimited JSON-RPC 2.0 over stdio, written by hand; no async runtime. Stdout carries only protocol messages, logs go to stderr |
| Launch | `vizcraft-cli mcp [--connect ADDR \| --headless]` |
| Backends | A `Backend` trait with two implementations, `Remote` and `Headless` (below) |
| Origin | The protocol core (server loop, version negotiation, `Backend`, `Remote`, `Headless` skeleton, logging) is ported from VectorCraft's MCP crate, which has the same author and licence. The tool and resource lists are VizCraft's own |

The server reads one line, answers it, and reads the next. While a tool call runs it neither
reads nor writes, so progress notifications, cancellation, subscriptions and server-to-client
requests are out of scope until the transport gains a reader thread.

### Backends

| Backend | How it starts | What works |
|---|---|---|
| `Remote` | `vizcraft-cli mcp --connect 127.0.0.1:7990`, with the app started as `vizcraft --control 7990` | Everything. Each tool call becomes one or more requests on the app's [control channel](control-protocol.md), so the diagram changes live in the window |
| `Headless` | `vizcraft-cli mcp --headless` | An in-process engine `Session` that answers the same method names itself, with screenshots rendered offscreen. UI-only methods (`ui.*` input, `ui.inspect`) are unavailable and say so |

With neither flag the server tries the default address and falls back to headless. The trait is
small: call one method with JSON params and get a JSON result or an error message, report
whether a real UI is attached, and describe the backend for logs.

## Tools

### Generic tools (Cascade 1)

Every user-visible action in VizCraft is a command with an id, so these five tools already reach
every feature, including ones added after the server ships.

| Tool | Purpose | Key params | Cascade |
|---|---|---|---|
| `command_list` | List every command with its label, params doc and whether it is enabled now | `prefix?` to filter by id prefix | 1 |
| `command_run` | Run one command by id | `id`, `params?` | 1 |
| `command_batch` | Run several commands in order, stopping at the first failure | `commands` (array of `{id, params?}`) | 1 |
| `doc_inspect` | Return a summary of the active document: pages, shapes, selection, revision, dirty flag | `page?` | 1 |
| `screenshot` | Render the active page (or the whole window in `Remote`) and return it as a PNG image | `page?`, `window?`, `scale?` | 1 |

### Diagram tools (Cascade 3)

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
| `vizcraft://commands` | The command catalog, the same list as `command_list` | 1 |
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

Use an absolute path to the binary when it is not on `PATH`, and add `"--headless"` or
`"--connect", "127.0.0.1:7990"` to `args` to pin a backend. For a live session, start the app
first with `vizcraft --control 7990`.

## Safety

- Loopback only. `Remote` connects to `127.0.0.1`; the control channel it talks to listens on
  loopback and nothing else. The server opens no network port of its own.
- Every input is hostile: JSON-RPC messages, tool params, XML, Mermaid, CSV and file paths alike.
  Sizes are bounded (message length, node counts, nesting depth, image dimensions), numbers are
  clamped, and parsers are fed truncated and garbage input in tests.
- No panics. Errors come back as JSON-RPC errors or as tool results flagged as errors; the
  crate root carries the same `deny` list as every production crate ([AGENTS.md](../AGENTS.md)).
- A malformed line gets a parse error and the server keeps reading; it never exits because of
  what a client sent.
