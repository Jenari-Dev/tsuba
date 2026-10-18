//! The PC-side profile library (Synapse's model): any number of profiles per device, kept in
//! `~/.config/tsuba/profiles/<pid>/<id>.json`. A device's on-board slots (5 on the supported
//! models) are where profiles *run*: making a library profile active writes it into an
//! on-board slot (only what differs from what the slot holds), and up to one profile per slot
//! can be *pinned* there so it works with the hardware profile button and without Tsuba.
//!
//! A profile stores what differs from the model's factory defaults (all keys when the defaults
//! aren't known), macro bindings by library GUID (uploaded on activation when missing),
//! software actions, per-profile performance settings and the command dial modes. Lighting
//! stays device-wide (config.toml).
//!
//! `library.json` next to the profiles records which library profile each on-board slot holds
//! (`holds`), the optional working slot and which slots were already migrated (`seen`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{Action, ActionConfig, DialSlotConfig, Lighting};
use crate::macros::Playback;
use crate::performance::Performance;
use crate::state::AssignmentState;

/// A key/button's firmware function on one layer (0 standard, 1 Hypershift).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub layer: u8,
    pub key: u8,
    pub function: u8,
    #[serde(default)]
    pub data: Vec<u8>,
}

/// A key/button bound to a library macro (uploaded to the device when the profile is activated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroBinding {
    pub layer: u8,
    pub key: u8,
    pub guid: String,
    pub playback: Playback,
    #[serde(default = "one")]
    pub count: u8,
}

fn one() -> u8 {
    1
}

/// A key/button running a software action (tsubad allocates its shortcut slot on activation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionBinding {
    pub layer: u8,
    pub key: u8,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryProfile {
    /// 32 hex digits, stable for the life of the profile.
    pub id: String,
    pub name: String,
    /// Unix milliseconds.
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub modified: u64,
    /// The Synapse profile it was imported from (re-importing updates this profile).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synapse_guid: Option<String>,
    /// The on-board slot this profile is pinned to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_slot: Option<u8>,
    /// Keys/buttons whose function differs from the factory default (every key the device
    /// reported when the model's defaults aren't known).
    #[serde(default)]
    pub assignments: Vec<KeyBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<MacroBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionBinding>,
    /// DPI stages, polling rate and scroll settings (`profile` unset). None = leave as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<Performance>,
    /// Command dial modes (keyboards). None = leave as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial: Option<Vec<DialSlotConfig>>,
    /// Lighting that comes with the profile (effect, colours, brightness; a Lighting Studio
    /// profile counts as an effect). Applied whenever the profile is activated. None = the
    /// device keeps whatever lighting it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<Lighting>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn new_id() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        b[..8].copy_from_slice(&now_ms().to_be_bytes());
        b[8..12].copy_from_slice(&std::process::id().to_be_bytes());
    }
    crate::macros::hex(&b)
}

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------------------
// Factory defaults

type Table = BTreeMap<u8, (u8, Vec<u8>)>;

/// Factory tables per model and layer, read from freshly created on-board profiles on the
/// BlackWidow V4 Pro and Basilisk V3 Pro. The GUI's KeyNames.qml holds the same data (a test
/// keeps them identical).
const DEFAULTS_JSON: &str = include_str!("../data/factory-defaults.json");

fn defaults() -> &'static HashMap<String, [Table; 2]> {
    static D: OnceLock<HashMap<String, [Table; 2]>> = OnceLock::new();
    D.get_or_init(|| {
        let raw: HashMap<String, HashMap<String, BTreeMap<String, (u8, Vec<u8>)>>> = serde_json::from_str(DEFAULTS_JSON).expect("factory-defaults.json");
        raw.into_iter()
            .map(|(pid, layers)| {
                let t = |l: &str| layers.get(l).map(|m| m.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.clone()))).collect()).unwrap_or_default();
                (pid, [t("0"), t("1")])
            })
            .collect()
    })
}

/// The device key libraries are stored under: wired/wireless twins share one library.
pub fn library_key(pid: &str) -> String {
    let pair = u16::from_str_radix(pid, 16).ok().and_then(tsuba_hid::db::get).and_then(|s| s.pair).map(crate::pid_key);
    match pair {
        Some(p) if p.as_str() < pid => p,
        _ => pid.to_string(),
    }
}

/// The factory table of a model and layer (empty when not known).
pub fn factory_table(pid: &str, layer: u8) -> Table {
    defaults().get(&library_key(pid)).and_then(|t| t.get(layer as usize)).cloned().unwrap_or_default()
}

pub fn factory_default(pid: &str, layer: u8, key: u8) -> Option<(u8, Vec<u8>)> {
    defaults().get(&library_key(pid))?.get(layer as usize)?.get(&key).cloned()
}

// ---------------------------------------------------------------------------------------
// Editing

