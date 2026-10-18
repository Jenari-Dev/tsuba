//! Game lighting: sessions opened by games over the local REST interface (see
//! [`crate::game_http`]), which device each session controls, and the output thread that
//! streams the winning session's effects to the devices as custom frames and hands the
//! devices back to the user's lighting afterwards.
//!
//! Priority: per device class, the session that applied an effect most recently wins.
//! A device is taken over while a session has an effect for its class; it is released
//! (user lighting restored) after `CHROMA_NONE`, session end or session timeout.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use serde_json::json;
use tracing::{info, warn};
use tsuba_core::game_lighting::{self as gl, result, DeviceMap, Effect, Output, Rgb, Show, Target};

/// Frame period of the output thread (≈30 fps per device; updates in between coalesce).
pub const FRAME: Duration = Duration::from_millis(33);
/// Default session timeout without requests (the documented 15 s).
pub const SESSION_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a device stays dark-but-held after `CHROMA_NONE` before the user's lighting is
/// restored (games often send NONE between two effects).
pub const RELEASE_GRACE: Duration = Duration::from_millis(300);
/// Re-send the whole frame (and re-arm custom-frame mode) this often while held.
// Full re-send + re-arm period. Mice (Basilisk) fall back to their stored effect when custom
// frames stop arriving, so keep this short.
const REFRESH: Duration = Duration::from_millis(500);
pub const MAX_SESSIONS: usize = 16;
const MAX_STORED_EFFECTS: usize = 4096;
/// Caps on what a client may put into its application info (shown in the GUI and the status).
pub const MAX_INFO_CHARS: usize = 128;
pub const MAX_DEVICE_SUPPORTED: usize = 16;
pub const MAX_DEVICE_NAME_CHARS: usize = 32;

