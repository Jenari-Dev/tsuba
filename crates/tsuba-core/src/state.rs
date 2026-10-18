//! Read-only snapshot of everything Tsuba manages on a device.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::{Device, Kind, Model};
use tsuba_proto::{chroma, commands as c, decode};

use crate::{hex_rgb, level_to_percent, pid_key};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceState {
    pub pid: String,
    pub name: String,
    pub kind: String,
    pub firmware: String,
    pub mode: String,
    pub lighting: Vec<RegionState>,
    pub dpi: Option<DpiState>,
    pub dial: Option<Vec<DialSlot>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RegionState {
    pub id: u8,
    pub name: String,
    pub rows: u8,
    pub cols: u8,
    pub effect: String,
    pub colours: Vec<String>,
    pub brightness: u8,
    pub supported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DpiState {
    pub active: [u16; 2],
    pub active_stage: u8,
    pub stages: Vec<[u16; 2]>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DialSlot {
    pub slot: u8,
    pub function: String,
    pub colour: String,
    pub enabled: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssignmentState {
    /// AT-101 key position (keyboards) or button id (mice).
    pub key: u8,
    pub function: u8,
    pub function_name: String,
    pub data: Vec<u8>,
    /// Human-readable meaning, e.g. "key HID 0x04", "DPI cycle up".
    pub label: String,
}

/// Which command reads one assignment, and the key / button positions to read it for.
pub fn assignment_keys(dev: &Device, model: &Model) -> Result<(tsuba_proto::Command, Vec<u8>)> {
    if model.kind.has_buttons() {
        let a = dev.query(c::GET_BUTTON_ID_LIST, &[])?;
        let n = *a.first().unwrap_or(&0) as usize;
        Ok((c::GET_SINGLE_BUTTON_ASSIGNMENT, a.get(1..1 + n).unwrap_or(&[]).to_vec()))
    } else if model.kind.has_keys() {
        Ok((c::GET_SINGLE_KEY_ASSIGNMENT, (1..=209).collect()))
    } else {
        bail!("a {} has no remappable keys or buttons", model.kind.as_str());
    }
}

/// Read the assignments of `keys` (from [`assignment_keys`]); keys the device doesn't answer for
/// are skipped. An I/O error (device unplugged) fails the read: a table with keys silently
/// missing would be taken for the whole one.
pub fn read_assignment_keys(dev: &Device, get: tsuba_proto::Command, profile: u8, layer: u8, keys: &[u8]) -> Result<Vec<AssignmentState>> {
    let mut out = Vec::new();
    for &k in keys {
        let raw = match dev.query(get, &[profile, k, layer]) {
            Ok(raw) => raw,
            Err(e @ tsuba_hid::HidError::Io { .. }) => return Err(e).with_context(|| format!("reading key {k} of profile {profile}")),
            Err(_) => continue,
        };
        let Some(a) = decode::assignment(&raw) else { continue };
        out.push(assignment_state(a));
    }
    Ok(out)
}

pub fn assignment_state(a: decode::Assignment) -> AssignmentState {
    AssignmentState {
        key: a.key,
        function: a.function,
        function_name: format!("{:?}", decode::Function::from(a.function)),
        label: decode::describe(a.function, &a.data),
        data: a.data,
    }
}

/// Every assignment on one onboard profile and layer (0 = standard, 1 = Hypershift).
/// Mice report their button ids; keyboards and keypads are scanned over positions 1..=209.
pub fn read_assignments(dev: &Device, model: &Model, profile: u8, layer: u8) -> Result<Vec<AssignmentState>> {
    let (get, keys) = assignment_keys(dev, model)?;
    read_assignment_keys(dev, get, profile, layer, &keys)
}

pub fn read_state(dev: &Device, model: &Model) -> Result<DeviceState> {
    let fw = decode::firmware_version(&dev.query(c::GET_FIRMWARE_VERSION, &[])?).context("firmware")?;
    let mode = decode::device_mode(&dev.query(c::GET_DEVICE_MODE, &[])?).context("mode")?;
    Ok(DeviceState {
        pid: pid_key(model.pid),
        name: model.name.into(),
        kind: model.kind.as_str().into(),
        firmware: format!("{}.{:02}", fw.major, fw.minor),
        mode: format!("{mode:?}"),
        // Verified devices must answer; untested ones may use an older lighting protocol.
        lighting: if model.tested() { read_lighting(dev)? } else { read_lighting(dev).unwrap_or_default() },
        dpi: if has_dpi(model) {
            if model.tested() { Some(read_dpi(dev)?) } else { read_dpi(dev).ok() }
        } else {
            None
        },
        dial: if model.spec.features.dial { read_dial(dev).ok() } else { None },
    })
}

/// Mice with adjustable DPI (per the device database).
pub fn has_dpi(model: &Model) -> bool {
    model.kind == Kind::Mouse && (model.spec.features.dpi_stages || model.spec.max_dpi.is_some())
}

pub fn read_lighting(dev: &Device) -> Result<Vec<RegionState>> {
    let mut out = Vec::new();
    for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
        let supported = dev
            .query(chroma::GET_EFFECT_LIST, &[r.id])
            .ok()
            .and_then(|a| chroma::effect_list(&a))
            .map(|(_, e)| e.iter().map(|x| format!("{x:?}")).collect())
            .unwrap_or_default();
        let cur = chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, r.id])?).context("effect")?;
        let level = dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0);
        out.push(RegionState {
            id: r.id,
            name: chroma::region_name(r.id).into(),
            rows: r.rows,
            cols: r.cols,
            effect: format!("{:?}", cur.effect),
            colours: cur.colours.iter().map(|c| hex_rgb(*c)).collect(),
            brightness: level_to_percent(level),
            supported,
        });
    }
    Ok(out)
}

pub fn read_dpi(dev: &Device) -> Result<DpiState> {
    let a = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
    let s = decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[decode::VARSTORE])?).context("stages")?;
    Ok(DpiState {
        active: [a.x, a.y],
        active_stage: s.active,
        stages: s.stages.iter().map(|st| [st.dpi.x, st.dpi.y]).collect(),
    })
}

pub fn read_dial(dev: &Device) -> Result<Vec<DialSlot>> {
    let cur = decode::current_knob_mode(&dev.query(c::GET_CURRENT_KNOB_MODE, &decode::KNOB_LED)?).context("dial mode")?;
    let modes = decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?).context("dial")?;
    Ok(modes
        .iter()
        .map(|k| DialSlot {
            slot: k.stage,
            function: format!("{:?}", k.function),
            colour: hex_rgb(k.rgb),
            enabled: k.enabled,
            current: *k == cur,
        })
        .collect())
}
