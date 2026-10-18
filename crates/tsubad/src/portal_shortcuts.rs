//! Global shortcuts through the xdg-desktop-portal `GlobalShortcuts` interface, for desktops
//! without KDE's kglobalaccel (GNOME and others). The portal takes a list of shortcuts with
//! preferred triggers and tells us when one is pressed or released.
//!
//! A desktop shows its own approval dialog the first time the list is bound, and may let the
//! user pick other triggers; a trigger different from the key combination a slot's key sends
//! can't fire the action, so that is reported in the log.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tracing::{info, warn};
use tsuba_core::config::slot_keys;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::MatchRule;

use crate::manager::Manager;
use crate::shortcuts::SlotEntry;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// How the desktop names this program in its shortcut settings (a reverse-DNS app id).
const APP_ID: &str = "io.github.Jenari_Dev.Tsubad";
/// Waiting for the desktop's approval dialog can take a while.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// One portal answer: the response code and its results.
type Answer = (u32, HashMap<String, OwnedValue>);
/// Requests waiting for their `Response`, by request path.
type Waiters = Arc<Mutex<HashMap<String, mpsc::Sender<Answer>>>>;

/// How the last bind went (see [`Portal::bind_state`]).
pub const BIND_IDLE: u8 = 0;
pub const BIND_PENDING: u8 = 1;
pub const BIND_FAILED: u8 = 2;

pub struct Portal {
    conn: Connection,
    session: OwnedObjectPath,
    waiters: Waiters,
    /// How long a request waits for the desktop's answer.
    timeout: Duration,
    /// What the desktop has been given successfully.
    bound: Mutex<Vec<SlotEntry>>,
    /// The latest list wanted, picked up by the binding thread.
    wanted: Mutex<Option<Vec<SlotEntry>>>,
    working: AtomicBool,
    state: AtomicU8,
}

/// The portal's trigger string for shortcut slot `slot` (XKB key names, CTRL/ALT/SHIFT).
pub fn trigger(slot: u8) -> String {
    let [mods, hid] = slot_keys(slot);
    let mut parts: Vec<&str> = Vec::new();
    for (bit, name) in [(0x01u8, "CTRL"), (0x04, "ALT"), (0x02, "SHIFT")] {
        if mods & bit != 0 {
            parts.push(name);
        }
    }
    let key = match hid {
        0x69..=0x6d => format!("XF86Launch{}", 5 + (hid - 0x69)), // F14–F18 under the default XKB layouts
        0x6e => "F19".to_string(),
        _ => "F24".to_string(),
    };
    parts.push(&key);
    parts.join("+")
}

