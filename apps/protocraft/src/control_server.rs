//! Loopback JSON-lines control server: one request per line, one reply per line. Requests are
//! handed to the UI thread through a channel and answered when it has handled them, so every
//! method sees the app in a consistent state between frames.
//!
//! Each client gets its own thread, so several agents can be connected at once; a client that
//! goes away mid-request is simply dropped. A request the UI does not answer within
//! [`REPLY_TIMEOUT`] (the window is hidden, or a modal has the thread) gets `timeout` back and the
//! connection stays usable. Port `0` asks the OS for a free port; the chosen one is printed as
//! `protocraft: control server listening on 127.0.0.1:<port>` so a script can parse it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use protocraft_ui_egui::ControlRequest;
use serde_json::{Value, json};

/// How long a client waits for the UI thread before the request is answered with `timeout`.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

/// Longest request line accepted (a client that sends more is cut off).
const MAX_LINE: u64 = 16 * 1024 * 1024;

/// Listen on `127.0.0.1:port` (`0` picks a free port); requests arrive on the returned receiver.
/// A failed bind is reported on stderr and leaves the app running without control.
pub fn start(port: u16, ctx: egui::Context) -> Receiver<ControlRequest> {
    let (tx, rx) = channel::<ControlRequest>();
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("protocraft: control server failed to bind 127.0.0.1:{port}: {e} (is another instance using --control {port}?)");
            return rx;
        }
    };
    let bound = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    eprintln!("protocraft: control server listening on 127.0.0.1:{bound}");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                // A refused or reset connection is the client's problem; keep listening.
                Err(_) => continue,
            };
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || serve(stream, tx, ctx));
        }
    });
    rx
}

/// Build the reply for one request line: parse, forward to the UI, wait for its answer.
fn answer(line: &str, tx: &Sender<ControlRequest>, ctx: &egui::Context) -> Option<Value> {
    let msg = match serde_json::from_str::<Value>(line) {
        Ok(m) => m,
        Err(e) => return Some(json!({"ok": false, "error": format!("bad JSON: {e}")})),
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let with_id = |mut r: Value| {
        if let Some(o) = r.as_object_mut() {
            o.insert("id".into(), id.clone());
        }
        r
    };
    if !msg.is_object() {
        return Some(with_id(json!({"ok": false, "error": "bad request: expected an object with `method` and `params`"})));
    }
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return Some(with_id(json!({"ok": false, "error": "bad request: missing `method`"})));
    };
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let (req, rrx) = ControlRequest::new(method, params);
    if tx.send(req).is_err() {
        // The app dropped the channel: it is quitting.
        return None;
    }
    // The UI thread only runs when something asks it to.
    ctx.request_repaint();
    let r = rrx.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|_| json!({"ok": false, "error": "timeout"}));
    Some(with_id(r))
}

fn serve(stream: TcpStream, tx: Sender<ControlRequest>, ctx: egui::Context) {
    let Ok(read) = stream.try_clone() else { return };
    let _ = stream.set_nodelay(true);
    let mut out = stream;
    let mut reader = BufReader::new(read);
    loop {
        let mut line = String::new();
        let n = match (&mut reader).take(MAX_LINE).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if n as u64 >= MAX_LINE && !line.ends_with('\n') {
            let _ = writeln!(out, "{}", json!({"ok": false, "error": "request line too long"}));
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = answer(line.trim_end(), &tx, &ctx) else { break };
        if writeln!(out, "{reply}").is_err() || out.flush().is_err() {
            break;
        }
    }
}
