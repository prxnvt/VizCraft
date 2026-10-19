//! VizCraft command line: the engine without a window, and a one-shot client for the running
//! app's control channel.
//!
//! ```text
//! vizcraft-cli commands [--json]                           every engine command: id, label, shortcut, enabled
//! vizcraft-cli run [--open FILE | --sample] --cmd ID[=JSON] ... [--save OUT] [--json]
//!                                                            run commands headless, in order, then save
//! vizcraft-cli inspect FILE [--json]                       document summary (or the document as JSON)
//! vizcraft-cli validate FILE|DIR [--ruleset ID]... [--format text|json] [--packs DIR]
//!                                                            run the rules headless on a document, a pack
//!                                                            test case or a folder of cases
//! vizcraft-cli generate FILE [--spec SPEC | --target ID --output DIR] [--check | --preview] [--json]
//!                                                            run a generator over the model; --check for CI
//! vizcraft-cli analyze FILE --kind KIND [--id ID] [--from ID] [--to ID] [--direction D] [--depth N]
//!                      [--types T,T] [--against FILE] [--check] [--json]
//!                                                            graph analysis or model compare; --check for CI
//! vizcraft-cli glyph BUILDER [--param K=V]... [--module N] one parametric glyph as SVG on stdout
//! vizcraft-cli control HOST:PORT METHOD [JSON]             one request to a running app, reply on stdout
//! vizcraft-cli control HOST:PORT -                         forward request lines from stdin, one reply each
//! vizcraft-cli mcp [--connect ADDR | --headless]           MCP server on stdio, driving the running app or,
//!                                                            with --headless or when no app listens, an
//!                                                            engine session in this process
//! vizcraft-cli --version
//! ```
//!
//! Exit codes: 0 success, 1 a command, file or request failed (the reason is on stderr, or in
//! the reply for `control`), 2 usage error.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};
use vizcraft_engine::{Document, Session};

mod analyze;
mod generate;
mod glyph;
mod validate;

const USAGE: &str = "\
vizcraft-cli: VizCraft from the command line

USAGE:
  vizcraft-cli commands [--json]                       every engine command: id, label, shortcut, enabled
  vizcraft-cli run [--open FILE | --sample] --cmd ID[=JSON] ... [--save OUT] [--json]
                                                         run commands headless, in order, then save
  vizcraft-cli inspect FILE [--json]                   document summary (or the whole document as JSON)
  vizcraft-cli validate FILE|DIR [--ruleset ID]... [--format text|json] [--packs DIR]
                                                         run the rules headless: a document (exit 1 on an
                                                         error issue), a pack test case or a folder of cases
                                                         (exit 1 when one fails); packs also load from the
                                                         folder in VIZCRAFT_PACKS_DIR
  vizcraft-cli generate FILE [--spec SPEC | --target ID --output DIR] [--check | --preview] [--renames] [--json]
                                                         run a generator over the document's model (default:
                                                         the .vizcraft-gen.toml beside FILE); --check exits 1
                                                         when the output is stale, a run exits 1 when hand
                                                         edits conflict
  vizcraft-cli analyze FILE --kind KIND [--id ID] [--from ID] [--to ID] [--direction D] [--depth N]
                       [--types T,T] [--against FILE] [--check] [--json]
                                                         graph analysis over the model: KIND is dependencies,
                                                         cycles, unreachable, spof, metrics, path or diff
                                                         (FILE against --against); --check exits 1 when cycles,
                                                         unreachable elements, single points of failure or
                                                         differences are found
  vizcraft-cli glyph BUILDER [--param KEY=VALUE]... [--module N]
                                                         draw one parametric glyph (a builder name or a
                                                         pack glyph id) and print its SVG; warnings on stderr
  vizcraft-cli control HOST:PORT METHOD [JSON]         one request to a running app (started with --control PORT)
  vizcraft-cli control HOST:PORT -                     forward request lines from stdin, one reply line each
  vizcraft-cli mcp [--connect ADDR | --headless]      MCP server (JSON-RPC on stdin and stdout). --connect drives
                                                         the app at ADDR (HOST:PORT or PORT) and waits for it;
                                                         --headless works on a document in this process with no
                                                         window; with neither it drives the app at 127.0.0.1:7990
                                                         (or the port in VIZCRAFT_CONTROL_PORT) when one listens
                                                         there and runs headless otherwise
  vizcraft-cli --version

