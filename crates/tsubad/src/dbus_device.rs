//! D-Bus interface `io.github.Jenari_Dev.Tsuba1.Device` (served at [`crate::dbus::PATH`]):
//! firmware features found by the read-only feature probe, on-board profile / factory reset,
//! mouse lighting zones, lighting on every device at once, and lighting on battery.
//!
//! Writes go through the usual gates ([`Manager::ensure_writable_for`]: read-only daemon,
//! untested models) and the device's write counters, so cached reads stay exact.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, ensure, Context, Result};
use serde_json::json;
use tracing::{info, warn};
use tsuba_core::config::{EffectConfig, Lighting};
use tsuba_core::features::{self, Change, ProbeReport};
use tsuba_core::{apply, percent_to_level};
use tsuba_proto::{chroma, commands as c};
use zbus::{fdo, interface};

use crate::manager::Manager;

pub struct DeviceIface {
    pub mgr: Arc<Manager>,
}

/// Probe reports per device, for the attachment they were read from ([`tsuba_hid::WriteCounters::id`]).
static PROBES: Mutex<Option<HashMap<String, (u64, ProbeReport)>>> = Mutex::new(None);

/// Typed confirmation the factory reset needs: the device's name, exactly.
pub fn factory_reset_phrase(model: &tsuba_hid::Model) -> String {
    model.name.to_string()
}

/// Typed confirmation a profile reset needs.
pub fn profile_reset_phrase(slot: u8) -> String {
    format!("reset profile {slot}")
}

impl Manager {
    pub(crate) fn model_for(&self, pid: &str) -> Result<tsuba_hid::Model> {
        self.devices.lock().unwrap().get(pid).map(|a| a.model).ok_or_else(|| anyhow!("no device {pid}"))
    }

    /// The device's probe report (probed once per attachment; `fresh` probes again). Read-only.
    /// Each probe is also saved where `tsuba probe-features` saves it.
    pub fn probe_report(&self, pid: &str, fresh: bool) -> Result<ProbeReport> {
        let id = self.counters_of(pid)?.id();
        if !fresh {
            if let Some((at, r)) = PROBES.lock().unwrap().get_or_insert_with(HashMap::new).get(pid) {
                if *at == id {
                    return Ok(r.clone());
                }
            }
        }
        let report = self.with_device(pid, |dev, model| Ok(features::probe(dev, model)))?;
        info!("{pid}: feature probe: {}", report.supported_keys().join(", "));
        let path = ProbeReport::default_path(pid);
        if cfg!(not(test)) {
            if let Err(e) = report.save(&path) {
                warn!("{pid}: saving {}: {e:#}", path.display());
            }
        }
        PROBES.lock().unwrap().get_or_insert_with(HashMap::new).insert(pid.to_string(), (id, report.clone()));
        Ok(report)
    }

    /// Feature values for one on-board profile as JSON ([`features::Features`] plus the saved
    /// `smart_tracking`, `zones`, `battery_lighting` and the reset confirmation phrases).
    pub fn features_json(&self, pid: &str, profile: u8) -> Result<String> {
        let report = self.probe_report(pid, false)?;
        let f = self.with_device(pid, |dev, model| Ok(features::read(dev, model, &report, profile)))?;
        let model = self.model_for(pid)?;
        let dc = self.config.lock().unwrap().device(pid).cloned().unwrap_or_default();
        let mut v = serde_json::to_value(&f)?;
        let o = v.as_object_mut().expect("features serialise to an object");
        o.insert("smart_tracking".into(), serde_json::to_value(dc.smart_tracking.unwrap_or_default())?);
        o.insert("smart_tracking_saved".into(), dc.smart_tracking.is_some().into());
        o.insert("zones".into(), serde_json::to_value(&dc.zones)?);
        o.insert("battery_lighting".into(), serde_json::to_value(&dc.battery_lighting)?);
        o.insert("battery".into(), (report.supported("battery") && model.spec.features.battery).into());
        o.insert("wireless".into(), model.spec.features.wireless.into());
        o.insert("factory_reset_phrase".into(), factory_reset_phrase(&model).into());
        o.insert("profile_reset_phrase".into(), profile_reset_phrase(profile).into());
        o.insert("tested".into(), model.tested().into());
        Ok(v.to_string())
    }

