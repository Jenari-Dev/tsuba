//! Keyboard Game Mode: turn off the Win/Menu keys in the keyboard (restoring them afterwards)
//! and block KDE's desktop shortcuts (Alt+Tab, Alt+F4, Meta…). Toggled from the app or with
//! the keyboard's Game Mode key (Fn+F10 reports Razer key 3).
//!
//! The two parts are kept apart:
//! - the **device part** (Win/Menu set to Off in the active on-board slot, their bindings kept
//!   in `game_mode.saved`) is what profile switches have to look after
//!   ([`Manager::with_game_mode_keys_given_back`]);
//! - the **desktop part** (KDE's `blockGlobalShortcuts`) is only changed when Game Mode itself
//!   is switched on or off, best effort: without KDE (or if the call fails) the device part
//!   still works and nothing else fails.
//!
//! `game_mode.active` is the user's switch; `game_mode.saved` lists exactly the keys that are
//! off on the device right now. A device that was gone while its keys should have been off
//! gets them turned off again when it comes back ([`Manager::game_mode_attached`]).

use anyhow::{Context, Result};
use tracing::{info, warn};
use tsuba_core::config::GameModeConfig;
use tsuba_core::profiles;
use tsuba_proto::{commands as c, decode};

use crate::manager::Manager;

pub const GAME_MODE_KEY: u8 = 3;
const WIN: u8 = 127;
const MENU: u8 = 129;

/// (profile, key, function, data) of a key Game Mode turned off.
type SavedKey = (u8, u8, u8, Vec<u8>);

#[cfg(not(test))]
fn block_shortcuts(on: bool) -> Result<()> {
    let conn = zbus::blocking::Connection::session()?;
    conn.call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "blockGlobalShortcuts", &(on,))?;
    Ok(())
}