Exit codes: 0 success, 1 a command, file or request failed, 2 usage error.
";

/// Why a subcommand did not finish: a usage mistake (exit 2) or a failure (exit 1).
enum CliError {
    Usage(String),
    Failed(String),
}

fn usage(msg: impl Into<String>) -> CliError {
    CliError::Usage(msg.into())
}
fn failed(msg: impl Into<String>) -> CliError {
    CliError::Failed(msg.into())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("commands") => commands(&args[1..]),
        Some("run") => run(&args[1..]),
        Some("inspect") => inspect(&args[1..]),
        Some("validate") => validate::validate(&args[1..]),
        Some("generate") => generate::generate(&args[1..]),
        Some("analyze") => analyze::analyze(&args[1..]),
        Some("glyph") => glyph::glyph(&args[1..]),
        Some("control") => control(&args[1..]),
        Some("mcp") => mcp(&args[1..]),
        Some("--version" | "-V") => {
            println!("vizcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(usage(format!("unknown subcommand `{other}`"))),
        None => Err(usage("a subcommand is required")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(m)) => {
            eprintln!("vizcraft-cli: {m}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(m)) => {
            eprintln!("vizcraft-cli: {m}");
            ExitCode::FAILURE
        }
    }
}

/// `--json` anywhere in `args`; everything else is returned for the subcommand to parse.
fn take_json_flag(args: &[String]) -> (bool, Vec<String>) {
    let json = args.iter().any(|a| a == "--json");
    (json, args.iter().filter(|a| *a != "--json").cloned().collect())
}

/// `commands [--json]`: every engine command (enablement is for a fresh blank document). The
/// `ui.*` and `app.*` commands live in the UI layer; `control HOST:PORT engine.commands` lists
/// those too.
fn commands(args: &[String]) -> Result<(), CliError> {
    let (as_json, rest) = take_json_flag(args);
    if let Some(a) = rest.first() {
        return Err(usage(format!("commands: unexpected argument `{a}`")));
    }
    let s = Session::new(Document::new());
    let infos = s.commands();
    if as_json {
        println!("{}", serde_json::to_string_pretty(&infos).map_err(|e| failed(e.to_string()))?);
        return Ok(());
    }
    let w_id = infos.iter().map(|c| c.id.len()).max().unwrap_or(2).max(2);
    let w_label = infos.iter().map(|c| c.label.len()).max().unwrap_or(5).max(5);
    let w_sc = infos.iter().map(|c| c.shortcut.len()).max().unwrap_or(8).max(8);
    println!("{:<w_id$}  {:<w_label$}  {:<w_sc$}  enabled", "id", "label", "shortcut");
    for c in infos {
        let enabled = if c.enabled { "yes".to_string() } else { format!("no ({})", c.reason) };
        println!("{:<w_id$}  {:<w_label$}  {:<w_sc$}  {enabled}", c.id, c.label, c.shortcut);
    }
    Ok(())
}

/// Open a document file into a fresh session.
fn open(path: &str) -> Result<Session, CliError> {
    let mut s = Session::new(Document::new());
    s.run("file.open", &json!({"path": path})).map_err(|e| failed(format!("{path}: {e}")))?;
    Ok(s)
}

/// `run [--open FILE | --sample] --cmd ID[=JSON] ... [--save OUT] [--json]`.
fn run(args: &[String]) -> Result<(), CliError> {
    let (as_json, rest) = take_json_flag(args);
    let mut open_path: Option<String> = None;
    let mut sample = false;
    let mut save: Option<String> = None;
    let mut cmds: Vec<(String, Value)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--open" => open_path = Some(it.next().ok_or_else(|| usage("--open needs a file"))?.clone()),
            "--sample" => sample = true,
            "--save" => save = Some(it.next().ok_or_else(|| usage("--save needs a file"))?.clone()),
            "--cmd" => {
                let spec = it.next().ok_or_else(|| usage("--cmd needs ID[=JSON]"))?;
                let (id, params) = match spec.split_once('=') {
                    Some((id, js)) => {
                        let v = serde_json::from_str::<Value>(js).map_err(|e| usage(format!("{id}: bad JSON params: {e}")))?;
                        (id.to_string(), v)
                    }
                    None => (spec.clone(), json!({})),
                };
                if id.trim().is_empty() {
                    return Err(usage("--cmd: empty command id"));
                }
                cmds.push((id, params));
            }
            other => return Err(usage(format!("run: unknown argument `{other}`"))),
        }
    }
    if open_path.is_some() && sample {
        return Err(usage("run: --open and --sample exclude each other"));
    }
    if cmds.is_empty() && save.is_none() {
        return Err(usage("run: nothing to do; give at least one --cmd or --save"));
    }
    let mut s = match &open_path {
        Some(p) => open(p)?,
        None if sample => Session::new(vizcraft_engine::sample::sample_document()),
        None => Session::new(Document::new()),
    };
    for (id, params) in cmds {
        let r = s.run(&id, &params);
        // A command may ask the UI for something; headless there is none, so say so.
        for req in std::mem::take(&mut s.ui_requests) {
            eprintln!("vizcraft-cli: {id}: UI request ignored (no window): {req}");
        }
        match r {
            Ok(v) => {
                if as_json {
                    println!("{}", json!({"command": id, "ok": true, "result": v}));
                } else {
                    println!("{id}: ok {v}");
                }
            }
            Err(e) => {
                if as_json {
                    println!("{}", json!({"command": id, "ok": false, "error": e.to_string()}));
                    return Err(failed(format!("{id} failed")));
                }
                return Err(failed(format!("{id}: {e}")));
            }
        }
    }
    if let Some(path) = save {
        s.run("file.save", &json!({"path": path})).map_err(|e| failed(format!("save {path}: {e}")))?;
        if as_json {
            println!("{}", json!({"saved": path}));
        } else {
            println!("saved {path}");
        }
    }
    Ok(())
}

