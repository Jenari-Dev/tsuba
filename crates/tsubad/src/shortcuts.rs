//! Global shortcuts for software actions. Each action's key sends an F14–F24 combination
//! (see `config::slot_keys`); that combination is registered with the desktop's global
//! shortcut service and the action runs on its pressed/released notifications. Tsuba never
//! reads typed keys.
//!
//! Two backends: KDE's kglobalaccel (component "tsuba") and the xdg-desktop-portal
//! GlobalShortcuts interface (GNOME, others — see [`crate::portal_shortcuts`]). KDE is used
//! when it is there; the portal otherwise.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing::{info, warn};
use tsuba_core::config::slot_keys;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::MatchRule;

use crate::manager::Manager;

const SERVICE: &str = "org.kde.kglobalaccel";
const COMPONENT: &str = "tsuba";
/// Id prefix of the shortcuts macros' run-command events use (`macro-<slot>`).
pub const MACRO_ID: &str = "macro";

/// A shortcut slot in use: the id it is registered under (`<pid>-<slot>` / `macro-<slot>`),
/// the slot number and the text the desktop's shortcut settings show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotEntry {
    pub id: String,
    pub code: u8,
    pub label: String,
}

enum Backend {
    Kde(Connection),
    Portal(crate::portal_shortcuts::Portal),
}

static ACTIVE: Mutex<Option<Backend>> = Mutex::new(None);

/// Which backend registers the shortcuts: "kde", "portal" or "none".
pub fn backend_name() -> &'static str {
    match ACTIVE.lock().unwrap().as_ref() {
        Some(Backend::Kde(_)) => "kde",
        Some(Backend::Portal(_)) => "portal",
        None => "none",
    }
}

/// Qt key code (with modifier flags) KDE sees for shortcut slot `slot` under the default XKB
/// layouts (F13 → Tools, F14–F18 → Launch5–9, F19, F24).
fn qt_key(slot: u8) -> i32 {
    let [mods, hid] = slot_keys(slot);
    let mut k = match hid {
        0x69..=0x6d => 0x0100_00a7 + (hid as i32 - 0x69), // Qt::Key_Launch5…9
        0x6e => 0x0100_0042,                        // Qt::Key_F19
        _ => 0x0100_0047,                           // Qt::Key_F24
    };
    if mods & 0x01 != 0 {
        k |= 0x0400_0000; // Ctrl
    }
    if mods & 0x02 != 0 {
        k |= 0x0200_0000; // Shift
    }
    if mods & 0x04 != 0 {
        k |= 0x0800_0000; // Alt
    }
    k
}

fn kde_register(conn: &Connection, entries: &[SlotEntry]) -> Result<()> {
    for e in entries {
        let id = vec![COMPONENT.to_string(), e.id.clone(), "Tsuba".into(), e.label.clone()];
        conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "doRegister", &(id.clone(),))?;
        // SetPresent (2) | NoAutoloading (4): use our key even if a stale one was saved.
        let reply = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "setShortcut", &(id, vec![qt_key(e.code)], 6u32))?;
        let got: Vec<i32> = reply.body().deserialize()?;
        if got != vec![qt_key(e.code)] {
            warn!("shortcut for {} is taken in KDE (got {got:?})", e.label);
        }
    }
    Ok(())
}

/// Register every slot in `entries` with the active backend (idempotent). Without a backend
/// this does nothing: the key still sends its combination, nothing listens for it.
pub fn register(entries: &[SlotEntry]) -> Result<()> {
    match ACTIVE.lock().unwrap().as_ref() {
        Some(Backend::Kde(conn)) => kde_register(conn, entries),
        Some(Backend::Portal(p)) => p.bind(entries),
        None => Ok(()),
    }
}

/// Forget the shortcuts registered under these ids.
pub fn unregister(ids: &[String]) {
    match ACTIVE.lock().unwrap().as_ref() {
        Some(Backend::Kde(conn)) => {
            for id in ids {
                let _ = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "unregister", &(COMPONENT, id.as_str()));
            }
        }
        // The portal has no per-shortcut removal; the next bind replaces the whole list.
        Some(Backend::Portal(_)) | None => {}
    }
}

/// Re-register the current slots and drop registrations nothing uses any more (after macros
/// with run-command events were saved or deleted).
pub fn sync(mgr: &Manager) {
    let entries = mgr.slot_entries();
    if let Some(Backend::Kde(conn)) = ACTIVE.lock().unwrap().as_ref() {
        if let Err(e) = kde_drop_stale(conn, &entries) {
            warn!("shortcuts: {e:#}");
        }
    }
    if let Err(e) = register(&entries) {
        warn!("registering shortcuts: {e:#}");
    }
}

