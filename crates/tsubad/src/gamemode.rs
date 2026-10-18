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
    /// Tests that touch the fake desktop run one at a time.
    pub static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    /// The desktop's global shortcuts for the per-combination block.
    pub static SHORTCUTS: std::sync::Mutex<Vec<(String, String, Vec<i32>)>> = std::sync::Mutex::new(Vec::new());
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

/// A global shortcut as the desktop lists it: (component, action, key codes).
type ShortcutRow = (String, String, Vec<i32>);

#[cfg(not(test))]
fn kde_shortcuts() -> Result<Vec<ShortcutRow>> {
    type Info = (String, String, String, String, String, String, Vec<i32>, Vec<i32>);
    let conn = zbus::blocking::Connection::session()?;
    let comps: Vec<zbus::zvariant::OwnedObjectPath> = conn.call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "allComponents", &())?.body().deserialize()?;
    let mut out = Vec::new();
    for c in comps {
        let Ok(reply) = conn.call_method(Some("org.kde.kglobalaccel"), c.as_str(), Some("org.kde.kglobalaccel.Component"), "allShortcutInfos", &()) else { continue };
        let Ok(infos) = reply.body().deserialize::<Vec<Info>>() else { continue };
        out.extend(infos.into_iter().map(|i| (i.2, i.0, i.6)));
    }
    Ok(out)
}

#[cfg(not(test))]
fn kde_set_shortcut(component: &str, action: &str, keys: &[i32]) -> Result<()> {
    let conn = zbus::blocking::Connection::session()?;
    let id = vec![component.to_string(), action.to_string(), String::new(), String::new()];
    conn.call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "setForeignShortcut", &(id, keys.to_vec()))?;
    Ok(())
}

#[cfg(test)]
fn kde_shortcuts() -> Result<Vec<ShortcutRow>> {
    fake_kde::CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    anyhow::ensure!(!fake_kde::FAIL.load(std::sync::atomic::Ordering::SeqCst), "org.kde.kglobalaccel was not provided by any .service files");
    Ok(fake_kde::SHORTCUTS.lock().unwrap().clone())
}

#[cfg(test)]
fn kde_set_shortcut(component: &str, action: &str, keys: &[i32]) -> Result<()> {
    anyhow::ensure!(!fake_kde::FAIL.load(std::sync::atomic::Ordering::SeqCst), "org.kde.kglobalaccel was not provided by any .service files");
    let mut all = fake_kde::SHORTCUTS.lock().unwrap();
    if let Some(r) = all.iter_mut().find(|r| r.0 == component && r.1 == action) {
        r.2 = keys.to_vec();
    }
    Ok(())
}

/// What Game Mode wants blocked on the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    Off,
    /// KDE's own "block all global shortcuts".
    All,
    /// Only these combinations ([`tsuba_core::config::GAME_MODE_COMBOS`] names, sorted).
    Combos(Vec<String>),
}

pub(crate) fn block_of(gm: &GameModeConfig) -> Block {
    if !(gm.active && gm.block_shortcuts) {
        return Block::Off;
    }
    match &gm.blocked_combos {
        None => Block::All,
        Some(c) if c.is_empty() => Block::Off,
        Some(c) => {
            let mut c = c.clone();
            c.sort();
            c.dedup();
            Block::Combos(c)
        }
    }
}

/// The union of what several devices' Game Modes want (any "all" wins).
fn merge(blocks: impl Iterator<Item = Block>) -> Block {
    let mut combos: Vec<String> = Vec::new();
    for b in blocks {
        match b {
            Block::All => return Block::All,
            Block::Combos(c) => combos.extend(c),
            Block::Off => {}
        }
    }
    combos.sort();
    combos.dedup();
    if combos.is_empty() { Block::Off } else { Block::Combos(combos) }
}

/// Shortcuts the per-combination block took away, kept on disk so a crash or a killed daemon
/// can't leave the desktop without its Alt+Tab (restored at the next start).
fn saved_path() -> std::path::PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join(format!("tsuba-gm-shortcuts-test-{}.json", std::process::id()))
    }
    #[cfg(not(test))]
    {
        dirs::config_dir().unwrap_or_default().join("tsuba").join("game-mode-shortcuts.json")
    }
}