    /// Apply a [`Change`] (only features the probe found). The lift-off distance has no
    /// read-back, so it is kept in the config.
    pub fn set_features(&self, pid: &str, want: &Change) -> Result<Vec<String>> {
        self.ensure_writable_for(pid)?;
        let report = self.probe_report(pid, false)?;
        let log = self.with_device(pid, |dev, model| features::apply(dev, model, &report, want))?;
        if let Some(st) = want.smart_tracking {
            self.edit_device_config(pid, |dc| dc.smart_tracking = Some(st))?;
        }
        Ok(log)
    }

    /// Reset on-board profile `slot` to factory defaults. `confirm` must be
    /// [`profile_reset_phrase`].
    pub fn reset_profile(self: &Arc<Self>, pid: &str, slot: u8, confirm: &str) -> Result<String> {
        self.ensure_writable_for(pid)?;
        ensure!(confirm == profile_reset_phrase(slot), "type \"{}\" to confirm the reset", profile_reset_phrase(slot));
        let msg = {
            let _held = self.lib_lock.lock().unwrap();
            // Game Mode's keys go back first and are turned off again in the reset slot, so
            // its saved bindings never describe a slot that no longer has them.
            let msg = self.with_game_mode_keys_given_back(pid, || self.with_device(pid, |dev, model| features::reset_profile(dev, model, slot)))?;
            // The slot no longer holds the library profile that was in it.
            self.forget_library_state(pid, |st| {
                st.holds.remove(&slot);
            });
            msg
        };
        warn!("{pid}: {msg}");
        self.refresh_cache(pid);
        self.library_changed(pid);
        Ok(msg)
    }

    /// Factory reset: erases the device's on-board profiles and macros. `confirm` must be the
    /// device's name ([`factory_reset_phrase`]).
    pub fn factory_reset(self: &Arc<Self>, pid: &str, confirm: &str) -> Result<String> {
        self.ensure_writable_for(pid)?;
        let model = self.model_for(pid)?;
        ensure!(confirm == factory_reset_phrase(&model), "type the device name \"{}\" to confirm the factory reset", factory_reset_phrase(&model));
        self.stop_studio(pid);
        let msg = {
            let _held = self.lib_lock.lock().unwrap();
            let msg = self.with_game_mode_keys_given_back(pid, || self.with_device(pid, |dev, model| features::factory_reset(dev, model)))?;
            // Every on-board profile and macro is gone: the library learns the device again.
            self.forget_library_state(pid, |st| {
                st.holds.clear();
                st.seen.clear();
                st.uploaded_macros.clear();
            });
            msg
        };
        warn!("{pid}: {msg}");
        self.refresh_cache(pid);
        self.library_changed(pid);
        Ok(msg)
    }

    /// Change the library's bookkeeping about the device (call with `lib_lock` held).
    fn forget_library_state(&self, pid: &str, f: impl FnOnce(&mut tsuba_core::library::LibraryState)) {
        let r = tsuba_core::library::DeviceLibrary::load(&tsuba_core::library::root(), pid).and_then(|mut l| {
            f(&mut l.state);
            l.save_state()
        });
        if let Err(e) = r {
            warn!("{pid}: updating the profile library after the reset: {e:#}");
        }
    }

