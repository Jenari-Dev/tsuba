//! Key tables for the Synapse import: Synapse's `KEY_*` names and the key codes in its
//! recorded macros, both mapped to HID keyboard usages (page 7).

/// HID usage for a Synapse key name such as `KEY_A`, `KEY_PAGE_DOWN`, `KEY_LEFT_CTRL`
/// (Synapse spells the minus key `KEY_HYPEN`). Matching ignores case and underscores.
pub fn hid_for_name(name: &str) -> Option<u8> {
    let n: String = name.trim_start_matches("KEY_").chars().filter(|c| *c != '_' && *c != ' ').collect::<String>().to_ascii_uppercase();
    let b = n.as_bytes();
    if b.len() == 1 {
        return match b[0] {
            c @ b'A'..=b'Z' => Some(0x04 + (c - b'A')),
            b'0' => Some(0x27),
            c @ b'1'..=b'9' => Some(0x1e + (c - b'1')),
            _ => None,
        };
    }
    if let Some(f) = n.strip_prefix('F').and_then(|x| x.parse::<u8>().ok()) {
        return match f {
            1..=12 => Some(0x3a + f - 1),
            13..=24 => Some(0x68 + f - 13),
            _ => None,
        };
    }
    for p in ["NUMPAD", "NUM", "KP", "KEYPAD"] {
        if let Some(d) = n.strip_prefix(p).and_then(|x| x.parse::<u8>().ok()) {
            return match d {
                0 => Some(0x62),
                1..=9 => Some(0x59 + d - 1),
                _ => None,
            };
        }
    }
    Some(match n.as_str() {
        "ENTER" | "RETURN" => 0x28,
        "ESC" | "ESCAPE" => 0x29,
        "BACKSPACE" | "BACK" => 0x2a,
        "TAB" => 0x2b,
        "SPACE" | "SPACEBAR" => 0x2c,
        "MINUS" | "HYPEN" | "HYPHEN" | "DASH" => 0x2d,
        "EQUAL" | "EQUALS" | "PLUS" => 0x2e,
        "LEFTBRACKET" | "OPENBRACKET" | "LBRACKET" => 0x2f,
        "RIGHTBRACKET" | "CLOSEBRACKET" | "RBRACKET" => 0x30,
        "BACKSLASH" => 0x31,
        "NONUSHASH" | "HASH" => 0x32,
        "SEMICOLON" | "COLON" => 0x33,
        "APOSTROPHE" | "QUOTE" | "SINGLEQUOTE" => 0x34,
        "GRAVE" | "TILDE" | "BACKQUOTE" | "BACKTICK" | "ACCENT" => 0x35,
        "COMMA" => 0x36,
        "PERIOD" | "DOT" => 0x37,
        "SLASH" | "FORWARDSLASH" => 0x38,
        "CAPSLOCK" | "CAPS" => 0x39,
        "PRINTSCREEN" | "PRTSC" | "PRINT" | "PRTSCN" => 0x46,
        "SCROLLLOCK" | "SCROLL" => 0x47,
        "PAUSE" | "BREAK" | "PAUSEBREAK" => 0x48,
        "INSERT" | "INS" => 0x49,
        "HOME" => 0x4a,
        "PAGEUP" | "PGUP" => 0x4b,
        "DELETE" | "DEL" => 0x4c,
        "END" => 0x4d,
        "PAGEDOWN" | "PGDN" => 0x4e,
        "RIGHT" | "RIGHTARROW" | "ARROWRIGHT" => 0x4f,
        "LEFT" | "LEFTARROW" | "ARROWLEFT" => 0x50,
        "DOWN" | "DOWNARROW" | "ARROWDOWN" => 0x51,
        "UP" | "UPARROW" | "ARROWUP" => 0x52,
        "NUMLOCK" => 0x53,
        "NUMPADDIVIDE" | "NUMDIVIDE" | "DIVIDE" | "NUMPADSLASH" => 0x54,
        "NUMPADMULTIPLY" | "NUMMULTIPLY" | "MULTIPLY" | "NUMPADASTERISK" => 0x55,
        "NUMPADSUBTRACT" | "NUMSUBTRACT" | "SUBTRACT" | "NUMPADMINUS" => 0x56,
        "NUMPADADD" | "NUMADD" | "ADD" | "NUMPADPLUS" => 0x57,
        "NUMPADENTER" | "NUMENTER" => 0x58,
        "NUMPADDECIMAL" | "NUMDECIMAL" | "DECIMAL" | "NUMPADPERIOD" => 0x63,
        "NONUSBACKSLASH" | "INTLBACKSLASH" | "OEM102" => 0x64,
        "APPLICATION" | "APPS" | "MENU" | "CONTEXTMENU" => 0x65,
        "INTERNATIONAL1" | "RO" => 0x87,
        "INTERNATIONAL3" | "YEN" => 0x89,
        "LEFTCTRL" | "LCTRL" | "LEFTCONTROL" | "CTRL" | "CONTROL" => 0xe0,
        "LEFTSHIFT" | "LSHIFT" | "SHIFT" => 0xe1,
        "LEFTALT" | "LALT" | "ALT" => 0xe2,
        "LEFTWIN" | "LWIN" | "LEFTGUI" | "LEFTWINDOWS" | "WIN" | "WINDOWS" | "LEFTMETA" => 0xe3,
        "RIGHTCTRL" | "RCTRL" | "RIGHTCONTROL" => 0xe4,
        "RIGHTSHIFT" | "RSHIFT" => 0xe5,
        "RIGHTALT" | "RALT" | "ALTGR" => 0xe6,
        "RIGHTWIN" | "RWIN" | "RIGHTGUI" | "RIGHTWINDOWS" | "RIGHTMETA" => 0xe7,
        _ => return None,
    })
}

