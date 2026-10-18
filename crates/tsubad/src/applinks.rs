//! Linked games: watch running processes and switch device profiles / Chroma lighting while a
//! linked program runs, restoring the previous state when it exits.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};
use tsuba_core::app_links::{self, AppLink, AppLinks, ProfileRef};
use tsuba_core::chroma_profiles as cp;
use tsuba_core::library::{self, DeviceLibrary};
use tsuba_core::{apply, config::EffectConfig};

use crate::manager::Manager;

/// What was active before a link took over.
#[derive(Default)]
pub struct LinkState {
    pub current: Option<String>,
    saved_profiles: BTreeMap<String, ProfileRef>,
    /// Polling rate (Hz) each device had before the link changed it.
    saved_polling: BTreeMap<String, u32>,
    chroma_applied: bool,
}

impl Manager {
    fn link_apply(&self, link: &AppLink, st: &mut LinkState) {
        st.saved_profiles.clear();
        for (pid, want) in &link.device_profiles {
            // The link may name the wired or the wireless twin of the attached device.
            let Some(pid) = self.attached_twin(pid) else { continue };
            let r = self.library_current(&pid).and_then(|before| {
                if before != *want {
                    self.library_switch(&pid, want)?;
                }
                Ok(before)
            });
            match r {
                Ok(before) => {
                    st.saved_profiles.insert(pid.clone(), before);
                }
                Err(e) => warn!("{pid}: linked profile {want:?}: {e:#}"),
            }
        }
        st.saved_polling.clear();
        for (pid, hz) in &link.polling_hz {
            let Some(pid) = self.attached_twin(pid) else { continue };
            match self.set_polling(&pid, *hz) {
                Ok(Some(before)) => {
                    st.saved_polling.insert(pid, before);
                }
                Ok(None) => {}
                Err(e) => warn!("{pid}: linked polling rate {hz} Hz: {e:#}"),
            }
        }
        st.chroma_applied = false;
        if let Some(name) = &link.chroma {
            match self.chroma_live(name) {
                Ok(()) => st.chroma_applied = true,
                Err(e) => warn!("linked lighting profile {name:?}: {e:#}"),
            }
        }
    }

    fn link_restore(&self, st: &mut LinkState) {
        for (pid, before) in std::mem::take(&mut st.saved_profiles) {
            let r = self.library_current(&pid).and_then(|now| if now != before { self.library_switch(&pid, &before) } else { Ok(()) });
            if let Err(e) = r {
                warn!("{pid}: restoring profile {before:?}: {e:#}");
            }
        }
        for (pid, hz) in std::mem::take(&mut st.saved_polling) {
            if let Err(e) = self.set_polling(&pid, hz) {
                warn!("{pid}: restoring polling rate {hz} Hz: {e:#}");
            }
        }
        if std::mem::take(&mut st.chroma_applied) {
            let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
            for pid in pids {
                self.restore_saved_lighting(&pid);
            }
        }
    }

    /// Set a device's polling rate; returns the rate it had before, or None when it already
    /// had `hz`.
    fn set_polling(&self, pid: &str, hz: u32) -> Result<Option<u32>> {
        self.ensure_writable_for(pid)?;
        self.with_device(pid, |dev, model| {
            let cur = tsuba_core::performance::read(dev, model, 1)?.polling_hz.context("this device has no polling rate setting")?;
            if cur == hz {
                return Ok(None);
            }
            tsuba_core::performance::apply(dev, model, &tsuba_core::performance::Performance { polling_hz: Some(hz), ..Default::default() })?;
            Ok(Some(cur))
        })
    }

    /// The window with the keyboard focus changed (reported by the KWin script): links that
    /// only apply while their game has the focus follow it at once.
    pub fn focus_changed(self: &Arc<Self>, info: app_links::FocusInfo) {
        *self.actions.focus.lock().unwrap() = Some(info);
        if self.read_only {
            return;
        }
        let m = self.clone();
        std::thread::spawn(move || m.link_tick());
    }

    /// `pid` if attached, else its attached wired/wireless twin.
    fn attached_twin(&self, pid: &str) -> Option<String> {
        let devices = self.devices.lock().unwrap();
        if devices.contains_key(pid) {
            return Some(pid.to_string());
        }
        let key = tsuba_core::library::library_key(pid);
        devices.keys().find(|k| tsuba_core::library::library_key(k) == key).cloned()
    }

    /// Run a Chroma profile on the devices without making it the startup lighting.
    fn chroma_live(&self, name: &str) -> Result<()> {
        let lib = cp::ChromaProfiles::load(&cp::path())?;
        let prof = lib.get(name).cloned().context("no such lighting profile")?;
        for (pid, studio) in prof.expanded() {
            if self.devices.lock().unwrap().contains_key(&pid) && !studio.layers.is_empty() {
                self.start_studio(&pid, studio)?;
            }
        }
        Ok(())
    }

