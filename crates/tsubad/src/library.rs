//! The profile library on the daemon side (see `tsuba_core::library` for the model): listing,
//! migration of existing on-board profiles, activation (diff-writing a library profile into an
//! on-board slot), pinning, and keeping the active library profile in step with edits.

use std::collections::{HashMap, HashSet};
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::{info, warn};
use tsuba_core::app_links::ProfileRef;
use tsuba_core::config::{ActionConfig, DialSlotConfig, EffectConfig, Lighting};
use tsuba_core::library::{self as lib, ActionBinding, DeviceLibrary, LibraryProfile, MacroBinding, Op};
use tsuba_core::macros::Playback;
use tsuba_core::performance::Performance;
use tsuba_core::profiles::{self, Profiles};
use tsuba_core::state::AssignmentState;
use tsuba_proto::decode;

use crate::manager::Manager;

/// Assignment writes per device-lock hold during activation (Lighting Studio frames and other
/// callers get the device between chunks).
const WRITE_CHUNK: usize = 12;

/// An edit made on a device that belongs to the library profile its slot holds.
#[derive(Debug, Clone)]
pub enum Mirror {
    Key { layer: u8, key: u8, function: u8, data: Vec<u8> },
    Macro(MacroBinding),
    Action(ActionBinding),
    Perf(Performance),
    Dial(Vec<DialSlotConfig>),
}

/// What an activation did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Activation {
    pub id: String,
    pub slot: u8,
    /// Settings written (assignments, macros, actions, performance fields, dial).
    pub writes: usize,
    pub ms: u64,
    /// Settings that couldn't be written (the rest was).
    pub warnings: Vec<String>,
    /// On-board macros removed because nothing on the device uses them any more (Tsuba's own
    /// uploads only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub macros_removed: Vec<String>,
}

/// Macro memory counts as low (and Tsuba's unused uploads are removed) below this fraction
/// of the device's macro memory, or with fewer than [`MACRO_IDS_SPARE`] macro ids left.
const MACRO_LOW_MEMORY_DIV: u32 = 4;
const MACRO_IDS_SPARE: usize = 2;

impl Manager {
    fn lib_load(&self, pid: &str) -> Result<DeviceLibrary> {
        DeviceLibrary::load(&lib::root(), pid)
    }

    /// The device's on-board profile list (cached read).
    fn onboard(&self, pid: &str) -> Result<Profiles> {
        Ok(serde_json::from_str(&self.profiles_json(pid)?)?)
    }

    fn table(&self, pid: &str, slot: u8, layer: u8) -> Result<Vec<AssignmentState>> {
        Ok(serde_json::from_str(&self.assignments_chunked(pid, slot, layer, crate::cache::CHUNK_FAST, None)?)?)
    }