/// `inspect FILE [--json]`: a summary of the document, or the document itself as JSON.
fn inspect(args: &[String]) -> Result<(), CliError> {
    let (as_json, rest) = take_json_flag(args);
    let path = match rest.as_slice() {
        [p] => p,
        [] => return Err(usage("inspect needs a file")),
        [_, extra, ..] => return Err(usage(format!("inspect: unexpected argument `{extra}`"))),
    };
    let s = open(path)?;
    let doc = &s.doc;
    if as_json {
        println!("{}", serde_json::to_string_pretty(doc).map_err(|e| failed(e.to_string()))?);
        return Ok(());
    }
    println!("{path}");
    println!("  title:     {}", if doc.title.is_empty() { "(untitled)" } else { &doc.title });
    if !doc.core.author.is_empty() {
        println!("  author:    {}", doc.core.author);
    }
    if !doc.core.last_modified_by.is_empty() {
        println!("  modified:  {} by {}", doc.core.modified, doc.core.last_modified_by);
    }
    if !doc.core.created.is_empty() {
        println!("  created:   {}", doc.core.created);
    }
    println!("  revision:  {}", doc.core.revision);
    for page in &doc.pages {
        let frame = page.frame();
        println!(
            "  page {:<5} {} ({} x {}): {} shapes, {} connectors",
            page.id,
            page.name,
            frame.width(),
            frame.height(),
            page.shapes.len(),
            page.connectors.len()
        );
    }
    Ok(())
}

/// `HOST:PORT`, or just `PORT` for the loopback address the app listens on.
fn resolve(addr: &str) -> Result<SocketAddr, CliError> {
    let spec = if addr.chars().all(|c| c.is_ascii_digit()) { format!("127.0.0.1:{addr}") } else { addr.to_string() };
    spec.to_socket_addrs()
        .map_err(|e| usage(format!("control: bad address `{addr}`: {e}")))?
        .next()
        .ok_or_else(|| usage(format!("control: bad address `{addr}`")))
}

/// Which backend `mcp` serves.
#[derive(Debug, PartialEq, Eq)]
enum McpBackend {
    /// `--headless`: an engine session in this process.
    Headless,
    /// `--connect ADDR`: the app at this address, retried on every call until it answers.
    Connect(String),
    /// No flag: the app at this address if one listens now, else headless.
    Auto(String),
}

