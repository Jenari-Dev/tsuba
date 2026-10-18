//! Device registry: discovery/hotplug, applying config on attach, and one
//! event thread per device forwarding decoded hardware events.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tsuba_core::{apply, config, pid_key, state, Config, DeviceState};
use tsuba_hid::{discover, discover_events, poll_readable, Device, DeviceInfo, EventReader, Model};
use tsuba_proto::events;

use crate::studio_runner::{self, StudioHandle};

pub struct Attached {
    pub model: Model,
    pub hidraw: PathBuf,
    pub dev: Arc<Mutex<Device>>,
    /// The device's write counters (check cache validity without taking the device lock).
    pub counters: Arc<tsuba_hid::WriteCounters>,
}

/// (device pid, event) pairs sent from event threads to the D-Bus task.
pub type EventTx = mpsc::UnboundedSender<(String, events::Event)>;

pub struct Manager {
    pub read_only: bool,
    pub config_path: PathBuf,
    pub config: Mutex<Config>,
    pub devices: Mutex<BTreeMap<String, Attached>>,
    studios: Mutex<HashMap<String, StudioHandle>>,
    events: EventTx,
    pub(crate) actions: crate::actions::ActionState,
    pub(crate) shortcut_conn: Mutex<Option<zbus::blocking::Connection>>,
    /// When the user went idle (None = active), and devices whose lighting was turned off for it.
    pub(crate) idle_since: Mutex<Option<std::time::Instant>>,
    pub(crate) dimmed: Mutex<HashMap<String, Vec<(u8, u8)>>>,
    /// The linked-games watcher's state. Held by the watcher across a whole switch: D-Bus
    /// readers use `link_active` instead.
    pub(crate) link_state: Mutex<crate::applinks::LinkState>,
    /// The linked game currently applied (a copy of `link_state.current`, short-held).
    pub(crate) link_active: Mutex<Option<String>>,
    /// The single writer of `app-links.json` inside the daemon: every load → change → save
    /// of the linked games list holds it (see [`crate::applinks`]).
    pub(crate) links_lock: Mutex<()>,
    /// Serialises Game Mode changes (on/off, keys given back around profile switches).
    /// Lock order: `lib_lock` → `gm_lock` → device.
    pub(crate) gm_lock: Mutex<()>,
    pub(crate) cache: crate::cache::Cache,
    pub(crate) warming: crate::cache::Warming,
    /// Devices a game is lighting (game lighting), with what to put back afterwards.
    pub(crate) game_held: Mutex<HashMap<String, GameRestore>>,
    /// Serialises profile-library changes (load → modify → save, activation).
    pub(crate) lib_lock: Mutex<()>,
    /// Devices whose profile library changed (forwarded as the LibraryChanged signal).
    pub(crate) library_tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
}

/// The user's lighting to restore when game lighting hands a device back.
#[derive(Debug, Clone)]
pub enum GameRestore {
    /// A Lighting Studio profile was running (or was started while the game held the device).
    Studio(tsuba_core::studio::Studio),
    /// Hardware effects read back from the device before the game took over (per region).
    Effects(Vec<tsuba_proto::chroma::EffectState>),
    /// The user picked a lighting while the game held the device.
    Lighting(config::Lighting),
    /// The saved (startup) lighting from the config file.
    Saved,
}

impl Manager {
    pub fn new(read_only: bool, config_path: PathBuf, events: EventTx) -> Result<Self> {
        let config = Config::load(&config_path)?;
        Ok(Self { read_only, config_path, config: Mutex::new(config), devices: Mutex::new(BTreeMap::new()), studios: Mutex::new(HashMap::new()), events, actions: Default::default(), shortcut_conn: Mutex::new(None), idle_since: Mutex::new(None), dimmed: Mutex::new(HashMap::new()), link_state: Mutex::new(Default::default()), link_active: Mutex::new(None), links_lock: Mutex::new(()), gm_lock: Mutex::new(()), cache: Default::default(), warming: Default::default(), game_held: Mutex::new(HashMap::new()), lib_lock: Mutex::new(()), library_tx: Mutex::new(None) })
    }

