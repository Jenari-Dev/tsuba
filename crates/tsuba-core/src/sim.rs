//! A simulated BlackWidow V4 Pro / Basilisk V3 Pro for tests: on-board profiles with their
//! key/button tables, DPI stages, scroll flags, polling rate, the command dial and macro memory,
//! answering the same commands (and reply layouts) as the hardware. Nothing here touches a
//! real device. Enabled by the `sim` feature (tsubad's tests use it).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tsuba_hid::{Device, HidError};
use tsuba_proto::{commands as c, decode, macros as m, profiles as p, Command, ProtoError, Status};

type Table = BTreeMap<u8, (u8, Vec<u8>)>;

#[derive(Debug, Clone, Default)]
pub struct SimProfile {
    /// Name block bytes (empty = never named: Get Name fails, like the Basilisk's profile 1).
    pub name: Vec<u8>,
    pub tables: [Table; 2],
    pub dpi_active: u8,
    pub dpi: Vec<(u16, u16)>,
    pub active_dpi: (u16, u16),
    pub free_spin: bool,
    pub accel: bool,
    pub smart_reel: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SimMacro {
    pub name: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct SimState {
    pub mouse: bool,
    pub max: u8,
    pub active: u8,
    pub profiles: BTreeMap<u8, SimProfile>,
    /// Keys/buttons this model has (a factory table for new profiles).
    pub factory: [Table; 2],
    pub polling_period: u8,
    pub knob: Vec<decode::KnobMode>,
    pub macros: BTreeMap<u16, SimMacro>,
    /// Every Set command received: (class, id).
    pub writes: Vec<(u8, u8)>,
    /// Every Get command received.
    pub reads: usize,
    /// Firmware features (debounce, scroll levels, indicators, lift-off…); see [`FeatureSim`].
    pub feat: FeatureSim,
}

impl SimState {
    pub fn assignment_writes(&self) -> usize {
        self.writes.iter().filter(|w| w.0 == 0x02 && (w.1 == 0x0c || w.1 == 0x0d)).count()
    }
}

pub type Sim = Arc<Mutex<SimState>>;

fn unsupported() -> HidError {
    HidError::Proto(ProtoError::Status(Status::Unsupported))
}

fn fail() -> HidError {
    HidError::Proto(ProtoError::Status(Status::Fail))
}

fn be16(a: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([a.get(i).copied().unwrap_or(0), a.get(i + 1).copied().unwrap_or(0)])
}

fn be32(a: &[u8], i: usize) -> u32 {
    (be16(a, i) as u32) << 16 | be16(a, i + 2) as u32
}

/// A simulated device with `slots` on-board profiles (1..=slots) holding the factory tables of
/// `pid` ("028d" keyboard or "00aa" mouse) from [`crate::library::factory_table`].
pub fn new_state(pid: &str, slots: u8) -> SimState {
    let mouse = pid == "00aa";
    let factory = [0u8, 1].map(|l| crate::library::factory_table(pid, l).into_iter().map(|(k, (f, d))| (k, (f, d))).collect::<Table>());
    let mut s = SimState {
        mouse,
        max: 5,
        active: 1,
        factory: factory.clone(),
        polling_period: 1,
        knob: (0..8).map(|i| decode::KnobMode { enabled: true, stage: i, function: i.into(), rgb: [0, 0xff, 0] }).collect(),
        ..Default::default()
    };
    for id in 1..=slots {
        s.profiles.insert(id, s.blank());
    }
    s
}

impl SimState {
    fn blank(&self) -> SimProfile {
        SimProfile {
            tables: self.factory.clone(),
            dpi_active: 1,
            dpi: vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200), (6400, 6400)],
            active_dpi: (400, 400),
            ..Default::default()
        }
    }

