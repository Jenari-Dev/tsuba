//! The app-facing part of the D-Bus API that is not about a device: notification settings and
//! daemon info. Same bus name and object path as [`crate::dbus`], its own interface so the two
//! can grow independently.

use std::sync::Arc;

use serde_json::json;
use zbus::{fdo, interface};

use crate::notify::{Notifier, Settings};

pub struct App {
    pub notifier: Arc<Notifier>,
    pub read_only: bool,
}

#[interface(name = "io.github.JenAriDev.Tsuba1.App")]
impl App {
    /// Notification settings as JSON (see [`Settings`]).
    async fn get_notification_settings(&self) -> String {
        serde_json::to_string(&self.notifier.settings()).unwrap_or_default()
    }

    /// Replace the notification settings; returns what was stored (values are clamped).
    async fn set_notification_settings(&self, settings: String) -> fdo::Result<String> {
        let s: Settings = serde_json::from_str(&settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let stored = self.notifier.set_settings(s).map_err(|e| fdo::Error::Failed(format!("{e:#}")))?;
        Ok(serde_json::to_string(&stored).unwrap_or_default())
    }

    /// Show one notification now ("Send a test notification").
    async fn send_test_notification(&self) {
        self.notifier.test();
    }

    /// Daemon version, mode and process id as JSON.
    async fn get_info(&self) -> String {
        json!({ "version": env!("CARGO_PKG_VERSION"), "read_only": self.read_only, "pid": std::process::id() }).to_string()
    }
}
