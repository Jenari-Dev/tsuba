//! Tests of the customize / macro / profile / linked-game work (WP-C) on the simulated devices.

use serde_json::Value;
use tsuba_core::app_links::{AppLink, AppLinks, FocusInfo};
use tsuba_core::config::{Action, DeviceOp, EffectConfig, Lighting};
use tsuba_core::library::{self as lib, DeviceLibrary, MacroBinding};
use tsuba_core::macros::{self, Macro, MacroEvent, Playback};

use crate::gamemode::fake_kde::SERIAL;
use crate::library::tests::rig;
use crate::manager::Manager;

fn red() -> Lighting {
    Lighting { effect: EffectConfig::Static { colour: "ff0000".into() }, brightness: Some(60) }
}

fn blue() -> Lighting {
    Lighting { effect: EffectConfig::Static { colour: "0000ff".into() }, brightness: Some(40) }
}

fn profile_ids(mgr: &Manager, pid: &str) -> Vec<String> {
    mgr.library_migrate(pid).unwrap();
    DeviceLibrary::load(&lib::root(), pid).unwrap().profiles.iter().map(|p| p.id.clone()).collect()
}

fn saved_lighting(mgr: &Manager, pid: &str) -> Option<Lighting> {
    mgr.config.lock().unwrap().device(pid).and_then(|d| d.lighting.clone())
}

#[test]
fn lighting_is_stored_with_a_profile_applied_on_activation_and_follows_edits() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("028d", 2);
    let ids = profile_ids(&mgr, "028d");
    let (a, b) = (ids[0].clone(), ids[1].clone());
    mgr.library_activate("028d", &a).unwrap();
    // Store what the device shows now (the saved lighting, as the simulated device has no lighting).
    mgr.store_lighting("028d", red()).unwrap();
    mgr.library_set_lighting("028d", &a, "current").unwrap();
    let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
    assert_eq!(l.get(&a).unwrap().lighting, Some(red()));
    assert!(l.get(&b).unwrap().lighting.is_none());
    let listed: Value = serde_json::from_str(&mgr.library_json("028d").unwrap()).unwrap();
    assert_eq!(listed["profiles"][0]["lighting"]["effect"], "static");
    assert!(listed["profiles"][1]["lighting"].is_null());

    // Switching to a profile without lighting leaves the lighting alone.
    mgr.store_lighting("028d", blue()).unwrap();
    mgr.library_activate("028d", &b).unwrap();
    assert_eq!(saved_lighting(&mgr, "028d"), Some(blue()));
    // Switching to A brings its lighting back (the simulated device answers lighting commands).
    let act = mgr.library_activate("028d", &a).unwrap();
    assert_eq!(saved_lighting(&mgr, "028d"), Some(red()), "the profile's lighting became the device's lighting");
    assert!(!act.warnings.iter().any(|w| w.starts_with("lighting:")), "{:?}", act.warnings);

    // The user changes the lighting while A is active: A keeps the change; B (no lighting) doesn't grow one.
    mgr.save_lighting("028d", blue()).unwrap();
    assert_eq!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&a).unwrap().lighting, Some(blue()));
    mgr.library_activate("028d", &b).unwrap();
    mgr.save_lighting("028d", red()).unwrap();
    let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
    assert!(l.get(&b).unwrap().lighting.is_none());
    assert_eq!(l.get(&a).unwrap().lighting, Some(blue()), "A isn't touched while B is active");
    // Taking the lighting off a profile.
    mgr.library_set_lighting("028d", &a, "off").unwrap();
    assert!(DeviceLibrary::load(&lib::root(), "028d").unwrap().get(&a).unwrap().lighting.is_none());
    assert!(mgr.library_set_lighting("028d", "00000000000000000000000000000000", "off").is_err());
}