    pub fn set_name(&mut self, slot: u8, name: &str) {
        let block = p::encode_name_block(&p::NameBlock { guid: [slot; 16], name: name.into() });
        if let Some(pr) = self.profiles.get_mut(&slot) {
            pr.name = block;
        }
    }

    fn profile(&mut self, id: u8) -> Result<&mut SimProfile, HidError> {
        let id = if id == 0 { self.active } else { id };
        self.profiles.get_mut(&id).ok_or_else(fail)
    }

    pub fn answer(&mut self, cmd: Command, a: &[u8]) -> Result<Vec<u8>, HidError> {
        if cmd.is_get() {
            self.reads += 1;
        } else {
            self.writes.push((cmd.class, cmd.id));
        }
        if let Some(r) = self.answer_features(cmd, a) {
            return r;
        }
        let arg = |i: usize| a.get(i).copied().unwrap_or(0);
        match (cmd.class, cmd.id) {
            // ---- profiles
            (0x05, 0x80) => Ok(vec![self.profiles.len() as u8]),
            (0x05, 0x81) => {
                let mut v = vec![self.profiles.len() as u8];
                v.extend(self.profiles.keys());
                Ok(v)
            }
            (0x05, 0x84) => Ok(vec![self.active]),
            (0x05, 0x8a) => Ok(vec![self.max]),
            (0x05, 0x04) => {
                if !self.profiles.contains_key(&arg(0)) {
                    return Err(fail());
                }
                self.active = arg(0);
                Ok(vec![arg(0)])
            }
            (0x05, 0x02) => {
                if arg(0) == 0 || arg(0) > self.max || self.profiles.contains_key(&arg(0)) {
                    return Err(fail());
                }
                let b = self.blank();
                self.profiles.insert(arg(0), b);
                Ok(vec![arg(0)])
            }
            (0x05, 0x03) => {
                if arg(0) == 1 || self.profiles.remove(&arg(0)).is_none() {
                    return Err(fail());
                }
                Ok(vec![arg(0)])
            }
            (0x05, 0x88) => {
                let id = arg(0);
                let off = be16(a, 1) as usize;
                let pr = self.profiles.get(&id).ok_or_else(fail)?;
                if pr.name.is_empty() {
                    return Err(fail());
                }
                let mut v = vec![id];
                v.extend((off as u16).to_be_bytes());
                v.extend((pr.name.len() as u16).to_be_bytes());
                v.extend(pr.name.iter().skip(off).take(p::NAME_CHUNK));
                Ok(v)
            }
            (0x05, 0x08) => {
                let off = be16(a, 1) as usize;
                let chunk = a.get(5..).unwrap_or(&[]).to_vec();
                let pr = self.profiles.get_mut(&arg(0)).ok_or_else(fail)?;
                if pr.name.len() != p::NAME_BLOCK {
                    pr.name = vec![0; p::NAME_BLOCK];
                }
                for (i, b) in chunk.iter().enumerate() {
                    if let Some(x) = pr.name.get_mut(off + i) {
                        *x = *b;
                    }
                }
                Ok(a.to_vec())
            }
            // ---- assignments
            (0x02, 0x84) if self.mouse => {
                let keys: Vec<u8> = self.factory[0].keys().copied().collect();
                let mut v = vec![keys.len() as u8];
                v.extend(keys);
                Ok(v)
            }
            (0x02, 0x8c) | (0x02, 0x8d) => {
                if (cmd.id == 0x8c) != self.mouse {
                    return Err(unsupported());
                }
                let (key, layer) = (arg(1), arg(2));
                let pr = self.profile(arg(0))?;
                let (f, d) = pr.tables.get(layer as usize).and_then(|t| t.get(&key)).cloned().ok_or_else(fail)?;
                Ok(decode::encode_assignment(&decode::Assignment { profile: arg(0), key, layer, function: f, data: d }))
            }
            (0x02, 0x0c) | (0x02, 0x0d) => {
                if (cmd.id == 0x0c) != self.mouse {
                    return Err(unsupported());
                }
                let x = decode::assignment(a).ok_or_else(fail)?;
                let pr = self.profile(x.profile)?;
                let t = pr.tables.get_mut(x.layer as usize).ok_or_else(fail)?;
                if !t.contains_key(&x.key) {
                    return Err(fail());
                }
                t.insert(x.key, (x.function, x.data));
                Ok(a.to_vec())
            }
            // ---- performance
            (0x00, 0xc0) => Err(unsupported()),
            (0x00, 0x85) => Ok(vec![self.polling_period]),
            (0x00, 0x05) => {
                self.polling_period = arg(0);
                Ok(a.to_vec())
            }
            (0x00, 0x0e) => Ok(a.to_vec()),
            (0x02, 0x94) | (0x02, 0x96) | (0x02, 0x97) if self.mouse => {
                let pr = self.profile(arg(0))?;
                let v = match cmd.id {
                    0x94 => pr.free_spin,
                    0x96 => pr.accel,
                    _ => pr.smart_reel,
                };
                Ok(vec![arg(0), v as u8])
            }
            (0x02, 0x14) | (0x02, 0x16) | (0x02, 0x17) if self.mouse => {
                let on = arg(1) != 0;
                let pr = self.profile(arg(0))?;
                match cmd.id {
                    0x14 => pr.free_spin = on,
                    0x16 => pr.accel = on,
                    _ => pr.smart_reel = on,
                }
                Ok(a.to_vec())
            }
            (0x04, 0x86) if self.mouse => {
                let pr = self.profile(arg(0))?;
                let mut v = vec![arg(0), pr.dpi_active, pr.dpi.len() as u8];
                for (i, (x, y)) in pr.dpi.iter().enumerate() {
                    v.push(i as u8 + 1);
                    v.extend(x.to_be_bytes());
                    v.extend(y.to_be_bytes());
                    v.extend([0, 0]);
                }
                Ok(v)
            }
            (0x04, 0x06) if self.mouse => {
                let s = decode::dpi_stages(a).ok_or_else(fail)?;
                let pr = self.profile(arg(0))?;
                pr.dpi_active = s.active;
                pr.dpi = s.stages.iter().map(|st| (st.dpi.x, st.dpi.y)).collect();
                // The firmware also moves the active DPI to the active stage.
                pr.active_dpi = pr.dpi.get(s.active as usize - 1).copied().unwrap_or(pr.active_dpi);
                Ok(a.to_vec())
            }
            (0x04, 0x85) if self.mouse => {
                let pr = self.profile(arg(0))?;
                let mut v = vec![arg(0)];
                v.extend(pr.active_dpi.0.to_be_bytes());
                v.extend(pr.active_dpi.1.to_be_bytes());
                v.extend([0, 0]);
                Ok(v)
            }
            (0x04, 0x05) if self.mouse => {
                let d = decode::active_dpi(a).ok_or_else(fail)?;
                self.profile(arg(0))?.active_dpi = (d.x, d.y);
                Ok(a.to_vec())
            }
            // ---- command dial (keyboard)
            (0x03, 0x92) if !self.mouse => {
                let mut v = decode::KNOB_LED.to_vec();
                self.knob.iter().for_each(|k| v.extend(decode::encode_knob_mode(k)));
                Ok(v)
            }
            (0x03, 0x12) if !self.mouse => {
                self.knob = decode::knob_configuration(a).ok_or_else(fail)?;
                Ok(a.to_vec())
            }
            (0x03, 0x93) if !self.mouse => {
                let mut v = decode::KNOB_LED.to_vec();
                v.extend(decode::encode_knob_mode(&self.knob[0]));
                Ok(v)
            }
            // ---- macros
            (0x06, 0x8e) => {
                let used: u32 = self.macros.values().map(|x| m::required_memory(x.data.len())).sum();
                let mut v = 100u16.to_be_bytes().to_vec();
                v.extend(300_000u32.to_be_bytes());
                v.extend((300_000 - used).to_be_bytes());
                v.extend(0u32.to_be_bytes());
                Ok(v)
            }
            (0x06, 0x80) => Ok((self.macros.len() as u16).to_be_bytes().to_vec()),
            (0x06, 0x8b) => {
                let off = be16(a, 0) as usize;
                let ids: Vec<u16> = self.macros.keys().skip(off).copied().collect();
                let mut v = (off as u16).to_be_bytes().to_vec();
                v.extend((ids.len() as u16).to_be_bytes());
                ids.iter().take(38).for_each(|id| v.extend(id.to_be_bytes()));
                Ok(v)
            }
            (0x06, 0x8c) => {
                let (id, off) = (be16(a, 0), be16(a, 2) as usize);
                let mac = self.macros.get(&id).ok_or_else(fail)?;
                let mut v = id.to_be_bytes().to_vec();
                v.extend((off as u16).to_be_bytes());
                v.extend((m::NAME_BLOCK as u16).to_be_bytes());
                v.extend(mac.name.iter().skip(off).take(m::NAME_CHUNK));
                Ok(v)
            }
            (0x06, 0x0c) => {
                let (id, off) = (be16(a, 0), be16(a, 2) as usize);
                let mac = self.macros.get_mut(&id).ok_or_else(fail)?;
                for (i, b) in a.get(6..).unwrap_or(&[]).iter().enumerate() {
                    if let Some(x) = mac.name.get_mut(off + i) {
                        *x = *b;
                    }
                }
                Ok(a.to_vec())
            }
            (0x06, 0x88) => {
                let id = be16(a, 0);
                let mac = self.macros.get(&id).ok_or_else(fail)?;
                let mut v = id.to_be_bytes().to_vec();
                v.extend((mac.data.len() as u32).to_be_bytes());
                Ok(v)
            }
            (0x06, 0x89) => {
                let (id, off, n) = (be16(a, 0), be32(a, 2) as usize, arg(6) as usize);
                let mac = self.macros.get(&id).ok_or_else(fail)?;
                let chunk: Vec<u8> = mac.data.iter().skip(off).take(n).copied().collect();
                let mut v = a[..6].to_vec();
                v.push(chunk.len() as u8);
                v.extend(chunk);
                Ok(v)
            }
            (0x06, 0x08) => {
                let (id, size) = (be16(a, 0), be32(a, 2) as usize);
                self.macros.insert(id, SimMacro { name: vec![0; m::NAME_BLOCK], data: vec![0; size] });
                Ok(a.to_vec())
            }
            (0x06, 0x09) => {
                let (id, off, n) = (be16(a, 0), be32(a, 2) as usize, arg(6) as usize);
                let mac = self.macros.get_mut(&id).ok_or_else(fail)?;
                for (i, b) in a.get(7..7 + n).unwrap_or(&[]).iter().enumerate() {
                    if let Some(x) = mac.data.get_mut(off + i) {
                        *x = *b;
                    }
                }
                Ok(a.to_vec())
            }
            (0x06, 0x03) => {
                self.macros.remove(&be16(a, 0)).ok_or_else(fail)?;
                Ok(a.to_vec())
            }
            _ => {
                let _ = c::GET_FIRMWARE_VERSION;
                Err(unsupported())
            }
        }
    }
}