    pub(crate) fn edit_device_config(&self, pid: &str, f: impl FnOnce(&mut tsuba_core::DeviceConfig)) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        f(&mut dc);
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Give one lighting zone (region) its own hardware effect, live and saved.
    pub fn set_zone(&self, pid: &str, region: u8, l: Lighting) -> Result<Vec<String>> {
        self.ensure_writable_for(pid)?;
        ensure!(!matches!(l.effect, EffectConfig::Studio { .. } | EffectConfig::Custom { .. }), "zones take hardware effects only");
        let log = if self.game_holds(pid) {
            vec!["a game is controlling the lighting; the zone's effect applies when it stops".into()]
        } else {
            self.stop_studio(pid);
            self.with_device(pid, |dev, _| {
                ensure!(chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?).iter().any(|r| r.id == region), "no lighting region {region}");
                apply::apply_region(dev, region, &l)
            })?
        };
        self.edit_device_config(pid, |dc| {
            dc.zones.insert(region.to_string(), l);
        })?;
        // A game took the device meanwhile (or before): the saved lighting, zones included,
        // comes back when it lets go — not whatever it held before.
        if self.game_holds(pid) && !self.game_defer(pid, crate::manager::GameRestore::Saved) {
            self.restore_saved_lighting(pid);
        }
        Ok(log)
    }

    /// One zone's brightness, live and saved (its effect stays).
    pub fn set_zone_brightness(&self, pid: &str, region: u8, percent: u8) -> Result<()> {
        self.ensure_writable_for(pid)?;
        self.with_device(pid, |dev, _| apply::set_region_brightness(dev, region, percent))?;
        let base = self.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone());
        self.edit_device_config(pid, |dc| {
            let key = region.to_string();
            match dc.zones.get_mut(&key) {
                Some(z) => z.brightness = Some(percent),
                None => {
                    if let Some(b) = base.filter(|b| !matches!(b.effect, EffectConfig::Studio { .. } | EffectConfig::Custom { .. })) {
                        dc.zones.insert(key, Lighting { effect: b.effect, brightness: Some(percent) });
                    }
                }
            }
        })
    }

    /// Forget per-zone lighting (the device's lighting applies to every zone again).
    pub fn clear_zones(&self, pid: &str) -> Result<()> {
        self.edit_device_config(pid, |dc| dc.zones.clear())
    }

    /// Apply a hardware effect to every attached device that may be written, and save it as
    /// each one's lighting. Regions without the effect are skipped (logged).
    pub fn set_effect_all(&self, effect: EffectConfig, brightness: Option<u8>) -> Result<Vec<String>> {
        self.ensure_writable()?;
        ensure!(!matches!(effect, EffectConfig::Studio { .. } | EffectConfig::Custom { .. }), "only built-in effects can be sent to every device");
        let l = Lighting { effect, brightness };
        let mut log = Vec::new();
        let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
        for pid in pids {
            if let Err(e) = self.ensure_writable_for(&pid) {
                log.push(format!("{pid}: skipped ({e:#})"));
                continue;
            }
            if self.game_defer(&pid, crate::manager::GameRestore::Lighting(l.clone())) {
                log.push(format!("{pid}: a game is controlling the lighting; applies when it stops"));
            } else {
                self.stop_studio(&pid);
                match self.with_device(&pid, |dev, _| apply::apply_lighting(dev, &l)) {
                    Ok(lines) => log.extend(lines.into_iter().map(|x| format!("{pid}: {x}"))),
                    Err(e) => {
                        log.push(format!("{pid}: failed ({e:#})"));
                        continue;
                    }
                }
            }
            self.edit_device_config(&pid, |dc| {
                dc.lighting = Some(l.clone());
                dc.zones.clear();
            })?;
            self.mirror_lighting(&pid, &l);
        }
        Ok(log)
    }

    /// Brightness (percent) on every region of every attached device that may be written; saved.
    pub fn set_brightness_all(&self, percent: u8) -> Result<Vec<String>> {
        self.ensure_writable()?;
        ensure!(percent <= 100, "brightness is a percentage");
        let mut log = Vec::new();
        let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
        for pid in pids {
            if self.ensure_writable_for(&pid).is_err() {
                log.push(format!("{pid}: skipped (writes off)"));
                continue;
            }
            let r = self.with_device(&pid, |dev, _| {
                for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                    dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, percent_to_level(percent)))?;
                }
                Ok(())
            });
            match r {
                Ok(()) => {
                    log.push(format!("{pid}: brightness {percent}%"));
                    self.edit_device_config(&pid, |dc| {
                        if let Some(l) = dc.lighting.as_mut() {
                            l.brightness = Some(percent);
                        }
                        dc.zones.values_mut().for_each(|z| z.brightness = Some(percent));
                    })?;
                    if let Some(l) = self.config.lock().unwrap().device(&pid).and_then(|d| d.lighting.clone()) {
                        self.mirror_lighting(&pid, &l);
                    }
                }
                Err(e) => log.push(format!("{pid}: failed ({e:#})")),
            }
        }
        Ok(log)
    }

    /// Lighting to use while the device runs on battery (None = same as plugged in).
    pub fn set_battery_lighting(&self, pid: &str, l: Option<Lighting>) -> Result<()> {
        if let Some(l) = &l {
            ensure!(!matches!(l.effect, EffectConfig::Studio { .. } | EffectConfig::Custom { .. }), "lighting on battery takes a built-in effect");
        }
        self.edit_device_config(pid, |dc| dc.battery_lighting = l)?;
        // Re-evaluate at the next battery check.
        ON_BATTERY.lock().unwrap().get_or_insert_with(HashMap::new).remove(pid);
        Ok(())
    }

    /// One battery check: devices with a battery lighting switch to it when they leave the
    /// charger and back when they're plugged in again.
    pub fn battery_lighting_tick(&self) {
        let cfg = self.config.lock().unwrap().clone();
        for dc in cfg.devices.iter().filter(|d| d.battery_lighting.is_some()) {
            if self.ensure_writable_for(&dc.pid).is_err() {
                continue;
            }
            let charging = self.with_device(&dc.pid, |dev, _| Ok(dev.query(c::GET_CHARGING_STATUS, &[])?.get(1) == Some(&1)));
            let Ok(charging) = charging else { continue };
            let on_battery = !charging;
            let prev = ON_BATTERY.lock().unwrap().get_or_insert_with(HashMap::new).insert(dc.pid.clone(), on_battery);
            if prev == Some(on_battery) || (prev.is_none() && !on_battery) {
                continue;
            }
            if on_battery {
                let l = dc.battery_lighting.clone().expect("filtered");
                if self.game_defer(&dc.pid, crate::manager::GameRestore::Lighting(l.clone())) {
                    continue;
                }
                self.stop_studio(&dc.pid);
                match self.with_device(&dc.pid, |dev, _| apply::apply_lighting(dev, &l)) {
                    Ok(_) => info!("{}: on battery — battery lighting", dc.pid),
                    Err(e) => warn!("{}: battery lighting: {e:#}", dc.pid),
                }
            } else {
                info!("{}: charging — saved lighting", dc.pid);
                self.restore_saved_lighting(&dc.pid);
            }
        }
    }
}