    fn slot_perf(&self, pid: &str, slot: u8) -> Option<Performance> {
        self.performance_json(pid, slot).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Library GUID → on-board id of the macros the device holds.
    fn device_macro_ids(&self, pid: &str) -> HashMap<String, u16> {
        let Ok(v) = self.device_macros_json(pid).and_then(|s| Ok(serde_json::from_str::<Value>(&s)?)) else { return HashMap::new() };
        v["macros"].as_array().into_iter().flatten().filter_map(|m| Some((m["guid"].as_str()?.to_string(), m["id"].as_u64()? as u16))).collect()
    }

    fn slot_actions(&self, pid: &str) -> Vec<ActionConfig> {
        self.config.lock().unwrap().device(pid).map(|d| d.actions.clone()).unwrap_or_default()
    }

    fn model_of(&self, pid: &str) -> Result<tsuba_hid::Model> {
        self.devices.lock().unwrap().get(pid).map(|a| a.model).ok_or_else(|| anyhow!("no device {pid}"))
    }

    pub(crate) fn library_changed(&self, pid: &str) {
        if let Some(tx) = self.library_tx.lock().unwrap().as_ref() {
            let _ = tx.send(pid.to_string());
        }
    }

    // ---- listing -------------------------------------------------------------------------

    /// The library as JSON: `{"pid", "profiles": [{"id", "name", "pinned_slot", "in_slot",
    /// "active", "synapse", "modified"}], "active": id|null, "active_slot", "max_slots",
    /// "working_slot", "slots": [{"slot", "name", "holder", "pinned"}], "free_slots": [..],
    /// "attached"}`. Works for detached devices too (no slot information then).
    pub fn library_json(&self, pid: &str) -> Result<String> {
        let l = self.lib_load(pid)?;
        let attached = self.devices.lock().unwrap().contains_key(pid);
        let on = if attached { self.onboard(pid).ok() } else { None };
        let active_slot = on.as_ref().map(|o| o.active);
        let active = active_slot.and_then(|s| l.holder(s)).map(|p| p.id.clone());
        let max = on.as_ref().map_or(0, |o| o.max);
        let slots: Vec<Value> = on
            .as_ref()
            .map(|o| o.profiles.iter().map(|s| json!({ "slot": s.id, "name": s.name, "holder": l.holder(s.id).map(|p| &p.id), "pinned": l.pinned(s.id).map(|p| &p.id) })).collect())
            .unwrap_or_default();
        let free: Vec<u8> = (1..=max).filter(|s| l.pinned(*s).is_none()).collect();
        let profiles: Vec<Value> = l
            .profiles
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "name": p.name,
                    "pinned_slot": p.pinned_slot,
                    "in_slot": p.pinned_slot.is_some_and(|s| l.state.holds.get(&s) == Some(&p.id)),
                    "active": active.as_deref() == Some(p.id.as_str()),
                    "synapse": p.synapse_guid.is_some(),
                    "synapse_guid": p.synapse_guid,
                    "modified": p.modified,
                    "lighting": p.lighting.as_ref().map(|l| json!({ "effect": tsuba_core::config::effect_name(&l.effect), "brightness": l.brightness })),
                })
            })
            .collect();
        Ok(json!({
            "pid": pid,
            "profiles": profiles,
            "active": active,
            "active_slot": active_slot,
            "max_slots": max,
            "working_slot": l.state.working_slot,
            "slots": slots,
            "free_slots": free,
            "attached": attached,
        })
        .to_string())
    }

    // ---- migration -----------------------------------------------------------------------

    /// Copy every on-board profile not yet in the library into it, pinned to its slot (once per
    /// slot; later runs skip slots already seen). Only reads the device. Returns what it added.
    pub fn library_migrate(&self, pid: &str) -> Result<Vec<String>> {
        let model = self.model_of(pid)?;
        if !model.tested() || !(model.kind.has_buttons() || model.kind.has_keys()) {
            return Ok(vec![]);
        }
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let on = self.onboard(pid)?;
        let todo: Vec<(u8, String)> = on.profiles.iter().filter(|s| !l.state.seen.contains(&s.id)).map(|s| (s.id, s.name.clone())).collect();
        if todo.is_empty() {
            drop(_g);
            // Links that still name slots (an earlier save failed, or a client wrote them back).
            self.library_migrate_links(pid);
            return Ok(vec![]);
        }
        let macro_ids = self.device_macro_ids(pid);
        let by_id: HashMap<u16, String> = macro_ids.iter().map(|(g, id)| (*id, g.clone())).collect();
        let actions = self.slot_actions(pid);
        let dial = if model.spec.features.dial { self.with_device(pid, |dev, _| tsuba_core::state::read_dial(dev)).ok() } else { None };
        // Keys Game Mode turned off are copied with their real bindings.
        let gm = self.game_mode(pid);
        let gm_saved = if gm.active { gm.saved } else { Vec::new() };
        let base = lib::now_ms();
        let mut added = Vec::new();
        for (slot, name) in todo {
            if l.holder(slot).is_some() || l.pinned(slot).is_some() {
                l.state.seen.insert(slot);
                continue;
            }
            let pause = Some(Duration::from_millis(4));
            let mut tables: [Vec<AssignmentState>; 2] = Default::default();
            for layer in [0u8, 1] {
                let j = self.assignments_chunked(pid, slot, layer, crate::cache::CHUNK_WARM, pause).with_context(|| format!("reading profile {slot}"))?;
                tables[layer as usize] = serde_json::from_str(&j)?;
            }
            for (_, key, function, data) in gm_saved.iter().filter(|g| g.0 == slot) {
                if let Some(a) = tables[0].iter_mut().find(|a| a.key == *key) {
                    *a = tsuba_core::state::assignment_state(decode::Assignment { profile: slot, key: *key, layer: 0, function: *function, data: data.clone() });
                }
            }
            let shown = if name.trim().is_empty() { format!("Profile {slot}") } else { name.trim().to_string() };
            let mut p = LibraryProfile::new(&l.unique_name(&shown, None));
            p.created = base + slot as u64;
            p.modified = p.created;
            p.set_tables(pid, &tables);
            // Keys bound to software actions and library macros keep that meaning.
            for (layer, t) in tables.iter().enumerate() {
                let layer = layer as u8;
                for a in t {
                    if let Some(act) = actions.iter().find(|x| x.profile == slot && x.layer == layer && x.key == a.key) {
                        p.set_action(ActionBinding { layer, key: a.key, action: act.action.clone() });
                        continue;
                    }
                    let Some(id) = tsuba_core::macros::bound_id(a.function, &a.data) else { continue };
                    let Some(guid) = by_id.get(&id).cloned() else { continue };
                    let playback = match a.function {
                        3 if a.data.get(2).copied().unwrap_or(1) <= 1 => Some(Playback::Once),
                        3 => Some(Playback::Times),
                        4 => Some(Playback::WhileHeld),
                        5 => Some(Playback::Toggle),
                        _ => None,
                    };
                    let Some(playback) = playback else { continue };
                    if self.ensure_macro_in_library(pid, id, &guid) {
                        p.set_macro(MacroBinding { layer, key: a.key, guid, playback, count: a.data.get(2).copied().unwrap_or(1).max(1) });
                    }
                }
            }
            if let Some(perf) = self.slot_perf(pid, slot) {
                p.merge_performance(&perf);
            }
            p.dial = dial.as_ref().map(|d| d.iter().map(|k| DialSlotConfig { function: k.function.clone(), colour: k.colour.clone(), enabled: k.enabled }).collect());
            p.pinned_slot = Some(slot);
            info!("{pid}: on-board profile {slot} ({shown}) copied into the profile library");
            added.push(format!("{shown} (slot {slot})"));
            l.set_holds(slot, &p.id);
            l.put(p)?;
            l.state.seen.insert(slot);
            // Saved per slot: a later slot failing (device unplugged) mustn't lose this one's holder.
            l.save_state()?;
        }
        l.save_state()?;
        drop(_g);
        self.library_migrate_links(pid);
        self.library_changed(pid);
        Ok(added)
    }

    /// Put an on-board macro into the macro library unless it is there; false if it can't be.
    fn ensure_macro_in_library(&self, pid: &str, id: u16, guid: &str) -> bool {
        if self.library().is_ok_and(|l| l.get(guid).is_some()) {
            return true;
        }
        match self.import_macro(pid, id) {
            Ok(_) => true,
            Err(e) => {
                warn!("{pid}: macro 0x{id:04x} stays on-board only: {e:#}");
                false
            }
        }
    }

    /// Linked games saved with on-board slot numbers now point at the library profiles pinned
    /// to (or held by) those slots (and references to deleted profiles go).
    pub fn library_migrate_links(&self, _pid: &str) {
        self.tidy_app_links();
    }

    // ---- activation ----------------------------------------------------------------------

    /// Make library profile `id` the active one: switch to its pinned slot, or write it into the
    /// working slot (default: the active slot), writing only what differs.
    pub fn library_activate(&self, pid: &str, id: &str) -> Result<Activation> {
        self.ensure_writable_for(pid)?;
        let t0 = Instant::now();
        // An on-board profile that appeared since (another program made it) is copied before
        // anything is written over it.
        self.library_migrate(pid).context("copying new on-board profiles into the library")?;
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let p = l.require(id)?.clone();
        let on = self.onboard(pid)?;
        let ids: Vec<u8> = on.profiles.iter().map(|x| x.id).collect();
        let slot = match p.pinned_slot {
            Some(s) => {
                if !ids.contains(&s) {
                    self.with_device(pid, |dev, _| profiles::create_at(dev, s, &p.name))?;
                }
                s
            }
            None => l.state.working_slot.filter(|s| ids.contains(s)).unwrap_or(on.active),
        };
        // Game Mode turns keys off on the active slot: they are given back before the switch
        // and turned off again (on the new profile) after it — device keys only, never KDE.
        let r = self.with_game_mode_keys_given_back(pid, || {
            // Switch first: a pinned profile is usually already in its slot, so this is the change.
            if on.active != slot {
                self.with_device(pid, |dev, _| profiles::set_active(dev, slot))?;
            }
            self.write_into(pid, &p, slot, &on, &mut l)
        });
        let mut a = r?;
        l.set_holds(slot, id);
        l.save_state()?;
        // Lighting that comes with the profile (a failure is a warning, the keys are done).
        if let Some(light) = &p.lighting {
            match self.apply_profile_lighting(pid, light) {
                Ok(true) => a.writes += 1,
                Ok(false) => {}
                Err(e) => a.warnings.push(format!("lighting: {e:#}")),
            }
        }
        // Macros Tsuba uploaded for profiles no slot runs any more go when memory runs low.
        a.macros_removed = self.gc_macros(pid, &mut l, &[], false);
        a.ms = t0.elapsed().as_millis() as u64;
        info!("{pid}: library profile {:?} active in slot {slot} ({} write(s), {} ms{})", p.name, a.writes, a.ms, if a.warnings.is_empty() { String::new() } else { format!(", {} failed", a.warnings.len()) });
        drop(_g);
        self.library_changed(pid);
        Ok(a)
    }

    /// Write profile `p` into on-board `slot` (only what differs from the slot's current state).
    ///
    /// Macros the device lacks are uploaded and recorded in `l` as Tsuba's own (see
    /// [`Self::gc_macros`]); when an upload fails, unused own uploads are removed and it is
    /// tried once more.
    fn write_into(&self, pid: &str, p: &LibraryProfile, slot: u8, on: &Profiles, l: &mut DeviceLibrary) -> Result<Activation> {
        let model = self.model_of(pid)?;
        let tables = [self.table(pid, slot, 0)?, self.table(pid, slot, 1)?];
        let perf = if p.performance.is_some() { self.slot_perf(pid, slot) } else { None };
        let mut macro_ids = if p.macros.is_empty() { HashMap::new() } else { self.device_macro_ids(pid) };
        let actions = self.slot_actions(pid);
        let plan = lib::plan(pid, slot, p, &tables, perf.as_ref(), &macro_ids, &actions);
        let mut a = Activation { id: p.id.clone(), slot, writes: plan.writes(), ..Default::default() };
        // Pinned profiles carry their name on the device too (an unnamed slot showing as
        // "Profile N" already matches).
        let rename = p.pinned_slot == Some(slot) && on.profiles.iter().any(|x| x.id == slot && x.name != p.name && !(x.name.is_empty() && p.name == format!("Profile {slot}")));
        if plan.is_empty() && !rename {
            return Ok(a);
        }
        let keys: Vec<decode::Assignment> = plan
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::Key(k) => Some(decode::Assignment { profile: slot, key: k.key, layer: k.layer, function: k.function, data: k.data.clone() }),
                _ => None,
            })
            .collect();
        // A write that timed out with `HidError::Busy` (the device kept answering "busy") is
        // reported as failed here although the device may still apply it; the cache isn't
        // patched for it (its counter moved, so the slot is read again next time). Not
        // retried: what the firmware does with a late Set needs checking on real hardware.
        for part in keys.chunks(WRITE_CHUNK) {
            let errs = self.with_device(pid, |dev, model| {
                Ok(part.iter().filter_map(|x| self.write_assignment(pid, dev, model, x).err().map(|e| format!("key {} layer {}: {e:#}", x.key, x.layer))).collect::<Vec<_>>())
            })?;
            a.warnings.extend(errs);
        }
        let mlib = if p.macros.is_empty() { None } else { self.library().ok() };
        for o in &plan.ops {
            match o {
                Op::Macro(b) => {
                    let Some(mac) = mlib.as_ref().and_then(|l| l.get(&b.guid)).cloned() else {
                        a.warnings.push(format!("key {} layer {}: macro {} isn't in the macro library", b.key, b.layer, b.guid));
                        continue;
                    };
                    let bind = |macro_ids: &HashMap<String, u16>| {
                        self.with_device(pid, |dev, model| {
                            let (id, uploaded) = match macro_ids.get(&b.guid) {
                                Some(id) => (*id, false),
                                None => {
                                    let before = tsuba_core::macros::ids(dev)?;
                                    let id = tsuba_core::macros::ensure_on_device(dev, &mac)?;
                                    (id, !before.contains(&id))
                                }
                            };
                            let (function, data) = tsuba_core::macros::binding(id, b.playback, b.count);
                            self.write_assignment(pid, dev, model, &decode::Assignment { profile: slot, key: b.key, layer: b.layer, function, data })?;
                            Ok((id, uploaded))
                        })
                    };
                    let mut r = bind(&macro_ids);
                    // Out of macro memory / ids: make room by removing Tsuba's unused uploads
                    // (never one this profile uses), then try once more.
                    if r.is_err() && !macro_ids.contains_key(&b.guid) {
                        let keep: Vec<String> = p.macros.iter().map(|m| m.guid.clone()).collect();
                        if !self.gc_macros(pid, l, &keep, true).is_empty() {
                            r = bind(&macro_ids);
                        }
                    }
                    match r {
                        Ok((id, uploaded)) => {
                            macro_ids.insert(b.guid.clone(), id);
                            if uploaded {
                                l.state.uploaded_macros.insert(id, b.guid.clone());
                                // Recorded at once: an upload nobody remembers is never removed.
                                if let Err(e) = l.save_state() {
                                    warn!("{pid}: recording uploaded macro 0x{id:04x}: {e:#}");
                                }
                            }
                        }
                        Err(e) => a.warnings.push(format!("key {} layer {}: macro {:?}: {e:#}", b.key, b.layer, mac.name)),
                    }
                }
                Op::Action(b) => {
                    if let Err(e) = self.assign_action(pid, slot, b.layer, b.key, b.action.clone()) {
                        a.warnings.push(format!("key {} layer {}: {}: {e:#}", b.key, b.layer, b.action.label()));
                    }
                }
                Op::Key(_) => {}
            }
        }
        for (layer, key) in &plan.drop_actions {
            if let Err(e) = self.drop_action(pid, slot, *layer, *key) {
                a.warnings.push(format!("key {key} layer {layer}: {e:#}"));
            }
        }
        if let Some(perf) = &plan.performance {
            match self.with_device(pid, |dev, model| tsuba_core::performance::apply(dev, model, perf)) {
                Ok(_) => {
                    // The saved DPI is re-applied to profile 1 at startup: keep it the same.
                    if let (1, Some(stages)) = (slot, perf.dpi_stages.clone()) {
                        let active_stage = perf.dpi_active_stage.unwrap_or(1);
                        let _ = self.save_dpi(pid, tsuba_core::config::DpiConfig { stages, active_stage, active: None });
                    }
                }
                Err(e) => a.warnings.push(format!("performance: {e:#}")),
            }
        }
        if let Some(dial) = &plan.dial {
            if model.spec.features.dial {
                match self.with_device(pid, |dev, _| tsuba_core::apply::apply_dial(dev, dial)) {
                    Ok(log) => {
                        if log.is_empty() {
                            a.writes = a.writes.saturating_sub(1);
                        }
                        let _ = self.save_dial(pid, dial.clone());
                    }
                    Err(e) => a.warnings.push(format!("command dial: {e:#}")),
                }
            }
        }
        // Nothing at all could be written: report it as a failure.
        if plan.writes() > 0 && a.warnings.len() >= plan.writes() {
            bail!("{}", a.warnings.join("; "));
        }
        if rename {
            if let Err(e) = self.with_device(pid, |dev, _| profiles::rename(dev, slot, &p.name)) {
                a.warnings.push(format!("on-board name: {e:#}"));
            }
        }
        Ok(a)
    }

    // ---- macro memory --------------------------------------------------------------------

    /// Remove on-board macros Tsuba uploaded for library profiles (`uploaded_macros`) that no
    /// on-board slot binds any more, when macro memory or macro ids run low (`force`: whenever
    /// there are such macros, e.g. an upload just failed). Never removes a macro Tsuba didn't
    /// upload, one any key/button of any slot (either layer) is bound to, one in `keep`, or
    /// one whose id now holds another macro. Stops without removing anything when a slot's
    /// table can't be read. Returns the names of the macros removed.
    pub(crate) fn gc_macros(&self, pid: &str, l: &mut DeviceLibrary, keep: &[String], force: bool) -> Vec<String> {
        if l.state.uploaded_macros.is_empty() {
            return Vec::new();
        }
        let Ok(dm) = self.device_macros_json(pid).and_then(|s| Ok(serde_json::from_str::<Value>(&s)?)) else { return Vec::new() };
        let on_device: HashMap<u16, (String, String)> = dm["macros"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| Some((m["id"].as_u64()? as u16, (m["guid"].as_str()?.to_string(), m["name"].as_str().unwrap_or_default().to_string()))))
            .collect();
        // Forget uploads that are gone (deleted elsewhere) or whose id was reused.
        let before = l.state.uploaded_macros.len();
        l.state.uploaded_macros.retain(|id, guid| on_device.get(id).is_some_and(|(g, _)| g == guid));
        let mut dirty = l.state.uploaded_macros.len() != before;
        let max_memory = dm["max_memory"].as_u64().unwrap_or(0) as u32;
        let available = dm["available"].as_u64().unwrap_or(u64::MAX).min(u32::MAX as u64) as u32;
        let max_id = dm["max_id"].as_u64().unwrap_or(u64::MAX) as usize;
        let low = available < max_memory / MACRO_LOW_MEMORY_DIV || on_device.len() + MACRO_IDS_SPARE > max_id;
        let mut removed = Vec::new();
        if (force || low) && !l.state.uploaded_macros.is_empty() {
            match self.macros_bound_on_device(pid) {
                Ok(bound) => {
                    let unused: Vec<(u16, String)> = l.state.uploaded_macros.iter().filter(|(id, g)| !bound.contains(id) && !keep.contains(g)).map(|(id, g)| (*id, g.clone())).collect();
                    for (id, guid) in unused {
                        let r = self.with_device(pid, |dev, _| {
                            // Checked again under the device lock: still the macro Tsuba uploaded.
                            ensure!(tsuba_core::macros::hex(&tsuba_core::macros::read_name(dev, id)?.guid) == guid, "macro 0x{id:04x} is no longer the one Tsuba uploaded");
                            tsuba_core::macros::delete(dev, id)
                        });
                        match r {
                            Ok(()) => {
                                let name = on_device.get(&id).map(|x| x.1.clone()).unwrap_or_default();
                                info!("{pid}: removed unused macro {name:?} (0x{id:04x}) Tsuba had uploaded for a library profile");
                                removed.push(name);
                                l.state.uploaded_macros.remove(&id);
                                dirty = true;
                            }
                            Err(e) => warn!("{pid}: removing unused macro 0x{id:04x}: {e:#}"),
                        }
                    }
                }
                Err(e) => warn!("{pid}: macro clean-up skipped (can't read every slot): {e:#}"),
            }
        }
        if dirty {
            if let Err(e) = l.save_state() {
                warn!("{pid}: {e:#}");
            }
        }
        removed
    }

    /// On-board macro ids any key/button of any on-board slot is bound to (both layers).
    fn macros_bound_on_device(&self, pid: &str) -> Result<HashSet<u16>> {
        let mut bound = HashSet::new();
        for s in self.onboard(pid)?.profiles.iter().map(|x| x.id) {
            for layer in [0u8, 1] {
                for a in self.table(pid, s, layer)? {
                    if let Some(id) = tsuba_core::macros::bound_id(a.function, &a.data) {
                        bound.insert(id);
                    }
                }
            }
        }
        Ok(bound)
    }

    // ---- pinning -------------------------------------------------------------------------

    /// Pin a profile to on-board `slot` (0 = the lowest slot without a pinned profile): the
    /// device keeps it there for its profile button and for use without Tsuba. Returns the slot.
    pub fn library_pin(&self, pid: &str, id: &str, slot: u8) -> Result<u8> {
        self.ensure_writable_for(pid)?;
        self.library_migrate(pid).context("copying new on-board profiles into the library")?;
        let g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let mut p = l.require(id)?.clone();
        if let Some(s) = p.pinned_slot {
            ensure!(slot == 0 || slot == s, "“{}” is already pinned to slot {s}; unpin it first", p.name);
            return Ok(s);
        }
        let on = self.onboard(pid)?;
        let slot = if slot == 0 {
            (1..=on.max).find(|s| l.pinned(*s).is_none()).with_context(|| format!("all {} on-board slots hold pinned profiles; unpin one first", on.max))?
        } else {
            slot
        };
        ensure!((1..=on.max).contains(&slot), "the device has profile slots 1–{}", on.max);
        if let Some(other) = l.pinned(slot) {
            bail!("slot {slot} holds “{}”; unpin it first", other.name);
        }
        let create = !on.profiles.iter().any(|x| x.id == slot);
        let was_in = l.state.holds.iter().find(|(_, v)| **v == id).map(|(s, _)| *s);
        p.pinned_slot = Some(slot);
        // Pinning the active profile switches slots, and writing into the active slot would see
        // Game Mode's disabled keys: both run with those keys given back (device only).
        let a = self.with_game_mode_keys_given_back(pid, || {
            let written = (|| {
                if create {
                    self.with_device(pid, |dev, _| profiles::create_at(dev, slot, &p.name))?;
                    // The slot exists now: record it at once, so that whatever fails next it is
                    // never copied back into the library as a second copy ("Sniper 2").
                    l.state.seen.insert(slot);
                    l.save_state()?;
                }
                l.state.seen.insert(slot);
                let on = self.onboard(pid)?;
                let a = self.write_into(pid, &p, slot, &on, &mut l)?;
                Ok((a, on))
            })();
            let (a, on) = match written {
                Ok(x) => x,
                Err(e) => {
                    if create {
                        self.pin_rollback(pid, &mut l, slot);
                    }
                    return Err(e);
                }
            };
            l.put(p.clone())?;
            l.set_holds(slot, id);
            l.save_state()?;
            // It was running from another slot: run it from its own slot now, and give that
            // slot back to the profile pinned there.
            if let Some(w) = was_in.filter(|w| *w != slot) {
                if on.active == w {
                    self.with_device(pid, |dev, _| profiles::set_active(dev, slot))?;
                }
                if let Some(owner) = l.pinned(w).cloned() {
                    match self.write_into(pid, &owner, w, &on, &mut l) {
                        Ok(_) => l.set_holds(w, &owner.id),
                        Err(e) => warn!("{pid}: restoring “{}” in slot {w}: {e:#}", owner.name),
                    }
                }
                l.save_state()?;
            }
            Ok(a)
        })?;
        info!("{pid}: “{}” pinned to slot {slot} ({} write(s))", p.name, a.writes);
        drop(g);
        self.library_changed(pid);
        Ok(slot)
    }

    /// Pinning failed after it created `slot`: remove the slot again (unless the device runs
    /// it); if that isn't possible the slot stays recorded as seen, so it isn't copied into
    /// the library as a duplicate on the next attach.
    fn pin_rollback(&self, pid: &str, l: &mut DeviceLibrary, slot: u8) {
        let r = self.with_device(pid, |dev, _| {
            if !profiles::ids(dev)?.contains(&slot) {
                return Ok(());
            }
            ensure!(profiles::active(dev)? != slot, "the device runs it");
            profiles::delete(dev, slot)
        });
        match r {
            Ok(()) => {
                l.state.seen.remove(&slot);
                l.state.holds.remove(&slot);
            }
            Err(e) => {
                warn!("{pid}: removing the new slot {slot} again: {e:#} (kept, not copied into the library)");
                l.state.seen.insert(slot);
            }
        }
        if let Err(e) = l.save_state() {
            warn!("{pid}: {e:#}");
        }
    }

    /// Unpin a profile. Its slot is removed from the device unless it is slot 1 or the active one.
    pub fn library_unpin(&self, pid: &str, id: &str) -> Result<()> {
        let g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let mut p = l.require(id)?.clone();
        let Some(slot) = p.pinned_slot else { return Ok(()) };
        if self.devices.lock().unwrap().contains_key(pid) && self.ensure_writable_for(pid).is_ok() {
            let active = self.with_device(pid, |dev, _| profiles::active(dev))?;
            if slot != 1 && slot != active {
                self.with_device(pid, |dev, _| profiles::delete(dev, slot))?;
                l.state.holds.remove(&slot);
                // A profile another program puts there later is copied in again.
                l.state.seen.remove(&slot);
            }
        }
        p.pinned_slot = None;
        l.put(p)?;
        l.save_state()?;
        drop(g);
        self.library_changed(pid);
        Ok(())
    }

    // ---- library edits (PC only) ---------------------------------------------------------

    pub fn library_create(&self, pid: &str, name: &str, copy_from: &str) -> Result<String> {
        ensure!(!name.trim().is_empty(), "the profile needs a name");
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let n = l.unique_name(name, None);
        let p = if copy_from.is_empty() { LibraryProfile::new(&n) } else { l.require(copy_from)?.duplicate(&n) };
        let id = p.id.clone();
        l.put(p)?;
        drop(_g);
        self.library_changed(pid);
        Ok(id)
    }

    pub fn library_rename(&self, pid: &str, id: &str, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "the profile needs a name");
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let mut p = l.require(id)?.clone();
        p.name = l.unique_name(name, Some(id));
        p.modified = lib::now_ms();
        if let Some(s) = p.pinned_slot.filter(|s| l.state.holds.get(s).map(String::as_str) == Some(id)) {
            if self.devices.lock().unwrap().contains_key(pid) && self.ensure_writable_for(pid).is_ok() {
                if let Err(e) = self.with_device(pid, |dev, _| profiles::rename(dev, s, &p.name)) {
                    warn!("{pid}: renaming on-board profile {s}: {e:#}");
                }
            }
        }
        l.put(p)?;
        drop(_g);
        self.library_changed(pid);
        Ok(())
    }

    /// Delete a profile (unpinning it first). The active profile can't be deleted.
    pub fn library_delete(&self, pid: &str, id: &str) -> Result<()> {
        let not_active = |l: &DeviceLibrary| -> Result<()> {
            let p = l.require(id)?;
            if self.devices.lock().unwrap().contains_key(pid) {
                let active = self.onboard(pid)?.active;
                ensure!(l.holder(active).map(|x| x.id.as_str()) != Some(id), "“{}” is the active profile; switch to another one first", p.name);
            }
            Ok(())
        };
        {
            let l = self.lib_load(pid)?;
            not_active(&l)?;
            if l.require(id)?.pinned_slot.is_some() {
                self.library_unpin(pid, id)?;
            }
        }
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        // Again under the lock: an activation (linked game) may have made it active meanwhile.
        not_active(&l)?;
        l.remove(id)?;
        l.save_state()?;
        drop(_g);
        // Linked games no longer switch to it (the file is gone, so tidying drops it).
        self.tidy_app_links();
        self.library_changed(pid);
        Ok(())
    }

    /// Create or update a profile from JSON (Synapse import): a profile with the same
    /// `synapse_guid` is updated in place (keeping its id, pin and creation time). Slots holding
    /// it get the new settings. Returns its id.
    pub fn library_save(&self, pid: &str, profile_json: &str) -> Result<String> {
        let mut p: LibraryProfile = serde_json::from_str(profile_json).context("profile JSON")?;
        ensure!(!p.name.trim().is_empty(), "the profile needs a name");
        let g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let existing = p.synapse_guid.as_deref().and_then(|s| l.by_synapse(s)).or_else(|| l.get(&p.id)).cloned();
        match &existing {
            Some(old) => {
                p.id = old.id.clone();
                p.created = old.created;
                p.pinned_slot = old.pinned_slot;
                // A re-import brings what Synapse knows; lighting that comes with the profile
                // is Tsuba's own and stays.
                if p.lighting.is_none() {
                    p.lighting = old.lighting.clone();
                }
            }
            None => {
                p.id = lib::new_id();
                p.created = lib::now_ms();
                p.pinned_slot = None;
            }
        }
        p.name = l.unique_name(&p.name, Some(&p.id));
        p.modified = lib::now_ms();
        if let Some(perf) = &mut p.performance {
            perf.profile = None;
        }
        let id = p.id.clone();
        l.put(p.clone())?;
        // Slots holding the old version get the new one.
        let held: Vec<u8> = l.state.holds.iter().filter(|(_, v)| **v == id).map(|(s, _)| *s).collect();
        if !held.is_empty() && self.ensure_writable_for(pid).is_ok() {
            // A held slot may be the active one, where Game Mode has keys off.
            let r = self.with_game_mode_keys_given_back(pid, || {
                let on = self.onboard(pid)?;
                for s in held {
                    if let Err(e) = self.write_into(pid, &p, s, &on, &mut l) {
                        warn!("{pid}: updating slot {s} with “{}”: {e:#}", p.name);
                    }
                }
                Ok(())
            });
            if let Err(e) = r {
                warn!("{pid}: updating the slots holding “{}”: {e:#}", p.name);
            }
        }
        drop(g);
        self.library_changed(pid);
        Ok(id)
    }

    pub fn library_export(&self, pid: &str, id: &str, path: &str) -> Result<()> {
        let l = self.lib_load(pid)?;
        let text = lib::export(pid, l.require(id)?)?;
        std::fs::write(path, text).with_context(|| format!("writing {path}"))
    }

    /// What importing `path` would bring along that runs things on this computer (Launch, Open
    /// URL and Type actions): `{"name", "software": [{"layer", "key", "kind", "text"}],
    /// "digest"}`. The app shows the list and passes `digest` to [`Self::library_import`] to
    /// approve exactly these.
    pub fn library_import_preview(&self, pid: &str, path: &str) -> Result<serde_json::Value> {
        let text = lib::read_import_file(path)?;
        let imp = lib::import(pid, &text, "")?;
        Ok(serde_json::json!({ "name": imp.profile.name, "software": imp.software, "digest": lib::approval_digest(&imp.software) }))
    }

    /// Add a profile from a file. Software actions in it are dropped unless `approved` is the
    /// digest [`Self::library_import_preview`] gave for the file as it is now.
    pub fn library_import(&self, pid: &str, path: &str, approved: &str) -> Result<String> {
        let text = lib::read_import_file(path)?;
        let imp = lib::import(pid, &text, approved)?;
        ensure!(approved.is_empty() || imp.approved, "the file changed since it was shown: its commands are not what was approved");
        if !imp.software.is_empty() && !imp.approved {
            info!("{pid}: imported a profile without its {} software action(s) (not approved)", imp.software.len());
        }
        let mut p = imp.profile;
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        p.name = l.unique_name(&p.name, None);
        // The Synapse GUID says which profile a Synapse re-import updates: it stays unique.
        if p.synapse_guid.as_deref().is_some_and(|g| l.by_synapse(g).is_some()) {
            p.synapse_guid = None;
        }
        let id = p.id.clone();
        l.put(p)?;
        drop(_g);
        self.library_changed(pid);
        Ok(id)
    }

    /// The slot unpinned profiles are written into (0 = the active slot).
    pub fn library_set_working_slot(&self, pid: &str, slot: u8) -> Result<()> {
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        l.state.working_slot = (slot > 0).then_some(slot);
        l.save_state()
    }

    // ---- lighting that follows a profile ---------------------------------------------------

    /// Make `light` the device's lighting (live and as its saved startup lighting). Returns
    /// whether anything was applied: lighting that is already the saved one is left running
    /// (re-activating a profile must not restart a Lighting Studio effect).
    fn apply_profile_lighting(&self, pid: &str, light: &Lighting) -> Result<bool> {
        let saved = self.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone());
        if saved.as_ref() == Some(light) {
            return Ok(false);
        }
        // The profile's lighting covers every zone: per-zone effects from before would
        // otherwise come back over it at the next restore.
        self.edit_device_config(pid, |dc| {
            dc.lighting = Some(light.clone());
            dc.zones.clear();
        })?;
        // A game is lighting the device: this is what to put back afterwards.
        if self.game_defer(pid, crate::manager::GameRestore::Lighting(light.clone())) {
            return Ok(true);
        }
        match &light.effect {
            EffectConfig::Studio { studio } => self.start_studio(pid, studio.clone())?,
            _ => {
                self.stop_studio(pid);
                self.with_device(pid, |dev, _| tsuba_core::apply::apply_lighting(dev, light).map(|_| ()))?;
            }
        }
        Ok(true)
    }

    /// The lighting the device has now, as a config value: the saved one for Lighting Studio
    /// and custom frames (they can't be read back), else what the hardware reports.
    fn current_lighting(&self, pid: &str) -> Result<Lighting> {
        let saved = self.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone());
        if let Some(l) = saved.clone().filter(|l| matches!(l.effect, EffectConfig::Studio { .. } | EffectConfig::Custom { .. })) {
            return Ok(l);
        }
        let from_device = self.state(pid).ok().and_then(|st| tsuba_core::config::from_state(&st).lighting);
        from_device.or(saved).context("this device has no lighting to store in the profile")
    }

    /// Store lighting in a profile (`what` = "current": what the device shows now; "off": the
    /// profile no longer carries lighting; or a Lighting JSON). Active profiles with lighting
    /// are applied right away only through activation, so this just saves.
    pub fn library_set_lighting(&self, pid: &str, id: &str, what: &str) -> Result<()> {
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let mut p = l.require(id)?.clone();
        p.lighting = match what.trim() {
            "off" | "none" | "null" => None,
            "current" => Some(self.current_lighting(pid)?),
            json => Some(serde_json::from_str(json).context("lighting JSON")?),
        };
        p.modified = lib::now_ms();
        l.put(p)?;
        drop(_g);
        self.library_changed(pid);
        Ok(())
    }

    /// The user changed the lighting (and saved it): the profile that is active keeps that as
    /// its lighting when it carries lighting at all.
    pub(crate) fn mirror_lighting(&self, pid: &str, light: &Lighting) {
        let r = (|| -> Result<()> {
            if !self.devices.lock().unwrap().contains_key(pid) {
                return Ok(());
            }
            let _g = self.lib_lock.lock().unwrap();
            let active = self.with_device(pid, |dev, _| profiles::active(dev))?;
            let mut l = self.lib_load(pid)?;
            let Some(mut p) = l.holder(active).cloned() else { return Ok(()) };
            if p.lighting.is_none() || p.lighting.as_ref() == Some(light) {
                return Ok(());
            }
            p.lighting = Some(light.clone());
            p.modified = lib::now_ms();
            l.put(p)?;
            drop(_g);
            self.library_changed(pid);
            Ok(())
        })();
        if let Err(e) = r {
            warn!("{pid}: saving the lighting in the active profile: {e:#}");
        }
    }

    // ---- more library edits ------------------------------------------------------------------

    /// A profile that resets the device to its factory settings (keys, performance, dial).
    pub fn library_create_factory(&self, pid: &str, name: &str) -> Result<String> {
        ensure!(!name.trim().is_empty(), "the profile needs a name");
        let _g = self.lib_lock.lock().unwrap();
        let mut l = self.lib_load(pid)?;
        let p = LibraryProfile::factory(pid, &l.unique_name(name, None));
        let id = p.id.clone();
        l.put(p)?;
        drop(_g);
        self.library_changed(pid);
        Ok(id)
    }

    /// Write several profiles into folder `dir`, one `<name>.tsuba-profile.json` each (existing
    /// files are never overwritten: a number is added). Returns the paths written.
    pub fn library_export_many(&self, pid: &str, ids: &[String], dir: &str) -> Result<Vec<String>> {
        let l = self.lib_load(pid)?;
        let dir = std::path::Path::new(dir);
        ensure!(dir.is_dir(), "{} is not a folder", dir.display());
        let mut out = Vec::new();
        for id in ids {
            let p = l.require(id)?;
            let text = lib::export(pid, p)?;
            let stem: String = p.name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' }).take(60).collect();
            let stem = if stem.trim().is_empty() { "profile".to_string() } else { stem.trim().to_string() };
            let mut path = dir.join(format!("{stem}.tsuba-profile.json"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{stem} {n}.tsuba-profile.json"));
                n += 1;
            }
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
            out.push(path.display().to_string());
        }
        Ok(out)
    }

    /// Which library profiles (of every device) bind each library macro, and the bindings whose
    /// macro isn't in the macro library any more:
    /// `{"used": {guid: [{pid, profile, profile_id, key, layer}]}, "missing": [{…, guid}]}`.
    pub fn macro_usage_json(&self) -> Result<Value> {
        let macros = self.library()?;
        let mut used: serde_json::Map<String, Value> = serde_json::Map::new();
        let mut missing = Vec::new();
        let root = lib::root();
        let mut pids: Vec<String> = std::fs::read_dir(&root).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.len() == 4 && n.bytes().all(|b| b.is_ascii_hexdigit())).collect();
        pids.sort();
        for pid in pids {
            let Ok(l) = DeviceLibrary::load(&root, &pid) else { continue };
            for p in &l.profiles {
                for b in &p.macros {
                    let entry = json!({ "pid": pid, "profile": p.name, "profile_id": p.id, "key": b.key, "layer": b.layer });
                    if macros.get(&b.guid).is_some() {
                        used.entry(b.guid.clone()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(entry);
                    } else {
                        let mut e = entry;
                        e["guid"] = json!(b.guid);
                        missing.push(e);
                    }
                }
            }
        }
        Ok(json!({ "used": used, "missing": missing }))
    }

    // ---- keeping the library in step with edits -----------------------------------------

    /// An edit on `slot` (0 = the active slot) went to the device: store it in the library
    /// profile that slot holds. Never fails the caller; problems are logged. `_held` is the
    /// library lock, taken *before* the edit was written, so that no activation can change
    /// what the slot holds between the write and this.
    pub fn library_mirror(&self, _held: &MutexGuard<'_, ()>, pid: &str, slot: u8, m: Mirror) {
        if let Err(e) = self.try_mirror(pid, slot, m) {
            warn!("{pid}: saving the edit in the profile library: {e:#}");
        }
    }

    fn try_mirror(&self, pid: &str, slot: u8, m: Mirror) -> Result<()> {
        let slot = if slot == 0 || matches!(m, Mirror::Dial(_)) { self.with_device(pid, |dev, _| profiles::active(dev))? } else { slot };
        let mut l = self.lib_load(pid)?;
        let Some(mut p) = l.holder(slot).cloned() else { return Ok(()) };
        match m {
            Mirror::Key { layer, key, function, data } => p.set_key(pid, layer, key, function, &data),
            Mirror::Macro(b) => p.set_macro(b),
            Mirror::Action(b) => p.set_action(b),
            Mirror::Perf(perf) => p.merge_performance(&perf),
            Mirror::Dial(d) => p.dial = Some(d),
        }
        p.modified = lib::now_ms();
        l.put(p)?;
        self.library_changed(pid);
        Ok(())
    }

    /// For linked games: the profile to come back to (the active library profile, or the
    /// active slot when it holds none).
    pub fn library_current(&self, pid: &str) -> Result<ProfileRef> {
        let active = self.with_device(pid, |dev, _| profiles::active(dev))?;
        let l = self.lib_load(pid)?;
        Ok(match l.holder(active) {
            Some(p) => ProfileRef::Library(p.id.clone()),
            None => ProfileRef::Slot(active),
        })
    }

    /// Switch to a linked game's profile reference.
    pub fn library_switch(&self, pid: &str, r: &ProfileRef) -> Result<()> {
        match r {
            ProfileRef::Library(id) => self.library_activate(pid, id).map(|_| ()),
            ProfileRef::Slot(s) => {
                self.ensure_writable_for(pid)?;
                self.with_device(pid, |dev, _| profiles::set_active(dev, *s))?;
                self.library_changed(pid);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tsuba_core::app_links::{self, AppLinks};
    use tsuba_core::sim::{self, Sim};

    /// Every test here keeps its config (library, macros, app links) in a temp folder.
    pub(crate) fn temp_config() -> std::path::PathBuf {
        static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            let d = std::env::temp_dir().join(format!("tsubad-library-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            // SAFETY: set once, before any test of this module reads the config directory.
            unsafe { std::env::set_var("XDG_CONFIG_HOME", &d) };
            d
        })
        .clone()
    }

    use crate::gamemode::fake_kde::SERIAL;

    pub(crate) fn rig(pid: &str, slots: u8) -> (Arc<Manager>, Sim) {
        rig_hooked(pid, slots, Arc::new(|_, _| None))
    }

    /// Runs before the simulated device answers a command; `Some(error)` fails it instead
    /// (e.g. an I/O error for an unplugged device).
    pub(crate) type Hook = Arc<dyn Fn(tsuba_proto::Command, &[u8]) -> Option<tsuba_hid::HidError> + Send + Sync>;

    fn unplugged() -> tsuba_hid::HidError {
        tsuba_hid::HidError::Io { path: "/dev/null".into(), source: std::io::Error::from_raw_os_error(19) }
    }

    pub(crate) fn rig_hooked(pid: &str, slots: u8, hook: Hook) -> (Arc<Manager>, Sim) {
        let dir = temp_config();
        let _ = std::fs::remove_dir_all(dir.join("tsuba"));
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mgr = Arc::new(Manager::new(false, dir.join("tsuba").join("config.toml"), tx).unwrap());
        let sim: Sim = Arc::new(Mutex::new(sim::new_state(pid, slots)));
        let s = sim.clone();
        let mut dev = tsuba_hid::Device::simulated(Arc::new(move |cmd, a| match hook(cmd, a) {
            Some(e) => Err(e),
            None => s.lock().unwrap().answer(cmd, a),
        }))
        .unwrap();
        dev.allow_writes(true);
        let counters = dev.counters().clone();
        let n = u16::from_str_radix(pid, 16).unwrap();
        let a = crate::manager::Attached { model: tsuba_hid::model(n).unwrap(), hidraw: "/dev/null".into(), dev: Arc::new(Mutex::new(dev)), counters };
        mgr.devices.lock().unwrap().insert(pid.into(), a);
        (mgr, sim)
    }

    pub(crate) fn list(mgr: &Manager, pid: &str) -> Value {
        serde_json::from_str(&mgr.library_json(pid).unwrap()).unwrap()
    }

    #[test]
    fn migration_copies_slots_once_and_maps_links() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("00aa", 3);
        {
            let mut s = sim.lock().unwrap();
            s.set_name(2, "FPS");
            s.profiles.get_mut(&2).unwrap().tables[0].insert(4, (2, vec![0, 0x3a]));
            s.profiles.get_mut(&3).unwrap().dpi = vec![(1600, 1600)];
            s.active = 2;
        }
        AppLinks { links: vec![app_links::AppLink { name: "G".into(), matches: "g".into(), enabled: true, device_profiles: [("00aa".to_string(), ProfileRef::Slot(2))].into(), ..Default::default() }] }.save(&app_links::path()).unwrap();
        let added = mgr.library_migrate("00aa").unwrap();
        assert_eq!(added.len(), 3, "{added:?}");
        assert!(sim.lock().unwrap().writes.is_empty(), "migration only reads");
        let v = list(&mgr, "00aa");
        let names: Vec<&str> = v["profiles"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Profile 1", "FPS", "Profile 3"]);
        assert_eq!(v["profiles"][1]["pinned_slot"], 2);
        assert_eq!(v["profiles"][1]["active"], true);
        assert_eq!(v["free_slots"], json!([4, 5]));
        let l = mgr.lib_load("00aa").unwrap();
        let fps = l.pinned(2).unwrap();
        assert_eq!(fps.assignments, vec![lib::KeyBinding { layer: 0, key: 4, function: 2, data: vec![0, 0x3a] }]);
        assert_eq!(l.pinned(3).unwrap().performance.as_ref().unwrap().dpi_stages, Some(vec![[1600, 1600]]));
        let links = AppLinks::load(&app_links::path()).unwrap();
        assert_eq!(links.links[0].device_profiles["00aa"], ProfileRef::Library(fps.id.clone()));
        // Idempotent.
        assert!(mgr.library_migrate("00aa").unwrap().is_empty());
        assert_eq!(mgr.lib_load("00aa").unwrap().profiles.len(), 3);
    }

    #[test]
    fn activation_writes_only_differences_and_edits_are_mirrored() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let base = mgr.lib_load("028d").unwrap().profiles[0].id.clone();
        // A new unpinned profile with three remaps, written into the active slot.
        let id = mgr.library_create("028d", "Gaming", &base).unwrap();
        mgr.library_mirror(&mgr.lib_lock.lock().unwrap(), "028d", 1, Mirror::Key { layer: 0, key: 2, function: 2, data: vec![0, 0x3a] }); // goes to the base (held) profile
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("028d").unwrap();
            let mut p = l.get(&id).unwrap().clone();
            p.set_key("028d", 0, 3, 2, &[0, 0x3b]);
            p.set_key("028d", 0, 4, 2, &[0, 0x3c]);
            p.set_key("028d", 1, 5, 2, &[0, 0x3d]);
            l.put(p).unwrap();
        }
        // The base profile got the mirrored edit but the device didn't (mirroring only records).
        assert_eq!(mgr.lib_load("028d").unwrap().get(&base).unwrap().assignments.len(), 1);
        sim.lock().unwrap().writes.clear();
        let a = mgr.library_activate("028d", &id).unwrap();
        assert_eq!((a.slot, a.writes, a.warnings.len()), (1, 3, 0), "{a:?}");
        assert_eq!(sim.lock().unwrap().assignment_writes(), 3);
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&3], (2, vec![0, 0x3b]));
        assert_eq!(list(&mgr, "028d")["active"], json!(id));
        // Again: nothing to write.
        sim.lock().unwrap().writes.clear();
        assert_eq!(mgr.library_activate("028d", &id).unwrap().writes, 0);
        assert!(sim.lock().unwrap().writes.is_empty());
        // Back to the base profile: the three keys go back, its own remap (key 2) is written.
        let a = mgr.library_activate("028d", &base).unwrap();
        assert_eq!(a.writes, 4, "{a:?}");
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&2], (2, vec![0, 0x3a]));
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&3], lib::factory_default("028d", 0, 3).unwrap());
        // An edit while it's active lands in it.
        mgr.library_mirror(&mgr.lib_lock.lock().unwrap(), "028d", 1, Mirror::Key { layer: 0, key: 10, function: 2, data: vec![0, 0x04] });
        let l = mgr.lib_load("028d").unwrap();
        assert!(l.get(&base).unwrap().assignments.iter().any(|k| k.key == 10));
        assert!(!l.get(&id).unwrap().assignments.iter().any(|k| k.key == 10));
        // The active profile can't be deleted; the other can.
        assert!(mgr.library_delete("028d", &base).is_err());
        mgr.library_delete("028d", &id).unwrap();
        assert_eq!(mgr.lib_load("028d").unwrap().profiles.len(), 1);
    }

    #[test]
    fn pinning_uses_free_slots_and_the_profile_button() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("00aa", 1);
        mgr.library_migrate("00aa").unwrap();
        let id = mgr.library_create("00aa", "Sniper", "").unwrap();
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("00aa").unwrap();
            let mut p = l.get(&id).unwrap().clone();
            p.set_key("00aa", 0, 4, 1, &[5]);
            p.merge_performance(&Performance { dpi_stages: Some(vec![[400, 400], [800, 800]]), dpi_active_stage: Some(2), ..Default::default() });
            l.put(p).unwrap();
        }
        assert_eq!(mgr.library_pin("00aa", &id, 0).unwrap(), 2);
        {
            let s = sim.lock().unwrap();
            assert!(s.profiles.contains_key(&2), "slot 2 created");
            assert_eq!(s.profiles[&2].tables[0][&4], (1, vec![5]));
            assert_eq!(s.profiles[&2].dpi, vec![(400, 400), (800, 800)]);
            assert_eq!(s.active, 1, "pinning doesn't switch");
        }
        let v = list(&mgr, "00aa");
        assert_eq!(v["profiles"][1]["pinned_slot"], 2);
        assert_eq!(v["profiles"][1]["in_slot"], true);
        // The hardware button moves to slot 2: the pinned profile shows as active.
        sim.lock().unwrap().active = 2;
        mgr.counters_of("00aa").unwrap().touch(tsuba_hid::Domain::Settings);
        assert_eq!(list(&mgr, "00aa")["active"], json!(id));
        // Activating it from the library while on slot 1 just switches (it's in its slot).
        sim.lock().unwrap().active = 1;
        mgr.counters_of("00aa").unwrap().touch(tsuba_hid::Domain::Settings);
        sim.lock().unwrap().writes.clear();
        let a = mgr.library_activate("00aa", &id).unwrap();
        assert_eq!((a.slot, a.writes), (2, 0));
        assert_eq!(sim.lock().unwrap().writes, vec![(0x05, 0x04)], "one Set Active Profile");
        // Unpin a non-active slot: it leaves the device. Switch away first.
        let base = mgr.lib_load("00aa").unwrap().pinned(1).unwrap().id.clone();
        mgr.library_activate("00aa", &base).unwrap();
        mgr.library_unpin("00aa", &id).unwrap();
        assert!(!sim.lock().unwrap().profiles.contains_key(&2));
        assert_eq!(list(&mgr, "00aa")["profiles"][1]["pinned_slot"], Value::Null);
        // Read-only daemons refuse device changes.
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let ro = Manager::new(true, temp_config().join("ro.toml"), tx).unwrap();
        assert!(ro.library_activate("00aa", &id).is_err());
    }

    #[test]
    fn synapse_reimport_updates_in_place() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("00aa", 1);
        mgr.library_migrate("00aa").unwrap();
        let mut p = LibraryProfile::new("From Synapse");
        p.synapse_guid = Some("syn-1".into());
        p.set_key("00aa", 0, 5, 1, &[4]);
        let id = mgr.library_save("00aa", &serde_json::to_string(&p).unwrap()).unwrap();
        mgr.library_activate("00aa", &id).unwrap();
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&5], (1, vec![4]));
        // Re-import with a change: same id, and the slot holding it is updated.
        p.set_key("00aa", 0, 5, 1, &[3]);
        p.id = lib::new_id();
        assert_eq!(mgr.library_save("00aa", &serde_json::to_string(&p).unwrap()).unwrap(), id);
        assert_eq!(mgr.lib_load("00aa").unwrap().profiles.len(), 2);
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&5], (1, vec![3]));
    }

    #[test]
    fn timing_on_a_slow_device() {
        // ~2.6 ms per HID round trip as measured on the BlackWidow V4 Pro: a full keyboard
        // profile change (every remappable key different) must stay within ~2 s.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let id = mgr.library_create("028d", "All different", "").unwrap();
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("028d").unwrap();
            let mut p = l.get(&id).unwrap().clone();
            for layer in [0, 1] {
                for (k, _) in lib::factory_table("028d", layer) {
                    p.set_key("028d", layer, k, 2, &[0, 0x04]);
                }
            }
            l.put(p).unwrap();
        }
        let n = [0u8, 1].iter().map(|l| lib::factory_table("028d", *l).values().filter(|v| **v != (2, vec![0, 0x04])).count()).sum::<usize>();
        sim.lock().unwrap().writes.clear();
        let a = mgr.library_activate("028d", &id).unwrap();
        assert_eq!(a.writes, n);
        let s = sim.lock().unwrap();
        // Each assignment = Set + read-back Get.
        let round_trips = s.assignment_writes() * 2;
        let est_ms = round_trips as f64 * 2.6;
        assert!(est_ms < 2000.0, "{round_trips} round trips ≈ {est_ms} ms");
    }

    #[test]
    fn migration_keeps_keys_game_mode_turned_off() {
        // Game Mode (on while the daemon was stopped) has the Win key off in slot 1: the library
        // copy gets the key's real binding, not "disabled".
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        let win = lib::factory_default("028d", 0, 127).unwrap();
        sim.lock().unwrap().profiles.get_mut(&1).unwrap().tables[0].insert(127, (0, vec![]));
        {
            let mut cfg = mgr.config.lock().unwrap();
            let mut dc = cfg.device("028d").cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: "028d".into(), ..Default::default() });
            dc.game_mode = Some(tsuba_core::config::GameModeConfig { disable_win: true, block_shortcuts: false, active: true, saved: vec![(1, 127, win.0, win.1.clone())], ..Default::default() });
            cfg.upsert(dc);
        }
        mgr.library_migrate("028d").unwrap();
        let l = mgr.lib_load("028d").unwrap();
        let p = l.pinned(1).unwrap();
        assert!(!p.assignments.iter().any(|a| a.layer == 0 && a.key == 127), "{:?}", p.assignments);
    }

    #[test]
    fn interrupted_migration_resumes_with_complete_copies_and_holders() {
        // The mouse is unplugged while slot 2's Hypershift layer is read: nothing incomplete may
        // be stored, and the next run copies slot 2 in full and keeps the slot holders.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let g = gone.clone();
        let tripped = std::sync::atomic::AtomicBool::new(false);
        let mid = *lib::factory_table("00aa", 1).keys().nth(3).unwrap();
        let last = *lib::factory_table("00aa", 1).keys().last().unwrap();
        let hook: Hook = Arc::new(move |cmd, a| {
            if cmd.class == 0x02 && cmd.id == 0x8c && a.first() == Some(&2) && a.get(1) == Some(&mid) && a.get(2) == Some(&1) && !tripped.swap(true, std::sync::atomic::Ordering::SeqCst) {
                g.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            g.load(std::sync::atomic::Ordering::SeqCst).then(unplugged)
        });
        let (mgr, sim) = rig_hooked("00aa", 3, hook);
        sim.lock().unwrap().profiles.get_mut(&2).unwrap().tables[1].insert(last, (1, vec![9]));
        assert!(mgr.library_migrate("00aa").is_err());
        let l = mgr.lib_load("00aa").unwrap();
        if let Some(p) = l.pinned(2) {
            assert!(p.assignments.iter().any(|a| a.layer == 1 && a.key == last), "slot 2 copied without its Hypershift remap: {:?}", p.assignments);
        }
        // Plugged in again.
        gone.store(false, std::sync::atomic::Ordering::SeqCst);
        mgr.counters_of("00aa").unwrap().touch_all();
        mgr.library_migrate("00aa").unwrap();
        let l = mgr.lib_load("00aa").unwrap();
        assert_eq!(l.profiles.len(), 3);
        assert!(l.pinned(2).unwrap().assignments.iter().any(|a| a.layer == 1 && a.key == last), "{:?}", l.pinned(2).unwrap().assignments);
        for s in 1..=3 {
            assert_eq!(l.holder(s).map(|p| p.pinned_slot), Some(Some(s)), "slot {s} holder: {:?}", l.state.holds);
        }
    }

    #[test]
    fn slot_links_are_migrated_on_every_attach() {
        // Links still naming a slot after the first migration (saving failed, or an older
        // client wrote them back) are migrated on the next attach.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("00aa", 2);
        mgr.library_migrate("00aa").unwrap();
        AppLinks { links: vec![app_links::AppLink { name: "G".into(), matches: "g".into(), enabled: true, device_profiles: [("00ab".to_string(), ProfileRef::Slot(2))].into(), ..Default::default() }] }.save(&app_links::path()).unwrap();
        mgr.library_migrate("00aa").unwrap();
        let id = mgr.lib_load("00aa").unwrap().pinned(2).unwrap().id.clone();
        assert_eq!(AppLinks::load(&app_links::path()).unwrap().links[0].device_profiles["00ab"], ProfileRef::Library(id));
    }

    #[test]
    fn failed_switch_keeps_game_mode_on() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let hook: Hook = Arc::new(|cmd, _| (cmd.class == 0x05 && cmd.id == 0x04).then(unplugged));
        let (mgr, sim) = rig_hooked("028d", 2, hook);
        mgr.library_migrate("028d").unwrap();
        mgr.set_game_mode("028d", tsuba_core::config::GameModeConfig { disable_win: true, block_shortcuts: false, active: true, ..Default::default() }).unwrap();
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127].0, 0);
        let other = mgr.lib_load("028d").unwrap().pinned(2).unwrap().id.clone();
        assert!(mgr.library_activate("028d", &other).is_err());
        assert!(mgr.game_mode("028d").active, "Game Mode stays on");
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127].0, 0, "the Win key is off again");
    }

    #[test]
    fn deleting_a_profile_while_it_is_being_activated_is_refused() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let started_tx = Mutex::new(Some(started_tx));
        let hook: Hook = Arc::new(move |cmd, _| {
            if cmd.class == 0x02 && cmd.id == 0x0c {
                if let Some(tx) = started_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            None
        });
        let (mgr, _sim) = rig_hooked("00aa", 1, hook);
        mgr.library_migrate("00aa").unwrap();
        let id = mgr.library_create("00aa", "Slow", "").unwrap();
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("00aa").unwrap();
            let mut p = l.get(&id).unwrap().clone();
            for k in lib::factory_table("00aa", 0).keys().take(4) {
                p.set_key("00aa", 0, *k, 1, &[0x3a]);
            }
            l.put(p).unwrap();
        }
        let m = mgr.clone();
        let i = id.clone();
        let t = std::thread::spawn(move || m.library_activate("00aa", &i));
        started_rx.recv().unwrap();
        let del = mgr.library_delete("00aa", &id);
        t.join().unwrap().unwrap();
        assert!(del.is_err(), "deleted the profile that just became active");
        assert!(mgr.lib_load("00aa").unwrap().get(&id).is_some());
        assert_eq!(list(&mgr, "00aa")["active"], json!(id));
    }

    #[test]
    fn edits_land_in_the_profile_they_were_made_on() {
        // A key edit on slot 1 races a linked game's switch to another profile: the edit belongs
        // to the profile slot 1 held when it was written.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let base = mgr.lib_load("028d").unwrap().profiles[0].id.clone();
        let game = mgr.library_create("028d", "Game", "").unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let m = mgr.clone();
        let edit = std::thread::spawn(move || {
            let held = m.lib_lock.lock().unwrap();
            let x = decode::Assignment { profile: 1, key: 10, layer: 0, function: 2, data: vec![0, 0x04] };
            let back = m.with_device("028d", |dev, model| m.write_assignment("028d", dev, model, &x)).unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(150));
            m.library_mirror(&held, "028d", 1, Mirror::Key { layer: 0, key: 10, function: back.function, data: back.data });
        });
        rx.recv().unwrap();
        mgr.library_activate("028d", &game).unwrap();
        edit.join().unwrap();
        let l = mgr.lib_load("028d").unwrap();
        assert!(l.get(&base).unwrap().assignments.iter().any(|k| k.key == 10), "the edit was made on {base}");
        assert!(!l.get(&game).unwrap().assignments.iter().any(|k| k.key == 10), "saved into the profile switched to afterwards");
    }

    #[test]
    fn device_keys_cannot_leave_the_library_folder() {
        let root = temp_config().join("tsuba").join("profiles");
        for bad in ["../x", "/etc", "..", "00aa/../../x", ""] {
            assert!(DeviceLibrary::load(&root, bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn unreadable_hypershift_layer_fails_the_switch() {
        // The Hypershift table can't be read (cold cache, device gone mid-read): the switch must
        // not report the new profile active while the old profile's Hypershift keys stay.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = fail.clone();
        let hook: Hook = Arc::new(move |cmd, a| (f.load(std::sync::atomic::Ordering::SeqCst) && cmd.class == 0x02 && cmd.id == 0x8d && a.get(2) == Some(&1)).then(unplugged));
        let (mgr, sim) = rig_hooked("028d", 1, hook);
        mgr.library_migrate("028d").unwrap();
        let a = mgr.library_create("028d", "A", "").unwrap();
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("028d").unwrap();
            let mut p = l.get(&a).unwrap().clone();
            p.set_key("028d", 1, 5, 2, &[0, 0x3d]);
            l.put(p).unwrap();
        }
        mgr.library_activate("028d", &a).unwrap();
        let b = mgr.library_create("028d", "B", "").unwrap();
        mgr.counters_of("028d").unwrap().touch_all();
        fail.store(true, std::sync::atomic::Ordering::SeqCst);
        let r = mgr.library_activate("028d", &b);
        fail.store(false, std::sync::atomic::Ordering::SeqCst);
        let key5 = sim.lock().unwrap().profiles[&1].tables[1][&5].clone();
        if r.is_ok() {
            assert_eq!(Some(key5), lib::factory_default("028d", 1, 5), "B reported active with A's Hypershift key");
        }
        assert_eq!(list(&mgr, "028d")["active"], json!(if r.is_ok() { b } else { a }));
    }

    #[test]
    fn a_new_onboard_profile_is_copied_before_anything_is_written_over_it() {
        // Another program (Synapse in a Windows dual boot) added slot 3 and the device runs it;
        // switching to a library profile before tsubad copied slot 3 must not lose it.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("00aa", 2);
        mgr.library_migrate("00aa").unwrap();
        let id = mgr.library_create("00aa", "Game", "").unwrap();
        {
            let mut s = sim.lock().unwrap();
            let mut p = s.profiles[&1].clone();
            p.tables[0].insert(4, (1, vec![0x3a]));
            s.profiles.insert(3, p);
            s.set_name(3, "From Windows");
            s.active = 3;
        }
        mgr.counters_of("00aa").unwrap().touch_all();
        mgr.library_activate("00aa", &id).unwrap();
        let l = mgr.lib_load("00aa").unwrap();
        let copy = l.profiles.iter().find(|p| p.name == "From Windows").expect("slot 3 copied into the library");
        assert!(copy.assignments.iter().any(|a| a.key == 4 && a.data == vec![0x3a]), "{:?}", copy.assignments);
        assert_eq!(l.holder(3).map(|p| p.id.as_str()), Some(id.as_str()));
    }

    // ---- Game Mode around profile changes -------------------------------------------------

    fn game_mode_on(mgr: &Manager, pid: &str, block_shortcuts: bool) {
        mgr.set_game_mode(pid, tsuba_core::config::GameModeConfig { disable_win: true, block_shortcuts, active: true, ..Default::default() }).unwrap();
    }

    fn win_fn(sim: &Sim, slot: u8) -> u8 {
        sim.lock().unwrap().profiles[&slot].tables[0][&127].0
    }

    #[test]
    fn pinning_the_active_profile_keeps_game_mode_keys_off() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let id = mgr.library_create("028d", "Game", "").unwrap();
        mgr.library_activate("028d", &id).unwrap();
        game_mode_on(&mgr, "028d", false);
        assert_eq!(win_fn(&sim, 1), 0);
        // Pinning the active profile moves it into slot 2 and runs it from there.
        assert_eq!(mgr.library_pin("028d", &id, 0).unwrap(), 2);
        assert_eq!(sim.lock().unwrap().active, 2);
        let win = lib::factory_default("028d", 0, 127).unwrap();
        assert_eq!(win_fn(&sim, 2), 0, "Win key on in the active slot while Game Mode is on");
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127], win, "slot 1 got its Win key back");
        let gm = mgr.game_mode("028d");
        assert!(gm.active);
        assert_eq!(gm.saved, vec![(2, 127, win.0, win.1.clone())]);
        // And the library never stores the disabled key.
        assert!(!mgr.lib_load("028d").unwrap().get(&id).unwrap().assignments.iter().any(|a| a.key == 127));
    }

    #[test]
    fn reimporting_the_active_profile_keeps_game_mode_keys_off() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let mut p = LibraryProfile::new("From Synapse");
        p.synapse_guid = Some("syn-gm".into());
        p.set_key("028d", 0, 5, 2, &[0, 0x3a]);
        let id = mgr.library_save("028d", &serde_json::to_string(&p).unwrap()).unwrap();
        mgr.library_activate("028d", &id).unwrap();
        game_mode_on(&mgr, "028d", false);
        // Re-import with a change: the active slot is rewritten (diff) — the Win key stays off.
        p.set_key("028d", 0, 5, 2, &[0, 0x3b]);
        assert_eq!(mgr.library_save("028d", &serde_json::to_string(&p).unwrap()).unwrap(), id);
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&5], (2, vec![0, 0x3b]));
        assert_eq!(win_fn(&sim, 1), 0, "re-import turned the Win key back on");
        assert_eq!(mgr.game_mode("028d").saved.len(), 1);
    }

    #[test]
    fn switching_profiles_never_needs_kde() {
        use crate::gamemode::fake_kde;
        use std::sync::atomic::Ordering::SeqCst;
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 2);
        mgr.library_migrate("028d").unwrap();
        fake_kde::FAIL.store(false, SeqCst);
        game_mode_on(&mgr, "028d", true);
        assert!(fake_kde::BLOCKED.load(SeqCst));
        // KDE goes away (or the call fails): switching still works, and leaves KDE alone.
        fake_kde::FAIL.store(true, SeqCst);
        let calls = fake_kde::CALLS.load(SeqCst);
        let other = mgr.lib_load("028d").unwrap().pinned(2).unwrap().id.clone();
        let r = mgr.library_activate("028d", &other);
        fake_kde::FAIL.store(false, SeqCst);
        r.unwrap();
        assert_eq!(fake_kde::CALLS.load(SeqCst), calls, "a profile switch talked to KDE");
        let active = sim.lock().unwrap().active;
        assert_eq!((active, win_fn(&sim, 2)), (2, 0));
        assert!(mgr.game_mode("028d").active);
        // Toggling Game Mode itself without KDE still switches the keys (desktop part logged).
        fake_kde::FAIL.store(true, SeqCst);
        let r = mgr.set_game_mode("028d", tsuba_core::config::GameModeConfig { active: false, ..mgr.game_mode("028d") });
        fake_kde::FAIL.store(false, SeqCst);
        assert!(!r.unwrap().active);
        assert_ne!(win_fn(&sim, 2), 0);
        fake_kde::block(false).unwrap();
    }

    #[test]
    fn unplug_mid_switch_keeps_game_mode_true_to_the_device() {
        // The keyboard goes away right after Set Active Profile: the keys were given back but
        // can't be turned off again. Game Mode stays on with nothing listed as off, and the
        // keys go off again when the keyboard is back.
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (g, arm) = (gone.clone(), armed.clone());
        let hook: Hook = Arc::new(move |cmd, _| {
            use std::sync::atomic::Ordering::SeqCst;
            if g.load(SeqCst) {
                return Some(unplugged());
            }
            if arm.load(SeqCst) && cmd.class == 0x05 && cmd.id == 0x04 {
                g.store(true, SeqCst);
                return Some(unplugged());
            }
            None
        });
        let (mgr, sim) = rig_hooked("028d", 2, hook);
        mgr.library_migrate("028d").unwrap();
        game_mode_on(&mgr, "028d", false);
        let other = mgr.lib_load("028d").unwrap().pinned(2).unwrap().id.clone();
        armed.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(mgr.library_activate("028d", &other).is_err());
        let gm = mgr.game_mode("028d");
        assert!(gm.active, "Game Mode was switched off by a failed profile switch");
        assert!(gm.saved.is_empty(), "keys listed as off that are on: {:?}", gm.saved);
        assert_ne!(win_fn(&sim, 1), 0);
        // Plugged in again.
        armed.store(false, std::sync::atomic::Ordering::SeqCst);
        gone.store(false, std::sync::atomic::Ordering::SeqCst);
        mgr.counters_of("028d").unwrap().touch_all();
        mgr.game_mode_attached("028d");
        assert_eq!(win_fn(&sim, 1), 0, "Win key off again");
        assert_eq!(mgr.game_mode("028d").saved.len(), 1);
    }

    // ---- pinning failures ------------------------------------------------------------------

    fn sniper(mgr: &Manager) -> String {
        let id = mgr.library_create("00aa", "Sniper", "").unwrap();
        let _g = mgr.lib_lock.lock().unwrap();
        let mut l = mgr.lib_load("00aa").unwrap();
        let mut p = l.get(&id).unwrap().clone();
        p.set_key("00aa", 0, 4, 1, &[5]);
        l.put(p).unwrap();
        id
    }

    #[test]
    fn a_failed_pin_leaves_no_duplicate_when_the_device_goes_away() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let g = gone.clone();
        // Unplugged right after the new slot was created (its name written).
        let hook: Hook = Arc::new(move |cmd, _| {
            use std::sync::atomic::Ordering::SeqCst;
            if g.load(SeqCst) {
                return Some(unplugged());
            }
            if cmd.class == 0x05 && cmd.id == 0x08 {
                g.store(true, SeqCst);
            }
            None
        });
        let (mgr, sim) = rig_hooked("00aa", 1, hook);
        mgr.library_migrate("00aa").unwrap();
        let id = sniper(&mgr);
        assert!(mgr.library_pin("00aa", &id, 0).is_err());
        assert!(sim.lock().unwrap().profiles.contains_key(&2), "slot 2 was created");
        gone.store(false, std::sync::atomic::Ordering::SeqCst);
        mgr.counters_of("00aa").unwrap().touch_all();
        mgr.library_migrate("00aa").unwrap();
        let names: Vec<String> = mgr.lib_load("00aa").unwrap().profiles.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, ["Profile 1", "Sniper"], "copied back into the library");
        // Pinning again uses the slot that is there.
        assert_eq!(mgr.library_pin("00aa", &id, 0).unwrap(), 2);
        assert_eq!(sim.lock().unwrap().profiles[&2].tables[0][&4], (1, vec![5]));
    }

    #[test]
    fn a_failed_pin_removes_the_slot_it_created() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Every assignment write fails: nothing of the profile reaches the new slot.
        let hook: Hook = Arc::new(|cmd, _| (cmd.class == 0x02 && cmd.id == 0x0c).then(unplugged));
        let (mgr, sim) = rig_hooked("00aa", 1, hook);
        mgr.library_migrate("00aa").unwrap();
        let id = sniper(&mgr);
        assert!(mgr.library_pin("00aa", &id, 0).is_err());
        assert!(!sim.lock().unwrap().profiles.contains_key(&2), "slot 2 left behind");
        mgr.counters_of("00aa").unwrap().touch_all();
        mgr.library_migrate("00aa").unwrap();
        let l = mgr.lib_load("00aa").unwrap();
        assert_eq!(l.profiles.len(), 2);
        assert_eq!(l.get(&id).unwrap().pinned_slot, None);
        assert!(!l.state.seen.contains(&2));
    }

    // ---- import -----------------------------------------------------------------------------

    #[test]
    fn importing_an_export_never_duplicates_a_synapse_guid() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("00aa", 1);
        let mut p = LibraryProfile::new("From Synapse");
        p.synapse_guid = Some("syn-exp".into());
        let id = mgr.library_save("00aa", &serde_json::to_string(&p).unwrap()).unwrap();
        let file = temp_config().join("exported.tsuba-profile.json");
        mgr.library_export("00aa", &id, file.to_str().unwrap()).unwrap();
        let copy = mgr.library_import("00aa", file.to_str().unwrap(), "").unwrap();
        let l = mgr.lib_load("00aa").unwrap();
        assert_eq!(l.get(&copy).unwrap().synapse_guid, None, "two profiles with one Synapse GUID");
        assert_eq!(l.by_synapse("syn-exp").unwrap().id, id);
        // Into a library without it (e.g. another computer), the GUID comes along.
        mgr.library_delete("00aa", &id).unwrap();
        let again = mgr.library_import("00aa", file.to_str().unwrap(), "").unwrap();
        assert_eq!(mgr.lib_load("00aa").unwrap().get(&again).unwrap().synapse_guid.as_deref(), Some("syn-exp"));
    }

    // ---- linked games ------------------------------------------------------------------------

    #[test]
    fn set_app_links_maps_slots_and_drops_deleted_profiles() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("00aa", 2);
        mgr.library_migrate("00aa").unwrap();
        let gone = mgr.library_create("00aa", "Gone", "").unwrap();
        let slot2 = mgr.lib_load("00aa").unwrap().pinned(2).unwrap().id.clone();
        let link = |name: &str, r: ProfileRef| app_links::AppLink { name: name.into(), matches: name.into(), enabled: true, device_profiles: [("00aa".to_string(), r)].into(), ..Default::default() };
        mgr.set_app_links(AppLinks { links: vec![link("A", ProfileRef::Library(gone.clone()))] }).unwrap();
        mgr.library_delete("00aa", &gone).unwrap();
        assert!(AppLinks::load(&app_links::path()).unwrap().links[0].device_profiles.is_empty());
        // A client that still has the old list (slot numbers, the deleted profile) writes it back.
        let saved = mgr.set_app_links(AppLinks { links: vec![link("A", ProfileRef::Library(gone.clone())), link("B", ProfileRef::Slot(2)), link("C", ProfileRef::Slot(4))] }).unwrap();
        let on_disk = AppLinks::load(&app_links::path()).unwrap();
        assert_eq!(saved, on_disk);
        assert!(on_disk.links[0].device_profiles.is_empty(), "deleted profile resurrected");
        assert_eq!(on_disk.links[1].device_profiles["00aa"], ProfileRef::Library(slot2));
        assert_eq!(on_disk.links[2].device_profiles["00aa"], ProfileRef::Slot(4), "not migrated yet: kept");
    }

    #[test]
    fn linked_game_state_is_readable_during_a_switch() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let started_tx = Mutex::new(Some(started_tx));
        let hook: Hook = Arc::new(move |cmd, _| {
            if cmd.class == 0x05 && cmd.id == 0x04 {
                if let Some(tx) = started_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                    std::thread::sleep(Duration::from_millis(400));
                }
            }
            None
        });
        let (mgr, sim) = rig_hooked("00aa", 2, hook);
        mgr.library_migrate("00aa").unwrap();
        let slot2 = mgr.lib_load("00aa").unwrap().pinned(2).unwrap().id.clone();
        // This test binary is the "game".
        let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        mgr.set_app_links(AppLinks { links: vec![app_links::AppLink { name: "Me".into(), matches: me, enabled: true, device_profiles: [("00aa".to_string(), ProfileRef::Library(slot2))].into(), ..Default::default() }] }).unwrap();
        let m = mgr.clone();
        let t = std::thread::spawn(move || m.link_tick());
        started_rx.recv().unwrap();
        let t0 = Instant::now();
        assert_eq!(mgr.link_active(), None);
        assert!(t0.elapsed() < Duration::from_millis(200), "waited {:?} for the switch", t0.elapsed());
        t.join().unwrap();
        assert_eq!(mgr.link_active().as_deref(), Some("Me"));
        assert_eq!(sim.lock().unwrap().active, 2);
        mgr.set_app_links(AppLinks::default()).unwrap();
        mgr.link_tick();
        let active = sim.lock().unwrap().active;
        assert_eq!((mgr.link_active(), active), (None, 1));
    }

    // ---- device lock order -------------------------------------------------------------------

    #[test]
    fn write_check_does_not_hold_the_device_map_while_waiting_for_the_device() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("00aa", 1);
        let (busy_tx, busy_rx) = std::sync::mpsc::channel::<()>();
        let m = mgr.clone();
        let holder = std::thread::spawn(move || {
            m.with_device("00aa", |_, _| {
                busy_tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(400));
                Ok(())
            })
        });
        busy_rx.recv().unwrap();
        let m = mgr.clone();
        let checker = std::thread::spawn(move || m.ensure_writable_for("00aa"));
        std::thread::sleep(Duration::from_millis(50));
        let t0 = Instant::now();
        let _ = mgr.list();
        assert!(t0.elapsed() < Duration::from_millis(200), "device map held for {:?}", t0.elapsed());
        holder.join().unwrap().unwrap();
        checker.join().unwrap().unwrap();
    }

    // ---- macro memory ------------------------------------------------------------------------

    #[test]
    fn unused_macros_tsuba_uploaded_are_removed_when_memory_runs_low() {
        use tsuba_core::macros::{Macro, MacroEvent};
        use tsuba_proto::macros as m;
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("00aa", 1);
        mgr.library_migrate("00aa").unwrap();
        let guid = mgr.save_macro(Macro { name: "M1".into(), events: vec![MacroEvent::Key { hid: 4, down: true }, MacroEvent::Key { hid: 4, down: false }], ..Default::default() }).unwrap();
        let key = *lib::factory_table("00aa", 0).keys().nth(2).unwrap();
        let a = mgr.library_create("00aa", "A", "").unwrap();
        {
            let _g = mgr.lib_lock.lock().unwrap();
            let mut l = mgr.lib_load("00aa").unwrap();
            let mut p = l.get(&a).unwrap().clone();
            p.set_macro(MacroBinding { layer: 0, key, guid: guid.clone(), playback: Playback::Once, count: 1 });
            l.put(p).unwrap();
        }
        let b = mgr.library_create("00aa", "B", "").unwrap();
        // A, pinned to slot 2: its macro is uploaded and recorded as Tsuba's.
        assert_eq!(mgr.library_pin("00aa", &a, 0).unwrap(), 2);
        let uploaded: Vec<u16> = mgr.lib_load("00aa").unwrap().state.uploaded_macros.keys().copied().collect();
        assert_eq!(uploaded.len(), 1);
        let mid = uploaded[0];
        // Someone else's macro fills most of the memory and is used nowhere.
        let user = 0x1111u16;
        sim.lock().unwrap().macros.insert(user, sim::SimMacro { name: m::encode_name_block(&m::NameBlock { guid: [0x22; 16], items: 0, name: "User".into() }), data: vec![0; 230_000] });
        mgr.counters_of("00aa").unwrap().touch_all();
        // Memory is low, but slot 2 still binds M1: nothing goes.
        let r = mgr.library_activate("00aa", &b).unwrap();
        assert!(r.macros_removed.is_empty(), "{r:?}");
        assert!(sim.lock().unwrap().macros.contains_key(&mid));
        // Slot 2 is removed: M1 is bound nowhere and goes; the user's macro stays.
        mgr.library_unpin("00aa", &a).unwrap();
        let base = mgr.lib_load("00aa").unwrap().pinned(1).unwrap().id.clone();
        let r = mgr.library_activate("00aa", &base).unwrap();
        assert_eq!(r.macros_removed, ["M1"]);
        let s = sim.lock().unwrap();
        assert!(!s.macros.contains_key(&mid));
        assert!(s.macros.contains_key(&user), "removed a macro Tsuba didn't upload");
        drop(s);
        assert!(mgr.lib_load("00aa").unwrap().state.uploaded_macros.is_empty());
        // Activating A again uploads M1 again.
        mgr.library_activate("00aa", &a).unwrap();
        assert_eq!(mgr.lib_load("00aa").unwrap().state.uploaded_macros.len(), 1);
    }
}
