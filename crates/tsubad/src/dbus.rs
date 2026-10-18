//! D-Bus API on the session bus.
//!
//! Bus name `io.github.JenAriDev.Tsuba`, object `/io/github/JenAriDev/Tsuba`,
//! interface `io.github.JenAriDev.Tsuba1`. Structured data is passed as JSON.

use std::sync::Arc;

use serde_json::json;
use anyhow::Context;
use tsuba_core::{apply, macros, profiles, config::EffectConfig, config::Lighting, percent_to_level};
use tsuba_proto::{chroma, events};
use zbus::{fdo, interface, object_server::SignalEmitter};

use crate::library::Mirror;
use crate::manager::Manager;

pub const BUS_NAME: &str = "io.github.JenAriDev.Tsuba";
pub const PATH: &str = "/io/github/JenAriDev/Tsuba";

pub struct Tsuba {
    pub mgr: Arc<Manager>,
    pub game: Arc<crate::game_lighting::GameLighting>,
}

fn err(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{e:#}"))
}

/// Run blocking device work off the async executor.
fn logged<T>(what: &str, r: fdo::Result<T>) -> fdo::Result<T> {
    match &r {
        Ok(_) => tracing::info!("{what}: ok"),
        Err(e) => tracing::warn!("{what}: {e}"),
    }
    r
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> fdo::Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| fdo::Error::Failed(e.to_string()))?.map_err(err)
}

impl Tsuba {
    /// [`Manager::ensure_writable_for`] off the executor (it waits for the device lock, which
    /// an activation or a macro upload can hold for a while).
    async fn writable(&self, pid: &str) -> fdo::Result<()> {
        let (mgr, pid) = (self.mgr.clone(), pid.to_string());
        blocking(move || mgr.ensure_writable_for(&pid)).await
    }
}

#[interface(name = "io.github.JenAriDev.Tsuba1")]
impl Tsuba {
    /// (pid, name, kind) for every attached device.
    async fn list_devices(&self) -> Vec<(String, String, String)> {
        self.mgr.list()
    }

    /// Full device state as JSON (lighting, DPI, dial, firmware…).
    async fn get_state(&self, pid: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        let s = blocking(move || mgr.state(&pid)).await?;
        Ok(serde_json::to_string(&s).unwrap_or_default())
    }