impl LibraryProfile {
    pub fn new(name: &str) -> Self {
        let t = now_ms();
        Self {
            id: new_id(),
            name: name.trim().to_string(),
            created: t,
            modified: t,
            synapse_guid: None,
            pinned_slot: None,
            assignments: Vec::new(),
            macros: Vec::new(),
            actions: Vec::new(),
            performance: None,
            dial: None,
            lighting: None,
        }
    }

    /// A profile that puts the device back to its factory settings: factory keys (nothing
    /// stored), the stock performance values and, on keyboards with a command dial, the stock
    /// dial slot order. Lighting is left alone.
    pub fn factory(pid: &str, name: &str) -> Self {
        let mut p = Self::new(name);
        let spec = u16::from_str_radix(pid, 16).ok().and_then(tsuba_hid::db::get);
        if let Some(spec) = spec {
            let mut perf = Performance { polling_hz: Some(1000), ..Default::default() };
            if spec.features.dpi_stages {
                perf.dpi_stages = Some(FACTORY_DPI_STAGES.to_vec());
                perf.dpi_active_stage = Some(1);
            }
            if spec.features.scroll_modes {
                perf.free_spin = Some(false);
            }
            if spec.features.scroll_acceleration {
                perf.scroll_acceleration = Some(false);
            }
            if spec.features.smart_reel {
                perf.smart_reel = Some(false);
            }
            p.performance = Some(perf);
            if spec.features.dial {
                p.dial = Some(factory_dial(None));
            }
        }
        p
    }

    fn clear_key(&mut self, layer: u8, key: u8) {
        self.assignments.retain(|a| !(a.layer == layer && a.key == key));
        self.macros.retain(|a| !(a.layer == layer && a.key == key));
        self.actions.retain(|a| !(a.layer == layer && a.key == key));
    }

    /// A key now runs a firmware function (stored only when it differs from the default).
    pub fn set_key(&mut self, pid: &str, layer: u8, key: u8, function: u8, data: &[u8]) {
        self.clear_key(layer, key);
        if factory_default(pid, layer, key).is_none_or(|(f, d)| f != function || d != data) {
            self.assignments.push(KeyBinding { layer, key, function, data: data.to_vec() });
            self.assignments.sort_by_key(|a| (a.layer, a.key));
        }
    }

    pub fn set_macro(&mut self, b: MacroBinding) {
        self.clear_key(b.layer, b.key);
        self.macros.push(b);
        self.macros.sort_by_key(|a| (a.layer, a.key));
    }

    pub fn set_action(&mut self, b: ActionBinding) {
        self.clear_key(b.layer, b.key);
        self.actions.push(b);
        self.actions.sort_by_key(|a| (a.layer, a.key));
    }

    /// Take over the fields `p` sets (a partial update, as SetPerformance sends).
    pub fn merge_performance(&mut self, p: &Performance) {
        let cur = self.performance.get_or_insert_with(Default::default);
        cur.profile = None;
        cur.polling_options.clear();
        if p.polling_hz.is_some() {
            cur.polling_hz = p.polling_hz;
        }
        if p.free_spin.is_some() {
            cur.free_spin = p.free_spin;
        }
        if p.scroll_acceleration.is_some() {
            cur.scroll_acceleration = p.scroll_acceleration;
        }
        if p.smart_reel.is_some() {
            cur.smart_reel = p.smart_reel;
        }
        if let Some(stages) = &p.dpi_stages {
            cur.dpi_stages = Some(stages.clone());
            cur.dpi_active_stage = Some(p.dpi_active_stage.unwrap_or(1).clamp(1, stages.len().max(1) as u8));
        }
    }

    /// What the key should do on `layer`: its stored binding or the factory default.
    pub fn desired(&self, pid: &str, layer: u8, key: u8) -> Option<(u8, Vec<u8>)> {
        self.assignments.iter().find(|a| a.layer == layer && a.key == key).map(|a| (a.function, a.data.clone())).or_else(|| factory_default(pid, layer, key))
    }

    /// Fill the key tables from a device's two read tables (factory-default keys are left out).
    pub fn set_tables(&mut self, pid: &str, tables: &[Vec<AssignmentState>; 2]) {
        self.assignments.clear();
        for (layer, t) in tables.iter().enumerate() {
            for a in t {
                if factory_default(pid, layer as u8, a.key).is_none_or(|(f, d)| f != a.function || d != a.data) {
                    self.assignments.push(KeyBinding { layer: layer as u8, key: a.key, function: a.function, data: a.data.clone() });
                }
            }
        }
        self.assignments.sort_by_key(|a| (a.layer, a.key));
    }

