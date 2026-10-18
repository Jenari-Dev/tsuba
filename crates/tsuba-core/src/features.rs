//! Device firmware features beyond the basics (key debounce, scroll acceleration level,
//! Smart-Reel threshold, profile LED colour, indicator LEDs, startup animation, lift-off /
//! Smart Tracking, wireless dimming), found per device by a read-only probe.
//!
//! The probe sends one Get per candidate command and records whether the firmware answers
//! (supported), refuses it (not supported) or fails. Only commands that answered are ever
//! written, and every write that has a Get counterpart is read back. Nothing in [`probe`] can
//! send a Set: it only takes commands from [`CANDIDATES`], each checked by [`is_read_only`]
//! (bit 7 set *and* every catalog name for the slot is a "Get …"), and the hid layer refuses
//! Sets on a device opened without write access anyway.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::{Device, HidError, Kind, Model};
use tsuba_proto::{commands as c, Command, ProtoError, Status};

/// One probe candidate: a feature key and the Get that tells whether the device has it.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub key: &'static str,
    pub cmd: Command,
    /// Leading argument bytes (the rest of the report is zero, as Synapse sends it).
    pub args: &'static [u8],
}

const P: u8 = 1; // profile-scoped reads probe on-board profile 1
const PROX: [u8; 2] = [c::PROXIMITY_CLASS, c::PROXIMITY_SENSOR];

/// Everything `tsuba probe-features` asks. Already-used commands are included so the report
/// is complete for a device; the feature keys below are what the GUI shows or hides.
pub const CANDIDATES: &[Candidate] = &[
    Candidate { key: "debounce", cmd: c::GET_DEBOUNCE, args: &[] },
    Candidate { key: "profile_debounce", cmd: c::GET_PROFILE_DEBOUNCE, args: &[P] },
    Candidate { key: "scroll_free_spin", cmd: c::GET_SCROLL_FREE_SPIN, args: &[P] },
    Candidate { key: "scroll_acceleration", cmd: c::GET_SCROLL_ACCELERATION, args: &[P] },
    Candidate { key: "smart_reel", cmd: c::GET_SCROLL_SMART_REEL, args: &[P] },
    Candidate { key: "scroll_accel_level", cmd: c::GET_SCROLL_ACCEL_LEVEL, args: &[P] },
    Candidate { key: "smart_reel_threshold", cmd: c::GET_SMART_REEL_THRESHOLD, args: &[P] },
    Candidate { key: "profile_colour", cmd: c::GET_PROFILE_COLOUR, args: &[P] },
    Candidate { key: "indicator_led", cmd: c::GET_INDICATOR_LED, args: &[] },
    Candidate { key: "indicator_leds", cmd: c::GET_INDICATOR_LEDS, args: &[] },
    Candidate { key: "startup_animation", cmd: c::GET_STARTUP_ANIMATION, args: &[0] },
    Candidate { key: "led_charging_effect", cmd: c::GET_LED_CHARGING_EFFECT, args: &[] },
    Candidate { key: "battery", cmd: c::GET_BATTERY_LEVEL, args: &[] },
    Candidate { key: "charging", cmd: c::GET_CHARGING_STATUS, args: &[] },
    Candidate { key: "wireless_connection", cmd: c::GET_WIRELESS_CONNECTION, args: &[] },
    Candidate { key: "dongle_quality", cmd: c::GET_DONGLE_QUALITY, args: &[] },
    Candidate { key: "smart_illumination", cmd: c::GET_SMART_ILLUMINATION, args: &[P] },
    Candidate { key: "time_to_dim", cmd: c::GET_TIME_TO_DIM, args: &[0] },
    Candidate { key: "proximity_sensors", cmd: c::GET_PROXIMITY_SENSORS, args: &[] },
    Candidate { key: "proximity_state", cmd: c::GET_PROXIMITY_STATE, args: &PROX },
    Candidate { key: "proximity_config", cmd: c::GET_PROXIMITY_CONFIG, args: &PROX },
    Candidate { key: "proximity_accel", cmd: c::GET_PROXIMITY_ACCEL_STATE, args: &[P] },
];

