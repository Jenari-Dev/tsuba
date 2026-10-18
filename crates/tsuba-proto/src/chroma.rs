//! Chroma lighting (class 0x0f): regions, hardware effects, brightness and
//! custom frames. Layouts from Synapse 4; effect parameters match openrazer's
//! extended-matrix encoders. Software effects stream custom frames row by row.

use crate::Command;

pub const GET_REGION_LIST: Command = Command::new(80, 0x0f, 0x80);
pub const GET_EFFECT_LIST: Command = Command::new(80, 0x0f, 0x81);
pub const GET_EFFECT: Command = Command::new(80, 0x0f, 0x82);
pub const GET_BRIGHTNESS: Command = Command::new(3, 0x0f, 0x84);
pub const SET_BRIGHTNESS: Command = Command::new(3, 0x0f, 0x04);

/// Set Chroma Effect; data size = 6 + 3 × colours.
pub const fn set_effect(colours: usize) -> Command {
    Command::new((6 + 3 * colours) as u8, 0x0f, 0x02)
}

/// Set Chroma Frame for one row segment; data size = 5 + 3 × LEDs.
pub const fn set_frame(leds: usize) -> Command {
    Command::new((5 + 3 * leds) as u8, 0x0f, 0x03)
}

/// Variable store: 0x01 persists in the device, 0x00 does not (used for streaming).
pub const STORE: u8 = 0x01;
pub const NOSTORE: u8 = 0x00;

pub type Rgb = [u8; 3];

/// Lighting regions (Synapse `ledId` enum, subset).
pub fn region_name(id: u8) -> &'static str {
    match id {
        1 => "scroll wheel",
        2 => "DPI",
        3 => "battery",
        4 => "logo",
        5 => "backlight",
        10 => "underglow",
        11 => "side stripe",
        16 => "right side",
        17 => "left side",
        36 => "command dial",
        _ => "region",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub id: u8,
    /// Frame update rate the device advertises (Hz).
    pub update_rate: u8,
    pub data_type: u8,
    pub rows: u8,
    pub cols: u8,
}

/// Get Chroma Region ID List: 5 bytes per region `{id, rate, data type, rows, cols}`.
pub fn regions(a: &[u8]) -> Vec<Region> {
    a.chunks_exact(5)
        .map(|c| Region { id: c[0], update_rate: c[1], data_type: c[2], rows: c[3], cols: c[4] })
        .collect()
}

/// Firmware effect ids (Synapse `EFX_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectId {
    Off,
    Static,
    Breathing,
    Spectrum,
    Wave,
    Reactive,
    Ripple,
    Starlight,
    CustomFrame,
    Fire,
    AudioMeter,
    Wheel,
    Ambient,
    Other(u8),
}

impl From<u8> for EffectId {
    fn from(b: u8) -> Self {
        use EffectId::*;
        match b {
            0 => Off,
            1 => Static,
            2 => Breathing,
            3 => Spectrum,
            4 => Wave,
            5 => Reactive,
            6 => Ripple,
            7 => Starlight,
            8 => CustomFrame,
            9 => Fire,
            10 => AudioMeter,
            12 => Wheel,
            13 => Ambient,
            x => Other(x),
        }
    }
}

/// Get Chroma Effect ID List reply: `[region, effect ids…]`.
pub fn effect_list(a: &[u8]) -> Option<(u8, Vec<EffectId>)> {
    let (&region, ids) = a.split_first()?;
    Some((region, ids.iter().map(|&b| b.into()).collect()))
}

/// Get Chroma Effect reply: `[store, region, effect, p1, p2, colours, rgb…]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectState {
    pub store: u8,
    pub region: u8,
    pub effect: EffectId,
    pub p1: u8,
    pub p2: u8,
    pub colours: Vec<Rgb>,
}

pub fn effect_state(a: &[u8]) -> Option<EffectState> {
    let n = *a.get(5)? as usize;
    let colours = (0..n).map(|i| a.get(6 + 3 * i..9 + 3 * i)?.try_into().ok()).collect::<Option<Vec<Rgb>>>()?;
    Some(EffectState { store: a[0], region: a[1], effect: a[2].into(), p1: a[3], p2: a[4], colours })
}

/// A hardware effect to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Off,
    Static(Rgb),
    /// 0 colours = random, 1 = single, 2 = dual.
    Breathing(Vec<Rgb>),
    Spectrum,
    /// direction 1 or 2; speed is a frame period (openrazer default 0x28).
    Wave { direction: u8, speed: u8 },
    /// speed 1 (fast) .. 4 (slow).
    Reactive { speed: u8, colour: Rgb },
    /// speed 1..3; 0 colours = random, 1 or 2 = fixed.
    Starlight { speed: u8, colours: Vec<Rgb> },
    /// Rings from each pressed key (keyboard firmware); 0 colours = random, 1 = fixed.
    Ripple { speed: u8, colours: Vec<Rgb> },
    CustomFrame,
}