/// Tests never talk to the real desktop (that would block the shortcuts of whoever runs them):
/// a stand-in that records the state and can fail like a desktop without KDE.
#[cfg(test)]
pub(crate) mod fake_kde {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    pub static FAIL: AtomicBool = AtomicBool::new(false);
    pub static BLOCKED: AtomicBool = AtomicBool::new(false);
    pub static CALLS: AtomicUsize = AtomicUsize::new(0);
    pub fn block(on: bool) -> anyhow::Result<()> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        anyhow::ensure!(!FAIL.load(Ordering::SeqCst), "org.kde.kglobalaccel was not provided by any .service files");
        BLOCKED.store(on, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
fn block_shortcuts(on: bool) -> Result<()> {
    fake_kde::block(on)
}

/// The desktop part, best effort: a desktop without KDE only loses the shortcut block.
fn desktop_block(on: bool) {
    if let Err(e) = block_shortcuts(on) {
        warn!("Game Mode: KDE shortcut block {}: {e:#}", if on { "on" } else { "off" });
    }
}

impl Manager {
    pub fn game_mode(&self, pid: &str) -> GameModeConfig {
        self.config.lock().unwrap().device(pid).and_then(|d| d.game_mode.clone()).unwrap_or(GameModeConfig { disable_win: true, block_shortcuts: true, ..Default::default() })
    }

    fn store_game_mode(&self, pid: &str, gm: &GameModeConfig) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        dc.game_mode = Some(gm.clone());
        cfg.upsert(dc);
        cfg.save(&self.config_path)?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// Device part, off: give the keys in `saved` their bindings back. Returns the keys still
    /// off (all from the first failure on) and the error, if any.
    fn gm_give_back(&self, pid: &str, saved: &[SavedKey]) -> (Vec<SavedKey>, Result<()>) {
        if saved.is_empty() {
            return (Vec::new(), Ok(()));
        }
        let mut done = 0;
        let r = self.with_device(pid, |dev, model| {
            for (profile, key, function, data) in saved {
                self.write_assignment(pid, dev, model, &decode::Assignment { profile: *profile, key: *key, layer: 0, function: *function, data: data.clone() })?;
                done += 1;
            }
            Ok(())
        });
        (saved[done..].to_vec(), r)
    }

    /// Device part, on: turn the configured keys off in the active slot. Returns the keys it
    /// turned off (also those done before a failure) and the error, if any.
    fn gm_take_away(&self, pid: &str, gm: &GameModeConfig) -> (Vec<SavedKey>, Result<()>) {
        let keys: Vec<u8> = [(gm.disable_win, WIN), (gm.disable_menu, MENU)].iter().filter(|x| x.0).map(|x| x.1).collect();
        let mut saved = Vec::new();
        if keys.is_empty() {
            return (saved, Ok(()));
        }
        let r = self.with_device(pid, |dev, model| {
            let profile = profiles::active(dev).unwrap_or(1);
            for key in keys {
                let a = decode::assignment(&dev.query(c::GET_SINGLE_KEY_ASSIGNMENT, &[profile, key, 0])?).context("key")?;
                if a.function == 0 {
                    continue;
                }
                self.write_assignment(pid, dev, model, &decode::Assignment { profile, key, layer: 0, function: 0, data: vec![] })?;
                saved.push((profile, key, a.function, a.data.clone()));
            }
            Ok(())
        });
        (saved, r)
    }

    /// Store Game Mode settings and switch it on/off to `want.active`. The device part decides
    /// success; the KDE shortcut block follows best effort (logged when it fails).
    pub fn set_game_mode(&self, pid: &str, mut want: GameModeConfig) -> Result<GameModeConfig> {
        let _g = self.gm_lock.lock().unwrap();
        let cur = self.game_mode(pid);
        // Always undo what's applied, then apply the new settings if active.
        let (still_off, r) = self.gm_give_back(pid, &cur.saved);
        if let Err(e) = r {
            // Nothing else changed: the config keeps listing what is still off.
            if still_off != cur.saved {
                let _ = self.store_game_mode(pid, &GameModeConfig { saved: still_off, ..cur.clone() });
            }
            return Err(e.context("giving back the keys Game Mode turned off"));
        }
        want.saved.clear();
        let mut err = None;
        if want.active {
            let (saved, r) = self.gm_take_away(pid, &want);
            want.saved = saved;
            err = r.err();
        }
        self.store_game_mode(pid, &want)?;
        let blocked = cur.active && cur.block_shortcuts;
        let block = want.active && want.block_shortcuts;
        if blocked != block {
            desktop_block(block);
        }
        if let Some(e) = err {
            return Err(e.context("turning keys off for Game Mode"));
        }
        info!("{pid}: game mode {}", if want.active { "on" } else { "off" });
        Ok(want)
    }

    pub fn toggle_game_mode(&self, pid: &str) {
        let mut g = self.game_mode(pid);
        g.active = !g.active;
        if let Err(e) = self.set_game_mode(pid, g) {
            warn!("{pid}: game mode: {e:#}");
        }
    }

    /// Run `f` — anything that changes the active slot or rewrites a slot (library switch,
    /// pin, re-import) — with Game Mode's keys given back first and turned off again
    /// afterwards in whatever slot is then active, so they follow the profile and the slot
    /// diff never sees them as "off". Only the device part: KDE is never touched, so this
    /// never fails because of the desktop. When the keys can't be given back, `f` doesn't
    /// run. When they can't be turned off again (device unplugged), Game Mode stays on in the
    /// config with `saved` listing only what really is off; the keys are turned off when the
    /// device is back ([`Self::game_mode_attached`]) or on the next switch.
    pub(crate) fn with_game_mode_keys_given_back<T>(&self, pid: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let _g = self.gm_lock.lock().unwrap();
        let gm = self.game_mode(pid);
        if !gm.active {
            return f();
        }
        let (still_off, r) = self.gm_give_back(pid, &gm.saved);
        if let Err(e) = r {
            if still_off != gm.saved {
                if let Err(e) = self.store_game_mode(pid, &GameModeConfig { saved: still_off, ..gm.clone() }) {
                    warn!("{pid}: saving Game Mode state: {e:#}");
                }
            }
            return Err(e.context("giving back the keys Game Mode turned off"));
        }
        let out = f();
        let (saved, r) = self.gm_take_away(pid, &gm);
        if let Err(e) = r {
            warn!("{pid}: Game Mode keys after the profile change: {e:#} (turned off again when the device is back)");
        }
        if let Err(e) = self.store_game_mode(pid, &GameModeConfig { saved, ..gm }) {
            warn!("{pid}: saving Game Mode state: {e:#}");
        }
        out
    }

    /// A device (re)attached: if Game Mode is on but its keys aren't off on the device (it was
    /// unplugged in the middle of a profile switch), turn them off again.
    pub fn game_mode_attached(&self, pid: &str) {
        if self.ensure_writable_for(pid).is_err() || !self.devices.lock().unwrap().get(pid).is_some_and(|a| a.model.kind.has_keys()) {
            return;
        }
        let _g = self.gm_lock.lock().unwrap();
        let gm = self.game_mode(pid);
        if !gm.active || !gm.saved.is_empty() || !(gm.disable_win || gm.disable_menu) {
            return;
        }
        let (saved, r) = self.gm_take_away(pid, &gm);
        if let Err(e) = r {
            warn!("{pid}: Game Mode keys: {e:#}");
        }
        if !saved.is_empty() {
            info!("{pid}: Game Mode keys turned off again");
            if let Err(e) = self.store_game_mode(pid, &GameModeConfig { saved, ..gm }) {
                warn!("{pid}: saving Game Mode state: {e:#}");
            }
        }
    }

    /// At startup: KDE's shortcut block is global state, so make it match the saved setting.
    pub fn restore_shortcut_block(&self) {
        let any = self.config.lock().unwrap().devices.iter().any(|d| d.game_mode.as_ref().is_some_and(|g| g.active && g.block_shortcuts));
        if let Err(e) = block_shortcuts(any) {
            warn!("kglobalaccel: {e:#}");
        }
    }
}

/// On shutdown: never leave the desktop with its shortcuts blocked.
pub fn unblock_on_exit() {
    let _ = block_shortcuts(false);
}