/// A command the probe may send: a Get (bit 7) whose every catalog name is a "Get …"
/// (some bit-7 ids are actions, e.g. "Keep Device On").
pub fn is_read_only(cmd: Command) -> bool {
    let mut names = c::names(cmd.class, cmd.id).peekable();
    cmd.is_get() && names.peek().is_some() && names.all(|n| n.starts_with("Get "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    /// The firmware answered with success.
    Supported,
    /// The firmware said "not supported".
    Unsupported,
    /// The firmware answered with failure (exists but not with these arguments, or busy).
    Failed,
    /// No usable answer (timeout, I/O error, mismatched reply).
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeEntry {
    pub key: String,
    /// Catalog name(s) of the command.
    pub command: String,
    /// `[data size, class, id]`.
    pub triplet: [u8; 3],
    pub args: Vec<u8>,
    pub status: ProbeStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reply: Vec<u8>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// Result of probing one device; saved as JSON so the daemon and GUI can show only the
/// controls the device answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub pid: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    pub entries: Vec<ProbeEntry>,
}

impl ProbeReport {
    pub fn entry(&self, key: &str) -> Option<&ProbeEntry> {
        self.entries.iter().find(|e| e.key == key)
    }

    pub fn supported(&self, key: &str) -> bool {
        self.entry(key).is_some_and(|e| e.status == ProbeStatus::Supported)
    }

    /// Keys the device answered.
    pub fn supported_keys(&self) -> Vec<String> {
        self.entries.iter().filter(|e| e.status == ProbeStatus::Supported).map(|e| e.key.clone()).collect()
    }

    /// Where the report for `pid` is kept (`$XDG_DATA_HOME/tsuba/features/<pid>.json`).
    pub fn default_path(pid: &str) -> PathBuf {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("features").join(format!("{pid}.json"))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// Plain-text table (CLI output).
    pub fn table(&self) -> String {
        let mut s = String::new();
        for e in &self.entries {
            let st = match e.status {
                ProbeStatus::Supported => "supported",
                ProbeStatus::Unsupported => "not supported",
                ProbeStatus::Failed => "failed",
                ProbeStatus::Error => "error",
            };
            let [n, cl, id] = e.triplet;
            let detail = if e.status == ProbeStatus::Supported { format!("{:02x?}", e.reply) } else { e.error.clone() };
            s.push_str(&format!("{:<22} [{n:>2}, 0x{cl:02x}, 0x{id:02x}] {:<14} {detail}   ({})\n", e.key, st, e.command));
        }
        s
    }
}

/// Send one Get from the candidate list. Panics (a programming error) for anything else.
fn get_only(dev: &Device, cmd: Command, args: &[u8]) -> Result<Vec<u8>, HidError> {
    assert!(is_read_only(cmd), "probe: 0x{:02x}/0x{:02x} is not a read-only command", cmd.class, cmd.id);
    dev.query(cmd, args)
}

/// Probe every candidate (read-only; see the module docs).
pub fn probe(dev: &Device, model: &Model) -> ProbeReport {
    let firmware = get_only(dev, c::GET_FIRMWARE_VERSION, &[]).ok().and_then(|a| tsuba_proto::decode::firmware_version(&a)).map(|f| format!("{}.{:02}", f.major, f.minor));
    let entries = CANDIDATES
        .iter()
        .map(|cand| {
            let (status, reply, error) = match get_only(dev, cand.cmd, cand.args) {
                Ok(r) => (ProbeStatus::Supported, r, String::new()),
                Err(HidError::Proto(ProtoError::Status(Status::Unsupported))) => (ProbeStatus::Unsupported, vec![], String::new()),
                Err(HidError::Proto(ProtoError::Status(s))) => (ProbeStatus::Failed, vec![], format!("{s:?}")),
                Err(e) => (ProbeStatus::Error, vec![], e.to_string()),
            };
            ProbeEntry {
                key: cand.key.into(),
                command: c::names(cand.cmd.class, cand.cmd.id).collect::<Vec<_>>().join(" | "),
                triplet: [cand.cmd.size, cand.cmd.class, cand.cmd.id],
                args: cand.args.to_vec(),
                status,
                reply,
                error,
            }
        })
        .collect();
    ProbeReport { pid: crate::pid_key(model.pid), name: model.name.to_string(), firmware, entries }
}

// ---- values ---------------------------------------------------------------------------------

/// Keyboard switch debounce, milliseconds (Synapse's "make / make mask / break / break mask").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debounce {
    pub make: u8,
    pub make_mask: u8,
    #[serde(rename = "break")]
    pub brk: u8,
    pub break_mask: u8,
}

impl Debounce {
    /// Synapse's default ("standard") timing.
    pub const STANDARD: Debounce = Debounce { make: 0, make_mask: 20, brk: 0, break_mask: 40 };
    /// No masking at all (Synapse's gaming timing).
    pub const FAST: Debounce = Debounce { make: 0, make_mask: 0, brk: 0, break_mask: 0 };

    fn bytes(self) -> [u8; 4] {
        [self.make, self.make_mask, self.brk, self.break_mask]
    }
    fn from(a: &[u8]) -> Option<Self> {
        Some(Debounce { make: *a.first()?, make_mask: *a.get(1)?, brk: *a.get(2)?, break_mask: *a.get(3)? })
    }
}

/// Dim the lighting after the device has been idle (wireless devices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeToDim {
    pub enabled: bool,
    pub seconds: u16,
    /// Brightness while dimmed, percent (Synapse sends 20).
    pub percent: u8,
}

/// Wireless: dim the lighting while the mouse moves ("smart illumination"). `mode` is kept as
/// the device reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartIllumination {
    pub on: bool,
    pub mode: u8,
}

/// Lift-off distance (Smart Tracking). Symmetric: one distance 1–3 mm for lift-off and
/// landing. Asymmetric: separate lift-off (2–3 mm) and landing (1–2 mm) distances, landing
/// below lift-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartTracking {
    #[serde(default)]
    pub asymmetric: bool,
    #[serde(default = "two")]
    pub distance_mm: u8,
    #[serde(default = "three")]
    pub lift_mm: u8,
    #[serde(default = "two")]
    pub landing_mm: u8,
}

