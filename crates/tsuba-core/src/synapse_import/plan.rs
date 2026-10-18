//! Turn what Synapse stored into Tsuba settings: one plan entry per device profile, macro and
//! Chroma Studio profile. Anything that has no Tsuba equivalent becomes a readable warning on
//! its entry, so nothing is dropped silently.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsuba_hid::{db, Kind};

use super::keys;
use super::SynapseData;
use crate::chroma_profiles::ChromaProfile;
use crate::config::{Action, DeviceOp, DialSlotConfig, EffectConfig};
use crate::macros::{Macro, MacroEvent, Playback};
use crate::performance::Performance;
use crate::studio::{Direction, Duration, Layer, Studio, StudioEffect};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub store: String,
    pub devices: Vec<DevicePlan>,
    pub macros: Vec<MacroPlan>,
    pub lighting: Vec<LightingPlan>,
    /// Records of the store that couldn't be read (path: reason).
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevicePlan {
    /// Synapse's product id (not the USB pid).
    pub product_id: u32,
    /// Name as Synapse shows it.
    pub name: String,
    /// Tsuba device key ("028d") when the device is in Tsuba's database.
    pub pid: Option<String>,
    pub kind: Option<String>,
    /// Key/button positions of this model are known, so assignments can be imported.
    pub verified: bool,
    /// Profiles can be imported (the device is known to Tsuba).
    pub importable: bool,
    pub note: String,
    pub profiles: Vec<ProfilePlan>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilePlan {
    /// Synapse profile GUID.
    pub guid: String,
    pub name: String,
    /// The profile Synapse last had active on this device.
    pub active: bool,
    /// What will be written, one line each.
    pub summary: Vec<String>,
    pub warnings: Vec<String>,
    /// Per-profile settings (DPI stages, scroll wheel); `profile` is filled in on import.
    pub performance: Option<Performance>,
    pub assignments: Vec<AssignmentPlan>,
    /// Library macros (Tsuba GUIDs) the assignments use.
    pub macros: Vec<String>,
    /// Settings Tsuba keeps once per device rather than per profile.
    pub device_wide: DeviceWide,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DeviceWide {
    pub polling_hz: Option<u32>,
    pub effect: Option<EffectConfig>,
    /// Brightness percent.
    pub brightness: Option<u8>,
    pub dial: Option<Vec<DialSlotConfig>>,
    pub summary: Vec<String>,
}

impl DeviceWide {
    pub fn is_empty(&self) -> bool {
        self.polling_hz.is_none() && self.effect.is_none() && self.brightness.is_none() && self.dial.is_none()
    }
}

/// One key/button binding, in the form the matching D-Bus call takes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum AssignmentPlan {
    /// SetAssignment: a firmware function.
    Function { key: u8, layer: u8, function: u8, data: Vec<u8>, label: String },
    /// AssignMacro: a library macro, uploaded to the device.
    Macro { key: u8, layer: u8, guid: String, playback: Playback, count: u8, label: String },
    /// AssignAction: a software action run by tsubad.
    Action { key: u8, layer: u8, action: Action, label: String },
}

impl AssignmentPlan {
    pub fn label(&self) -> &str {
        match self {
            Self::Function { label, .. } | Self::Macro { label, .. } | Self::Action { label, .. } => label,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroPlan {
    /// Tsuba library GUID (Synapse's GUID without dashes, so re-importing replaces it).
    pub guid: String,
    pub synapse_guid: String,
    pub name: String,
    pub events: usize,
    pub warnings: Vec<String>,
    #[serde(rename = "macro")]
    pub mac: Macro,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LightingPlan {
    pub id: String,
    pub name: String,
    /// Device names with layers in the imported profile.
    pub devices: Vec<String>,
    pub layers: usize,
    pub warnings: Vec<String>,
    pub importable: bool,
    pub profile: ChromaProfile,
}

// ---------------------------------------------------------------------------------------
// Devices

/// Synapse product ids verified against real hardware with Tsuba.
const KNOWN: &[(u32, u16)] = &[(653, 0x028d), (170, 0x00aa)];

fn norm_name(s: &str) -> String {
    let s = s.to_ascii_lowercase();
    let s = s.split('(').next().unwrap_or("");
    let s = s.trim().strip_prefix("razer").unwrap_or(s.trim());
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// Tsuba database entry for a Synapse device: by product id for verified devices, else by name.
pub fn match_device(product_id: u32, name: &str) -> Option<&'static db::Spec> {
    if let Some((_, pid)) = KNOWN.iter().find(|(p, _)| *p == product_id) {
        return db::get(*pid);
    }
    let want = norm_name(name);
    if want.is_empty() {
        return None;
    }
    let mut hits: Vec<&db::Spec> = db::all().iter().filter(|s| norm_name(&s.name) == want).collect();
    // Prefer the wired entry of a wired/wireless pair.
    hits.sort_by_key(|s| match s.connection.as_deref() {
        Some("wired") | None => 0,
        _ => 1,
    });
    hits.first().copied()
}

/// BlackWidow V4 Pro key positions by the HID usage (or modifier bit) each key sends by default —
/// read from the keyboard's factory-default profile.
const BW_V4_PRO_BY_HID: &[(u8, u8)] = &[(0x04, 31), (0x05, 50), (0x06, 48), (0x07, 33), (0x08, 19), (0x09, 34), (0x0a, 35), (0x0b, 36), (0x0c, 24), (0x0d, 37), (0x0e, 38), (0x0f, 39), (0x10, 52), (0x11, 51), (0x12, 25), (0x13, 26), (0x14, 17), (0x15, 20), (0x16, 32), (0x17, 21), (0x18, 23), (0x19, 49), (0x1a, 18), (0x1b, 47), (0x1c, 22), (0x1d, 46), (0x1e, 2), (0x1f, 3), (0x20, 4), (0x21, 5), (0x22, 6), (0x23, 7), (0x24, 8), (0x25, 9), (0x26, 10), (0x27, 11), (0x28, 43), (0x29, 110), (0x2a, 15), (0x2b, 16), (0x2c, 61), (0x2d, 12), (0x2e, 13), (0x2f, 27), (0x30, 28), (0x31, 29), (0x32, 42), (0x33, 40), (0x34, 41), (0x35, 1), (0x36, 53), (0x37, 54), (0x38, 55), (0x39, 30), (0x3a, 112), (0x3b, 113), (0x3c, 114), (0x3d, 115), (0x3e, 116), (0x3f, 117), (0x40, 118), (0x41, 119), (0x42, 120), (0x43, 121), (0x44, 122), (0x45, 123), (0x46, 124), (0x47, 125), (0x48, 126), (0x49, 75), (0x4a, 80), (0x4b, 85), (0x4c, 76), (0x4d, 81), (0x4e, 86), (0x4f, 89), (0x50, 79), (0x51, 84), (0x52, 83), (0x53, 90), (0x54, 95), (0x55, 100), (0x56, 105), (0x57, 106), (0x58, 108), (0x59, 93), (0x5a, 98), (0x5b, 103), (0x5c, 92), (0x5d, 97), (0x5e, 102), (0x5f, 91), (0x60, 96), (0x61, 101), (0x62, 99), (0x63, 104), (0x64, 45), (0x65, 129), (0x85, 107), (0x87, 56), (0x88, 151), (0x89, 14), (0x8a, 154), (0x8b, 150), (0x90, 153), (0x91, 152)];
const BW_V4_PRO_BY_MODIFIER: &[(u8, u8)] = &[(0x01, 58), (0x02, 44), (0x04, 60), (0x08, 127), (0x10, 64), (0x20, 57), (0x40, 62)];

/// Tsuba key/button position for a Synapse input id on a verified model.
fn input_position(pid: u16, input: &str) -> Option<u8> {
    match pid {
        0x00aa | 0x00ab => Some(match input {
            "Button4" => 4,
            "Button5" => 5,
            "RepeatScrollLeft" => 52,
            "RepeatScrollRight" => 53,
            "ScrollButton" => 3,
            // The sniper/clutch button under the thumb (DPI clutch by default).
            "DKM_SB_02" => 15,
            _ => return None,
        }),
        0x028d => {
            if let Some(n) = input.strip_prefix("DKM_M_0") {
                let n = n.parse::<u8>().ok()?;
                return (1..=5).contains(&n).then_some(160 + n - 1);
            }
            // Ordinary keys (inputType KeyInput, inputID KEY_*): the key that sends this usage.
            let hid = keys::hid_for_name(input.strip_prefix("KEY_")?).or_else(|| keys::hid_for_name(input))?;
            match keys::modifier_bit(hid) {
                Some(bit) => BW_V4_PRO_BY_MODIFIER.iter().find(|(m, _)| *m == bit).map(|(_, p)| *p),
                None => BW_V4_PRO_BY_HID.iter().find(|(h, _)| *h == hid).map(|(_, p)| *p),
            }
        }
        _ => None,
    }
}

fn input_label(input: &str) -> String {
    match input {
        "Button4" => "Mouse button 4".into(),
        "Button5" => "Mouse button 5".into(),
        "RepeatScrollLeft" => "Wheel tilt left".into(),
        "RepeatScrollRight" => "Wheel tilt right".into(),
        "ScrollButton" => "Wheel click".into(),
        "DKM_SB_02" => "Sniper button".into(),
        "ScrollLeft" => "Dial left".into(),
        "ScrollRight" => "Dial right".into(),
        s => match s.strip_prefix("DKM_M_0") {
            Some(n) => format!("M{n}"),
            None => s.to_string(),
        },
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn n(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

/// Synapse GUID → Tsuba macro GUID (32 hex digits).
pub fn tsuba_guid(g: &str) -> String {
    g.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_ascii_lowercase()
}

fn playback(opt: &str, count: u8) -> Option<(Playback, u8)> {
    Some(match opt {
        "Once" | "" => (Playback::Once, 1),
        "Repeat" | "RepeatCount" | "Times" | "MultipleTimes" => (Playback::Times, count.max(2)),
        "ContinuousHeld" | "WhileHeld" | "RepeatWhileHeld" => (Playback::WhileHeld, 1),
        "ContinuousToggle" | "Toggle" | "ToggleContinuous" => (Playback::Toggle, 1),
        _ => return None,
    })
}

fn playback_label(p: Playback, count: u8) -> String {
    match p {
        Playback::Once => "play once".into(),
        Playback::Times => format!("play {count} times"),
        Playback::WhileHeld => "repeat while held".into(),
        Playback::Toggle => "toggle on/off".into(),
        Playback::Sequence => "play as a sequence".into(),
    }
}

fn mouse_button(name: &str) -> Option<(u8, &'static str)> {
    Some(match name {
        "Click" | "LeftClick" => (1, "left click"),
        "RightClick" => (2, "right click"),
        "ScrollClick" | "MiddleClick" | "WheelClick" => (3, "wheel click"),
        "Back" | "Previous" | "Button4" | "MouseButton4" => (4, "mouse button 4"),
        "Next" | "Forward" | "Button5" | "MouseButton5" => (5, "mouse button 5"),
        "ScrollUp" | "WheelUp" => (9, "scroll up"),
        "ScrollDown" | "WheelDown" => (10, "scroll down"),
        "ScrollLeft" | "TiltLeft" => (104, "scroll left"),
        "ScrollRight" | "TiltRight" => (105, "scroll right"),
        _ => return None,
    })
}

fn media_usage(name: &str) -> Option<(u8, &'static str)> {
    Some(match name {
        "PlayPause" | "Play" => (0xcd, "play / pause"),
        "NextTrack" => (0xb5, "next track"),
        "PrevTrack" | "PreviousTrack" => (0xb6, "previous track"),
        "Stop" => (0xb7, "stop"),
        "Mute" | "MuteVolume" => (0xe2, "mute"),
        "VolumeUp" => (0xe9, "volume up"),
        "VolumeDown" => (0xea, "volume down"),
        _ => return None,
    })
}

fn turbo_ms(t: Option<&Value>) -> Option<u16> {
    let t = t?;
    if !b(t, "isTurbo").unwrap_or(false) {
        return None;
    }
    let rate = n(t, "keysPerSecond").unwrap_or(7.0).clamp(1.0, 1000.0);
    Some((1000.0 / rate).round() as u16)
}

/// One Synapse mapping → an assignment, or a warning.
fn map_assignment(pid: u16, m: &Value, macros: &BTreeMap<String, String>, out: &mut ProfilePlan) {
    let input = s(m, "inputID");
    let shift = b(m, "isHyperShift").unwrap_or(false);
    let layer = shift as u8;
    let what = format!("{}{}", input_label(input), if shift { " (Hypershift)" } else { "" });
    let out_type = s(m, "outputType");
    let g = m.get(out_type).cloned().unwrap_or(Value::Null);
    // The dial's rotation follows the command dial mode (imported with the dial).
    if pid == 0x028d && matches!(input, "ScrollLeft" | "ScrollRight") {
        return;
    }
    let Some(key) = input_position(pid, input) else {
        out.warnings.push(format!("{what}: this input isn't mapped to a Tsuba key position yet; its assignment ({out_type}) was skipped"));
        return;
    };
    let fail = |out: &mut ProfilePlan, why: String| out.warnings.push(format!("{what}: {why}"));
    let plan = match out_type {
        "keyboardGroup" => {
            let mods: Vec<String> = g.get("modifiers").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let k = s(&g, "key");
            let data = match keys::key_data(k, &mods) {
                Ok(d) => d,
                Err(e) => return fail(out, e),
            };
            let name = mods.iter().map(|m| m.trim_start_matches("KEY_").replace('_', " ")).chain([k.trim_start_matches("KEY_").replace('_', " ")]).collect::<Vec<_>>().join(" + ");
            match turbo_ms(g.get("turboMode")) {
                Some(ms) => AssignmentPlan::Function {
                    key,
                    layer,
                    function: 13,
                    data: vec![data[0], data[1], (ms >> 8) as u8, ms as u8],
                    label: format!("{what} → {name} (turbo)"),
                },
                None => AssignmentPlan::Function { key, layer, function: 2, data: data.to_vec(), label: format!("{what} → {name}") },
            }
        }
        "mouseGroup" => {
            let a = s(&g, "mouseAssignment");
            if a == "DoubleClick" {
                AssignmentPlan::Function { key, layer, function: 11, data: vec![1], label: format!("{what} → double click") }
            } else {
                let Some((btn, name)) = mouse_button(a) else { return fail(out, format!("mouse function {a:?} isn't supported")) };
                match turbo_ms(g.get("turboMode")) {
                    Some(ms) => AssignmentPlan::Function { key, layer, function: 14, data: vec![btn, (ms >> 8) as u8, ms as u8], label: format!("{what} → {name} (turbo)") },
                    None => AssignmentPlan::Function { key, layer, function: 1, data: vec![btn], label: format!("{what} → {name}") },
                }
            }
        }
        "macroGroup" => {
            let guid = tsuba_guid(s(&g, "guid"));
            let Some(mname) = macros.get(&guid) else { return fail(out, format!("macro {:?} isn't in the Synapse data", s(&g, "name"))) };
            let count = n(&g, "repeatCount").unwrap_or(1.0).clamp(1.0, 255.0) as u8;
            let opt = s(&g, "macroPlaybackOption");
            let Some((pb, count)) = playback(opt, count) else { return fail(out, format!("macro playback option {opt:?} isn't supported")) };
            if !out.macros.contains(&guid) {
                out.macros.push(guid.clone());
            }
            AssignmentPlan::Macro { key, layer, guid, playback: pb, count, label: format!("{what} → macro “{mname}” ({})", playback_label(pb, count)) }
        }
        "sensitivityGroup" => {
            let a = s(&g, "sensitivityAssignment");
            match a {
                "DPI_Clutch" | "SensitivityClutch" | "Clutch" => {
                    let x = n(&g, "x").unwrap_or(400.0).clamp(100.0, 65535.0) as u16;
                    let y = if b(&g, "independent").unwrap_or(false) { n(&g, "y").unwrap_or(x as f64).clamp(100.0, 65535.0) as u16 } else { x };
                    let label = if x == y { format!("{what} → sensitivity clutch {x} DPI") } else { format!("{what} → sensitivity clutch {x}×{y} DPI") };
                    AssignmentPlan::Function { key, layer, function: 6, data: vec![5, (x >> 8) as u8, x as u8, (y >> 8) as u8, y as u8], label }
                }
                "DPI_StageUp" | "StageUp" | "SensitivityStageUp" => AssignmentPlan::Function { key, layer, function: 6, data: vec![1], label: format!("{what} → sensitivity stage up") },
                "DPI_StageDown" | "StageDown" | "SensitivityStageDown" => AssignmentPlan::Function { key, layer, function: 6, data: vec![2], label: format!("{what} → sensitivity stage down") },
                "DPI_CycleUp" | "CycleUp" => AssignmentPlan::Function { key, layer, function: 6, data: vec![6], label: format!("{what} → cycle sensitivity stages up") },
                "DPI_CycleDown" | "CycleDown" => AssignmentPlan::Function { key, layer, function: 6, data: vec![7], label: format!("{what} → cycle sensitivity stages down") },
                _ => return fail(out, format!("sensitivity function {a:?} isn't supported")),
            }
        }
        "backlightGroup" => {
            let op = match s(&g, "backlightAssignment") {
                "BrightnessUp" => DeviceOp::BrightnessUp,
                "BrightnessDown" => DeviceOp::BrightnessDown,
                a => return fail(out, format!("lighting function {a:?} isn't supported")),
            };
            let up = op == DeviceOp::BrightnessUp;
            AssignmentPlan::Action {
                key,
                layer,
                action: Action::Device { target: crate::pid_key(pid), op },
                label: format!("{what} → brightness {}", if up { "up" } else { "down" }),
            }
        }
        "multimediaGroup" => {
            let a = s(&g, "multimediaAssignment");
            let Some((u, name)) = media_usage(a) else { return fail(out, format!("multimedia function {a:?} isn't supported")) };
            AssignmentPlan::Function { key, layer, function: 10, data: vec![0, u], label: format!("{what} → {name}") }
        }
        "disabledGroup" | "disabled" => AssignmentPlan::Function { key, layer, function: 0, data: vec![], label: format!("{what} → disabled") },
        other => return fail(out, format!("{} assignments aren't supported", other.trim_end_matches("Group"))),
    };
    out.summary.push(plan.label().to_string());
    out.assignments.push(plan);
}

/// Synapse quick-effect id → Tsuba effect. Ids are Synapse's lighting-effect enum (1 static,
/// 2 breathing, 3 spectrum, 4 wave, 5 reactive, 6 ripple, 7 starlight, 8 fire, 11 ambient,
/// 12 audio meter, 13 wheel, 14 battery level). Id 19 isn't in Synapse's local enum; its settings
/// (two colours + direction) make Wave the closest Tsuba effect.
fn quick_effect(q: &Value) -> Result<Option<EffectConfig>, String> {
    let id = n(q, "selectedEffectId").map(|x| x as i64);
    let set = q.get("selectedEffectSetting").cloned().unwrap_or(Value::Null);
    let colour = |k: &str| {
        let c = s(&set, k).trim_start_matches('#').to_ascii_lowercase();
        crate::parse_rgb(&c).is_ok().then_some(c)
    };
    let random = b(&set, "isRandom").unwrap_or(false);
    let colours = || if random { vec![] } else { ["color1", "color2", "color"].iter().filter_map(|k| colour(k)).collect::<Vec<_>>() };
    // Synapse durations/speeds are 1 (short/fast) … 3 (long/slow); Tsuba uses the same 1–3 scale.
    let speed = || (n(&set, "duration").or_else(|| n(&set, "speed")).unwrap_or(2.0) as u8).clamp(1, 3);
    let duration = || ["short", "medium", "long"][speed() as usize - 1].to_string();
    let studio = |effect: Value| -> Result<Option<EffectConfig>, String> {
        let layer = serde_json::json!({ "name": "Imported", "enabled": true, "leds": [], "effect": effect });
        serde_json::from_value::<crate::studio::Studio>(serde_json::json!({ "layers": [layer] }))
            .map(|studio| Some(EffectConfig::Studio { studio }))
            .map_err(|e| format!("couldn't build the lighting effect: {e}"))
    };
    let wave = |set: &Value| {
        let dir = n(set, "direction").unwrap_or(1.0) as u8;
        EffectConfig::Wave { direction: if dir == 2 { 2 } else { 1 }, speed: 0x28 }
    };
    match id {
        None => Ok(None),
        Some(1) => Ok(colour("color1").or_else(|| colour("color")).map(|c| EffectConfig::Static { colour: c })),
        Some(2) => Ok(Some(EffectConfig::Breathing { colours: colours() })),
        Some(3) => Ok(Some(EffectConfig::Spectrum)),
        Some(4) | Some(19) => Ok(Some(wave(&set))),
        Some(5) => Ok(Some(EffectConfig::Reactive { speed: speed(), colour: colour("color1").or_else(|| colour("color")).unwrap_or_else(|| "44d62c".into()) })),
        Some(6) => Ok(Some(EffectConfig::Ripple { speed: speed(), colours: colours() })),
        Some(7) => Ok(Some(EffectConfig::Starlight { speed: speed(), colours: colours() })),
        Some(8) => studio(serde_json::json!({ "type": "fire", "duration": duration() })),
        Some(11) => studio(serde_json::json!({ "type": "ambient" })),
        Some(12) => studio(serde_json::json!({ "type": "audio_meter" })),
        Some(13) => studio(serde_json::json!({ "type": "wheel", "direction": if n(&set, "direction") == Some(2.0) { "counter_clockwise" } else { "clockwise" }, "duration": duration() })),
        Some(14) => studio(serde_json::json!({ "type": "battery" })),
        Some(x) => {
            let keys: Vec<&str> = set.as_object().map(|o| o.keys().map(String::as_str).filter(|k| *k != "effectId").collect()).unwrap_or_default();
            Err(format!("quick effect #{x} isn't identified yet (settings: {}); set the lighting in Tsuba's LIGHTING tab", if keys.is_empty() { "none".into() } else { keys.join(", ") }))
        }
    }
}

fn dial(cd: &Value, warnings: &mut Vec<String>) -> Option<Vec<DialSlotConfig>> {
    let modes = cd.get("modes")?.as_array()?;
    let mut slots: Vec<Option<DialSlotConfig>> = vec![None; 8];
    // Synapse's built-in modes in the order of their ids (profiles migrated from Synapse 3
    // carry only the name).
    const MODES: [&str; 8] = [
        "KEYBOARD_BRIGHTNESS",
        "WINDOWS_ZOOM",
        "SWITCH_APPLICATIONS",
        "TRACK_JOGGING",
        "TRACK_SELECTOR",
        "VERTICAL_SCROLLING",
        "HORIZONTAL_SCROLLING",
        "SWITCH_BROWSER_TABS",
    ];
    for m in modes {
        let name = s(m, "name");
        let id = match (n(m, "modeId"), MODES.iter().position(|x| *x == name)) {
            (Some(id), _) => id as i64,
            (None, Some(i)) => i as i64,
            (None, None) => -1,
        };
        if b(m, "isCustom").unwrap_or(false) || !(0..8).contains(&id) {
            warnings.push(format!("Command dial: custom mode {name:?} isn't supported"));
            continue;
        }
        let colour = s(m, "color").trim_start_matches('#').to_ascii_lowercase();
        if crate::parse_rgb(&colour).is_err() {
            warnings.push(format!("Command dial: mode {name:?} has no valid colour"));
            continue;
        }
        // The dial's function byte is Synapse's mode id (the device reports exactly these).
        let function = format!("{:?}", tsuba_proto::decode::KnobFunction::from(id as u8));
        slots[id as usize] = Some(DialSlotConfig { function, colour, enabled: b(m, "isEnabled").unwrap_or(true) });
    }
    if slots.iter().any(Option::is_none) {
        warnings.push("Command dial: Synapse didn't store all 8 modes; the dial setup was skipped".into());
        return None;
    }
    let slots: Vec<DialSlotConfig> = slots.into_iter().flatten().collect();
    if !slots.iter().any(|s| s.enabled) {
        warnings.push("Command dial: every mode is switched off; the dial setup was skipped".into());
        return None;
    }
    Some(slots)
}

fn device_profile(spec: Option<&db::Spec>, p: &Value, macros: &BTreeMap<String, String>, active: bool) -> ProfilePlan {
    let mut out = ProfilePlan { guid: s(p, "guid").to_string(), name: s(p, "name").trim().to_string(), active, ..Default::default() };
    if out.name.is_empty() {
        out.name = "Synapse profile".into();
    }
    let Some(spec) = spec else { return out };
    let f = &spec.features;
    let mouse = spec.kind == Kind::Mouse;
    let mut perf = Performance::default();

    // DPI stages (mice). `active` is 1-based; hidden stages are left out.
    if let Some(d) = p.get("dpiStages") {
        let stages = d.get("stages").and_then(Value::as_array).cloned().unwrap_or_default();
        if !mouse || !f.dpi_stages {
            out.warnings.push("Sensitivity stages: this device has none in Tsuba".into());
        } else if !stages.is_empty() {
            let active = n(d, "active").unwrap_or(1.0) as usize;
            let mut list = Vec::new();
            let mut act = 1;
            for (i, st) in stages.iter().enumerate() {
                if b(st, "visible") == Some(false) {
                    continue;
                }
                let x = n(st, "x").unwrap_or(800.0).max(100.0) as u32;
                let y = if b(st, "independent").unwrap_or(false) { n(st, "y").unwrap_or(x as f64).max(100.0) as u32 } else { x };
                if let Some(max) = spec.max_dpi.filter(|m| x > *m || y > *m) {
                    out.warnings.push(format!("Sensitivity stage {}: {x} DPI is above this mouse's {max} DPI; limited to {max}", i + 1));
                }
                let cap = spec.max_dpi.unwrap_or(65535).min(65535);
                list.push([x.min(cap) as u16, y.min(cap) as u16]);
                if i + 1 == active {
                    act = list.len();
                }
            }
            if !list.is_empty() {
                out.summary.push(format!(
                    "Sensitivity stages: {} (stage {act} active)",
                    list.iter().map(|[x, y]| if x == y { x.to_string() } else { format!("{x}×{y}") }).collect::<Vec<_>>().join(" / ")
                ));
                if b(d, "enable") == Some(false) {
                    out.warnings.push("Sensitivity stages were switched off in Synapse; Tsuba always uses them".into());
                }
                perf.dpi_active_stage = Some(act as u8);
                perf.dpi_stages = Some(list);
            }
        }
    }
    if let (Some(d), Some(stages)) = (p.get("dpi"), perf.dpi_stages.as_ref()) {
        let (x, y) = (n(d, "x").unwrap_or(0.0) as u16, n(d, "y").unwrap_or(0.0) as u16);
        let cur = perf.dpi_active_stage.and_then(|a| stages.get(a as usize - 1)).copied();
        if x > 0 && cur != Some([x, y]) && !stages.contains(&[x, y]) {
            out.warnings.push(format!("Current sensitivity {x} DPI isn't one of the stages; the active stage is used instead"));
        }
    }

    // Scroll wheel (mice; per profile).
    if let Some(w) = p.get("scrollWheel") {
        let mut parts = Vec::new();
        if let Some(mode) = w.get("scrollMode").and_then(Value::as_str) {
            let free = mode.eq_ignore_ascii_case("FreeSpin");
            if f.scroll_modes {
                perf.free_spin = Some(free);
                parts.push(if free { "free-spin" } else { "tactile" });
            } else if free {
                out.warnings.push("Free-spin scrolling: this device has no scroll modes in Tsuba".into());
            }
        }
        if let Some(a) = b(w, "accelerationEnabled") {
            if f.scroll_acceleration {
                perf.scroll_acceleration = Some(a);
                parts.push(if a { "acceleration on" } else { "acceleration off" });
            } else if a {
                out.warnings.push("Scroll acceleration: not available on this device in Tsuba".into());
            }
        }
        if let Some(r) = b(w, "smartReelEnabled") {
            if f.smart_reel {
                perf.smart_reel = Some(r);
                parts.push(if r { "Smart-Reel on" } else { "Smart-Reel off" });
            } else if r {
                out.warnings.push("Smart-Reel: not available on this device in Tsuba".into());
            }
        }
        if !parts.is_empty() {
            out.summary.push(format!("Scroll wheel: {}", parts.join(", ")));
        }
    }
    if perf != Performance::default() {
        out.performance = Some(perf);
    }

    // Device-wide: polling rate, lighting, dial.
    let mut dw = DeviceWide::default();
    if let Some(hz) = n(p, "pollingRate").map(|x| x as u32) {
        let ok = spec.polling_rates.contains(&hz) || (spec.polling_rates.is_empty() && tsuba_proto::decode::polling_period(hz).is_some());
        if ok {
            dw.polling_hz = Some(hz);
            dw.summary.push(format!("Polling rate {hz} Hz"));
        } else {
            out.warnings.push(format!("Polling rate {hz} Hz isn't offered for this device in Tsuba"));
        }
        if let Some(w) = n(p, "pollingRateWireless").map(|x| x as u32).filter(|w| *w != hz) {
            out.warnings.push(format!("Wireless polling rate {w} Hz: Tsuba sets one rate for the current connection"));
        }
    }
    if let Some(g) = p.get("inGamePollingRate").filter(|g| n(g, "state").unwrap_or(0.0) != 0.0) {
        out.warnings.push(format!("In-game polling rate ({} Hz) isn't supported", n(g, "wired").unwrap_or(0.0)));
    }
    if let Some(br) = p.get("brightness") {
        if b(br, "isEnabled") != Some(false) {
            if let Some(v) = n(br, "value") {
                dw.brightness = Some(v.clamp(0.0, 100.0) as u8);
                dw.summary.push(format!("Brightness {}%", v.clamp(0.0, 100.0) as u8));
            }
        }
    }
    if let Some(q) = p.get("quickEffects") {
        match quick_effect(q) {
            Ok(Some(e)) => {
                dw.summary.push(format!("Lighting effect: {}", crate::config::effect_name(&e)));
                dw.effect = Some(e);
            }
            Ok(None) => {}
            Err(w) => out.warnings.push(w),
        }
    }
    if let Some(cd) = p.get("commandDial") {
        if f.dial {
            if let Some(slots) = dial(cd, &mut out.warnings) {
                dw.summary.push(format!("Command dial: {} of 8 modes on", slots.iter().filter(|s| s.enabled).count()));
                dw.dial = Some(slots);
            }
        } else {
            out.warnings.push("Command dial: this device has no dial in Tsuba".into());
        }
    }
    out.device_wide = dw;

    // Key / button assignments.
    let mappings = p.get("mappings").and_then(Value::as_array).cloned().unwrap_or_default();
    if !mappings.is_empty() && !spec.tested {
        out.warnings.push(format!("{} key/button assignment(s) skipped: this device's button layout isn't verified with Tsuba", mappings.len()));
    } else {
        let pid = spec.pid;
        for m in &mappings {
            map_assignment(pid, m, macros, &mut out);
        }
        if pid == 0x028d && p.get("commandDial").is_none() && mappings.iter().any(|m| matches!(s(m, "inputID"), "ScrollLeft" | "ScrollRight")) {
            out.warnings.push("Dial rotation assignments without a command dial setup were skipped".into());
        }
    }
    if let Some(sp) = p.get("sidePanelMappings").and_then(Value::as_object).filter(|o| !o.is_empty()) {
        out.warnings.push(format!("{} side-panel assignment(s) aren't supported", sp.len()));
    }

    // Everything else Synapse stores in a profile.
    if let Some(g) = p.get("gamingMode") {
        let set: Vec<&str> = [("isWindowsKeyDisabled", "Windows key off"), ("isAltTabDisabled", "Alt+Tab off"), ("isAltF4Disabled", "Alt+F4 off")]
            .iter()
            .filter(|(k, _)| b(g, k) == Some(true))
            .map(|(_, l)| *l)
            .collect();
        if n(g, "state").unwrap_or(0.0) != 0.0 || !set.is_empty() {
            out.warnings.push(format!("Game Mode{} isn't imported; set it in Tsuba's PERFORMANCE tab", if set.is_empty() { String::new() } else { format!(" ({})", set.join(", ")) }));
        }
    }
    let power: Vec<&str> = ["switchOffLighting", "ledPowerSettings", "lowBatteryEffects", "sleepModeInSeconds", "lowPowerMode", "powerSavingValue"]
        .into_iter()
        .filter(|k| p.get(*k).is_some())
        .collect();
    if !power.is_empty() {
        out.warnings.push("Lighting-off / sleep / power-saving options aren't imported (Tsuba keeps them in the POWER and LIGHTING tabs)".into());
    }
    for (k, what) in [("snapTap", "Snap Tap"), ("functionKeyPrimary", "Function-key row mode"), ("calibration", "Surface calibration")] {
        if p.get(k).is_some() {
            out.warnings.push(format!("{what} isn't supported"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// Macros

fn convert_macro(m: &Value) -> MacroPlan {
    let sg = s(m, "guid").to_string();
    let mut name = s(m, "name").trim().to_string();
    if name.is_empty() {
        name = "Synapse macro".into();
    }
    let mut plan = MacroPlan { guid: tsuba_guid(&sg), synapse_guid: sg, name: name.clone(), events: 0, warnings: Vec::new(), mac: Macro { name, ..Default::default() } };
    let events = m.get("macroEvents").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<MacroEvent> = Vec::new();
    let mut loop_start: Option<(usize, u32)> = None;
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    for e in &events {
        let t = e.get("Type");
        match t.and_then(Value::as_i64) {
            Some(0) => {
                let ms = (n(e, "Number").unwrap_or(0.0) * 1000.0).round().max(0.0) as u32;
                if ms > 0 {
                    out.push(MacroEvent::Delay { ms });
                }
            }
            Some(1) => {
                let k = e.get("KeyEvent").cloned().unwrap_or(Value::Null);
                let code = n(&k, "Makecode").unwrap_or(-1.0) as i64;
                let state = n(&k, "State").unwrap_or(0.0) as i64;
                match u32::try_from(code).ok().and_then(|c| keys::hid_for_vk(c, state & 2 != 0)) {
                    Some(hid) => out.push(MacroEvent::Key { hid, down: state & 1 == 0 }),
                    None => *skipped.entry(format!("key code {code}")).or_default() += 1,
                }
            }
            Some(2) => {
                let me = e.get("MouseEvent").cloned().unwrap_or(Value::Null);
                // Browser button numbering: 0 left, 1 middle, 2 right, 3 back, 4 forward.
                let btn = match n(&me, "MouseButton").map(|x| x as i64) {
                    Some(0) => Some(1),
                    Some(1) => Some(3),
                    Some(2) => Some(2),
                    Some(3) => Some(4),
                    Some(4) => Some(5),
                    _ => None,
                };
                match btn {
                    Some(button) => out.push(MacroEvent::Button { button, down: n(&me, "State").unwrap_or(0.0) == 0.0 }),
                    None => *skipped.entry("mouse button".into()).or_default() += 1,
                }
            }
            Some(6) => {
                let le = e.get("LoopEvent").cloned().unwrap_or(Value::Null);
                if n(&le, "State").unwrap_or(0.0) == 0.0 {
                    loop_start = Some((out.len(), n(e, "Number").unwrap_or(1.0).max(1.0) as u32));
                } else if let Some((start, times)) = loop_start.take() {
                    let body: Vec<MacroEvent> = out[start..].to_vec();
                    let times = times.min(100);
                    for _ in 1..times {
                        out.extend(body.iter().cloned());
                    }
                    if times > 1 {
                        plan.warnings.push(format!("A loop ({times}×) was written out as repeated events"));
                    }
                }
            }
            _ if t.and_then(Value::as_str) == Some("actionBar") => {}
            _ => *skipped.entry(format!("event type {}", t.map(|v| v.to_string()).unwrap_or_else(|| "?".into()))).or_default() += 1,
        }
    }
    for (what, count) in skipped {
        plan.warnings.push(format!("{count} event(s) skipped: {what} isn't supported"));
    }
    if out.iter().all(|e| matches!(e, MacroEvent::Delay { .. })) {
        plan.warnings.push("The macro has no key or button events".into());
    }
    // Keys/buttons left down at the end stay pressed until released by hand.
    let mut held: Vec<String> = Vec::new();
    let mut down: BTreeMap<(bool, u8), bool> = BTreeMap::new();
    for e in &out {
        match *e {
            MacroEvent::Key { hid, down: d } => {
                down.insert((true, hid), d);
            }
            MacroEvent::Button { button, down: d } => {
                down.insert((false, button), d);
            }
            _ => {}
        }
    }
    for ((key, code), d) in down {
        if d {
            held.push(if key { format!("key 0x{code:02x}") } else { format!("mouse button {code}") });
        }
    }
    if !held.is_empty() {
        plan.warnings.push(format!("Pressed but never released: {} (Synapse may have released it for you)", held.join(", ")));
    }
    plan.events = out.len();
    plan.mac = Macro { guid: plan.guid.clone(), name: plan.name.clone(), events: out, ..Default::default() };
    plan
}

// ---------------------------------------------------------------------------------------
// Chroma Studio

fn colour(v: Option<&Value>) -> Option<String> {
    let c = v?.as_f64()? as u32;
    Some(format!("{:06x}", c & 0xff_ffff))
}

fn stop_colours(p: &Value) -> Vec<String> {
    p.get("colorStops").and_then(Value::as_array).map(|a| a.iter().filter_map(|s| colour(s.get("Color"))).collect()).unwrap_or_default()
}

/// Effect duration relative to Synapse's default for the effect.
fn duration(ms: Option<f64>, default_ms: f64) -> Duration {
    match ms {
        Some(ms) if ms < default_ms * 0.7 => Duration::Short,
        Some(ms) if ms > default_ms * 1.4 => Duration::Long,
        _ => Duration::Medium,
    }
}

/// Synapse wave angle (degrees, CSS-style: 90 = towards the right) → direction.
fn wave_direction(angle: f64) -> Direction {
    match (angle.rem_euclid(360.0) / 90.0).round() as i64 % 4 {
        0 => Direction::BottomToTop,
        1 => Direction::LeftToRight,
        2 => Direction::TopToBottom,
        _ => Direction::RightToLeft,
    }
}

fn studio_effect(name: &str, p: &Value, warnings: &mut BTreeSet<String>) -> Option<StudioEffect> {
    let custom = b(p, "colorStopsCustom").unwrap_or(false);
    let random = b(p, "randomColor").unwrap_or(false);
    let e = match name {
        "static" => StudioEffect::Static { colour: colour(p.get("color")).unwrap_or_else(|| "00ff00".into()) },
        "breathing" => {
            let colours = if random { vec![] } else { [colour(p.get("color")), colour(p.get("color2"))].into_iter().flatten().collect() };
            StudioEffect::Breathing { colours, duration: duration(n(p, "duration"), 6000.0) }
        }
        "starlight" => {
            let colours = if random {
                vec![]
            } else if custom {
                stop_colours(p)
            } else {
                [colour(p.get("color")), colour(p.get("color2"))].into_iter().flatten().collect()
            };
            let density = n(p, "density").unwrap_or(10.0).clamp(1.0, 100.0) as f32;
            StudioEffect::Starlight { colours, duration: duration(n(p, "duration"), 2000.0), density: 0.15 * density / 10.0 }
        }
        "wave" => {
            if n(p, "width").is_some_and(|w| w != 100.0) || b(p, "split") == Some(true) || n(p, "pause").is_some_and(|x| x != 0.0) {
                warnings.insert("Wave width / split / pause settings aren't supported".into());
            }
            let speed = n(p, "speed").unwrap_or(15.0);
            let dur = if speed >= 25.0 { Duration::Short } else if speed <= 8.0 { Duration::Long } else { Duration::Medium };
            StudioEffect::Wave { direction: wave_direction(n(p, "angle").unwrap_or(90.0)), duration: dur, colours: if custom { stop_colours(p) } else { vec![] } }
        }
        "spectrum" | "spectrumCycling" | "spectrumcycling" => StudioEffect::Spectrum { duration: duration(n(p, "duration"), 4000.0) },
        "wheel" => StudioEffect::Wheel { direction: Direction::Clockwise, duration: Duration::Medium, centre: [0.5, 0.5], spread: 1.0, colours: vec![] },
        "fire" => StudioEffect::Fire { duration: Duration::Medium },
        "ambient" | "ambientAwareness" => StudioEffect::Ambient { boost: 1.3, area: [0.0, 0.0, 1.0, 1.0] },
        "audio" | "audioMeter" => {
            if custom {
                warnings.insert("Audio meter: custom colours aren't supported; the green–red gradient is used".into());
            }
            StudioEffect::AudioMeter { colour: None, gain: 1.0, flow: false, boost: n(p, "boost").unwrap_or(1.0).clamp(0.1, 4.0) as f32, source: None, mode: Default::default(), peak_hold: false }
        }
        other => {
            warnings.insert(format!("{} layers aren't supported yet", if other.is_empty() { "Unnamed" } else { other }));
            return None;
        }
    };
    if p.get("playbackEnd").and_then(Value::as_str).is_some_and(|e| e != "never") || n(p, "cycles").is_some_and(|c| c > 0.0) {
        warnings.insert(format!("{name}: limited play counts aren't supported; the layer plays continuously"));
    }
    Some(e)
}

/// Synapse Chroma Studio grid position → device LED grid position, from the lighting-engine
/// manifest Synapse cached for that device (`ledConfig.LedMatrix`).
fn led_map(manifest: Option<&Value>) -> Option<BTreeMap<(u32, u32), [u8; 2]>> {
    let cfgs = manifest?.get("ledConfig")?.as_object()?;
    let cfg = cfgs.values().next()?;
    let mut m = BTreeMap::new();
    for e in cfg.get("LedMatrix")?.as_array()? {
        let mp = e.get("MatrixPos")?.as_array()?;
        let dp = e.get("DevicePos")?.as_array()?;
        let (r, c) = (mp.first()?.as_u64()? as u32, mp.get(1)?.as_u64()? as u32);
        m.insert((r, c), [dp.first()?.as_u64()? as u8, dp.get(1)?.as_u64()? as u8]);
    }
    (!m.is_empty()).then_some(m)
}

fn lighting_profile(prof: &Value, data: &SynapseData, all_leds: &BTreeMap<u32, BTreeSet<(u32, u32)>>) -> LightingPlan {
    let mut plan = LightingPlan { id: s(prof, "id").to_string(), name: s(prof, "title").trim().to_string(), ..Default::default() };
    if plan.name.is_empty() {
        plan.name = "Synapse effect".into();
    }
    let names: BTreeMap<u32, String> = prof
        .get("devices")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|d| Some((n(d, "productId")? as u32, s(d, "productName").to_string()))).collect())
        .unwrap_or_default();
    let mut warnings: BTreeSet<String> = BTreeSet::new();
    let mut devices: BTreeMap<String, Studio> = BTreeMap::new();
    let mut skipped_dev: BTreeMap<String, usize> = BTreeMap::new();
    let layers = prof.get("layers").and_then(Value::as_array).cloned().unwrap_or_default();
    // Synapse lists the top layer first; Tsuba stacks bottom first.
    for l in layers.iter().rev() {
        if s(l, "type") != "layer" && !s(l, "type").is_empty() {
            warnings.insert(format!("{} entries (layer groups) aren't supported; their layers are imported ungrouped", s(l, "type")));
            continue;
        }
        let ename = s(l, "name");
        let title = s(l, "title");
        let regions = l.get("regions").and_then(Value::as_array).cloned().unwrap_or_default();
        for (ri, r) in regions.iter().enumerate() {
            let params = r.get("params").cloned().unwrap_or(Value::Null);
            let Some(effect) = studio_effect(ename, &params, &mut warnings) else { continue };
            for d in r.get("devices").and_then(Value::as_array).cloned().unwrap_or_default() {
                let pidn = n(&d, "productId").unwrap_or(0.0) as u32;
                let dname = names.get(&pidn).cloned().unwrap_or_else(|| format!("Synapse device {pidn}"));
                let Some(spec) = match_device(pidn, &dname) else {
                    *skipped_dev.entry(format!("{dname} (not in Tsuba's device database)")).or_default() += 1;
                    continue;
                };
                if !spec.supported || spec.matrix.is_none() {
                    *skipped_dev.entry(format!("{dname} (no per-LED lighting in Tsuba)")).or_default() += 1;
                    continue;
                }
                let sel: BTreeSet<(u32, u32)> = d
                    .get("leds")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|e| Some((n(e, "row")? as u32, n(e, "col")? as u32))).collect())
                    .unwrap_or_default();
                let [rows, cols] = spec.matrix.unwrap_or([1, 1]);
                let leds: Vec<[u8; 2]> = if rows as u32 * cols as u32 <= 1 {
                    vec![]
                } else if let Some(map) = led_map(data.manifests.get(&pidn)) {
                    let mut v: Vec<[u8; 2]> = sel.iter().filter_map(|p| map.get(p).copied()).filter(|[r, c]| *r < rows && *c < cols).collect();
                    v.sort();
                    v.dedup();
                    if v.is_empty() {
                        *skipped_dev.entry(format!("{dname} (none of the selected LEDs exist on the device)")).or_default() += 1;
                        continue;
                    }
                    let all: BTreeSet<[u8; 2]> = map.values().copied().collect();
                    if v.len() == all.len() { vec![] } else { v }
                } else {
                    // Without Synapse's LED map only a whole-device selection converts safely.
                    let seen = all_leds.get(&pidn).map_or(0, BTreeSet::len);
                    if sel.len() >= seen && seen > 0 {
                        vec![]
                    } else {
                        *skipped_dev.entry(format!("{dname} (part-device LED selections can't be converted without Synapse's LED map for it)")).or_default() += 1;
                        continue;
                    }
                };
                let layer_name = if regions.len() > 1 { format!("{} {}", title_case(if title.is_empty() { ename } else { title }), ri + 1) } else { title_case(if title.is_empty() { ename } else { title }) };
                devices.entry(crate::pid_key(spec.pid)).or_default().layers.push(Layer {
                    name: layer_name,
                    enabled: b(l, "visible").unwrap_or(true),
                    leds,
                    effect: effect.clone(),
                    brightness: 1.0,
                    blend: Default::default(),
                    opacity: 1.0,
                    speed: 1.0,
                    span: None,
                });
            }
        }
    }
    // Synapse users often stack many copies of one effect on different keys (one layer per key
    // group); neighbouring layers that only differ in their LEDs are merged.
    for st in devices.values_mut() {
        let mut merged: Vec<Layer> = Vec::new();
        for l in std::mem::take(&mut st.layers) {
            match merged.last_mut() {
                Some(prev) if prev.effect == l.effect && prev.enabled == l.enabled && prev.blend == l.blend && prev.brightness == l.brightness => {
                    if prev.leds.is_empty() || l.leds.is_empty() {
                        prev.leds.clear();
                    } else {
                        prev.leds.extend(l.leds);
                        prev.leds.sort();
                        prev.leds.dedup();
                    }
                }
                _ => merged.push(l),
            }
        }
        st.layers = merged;
    }
    for (dev, count) in skipped_dev {
        warnings.insert(format!("{count} layer(s) on {dev} skipped"));
    }
    plan.devices = devices.keys().map(|p| u16::from_str_radix(p, 16).ok().and_then(db::get).map_or(p.clone(), |s| s.name.replace(" (wired)", ""))).collect();
    plan.layers = devices.values().map(|s| s.layers.len()).sum();
    plan.importable = plan.layers > 0;
    if !plan.importable {
        warnings.insert("Nothing in this effect maps to a Tsuba device".into());
    }
    plan.warnings = warnings.into_iter().collect();
    plan.profile = ChromaProfile { name: plan.name.clone(), devices, canvas: Default::default() };
    plan
}

fn title_case(s: &str) -> String {
    let lower = s.replace('_', " ").to_lowercase();
    let mut c = lower.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------------------

/// Build the import plan from a decoded store.
pub fn build_plan(data: &SynapseData, store: &str) -> Plan {
    let mut plan = Plan { store: store.to_string(), ..Default::default() };
    plan.errors = data.errors.iter().map(|(p, e)| format!("{p}: {e}")).collect();

    // Macros first: profiles refer to them.
    for (path, v) in &data.nodes {
        if path.starts_with("macro/profiles/") && v.is_object() {
            plan.macros.push(convert_macro(v));
        }
    }
    plan.macros.sort_by_key(|m| m.name.to_lowercase());
    let macro_names: BTreeMap<String, String> = plan.macros.iter().map(|m| (m.guid.clone(), m.name.clone())).collect();

    // Devices: devices/<productId>/{profiles,deviceMetadatas}/…
    let mut by_product: BTreeMap<u32, (String, Option<String>, Vec<&Value>)> = BTreeMap::new();
    for (path, v) in &data.nodes {
        let parts: Vec<&str> = path.split('/').collect();
        let ["devices", pid, folder, _] = parts.as_slice() else { continue };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        let e = by_product.entry(pid).or_default();
        match *folder {
            "profiles" if v.is_object() => e.2.push(v),
            "deviceMetadatas" => {
                if e.0.is_empty() {
                    e.0 = s(v, "name").to_string();
                }
                if let Some(a) = v.get("activeProfileGuid").and_then(Value::as_str) {
                    e.1 = Some(a.to_string());
                }
            }
            _ => {}
        }
    }
    for (pid, (mut name, active, profiles)) in by_product {
        if name.is_empty() {
            name = data.manifests.get(&pid).map(|m| s(m, "productName").to_string()).unwrap_or_default();
        }
        if name.is_empty() {
            name = format!("Synapse device {pid}");
        }
        let spec = match_device(pid, &name);
        let mut d = DevicePlan { product_id: pid, name: name.clone(), ..Default::default() };
        match spec {
            Some(sp) if sp.supported => {
                d.pid = Some(crate::pid_key(sp.pid));
                d.kind = Some(sp.kind.as_str().to_string());
                d.verified = sp.tested;
                d.importable = true;
                if !sp.tested {
                    d.note = format!("Matched to {} by name. Its buttons aren't verified with Tsuba, so key assignments are skipped.", sp.name);
                }
            }
            Some(sp) => d.note = format!("{} is known to Tsuba but not supported (read-only).", sp.name),
            None => d.note = "Not in Tsuba's device database; its profiles can't be imported.".into(),
        }
        let mut list: Vec<ProfilePlan> = profiles.iter().map(|p| device_profile(spec.filter(|s| s.supported), p, &macro_names, active.as_deref() == Some(s(p, "guid")))).collect();
        list.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        d.profiles = list;
        plan.devices.push(d);
    }
    plan.devices.sort_by_key(|d| (!d.importable, d.name.clone()));

    // Chroma Studio profiles.
    let mut all_leds: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let studio: Vec<&Value> = data.nodes.iter().filter(|(p, v)| p.starts_with("chromaStudio/chromaStudioProfiles/") && v.is_object()).map(|(_, v)| v).collect();
    for prof in &studio {
        for l in prof.get("layers").and_then(Value::as_array).into_iter().flatten() {
            for r in l.get("regions").and_then(Value::as_array).into_iter().flatten() {
                for d in r.get("devices").and_then(Value::as_array).into_iter().flatten() {
                    let set = all_leds.entry(n(d, "productId").unwrap_or(0.0) as u32).or_default();
                    for e in d.get("leds").and_then(Value::as_array).into_iter().flatten() {
                        if let (Some(r), Some(c)) = (n(e, "row"), n(e, "col")) {
                            set.insert((r as u32, c as u32));
                        }
                    }
                }
            }
        }
    }
    plan.lighting = studio.iter().map(|p| lighting_profile(p, data, &all_leds)).collect();
    plan.lighting.sort_by_key(|l| l.name.to_lowercase());
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data(nodes: Vec<(&str, Value)>) -> SynapseData {
        SynapseData { nodes: nodes.into_iter().map(|(k, v)| (k.to_string(), v)).collect(), ..Default::default() }
    }

    #[test]
    fn device_matching() {
        assert_eq!(match_device(653, "").unwrap().pid, 0x028d);
        assert_eq!(match_device(170, "whatever").unwrap().pid, 0x00aa);
        let m = match_device(9999, "Razer Basilisk Ultimate").unwrap();
        assert!(m.name.starts_with("Basilisk Ultimate"));
        assert_eq!(m.connection.as_deref().unwrap_or("wired"), "wired");
        assert!(match_device(9999, "G733").is_none());
    }

    #[test]
    fn mouse_profile() {
        let p = json!({
            "guid": "p1", "name": "Game",
            "pollingRate": 1000, "pollingRateWireless": 500,
            "brightness": {"isEnabled": true, "value": 33},
            "quickEffects": {"selectedEffectId": 7, "selectedEffectSetting": {"color1": "#00ff00", "duration": 2}},
            "dpiStages": {"active": 3, "count": 4, "enable": true, "stages": [
                {"x": 400, "y": 400, "visible": true}, {"x": 800, "y": 800, "visible": false},
                {"x": 1600, "y": 1600, "visible": true}, {"x": 3200, "y": 1600, "independent": true, "visible": true}]},
            "scrollWheel": {"scrollMode": "FreeSpin", "accelerationEnabled": false, "smartReelEnabled": true},
            "mappings": [
                {"inputType": "MouseInput", "inputID": "Button4", "isHyperShift": false, "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_F11", "modifiers": ["KEY_LEFT_CTRL"]}},
                {"inputType": "MouseInput", "inputID": "Button5", "isHyperShift": true, "outputType": "mouseGroup", "mouseGroup": {"mouseAssignment": "Click", "turboMode": {"isTurbo": true, "keysPerSecond": 20}}},
                {"inputType": "MouseInput", "inputID": "RepeatScrollRight", "isHyperShift": false, "outputType": "sensitivityGroup", "sensitivityGroup": {"sensitivityAssignment": "DPI_Clutch", "x": 30000, "y": 30000}},
                {"inputType": "DKMInput", "inputID": "DKM_SB_02", "isHyperShift": false, "outputType": "macroGroup", "macroGroup": {"guid": "0000aaaa-0000-0000-0000-000000000001", "name": "M", "macroPlaybackOption": "ContinuousToggle", "repeatCount": 2}},
                {"inputType": "MouseInput", "inputID": "ScrollButton", "isHyperShift": false, "outputType": "launchGroup", "launchGroup": {}},
                {"inputType": "MouseInput", "inputID": "Mystery", "isHyperShift": false, "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_A"}}
            ],
            "switchOffLighting": {}
        });
        let mac = json!({"guid": "0000aaaa-0000-0000-0000-000000000001", "name": "M", "macroEvents": []});
        let d = data(vec![("devices/170/profiles/p1", p), ("devices/170/deviceMetadatas/x", json!({"name": "Razer Basilisk V3 Pro", "activeProfileGuid": "p1"})), ("macro/profiles/0000aaaa-0000-0000-0000-000000000001", mac)]);
        let plan = build_plan(&d, "x");
        let dev = &plan.devices[0];
        assert_eq!(dev.pid.as_deref(), Some("00aa"));
        let pr = &dev.profiles[0];
        assert!(pr.active);
        let perf = pr.performance.as_ref().unwrap();
        assert_eq!(perf.dpi_stages.as_deref(), Some(&[[400, 400], [1600, 1600], [3200, 1600]][..]));
        assert_eq!(perf.dpi_active_stage, Some(2));
        assert_eq!((perf.free_spin, perf.scroll_acceleration, perf.smart_reel), (Some(true), Some(false), Some(true)));
        assert_eq!(pr.device_wide.polling_hz, Some(1000));
        assert_eq!(pr.device_wide.brightness, Some(33));
        // Synapse effect 7 = Starlight (colour + duration).
        assert_eq!(pr.device_wide.effect, Some(EffectConfig::Starlight { speed: 2, colours: vec!["00ff00".into()] }));
        assert_eq!(
            pr.assignments,
            vec![
                AssignmentPlan::Function { key: 4, layer: 0, function: 2, data: vec![0x01, 0x44], label: "Mouse button 4 → LEFT CTRL + F11".into() },
                AssignmentPlan::Function { key: 5, layer: 1, function: 14, data: vec![1, 0, 50], label: "Mouse button 5 (Hypershift) → left click (turbo)".into() },
                AssignmentPlan::Function { key: 53, layer: 0, function: 6, data: vec![5, 0x75, 0x30, 0x75, 0x30], label: "Wheel tilt right → sensitivity clutch 30000 DPI".into() },
                AssignmentPlan::Macro { key: 15, layer: 0, guid: "0000aaaa000000000000000000000001".into(), playback: Playback::Toggle, count: 1, label: "Sniper button → macro “M” (toggle on/off)".into() },
            ]
        );
        assert_eq!(pr.macros, vec!["0000aaaa000000000000000000000001".to_string()]);
        let w = pr.warnings.join("\n");
        assert!(!w.contains("quick effect #7"), "{w}");
        assert!(w.contains("Wheel click: launch assignments aren't supported"), "{w}");
        assert!(w.contains("Mystery: this input isn't mapped"), "{w}");
        assert!(w.contains("Wireless polling rate 500"), "{w}");
        assert!(w.contains("power-saving"), "{w}");
    }

    #[test]
    fn keyboard_profile_with_dial() {
        let modes: Vec<Value> = ["KEYBOARD_BRIGHTNESS", "WINDOWS_ZOOM", "SWITCH_APPLICATIONS", "TRACK_JOGGING", "TRACK_SELECTOR", "VERTICAL_SCROLLING", "HORIZONTAL_SCROLLING", "SWITCH_BROWSER_TABS"]
            .iter()
            .enumerate()
            .map(|(i, n)| json!({"uid": i.to_string(), "isCustom": false, "color": "#FEED03", "name": n, "isEnabled": i < 4, "modeId": i, "mappings": []}))
            .collect();
        let p = json!({
            "guid": "k", "name": "Work", "pollingRate": 8000,
            "quickEffects": {"selectedEffectId": 4, "selectedEffectSetting": {"direction": 2, "speed": 1}},
            "commandDial": {"currentModeUID": "0", "modes": modes},
            "mappings": [
                {"inputType": "MouseInput", "inputID": "ScrollRight", "isHyperShift": false, "outputType": "backlightGroup", "backlightGroup": {"backlightAssignment": "BrightnessUp"}},
                {"inputType": "DKMInput", "inputID": "DKM_M_05", "isHyperShift": false, "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_BACKSPACE", "modifiers": ["KEY_RIGHT_ALT"]}},
                {"inputType": "DKMInput", "inputID": "DKM_M_01", "isHyperShift": true, "outputType": "backlightGroup", "backlightGroup": {"backlightAssignment": "BrightnessDown"}}
            ],
            "snapTap": {}
        });
        let plan = build_plan(&data(vec![("devices/653/profiles/k", p)]), "x");
        let pr = &plan.devices[0].profiles[0];
        assert_eq!(plan.devices[0].pid.as_deref(), Some("028d"));
        assert_eq!(pr.device_wide.polling_hz, Some(8000));
        assert_eq!(pr.device_wide.effect, Some(EffectConfig::Wave { direction: 2, speed: 0x28 }));
        let dial = pr.device_wide.dial.as_ref().unwrap();
        assert_eq!(dial.len(), 8);
        assert_eq!(dial[3], DialSlotConfig { function: "TrackJogging".into(), colour: "feed03".into(), enabled: true });
        assert!(!dial[5].enabled);
        assert_eq!(pr.assignments.len(), 2);
        assert_eq!(pr.assignments[0], AssignmentPlan::Function { key: 164, layer: 0, function: 2, data: vec![0x40, 0x2a], label: "M5 → RIGHT ALT + BACKSPACE".into() });
        assert!(matches!(&pr.assignments[1], AssignmentPlan::Action { key: 160, layer: 1, action: Action::Device { target, op: DeviceOp::BrightnessDown }, .. } if target == "028d"));
        assert!(pr.warnings.iter().any(|w| w.contains("Snap Tap")));
    }

    #[test]
    fn unknown_and_untested_devices() {
        let p = json!({"guid": "a", "name": "A", "dpiStages": {"active": 1, "stages": [{"x": 800, "y": 800}]},
                       "mappings": [{"inputID": "Button4", "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_1"}}]});
        let d = data(vec![
            ("devices/134/profiles/a", p.clone()),
            ("devices/134/deviceMetadatas/s", json!({"name": "Razer Basilisk Ultimate"})),
            ("devices/2814/profiles/a", p),
            ("devices/2814/deviceMetadatas/s", json!({"name": "G733"})),
        ]);
        let plan = build_plan(&d, "x");
        let bu = plan.devices.iter().find(|d| d.product_id == 134).unwrap();
        assert!(bu.importable && !bu.verified);
        assert!(bu.profiles[0].assignments.is_empty());
        assert!(bu.profiles[0].warnings.iter().any(|w| w.contains("isn't verified")), "{:?}", bu.profiles[0].warnings);
        // Tsuba's database doesn't list DPI stages for this model: reported, not dropped.
        assert!(bu.profiles[0].performance.is_none());
        assert!(bu.profiles[0].warnings.iter().any(|w| w.starts_with("Sensitivity stages")));
        let g = plan.devices.iter().find(|d| d.product_id == 2814).unwrap();
        assert!(!g.importable && g.pid.is_none());
    }

    #[test]
    fn macro_conversion() {
        let m = json!({"guid": "ABCD-ef01", "name": " Strat ", "macroEvents": [
            {"Type": "actionBar", "recordProfile": {}},
            {"Type": 1, "KeyEvent": {"Makecode": 67, "State": 0}},
            {"Type": 0, "Number": 0.461},
            {"Type": 1, "KeyEvent": {"Makecode": 38, "State": 2}},
            {"Type": 0, "Number": "0.1"},
            {"Type": 1, "KeyEvent": {"Makecode": 38, "State": 3}},
            {"Type": 6, "LoopEvent": {"State": 0}, "Number": 2},
            {"Type": 2, "MouseEvent": {"MouseButton": 0, "State": 0}},
            {"Type": 2, "MouseEvent": {"MouseButton": 2, "State": 1}},
            {"Type": 6, "LoopEvent": {"State": 1}, "Number": 2},
            {"Type": 1, "KeyEvent": {"Makecode": 67, "State": 1}},
            {"Type": 1, "KeyEvent": {"Makecode": 250, "State": 1}},
            {"Type": 9}
        ]});
        let p = convert_macro(&m);
        assert_eq!(p.guid, "abcdef01");
        assert_eq!(p.name, "Strat");
        use MacroEvent::*;
        assert_eq!(
            p.mac.events,
            vec![
                Key { hid: 0x06, down: true },
                Delay { ms: 461 },
                Key { hid: 0x52, down: true },
                Delay { ms: 100 },
                Key { hid: 0x52, down: false },
                Button { button: 1, down: true },
                Button { button: 2, down: false },
                Button { button: 1, down: true },
                Button { button: 2, down: false },
                Key { hid: 0x06, down: false },
            ]
        );
        assert_eq!(p.warnings.len(), 4, "{:?}", p.warnings);
        assert!(p.warnings[3].contains("mouse button 1"));
    }

    #[test]
    fn studio_conversion() {
        let manifest = json!({"productName": "Razer Basilisk V3 Pro", "ledConfig": {"ledConfig_0_0": {"LedMatrix": [
            {"MatrixPos": [0, 0], "DevicePos": [0, 2]}, {"MatrixPos": [0, 2], "DevicePos": [0, 1]}, {"MatrixPos": [5, 2], "DevicePos": [0, 0]}]}}});
        let prof = json!({"id": "e1", "title": "Mix", "devices": [
                {"productId": 170, "productName": "Razer Basilisk V3 Pro"}, {"productId": 653, "productName": "Razer BlackWidow V4 Pro"}, {"productId": 2814, "productName": "G733"}],
            "layers": [
                {"type": "layer", "name": "static", "title": "STATIC", "visible": true, "regions": [{"params": {"color": 16711680}, "devices": [
                    {"productId": 170, "leds": [{"row": 0, "col": 2}, {"row": 9, "col": 9}]},
                    {"productId": 653, "leds": [{"row": 1, "col": 3}]},
                    {"productId": 2814, "leds": [{"row": 0, "col": 0}]}]}]},
                {"type": "layer", "name": "wave", "title": "WAVE", "visible": false, "regions": [{"params": {"angle": 270, "speed": 15, "colorStopsCustom": false, "playbackEnd": "never", "cycles": -1}, "devices": [
                    {"productId": 653, "leds": [{"row": 1, "col": 3}, {"row": 2, "col": 4}]},
                    {"productId": 170, "leds": [{"row": 0, "col": 0}, {"row": 0, "col": 2}, {"row": 5, "col": 2}]}]}]},
                {"type": "layer", "name": "reactive", "regions": [{"params": {}, "devices": [{"productId": 653, "leds": []}]}]}
            ]});
        let mut d = data(vec![("chromaStudio/chromaStudioProfiles/e1", prof)]);
        d.manifests.insert(170, manifest);
        let plan = build_plan(&d, "x");
        let l = &plan.lighting[0];
        assert!(l.importable);
        let mouse = &l.profile.devices["00aa"].layers;
        // Bottom first: the wave (last in Synapse's list) comes first.
        assert_eq!(mouse.len(), 2);
        assert_eq!(mouse[0].name, "Wave");
        assert!(!mouse[0].enabled);
        assert!(mouse[0].leds.is_empty(), "every LED of the map selected");
        assert_eq!(mouse[0].effect, StudioEffect::Wave { direction: Direction::RightToLeft, duration: Duration::Medium, colours: vec![] });
        assert_eq!(mouse[1].leds, vec![[0, 1]]);
        assert_eq!(mouse[1].effect, StudioEffect::Static { colour: "ff0000".into() });
        // Keyboard: no LED map; the wave covers every LED seen for it, the static layer doesn't.
        let kb = &l.profile.devices["028d"].layers;
        assert_eq!(kb.len(), 1);
        assert!(kb[0].leds.is_empty());
        let w = l.warnings.join("\n");
        assert!(w.contains("reactive layers aren't supported"), "{w}");
        assert!(w.contains("G733"), "{w}");
        assert!(w.contains("part-device LED selections"), "{w}");
    }

    #[test]
    fn quick_effect_ids() {
        let q = |id: i64, set: Value| quick_effect(&json!({"selectedEffectId": id, "selectedEffectSetting": set}));
        assert_eq!(q(2, json!({"color1": "#ff0000", "color2": "#0000ff", "isRandom": false})).unwrap(), Some(EffectConfig::Breathing { colours: vec!["ff0000".into(), "0000ff".into()] }));
        assert_eq!(q(2, json!({"color1": "#ff0000", "isRandom": true})).unwrap(), Some(EffectConfig::Breathing { colours: vec![] }));
        assert_eq!(q(19, json!({"direction": 2})).unwrap(), Some(EffectConfig::Wave { direction: 2, speed: 0x28 }));
        assert!(matches!(q(8, json!({})).unwrap(), Some(EffectConfig::Studio { .. })));
        assert!(matches!(q(14, json!({})).unwrap(), Some(EffectConfig::Studio { .. })));
        assert!(q(99, json!({})).is_err());
    }

    #[test]
    fn keyboard_key_inputs() {
        assert_eq!(input_position(0x028d, "KEY_A"), Some(31));
        assert_eq!(input_position(0x028d, "KEY_LEFT_CTRL"), Some(58));
        assert_eq!(input_position(0x028d, "KEY_F1"), Some(112));
        assert_eq!(input_position(0x028d, "DKM_M_01"), Some(160));
        assert_eq!(input_position(0x028d, "KEY_NOT_A_KEY"), None);
    }
}
