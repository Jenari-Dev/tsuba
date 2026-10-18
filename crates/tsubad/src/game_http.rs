//! Game lighting REST server: a small HTTP/1.1 server on 127.0.0.1 compatible with the
//! Razer Chroma REST interface that games use (an independent implementation written from
//! Razer's public REST documentation, for interoperability).
//!
//! - `127.0.0.1:54235` — `GET|POST /razer/chromasdk` (version / create a session).
//! - each session gets its own listener on `127.0.0.1:<ephemeral>`; the returned `uri` is
//!   `http://localhost:<port>/chromasdk/<token>` (a random per-session token, so other local
//!   pages or processes can't drive or end someone else's session), under which the client
//!   calls `/heartbeat`, `/keyboard`, `/mouse`, `/mousepad`, `/headset`, `/keypad`,
//!   `/chromalink`, `/effect` and `DELETE` on the uri itself. Clients follow the uri verbatim.
//!
//! Every listener binds the IPv4 loopback address only and only answers requests whose `Host`
//! is `localhost` / `127.0.0.1` (DNS rebinding). Requests from web pages (an `Origin` or
//! `Sec-Fetch-Site` header) are refused unless the user allowed websites to control lighting
//! ([`GameLighting::web`]); then CORS preflights are answered so browser-based clients work.
//! Native clients (games, the Wine bridge, curl) send neither header and are unaffected.
//!
//! The port is reachable by every local user (plain loopback TCP); see README.
//! Plain threads (no async): a connection is one thread.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::{info, warn};
use tsuba_core::game_lighting::{result, Target};

use crate::game_lighting::{self as gl, lock, AppInfo, GameLighting};

/// The documented port.
pub const PORT: u16 = 54235;
/// Version reported by `GET /razer/chromasdk` (the documented REST revision).
const VERSION: &str = "4.0.0";
const MAX_HEADER: u64 = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
/// Connections over all listeners.
const MAX_CONNECTIONS: usize = 64;
/// Connections per listener (main / one session), so one client can't starve the others.
const MAX_CONNECTIONS_MAIN: usize = 32;
const MAX_CONNECTIONS_SESSION: usize = 8;
/// Keep-alive connections are closed after this long without a request.
const IDLE: Duration = Duration::from_secs(20);
/// A request (head + body) must arrive within this long of its first byte (slow senders).
#[cfg(not(test))]
const REQUEST_DEADLINE: Duration = Duration::from_secs(30);
#[cfg(test)]
const REQUEST_DEADLINE: Duration = Duration::from_millis(1500);
/// CORS: the request headers browser clients may send (fixed; never echoed).
const ALLOW_HEADERS: &str = "Content-Type";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ctx {
    Main,
    /// Session id and its uri token.
    Session(u32, Arc<str>),
}

/// One listening socket and its accept thread; dropping it stops the thread.
struct Listener {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the blocking accept().
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        if let Some(t) = self.thread.take() {
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

struct Shared {
    gl: Weak<GameLighting>,
    connections: AtomicUsize,
    stop: AtomicBool,
    sessions: Mutex<HashMap<u32, Listener>>,
}

/// A connection slot (global + per listener); released when dropped, also when the
/// connection thread panics or can't be spawned.
struct Slot {
    sh: Arc<Shared>,
    local: Arc<AtomicUsize>,
}

impl Slot {
    fn take(sh: &Arc<Shared>, local: &Arc<AtomicUsize>, cap: usize) -> Option<Self> {
        if sh.connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            sh.connections.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        if local.fetch_add(1, Ordering::AcqRel) >= cap {
            local.fetch_sub(1, Ordering::AcqRel);
            sh.connections.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self { sh: sh.clone(), local: local.clone() })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.local.fetch_sub(1, Ordering::AcqRel);
        self.sh.connections.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The running server (main listener + one listener per session).
pub struct Server {
    shared: Arc<Shared>,
    main: Option<Listener>,
    port: u16,
}

impl Server {
    /// Bind `127.0.0.1:port` (0 = any free port, for tests).
    pub fn start(gl: Arc<GameLighting>, port: u16) -> Result<Self> {
        let shared = Arc::new(Shared { gl: Arc::downgrade(&gl), connections: AtomicUsize::new(0), stop: AtomicBool::new(false), sessions: Mutex::new(HashMap::new()) });
        let main = listen(&shared, Ctx::Main, port).with_context(|| format!("game lighting: binding 127.0.0.1:{port}"))?;
        let port = main.addr.port();
        Ok(Self { shared, main: Some(main), port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop a session's listener (its keep-alive connections close on their next request).
    pub fn close_session(&self, id: u32) {
        let l = lock(&self.shared.sessions).remove(&id);
        drop(l);
    }

    #[cfg(test)]
    fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::Acquire)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.main.take();
        let all: Vec<Listener> = lock(&self.shared.sessions).drain().map(|(_, l)| l).collect();
        drop(all);
    }
}

fn listen(shared: &Arc<Shared>, ctx: Ctx, port: u16) -> Result<Listener> {
    let sock = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    let addr = sock.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let (flag, sh) = (stop.clone(), shared.clone());
    let (name, cap) = match ctx {
        Ctx::Main => ("game-http".to_string(), MAX_CONNECTIONS_MAIN),
        Ctx::Session(id, _) => (format!("game-http-{id}"), MAX_CONNECTIONS_SESSION),
    };
    let local = Arc::new(AtomicUsize::new(0));
    let thread = std::thread::Builder::new().name(name).spawn(move || {
        for conn in sock.incoming() {
            if flag.load(Ordering::Relaxed) || sh.stop.load(Ordering::Relaxed) {
                break;
            }
            let Ok(stream) = conn else { continue };
            let Some(slot) = Slot::take(&sh, &local, cap) else { continue };
            let (sh2, ctx) = (sh.clone(), ctx.clone());
            // The slot moves into the closure: it is released when the thread ends (normally
            // or by a panic) or right here when the thread can't be spawned.
            let _ = std::thread::Builder::new().name("game-http-conn".into()).spawn(move || {
                let _slot = slot;
                // A panic must not take the server down (or leak the slot); locks shared with
                // the output thread are poison-tolerant.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| serve(&sh2, &ctx, stream))).is_err() {
                    warn!("game lighting: a connection handler panicked; connection dropped");
                }
            });
        }
    })?;
    Ok(Listener { addr, stop, thread: Some(thread) })
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    keep_alive: bool,
    /// The client's local port (for finding which process is talking).
    peer_port: u16,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// Sent by a web page (browsers add `Origin` to cross-origin and all POST requests and
    /// `Sec-Fetch-Site` to every request); games and native tools send neither.
    fn is_from_browser(&self) -> bool {
        self.header("origin").is_some() || self.header("sec-fetch-site").is_some()
    }
}

/// `Host` must name the loopback interface (absent = HTTP/1.0 native client). Anything else
/// is a DNS-rebinding page or a confused proxy.
fn host_ok(host: Option<&str>) -> bool {
    let Some(h) = host else { return true };
    let h = h.trim();
    let (name, port) = match h.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (h, None),
    };
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
    port_ok && (name.eq_ignore_ascii_case("localhost") || name == "127.0.0.1")
}

/// The socket, with a deadline for the request being read.
struct Conn {
    s: TcpStream,
    deadline: Instant,
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let now = Instant::now();
        if now >= self.deadline {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "request deadline"));
        }
        self.s.set_read_timeout(Some((self.deadline - now).min(IDLE)))?;
        self.s.read(buf)
    }
}