#[test]
fn a_factory_profile_resets_performance_and_the_dial() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, sim) = rig("028d", 1);
    profile_ids(&mgr, "028d");
    // Someone moved the dial modes around.
    {
        let mut s = sim.lock().unwrap();
        s.knob.swap(0, 5);
        s.knob[1].enabled = false;
        s.knob[1].rgb = [1, 2, 3];
    }
    let id = mgr.library_create_factory("028d", "Stock").unwrap();
    assert_eq!(mgr.library_create_factory("028d", "Stock").map(|_| ()).is_ok(), true);
    let l = DeviceLibrary::load(&lib::root(), "028d").unwrap();
    assert_eq!(l.profiles.iter().filter(|p| p.name.starts_with("Stock")).count(), 2, "names are made unique");
    assert!(l.get(&id).unwrap().dial.is_some());
    mgr.library_activate("028d", &id).unwrap();
    let s = sim.lock().unwrap();
    let functions: Vec<u8> = s.knob.iter().map(|k| k.function.into()).collect();
    assert_eq!(functions, (0..8).collect::<Vec<u8>>(), "slot order is the stock order");
    assert_eq!(s.knob.iter().filter(|k| k.enabled).count(), 4);
    assert_eq!(s.knob[1].rgb, [0x44, 0xd6, 0x2c], "the stock colours aren't known: the profile uses one colour for every mode");
}

