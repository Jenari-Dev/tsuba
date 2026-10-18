//! QML-facing client of tsubad's `io.github.JenAriDev.Tsuba1.Device` interface: firmware
//! features (probe-gated), resets, mouse zones, lighting on every device, lighting on battery.
//! Instantiated by the device pages that use it (`DeviceBackend {}`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        type DeviceBackend = super::DeviceBackendRust;

        /// Feature values for one on-board profile as JSON ("" on error; see `status`).
        #[qinvokable]
        fn features(self: Pin<&mut DeviceBackend>, pid: &QString, profile: i32) -> QString;

        /// Run the read-only feature probe again; the report JSON ("" on error).
        #[qinvokable]
        fn probe(self: Pin<&mut DeviceBackend>, pid: &QString) -> QString;

        /// Apply feature settings JSON; the change log joined by newlines ("" on error).
        #[qinvokable]
        fn set_features(self: Pin<&mut DeviceBackend>, pid: &QString, settings_json: &QString) -> QString;

        /// Reset an on-board profile ("" on error). `confirm` is the typed phrase.
        #[qinvokable]
        fn reset_profile(self: Pin<&mut DeviceBackend>, pid: &QString, slot: i32, confirm: &QString) -> QString;

        /// Factory reset ("" on error). `confirm` is the typed device name.
        #[qinvokable]
        fn factory_reset(self: Pin<&mut DeviceBackend>, pid: &QString, confirm: &QString) -> QString;

        /// One zone's effect (config JSON) and brightness (255 = keep); false on error.
        #[qinvokable]
        fn set_zone_effect(self: Pin<&mut DeviceBackend>, pid: &QString, region: i32, effect_json: &QString, brightness: i32) -> bool;

        #[qinvokable]
        fn set_zone_brightness(self: Pin<&mut DeviceBackend>, pid: &QString, region: i32, percent: i32) -> bool;

        #[qinvokable]
        fn clear_zones(self: Pin<&mut DeviceBackend>, pid: &QString) -> bool;

        /// A built-in effect on every attached device; the log joined by newlines ("" on error).
        #[qinvokable]
        fn set_effect_all(self: Pin<&mut DeviceBackend>, effect_json: &QString, brightness: i32) -> QString;

        #[qinvokable]
        fn set_brightness_all(self: Pin<&mut DeviceBackend>, percent: i32) -> QString;

        /// Lighting on battery JSON ("null" = same as plugged in; "" on error).
        #[qinvokable]
        fn battery_lighting(self: Pin<&mut DeviceBackend>, pid: &QString) -> QString;

        /// Set ("{effect, brightness}") or clear ("null") the lighting on battery.
        #[qinvokable]
        fn set_battery_lighting(self: Pin<&mut DeviceBackend>, pid: &QString, lighting_json: &QString) -> bool;
    }
}

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use zbus::blocking::Connection;

const BUS: &str = "io.github.JenAriDev.Tsuba";
const PATH: &str = "/io/github/JenAriDev/Tsuba";
const IFACE: &str = "io.github.JenAriDev.Tsuba1.Device";

#[derive(Default)]
pub struct DeviceBackendRust {
    status: QString,
    conn: Option<Connection>,
}

/// The demo device (`TSUBA_FAKE_DEVICE`) only ever reads.
fn demo() -> bool {
    std::env::var("TSUBA_FAKE_DEVICE").is_ok_and(|s| !s.is_empty())
}

fn call<B, R>(conn: &Connection, method: &str, body: &B) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    if demo() && !method.starts_with("Get") {
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

fn u8_of(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

impl qobject::DeviceBackend {
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
                self.as_mut().set_status(QString::from(&error_text(&e)));
                None
            }
        }
    }

    fn text(self: Pin<&mut Self>, r: zbus::Result<String>) -> QString {
        QString::from(&self.report(r).unwrap_or_default())
    }

    fn lines(self: Pin<&mut Self>, r: zbus::Result<Vec<String>>) -> QString {
        QString::from(&self.report(r).map(|v| v.join("\n")).unwrap_or_default())
    }

    pub fn features(mut self: Pin<&mut Self>, pid: &QString, profile: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "GetFeatures", &(pid.to_string(), u8_of(profile).max(1)));
        self.text(r)
    }

    pub fn probe(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        // A read-only probe: allowed for the demo device too.
        let r = conn.call_method(Some(BUS), PATH, Some(IFACE), "ProbeFeatures", &(pid.to_string(),)).and_then(|m| m.body().deserialize());
        self.text(r)
    }

    pub fn set_features(mut self: Pin<&mut Self>, pid: &QString, settings_json: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "SetFeatures", &(pid.to_string(), settings_json.to_string()));
        self.lines(r)
    }

    pub fn reset_profile(mut self: Pin<&mut Self>, pid: &QString, slot: i32, confirm: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "ResetProfile", &(pid.to_string(), u8_of(slot), confirm.to_string()));
        self.text(r)
    }

    pub fn factory_reset(mut self: Pin<&mut Self>, pid: &QString, confirm: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "FactoryReset", &(pid.to_string(), confirm.to_string()));
        self.text(r)
    }

    pub fn set_zone_effect(mut self: Pin<&mut Self>, pid: &QString, region: i32, effect_json: &QString, brightness: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<Vec<String>> = call(&conn, "SetZoneEffect", &(pid.to_string(), u8_of(region), effect_json.to_string(), u8_of(brightness)));
        self.report(r).is_some()
    }

    pub fn set_zone_brightness(mut self: Pin<&mut Self>, pid: &QString, region: i32, percent: i32) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetZoneBrightness", &(pid.to_string(), u8_of(region), u8_of(percent).min(100)));
        self.report(r).is_some()
    }

    pub fn clear_zones(mut self: Pin<&mut Self>, pid: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "ClearZones", &(pid.to_string(),));
        self.report(r).is_some()
    }

    pub fn set_effect_all(mut self: Pin<&mut Self>, effect_json: &QString, brightness: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "SetEffectAll", &(effect_json.to_string(), u8_of(brightness)));
        self.lines(r)
    }

    pub fn set_brightness_all(mut self: Pin<&mut Self>, percent: i32) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "SetBrightnessAll", &(u8_of(percent).min(100),));
        self.lines(r)
    }

    pub fn battery_lighting(mut self: Pin<&mut Self>, pid: &QString) -> QString {
        let Some(conn) = self.as_mut().conn() else { return QString::default() };
        let r = call(&conn, "GetBatteryLighting", &(pid.to_string(),));
        self.text(r)
    }

    pub fn set_battery_lighting(mut self: Pin<&mut Self>, pid: &QString, lighting_json: &QString) -> bool {
        let Some(conn) = self.as_mut().conn() else { return false };
        let r: zbus::Result<()> = call(&conn, "SetBatteryLighting", &(pid.to_string(), lighting_json.to_string()));
        self.report(r).is_some()
    }
}