    /// A copy with a new id and name, not pinned, not linked to Synapse.
    pub fn duplicate(&self, name: &str) -> Self {
        let mut p = self.clone();
        let fresh = Self::new(name);
        p.id = fresh.id;
        p.name = fresh.name;
        p.created = fresh.created;
        p.modified = fresh.modified;
        p.pinned_slot = None;
        p.synapse_guid = None;
        p
    }

    /// The profile built from a Synapse import plan entry (performance without device-wide
    /// settings; those are applied per device by the importer).
    pub fn from_synapse(pid: &str, sp: &crate::synapse_import::plan::ProfilePlan) -> Self {
        use crate::synapse_import::plan::AssignmentPlan as A;
        let mut p = Self::new(&sp.name);
        p.synapse_guid = Some(sp.guid.clone());
        for a in &sp.assignments {
            match a {
                A::Function { key, layer, function, data, .. } => p.set_key(pid, *layer, *key, *function, data),
                A::Macro { key, layer, guid, playback, count, .. } => p.set_macro(MacroBinding { layer: *layer, key: *key, guid: guid.clone(), playback: *playback, count: *count }),
                A::Action { key, layer, action, .. } => p.set_action(ActionBinding { layer: *layer, key: *key, action: action.clone() }),
            }
        }
        if let Some(perf) = &sp.performance {
            p.merge_performance(perf);
        }
        p
    }
}

/// The five stages a freshly reset on-board profile of a mouse holds.
pub const FACTORY_DPI_STAGES: [[u16; 2]; 5] = [[400, 400], [800, 800], [1600, 1600], [3200, 3200], [6400, 6400]];

/// The command dial's stock slot table: the eight modes in their id order, the first four on.
/// `colours` (the slots' current colours, if known) are kept — the stock colours aren't known.
pub fn factory_dial(colours: Option<&[DialSlotConfig]>) -> Vec<DialSlotConfig> {
    const MODES: [&str; 8] = ["KeyboardBrightness", "WindowsZoom", "SwitchApplications", "TrackJogging", "TrackSelector", "VerticalScrolling", "HorizontalScrolling", "SwitchBrowserTabs"];
    MODES
        .iter()
        .enumerate()
        .map(|(i, f)| DialSlotConfig { function: (*f).to_string(), colour: colours.and_then(|c| c.get(i)).map_or_else(|| "44d62c".to_string(), |c| c.colour.clone()), enabled: i < 4 })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Activation plan

/// One write needed to make a slot match a profile.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Key(KeyBinding),
    Macro(MacroBinding),
    Action(ActionBinding),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub ops: Vec<Op>,
    /// Performance fields that differ (with `profile` = the slot).
    pub performance: Option<Performance>,
    /// Dial modes to write (None = leave).
    pub dial: Option<Vec<DialSlotConfig>>,
    /// Software actions saved for this slot that the profile doesn't have: (layer, key).
    pub drop_actions: Vec<(u8, u8)>,
}

