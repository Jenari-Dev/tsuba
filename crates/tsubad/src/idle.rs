//! Lighting power options: turn device lighting off when the user is idle, the display has
//! turned off (KDE's display-off timeout) or the session is locked, and back on with the next
//! input / unlock. Idle state comes from the compositor's ext-idle-notify-v1 protocol (no input
//! is read); the lock state from the session's `org.freedesktop.ScreenSaver` ActiveChanged signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::{info, warn};
use tsuba_proto::chroma;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

use crate::manager::Manager;

/// Notification threshold; longer timeouts are measured from here.
const BASE: Duration = Duration::from_secs(30);

struct State {
    mgr: Arc<Manager>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(st: &mut Self, _: &ExtIdleNotificationV1, ev: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            ext_idle_notification_v1::Event::Idled => {
                info!("idle (no input for {}s)", BASE.as_secs());
                *st.mgr.idle_since.lock().unwrap() = Some(Instant::now() - BASE);
            }
            ext_idle_notification_v1::Event::Resumed => {
                info!("input again");
                *st.mgr.idle_since.lock().unwrap() = None;
                let m = st.mgr.clone();
                std::thread::spawn(move || m.lighting_wake());
            }
            _ => {}
        }
    }
}

fn watch(mgr: Arc<Manager>) -> Result<()> {
    let conn = Connection::connect_to_env().context("no Wayland session")?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ())?;
    let notifier: ExtIdleNotifierV1 = globals.bind(&qh, 1..=2, ()).context("compositor lacks ext-idle-notify-v1")?;
    // v2's input-only notification ignores idle inhibitors (media playback keeps the screen
    // awake but shouldn't keep the keyboard lit); v1 falls back to the inhibitor-aware one.
    let v = wayland_client::Proxy::version(&notifier);
    let _n = if v >= 2 {
        notifier.get_input_idle_notification(BASE.as_millis() as u32, &seat, &qh, ())
    } else {
        notifier.get_idle_notification(BASE.as_millis() as u32, &seat, &qh, ())
    };
    info!("idle watcher running (ext-idle-notify v{v}{})", if v >= 2 { ", input-only" } else { ", respects inhibitors" });
    let mut st = State { mgr };
    loop {
        queue.blocking_dispatch(&mut st)?;
    }
}

/// The session is locked (screen locker active).
static LOCKED: AtomicBool = AtomicBool::new(false);

/// Follow the screen locker: `ActiveChanged(true)` when it locks, `(false)` on unlock. KDE
/// sends it on two object paths; repeats are ignored.
fn watch_lock(mgr: Arc<Manager>) -> Result<()> {
    let conn = zbus::blocking::Connection::session().context("no session bus")?;
    let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface("org.freedesktop.ScreenSaver")?.member("ActiveChanged")?.build();
    let signals = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(16))?;
    info!("lock watcher running");
    for msg in signals {
        let Ok(active) = msg?.body().deserialize::<bool>() else { continue };
        if LOCKED.swap(active, Ordering::SeqCst) != active {
            mgr.on_lock_changed(active);
        }
    }
    Ok(())
}

pub fn spawn(mgr: Arc<Manager>) {
    mgr.restore_dimmed_from_disk();
    crate::dbus_device::spawn_battery_watch(mgr.clone());
    let m = mgr.clone();
    std::thread::Builder::new()
        .name("lock".into())
        .spawn(move || {
            if let Err(e) = watch_lock(m) {
                warn!("lights off when locked unavailable: {e:#}");
            }
        })
        .ok();
    let m = mgr.clone();
    std::thread::Builder::new()
        .name("idle".into())
        .spawn(move || {
            if let Err(e) = watch(m) {
                warn!("lighting power options unavailable: {e:#}");
            }
        })
        .ok();
    std::thread::Builder::new()
        .name("idle-tick".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            mgr.lighting_tick();
        })
        .ok();
}

/// Brightness saved while lighting is off for idleness — on disk, so a daemon restart (update,
/// crash, logout) still restores it.
fn dimmed_path() -> std::path::PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("tsubad-test-idle-dimmed-{}.json", std::process::id()));
    }
    dirs::config_dir().unwrap_or_default().join("tsuba").join("idle-dimmed.json")
}

impl Manager {
    fn save_dimmed(&self) {
        let d = self.dimmed.lock().unwrap().clone();
        let p = dimmed_path();
        if d.is_empty() {
            let _ = std::fs::remove_file(p);
        } else if let Ok(j) = serde_json::to_string(&d) {
            let _ = std::fs::write(p, j);
        }
    }

    /// At startup: restore lighting a previous run switched off for idleness.
    pub fn restore_dimmed_from_disk(&self) {
        let Ok(j) = std::fs::read_to_string(dimmed_path()) else { return };
        if let Ok(d) = serde_json::from_str::<std::collections::HashMap<String, Vec<(u8, u8)>>>(&j) {
            self.dimmed.lock().unwrap().extend(d);
            self.lighting_wake();
        }
    }