fn two() -> u8 {
    2
}
fn three() -> u8 {
    3
}

impl Default for SmartTracking {
    fn default() -> Self {
        SmartTracking { asymmetric: false, distance_mm: 2, lift_mm: 3, landing_mm: 2 }
    }
}

impl SmartTracking {
    /// `(lift mode, lift height)` for Set Proximity Sensor Lift Setting. Modes: 1 = universal
    /// lift cut-off (symmetric), 2 = universal hysteresis (asymmetric).
    pub fn encode(&self) -> Result<(u8, u8)> {
        if self.asymmetric {
            let h = match (self.landing_mm, self.lift_mm) {
                (1, 2) => 0,
                (1, 3) => 1,
                (2, 3) => 2,
                (l, o) => bail!("landing {l} mm / lift-off {o} mm: landing must be 1–2 mm and below lift-off (2–3 mm)"),
            };
            Ok((2, h))
        } else {
            ensure!((1..=3).contains(&self.distance_mm), "lift-off distance must be 1–3 mm");
            Ok((1, self.distance_mm - 1))
        }
    }
}

/// Current values of the features a device answered, for one on-board profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Features {
    pub profile: u8,
    /// Probe keys the device answered.
    pub supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce: Option<Debounce>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_accel_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_reel_threshold: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_colour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicator_led: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicator_leds: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_animation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_illumination: Option<SmartIllumination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_to_dim: Option<TimeToDim>,
    /// The lift-off sensor answers (Smart Tracking can be set; the device has no read-back).
    #[serde(default)]
    pub lift_sensor: bool,
    /// "wired" | "wireless" (from the device when it says, else the database).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dongle_quality: Option<u8>,
    /// On-board profiles exist (Set Profile Reset applies).
    #[serde(default)]
    pub can_reset_profile: bool,
    /// Model name from the device database and where to look for firmware updates.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub support_url: String,
}

/// Razer's support site (firmware updates are only published there, for Windows).
pub const SUPPORT_URL: &str = "https://mysupport.razer.com/app/answers/list/c/firmware";

fn second(dev: &Device, cmd: Command, args: &[u8]) -> Option<u8> {
    dev.query(cmd, args).ok().and_then(|a| a.get(1).copied())
}

