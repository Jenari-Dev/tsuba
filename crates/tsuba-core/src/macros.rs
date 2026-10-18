//! Macros: a library on the PC (`~/.config/tsuba/macros.json`), pushed to a device's on-board
//! memory when a key or button is bound to it (Synapse's model). A macro is linked to its
//! on-board copy by the GUID stored in the device's name block.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::Device;
use tsuba_proto::macros as m;

/// One recorded action, in the form the app and the library file use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroEvent {
    /// Keyboard key by HID usage (modifiers are 0xe0–0xe7).
    Key { hid: u8, down: bool },
    /// Mouse button 1 left, 2 right, 3 middle, 4, 5.
    Button { button: u8, down: bool },
    Scroll { up: bool },
    Delay { ms: u32 },
    Cursor { x: u16, y: u16 },
    /// A multimedia key (HID consumer usage, e.g. 0xcd play/pause) pressed or released.
    Media { usage: u16, down: bool },
    /// A system key (power 0x81, sleep 0x82, wake 0x83) pressed or released.
    Power { code: u8, down: bool },
    /// Run a command line on the PC. The macro plays a shortcut-slot key combination that
    /// tsubad turns into the command (see [`slot_events`]); `slot` is allocated by tsubad when
    /// the macro is saved and is `None` until then.
    Run {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slot: Option<u8>,
    },
    Raw { op: u8, data: [u8; 2] },
}

/// Event opcodes of the media / system keys (FINDINGS "Macros": 03/04 power, 05/06 media; the
/// two data bytes are the usage, high byte first — to be confirmed on hardware).
const OP_POWER_DOWN: u8 = 0x03;
const OP_POWER_UP: u8 = 0x04;
const OP_MEDIA_DOWN: u8 = 0x05;
const OP_MEDIA_UP: u8 = 0x06;

/// The key presses a shortcut slot's combination consists of (modifiers down, key down, key up,
/// modifiers up, with short delays so the desktop sees them as one chord).
pub fn slot_events(slot: u8) -> Vec<m::Event> {
    let [mods, key] = crate::config::slot_keys(slot);
    let held: Vec<u8> = [(0x01u8, 0xe0u8), (0x02, 0xe1), (0x04, 0xe2), (0x08, 0xe3)].iter().filter(|(bit, _)| mods & bit != 0).map(|(_, hid)| *hid).collect();
    let mut out: Vec<m::Event> = held.iter().map(|&hid| m::Event::Key { hid, down: true }).collect();
    out.push(m::Event::Delay { ms: 20 });
    out.push(m::Event::Key { hid: key, down: true });
    out.push(m::Event::Delay { ms: 20 });
    out.push(m::Event::Key { hid: key, down: false });
    out.push(m::Event::Delay { ms: 20 });
    out.extend(held.iter().rev().map(|&hid| m::Event::Key { hid, down: false }));
    out
}

impl MacroEvent {
    /// The device events this event plays as. A [`MacroEvent::Run`] without a slot can't be
    /// written yet.
    pub fn to_events(&self) -> Result<Vec<m::Event>> {
        Ok(match self {
            Self::Key { hid, down } => vec![m::Event::Key { hid: *hid, down: *down }],
            Self::Button { button, down } => vec![m::Event::Button { button: *button, down: *down }],
            Self::Scroll { up } => vec![m::Event::Scroll { up: *up }],
            Self::Delay { ms } => vec![m::Event::Delay { ms: *ms }],
            Self::Cursor { x, y } => vec![m::Event::Cursor { x: *x, y: *y }],
            Self::Media { usage, down } => vec![m::Event::Raw { op: if *down { OP_MEDIA_DOWN } else { OP_MEDIA_UP }, data: usage.to_be_bytes() }],
            Self::Power { code, down } => vec![m::Event::Raw { op: if *down { OP_POWER_DOWN } else { OP_POWER_UP }, data: [0, *code] }],
            Self::Run { slot: Some(s), .. } => slot_events(*s),
            Self::Run { command, slot: None } => bail!("the command “{command}” has no shortcut slot yet (save the macro through tsubad)"),
            Self::Raw { op, data } => vec![m::Event::Raw { op: *op, data: *data }],
        })
    }
}

