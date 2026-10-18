//! Hardware-event input reports (device interface 1). Byte 0 is the record type
//! (Synapse `rzHardwareEventsProtocol25Parser`). Record 1 is the extended keyboard
//! buffer — ordinary typed keys — and is deliberately never decoded or surfaced.

/// Record types this crate decodes.
pub const MEDIA_KEYS: u8 = 2;
pub const POWER_KEYS: u8 = 3;
pub const RAZER_KEYS: u8 = 4;
pub const NOTIFICATION_EVENTS: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Consumer-page usages currently held (empty = release).
    Media(Vec<u16>),
    Power(Vec<u8>),
    /// Razer key codes currently held (empty = release). See [`razer_key_name`].
    RazerKeys(Vec<u8>),
    Notification { id: u8, data: Vec<u8> },
}

/// Decode one input report. Returns `None` for typed keys and unknown records.
pub fn parse(report: &[u8]) -> Option<Event> {
    let (&kind, rest) = report.split_first()?;
    let nonzero = |v: &[u8]| v.iter().copied().filter(|b| *b != 0).collect::<Vec<_>>();
    Some(match kind {
        MEDIA_KEYS => Event::Media(
            rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).filter(|u| *u != 0).collect(),
        ),
        POWER_KEYS => Event::Power(nonzero(rest)),
        RAZER_KEYS => Event::RazerKeys(nonzero(rest)),
        NOTIFICATION_EVENTS => Event::Notification { id: *rest.first()?, data: rest.get(1..)?.to_vec() },
        _ => return None,
    })
}

/// Synapse `rzHardwareEventsNotification` ids (subset relevant to keyboards and mice).
pub fn notification_name(id: u8) -> Option<&'static str> {
    Some(match id {
        1 => "profile change",
        2 => "DPI change",
        3 => "detachment",
        4 => "attachment",
        5 => "lifted",
        6 => "downed",
        8 => "brightness change",
        9 => "wireless change",
        11 => "macro key cover change",
        12 => "external power change",
        13 => "hardware change",
        23 => "auto-sleep mode change",
        24 => "LED effect colour change",
        49 => "battery change",
        53 => "wireless change v2",
        56 => "active connection change",
        57 => "scroll free/notch mode change",
        58 => "haptic scroll stage change",
        65 => "battery fully charged",
        67 => "Chroma button tap",
        70 => "button change",
        83 => "OLED command dial",
        86 => "dongle connection quality",
        88 => "polling rate change",
        89 => "mode switch slider change",
        _ => return None,
    })
}

/// Synapse Razer key codes (`RazerKey` function data / record 4 payload).
pub fn razer_key_name(code: u8) -> String {
    match code {
        1 => "Fn".into(),
        2 => "Razer".into(),
        3 => "Game Mode".into(),
        4 => "Macro Record".into(),
        8 => "Backlight up".into(),
        9 => "Backlight down".into(),
        11 => "Low power toggle".into(),
        32 => "DPI up".into(),
        33 => "DPI down".into(),
        // Synapse lists PrintScreen = 70 inside the MacroKeys 36..=79 range.
        70 => "Print Screen".into(),
        36..=79 => format!("Macro key {}", code - 35),
        80 => "Profile switch".into(),
        81 => "Sniper".into(),
        82 => "DPI cycle up".into(),
        83 => "DPI cycle down".into(),
        84 => "Wheel free/notch switch".into(),
        89 => "Hypershift".into(),
        216 => "Mic mute".into(),
        x => format!("Razer key {x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_keys_are_never_decoded() {
        assert_eq!(parse(&[1, 0, 0, 0x04, 0, 0]), None);
    }

    #[test]
    fn decodes_records() {
        assert_eq!(parse(&[4, 38, 0, 0]), Some(Event::RazerKeys(vec![38])));
        assert_eq!(parse(&[2, 0xe9, 0x00, 0, 0]), Some(Event::Media(vec![0x00e9])));
        assert_eq!(
            parse(&[5, 2, 0x07, 0x08, 0x07, 0x08]),
            Some(Event::Notification { id: 2, data: vec![0x07, 0x08, 0x07, 0x08] })
        );
        assert_eq!(razer_key_name(38), "Macro key 3");
    }
}
