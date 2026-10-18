//! QML-facing backend: a thin client of tsubad's D-Bus API. All device access
//! goes through the daemon; this object only relays calls and signals.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, connected)]
        #[qproperty(bool, readonly)]
        #[qproperty(QString, devices)]
        #[qproperty(QString, status)]
        type Backend = super::BackendRust;

        /// Re-query the daemon: connection, read-only flag and device list.
        #[qinvokable]
        fn refresh(self: Pin<&mut Backend>);

        /// Full device state as JSON ("" on failure; see `status`).
        #[qinvokable]
        fn state(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Key/button assignments on a profile and layer as JSON ("" on failure).
        #[qinvokable]
        fn assignments(self: Pin<&mut Backend>, pid: &QString, profile: i32, layer: i32) -> QString;

        /// Write one assignment (JSON {profile,key,layer,function,data}); returns its
        /// description, or "" on failure (see `status`).
        #[qinvokable]
        fn set_assignment(self: Pin<&mut Backend>, pid: &QString, assignment_json: &QString) -> QString;

        /// Make the daemon forget its cached reads for a device ("" = all) so the next read
        /// goes to the hardware (the refresh button).
        #[qinvokable]
        fn refresh_cache(self: Pin<&mut Backend>, pid: &QString) -> bool;

        /// Macro library + on-board macros JSON ("" on error).
        #[qinvokable]
        fn macros(self: Pin<&mut Backend>) -> QString;

        /// Save a library macro; returns its GUID ("" on error).
        #[qinvokable]
        fn save_macro(self: Pin<&mut Backend>, macro_json: &QString) -> QString;

        #[qinvokable]
        fn delete_macro(self: Pin<&mut Backend>, guid: &QString) -> bool;

        /// Copy an on-board macro into the library; returns its GUID ("" on error).
        #[qinvokable]
        fn import_macro(self: Pin<&mut Backend>, pid: &QString, id: i32) -> QString;

        /// Bind a key/button to a library macro; returns a description ("" on error).
        #[qinvokable]
        fn assign_macro(self: Pin<&mut Backend>, pid: &QString, binding_json: &QString) -> QString;

        /// Performance settings JSON for one on-board profile ("" on error).
        #[qinvokable]
        fn performance(self: Pin<&mut Backend>, pid: &QString, profile: i32) -> QString;

        /// Bind a key to a software action (launch / url / device); returns its label ("" on error).
        #[qinvokable]
        fn assign_action(self: Pin<&mut Backend>, pid: &QString, binding_json: &QString) -> QString;

        /// The device's software actions JSON.
        #[qinvokable]
        fn actions(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Linked games JSON {"links", "active"} ("" on error).
        #[qinvokable]
        fn app_links(self: Pin<&mut Backend>) -> QString;

        #[qinvokable]
        fn set_app_links(self: Pin<&mut Backend>, links_json: &QString) -> bool;

        /// Running program names (JSON array) for the link picker.
        #[qinvokable]
        fn running_programs(self: Pin<&mut Backend>) -> QString;

        /// Installed games found on this PC (Steam, Lutris, Heroic, Bottles) as a JSON list of
        /// {source, id, name, install_dir, art[, exe]}; "[]" if none.
        #[qinvokable]
        fn installed_games(self: &Backend) -> QString;

        /// Power/battery settings + device info JSON ("" on error).
        #[qinvokable]
        fn power(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Apply power settings; returns the change log joined by newlines ("" on error).
        #[qinvokable]
        fn set_power(self: Pin<&mut Backend>, pid: &QString, settings_json: &QString) -> QString;

        /// Copy text to the clipboard.
        #[qinvokable]
        fn copy_text(self: &Backend, text: &QString);

        /// Named Chroma profiles JSON ("" on error).
        #[qinvokable]
        fn chroma_profiles(self: Pin<&mut Backend>) -> QString;

        /// Save (and optionally apply) a Chroma profile; false on error.
        #[qinvokable]
        fn save_chroma_profile(self: Pin<&mut Backend>, profile_json: &QString, apply: bool) -> bool;

        #[qinvokable]
        fn apply_chroma_profile(self: Pin<&mut Backend>, name: &QString) -> bool;

        #[qinvokable]
        fn rename_chroma_profile(self: Pin<&mut Backend>, old_name: &QString, new_name: &QString) -> bool;

        #[qinvokable]
        fn delete_chroma_profile(self: Pin<&mut Backend>, name: &QString) -> bool;

        /// Start an on-screen preview of a studio (layers JSON) for device `pid`.
        #[qinvokable]
        fn preview_start(self: Pin<&mut Backend>, pid: &QString, studio_json: &QString) -> bool;

        /// Next preview frame of the most recently started preview: JSON {"row,col": "rrggbb"}.
        #[qinvokable]
        fn preview_frame(self: Pin<&mut Backend>) -> QString;

        /// Next preview frame for one device's preview ("{}" when none was started).
        #[qinvokable]
        fn preview_frame_for(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Forget Ambient's remembered screen (KDE asks again next time).
        #[qinvokable]
        fn reset_ambient_screen(self: Pin<&mut Backend>) -> bool;

        /// Live Ambient screen grid ("WxH:rrggbb…"; "" when no capture runs). Silent on errors.
        #[qinvokable]
        fn ambient_preview(self: Pin<&mut Backend>) -> QString;

        /// Lighting power options JSON.
        #[qinvokable]
        fn lighting_power(self: Pin<&mut Backend>, pid: &QString) -> QString;

        #[qinvokable]
        fn set_lighting_power(self: Pin<&mut Backend>, pid: &QString, settings_json: &QString) -> bool;

        /// Keyboard Game Mode settings JSON.
        #[qinvokable]
        fn game_mode(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Store Game Mode settings / switch it; returns the new state JSON ("" on error).
        #[qinvokable]
        fn set_game_mode(self: Pin<&mut Backend>, pid: &QString, settings_json: &QString) -> QString;

        /// Write the command dial modes (JSON list of {function, colour, enabled}).
        #[qinvokable]
        fn set_dial(self: Pin<&mut Backend>, pid: &QString, slots_json: &QString) -> bool;

        #[qinvokable]
        fn set_dial_mode(self: Pin<&mut Backend>, pid: &QString, slot: i32) -> bool;

        /// On-board profiles JSON ("" on error).
        #[qinvokable]
        fn profiles(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Create a profile (copy_from 0 = device defaults); returns its id or -1.
        #[qinvokable]
        fn create_profile(self: Pin<&mut Backend>, pid: &QString, name: &QString, copy_from: i32) -> i32;

        #[qinvokable]
        fn rename_profile(self: Pin<&mut Backend>, pid: &QString, id: i32, name: &QString) -> bool;

        #[qinvokable]
        fn delete_profile(self: Pin<&mut Backend>, pid: &QString, id: i32) -> bool;

        #[qinvokable]
        fn set_active_profile(self: Pin<&mut Backend>, pid: &QString, id: i32) -> bool;

        /// The device's profile library JSON (see tsubad ListLibraryProfiles; "" on error).
        #[qinvokable]
        fn library_profiles(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Add a library profile (copy_from = a profile id or ""); returns its id ("" on error).
        #[qinvokable]
        fn create_library_profile(self: Pin<&mut Backend>, pid: &QString, name: &QString, copy_from: &QString) -> QString;

        #[qinvokable]
        fn rename_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString, name: &QString) -> bool;

        #[qinvokable]
        fn duplicate_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString, name: &QString) -> QString;

        #[qinvokable]
        fn delete_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString) -> bool;

        /// Activate a library profile in the background; `library_activated` reports the result.
        #[qinvokable]
        fn activate_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString);

        /// Pin a profile to an on-board slot (0 = lowest free); returns the slot or -1.
        #[qinvokable]
        fn pin_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString, slot: i32) -> i32;

        #[qinvokable]
        fn unpin_library_profile(self: Pin<&mut Backend>, pid: &QString, id: &QString) -> bool;

        /// Apply performance settings; returns the change log joined with "\n", "" on error.
        #[qinvokable]
        fn set_performance(self: Pin<&mut Backend>, pid: &QString, settings_json: &QString) -> QString;

        #[qinvokable]
        fn set_effect(self: Pin<&mut Backend>, pid: &QString, effect_json: &QString, brightness: i32) -> bool;

        /// Persist lighting (effect JSON, brightness percent or 255 = keep) in tsubad's config.
        #[qinvokable]
        fn save_lighting(self: Pin<&mut Backend>, pid: &QString, effect_json: &QString, brightness: i32) -> bool;

        /// The device's saved config section as JSON ("{}" if none).
        #[qinvokable]
        fn config(self: Pin<&mut Backend>, pid: &QString) -> QString;

        #[qinvokable]
        fn set_brightness(self: Pin<&mut Backend>, pid: &QString, percent: i32) -> bool;

        #[qinvokable]
        fn set_dpi(self: Pin<&mut Backend>, pid: &QString, x: i32, y: i32) -> bool;

        /// Save every device's current state to ~/.config/tsuba/config.toml.
        #[qinvokable]
        fn save(self: Pin<&mut Backend>) -> bool;

        /// Start relaying daemon signals (call once).
        #[qinvokable]
        fn start_events(self: Pin<&mut Backend>);

        /// True once the running binary has been replaced by an update (restart to pick it up).
        #[qinvokable]
        fn binary_replaced(self: &Backend) -> bool;

        /// Optional local device-image pack (device.json + "dir"), or "" if none.
        #[qinvokable]
        fn device_pack(self: &Backend, pid: &QString) -> QString;

        /// Lighting Studio helpers (canvas expansion, profile files, game-lighting apps, audio
        /// sources, who controls the lighting): see `studio_api.rs`. JSON in, JSON out.
        #[qinvokable]
        fn studio_call(self: &Backend, method: &QString, arg: &QString) -> QString;

        /// Whether the user has a personal picture pack for this device (else Tsuba's line art).
        #[qinvokable]
        fn has_personal_pack(self: &Backend, pid: &QString) -> bool;

        /// Tsuba's own drawing pack for this device (device.json + "dir"), ignoring any personal
        /// pack; "" if it ships none. The alignment overlay draws it under the user's picture.
        #[qinvokable]
        fn builtin_pack(self: &Backend, pid: &QString) -> QString;

        /// Whether the personal pack is made from the user's own picture (art "custom").
        #[qinvokable]
        fn has_custom_picture(self: &Backend, pid: &QString) -> bool;

        /// Copy the picture at `file_url` (png/jpg/webp/svg) into the personal pack for `view`
        /// ("top" / "bottom"); the pack is created from Tsuba's drawing. Follow with
        /// `set_picture_rect`. Returns false (see `status`) on failure.
        #[qinvokable]
        fn set_custom_picture(self: Pin<&mut Backend>, pid: &QString, view: &QString, file_url: &QString) -> bool;

        /// Where the custom picture of `view` is drawn, in the drawing's coordinates.
        #[qinvokable]
        fn set_picture_rect(self: Pin<&mut Backend>, pid: &QString, view: &QString, x: f64, y: f64, w: f64, h: f64) -> bool;

        /// Drop the user's pictures and go back to Tsuba's drawings (or the backed-up photo pack).
        #[qinvokable]
        fn remove_custom_picture(self: Pin<&mut Backend>, pid: &QString) -> bool;

        /// What Tsuba knows about a device model (the daemon's GetDeviceSupport JSON: database
        /// entry plus allow_writes / allow_untested_writes / read_only / attached / approximate_art);
        /// "" on failure. Works for any model in the database, attached or not.
        #[qinvokable]
        fn device_support(self: Pin<&mut Backend>, pid: &QString) -> QString;

        /// Turn writes on or off for an untested model; returns the updated support JSON ("" on
        /// failure, see `status`).
        #[qinvokable]
        fn set_allow_untested_writes(self: Pin<&mut Backend>, pid: &QString, allow: bool) -> QString;

        /// Every model in the device database as JSON `[{pid, name, kind, tested, supported}]`.
        #[qinvokable]
        fn known_devices(self: Pin<&mut Backend>) -> QString;

        /// Environment variable value ("" if unset); used for headless UI checks
        /// (TSUBA_SCREENSHOT=/path.png, TSUBA_DEVICE=pid).
        #[qinvokable]
        fn env_var(self: &Backend, name: &QString) -> QString;

        /// Open a desktop settings page: "keyboard" or "mouse".
        #[qinvokable]
        fn open_system_settings(self: &Backend, page: &QString) -> bool;

        /// Synapse 4 stores found on mounted Windows drives (JSON list of paths).
        #[qinvokable]
        fn synapse_stores(self: &Backend) -> QString;

        /// Read the Synapse store at or below `path` on a worker thread; the result arrives as
        /// `synapse_scanned(json)` (devices with their profiles + free slots, macros, effects;
        /// or {"error"}). The store is only read.
        #[qinvokable]
        fn synapse_scan(self: Pin<&mut Backend>, path: &QString);

        /// Import the selection (JSON {profiles: {pid: [guid]}, device_settings: {pid: guid},
        /// macros: [guid], lighting: [id]}) on a worker thread through tsubad. Progress comes as
        /// `synapse_progress(text)`, the result as `synapse_imported({"done", "failed"})`.
        #[qinvokable]
        fn synapse_import(self: Pin<&mut Backend>, path: &QString, selection_json: &QString);
    }

    unsafe extern "RustQt" {
        /// Game lighting (games that support Razer Chroma) as JSON: {"available": bool (false when
        /// the daemon has no game lighting yet), "running": bool, "enabled": bool, "web": bool
        /// (websites allowed), "status": {… GetGameLightingStatus …}}.
        #[qinvokable]
        fn game_lighting(self: Pin<&mut Backend>) -> QString;

        /// Turn game lighting on/off in the daemon; false on error (see `status`).
        #[qinvokable]
        fn set_game_lighting(self: Pin<&mut Backend>, enabled: bool) -> bool;

        /// Let web pages (browser-based lighting apps) use game lighting; false on error.
        #[qinvokable]
        fn set_game_lighting_web(self: Pin<&mut Backend>, enabled: bool) -> bool;

        /// Wine/Proton prefixes and the bridge's install state in each, as JSON
        /// {"dll_dir", "auto", "prefixes": [{id, source, name, path, art, state}], "auto_installed": [id]}.
        /// With automatic install on, new prefixes get the bridge during this scan.
        #[qinvokable]
        fn game_lighting_prefixes(self: &Backend) -> QString;

        /// Install the bridge into prefixes (JSON list of ids, or ["all"]); returns a JSON report
        /// {"ok": bool, "message": text}.
        #[qinvokable]
        fn game_lighting_install(self: &Backend, ids_json: &QString) -> QString;

        /// Remove the bridge from prefixes (JSON list of ids); returns a JSON report like install.
        #[qinvokable]
        fn game_lighting_uninstall(self: &Backend, ids_json: &QString) -> QString;

        /// Remember a prefix folder the scan doesn't find ("" on success, else the reason).
        #[qinvokable]
        fn game_lighting_add_prefix(self: &Backend, path: &QString) -> QString;

        /// Install into new prefixes automatically; false if the setting couldn't be saved.
        #[qinvokable]
        fn set_game_lighting_auto(self: &Backend, on: bool) -> bool;

        /// Paint a test pattern on keyboard and mouse through the game lighting server for 3 s;
        /// the outcome arrives as `game_lighting_tested`.
        #[qinvokable]
        fn game_lighting_test(self: Pin<&mut Backend>);

        #[qsignal]
        fn game_lighting_changed(self: Pin<&mut Backend>, status: QString);

        #[qsignal]
        fn game_lighting_tested(self: Pin<&mut Backend>, ok: bool, message: QString);

        #[qsignal]
        fn hardware_event(self: Pin<&mut Backend>, pid: QString, event: QString);

        #[qsignal]
        fn devices_changed_on_bus(self: Pin<&mut Backend>);

        #[qsignal]
        fn synapse_scanned(self: Pin<&mut Backend>, result: QString);

        #[qsignal]
        fn synapse_progress(self: Pin<&mut Backend>, text: QString);

        #[qsignal]
        fn synapse_imported(self: Pin<&mut Backend>, result: QString);

        /// A library profile activation finished: `result` is tsubad's report JSON, or the error.
        #[qsignal]
        fn library_activated(self: Pin<&mut Backend>, pid: QString, ok: bool, result: QString);

        /// tsubad's LibraryChanged signal (profiles edited, or the active profile changed).
        #[qsignal]
        fn library_changed(self: Pin<&mut Backend>, pid: QString);
    }

    impl cxx_qt::Threading for Backend {}
}

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use zbus::blocking::Connection;

