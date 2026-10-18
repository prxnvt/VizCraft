//! Where MCP tool calls end up: a control-channel method call.

use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{Value, json};

use crate::line::{Line, read_bounded};

/// Something that answers control-channel methods (`engine.execute`, `engine.commands`,
/// `ui.inspect`, `ui.screenshot`, ...). The method list is in docs/control-protocol.md.
pub trait Backend {
    /// Call one method. `Ok` carries the `result`, `Err` the error message.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// True when a real UI is attached, so `ui.*` methods such as `ui.inspect` work.
    fn has_ui(&self) -> bool;
    /// Short human description for logs and the `initialize` instructions.
    fn describe(&self) -> String;
}

/// Longest reply line accepted from the app. The command catalog is the largest regular reply;
/// this leaves plenty of room while still bounding memory if the peer misbehaves.
const MAX_REPLY_BYTES: usize = 64 * 1024 * 1024;

/// How long to wait for the TCP connection itself; the app is local, so this is generous.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);

/// How long to wait for a reply. The app gives its UI 60 s before it answers `timeout` itself
/// (screenshots come a few frames late), so this leaves headroom over that.
const READ_TIMEOUT: Duration = Duration::from_secs(90);

/// A running VizCraft app, reached through its loopback control port.
///
/// The connection is opened lazily and reopened once per call if it broke (the app may have
/// restarted), so the server can start before the app does.
#[derive(Debug)]
pub struct Remote {
    addr: SocketAddr,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u64,
}

impl Remote {
    /// Resolve `addr` (`127.0.0.1:7990`, `localhost:7990`) without connecting yet.
    ///
    /// Only loopback addresses are accepted: the control channel listens on loopback alone, and
    /// a server that forwarded commands across the network would be a remote-control hole.
    pub fn new(addr: &str) -> Result<Self, String> {
        let resolved: Vec<SocketAddr> = addr.to_socket_addrs().map_err(|e| format!("bad address `{addr}`: {e}"))?.collect();
        let Some(sa) = resolved.iter().find(|sa| sa.ip().is_loopback()).copied() else {
            return Err(if resolved.is_empty() {
                format!("bad address `{addr}`: it resolves to nothing")
            } else {
                format!("`{addr}` is not a loopback address; the VizCraft control channel only listens on 127.0.0.1")
            });
        };
        Ok(Self { addr: sa, conn: None, next_id: 1 })
    }

    /// Resolve and connect at once, failing when nothing is listening.
    pub fn connect(addr: &str) -> Result<Self, String> {
        let mut r = Self::new(addr)?;
        r.probe()?;
        Ok(r)
    }

    /// The address this backend talks to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Open the connection now if it is not open, with the same message a tool call would give.
    pub fn probe(&mut self) -> Result<(), String> {
        if self.conn.is_some() {
            return Ok(());
        }
        self.reconnect().map_err(|e| self.unreachable(&e))
    }

    /// The message for an app that cannot be reached, with the way to start one.
    fn unreachable(&self, e: &std::io::Error) -> String {
        format!(
            "no VizCraft app is listening at {addr} ({e}); start it with `vizcraft --control {port}` or pass `--connect` with the right address",
            addr = self.addr,
            port = self.addr.port()
        )
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        self.conn = None;
        let s = TcpStream::connect_timeout(&self.addr, CONNECT_TIMEOUT)?;
        // Small request lines; send them now rather than waiting for more.
        s.set_nodelay(true).ok();
        s.set_read_timeout(Some(READ_TIMEOUT)).ok();
        let read = s.try_clone()?;
        self.conn = Some((BufReader::new(read), s));
        Ok(())
    }

    fn roundtrip(&mut self, line: &str) -> std::io::Result<String> {
        if self.conn.is_none() {
            self.reconnect()?;
        }
        let Some((reader, writer)) = self.conn.as_mut() else {
            return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "not connected to the app"));
        };
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        match read_bounded(reader, MAX_REPLY_BYTES)? {
            Some(Line::Text(reply)) => Ok(reply),
            Some(Line::TooLong) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "reply from the app is too long")),
            Some(Line::NotUtf8) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "reply from the app is not UTF-8")),
            None => Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the app closed the control connection")),
        }
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        // Commands run on behalf of an MCP client are logged with the Mcp origin, not Control.
        if method == "engine.execute"
            && let Some(map) = params.as_object_mut()
        {
            map.entry("origin").or_insert_with(|| Value::String("mcp".into()));
        }
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let reply = match self.roundtrip(&line) {
            Ok(r) => r,
            Err(first) => {
                // A timeout means the app is alive but slow; sending the request again could run
                // a command twice, so only a broken or missing connection is retried.
                if matches!(first.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
                    self.conn = None;
                    return Err(format!("the VizCraft app at {} did not answer `{method}` in time", self.addr));
                }
                self.conn = None;
                self.roundtrip(&line).map_err(|e| {
                    self.conn = None;
                    self.unreachable(&e)
                })?
            }
        };
        let v: Value = serde_json::from_str(reply.trim()).map_err(|e| format!("bad reply from the app: {e}"))?;
        // The connection is strictly one request, one reply; a reply for another id means the
        // stream is out of step, and reading on would pair every later reply with the wrong call.
        if v.get("id").is_some_and(|got| *got != json!(id)) {
            self.conn = None;
            return Err("the app answered out of order; the connection was reset".into());
        }
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("error").and_then(Value::as_str).unwrap_or("the app reported an error without a message").to_string())
        }
    }

    fn has_ui(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        format!("remote app at {}", self.addr)
    }
}
