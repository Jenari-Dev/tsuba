//! Cache of slow device reads (assignment tables, profile list, performance, power, macro
//! memory), so opening a device page doesn't repeat ~150 HID round-trips per layer.
//!
//! Correctness rests on [`tsuba_hid::WriteCounters`]: every Set command sent through the
//! device bumps the counter of the data it changes, so an entry is current exactly while the
//! counter of its domain still has the value it had when the entry was read. That covers all
//! write paths (D-Bus methods, software actions, game mode, idle, app links …) without each
//! one having to remember to invalidate. Writes the daemon can't see are covered by
//! hardware-event invalidation ([`Manager::on_event`]) and the explicit refresh
//! ([`Manager::refresh_cache`], the GUI's refresh button).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::{debug, info};
use tsuba_core::state::{self, AssignmentState};
use tsuba_hid::{Device, Domain, Model, WriteCounters};
use tsuba_proto::{decode, events};

use crate::manager::Manager;

/// Battery level drifts without any write, so power reads expire.
const POWER_TTL: Duration = Duration::from_secs(20);
/// Keys read per device-lock hold. Between holds other users (Lighting Studio frames, D-Bus
/// calls) get the device.
pub(crate) const CHUNK_FAST: usize = 24;
pub(crate) const CHUNK_WARM: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Assign(u8, u8),
    Profiles,
    Perf(u8),
    Power,
    Macros,
}

impl Key {
    fn domain(self) -> Domain {
        match self {
            Key::Assign(..) => Domain::Assign,
            Key::Profiles | Key::Perf(_) => Domain::Settings,
            Key::Macros => Domain::Macros,
            Key::Power => Domain::Power,
        }
    }
}

struct Entry {
    stamp: u64,
    at: Instant,
    json: String,
}

#[derive(Default)]
pub struct PidCache {
    /// [`WriteCounters::id`] of the attachment the entries were read from.
    dev_id: u64,
    entries: HashMap<Key, Entry>,
}

pub type Cache = std::sync::Mutex<HashMap<String, PidCache>>;
/// Devices with a warm-up running (value: another pass was requested meanwhile).
pub type Warming = std::sync::Mutex<HashMap<String, bool>>;

impl Manager {
    pub(crate) fn counters_of(&self, pid: &str) -> Result<Arc<WriteCounters>> {
        self.devices.lock().unwrap().get(pid).map(|a| a.counters.clone()).ok_or_else(|| anyhow::anyhow!("no device {pid}"))
    }

    fn lookup(&self, pid: &str, key: Key, counters: &WriteCounters) -> Option<String> {
        let cache = self.cache.lock().unwrap();
        let pc = cache.get(pid).filter(|pc| pc.dev_id == counters.id())?;
        let e = pc.entries.get(&key)?;
        let fresh = e.stamp == counters.get(key.domain()) && (key != Key::Power || e.at.elapsed() < POWER_TTL);
        fresh.then(|| e.json.clone())
    }

    /// Store `json`, read while `key`'s counter was `stamp`, unless the device was replaced or
    /// the counter has moved on since.
    fn store(&self, pid: &str, key: Key, counters: &Arc<WriteCounters>, stamp: u64, json: &str) {
        if counters.get(key.domain()) != stamp {
            return;
        }
        match self.devices.lock().unwrap().get(pid) {
            Some(a) if Arc::ptr_eq(&a.counters, counters) => {}
            _ => return,
        }
        let mut cache = self.cache.lock().unwrap();
        let pc = cache.entry(pid.to_string()).or_default();
        if pc.dev_id != counters.id() {
            pc.entries.clear();
            pc.dev_id = counters.id();
        }
        pc.entries.insert(key, Entry { stamp, at: Instant::now(), json: json.to_string() });
    }

    /// Drop everything cached for a device (detach / attach).
    pub(crate) fn forget(&self, pid: &str) {
        self.cache.lock().unwrap().remove(pid);
    }