/// `HOST:PORT` as given, or `PORT` as a loopback address.
fn loopback_spec(addr: &str) -> String {
    if addr.chars().all(|c| c.is_ascii_digit()) { format!("127.0.0.1:{addr}") } else { addr.to_string() }
}

/// Parse the `mcp` arguments; `env_port` is `VIZCRAFT_CONTROL_PORT`, read by the caller so this
/// stays testable.
fn mcp_backend(args: &[String], env_port: Option<&str>) -> Result<McpBackend, CliError> {
    let mut connect_to: Option<String> = None;
    let mut headless = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--connect" => connect_to = Some(it.next().ok_or_else(|| usage("--connect needs HOST:PORT or PORT"))?.clone()),
            "--headless" => headless = true,
            other => return Err(usage(format!("mcp: unknown argument `{other}`"))),
        }
    }
    match (headless, connect_to) {
        (true, Some(_)) => Err(usage("mcp: --headless and --connect exclude each other")),
        (true, None) => Ok(McpBackend::Headless),
        (false, Some(a)) => Ok(McpBackend::Connect(loopback_spec(&a))),
        (false, None) => match env_port.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => {
                let port: u16 = p.parse().map_err(|_| usage(format!("VIZCRAFT_CONTROL_PORT `{p}` is not a port number")))?;
                Ok(McpBackend::Auto(format!("127.0.0.1:{port}")))
            }
            None => Ok(McpBackend::Auto(vizcraft_mcp::DEFAULT_ADDR.to_string())),
        },
    }
}

/// `mcp [--connect ADDR | --headless]`: serve MCP on stdio until the client closes stdin.
///
/// With `--connect` the server starts even when the app is not running yet, because MCP clients
/// launch their servers up front; each tool call then says that no app is listening, and the
/// first call after the app starts connects. With no flag a missing app means headless, so an
/// agent gets a working server either way. Stdout belongs to the protocol, so every message here
/// goes to stderr.
fn mcp(args: &[String]) -> Result<(), CliError> {
    let env_port = std::env::var("VIZCRAFT_CONTROL_PORT").ok();
    let backend: Box<dyn vizcraft_mcp::Backend> = match mcp_backend(args, env_port.as_deref())? {
        McpBackend::Headless => {
            eprintln!("vizcraft-cli mcp: headless, working on a document in this process with no window");
            Box::new(vizcraft_mcp::Headless::new())
        }
        McpBackend::Connect(spec) => {
            let mut remote = vizcraft_mcp::Remote::new(&spec).map_err(|e| usage(format!("mcp: {e}")))?;
            match remote.probe() {
                Ok(()) => eprintln!("vizcraft-cli mcp: connected to the VizCraft app at {}", remote.addr()),
                Err(e) => eprintln!("vizcraft-cli mcp: {e}; serving anyway, tool calls will retry"),
            }
            Box::new(remote)
        }
        McpBackend::Auto(spec) => {
            let mut remote = vizcraft_mcp::Remote::new(&spec).map_err(|e| usage(format!("mcp: {e}")))?;
            match remote.probe() {
                Ok(()) => {
                    eprintln!("vizcraft-cli mcp: connected to the VizCraft app at {}", remote.addr());
                    Box::new(remote)
                }
                Err(_) => {
                    eprintln!(
                        "vizcraft-cli mcp: no VizCraft app is listening at {}; running headless (no window). Pass --connect to wait for the app instead",
                        remote.addr()
                    );
                    Box::new(vizcraft_mcp::Headless::new())
                }
            }
        }
    };
    vizcraft_mcp::logging::install();
    let mut server = vizcraft_mcp::Server::new(backend);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server.serve(stdin.lock(), stdout.lock()).map_err(|e| failed(format!("mcp: stdio failed: {e}")))
}