    /// Attach new devices, drop vanished ones. Returns (added, removed) pids.
    pub fn refresh(&self) -> (Vec<String>, Vec<String>) {
        let found: BTreeMap<String, DeviceInfo> = discover().into_iter().map(|d| (pid_key(d.model.pid), d)).collect();
        let mut devices = self.devices.lock().unwrap();
        let removed: Vec<String> = devices
            .iter()
            .filter(|(k, a)| found.get(*k).is_none_or(|d| d.hidraw != a.hidraw))
            .map(|(k, _)| k.clone())
            .collect();
        for k in &removed {
            devices.remove(k);
            self.forget(k);
            self.studios.lock().unwrap().remove(k);
            self.game_held.lock().unwrap().remove(k);
            info!("detached {k}");
        }
        let mut added = Vec::new();
        for (k, info) in found {
            if devices.contains_key(&k) {
                continue;
            }
            match self.attach(&info) {
                Ok(a) => {
                    self.forget(&k);
                    devices.insert(k.clone(), a);
                    added.push(k);
                }
                Err(e) => warn!("{}: attach failed: {e:#}", info.model.name),
            }
        }
        drop(devices);
        for pid in &added {
            self.start_saved_studio(pid);
        }
        (added, removed)
    }

    /// Run the saved Chroma Studio profile, if the device's saved lighting is one.
    fn start_saved_studio(&self, pid: &str) {
        let studio = match self.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone()) {
            Some(config::Lighting { effect: config::EffectConfig::Studio { studio }, .. }) => studio,
            _ => return,
        };
        if let Err(e) = self.start_studio(pid, studio) {
            warn!("{pid}: lighting studio: {e:#}");
        }
    }

    /// Start (or replace) live rendering of a Chroma Studio profile on one device.
    pub fn start_studio(&self, pid: &str, studio: tsuba_core::studio::Studio) -> Result<()> {
        self.ensure_writable()?;
        let dev = self.devices.lock().unwrap().get(pid).map(|a| a.dev.clone()).ok_or_else(|| anyhow!("no device {pid}"))?;
        // Stop the previous renderer before starting the new one (both would write frames).
        self.studios.lock().unwrap().remove(pid);
        let model = self.devices.lock().unwrap().get(pid).map(|a| a.model).ok_or_else(|| anyhow!("no device {pid}"))?;
        self.ensure_writable_for(pid)?;
        // A game is lighting this device: run the studio once the game lets go.
        if let Some(r) = self.game_held.lock().unwrap().get_mut(pid) {
            *r = GameRestore::Studio(studio);
            return Ok(());
        }
        let handle = studio_runner::spawn(pid.to_string(), model, dev, studio)?;
        self.studios.lock().unwrap().insert(pid.to_string(), handle);
        Ok(())
    }

    pub fn stop_studio(&self, pid: &str) {
        self.studios.lock().unwrap().remove(pid);
    }

    /// Whether a Lighting Studio profile is being rendered on this device right now.
    pub fn studio_running(&self, pid: &str) -> bool {
        self.studios.lock().unwrap().contains_key(pid)
    }

    /// Whether game lighting currently drives this device.
    pub fn game_holds(&self, pid: &str) -> bool {
        self.game_held.lock().unwrap().contains_key(pid)
    }

    /// While a game holds the device, remember `r` as the lighting to restore afterwards.
    /// Returns false when no game holds it (the caller applies the lighting itself).
    pub fn game_defer(&self, pid: &str, r: GameRestore) -> bool {
        match self.game_held.lock().unwrap().get_mut(pid) {
            Some(slot) => {
                *slot = r;
                true
            }
            None => false,
        }
    }

    /// Game lighting takes a device: stop its Lighting Studio stream (remembering the
    /// profile) or read back its hardware effects, so [`Self::game_release`] can restore them.
    /// Respects the write gates (read-only daemon, untested models).
    pub fn game_take(&self, pid: &str) -> Result<()> {
        self.ensure_writable_for(pid)?;
        if self.game_holds(pid) {
            return Ok(());
        }
        let running = self.studios.lock().unwrap().remove(pid);
        let restore = match running {
            Some(h) => GameRestore::Studio(h.studio.clone()),
            None => {
                use tsuba_proto::chroma;
                let read = self.with_device(pid, |dev, _| {
                    let mut out = Vec::new();
                    for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                        out.push(chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, r.id])?).context("effect")?);
                    }
                    Ok(out)
                });
                match read {
                    // A custom frame can't be read back; its source is the saved config.
                    Ok(states) if !states.iter().any(|s| s.effect == tsuba_proto::chroma::EffectId::CustomFrame) && !states.is_empty() => GameRestore::Effects(states),
                    Ok(_) => GameRestore::Saved,
                    Err(e) => {
                        warn!("{pid}: reading lighting before game lighting: {e:#}");
                        GameRestore::Saved
                    }
                }
            }
        };
        self.game_held.lock().unwrap().insert(pid.to_string(), restore);
        Ok(())
    }

    /// Game lighting lets go of a device: put the user's lighting back.
    pub fn game_release(&self, pid: &str) {
        let Some(restore) = self.game_held.lock().unwrap().remove(pid) else { return };
        let r = match restore {
            GameRestore::Studio(studio) => self.start_studio(pid, studio),
            GameRestore::Effects(states) => self.with_device(pid, |dev, _| {
                use tsuba_proto::chroma;
                for s in &states {
                    dev.query(chroma::set_effect(s.colours.len()), &chroma::encode_effect_state(s))?;
                }
                Ok(())
            }),
            GameRestore::Lighting(l) => match l.effect {
                config::EffectConfig::Studio { studio } => self.start_studio(pid, studio),
                _ => self.with_device(pid, |dev, _| apply::apply_lighting(dev, &l).map(|_| ())),
            },
            GameRestore::Saved => {
                self.restore_saved_lighting(pid);
                Ok(())
            }
        };
        if let Err(e) = r {
            warn!("{pid}: restoring lighting after game lighting: {e:#}");
        }
    }

    fn attach(&self, info: &DeviceInfo) -> Result<Attached> {
        let mut dev = Device::open(info)?;
        let key = pid_key(info.model.pid);
        let opted_in = self.config.lock().unwrap().device(&key).is_some_and(|d| d.allow_untested_writes);
        dev.allow_untested(info.model.tested() || opted_in);
        info!(
            "attached {} ({key}) at {}{}",
            info.model.name,
            info.hidraw.display(),
            match (info.model.tested(), opted_in) {
                (true, _) => "",
                (false, false) => " — untested model, read-only until writes are enabled for it",
                (false, true) => " — untested model, writes enabled by the user",
            }
        );
        if let Some(cfg) = self.config.lock().unwrap().device(&key).cloned() {
            if self.read_only {
                info!("{key}: read-only mode, not applying saved settings");
            } else if dev.untested_locked() {
                info!("{key}: untested model, not applying saved settings");
            } else {
                dev.allow_writes(true);
                match apply::apply(&dev, &info.model, &cfg) {
                    Ok(log) => log.iter().for_each(|l| info!("{key}: {l}")),
                    Err(e) => warn!("{key}: applying saved settings failed: {e:#}"),
                }
            }
        }
        dev.allow_writes(!self.read_only);
        spawn_event_thread(info.model, self.events.clone());
        let counters = dev.counters().clone();
        Ok(Attached { model: info.model, hidraw: info.hidraw.clone(), dev: Arc::new(Mutex::new(dev)), counters })
    }

    pub fn list(&self) -> Vec<(String, String, String)> {
        self.devices
            .lock()
            .unwrap()
            .iter()
            .map(|(k, a)| (k.clone(), a.model.name.to_string(), a.model.kind.as_str().to_string()))
            .collect()
    }

    /// Run `f` with exclusive access to one device.
    pub fn with_device<T>(&self, pid: &str, f: impl FnOnce(&Device, &Model) -> Result<T>) -> Result<T> {
        let (dev, model) = {
            let devices = self.devices.lock().unwrap();
            let a = devices.get(pid).ok_or_else(|| anyhow!("no device {pid}"))?;
            (a.dev.clone(), a.model)
        };
        let guard = dev.lock().unwrap();
        f(&guard, &model)
    }

    pub fn state(&self, pid: &str) -> Result<DeviceState> {
        self.with_device(pid, state::read_state)
    }

    pub fn ensure_writable(&self) -> Result<()> {
        if self.read_only {
            Err(anyhow!("tsubad is running read-only"))
        } else {
            Ok(())
        }
    }

    /// Like [`Self::ensure_writable`], and also refuses untested models the user has not
    /// enabled writes for.
    pub fn ensure_writable_for(&self, pid: &str) -> Result<()> {
        self.ensure_writable()?;
        // Take the device out of the map and let go of the map before locking the device: the
        // device lock can be held for a while (an activation chunk), and nobody may wait for
        // it while holding the map (lock order: map, then device — never both at once here).
        let (dev, model) = {
            let devices = self.devices.lock().unwrap();
            let a = devices.get(pid).ok_or_else(|| anyhow!("no device {pid}"))?;
            (a.dev.clone(), a.model)
        };
        if dev.lock().unwrap().untested_locked() {
            return Err(anyhow!(
                "{} ({pid}) has not been tested with Tsuba, so changing its settings is off. \
                 Enable writes for this device first (SetAllowUntestedWrites, or allow_untested_writes = true in its config section).",
                model.name
            ));
        }
        Ok(())
    }

    /// Fail unless the device's database entry has `feature` (e.g. "a command dial").
    pub fn require(&self, pid: &str, has: impl FnOnce(&tsuba_hid::Spec) -> bool, feature: &str) -> Result<()> {
        let devices = self.devices.lock().unwrap();
        let a = devices.get(pid).ok_or_else(|| anyhow!("no device {pid}"))?;
        anyhow::ensure!(has(a.model.spec), "{} has no {feature}", a.model.name);
        Ok(())
    }

    /// What Tsuba knows about a device model and whether it may write to it (JSON).
    /// Works for any database pid, attached or not.
    pub fn device_support(&self, pid: &str) -> Result<serde_json::Value> {
        let n = u16::from_str_radix(pid, 16).map_err(|_| anyhow!("bad device id {pid:?}"))?;
        let spec = tsuba_hid::db::get(n).ok_or_else(|| anyhow!("{pid} is not in Tsuba's device database"))?;
        let opted_in = self.config.lock().unwrap().device(pid).is_some_and(|d| d.allow_untested_writes);
        let attached = self.devices.lock().unwrap().contains_key(pid);
        let mut v = serde_json::to_value(spec)?;
        let o = v.as_object_mut().expect("spec serialises to an object");
        o.insert("pid".into(), pid_key(spec.pid).into());
        o.insert("pair".into(), spec.pair.map(pid_key).into());
        o.insert("allow_untested_writes".into(), opted_in.into());
        o.insert("allow_writes".into(), (spec.supported && (spec.tested || opted_in) && !self.read_only).into());
        o.insert("read_only".into(), self.read_only.into());
        o.insert("attached".into(), attached.into());
        o.insert("approximate_art".into(), (!spec.tested).into());
        Ok(v)
    }

    /// Turn writes on or off for an untested model (stored in the config file and applied to
    /// the attached device at once). Tested models always allow writes.
    pub fn set_allow_untested_writes(&self, pid: &str, allow: bool) -> Result<serde_json::Value> {
        let n = u16::from_str_radix(pid, 16).map_err(|_| anyhow!("bad device id {pid:?}"))?;
        let spec = tsuba_hid::db::get(n).ok_or_else(|| anyhow!("{pid} is not in Tsuba's device database"))?;
        anyhow::ensure!(spec.supported, "{} is not a device Tsuba can drive", spec.name);
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.allow_untested_writes = allow;
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        if !allow && !spec.tested {
            self.stop_studio(pid);
        }
        let dev = self.devices.lock().unwrap().get(pid).map(|a| a.dev.clone());
        if let Some(dev) = dev {
            dev.lock().unwrap().allow_untested(spec.tested || allow);
        }
        info!("{pid}: untested-model writes {}", if allow { "enabled" } else { "disabled" });
        self.device_support(pid)
    }

    /// Snapshot every attached device into the config file.
    pub fn save_from_devices(&self) -> Result<Vec<String>> {
        let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
        let mut cfg = self.config.lock().unwrap().clone();
        for pid in &pids {
            let mut fresh = config::from_state(&self.state(pid)?);
            // Keep what the hardware can't report back: software actions, the device mode,
            // and software-rendered lighting (Chroma Studio / custom frames).
            if let Some(old) = cfg.device(pid) {
                fresh.actions = old.actions.clone();
                fresh.mode = old.mode.clone();
                fresh.allow_untested_writes = old.allow_untested_writes;
                fresh.keep_software_settings(old);
                if let Some(l) = &old.lighting {
                    if matches!(l.effect, config::EffectConfig::Studio { .. } | config::EffectConfig::Custom { .. }) {
                        fresh.lighting = Some(l.clone());
                    }
                }
            }
            cfg.upsert(fresh);
        }
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(pids)
    }

    /// Store the game lighting switches in the config file (None = leave as is).
    pub fn save_game_lighting(&self, enabled: Option<bool>, underglow: Option<bool>) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        if enabled.is_some() {
            cfg.game_lighting = enabled;
        }
        if underglow.is_some() {
            cfg.game_lighting_underglow = underglow;
        }
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Store whether game lighting answers web pages (`game_lighting_web`).
    pub fn save_game_lighting_web(&self, web: bool) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        cfg.game_lighting_web = Some(web);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Store one device's lighting in the config file (the GUI already applied it live).
    pub fn save_lighting(&self, pid: &str, lighting: tsuba_core::config::Lighting) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.lighting = Some(lighting);
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    pub fn lighting_power(&self, pid: &str) -> tsuba_core::config::LightingPower {
        self.config.lock().unwrap().device(pid).and_then(|d| d.lighting_power.clone()).unwrap_or(tsuba_core::config::LightingPower { display_off_minutes: 10, ..Default::default() })
    }

    pub fn set_lighting_power(&self, pid: &str, p: tsuba_core::config::LightingPower) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.lighting_power = Some(p);
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Apply a named Chroma profile: each listed device runs its layers and keeps them as its
    /// startup lighting. Returns the devices it was applied to.
    pub fn apply_chroma_profile(&self, name: &str) -> Result<Vec<String>> {
        use tsuba_core::chroma_profiles as cp;
        let mut lib = cp::ChromaProfiles::load(&cp::path())?;
        let prof = lib.get(name).cloned().context("no such lighting profile")?;
        let mut done = Vec::new();
        for (pid, studio) in prof.expanded() {
            if !self.devices.lock().unwrap().contains_key(&pid) || studio.layers.is_empty() {
                continue;
            }
            if let Err(e) = self.ensure_writable_for(&pid) {
                warn!("{pid}: lighting profile skipped: {e:#}");
                continue;
            }
            self.start_studio(&pid, studio.clone())?;
            self.save_lighting(&pid, tsuba_core::config::Lighting { effect: tsuba_core::config::EffectConfig::Studio { studio }, brightness: None })?;
            done.push(pid);
        }
        lib.active = name.to_string();
        lib.save(&cp::path())?;
        Ok(done)
    }

    /// Keep the saved dial section in step with the device (it is re-applied at startup).
    pub fn save_dial(&self, pid: &str, dial: Vec<tsuba_core::config::DialSlotConfig>) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.dial = Some(dial);
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Keep the saved DPI section in step with the device (it is re-applied at startup).
    pub fn save_dpi(&self, pid: &str, dpi: tsuba_core::config::DpiConfig) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.dpi = Some(dpi);
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    pub fn library(&self) -> Result<tsuba_core::macros::Library> {
        tsuba_core::macros::Library::load(&tsuba_core::macros::library_path())
    }

    fn attached(&self) -> Vec<String> {
        self.devices.lock().unwrap().keys().cloned().collect()
    }

    pub fn macros_json(&self) -> Result<serde_json::Value> {
        let lib = self.library()?;
        let mut devices = serde_json::Map::new();
        for pid in self.attached() {
            match self.device_macros_json(&pid).and_then(|j| Ok(serde_json::from_str::<serde_json::Value>(&j)?)) {
                Ok(d) => {
                    devices.insert(pid, d);
                }
                Err(e) => tracing::warn!("{pid}: reading macros: {e:#}"),
            }
        }
        Ok(serde_json::json!({ "library": lib.macros, "devices": devices }))
    }

    pub fn save_macro(&self, mac: tsuba_core::macros::Macro) -> Result<String> {
        anyhow::ensure!(!mac.name.trim().is_empty(), "the macro needs a name");
        mac.encode()?;
        let path = tsuba_core::macros::library_path();
        let mut lib = tsuba_core::macros::Library::load(&path)?;
        let guid = lib.upsert(mac)?;
        lib.save(&path)?;
        let mac = lib.get(&guid).cloned().context("saved macro")?;
        // Refresh copies already on devices (same id, so bindings stay valid).
        for pid in self.attached() {
            self.with_device(&pid, |dev, _| {
                for id in tsuba_core::macros::ids(dev)? {
                    if tsuba_core::macros::hex(&tsuba_core::macros::read_name(dev, id)?.guid) == guid {
                        tsuba_core::macros::ensure_on_device(dev, &mac)?;
                        tracing::info!("{pid}: updated macro {} (0x{id:04x})", mac.name);
                    }
                }
                Ok(())
            })?;
        }
        Ok(guid)
    }

    /// Keys/buttons on profile 1 (both layers) bound to on-board macro `id`.
    fn macro_users(dev: &Device, model: &Model, id: u16) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for layer in [0, 1] {
            let Ok(list) = tsuba_core::state::read_assignments(dev, model, 1, layer) else { continue };
            for a in list {
                if tsuba_core::macros::bound_id(a.function, &a.data) == Some(id) {
                    out.push(format!("{}{}", a.key, if layer == 1 { " (shift layer)" } else { "" }));
                }
            }
        }
        Ok(out)
    }

    pub fn delete_macro(&self, guid: &str) -> Result<()> {
        let path = tsuba_core::macros::library_path();
        let mut lib = tsuba_core::macros::Library::load(&path)?;
        for pid in self.attached() {
            self.with_device(&pid, |dev, model| {
                for id in tsuba_core::macros::ids(dev)? {
                    if tsuba_core::macros::hex(&tsuba_core::macros::read_name(dev, id)?.guid) == guid {
                        let users = Self::macro_users(dev, model, id)?;
                        anyhow::ensure!(users.is_empty(), "still assigned on {pid}: key/button {}", users.join(", "));
                        tsuba_core::macros::delete(dev, id)?;
                    }
                }
                Ok(())
            })?;
        }
        lib.macros.retain(|m| m.guid != guid);
        lib.save(&path)
    }

    pub fn import_macro(&self, pid: &str, id: u16) -> Result<String> {
        let on = self.with_device(pid, |dev, _| tsuba_core::macros::read_macro(dev, id))?;
        let path = tsuba_core::macros::library_path();
        let mut lib = tsuba_core::macros::Library::load(&path)?;
        let guid = lib.upsert(tsuba_core::macros::Macro { guid: on.guid, name: on.name, events: on.events })?;
        lib.save(&path)?;
        Ok(guid)
    }

    /// One device's saved config section as JSON ("{}" if none).
    pub fn config_json(&self, pid: &str) -> String {
        self.config.lock().unwrap().device(pid).map_or_else(|| "{}".into(), |d| serde_json::to_string(d).unwrap_or_default())
    }

    /// Re-read the config file and apply it to every attached device.
    pub fn reload(&self) -> Result<Vec<String>> {
        self.ensure_writable()?;
        let cfg = Config::load(&self.config_path)?;
        *self.config.lock().unwrap() = cfg.clone();
        let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
        let mut log = Vec::new();
        for pid in pids {
            if let Some(dc) = cfg.device(&pid) {
                if self.ensure_writable_for(&pid).is_err() {
                    log.push(format!("{pid}: untested model, writes off — skipped"));
                    continue;
                }
                self.stop_studio(&pid);
                // A game is lighting it: the saved lighting comes back when the game lets go.
                self.game_defer(&pid, GameRestore::Saved);
                let lines = self.with_device(&pid, |dev, model| apply::apply(dev, model, dc))?;
                log.extend(lines.into_iter().map(|l| format!("{pid}: {l}")));
                self.start_saved_studio(&pid);
            }
        }
        Ok(log)
    }
}