    /// Cached read of one value, or `produce` under the device lock.
    fn cached(&self, pid: &str, key: Key, produce: impl FnOnce(&Device, &Model) -> Result<String>) -> Result<String> {
        let counters = self.counters_of(pid)?;
        if let Some(v) = self.lookup(pid, key, &counters) {
            return Ok(v);
        }
        let (json, stamp) = self.with_device(pid, |dev, model| {
            let g0 = dev.counters().get(key.domain());
            Ok((produce(dev, model)?, g0))
        })?;
        self.store(pid, key, &counters, stamp, &json);
        Ok(json)
    }

    pub fn profiles_json(&self, pid: &str) -> Result<String> {
        self.cached(pid, Key::Profiles, |dev, _| Ok(serde_json::to_string(&tsuba_core::profiles::list(dev)?)?))
    }

    pub fn performance_json(&self, pid: &str, profile: u8) -> Result<String> {
        let profile = profile.max(1);
        self.cached(pid, Key::Perf(profile), |dev, model| Ok(serde_json::to_string(&tsuba_core::performance::read(dev, model, profile)?)?))
    }

    pub fn power_json(&self, pid: &str) -> Result<String> {
        self.cached(pid, Key::Power, |dev, _| Ok(serde_json::to_string(&tsuba_core::power::read(dev))?))
    }

    /// Every attached device's on-board macro memory, from cache where current.
    pub(crate) fn device_macros_json(&self, pid: &str) -> Result<String> {
        self.cached(pid, Key::Macros, |dev, _| Ok(serde_json::to_string(&tsuba_core::macros::read_device(dev)?)?))
    }

    /// One profile/layer's assignments as JSON. A miss reads the table in chunks, letting go of
    /// the device between them.
    pub fn assignments_json(&self, pid: &str, profile: u8, layer: u8) -> Result<String> {
        self.assignments_chunked(pid, profile, layer, CHUNK_FAST, None)
    }

    pub(crate) fn assignments_chunked(&self, pid: &str, profile: u8, layer: u8, chunk: usize, pause: Option<Duration>) -> Result<String> {
        let key = Key::Assign(profile, layer);
        let counters = self.counters_of(pid)?;
        if let Some(v) = self.lookup(pid, key, &counters) {
            return Ok(v);
        }
        let g0 = counters.get(Domain::Assign);
        let (get, keys) = self.with_device(pid, |dev, model| state::assignment_keys(dev, model))?;
        let mut all: Vec<AssignmentState> = Vec::new();
        for part in keys.chunks(chunk) {
            all.extend(self.with_device(pid, |dev, _| state::read_assignment_keys(dev, get, profile, layer, part))?);
            if let Some(p) = pause {
                std::thread::sleep(p);
            }
        }
        let json = serde_json::to_string(&all)?;
        // Stored only if no assignment write happened meanwhile (checked against g0). The live
        // profile (0) follows whichever stored profile is active, so it is never kept.
        if profile != 0 {
            self.store(pid, key, &counters, g0, &json);
        }
        Ok(json)
    }

    /// Write one assignment inside a device operation and keep the cached tables current
    /// instead of throwing them away: the written key is patched into its table.
    pub fn write_assignment(&self, pid: &str, dev: &Device, model: &Model, x: &decode::Assignment) -> Result<decode::Assignment> {
        let counters = dev.counters().clone();
        let g0 = counters.get(Domain::Assign);
        let back = tsuba_core::apply::set_assignment(dev, model, x)?;
        let g1 = counters.get(Domain::Assign);
        let patched = serde_json::to_value(state::assignment_state(back.clone())).ok();
        let mut cache = self.cache.lock().unwrap();
        if let Some(pc) = cache.get_mut(pid).filter(|pc| pc.dev_id == counters.id()) {
            let target = Key::Assign(x.profile, x.layer);
            // Nothing but this write happened since the entries were read: they stay current
            // (other tables are independent of this key), with the target table patched.
            for (k, e) in pc.entries.iter_mut().filter(|(k, _)| k.domain() == Domain::Assign) {
                if e.stamp != g0 {
                    continue;
                }
                if *k == target {
                    match patched.as_ref().and_then(|p| patch_table(&e.json, back.key, p)) {
                        Some(j) => e.json = j,
                        None => continue,
                    }
                }
                e.stamp = g1;
            }
        }
        Ok(back)
    }

    // ---- invalidation -------------------------------------------------------------------

