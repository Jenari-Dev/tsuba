//! Opt-in: put the game lighting bridge into newly appeared Wine/Proton prefixes. Runs only when
//! the user turned on "Install automatically for new games" on the Game lighting page
//! (~/.config/tsuba/game-lighting.json); the installer never touches a library it didn't write.

use std::time::Duration;

use tracing::{info, warn};
use tsuba_core::game_lighting_install as gli;

const PERIOD: Duration = Duration::from_secs(120);

pub fn spawn() {
    let _ = std::thread::Builder::new().name("game-autoinstall".into()).spawn(|| loop {
        std::thread::sleep(PERIOD);
        let mut settings = gli::load_settings();
        if !settings.auto_install {
            continue;
        }
        let Some(dll_dir) = gli::default_dll_dir() else { continue };
        let before = settings.known.len();
        let done = gli::auto_install_new(&gli::prefixes(), &dll_dir, &mut settings);
        for id in &done {
            info!("game lighting: installed the bridge into new prefix {id}");
        }
        if settings.known.len() != before {
            if let Err(e) = gli::save_settings(&settings) {
                warn!("game lighting: saving auto-install state: {e:#}");
            }
        }
    });
}