    /// Put a device back on its saved (startup) lighting.
    pub fn restore_saved_lighting(&self, pid: &str) {
        let lighting = self.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone());
        let Some(l) = lighting else { return };
        // A game is lighting the device: restore this once it lets go.
        if self.game_defer(pid, crate::manager::GameRestore::Saved) {
            return;
        }
        if let EffectConfig::Studio { studio } = l.effect.clone() {
            if let Err(e) = self.start_studio(pid, studio) {
                warn!("{pid}: restoring lighting: {e:#}");
            }
        } else {
            self.stop_studio(pid);
            if let Err(e) = self.with_device(pid, |dev, _| apply::apply_lighting(dev, &l)) {
                warn!("{pid}: restoring lighting: {e:#}");
            }
        }
    }

    /// One watcher step: switch to / away from linked programs.
    pub fn link_tick(&self) {
        if self.read_only {
            return;
        }
        let links = AppLinks::load(&app_links::path()).unwrap_or_default();
        let procs = app_links::processes();
        let focus = self.actions.focus.lock().unwrap().clone();
        let want = links.active_focused(&procs, focus.as_ref()).cloned();
        let key = want.as_ref().map(|l| if l.name.is_empty() { l.matches.clone() } else { l.name.clone() });
        // Only this watcher uses `link_state`, so holding it across the (slow) profile switch
        // blocks nobody; D-Bus readers see `link_active`, updated once the switch is done.
        let mut st = self.link_state.lock().unwrap();
        if st.current == key {
            return;
        }
        if let Some(old) = st.current.take() {
            info!("linked game closed: {old}");
            self.link_restore(&mut st);
        }
        if let (Some(link), Some(k)) = (want, key) {
            info!("linked game running: {k}");
            self.link_apply(&link, &mut st);
            st.current = Some(k);
        }
        *self.link_active.lock().unwrap() = st.current.clone();
    }

    /// The linked game currently applied (for GetAppLinks; never waits for a switch).
    pub fn link_active(&self) -> Option<String> {
        self.link_active.lock().unwrap().clone()
    }

    // ---- app-links.json: one writer -----------------------------------------------------

    /// Load → change → save `app-links.json` under `links_lock`, the daemon's only way of
    /// writing it, so no writer undoes another's change. `f` returns whether it changed
    /// anything (nothing is written otherwise).
    pub(crate) fn update_app_links(&self, f: impl FnOnce(&mut AppLinks) -> bool) -> Result<bool> {
        let _g = self.links_lock.lock().unwrap();
        let path = app_links::path();
        let mut links = AppLinks::load(&path)?;
        if !f(&mut links) {
            return Ok(false);
        }
        links.save(&path).with_context(|| format!("saving {}", path.display()))?;
        Ok(true)
    }

    /// Replace the linked games list (SetAppLinks), normalised first: on-board slot numbers
    /// become the library profiles pinned to / held by those slots, and references to profiles
    /// no longer in the library are dropped. Returns what was saved.
    pub fn set_app_links(&self, mut links: AppLinks) -> Result<AppLinks> {
        let _g = self.links_lock.lock().unwrap();
        self.normalize_links(&mut links);
        let path = app_links::path();
        links.save(&path).with_context(|| format!("saving {}", path.display()))?;
        Ok(links)
    }

    /// Bring the saved list up to date with the profile libraries (see [`Self::set_app_links`]).
    pub fn tidy_app_links(&self) {
        if let Err(e) = self.update_app_links(|links| {
            let before = links.clone();
            self.normalize_links(links);
            *links != before
        }) {
            warn!("linked games: {e:#}");
        }
    }

    /// Slot references → library ids where a library profile is pinned to / holds that slot;
    /// library references to profiles that no longer exist are removed. A reference whose
    /// device library can't be read completely is left as it is.
    fn normalize_links(&self, links: &mut AppLinks) {
        let mut libs: HashMap<String, Option<DeviceLibrary>> = HashMap::new();
        for link in &mut links.links {
            let name = if link.name.is_empty() { link.matches.clone() } else { link.name.clone() };
            link.device_profiles.retain(|pid, r| {
                let l = libs.entry(library::library_key(pid)).or_insert_with(|| DeviceLibrary::load(&library::root(), pid).ok().filter(|l| l.errors.is_empty()));
                let Some(l) = l.as_ref() else { return true };
                match r {
                    ProfileRef::Slot(s) => {
                        if let Some(p) = l.pinned(*s).or_else(|| l.holder(*s)) {
                            info!("linked game {name:?}: {pid} profile {s} → library profile {:?}", p.name);
                            *r = ProfileRef::Library(p.id.clone());
                        }
                        true
                    }
                    ProfileRef::Library(id) => {
                        let keep = l.get(id).is_some();
                        if !keep {
                            info!("linked game {name:?}: {pid} profile {id} is no longer in the library");
                        }
                        keep
                    }
                }
            });
        }
    }
}

pub fn spawn(mgr: Arc<Manager>) {
    std::thread::Builder::new()
        .name("app-links".into())
        .spawn(move || loop {
            mgr.link_tick();
            std::thread::sleep(Duration::from_secs(2));
        })
        .ok();
}