/// Read the current value of every feature `report` says the device has (profile-scoped ones
/// for `profile`). Only Gets.
pub fn read(dev: &Device, model: &Model, report: &ProbeReport, profile: u8) -> Features {
    let has = |k: &str| report.supported(k);
    let mut f = Features { profile, supported: report.supported_keys(), model: model.name.to_string(), support_url: SUPPORT_URL.into(), ..Default::default() };
    if has("profile_debounce") {
        f.debounce = dev.query(c::GET_PROFILE_DEBOUNCE, &[profile]).ok().and_then(|a| Debounce::from(a.get(1..)?));
    }
    if f.debounce.is_none() && has("debounce") {
        f.debounce = dev.query(c::GET_DEBOUNCE, &[]).ok().and_then(|a| Debounce::from(&a));
    }
    if has("scroll_accel_level") {
        f.scroll_accel_level = second(dev, c::GET_SCROLL_ACCEL_LEVEL, &[profile]);
    }
    if has("smart_reel_threshold") {
        f.smart_reel_threshold = second(dev, c::GET_SMART_REEL_THRESHOLD, &[profile]);
    }
    if has("profile_colour") {
        f.profile_colour = dev.query(c::GET_PROFILE_COLOUR, &[profile]).ok().and_then(|a| Some(crate::hex_rgb(a.get(1..4)?.try_into().ok()?)));
    }
    if has("indicator_led") {
        f.indicator_led = dev.query(c::GET_INDICATOR_LED, &[]).ok().and_then(|a| a.first().map(|v| *v != 0));
    }
    if has("indicator_leds") {
        f.indicator_leds = dev.query(c::GET_INDICATOR_LEDS, &[]).ok();
    }
    if has("startup_animation") {
        f.startup_animation = dev.query(c::GET_STARTUP_ANIMATION, &[0]).ok().and_then(|a| startup_disabled(&a)).map(|d| !d);
    }
    if has("smart_illumination") {
        f.smart_illumination = dev.query(c::GET_SMART_ILLUMINATION, &[profile]).ok().and_then(|a| Some(SmartIllumination { on: *a.get(1)? != 0, mode: *a.get(2)? }));
    }
    if has("time_to_dim") {
        f.time_to_dim = dev.query(c::GET_TIME_TO_DIM, &[0]).ok().and_then(|a| decode_time_to_dim(&a));
    }
    f.lift_sensor = has("proximity_state") && model.kind == Kind::Mouse;
    let wireless = if has("wireless_connection") { dev.query(c::GET_WIRELESS_CONNECTION, &[]).ok().and_then(|a| a.first().copied()) } else { None };
    f.connection = match (wireless, model.spec.connection.as_deref()) {
        (Some(v), _) if model.spec.features.wireless => Some(if v != 0 { "wireless" } else { "wired" }.into()),
        (_, Some(c)) => Some(if c == "receiver" { "wireless" } else { c }.into()),
        _ => None,
    };
    if has("dongle_quality") {
        f.dongle_quality = dev.query(c::GET_DONGLE_QUALITY, &[]).ok().and_then(|a| a.first().copied());
    }
    f.can_reset_profile = (model.kind.has_buttons() || model.kind.has_keys()) && dev.query(tsuba_proto::profiles::GET_COUNT, &[]).is_ok();
    f
}

/// Get Startup Animation reply: `[disabled]` or `[profile, disabled]`.
fn startup_disabled(a: &[u8]) -> Option<bool> {
    match a {
        [d] => Some(*d != 0),
        [_, d, ..] => Some(*d != 0),
        _ => None,
    }
}

fn decode_time_to_dim(a: &[u8]) -> Option<TimeToDim> {
    Some(TimeToDim { enabled: *a.get(1)? != 0, seconds: u16::from_be_bytes([*a.get(2)?, *a.get(3)?]), percent: *a.get(4)? })
}

// ---- writes ---------------------------------------------------------------------------------

/// Settings to change (absent = leave). Same names as [`Features`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Change {
    /// On-board profile for profile-scoped settings (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce: Option<Debounce>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_accel_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_reel_threshold: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_colour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicator_led: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicator_leds: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_animation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_illumination: Option<SmartIllumination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_to_dim: Option<TimeToDim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_tracking: Option<SmartTracking>,
}