impl Plan {
    /// Device writes this plan sends at most (each assignment is one Set + one read-back).
    pub fn writes(&self) -> usize {
        self.ops.len() + self.performance.as_ref().map_or(0, perf_writes) + usize::from(self.dial.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.performance.is_none() && self.dial.is_none() && self.drop_actions.is_empty()
    }
}

fn perf_writes(p: &Performance) -> usize {
    [p.polling_hz.is_some(), p.free_spin.is_some(), p.scroll_acceleration.is_some(), p.smart_reel.is_some()].iter().filter(|x| **x).count() + if p.dpi_stages.is_some() { 2 } else { 0 }
}

/// The key/button binding (function, data) a saved action sends.
pub fn action_binding(code: u8) -> (u8, Vec<u8>) {
    if code < crate::config::ACTION_SLOTS {
        (2, crate::config::slot_keys(code).to_vec())
    } else {
        (17, vec![code])
    }
}

/// What to write so that a slot whose current state is `tables` / `perf` (and whose saved
/// software actions are `actions`) matches profile `p`. `macro_ids` maps library macro GUIDs to
/// the on-board ids the device already holds them under.
pub fn plan(pid: &str, slot: u8, p: &LibraryProfile, tables: &[Vec<AssignmentState>; 2], perf: Option<&Performance>, macro_ids: &HashMap<String, u16>, actions: &[ActionConfig]) -> Plan {
    let mut out = Plan::default();
    for layer in [0u8, 1] {
        let cur: BTreeMap<u8, (u8, &[u8])> = tables[layer as usize].iter().map(|a| (a.key, (a.function, a.data.as_slice()))).collect();
        let mut keys: BTreeSet<u8> = cur.keys().copied().collect();
        keys.extend(p.assignments.iter().filter(|a| a.layer == layer).map(|a| a.key));
        keys.extend(p.macros.iter().filter(|a| a.layer == layer).map(|a| a.key));
        keys.extend(p.actions.iter().filter(|a| a.layer == layer).map(|a| a.key));
        for key in keys {
            let now = cur.get(&key).copied();
            if let Some(b) = p.actions.iter().find(|a| a.layer == layer && a.key == key) {
                let saved = actions.iter().find(|a| a.profile == slot && a.layer == layer && a.key == key);
                let in_place = saved.is_some_and(|s| s.action == b.action && now.is_some_and(|(f, d)| (f, d.to_vec()) == action_binding(s.code)));
                if !in_place {
                    out.ops.push(Op::Action(b.clone()));
                }
                continue;
            }
            if let Some(b) = p.macros.iter().find(|a| a.layer == layer && a.key == key) {
                let ok = macro_ids.get(&b.guid).is_some_and(|id| {
                    let (f, d) = crate::macros::binding(*id, b.playback, b.count);
                    now.is_some_and(|(nf, nd)| nf == f && nd == d.as_slice())
                });
                if !ok {
                    out.ops.push(Op::Macro(b.clone()));
                }
                continue;
            }
            let stored = p.assignments.iter().find(|a| a.layer == layer && a.key == key);
            // A default for a key the device didn't report is skipped (it doesn't have the key).
            let want = match (stored, now) {
                (Some(a), _) => (a.function, a.data.clone()),
                (None, Some(_)) => match factory_default(pid, layer, key) {
                    Some(d) => d,
                    None => continue,
                },
                (None, None) => continue,
            };
            if now.is_none_or(|(f, d)| f != want.0 || d != want.1.as_slice()) {
                out.ops.push(Op::Key(KeyBinding { layer, key, function: want.0, data: want.1 }));
            }
        }
    }
    for a in actions.iter().filter(|a| a.profile == slot) {
        if !p.actions.iter().any(|b| b.layer == a.layer && b.key == a.key) {
            out.drop_actions.push((a.layer, a.key));
        }
    }
    if let Some(want) = &p.performance {
        out.performance = perf_diff(want, perf, slot);
    }
    out.dial = p.dial.clone();
    out
}

/// Performance fields of `want` that differ from `cur` (None when nothing does).
pub fn perf_diff(want: &Performance, cur: Option<&Performance>, slot: u8) -> Option<Performance> {
    let mut d = Performance { profile: Some(slot), ..Default::default() };
    let differs = |w: Option<bool>, c: Option<bool>| w.is_some() && c.is_some() && w != c;
    if want.polling_hz.is_some() && cur.is_none_or(|c| c.polling_hz.is_some() && c.polling_hz != want.polling_hz) {
        d.polling_hz = want.polling_hz;
    }
    if differs(want.free_spin, cur.and_then(|c| c.free_spin)) || (want.free_spin.is_some() && cur.is_none()) {
        d.free_spin = want.free_spin;
    }
    if differs(want.scroll_acceleration, cur.and_then(|c| c.scroll_acceleration)) || (want.scroll_acceleration.is_some() && cur.is_none()) {
        d.scroll_acceleration = want.scroll_acceleration;
    }
    if differs(want.smart_reel, cur.and_then(|c| c.smart_reel)) || (want.smart_reel.is_some() && cur.is_none()) {
        d.smart_reel = want.smart_reel;
    }
    if let Some(stages) = &want.dpi_stages {
        let active = want.dpi_active_stage.unwrap_or(1);
        let same = cur.is_some_and(|c| c.dpi_stages.as_ref() == Some(stages) && c.dpi_active_stage == Some(active));
        if !same {
            d.dpi_stages = Some(stages.clone());
            d.dpi_active_stage = Some(active);
        }
    }
    (perf_writes(&d) > 0).then_some(d)
}

// ---------------------------------------------------------------------------------------
// Storage

/// `~/.config/tsuba/profiles`.
pub fn root() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("profiles")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryState {
    /// On-board slot → the library profile whose settings it holds.
    #[serde(default)]
    pub holds: BTreeMap<u8, String>,
    /// Slot unpinned profiles are written into (None = whichever slot is active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_slot: Option<u8>,
    /// On-board slots already copied into the library (each is migrated once).
    #[serde(default)]
    pub seen: BTreeSet<u8>,
    /// On-board macros Tsuba uploaded to run a library profile (on-board id → macro GUID).
    /// The only macros it ever removes from the device again, once no on-board slot uses them
    /// (macros put there by the user, Synapse or AssignMacro are never in this list).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub uploaded_macros: BTreeMap<u16, String>,
}

#[derive(Debug, Clone)]
pub struct DeviceLibrary {
    pub dir: PathBuf,
    pub state: LibraryState,
    pub profiles: Vec<LibraryProfile>,
    /// Files that couldn't be read (left alone).
    pub errors: Vec<String>,
}

/// Unique temporary file (`O_EXCL | O_NOFOLLOW`) + fsync + rename (see [`crate::fsutil::write_atomic`]).
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    crate::fsutil::write_atomic(path, text.as_bytes())
}