    /// Every key/button assignment on a profile and layer (0 standard, 1 Hypershift), as JSON.
    async fn get_assignments(&self, pid: String, profile: u8, layer: u8) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.assignments_json(&pid, profile, layer)).await
    }

    /// Write one key/button assignment. JSON: {"profile", "key", "layer", "function", "data": [..]}.
    async fn set_assignment(&self, pid: String, assignment: String) -> fdo::Result<String> {
        self.writable(&pid).await?;
        #[derive(serde::Deserialize)]
        struct A {
            profile: u8,
            key: u8,
            layer: u8,
            function: u8,
            data: Vec<u8>,
        }
        let a: A = serde_json::from_str(&assignment).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let x = tsuba_proto::decode::Assignment { profile: a.profile, key: a.key, layer: a.layer, function: a.function, data: a.data };
        let mgr = self.mgr.clone();
        let what = format!("{pid} key {} layer {} -> fn {} {:02x?}", x.key, x.layer, x.function, x.data);
        match blocking(move || {
            let (p, l, k) = (x.profile, x.layer, x.key);
            // Held across the write and the library update (see `library_mirror`).
            let held = mgr.lib_lock.lock().unwrap();
            let back = mgr.with_device(&pid, |dev, model| mgr.write_assignment(&pid, dev, model, &x))?;
            mgr.drop_action(&pid, p, l, k)?;
            mgr.library_mirror(&held, &pid, p, Mirror::Key { layer: l, key: k, function: back.function, data: back.data.clone() });
            Ok(back)
        })
        .await
        {
            Ok(back) => {
                let d = tsuba_proto::decode::describe(back.function, &back.data);
                tracing::info!("assignment {what}: saved ({d})");
                Ok(d)
            }
            Err(e) => {
                tracing::warn!("assignment {what}: {e}");
                Err(e)
            }
        }
    }

    /// Bind a key to a software action. JSON: `{"profile", "key", "layer", "action": {"type": "launch",
    /// "command"} | {"type": "url", "url"} | {"type": "device", "target", "op": {"op": ..}}}`.
    async fn assign_action(&self, pid: String, binding: String) -> fdo::Result<String> {
        self.writable(&pid).await?;
        #[derive(serde::Deserialize)]
        struct B {
            #[serde(default = "one")]
            profile: u8,
            key: u8,
            layer: u8,
            action: tsuba_core::config::Action,
        }
        fn one() -> u8 {
            1
        }
        let b: B = serde_json::from_str(&binding).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let what = format!("{pid}: action on key {} layer {}", b.key, b.layer);
        logged(
            &what,
            blocking(move || {
                let held = mgr.lib_lock.lock().unwrap();
                let label = mgr.assign_action(&pid, b.profile, b.layer, b.key, b.action.clone())?;
                mgr.library_mirror(&held, &pid, b.profile, Mirror::Action(tsuba_core::library::ActionBinding { layer: b.layer, key: b.key, action: b.action }));
                Ok(label)
            })
            .await,
        )
    }

    /// The device's software actions (code → label) as JSON.
    async fn get_actions(&self, pid: String) -> String {
        let cfg = self.mgr.config.lock().unwrap();
        let list: Vec<serde_json::Value> = cfg
            .device(&pid)
            .map(|d| d.actions.iter().map(|a| serde_json::json!({ "code": a.code, "key": a.key, "layer": a.layer, "profile": a.profile, "label": a.action.label(), "action": a.action })).collect())
            .unwrap_or_default();
        serde_json::Value::from(list).to_string()
    }

    /// Linked games as JSON `{"links": [...], "active": name|null}`.
    async fn get_app_links(&self) -> fdo::Result<String> {
        use tsuba_core::app_links as al;
        let links = blocking(|| al::AppLinks::load(&al::path())).await?;
        let active = self.mgr.link_active();
        Ok(serde_json::json!({ "links": links.links, "active": active }).to_string())
    }

    /// Replace the linked games list (JSON `{"links": [...]}`). On-board slot numbers are
    /// saved as the library profiles pinned to / holding those slots; references to profiles
    /// that are no longer in the library are dropped.
    async fn set_app_links(&self, links: String) -> fdo::Result<()> {
        use tsuba_core::app_links as al;
        let l: al::AppLinks = serde_json::from_str(&links).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        blocking(move || mgr.set_app_links(l).map(|_| ())).await
    }

    /// Names of running programs (incl. Proton/Wine .exe names) for the link picker.
    async fn running_programs(&self) -> Vec<String> {
        use tsuba_core::app_links as al;
        al::program_names(&al::processes())
    }

    /// Battery, sleep timer, low-power threshold, battery lighting cap, serial (what the device has).
    async fn get_power(&self, pid: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.power_json(&pid)).await
    }

    async fn set_power(&self, pid: String, settings: String) -> fdo::Result<Vec<String>> {
        self.writable(&pid).await?;
        let want: tsuba_core::power::Power = serde_json::from_str(&settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: power"), blocking(move || mgr.with_device(&pid, |dev, _| tsuba_core::power::apply(dev, &want))).await)
    }

    /// Named Chroma Studio profiles as JSON `{"profiles": [{"name", "devices": {pid: studio}}], "active"}`.
    async fn get_chroma_profiles(&self) -> fdo::Result<String> {
        use tsuba_core::chroma_profiles as cp;
        let lib = cp::ChromaProfiles::load(&cp::path()).map_err(err)?;
        Ok(serde_json::to_string(&lib).unwrap_or_default())
    }

    /// Create or replace a profile (matched by name). With `apply`, also apply it.
    async fn save_chroma_profile(&self, profile: String, apply: bool) -> fdo::Result<Vec<String>> {
        use tsuba_core::chroma_profiles as cp;
        self.mgr.ensure_writable().map_err(err)?;
        let p: cp::ChromaProfile = serde_json::from_str(&profile).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let name = p.name.clone();
        let mgr = self.mgr.clone();
        logged(
            &format!("lighting profile {name:?}"),
            blocking(move || {
                let mut lib = cp::ChromaProfiles::load(&cp::path())?;
                lib.upsert(p)?;
                lib.save(&cp::path())?;
                if apply {
                    mgr.apply_chroma_profile(&name)
                } else {
                    Ok(vec![])
                }
            })
            .await,
        )
    }

    async fn apply_chroma_profile(&self, name: String) -> fdo::Result<Vec<String>> {
        self.mgr.ensure_writable().map_err(err)?;
        let mgr = self.mgr.clone();
        logged(&format!("apply chroma profile {name:?}"), blocking(move || mgr.apply_chroma_profile(&name)).await)
    }

    async fn rename_chroma_profile(&self, old: String, new: String) -> fdo::Result<()> {
        use tsuba_core::chroma_profiles as cp;
        blocking(move || {
            let mut lib = cp::ChromaProfiles::load(&cp::path())?;
            lib.rename(&old, &new)?;
            lib.save(&cp::path())
        })
        .await
    }

    async fn delete_chroma_profile(&self, name: String) -> fdo::Result<()> {
        use tsuba_core::chroma_profiles as cp;
        blocking(move || {
            let mut lib = cp::ChromaProfiles::load(&cp::path())?;
            lib.delete(&name);
            lib.save(&cp::path())
        })
        .await
    }

    /// Forget the remembered screen for Ambient, so the next start asks which screen to share.
    async fn reset_ambient_screen(&self) -> fdo::Result<()> {
        let p = dirs::config_dir().unwrap_or_default().join("tsuba").join("ambient-token");
        let _ = std::fs::remove_file(p);
        Ok(())
    }

    /// Live coarse screen colours from the Ambient capture ("WxH:rrggbb…"; "" when idle).
    async fn get_ambient_preview(&self) -> String {
        crate::ambient::preview()
    }

    /// Lighting power options (idle / display-off) as JSON.
    async fn get_lighting_power(&self, pid: String) -> String {
        serde_json::to_string(&self.mgr.lighting_power(&pid)).unwrap_or_default()
    }

    async fn set_lighting_power(&self, pid: String, settings: String) -> fdo::Result<()> {
        let p: tsuba_core::config::LightingPower = serde_json::from_str(&settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: lighting power"), blocking(move || mgr.set_lighting_power(&pid, p)).await)
    }

    /// Keyboard Game Mode settings + state as JSON.
    async fn get_game_mode(&self, pid: String) -> String {
        serde_json::to_string(&self.mgr.game_mode(&pid)).unwrap_or_default()
    }

    /// Store Game Mode settings and turn it on/off (`active`). Returns the new state.
    async fn set_game_mode(&self, pid: String, settings: String) -> fdo::Result<String> {
        self.writable(&pid).await?;
        let want: tsuba_core::config::GameModeConfig = serde_json::from_str(&settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let g = logged(&format!("{pid}: game mode"), blocking(move || mgr.set_game_mode(&pid, want)).await)?;
        Ok(serde_json::to_string(&g).unwrap_or_default())
    }

    /// Write the command dial's modes: JSON `[{"function", "colour", "enabled"}, ×8]` in slot order.
    async fn set_dial(&self, pid: String, slots: String) -> fdo::Result<Vec<String>> {
        self.writable(&pid).await?;
        self.mgr.require(&pid, |s| s.features.dial, "command dial").map_err(err)?;
        let slots: Vec<tsuba_core::config::DialSlotConfig> = serde_json::from_str(&slots).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        logged(
            &format!("{pid}: dial"),
            blocking(move || {
                let held = mgr.lib_lock.lock().unwrap();
                let log = mgr.with_device(&pid, |dev, _| apply::apply_dial(dev, &slots))?;
                mgr.save_dial(&pid, slots.clone())?;
                mgr.library_mirror(&held, &pid, 0, Mirror::Dial(slots));
                Ok(log)
            })
            .await,
        )
    }

    /// Make dial slot `slot` (0-based) the active mode.
    async fn set_dial_mode(&self, pid: String, slot: u8) -> fdo::Result<()> {
        self.writable(&pid).await?;
        self.mgr.require(&pid, |s| s.features.dial, "command dial").map_err(err)?;
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: dial mode {slot}"), blocking(move || mgr.with_device(&pid, |dev, _| apply::set_dial_mode(dev, slot as usize))).await)
    }

    /// On-board profiles as JSON `{"max", "active", "profiles": [{"id", "name"}]}`.
    async fn get_profiles(&self, pid: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.profiles_json(&pid)).await
    }

    /// Create a profile (copy_from 0 = start from the device defaults). Returns its id.
    async fn create_profile(&self, pid: String, name: String, copy_from: u8) -> fdo::Result<u8> {
        self.writable(&pid).await?;
        let mgr = self.mgr.clone();
        let what = format!("{pid}: create profile {name:?}");
        let warm = (self.mgr.clone(), pid.clone());
        let r = logged(&what, blocking(move || mgr.with_device(&pid, |dev, model| profiles::create(dev, model, &name, (copy_from > 0).then_some(copy_from)))).await);
        warm.0.request_warm(&warm.1);
        r
    }

    async fn rename_profile(&self, pid: String, id: u8, name: String) -> fdo::Result<()> {
        self.writable(&pid).await?;
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: rename profile {id}"), blocking(move || mgr.with_device(&pid, |dev, _| profiles::rename(dev, id, &name))).await)
    }

    async fn delete_profile(&self, pid: String, id: u8) -> fdo::Result<()> {
        self.writable(&pid).await?;
        let mgr = self.mgr.clone();
        let warm = (self.mgr.clone(), pid.clone());
        let r = logged(&format!("{pid}: delete profile {id}"), blocking(move || mgr.with_device(&pid, |dev, _| profiles::delete(dev, id))).await);
        warm.0.request_warm(&warm.1);
        r
    }

    async fn set_active_profile(&self, pid: String, id: u8) -> fdo::Result<()> {
        self.writable(&pid).await?;
        let mgr = self.mgr.clone();
        let warm = (self.mgr.clone(), pid.clone());
        let r = logged(&format!("{pid}: activate profile {id}"), blocking(move || mgr.with_device(&pid, |dev, _| profiles::set_active(dev, id))).await);
        warm.0.request_warm(&warm.1);
        warm.0.library_changed(&warm.1);
        r
    }

    // ---- profile library ----------------------------------------------------------------

    /// The device's profile library as JSON: `{"pid", "profiles": [{"id", "name", "pinned_slot",
    /// "in_slot", "active", "synapse", "modified"}], "active": id|null, "active_slot",
    /// "max_slots", "working_slot", "slots": [{"slot", "name", "holder", "pinned"}],
    /// "free_slots": [..], "attached"}`. `active` is the library profile the device's active
    /// on-board slot holds (null: an on-board profile that isn't in the library).
    async fn list_library_profiles(&self, pid: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.library_json(&pid)).await
    }

    /// Add a profile (`copy_from` = a library profile id, or "" for factory defaults). Returns its id.
    async fn create_library_profile(&self, pid: String, name: String, copy_from: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: new library profile {name:?}"), blocking(move || mgr.library_create(&pid, &name, &copy_from)).await)
    }

    async fn rename_library_profile(&self, pid: String, id: String, name: String) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: rename library profile"), blocking(move || mgr.library_rename(&pid, &id, &name)).await)
    }

    async fn duplicate_library_profile(&self, pid: String, id: String, name: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: duplicate library profile"), blocking(move || mgr.library_create(&pid, &name, &id)).await)
    }

    /// Delete a profile (unpinned first; the active profile can't be deleted).
    async fn delete_library_profile(&self, pid: String, id: String) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: delete library profile"), blocking(move || mgr.library_delete(&pid, &id)).await)
    }

    /// Make a library profile the active one (switches to its pinned slot, or writes it into
    /// the working slot; only what differs is written). Returns `{"id", "slot", "writes", "ms",
    /// "warnings": [..], "macros_removed": [..] (only when some were removed)}`.
    async fn activate_library_profile(&self, pid: String, id: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        let a = logged(&format!("{pid}: activate library profile {id}"), blocking(move || mgr.library_activate(&pid, &id)).await)?;
        Ok(serde_json::to_string(&a).unwrap_or_default())
    }

    /// Pin a profile to an on-board slot (0 = the lowest free one). Returns the slot.
    async fn pin_library_profile(&self, pid: String, id: String, slot: u8) -> fdo::Result<u8> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: pin library profile {id}"), blocking(move || mgr.library_pin(&pid, &id, slot)).await)
    }

    async fn unpin_library_profile(&self, pid: String, id: String) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: unpin library profile {id}"), blocking(move || mgr.library_unpin(&pid, &id)).await)
    }

    /// Create or update a profile from its JSON form (the importer's path): a profile with the
    /// same `synapse_guid` is updated in place. Returns its id.
    async fn save_library_profile(&self, pid: String, profile: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: save library profile"), blocking(move || mgr.library_save(&pid, &profile)).await)
    }

    /// Write a profile to a JSON file.
    async fn export_library_profile(&self, pid: String, id: String, path: String) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: export library profile"), blocking(move || mgr.library_export(&pid, &id, &path)).await)
    }

    /// What a profile file would bring along that runs on this computer (JSON, see the
    /// daemon's `library_import_preview`): the app lists it for the user's approval.
    async fn get_library_import_preview(&self, pid: String, path: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        Ok(blocking(move || mgr.library_import_preview(&pid, &path)).await?.to_string())
    }

    /// Add a profile from a file written by ExportLibraryProfile. Returns its id. Commands,
    /// URLs and typed text in the file are left out unless `approved` is the digest the
    /// preview gave for them.
    async fn import_library_profile(&self, pid: String, path: String, approved: String) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged(&format!("{pid}: import library profile"), blocking(move || mgr.library_import(&pid, &path, &approved)).await)
    }

    /// The on-board slot unpinned profiles are written into (0 = whichever slot is active).
    async fn set_library_working_slot(&self, pid: String, slot: u8) -> fdo::Result<()> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.library_set_working_slot(&pid, slot)).await
    }

    /// A device's profile library or its active profile changed.
    #[zbus(signal)]
    pub async fn library_changed(emitter: &SignalEmitter<'_>, pid: &str) -> zbus::Result<()>;

    /// The macro library plus every attached device's on-board macros and memory, as JSON
    /// `{"library": [..], "devices": {pid: {..}}}`.
    async fn get_macros(&self) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        let v = blocking(move || mgr.macros_json()).await?;
        Ok(v.to_string())
    }

    /// Create or update a library macro (`{"guid"?, "name", "events": [..]}`); devices that
    /// already hold it get the new version under the same id. Returns the GUID.
    async fn save_macro(&self, mac: String) -> fdo::Result<String> {
        self.mgr.ensure_writable().map_err(err)?;
        let mac: macros::Macro = serde_json::from_str(&mac).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        logged("save macro", blocking(move || mgr.save_macro(mac)).await)
    }

    /// Remove a macro from the library and from every device (refused while a key uses it).
    async fn delete_macro(&self, guid: String) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        let mgr = self.mgr.clone();
        logged("delete macro", blocking(move || mgr.delete_macro(&guid)).await)
    }

    /// Copy an on-board macro (e.g. one Synapse wrote) into the library. Returns its GUID.
    async fn import_macro(&self, pid: String, id: u16) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        logged("import macro", blocking(move || mgr.import_macro(&pid, id)).await)
    }

    /// Bind a key/button to a library macro, uploading it first if needed.
    /// JSON: `{"profile", "key", "layer", "guid", "playback": "once|times|while_held|toggle", "count"}`.
    async fn assign_macro(&self, pid: String, binding: String) -> fdo::Result<String> {
        self.writable(&pid).await?;
        #[derive(serde::Deserialize)]
        struct B {
            #[serde(default = "one")]
            profile: u8,
            key: u8,
            layer: u8,
            guid: String,
            playback: macros::Playback,
            #[serde(default = "one")]
            count: u8,
        }
        fn one() -> u8 {
            1
        }
        let b: B = serde_json::from_str(&binding).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let what = format!("bind {pid} key {} layer {} to macro {}", b.key, b.layer, b.guid);
        logged(
            &what,
            blocking(move || {
                let mac = mgr.library()?.get(&b.guid).cloned().context("no such macro in the library")?;
                let held = mgr.lib_lock.lock().unwrap();
                mgr.with_device(&pid, |dev, model| {
                    let id = macros::ensure_on_device(dev, &mac)?;
                    let (function, data) = macros::binding(id, b.playback, b.count);
                    let x = tsuba_proto::decode::Assignment { profile: b.profile, key: b.key, layer: b.layer, function, data };
                    let back = mgr.write_assignment(&pid, dev, model, &x)?;
                    Ok(format!("{} ({})", mac.name, tsuba_proto::decode::describe(back.function, &back.data)))
                })
                .and_then(|r| {
                    mgr.drop_action(&pid, b.profile, b.layer, b.key)?;
                    mgr.library_mirror(&held, &pid, b.profile, Mirror::Macro(tsuba_core::library::MacroBinding { layer: b.layer, key: b.key, guid: b.guid.clone(), playback: b.playback, count: b.count }));
                    Ok(r)
                })
            })
            .await,
        )
    }

    /// Performance settings (polling rate, scroll wheel, DPI stages) as JSON; unsupported ones are omitted.
    async fn get_performance(&self, pid: String, profile: u8) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.performance_json(&pid, profile)).await
    }

    /// Apply the fields present in `settings` (same JSON shape as GetPerformance). Returns what changed.
    async fn set_performance(&self, pid: String, settings: String) -> fdo::Result<Vec<String>> {
        self.writable(&pid).await?;
        let want: tsuba_core::performance::Performance =
            serde_json::from_str(&settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let res = blocking(move || {
            let held = mgr.lib_lock.lock().unwrap();
            let log = mgr.with_device(&pid, |dev, model| tsuba_core::performance::apply(dev, model, &want))?;
            mgr.library_mirror(&held, &pid, want.profile.unwrap_or(1), Mirror::Perf(want.clone()));
            // tsubad re-applies the saved DPI to profile 1 at startup, so keep that copy current.
            if let (Some(stages), 1) = (want.dpi_stages.clone(), want.profile.unwrap_or(1)) {
                let active_stage = want.dpi_active_stage.unwrap_or(1).clamp(1, stages.len().max(1) as u8);
                mgr.save_dpi(&pid, tsuba_core::config::DpiConfig { stages, active_stage, active: None })?;
            }
            Ok(log)
        })
        .await;
        match &res {
            Ok(log) => tracing::info!("performance: {}", log.join(", ")),
            Err(e) => tracing::warn!("performance: {e}"),
        }
        res
    }

    /// Apply an effect to every region. `effect` is the config JSON form,
    /// e.g. `{"type":"static","colour":"ff0000"}`.
    async fn set_effect(&self, pid: String, effect: String, brightness: u8) -> fdo::Result<Vec<String>> {
        self.writable(&pid).await?;
        let effect: EffectConfig = serde_json::from_str(&effect).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let brightness = (brightness <= 100).then_some(brightness);
        blocking(move || {
            if let EffectConfig::Studio { studio } = effect {
                mgr.start_studio(&pid, studio)?;
                if let Some(p) = brightness {
                    mgr.with_device(&pid, |dev, _| {
                        for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                            dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, percent_to_level(p)))?;
                        }
                        Ok(())
                    })?;
                }
                return Ok(vec!["chroma studio running".into()]);
            }
            // A game is lighting the device: this becomes the lighting it gets back afterwards.
            if mgr.game_defer(&pid, crate::manager::GameRestore::Lighting(Lighting { effect: effect.clone(), brightness })) {
                return Ok(vec!["a game is controlling the lighting; this effect applies when it stops".into()]);
            }
            mgr.stop_studio(&pid);
            mgr.with_device(&pid, |dev, _| apply::apply_lighting(dev, &Lighting { effect, brightness }))
        })
        .await
    }

    /// Game lighting on/off (the local REST server games that support Razer Chroma use).
    async fn get_game_lighting(&self) -> bool {
        self.mgr.config.lock().unwrap().game_lighting_enabled()
    }

    /// Turn game lighting on or off (saved as `game_lighting` in the config file). Turning it
    /// off ends every game session and gives the devices back to the user's lighting.
    async fn set_game_lighting(&self, enabled: bool) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        let (mgr, game) = (self.mgr.clone(), self.game.clone());
        logged(
            &format!("game lighting = {enabled}"),
            blocking(move || {
                game.set_enabled(enabled)?;
                mgr.save_game_lighting(Some(enabled), None)
            })
            .await,
        )
    }

    /// Whether games may also colour keyboard underglow from their Chroma Link / mouse mat /
    /// headset effects (default off).
    async fn get_game_lighting_underglow(&self) -> bool {
        self.mgr.config.lock().unwrap().game_lighting_underglow_enabled()
    }

    async fn set_game_lighting_underglow(&self, enabled: bool) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        self.game.underglow.store(enabled, std::sync::atomic::Ordering::Relaxed);
        let mgr = self.mgr.clone();
        blocking(move || mgr.save_game_lighting(None, Some(enabled))).await
    }

    /// Whether game lighting also answers web pages (browser-based lighting apps; default off).
    /// Off: requests carrying an `Origin` / `Sec-Fetch-Site` header get 403 and no CORS headers.
    async fn get_game_lighting_web(&self) -> bool {
        self.game.web.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Allow or refuse web pages (saved as `game_lighting_web` in the config file). Takes effect
    /// for the next request; games and other native clients are not affected.
    async fn set_game_lighting_web(&self, enabled: bool) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        let (mgr, game) = (self.mgr.clone(), self.game.clone());
        logged(
            &format!("game lighting web = {enabled}"),
            blocking(move || {
                mgr.save_game_lighting_web(enabled)?;
                game.web.store(enabled, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            })
            .await,
        )
    }

    /// Game lighting status as JSON: `{"enabled", "port", "sessions": [{"id", "title",
    /// "description", "author", "contact", "category", "device_supported", "since" (unix seconds), "targets": ["keyboard", ...],
    /// "devices_active": [pid, ...]}]}`. `devices_active` lists the devices that session
    /// is lighting right now.
    async fn get_game_lighting_status(&self) -> String {
        let game = self.game.clone();
        tokio::task::spawn_blocking(move || game.status().to_string()).await.unwrap_or_default()
    }

    /// The game lighting status changed (same JSON as GetGameLightingStatus).
    #[zbus(signal)]
    pub async fn game_lighting_changed(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;

    /// Set brightness (percent) on every region.
    async fn set_brightness(&self, pid: String, percent: u8) -> fdo::Result<()> {
        self.writable(&pid).await?;
        let mgr = self.mgr.clone();
        blocking(move || {
            mgr.with_device(&pid, |dev, _| {
                for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                    dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, percent_to_level(percent)))?;
                }
                Ok(())
            })
        })
        .await
    }

    /// Set the active DPI (mice).
    async fn set_dpi(&self, pid: String, x: u16, y: u16) -> fdo::Result<()> {
        self.writable(&pid).await?;
        self.mgr
            .require(&pid, |s| s.kind == tsuba_hid::Kind::Mouse && s.max_dpi.is_none_or(|m| x as u32 <= m && y as u32 <= m), "DPI setting in that range")
            .map_err(err)?;
        let mgr = self.mgr.clone();
        blocking(move || {
            mgr.with_device(&pid, |dev, _| {
                use tsuba_proto::{commands as c, decode};
                dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(decode::Dpi { x, y }))?;
                Ok(())
            })
        })
        .await
    }

    /// Save one device's lighting (effect JSON in config form, brightness percent or 255 = keep).
    async fn save_lighting(&self, pid: String, effect: String, brightness: u8) -> fdo::Result<()> {
        let effect: EffectConfig = serde_json::from_str(&effect).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let mgr = self.mgr.clone();
        let brightness = (brightness <= 100).then_some(brightness);
        blocking(move || mgr.save_lighting(&pid, Lighting { effect, brightness })).await
    }

    /// One device's saved settings as JSON.
    async fn get_config(&self, pid: String) -> String {
        self.mgr.config_json(&pid)
    }

    /// Save every attached device's current state into the config file.
    async fn save_from_devices(&self) -> fdo::Result<Vec<String>> {
        let mgr = self.mgr.clone();
        blocking(move || mgr.save_from_devices()).await
    }

    /// Re-read the config file and apply it.
    async fn reload(&self) -> fdo::Result<Vec<String>> {
        let mgr = self.mgr.clone();
        let r = blocking({
            let mgr = mgr.clone();
            move || mgr.reload()
        })
        .await;
        mgr.refresh_cache("");
        // A hand-edited `game_lighting_web` takes effect too.
        let web = mgr.config.lock().unwrap().game_lighting_web_enabled();
        self.game.web.store(web, std::sync::atomic::Ordering::Relaxed);
        r
    }

    /// Forget the cached device reads (assignments, profiles, performance, power, macros) of
    /// one device, or of all with an empty `pid`, and read them again in the background (after a short delay,
    /// so a caller that re-reads at once is served first). Use after something other than Tsuba may have changed the device.
    async fn refresh_cache(&self, pid: String) {
        self.mgr.refresh_cache(&pid);
    }

    /// What Tsuba knows about a device model and whether it may change its settings, as JSON:
    /// `{"pid", "name", "kind", "family", "layout", "tested", "supported", "allow_writes",
    /// "allow_untested_writes", "read_only", "attached", "approximate_art", "txid", "iface",
    /// "event_iface", "protocol", "connection", "pair", "matrix", "leds", "effects", "max_dpi",
    /// "polling_rates", "features": {..}, "notes"}`. Works for any pid in the database.
    async fn get_device_support(&self, pid: String) -> fdo::Result<String> {
        self.mgr.device_support(&pid).map(|v| v.to_string()).map_err(err)
    }

    /// Enable or disable writes for a device model not yet tested with Tsuba (saved in the
    /// config file as `allow_untested_writes`). Returns the new GetDeviceSupport JSON.
    async fn set_allow_untested_writes(&self, pid: String, allow: bool) -> fdo::Result<String> {
        let mgr = self.mgr.clone();
        let v = logged(&format!("{pid}: allow untested writes = {allow}"), blocking(move || mgr.set_allow_untested_writes(&pid, allow)).await)?;
        Ok(v.to_string())
    }

    /// Every model in Tsuba's device database as JSON `[{"pid", "name", "kind", "tested", "supported"}]`.
    async fn list_known_devices(&self) -> String {
        let v: Vec<serde_json::Value> = tsuba_hid::db::all()
            .iter()
            .map(|s| json!({ "pid": tsuba_core::pid_key(s.pid), "name": s.name, "kind": s.kind.as_str(), "tested": s.tested, "supported": s.supported }))
            .collect();
        serde_json::Value::from(v).to_string()
    }

    #[zbus(property)]
    async fn read_only(&self) -> bool {
        self.mgr.read_only
    }

    #[zbus(signal)]
    pub async fn device_added(emitter: &SignalEmitter<'_>, pid: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn device_removed(emitter: &SignalEmitter<'_>, pid: &str) -> zbus::Result<()>;

    /// A decoded hardware event as JSON: `{"kind": "...", ...}`.
    #[zbus(signal)]
    pub async fn hardware_event(emitter: &SignalEmitter<'_>, pid: &str, event: &str) -> zbus::Result<()>;
}

pub fn event_json(ev: &events::Event) -> String {
    let v = match ev {
        events::Event::Media(u) => json!({ "kind": "media", "usages": u }),
        events::Event::Power(p) => json!({ "kind": "power", "data": p }),
        events::Event::RazerKeys(k) => json!({
            "kind": "razer_keys",
            "keys": k.iter().map(|c| json!({ "code": c, "name": events::razer_key_name(*c) })).collect::<Vec<_>>(),
        }),
        events::Event::Notification { id, data } => json!({
            "kind": "notification",
            "id": id,
            "name": events::notification_name(*id),
            "data": data,
        }),
    };
    v.to_string()
}