/// A [`Device`] backed by `sim`, with writes enabled.
pub fn device(sim: &Sim) -> Device {
    let s = sim.clone();
    let mut d = Device::simulated(Arc::new(move |cmd, args| s.lock().unwrap().answer(cmd, args))).expect("open /dev/null");
    d.allow_writes(true);
    d
}

/// Simulated firmware features: the keyboard has key debounce, a startup animation and an
/// indicator LED switch; the mouse has scroll acceleration / Smart-Reel levels, profile
/// colours, the lift-off sensor and a wireless-connection status. Everything else answers
/// "not supported", as unknown commands do on the hardware.
#[derive(Debug, Clone)]
pub struct FeatureSim {
    pub debounce: [u8; 4],
    pub profile_debounce: BTreeMap<u8, [u8; 4]>,
    pub startup_disabled: u8,
    pub indicator: u8,
    pub accel_level: BTreeMap<u8, u8>,
    pub reel_threshold: BTreeMap<u8, u8>,
    pub colour: BTreeMap<u8, [u8; 3]>,
    pub prox_state: u8,
    /// Last Set Proximity Sensor Lift Setting (mode, height).
    pub lift: Option<(u8, u8)>,
    /// Slots reset with Set Profile Reset, in order.
    pub resets: Vec<u8>,
    /// Lighting per region id: (stored effect payload as sent, brightness 0–255). Mouse:
    /// scroll wheel 1, logo 4, underglow 10; keyboard: backlight 5.
    pub lighting: BTreeMap<u8, (Vec<u8>, u8)>,
    /// Get Device Charging Status answer (None = not supported).
    pub charging: Option<u8>,
}