const STATE_FILE: &str = "library.json";

impl DeviceLibrary {
    /// Load the library of device `pid` below `root` (empty when it doesn't exist yet).
    pub fn load(root: &Path, pid: &str) -> Result<Self> {
        // The pid names the folder: 4 hex digits only (it comes from D-Bus callers).
        ensure!(pid.len() == 4 && pid.bytes().all(|b| b.is_ascii_hexdigit()), "bad device id {pid:?}");
        let dir = root.join(library_key(pid));
        let state = match std::fs::read_to_string(dir.join(STATE_FILE)) {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", dir.join(STATE_FILE).display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => LibraryState::default(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", dir.join(STATE_FILE).display())),
        };
        let mut profiles = Vec::new();
        let mut errors = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let path = e.path();
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
                if path.extension().and_then(|x| x.to_str()) != Some("json") || !valid_id(stem) {
                    continue;
                }
                match std::fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|s| Ok(serde_json::from_str::<LibraryProfile>(&s)?)) {
                    Ok(p) if p.id == stem => profiles.push(p),
                    Ok(_) => errors.push(format!("{}: id doesn't match the file name", path.display())),
                    Err(e) => errors.push(format!("{}: {e:#}", path.display())),
                }
            }
        }
        profiles.sort_by(|a, b| (a.created, &a.name, &a.id).cmp(&(b.created, &b.name, &b.id)));
        Ok(Self { dir, state, profiles, errors })
    }

    pub fn save_state(&self) -> Result<()> {
        write_atomic(&self.dir.join(STATE_FILE), &serde_json::to_string_pretty(&self.state)?)
    }

    /// Insert or replace `p` (by id) and write its file.
    pub fn put(&mut self, p: LibraryProfile) -> Result<()> {
        ensure!(valid_id(&p.id), "bad profile id {:?}", p.id);
        ensure!(!p.name.trim().is_empty(), "the profile needs a name");
        write_atomic(&self.dir.join(format!("{}.json", p.id)), &serde_json::to_string_pretty(&p)?)?;
        match self.profiles.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p,
            None => self.profiles.push(p),
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        ensure!(valid_id(id), "bad profile id {id:?}");
        let path = self.dir.join(format!("{id}.json"));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", path.display())),
        }
        self.profiles.retain(|p| p.id != id);
        self.state.holds.retain(|_, v| v != id);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&LibraryProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn require(&self, id: &str) -> Result<&LibraryProfile> {
        self.get(id).with_context(|| format!("no library profile {id}"))
    }

    pub fn by_synapse(&self, guid: &str) -> Option<&LibraryProfile> {
        self.profiles.iter().find(|p| p.synapse_guid.as_deref() == Some(guid))
    }

    /// The profile pinned to `slot`.
    pub fn pinned(&self, slot: u8) -> Option<&LibraryProfile> {
        self.profiles.iter().find(|p| p.pinned_slot == Some(slot))
    }

    /// The profile whose settings `slot` holds.
    pub fn holder(&self, slot: u8) -> Option<&LibraryProfile> {
        self.state.holds.get(&slot).and_then(|id| self.get(id))
    }

    /// Record that `slot` now holds profile `id` (a profile is held by one slot at a time).
    pub fn set_holds(&mut self, slot: u8, id: &str) {
        self.state.holds.retain(|_, v| v != id);
        self.state.holds.insert(slot, id.to_string());
    }

    pub fn pinned_count(&self) -> usize {
        self.profiles.iter().filter(|p| p.pinned_slot.is_some()).count()
    }

    /// `name`, or `name 2`, `name 3`… when taken.
    pub fn unique_name(&self, name: &str, except: Option<&str>) -> String {
        let name = name.trim();
        let taken = |n: &str| self.profiles.iter().any(|p| p.name == n && Some(p.id.as_str()) != except);
        if !taken(name) {
            return name.to_string();
        }
        (2..).map(|i| format!("{name} {i}")).find(|n| !taken(n)).unwrap_or_default()
    }
}

/// The exported form of a profile (`.tsuba-profile.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub tsuba_profile: u32,
    pub device: String,
    pub profile: LibraryProfile,
}

pub fn export(pid: &str, p: &LibraryProfile) -> Result<String> {
    let mut p = p.clone();
    p.pinned_slot = None;
    Ok(serde_json::to_string_pretty(&Export { tsuba_profile: 1, device: library_key(pid), profile: p })?)
}

/// The largest profile file `import` reads. A real profile is a few KiB.
pub const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024;
/// Longest profile name kept.
pub const MAX_NAME_CHARS: usize = 80;