impl From<m::Event> for MacroEvent {
    fn from(e: m::Event) -> Self {
        match e {
            m::Event::Key { hid, down } => Self::Key { hid, down },
            m::Event::Button { button, down } => Self::Button { button, down },
            m::Event::Scroll { up } => Self::Scroll { up },
            m::Event::Delay { ms } => Self::Delay { ms },
            m::Event::Cursor { x, y } => Self::Cursor { x, y },
            m::Event::Raw { op: OP_MEDIA_DOWN | OP_MEDIA_UP, data } => Self::Media { usage: u16::from_be_bytes(data), down: matches!(e, m::Event::Raw { op: OP_MEDIA_DOWN, .. }) },
            m::Event::Raw { op: op @ (OP_POWER_DOWN | OP_POWER_UP), data: [0, code] } => Self::Power { code, down: op == OP_POWER_DOWN },
            m::Event::Raw { op, data } => Self::Raw { op, data },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    /// 32 hex digits; empty when creating a new macro.
    #[serde(default)]
    pub guid: String,
    pub name: String,
    /// Folder the macro is listed under in the app ("" = none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    pub events: Vec<MacroEvent>,
}

impl Macro {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut events: Vec<m::Event> = Vec::new();
        for e in &self.events {
            events.extend(e.to_events()?);
        }
        m::encode_events(&events).map_err(anyhow::Error::msg)
    }

    /// Number of device events (what the name block's item count holds).
    pub fn event_count(&self) -> usize {
        self.events.iter().map(|e| e.to_events().map_or(1, |v| v.len())).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub macros: Vec<Macro>,
}

pub fn library_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("macros.json")
}

impl Library {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn get(&self, guid: &str) -> Option<&Macro> {
        self.macros.iter().find(|x| x.guid == guid)
    }

    /// A copy of `guid` under a new GUID and `name` (not on any device yet). Run-command events
    /// lose their slot: the copy gets its own when saved through tsubad.
    pub fn duplicate(&mut self, guid: &str, name: &str) -> Result<String> {
        let mut m = self.get(guid).cloned().context("no such macro in the library")?;
        m.guid = String::new();
        m.name = name.trim().to_string();
        for e in &mut m.events {
            if let MacroEvent::Run { slot, .. } = e {
                *slot = None;
            }
        }
        self.upsert(m)
    }

    /// `name`, or `name 2`, `name 3`… when another macro has it.
    pub fn unique_name(&self, name: &str, except: &str) -> String {
        let name = name.trim();
        let taken = |n: &str| self.macros.iter().any(|m| m.name == n && m.guid != except);
        if !taken(name) {
            return name.to_string();
        }
        (2..).map(|i| format!("{name} {i}")).find(|n| !taken(n)).unwrap_or_default()
    }

    /// Shortcut slots the macros' run-command events use.
    pub fn run_slots(&self) -> Vec<u8> {
        self.macros.iter().flat_map(|m| m.events.iter()).filter_map(|e| if let MacroEvent::Run { slot: Some(s), .. } = e { Some(*s) } else { None }).collect()
    }

