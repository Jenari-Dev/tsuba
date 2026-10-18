//! "Import from Synapse" for the app: scan a Synapse store into a view for the dialog, and
//! carry out an import through tsubad's D-Bus API. Both run on a worker thread with their
//! own bus connection; the Synapse store itself is only read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};
use tsuba_core::synapse_import::{self as si, Plan};
use zbus::blocking::Connection;

const BUS: &str = "io.github.JenAriDev.Tsuba";
const PATH: &str = "/io/github/JenAriDev/Tsuba";
const IFACE: &str = "io.github.JenAriDev.Tsuba1";

fn call<B, R>(conn: &Connection, method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    let r = conn.call_method(Some(BUS), PATH, Some(IFACE), method, body).and_then(|m| m.body().deserialize());
    r.map_err(|e| match e {
        zbus::Error::MethodError(_, Some(msg), _) => msg,
        e => e.to_string(),
    })
}

/// Stores found on mounted filesystems, as a JSON list of paths.
pub fn stores_json() -> String {
    let v: Vec<String> = si::find_synapse_stores(&[]).iter().map(|p| p.display().to_string()).collect();
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

/// The store at or below `path` (a store, a user folder, a Users folder or a volume).
pub fn resolve(path: &str) -> Option<PathBuf> {
    let p = Path::new(path.strip_prefix("file://").unwrap_or(path));
    si::stores_below(&[p.to_path_buf()]).into_iter().next()
}

/// Where a Synapse device's profiles go: its library (keyed by the attached pid or its
/// wired/wireless twin when attached, else the Synapse device's own pid) and, when attached and
/// writable, how many on-board slots are free for pinning.
struct Target {
    pid: String,
    attached: bool,
    writable: bool,
    /// On-board slots without a pinned profile.
    free_pins: usize,
    /// Synapse GUIDs already in the library.
    in_library: BTreeSet<String>,
}

fn library_info(conn: &Connection, pid: &str) -> (usize, BTreeSet<String>) {
    let v: Value = call::<_, String>(conn, "ListLibraryProfiles", &(pid.to_string(),)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let free = v.get("free_slots").and_then(Value::as_array).map_or(0, |a| a.len());
    let guids = v.get("profiles").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| p.get("synapse_guid").and_then(Value::as_str).map(String::from)).collect()).unwrap_or_default();
    (free, guids)
}