const BUS: &str = "io.github.JenAriDev.Tsuba";
const PATH: &str = "/io/github/JenAriDev/Tsuba";
const IFACE: &str = "io.github.JenAriDev.Tsuba1";

#[derive(Default)]
pub struct BackendRust {
    connected: bool,
    readonly: bool,
    devices: QString,
    status: QString,
    conn: Option<Connection>,
    events_started: bool,
    /// One live renderer per device pid (the combined Studio view shows several at once).
    previews: std::collections::HashMap<String, (tsuba_core::studio::Renderer, std::time::Instant)>,
    /// The pid whose preview was started most recently (what `preview_frame()` returns).
    preview_last: String,
}

/// Test/demo hook: `TSUBA_FAKE_DEVICE=<pid>` shows that database model as an attached,
/// untested device whose state comes from the database. In that mode only read calls reach the
/// daemon, so nothing can ever be written.
fn fake_pid() -> Option<&'static str> {
    static FAKE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    FAKE.get_or_init(|| std::env::var("TSUBA_FAKE_DEVICE").ok().filter(|s| !s.is_empty())).as_deref()
}

fn call<B, R>(conn: &Connection, method: &str, body: &B) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    if fake_pid().is_some() && !(method.starts_with("Get") || method.starts_with("List") || method == "RunningPrograms") {
        return Err(zbus::Error::Failure("Demo device: nothing is written in this mode".into()));
    }
    conn.call_method(Some(BUS), PATH, Some(IFACE), method, body)?.body().deserialize()
}

