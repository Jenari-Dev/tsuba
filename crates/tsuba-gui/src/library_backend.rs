//! QML-facing client of the profile / macro / shortcut part of tsubad's D-Bus API (interface
//! `…Tsuba1.Library`) plus the few things that need no daemon (installed applications for the
//! Launch picker). One generic caller, `call`, so the QML pages (Customize, Macros, profile
//! menu, linked games) don't each need their own Rust glue.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type LibraryBackend = super::LibraryBackendRust;

        /// Call a daemon method with string arguments (`args_json` is a JSON array of strings).
        /// `method` is `Name` (the Library interface) or `Tsuba1.Name` (the main interface).
        /// Returns JSON `{"ok": true, "value": "<the reply string>"}` or `{"ok": false,
        /// "error": "<message>"}`.
        #[qinvokable]
        fn call(self: Pin<&mut LibraryBackend>, method: &QString, args_json: &QString) -> QString;

        /// The desktop's installed applications (JSON list of {name, command, icon, terminal,
        /// categories}) read from the `.desktop` files.
        #[qinvokable]
        fn desktop_apps(self: &LibraryBackend) -> QString;

        /// Launch presets (JSON list of {id, label, command}; empty command = not installed).
        #[qinvokable]
        fn launch_presets(self: &LibraryBackend) -> QString;

        /// Local file path of a `file://` URL ("" for anything else).
        #[qinvokable]
        fn path_of_url(self: &LibraryBackend, url: &QString) -> QString;

        /// The user's home folder as a `file://` URL (a starting folder for file dialogs).
        #[qinvokable]
        fn home_url(self: &LibraryBackend) -> QString;
    }
}

use crate::i18n::tr;
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use zbus::blocking::Connection;

const BUS: &str = "io.github.Jenari_Dev.Tsuba";
const PATH: &str = "/io/github/Jenari_Dev/Tsuba";
const MAIN: &str = "io.github.Jenari_Dev.Tsuba1";
const LIBRARY: &str = "io.github.Jenari_Dev.Tsuba1.Library";

#[derive(Default)]
pub struct LibraryBackendRust {
    conn: Option<Connection>,
}

/// Demo mode (`TSUBA_FAKE_DEVICE`): only reads reach the daemon.
fn fake_mode() -> bool {
    std::env::var("TSUBA_FAKE_DEVICE").is_ok_and(|s| !s.is_empty())
}

fn reads_only(method: &str) -> bool {
    method.starts_with("Get") || method.starts_with("List") || matches!(method, "MacroUsage" | "TextToEvents" | "TextLayouts" | "ShortcutBackend")
}

fn error_text(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(_, Some(msg), _) => msg.clone(),
        zbus::Error::Failure(msg) => msg.clone(),
        e => e.to_string(),
    }
}

/// The reply of a call with up to five string arguments, as a string ("" for methods that
/// return nothing).
pub fn call_strings(conn: &Connection, iface: &str, method: &str, args: &[String]) -> zbus::Result<String> {
    let a = |i: usize| args[i].as_str();
    let msg = match args.len() {
        0 => conn.call_method(Some(BUS), PATH, Some(iface), method, &()),
        1 => conn.call_method(Some(BUS), PATH, Some(iface), method, &(a(0),)),
        2 => conn.call_method(Some(BUS), PATH, Some(iface), method, &(a(0), a(1))),
        3 => conn.call_method(Some(BUS), PATH, Some(iface), method, &(a(0), a(1), a(2))),
        4 => conn.call_method(Some(BUS), PATH, Some(iface), method, &(a(0), a(1), a(2), a(3))),
        5 => conn.call_method(Some(BUS), PATH, Some(iface), method, &(a(0), a(1), a(2), a(3), a(4))),
        n => return Err(zbus::Error::Failure(format!("{n} arguments are too many"))),
    }?;
    Ok(msg.body().deserialize::<String>().unwrap_or_default())
}

impl qobject::LibraryBackend {
    fn conn(mut self: Pin<&mut Self>) -> Result<Connection, String> {
        if self.conn.is_none() {
            match Connection::session() {
                Ok(c) => self.as_mut().rust_mut().conn = Some(c),
                Err(e) => return Err(format!("D-Bus: {e}")),
            }
        }
        Ok(self.conn.clone().expect("connection"))
    }

    pub fn call(self: Pin<&mut Self>, method: &QString, args_json: &QString) -> QString {
        let out = |v: serde_json::Value| QString::from(&v.to_string());
        let method = method.to_string();
        let (iface, name) = match method.split_once('.') {
            Some(("Tsuba1", n)) => (MAIN, n.to_string()),
            _ => (LIBRARY, method),
        };
        let args: Vec<String> = match serde_json::from_str(&args_json.to_string()) {
            Ok(a) => a,
            Err(e) => return out(serde_json::json!({ "ok": false, "error": format!("bad arguments: {e}") })),
        };
        if fake_mode() && !reads_only(&name) {
            return out(serde_json::json!({ "ok": false, "error": tr("Demo device: nothing is written in this mode") }));
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(e) => return out(serde_json::json!({ "ok": false, "error": e })),
        };
        match call_strings(&conn, iface, &name, &args) {
            Ok(v) => out(serde_json::json!({ "ok": true, "value": v })),
            Err(e) => out(serde_json::json!({ "ok": false, "error": error_text(&e) })),
        }
    }

    pub fn desktop_apps(&self) -> QString {
        QString::from(&serde_json::to_string(&tsuba_core::desktop_apps::installed()).unwrap_or_else(|_| "[]".into()))
    }

    pub fn launch_presets(&self) -> QString {
        QString::from(&serde_json::to_string(&tsuba_core::desktop_apps::presets()).unwrap_or_else(|_| "[]".into()))
    }

    pub fn path_of_url(&self, url: &QString) -> QString {
        let u = url.to_string();
        let Some(rest) = u.strip_prefix("file://") else { return QString::default() };
        // Percent-decoding (spaces, non-ASCII).
        let bytes = rest.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        QString::from(&String::from_utf8_lossy(&out).into_owned())
    }

    pub fn home_url(&self) -> QString {
        QString::from(&dirs::home_dir().map(|h| format!("file://{}", h.display())).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reads_reach_the_daemon_in_demo_mode() {
        assert!(reads_only("MacroUsage") && reads_only("ListAudioDevices") && reads_only("GetMacros"));
        assert!(!reads_only("DuplicateMacro") && !reads_only("SetProfileLighting") && !reads_only("ExportLibraryProfile"));
    }
}