/// Modifier bit (HID report byte) for a modifier usage 0xe0–0xe7.
pub fn modifier_bit(hid: u8) -> Option<u8> {
    (0xe0..=0xe7).contains(&hid).then(|| 1 << (hid - 0xe0))
}

/// Keyboard-function data `[modifier bits, usage]` for a key plus modifier names.
/// A lone modifier key becomes `[bit, 0]` (how the keyboard stores it).
pub fn key_data(key: &str, modifiers: &[String]) -> Result<[u8; 2], String> {
    let hid = hid_for_name(key).ok_or_else(|| format!("unknown key {key}"))?;
    let mut bits = 0u8;
    for m in modifiers {
        let h = hid_for_name(m).ok_or_else(|| format!("unknown modifier {m}"))?;
        bits |= modifier_bit(h).ok_or_else(|| format!("{m} is not a modifier"))?;
    }
    Ok(match modifier_bit(hid) {
        Some(b) => [bits | b, 0],
        None => [bits, hid],
    })
}

/// HID usage for a key code in a Synapse macro. Synapse 4 records macros in its web UI, and
/// `KeyEvent.Makecode` holds the browser/Windows virtual-key code (W = 87, arrows = 37–40,
/// Shift = 16), with `State` bit 1 marking an extended key (right Ctrl/Alt, keypad Enter).
pub fn hid_for_vk(vk: u32, extended: bool) -> Option<u8> {
    Some(match vk {
        0x08 => 0x2a,
        0x09 => 0x2b,
        0x0d if extended => 0x58,
        0x0d => 0x28,
        0x10 => 0xe1,
        0x11 if extended => 0xe4,
        0x11 => 0xe0,
        0x12 if extended => 0xe6,
        0x12 => 0xe2,
        0x13 => 0x48,
        0x14 => 0x39,
        0x1b => 0x29,
        0x20 => 0x2c,
        0x21 => 0x4b,
        0x22 => 0x4e,
        0x23 => 0x4d,
        0x24 => 0x4a,
        0x25 => 0x50,
        0x26 => 0x52,
        0x27 => 0x4f,
        0x28 => 0x51,
        0x2c => 0x46,
        0x2d => 0x49,
        0x2e => 0x4c,
        0x30 => 0x27,
        0x31..=0x39 => 0x1e + (vk - 0x31) as u8,
        0x41..=0x5a => 0x04 + (vk - 0x41) as u8,
        0x5b => 0xe3,
        0x5c => 0xe7,
        0x5d => 0x65,
        0x60 => 0x62,
        0x61..=0x69 => 0x59 + (vk - 0x61) as u8,
        0x6a => 0x55,
        0x6b => 0x57,
        0x6d => 0x56,
        0x6e => 0x63,
        0x6f => 0x54,
        0x70..=0x7b => 0x3a + (vk - 0x70) as u8,
        0x7c..=0x87 => 0x68 + (vk - 0x7c) as u8,
        0x90 => 0x53,
        0x91 => 0x47,
        0xa0 => 0xe1,
        0xa1 => 0xe5,
        0xa2 => 0xe0,
        0xa3 => 0xe4,
        0xa4 => 0xe2,
        0xa5 => 0xe6,
        // Firefox reports ; = - with its own codes.
        0x3b | 0xba => 0x33,
        0x3d | 0xbb => 0x2e,
        0xad | 0xbd => 0x2d,
        0xbc => 0x36,
        0xbe => 0x37,
        0xbf => 0x38,
        0xc0 => 0x35,
        0xdb => 0x2f,
        0xdc => 0x31,
        0xdd => 0x30,
        0xde => 0x34,
        0xe2 => 0x64,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synapse_key_names() {
        assert_eq!(hid_for_name("KEY_A"), Some(0x04));
        assert_eq!(hid_for_name("KEY_Z"), Some(0x1d));
        assert_eq!(hid_for_name("KEY_1"), Some(0x1e));
        assert_eq!(hid_for_name("KEY_0"), Some(0x27));
        assert_eq!(hid_for_name("KEY_F12"), Some(0x45));
        assert_eq!(hid_for_name("KEY_F13"), Some(0x68));
        assert_eq!(hid_for_name("KEY_HYPEN"), Some(0x2d));
        assert_eq!(hid_for_name("KEY_PAGE_DOWN"), Some(0x4e));
        assert_eq!(hid_for_name("KEY_BACKSPACE"), Some(0x2a));
        assert_eq!(hid_for_name("KEY_NUMPAD_7"), Some(0x5f));
        assert_eq!(hid_for_name("KEY_WHATEVER"), None);
    }

    #[test]
    fn key_data_with_modifiers() {
        assert_eq!(key_data("KEY_F11", &["KEY_LEFT_CTRL".into()]), Ok([0x01, 0x44]));
        assert_eq!(key_data("KEY_BACKSPACE", &["KEY_RIGHT_ALT".into()]), Ok([0x40, 0x2a]));
        assert_eq!(key_data("KEY_LEFT_SHIFT", &[]), Ok([0x02, 0]));
        assert!(key_data("KEY_A", &["KEY_B".into()]).is_err());
    }

    #[test]
    fn macro_key_codes() {
        assert_eq!(hid_for_vk(87, false), Some(0x1a)); // W
        assert_eq!(hid_for_vk(38, true), Some(0x52)); // Up
        assert_eq!(hid_for_vk(16, false), Some(0xe1)); // Shift
        assert_eq!(hid_for_vk(17, true), Some(0xe4)); // right Ctrl
        assert_eq!(hid_for_vk(13, true), Some(0x58)); // keypad Enter
        assert_eq!(hid_for_vk(0xff, false), None);
    }
}
