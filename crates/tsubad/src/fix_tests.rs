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

#[test]
fn focus_changes_wake_the_link_thread_instead_of_spawning_one_each() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let dir = std::env::temp_dir().join(format!("tsuba-wake-test-{}", std::process::id()));
    let mgr = crate::manager::Manager::new(false, dir.join("config.toml"), tx).unwrap();
    for i in 0..200 {
        mgr.focus_changed(tsuba_core::app_links::FocusInfo { pid: i, ..Default::default() });
    }
    assert!(mgr.link_wait(std::time::Duration::from_secs(5)), "woken by the focus change");
    assert!(!mgr.link_wait(std::time::Duration::from_millis(10)), "a burst is one wake-up");
    assert_eq!(mgr.actions.focus.lock().unwrap().as_ref().map(|f| f.pid), Some(199));
}

mod resets {
    use super::*;
    use tsuba_core::config::GameModeConfig;
    use tsuba_core::library::{self as lib, DeviceLibrary};


    #[test]
    fn resets_resync_the_library_and_game_mode() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, sim) = rig("028d", 1);
        mgr.library_migrate("028d").unwrap();
        let id = mgr.library_create("028d", "Game", "").unwrap();
        mgr.library_activate("028d", &id).unwrap();
        mgr.set_game_mode("028d", GameModeConfig { disable_win: true, block_shortcuts: false, active: true, ..Default::default() }).unwrap();
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127].0, 0, "Win key off");
        let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
        assert_eq!(l.holder(1).map(|p| p.id.clone()), Some(id.clone()));
        let phrase = crate::dbus_device::profile_reset_phrase(1);
        mgr.reset_profile("028d", 1, &phrase).unwrap();
        let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
        assert!(l.holder(1).is_none(), "the slot no longer holds the profile");
        let gm = mgr.game_mode("028d");
        assert!(gm.active);
        // Game Mode keeps the key off in the reset slot and records the reset slot's binding.
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127].0, 0);
        assert_eq!(gm.saved.len(), 1);
        // Factory reset: all bookkeeping goes, and Game Mode is turned off in the new slot 1 again.
        let name = crate::dbus_device::factory_reset_phrase(&mgr.model_for("028d").unwrap());
        mgr.factory_reset("028d", &name).unwrap();
        let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
        assert!(l.state.holds.is_empty() && l.state.seen.is_empty());
        let gm = mgr.game_mode("028d");
        assert!(gm.active && gm.saved.len() == 1);
        assert_eq!(sim.lock().unwrap().profiles[&1].tables[0][&127].0, 0);
        // The library learns the device again.
        mgr.library_migrate("028d").unwrap();
        assert!(!DeviceLibrary::load(&lib::root(), "028d").unwrap().state.seen.is_empty());
    }
}

mod imports {
    use super::*;
    use tsuba_core::config::Action;
    use tsuba_core::library::{self as lib, ActionBinding, DeviceLibrary, LibraryProfile};

    fn file(name: &str, body: &str) -> String {
        let p = crate::library::tests::temp_config().join(name);
        std::fs::write(&p, body).unwrap();
        p.to_str().unwrap().to_string()
    }

    fn profile_file(name: &str, p: &LibraryProfile) -> String {
        file(name, &lib::export("028d", p).unwrap())
    }

    fn with_commands() -> LibraryProfile {
        let mut p = LibraryProfile::new("Evil\u{1b}[31m profile\n");
        p.actions = vec![
            ActionBinding { layer: 0, key: 160, action: Action::Launch { command: "touch /tmp/x".into() } },
            ActionBinding { layer: 0, key: 161, action: Action::Url { url: "https://example.org".into() } },
            ActionBinding { layer: 1, key: 162, action: Action::Audio { op: tsuba_core::config::AudioOp::VolumeUp } },
        ];
        p
    }

    #[test]
    fn imported_commands_need_approval_and_names_are_clean() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("028d", 1);
        let path = profile_file("cmds.tsuba-profile.json", &with_commands());
        let pre = mgr.library_import_preview("028d", &path).unwrap();
        assert_eq!(pre["name"], "Evil[31m profile");
        assert_eq!(pre["software"].as_array().unwrap().len(), 2, "launch and url, not audio");
        assert_eq!(pre["software"][0]["text"], "touch /tmp/x");
        // Without approval the commands are left out, the harmless action stays.
        let id = mgr.library_import("028d", &path, "").unwrap();
        let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
        let p = l.get(&id).unwrap();
        assert_eq!(p.name, "Evil[31m profile");
        assert_eq!(p.actions.len(), 1);
        assert!(matches!(p.actions[0].action, Action::Audio { .. }));
        // With the digest the user saw, they are kept.
        let digest = pre["digest"].as_str().unwrap();
        let id2 = mgr.library_import("028d", &path, digest).unwrap();
        assert_eq!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&id2).unwrap().actions.len(), 3);
        // A file that changed after the preview is refused, not imported with unseen commands.
        let mut changed = with_commands();
        changed.actions[0] = ActionBinding { layer: 0, key: 160, action: Action::Launch { command: "rm -rf ~".into() } };
        let path2 = profile_file("cmds.tsuba-profile.json", &changed);
        assert!(mgr.library_import("028d", &path2, digest).is_err());
    }

    #[test]
    fn import_files_are_bounded_regular_files() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (mgr, _sim) = rig("028d", 1);
        let big = file("big.json", &" ".repeat(lib::MAX_IMPORT_BYTES as usize + 10));
        assert!(mgr.library_import("028d", &big, "").unwrap_err().to_string().contains("too big"));
        // A FIFO with no writer would block a plain read forever.
        let fifo = crate::library::tests::temp_config().join("pipe.json");
        let _ = std::fs::remove_file(&fifo);
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        let t0 = std::time::Instant::now();
        assert!(mgr.library_import("028d", fifo.to_str().unwrap(), "").is_err());
        assert!(t0.elapsed() < std::time::Duration::from_secs(2));
        assert!(mgr.library_import("028d", "/dev/zero", "").is_err());
        assert!(mgr.library_import("028d", "/dev/null", "").is_err());
        let long = LibraryProfile::new(&"x".repeat(500));
        let id = mgr.library_import("028d", &profile_file("long.json", &long), "").unwrap();
        assert_eq!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&id).unwrap().name.chars().count(), lib::MAX_NAME_CHARS);
    }
}
