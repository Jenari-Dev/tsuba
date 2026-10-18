//! Lighting Studio helpers for the QML side, behind one `Backend.studio_call(method, arg)`
//! invokable (arguments and results are JSON strings): canvas expansion, profile files,
//! and the lighting-related daemon calls on the `…Tsuba1.Lighting` D-Bus interface.

use crate::i18n::tr;
use serde_json::{json, Value};
use tsuba_core::chroma_profiles::{self as cp, ChromaProfile};

const BUS: &str = "io.github.JenAriDev.Tsuba";
const PATH: &str = "/io/github/JenAriDev/Tsuba";
const IFACE: &str = "io.github.JenAriDev.Tsuba1.Lighting";

pub fn call(method: &str, arg: &str) -> String {
    let out = match method {
        "expand" => expand(arg),
        "export" => export(arg),
        "import" => import(arg),
        "owner" => dbus("GetLightingOwner", &(arg.to_string(),), "{\"owner\":\"device\"}"),
        "apps" => dbus("GetGameLightingApps", &(), "{\"only_allowed\":false,\"apps\":[]}"),
        "audio_sources" => dbus("GetAudioSources", &(), "[]"),
        "set_app" => match serde_json::from_str::<Value>(arg) {
            Ok(v) => dbus_unit("SetGameLightingApp", &(v["key"].as_str().unwrap_or("").to_string(), v["rule"].as_str().unwrap_or("").to_string())),
            Err(e) => failure(&e.to_string()),
        },
        "only_allowed" => dbus_unit("SetGameLightingOnlyAllowed", &(arg == "true",)),
        _ => failure("unknown call"),
    };
    out.to_string()
}

fn failure(e: &str) -> Value {
    json!({ "ok": false, "error": e })
}

/// The studio each device runs for a profile: `{pid: {"layers": […]}}` (device layers plus the
/// canvas layers laid out over the shared space).
fn expand(profile: &str) -> Value {
    match serde_json::from_str::<ChromaProfile>(profile) {
        Ok(p) => serde_json::to_value(p.expanded()).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

/// `file:///a%20b` or a plain path → path.
pub fn path_of(url: &str) -> String {
    let p = url.strip_prefix("file://").unwrap_or(url);
    let mut out = Vec::with_capacity(p.len());
    let b = p.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn export(arg: &str) -> Value {
    let Ok(v) = serde_json::from_str::<Value>(arg) else { return failure("bad request") };
    let Ok(profile) = serde_json::from_value::<ChromaProfile>(v["profile"].clone()) else { return failure("bad profile") };
    let mut path = path_of(v["path"].as_str().unwrap_or(""));
    if path.is_empty() {
        return failure("no file chosen");
    }
    if !path.ends_with(".json") {
        path.push_str(".json");
    }
    match cp::export_file(&profile, std::path::Path::new(&path)) {
        Ok(()) => json!({ "ok": true, "path": path }),
        Err(e) => failure(&format!("{e:#}")),
    }
}

fn import(arg: &str) -> Value {
    let Ok(v) = serde_json::from_str::<Value>(arg) else { return failure("bad request") };
    let taken: Vec<String> = v["taken"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    match cp::import_file(std::path::Path::new(&path_of(v["path"].as_str().unwrap_or(""))), &taken) {
        Ok(p) => json!({ "ok": true, "profile": p }),
        Err(e) => failure(&format!("{e:#}")),
    }
}

fn demo() -> bool {
    std::env::var("TSUBA_FAKE_DEVICE").is_ok_and(|s| !s.is_empty())
}

fn connection() -> Option<zbus::blocking::Connection> {
    static CONN: std::sync::Mutex<Option<zbus::blocking::Connection>> = std::sync::Mutex::new(None);
    let mut c = CONN.lock().ok()?;
    if c.is_none() {
        *c = zbus::blocking::Connection::session().ok();
    }
    c.clone()
}

fn dbus<B>(method: &str, body: &B, fallback: &str) -> Value
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let Some(conn) = connection() else { return serde_json::from_str(fallback).unwrap_or_default() };
    let r: zbus::Result<String> = conn.call_method(Some(BUS), PATH, Some(IFACE), method, body).and_then(|m| m.body().deserialize());
    r.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::from_str(fallback).unwrap_or_default())
}

fn dbus_unit<B>(method: &str, body: &B) -> Value
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    if demo() {
        return failure(&tr("Demo device: nothing is written in this mode"));
    }
    let Some(conn) = connection() else { return failure(&tr("tsubad isn't running")) };
    match conn.call_method(Some(BUS), PATH, Some(IFACE), method, body) {
        Ok(_) => json!({ "ok": true }),
        Err(zbus::Error::MethodError(_, Some(msg), _)) => failure(&msg),
        Err(e) => failure(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_decode() {
        assert_eq!(path_of("file:///home/a%20b/c.json"), "/home/a b/c.json");
        assert_eq!(path_of("/plain/path"), "/plain/path");
        assert_eq!(path_of("file:///100%"), "/100%");
    }

    #[test]
    fn expand_matches_the_core_expansion() {
        let profile = r#"{"name":"p","canvas":{"placements":{"028d":{"x":0,"y":0,"w":2,"h":1},"00aa":{"x":2,"y":0,"w":1,"h":1}},
            "layers":[{"effect":{"type":"static","colour":"ff0000"},"targets":{"028d":[],"00aa":[[0,1]]}}]}}"#;
        let v: Value = serde_json::from_str(&call("expand", profile)).unwrap();
        assert_eq!(v["028d"]["layers"].as_array().unwrap().len(), 1);
        assert_eq!(v["00aa"]["layers"][0]["leds"], json!([[0, 1]]));
        assert!(v["00aa"]["layers"][0]["span"]["scale"][0].as_f64().unwrap() < 0.4);
        assert_eq!(call("expand", "garbage"), "{}");
    }

    #[test]
    fn export_then_import_through_the_api() {
        let dir = std::env::temp_dir().join(format!("tsuba-api-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = format!("file://{}/My%20Profile", dir.display());
        let profile = json!({ "name": "Mine", "devices": { "028d": { "layers": [{ "effect": { "type": "battery" } }] } } });
        let r: Value = serde_json::from_str(&call("export", &json!({ "path": file, "profile": profile }).to_string())).unwrap();
        assert_eq!(r["ok"], true, "{r}");
        let written = r["path"].as_str().unwrap();
        assert!(written.ends_with("My Profile.json"));
        let back: Value = serde_json::from_str(&call("import", &json!({ "path": written, "taken": ["Mine"] }).to_string())).unwrap();
        assert_eq!(back["ok"], true, "{back}");
        assert_eq!(back["profile"]["name"], "Mine 2");
        let bad: Value = serde_json::from_str(&call("import", &json!({ "path": "/nonexistent/x.json" }).to_string())).unwrap();
        assert_eq!(bad["ok"], false);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
