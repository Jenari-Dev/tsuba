//! Lighting power options: turn device lighting off when the user is idle or the display has
//! turned off (KDE's display-off timeout), and back on with the next input. Idle state comes
//! from the compositor's ext-idle-notify-v1 protocol (no input is read).

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

pub fn spawn(mgr: Arc<Manager>) {
    mgr.restore_dimmed_from_disk();
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
            let saved = self.with_device(&dc.pid, |dev, _| {
                let mut saved = Vec::new();
                for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                    saved.push((r.id, dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0)));
                    dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, 0))?;
                }
                Ok(saved)
            });
            match saved {
                Ok(s) => {
                    info!("{}: lighting off (idle {} min)", dc.pid, idle.as_secs() / 60);
                    self.dimmed.lock().unwrap().insert(dc.pid.clone(), s);
                    self.save_dimmed();
                }
                Err(e) => warn!("{}: idle lighting off: {e:#}", dc.pid),
            }
        }
    }

    /// Restore lighting switched off for idleness.
    pub fn lighting_wake(&self) {
        let dimmed: Vec<_> = self.dimmed.lock().unwrap().drain().collect();
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