    /// Insert or replace by GUID (assigning a new GUID when empty); returns the GUID.
    pub fn upsert(&mut self, mut mac: Macro) -> Result<String> {
        if mac.guid.is_empty() {
            mac.guid = hex(&random::<16>()?);
        }
        let guid = mac.guid.clone();
        match self.macros.iter_mut().find(|x| x.guid == guid) {
            Some(slot) => *slot = mac,
            None => self.macros.push(mac),
        }
        Ok(guid)
    }
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn guid_bytes(s: &str) -> Result<[u8; 16]> {
    ensure!(s.len() == 32, "a macro GUID is 32 hex digits");
    let mut b = [0u8; 16];
    for (i, out) in b.iter_mut().enumerate() {
        *out = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).context("bad GUID")?;
    }
    Ok(b)
}

#[derive(Debug, Clone, Serialize)]
pub struct OnboardMacro {
    pub id: u16,
    /// Set when the event data couldn't be read back (e.g. a cut-off write).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damaged: Option<String>,
    pub guid: String,
    pub name: String,
    pub events: Vec<MacroEvent>,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceMacros {
    pub max_id: u16,
    pub max_memory: u32,
    pub available: u32,
    pub recyclable: u32,
    pub macros: Vec<OnboardMacro>,
}

pub fn storage(dev: &Device) -> Result<m::StorageInfo> {
    m::storage_info(&dev.query(m::GET_STORAGE_INFO, &[])?).context("macro storage info")
}

pub fn ids(dev: &Device) -> Result<Vec<u16>> {
    let c = dev.query(m::GET_COUNT, &[])?;
    let count = u16::from_be_bytes([*c.first().unwrap_or(&0), *c.get(1).unwrap_or(&0)]) as usize;
    let mut out: Vec<u16> = Vec::new();
    while out.len() < count {
        let got = m::id_list(&dev.query(m::GET_ID_LIST, &(out.len() as u16).to_be_bytes())?, count - out.len());
        if got.is_empty() {
            break;
        }
        out.extend(got);
    }
    out.dedup();
    Ok(out)
}

pub fn read_name(dev: &Device, id: u16) -> Result<m::NameBlock> {
    let mut b = Vec::with_capacity(m::NAME_BLOCK);
    while b.len() < m::NAME_BLOCK {
        let n = (m::NAME_BLOCK - b.len()).min(m::NAME_CHUNK);
        let r = dev.query(m::GET_NAME, &m::encode_get_name(id, b.len() as u16))?;
        b.extend_from_slice(m::name_chunk(&r, n).context("short name reply")?);
    }
    m::name_block(&b).context("name block")
}

/// The macro's stored size and as many of its event bytes as the device will return. The
/// BlackWidow V4 Pro refuses reads past byte 20 of a macro, so the result can be a prefix.
pub fn read_data_prefix(dev: &Device, id: u16) -> Result<(usize, Vec<u8>)> {
    let size = m::data_size(&dev.query(m::GET_DATA_SIZE, &id.to_be_bytes())?).context("macro size")? as usize;
    let mut b = Vec::with_capacity(size);
    while b.len() < size {
        let n = (size - b.len()).min(m::DATA_CHUNK);
        let r = match dev.query(m::GET_MEMORY_DATA, &m::encode_get_memory(id, b.len() as u32, n as u8)) {
            Ok(r) => r,
            Err(_) if !b.is_empty() => break,
            Err(e) => return Err(e.into()),
        };
        let chunk = m::memory_chunk(&r).context("short data reply")?;
        if chunk.is_empty() {
            break;
        }
        b.extend_from_slice(chunk);
    }
    b.truncate(size);
    Ok((size, b))
}

pub fn read_data(dev: &Device, id: u16) -> Result<Vec<u8>> {
    let (size, b) = read_data_prefix(dev, id)?;
    ensure!(b.len() == size, "device returned only {} of {size} macro bytes", b.len());
    Ok(b)
}

/// True if the on-board macro `id` holds exactly `data` as far as the device lets us read.
fn matches_device(dev: &Device, id: u16, data: &[u8]) -> Result<bool> {
    let (size, prefix) = read_data_prefix(dev, id)?;
    Ok(size == data.len() && prefix.len() >= data.len().min(m::DATA_CHUNK) && data.starts_with(&prefix))
}

pub fn read_macro(dev: &Device, id: u16) -> Result<OnboardMacro> {
    let name = read_name(dev, id)?;
    let (size, events, damaged) = match read_data_prefix(dev, id) {
        Ok((size, d)) => {
            // A prefix may end mid-event; keep what decodes.
            let mut end = d.len();
            let ev = loop {
                match m::decode_events(&d[..end]) {
                    Ok(ev) => break ev,
                    Err(_) if end > 0 => end -= 1,
                    Err(_) => break Vec::new(),
                }
            };
            let note = (d.len() < size).then(|| format!("device shows only the first {} of {size} bytes", d.len()));
            (size as u32, ev.into_iter().map(Into::into).collect(), note)
        }
        Err(e) => (0, Vec::new(), Some(format!("{e:#}"))),
    };
    Ok(OnboardMacro { id, damaged, guid: hex(&name.guid), name: name.name, events, size })
}

pub fn read_device(dev: &Device) -> Result<DeviceMacros> {
    let s = storage(dev)?;
    let macros = ids(dev)?.into_iter().map(|id| read_macro(dev, id)).collect::<Result<_>>()?;
    Ok(DeviceMacros { max_id: s.max_id, max_memory: s.max_memory, available: s.available, recyclable: s.recyclable, macros })
}

pub fn delete(dev: &Device, id: u16) -> Result<()> {
    dev.query(m::DELETE, &id.to_be_bytes())?;
    ensure!(!ids(dev)?.contains(&id), "device kept macro 0x{id:04x}");
    Ok(())
}

/// Write `mac` to the device. With `reuse` the old copy is replaced under the same id, so
/// existing key bindings keep pointing at it; otherwise a random unused id is chosen.
pub fn upload(dev: &Device, mac: &Macro, reuse: Option<u16>) -> Result<u16> {
    let data = mac.encode()?;
    ensure!(!data.is_empty(), "the macro is empty");
    let guid = guid_bytes(&mac.guid)?;
    let existing = ids(dev)?;
    let id = match reuse {
        Some(id) => {
            if existing.contains(&id) {
                delete(dev, id)?;
            }
            id
        }
        None => {
            // `max_id` is the macro-count limit; ids themselves are random u16s (as Synapse does).
            let s = storage(dev)?;
            ensure!(existing.len() < s.max_id as usize, "the device already holds its maximum of {} macros", s.max_id);
            let mut id = 0;
            for _ in 0..1000 {
                let r = u16::from_be_bytes(random::<2>()?);
                if r != 0 && r != 0xffff && !existing.contains(&r) {
                    id = r;
                    break;
                }
            }
            ensure!(id != 0, "no free macro id on the device");
            id
        }
    };
    let s = storage(dev)?;
    let need = m::required_memory(data.len());
    if need > s.available {
        bail!("not enough macro memory on the device ({} bytes needed, {} free)", need, s.available);
    }
    dev.query(m::ALLOCATE, &m::encode_allocate(id, data.len() as u32))?;
    let block = m::encode_name_block(&m::NameBlock { guid, items: mac.event_count() as u32, name: mac.name.clone() });
    for (i, chunk) in block.chunks(m::NAME_CHUNK).enumerate() {
        dev.query(m::set_name(chunk.len()), &m::encode_set_name(id, (i * m::NAME_CHUNK) as u16, chunk))?;
    }
    for (i, chunk) in data.chunks(m::DATA_CHUNK).enumerate() {
        dev.query(m::set_memory_data(chunk.len()), &m::encode_set_memory(id, (i * m::DATA_CHUNK) as u32, chunk))?;
    }
    ensure!(matches_device(dev, id, &data).context("read-back")?, "macro read-back differs from what was written");
    ensure!(read_name(dev, id)?.guid == guid, "macro name block read-back differs");
    Ok(id)
}

/// Rewrite the name block of on-board macro `id` (GUID, item count, name) without touching its
/// events, so a renamed library macro shows its new name on the device too.
pub fn write_name(dev: &Device, id: u16, mac: &Macro) -> Result<()> {
    let guid = guid_bytes(&mac.guid)?;
    let block = m::encode_name_block(&m::NameBlock { guid, items: mac.event_count() as u32, name: mac.name.clone() });
    for (i, chunk) in block.chunks(m::NAME_CHUNK).enumerate() {
        dev.query(m::set_name(chunk.len()), &m::encode_set_name(id, (i * m::NAME_CHUNK) as u16, chunk))?;
    }
    ensure!(read_name(dev, id)?.name == mac.name.chars().take(99).collect::<String>() || read_name(dev, id)?.guid == guid, "macro name read-back differs");
    Ok(())
}

/// How a bound macro plays (Synapse's playback options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Playback {
    Once,
    Times,
    WhileHeld,
    Toggle,
    /// Razer's sequence playback (function 15): `[id16]`. What it does exactly still has to be
    /// confirmed on hardware.
    Sequence,
}

/// Assignment function + data binding a key/button to on-board macro `id`.
pub fn binding(id: u16, playback: Playback, count: u8) -> (u8, Vec<u8>) {
    let [hi, lo] = id.to_be_bytes();
    match playback {
        Playback::Once => (3, vec![hi, lo, 1]),
        Playback::Times => (3, vec![hi, lo, count.max(1)]),
        Playback::WhileHeld => (4, vec![hi, lo]),
        Playback::Toggle => (5, vec![hi, lo]),
        Playback::Sequence => (15, vec![hi, lo]),
    }
}

/// The macro id a function/data pair refers to, if it is a macro binding.
pub fn bound_id(function: u8, data: &[u8]) -> Option<u16> {
    if !matches!(function, 3 | 4 | 5 | 15) {
        return None;
    }
    Some(u16::from_be_bytes([*data.first()?, *data.get(1)?]))
}

/// Make sure the device holds the current version of `mac`; returns its on-board id.
pub fn ensure_on_device(dev: &Device, mac: &Macro) -> Result<u16> {
    for id in ids(dev)? {
        if hex(&read_name(dev, id)?.guid) == mac.guid {
            if matches_device(dev, id, &mac.encode()?).unwrap_or(false) {
                return Ok(id);
            }
            return upload(dev, mac, Some(id));
        }
    }
    upload(dev, mac, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(events: Vec<MacroEvent>) -> Macro {
        Macro { guid: "00".repeat(16), name: "M".into(), events, ..Default::default() }
    }

    #[test]
    fn media_and_power_events_round_trip_through_the_device_format() {
        let m = mac(vec![MacroEvent::Media { usage: 0xcd, down: true }, MacroEvent::Media { usage: 0xcd, down: false }, MacroEvent::Power { code: 0x82, down: true }, MacroEvent::Power { code: 0x82, down: false }]);
        let bytes = m.encode().unwrap();
        assert_eq!(bytes, [0x05, 0x00, 0xcd, 0x06, 0x00, 0xcd, 0x03, 0x00, 0x82, 0x04, 0x00, 0x82]);
        let back: Vec<MacroEvent> = m::decode_events(&bytes).unwrap().into_iter().map(Into::into).collect();
        assert_eq!(back, m.events);
        // Anything else in those opcodes stays raw.
        let odd: MacroEvent = m::Event::Raw { op: 3, data: [7, 7] }.into();
        assert_eq!(odd, MacroEvent::Raw { op: 3, data: [7, 7] });
    }

    #[test]
    fn cursor_events_encode_big_endian() {
        let m = mac(vec![MacroEvent::Cursor { x: 1920, y: 1080 }]);
        assert_eq!(m.encode().unwrap(), [0x15, 0x07, 0x80, 0x04, 0x38]);
        assert_eq!(MacroEvent::from(m::decode_events(&m.encode().unwrap()).unwrap()[0]), m.events[0]);
    }

    #[test]
    fn run_command_plays_its_shortcut_slot() {
        // Slot 7 is Shift+F14 (first key of the second modifier group).
        let ev = MacroEvent::Run { command: "konsole".into(), slot: Some(7) }.to_events().unwrap();
        let keys: Vec<(u8, bool)> = ev.iter().filter_map(|e| if let m::Event::Key { hid, down } = e { Some((*hid, *down)) } else { None }).collect();
        assert_eq!(keys, [(0xe1, true), (0x69, true), (0x69, false), (0xe1, false)]);
        let m = mac(vec![MacroEvent::Run { command: "x".into(), slot: Some(0) }]);
        assert_eq!(m.event_count(), ev.len() - 2, "plain F14: no modifier events");
        assert!(MacroEvent::Run { command: "x".into(), slot: None }.to_events().is_err());
        assert!(mac(vec![MacroEvent::Run { command: "x".into(), slot: None }]).encode().is_err(), "can't be uploaded before it has a slot");
        // The slot is saved in the library file, the command stays readable.
        let j = serde_json::to_string(&MacroEvent::Run { command: "x y".into(), slot: Some(3) }).unwrap();
        assert_eq!(j, r#"{"type":"run","command":"x y","slot":3}"#);
        assert_eq!(serde_json::from_str::<MacroEvent>(r#"{"type":"run","command":"c"}"#).unwrap(), MacroEvent::Run { command: "c".into(), slot: None });
    }

    #[test]
    fn sequence_binds_function_15_and_is_found_again() {
        assert_eq!(binding(0x1234, Playback::Sequence, 1), (15, vec![0x12, 0x34]));
        assert_eq!(bound_id(15, &[0x12, 0x34]), Some(0x1234));
        assert_eq!(serde_json::to_string(&Playback::Sequence).unwrap(), "\"sequence\"");
    }

    #[test]
    fn library_duplicates_renames_and_tracks_slots() {
        let mut lib = Library::default();
        let a = lib.upsert(Macro { name: "Open".into(), group: "Work".into(), events: vec![MacroEvent::Run { command: "kate".into(), slot: Some(4) }, MacroEvent::Delay { ms: 5 }], ..Default::default() }).unwrap();
        assert_eq!(lib.run_slots(), vec![4]);
        let b = lib.duplicate(&a, "Open copy").unwrap();
        assert_ne!(a, b);
        let copy = lib.get(&b).unwrap();
        assert_eq!((copy.name.as_str(), copy.group.as_str()), ("Open copy", "Work"));
        assert_eq!(copy.events[0], MacroEvent::Run { command: "kate".into(), slot: None }, "the copy gets its own slot");
        assert_eq!(lib.run_slots(), vec![4]);
        assert_eq!(lib.unique_name("Open", ""), "Open 2");
        assert_eq!(lib.unique_name("Open", &a), "Open");
        assert!(lib.duplicate("nope", "x").is_err());
        // Old files without a group still load, and an empty group isn't written.
        let old: Macro = serde_json::from_str(r#"{"guid": "", "name": "O", "events": []}"#).unwrap();
        assert_eq!(old.group, "");
        assert!(!serde_json::to_string(&old).unwrap().contains("group"));
    }
}
