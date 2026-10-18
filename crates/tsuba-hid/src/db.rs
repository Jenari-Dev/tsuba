//! The device database (`data/devicedb/*.toml`), embedded at compile time.
//!
//! One file per family. Each file has a `[defaults]` table (control interface, event
//! interface, transaction id) and a `[[device]]` array; a device that leaves one of those
//! fields out takes the family default. Regenerate with `tools/import-openrazer.py`.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// (family name, file contents).
pub const FILES: &[(&str, &str)] = &[
    ("keyboards", include_str!("../../../data/devicedb/keyboards.toml")),
    ("keypads", include_str!("../../../data/devicedb/keypads.toml")),
    ("mice", include_str!("../../../data/devicedb/mice.toml")),
    ("mousemats", include_str!("../../../data/devicedb/mousemats.toml")),
    ("accessories", include_str!("../../../data/devicedb/accessories.toml")),
    ("headsets", include_str!("../../../data/devicedb/headsets.toml")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Keyboard,
    LaptopKeyboard,
    Keypad,
    Mouse,
    /// A wireless receiver/dongle with no input device of its own in the database entry.
    Receiver,
    Mousemat,
    Mousedock,
    HeadsetStand,
    Headset,
    Speaker,
    Egpu,
    Monitor,
    Accessory,
}

impl Kind {
    /// snake_case name, as used in the database and over D-Bus.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Keyboard => "keyboard",
            Kind::LaptopKeyboard => "laptop_keyboard",
            Kind::Keypad => "keypad",
            Kind::Mouse => "mouse",
            Kind::Receiver => "receiver",
            Kind::Mousemat => "mousemat",
            Kind::Mousedock => "mousedock",
            Kind::HeadsetStand => "headset_stand",
            Kind::Headset => "headset",
            Kind::Speaker => "speaker",
            Kind::Egpu => "egpu",
            Kind::Monitor => "monitor",
            Kind::Accessory => "accessory",
        }
    }

    /// Devices whose keys are addressed by keyboard positions (key-assignment commands).
    pub fn has_keys(self) -> bool {
        matches!(self, Kind::Keyboard | Kind::LaptopKeyboard | Kind::Keypad)
    }

    /// Devices whose buttons are addressed by button ids (button-assignment commands).
    pub fn has_buttons(self) -> bool {
        self == Kind::Mouse
    }
}

/// Optional capabilities. Absent = false / 0.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    pub dpi_stages: bool,
    pub battery: bool,
    pub wireless: bool,
    pub idle_time: bool,
    pub charging_lights: bool,
    pub scroll_modes: bool,
    pub scroll_acceleration: bool,
    pub smart_reel: bool,
    pub dial: bool,
    /// Number of dedicated macro keys when known (verified devices).
    pub macro_keys: u8,
    pub dedicated_macro_keys: bool,
    pub onboard_macros: bool,
    pub game_mode: bool,
    pub keyswitch_optical: bool,
}

/// One database entry with family defaults applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spec {
    pub pid: u16,
    pub family: &'static str,
    pub name: String,
    pub kind: Kind,
    /// Drawing hint: full, tkl, 75, 65, 60, keypad, laptop, mouse-right, mouse-ambi,
    /// mousemat-strip, mousedock, headset-stand, headset, accessory.
    pub layout: String,
    /// Verified with Tsuba on real hardware.
    pub tested: bool,
    /// False for devices Tsuba knows of but cannot drive (headsets).
    pub supported: bool,
    pub txid: u8,
    pub iface: u8,
    pub event_iface: u8,
    /// Lighting command family: "extended" (class 0x0f matrix) or "standard" (older class 0x03).
    pub protocol: Option<String>,
    /// wired | wireless | receiver | bluetooth.
    pub connection: Option<String>,
    /// The same device over its other connection (wired ↔ wireless/receiver).
    pub pair: Option<u16>,
    /// Custom-frame grid [rows, cols].
    pub matrix: Option<[u8; 2]>,
    pub leds: Vec<String>,
    pub effects: Vec<String>,
    pub max_dpi: Option<u32>,
    pub polling_rates: Vec<u32>,
    pub features: Features,
    pub notes: Option<String>,
}

impl Spec {
    pub fn has_effect(&self, name: &str) -> bool {
        self.effects.iter().any(|e| e == name)
    }

    /// Per-LED (custom frame) lighting with more than one LED.
    pub fn per_led(&self) -> bool {
        self.matrix.is_some_and(|[r, c]| r as u32 * c as u32 > 1)
    }
}

#[derive(Deserialize)]
struct Defaults {
    iface: u8,
    event_iface: u8,
    txid: u8,
}