/// One line of the head (without CR/LF). End of stream = `UnexpectedEof`.
fn read_line(r: &mut BufReader<Conn>, budget: &mut u64) -> std::io::Result<String> {
    let mut line = String::new();
    let n = r.by_ref().take(*budget).read_line(&mut line)?;
    *budget = budget.saturating_sub(n as u64);
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "connection closed"));
    }
    if !line.ends_with('\n') {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "header too large"));
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn read_request(r: &mut BufReader<Conn>) -> std::io::Result<Option<Request>> {
    // Wait (up to IDLE) for the next request; the clock for the whole request starts with its
    // first byte.
    r.get_mut().deadline = Instant::now() + IDLE;
    if r.fill_buf()?.is_empty() {
        return Ok(None);
    }
    r.get_mut().deadline = Instant::now() + REQUEST_DEADLINE;
    let mut budget = MAX_HEADER;
    // Tolerate stray blank lines between keep-alive requests; end of stream = done.
    let start = loop {
        match read_line(r, &mut budget) {
            Ok(l) if l.is_empty() => continue,
            Ok(l) => break l,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
    };
    let mut parts = start.split_whitespace();
    let (Some(method), Some(target), version) = (parts.next(), parts.next(), parts.next().unwrap_or("HTTP/1.0")) else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad request line"));
    };
    let mut headers = Vec::new();
    loop {
        let line = read_line(r, &mut budget)?;
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut req = Request { method: method.to_ascii_uppercase(), path: target.split(['?', '#']).next().unwrap_or("").to_string(), headers, body: Vec::new(), keep_alive: false, peer_port: 0 };
    let conn = req.header("connection").map(|c| c.to_ascii_lowercase()).unwrap_or_default();
    req.keep_alive = if version == "HTTP/1.1" { !conn.contains("close") } else { conn.contains("keep-alive") };
    if req.header("transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked")) {
        loop {
            let mut b = MAX_HEADER;
            let size = read_line(r, &mut b)?;
            let n = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad chunk"))?;
            if n == 0 {
                // Trailers until the blank line.
                while !read_line(r, &mut b)?.is_empty() {}
                break;
            }
            // `req.body.len() <= MAX_BODY` always holds, so this can't overflow.
            if n > MAX_BODY - req.body.len() {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "body too large"));
            }
            let at = req.body.len();
            req.body.resize(at + n, 0);
            r.read_exact(&mut req.body[at..])?;
            read_line(r, &mut b)?;
        }
    } else if let Some(len) = req.header("content-length") {
        let n: usize = len.parse().map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad length"))?;
        if n > MAX_BODY {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "body too large"));
        }
        req.body.resize(n, 0);
        r.read_exact(&mut req.body)?;
    }
    Ok(Some(req))
}

struct Response {
    status: u16,
    body: Option<Value>,
    extra: Vec<(String, String)>,
}

impl Response {
    fn json(v: Value) -> Self {
        Self { status: 200, body: Some(v), extra: Vec::new() }
    }
    fn result(code: i64) -> Self {
        Self::json(json!({ "result": code }))
    }
    fn forbidden() -> Self {
        Self { status: 403, ..Self::result(result::FAILED) }
    }
}

