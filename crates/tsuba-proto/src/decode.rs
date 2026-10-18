//! Payload decoders for protocol-2.5 replies. Layouts were recovered from Synapse 4
//! and checked against live replies from a BlackWidow V4 Pro and a Basilisk V3 Pro.

/// Variable store selector used by DPI and other profile-scoped commands.
pub const VARSTORE: u8 = 0x01;

/// LED class/id that addresses the BlackWidow V4 Pro command dial.
pub const KNOB_LED: [u8; 2] = [0x01, 0x24];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub major: u8,
    pub minor: u8,
}

pub fn firmware_version(a: &[u8]) -> Option<FirmwareVersion> {
    Some(FirmwareVersion { major: *a.first()?, minor: *a.get(1)? })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMode {
    Normal,
    Factory,
    Driver,
    Other(u8),
}

pub fn device_mode(a: &[u8]) -> Option<DeviceMode> {
    Some(match *a.first()? {
        0x00 => DeviceMode::Normal,
        0x02 => DeviceMode::Factory,
        0x03 => DeviceMode::Driver,
        x => DeviceMode::Other(x),
    })
}

/// Polling period in milliseconds (1 = 1000 Hz, 2 = 500 Hz, 8 = 125 Hz).
pub fn polling_hz(a: &[u8]) -> Option<u32> {
    let p = *a.first()? as u32;
    (p > 0).then(|| 1000 / p)
}

/// Polling period byte (standard devices) for a rate in Hz, as Synapse encodes it.
pub fn polling_period(hz: u32) -> Option<u8> {
    Some(match hz {
        1000 => 1,
        500 => 2,
        333 => 3,
        250 => 4,
        200 => 5,
        125 => 8,
        100 => 10,
        _ => return None,
    })
}

/// HyperPolling rate bit ↔ Hz (8000 = 0x01 … 125 = 0x40).
pub const HS_POLLING: [(u32, u8); 7] = [(8000, 0x01), (4000, 0x02), (2000, 0x04), (1000, 0x08), (500, 0x10), (250, 0x20), (125, 0x40)];

pub fn hs_polling_hz(bit: u8) -> Option<u32> {
    HS_POLLING.iter().find(|e| e.1 == bit).map(|e| e.0)
}

pub fn hs_polling_bit(hz: u32) -> Option<u8> {
    HS_POLLING.iter().find(|e| e.0 == hz).map(|e| e.1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpi {
    pub x: u16,
    pub y: u16,
}

/// Get Profile Active DPI Setting: `[varstore, x_hi, x_lo, y_hi, y_lo, 0, 0]`.
pub fn active_dpi(a: &[u8]) -> Option<Dpi> {
    Some(Dpi { x: be16(a, 1)?, y: be16(a, 3)? })
}

/// Payload for Set Profile Active DPI Setting.
/// Payload for Set Profile Active DPI Setting on `profile` (0 = live, 1–5 stored).
pub fn encode_active_dpi_for(profile: u8, d: Dpi) -> Vec<u8> {
    let mut v = encode_active_dpi(d);
    v[0] = profile;
    v
}

pub fn encode_active_dpi(d: Dpi) -> Vec<u8> {
    let [xh, xl] = d.x.to_be_bytes();
    let [yh, yl] = d.y.to_be_bytes();
    vec![VARSTORE, xh, xl, yh, yl, 0, 0]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiStage {
    pub id: u8,
    pub dpi: Dpi,
    /// Third 16-bit field per stage; zero on every device seen so far. Preserved on write.
    pub extra: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiStages {
    pub active: u8,
    pub stages: Vec<DpiStage>,
}

/// Get Profile DPI Class Data: `[varstore, active, count, count × {id, x16, y16, extra16}]`.
pub fn dpi_stages(a: &[u8]) -> Option<DpiStages> {
    let active = *a.get(1)?;
    let count = *a.get(2)? as usize;
    let stages = (0..count)
        .map(|i| {
            let o = 3 + i * 7;
            Some(DpiStage { id: *a.get(o)?, dpi: Dpi { x: be16(a, o + 1)?, y: be16(a, o + 3)? }, extra: be16(a, o + 5)? })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(DpiStages { active, stages })
}

/// Payload for Set Profile DPI Class Data. The data size is `7 × count + 3`.
/// Payload for Set Profile DPI Class Data on `profile`.
pub fn encode_dpi_stages_for(profile: u8, s: &DpiStages) -> Vec<u8> {
    let mut v = encode_dpi_stages(s);
    v[0] = profile;
    v
}

pub fn encode_dpi_stages(s: &DpiStages) -> Vec<u8> {
    let mut v = vec![VARSTORE, s.active, s.stages.len() as u8];
    for st in &s.stages {
        v.push(st.id);
        for w in [st.dpi.x, st.dpi.y, st.extra] {
            v.extend_from_slice(&w.to_be_bytes());
        }
    }
    v
}

/// Command-dial functions (Synapse `modeIdEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Function bytes verified against the keyboard (slot colours) and Synapse's own mode ids.
pub enum KnobFunction {
    KeyboardBrightness,
    WindowsZoom,
    SwitchApplications,
    TrackJogging,
    TrackSelector,
    VerticalScrolling,
    HorizontalScrolling,
    SwitchBrowserTabs,
    Other(u8),
}

impl From<u8> for KnobFunction {
    fn from(b: u8) -> Self {
        use KnobFunction::*;
        match b {
            0 => KeyboardBrightness,
            1 => WindowsZoom,
            2 => SwitchApplications,
            3 => TrackJogging,
            4 => TrackSelector,
            5 => VerticalScrolling,
            6 => HorizontalScrolling,
            7 => SwitchBrowserTabs,
            x => Other(x),
        }
    }
}

impl From<KnobFunction> for u8 {
    fn from(f: KnobFunction) -> u8 {
        use KnobFunction::*;
        match f {
            KeyboardBrightness => 0,
            WindowsZoom => 1,
            SwitchApplications => 2,
            TrackJogging => 3,
            TrackSelector => 4,
            VerticalScrolling => 5,
            HorizontalScrolling => 6,
            SwitchBrowserTabs => 7,
            Other(x) => x,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnobMode {
    pub enabled: bool,
    pub stage: u8,
    pub function: KnobFunction,
    pub rgb: [u8; 3],
}

fn knob_mode(e: &[u8]) -> Option<KnobMode> {
    Some(KnobMode {
        enabled: *e.first()? == 0,
        stage: *e.get(1)?,
        function: (*e.get(2)?).into(),
        rgb: e.get(3..6)?.try_into().ok()?,
    })
}

pub fn encode_knob_mode(k: &KnobMode) -> [u8; 6] {
    let [r, g, b] = k.rgb;
    [u8::from(!k.enabled), k.stage, k.function.into(), r, g, b]
}

/// Payload for Set Knob Configuration Information (same layout as the Get reply).
pub fn encode_knob_configuration(modes: &[KnobMode]) -> Vec<u8> {
    let mut v = KNOB_LED.to_vec();
    modes.iter().for_each(|k| v.extend_from_slice(&encode_knob_mode(k)));
    v
}

/// Get Knob Configuration Information: `[led class, led id, 8 × {disabled, stage, mode, R, G, B}]`.
pub fn knob_configuration(a: &[u8]) -> Option<Vec<KnobMode>> {
    a.get(2..50)?.chunks_exact(6).map(knob_mode).collect()
}

/// Get Current Knob Mode: `[led class, led id, disabled, stage, mode, R, G, B]`.
pub fn current_knob_mode(a: &[u8]) -> Option<KnobMode> {
    knob_mode(a.get(2..8)?)
}

/// Firmware function type of a key/button assignment (Synapse `fnIdEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    Off,
    MouseButton,
    Key,
    MacroTypeI,
    MacroTypeII,
    MacroTypeIII,
    Dpi,
    Profile,
    Lighting,
    PowerKeys,
    MediaKeys,
    DoubleClick,
    ModeButton,
    TurboKey,
    TurboButton,
    MacroTypeIV,
    Controller,
    RazerKey,
    ScrollWheelMode,
    Win8Shortcut,
    Other(u8),
}

impl From<u8> for Function {
    fn from(b: u8) -> Self {
        use Function::*;
        match b {
            0 => Off,
            1 => MouseButton,
            2 => Key,
            3 => MacroTypeI,
            4 => MacroTypeII,
            5 => MacroTypeIII,
            6 => Dpi,
            7 => Profile,
            8 => Lighting,
            9 => PowerKeys,
            10 => MediaKeys,
            11 => DoubleClick,
            12 => ModeButton,
            13 => TurboKey,
            14 => TurboButton,
            15 => MacroTypeIV,
            16 => Controller,
            17 => RazerKey,
            18 => ScrollWheelMode,
            21 => Win8Shortcut,
            x => Other(x),
        }
    }
}

/// Human-readable meaning of an assignment's function + data, where known.
pub fn describe(function: u8, data: &[u8]) -> String {
    let b = |i: usize| data.get(i).copied().unwrap_or(0);
    let w = |i: usize| u16::from_be_bytes([b(i), b(i + 1)]);
    match Function::from(function) {
        Function::Off => "disabled".into(),
        Function::MouseButton => format!("mouse button {}", b(0)),
        Function::Key => format!("key HID 0x{:02x}{}", b(1), if b(0) != 0 { format!(" + modifiers 0x{:02x}", b(0)) } else { String::new() }),
        Function::Dpi => match b(0) {
            1 => "DPI up".into(),
            2 => "DPI down".into(),
            5 => format!("DPI clutch {} x {}", w(1), w(3)),
            6 => "DPI cycle up".into(),
            7 => "DPI cycle down".into(),
            x => format!("DPI action {x}"),
        },
        Function::ScrollWheelMode => "scroll-wheel mode switch".into(),
        Function::MacroTypeI if b(2) <= 1 => format!("macro 0x{:04x} once", w(0)),
        Function::MacroTypeI => format!("macro 0x{:04x} x{}", w(0), b(2)),
        Function::MacroTypeII => format!("macro 0x{:04x} while held", w(0)),
        Function::MacroTypeIII => format!("macro 0x{:04x} toggle", w(0)),
        Function::MacroTypeIV => format!("macro 0x{:04x} sequence", w(0)),
        f => format!("{f:?} {data:02x?}"),
    }
}

/// A key (keyboard) or button (mouse) assignment on one onboard profile and layer.
/// `key` is the AT-101 key position number on keyboards, the button number on mice.
/// `layer` 0 = normal, 1 = Hypershift. `function` 0x01 = mouse button, 0x02 = keyboard
/// key (data = modifier, HID usage); other function ids are still being mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub profile: u8,
    pub key: u8,
    pub layer: u8,
    pub function: u8,
    pub data: Vec<u8>,
}

/// Get Single Key/Button Assignment reply: `[profile, key, layer, function, size, data…]`.
pub fn assignment(a: &[u8]) -> Option<Assignment> {
    let n = *a.get(4)? as usize;
    Some(Assignment {
        profile: *a.first()?,
        key: *a.get(1)?,
        layer: *a.get(2)?,
        function: *a.get(3)?,
        data: a.get(5..5 + n)?.to_vec(),
    })
}

/// Payload for Set Single Key/Button Assignment. Synapse sends data size 10.
pub fn encode_assignment(x: &Assignment) -> Vec<u8> {
    let mut v = vec![x.profile, x.key, x.layer, x.function, x.data.len() as u8];
    v.extend_from_slice(&x.data);
    v
}

fn be16(a: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*a.get(o)?, *a.get(o + 1)?]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_stages_live() {
        // Basilisk V3 Pro reply.
        let a = [
            0x01, 0x04, 0x05, 0x01, 0x0b, 0xb8, 0x0b, 0xb8, 0, 0, 0x02, 0x0d, 0xac, 0x0d, 0xac, 0, 0,
            0x03, 0x0f, 0xa0, 0x0f, 0xa0, 0, 0, 0x04, 0x13, 0x88, 0x13, 0x88, 0, 0, 0x05, 0x17, 0x70,
            0x17, 0x70, 0, 0,
        ];
        let s = dpi_stages(&a).unwrap();
        assert_eq!(s.active, 4);
        let xs: Vec<u16> = s.stages.iter().map(|st| st.dpi.x).collect();
        assert_eq!(xs, [3000, 3500, 4000, 5000, 6000]);
        assert_eq!(encode_dpi_stages(&s), a);
        // Get Profile Active DPI reply for 1800 x 1800.
        let active = [0x01, 0x07, 0x08, 0x07, 0x08, 0x00, 0x00];
        assert_eq!(encode_active_dpi(active_dpi(&active).unwrap()), active);
    }

    #[test]
    fn knob_live() {
        // BlackWidow V4 Pro reply; identical bytes appear in Synapse's own log.
        let a = [
            0x01, 0x24, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x01, 0x01, 0xff, 0x00, 0x00, 0x00,
            0x02, 0x02, 0x00, 0x00, 0xff, 0x00, 0x03, 0x03, 0xfe, 0xed, 0x03, 0x01, 0x04, 0x04, 0x00,
            0xff, 0xff, 0x01, 0x05, 0x05, 0xff, 0x00, 0xff, 0x01, 0x06, 0x06, 0xff, 0xff, 0xff, 0x01,
            0x07, 0x07, 0xfd, 0x86, 0x11,
        ];
        let modes = knob_configuration(&a).unwrap();
        assert_eq!(modes.len(), 8);
        assert_eq!(modes.iter().filter(|m| m.enabled).count(), 4);
        assert_eq!(modes[3].function, KnobFunction::TrackJogging);
        assert_eq!(modes[3].rgb, [0xfe, 0xed, 0x03]);
        let cur = current_knob_mode(&[0x01, 0x24, 0x00, 0x03, 0x03, 0xfe, 0xed, 0x03]).unwrap();
        assert_eq!(cur, modes[3]);
        assert_eq!(encode_knob_configuration(&modes), a);
    }

    #[test]
    fn assignment_roundtrip() {
        // BlackWidow V4 Pro: key position 4 ("3") -> keyboard function, HID usage 0x20.
        let a = [0x01, 0x04, 0x00, 0x02, 0x02, 0x00, 0x20];
        let x = assignment(&a).unwrap();
        assert_eq!((x.key, x.function, x.data.as_slice()), (4, 2, &[0x00, 0x20][..]));
        assert_eq!(encode_assignment(&x), a);
    }
}