fn error_text(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(_, Some(msg), _) => msg.clone(),
        zbus::Error::Failure(msg) => msg.clone(),
        e => e.to_string(),
    }
}

impl qobject::Backend {
    fn conn(mut self: Pin<&mut Self>) -> Option<Connection> {
        if self.conn.is_none() {
            match Connection::session() {
                Ok(c) => self.as_mut().rust_mut().conn = Some(c),
                Err(e) => self.as_mut().set_status(QString::from(&format!("D-Bus: {e}"))),
            }
        }
        self.conn.clone()
    }

    fn report<T>(mut self: Pin<&mut Self>, r: zbus::Result<T>) -> Option<T> {
        match r {
            Ok(v) => {
                self.as_mut().set_status(QString::default());
                Some(v)
            }
            Err(e) => {
                // The demo device doesn't exist for the daemon; its "not attached" errors are noise.
                let quiet = fake_pid().is_some() && !matches!(e, zbus::Error::Failure(_));
                self.as_mut().set_status(QString::from(&if quiet { String::new() } else { error_text(&e) }));
                None
            }
        }
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let Some(conn) = self.as_mut().conn() else { return self.set_connected(false) };
        let list: zbus::Result<Vec<(String, String, String)>> = call(&conn, "ListDevices", &());
        match list {
            Ok(list) => {
                let mut json: Vec<_> =
                    list.iter().map(|(pid, name, kind)| serde_json::json!({ "pid": pid, "name": name, "kind": kind })).collect();
                if let Some(fake) = fake_pid() {
                    if !list.iter().any(|d| d.0 == fake) {
                        if let Ok(v) = call::<_, String>(&conn, "GetDeviceSupport", &(fake.to_string(),)) {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v) {
                                json.push(serde_json::json!({ "pid": fake, "name": v["name"], "kind": v["kind"] }));
                            }
                        }
                    }
                }
                let ro = conn
                    .call_method(Some(BUS), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(IFACE, "ReadOnly"))
                    .ok()
                    .and_then(|m| m.body().deserialize::<zbus::zvariant::OwnedValue>().ok())
                    .and_then(|v| bool::try_from(v).ok())
                    .unwrap_or(false);
                self.as_mut().set_readonly(ro);
                self.as_mut().set_devices(QString::from(&serde_json::Value::from(json).to_string()));
                self.as_mut().set_connected(true);
                self.as_mut().set_status(QString::default());
            }
            Err(_) => {
                self.as_mut().set_connected(false);
                self.as_mut().set_devices(QString::from("[]"));
                self.as_mut().set_status(QString::from("tsubad isn't running — start it with: systemctl --user start tsubad"));
            }
        }
    }

    pub fn state(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        if fake_pid() == Some(pid.to_string().as_str()) {
            return QString::from(&fake_state(&conn, &pid.to_string()));
        }
        let r: zbus::Result<String> = call(&conn, "GetState", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn device_support(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetDeviceSupport", &(pid.to_string(),));
        let text = self.report(r).unwrap_or_default();
        if fake_pid() == Some(pid.to_string().as_str()) {
            return QString::from(&fake_support(&text).unwrap_or(text));
        }
        QString::from(&text)
    }

    pub fn set_allow_untested_writes(mut self: Pin<&mut Self>, pid: &QString, allow: bool) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "SetAllowUntestedWrites", &(pid.to_string(), allow));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn known_devices(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::from("[]") };
        let r: zbus::Result<String> = call(&conn, "ListKnownDevices", &());
        QString::from(&self.report(r).unwrap_or_else(|| "[]".into()))
    }

    pub fn assignments(mut self: Pin<&mut Self>, pid: &QString, profile: i32, layer: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetAssignments", &(pid.to_string(), profile as u8, layer as u8));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_assignment(mut self: Pin<&mut Self>, pid: &QString, assignment_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "SetAssignment", &(pid.to_string(), assignment_json.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn refresh_cache(mut self: Pin<&mut Self>, pid: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "RefreshCache", &(pid.to_string(),));
        self.report(r).is_some()
    }

    pub fn macros(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetMacros", &());
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn save_macro(mut self: Pin<&mut Self>, macro_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "SaveMacro", &(macro_json.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn delete_macro(mut self: Pin<&mut Self>, guid: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "DeleteMacro", &(guid.to_string(),));
        self.report(r).is_some()
    }

    pub fn import_macro(mut self: Pin<&mut Self>, pid: &QString, id: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "ImportMacro", &(pid.to_string(), id.clamp(0, 0xffff) as u16));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn assign_macro(mut self: Pin<&mut Self>, pid: &QString, binding_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "AssignMacro", &(pid.to_string(), binding_json.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn performance(mut self: Pin<&mut Self>, pid: &QString, profile: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetPerformance", &(pid.to_string(), profile.clamp(1, 5) as u8));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn assign_action(mut self: Pin<&mut Self>, pid: &QString, binding_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "AssignAction", &(pid.to_string(), binding_json.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn actions(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::from("[]") };
        let r: zbus::Result<String> = call(&conn, "GetActions", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_else(|| "[]".into()))
    }

    pub fn app_links(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetAppLinks", &());
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_app_links(mut self: Pin<&mut Self>, links_json: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetAppLinks", &(links_json.to_string(),));
        self.report(r).is_some()
    }

    pub fn running_programs(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::from("[]") };
        let r: zbus::Result<Vec<String>> = call(&conn, "RunningPrograms", &());
        QString::from(&serde_json::to_string(&self.report(r).unwrap_or_default()).unwrap_or_default())
    }

    pub fn installed_games(&self) -> QString {
        QString::from(&serde_json::to_string(&tsuba_core::games::installed_games()).unwrap_or_else(|_| "[]".into()))
    }

    pub fn power(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetPower", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_power(mut self: Pin<&mut Self>, pid: &QString, settings_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<Vec<String>> = call(&conn, "SetPower", &(pid.to_string(), settings_json.to_string()));
        QString::from(&self.report(r).map(|v| v.join("\n")).unwrap_or_default())
    }

    pub fn copy_text(&self, text: &QString) {
        // wl-copy (Wayland) or xclip; clipboard is a convenience, failures are ignored.
        let t = text.to_string();
        for (cmd, args) in [("wl-copy", vec![]), ("xclip", vec!["-selection", "clipboard"])] {
            if let Ok(mut child) = std::process::Command::new(cmd).args(&args).stdin(std::process::Stdio::piped()).spawn() {
                use std::io::Write;
                if let Some(mut i) = child.stdin.take() {
                    let _ = i.write_all(t.as_bytes());
                }
                std::thread::spawn(move || child.wait());
                return;
            }
        }
    }

    pub fn chroma_profiles(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetChromaProfiles", &());
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn save_chroma_profile(mut self: Pin<&mut Self>, profile_json: &QString, apply: bool) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<Vec<String>> = call(&conn, "SaveChromaProfile", &(profile_json.to_string(), apply));
        self.report(r).is_some()
    }

    pub fn apply_chroma_profile(mut self: Pin<&mut Self>, name: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<Vec<String>> = call(&conn, "ApplyChromaProfile", &(name.to_string(),));
        self.report(r).is_some()
    }

    pub fn rename_chroma_profile(mut self: Pin<&mut Self>, old_name: &QString, new_name: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "RenameChromaProfile", &(old_name.to_string(), new_name.to_string()));
        self.report(r).is_some()
    }

    pub fn delete_chroma_profile(mut self: Pin<&mut Self>, name: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "DeleteChromaProfile", &(name.to_string(),));
        self.report(r).is_some()
    }

    pub fn preview_start(self: Pin<&mut Self>, pid: &QString, studio_json: &QString) -> bool {
        use tsuba_core::studio::{Geometry, Renderer, Studio};
        let Ok(studio) = serde_json::from_str::<Studio>(&studio_json.to_string()) else { return false };
        let pid = pid.to_string();
        let geo = if pid == "00aa" {
            Geometry::basilisk_v3_pro()
        } else {
            Geometry::from_pack(&tsuba_core::studio::pack_path(&pid), 9, 24).unwrap_or_else(|| Geometry::grid(9, 24))
        };
        // Keep the clock running across edits (and shared between devices) so animations don't restart.
        let mut me = self;
        let mut rust = me.as_mut().rust_mut();
        let start = rust
            .previews
            .get(&pid)
            .map(|p| p.1)
            .or_else(|| rust.previews.values().map(|p| p.1).min())
            .unwrap_or_else(std::time::Instant::now);
        rust.previews.insert(pid.clone(), (Renderer::new(studio, geo), start));
        rust.preview_last = pid;
        true
    }

    pub fn preview_frame(self: Pin<&mut Self>) -> QString {
        let pid = QString::from(&self.preview_last);
        self.preview_frame_for(&pid)
    }

    pub fn preview_frame_for(self: Pin<&mut Self>, pid: &QString) -> QString {
        let mut me = self;
        let mut rust = me.as_mut().rust_mut();
        let Some((r, start)) = rust.previews.get_mut(&pid.to_string()) else { return QString::from("{}") };
        let t = start.elapsed().as_secs_f32();
        let frame = r.frame(t);
        let keys: Vec<(u8, u8)> = r.geometry().pos.keys().copied().collect();
        let mut m = serde_json::Map::new();
        for (row, col) in keys {
            if let Some(c) = frame.get(row as usize).and_then(|x| x.get(col as usize)) {
                m.insert(format!("{row},{col}"), serde_json::Value::from(format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])));
            }
        }
        QString::from(&serde_json::Value::Object(m).to_string())
    }

    pub fn reset_ambient_screen(mut self: Pin<&mut Self>) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "ResetAmbientScreen", &());
        self.report(r).is_some()
    }

    pub fn ambient_preview(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetAmbientPreview", &());
        QString::from(&r.unwrap_or_default())
    }

    pub fn lighting_power(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetLightingPower", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_lighting_power(mut self: Pin<&mut Self>, pid: &QString, settings_json: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetLightingPower", &(pid.to_string(), settings_json.to_string()));
        self.report(r).is_some()
    }

    pub fn game_mode(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetGameMode", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_game_mode(mut self: Pin<&mut Self>, pid: &QString, settings_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "SetGameMode", &(pid.to_string(), settings_json.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn set_dial(mut self: Pin<&mut Self>, pid: &QString, slots_json: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<Vec<String>> = call(&conn, "SetDial", &(pid.to_string(), slots_json.to_string()));
        self.report(r).is_some()
    }

    pub fn set_dial_mode(mut self: Pin<&mut Self>, pid: &QString, slot: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetDialMode", &(pid.to_string(), slot.clamp(0, 7) as u8));
        self.report(r).is_some()
    }

    pub fn profiles(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "GetProfiles", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn create_profile(mut self: Pin<&mut Self>, pid: &QString, name: &QString, copy_from: i32) -> i32 {
        let Some(conn) = self.as_mut().conn() else { return -1 };
        let r: zbus::Result<u8> = call(&conn, "CreateProfile", &(pid.to_string(), name.to_string(), copy_from.clamp(0, 5) as u8));
        self.report(r).map_or(-1, i32::from)
    }

    pub fn rename_profile(mut self: Pin<&mut Self>, pid: &QString, id: i32, name: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "RenameProfile", &(pid.to_string(), id.clamp(1, 5) as u8, name.to_string()));
        self.report(r).is_some()
    }

    pub fn delete_profile(mut self: Pin<&mut Self>, pid: &QString, id: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "DeleteProfile", &(pid.to_string(), id.clamp(1, 5) as u8));
        self.report(r).is_some()
    }

    pub fn set_active_profile(mut self: Pin<&mut Self>, pid: &QString, id: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetActiveProfile", &(pid.to_string(), id.clamp(1, 5) as u8));
        self.report(r).is_some()
    }

    pub fn library_profiles(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "ListLibraryProfiles", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn create_library_profile(mut self: Pin<&mut Self>, pid: &QString, name: &QString, copy_from: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "CreateLibraryProfile", &(pid.to_string(), name.to_string(), copy_from.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn rename_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString, name: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "RenameLibraryProfile", &(pid.to_string(), id.to_string(), name.to_string()));
        self.report(r).is_some()
    }

    pub fn duplicate_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString, name: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<String> = call(&conn, "DuplicateLibraryProfile", &(pid.to_string(), id.to_string(), name.to_string()));
        QString::from(&self.report(r).unwrap_or_default())
    }

    pub fn delete_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "DeleteLibraryProfile", &(pid.to_string(), id.to_string()));
        self.report(r).is_some()
    }

    pub fn activate_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString) {
        let qt = self.qt_thread();
        let (pid, id) = (pid.to_string(), id.to_string());
        let conn = self.as_mut().conn();
        std::thread::spawn(move || {
            let r: zbus::Result<String> = match conn {
                Some(c) => call(&c, "ActivateLibraryProfile", &(pid.clone(), id)),
                None => Err(zbus::Error::Failure("not connected to tsubad".into())),
            };
            let (ok, text) = match r {
                Ok(j) => (true, j),
                Err(e) => (false, error_text(&e)),
            };
            let _ = qt.queue(move |mut o| o.as_mut().library_activated(QString::from(&pid), ok, QString::from(&text)));
        });
    }

    pub fn pin_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString, slot: i32) -> i32 {
        let Some(conn) = self.as_mut().conn() else { return -1 };
        let r: zbus::Result<u8> = call(&conn, "PinLibraryProfile", &(pid.to_string(), id.to_string(), slot.clamp(0, 255) as u8));
        self.report(r).map_or(-1, i32::from)
    }

    pub fn unpin_library_profile(mut self: Pin<&mut Self>, pid: &QString, id: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "UnpinLibraryProfile", &(pid.to_string(), id.to_string()));
        self.report(r).is_some()
    }

    pub fn set_performance(mut self: Pin<&mut Self>, pid: &QString, settings_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r: zbus::Result<Vec<String>> = call(&conn, "SetPerformance", &(pid.to_string(), settings_json.to_string()));
        QString::from(&self.report(r).map(|v| v.join("\n")).unwrap_or_default())
    }

    pub fn set_effect(mut self: Pin<&mut Self>, pid: &QString, effect_json: &QString, brightness: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let b = brightness.clamp(0, 255) as u8;
        let r: zbus::Result<Vec<String>> = call(&conn, "SetEffect", &(pid.to_string(), effect_json.to_string(), b));
        self.report(r).is_some()
    }

    pub fn save_lighting(mut self: Pin<&mut Self>, pid: &QString, effect_json: &QString, brightness: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let b = brightness.clamp(0, 255) as u8;
        let r: zbus::Result<()> = call(&conn, "SaveLighting", &(pid.to_string(), effect_json.to_string(), b));
        self.report(r).is_some()
    }

    pub fn config(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::from("{}") };
        let r: zbus::Result<String> = call(&conn, "GetConfig", &(pid.to_string(),));
        QString::from(&self.report(r).unwrap_or_else(|| "{}".into()))
    }

    pub fn set_brightness(mut self: Pin<&mut Self>, pid: &QString, percent: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetBrightness", &(pid.to_string(), percent.clamp(0, 100) as u8));
        self.report(r).is_some()
    }

    pub fn set_dpi(mut self: Pin<&mut Self>, pid: &QString, x: i32, y: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetDpi", &(pid.to_string(), x.clamp(100, 30000) as u16, y.clamp(100, 30000) as u16));
        self.report(r).is_some()
    }

    pub fn save(mut self: Pin<&mut Self>) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<Vec<String>> = call(&conn, "SaveFromDevices", &());
        self.report(r).is_some()
    }

    pub fn start_events(mut self: Pin<&mut Self>) {
        if self.events_started {
            return;
        }
        self.as_mut().rust_mut().events_started = true;
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let Ok(conn) = Connection::session() else { return };
            let rule = match zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(BUS)
                .and_then(|b| b.interface(IFACE))
            {
                Ok(b) => b.build(),
                Err(_) => return,
            };
            let Ok(iter) = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, None) else { return };
            for msg in iter.flatten() {
                let member = msg.header().member().map(|m| m.to_string()).unwrap_or_default();
                let queued = match member.as_str() {
                    "HardwareEvent" => {
                        let Ok((pid, ev)) = msg.body().deserialize::<(String, String)>() else { continue };
                        qt.queue(move |mut o| o.as_mut().hardware_event(QString::from(&pid), QString::from(&ev)))
                    }
                    "GameLightingChanged" => {
                        let Ok(status) = msg.body().deserialize::<String>() else { continue };
                        qt.queue(move |mut o| o.as_mut().game_lighting_changed(QString::from(&status)))
                    }
                    "LibraryChanged" => {
                        let Ok(pid) = msg.body().deserialize::<String>() else { continue };
                        qt.queue(move |mut o| o.as_mut().library_changed(QString::from(&pid)))
                    }
                    "DeviceAdded" | "DeviceRemoved" => qt.queue(|mut o| {
                        o.as_mut().refresh();
                        o.as_mut().devices_changed_on_bus();
                    }),
                    _ => continue,
                };
                if queued.is_err() {
                    break;
                }
            }
        });
    }

    pub fn open_system_settings(&self, page: &QString) -> bool {
        let module = match page.to_string().as_str() {
            "keyboard" => "kcm_keyboard",
            "mouse" => "kcm_mouse",
            _ => return false,
        };
        std::process::Command::new("systemsettings").arg(module).spawn().map(|mut c| {
            std::thread::spawn(move || c.wait());
        }).is_ok()
    }

    pub fn binary_replaced(&self) -> bool {
        std::fs::read_link("/proc/self/exe").is_ok_and(|p| p.to_string_lossy().ends_with(" (deleted)"))
    }

    pub fn device_pack(&self, pid: &QString) -> QString {
        let Some(base) = tsuba_core::devices::pack_dir(&pid.to_string()) else { return QString::default() };
        let Ok(text) = std::fs::read_to_string(base.join("device.json")) else { return QString::default() };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else { return QString::default() };
        v["dir"] = serde_json::Value::from(format!("file://{}/", base.display()));
        QString::from(&v.to_string())
    }

    pub fn studio_call(&self, method: &QString, arg: &QString) -> QString {
        QString::from(&crate::studio_api::call(&method.to_string(), &arg.to_string()))
    }

    pub fn has_personal_pack(&self, pid: &QString) -> bool {
        tsuba_core::devices::has_personal(&pid.to_string())
    }

    pub fn builtin_pack(&self, pid: &QString) -> QString {
        let Some(base) = tsuba_core::devices::builtin_dir(&pid.to_string()) else { return QString::default() };
        let Ok(text) = std::fs::read_to_string(base.join("device.json")) else { return QString::default() };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else { return QString::default() };
        v["dir"] = serde_json::Value::from(format!("file://{}/", base.display()));
        QString::from(&v.to_string())
    }

    pub fn has_custom_picture(&self, pid: &QString) -> bool {
        tsuba_core::devices::has_custom_picture(&pid.to_string())
    }

    pub fn set_custom_picture(mut self: Pin<&mut Self>, pid: &QString, view: &QString, file_url: &QString) -> bool {
        let path = file_url_to_path(&file_url.to_string());
        let r = tsuba_core::devices::set_custom_picture(&pid.to_string(), &view.to_string(), std::path::Path::new(&path));
        self.as_mut().picture_result(r)
    }

    pub fn set_picture_rect(mut self: Pin<&mut Self>, pid: &QString, view: &QString, x: f64, y: f64, w: f64, h: f64) -> bool {
        let r = tsuba_core::devices::set_picture_rect(&pid.to_string(), &view.to_string(), [x, y, w, h]);
        self.as_mut().picture_result(r)
    }

    pub fn remove_custom_picture(mut self: Pin<&mut Self>, pid: &QString) -> bool {
        let r = tsuba_core::devices::remove_custom_picture(&pid.to_string());
        self.as_mut().picture_result(r)
    }

    fn picture_result(mut self: Pin<&mut Self>, r: Result<(), String>) -> bool {
        match r {
            Ok(()) => true,
            Err(e) => {
                self.as_mut().set_status(QString::from(&format!("Device picture: {e}")));
                false
            }
        }
    }

    pub fn synapse_stores(&self) -> QString {
        QString::from(&crate::synapse::stores_json())
    }

    pub fn synapse_scan(self: Pin<&mut Self>, path: &QString) {
        let qt = self.qt_thread();
        let path = path.to_string();
        std::thread::spawn(move || {
            let json = crate::synapse::scan_json(&path);
            let _ = qt.queue(move |mut o| o.as_mut().synapse_scanned(QString::from(&json)));
        });
    }

    pub fn synapse_import(self: Pin<&mut Self>, path: &QString, selection_json: &QString) {
        let qt = self.qt_thread();
        let (path, sel) = (path.to_string(), selection_json.to_string());
        std::thread::spawn(move || {
            let q = qt.clone();
            let json = crate::synapse::import(&path, &sel, |line| {
                let _ = q.queue(move |mut o| o.as_mut().synapse_progress(QString::from(&line)));
            });
            let _ = qt.queue(move |mut o| {
                o.as_mut().refresh();
                o.as_mut().synapse_imported(QString::from(&json));
            });
        });
    }

    pub fn game_lighting(mut self: Pin<&mut Self>) -> QString {
        let Some(conn) = self.as_mut().conn() else {
            return QString::from(r#"{"available":false,"running":false,"enabled":false,"web":false,"status":{}}"#);
        };
        // Called quietly: an old daemon or a stopped one is shown on the page, not in the status bar.
        let enabled: zbus::Result<bool> = call(&conn, "GetGameLighting", &());
        let v = match enabled {
            Ok(enabled) => {
                let status: zbus::Result<String> = call(&conn, "GetGameLightingStatus", &());
                let status = status.ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or(serde_json::json!({}));
                // Older daemons have no web setting (and answer websites): report false.
                let web: bool = call::<_, bool>(&conn, "GetGameLightingWeb", &()).unwrap_or(false);
                serde_json::json!({ "available": true, "running": true, "enabled": enabled, "web": web, "status": status })
            }
            Err(e) => {
                let unknown = matches!(&e, zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod");
                serde_json::json!({ "available": false, "running": unknown, "enabled": false, "web": false, "status": {} })
            }
        };
        QString::from(&v.to_string())
    }

    pub fn set_game_lighting(mut self: Pin<&mut Self>, enabled: bool) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetGameLighting", &(enabled,));
        self.report(r).is_some()
    }

    pub fn set_game_lighting_web(mut self: Pin<&mut Self>, enabled: bool) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetGameLightingWeb", &(enabled,));
        self.report(r).is_some()
    }

    pub fn game_lighting_prefixes(&self) -> QString {
        use tsuba_core::game_lighting_install as gli;
        let dll_dir = gli::default_dll_dir();
        let list = gli::prefixes();
        let mut settings = gli::load_settings();
        let mut auto_installed = Vec::new();
        if let Some(dir) = &dll_dir {
            let before = settings.known.len();
            auto_installed = gli::auto_install_new(&list, dir, &mut settings);
            if settings.known.len() != before {
                let _ = gli::save_settings(&settings);
            }
        }
        let prefixes: Vec<_> = list
            .iter()
            .map(|p| {
                let mut v = serde_json::to_value(p).unwrap_or_default();
                v["state"] = serde_json::to_value(gli::state(&p.path, dll_dir.as_deref())).unwrap_or_default();
                v["custom"] = serde_json::Value::Bool(settings.custom.iter().any(|c| std::path::Path::new(c) == p.path));
                v
            })
            .collect();
        let v = serde_json::json!({
            "dll_dir": dll_dir.map(|d| d.display().to_string()).unwrap_or_default(),
            "auto": settings.auto_install,
            "prefixes": prefixes,
            "auto_installed": auto_installed,
        });
        QString::from(&v.to_string())
    }

    pub fn game_lighting_install(&self, ids_json: &QString) -> QString {
        game_lighting_apply(&ids_json.to_string(), true)
    }

    pub fn game_lighting_uninstall(&self, ids_json: &QString) -> QString {
        game_lighting_apply(&ids_json.to_string(), false)
    }

    pub fn game_lighting_add_prefix(&self, path: &QString) -> QString {
        use tsuba_core::game_lighting_install as gli;
        let raw = file_url_to_path(&path.to_string());
        let Some(pfx) = gli::resolve_prefix(std::path::Path::new(&raw)) else {
            return QString::from("That folder isn't a Wine prefix (it has no drive_c/windows/system32).");
        };
        let mut s = gli::load_settings();
        let p = pfx.display().to_string();
        if !s.custom.contains(&p) {
            s.custom.push(p);
        }
        match gli::save_settings(&s) {
            Ok(()) => QString::default(),
            Err(e) => QString::from(&format!("Couldn't save the setting: {e:#}")),
        }
    }

    pub fn set_game_lighting_auto(&self, on: bool) -> bool {
        use tsuba_core::game_lighting_install as gli;
        let mut s = gli::load_settings();
        s.auto_install = on;
        gli::save_settings(&s).is_ok()
    }

    pub fn game_lighting_test(mut self: Pin<&mut Self>) {
        // The server's port comes from the daemon's status (it can differ from the default).
        let port = self
            .as_mut()
            .conn()
            .and_then(|c| call::<_, String>(&c, "GetGameLightingStatus", &()).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["port"].as_u64())
            .filter(|p| *p > 0 && *p < 65536)
            .map(|p| p as u16)
            .unwrap_or(54235);
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let r = tsuba_core::game_lighting_install::test_pattern(port, 3.0);
            let (ok, msg) = match r {
                Ok(m) => (true, m),
                Err(e) => (false, format!("{e:#}")),
            };
            let _ = qt.queue(move |mut o| o.as_mut().game_lighting_tested(ok, QString::from(&msg)));
        });
    }

    pub fn env_var(&self, name: &QString) -> QString {
        QString::from(&std::env::var(name.to_string()).unwrap_or_default())
    }
}