/// `cors` = answer browsers (websites allowed): adds `Access-Control-Allow-Origin: *`.
fn write_response(w: &mut TcpStream, resp: &Response, keep_alive: bool, cors: bool) -> std::io::Result<()> {
    let body = resp.body.as_ref().map(|b| b.to_string()).unwrap_or_default();
    let reason = match resp.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {} {reason}\r\n", resp.status);
    if cors {
        head.push_str("Access-Control-Allow-Origin: *\r\n");
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: {}\r\n", body.len(), if keep_alive { "keep-alive" } else { "close" }));
    if resp.body.is_some() {
        head.push_str("Content-Type: application/json; charset=utf-8\r\n");
    }
    for (k, v) in &resp.extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body.as_bytes())?;
    w.flush()
}

fn serve(sh: &Arc<Shared>, ctx: &Ctx, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(IDLE));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);
    let peer_port = stream.peer_addr().map(|a| a.port()).unwrap_or(0);
    let Ok(mut w) = stream.try_clone() else { return };
    let mut r = BufReader::new(Conn { s: stream, deadline: Instant::now() + IDLE });
    loop {
        if sh.stop.load(Ordering::Relaxed) {
            return;
        }
        let req = match read_request(&mut r) {
            Ok(Some(mut req)) => {
                req.peer_port = peer_port;
                req
            }
            Ok(None) => return,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    let _ = write_response(&mut w, &Response { status: 400, body: Some(json!({"result": result::INVALID_PARAMETER})), extra: Vec::new() }, false, false);
                }
                return;
            }
        };
        let Some(gl) = sh.gl.upgrade() else { return };
        let web = gl.web.load(Ordering::Relaxed);
        if !host_ok(req.header("host")) || (req.is_from_browser() && !web) {
            // DNS rebinding / a website while websites aren't allowed: no CORS headers, so the
            // page can't read the answer either.
            let _ = write_response(&mut w, &Response::forbidden(), false, false);
            return;
        }
        let resp = handle(&gl, sh, ctx, &req, web);
        // Sessions that ended (DELETE / timeout) stop serving their keep-alive connections.
        let alive = match ctx {
            Ctx::Main => true,
            Ctx::Session(id, _) => lock(&gl.engine).contains(*id),
        };
        drop(gl);
        let keep = req.keep_alive && alive && !sh.stop.load(Ordering::Relaxed);
        if write_response(&mut w, &resp, keep, web).is_err() || !keep {
            return;
        }
    }
}

fn handle(gl: &Arc<GameLighting>, sh: &Arc<Shared>, ctx: &Ctx, req: &Request, web: bool) -> Response {
    if req.method == "OPTIONS" {
        // CORS preflight from browser-based clients (only reached when websites are allowed or
        // the client is not a browser).
        let mut extra = vec![
            ("Access-Control-Allow-Methods".into(), "GET, POST, PUT, DELETE, OPTIONS".into()),
            ("Access-Control-Allow-Headers".into(), ALLOW_HEADERS.into()),
            ("Access-Control-Max-Age".into(), "86400".into()),
        ];
        if web && req.header("access-control-request-private-network").is_some() {
            extra.push(("Access-Control-Allow-Private-Network".into(), "true".into()));
        }
        return Response { status: 204, body: None, extra };
    }
    // The session part of the path: `/chromasdk/<token>[/…]`. Without the right token the
    // session doesn't exist for this client.
    let sub = match ctx {
        Ctx::Main => None,
        Ctx::Session(_, token) => {
            let p = req.path.trim_end_matches('/');
            match p.strip_prefix("/chromasdk/").and_then(|p| p.strip_prefix(&**token)).filter(|s| s.is_empty() || s.starts_with('/')) {
                Some(s) => Some(s),
                None => return Response { status: 404, ..Response::result(result::NOT_FOUND) },
            }
        }
    };
    let body: Value = if req.body.iter().all(|b| b.is_ascii_whitespace()) {
        Value::Null
    } else {
        match serde_json::from_slice(&req.body) {
            Ok(v) => v,
            Err(_) => return Response::result(result::INVALID_PARAMETER),
        }
    };
    let path = req.path.trim_end_matches('/');
    match (ctx, sub) {
        (Ctx::Session(id, _), Some(sub)) => {
            let id = *id;
            let now = Instant::now();
            let res = match (req.method.as_str(), sub) {
                ("DELETE", "") => {
                    let ok = gl.end_session(id);
                    Response::result(if ok { result::SUCCESS } else { result::NOT_FOUND })
                }
                ("GET", "") => match lock(&gl.engine).session(id) {
                    Some(s) => Response::json(json!({ "result": result::SUCCESS, "sessionid": id, "title": s.info.title })),
                    None => Response::result(result::NOT_FOUND),
                },
                (_, "/heartbeat") => match lock(&gl.engine).heartbeat(id, now) {
                    Some(tick) => Response::json(json!({ "tick": tick })),
                    None => Response::result(result::NOT_FOUND),
                },
                ("PUT", "/effect") | ("DELETE", "/effect") => effect_ids(gl, id, req.method == "PUT", &body, now),
                (m @ ("PUT" | "POST"), dev) => match dev.strip_prefix('/').and_then(Target::from_path) {
                    Some(t) => device(gl, id, t, m == "POST", &body, now),
                    // Desktop and device-id endpoints (`/devid=…`): no such devices here.
                    None if dev.starts_with("/devid=") => {
                        lock(&gl.engine).touch(id, now);
                        Response::result(result::DEVICE_NOT_CONNECTED)
                    }
                    None => Response { status: 404, ..Response::result(result::NOT_FOUND) },
                },
                _ => Response { status: 404, ..Response::result(result::NOT_FOUND) },
            };
            gl.notify();
            res
        }
        _ => match (req.method.as_str(), path) {
            // Tests: a handler that panics while holding the engine lock.
            #[cfg(test)]
            ("GET", "/test/panic") => {
                let _eng = lock(&gl.engine);
                panic!("test panic");
            }
            ("GET", "/razer/chromasdk") => Response::json(json!({ "core": VERSION, "device": VERSION, "version": VERSION })),
            ("POST", "/razer/chromasdk") => create_session(gl, sh, &body, req.peer_port),
            (_, "/razer/chromasdk") => Response { status: 405, ..Response::result(result::NOT_SUPPORTED) },
            _ => Response { status: 404, ..Response::result(result::NOT_FOUND) },
        },
    }
}