#[test]
fn profiles_export_to_a_folder_without_overwriting() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("00aa", 2);
    let ids = profile_ids(&mgr, "00aa");
    mgr.library_rename("00aa", &ids[0], "Fast/Slow: mix?").unwrap();
    let dir = std::env::temp_dir().join(format!("tsuba-export-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_str().unwrap();
    let paths = mgr.library_export_many("00aa", &ids, dir_s).unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths[0].ends_with("Fast_Slow_ mix_.tsuba-profile.json"), "{paths:?}");
    let again = mgr.library_export_many("00aa", &ids[..1], dir_s).unwrap();
    assert!(again[0].ends_with("Fast_Slow_ mix_ 2.tsuba-profile.json"), "never overwrites: {again:?}");
    // What was written imports back.
    let id = mgr.library_import("00aa", &paths[0]).unwrap();
    assert!(DeviceLibrary::load(&lib::root(), "00aa").unwrap().get(&id).is_some());
    assert!(mgr.library_export_many("00aa", &ids, "/definitely/not/a/folder").is_err());
    assert!(mgr.library_export_many("00aa", &["0".repeat(32)], dir_s).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

fn macro_with(name: &str, events: Vec<MacroEvent>) -> Macro {
    Macro { name: name.into(), events, ..Default::default() }
}

fn clear_macro_library() {
    let _ = std::fs::remove_file(macros::library_path());
}

#[test]
fn macro_usage_names_profiles_and_flags_missing_macros() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("00aa", 1);
    clear_macro_library();
    let ids = profile_ids(&mgr, "00aa");
    let used = mgr.save_macro(macro_with("Used", vec![MacroEvent::Key { hid: 4, down: true }, MacroEvent::Key { hid: 4, down: false }])).unwrap();
    let spare = mgr.save_macro(macro_with("Spare", vec![MacroEvent::Delay { ms: 5 }])).unwrap();
    let gone = mgr.save_macro(macro_with("Gone", vec![MacroEvent::Scroll { up: true }])).unwrap();
    let mut l = DeviceLibrary::load(&lib::root(), "00aa").unwrap();
    let mut p = l.get(&ids[0]).unwrap().clone();
    p.set_macro(MacroBinding { layer: 0, key: 4, guid: used.clone(), playback: Playback::Once, count: 1 });
    p.set_macro(MacroBinding { layer: 1, key: 5, guid: gone.clone(), playback: Playback::Sequence, count: 1 });
    l.put(p).unwrap();
    mgr.delete_macro(&gone).unwrap();
    let u = mgr.macro_usage_json().unwrap();
    assert_eq!(u["used"][&used][0]["key"], 4);
    assert!(u["used"][&spare].is_null(), "an unbound macro is simply absent from the used list");
    assert_eq!(u["missing"].as_array().unwrap().len(), 1);
    assert_eq!((u["missing"][0]["guid"].as_str(), u["missing"][0]["layer"].as_u64()), (Some(gone.as_str()), Some(1)));
    clear_macro_library();
}

#[test]
fn macros_are_renamed_duplicated_and_filed() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("00aa", 1);
    clear_macro_library();
    let ev = vec![MacroEvent::Key { hid: 4, down: true }, MacroEvent::Key { hid: 4, down: false }];
    let guid = mgr.save_macro(macro_with("First", ev.clone())).unwrap();
    // Put a copy on the device (what binding it to a key does).
    let mac = mgr.library().unwrap().get(&guid).cloned().unwrap();
    mgr.with_device("00aa", |dev, _| macros::ensure_on_device(dev, &mac)).unwrap();
    mgr.rename_macro(&guid, "Second").unwrap();
    assert_eq!(mgr.library().unwrap().get(&guid).unwrap().name, "Second");
    let on_device = mgr.with_device("00aa", |dev, _| macros::read_macro(dev, macros::ids(dev)?[0])).unwrap();
    assert_eq!(on_device.name, "Second", "the device shows the new name");
    assert_eq!(on_device.events, ev, "the events are untouched");
    assert!(mgr.rename_macro(&guid, "  ").is_err());
    assert!(mgr.rename_macro("nope", "x").is_err());
    // Duplicate, with a unique name, then file under a folder.
    let copy = mgr.duplicate_macro(&guid, "Second").unwrap();
    assert_ne!(copy, guid);
    let lib = mgr.library().unwrap();
    assert_eq!(lib.get(&copy).unwrap().name, "Second 2");
    assert_eq!(lib.get(&copy).unwrap().events, ev);
    mgr.set_macro_group(&copy, "  Work  ").unwrap();
    assert_eq!(mgr.library().unwrap().get(&copy).unwrap().group, "Work");
    mgr.set_macro_group(&copy, "").unwrap();
    assert_eq!(mgr.library().unwrap().get(&copy).unwrap().group, "");
    // Saving the macro again without naming a folder keeps its folder.
    mgr.set_macro_group(&guid, "Keep").unwrap();
    mgr.save_macro(Macro { guid: guid.clone(), name: "Second".into(), events: ev.clone(), ..Default::default() }).unwrap();
    assert_eq!(mgr.library().unwrap().get(&guid).unwrap().group, "Keep");
    // A re-import from the device keeps the folder.
    mgr.set_macro_group(&guid, "Keep").unwrap();
    let id = mgr.with_device("00aa", |dev, _| macros::ids(dev)).unwrap()[0];
    mgr.import_macro("00aa", id).unwrap();
    assert_eq!(mgr.library().unwrap().get(&guid).unwrap().group, "Keep");
    clear_macro_library();
}