/// Payload for Set Chroma Effect: `[store, region, effect, p1, p2, n, rgb…]`.
pub fn encode_effect(store: u8, region: u8, e: &Effect) -> Vec<u8> {
    let (id, p1, p2, colours): (u8, u8, u8, Vec<Rgb>) = match e {
        Effect::Off => (0, 0, 0, vec![]),
        Effect::Static(c) => (1, 0, 0, vec![*c]),
        Effect::Breathing(cs) => (2, cs.len() as u8, 0, cs.clone()),
        Effect::Spectrum => (3, 0, 0, vec![]),
        Effect::Wave { direction, speed } => (4, *direction, *speed, vec![]),
        Effect::Reactive { speed, colour } => (5, 0, *speed, vec![*colour]),
        Effect::Starlight { speed, colours } => (7, 0, *speed, colours.clone()),
        Effect::Ripple { speed, colours } => (6, 0, *speed, colours.clone()),
        Effect::CustomFrame => (8, 0, 0, vec![]),
    };
    let mut v = vec![store, region, id, p1, p2, colours.len() as u8];
    colours.iter().for_each(|c| v.extend_from_slice(c));
    v
}

/// Re-encode a read-back effect state (used to restore exactly what was there).
pub fn encode_effect_state(s: &EffectState) -> Vec<u8> {
    let id = match s.effect {
        EffectId::Off => 0,
        EffectId::Static => 1,
        EffectId::Breathing => 2,
        EffectId::Spectrum => 3,
        EffectId::Wave => 4,
        EffectId::Reactive => 5,
        EffectId::Ripple => 6,
        EffectId::Starlight => 7,
        EffectId::CustomFrame => 8,
        EffectId::Fire => 9,
        EffectId::AudioMeter => 10,
        EffectId::Wheel => 12,
        EffectId::Ambient => 13,
        EffectId::Other(x) => x,
    };
    let mut v = vec![s.store, s.region, id, s.p1, s.p2, s.colours.len() as u8];
    s.colours.iter().for_each(|c| v.extend_from_slice(c));
    v
}

/// Payload for Set Chroma Frame: `[0, 0, row, first col, last col, rgb…]`.
pub fn encode_frame_row(row: u8, first_col: u8, leds: &[Rgb]) -> Vec<u8> {
    let last = first_col + leds.len().saturating_sub(1) as u8;
    let mut v = vec![0, 0, row, first_col, last];
    leds.iter().for_each(|c| v.extend_from_slice(c));
    v
}

/// Payload for Set Chroma Brightness: `[store, region, 0..255]`.
pub fn encode_brightness(store: u8, region: u8, level: u8) -> Vec<u8> {
    vec![store, region, level]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_regions_and_state() {
        // Basilisk V3 Pro region list and the keyboard's current effect (Spectrum).
        let r = regions(&[1, 25, 3, 1, 1, 4, 25, 3, 1, 1, 10, 25, 3, 1, 11]);
        assert_eq!(r.iter().map(|x| (x.id, x.cols)).collect::<Vec<_>>(), [(1, 1), (4, 1), (10, 11)]);
        let s = effect_state(&[1, 5, 3, 0, 0, 0]).unwrap();
        assert_eq!((s.region, s.effect), (5, EffectId::Spectrum));
        assert_eq!(encode_effect(1, 5, &Effect::Spectrum), [1, 5, 3, 0, 0, 0]);
        assert_eq!(encode_effect_state(&s), [1, 5, 3, 0, 0, 0]);
    }

    #[test]
    fn effect_payloads_match_openrazer() {
        assert_eq!(encode_effect(1, 5, &Effect::Static([0xff, 0, 0])), [1, 5, 1, 0, 0, 1, 0xff, 0, 0]);
        assert_eq!(encode_effect(1, 5, &Effect::Wave { direction: 1, speed: 0x28 }), [1, 5, 4, 1, 0x28, 0]);
        assert_eq!(
            encode_effect(1, 5, &Effect::Breathing(vec![[1, 2, 3], [4, 5, 6]])),
            [1, 5, 2, 2, 0, 2, 1, 2, 3, 4, 5, 6]
        );
        assert_eq!(encode_frame_row(2, 0, &[[9, 9, 9], [1, 1, 1]]), [0, 0, 2, 0, 1, 9, 9, 9, 1, 1, 1]);
    }
}
