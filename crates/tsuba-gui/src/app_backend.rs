//! App-level services for the QML shell: launch at login, the environment check behind the
//! "no device" screen, diagnostics export, notification settings, the system tray and the
//! single-instance hand-off. Device control stays in [`crate::backend`].

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, tray_available)]
        #[qproperty(bool, start_minimized)]
        type AppBackend = super::AppBackendRust;

        /// Launch at login: is the autostart entry present?
        #[qinvokable]
        fn autostart_enabled(self: &AppBackend) -> bool;
        #[qinvokable]
        fn set_autostart(self: &AppBackend, on: bool) -> bool;

        /// Start tsubad at login (the systemd user unit).
        #[qinvokable]
        fn service_enabled(self: &AppBackend) -> bool;
        #[qinvokable]
        fn set_service_enabled(self: &AppBackend, on: bool) -> bool;

        /// Start the background service now (systemd user unit, else the tsubad next to this app).
        #[qinvokable]
        fn start_service(self: Pin<&mut AppBackend>);

        /// Run `tsubad --check-env` in the background; answers with `environment_checked`.
        #[qinvokable]
        fn check_environment(self: Pin<&mut AppBackend>);

        /// Write a support bundle (a zip without personal data) to `path`.
        #[qinvokable]
        fn export_diagnostics(self: Pin<&mut AppBackend>, path: &QString);
        /// A good default file name for the bundle, e.g. `tsuba-diagnostics-2026-10-18.zip`.
        #[qinvokable]
        fn diagnostics_file_name(self: &AppBackend) -> QString;

        #[qinvokable]
        fn notification_settings(self: &AppBackend) -> QString;
        #[qinvokable]
        fn set_notification_settings(self: &AppBackend, settings_json: &QString) -> QString;
        #[qinvokable]
        fn send_test_notification(self: &AppBackend) -> bool;

        /// Version, system and daemon details as JSON (About page, "copy details").
        #[qinvokable]
        fn system_details(self: &AppBackend) -> QString;
        #[qinvokable]
        fn app_version(self: &AppBackend) -> QString;
        #[qinvokable]
        fn project_url(self: &AppBackend) -> QString;
        #[qinvokable]
        fn licence_text(self: &AppBackend) -> QString;
        #[qinvokable]
        fn third_party_licences(self: &AppBackend) -> QString;

        /// Start the tray icon (idempotent); messages from it arrive as `tray_action`.
        #[qinvokable]
        fn tray_start(self: Pin<&mut AppBackend>);
        /// Replace the tray menu: `devices_json` = [{pid, name, profiles: [{id, name, active}]}],
        /// `labels_json` = {open, settings, quit, profiles} (already translated).
        #[qinvokable]
        fn tray_update(self: &AppBackend, devices_json: &QString, labels_json: &QString, tooltip: &QString);

        /// Start a new copy of this app and let the caller quit (language change).
        #[qinvokable]
        fn restart_app(self: &AppBackend) -> bool;
        /// Seconds since this app started.
        #[qinvokable]
        fn uptime_seconds(self: &AppBackend) -> i32;

        #[qsignal]
        fn environment_checked(self: Pin<&mut AppBackend>, report: QString);
        #[qsignal]
        fn diagnostics_exported(self: Pin<&mut AppBackend>, ok: bool, message: QString);
        #[qsignal]
        fn service_started(self: Pin<&mut AppBackend>, ok: bool, message: QString);
        /// kind: "activate" | "quit" | "profile" (a = device pid, b = profile id) | "page" (a = page name)
        #[qsignal]
        fn tray_action(self: Pin<&mut AppBackend>, kind: QString, a: QString, b: QString);
    }

    impl cxx_qt::Threading for AppBackend {}
}

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::Threading;
use cxx_qt_lib::QString;
use serde_json::{json, Value};

use crate::tray;

const BUS: &str = "io.github.JenAriDev.Tsuba";
const PATH: &str = "/io/github/JenAriDev/Tsuba";
const APP_IFACE: &str = "io.github.JenAriDev.Tsuba1.App";
const PROJECT_URL: &str = "https://github.com/JenAriDev/tsuba";

pub struct AppBackendRust {
    tray_available: bool,
    start_minimized: bool,
    started: std::time::Instant,
}

impl Default for AppBackendRust {
    fn default() -> Self {
        Self { tray_available: false, start_minimized: std::env::args().any(|a| a == "--minimized"), started: std::time::Instant::now() }
    }
}

// ---- autostart -------------------------------------------------------------------------------

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

fn autostart_path() -> PathBuf {
    config_home().join("autostart").join("io.github.jenaridev.tsuba.desktop")
}

/// The command for the autostart entry: the bare name when installed system-wide, else this binary's path.
pub fn autostart_exec(exe: &Path) -> String {
    let s = exe.to_string_lossy().trim_end_matches(" (deleted)").to_string();
    if s.starts_with("/usr/bin/") {
        "tsuba-app".to_string()
    } else if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s
    }
}