    /// Forget everything cached for `pid` ("" = all devices) and read it again in the background.
    pub fn refresh_cache(self: &Arc<Self>, pid: &str) {
        let pids: Vec<String> = if pid.is_empty() { self.devices.lock().unwrap().keys().cloned().collect() } else { vec![pid.to_string()] };
        for p in pids {
            if let Ok(c) = self.counters_of(&p) {
                c.touch_all();
                // Whoever asked usually re-reads what it shows right away; fill in the rest after.
                let mgr = self.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    mgr.request_warm(&p);
                });
            }
        }
    }

    /// A hardware event arrived (runs off the async executor). Notifications mean the device
    /// changed something itself: settings and power are dropped; a profile change, hardware
    /// change or button change also drops the assignment tables and macro memory.
    pub fn on_event(self: &Arc<Self>, pid: &str, ev: &events::Event) {
        let Ok(c) = self.counters_of(pid) else { return };
        match ev {
            events::Event::Power(_) => c.touch(Domain::Power),
            events::Event::Notification { id, .. } => {
                c.touch(Domain::Settings);
                c.touch(Domain::Power);
                // The profile button may have switched profiles: the active library profile follows.
                self.library_changed(pid);
                if matches!(id, 1 | 13 | 70) {
                    c.touch(Domain::Assign);
                    c.touch(Domain::Macros);
                    self.request_warm(pid);
                }
            }
            _ => {}
        }
    }

    // ---- warm-up ------------------------------------------------------------------------

    /// Read a device's likely first-open data in the background (profile list, the active
    /// profile's two assignment tables and performance, power, macros). Never blocks the caller.
    pub fn request_warm(self: &Arc<Self>, pid: &str) {
        {
            let mut w = self.warming.lock().unwrap();
            if let Some(again) = w.get_mut(pid) {
                *again = true;
                return;
            }
            w.insert(pid.to_string(), false);
        }
        let (mgr, pid) = (self.clone(), pid.to_string());
        let spawned = std::thread::Builder::new().name(format!("warm-{pid}")).spawn({
            let pid = pid.clone();
            move || loop {
                let t = Instant::now();
                match mgr.warm_once(&pid) {
                    Ok(()) => debug!("{pid}: cache warmed in {:?}", t.elapsed()),
                    Err(e) => debug!("{pid}: cache warm-up stopped: {e:#}"),
                }
                let mut w = mgr.warming.lock().unwrap();
                if w.get(&pid) == Some(&true) {
                    w.insert(pid.clone(), false);
                } else {
                    w.remove(&pid);
                    break;
                }
            }
        });
        if spawned.is_err() {
            self.warming.lock().unwrap().remove(&pid);
        }
    }

    fn warm_once(&self, pid: &str) -> Result<()> {
        let pause = Some(Duration::from_millis(4));
        let active = serde_json::from_str::<serde_json::Value>(&self.profiles_json(pid)?)
            .ok()
            .and_then(|v| v["active"].as_u64())
            .unwrap_or(1)
            .clamp(1, 255) as u8;
        // Unsupported tables (e.g. a headset) just fail; the rest still warms.
        for layer in [0, 1] {
            if self.assignments_chunked(pid, active, layer, CHUNK_WARM, pause).is_err() {
                break;
            }
        }
        let _ = self.performance_json(pid, active);
        let _ = self.power_json(pid);
        let _ = self.device_macros_json(pid);
        info!("{pid}: device data cached");
        Ok(())
    }
}

