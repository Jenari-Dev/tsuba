//! KDE global shortcuts for software actions. Each action's key sends an F13–F24 combination
//! (see `config::slot_keys`); we register that combination with kglobalaccel under the "tsuba"
//! component and run the action on its pressed/released signals. Tsuba never reads typed keys.

use std::sync::Arc;

use anyhow::Result;
use tracing::{info, warn};
use tsuba_core::config::{slot_keys, ActionConfig, ACTION_SLOTS};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::MatchRule;

use crate::manager::Manager;

const SERVICE: &str = "org.kde.kglobalaccel";
const COMPONENT: &str = "tsuba";

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

fn action_id(pid: &str, a: &ActionConfig) -> Vec<String> {
    vec![COMPONENT.into(), format!("{pid}-{}", a.code), "Tsuba".into(), a.action.label()]
}

/// Register every shortcut-slot action with kglobalaccel (idempotent).
pub fn register(conn: &Connection, actions: &[(String, ActionConfig)]) -> Result<()> {
    for (pid, a) in actions.iter().filter(|(_, a)| a.code < ACTION_SLOTS) {
        let id = action_id(pid, a);
        conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "doRegister", &(id.clone(),))?;
        // SetPresent (2) | NoAutoloading (4): use our key even if a stale one was saved.
        let reply = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "setShortcut", &(id, vec![qt_key(a.code)], 6u32))?;
        let got: Vec<i32> = reply.body().deserialize()?;
        if got != vec![qt_key(a.code)] {
            warn!("shortcut for {} is taken in KDE (got {got:?})", a.action.label());
        }
    }
    Ok(())
}

/// Register the configured actions, then run them on KDE's pressed/released signals.
pub fn spawn(mgr: Arc<Manager>) {
    std::thread::Builder::new()
        .name("shortcuts".into())
        .spawn(move || {
            if let Err(e) = run(&mgr) {
                warn!("global shortcuts unavailable (KDE kglobalaccel): {e:#}");
            }
        })
        .ok();
}

fn run(mgr: &Arc<Manager>) -> Result<()> {
    let conn = Connection::session()?;
    if let Err(e) = mgr.sync_actions() {
        warn!("syncing action key bindings: {e:#}");
    }
    // Drop registrations left by earlier versions (e.g. the F13/Tools slot KDE uses itself).
    let reply = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "allActionsForComponent", &(vec![COMPONENT, "", "", ""],))?;
    let current: Vec<String> = mgr.all_actions().iter().filter(|(_, a)| a.code < ACTION_SLOTS).map(|(p, a)| format!("{p}-{}", a.code)).collect();
    for id in reply.body().deserialize::<Vec<Vec<String>>>()? {
        if id.len() > 1 && !current.contains(&id[1]) {
            let _ = conn.call_method(Some(SERVICE), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "unregister", &(COMPONENT, id[1].as_str()));
        }
    }
    register(&conn, &mgr.all_actions())?;
    *mgr.shortcut_conn.lock().unwrap() = Some(conn.clone());
    let rule = MatchRule::builder().msg_type(Type::Signal).interface("org.kde.kglobalaccel.Component")?.build();
    info!("listening for Tsuba global shortcuts");
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
        info!("shortcut {unique} {}", if down { "pressed" } else { "released" });
        let Some((pid, code)) = unique.rsplit_once('-').and_then(|(p, c)| Some((p.to_string(), c.parse::<u8>().ok()?))) else { continue };
        let m = mgr.clone();
        std::thread::spawn(move || m.on_action_code(&pid, code, down));
    }
    Ok(())
}