#[derive(Deserialize)]
struct Raw {
    pid: String,
    name: String,
    kind: Kind,
    layout: Option<String>,
    #[serde(default)]
    tested: bool,
    supported: Option<bool>,
    txid: Option<u8>,
    iface: Option<u8>,
    event_iface: Option<u8>,
    protocol: Option<String>,
    connection: Option<String>,
    pair: Option<String>,
    matrix: Option<[u8; 2]>,
    #[serde(default)]
    leds: Vec<String>,
    #[serde(default)]
    effects: Vec<String>,
    max_dpi: Option<u32>,
    #[serde(default)]
    polling_rates: Vec<u32>,
    #[serde(flatten)]
    features: Features,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct FileDoc {
    defaults: Defaults,
    #[serde(default)]
    device: Vec<Raw>,
}

fn hex_pid(s: &str) -> Result<u16, String> {
    u16::from_str_radix(s, 16).map_err(|_| format!("bad pid {s:?}"))
}

/// Parse one family file.
pub fn parse(family: &'static str, text: &str) -> Result<Vec<Spec>, String> {
    let doc: FileDoc = toml::from_str(text).map_err(|e| format!("{family}: {e}"))?;
    let d = &doc.defaults;
    doc.device
        .into_iter()
        .map(|r| {
            Ok(Spec {
                pid: hex_pid(&r.pid)?,
                family,
                layout: r.layout.unwrap_or_else(|| "accessory".into()),
                tested: r.tested,
                supported: r.supported.unwrap_or(true),
                txid: r.txid.unwrap_or(d.txid),
                iface: r.iface.unwrap_or(d.iface),
                event_iface: r.event_iface.unwrap_or(d.event_iface),
                pair: r.pair.as_deref().map(hex_pid).transpose()?,
                name: r.name,
                kind: r.kind,
                protocol: r.protocol,
                connection: r.connection,
                matrix: r.matrix,
                leds: r.leds,
                effects: r.effects,
                max_dpi: r.max_dpi,
                polling_rates: r.polling_rates,
                features: r.features,
                notes: r.notes,
            })
        })
        .collect()
}

/// Every database entry, sorted by pid. Panics if the embedded database is malformed
/// (caught by the tests).
pub fn all() -> &'static [Spec] {
    static DB: OnceLock<Vec<Spec>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut v: Vec<Spec> = FILES.iter().flat_map(|(f, t)| parse(f, t).expect("embedded device database")).collect();
        v.sort_by_key(|s| s.pid);
        v
    })
}

/// The entry for `pid`, if the database knows it.
pub fn get(pid: u16) -> Option<&'static Spec> {
    let db = all();
    db.binary_search_by_key(&pid, |s| s.pid).ok().map(|i| &db[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_parses_and_is_large() {
        let db = all();
        assert!(db.len() > 250, "only {} devices", db.len());
        for k in [Kind::Keyboard, Kind::Mouse, Kind::Keypad, Kind::Mousemat, Kind::Mousedock, Kind::HeadsetStand, Kind::Headset, Kind::LaptopKeyboard] {
            assert!(db.iter().any(|s| s.kind == k), "no {k:?}");
        }
    }

    #[test]
    fn no_duplicate_pids() {
        let mut seen = std::collections::BTreeSet::new();
        for (f, t) in FILES {
            for s in parse(f, t).unwrap() {
                assert!(seen.insert(s.pid), "duplicate pid {:04x}", s.pid);
            }
        }
    }

    #[test]
    fn verified_entries_unchanged() {
        let kb = get(0x028d).unwrap();
        assert_eq!((kb.name.as_str(), kb.kind, kb.iface, kb.event_iface, kb.txid), ("BlackWidow V4 Pro", Kind::Keyboard, 3, 1, 0x1f));
        assert!(kb.tested && kb.supported && kb.features.dial);
        assert_eq!(kb.matrix, Some([9, 24]));
        let m = get(0x00aa).unwrap();
        assert_eq!((m.name.as_str(), m.kind, m.iface, m.event_iface, m.txid), ("Basilisk V3 Pro (wired)", Kind::Mouse, 0, 1, 0x1f));
        assert!(m.tested && m.features.dpi_stages && !m.features.dial);
        // Only these two are verified.
        assert_eq!(all().iter().filter(|s| s.tested).count(), 2);
    }

    #[test]
    fn family_defaults_applied() {
        let text = "[defaults]\niface = 2\nevent_iface = 4\ntxid = 0x3f\n\n[[device]]\npid = \"1234\"\nname = \"X\"\nkind = \"mouse\"\n\n[[device]]\npid = \"1235\"\nname = \"Y\"\nkind = \"keyboard\"\niface = 3\ntxid = 0x1f\nbattery = true\n";
        let v = parse("test", text).unwrap();
        assert_eq!((v[0].iface, v[0].event_iface, v[0].txid, v[0].tested, v[0].supported), (2, 4, 0x3f, false, true));
        assert_eq!((v[1].iface, v[1].event_iface, v[1].txid), (3, 4, 0x1f));
        assert!(v[1].features.battery && !v[0].features.battery);
        // Real data: an old keyboard with no explicit interface takes the keyboard default.
        let old = get(0x0203).unwrap();
        assert_eq!((old.iface, old.family), (1, "keyboards"));
    }

    #[test]
    fn headsets_are_listed_but_unsupported() {
        let h: Vec<_> = all().iter().filter(|s| s.kind == Kind::Headset).collect();
        assert!(!h.is_empty());
        assert!(h.iter().all(|s| !s.supported && s.notes.is_some()));
    }
}