/// Read hardware events for one device until its node disappears.
fn spawn_event_thread(model: Model, tx: EventTx) {
    let key = pid_key(model.pid);
    let Some(info) = discover_events().into_iter().find(|d| d.model.pid == model.pid) else {
        warn!("{key}: no event interface");
        return;
    };
    std::thread::Builder::new()
        .name(format!("events-{key}"))
        .spawn(move || {
            let mut reader = match EventReader::open(&info) {
                Ok(r) => r,
                Err(e) => return warn!("{key}: {e}"),
            };
            let mut buf = [0u8; 64];
            loop {
                if poll_readable(&[reader.fd()], 1000).is_empty() {
                    if !info.hidraw.exists() {
                        break;
                    }
                    continue;
                }
                let n = match reader.read_report(&mut buf) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if let Some(ev) = events::parse(&buf[..n]) {
                    if tx.send((key.clone(), ev)).is_err() {
                        break;
                    }
                }
            }
            info!("{key}: event thread stopped");
        })
        .expect("spawn event thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untested_models_need_opt_in() {
        let dir = std::env::temp_dir().join(format!("tsubad-support-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let (tx, _rx) = mpsc::unbounded_channel();
        let mgr = Manager::new(false, path.clone(), tx).unwrap();

        let v = mgr.device_support("028d").unwrap();
        assert_eq!((v["tested"].as_bool(), v["allow_writes"].as_bool(), v["kind"].as_str()), (Some(true), Some(true), Some("keyboard")));
        assert_eq!(v["features"]["dial"], true);

        let v = mgr.device_support("0084").unwrap();
        assert_eq!((v["tested"].as_bool(), v["allow_writes"].as_bool()), (Some(false), Some(false)));
        let v = mgr.set_allow_untested_writes("0084", true).unwrap();
        assert_eq!(v["allow_writes"], true);
        assert!(std::fs::read_to_string(&path).unwrap().contains("allow_untested_writes = true"));
        let v = mgr.set_allow_untested_writes("0084", false).unwrap();
        assert_eq!(v["allow_writes"], false);

        // Headsets are listed but can't be enabled; unknown pids are errors.
        assert!(mgr.set_allow_untested_writes("0501", true).is_err());
        assert!(mgr.device_support("ffff").is_err());
        // Not attached: per-device writes are refused.
        assert!(mgr.ensure_writable_for("0084").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
