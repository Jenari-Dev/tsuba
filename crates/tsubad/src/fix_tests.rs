//! Tests for the post-merge review fixes, on the simulated devices.

use std::sync::atomic::Ordering::SeqCst;
use tsuba_core::config::{DeviceConfig, GameModeConfig};

use crate::gamemode::fake_kde::{self, SERIAL};
use crate::library::tests::rig;

#[test]
fn game_mode_desktop_block_is_merged_across_devices() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    fake_kde::FAIL.store(false, SeqCst);
    let (mgr, _sim) = rig("028d", 1);
    // Another device has Game Mode on with the full block, and the desktop shows it.
    {
        let mut c = mgr.config.lock().unwrap();
        c.upsert(DeviceConfig { pid: "00aa".into(), game_mode: Some(GameModeConfig { active: true, block_shortcuts: true, disable_win: false, ..Default::default() }), ..Default::default() });
    }
    mgr.restore_shortcut_block();
    assert!(fake_kde::BLOCKED.load(SeqCst));
    // This device asks for just Alt+F4: the other device's full block must stay.
    let want = GameModeConfig { active: true, block_shortcuts: true, disable_win: false, blocked_combos: Some(vec!["alt_f4".into()]), ..Default::default() };
    mgr.set_game_mode("028d", want).unwrap();
    assert!(fake_kde::BLOCKED.load(SeqCst), "the merged block is still the full one");
    // Both off: unblocked.
    mgr.set_game_mode("028d", GameModeConfig { active: false, disable_win: false, ..Default::default() }).unwrap();
    {
        let mut c = mgr.config.lock().unwrap();
        c.devices.iter_mut().find(|d| d.pid == "00aa").unwrap().game_mode.as_mut().unwrap().active = false;
    }
    mgr.restore_shortcut_block();
    assert!(!fake_kde::BLOCKED.load(SeqCst));
}

mod lighting {
    use super::*;
    use tsuba_core::app_links::{AppLink, AppLinks, ProfileRef};
    use tsuba_core::config::{EffectConfig, Lighting};
    use tsuba_core::library::{self as lib, DeviceLibrary};

    fn st(colour: &str, b: u8) -> Lighting {
        Lighting { effect: EffectConfig::Static { colour: colour.into() }, brightness: Some(b) }
    }

    fn ids(mgr: &crate::manager::Manager, pid: &str) -> Vec<String> {
        mgr.library_migrate(pid).unwrap();
        DeviceLibrary::load(&lib::root(), pid).unwrap().profiles.iter().map(|p| p.id.clone()).collect()
    }

    fn saved(mgr: &crate::manager::Manager, pid: &str) -> (Option<Lighting>, std::collections::BTreeMap<String, Lighting>) {
        mgr.config.lock().unwrap().device(pid).map(|d| (d.lighting.clone(), d.zones.clone())).unwrap_or_default()
    }

    #[test]
    fn a_linked_games_profile_lighting_goes_with_the_game() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 2);
        let v = ids(&mgr, "028d");
        let (a, b) = (v[0].clone(), v[1].clone());
        mgr.library_set_lighting("028d", &b, &serde_json::to_string(&st("ff0000", 60)).unwrap()).unwrap();
        mgr.library_activate("028d", &a).unwrap();
        mgr.set_effect_all(EffectConfig::Static { colour: "0000ff".into() }, Some(40)).unwrap();
        let before_cfg = saved(&mgr, "028d");
        let before_dev = sim.lock().unwrap().feat.lighting.clone();
        let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        let link = AppLink { name: "Me".into(), matches: me, enabled: true, device_profiles: [("028d".to_string(), ProfileRef::Library(b.clone()))].into(), ..Default::default() };
        mgr.set_app_links(AppLinks { links: vec![link] }).unwrap();
        mgr.link_tick();
        assert_eq!(mgr.link_active().as_deref(), Some("Me"));
        assert_eq!(saved(&mgr, "028d").0, Some(st("ff0000", 60)), "the game's profile lighting is in force");
        assert_ne!(sim.lock().unwrap().feat.lighting, before_dev);
        mgr.set_app_links(AppLinks::default()).unwrap();
        mgr.link_tick();
        assert_eq!(saved(&mgr, "028d"), before_cfg, "the saved lighting is back in the config");
        assert_eq!(sim.lock().unwrap().feat.lighting, before_dev, "and on the device");
    }

    #[test]
    fn zones_follow_game_lighting_and_the_profile_lighting_mirror() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 2);
        let v = ids(&mgr, "028d");
        let (a, b) = (v[0].clone(), v[1].clone());
        mgr.library_set_lighting("028d", &b, &serde_json::to_string(&st("ff0000", 60)).unwrap()).unwrap();
        mgr.library_activate("028d", &b).unwrap();
        // Lighting-to-all lands in the active profile that carries lighting.
        mgr.set_effect_all(EffectConfig::Static { colour: "00ff00".into() }, Some(70)).unwrap();
        assert_eq!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&b).unwrap().lighting, Some(st("00ff00", 70)));
        mgr.set_brightness_all(30).unwrap();
        assert_eq!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&b).unwrap().lighting, Some(st("00ff00", 30)));
        // What a zone looks like on the device when applied plainly.
        mgr.set_zone("028d", 5, st("0000ff", 80)).unwrap();
        let expected = sim.lock().unwrap().feat.lighting.clone();
        mgr.set_effect_all(EffectConfig::Static { colour: "00ff00".into() }, Some(30)).unwrap();
        mgr.set_zone("028d", 5, st("0000ff", 80)).unwrap();
        // A game holds the device: the zone is saved and comes back with it.
        mgr.game_take("028d").unwrap();
        mgr.set_zone("028d", 5, st("0000ff", 80)).unwrap();
        mgr.library_set_lighting("028d", &a, "off").unwrap();
        sim.lock().unwrap().feat.lighting.clear();
        mgr.game_release("028d");
        assert_eq!(sim.lock().unwrap().feat.lighting, expected, "the zone is applied after the game, not just the base lighting");
        // Applying a profile's lighting replaces the zones.
        assert!(!mgr.config.lock().unwrap().device("028d").unwrap().zones.is_empty());
        mgr.library_set_lighting("028d", &b, &serde_json::to_string(&st("ff00ff", 20)).unwrap()).unwrap();
        mgr.library_activate("028d", &a).unwrap();
        mgr.library_activate("028d", &b).unwrap();
        assert!(mgr.config.lock().unwrap().device("028d").unwrap().zones.is_empty());
    }
}