/// The demo device's support JSON: the database entry, untested and attached, writes blocked.
fn fake_support(text: &str) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(text).ok()?;
    let o = v.as_object_mut()?;
    o.insert("tested".into(), false.into());
    o.insert("attached".into(), true.into());
    o.insert("allow_writes".into(), false.into());
    o.insert("allow_untested_writes".into(), false.into());
    o.insert("approximate_art".into(), true.into());
    Some(v.to_string())
}

/// The demo device's state, built from the database entry (no hardware reads).
fn fake_state(conn: &Connection, pid: &str) -> String {
    let Ok(text) = call::<_, String>(conn, "GetDeviceSupport", &(pid.to_string(),)) else { return String::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return String::new() };
    let supported: Vec<String> = v["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e.as_str())
        .filter(|e| *e != "custom_frame")
        .map(|e| {
            let mut c = e.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect();
    let (rows, cols) = v["matrix"].as_array().map_or((1, 1), |m| (m[0].as_u64().unwrap_or(1), m[1].as_u64().unwrap_or(1)));
    let lighting = if supported.is_empty() {
        serde_json::json!([])
    } else {
        serde_json::json!([{ "id": 0, "name": "Lighting", "rows": rows, "cols": cols, "effect": "Off",
            "colours": [], "brightness": 100, "supported": supported }])
    };
    serde_json::json!({ "pid": pid, "name": v["name"], "kind": v["kind"], "firmware": "", "mode": "",
        "lighting": lighting, "dpi": null, "dial": null })
    .to_string()
}

/// `file:///a/b%20c.png` (as Qt hands it over) or a plain path -> a filesystem path.
fn file_url_to_path(url: &str) -> String {
    let p = url.strip_prefix("file://").unwrap_or(url);
    let b = p.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
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

/// Install or remove the game lighting bridge in the prefixes named by a JSON list of ids
/// (`["all"]` = every detected prefix without a foreign library). Returns {"ok", "message"}.
fn game_lighting_apply(ids_json: &str, install: bool) -> QString {
    use tsuba_core::game_lighting_install as gli;
    let ids: Vec<String> = serde_json::from_str(ids_json).unwrap_or_default();
    let all = gli::prefixes();
    let chosen: Vec<&gli::Prefix> = if ids.iter().any(|i| i == "all") {
        all.iter().filter(|p| gli::state(&p.path, None) != gli::State::Foreign).collect()
    } else {
        all.iter().filter(|p| ids.contains(&p.id)).collect()
    };
    let dll_dir = gli::default_dll_dir();
    if install && dll_dir.is_none() {
        return QString::from(&serde_json::json!({ "ok": false, "message": "The bridge files aren't installed (expected in /usr/share/tsuba/game-lighting)." }).to_string());
    }
    let (mut changed, mut skipped, mut errors) = (0usize, Vec::new(), Vec::new());
    for p in &chosen {
        let r = match &dll_dir {
            Some(d) if install => gli::install(&p.path, d),
            _ => gli::uninstall(&p.path),
        };
        match r {
            Ok(rep) => {
                changed += usize::from(!rep.changed.is_empty());
                skipped.extend(rep.skipped.into_iter().filter(|(_, why)| !why.starts_with("already")).map(|(f, why)| format!("{}: {f} — {why}", p.name)));
            }
            Err(e) => errors.push(format!("{}: {e:#}", p.name)),
        }
    }
    let verb = if install { "Installed in" } else { "Removed from" };
    let mut msg = format!("{verb} {changed} of {} game{}.", chosen.len(), if chosen.len() == 1 { "" } else { "s" });
    for line in skipped.iter().chain(errors.iter()) {
        msg.push('\n');
        msg.push_str(line);
    }
    QString::from(&serde_json::json!({ "ok": errors.is_empty(), "message": msg }).to_string())
}
