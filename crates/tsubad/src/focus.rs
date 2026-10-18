//! Window focus for linked games ("only while the game is focused"). On KDE a small KWin
//! script (`contrib/kwin-script`, built into tsubad) reports the focused window to the
//! `FocusChanged` D-Bus method. tsubad loads it into KWin only while some linked game asks for
//! it and unloads it again afterwards. Other desktops (or when KWin refuses the script) give
//! no focus information, and such links simply apply while their game runs.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};
use tsuba_core::app_links::{self, AppLinks};
use zbus::blocking::Connection;

use crate::manager::Manager;

const SCRIPT: &str = include_str!("../../../contrib/kwin-script/contents/code/main.js");
const NAME: &str = "tsuba-focus-daemon";

fn kwin(conn: &Connection, path: &str, iface: &str, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> zbus::Result<zbus::Message> {
    conn.call_method(Some("org.kde.KWin"), path, Some(iface), method, body)
}

/// The script file under the user's runtime directory (created fresh each time).
fn script_path() -> Result<std::path::PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).context("no XDG_RUNTIME_DIR")?;
    let path = dir.join("tsuba-focus.js");
    std::fs::write(&path, SCRIPT)?;
    Ok(path)
}

fn load(conn: &Connection) -> Result<()> {
    let loaded: bool = kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "isScriptLoaded", &(NAME,))?.body().deserialize()?;
    if loaded {
        return Ok(());
    }
    let path = script_path()?;
    let id: i32 = kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path.to_string_lossy().as_ref(), NAME))?.body().deserialize()?;
    anyhow::ensure!(id >= 0, "KWin refused the focus script");
    kwin(conn, &format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run", &())?;
    Ok(())
}

fn unload(conn: &Connection) {
    let _ = kwin(conn, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(NAME,));
}

/// Whether any enabled link wants the focus.
fn wanted() -> bool {
    AppLinks::load(&app_links::path()).map(|l| l.links.iter().any(|l| l.enabled && l.only_when_focused)).unwrap_or(false)
}

/// Keep the KWin script loaded exactly while a link wants it.
pub fn spawn(mgr: Arc<Manager>) {
    std::thread::Builder::new()
        .name("focus".into())
        .spawn(move || {
            let Ok(conn) = Connection::session() else { return };
            let mut loaded = false;
            let mut failed = false;
            loop {
                let want = wanted();
                if want && !loaded && !failed {
                    match load(&conn) {
                        Ok(()) => {
                            info!("focus: KWin script loaded");
                            loaded = true;
                        }
                        Err(e) => {
                            // Not KDE, or scripting is off: such links apply while the game runs.
                            warn!("focus: no window information ({e:#}); links apply while their game runs");
                            failed = true;
                        }
                    }
                } else if !want && loaded {
                    unload(&conn);
                    *mgr.actions.focus.lock().unwrap() = None;
                    info!("focus: KWin script unloaded");
                    loaded = false;
                }
                if !want {
                    failed = false;
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .ok();
}

/// On shutdown: leave KWin as it was.
pub fn unload_on_exit() {
    if let Ok(conn) = Connection::session() {
        unload(&conn);
    }
}
