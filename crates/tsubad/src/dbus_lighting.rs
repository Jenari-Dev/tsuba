//! Lighting-related D-Bus methods that live on their own interface,
//! `io.github.JenAriDev.Tsuba1.Lighting`, at the same object path as the main one:
//! who controls a device's lighting, which applications may control it, audio sources.

use std::sync::Arc;

use serde_json::json;
use zbus::{fdo, interface};

use crate::manager::Manager;

pub struct Lighting {
    pub mgr: Arc<Manager>,
    pub game: Arc<crate::game_lighting::GameLighting>,
}

fn err(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(format!("{e:#}"))
}

impl Lighting {
    /// Who controls `pid`'s lighting right now, as JSON `{"owner": "game" | "studio" | "device",
    /// "name": …, "link": …}`: a game over the Chroma interface (name = its title), the
    /// Lighting Studio (name = the applied profile), or the device's own effect. `link` is the
    /// linked game whose profile is active, if any.
    pub fn owner(&self, pid: &str) -> serde_json::Value {
        let link = self.mgr.link_active();
        if self.mgr.game_holds(pid) {
            let status = self.game.status();
            let title = status["sessions"]
                .as_array()
                .and_then(|s| s.iter().rev().find(|x| x["devices_active"].as_array().is_some_and(|d| d.iter().any(|p| p == pid))))
                .and_then(|x| x["title"].as_str().map(String::from))
                .unwrap_or_default();
            return json!({ "owner": "game", "name": title, "link": link });
        }
        if self.mgr.studio_running(pid) {
            let name = tsuba_core::chroma_profiles::ChromaProfiles::load(&tsuba_core::chroma_profiles::path()).map(|l| l.active).unwrap_or_default();
            return json!({ "owner": "studio", "name": name, "link": link });
        }
        json!({ "owner": "device", "name": "", "link": link })
    }
}

#[interface(name = "io.github.JenAriDev.Tsuba1.Lighting")]
impl Lighting {
    async fn get_lighting_owner(&self, pid: String) -> String {
        let me = Lighting { mgr: self.mgr.clone(), game: self.game.clone() };
        tokio::task::spawn_blocking(move || me.owner(&pid).to_string()).await.unwrap_or_default()
    }

    /// Per-application game-lighting rules and the applications seen so far (JSON, see
    /// `GameLighting::rules_json`).
    async fn get_game_lighting_apps(&self) -> String {
        self.game.rules_json().to_string()
    }

    /// Allow, block or reset ("allow" / "block" / "default") one application, by its key.
    async fn set_game_lighting_app(&self, key: String, rule: String) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        let game = self.game.clone();
        tokio::task::spawn_blocking(move || game.set_app_rule(&key, &rule)).await.map_err(|e| fdo::Error::Failed(e.to_string()))?.map_err(err)
    }

    /// true = only applications marked "allow" may control the lighting; false = all but blocked.
    async fn set_game_lighting_only_allowed(&self, on: bool) -> fdo::Result<()> {
        self.mgr.ensure_writable().map_err(err)?;
        let game = self.game.clone();
        tokio::task::spawn_blocking(move || game.set_only_allowed(on)).await.map_err(|e| fdo::Error::Failed(e.to_string()))?.map_err(err)
    }

    /// Audio sources the Audio Meter can listen to: JSON `[{"name", "description", "monitor"}]`.
    async fn get_audio_sources(&self) -> String {
        tokio::task::spawn_blocking(|| serde_json::to_string(&crate::audio::sources()).unwrap_or_else(|_| "[]".into())).await.unwrap_or_else(|_| "[]".into())
    }
}