pub fn autostart_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Tsuba\nComment=Tsuba tray icon and notifications\nExec={exec} --minimized\nIcon=io.github.jenaridev.tsuba\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n"
    )
}

fn tsubad_binary() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let exe = PathBuf::from(exe.to_string_lossy().trim_end_matches(" (deleted)"));
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("tsubad");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from("tsubad")
}

fn systemctl(args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("systemctl").arg("--user").args(args).output()
}

// ---- daemon calls ----------------------------------------------------------------------------

fn bus_call<B, R>(method: &str, body: &B) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    let conn = zbus::blocking::Connection::session()?;
    conn.call_method(Some(BUS), PATH, Some(APP_IFACE), method, body)?.body().deserialize()
}

fn q(s: &str) -> QString {
    QString::from(s)
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "unknown".into())
}

impl qobject::AppBackend {
    pub fn autostart_enabled(&self) -> bool {
        autostart_path().exists()
    }

    pub fn set_autostart(&self, on: bool) -> bool {
        let path = autostart_path();
        if !on {
            return match std::fs::remove_file(&path) {
                Ok(()) => true,
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
        }
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tsuba-app"));
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, autostart_entry(&autostart_exec(&exe))).is_ok()
    }

    pub fn service_enabled(&self) -> bool {
        systemctl(&["is-enabled", "tsubad"]).map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled").unwrap_or(false)
    }

    pub fn set_service_enabled(&self, on: bool) -> bool {
        systemctl(&[if on { "enable" } else { "disable" }, "tsubad"]).map(|o| o.status.success()).unwrap_or(false)
    }

    pub fn start_service(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let (ok, msg) = start_daemon();
            let _ = qt.queue(move |mut o| o.as_mut().service_started(ok, q(&msg)));
        });
    }

    pub fn check_environment(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let report = std::process::Command::new(tsubad_binary())
                .arg("--check-env")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .filter(|s| serde_json::from_str::<Value>(s).is_ok())
                .unwrap_or_else(|| json!({ "diagnosis": "unknown" }).to_string());
            let _ = qt.queue(move |mut o| o.as_mut().environment_checked(q(&report)));
        });
    }

    pub fn export_diagnostics(self: Pin<&mut Self>, path: &QString) {
        let path = file_url_to_path(&path.to_string());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let r = std::process::Command::new(tsubad_binary()).arg("--diagnostics").arg(&path).output();
            let (ok, msg) = match r {
                Ok(o) if o.status.success() => (true, path.clone()),
                Ok(o) => (false, String::from_utf8_lossy(&o.stderr).trim().to_string()),
                Err(e) => (false, format!("could not run tsubad: {e}")),
            };
            let _ = qt.queue(move |mut o| o.as_mut().diagnostics_exported(ok, q(&msg)));
        });
    }

    pub fn diagnostics_file_name(&self) -> QString {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (y, m, d) = civil_date(secs);
        q(&format!("tsuba-diagnostics-{y:04}-{m:02}-{d:02}.zip"))
    }

    pub fn notification_settings(&self) -> QString {
        q(&bus_call::<_, String>("GetNotificationSettings", &()).unwrap_or_default())
    }

    pub fn set_notification_settings(&self, settings_json: &QString) -> QString {
        q(&bus_call::<_, String>("SetNotificationSettings", &(settings_json.to_string(),)).unwrap_or_default())
    }

    pub fn send_test_notification(&self) -> bool {
        bus_call::<_, ()>("SendTestNotification", &()).is_ok()
    }

    pub fn system_details(&self) -> QString {
        let daemon = bus_call::<_, String>("GetInfo", &()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        q(&json!({
            "app": env!("CARGO_PKG_VERSION"),
            "daemon": daemon.as_ref().and_then(|d| d["version"].as_str()),
            "daemon_read_only": daemon.as_ref().and_then(|d| d["read_only"].as_bool()),
            "system": os_pretty_name(),
            "kernel": std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim(),
            "session": env("XDG_SESSION_TYPE"),
            "desktop": env("XDG_CURRENT_DESKTOP"),
            "arch": std::env::consts::ARCH,
        })
        .to_string())
    }

    pub fn app_version(&self) -> QString {
        q(env!("CARGO_PKG_VERSION"))
    }

    pub fn project_url(&self) -> QString {
        q(PROJECT_URL)
    }

    pub fn licence_text(&self) -> QString {
        q(include_str!("../../../LICENSE"))
    }

    pub fn third_party_licences(&self) -> QString {
        q(include_str!("../../../docs/THIRD-PARTY-LICENSES.md"))
    }

    pub fn tray_start(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        tray::set_dispatcher(Box::new(move |m| {
            let _ = qt.queue(move |mut o| match m {
                tray::Msg::Activate => o.as_mut().tray_action(q("activate"), q(""), q("")),
                tray::Msg::Quit => o.as_mut().tray_action(q("quit"), q(""), q("")),
                tray::Msg::Profile(pid, id) => o.as_mut().tray_action(q("profile"), q(&pid), q(&id)),
                tray::Msg::OpenPage(p) => o.as_mut().tray_action(q("page"), q(&p), q("")),
                tray::Msg::TrayAvailable(v) => o.as_mut().set_tray_available(v),
            });
        }));
        tray::start();
    }

    pub fn tray_update(&self, devices_json: &QString, labels_json: &QString, tooltip: &QString) {
        let devices: Vec<tray::TrayDevice> = serde_json::from_str::<Value>(&devices_json.to_string())
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|d| tray::TrayDevice {
                pid: d["pid"].as_str().unwrap_or("").into(),
                name: d["name"].as_str().unwrap_or("").into(),
                profiles: d["profiles"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|p| tray::TrayProfile {
                                id: p["id"].as_str().unwrap_or("").into(),
                                name: p["name"].as_str().unwrap_or("").into(),
                                active: p["active"].as_bool().unwrap_or(false),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        let l: Value = serde_json::from_str(&labels_json.to_string()).unwrap_or_default();
        let d = tray::Labels::default();
        let get = |k: &str, dflt: &str| l[k].as_str().map(str::to_string).unwrap_or_else(|| dflt.to_string());
        let labels = tray::Labels { open: get("open", &d.open), settings: get("settings", &d.settings), quit: get("quit", &d.quit), profiles: get("profiles", &d.profiles) };
        tray::update(&devices, &labels, &tooltip.to_string());
    }

    pub fn restart_app(&self) -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let exe = PathBuf::from(exe.to_string_lossy().trim_end_matches(" (deleted)"));
        let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--minimized").collect();
        // The running instance owns the single-instance name until it exits: wait for it in the child.
        std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 0.7; exec \"$0\" \"$@\"")
            .arg(exe)
            .args(args)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|mut c| {
                std::thread::spawn(move || c.wait());
            })
            .is_ok()
    }

    pub fn uptime_seconds(&self) -> i32 {
        self.started.elapsed().as_secs() as i32
    }
}