/// Replace (or append) the entry for `key` in a JSON assignment table.
fn patch_table(json: &str, key: u8, new: &serde_json::Value) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(json).ok()?;
    let list = v.as_array_mut()?;
    match list.iter_mut().find(|a| a["key"] == key) {
        Some(slot) => *slot = new.clone(),
        None => list.push(new.clone()),
    }
    serde_json::to_string(&v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patching_a_table() {
        let t = r#"[{"key":1,"function":1,"function_name":"X","data":[],"label":"a"},{"key":2,"function":1,"function_name":"X","data":[],"label":"b"}]"#;
        let new = serde_json::json!({"key": 2, "function": 2, "function_name": "Y", "data": [1], "label": "c"});
        let out: serde_json::Value = serde_json::from_str(&patch_table(t, 2, &new).unwrap()).unwrap();
        assert_eq!(out[1]["label"], "c");
        assert_eq!(out[0]["label"], "a");
        let out: serde_json::Value = serde_json::from_str(&patch_table(t, 9, &new).unwrap()).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 3);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use tsuba_proto::{chroma, commands as c};

    /// A manager with one "attached" keyboard backed by /dev/null: reads here are produced by
    /// closures, writes only bump the counters (the ioctl then fails, which is fine).
    fn rig() -> (Arc<Manager>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("tsubad-cache-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mgr = Arc::new(Manager::new(false, dir.join("config.toml"), tx).unwrap());
        attach_fake(&mgr);
        (mgr, dir)
    }

    fn attach_fake(mgr: &Manager) {
        let mut dev = Device::open_path("/dev/null", 0x1f).unwrap();
        dev.allow_writes(true);
        let counters = dev.counters().clone();
        let a = crate::manager::Attached { model: tsuba_hid::model(0x028d).unwrap(), hidraw: "/dev/null".into(), dev: Arc::new(std::sync::Mutex::new(dev)), counters };
        mgr.devices.lock().unwrap().insert("028d".into(), a);
    }

    fn read(mgr: &Manager, key: Key, n: &AtomicUsize) -> String {
        mgr.cached("028d", key, |_, _| Ok(format!("v{}", n.fetch_add(1, Ordering::SeqCst) + 1))).unwrap()
    }

    fn write(mgr: &Manager, cmd: tsuba_proto::Command) {
        let _ = mgr.with_device("028d", |dev, _| Ok(dev.query(cmd, &[0; 4]).is_ok()));
    }

    #[test]
    fn cache_invalidation() {
        let (mgr, dir) = rig();
        let (assign, prof, mac, pow) = (Key::Assign(1, 0), Key::Profiles, Key::Macros, Key::Power);
        let n = AtomicUsize::new(0);
        let all = |n: &AtomicUsize| [assign, prof, mac, pow].map(|k| read(&mgr, k, n));
        let first = all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 4);
        assert_eq!(all(&n), first, "second pass is served from cache");
        assert_eq!(n.load(Ordering::SeqCst), 4);

        // Lighting writes don't touch any cached data.
        write(&mgr, chroma::SET_BRIGHTNESS);
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 4);

        // A key assignment write drops assignment tables only.
        write(&mgr, c::SET_SINGLE_KEY_ASSIGNMENT);
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 5);
        // A DPI write drops settings (and the assignment tables, conservatively).
        write(&mgr, c::set_dpi_stages(2));
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 7);
        // Macro and power writes only drop their own data.
        write(&mgr, tsuba_proto::macros::DELETE);
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 8);
        write(&mgr, c::SET_SLEEP_TIME);
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 9);

        // Hardware events: a DPI notification drops settings + power, not assignments or macros;
        // a profile change drops assignments and macros too.
        mgr.on_event("028d", &events::Event::Notification { id: 2, data: vec![] });
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 11);
        mgr.on_event("028d", &events::Event::Notification { id: 1, data: vec![] });
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 15);
        mgr.on_event("028d", &events::Event::Power(vec![1]));
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 16);

        // The refresh path drops everything.
        mgr.refresh_cache("028d");
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 20);

        // Detach / re-attach (a new Device) never serves the old attachment's data.
        mgr.devices.lock().unwrap().remove("028d");
        mgr.forget("028d");
        attach_fake(&mgr);
        all(&n);
        assert_eq!(n.load(Ordering::SeqCst), 24);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_reads_are_not_stored() {
        let (mgr, dir) = rig();
        // A write lands while the value is being read: the value is returned but not cached.
        let r = mgr.cached("028d", Key::Assign(1, 0), |dev, _| {
            let _ = dev.query(c::SET_SINGLE_KEY_ASSIGNMENT, &[0; 4]);
            Ok("racy".into())
        });
        assert_eq!(r.unwrap(), "racy");
        let n = AtomicUsize::new(0);
        assert_eq!(read(&mgr, Key::Assign(1, 0), &n), "v1");
        assert_eq!(read(&mgr, Key::Assign(1, 0), &n), "v1");
        let _ = std::fs::remove_dir_all(dir);
    }
}