#[test]
fn run_command_events_get_shared_shortcut_slots_and_play_them() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, sim) = rig("028d", 1);
    clear_macro_library();
    profile_ids(&mgr, "028d");
    let run = |c: &str| MacroEvent::Run { command: c.into(), slot: None };
    let a = mgr.save_macro(macro_with("Terminal", vec![run("konsole"), MacroEvent::Delay { ms: 10 }])).unwrap();
    let b = mgr.save_macro(macro_with("Terminal again", vec![run("konsole")])).unwrap();
    let c = mgr.save_macro(macro_with("Editor", vec![run("kate")])).unwrap();
    let slot = |g: &str| match &mgr.library().unwrap().get(g).unwrap().events[0] {
        MacroEvent::Run { slot, .. } => slot.unwrap(),
        e => panic!("{e:?}"),
    };
    assert_eq!((slot(&a), slot(&b), slot(&c)), (0, 0, 1), "the same command shares a slot");
    // A key bound to a software action never takes a macro's slot.
    mgr.assign_action("028d", 1, 0, 160, Action::Launch { command: "x".into() }).unwrap();
    let code = mgr.config.lock().unwrap().device("028d").unwrap().actions[0].code;
    assert_eq!(code, 2, "slots 0 and 1 belong to macros");
    let entries = mgr.slot_entries();
    assert_eq!(entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["028d-2", "macro-0", "macro-1"]);
    assert!(entries.iter().any(|e| e.label == "Macro command: kate"));
    // Uploading plays the slot's key chord: F14 for slot 0.
    let mac = mgr.library().unwrap().get(&a).cloned().unwrap();
    mgr.with_device("028d", |dev, _| macros::ensure_on_device(dev, &mac)).unwrap();
    let data = sim.lock().unwrap().macros.values().next().unwrap().data.clone();
    assert_eq!(&data[..6], &[0x11, 0x14, 0x01, 0x69, 0x11, 0x14], "delay, F14 down, delay… (modifier-free slot)");
    // A copy gets its own slot; deleting a macro frees the slot for the next one.
    let d = mgr.duplicate_macro(&c, "Editor copy").unwrap();
    assert_eq!(slot(&d), 1, "the copy runs the same command, so it shares the slot too");
    mgr.delete_macro(&c).unwrap();
    mgr.delete_macro(&d).unwrap();
    let e = mgr.save_macro(macro_with("Browser", vec![run("firefox")])).unwrap();
    assert_eq!(slot(&e), 1);
    // The run command is found again from the slot.
    assert!(mgr.library().unwrap().run_slots().contains(&1));
    clear_macro_library();
}

#[test]
fn global_actions_need_no_connected_target_device() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("028d", 1);
    clear_macro_library();
    let act = Action::Device { target: tsuba_core::config::ALL_DEVICES.into(), op: DeviceOp::BrightnessAllUp };
    assert_eq!(mgr.assign_action("028d", 1, 0, 160, act).unwrap(), "All devices: brightness up");
    let studio = Action::Device { target: tsuba_core::config::ALL_DEVICES.into(), op: DeviceOp::StudioNext };
    assert_eq!(mgr.assign_action("028d", 1, 0, 161, studio).unwrap(), "All devices: next lighting profile");
    // A named device that isn't there is still refused.
    assert!(mgr.assign_action("028d", 1, 0, 162, Action::Device { target: "00aa".into(), op: DeviceOp::BrightnessUp }).is_err());
    // The text action and the new audio ops have readable labels.
    assert_eq!(Action::Type { text: "hi 😀".into() }.label(), "Type: hi 😀");
    let a = Action::Audio { op: tsuba_core::config::AudioOp::SetDefaultOutput { name: "sink".into(), label: "Headphones".into() } };
    assert_eq!(a.label(), "Output: Headphones");
    assert_eq!(serde_json::to_string(&a).unwrap(), r#"{"type":"audio","op":{"set_default_output":{"name":"sink","label":"Headphones"}}}"#);
    assert_eq!(serde_json::from_str::<Action>(r#"{"type":"audio","op":"mic_volume_up"}"#).unwrap().label(), "Microphone volume up");
}

#[test]
fn audio_devices_are_listed_without_monitors() {
    let json = r#"[{"name": "alsa_output.a", "description": "Speakers"}, {"name": "alsa_output.a.monitor", "description": "Monitor of Speakers"}, {"name": "alsa_output.b", "description": "Headphones"}]"#;
    let d = crate::actions::parse_audio_devices(json, "alsa_output.b");
    assert_eq!(d.iter().map(|x| (x.name.as_str(), x.default)).collect::<Vec<_>>(), [("alsa_output.a", false), ("alsa_output.b", true)]);
    assert!(crate::actions::parse_audio_devices("not json", "").is_empty());
}

/// This test binary stands in for the "game" (see `matches`).
fn game_link(polling: &[(&str, u32)], focused: bool) -> AppLink {
    let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().into_owned();
    AppLink { name: "Me".into(), matches: me, enabled: true, polling_hz: polling.iter().map(|(p, h)| (p.to_string(), *h)).collect(), only_when_focused: focused, ..Default::default() }
}

#[test]
fn a_linked_game_sets_the_polling_rate_and_restores_it() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, sim) = rig("00aa", 1);
    profile_ids(&mgr, "00aa");
    let before = sim.lock().unwrap().polling_period;
    mgr.set_app_links(AppLinks { links: vec![game_link(&[("00aa", 500)], false)] }).unwrap();
    mgr.link_tick();
    let during = sim.lock().unwrap().polling_period;
    assert_ne!(before, during, "the game's rate is in force");
    assert_eq!(tsuba_proto::decode::polling_hz(&[during]), Some(500));
    assert_eq!(mgr.link_active().as_deref(), Some("Me"));
    mgr.set_app_links(AppLinks::default()).unwrap();
    mgr.link_tick();
    assert_eq!(sim.lock().unwrap().polling_period, before, "back to what it was");
    // A rate the device already has changes nothing and records nothing to restore.
    let writes = sim.lock().unwrap().writes.len();
    mgr.set_app_links(AppLinks { links: vec![game_link(&[("00aa", 1000)], false)] }).unwrap();
    mgr.link_tick();
    assert_eq!(sim.lock().unwrap().writes.len(), writes);
    mgr.set_app_links(AppLinks::default()).unwrap();
    mgr.link_tick();
}