/// Levels Tsuba lets the scroll-acceleration / Smart-Reel sliders reach. The firmware's range
/// is not documented; the device's own value is always accepted too.
pub const LEVEL_MAX: u8 = 10;

/// Apply every field of `want` the device supports (per `report`); refuses the others.
/// Returns one line per change. Writes with a Get counterpart are read back.
pub fn apply(dev: &Device, model: &Model, report: &ProbeReport, want: &Change) -> Result<Vec<String>> {
    let profile = want.profile.unwrap_or(1);
    let need = |key: &str, what: &str| -> Result<()> {
        ensure!(report.supported(key), "{} does not support {what} (the read-only probe got no answer)", model.name);
        Ok(())
    };
    let mut log = Vec::new();
    if let Some(d) = want.debounce {
        ensure!(report.supported("debounce") || report.supported("profile_debounce"), "{} has no key debounce setting", model.name);
        ensure!(d.bytes().iter().all(|v| *v <= 100), "debounce times are at most 100 ms");
        if report.supported("debounce") {
            dev.query(c::SET_DEBOUNCE, &d.bytes())?;
            ensure!(dev.query(c::GET_DEBOUNCE, &[]).ok().and_then(|a| Debounce::from(&a)) == Some(d), "device kept its old debounce times");
        }
        if report.supported("profile_debounce") {
            let mut v = vec![profile];
            v.extend(d.bytes());
            dev.query(c::SET_PROFILE_DEBOUNCE, &v)?;
            ensure!(dev.query(c::GET_PROFILE_DEBOUNCE, &[profile]).ok().and_then(|a| Debounce::from(a.get(1..)?)) == Some(d), "device kept the profile's old debounce times");
        }
        log.push(format!("key debounce {}/{}/{}/{} ms", d.make, d.make_mask, d.brk, d.break_mask));
    }
    for (key, value, set, get, what) in [
        ("scroll_accel_level", want.scroll_accel_level, c::SET_SCROLL_ACCEL_LEVEL, c::GET_SCROLL_ACCEL_LEVEL, "scroll acceleration level"),
        ("smart_reel_threshold", want.smart_reel_threshold, c::SET_SMART_REEL_THRESHOLD, c::GET_SMART_REEL_THRESHOLD, "auto free-spin threshold"),
    ] {
        let Some(v) = value else { continue };
        need(key, what)?;
        let current = second(dev, get, &[profile]);
        ensure!(v <= LEVEL_MAX || current == Some(v), "{what} must be 0–{LEVEL_MAX}");
        dev.query(set, &[profile, v])?;
        ensure!(second(dev, get, &[profile]) == Some(v), "device kept its old {what}");
        log.push(format!("{what} {v}"));
    }
    if let Some(hex) = &want.profile_colour {
        need("profile_colour", "a profile colour")?;
        let [r, g, b] = crate::parse_rgb(hex)?;
        dev.query(c::SET_PROFILE_COLOUR, &[profile, r, g, b])?;
        let back = dev.query(c::GET_PROFILE_COLOUR, &[profile])?;
        ensure!(back.get(1..4) == Some(&[r, g, b][..]), "device kept the old profile colour");
        log.push(format!("profile {profile} colour #{}", crate::hex_rgb([r, g, b])));
    }
    if let Some(on) = want.indicator_led {
        need("indicator_led", "indicator LED settings")?;
        dev.query(c::SET_INDICATOR_LED, &[on as u8])?;
        ensure!(dev.query(c::GET_INDICATOR_LED, &[])?.first().map(|v| *v != 0) == Some(on), "device kept its old indicator LED setting");
        log.push(format!("indicator LEDs {}", if on { "on" } else { "off" }));
    }
    if let Some(states) = &want.indicator_leds {
        need("indicator_leds", "per-indicator LED settings")?;
        let now = dev.query(c::GET_INDICATOR_LEDS, &[])?;
        ensure!(states.len() == now.len(), "the device has {} indicators", now.len());
        dev.query(c::SET_INDICATOR_LEDS, states)?;
        ensure!(&dev.query(c::GET_INDICATOR_LEDS, &[])? == states, "device kept its old indicator states");
        log.push(format!("indicators {states:?}"));
    }
    if let Some(on) = want.startup_animation {
        need("startup_animation", "a startup animation setting")?;
        dev.query(c::SET_STARTUP_ANIMATION, &[0, !on as u8])?;
        ensure!(dev.query(c::GET_STARTUP_ANIMATION, &[0]).ok().and_then(|a| startup_disabled(&a)) == Some(!on), "device kept its old startup animation setting");
        log.push(format!("startup animation {}", if on { "on" } else { "off" }));
    }
    if let Some(s) = want.smart_illumination {
        need("smart_illumination", "smart dimming")?;
        dev.query(c::SET_SMART_ILLUMINATION, &[profile, s.on as u8, s.mode])?;
        let back = dev.query(c::GET_SMART_ILLUMINATION, &[profile])?;
        ensure!(back.get(1).map(|v| *v != 0) == Some(s.on), "device kept its old smart dimming setting");
        log.push(format!("smart dimming {}", if s.on { "on" } else { "off" }));
    }
    if let Some(t) = want.time_to_dim {
        need("time_to_dim", "dimming when idle")?;
        ensure!((60..=900).contains(&t.seconds), "dim after 1–15 minutes");
        ensure!(t.percent <= 100, "dimmed brightness is a percentage");
        let [hi, lo] = t.seconds.to_be_bytes();
        dev.query(c::SET_TIME_TO_DIM, &[0, t.enabled as u8, hi, lo, t.percent])?;
        let back = dev.query(c::GET_TIME_TO_DIM, &[0]).ok().and_then(|a| decode_time_to_dim(&a));
        ensure!(back.is_some_and(|b| b.enabled == t.enabled && b.seconds == t.seconds), "device kept its old dimming settings");
        log.push(if t.enabled { format!("dim lighting after {} min idle", t.seconds / 60) } else { "dim when idle off".into() });
    }
    if let Some(st) = want.smart_tracking {
        ensure!(model.kind == Kind::Mouse, "lift-off distance applies to mice");
        need("proximity_state", "lift-off distance")?;
        let (mode, height) = st.encode()?;
        // Synapse's sequence for this sensor: surface calibration off, then the lift setting.
        dev.query(c::SET_PROXIMITY_STATE, &[c::PROXIMITY_CLASS, c::PROXIMITY_SENSOR, 0])?;
        dev.query(c::SET_PROXIMITY_LIFT, &[c::PROXIMITY_CLASS, c::PROXIMITY_SENSOR, mode, height])?;
        log.push(if st.asymmetric {
            format!("lift-off {} mm / landing {} mm (sent; the device has no read-back)", st.lift_mm, st.landing_mm)
        } else {
            format!("lift-off distance {} mm (sent; the device has no read-back)", st.distance_mm)
        });
    }
    if log.is_empty() {
        log.push("nothing to change".into());
    }
    Ok(log)
}

