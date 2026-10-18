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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroEvent {
    /// Keyboard key by HID usage (modifiers are 0xe0–0xe7).
    Key { hid: u8, down: bool },
    /// Mouse button 1 left, 2 right, 3 middle, 4, 5.
    Button { button: u8, down: bool },
    Scroll { up: bool },
    Delay { ms: u32 },
    Cursor { x: u16, y: u16 },
    Raw { op: u8, data: [u8; 2] },
}

impl From<m::Event> for MacroEvent {
    fn from(e: m::Event) -> Self {
        match e {
            m::Event::Key { hid, down } => Self::Key { hid, down },
            m::Event::Button { button, down } => Self::Button { button, down },
            m::Event::Scroll { up } => Self::Scroll { up },
            m::Event::Delay { ms } => Self::Delay { ms },
            m::Event::Cursor { x, y } => Self::Cursor { x, y },
            m::Event::Raw { op, data } => Self::Raw { op, data },
        }
    }
}

impl From<MacroEvent> for m::Event {
    fn from(e: MacroEvent) -> Self {
        match e {
            MacroEvent::Key { hid, down } => Self::Key { hid, down },
            MacroEvent::Button { button, down } => Self::Button { button, down },
            MacroEvent::Scroll { up } => Self::Scroll { up },
            MacroEvent::Delay { ms } => Self::Delay { ms },
            MacroEvent::Cursor { x, y } => Self::Cursor { x, y },
            MacroEvent::Raw { op, data } => Self::Raw { op, data },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    /// 32 hex digits; empty when creating a new macro.
    #[serde(default)]
    pub guid: String,
    pub name: String,
    pub events: Vec<MacroEvent>,
}

impl Macro {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let events: Vec<m::Event> = self.events.iter().map(|e| (*e).into()).collect();
        m::encode_events(&events).map_err(anyhow::Error::msg)
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
    let block = m::encode_name_block(&m::NameBlock { guid, items: mac.events.len() as u32, name: mac.name.clone() });
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

/// How a bound macro plays (Synapse's playback options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Playback {
    Once,
    Times,
    WhileHeld,
    Toggle,
}

/// Assignment function + data binding a key/button to on-board macro `id`.
pub fn binding(id: u16, playback: Playback, count: u8) -> (u8, Vec<u8>) {
    let [hi, lo] = id.to_be_bytes();
    match playback {
        Playback::Once => (3, vec![hi, lo, 1]),
        Playback::Times => (3, vec![hi, lo, count.max(1)]),
        Playback::WhileHeld => (4, vec![hi, lo]),
        Playback::Toggle => (5, vec![hi, lo]),
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