/// Read a profile file: a regular file (no pipes or devices, which could block or never end)
/// of at most [`MAX_IMPORT_BYTES`].
pub fn read_import_file(path: &str) -> Result<String> {
    use std::io::Read as _;
    // O_NONBLOCK so opening a FIFO without a writer returns at once; checked below anyway.
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path)
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path);
    let file = file.with_context(|| format!("opening {path}"))?;
    let meta = file.metadata().with_context(|| format!("reading {path}"))?;
    ensure!(meta.is_file(), "{path} is not a regular file");
    ensure!(meta.len() <= MAX_IMPORT_BYTES, "{path} is too big for a profile ({} bytes, at most {MAX_IMPORT_BYTES})", meta.len());
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1).read_to_end(&mut bytes).with_context(|| format!("reading {path}"))?;
    ensure!(bytes.len() as u64 <= MAX_IMPORT_BYTES, "{path} is too big for a profile");
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("{path} is not text"))
}

/// A name safe to show and to use in file names and notifications: no control characters,
/// no surrounding blanks, at most [`MAX_NAME_CHARS`] characters.
pub fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).collect();
    s.trim().chars().take(MAX_NAME_CHARS).collect::<String>().trim().to_string()
}

/// A software action in an imported profile: it runs commands or opens things on this
/// computer when the key is pressed, so it needs the user's approval before it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoftwareAction {
    pub layer: u8,
    pub key: u8,
    /// "launch", "url" or "type".
    pub kind: &'static str,
    /// The command line / URL / text.
    pub text: String,
}

/// Launch / Open-URL / Type actions of `p`.
pub fn software_actions(p: &LibraryProfile) -> Vec<SoftwareAction> {
    p.actions
        .iter()
        .filter_map(|a| {
            let (kind, text) = match &a.action {
                Action::Launch { command } => ("launch", command),
                Action::Url { url } => ("url", url),
                Action::Type { text } => ("type", text),
                _ => return None,
            };
            Some(SoftwareAction { layer: a.layer, key: a.key, kind, text: text.clone() })
        })
        .collect()
}