impl Default for FeatureSim {
    fn default() -> Self {
        FeatureSim {
            debounce: [0, 20, 0, 40],
            profile_debounce: BTreeMap::new(),
            startup_disabled: 0,
            indicator: 1,
            accel_level: BTreeMap::new(),
            reel_threshold: BTreeMap::new(),
            colour: BTreeMap::new(),
            prox_state: 1,
            lift: None,
            resets: Vec::new(),
            lighting: BTreeMap::new(),
            charging: None,
        }
    }
}

impl SimState {
    fn answer_features(&mut self, cmd: Command, a: &[u8]) -> Option<Result<Vec<u8>, HidError>> {
        let arg = |i: usize| a.get(i).copied().unwrap_or(0);
        let kb = !self.mouse;
        let slot = |s: &SimState, p: u8| -> Result<u8, HidError> {
            let id = if p == 0 { s.active } else { p };
            if s.profiles.contains_key(&id) { Ok(id) } else { Err(fail()) }
        };
        let f = &mut self.feat;
        Some(match (cmd.class, cmd.id) {
            (0x02, 0x82) if kb => Ok(f.debounce.to_vec()),
            (0x02, 0x02) if kb => {
                f.debounce.copy_from_slice(a.get(..4).unwrap_or(&[0; 4]));
                Ok(a.to_vec())
            }
            (0x02, 0x95) if kb => {
                let d = f.profile_debounce.get(&arg(0)).copied().unwrap_or([0, 20, 0, 40]);
                let mut v = vec![arg(0)];
                v.extend(d);
                Ok(v)
            }
            (0x02, 0x15) if kb => {
                let mut d = [0; 4];
                d.copy_from_slice(a.get(1..5).unwrap_or(&[0; 4]));
                f.profile_debounce.insert(arg(0), d);
                Ok(a.to_vec())
            }
            (0x0f, 0x98) if kb => Ok(vec![f.startup_disabled]),
            (0x0f, 0x18) if kb => {
                f.startup_disabled = arg(1);
                Ok(a.to_vec())
            }
            (0x07, 0x90) if kb => Ok(vec![f.indicator]),
            (0x07, 0x10) if kb => {
                f.indicator = arg(0);
                Ok(a.to_vec())
            }
            (0x02, 0xa2) | (0x02, 0xa3) | (0x05, 0x8c) if self.mouse => {
                let id = match slot(self, arg(0)) {
                    Ok(id) => id,
                    Err(e) => return Some(Err(e)),
                };
                let f = &self.feat;
                Ok(match cmd.id {
                    0xa2 => vec![arg(0), f.accel_level.get(&id).copied().unwrap_or(1)],
                    0xa3 => vec![arg(0), f.reel_threshold.get(&id).copied().unwrap_or(1)],
                    _ => {
                        let mut v = vec![arg(0)];
                        v.extend(f.colour.get(&id).copied().unwrap_or([0, 0xff, 0]));
                        v
                    }
                })
            }
            (0x02, 0x22) | (0x02, 0x23) | (0x05, 0x0c) if self.mouse => {
                let id = match slot(self, arg(0)) {
                    Ok(id) => id,
                    Err(e) => return Some(Err(e)),
                };
                let f = &mut self.feat;
                match cmd.id {
                    0x22 => {
                        f.accel_level.insert(id, arg(1));
                    }
                    0x23 => {
                        f.reel_threshold.insert(id, arg(1));
                    }
                    _ => {
                        f.colour.insert(id, [arg(1), arg(2), arg(3)]);
                    }
                }
                Ok(a.to_vec())
            }
            (0x0b, 0x80) if self.mouse => Ok(vec![1, c::PROXIMITY_SENSOR]),
            (0x0b, 0x83) if self.mouse && a.get(..2) == Some(&[c::PROXIMITY_CLASS, c::PROXIMITY_SENSOR]) => Ok(vec![arg(0), arg(1), f.prox_state]),
            (0x0b, 0x85) if self.mouse => Ok(vec![arg(0), arg(1), 5, 3, 0, 0, 0, 0, 0, 0]),
            (0x0b, 0x03) if self.mouse => {
                f.prox_state = arg(2);
                Ok(a.to_vec())
            }
            (0x0b, 0x0b) if self.mouse => {
                f.lift = Some((arg(2), arg(3)));
                Ok(a.to_vec())
            }
            (0x00, 0xb8) if self.mouse => Ok(vec![0]),
            (0x07, 0x84) if f.charging.is_some() => Ok(vec![0, f.charging.unwrap_or(0)]),
            // ---- lighting (class 0x0f): regions, effects, brightness
            (0x0f, 0x80) => {
                let ids: &[(u8, u8)] = if self.mouse { &[(1, 1), (4, 1), (10, 11)] } else { &[(5, 24)] };
                Ok(ids.iter().flat_map(|&(id, cols)| [id, 25, 3, if cols == 24 { 9 } else { 1 }, cols]).collect())
            }
            (0x0f, 0x81) => {
                let mut v = vec![arg(0)];
                v.extend(if self.mouse { &[0u8, 1, 2, 3, 5, 8][..] } else { &[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 10][..] });
                Ok(v)
            }
            (0x0f, 0x82) => Ok(f.lighting.get(&arg(1)).map(|l| l.0.clone()).unwrap_or_else(|| vec![1, arg(1), 3, 0, 0, 0])),
            (0x0f, 0x02) => {
                let e = f.lighting.entry(arg(1)).or_insert((vec![], 255));
                e.0 = a.to_vec();
                Ok(a.to_vec())
            }
            (0x0f, 0x84) => Ok(vec![arg(0), arg(1), f.lighting.get(&arg(1)).map_or(255, |l| l.1)]),
            (0x0f, 0x04) => {
                let e = f.lighting.entry(arg(1)).or_insert((vec![1, arg(1), 3, 0, 0, 0], 255));
                e.1 = arg(2);
                Ok(a.to_vec())
            }
            (0x05, 0x0b) => {
                if !self.profiles.contains_key(&arg(0)) {
                    return Some(Err(fail()));
                }
                let b = self.blank();
                self.profiles.insert(arg(0), b);
                self.feat.resets.push(arg(0));
                Ok(a.to_vec())
            }
            (0x00, 0x0b) => {
                if arg(0) == 1 {
                    let b = self.blank();
                    self.profiles = BTreeMap::from([(1, b)]);
                    self.active = 1;
                    self.macros.clear();
                    self.feat = FeatureSim::default();
                }
                Ok(a.to_vec())
            }
            _ => return None,
        })
    }
}