fn targets(conn: Option<&Connection>, plan: &Plan) -> BTreeMap<String, Target> {
    let mut out = BTreeMap::new();
    let Some(conn) = conn else { return out };
    let attached: Vec<(String, String, String)> = call(conn, "ListDevices", &()).unwrap_or_default();
    let read_only = conn
        .call_method(Some(BUS), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(IFACE, "ReadOnly"))
        .ok()
        .and_then(|m| m.body().deserialize::<zbus::zvariant::OwnedValue>().ok())
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(true);
    for d in &plan.devices {
        let Some(pid) = d.pid.clone() else { continue };
        let pair = u16::from_str_radix(&pid, 16).ok().and_then(tsuba_hid::db::get).and_then(|s| s.pair).map(tsuba_core::pid_key);
        let at = [Some(pid.clone()), pair].into_iter().flatten().find(|p| attached.iter().any(|a| &a.0 == p));
        let t = at.clone().unwrap_or_else(|| pid.clone());
        let support: Value = call::<_, String>(conn, "GetDeviceSupport", &(t.clone(),)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
        let writable = !read_only && support.get("allow_writes").and_then(Value::as_bool).unwrap_or(false);
        let (free, guids) = library_info(conn, &t);
        out.insert(pid, Target { pid: t, attached: at.is_some(), writable, free_pins: if at.is_some() && writable { free } else { 0 }, in_library: guids });
    }
    out
}

/// Scan `path` and describe it for the dialog (JSON). Heavy data (macro events, layers) stays
/// out; the import re-reads the store.
pub fn scan_json(path: &str) -> String {
    let t0 = std::time::Instant::now();
    let Some(store) = resolve(path) else {
        return json!({ "error": "No Synapse data was found there. Pick the Windows drive, its Users folder or a user folder." }).to_string();
    };
    let plan = match si::scan(&store) {
        Ok(p) => p,
        Err(e) => return json!({ "error": format!("Couldn't read the Synapse data: {e:#}") }).to_string(),
    };
    let conn = Connection::session().ok();
    let tg = targets(conn.as_ref(), &plan);
    let library: BTreeSet<String> = conn
        .as_ref()
        .and_then(|c| call::<_, String>(c, "GetMacros", &()).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("library").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| m.get("guid").and_then(Value::as_str).map(String::from)).collect()))
        .unwrap_or_default();
    let effects: BTreeSet<String> = conn
        .as_ref()
        .and_then(|c| call::<_, String>(c, "GetChromaProfiles", &()).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("profiles").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from)).collect()))
        .unwrap_or_default();
    let devices: Vec<Value> = plan
        .devices
        .iter()
        .map(|d| {
            let t = d.pid.as_ref().and_then(|p| tg.get(p));
            json!({
                "product_id": d.product_id,
                "name": d.name.trim_start_matches("Razer ").to_string(),
                "pid": d.pid,
                "importable": d.importable,
                "verified": d.verified,
                "note": d.note,
                "attached": t.is_some_and(|t| t.attached),
                "target": t.map(|t| t.pid.clone()),
                "writable": t.is_some_and(|t| t.writable),
                "free_pins": t.map_or(0, |t| t.free_pins),
                "profiles": d.profiles.iter().map(|p| json!({
                    "guid": p.guid,
                    "name": p.name,
                    "active": p.active,
                    "in_library": t.is_some_and(|t| t.in_library.contains(&p.guid)),
                    "summary": p.summary,
                    "device_wide": p.device_wide.summary,
                    "warnings": p.warnings,
                    "macros": p.macros,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let macros: Vec<Value> = plan
        .macros
        .iter()
        .map(|m| json!({ "guid": m.guid, "name": m.name, "events": m.events, "warnings": m.warnings, "in_library": library.contains(&m.guid) }))
        .collect();
    let lighting: Vec<Value> = plan
        .lighting
        .iter()
        .map(|l| json!({ "id": l.id, "name": l.name, "layers": l.layers, "devices": l.devices, "warnings": l.warnings, "importable": l.importable, "exists": effects.contains(&l.name) }))
        .collect();
    json!({
        "store": store.display().to_string(),
        "connected": conn.is_some(),
        "devices": devices,
        "macros": macros,
        "lighting": lighting,
        "errors": plan.errors,
        "ms": t0.elapsed().as_millis() as u64,
    })
    .to_string()
}

#[derive(Debug, Default, Deserialize)]
pub struct Selection {
    /// Synapse product device pid ("00aa") → Synapse profile GUIDs.
    #[serde(default)]
    pub profiles: BTreeMap<String, Vec<String>>,
    /// pid → the profile whose device-wide settings (polling rate, lighting, dial) to apply.
    #[serde(default)]
    pub device_settings: BTreeMap<String, String>,
    /// pid → also pin the imported profiles to the device's free on-board slots.
    #[serde(default)]
    pub pin: BTreeMap<String, bool>,
    /// Tsuba macro GUIDs.
    #[serde(default)]
    pub macros: Vec<String>,
    /// Chroma Studio profile ids.
    #[serde(default)]
    pub lighting: Vec<String>,
}

/// A name not in `taken`: `name`, `name (Synapse)`, `name (Synapse 2)`…
fn unique_name(name: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    (1..)
        .map(|i| if i == 1 { format!("{name} (Synapse)") } else { format!("{name} (Synapse {i})") })
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// Carry out an import. `progress` gets one line per step; returns the result as JSON
/// `{"done": [..], "failed": [..]}`.
pub fn import(path: &str, selection: &str, mut progress: impl FnMut(String)) -> String {
    let mut done: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let finish = |done: Vec<String>, failed: Vec<String>| json!({ "done": done, "failed": failed }).to_string();
    let sel: Selection = match serde_json::from_str(selection) {
        Ok(s) => s,
        Err(e) => return finish(done, vec![format!("Bad selection: {e}")]),
    };
    let Some(store) = resolve(path) else { return finish(done, vec!["The Synapse data is no longer there".into()]) };
    progress("Reading the Synapse data…".into());
    let plan = match si::scan(&store) {
        Ok(p) => p,
        Err(e) => return finish(done, vec![format!("Couldn't read the Synapse data: {e:#}")]),
    };
    let conn = match Connection::session() {
        Ok(c) => c,
        Err(e) => return finish(done, vec![format!("D-Bus: {e}")]),
    };
    let tg = targets(Some(&conn), &plan);

    // Macros: the chosen ones plus every macro a chosen profile uses.
    let mut want: BTreeSet<String> = sel.macros.iter().cloned().collect();
    for d in &plan.devices {
        let chosen = d.pid.as_ref().and_then(|p| sel.profiles.get(p)).cloned().unwrap_or_default();
        d.profiles.iter().filter(|p| chosen.contains(&p.guid)).for_each(|p| want.extend(p.macros.iter().cloned()));
    }
    for m in plan.macros.iter().filter(|m| want.contains(&m.guid)) {
        progress(format!("Saving macro “{}”…", m.name));
        match call::<_, String>(&conn, "SaveMacro", &(serde_json::to_string(&m.mac).unwrap_or_default(),)) {
            Ok(_) => done.push(format!("Macro “{}” saved to the library", m.name)),
            Err(e) => failed.push(format!("Macro “{}”: {e}", m.name)),
        }
    }

    // Lighting Studio profiles (never applied automatically).
    if !sel.lighting.is_empty() {
        let mut taken: BTreeSet<String> = call::<_, String>(&conn, "GetChromaProfiles", &())
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.get("profiles").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| p.get("name").and_then(Value::as_str).map(String::from)).collect()))
            .unwrap_or_default();
        for l in plan.lighting.iter().filter(|l| sel.lighting.contains(&l.id) && l.importable) {
            let mut prof = l.profile.clone();
            prof.name = unique_name(&l.name, &taken);
            progress(format!("Saving Lighting Studio effect “{}”…", prof.name));
            match call::<_, Vec<String>>(&conn, "SaveChromaProfile", &(serde_json::to_string(&prof).unwrap_or_default(), false)) {
                Ok(_) => {
                    done.push(format!("Lighting Studio effect “{}” saved", prof.name));
                    taken.insert(prof.name);
                }
                Err(e) => failed.push(format!("Lighting Studio effect “{}”: {e}", l.name)),
            }
        }
    }

    // Device profiles: each goes into the device's profile library (a re-import updates it).
    for d in &plan.devices {
        let Some(pid) = d.pid.as_ref() else { continue };
        let chosen = sel.profiles.get(pid).cloned().unwrap_or_default();
        let settings_from = sel.device_settings.get(pid);
        if chosen.is_empty() && settings_from.is_none() {
            continue;
        }
        let dname = d.name.trim_start_matches("Razer ");
        let Some(t) = tg.get(pid) else {
            failed.push(format!("{dname}: not available"));
            continue;
        };
        let mut pins = if sel.pin.get(pid).copied().unwrap_or(false) { t.free_pins } else { 0 };
        for p in d.profiles.iter().filter(|p| chosen.contains(&p.guid)) {
            progress(format!("Adding “{}” to the {dname} profile library…", p.name));
            let prof = tsuba_core::library::LibraryProfile::from_synapse(&t.pid, p);
            let id: String = match call(&conn, "SaveLibraryProfile", &(t.pid.clone(), serde_json::to_string(&prof).unwrap_or_default())) {
                Ok(id) => id,
                Err(e) => {
                    failed.push(format!("{dname} “{}”: {e}", p.name));
                    continue;
                }
            };
            let n = prof.assignments.len() + prof.macros.len() + prof.actions.len() + usize::from(prof.performance.is_some());
            let verb = if t.in_library.contains(&p.guid) { "updated in" } else { "added to" };
            let mut line = format!("{dname}: “{}” {verb} the profile library ({n} setting(s))", p.name);
            if pins > 0 {
                match call::<_, u8>(&conn, "PinLibraryProfile", &(t.pid.clone(), id.clone(), 0u8)) {
                    Ok(slot) => {
                        pins -= 1;
                        line.push_str(&format!(", pinned to on-board slot {slot}"));
                    }
                    Err(e) => failed.push(format!("{dname} “{}”: pinning: {e}", p.name)),
                }
            }
            done.push(line);
        }

        // Device-wide settings from one profile (written to the attached device).
        let Some(p) = settings_from.and_then(|g| d.profiles.iter().find(|p| &p.guid == g)) else { continue };
        if !t.attached {
            failed.push(format!("{dname}: device-wide settings need the device connected"));
            continue;
        }
        let dw = &p.device_wide;
        progress(format!("Applying device-wide settings of “{}” to {dname}…", p.name));
        let mut step = |what: &str, r: Result<(), String>| match r {
            Ok(()) => done.push(format!("{dname}: {what} (from “{}”)", p.name)),
            Err(e) => failed.push(format!("{dname}: {what}: {e}")),
        };
        if let Some(hz) = dw.polling_hz {
            let j = json!({ "polling_hz": hz });
            step(&format!("polling rate {hz} Hz"), call::<_, Vec<String>>(&conn, "SetPerformance", &(t.pid.clone(), j.to_string())).map(|_| ()));
        }
        let b = dw.brightness.unwrap_or(255);
        if let Some(e) = &dw.effect {
            let ej = serde_json::to_string(e).unwrap_or_default();
            let r = call::<_, Vec<String>>(&conn, "SetEffect", &(t.pid.clone(), ej.clone(), b)).and_then(|_| call::<_, ()>(&conn, "SaveLighting", &(t.pid.clone(), ej, b)));
            step(&format!("lighting {}", tsuba_core::config::effect_name(e)), r);
        } else if let Some(pc) = dw.brightness {
            step(&format!("brightness {pc}%"), call::<_, ()>(&conn, "SetBrightness", &(t.pid.clone(), pc)));
        }
        if let Some(slots) = &dw.dial {
            step("command dial modes", call::<_, Vec<String>>(&conn, "SetDial", &(t.pid.clone(), serde_json::to_string(slots).unwrap_or_default())).map(|_| ()));
        }
    }
    if done.is_empty() && failed.is_empty() {
        failed.push("Nothing was selected".into());
    }
    finish(done, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names() {
        let taken: BTreeSet<String> = ["A".to_string(), "A (Synapse)".to_string()].into();
        assert_eq!(unique_name("B", &taken), "B");
        assert_eq!(unique_name("A", &taken), "A (Synapse 2)");
    }

    #[test]
    fn selection_parses() {
        let s: Selection = serde_json::from_str(r#"{"profiles": {"00aa": ["g"]}, "macros": ["m"]}"#).unwrap();
        assert_eq!(s.profiles["00aa"], vec!["g".to_string()]);
        assert!(s.lighting.is_empty() && s.device_settings.is_empty());
    }
}