/// A stable fingerprint of a list of software actions: the app shows the list to the user and
/// sends this back to say "these exact ones are approved", so a file that changed in between
/// is not imported with commands nobody saw.
pub fn approval_digest(list: &[SoftwareAction]) -> String {
    if list.is_empty() {
        return String::new();
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for a in list {
        for b in format!("{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1e}", a.layer, a.key, a.kind, a.text).bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// An imported profile and the software actions the file carried.
pub struct Imported {
    pub profile: LibraryProfile,
    /// Every software action in the file (kept only when approved).
    pub software: Vec<SoftwareAction>,
    /// Whether they were kept.
    pub approved: bool,
}

/// Parse an exported profile for device `pid`: a new id, not pinned, a clean name. Software
/// actions (commands, URLs, typed text) are removed unless `approved` is the
/// [`approval_digest`] of exactly the ones the file carries.
pub fn import(pid: &str, text: &str, approved: &str) -> Result<Imported> {
    let e: Export = serde_json::from_str(text).context("not a Tsuba profile file")?;
    if e.tsuba_profile != 1 {
        bail!("unsupported profile file version {}", e.tsuba_profile);
    }
    let want = library_key(pid);
    let kind = |p: &str| u16::from_str_radix(p, 16).ok().and_then(tsuba_hid::db::get).map(|s| s.kind);
    ensure!(e.device == want || (kind(&e.device).is_some() && kind(&e.device) == kind(&want)), "this profile is for device {} (this one is {want})", e.device);
    let name = match clean_name(&e.profile.name) {
        n if n.is_empty() => "Imported profile".to_string(),
        n => n,
    };
    let mut p = e.profile.duplicate(&name);
    p.synapse_guid = e.profile.synapse_guid.filter(|g| g.len() <= 80 && !g.chars().any(|c| c.is_control()));
    let software = software_actions(&p);
    let keep = software.is_empty() || (!approved.is_empty() && approved == approval_digest(&software));
    if !keep {
        p.actions.retain(|a| !matches!(a.action, Action::Launch { .. } | Action::Url { .. } | Action::Type { .. }));
    }
    Ok(Imported { profile: p, software, approved: keep })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(key: u8, function: u8, data: &[u8]) -> AssignmentState {
        crate::state::assignment_state(tsuba_proto::decode::Assignment { profile: 1, key, layer: 0, function, data: data.to_vec() })
    }

    fn factory_states(pid: &str, layer: u8) -> Vec<AssignmentState> {
        factory_table(pid, layer).into_iter().map(|(k, (f, d))| st(k, f, &d)).collect()
    }

    #[test]
    fn defaults_match_the_gui_tables() {
        // KeyNames.qml keeps its own copy for the editor; both must stay identical.
        let qml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tsuba-gui/qml/KeyNames.qml")).unwrap();
        for (name, pid, layer) in [("kbDefaults", "028d", 0), ("kbDefaultsShift", "028d", 1), ("mouseDefaults", "00aa", 0), ("mouseDefaultsShift", "00aa", 1)] {
            let start = qml.find(&format!("readonly property var {name}: ({{")).unwrap();
            let body = &qml[start..start + qml[start..].find("})").unwrap()];
            let mut parsed = Table::new();
            for entry in body.split("]],").map(|e| e.trim()) {
                let Some((k, rest)) = entry.rsplit_once(": [") else { continue };
                let key: u8 = k.rsplit(['{', ' ', '\n']).next().unwrap().trim().parse().unwrap();
                let (f, d) = rest.split_once(", [").unwrap();
                let num = |s: &str| if let Some(h) = s.trim().strip_prefix("0x") { u8::from_str_radix(h, 16).unwrap() } else { s.trim().parse().unwrap() };
                let data: Vec<u8> = d.trim_end_matches([']', ')', '}', '\n', ' ']).split(',').filter(|x| !x.trim().is_empty()).map(num).collect();
                parsed.insert(key, (num(f), data));
            }
            assert_eq!(parsed, factory_table(pid, layer), "{name}");
        }
        assert_eq!(factory_table("00ab", 0), factory_table("00aa", 0), "wireless twin shares the table");
        assert_eq!(library_key("00ab"), "00aa");
        assert_eq!(library_key("028d"), "028d");
    }

    #[test]
    fn tables_store_only_differences() {
        let mut t = factory_states("00aa", 0);
        t.iter_mut().find(|a| a.key == 4).unwrap().data = vec![9];
        let mut p = LibraryProfile::new("A");
        p.set_tables("00aa", &[t, factory_states("00aa", 1)]);
        assert_eq!(p.assignments, vec![KeyBinding { layer: 0, key: 4, function: 1, data: vec![9] }]);
        // Setting it back to the default removes the entry.
        p.set_key("00aa", 0, 4, 1, &[4]);
        assert!(p.assignments.is_empty());
        // Unknown-default keys are always kept.
        p.set_key("00aa", 0, 200, 2, &[0, 4]);
        assert_eq!(p.assignments.len(), 1);
    }

    #[test]
    fn plan_diffs_against_the_slot() {
        let pid = "028d";
        let cur = [factory_states(pid, 0), factory_states(pid, 1)];
        let mut p = LibraryProfile::new("Gaming");
        p.set_key(pid, 0, 2, 2, &[0, 0x3a]); // 1 → F1
        p.set_macro(MacroBinding { layer: 0, key: 160, guid: "g".into(), playback: Playback::Once, count: 1 });
        p.set_action(ActionBinding { layer: 1, key: 161, action: Action::Url { url: "https://x".into() } });
        let pl = plan(pid, 1, &p, &cur, None, &HashMap::new(), &[]);
        assert_eq!(pl.ops.len(), 3);
        assert!(matches!(&pl.ops[0], Op::Key(k) if k.key == 2));
        // Already there: nothing to do.
        let mut done = cur.clone();
        done[0].iter_mut().find(|a| a.key == 2).unwrap().data = vec![0, 0x3a];
        let (f, d) = crate::macros::binding(0x1234, Playback::Once, 1);
        let m = done[0].iter_mut().find(|a| a.key == 160).unwrap();
        (m.function, m.data) = (f, d);
        let saved = ActionConfig { code: 5, profile: 1, layer: 1, key: 161, action: Action::Url { url: "https://x".into() } };
        let a = done[1].iter_mut().find(|a| a.key == 161).unwrap();
        (a.function, a.data) = action_binding(5);
        let ids: HashMap<String, u16> = [("g".to_string(), 0x1234)].into();
        let pl = plan(pid, 1, &p, &done, None, &ids, &[saved.clone()]);
        assert!(pl.ops.is_empty(), "{:?}", pl.ops);
        // Switching to a profile without those: keys go back to their defaults, the action is dropped.
        let other = LibraryProfile::new("Plain");
        let pl = plan(pid, 1, &other, &done, None, &ids, &[saved]);
        assert_eq!(pl.ops.len(), 3);
        assert_eq!(pl.drop_actions, vec![(1, 161)]);
        assert!(pl.ops.iter().all(|o| matches!(o, Op::Key(k) if factory_default(pid, k.layer, k.key) == Some((k.function, k.data.clone())))));
    }

    #[test]
    fn performance_diff() {
        let cur = Performance { polling_hz: Some(1000), dpi_stages: Some(vec![[800, 800]]), dpi_active_stage: Some(1), smart_reel: Some(false), ..Default::default() };
        let want = Performance { polling_hz: Some(1000), dpi_stages: Some(vec![[800, 800]]), dpi_active_stage: Some(1), smart_reel: Some(true), ..Default::default() };
        let d = perf_diff(&want, Some(&cur), 2).unwrap();
        assert_eq!((d.profile, d.smart_reel, d.polling_hz, d.dpi_stages.is_some()), (Some(2), Some(true), None, false));
        assert!(perf_diff(&cur, Some(&cur), 2).is_none());
        let mut p = LibraryProfile::new("x");
        p.merge_performance(&Performance { profile: Some(3), dpi_stages: Some(vec![[400, 400], [800, 800]]), dpi_active_stage: Some(7), ..Default::default() });
        let perf = p.performance.as_ref().unwrap();
        assert_eq!((perf.profile, perf.dpi_active_stage), (None, Some(2)));
    }

    #[test]
    fn factory_profiles_reset_what_the_model_has() {
        let kb = LibraryProfile::factory("028d", "Stock");
        assert!(kb.assignments.is_empty() && kb.lighting.is_none(), "keys follow the factory table, lighting is left alone");
        let perf = kb.performance.as_ref().unwrap();
        assert_eq!((perf.polling_hz, perf.dpi_stages.is_some(), perf.free_spin), (Some(1000), false, None));
        let dial = kb.dial.as_ref().unwrap();
        assert_eq!(dial.len(), 8);
        assert_eq!(dial.iter().filter(|d| d.enabled).count(), 4);
        assert_eq!(dial[3].function, "TrackJogging");
        let mouse = LibraryProfile::factory("00aa", "Stock");
        let perf = mouse.performance.as_ref().unwrap();
        assert_eq!(perf.dpi_stages.as_deref(), Some(&FACTORY_DPI_STAGES[..]));
        assert_eq!(perf.free_spin, Some(false));
        assert!(mouse.dial.is_none());
        // The dial slots keep the colours they had.
        let mut cur = factory_dial(None);
        cur[2].colour = "ff0000".into();
        assert_eq!(factory_dial(Some(&cur))[2].colour, "ff0000");
        // Every dial function name is one apply_dial accepts.
        assert!(dial.iter().all(|d| (0..=7u8).any(|i| format!("{:?}", tsuba_proto::decode::KnobFunction::from(i)) == d.function)));
    }

    #[test]
    fn lighting_travels_with_the_profile_and_stays_out_of_old_files() {
        let mut p = LibraryProfile::new("Night");
        p.lighting = Some(Lighting { effect: crate::config::EffectConfig::Static { colour: "112233".into() }, brightness: Some(30) });
        let s = serde_json::to_string(&p).unwrap();
        let back: LibraryProfile = serde_json::from_str(&s).unwrap();
        assert_eq!(back.lighting, p.lighting);
        let old: LibraryProfile = serde_json::from_str(r#"{"id": "00000000000000000000000000000000", "name": "Old"}"#).unwrap();
        assert!(old.lighting.is_none());
        assert!(!serde_json::to_string(&old).unwrap().contains("lighting"));
        // A duplicate keeps the lighting; an export carries it.
        assert_eq!(p.duplicate("Copy").lighting, p.lighting);
        assert_eq!(import("028d", &export("028d", &p).unwrap(), "").unwrap().profile.lighting, p.lighting);
    }

    #[test]
    fn storage_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tsuba-library-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = DeviceLibrary::load(&dir, "00ab").unwrap();
        assert!(lib.dir.ends_with("00aa"));
        let mut a = LibraryProfile::new("Aim");
        a.pinned_slot = Some(2);
        let b = LibraryProfile::new("Bee");
        lib.put(a.clone()).unwrap();
        lib.put(b.clone()).unwrap();
        lib.set_holds(2, &a.id);
        lib.set_holds(3, &a.id);
        assert_eq!(lib.state.holds.len(), 1, "a profile is held by one slot");
        lib.save_state().unwrap();
        std::fs::write(lib.dir.join("notes.json"), "{}").unwrap();
        let again = DeviceLibrary::load(&dir, "00aa").unwrap();
        assert_eq!(again.profiles.len(), 2);
        assert_eq!(again.holder(3).map(|p| p.id.clone()), Some(a.id.clone()));
        assert_eq!(again.pinned(2).map(|p| p.name.as_str()), Some("Aim"));
        assert_eq!(again.unique_name("Aim", None), "Aim 2");
        assert_eq!(again.unique_name("Aim", Some(&a.id)), "Aim", "renaming to its own name");
        let mut lib = again;
        lib.remove(&a.id).unwrap();
        assert!(lib.state.holds.is_empty());
        assert_eq!(DeviceLibrary::load(&dir, "00aa").unwrap().profiles.len(), 1);
        // Export / import.
        let text = export("00aa", &b).unwrap();
        let back = import("00ab", &text, "").unwrap().profile;
        assert_ne!(back.id, b.id);
        assert_eq!(back.name, "Bee");
        assert!(import("028d", &text, "").is_err(), "a mouse profile doesn't fit the keyboard");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