#[test]
fn a_focus_only_link_follows_the_window_focus() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, sim) = rig("00aa", 1);
    profile_ids(&mgr, "00aa");
    let before = sim.lock().unwrap().polling_period;
    mgr.set_app_links(AppLinks { links: vec![game_link(&[("00aa", 500)], true)] }).unwrap();
    // No window information yet: applies while running.
    mgr.link_tick();
    assert_eq!(mgr.link_active().as_deref(), Some("Me"));
    // Focus moves to another program: the link lets go; back on the game: it applies again.
    *mgr.actions.focus.lock().unwrap() = Some(FocusInfo { pid: 1, class: "firefox".into(), ..Default::default() });
    mgr.link_tick();
    assert_eq!(mgr.link_active(), None);
    assert_eq!(sim.lock().unwrap().polling_period, before);
    *mgr.actions.focus.lock().unwrap() = Some(FocusInfo { pid: std::process::id(), ..Default::default() });
    mgr.link_tick();
    assert_eq!(mgr.link_active().as_deref(), Some("Me"));
    assert_ne!(sim.lock().unwrap().polling_period, before);
    mgr.set_app_links(AppLinks::default()).unwrap();
    mgr.link_tick();
    *mgr.actions.focus.lock().unwrap() = None;
}

#[test]
fn a_synapse_reimport_keeps_the_profiles_lighting() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mgr, _sim) = rig("00aa", 1);
    let ids = profile_ids(&mgr, "00aa");
    let mut l = DeviceLibrary::load(&lib::root(), "00aa").unwrap();
    let mut p = l.get(&ids[0]).unwrap().clone();
    p.synapse_guid = Some("syn-guid".into());
    p.lighting = Some(red());
    l.put(p.clone()).unwrap();
    // What the importer sends: same Synapse guid, no lighting.
    let mut incoming = p.clone();
    incoming.lighting = None;
    incoming.name = "Renamed in Synapse".into();
    mgr.library_save("00aa", &serde_json::to_string(&incoming).unwrap()).unwrap();
    let l = DeviceLibrary::load(&lib::root(), "00aa").unwrap();
    assert_eq!(l.profiles.len(), 1);
    assert_eq!(l.profiles[0].name, "Renamed in Synapse");
    assert_eq!(l.profiles[0].lighting, Some(red()));
}
