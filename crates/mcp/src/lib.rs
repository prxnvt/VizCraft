//! VizCraft's MCP server.
//!
//! [Model Context Protocol](https://modelcontextprotocol.io) over stdio: newline-delimited
//! JSON-RPC 2.0, written by hand with no async runtime. The server offers VizCraft's command
//! registry as MCP tools and resources and forwards every call to a [`Backend`]:
//!
//! - [`Remote`] talks to a running app through its loopback control channel
//!   (`vizcraft --control 7990`): one `{"id","method","params"}` line out, one
//!   `{"id","ok","result"|"error"}` line back (docs/control-protocol.md).
//!
//! Tools: the generic command tools (`command_list`, `command_run`, `command_batch`,
//! `doc_inspect`, `screenshot`) and the diagram tools (`open_xml`, `open_mermaid`, `open_csv`,
//! `list_pages`, `get_page`, `set_page`, `search_shapes`, `get_reference`), each of which is one
//! engine command. An in-process headless backend is planned for Cascade 3 (docs/mcp.md).
//!
//! Entry points: [`Server::serve`] (the stdio loop) and [`Server::handle_line`] (one message).
//! Stdout carries protocol messages only; nothing in this crate prints.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod backend;
mod base64;
mod diagram;
mod line;
pub mod logging;
mod reference;
mod resources;
mod server;
mod tools;

pub use backend::{Backend, Remote};
pub use base64::encode as base64_encode;
pub use server::{PROTOCOL_VERSION, Server};
pub use tools::{ToolResult, call_tool, tool_definitions};

/// Default control-channel address of the desktop app.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7990";

/// The one resource Cascade 1 serves: the command catalog.
pub const COMMANDS_URI: &str = "vizcraft://commands";

/// Longest JSON-RPC line the server accepts from its client, in bytes. Longer lines are answered
/// with an error and skipped, so a runaway client cannot grow the server's memory without bound.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// Most commands one `command_batch` call may carry.
pub const MAX_BATCH: usize = 256;

#[cfg(test)]
mod tests;
