//! On-disk configuration: `~/.config/tsuba/config.toml`.
//!
//! ```toml
//! [[device]]
//! pid = "028d"
//! name = "BlackWidow V4 Pro"
//! lighting = { effect = { type = "spectrum" }, brightness = 100 }
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::state::DeviceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Game lighting: let games that support Razer Chroma light the devices through the
    /// local REST server on 127.0.0.1:54235. Unset = on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_lighting: Option<bool>,
    /// Game lighting: colour keyboard underglow from a game's Chroma Link / mouse mat /
    /// headset effect (when the game sends one). Unset = off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_lighting_underglow: Option<bool>,
    /// Game lighting: also answer web pages (browser-based lighting apps). Any website could
    /// then change the lighting while it is open. Unset = off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_lighting_web: Option<bool>,
    #[serde(default, rename = "device")]
    pub devices: Vec<DeviceConfig>,
}

impl Config {
    pub fn game_lighting_enabled(&self) -> bool {
        self.game_lighting.unwrap_or(true)
    }

    pub fn game_lighting_underglow_enabled(&self) -> bool {
        self.game_lighting_underglow.unwrap_or(false)
    }

    pub fn game_lighting_web_enabled(&self) -> bool {
        self.game_lighting_web.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DeviceConfig {
    pub pid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Device mode applied on attach: "normal" (firmware runs remaps, macros, media keys and
    /// the dial itself) or "driver" (Razer keys are reported to software). Unset = leave as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<Lighting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<DpiConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial: Option<Vec<DialSlotConfig>>,
    /// Software actions bound to keys via dynamic Razer-key codes (tsubad runs them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionConfig>,
    /// Switch lighting off when idle / when the display turns off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting_power: Option<LightingPower>,
    /// Keyboard Game Mode (Synapse does it in software; so does Tsuba).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_mode: Option<GameModeConfig>,
    /// Let tsubad write to this device although its model has not been tested with Tsuba
    /// (`tested = false` in the device database). Off by default: untested devices are read-only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_untested_writes: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LightingPower {
    /// Minutes without input before lighting turns off (0 = never).
    #[serde(default)]
    pub idle_minutes: u32,
    /// Turn off together with the display (after `display_off_minutes` idle).
    #[serde(default)]
    pub display_off: bool,
    /// KDE's display-off timeout (Plasma default: 10 minutes).
    #[serde(default = "ten")]
    pub display_off_minutes: u32,
}

fn ten() -> u32 {
    10
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GameModeConfig {
    #[serde(default = "yes")]
    pub disable_win: bool,
    #[serde(default)]
    pub disable_menu: bool,
    /// Block desktop shortcuts (Alt+Tab, Alt+F4, Meta…) through KDE while active.
    #[serde(default = "yes")]
    pub block_shortcuts: bool,
    #[serde(default)]
    pub active: bool,
    /// Original bindings of the keys Game Mode turned off: (profile, key, function, data).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved: Vec<(u8, u8, u8, Vec<u8>)>,
    /// Which desktop shortcuts to block while active, by name (see [`GAME_MODE_COMBOS`]).
    /// None = all of them, through KDE's global block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_combos: Option<Vec<String>>,
}

/// Desktop shortcuts Game Mode can block one by one: (name, label, Qt key codes). Each name
/// stands for every global shortcut that uses one of these key combinations; the code
/// [`META_ANY`] stands for every combination that includes the Windows/Meta key.
pub const GAME_MODE_COMBOS: [(&str, &str, &[i32]); 5] = [
    ("alt_tab", "Alt+Tab (switch window)", &[0x0900_0001, 0x0b00_0001]),
    ("alt_f4", "Alt+F4 (close window)", &[0x0900_0033]),
    ("alt_space", "Alt+Space / Alt+F2 (launcher)", &[0x0800_0020, 0x0900_0031]),
    ("meta", "Windows key (launcher, overview and Windows-key shortcuts)", &[META_ANY]),
    ("ctrl_esc", "Ctrl+Esc (task manager)", &[0x0500_0000]),
];

/// Stands for "any key combination with the Meta modifier (or Meta alone)" in [`GAME_MODE_COMBOS`].
pub const META_ANY: i32 = -1;

/// Whether the Qt key code `key` is one of the combinations named in `combos`.
pub fn combo_blocks(combos: &[String], key: i32) -> bool {
    GAME_MODE_COMBOS.iter().filter(|(n, _, _)| combos.iter().any(|c| c == n)).any(|(_, _, keys)| {
        keys.iter().any(|k| if *k == META_ANY { key & 0x1000_0000 != 0 || key == 0x0100_0022 } else { *k == key })
    })
}


fn yes() -> bool {
    true
}

/// Dynamic Razer-key codes the firmware reserves for software-defined keys. Only some keys
/// (dial press, Fn) report them in Normal mode, so new actions use shortcut slots instead.
pub const ACTION_CODES: std::ops::RangeInclusive<u8> = 128..=207;

/// Keys usable for shortcut slots. With the default XKB layouts F14–F18 arrive as
/// XF86Launch5–9. F13 (XF86Tools) is KDE System Settings' shortcut and F20–F23 are mic-mute /
/// touchpad toggles, so those are never used.
pub const SLOT_KEYS: [u8; 7] = [0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x73]; // F14–F19, F24
/// Modifier bits per slot group (HID: 0x01 Ctrl, 0x02 Shift, 0x04 Alt).
const SLOT_MODS: [u8; 7] = [0x00, 0x02, 0x04, 0x06, 0x01, 0x03, 0x05];
pub const ACTION_SLOTS: u8 = 46;

/// All shortcut slots in order: modifier groups × keys, skipping Ctrl+F24 (touchpad toggle).
pub(crate) fn slots() -> impl Iterator<Item = [u8; 2]> {
    SLOT_MODS.iter().flat_map(|&m| SLOT_KEYS.iter().map(move |&k| [m, k])).filter(|[m, k]| !(*k == 0x73 && m & 0x01 != 0))
}

/// Keyboard-function data `[modifier bits, HID usage]` for shortcut slot `slot`.
pub fn slot_keys(slot: u8) -> [u8; 2] {
    slots().nth(slot as usize).unwrap_or([0, 0x69])
}

/// A key bound to a software action. `code` < [`ACTION_SLOTS`]: the key sends shortcut slot
/// `code` (see [`slot_keys`]) and KDE's global-shortcut service reports it; `code` in
/// [`ACTION_CODES`]: the key sends `RazerKey [code]` (works for the dial press).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionConfig {
    pub code: u8,
    pub profile: u8,
    pub layer: u8,
    pub key: u8,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Run a command line (through `sh -c`).
    Launch { command: String },
    /// Type text on the PC (wtype / ydotool / xdotool) — for characters a macro can't press on
    /// the active keyboard layout, such as emoji.
    Type { text: String },
    /// Open a URL with the desktop's default handler.
    Url { url: String },
    /// Control a device — another one (Synapse's inter-device functions) or this one (Chroma
    /// functions on keys).
    Device { target: String, op: DeviceOp },
    /// Desktop audio (PipeWire via wpctl/pactl).
    Audio { op: AudioOp },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AudioOp {
    VolumeUp,
    VolumeDown,
    MuteToggle,
    MicMuteToggle,
    NextOutput,
    PreviousOutput,
    MicVolumeUp,
    MicVolumeDown,
    /// Make this output (PipeWire/Pulse sink name) the default.
    SetDefaultOutput { name: String, #[serde(default)] label: String },
    /// Make this input (source name) the default.
    SetDefaultInput { name: String, #[serde(default)] label: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DeviceOp {
    DpiStageUp,
    DpiStageDown,
    /// Hold for this DPI, release to restore.
    DpiClutch { dpi: u16 },
    ProfileNext,
    ProfilePrevious,
    Profile { id: u8 },
    BrightnessUp,
    BrightnessDown,
    LightingToggle,
    /// Step through the quick effects (spectrum, wave, breathing, starlight, reactive, static).
    EffectNext,
    EffectPrevious,
    /// Apply one effect.
    Effect { effect: EffectConfig },
    /// Brightness up / down on every attached device (use target `all`).
    BrightnessAllUp,
    BrightnessAllDown,
    /// Switch to a saved Lighting Studio profile / step through them (target is ignored).
    StudioProfile { name: String },
    StudioNext,
    StudioPrevious,
}

/// Target value of actions that act on every attached device.
pub const ALL_DEVICES: &str = "all";

pub fn effect_name(e: &EffectConfig) -> &'static str {
    match e {
        EffectConfig::Off => "off",
        EffectConfig::Static { .. } => "static",
        EffectConfig::Breathing { .. } => "breathing",
        EffectConfig::Spectrum => "spectrum",
        EffectConfig::Wave { .. } => "wave",
        EffectConfig::Reactive { .. } => "reactive",
        EffectConfig::Starlight { .. } => "starlight",
        EffectConfig::Ripple { .. } => "ripple",
        EffectConfig::Custom { .. } => "custom",
        EffectConfig::Studio { .. } => "Chroma Studio",
    }
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Action::Launch { command } => format!("Launch: {command}"),
            Action::Type { text } => format!("Type: {}", text.chars().take(24).collect::<String>()),
            Action::Url { url } => format!("Open: {url}"),
            Action::Device { target, op } => {
                let dev = match target.as_str() {
                    ALL_DEVICES => "All devices",
                    "00aa" => "Basilisk V3 Pro",
                    "028d" => "BlackWidow V4 Pro",
                    t => u16::from_str_radix(t, 16).ok().and_then(tsuba_hid::db::get).map_or(t, |s| s.name.as_str()),
                };
                let what = match op {
                    DeviceOp::DpiStageUp => "sensitivity stage up".to_string(),
                    DeviceOp::DpiStageDown => "sensitivity stage down".to_string(),
                    DeviceOp::DpiClutch { dpi } => format!("sensitivity clutch {dpi} DPI"),
                    DeviceOp::ProfileNext => "next profile".to_string(),
                    DeviceOp::ProfilePrevious => "previous profile".to_string(),
                    DeviceOp::Profile { id } => format!("profile {id}"),
                    DeviceOp::BrightnessUp => "brightness up".to_string(),
                    DeviceOp::BrightnessDown => "brightness down".to_string(),
                    DeviceOp::LightingToggle => "lighting on/off".to_string(),
                    DeviceOp::EffectNext => "next lighting effect".to_string(),
                    DeviceOp::EffectPrevious => "previous lighting effect".to_string(),
                    DeviceOp::Effect { effect } => format!("lighting {}", effect_name(effect)),
                    DeviceOp::BrightnessAllUp => "brightness up".to_string(),
                    DeviceOp::BrightnessAllDown => "brightness down".to_string(),
                    DeviceOp::StudioProfile { name } => format!("lighting profile “{name}”"),
                    DeviceOp::StudioNext => "next lighting profile".to_string(),
                    DeviceOp::StudioPrevious => "previous lighting profile".to_string(),
                };
                format!("{dev}: {what}")
            }
            Action::Audio { op } => match op {
                AudioOp::VolumeUp => "Volume up".into(),
                AudioOp::VolumeDown => "Volume down".into(),
                AudioOp::MuteToggle => "Mute".into(),
                AudioOp::MicMuteToggle => "Mic mute".into(),
                AudioOp::NextOutput => "Next audio output".into(),
                AudioOp::PreviousOutput => "Previous audio output".into(),
                AudioOp::MicVolumeUp => "Microphone volume up".into(),
                AudioOp::MicVolumeDown => "Microphone volume down".into(),
                AudioOp::SetDefaultOutput { name, label } => format!("Output: {}", if label.is_empty() { name } else { label }),
                AudioOp::SetDefaultInput { name, label } => format!("Input: {}", if label.is_empty() { name } else { label }),
            },
        }
    }
}

impl Config {
    /// Shortcut slot for a key: the one it already has, or the lowest slot unused on any device
    /// (slots are global — each is one key combination for the whole desktop).
    pub fn action_code(&self, pid: &str, profile: u8, layer: u8, key: u8) -> Option<u8> {
        self.action_code_excluding(pid, profile, layer, key, &[])
    }

    /// Like [`Self::action_code`], but never a slot in `reserved` (the slots macros use for
    /// their run-command events).
    pub fn action_code_excluding(&self, pid: &str, profile: u8, layer: u8, key: u8, reserved: &[u8]) -> Option<u8> {
        if let Some(a) = self.device(pid).and_then(|d| d.actions.iter().find(|a| a.profile == profile && a.layer == layer && a.key == key && a.code < ACTION_SLOTS)) {
            return Some(a.code);
        }
        (0..ACTION_SLOTS).find(|c| !reserved.contains(c) && !self.devices.iter().any(|d| d.actions.iter().any(|a| a.code == *c)))
    }
}

/// Lighting applied to every region of the device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lighting {
    pub effect: EffectConfig,
    /// Percent, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectConfig {
    Off,
    Static { colour: String },
    Breathing { #[serde(default)] colours: Vec<String> },
    Spectrum,
    Wave { #[serde(default = "one")] direction: u8, #[serde(default = "wave_speed")] speed: u8 },
    Reactive { #[serde(default = "two")] speed: u8, colour: String },
    Starlight { #[serde(default = "two")] speed: u8, #[serde(default)] colours: Vec<String> },
    Ripple { #[serde(default = "two")] speed: u8, #[serde(default)] colours: Vec<String> },
    /// Per-LED colours streamed as a custom frame: one string per grid row,
    /// space-separated RRGGBB values (missing = off).
    Custom { rows: Vec<String> },
    /// Chroma Studio layers, rendered continuously by tsubad.
    Studio { studio: crate::studio::Studio },
}

fn one() -> u8 {
    1
}
fn two() -> u8 {
    2
}
fn wave_speed() -> u8 {
    0x28
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DpiConfig {
    pub stages: Vec<[u16; 2]>,
    pub active_stage: u8,
    /// Active DPI; may differ from the active stage's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<[u16; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DialSlotConfig {
    pub function: String,
    pub colour: String,
    pub enabled: bool,
}

const HEADER: &str = "\
# Tsuba configuration. tsubad applies this to each device when it appears.
# Edit by hand, then reload:  busctl --user call io.github.JenAriDev.Tsuba \\
#   /io/github/JenAriDev/Tsuba io.github.JenAriDev.Tsuba1 Reload
# Effects: off | static {colour} | breathing {colours} | spectrum | wave {direction, speed}
#          | reactive {speed, colour} | starlight {speed, colours} | custom {rows} | studio {studio}.
#          Colours are RRGGBB.

";

impl Config {
    pub fn default_path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("config.toml")
    }

    /// Missing file = empty config.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Write atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, format!("{HEADER}{}", toml::to_string(self)?))?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn device(&self, pid: &str) -> Option<&DeviceConfig> {
        self.devices.iter().find(|d| d.pid == pid)
    }

    /// Insert or replace one device's section.
    pub fn upsert(&mut self, d: DeviceConfig) {
        match self.devices.iter_mut().find(|x| x.pid == d.pid) {
            Some(x) => *x = d,
            None => self.devices.push(d),
        }
    }
}

/// Build a config section that reproduces a device's current state.
pub fn from_state(s: &DeviceState) -> DeviceConfig {
    let lighting = s.lighting.first().map(|r| Lighting {
        effect: match r.effect.as_str() {
            "Off" => EffectConfig::Off,
            "Static" => EffectConfig::Static { colour: r.colours.first().cloned().unwrap_or_else(|| "ffffff".into()) },
            "Breathing" => EffectConfig::Breathing { colours: r.colours.clone() },
            "Wave" => EffectConfig::Wave { direction: 1, speed: wave_speed() },
            "Reactive" => EffectConfig::Reactive { speed: 2, colour: r.colours.first().cloned().unwrap_or_else(|| "ffffff".into()) },
            "Starlight" => EffectConfig::Starlight { speed: 2, colours: r.colours.clone() },
            "Ripple" => EffectConfig::Ripple { speed: 2, colours: r.colours.clone() },
            _ => EffectConfig::Spectrum,
        },
        brightness: Some(r.brightness),
    });
    DeviceConfig {
        pid: s.pid.clone(),
        name: Some(s.name.clone()),
        mode: None,
        lighting,
        dpi: s.dpi.as_ref().map(|d| DpiConfig { stages: d.stages.clone(), active_stage: d.active_stage, active: Some(d.active) }),
        dial: s.dial.as_ref().map(|slots| {
            slots.iter().map(|k| DialSlotConfig { function: k.function.clone(), colour: k.colour.clone(), enabled: k.enabled }).collect()
        }),
        actions: Vec::new(),
        lighting_power: None,
        game_mode: None,
        allow_untested_writes: false,
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    #[test]
    fn slots_map_to_f13_f24_with_modifiers() {
        assert_eq!(slots().count(), ACTION_SLOTS as usize);
        assert_eq!(slot_keys(0), [0x00, 0x69]);
        assert_eq!(slot_keys(6), [0x00, 0x73]);
        assert_eq!(slot_keys(7), [0x02, 0x69]);
        assert!(slots().all(|[_, k]| k != 0x68));
        // no Ctrl+F24 anywhere, never F20–F23
        assert!(slots().all(|[m, k]| !(k == 0x73 && m & 1 != 0) && !(0x6f..=0x72).contains(&k)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_actions_survive_the_toml_config() {
        let actions: Vec<Action> = vec![
            Action::Type { text: "hi 😀".into() },
            Action::Audio { op: AudioOp::MicVolumeUp },
            Action::Audio { op: AudioOp::SetDefaultOutput { name: "alsa_output.x".into(), label: "Speakers".into() } },
            Action::Audio { op: AudioOp::SetDefaultInput { name: "alsa_input.y".into(), label: String::new() } },
            Action::Device { target: ALL_DEVICES.into(), op: DeviceOp::BrightnessAllDown },
            Action::Device { target: ALL_DEVICES.into(), op: DeviceOp::StudioProfile { name: "Fire".into() } },
            Action::Device { target: ALL_DEVICES.into(), op: DeviceOp::StudioNext },
        ];
        let c = Config {
            devices: vec![DeviceConfig {
                pid: "028d".into(),
                actions: actions.iter().enumerate().map(|(i, a)| ActionConfig { code: i as u8, profile: 1, layer: 0, key: 160 + i as u8, action: a.clone() }).collect(),
                game_mode: Some(GameModeConfig { active: true, blocked_combos: Some(vec!["alt_tab".into(), "meta".into()]), ..Default::default() }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = toml::to_string(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), c, "{text}");
        for a in &actions {
            assert_eq!(&serde_json::from_str::<Action>(&serde_json::to_string(a).unwrap()).unwrap(), a);
        }
        // The old game-mode section (no combos) still reads.
        let old: GameModeConfig = toml::from_str("active = true\nblock_shortcuts = true\n").unwrap();
        assert_eq!(old.blocked_combos, None);
    }

    #[test]
    fn game_mode_combos_match_the_gui_list() {
        let qml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tsuba-gui/qml/GameModeCombos.qml")).unwrap();
        for (name, _, _) in GAME_MODE_COMBOS {
            assert!(qml.contains(&format!("[\"{name}\",")), "GameModeCombos.qml lacks {name}");
        }
        assert_eq!(qml.matches("[\"").count(), GAME_MODE_COMBOS.len(), "GameModeCombos.qml lists other combinations than the daemon knows");
        // Key codes: Alt+Tab, Meta alone and with letters, Ctrl+Esc.
        let n = |s: &str| vec![s.to_string()];
        assert!(combo_blocks(&n("alt_tab"), 0x0900_0001) && !combo_blocks(&n("alt_tab"), 0x1100_0001));
        assert!(combo_blocks(&n("meta"), 0x1100_0001) && combo_blocks(&n("meta"), 0x0100_0022) && !combo_blocks(&n("meta"), 0x0900_0033));
        assert!(!combo_blocks(&[], 0x0900_0001));
    }

    #[test]
    fn toml_roundtrip() {
        let c = Config {
            devices: vec![DeviceConfig {
                pid: "00aa".into(),
                name: Some("Basilisk V3 Pro".into()),
                mode: Some("normal".into()),
                lighting: Some(Lighting { effect: EffectConfig::Static { colour: "ff0000".into() }, brightness: Some(80) }),
                dpi: Some(DpiConfig { stages: vec![[800, 800], [1800, 1800]], active_stage: 2, active: None }),
                dial: None,
                actions: Vec::new(),
                lighting_power: None,
                game_mode: None,
                allow_untested_writes: true,
            }],
            ..Default::default()
        };
        let s = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    }

    #[test]
    fn game_lighting_flag_defaults_on_and_roundtrips() {
        let c: Config = toml::from_str("[[device]]\npid = \"0084\"\n").unwrap();
        assert!(c.game_lighting_enabled());
        assert!(!c.game_lighting_underglow_enabled());
        assert!(!c.game_lighting_web_enabled());
        let mut c = c;
        c.game_lighting = Some(false);
        let s = toml::to_string(&c).unwrap();
        assert!(s.starts_with("game_lighting = false"), "{s}");
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    }

    #[test]
    fn game_lighting_web_defaults_off_and_roundtrips() {
        let c: Config = toml::from_str("").unwrap();
        assert!(!c.game_lighting_web_enabled());
        assert!(!toml::to_string(&c).unwrap().contains("game_lighting_web"));
        let c: Config = toml::from_str("game_lighting_web = true\n").unwrap();
        assert!(c.game_lighting_web_enabled());
        let s = toml::to_string(&c).unwrap();
        assert!(s.contains("game_lighting_web = true"), "{s}");
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    }

    #[test]
    fn untested_writes_flag_defaults_off() {
        let c: Config = toml::from_str("[[device]]\npid = \"0084\"\n").unwrap();
        assert!(!c.devices[0].allow_untested_writes);
        let c: Config = toml::from_str("[[device]]\npid = \"0084\"\nallow_untested_writes = true\n").unwrap();
        assert!(c.devices[0].allow_untested_writes);
        // Off is not written out.
        let s = toml::to_string(&Config { devices: vec![DeviceConfig { pid: "0084".into(), ..Default::default() }], ..Default::default() }).unwrap();
        assert!(!s.contains("allow_untested"));
    }

    #[test]
    fn custom_rows_roundtrip() {
        let e = EffectConfig::Custom { rows: vec!["000000 ff0000".into(), "".into()] };
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, r#"{"type":"custom","rows":["000000 ff0000",""]}"#);
        assert_eq!(serde_json::from_str::<EffectConfig>(&s).unwrap(), e);
    }

    #[test]
    fn minimal_hand_written() {
        let c: Config = toml::from_str(
            r#"
            [[device]]
            pid = "028d"
            lighting = { effect = { type = "wave" } }
            "#,
        )
        .unwrap();
        assert_eq!(c.devices[0].lighting.as_ref().unwrap().effect, EffectConfig::Wave { direction: 1, speed: 0x28 });
    }
}