/// Restore on-board profile `slot` to its factory defaults (Set Profile Reset). The slot must
/// exist. Everything Tsuba cached about the device is stale afterwards.
pub fn reset_profile(dev: &Device, model: &Model, slot: u8) -> Result<String> {
    ensure!(model.kind.has_buttons() || model.kind.has_keys(), "{} has no on-board profiles", model.name);
    let ids = crate::profiles::list(dev).context("reading the on-board profiles")?;
    ensure!(ids.profiles.iter().any(|p| p.id == slot), "on-board profile {slot} does not exist");
    dev.query(c::SET_PROFILE_RESET, &[slot])?;
    dev.counters().touch_all();
    Ok(format!("on-board profile {slot} reset to factory defaults"))
}

/// Factory reset (Set Device Reset, mode 1): erases on-board profiles and macros.
pub fn factory_reset(dev: &Device, model: &Model) -> Result<String> {
    ensure!(model.kind.has_buttons() || model.kind.has_keys(), "{} has no on-board memory to reset", model.name);
    dev.query(c::SET_DEVICE_RESET, &[1])?;
    dev.counters().touch_all();
    Ok(format!("{} reset to factory settings", model.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim;
    use std::sync::{Arc, Mutex};

    #[test]
    fn candidates_are_read_only_catalog_gets() {
        for cand in CANDIDATES {
            assert!(is_read_only(cand.cmd), "{} is not a Get", cand.key);
            let entry = tsuba_proto::commands::TABLE.iter().find(|e| e.0 == cand.cmd.class && e.1 == cand.cmd.id && e.2 == cand.cmd.size);
            assert!(entry.is_some(), "{}: [{}, {:#04x}, {:#04x}] is not in the catalog", cand.key, cand.cmd.size, cand.cmd.class, cand.cmd.id);
        }
        // Bit-7 actions and Sets are refused.
        assert!(!is_read_only(Command::new(0, 0x07, 0xa1)), "Keep Device On");
        assert!(!is_read_only(c::SET_DEVICE_RESET));
        assert!(!is_read_only(c::SET_PROFILE_RESET));
        assert!(!is_read_only(Command::new(1, 0x42, 0x99)), "unknown commands");
    }

    #[test]
    fn set_constants_match_the_catalog() {
        for cmd in [
            c::SET_DEVICE_RESET, c::SET_DEBOUNCE, c::SET_PROFILE_DEBOUNCE, c::SET_SCROLL_ACCEL_LEVEL, c::SET_SMART_REEL_THRESHOLD,
            c::SET_PROFILE_RESET, c::SET_PROFILE_COLOUR, c::SET_INDICATOR_LED, c::SET_INDICATOR_LEDS, c::SET_SMART_ILLUMINATION,
            c::SET_TIME_TO_DIM, c::SET_PROXIMITY_STATE, c::SET_PROXIMITY_LIFT, c::SET_STARTUP_ANIMATION,
        ] {
            assert!(!cmd.is_get());
            assert!(tsuba_proto::commands::TABLE.iter().any(|e| (e.0, e.1, e.2) == (cmd.class, cmd.id, cmd.size)), "{cmd:?}");
        }
    }

    fn rig(pid: &str) -> (sim::Sim, Model) {
        let s = Arc::new(Mutex::new(sim::new_state(pid, 2)));
        (s, tsuba_hid::model(u16::from_str_radix(pid, 16).unwrap()).unwrap())
    }

    #[test]
    fn probe_sends_no_set_even_with_writes_enabled() {
        for pid in ["028d", "00aa"] {
            let (s, m) = rig(pid);
            let dev = sim::device(&s);
            let r = probe(&dev, &m);
            assert_eq!(r.entries.len(), CANDIDATES.len());
            assert!(s.lock().unwrap().writes.is_empty(), "{pid}: probe wrote {:?}", s.lock().unwrap().writes);
        }
    }

    #[test]
    fn probe_classifies_answers() {
        let (s, m) = rig("028d");
        let r = probe(&sim::device(&s), &m);
        assert!(r.supported("debounce") && r.supported("startup_animation") && r.supported("indicator_led"));
        assert!(!r.supported("proximity_state") && !r.supported("scroll_accel_level"));
        assert_eq!(r.entry("smart_reel").unwrap().status, ProbeStatus::Unsupported);
        let (s, m) = rig("00aa");
        let r = probe(&sim::device(&s), &m);
        assert!(r.supported("proximity_state") && r.supported("scroll_accel_level") && r.supported("smart_reel_threshold"));
        assert!(!r.supported("debounce"));
        // Round-trips through JSON (the saved file).
        let back: ProbeReport = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(r.table().contains("scroll_accel_level"));
    }

    #[test]
    fn read_and_apply_keyboard_features() {
        let (s, m) = rig("028d");
        let dev = sim::device(&s);
        let r = probe(&dev, &m);
        let f = read(&dev, &m, &r, 1);
        assert_eq!(f.debounce, Some(Debounce::STANDARD));
        assert_eq!((f.startup_animation, f.indicator_led), (Some(true), Some(true)));
        assert!(f.can_reset_profile && !f.lift_sensor);
        let log = apply(&dev, &m, &r, &Change { debounce: Some(Debounce::FAST), startup_animation: Some(false), indicator_led: Some(false), ..Default::default() }).unwrap();
        assert_eq!(log.len(), 3, "{log:?}");
        let f = read(&dev, &m, &r, 1);
        assert_eq!((f.debounce, f.startup_animation, f.indicator_led), (Some(Debounce::FAST), Some(false), Some(false)));
        // Both the live and the profile copy were written.
        let w = s.lock().unwrap().writes.clone();
        assert!(w.contains(&(0x02, 0x02)) && w.contains(&(0x02, 0x15)) && w.contains(&(0x0f, 0x18)) && w.contains(&(0x07, 0x10)));
        // Unsupported features are refused before anything is sent.
        let n = s.lock().unwrap().writes.len();
        assert!(apply(&dev, &m, &r, &Change { smart_tracking: Some(SmartTracking::default()), ..Default::default() }).is_err());
        assert!(apply(&dev, &m, &r, &Change { scroll_accel_level: Some(3), ..Default::default() }).is_err());
        assert_eq!(s.lock().unwrap().writes.len(), n);
    }

    #[test]
    fn read_and_apply_mouse_features() {
        let (s, m) = rig("00aa");
        let dev = sim::device(&s);
        let r = probe(&dev, &m);
        let f = read(&dev, &m, &r, 1);
        assert!(f.lift_sensor);
        assert_eq!(f.connection.as_deref(), Some("wired"));
        assert_eq!(f.profile_colour.as_deref(), Some("00ff00"));
        let log = apply(
            &dev,
            &m,
            &r,
            &Change { profile: Some(2), scroll_accel_level: Some(4), smart_reel_threshold: Some(2), profile_colour: Some("ff0000".into()), smart_tracking: Some(SmartTracking { asymmetric: true, lift_mm: 3, landing_mm: 1, distance_mm: 2 }), ..Default::default() },
        )
        .unwrap();
        assert_eq!(log.len(), 4, "{log:?}");
        let f2 = read(&dev, &m, &r, 2);
        assert_eq!((f2.scroll_accel_level, f2.smart_reel_threshold, f2.profile_colour.as_deref()), (Some(4), Some(2), Some("ff0000")));
        // Profile 1 is untouched.
        assert_eq!(read(&dev, &m, &r, 1).scroll_accel_level, f.scroll_accel_level);
        assert_eq!(s.lock().unwrap().feat.lift, Some((2, 1)));
        assert!(apply(&dev, &m, &r, &Change { scroll_accel_level: Some(200), ..Default::default() }).is_err());
        assert!(apply(&dev, &m, &r, &Change { smart_tracking: Some(SmartTracking { asymmetric: true, lift_mm: 2, landing_mm: 2, distance_mm: 1 }), ..Default::default() }).is_err());
    }

    #[test]
    fn smart_tracking_encoding() {
        let sym = |d| SmartTracking { distance_mm: d, ..Default::default() }.encode();
        assert_eq!(sym(1).unwrap(), (1, 0));
        assert_eq!(sym(3).unwrap(), (1, 2));
        assert!(sym(4).is_err() && sym(0).is_err());
        let asym = |l, o| SmartTracking { asymmetric: true, landing_mm: l, lift_mm: o, distance_mm: 2 }.encode();
        assert_eq!(asym(1, 2).unwrap(), (2, 0));
        assert_eq!(asym(1, 3).unwrap(), (2, 1));
        assert_eq!(asym(2, 3).unwrap(), (2, 2));
        assert!(asym(3, 3).is_err() && asym(2, 2).is_err());
    }

    #[test]
    fn resets_need_confirmable_targets() {
        let (s, m) = rig("00aa");
        let dev = sim::device(&s);
        assert!(reset_profile(&dev, &m, 4).is_err(), "slot 4 does not exist");
        let g0 = dev.counters().get(tsuba_hid::Domain::Macros);
        reset_profile(&dev, &m, 2).unwrap();
        assert_eq!(s.lock().unwrap().feat.resets, vec![2]);
        assert!(dev.counters().get(tsuba_hid::Domain::Macros) > g0, "every cache is stale after a reset");
        factory_reset(&dev, &m).unwrap();
        assert_eq!(s.lock().unwrap().profiles.len(), 1, "factory reset leaves profile 1");
        // A read-only device can't be reset.
        let mut ro = sim::device(&s);
        ro.allow_writes(false);
        assert!(factory_reset(&ro, &m).is_err());
    }

    #[test]
    fn report_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tsuba-probe-{}", std::process::id()));
        let path = dir.join("00aa.json");
        let (s, m) = rig("00aa");
        let r = probe(&sim::device(&s), &m);
        r.save(&path).unwrap();
        assert_eq!(ProbeReport::load(&path).unwrap(), r);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
