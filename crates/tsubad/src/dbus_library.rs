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

    /// The focused window changed (called by the KWin script): process id, class, name, title.
    async fn focus_changed(&self, pid: String, class: String, name: String, caption: String) -> fdo::Result<()> {
        let info = FocusInfo { pid: pid.trim().parse().unwrap_or(0), class, name, caption };
        let m = self.mgr.clone();
        tokio::task::spawn_blocking(move || m.focus_changed(info)).await.map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
}