/// Random uri token for a session (128 bits from the system RNG, hex).
fn session_token() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        // No /dev/urandom (never on Linux): fall back to the uuid generator's mixing.
        return gl::new_uuid().replace('-', "").to_ascii_lowercase();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn create_session(gl: &Arc<GameLighting>, sh: &Arc<Shared>, body: &Value, peer_port: u16) -> Response {
    if !body.is_object() {
        return Response::result(result::INVALID_PARAMETER);
    }
    let mut info = AppInfo::from_json(body);
    info.exe = if peer_port == 0 { String::new() } else { tsuba_core::game_lighting::peer_process(peer_port).unwrap_or_default() };
    // The user's allow / block list (per application).
    if !gl.admit(&info) {
        info!("game lighting: refused {:?} ({:?}): blocked by the user", info.title, info.exe);
        return Response::result(result::ACCESS_DENIED);
    }
    let Some(id) = lock(&gl.engine).create(info.clone(), 0, Instant::now()) else {
        return Response::result(result::FAILED);
    };
    let token: Arc<str> = session_token().into();
    // The session's own listener (any free loopback port) serves only this session.
    let listener = match listen(sh, Ctx::Session(id, token.clone()), 0) {
        Ok(l) => l,
        Err(e) => {
            warn!("game lighting: session listener: {e}");
            lock(&gl.engine).end(id);
            return Response::result(result::FAILED);
        }
    };
    let port = listener.addr.port();
    lock(&gl.engine).set_port(id, port);
    lock(&sh.sessions).insert(id, listener);
    // Debug formatting: the strings come from the client (no control characters in the log).
    info!("game lighting: session {id} for {:?} ({:?}) on port {port}", info.title, if info.author.is_empty() { "unknown author" } else { &info.author });
    gl.notify();
    // "devices" is a Tsuba extension (clients ignore unknown fields): which device classes are
    // attached, so the native bridge can answer QueryDevice truthfully.
    Response::json(json!({ "sessionid": id, "uri": format!("http://localhost:{port}/chromasdk/{token}"), "devices": gl.attached_classes() }))
}

fn device(gl: &GameLighting, id: u32, t: Target, store: bool, body: &Value, now: Instant) -> Response {
    let mut eng = lock(&gl.engine);
    if store {
        if let Some(list) = body.get("effects").and_then(|e| e.as_array()) {
            let results: Vec<Value> = list
                .iter()
                .map(|e| match gl::parse(t, e).and_then(|eff| eng.store(id, t, eff, now)) {
                    Ok(eid) => json!({ "id": eid, "result": result::SUCCESS }),
                    Err(code) => json!({ "result": code }),
                })
                .collect();
            return Response::json(json!({ "results": results }));
        }
        return match gl::parse(t, body).and_then(|eff| eng.store(id, t, eff, now)) {
            Ok(eid) => Response::json(json!({ "result": result::SUCCESS, "id": eid })),
            Err(code) => Response::result(code),
        };
    }
    let code = match gl::parse(t, body) {
        Ok(eff) => eng.apply(id, t, eff, now),
        Err(code) => {
            eng.touch(id, now);
            code
        }
    };
    Response::result(code)
}