fn kde_drop_stale(conn: &Connection, entries: &[SlotEntry]) -> Result<()> {
    let reply = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "allActionsForComponent", &(vec![COMPONENT, "", "", ""],))?;
    for id in reply.body().deserialize::<Vec<Vec<String>>>()? {
        if id.len() > 1 && !entries.iter().any(|e| e.id == id[1]) {
            let _ = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "unregister", &(COMPONENT, id[1].as_str()));
        }
    }
    Ok(())
}

/// Whether KDE's global shortcut service can be reached on the session bus.
fn kde_available(conn: &Connection) -> bool {
    const DBUS: &str = "org.freedesktop.DBus";
    let owned = conn.call_method(Some(DBUS), "/org/freedesktop/DBus", Some(DBUS), "NameHasOwner", &(SERVICE,)).ok().and_then(|r| r.body().deserialize::<bool>().ok());
    if owned == Some(true) {
        return true;
    }
    conn.call_method(Some(DBUS), "/org/freedesktop/DBus", Some(DBUS), "ListActivatableNames", &()).ok().and_then(|r| r.body().deserialize::<Vec<String>>().ok()).is_some_and(|l| l.iter().any(|n| n == SERVICE))
}

/// Pick the backend, register the configured actions, then run them on the desktop's
/// pressed/released notifications.
pub fn spawn(mgr: Arc<Manager>) {
    std::thread::Builder::new()
        .name("shortcuts".into())
        .spawn(move || {
            if let Err(e) = run(&mgr) {
                warn!("global shortcuts unavailable: {e:#}");
            }
        })
        .ok();
}

pub(crate) fn dispatch(mgr: &Arc<Manager>, unique: &str, down: bool) {
    info!("shortcut {unique} {}", if down { "pressed" } else { "released" });
    let Some((pid, code)) = unique.rsplit_once('-').and_then(|(p, c)| Some((p.to_string(), c.parse::<u8>().ok()?))) else { return };
    let m = mgr.clone();
    std::thread::spawn(move || m.on_action_code(&pid, code, down));
}

fn run(mgr: &Arc<Manager>) -> Result<()> {
    let conn = Connection::session()?;
    if let Err(e) = mgr.sync_actions() {
        warn!("syncing action key bindings: {e:#}");
    }
    if kde_available(&conn) {
        run_kde(mgr, conn)
    } else {
        crate::portal_shortcuts::run(mgr, conn)
    }
}

#[cfg(test)]
pub(crate) fn clear_backend() {
    *ACTIVE.lock().unwrap() = None;
}

/// The portal backend publishes itself as the active one.
pub(crate) fn set_portal(p: crate::portal_shortcuts::Portal) {
    *ACTIVE.lock().unwrap() = Some(Backend::Portal(p));
}

fn run_kde(mgr: &Arc<Manager>, conn: Connection) -> Result<()> {
    let entries = mgr.slot_entries();
    // Drop registrations left by earlier versions (e.g. the F13/Tools slot KDE uses itself).
    kde_drop_stale(&conn, &entries)?;
    kde_register(&conn, &entries)?;
    *ACTIVE.lock().unwrap() = Some(Backend::Kde(conn.clone()));
    let rule = MatchRule::builder().msg_type(Type::Signal).interface("org.kde.kglobalaccel.Component")?.build();
    info!("listening for Tsuba global shortcuts (KDE)");
    for msg in MessageIterator::for_match_rule(rule, &conn, None)? {
        let Ok(msg) = msg else { continue };
        let h = msg.header();
        let down = match h.member().map(|m| m.as_str()) {
            Some("globalShortcutPressed") => true,
            Some("globalShortcutReleased") => false,
            _ => continue,
        };
        let Ok((component, unique, _ts)) = msg.body().deserialize::<(String, String, i64)>() else { continue };
        if component != COMPONENT {
            continue;
        }
        dispatch(mgr, &unique, down);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_map_to_qt_keys() {
        assert_eq!(qt_key(0), 0x0100_00a7); // F14 = Launch5
        assert_eq!(qt_key(7), 0x0100_00a7 | 0x0200_0000); // Shift+F14
        assert_eq!(qt_key(6), 0x0100_0047, "F24");
        assert_eq!(qt_key(5), 0x0100_0042, "F19");
        assert_eq!(qt_key(28), 0x0100_00a7 | 0x0400_0000, "Ctrl+F14");
    }
}