fn portal_call(conn: &Connection, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> Result<zbus::Message> {
    Ok(conn.call_method(Some(PORTAL), PORTAL_PATH, Some(IFACE), method, body)?)
}

/// One thread listens to every `Response` the portal sends and hands it to whoever waits for
/// that request path, so a request that times out leaves nothing behind but a map entry,
/// which its caller removes. Only the portal's own signals count: another client could
/// otherwise answer in its place.
fn spawn_router(conn: &Connection, waiters: &Waiters) -> Result<()> {
    let rule = MatchRule::builder().msg_type(Type::Signal).sender(PORTAL)?.interface("org.freedesktop.portal.Request")?.member("Response")?.build();
    let responses = MessageIterator::for_match_rule(rule, conn, None)?;
    let waiters = waiters.clone();
    std::thread::Builder::new()
        .name("portal-responses".into())
        .spawn(move || {
            for msg in responses {
                let Ok(msg) = msg else { continue };
                let Some(path) = msg.header().path().map(|p| p.to_string()) else { continue };
                let Ok(answer) = msg.body().deserialize::<Answer>() else { continue };
                if let Some(tx) = waiters.lock().unwrap().remove(&path) {
                    let _ = tx.send(answer);
                }
            }
        })?;
    Ok(())
}

/// Call a portal method that answers through a `Request` object: register for its `Response`
/// first (the path is known from our bus name and the handle token), call, wait.
fn request(conn: &Connection, waiters: &Waiters, token: &str, timeout: Duration, call: impl FnOnce() -> Result<zbus::Message>) -> Result<HashMap<String, OwnedValue>> {
    let me = conn.unique_name().context("no bus name")?.as_str().trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{me}/{token}");
    let (tx, rx) = mpsc::channel();
    waiters.lock().unwrap().insert(path.clone(), tx);
    let result = (|| {
        let handle: OwnedObjectPath = call()?.body().deserialize()?;
        if handle.as_str() != path {
            // An older portal that doesn't honour the token: its Response goes to another path.
            bail!("the portal answered on {} instead of {path}", handle.as_str());
        }
        rx.recv_timeout(timeout).context("the desktop didn't answer the shortcut request in time")
    })();
    waiters.lock().unwrap().remove(&path);
    let (code, results) = result?;
    match code {
        0 => Ok(results),
        1 => bail!("the shortcut request was declined"),
        _ => bail!("the shortcut request failed (portal response {code})"),
    }
}

fn token(what: &str) -> String {
    format!("tsuba_{what}_{}", std::process::id())
}

impl Portal {
    /// Whether a portal with GlobalShortcuts is on the bus.
    pub fn available(conn: &Connection) -> bool {
        conn.call_method(Some(PORTAL), PORTAL_PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(IFACE, "version")).is_ok()
    }

    /// Open a session (the desktop may ask the user to allow it).
    pub fn connect(conn: Connection) -> Result<Self> {
        // Desktops that identify host (non-sandboxed) programs want an app id first. Best effort:
        // older portals don't have the registry.
        let opts: HashMap<&str, Value> = HashMap::new();
        let _ = conn.call_method(Some(PORTAL), "/org/freedesktop/host/portal/Registry", Some("org.freedesktop.host.portal.Registry"), "Register", &(APP_ID, opts));
        let t = token("session");
        let mut opts: HashMap<&str, Value> = HashMap::new();
        opts.insert("handle_token", Value::from(t.as_str()));
        opts.insert("session_handle_token", Value::from(t.as_str()));
        let waiters: Waiters = Arc::default();
        spawn_router(&conn, &waiters)?;
        let results = request(&conn, &waiters, &t, REQUEST_TIMEOUT, || portal_call(&conn, "CreateSession", &(opts,)))?;
        let session = results.get("session_handle").context("the portal returned no session")?;
        let session = String::try_from(session.try_clone()?).context("session handle")?;
        let session = OwnedObjectPath::try_from(session)?;
        Ok(Self { conn, session, waiters, timeout: REQUEST_TIMEOUT, bound: Mutex::new(Vec::new()), wanted: Mutex::new(None), working: AtomicBool::new(false), state: AtomicU8::new(BIND_IDLE) })
    }

    /// Bind (or re-bind) the shortcut list. A portal replaces its list on every bind, so this
    /// always sends all of `entries`.
    pub fn bind(&self, entries: &[SlotEntry]) -> Result<()> {
        if *self.bound.lock().unwrap() == entries {
            return Ok(());
        }
        if entries.is_empty() {
            *self.bound.lock().unwrap() = Vec::new();
            return Ok(());
        }
        let list: Vec<(String, HashMap<&str, Value>)> = entries
            .iter()
            .map(|e| {
                let mut o: HashMap<&str, Value> = HashMap::new();
                o.insert("description", Value::from(e.label.as_str()));
                o.insert("preferred_trigger", Value::from(trigger(e.code)));
                (e.id.clone(), o)
            })
            .collect();
        let t = token("bind");
        let mut opts: HashMap<&str, Value> = HashMap::new();
        opts.insert("handle_token", Value::from(t.as_str()));
        let results = request(&self.conn, &self.waiters, &t, self.timeout, || portal_call(&self.conn, "BindShortcuts", &(&self.session, list, "", opts)))?;
        // Only now does the desktop have this list; a failed or declined bind is tried again
        // the next time the list is registered.
        *self.bound.lock().unwrap() = entries.to_vec();
        // The desktop reports the triggers it actually assigned.
        if let Some(v) = results.get("shortcuts") {
            if let Ok(got) = <Vec<(String, HashMap<String, OwnedValue>)>>::try_from(v.try_clone()?) {
                for (id, o) in got {
                    let want = entries.iter().find(|e| e.id == id).map(|e| trigger(e.code));
                    let have = o.get("trigger_description").and_then(|t| String::try_from(t.try_clone().ok()?).ok());
                    info!("portal shortcut {id}: wants {} (desktop shows {})", want.unwrap_or_default(), have.unwrap_or_default());
                }
            }
        }
        Ok(())
    }
}

impl Portal {
    /// Bind `entries` on a background thread: the desktop's approval dialog can stay open for
    /// minutes and callers (assigning an action, saving a macro) must not wait for it. Newer
    /// lists replace older ones that were not sent yet.
    pub fn bind_async(self: &Arc<Self>, entries: &[SlotEntry]) {
        *self.wanted.lock().unwrap() = Some(entries.to_vec());
        if self.working.swap(true, SeqCst) {
            return;
        }
        let p = self.clone();
        let spawned = std::thread::Builder::new().name("portal-bind".into()).spawn(move || loop {
            let next = p.wanted.lock().unwrap().take();
            match next {
                Some(list) => {
                    p.state.store(BIND_PENDING, SeqCst);
                    match p.bind(&list) {
                        Ok(()) => p.state.store(BIND_IDLE, SeqCst),
                        Err(e) => {
                            p.state.store(BIND_FAILED, SeqCst);
                            warn!("binding shortcuts through the desktop portal: {e:#}");
                        }
                    }
                }
                None => {
                    p.working.store(false, SeqCst);
                    if p.wanted.lock().unwrap().is_some() && !p.working.swap(true, SeqCst) {
                        continue;
                    }
                    break;
                }
            }
        });
        if spawned.is_err() {
            self.working.store(false, SeqCst);
        }
    }

    /// [`BIND_IDLE`] (everything asked for is bound), [`BIND_PENDING`] (waiting for the
    /// desktop, e.g. its approval dialog) or [`BIND_FAILED`].
    pub fn bind_state(&self) -> u8 {
        self.state.load(SeqCst)
    }
}

/// Connect, bind the configured actions and run them on the portal's Activated / Deactivated
/// signals. Returns when the signal stream ends.
pub fn run(mgr: &Arc<Manager>, conn: Connection) -> Result<()> {
    if !Portal::available(&conn) {
        warn!("global shortcuts: neither KDE's kglobalaccel nor the desktop portal's GlobalShortcuts is available, so keys bound to Launch / Audio / Inter-device actions have no effect (they still send their key combination)");
        return Ok(());
    }
    // Listen before binding so no early press is lost.
    // Only the portal's own signals count (not another client's look-alikes).
    let rule = MatchRule::builder().msg_type(Type::Signal).sender(PORTAL)?.interface(IFACE)?.build();
    let signals = MessageIterator::for_match_rule(rule, &conn, None)?;
    let portal = Arc::new(Portal::connect(conn.clone()).context("opening a shortcut session with the desktop portal")?);
    let session = portal.session.clone();
    crate::shortcuts::set_portal(portal);
    if let Err(e) = crate::shortcuts::register(&mgr.slot_entries()) {
        warn!("binding shortcuts through the desktop portal: {e:#}");
    }
    info!("listening for Tsuba global shortcuts (desktop portal)");
    for msg in signals {
        let Ok(msg) = msg else { continue };
        let h = msg.header();
        let down = match h.member().map(|m| m.as_str()) {
            Some("Activated") => true,
            Some("Deactivated") => false,
            _ => continue,
        };
        let Ok((handle, id, _ts, _opts)) = msg.body().deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>() else { continue };
        if handle != session {
            continue;
        }
        crate::shortcuts::dispatch(mgr, &id, down);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};

    #[test]
    fn triggers_use_xkb_names() {
        assert_eq!(trigger(0), "XF86Launch5");
        assert_eq!(trigger(4), "XF86Launch9");
        assert_eq!(trigger(5), "F19");
        assert_eq!(trigger(6), "F24");
        assert_eq!(trigger(7), "SHIFT+XF86Launch5");
        assert_eq!(trigger(14), "ALT+XF86Launch5");
        assert_eq!(trigger(21), "ALT+SHIFT+XF86Launch5");
        assert_eq!(trigger(28), "CTRL+XF86Launch5");
        assert_eq!(trigger(34), "CTRL+SHIFT+XF86Launch5");
    }

    struct Bus(Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_bus() -> Option<Bus> {
        let mut child = Command::new("dbus-daemon").args(["--session", "--nofork", "--print-address=1"]).stdout(Stdio::piped()).spawn().ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        Some(Bus(child, line.trim().to_string()))
    }

    /// A stand-in for the desktop portal: answers CreateSession / BindShortcuts through
    /// Request objects like the real one, and can fire Activated / Deactivated.
    struct Mock {
        conn: Connection,
        binds: Arc<Mutex<Vec<(String, String)>>>,
    }

    // The mock needs the caller's unique name inside the request path; a shared slot carries it.
    static CALLER: Mutex<String> = Mutex::new(String::new());
    /// How long the mock desktop takes to answer (an approval dialog).
    static DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(80);

    #[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
    impl Mock {
        #[zbus(property, name = "version")]
        fn version(&self) -> u32 {
            1
        }

        fn create_session(&self, options: HashMap<String, OwnedValue>) -> OwnedObjectPath {
            let mut results: HashMap<&'static str, Value<'static>> = HashMap::new();
            results.insert("session_handle", Value::from("/org/freedesktop/portal/desktop/session/SENDER/mock".to_string()));
            self.respond_as_caller(&options, results)
        }

        fn bind_shortcuts(&self, session: OwnedObjectPath, shortcuts: Vec<(String, HashMap<String, OwnedValue>)>, _parent: String, options: HashMap<String, OwnedValue>) -> OwnedObjectPath {
            assert!(session.as_str().ends_with("/mock"), "{session:?}");
            for (id, o) in shortcuts {
                let trig = String::try_from(o["preferred_trigger"].try_clone().unwrap()).unwrap();
                self.binds.lock().unwrap().push((id, trig));
            }
            self.respond_as_caller(&options, HashMap::new())
        }
    }

    impl Mock {
        fn respond_as_caller(&self, options: &HashMap<String, OwnedValue>, results: HashMap<&'static str, Value<'static>>) -> OwnedObjectPath {
            let caller = CALLER.lock().unwrap().clone();
            let token = String::try_from(options["handle_token"].try_clone().unwrap()).unwrap();
            let path = format!("/org/freedesktop/portal/desktop/request/{caller}/{token}");
            let conn = self.conn.clone();
            let p = path.clone();
            let mut results = results;
            if let Some(Value::Str(s)) = results.get("session_handle").cloned() {
                results.insert("session_handle", Value::from(s.as_str().replace("SENDER", &caller)));
            }
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(DELAY_MS.load(std::sync::atomic::Ordering::SeqCst)));
                let _ = conn.emit_signal(None::<&str>, p.as_str(), "org.freedesktop.portal.Request", "Response", &(0u32, results));
            });
            OwnedObjectPath::try_from(path).unwrap()
        }
    }

    #[test]
    fn portal_handshake_binds_slots_and_runs_the_action() {
        let _s = crate::gamemode::fake_kde::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let Some(bus) = private_bus() else { return };
        // Serving an object needs a tokio context (the daemon always has one).
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let _enter = rt.enter();
        let build = || zbus::blocking::connection::Builder::address(bus.1.as_str()).unwrap();
        let mock_conn = build().build().unwrap();
        let binds = Arc::new(Mutex::new(Vec::new()));
        mock_conn.object_server().at(PORTAL_PATH, Mock { conn: mock_conn.clone(), binds: binds.clone() }).unwrap();
        mock_conn.request_name(PORTAL).unwrap();

        let dir = std::env::temp_dir().join(format!("tsuba-portal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("ran");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mgr = Arc::new(Manager::new(true, dir.join("config.toml"), tx).unwrap());
        {
            let mut cfg = mgr.config.lock().unwrap();
            cfg.upsert(tsuba_core::DeviceConfig {
                pid: "028d".into(),
                actions: vec![tsuba_core::config::ActionConfig { code: 7, profile: 1, layer: 0, key: 160, action: tsuba_core::config::Action::Launch { command: format!("touch {}", marker.display()) } }],
                ..Default::default()
            });
        }
        let client = build().build().unwrap();
        *CALLER.lock().unwrap() = client.unique_name().unwrap().as_str().trim_start_matches(':').replace('.', "_");
        assert!(Portal::available(&client));
        let m = mgr.clone();
        let c = client.clone();
        std::thread::spawn(move || {
            let _ = run(&m, c);
        });
        // The slot was bound with its trigger.
        let t0 = std::time::Instant::now();
        while binds.lock().unwrap().is_empty() && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(binds.lock().unwrap().as_slice(), &[("028d-7".to_string(), "SHIFT+XF86Launch5".to_string())]);
        assert_eq!(crate::shortcuts::backend_name(), "portal");
        // The desktop reports the shortcut pressed: the action runs.
        let session = format!("/org/freedesktop/portal/desktop/session/{}/mock", CALLER.lock().unwrap());
        let body = (OwnedObjectPath::try_from(session).unwrap(), "028d-7".to_string(), 1u64, HashMap::<String, Value>::new());
        mock_conn.emit_signal(None::<&str>, PORTAL_PATH, IFACE, "Activated", &body).unwrap();
        let t0 = std::time::Instant::now();
        while !marker.exists() && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(marker.exists(), "the Launch action didn't run");

        // Signals from anyone but the portal are ignored.
        std::fs::remove_file(&marker).unwrap();
        let spoof = build().build().unwrap();
        let body = (OwnedObjectPath::try_from(format!("/org/freedesktop/portal/desktop/session/{}/mock", CALLER.lock().unwrap())).unwrap(), "028d-7".to_string(), 2u64, HashMap::<String, Value>::new());
        spoof.emit_signal(None::<&str>, PORTAL_PATH, IFACE, "Activated", &body).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(!marker.exists(), "a signal from another client ran an action");

        // A slow approval dialog doesn't hold up whoever registers shortcuts, and the state says so.
        DELAY_MS.store(1200, std::sync::atomic::Ordering::SeqCst);
        let more = [SlotEntry { id: "028d-7".into(), code: 7, label: "a".into() }, SlotEntry { id: "028d-8".into(), code: 8, label: "b".into() }];
        let t0 = std::time::Instant::now();
        crate::shortcuts::register(&more).unwrap();
        assert!(t0.elapsed() < Duration::from_millis(300), "registering waited for the desktop: {:?}", t0.elapsed());
        let t0 = std::time::Instant::now();
        assert_eq!(crate::shortcuts::backend_name(), "portal");
        assert!(t0.elapsed() < Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(crate::shortcuts::backend_state(), "pending");
        let t0 = std::time::Instant::now();
        while crate::shortcuts::backend_state() != "ready" && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(crate::shortcuts::backend_state(), "ready");
        assert!(binds.lock().unwrap().iter().any(|b| b.0 == "028d-8"));
        crate::shortcuts::clear_backend();

        // A request that times out leaves nothing waiting and isn't counted as bound.
        DELAY_MS.store(80, std::sync::atomic::Ordering::SeqCst);
        let client2 = build().build().unwrap();
        *CALLER.lock().unwrap() = client2.unique_name().unwrap().as_str().trim_start_matches(':').replace('.', "_");
        let mut portal = Portal::connect(client2).unwrap();
        portal.timeout = Duration::from_millis(200);
        DELAY_MS.store(1000, std::sync::atomic::Ordering::SeqCst);
        assert!(portal.bind(&more).is_err());
        assert!(portal.waiters.lock().unwrap().is_empty(), "the timed-out request is forgotten");
        assert!(portal.bound.lock().unwrap().is_empty(), "a failed bind isn't remembered as done");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