    /// Switch off the lighting of every device whose idle / display-off threshold has passed.
    pub fn lighting_tick(&self) {
        let Some(since) = *self.idle_since.lock().unwrap() else { return };
        let idle = since.elapsed();
        let cfg = self.config.lock().unwrap().clone();
        for dc in &cfg.devices {
            let Some(p) = &dc.lighting_power else { continue };
            let mut limits = Vec::new();
            if p.idle_minutes > 0 {
                limits.push(p.idle_minutes);
            }
            if p.display_off {
                limits.push(p.display_off_minutes.max(1));
            }
            let Some(min) = limits.into_iter().min() else { continue };
            if idle < Duration::from_secs(min as u64 * 60) || self.dimmed.lock().unwrap().contains_key(&dc.pid) {
                continue;
            }
            self.switch_off(&dc.pid, &format!("idle {} min", idle.as_secs() / 60));
        }
    }

    /// Save every region's brightness and set it to 0 (undone by [`Self::lighting_wake`]).
    fn switch_off(&self, pid: &str, why: &str) {
        let saved = self.with_device(pid, |dev, _| {
            let mut saved = Vec::new();
            for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                saved.push((r.id, dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0)));
                dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, 0))?;
            }
            Ok(saved)
        });
        match saved {
            Ok(s) => {
                info!("{pid}: lighting off ({why})");
                self.dimmed.lock().unwrap().insert(pid.to_string(), s);
                self.save_dimmed();
            }
            Err(e) => warn!("{pid}: lighting off ({why}): {e:#}"),
        }
    }

    /// The session was locked (true) or unlocked: devices with "off when locked" follow.
    pub fn on_lock_changed(&self, locked: bool) {
        LOCKED.store(locked, Ordering::SeqCst);
        if !locked {
            info!("session unlocked");
            self.lighting_wake();
            return;
        }
        info!("session locked");
        let cfg = self.config.lock().unwrap().clone();
        for dc in cfg.devices.iter().filter(|d| d.lighting_power.as_ref().is_some_and(|p| p.on_lock)) {
            if self.devices.lock().unwrap().contains_key(&dc.pid) && !self.dimmed.lock().unwrap().contains_key(&dc.pid) {
                self.switch_off(&dc.pid, "session locked");
            }
        }
    }

    /// Restore lighting switched off for idleness.
    /// While the session is locked, devices set to stay off when locked are left off.
    pub fn lighting_wake(&self) {
        let keep_off: Vec<String> = if LOCKED.load(Ordering::SeqCst) {
            self.config.lock().unwrap().devices.iter().filter(|d| d.lighting_power.as_ref().is_some_and(|p| p.on_lock)).map(|d| d.pid.clone()).collect()
        } else {
            Vec::new()
        };
        let dimmed: Vec<_> = {
            let mut d = self.dimmed.lock().unwrap();
            let all: Vec<_> = d.drain().collect();
            let (stay, go): (Vec<_>, Vec<_>) = all.into_iter().partition(|(pid, _)| keep_off.contains(pid));
            d.extend(stay);
            go
        };
        self.save_dimmed();
        for (pid, saved) in dimmed {
            let r = self.with_device(&pid, |dev, _| {
                for (region, level) in &saved {
                    dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, *region, *level))?;
                }
                Ok(())
            });
            match r {
                Ok(()) => info!("{pid}: lighting back on"),
                Err(e) => warn!("{pid}: restoring lighting: {e:#}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tsuba_core::sim;

    #[test]
    fn lock_switches_off_and_unlock_restores() {
        let dir = std::env::temp_dir().join(format!("tsubad-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mgr = Arc::new(Manager::new(false, dir.join("config.toml"), tx).unwrap());
        let mouse = Arc::new(Mutex::new(sim::new_state("00aa", 1)));
        let dev = sim::device(&mouse);
        let counters = dev.counters().clone();
        mgr.devices.lock().unwrap().insert("00aa".into(), crate::manager::Attached { model: tsuba_hid::model(0x00aa).unwrap(), hidraw: "/dev/null".into(), dev: Arc::new(Mutex::new(dev)), counters });
        mgr.set_lighting_power("00aa", tsuba_core::config::LightingPower { on_lock: true, display_off_minutes: 10, ..Default::default() }).unwrap();
        let level = |id: u8| mouse.lock().unwrap().feat.lighting.get(&id).map_or(255, |l| l.1);
        mgr.on_lock_changed(true);
        assert_eq!((level(1), level(4), level(10)), (0, 0, 0));
        // Input on the lock screen doesn't bring the lighting back.
        mgr.lighting_wake();
        assert_eq!(level(4), 0);
        mgr.on_lock_changed(false);
        assert_eq!((level(1), level(4), level(10)), (255, 255, 255));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(dimmed_path());
    }
}