/// Connect to the app's control server, or say how to start one.
fn connect(addr: &str) -> Result<TcpStream, CliError> {
    let sa = resolve(addr)?;
    let stream = TcpStream::connect_timeout(&sa, Duration::from_secs(5))
        .map_err(|e| failed(format!("cannot reach the VizCraft app at {sa}: {e} (start it with `vizcraft --control PORT`)")))?;
    // Screenshots answer a few frames late and the server gives the UI 60 s; wait a little longer.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(70)));
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Send one line, read one reply line.
fn exchange(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>, line: &str) -> Result<Value, CliError> {
    writeln!(stream, "{line}").map_err(|e| failed(format!("control: send failed: {e}")))?;
    stream.flush().map_err(|e| failed(format!("control: send failed: {e}")))?;
    let mut reply = String::new();
    match reader.read_line(&mut reply) {
        Ok(0) => Err(failed("control: the app closed the connection without a reply")),
        Ok(_) => serde_json::from_str::<Value>(reply.trim_end()).map_err(|e| failed(format!("control: bad reply `{}`: {e}", reply.trim_end()))),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
            Err(failed("control: no reply within 70 s (is the window hidden or the app stuck?)"))
        }
        Err(e) => Err(failed(format!("control: read failed: {e}"))),
    }
}

/// `control HOST:PORT METHOD [JSON]` or `control HOST:PORT -` (requests from stdin). Replies are
/// printed as JSON lines; the exit code is 1 when any reply has `ok: false`.
fn control(args: &[String]) -> Result<(), CliError> {
    let (addr, method, params) = match args {
        [addr, method] => (addr, method, None),
        [addr, method, params] => (addr, method, Some(params)),
        [] | [_] => return Err(usage("control needs HOST:PORT and a METHOD (or `-` to read requests from stdin)")),
        [_, _, _, extra, ..] => return Err(usage(format!("control: unexpected argument `{extra}`"))),
    };
    let mut stream = connect(addr)?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| failed(e.to_string()))?);
    if method == "-" {
        let stdin = std::io::stdin();
        let mut all_ok = true;
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| failed(format!("stdin: {e}")))?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = exchange(&mut stream, &mut reader, line.trim())?;
            all_ok &= reply.get("ok") == Some(&Value::Bool(true));
            println!("{reply}");
        }
        return if all_ok { Ok(()) } else { Err(failed("a request failed (see the replies above)")) };
    }
    let params = match params {
        Some(p) => serde_json::from_str::<Value>(p).map_err(|e| usage(format!("control: params are not JSON: {e}")))?,
        None => json!({}),
    };
    let request = json!({"id": 1, "method": method, "params": params});
    let reply = exchange(&mut stream, &mut reader, &request.to_string())?;
    println!("{reply}");
    if reply.get("ok") == Some(&Value::Bool(true)) {
        Ok(())
    } else {
        Err(failed(format!("{method}: {}", reply.get("error").and_then(Value::as_str).unwrap_or("failed"))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_string()).collect()
    }

    fn is_usage(r: Result<McpBackend, CliError>) -> bool {
        matches!(r, Err(CliError::Usage(_)))
    }

    #[test]
    fn mcp_arguments_pick_the_backend() {
        assert_eq!(mcp_backend(&args(&["--headless"]), None).ok(), Some(McpBackend::Headless));
        assert_eq!(mcp_backend(&args(&["--headless"]), Some("7991")).ok(), Some(McpBackend::Headless));
        assert_eq!(mcp_backend(&args(&["--connect", "7995"]), None).ok(), Some(McpBackend::Connect("127.0.0.1:7995".into())));
        assert_eq!(mcp_backend(&args(&["--connect", "localhost:7996"]), Some("1")).ok(), Some(McpBackend::Connect("localhost:7996".into())));
        assert_eq!(mcp_backend(&[], None).ok(), Some(McpBackend::Auto(vizcraft_mcp::DEFAULT_ADDR.into())));
        assert_eq!(mcp_backend(&[], Some(" 7993 ")).ok(), Some(McpBackend::Auto("127.0.0.1:7993".into())));
        assert_eq!(mcp_backend(&[], Some("")).ok(), Some(McpBackend::Auto(vizcraft_mcp::DEFAULT_ADDR.into())));

        assert!(is_usage(mcp_backend(&args(&["--headless", "--connect", "7990"]), None)));
        assert!(is_usage(mcp_backend(&args(&["--connect", "7990", "--headless"]), None)));
        assert!(is_usage(mcp_backend(&args(&["--connect"]), None)));
        assert!(is_usage(mcp_backend(&args(&["--bogus"]), None)));
        assert!(is_usage(mcp_backend(&[], Some("not-a-port"))));
        assert!(is_usage(mcp_backend(&[], Some("70000"))));
    }
}