fn load_saved() -> Vec<ShortcutRow> {
    std::fs::read_to_string(saved_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store_saved(rows: &[ShortcutRow]) -> Result<()> {
    let path = saved_path();
    if rows.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    tsuba_core::fsutil::write_atomic(&path, serde_json::to_string(rows)?.as_bytes())
}

/// Give back every shortcut the per-combination block took.
fn restore_saved() {
    let saved = load_saved();
    if saved.is_empty() {
        return;
    }
    let mut left = Vec::new();
    for row in saved {
        if let Err(e) = kde_set_shortcut(&row.0, &row.1, &row.2) {
            warn!("Game Mode: giving {} / {} back: {e:#}", row.0, row.1);
            left.push(row);
        }
    }
    if let Err(e) = store_saved(&left) {
        warn!("Game Mode: saving the shortcut list: {e:#}");
    }
}

/// Take the chosen combinations away from the desktop's global shortcuts.
fn block_combos(combos: &[String]) -> Result<()> {
    let mut saved = load_saved();
    let mut todo = Vec::new();
    for (component, action, keys) in kde_shortcuts()? {
        // Tsuba's own action shortcuts are never blocked.
        if component == "tsuba" {
            continue;
        }
        let keep: Vec<i32> = keys.iter().copied().filter(|k| !tsuba_core::config::combo_blocks(combos, *k)).collect();
        if keep.len() != keys.len() {
            todo.push((component, action, keys, keep));
        }
    }
    // The originals are written down before anything changes.
    saved.extend(todo.iter().map(|(c, a, k, _)| (c.clone(), a.clone(), k.clone())));
    store_saved(&saved)?;
    for (component, action, _, keep) in todo {
        if let Err(e) = kde_set_shortcut(&component, &action, &keep) {
            warn!("Game Mode: blocking {component} / {action}: {e:#}");
        }
    }
    Ok(())
}

/// The desktop part, best effort: a desktop without KDE only loses the shortcut block.
pub(crate) fn desktop_apply(b: &Block) {
    let r = (|| -> Result<()> {
        restore_saved();
        match b {
            Block::Off => block_shortcuts(false),
            Block::All => block_shortcuts(true),
            Block::Combos(c) => {
                block_shortcuts(false)?;
                block_combos(c)
            }
        }
    })();
    if let Err(e) = r {
        warn!("Game Mode: KDE shortcut block ({b:?}): {e:#}");
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
        let was = self.merged_block();
        let mut err = None;
        if want.active {
            let (saved, r) = self.gm_take_away(pid, &want);
            want.saved = saved;
            err = r.err();
        }
        self.store_game_mode(pid, &want)?;
        // KDE's block is global: it follows the merge over every device, not this device alone.
        let now = self.merged_block();
        if was != now {
            desktop_apply(&now);
        }
        if let Some(e) = err {
            return Err(e.context("turning keys off for Game Mode"));
        }
        info!("{pid}: game mode {}", if want.active { "on" } else { "off" });
        Ok(want)
    }

    /// What every device's Game Mode together wants blocked on the desktop.
    pub(crate) fn merged_block(&self) -> Block {
        merge(self.config.lock().unwrap().devices.iter().filter_map(|d| d.game_mode.as_ref()).map(block_of))
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
        desktop_apply(&self.merged_block());
    }
}

/// On shutdown: never leave the desktop with its shortcuts blocked.
pub fn unblock_on_exit() {
    restore_saved();
    let _ = block_shortcuts(false);
}

#[cfg(test)]
mod combo_tests {
    use super::*;
    use std::sync::atomic::Ordering::SeqCst;

    const ALT_TAB: i32 = 0x0900_0001;
    const META_TAB: i32 = 0x1100_0001;
    const ALT_F4: i32 = 0x0900_0033;
    const META: i32 = 0x0100_0022;

    fn desktop() {
        *fake_kde::SHORTCUTS.lock().unwrap() = vec![
            ("kwin".into(), "Walk Through Windows".into(), vec![ALT_TAB, META_TAB]),
            ("kwin".into(), "Window Close".into(), vec![ALT_F4]),
            ("plasmashell".into(), "activate launcher".into(), vec![META]),
            ("kwin".into(), "Overview".into(), vec![0x1000_0057]),
            // Tsuba's own shortcut slots are never touched.
            ("tsuba".into(), "028d-3".into(), vec![0x1000_0057]),
        ];
        fake_kde::FAIL.store(false, SeqCst);
        fake_kde::BLOCKED.store(false, SeqCst);
        let _ = std::fs::remove_file(saved_path());
    }

    fn keys(component: &str, action: &str) -> Vec<i32> {
        fake_kde::SHORTCUTS.lock().unwrap().iter().find(|r| r.0 == component && r.1 == action).unwrap().2.clone()
    }

    fn combos(names: &[&str]) -> Block {
        Block::Combos(names.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn only_the_chosen_combinations_are_taken_and_all_come_back() {
        let _s = fake_kde::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        desktop();
        desktop_apply(&combos(&["alt_tab"]));
        assert_eq!(keys("kwin", "Walk Through Windows"), vec![META_TAB], "Alt+Tab is gone, Meta+Tab stays");
        assert_eq!(keys("kwin", "Window Close"), vec![ALT_F4]);
        assert!(!fake_kde::BLOCKED.load(SeqCst), "no global block for a per-combination block");
        assert!(saved_path().exists());
        // Changing the selection first gives everything back, then takes the new set.
        desktop_apply(&combos(&["alt_f4", "meta"]));
        assert_eq!(keys("kwin", "Walk Through Windows"), vec![ALT_TAB], "Meta+Tab goes with the Windows key");
        assert_eq!(keys("kwin", "Window Close"), Vec::<i32>::new());
        assert_eq!(keys("plasmashell", "activate launcher"), Vec::<i32>::new());
        assert_eq!(keys("kwin", "Overview"), Vec::<i32>::new());
        assert_eq!(keys("tsuba", "028d-3"), vec![0x1000_0057], "Tsuba's own shortcuts stay");
        desktop_apply(&Block::Off);
        assert_eq!(keys("kwin", "Walk Through Windows"), vec![ALT_TAB, META_TAB]);
        assert_eq!(keys("kwin", "Window Close"), vec![ALT_F4]);
        assert_eq!(keys("plasmashell", "activate launcher"), vec![META]);
        assert!(!saved_path().exists(), "nothing left to restore");
        // "All" uses KDE's own block and takes no shortcuts away.
        desktop_apply(&Block::All);
        assert!(fake_kde::BLOCKED.load(SeqCst));
        assert_eq!(keys("kwin", "Window Close"), vec![ALT_F4]);
        desktop_apply(&Block::Off);
        assert!(!fake_kde::BLOCKED.load(SeqCst));
    }

    #[test]
    fn a_restart_gives_back_what_a_crashed_daemon_took() {
        let _s = fake_kde::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        desktop();
        desktop_apply(&combos(&["alt_tab", "alt_f4"]));
        assert_eq!(keys("kwin", "Window Close"), Vec::<i32>::new());
        // The daemon died. The next one starts with Game Mode off everywhere.
        unblock_on_exit();
        assert_eq!(keys("kwin", "Window Close"), vec![ALT_F4]);
        assert_eq!(keys("kwin", "Walk Through Windows"), vec![ALT_TAB, META_TAB]);
        assert!(!saved_path().exists());
    }

    #[test]
    fn a_failing_desktop_leaves_the_list_for_later() {
        let _s = fake_kde::SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        desktop();
        desktop_apply(&combos(&["alt_f4"]));
        fake_kde::FAIL.store(true, SeqCst);
        restore_saved();
        assert!(saved_path().exists(), "the entry stays until the desktop answers");
        fake_kde::FAIL.store(false, SeqCst);
        restore_saved();
        assert_eq!(keys("kwin", "Window Close"), vec![ALT_F4]);
        assert!(!saved_path().exists());
    }

    #[test]
    fn what_a_device_wants_blocked() {
        let gm = |active, block, combos: Option<&[&str]>| GameModeConfig { active, block_shortcuts: block, blocked_combos: combos.map(|c| c.iter().map(|s| s.to_string()).collect()), ..Default::default() };
        assert_eq!(block_of(&gm(false, true, None)), Block::Off);
        assert_eq!(block_of(&gm(true, false, None)), Block::Off);
        assert_eq!(block_of(&gm(true, true, None)), Block::All);
        assert_eq!(block_of(&gm(true, true, Some(&[]))), Block::Off, "no combination chosen = nothing to block");
        assert_eq!(block_of(&gm(true, true, Some(&["meta", "alt_tab", "meta"]))), combos(&["alt_tab", "meta"]));
        assert_eq!(merge([combos(&["alt_tab"]), Block::Off, combos(&["alt_f4"])].into_iter()), combos(&["alt_f4", "alt_tab"]));
        assert_eq!(merge([combos(&["alt_tab"]), Block::All].into_iter()), Block::All);
        assert_eq!(merge(std::iter::empty()), Block::Off);
        // The classic setting (no list) is read from old config files unchanged.
        let old: GameModeConfig = serde_json::from_str(r#"{"active": true}"#).unwrap();
        assert_eq!(old.blocked_combos, None);
    }
}
