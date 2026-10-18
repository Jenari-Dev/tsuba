//! D-Bus API of the profile / macro / shortcut work: interface `io.github.JenAriDev.Tsuba1.Library`
//! at the same object path as the main interface. Every argument is a string (JSON where
//! structured) and so is every reply, so the app can call all of it through one generic caller.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tsuba_core::app_links::FocusInfo;
use tsuba_core::text_layout::{self, Layout, LayoutName};
use zbus::{fdo, interface};

use crate::manager::Manager;

pub struct LibraryApi {
    pub mgr: Arc<Manager>,
}

fn err(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{e:#}"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> fdo::Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| fdo::Error::Failed(e.to_string()))?.map_err(err)
}

fn invalid(e: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::InvalidArgs(e.to_string())
}

/// The layout compiled last (compiling one takes a few milliseconds).
static LAYOUT_CACHE: Mutex<Option<Layout>> = Mutex::new(None);

/// Key events typing `text` on the layout `want` ("" = the desktop's first layout):
/// `{"events", "skipped", "layout"}`.
pub fn text_events(text: &str, delay_ms: u32, want: &str) -> anyhow::Result<Value> {
    let name = if want.is_empty() {
        text_layout::configured_layouts().into_iter().next().unwrap_or_default()
    } else {
        let (layout, variant) = want.split_once('(').map_or((want, ""), |(l, v)| (l.trim(), v.trim_end_matches(')').trim()));
        LayoutName { layout: layout.to_string(), variant: variant.to_string() }
    };
    let mut cache = LAYOUT_CACHE.lock().unwrap();
    if cache.as_ref().is_none_or(|l| l.name != name) {
        *cache = Some(Layout::from_names(&name).ok_or_else(|| anyhow::anyhow!("can't read the keyboard layout “{}” (is xkeyboard-config installed?)", name.label()))?);
    }
    let t = cache.as_ref().expect("layout").text_to_events(text, delay_ms);
    Ok(json!({ "events": t.events, "skipped": t.skipped, "layout": name.label() }))
}

/// Whether `sender` (a unique bus name) currently owns the well-known name `name`.
async fn is_owner(conn: &zbus::Connection, sender: Option<&str>, name: &'static str) -> bool {
    let Some(sender) = sender else { return false };
    let Ok(dbus) = fdo::DBusProxy::new(conn).await else { return false };
    let Ok(name) = zbus::names::BusName::try_from(name) else { return false };
    dbus.get_name_owner(name).await.is_ok_and(|o| o.as_str() == sender)
}

#[interface(name = "io.github.JenAriDev.Tsuba1.Library")]
impl LibraryApi {
    /// Which library profiles bind each macro and which bindings lost their macro (JSON).
    async fn macro_usage(&self) -> fdo::Result<String> {
        let m = self.mgr.clone();
        Ok(blocking(move || m.macro_usage_json()).await?.to_string())
    }

    async fn rename_macro(&self, guid: String, name: String) -> fdo::Result<String> {
        self.mgr.ensure_writable().map_err(err)?;
        let m = self.mgr.clone();
        blocking(move || m.rename_macro(&guid, &name).map(|_| String::new())).await
    }

    /// A copy of a macro under a new name; returns its GUID.
    async fn duplicate_macro(&self, guid: String, name: String) -> fdo::Result<String> {
        self.mgr.ensure_writable().map_err(err)?;
        let m = self.mgr.clone();
        blocking(move || m.duplicate_macro(&guid, &name)).await
    }

    /// File a macro under a folder ("" = none).
    async fn set_macro_group(&self, guid: String, group: String) -> fdo::Result<String> {
        self.mgr.ensure_writable().map_err(err)?;
        let m = self.mgr.clone();
        blocking(move || m.set_macro_group(&guid, &group).map(|_| String::new())).await
    }

    /// Lighting that follows a profile: `what` = "current" (what the device shows now), "off"
    /// (the profile carries none) or a Lighting JSON.
    async fn set_profile_lighting(&self, pid: String, id: String, what: String) -> fdo::Result<String> {
        let m = self.mgr.clone();
        blocking(move || m.library_set_lighting(&pid, &id, &what).map(|_| String::new())).await
    }

    /// A new profile holding the factory settings; returns its id.
    async fn create_factory_profile(&self, pid: String, name: String) -> fdo::Result<String> {
        let m = self.mgr.clone();
        blocking(move || m.library_create_factory(&pid, &name)).await
    }

    /// Write profiles (JSON list of ids) into a folder; returns the paths (JSON list).
    async fn export_library_profiles(&self, pid: String, ids: String, dir: String) -> fdo::Result<String> {
        let ids: Vec<String> = serde_json::from_str(&ids).map_err(invalid)?;
        let m = self.mgr.clone();
        Ok(json!(blocking(move || m.library_export_many(&pid, &ids, &dir)).await?).to_string())
    }

    /// The command dial's factory slot table with the slots' current colours (JSON list for SetDial).
    async fn factory_dial(&self, pid: String) -> fdo::Result<String> {
        let m = self.mgr.clone();
        blocking(move || {
            m.require(&pid, |s| s.features.dial, "a command dial")?;
            let current: Vec<tsuba_core::config::DialSlotConfig> = m.with_device(&pid, |dev, _| tsuba_core::state::read_dial(dev))?.into_iter().map(|d| tsuba_core::config::DialSlotConfig { function: d.function, colour: d.colour, enabled: d.enabled }).collect();
            Ok(serde_json::to_string(&tsuba_core::library::factory_dial(Some(&current)))?)
        })
        .await
    }

    /// Audio outputs and inputs for the Audio pickers (JSON `{"sinks": [], "sources": []}`).
    async fn list_audio_devices(&self) -> fdo::Result<String> {
        Ok(blocking(|| Ok(crate::actions::audio_devices_json())).await?.to_string())
    }

    /// Key events that type text on the desktop's keyboard layout (JSON `{"events", "skipped", "layout"}`).
    async fn text_to_events(&self, text: String, delay_ms: String, layout: String) -> fdo::Result<String> {
        let delay = delay_ms.trim().parse::<u32>().unwrap_or(0).min(5000);
        Ok(blocking(move || text_events(&text, delay, &layout)).await?.to_string())
    }

    /// The keyboard layouts the desktop has configured (JSON `{"layouts": [..], "active": ".."}`).
    async fn text_layouts(&self) -> fdo::Result<String> {
        let l = blocking(|| Ok(text_layout::configured_layouts())).await?;
        Ok(json!({ "layouts": l.iter().map(|x| x.label()).collect::<Vec<_>>(), "active": l.first().map(|x| x.label()) }).to_string())
    }

    /// Which global shortcut service runs software actions: "kde", "portal" or "none".
    async fn shortcut_backend(&self) -> String {
        crate::shortcuts::backend_name().to_string()
    }

    /// How binding the shortcuts went: "none", "ready", "pending" (waiting for the desktop's
    /// approval dialog) or "failed".
    async fn shortcut_backend_state(&self) -> String {
        crate::shortcuts::backend_state().to_string()
    }

    /// The focused window changed (called by the KWin script): process id, class, name, title.
    async fn focus_changed(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, #[zbus(connection)] conn: &zbus::Connection, pid: String, class: String, name: String, caption: String) -> fdo::Result<()> {
        // Only KWin (the owner of org.kde.KWin) may report the focus: any other program on
        // the bus could otherwise make a linked game look focused, or not.
        if !is_owner(conn, hdr.sender().map(|s| s.as_str()), "org.kde.KWin").await {
            return Err(fdo::Error::AccessDenied("only KWin may report the focused window".into()));
        }
        let info = FocusInfo { pid: pid.trim().parse().unwrap_or(0), class, name, caption };
        let m = self.mgr.clone();
        tokio::task::spawn_blocking(move || m.focus_changed(info)).await.map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_kwin_may_report_the_focus() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let Ok(mut child) = Command::new("dbus-daemon").args(["--session", "--nofork", "--print-address=1"]).stdout(Stdio::piped()).spawn() else { return };
        let mut addr = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut addr).unwrap();
        let addr = addr.trim().to_string();
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let build = || zbus::connection::Builder::address(addr.as_str()).unwrap();
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let dir = std::env::temp_dir().join(format!("tsuba-focus-sender-{}", std::process::id()));
            let mgr = Arc::new(Manager::new(true, dir.join("config.toml"), tx).unwrap());
            let daemon = build().name("io.github.JenAriDev.TestTsuba").unwrap().serve_at("/t", LibraryApi { mgr: mgr.clone() }).unwrap().build().await.unwrap();
            let kwin = build().name("org.kde.KWin").unwrap().build().await.unwrap();
            let other = build().build().await.unwrap();
            let call = |c: zbus::Connection| async move {
                c.call_method(Some("io.github.JenAriDev.TestTsuba"), "/t", Some("io.github.JenAriDev.Tsuba1.Library"), "FocusChanged", &("4242", "game", "game.exe", "Game")).await
            };
            assert!(call(other).await.is_err(), "another client reported the focus");
            assert!(mgr.actions.focus.lock().unwrap().is_none());
            call(kwin).await.unwrap();
            assert_eq!(mgr.actions.focus.lock().unwrap().as_ref().map(|f| f.pid), Some(4242));
            drop(daemon);
        });
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("tsuba-focus-sender-{}", std::process::id())));
    }

    #[test]
    fn text_events_follow_the_requested_layout() {
        // The layout argument is "name" or "name (variant)", as the app lists them.
        let Ok(us) = text_events("a@", 0, "us") else { return };
        assert_eq!(us["layout"], "us");
        assert_eq!(us["skipped"], "");
        let Ok(de) = text_events("z@", 3, "de") else { return };
        assert_eq!(de["events"][0], json!({ "type": "key", "hid": 0x1c, "down": true }), "Z is on the Y key of a German board");
        assert!(de["events"].as_array().unwrap().contains(&json!({ "type": "delay", "ms": 3 })));
        let emoji = text_events("hi 😀", 0, "us").unwrap();
        assert_eq!(emoji["skipped"], "😀");
        assert!(text_events("x", 0, "nosuchlayout").is_err());
        assert_eq!(text_events("a", 0, "de (nodeadkeys)").unwrap()["layout"], "de (nodeadkeys)");
    }
}