/// Poison-tolerant lock: a panicking connection thread must never stop the output thread or the
/// D-Bus status.
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A client-supplied string for display: control characters dropped, at most `max` chars.
fn clean(v: &serde_json::Value, max: usize) -> String {
    v.as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Application info sent with `POST /razer/chromasdk`.
#[derive(Debug, Clone, Default)]
pub struct AppInfo {
    pub title: String,
    pub description: String,
    pub author: String,
    pub contact: String,
    pub category: String,
    pub devices: Vec<String>,
    /// Process name found from the connection (empty when unknown). Set by the server, never by the client.
    pub exe: String,
}

impl AppInfo {
    /// The key the user's allow / block rules are stored under.
    pub fn key(&self) -> String {
        gl::app_key(&self.exe, &self.title)
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        Self {
            title: clean(&v["title"], 64),
            description: clean(&v["description"], MAX_INFO_CHARS),
            author: clean(&v["author"]["name"], 64),
            contact: clean(&v["author"]["contact"], 64),
            category: clean(&v["category"], 32),
            devices: v["device_supported"]
                .as_array()
                .map(|a| a.iter().filter(|x| x.is_string()).take(MAX_DEVICE_SUPPORTED).map(|x| clean(x, MAX_DEVICE_NAME_CHARS)).collect())
                .unwrap_or_default(),
            exe: String::new(),
        }
    }
}

/// Per session: id, targets with an effect, devices driven (what the status shows that changes).
type StatusShape = (u32, Vec<Target>, Vec<String>);

struct Active {
    show: Show,
    since: Instant,
}

pub struct Session {
    pub id: u32,
    pub info: AppInfo,
    pub port: u16,
    pub since: SystemTime,
    last_seen: Instant,
    tick: u64,
    /// Activity order of the last applied effect (higher = more recent).
    activity: u64,
    effects: HashMap<String, (Target, Effect)>,
    active: BTreeMap<Target, Active>,
}

/// Session bookkeeping, independent of devices and sockets.
#[derive(Default)]
pub struct Engine {
    sessions: BTreeMap<u32, Session>,
    next_id: u32,
    seq: u64,
}

impl Engine {
    pub fn create(&mut self, info: AppInfo, port: u16, now: Instant) -> Option<u32> {
        if self.sessions.len() >= MAX_SESSIONS {
            return None;
        }
        // Ids look like Razer's (5–6 digits) and are never reused while the daemon runs.
        self.next_id = self.next_id.max(10000) + 1;
        let id = self.next_id;
        self.sessions.insert(
            id,
            Session { id, info, port, since: SystemTime::now(), last_seen: now, tick: 0, activity: 0, effects: HashMap::new(), active: BTreeMap::new() },
        );
        Some(id)
    }

    pub fn set_port(&mut self, id: u32, port: u16) {
        if let Some(s) = self.sessions.get_mut(&id) {
            s.port = port;
        }
    }

    pub fn contains(&self, id: u32) -> bool {
        self.sessions.contains_key(&id)
    }

    pub fn session_ids(&self) -> Vec<u32> {
        self.sessions.keys().copied().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Any request keeps a session alive.
    pub fn touch(&mut self, id: u32, now: Instant) -> bool {
        self.sessions.get_mut(&id).map(|s| s.last_seen = now).is_some()
    }

    pub fn heartbeat(&mut self, id: u32, now: Instant) -> Option<u64> {
        let s = self.sessions.get_mut(&id)?;
        s.last_seen = now;
        s.tick += 1;
        Some(s.tick)
    }

    pub fn end(&mut self, id: u32) -> bool {
        self.sessions.remove(&id).is_some()
    }

    /// Drop sessions idle for longer than `timeout`; returns their ids.
    pub fn expire(&mut self, now: Instant, timeout: Duration) -> Vec<u32> {
        let old: Vec<u32> = self.sessions.values().filter(|s| now.duration_since(s.last_seen) > timeout).map(|s| s.id).collect();
        for id in &old {
            self.sessions.remove(id);
        }
        old
    }

    /// Show an effect now (`PUT /<device>`).
    pub fn apply(&mut self, id: u32, target: Target, effect: Effect, now: Instant) -> i64 {
        self.seq += 1;
        let seq = self.seq;
        let Some(s) = self.sessions.get_mut(&id) else { return result::NOT_FOUND };
        s.last_seen = now;
        s.activity = seq;
        match effect {
            Effect::None => {
                s.active.remove(&target);
            }
            Effect::Show(show) => {
                // A new static frame keeps the animation clock of nothing; restart it.
                s.active.insert(target, Active { show, since: now });
            }
        }
        result::SUCCESS
    }

    /// Store an effect (`POST /<device>`); returns its id.
    pub fn store(&mut self, id: u32, target: Target, effect: Effect, now: Instant) -> Result<String, i64> {
        let s = self.sessions.get_mut(&id).ok_or(result::NOT_FOUND)?;
        s.last_seen = now;
        if s.effects.len() >= MAX_STORED_EFFECTS {
            return Err(result::FAILED);
        }
        let eid = new_uuid();
        s.effects.insert(eid.clone(), (target, effect));
        Ok(eid)
    }

    /// Show a stored effect (`PUT /effect`).
    pub fn apply_stored(&mut self, id: u32, eid: &str, now: Instant) -> i64 {
        let Some(s) = self.sessions.get(&id) else { return result::NOT_FOUND };
        let Some((target, effect)) = s.effects.get(&eid.to_ascii_uppercase()).cloned() else { return result::NOT_FOUND };
        self.apply(id, target, effect, now)
    }

    /// Delete a stored effect (`DELETE /effect`).
    pub fn delete_stored(&mut self, id: u32, eid: &str, now: Instant) -> i64 {
        let Some(s) = self.sessions.get_mut(&id) else { return result::NOT_FOUND };
        s.last_seen = now;
        if s.effects.remove(&eid.to_ascii_uppercase()).is_some() { result::SUCCESS } else { result::NOT_FOUND }
    }

    /// The session currently driving `target` (most recent activity) and its effect.
    pub fn winner(&self, target: Target) -> Option<(u32, &Show, Instant)> {
        self.sessions
            .values()
            .filter_map(|s| s.active.get(&target).map(|a| (s.activity, s.id, a)))
            .max_by_key(|(act, id, _)| (*act, *id))
            .map(|(_, id, a)| (id, &a.show, a.since))
    }

    pub fn session(&self, id: u32) -> Option<&Session> {
        self.sessions.get(&id)
    }

    #[cfg(test)]
    pub fn stored_count(&self, id: u32) -> usize {
        self.sessions.get(&id).map_or(0, |s| s.effects.len())
    }
}

/// Random RFC 4122 v4 id, upper case like the documented examples.
pub fn new_uuid() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let c = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
        b = (t ^ (c << 64) ^ c.wrapping_mul(0x9e37_79b9_7f4a_7c15)).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ---- devices ------------------------------------------------------------------------------

/// Where frames go: the real devices ([`ManagerSink`]) or a mock in tests.
pub trait Sink: Send + Sync {
    /// Attached devices: (pid, kind).
    fn devices(&self) -> Vec<(String, tsuba_hid::Kind)>;
    /// How a device takes colours (queried once per attach; may do device I/O).
    fn describe(&self, pid: &str) -> Result<DeviceMap>;
    /// Take a device over for a game: suspend the user's lighting. False = not allowed
    /// (read-only daemon, untested device without opt-in…).
    fn take(&self, pid: &str) -> bool;
    /// Send a frame (`Output::Frame`: rows; `Output::Zones`: one row, a colour per region).
    /// `changed` = the rows/zones that differ from the last frame sent; `arm` = (re-)enable
    /// custom-frame mode afterwards.
    fn send(&self, pid: &str, out: &Output, frame: &[Vec<Rgb>], changed: &[usize], arm: bool) -> Result<()>;
    /// Hand the device back: restore the user's lighting.
    fn release(&self, pid: &str);
}

struct Held {
    last: Option<Vec<Vec<Rgb>>>,
    sent_at: Instant,
    none_since: Option<Instant>,
    session: u32,
}

#[derive(Default)]
struct OutDev {
    map: Option<DeviceMap>,
    held: Option<Held>,
    /// The session we last refused (not writable) — log once.
    refused: Option<u32>,
}

/// Game lighting service: engine + output thread (+ HTTP server, see `game_http`).
pub struct GameLighting {
    pub(crate) engine: Mutex<Engine>,
    wake: Condvar,
    sink: Arc<dyn Sink>,
    on_change: Box<dyn Fn(String) + Send + Sync>,
    pub(crate) enabled: AtomicBool,
    /// False = never open the REST server (read-only test daemons without a port override).
    pub listen_allowed: AtomicBool,
    pub underglow: AtomicBool,
    /// Answer web pages (requests with `Origin` / `Sec-Fetch-Site`). Off by default: any
    /// website could otherwise change the lighting.
    pub web: AtomicBool,
    pub timeout: Mutex<Duration>,
    /// Which applications may control the lighting (and the file it is saved to, if any).
    rules: Mutex<(gl::AppRules, Option<std::path::PathBuf>)>,
    stop: AtomicBool,
    output: Mutex<Option<JoinHandle<()>>>,
    pub(crate) server: Mutex<Option<crate::game_http::Server>>,
    /// Main port to bind when enabled (54235; tests use 0 = any).
    pub(crate) port: u16,
    /// Devices each session drives right now (for the status).
    active: Mutex<BTreeMap<u32, Vec<String>>>,
    last_status: Mutex<String>,
    /// What the status was last built from (sessions, their targets, active devices): the
    /// output thread rebuilds the status only when this changes, not every frame.
    last_shape: Mutex<Vec<StatusShape>>,
    #[cfg(test)]
    pub(crate) status_builds: std::sync::atomic::AtomicUsize,
}

impl GameLighting {
    /// Device classes (SDK names: "keyboard", "mouse", …) that currently have an attached device.
    pub fn attached_classes(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.sink.devices().into_iter().filter_map(|(_, k)| Target::for_kind(k)).map(|t| t.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    pub fn new(sink: Arc<dyn Sink>, port: u16, on_change: Box<dyn Fn(String) + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            engine: Mutex::new(Engine::default()),
            wake: Condvar::new(),
            sink,
            on_change,
            enabled: AtomicBool::new(false),
            listen_allowed: AtomicBool::new(true),
            underglow: AtomicBool::new(false),
            web: AtomicBool::new(false),
            timeout: Mutex::new(SESSION_TIMEOUT),
            rules: Mutex::new((gl::AppRules::default(), None)),
            stop: AtomicBool::new(false),
            output: Mutex::new(None),
            server: Mutex::new(None),
            port,
            active: Mutex::new(BTreeMap::new()),
            last_status: Mutex::new(String::new()),
            last_shape: Mutex::new(Vec::new()),
            #[cfg(test)]
            status_builds: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Load the allow / block list from `path` and keep it saved there.
    pub fn use_rules_file(&self, path: std::path::PathBuf) {
        *lock(&self.rules) = (gl::AppRules::load(&path), Some(path));
    }

    /// Whether this application may open a session (and remember that it tried).
    pub(crate) fn admit(&self, info: &AppInfo) -> bool {
        let key = info.key();
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let mut r = lock(&self.rules);
        let seen_before = r.0.seen.contains_key(&key);
        r.0.record(&key, &info.title, &info.exe, now);
        // Only write the file for a new application (not for every session of a known one).
        if !seen_before {
            if let Some(p) = r.1.clone() {
                let _ = r.0.save(&p);
            }
        }
        r.0.allowed(&key)
    }

    /// The rules, plus the connected applications, as JSON for the GUI:
    /// `{"only_allowed", "apps": [{"key", "name", "exe", "rule": "allow"|"block"|"default", "allowed", "last_seen", "connected"}]}`.
    pub fn rules_json(&self) -> serde_json::Value {
        let connected: Vec<String> = lock(&self.engine).sessions.values().map(|s| s.info.key()).collect();
        let r = lock(&self.rules);
        let mut keys: Vec<&String> = r.0.seen.keys().chain(r.0.rules.keys()).collect();
        keys.sort();
        keys.dedup();
        let apps: Vec<_> = keys
            .into_iter()
            .map(|k| {
                let seen = r.0.seen.get(k);
                json!({
                    "key": k,
                    "name": seen.map(|s| s.name.clone()).unwrap_or_default(),
                    "exe": seen.map(|s| s.exe.clone()).unwrap_or_default(),
                    "rule": match r.0.rules.get(k) { Some(true) => "allow", Some(false) => "block", None => "default" },
                    "allowed": r.0.allowed(k),
                    "last_seen": seen.map(|s| s.last_seen).unwrap_or(0),
                    "connected": connected.contains(k),
                })
            })
            .collect();
        json!({ "only_allowed": r.0.only_allowed, "apps": apps })
    }

    /// Set an application's rule ("allow" / "block" / "default") and end its sessions when it
    /// is no longer allowed.
    pub fn set_app_rule(&self, key: &str, rule: &str) -> Result<()> {
        let allow = match rule {
            "allow" => Some(true),
            "block" => Some(false),
            "default" => None,
            _ => anyhow::bail!("rule must be allow, block or default"),
        };
        self.change_rules(|r| r.set_rule(key, allow))
    }

    /// Switch between "everyone except blocked" and "only allowed applications".
    pub fn set_only_allowed(&self, on: bool) -> Result<()> {
        self.change_rules(|r| {
            r.only_allowed = on;
            Ok(())
        })
    }

    fn change_rules(&self, f: impl FnOnce(&mut gl::AppRules) -> Result<()>) -> Result<()> {
        {
            let mut r = lock(&self.rules);
            f(&mut r.0)?;
            if let Some(p) = r.1.clone() {
                r.0.save(&p)?;
            }
        }
        let rules = lock(&self.rules).0.clone();
        let gone: Vec<u32> = lock(&self.engine).sessions.values().filter(|s| !rules.allowed(&s.info.key())).map(|s| s.id).collect();
        for id in gone {
            info!("game lighting: session {id} ended (application blocked)");
            self.end_session(id);
        }
        self.status_changed();
        Ok(())
    }

    /// Start or stop the REST server (and the output thread with it). Ending the server
    /// ends every session, which hands the devices back.
    pub fn set_enabled(self: &Arc<Self>, on: bool) -> Result<()> {
        if on {
            anyhow::ensure!(
                self.listen_allowed.load(Ordering::Relaxed),
                "tsubad is running read-only; game lighting is off (set TSUBA_GAME_LIGHTING_PORT to test it)"
            );
            self.start_output();
            let mut server = lock(&self.server);
            if server.is_none() {
                *server = Some(crate::game_http::Server::start(self.clone(), self.port)?);
                info!("game lighting: listening on 127.0.0.1:{}", server.as_ref().map_or(0, |s| s.port()));
            }
            self.enabled.store(true, Ordering::Relaxed);
        } else {
            self.enabled.store(false, Ordering::Relaxed);
            let server = lock(&self.server).take();
            drop(server);
            let ids = lock(&self.engine).session_ids();
            for id in ids {
                self.end_session(id);
            }
            info!("game lighting: off");
        }
        self.notify();
        self.status_changed();
        Ok(())
    }

    /// The bound main port (None when off).
    pub fn bound_port(&self) -> Option<u16> {
        lock(&self.server).as_ref().map(|s| s.port())
    }

    pub(crate) fn notify(&self) {
        self.wake.notify_all();
    }

    /// End a session and close its listener.
    pub(crate) fn end_session(&self, id: u32) -> bool {
        let ended = lock(&self.engine).end(id);
        if let Some(s) = lock(&self.server).as_ref() {
            s.close_session(id);
        }
        if ended {
            info!("game lighting: session {id} ended");
            self.notify();
        }
        ended
    }

    fn start_output(self: &Arc<Self>) {
        let mut out = lock(&self.output);
        if out.is_some() {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        let me = self.clone();
        *out = std::thread::Builder::new().name("game-lighting".into()).spawn(move || me.output_loop()).ok();
    }

    /// Stop everything and hand all devices back (daemon shutdown).
    pub fn shutdown(self: &Arc<Self>) {
        let _ = self.set_enabled(false);
        self.stop.store(true, Ordering::Relaxed);
        self.notify();
        if let Some(t) = lock(&self.output).take() {
            let _ = t.join();
        }
    }

    /// JSON status for D-Bus: `{enabled, port, sessions: [{id, title, description, author,
    /// contact, category, device_supported, since, targets, devices_active}]}`.
    pub fn status(&self) -> serde_json::Value {
        #[cfg(test)]
        self.status_builds.fetch_add(1, Ordering::Relaxed);
        let eng = lock(&self.engine);
        let active = lock(&self.active);
        let sessions: Vec<_> = eng
            .sessions
            .values()
            .map(|s| {
                json!({
                    "id": s.id,
                    "title": s.info.title,
                    "description": s.info.description,
                    "author": s.info.author,
                    "contact": s.info.contact,
                    "category": s.info.category,
                    "app": s.info.key(),
                    "exe": s.info.exe,
                    "device_supported": s.info.devices,
                    "since": s.since.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                    "targets": s.active.keys().map(|t| t.as_str()).collect::<Vec<_>>(),
                    "devices_active": active.get(&s.id).cloned().unwrap_or_default(),
                })
            })
            .collect();
        json!({ "enabled": self.enabled(), "port": self.bound_port(), "sessions": sessions })
    }

    fn status_changed(&self) {
        let s = self.status().to_string();
        let mut last = lock(&self.last_status);
        if *last != s {
            *last = s.clone();
            drop(last);
            (self.on_change)(s);
        }
    }

    fn output_loop(self: Arc<Self>) {
        let mut devs: HashMap<String, OutDev> = HashMap::new();
        let mut next = Instant::now();
        loop {
            // Idle: sleep until something happens; busy: run at the frame rate.
            {
                let eng = lock(&self.engine);
                let busy = !eng.is_empty() || devs.values().any(|d| d.held.is_some());
                let wait = if busy { next.saturating_duration_since(Instant::now()) } else { Duration::from_millis(500) };
                if !wait.is_zero() && !self.stop.load(Ordering::Relaxed) {
                    let _ = self.wake.wait_timeout(eng, wait).unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
            let now = Instant::now();
            if now < next && !self.stop.load(Ordering::Relaxed) {
                // Woken early by a new effect: keep the frame rate (coalesce).
                std::thread::sleep(next - now);
            }
            next = Instant::now() + FRAME;
            if self.stop.load(Ordering::Relaxed) {
                for (pid, d) in devs.iter_mut() {
                    if d.held.take().is_some() {
                        self.sink.release(pid);
                    }
                }
                lock(&self.active).clear();
                return;
            }
            self.step(&mut devs);
        }
    }

    /// One output step: expire sessions, pick each device's winner, send/release.
    fn step(&self, devs: &mut HashMap<String, OutDev>) {
        let now = Instant::now();
        // Snapshot the winners without holding the engine lock during device I/O.
        let (winners, expired) = {
            let mut eng = lock(&self.engine);
            let timeout = *lock(&self.timeout);
            let expired = eng.expire(now, timeout);
            let winners: HashMap<Target, (u32, Show, f32)> = Target::ALL
                .iter()
                .filter_map(|t| eng.winner(*t).map(|(id, show, since)| (*t, (id, show.clone(), now.duration_since(since).as_secs_f32()))))
                .collect();
            (winners, expired)
        };
        for id in expired {
            info!("game lighting: session {id} timed out");
            if let Some(s) = lock(&self.server).as_ref() {
                s.close_session(id);
            }
        }
        let underglow: Option<Vec<Rgb>> = self.underglow.load(Ordering::Relaxed).then(|| {
            [Target::ChromaLink, Target::Mousepad, Target::Headset].iter().find_map(|t| winners.get(t)).map(|(_, show, t)| show.grid(*t).cells)
        }).flatten();

        let attached = self.sink.devices();
        devs.retain(|pid, _| attached.iter().any(|(p, _)| p == pid));
        let mut active: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for (pid, kind) in attached {
            let Some(target) = Target::for_kind(kind) else { continue };
            let d = devs.entry(pid.clone()).or_default();
            match winners.get(&target) {
                Some((sid, show, t)) => {
                    if d.held.is_none() {
                        if d.refused == Some(*sid) {
                            continue;
                        }
                        if d.map.is_none() {
                            match self.sink.describe(&pid) {
                                Ok(m) => d.map = Some(m),
                                Err(e) => {
                                    warn!("{pid}: game lighting can't drive this device: {e:#}");
                                    d.refused = Some(*sid);
                                    continue;
                                }
                            }
                        }
                        if !self.sink.take(&pid) {
                            d.refused = Some(*sid);
                            continue;
                        }
                        info!("{pid}: game lighting took over (session {sid})");
                        d.held = Some(Held { last: None, sent_at: now, none_since: None, session: *sid });
                    }
                    d.refused = None;
                    let map = d.map.as_ref().expect("described");
                    let h = d.held.as_mut().expect("held");
                    h.none_since = None;
                    h.session = *sid;
                    let ug = if target == Target::Keyboard || target == Target::Mouse { underglow.as_deref() } else { None };
                    let frame = map.render(show, *t, ug);
                    let refresh = h.last.is_none() || now.duration_since(h.sent_at) >= REFRESH;
                    let changed: Vec<usize> = match (&h.last, refresh) {
                        (Some(last), false) => (0..frame.len()).filter(|&i| last.get(i) != frame.get(i)).collect(),
                        _ => (0..frame.len()).collect(),
                    };
                    let changed = if let Output::Zones(_) = map.out {
                        // One row of zones: send the zones that changed.
                        match (&h.last, refresh) {
                            (Some(last), false) => (0..frame[0].len()).filter(|&i| last[0].get(i) != frame[0].get(i)).collect(),
                            _ => (0..frame.first().map_or(0, |r| r.len())).collect(),
                        }
                    } else {
                        changed
                    };
                    if !changed.is_empty() {
                        if let Err(e) = self.sink.send(&pid, &map.out, &frame, &changed, refresh) {
                            warn!("{pid}: game lighting frame: {e:#}");
                        }
                        if refresh {
                            h.sent_at = now;
                        }
                        h.last = Some(frame);
                    }
                    active.entry(*sid).or_default().push(pid.clone());
                }
                None => {
                    d.refused = None;
                    if let Some(h) = d.held.as_mut() {
                        let since = *h.none_since.get_or_insert(now);
                        if now.duration_since(since) >= RELEASE_GRACE || !lock(&self.engine).contains(h.session) {
                            d.held = None;
                            info!("{pid}: game lighting released");
                            self.sink.release(&pid);
                        }
                    }
                }
            }
        }
        // Status only when sessions, their targets or the driven devices changed.
        let shape: Vec<StatusShape> = {
            let eng = lock(&self.engine);
            eng.sessions.values().map(|s| (s.id, s.active.keys().copied().collect(), active.get(&s.id).cloned().unwrap_or_default())).collect()
        };
        *lock(&self.active) = active;
        let changed = {
            let mut last = lock(&self.last_shape);
            let changed = *last != shape;
            *last = shape;
            changed
        };
        if changed {
            self.status_changed();
        }
    }
}

// ---- the real devices --------------------------------------------------------------------

/// Drives attached devices through the [`crate::manager::Manager`], respecting its write
/// gates (read-only daemon, untested models).
pub struct ManagerSink(pub Arc<crate::manager::Manager>);

impl Sink for ManagerSink {
    fn devices(&self) -> Vec<(String, tsuba_hid::Kind)> {
        lock(&self.0.devices).iter().map(|(k, a)| (k.clone(), a.model.spec.kind)).collect()
    }

    fn describe(&self, pid: &str) -> Result<DeviceMap> {
        use tsuba_core::studio::{pack_path, Geometry};
        use tsuba_proto::chroma;
        self.0.with_device(pid, |dev, model| {
            let kind = model.spec.kind;
            if model.pid == 0x00aa {
                return DeviceMap::new(model.pid, kind, Output::Frame { rows: 1, cols: 13 }, None).ok_or_else(|| anyhow::anyhow!("no game lighting target"));
            }
            let regions = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?);
            let big = regions.iter().max_by_key(|r| r.rows as u32 * r.cols as u32).ok_or_else(|| anyhow::anyhow!("no lighting region"))?;
            let (out, geo) = if big.rows as u32 * big.cols as u32 > 1 && (model.spec.effects.is_empty() || model.spec.has_effect("custom_frame")) {
                (Output::Frame { rows: big.rows, cols: big.cols }, Geometry::from_pack(&pack_path(pid), big.rows, big.cols))
            } else {
                (Output::Zones(regions.iter().map(|r| r.id).collect()), None)
            };
            DeviceMap::new(model.pid, kind, out, geo.as_ref()).ok_or_else(|| anyhow::anyhow!("no game lighting target"))
        })
    }

    fn take(&self, pid: &str) -> bool {
        match self.0.game_take(pid) {
            Ok(()) => true,
            Err(e) => {
                info!("{pid}: game lighting skipped: {e:#}");
                false
            }
        }
    }

    fn send(&self, pid: &str, out: &Output, frame: &[Vec<Rgb>], changed: &[usize], arm: bool) -> Result<()> {
        use tsuba_proto::chroma;
        self.0.with_device(pid, |dev, _| {
            match out {
                Output::Frame { .. } => {
                    let rows = |dev: &tsuba_hid::Device| -> Result<()> {
                        for &r in changed {
                            let leds = &frame[r];
                            dev.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(r as u8, 0, leds))?;
                        }
                        Ok(())
                    };
                    if arm {
                        // Diagnostics while bringing up mice: what the device is being sent.
                        let first: Vec<String> = frame.first().map(|r| r.iter().take(13).map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect()).unwrap_or_default();
                        tracing::debug!("{pid}: game frame arm, {} row(s), row0 = {}", frame.len(), first.join(" "));
                    }
                    rows(dev)?;
                    if arm {
                        dev.query(chroma::set_effect(0), &chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame))?;
                        // Some devices (the Basilisk) clear the frame buffer when custom-frame mode is
                        // (re)armed: send the rows again so they never show their stored effect.
                        rows(dev)?;
                    }
                }
                Output::Zones(ids) => {
                    for &i in changed {
                        let (Some(&id), Some(&c)) = (ids.get(i), frame[0].get(i)) else { continue };
                        dev.query(chroma::set_effect(1), &chroma::encode_effect(chroma::NOSTORE, id, &chroma::Effect::Static(c)))?;
                    }
                }
            }
            Ok(())
        })
    }

    fn release(&self, pid: &str) {
        self.0.game_release(pid);
    }
}

/// Parse an effect body for the REST layer.
pub fn parse(target: Target, v: &serde_json::Value) -> Result<Effect, i64> {
    gl::parse_effect(target, v).map_err(|e| e.code())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn info(title: &str) -> AppInfo {
        AppInfo { title: title.into(), ..Default::default() }
    }

    fn red() -> Effect {
        parse(Target::Keyboard, &json!({"effect": "CHROMA_STATIC", "param": {"color": 255}})).unwrap()
    }

    #[test]
    fn session_lifecycle_and_expiry() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let a = e.create(info("A"), 1, t0).unwrap();
        let b = e.create(info("B"), 2, t0).unwrap();
        assert_ne!(a, b);
        assert_eq!(e.heartbeat(a, t0), Some(1));
        assert_eq!(e.heartbeat(a, t0), Some(2));
        assert_eq!(e.heartbeat(999, t0), None);
        // b keeps alive, a goes quiet.
        let t1 = t0 + Duration::from_secs(10);
        e.heartbeat(b, t1);
        assert!(e.expire(t1, SESSION_TIMEOUT).is_empty());
        let t2 = t0 + Duration::from_secs(16);
        assert_eq!(e.expire(t2, SESSION_TIMEOUT), vec![a]);
        assert!(e.contains(b) && !e.contains(a));
        assert!(e.end(b) && !e.end(b));
        // Session cap.
        for _ in 0..MAX_SESSIONS {
            e.create(info("x"), 0, t0).unwrap();
        }
        assert!(e.create(info("x"), 0, t0).is_none());
    }

    #[test]
    fn effect_storage_and_priority() {
        let mut e = Engine::default();
        let t0 = Instant::now();
        let a = e.create(info("A"), 1, t0).unwrap();
        let b = e.create(info("B"), 2, t0).unwrap();
        let id = e.store(a, Target::Keyboard, red(), t0).unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(e.stored_count(a), 1);
        assert!(e.winner(Target::Keyboard).is_none());
        assert_eq!(e.apply_stored(a, &id, t0), 0);
        assert_eq!(e.apply_stored(a, &id.to_lowercase(), t0), 0);
        assert_eq!(e.winner(Target::Keyboard).unwrap().0, a);
        // b applies later → b wins; a again → a wins.
        assert_eq!(e.apply(b, Target::Keyboard, red(), t0), 0);
        assert_eq!(e.winner(Target::Keyboard).unwrap().0, b);
        assert_eq!(e.apply_stored(a, &id, t0), 0);
        assert_eq!(e.winner(Target::Keyboard).unwrap().0, a);
        // NONE hands the target to the other session that still has one.
        assert_eq!(e.apply(a, Target::Keyboard, Effect::None, t0), 0);
        assert_eq!(e.winner(Target::Keyboard).unwrap().0, b);
        // Deleting.
        assert_eq!(e.delete_stored(a, &id, t0), 0);
        assert_eq!(e.delete_stored(a, &id, t0), result::NOT_FOUND);
        assert_eq!(e.apply_stored(a, &id, t0), result::NOT_FOUND);
        assert_eq!(e.apply(777, Target::Mouse, red(), t0), result::NOT_FOUND);
        e.end(b);
        assert!(e.winner(Target::Keyboard).is_none());
    }

    /// Records what the output thread does.
    #[derive(Default)]
    pub struct MockSink {
        pub devices: Mutex<Vec<(String, tsuba_hid::Kind)>>,
        pub log: Mutex<Vec<String>>,
        pub frames: Mutex<HashMap<String, Vec<Vec<Rgb>>>>,
        pub locked: Mutex<Vec<String>>,
    }

    impl Sink for MockSink {
        fn devices(&self) -> Vec<(String, tsuba_hid::Kind)> {
            lock(&self.devices).clone()
        }
        fn describe(&self, pid: &str) -> Result<DeviceMap> {
            let kind = lock(&self.devices).iter().find(|(p, _)| p == pid).map(|d| d.1).unwrap();
            let n = u16::from_str_radix(pid, 16).unwrap();
            let out = if n == 0x00aa { Output::Frame { rows: 1, cols: 13 } } else { Output::Frame { rows: 9, cols: 24 } };
            let geo = (n == 0x028d).then(|| {
                let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices/028d/device.json");
                tsuba_core::studio::Geometry::from_pack(&p, 9, 24).unwrap()
            });
            Ok(DeviceMap::new(n, kind, out, geo.as_ref()).unwrap())
        }
        fn take(&self, pid: &str) -> bool {
            if lock(&self.locked).iter().any(|p| p == pid) {
                lock(&self.log).push(format!("refused {pid}"));
                return false;
            }
            lock(&self.log).push(format!("take {pid}"));
            true
        }
        fn send(&self, pid: &str, _: &Output, frame: &[Vec<Rgb>], changed: &[usize], arm: bool) -> Result<()> {
            lock(&self.log).push(format!("send {pid} rows={} arm={arm}", changed.len()));
            lock(&self.frames).insert(pid.into(), frame.to_vec());
            Ok(())
        }
        fn release(&self, pid: &str) {
            lock(&self.log).push(format!("release {pid}"));
            lock(&self.frames).remove(pid);
        }
    }

    #[test]
    fn output_takes_streams_and_releases() {
        let sink = Arc::new(MockSink::default());
        lock(&sink.devices).push(("028d".into(), tsuba_hid::Kind::Keyboard));
        lock(&sink.devices).push(("00aa".into(), tsuba_hid::Kind::Mouse));
        lock(&sink.devices).push(("0084".into(), tsuba_hid::Kind::Mouse));
        lock(&sink.locked).push("0084".into());
        let gl = GameLighting::new(sink.clone(), 0, Box::new(|_| {}));
        let mut devs = HashMap::new();
        let now = Instant::now();
        let id = lock(&gl.engine).create(info("Game"), 0, now).unwrap();
        lock(&gl.engine).apply(id, Target::Keyboard, red(), now);
        gl.step(&mut devs);
        let log = lock(&sink.log).clone();
        assert_eq!(log, ["take 028d", "send 028d rows=9 arm=true"]);
        assert_eq!(lock(&sink.frames)["028d"][0][2], [255, 0, 0]);
        // Unchanged frame: nothing sent.
        gl.step(&mut devs);
        assert_eq!(lock(&sink.log).len(), 2);
        // Mouse effect: Basilisk taken; the untested mouse refused once.
        let mouse = parse(Target::Mouse, &json!({"effect": "CHROMA_STATIC", "param": {"color": 65280}})).unwrap();
        lock(&gl.engine).apply(id, Target::Mouse, mouse, now);
        gl.step(&mut devs);
        gl.step(&mut devs);
        let log = lock(&sink.log).clone();
        assert_eq!(log[2..], ["take 00aa", "send 00aa rows=1 arm=true", "refused 0084"]);
        assert_eq!(lock(&sink.frames)["00aa"][0][0], [0, 255, 0]);
        let st = gl.status();
        assert_eq!(st["sessions"][0]["devices_active"], json!(["028d", "00aa"]));
        // Ending the session releases at once.
        lock(&gl.engine).end(id);
        gl.step(&mut devs);
        let log = lock(&sink.log).clone();
        assert!(log.contains(&"release 028d".to_string()) && log.contains(&"release 00aa".to_string()));
        assert!(lock(&sink.frames).is_empty());
    }

    #[test]
    fn none_releases_after_grace() {
        let sink = Arc::new(MockSink::default());
        lock(&sink.devices).push(("028d".into(), tsuba_hid::Kind::Keyboard));
        let gl = GameLighting::new(sink.clone(), 0, Box::new(|_| {}));
        let mut devs = HashMap::new();
        let now = Instant::now();
        let id = lock(&gl.engine).create(info("Game"), 0, now).unwrap();
        lock(&gl.engine).apply(id, Target::Keyboard, red(), now);
        gl.step(&mut devs);
        lock(&gl.engine).apply(id, Target::Keyboard, Effect::None, now);
        gl.step(&mut devs);
        assert!(!lock(&sink.log).iter().any(|l| l.starts_with("release")));
        std::thread::sleep(RELEASE_GRACE + Duration::from_millis(20));
        gl.step(&mut devs);
        assert!(lock(&sink.log).iter().any(|l| l == "release 028d"));
    }

    /// The output thread used to rebuild (and diff) the whole status JSON every frame; now only
    /// when sessions, their targets or the driven devices change.
    #[test]
    fn status_is_not_rebuilt_every_frame() {
        let sink = Arc::new(MockSink::default());
        lock(&sink.devices).push(("028d".into(), tsuba_hid::Kind::Keyboard));
        let changes = Arc::new(Mutex::new(0usize));
        let ch = changes.clone();
        let gl = GameLighting::new(sink.clone(), 0, Box::new(move |_| *lock(&ch) += 1));
        let mut devs = HashMap::new();
        let now = Instant::now();
        let id = lock(&gl.engine).create(info("Game"), 0, now).unwrap();
        lock(&gl.engine).apply(id, Target::Keyboard, red(), now);
        gl.step(&mut devs);
        let (builds, sent) = (gl.status_builds.load(Ordering::Relaxed), *lock(&changes));
        assert_eq!(sent, 1);
        for _ in 0..100 {
            gl.step(&mut devs);
        }
        assert_eq!(gl.status_builds.load(Ordering::Relaxed), builds, "status rebuilt on unchanged frames");
        assert_eq!(*lock(&changes), 1);
        // A change (the session ends) is still reported.
        lock(&gl.engine).end(id);
        gl.step(&mut devs);
        assert_eq!(*lock(&changes), 2);
    }

    #[test]
    fn app_info_is_capped_and_cleaned() {
        let devs: Vec<String> = (0..1000).map(|i| format!("{i}{}", "y".repeat(100))).collect();
        let a = AppInfo::from_json(&json!({"title": "A\u{0}B\u{7f}<i>x</i>", "description": "d".repeat(10_000), "device_supported": devs, "author": {"name": 5}}));
        assert_eq!(a.title, "AB<i>x</i>");
        assert_eq!(a.description.chars().count(), MAX_INFO_CHARS);
        assert_eq!(a.devices.len(), MAX_DEVICE_SUPPORTED);
        assert!(a.devices.iter().all(|d| d.chars().count() == MAX_DEVICE_NAME_CHARS));
        assert_eq!(a.author, "");
    }
}