fn effect_ids(gl: &GameLighting, id: u32, apply: bool, body: &Value, now: Instant) -> Response {
    let mut eng = lock(&gl.engine);
    let mut one = |eid: &Value| -> i64 {
        let Some(eid) = eid.as_str() else { return result::INVALID_PARAMETER };
        if apply { eng.apply_stored(id, eid, now) } else { eng.delete_stored(id, eid, now) }
    };
    if let Some(ids) = body.get("ids").and_then(|v| v.as_array()) {
        let results: Vec<Value> = ids.iter().map(|e| json!({ "result": one(e) })).collect();
        return Response::json(json!({ "results": results }));
    }
    match body.get("id") {
        Some(e) => Response::result(one(e)),
        None => Response::result(result::INVALID_PARAMETER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_lighting::tests::MockSink;

    /// Minimal HTTP client: one request per connection (plus one keep-alive check below).
    fn call(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String, Value) {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let body = body.unwrap_or("");
        write!(s, "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let (head, body) = out.split_once("\r\n\r\n").unwrap();
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, head.to_string(), serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn uri_port(uri: &str) -> u16 {
        uri.trim_start_matches("http://localhost:").split('/').next().unwrap().parse().unwrap()
    }

    /// The session's base path (`/chromasdk/<token>`), as clients follow it.
    fn uri_path(uri: &str) -> String {
        let rest = uri.trim_start_matches("http://localhost:");
        rest[rest.find('/').unwrap()..].to_string()
    }

    fn wait_for(f: impl Fn() -> bool) -> bool {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn rest_session_paints_the_mock_devices() {
        let sink = Arc::new(MockSink::default());
        sink.devices.lock().unwrap().push(("028d".into(), tsuba_hid::Kind::Keyboard));
        sink.devices.lock().unwrap().push(("00aa".into(), tsuba_hid::Kind::Mouse));
        let changes = Arc::new(Mutex::new(Vec::<String>::new()));
        let ch = changes.clone();
        let gl = GameLighting::new(sink.clone(), 0, Box::new(move |s| ch.lock().unwrap().push(s)));
        *gl.timeout.lock().unwrap() = Duration::from_millis(1000);
        gl.set_enabled(true).unwrap();
        let port = gl.bound_port().unwrap();
        assert_ne!(port, 0);

        // Version and CORS preflight (no CORS answer while websites aren't allowed).
        let (st, _, v) = call(port, "GET", "/razer/chromasdk", None);
        assert_eq!((st, v["version"].as_str()), (200, Some(VERSION)));
        let (st, head, _) = call(port, "OPTIONS", "/razer/chromasdk", None);
        assert_eq!(st, 204);
        assert!(!head.contains("Access-Control-Allow-Origin") && head.contains("Access-Control-Allow-Methods"));
        gl.web.store(true, Ordering::Relaxed);
        let (st, head, _) = call(port, "OPTIONS", "/razer/chromasdk", None);
        assert_eq!(st, 204);
        assert!(head.contains("Access-Control-Allow-Origin: *") && head.contains("Access-Control-Allow-Methods"));
        gl.web.store(false, Ordering::Relaxed);

        // Init.
        let init = r#"{"title":"Test Game","description":"d","author":{"name":"Dev","contact":"x"},"device_supported":["keyboard","mouse"],"category":"game"}"#;
        let (st, _, v) = call(port, "POST", "/razer/chromasdk", Some(init));
        assert_eq!(st, 200);
        let sid = v["sessionid"].as_u64().unwrap();
        let uri = v["uri"].as_str().unwrap().to_string();
        assert!(uri.starts_with("http://localhost:") && uri.contains("/chromasdk/"), "{uri}");
        let sp = uri_port(&uri);
        let b = uri_path(&uri);
        assert_eq!(b.len(), "/chromasdk/".len() + 32, "{b}");
        assert_ne!(sp, port);
        // The session port is bound on loopback only.
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, sp)).is_ok());

        let (_, _, v) = call(sp, "PUT", &format!("{b}/heartbeat"), None);
        assert_eq!(v["tick"], 1);

        // Keyboard red, applied immediately.
        let (_, _, v) = call(sp, "PUT", &format!("{b}/keyboard"), Some(r#"{"effect":"CHROMA_STATIC","param":{"color":255}}"#));
        assert_eq!(v["result"], 0);
        assert!(wait_for(|| sink.frames.lock().unwrap().get("028d").is_some_and(|f| f[0][2] == [255, 0, 0])));
        assert!(sink.log.lock().unwrap().contains(&"take 028d".to_string()));
        // Status names the game and the device.
        assert!(wait_for(|| gl.status()["sessions"][0]["devices_active"] == json!(["028d"])));
        assert_eq!(gl.status()["sessions"][0]["title"], "Test Game");
        assert_eq!(gl.status()["sessions"][0]["id"], sid);
        assert!(changes.lock().unwrap().iter().any(|s| s.contains("Test Game") && s.contains("028d")));

        // Stored effects: mouse green via POST + PUT /effect.
        let (_, _, v) = call(sp, "POST", &format!("{b}/mouse"), Some(r#"{"effect":"CHROMA_STATIC","param":{"color":65280}}"#));
        assert_eq!(v["result"], 0);
        let eid = v["id"].as_str().unwrap().to_string();
        let (_, _, v) = call(sp, "PUT", &format!("{b}/effect"), Some(&format!(r#"{{"id":"{eid}"}}"#)));
        assert_eq!(v["result"], 0);
        assert!(wait_for(|| sink.frames.lock().unwrap().get("00aa").is_some_and(|f| f[0][0] == [0, 255, 0])));
        // Group create / apply / delete.
        let (_, _, v) = call(sp, "POST", &format!("{b}/keyboard"), Some(r#"{"effects":[{"effect":"CHROMA_NONE"},{"effect":"CHROMA_BOGUS"},{"effect":"CHROMA_STATIC","param":{"color":16711680}}]}"#));
        let r = v["results"].as_array().unwrap();
        assert_eq!((r[0]["result"].as_i64(), r[1]["result"].as_i64(), r[2]["result"].as_i64()), (Some(0), Some(50), Some(0)));
        let blue = r[2]["id"].as_str().unwrap().to_string();
        let (_, _, v) = call(sp, "PUT", &format!("{b}/effect"), Some(&format!(r#"{{"ids":["{blue}","nope"]}}"#)));
        assert_eq!(v["results"], json!([{"result": 0}, {"result": 1168}]));
        assert!(wait_for(|| sink.frames.lock().unwrap().get("028d").is_some_and(|f| f[0][2] == [0, 0, 255])));
        let (_, _, v) = call(sp, "DELETE", &format!("{b}/effect"), Some(&format!(r#"{{"ids":["{blue}","{eid}"]}}"#)));
        assert_eq!(v["results"], json!([{"result": 0}, {"result": 0}]));
        // Errors.
        let (_, _, v) = call(sp, "PUT", &format!("{b}/keyboard"), Some("{not json"));
        assert_eq!(v["result"], 87);
        let (_, _, v) = call(sp, "PUT", &format!("{b}/devid=EB96AB11-E327-4BC0-B3E3-862BBB963B5D"), Some(r#"{"effect":"CHROMA_NONE"}"#));
        assert_eq!(v["result"], 1167);

        // Keep-alive: two requests on one connection.
        {
            let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, sp)).unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            for _ in 0..2 {
                write!(s, "PUT {b}/heartbeat HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n").unwrap();
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                assert!(line.starts_with("HTTP/1.1 200"), "{line}");
                let mut len = 0;
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut b = vec![0; len];
                r.read_exact(&mut b).unwrap();
            }
        }

        // DELETE ends the session; devices go back to the user's lighting; the port closes.
        let (_, _, v) = call(sp, "DELETE", &b, None);
        assert_eq!(v["result"], 0);
        assert!(wait_for(|| { let l = sink.log.lock().unwrap(); l.contains(&"release 028d".into()) && l.contains(&"release 00aa".into()) }));
        assert!(wait_for(|| TcpStream::connect((Ipv4Addr::LOCALHOST, sp)).is_err()));
        assert!(wait_for(|| gl.status()["sessions"] == json!([])));

        // A second session that stops sending heartbeats times out and releases.
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(init));
        let sp = uri_port(v["uri"].as_str().unwrap());
        let b = uri_path(v["uri"].as_str().unwrap());
        call(sp, "PUT", &format!("{b}/keyboard"), Some(r#"{"effect":"CHROMA_CUSTOM","param":[[65280]]}"#));
        assert!(wait_for(|| sink.frames.lock().unwrap().get("028d").is_some_and(|f| f[0][1] == [0, 255, 0])));
        sink.log.lock().unwrap().clear();
        assert!(wait_for(|| sink.log.lock().unwrap().contains(&"release 028d".into())));
        assert!(wait_for(|| TcpStream::connect((Ipv4Addr::LOCALHOST, sp)).is_err()));

        // Disabling closes the main port.
        gl.set_enabled(false).unwrap();
        assert!(gl.bound_port().is_none());
        assert!(wait_for(|| TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err()));
        assert_eq!(gl.status()["enabled"], false);
        gl.shutdown();
    }

    // ---- security regressions (from the security review's proofs of concept) ----

    fn server() -> (Arc<GameLighting>, u16) {
        let sink = Arc::new(MockSink::default());
        sink.devices.lock().unwrap().push(("028d".into(), tsuba_hid::Kind::Keyboard));
        let gl = GameLighting::new(sink, 0, Box::new(|_| {}));
        gl.set_enabled(true).unwrap();
        let p = gl.bound_port().unwrap();
        (gl, p)
    }

    fn connections(gl: &GameLighting) -> usize {
        lock(&gl.server).as_ref().unwrap().connections()
    }

    /// Raw request on its own connection; the whole reply (empty when the server dropped it).
    fn raw(port: u16, req: &str) -> String {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        String::from_utf8_lossy(&out).into_owned()
    }

    fn new_session(port: u16) -> (u16, String) {
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Secret Game"}"#));
        let uri = v["uri"].as_str().unwrap();
        (uri_port(uri), uri_path(uri))
    }

    /// Chunk sizes that overflowed `body.len() + n` (panic, leaked connection slot; 64 of them
    /// locked the server). Now: 400, slot released, the server keeps answering.
    #[test]
    fn chunked_overflow_is_rejected_and_slots_are_released() {
        let (gl, port) = server();
        for size in ["ffffffffffffffff", "fffffffffffffff0", "100001"].iter().cycle().take(MAX_CONNECTIONS + 8) {
            let out = raw(port, &format!("POST /razer/chromasdk HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nA\r\n{size}\r\n"));
            assert!(out.starts_with("HTTP/1.1 400"), "{out:?}");
        }
        assert!(wait_for(|| connections(&gl) == 0), "slots leaked: {}", connections(&gl));
        let (st, _, v) = call(port, "GET", "/razer/chromasdk", None);
        assert_eq!((st, v["version"].as_str()), (200, Some(VERSION)));
        gl.shutdown();
    }

    /// A blank line followed by EOF spun the header loop forever (busy thread, slot held).
    #[test]
    fn blank_line_then_eof_ends_the_connection() {
        let (gl, port) = server();
        for _ in 0..4 {
            let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            s.write_all(b"\r\n").unwrap();
            drop(s);
        }
        let t = Instant::now();
        assert!(wait_for(|| connections(&gl) == 0), "connection threads still running: {}", connections(&gl));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        gl.shutdown();
    }

    /// A handler that panics while holding the engine lock: the slot is released, the lock is
    /// not left poisoned for the output thread / status, and the server keeps answering.
    #[test]
    fn handler_panic_neither_leaks_nor_poisons() {
        let (gl, port) = server();
        for _ in 0..MAX_CONNECTIONS + 4 {
            let out = raw(port, "GET /test/panic HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            assert!(out.is_empty(), "{out:?}");
        }
        assert!(wait_for(|| connections(&gl) == 0));
        assert!(gl.engine.is_poisoned());
        // Status (D-Bus) and sessions still work.
        assert_eq!(gl.status()["enabled"], true);
        let (sp, b) = new_session(port);
        let (_, _, v) = call(sp, "PUT", &format!("{b}/keyboard"), Some(r#"{"effect":"CHROMA_STATIC","param":{"color":255}}"#));
        assert_eq!(v["result"], 0);
        assert!(wait_for(|| gl.status()["sessions"][0]["devices_active"] == json!(["028d"])), "output thread stopped");
        gl.shutdown();
    }

    /// A client that trickles its request is cut off at the request deadline.
    #[test]
    fn slow_request_hits_the_deadline() {
        let (gl, port) = server();
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let t = Instant::now();
        let mut closed = false;
        for b in "GET /razer/chromasdk HTTP/1.1\r\nHost: localhost\r\nX-Slow: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".bytes() {
            if s.write_all(&[b]).is_err() {
                closed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            if connections(&gl) == 0 {
                closed = true;
                break;
            }
        }
        assert!(closed, "still connected after {:?}", t.elapsed());
        assert!(t.elapsed() < REQUEST_DEADLINE + Duration::from_secs(1), "{:?}", t.elapsed());
        gl.shutdown();
    }

    /// One listener can't take every connection: a full main listener leaves sessions working.
    #[test]
    fn per_listener_connection_cap() {
        let (gl, port) = server();
        let (sp, b) = new_session(port);
        let idle: Vec<TcpStream> = (0..MAX_CONNECTIONS_MAIN)
            .map(|_| {
                let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
                s.write_all(b"G").unwrap();
                s
            })
            .collect();
        assert!(wait_for(|| connections(&gl) == MAX_CONNECTIONS_MAIN));
        let out = raw(port, "GET /razer/chromasdk HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        assert!(out.is_empty(), "main listener over its cap answered: {out:?}");
        let (st, _, v) = call(sp, "PUT", &format!("{b}/heartbeat"), None);
        assert_eq!((st, v["tick"].as_u64()), (200, Some(1)));
        drop(idle);
        gl.shutdown();
    }

    /// Web pages are refused unless the user allowed them; then CORS is answered with a fixed
    /// header list (nothing echoed).
    #[test]
    fn websites_need_the_web_setting() {
        let (gl, port) = server();
        let body = r#"{"title":"Web"}"#;
        let post = |extra: &str| raw(port, &format!("POST /razer/chromasdk HTTP/1.1\r\nHost: localhost:{port}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()));
        for extra in ["Origin: https://evil.example\r\n", "Origin: null\r\n", "Sec-Fetch-Site: cross-site\r\n"] {
            let out = post(extra);
            assert!(out.starts_with("HTTP/1.1 403"), "{extra:?}: {out:?}");
            assert!(!out.contains("Access-Control-Allow-Origin"), "{out}");
        }
        let pre = |extra: &str| raw(port, &format!("OPTIONS /razer/chromasdk HTTP/1.1\r\nHost: localhost\r\nOrigin: https://evil.example\r\nAccess-Control-Request-Private-Network: true\r\nAccess-Control-Request-Headers: x-evil, content-type\r\n{extra}Connection: close\r\n\r\n"));
        let out = pre("");
        assert!(out.starts_with("HTTP/1.1 403") && !out.contains("Access-Control-Allow-Private-Network") && !out.contains("Access-Control-Allow-Origin"), "{out}");
        assert!(lock(&gl.engine).is_empty(), "a session was created");
        // Allowed: the old permissive behaviour, but the allowed headers are a fixed list.
        gl.web.store(true, Ordering::Relaxed);
        let out = post("Origin: https://app.example\r\n");
        assert!(out.starts_with("HTTP/1.1 200") && out.contains("Access-Control-Allow-Origin: *") && out.contains("sessionid"), "{out}");
        let out = pre("");
        assert!(out.starts_with("HTTP/1.1 204") && out.contains("Access-Control-Allow-Private-Network: true"), "{out}");
        assert!(out.contains(&format!("Access-Control-Allow-Headers: {ALLOW_HEADERS}\r\n")) && !out.to_ascii_lowercase().contains("x-evil"), "{out}");
        gl.shutdown();
    }

    /// DNS rebinding: any Host other than localhost / 127.0.0.1 is refused (on every listener).
    #[test]
    fn foreign_host_is_refused() {
        let (gl, port) = server();
        for host in ["evil.example", "evil.example:54235", "localhost.evil.example", "127.0.0.1.nip.io", "localhost:", "localhost:12x", "0.0.0.0", "[::1]"] {
            let out = raw(port, &format!("GET /razer/chromasdk HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"));
            assert!(out.starts_with("HTTP/1.1 403"), "{host}: {out:?}");
        }
        for host in ["localhost", "LOCALHOST", "127.0.0.1", &format!("localhost:{port}"), &format!("127.0.0.1:{port}")] {
            let out = raw(port, &format!("GET /razer/chromasdk HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"));
            assert!(out.starts_with("HTTP/1.1 200"), "{host}: {out:?}");
        }
        // HTTP/1.0 without Host (native client).
        assert!(raw(port, "GET /razer/chromasdk HTTP/1.0\r\n\r\n").starts_with("HTTP/1.1 200"));
        let (sp, b) = new_session(port);
        let out = raw(sp, &format!("GET {b} HTTP/1.1\r\nHost: attacker.example:1\r\nConnection: close\r\n\r\n"));
        assert!(out.starts_with("HTTP/1.1 403") && !out.contains("Secret Game"), "{out}");
        gl.shutdown();
    }

    /// The session uri carries a token: without it another local client can neither read nor
    /// end (nor drive) the session.
    #[test]
    fn session_requires_its_token() {
        let (gl, port) = server();
        let (sp, b) = new_session(port);
        let token = b.trim_start_matches("/chromasdk/");
        let wrong: String = token.chars().rev().collect();
        for path in ["/chromasdk".to_string(), "/chromasdk/".into(), "/chromasdk/heartbeat".into(), format!("/chromasdk/{wrong}"), format!("/chromasdk/{}", &token[..31]), format!("/chromasdk/{token}x"), format!("{b}x/heartbeat"), "/heartbeat".into(), token.to_string()] {
            for m in ["GET", "DELETE", "PUT"] {
                let (st, _, v) = call(sp, m, &path, None);
                assert_eq!(st, 404, "{m} {path}: {v}");
                assert!(!v.to_string().contains("Secret Game"));
            }
        }
        let (_, _, v) = call(sp, "PUT", &format!("{b}/keyboard"), Some(r#"{"effect":"CHROMA_STATIC","param":{"color":255}}"#));
        assert_eq!(v["result"], 0, "untokened calls must not have ended the session");
        let (st, _, v) = call(sp, "GET", &b, None);
        assert_eq!((st, v["title"].as_str()), (200, Some("Secret Game")));
        let (_, _, v) = call(sp, "PUT", &format!("{b}/heartbeat/"), None);
        assert!(v["tick"].as_u64().is_some());
        gl.shutdown();
    }

    /// `device_supported` and the strings are capped before they reach the engine / status.
    #[test]
    fn huge_app_info_is_truncated() {
        let (gl, port) = server();
        let mut big: Vec<String> = (0..40_000).map(|i| format!("d{i:010}")).collect();
        big[0] = "k".repeat(500);
        let body = json!({"title": format!("Evil\u{1b}[31m<b>{}</b>\n", "T".repeat(500)), "description": "D".repeat(50_000), "author": {"name": "A\r\nB", "contact": "C".repeat(999)}, "category": "c".repeat(999), "device_supported": big}).to_string();
        assert!(body.len() < MAX_BODY);
        for _ in 0..gl::MAX_SESSIONS {
            let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(&body));
            assert!(v["sessionid"].as_u64().is_some());
        }
        let st = gl.status();
        let s = &st["sessions"][0];
        assert_eq!(s["device_supported"].as_array().unwrap().len(), gl::MAX_DEVICE_SUPPORTED);
        assert!(s["device_supported"].as_array().unwrap().iter().all(|d| d.as_str().unwrap().chars().count() <= gl::MAX_DEVICE_NAME_CHARS));
        for k in ["title", "description", "author", "contact", "category"] {
            let v = s[k].as_str().unwrap();
            assert!(v.chars().count() <= gl::MAX_INFO_CHARS && !v.chars().any(char::is_control), "{k}: {v:?}");
        }
        assert_eq!(s["author"], "AB");
        assert!(st.to_string().len() < 64 * 1024, "status is {} bytes", st.to_string().len());
        gl.shutdown();
    }

    /// A blocked application is refused (RZRESULT_ACCESS_DENIED), its running sessions end, and
    /// "only allowed" mode refuses applications that have no rule.
    #[test]
    fn blocked_applications_are_refused() {
        let (gl, port) = server();
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Some Game"}"#));
        let id = v["sessionid"].as_u64().unwrap();
        let key = gl.status()["sessions"][0]["app"].as_str().unwrap().to_string();
        // The connecting process was found from the socket (this test process), not guessed from the title.
        assert!(!key.is_empty() && !key.starts_with("title:"), "{key}");
        // The application was recorded so the user can pick a rule for it later.
        assert!(gl.rules_json()["apps"].as_array().unwrap().iter().any(|a| a["key"] == key.as_str() && a["rule"] == "default" && a["connected"] == true));
        gl.set_app_rule(&key, "block").unwrap();
        assert!(!gl.engine.lock().unwrap().contains(id as u32), "the running session ended");
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Some Game"}"#));
        assert_eq!(v["result"], result::ACCESS_DENIED);
        assert!(v["sessionid"].is_null());
        gl.set_app_rule(&key, "default").unwrap();
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Some Game"}"#));
        assert!(v["sessionid"].as_u64().is_some());
        // Allow-list mode: no rule = refused; "allow" = in.
        gl.set_only_allowed(true).unwrap();
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Some Game"}"#));
        assert_eq!(v["result"], result::ACCESS_DENIED);
        gl.set_app_rule(&key, "allow").unwrap();
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Some Game"}"#));
        assert!(v["sessionid"].as_u64().is_some());
        assert!(gl.set_app_rule(&key, "bogus").is_err());
        gl.shutdown();
    }

    #[test]
    fn rules_are_saved_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("tsuba-rules-{}", std::process::id()));
        let file = dir.join("apps.json");
        let (gl, port) = server();
        gl.use_rules_file(file.clone());
        let (_, _, v) = call(port, "POST", "/razer/chromasdk", Some(r#"{"title":"Persist Me"}"#));
        assert!(v["sessionid"].as_u64().is_some());
        let key = gl.status()["sessions"][0]["app"].as_str().unwrap().to_string();
        gl.set_app_rule(&key, "block").unwrap();
        let again = tsuba_core::game_lighting::AppRules::load(&file);
        assert_eq!(again.rules.get(&key), Some(&false));
        assert!(again.seen.contains_key(&key));
        gl.shutdown();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