/// `systemctl --user start tsubad`; without the unit, the tsubad next to this app (detached).
fn start_daemon() -> (bool, String) {
    if let Ok(o) = systemctl(&["start", "tsubad"]) {
        if o.status.success() {
            return (true, "started".into());
        }
    }
    match std::process::Command::new(tsubad_binary())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut c) => {
            std::thread::spawn(move || c.wait());
            (true, "started without the systemd service".into())
        }
        Err(e) => (false, format!("could not start tsubad: {e}")),
    }
}

pub fn file_url_to_path(url: &str) -> String {
    let Some(rest) = url.strip_prefix("file://") else { return url.to_string() };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut out = Vec::new();
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn civil_date(secs: u64) -> (u32, u32, u32) {
    let z = (secs / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as u32;
    (y, m, d)
}

/// Settings written by the QML (`[app] language=…` in app.ini) that must be known before Qt starts.
pub fn saved_language() -> Option<String> {
    let text = std::fs::read_to_string(config_home().join("tsuba").join("app.ini")).ok()?;
    parse_ini_value(&text, "app", "language").filter(|l| !l.is_empty() && l != "system")
}

pub fn parse_ini_value(text: &str, group: &str, key: &str) -> Option<String> {
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(g) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = g == group;
        } else if in_group {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_entry_starts_minimized_with_the_right_command() {
        assert_eq!(autostart_exec(Path::new("/usr/bin/tsuba-app")), "tsuba-app");
        assert_eq!(autostart_exec(Path::new("/home/x/.local/bin/tsuba-app (deleted)")), "/home/x/.local/bin/tsuba-app");
        assert_eq!(autostart_exec(Path::new("/opt/my apps/tsuba-app")), "\"/opt/my apps/tsuba-app\"");
        let e = autostart_entry("tsuba-app");
        assert!(e.contains("Exec=tsuba-app --minimized\n") && e.starts_with("[Desktop Entry]\n") && e.contains("Icon=io.github.jenaridev.tsuba"));
    }

    #[test]
    fn ini_values() {
        let t = "[appearance]\naccent=#1fc9a7\nlanguage=no\n\n[app]\nlanguage=ja\nother=1\n";
        assert_eq!(parse_ini_value(t, "app", "language").as_deref(), Some("ja"));
        assert_eq!(parse_ini_value(t, "appearance", "language").as_deref(), Some("no"));
        assert_eq!(parse_ini_value(t, "app", "missing"), None);
        assert_eq!(parse_ini_value("", "app", "language"), None);
    }

    #[test]
    fn file_urls() {
        assert_eq!(file_url_to_path("file:///tmp/a%20b.zip"), "/tmp/a b.zip");
        assert_eq!(file_url_to_path("/tmp/x.zip"), "/tmp/x.zip");
        assert_eq!(file_url_to_path("file:///tmp/100%"), "/tmp/100%");
    }

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(951_782_400), (2000, 2, 29));
    }
}