/// Last seen battery state per device (true = on battery).
static ON_BATTERY: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// Check battery state every 20 s (only devices with a battery lighting are asked).
pub fn spawn_battery_watch(mgr: Arc<Manager>) {
    std::thread::Builder::new()
        .name("battery-lighting".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(20));
            mgr.battery_lighting_tick();
        })
        .ok();
}

fn err(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{e:#}"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> fdo::Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| fdo::Error::Failed(e.to_string()))?.map_err(err)
}

fn parse<T: serde::de::DeserializeOwned>(s: &str) -> fdo::Result<T> {
    serde_json::from_str(s).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))
}

fn logged<T>(what: String, r: fdo::Result<T>) -> fdo::Result<T> {
    match &r {
        Ok(_) => info!("{what}: ok"),
        Err(e) => warn!("{what}: {e}"),
    }
    r
}

#[interface(name = "io.github.Jenari_Dev.Tsuba1.Device")]
impl DeviceIface {
    /// Firmware feature values for one on-board profile as JSON: `{"profile", "supported":
    /// [probe keys], "debounce", "scroll_accel_level", "smart_reel_threshold",
    /// "profile_colour", "indicator_led", "indicator_leds", "startup_animation",
    /// "smart_illumination", "time_to_dim", "lift_sensor", "connection", "dongle_quality",
    /// "can_reset_profile", "model", "support_url", "smart_tracking", "zones",
    /// "battery_lighting", "factory_reset_phrase", "profile_reset_phrase"}` (what the device has).
    async fn get_features(&self, pid: String, profile: u8) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.features_json(&pid, profile.max(1))).await
    }

    /// Run the read-only feature probe again; returns the report JSON (also saved to
    /// `$XDG_DATA_HOME/tsuba/features/<pid>.json`).
    async fn probe_features(&self, pid: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || Ok(serde_json::to_string(&mgr.probe_report(&pid, true)?)?)).await
    }

    /// Apply feature settings (same JSON names as GetFeatures, plus "profile"). Returns what changed.
    async fn set_features(&self, pid: String, settings: String) -> fdo::Result<Vec<String>> {
        let want: Change = parse(&settings)?;
        let mgr = self.mgr.clone();
        logged(format!("{pid}: features"), blocking(move || mgr.set_features(&pid, &want)).await)
    }

    /// Reset on-board profile `slot` to factory defaults; `confirm` must be "reset profile <slot>".
    async fn reset_profile(&self, pid: String, slot: u8, confirm: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(format!("{pid}: reset profile {slot}"), blocking(move || mgr.reset_profile(&pid, slot, &confirm)).await)
    }

    /// Factory reset (erases on-board profiles and macros); `confirm` must be the device name.
    async fn factory_reset(&self, pid: String, confirm: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(format!("{pid}: factory reset"), blocking(move || mgr.factory_reset(&pid, &confirm)).await)
    }

    /// Give one zone (region id) its own effect (config JSON form) and brightness (255 = keep).
    async fn set_zone_effect(&self, pid: String, region: u8, effect: String, brightness: u8) -> fdo::Result<Vec<String>> {
        let effect: EffectConfig = parse(&effect)?;
        let mgr = self.mgr.clone();
        let brightness = (brightness <= 100).then_some(brightness);
        logged(format!("{pid}: zone {region}"), blocking(move || mgr.set_zone(&pid, region, Lighting { effect, brightness })).await)
    }

    async fn set_zone_brightness(&self, pid: String, region: u8, percent: u8) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.set_zone_brightness(&pid, region, percent)).await
    }

    async fn clear_zones(&self, pid: String) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.clear_zones(&pid)).await
    }

    /// Apply a built-in effect (config JSON form) to every attached device; brightness 255 = keep.
    async fn set_effect_all(&self, effect: String, brightness: u8) -> fdo::Result<Vec<String>> {
        let effect: EffectConfig = parse(&effect)?;
        let mgr = self.mgr.clone();
        let brightness = (brightness <= 100).then_some(brightness);
        logged("all devices: effect".into(), blocking(move || mgr.set_effect_all(effect, brightness)).await)
    }

    async fn set_brightness_all(&self, percent: u8) -> fdo::Result<Vec<String>> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.set_brightness_all(percent)).await
    }

    /// Lighting on battery as JSON (`null` = same as plugged in).
    async fn get_battery_lighting(&self, pid: String) -> String {
        let l = self.mgr.config.lock().unwrap().device(&pid).and_then(|d| d.battery_lighting.clone());
        json!(l).to_string()
    }

    /// Set (JSON `{"effect", "brightness"}`) or clear (`null`) the lighting on battery.
    async fn set_battery_lighting(&self, pid: String, lighting: String) -> fdo::Result<()> {
        let l: Option<Lighting> = parse(&lighting)?;
        let mgr = self.mgr.clone();
        blocking(move || mgr.set_battery_lighting(&pid, l)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsuba_core::sim;

    fn rig(read_only: bool) -> (Arc<Manager>, std::path::PathBuf, sim::Sim, sim::Sim) {
        let dir = std::env::temp_dir().join(format!("tsubad-device-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mgr = Arc::new(Manager::new(read_only, dir.join("config.toml"), tx).unwrap());
        let kb = Arc::new(Mutex::new(sim::new_state("028d", 1)));
        let mouse = Arc::new(Mutex::new(sim::new_state("00aa", 3)));
        for (pid, s) in [("028d", &kb), ("00aa", &mouse)] {
            let dev = sim::device(s);
            let counters = dev.counters().clone();
            let model = tsuba_hid::model(u16::from_str_radix(pid, 16).unwrap()).unwrap();
            mgr.devices.lock().unwrap().insert(pid.into(), crate::manager::Attached { model, hidraw: "/dev/null".into(), dev: Arc::new(Mutex::new(dev)), counters });
        }
        (mgr, dir, kb, mouse)
    }

    fn writes(s: &sim::Sim) -> usize {
        s.lock().unwrap().writes.len()
    }

    #[test]
    fn features_json_and_gated_writes() {
        let (mgr, dir, kb, mouse) = rig(false);
        let v: serde_json::Value = serde_json::from_str(&mgr.features_json("028d", 1).unwrap()).unwrap();
        assert_eq!(v["debounce"]["make_mask"], 20);
        assert_eq!(v["lift_sensor"], false);
        assert_eq!(v["factory_reset_phrase"], "BlackWidow V4 Pro");
        assert_eq!(writes(&kb), 0, "reading features writes nothing");
        let log = mgr.set_features("028d", &serde_json::from_str(r#"{"debounce":{"make":0,"make_mask":0,"break":0,"break_mask":0}}"#).unwrap()).unwrap();
        assert!(log[0].contains("debounce"), "{log:?}");
        // The mouse: lift-off goes to the device and into the config.
        let st = features::SmartTracking { distance_mm: 3, ..Default::default() };
        mgr.set_features("00aa", &Change { smart_tracking: Some(st), ..Default::default() }).unwrap();
        assert_eq!(mouse.lock().unwrap().feat.lift, Some((1, 2)));
        assert_eq!(mgr.config.lock().unwrap().device("00aa").unwrap().smart_tracking, Some(st));
        let v: serde_json::Value = serde_json::from_str(&mgr.features_json("00aa", 1).unwrap()).unwrap();
        assert_eq!((v["smart_tracking"]["distance_mm"].as_u64(), v["smart_tracking_saved"].as_bool()), (Some(3), Some(true)));
        // Unsupported features never reach the device.
        let n = writes(&kb);
        assert!(mgr.set_features("028d", &Change { profile_colour: Some("ff0000".into()), ..Default::default() }).is_err());
        assert_eq!(writes(&kb), n);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_only_daemon_reads_but_never_writes() {
        let (mgr, dir, kb, mouse) = rig(true);
        assert!(mgr.features_json("00aa", 1).is_ok());
        assert!(mgr.set_features("028d", &Change { debounce: Some(features::Debounce::FAST), ..Default::default() }).is_err());
        assert!(mgr.reset_profile("00aa", 2, "reset profile 2").is_err());
        assert!(mgr.factory_reset("00aa", "Basilisk V3 Pro (wired)").is_err());
        assert!(mgr.set_zone("00aa", 4, Lighting { effect: EffectConfig::Off, brightness: None }).is_err());
        assert!(mgr.set_effect_all(EffectConfig::Spectrum, Some(50)).is_err());
        assert!(mgr.set_brightness_all(50).is_err());
        assert_eq!((writes(&kb), writes(&mouse)), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resets_need_the_typed_phrase() {
        let (mgr, dir, _kb, mouse) = rig(false);
        assert!(mgr.reset_profile("00aa", 2, "yes").is_err());
        assert!(mouse.lock().unwrap().feat.resets.is_empty());
        mgr.reset_profile("00aa", 2, "reset profile 2").unwrap();
        assert_eq!(mouse.lock().unwrap().feat.resets, vec![2]);
        assert!(mgr.factory_reset("00aa", "basilisk").is_err());
        assert_eq!(mouse.lock().unwrap().profiles.len(), 3);
        mgr.factory_reset("00aa", "Basilisk V3 Pro (wired)").unwrap();
        assert_eq!(mouse.lock().unwrap().profiles.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zones_all_devices_and_battery_lighting() {
        let (mgr, dir, kb, mouse) = rig(false);
        mgr.set_zone("00aa", 4, Lighting { effect: EffectConfig::Static { colour: "0000ff".into() }, brightness: Some(30) }).unwrap();
        assert_eq!(mouse.lock().unwrap().feat.lighting[&4].0, vec![1, 4, 1, 0, 0, 1, 0, 0, 0xff]);
        assert!(mgr.set_zone("00aa", 7, Lighting { effect: EffectConfig::Off, brightness: None }).is_err(), "no region 7");
        mgr.set_zone_brightness("00aa", 10, 20).unwrap();
        let zones = mgr.config.lock().unwrap().device("00aa").unwrap().zones.clone();
        assert_eq!(zones.keys().collect::<Vec<_>>(), ["4"], "no saved base lighting: underglow brightness is live only");
        // Every device at once; zones are cleared because the whole device got the effect.
        let log = mgr.set_effect_all(EffectConfig::Static { colour: "ff0000".into() }, Some(80)).unwrap();
        assert!(log.iter().any(|l| l.starts_with("028d")) && log.iter().any(|l| l.starts_with("00aa")), "{log:?}");
        assert_eq!(kb.lock().unwrap().feat.lighting[&5].0[2], 1);
        assert!(mgr.config.lock().unwrap().device("00aa").unwrap().zones.is_empty());
        assert!(mgr.set_effect_all(EffectConfig::Custom { rows: vec![] }, None).is_err());
        mgr.set_brightness_all(40).unwrap();
        assert_eq!(mouse.lock().unwrap().feat.lighting[&10].1, percent_to_level(40));
        assert_eq!(mgr.config.lock().unwrap().device("028d").unwrap().lighting.as_ref().unwrap().brightness, Some(40));
        // Battery lighting: no charging status → no switch at all.
        mgr.set_battery_lighting("00aa", Some(Lighting { effect: EffectConfig::Off, brightness: None })).unwrap();
        let n = writes(&mouse);
        mgr.battery_lighting_tick();
        assert_eq!(writes(&mouse), n, "no charging status, no switch");
        // Plugged in at start: nothing to do. Unplugged: battery lighting. Plugged in: back.
        mouse.lock().unwrap().feat.charging = Some(1);
        mgr.battery_lighting_tick();
        assert_eq!(writes(&mouse), n);
        mouse.lock().unwrap().feat.charging = Some(0);
        mgr.battery_lighting_tick();
        assert_eq!(mouse.lock().unwrap().feat.lighting[&4].0[2], 0, "off on battery");
        mgr.battery_lighting_tick();
        let m = writes(&mouse);
        mgr.battery_lighting_tick();
        assert_eq!(writes(&mouse), m, "no repeat while the state holds");
        mouse.lock().unwrap().feat.charging = Some(1);
        mgr.battery_lighting_tick();
        assert_eq!(mouse.lock().unwrap().feat.lighting[&4].0, vec![1, 4, 1, 0, 0, 1, 0xff, 0, 0], "saved static red is back");
        assert!(mgr.set_battery_lighting("00aa", Some(Lighting { effect: EffectConfig::Studio { studio: Default::default() }, brightness: None })).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
