//! Typing text on the keyboard's behalf: which key (and Shift / AltGr) produces a character
//! under the desktop's active XKB layout. The keyboard sends HID usages and the OS turns them
//! into characters with its layout, so a text macro made for a US board types the wrong thing
//! on a German or French desktop. The map is built from the layout itself (libxkbcommon), not
//! from hand-written tables, so every layout xkeyboard-config ships works.
//!
//! Characters no key produces directly (emoji, dead-key compositions) are reported as skipped;
//! the caller falls back to a software "type text" action for them.

use std::collections::HashMap;

use xkbcommon::xkb;

use crate::macros::MacroEvent;

/// How to press one character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    /// HID keyboard usage of the key.
    pub hid: u8,
    pub shift: bool,
    pub altgr: bool,
}

/// A layout as the desktop names it: `us`, `de`, `fr` with an optional variant (`intl`, `nodeadkeys`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutName {
    pub layout: String,
    pub variant: String,
}

impl LayoutName {
    pub fn label(&self) -> String {
        if self.variant.is_empty() { self.layout.clone() } else { format!("{} ({})", self.layout, self.variant) }
    }
}

pub struct Layout {
    pub name: LayoutName,
    map: HashMap<char, KeyStroke>,
}

/// Linux evdev key code → HID keyboard usage (the physical keys that type characters).
fn evdev_to_hid(code: u32) -> Option<u8> {
    Some(match code {
        1 => 0x29,
        2..=10 => 0x1e + (code - 2) as u8, // 1..9
        11 => 0x27,
        12 => 0x2d,
        13 => 0x2e,
        14 => 0x2a,
        15 => 0x2b,
        16 => 0x14,
        17 => 0x1a,
        18 => 0x08,
        19 => 0x15,
        20 => 0x17,
        21 => 0x1c,
        22 => 0x18,
        23 => 0x0c,
        24 => 0x12,
        25 => 0x13,
        26 => 0x2f,
        27 => 0x30,
        28 => 0x28,
        30 => 0x04,
        31 => 0x16,
        32 => 0x07,
        33 => 0x09,
        34 => 0x0a,
        35 => 0x0b,
        36 => 0x0d,
        37 => 0x0e,
        38 => 0x0f,
        39 => 0x33,
        40 => 0x34,
        41 => 0x35,
        43 => 0x31,
        44 => 0x1d,
        45 => 0x1b,
        46 => 0x06,
        47 => 0x19,
        48 => 0x05,
        49 => 0x11,
        50 => 0x10,
        51 => 0x36,
        52 => 0x37,
        53 => 0x38,
        55 => 0x55,
        57 => 0x2c,
        71..=73 => 0x5f + (code - 71) as u8, // keypad 7 8 9
        74 => 0x56,
        75..=77 => 0x5c + (code - 75) as u8, // keypad 4 5 6
        78 => 0x57,
        79..=81 => 0x59 + (code - 79) as u8, // keypad 1 2 3
        82 => 0x62,
        83 => 0x63,
        86 => 0x64, // the extra key left of Z on ISO boards
        89 => 0x87, // ro (JIS)
        98 => 0x54,
        124 => 0x89, // yen (JIS)
        _ => return None,
    })
}

fn is_keypad(hid: u8) -> bool {
    (0x54..=0x63).contains(&hid)
}

impl Layout {
    /// Compile `name` with xkeyboard-config; None when the layout doesn't exist or the XKB data
    /// isn't installed.
    pub fn from_names(name: &LayoutName) -> Option<Self> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(&ctx, "evdev", "pc105", name.layout.as_str(), name.variant.as_str(), Some(String::new()), xkb::KEYMAP_COMPILE_NO_FLAGS)?;
        let shift_bit = 1u32 << keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
        let altgr_idx = keymap.mod_get_index("Mod5");
        let altgr_bit = if altgr_idx == xkb::MOD_INVALID { 0 } else { 1u32 << altgr_idx };
        // (modifier count, keypad, evdev code) decides between several keys for one character.
        let mut best: HashMap<char, ((u8, bool, u32), KeyStroke)> = HashMap::new();
        let (min, max) = (keymap.min_keycode().raw(), keymap.max_keycode().raw());
        for raw in min..=max {
            let key = xkb::Keycode::new(raw);
            let Some(hid) = raw.checked_sub(8).and_then(evdev_to_hid) else { continue };
            for level in 0..keymap.num_levels_for_key(key, 0) {
                let mut masks = [0u32; 4];
                let n = keymap.key_get_mods_for_level(key, 0, level, &mut masks);
                // The cheapest modifier combination that reaches the level, if it is only Shift / AltGr.
                let Some(mask) = masks[..n.min(4)].iter().copied().filter(|m| m & !(shift_bit | altgr_bit) == 0).min_by_key(|m| m.count_ones()) else { continue };
                let (shift, altgr) = (mask & shift_bit != 0, altgr_bit != 0 && mask & altgr_bit != 0);
                let syms = keymap.key_get_syms_by_level(key, 0, level);
                let [sym] = syms else { continue };
                let text = xkb::keysym_to_utf8(*sym);
                let mut chars = text.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { continue };
                if c.is_control() {
                    continue;
                }
                let rank = (u8::from(shift) + u8::from(altgr), is_keypad(hid), raw);
                let stroke = KeyStroke { hid, shift, altgr };
                match best.get(&c) {
                    Some((r, _)) if *r <= rank => {}
                    _ => {
                        best.insert(c, (rank, stroke));
                    }
                }
            }
        }
        let mut map: HashMap<char, KeyStroke> = best.into_iter().map(|(c, (_, s))| (c, s)).collect();
        // Whitespace keys exist in every layout.
        map.entry(' ').or_insert(KeyStroke { hid: 0x2c, shift: false, altgr: false });
        map.entry('\n').or_insert(KeyStroke { hid: 0x28, shift: false, altgr: false });
        map.entry('\t').or_insert(KeyStroke { hid: 0x2b, shift: false, altgr: false });
        Some(Self { name: name.clone(), map })
    }

    pub fn stroke(&self, c: char) -> Option<KeyStroke> {
        self.map.get(&c).copied()
    }

    /// Key events that type `text`, with `delay_ms` between characters. Characters this layout
    /// can't type directly are returned in `skipped`.
    pub fn text_to_events(&self, text: &str, delay_ms: u32) -> TextEvents {
        let mut events = Vec::new();
        let mut skipped = String::new();
        for c in text.chars() {
            let Some(k) = self.stroke(c) else {
                skipped.push(c);
                continue;
            };
            let mut mods = Vec::new();
            if k.shift {
                mods.push(0xe1);
            }
            if k.altgr {
                mods.push(0xe6);
            }
            events.extend(mods.iter().map(|&hid| MacroEvent::Key { hid, down: true }));
            events.push(MacroEvent::Key { hid: k.hid, down: true });
            events.push(MacroEvent::Key { hid: k.hid, down: false });
            events.extend(mods.iter().rev().map(|&hid| MacroEvent::Key { hid, down: false }));
            if delay_ms > 0 {
                events.push(MacroEvent::Delay { ms: delay_ms });
            }
        }
        TextEvents { events, skipped }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TextEvents {
    pub events: Vec<MacroEvent>,
    pub skipped: String,
}

/// The layouts the desktop has configured, first = the one in use at login. From KDE's
/// `kxkbrc`, else `localectl`/the XKB environment variables, else US.
pub fn configured_layouts() -> Vec<LayoutName> {
    if let Ok(v) = std::env::var("TSUBA_XKB_LAYOUT") {
        return parse_list(&v, "");
    }
    if let Some(cfg) = dirs::config_dir().and_then(|d| std::fs::read_to_string(d.join("kxkbrc")).ok()) {
        if let Some(l) = parse_kxkbrc(&cfg) {
            return l;
        }
    }
    if let Ok(out) = std::process::Command::new("localectl").arg("status").output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let field = |k: &str| text.lines().find_map(|l| l.trim().strip_prefix(k)).map(|v| v.trim().to_string()).unwrap_or_default();
        let l = parse_list(&field("X11 Layout:"), &field("X11 Variant:"));
        if !l.is_empty() {
            return l;
        }
    }
    let env = std::env::var("XKB_DEFAULT_LAYOUT").unwrap_or_default();
    let l = parse_list(&env, &std::env::var("XKB_DEFAULT_VARIANT").unwrap_or_default());
    if l.is_empty() { vec![LayoutName { layout: "us".into(), variant: String::new() }] } else { l }
}

fn parse_list(layouts: &str, variants: &str) -> Vec<LayoutName> {
    let vs: Vec<&str> = variants.split(',').collect();
    layouts
        .split(',')
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| LayoutName { layout: l.trim().to_string(), variant: vs.get(i).map(|v| v.trim().to_string()).unwrap_or_default() })
        .collect()
}

/// KDE's keyboard layout settings (`[Layout]` with `Use=true`, `LayoutList`, `VariantList`).
fn parse_kxkbrc(text: &str) -> Option<Vec<LayoutName>> {
    let mut in_layout = false;
    let (mut layouts, mut variants, mut use_it) = (String::new(), String::new(), false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_layout = line == "[Layout]";
        } else if in_layout {
            if let Some(v) = line.strip_prefix("LayoutList=") {
                layouts = v.to_string();
            } else if let Some(v) = line.strip_prefix("VariantList=") {
                variants = v.to_string();
            } else if let Some(v) = line.strip_prefix("Use=") {
                use_it = v == "true";
            }
        }
    }
    let l = parse_list(&layouts, &variants);
    (use_it && !l.is_empty()).then_some(l)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(l: &str, v: &str) -> Option<Layout> {
        // xkeyboard-config isn't installed everywhere the tests run.
        Layout::from_names(&LayoutName { layout: l.into(), variant: v.into() })
    }

    #[test]
    fn kde_config_and_lists_parse() {
        let cfg = "[Layout]\nLayoutList=de,us\nUse=true\nVariantList=nodeadkeys,\n[Other]\nUse=false\n";
        let l = parse_kxkbrc(cfg).unwrap();
        assert_eq!(l, vec![LayoutName { layout: "de".into(), variant: "nodeadkeys".into() }, LayoutName { layout: "us".into(), variant: String::new() }]);
        assert!(parse_kxkbrc("[Layout]\nLayoutList=de\nUse=false\n").is_none());
        assert_eq!(l[0].label(), "de (nodeadkeys)");
    }

    #[test]
    fn us_layout_matches_the_classic_table() {
        let Some(us) = layout("us", "") else { return };
        assert_eq!(us.stroke('a'), Some(KeyStroke { hid: 0x04, shift: false, altgr: false }));
        assert_eq!(us.stroke('A'), Some(KeyStroke { hid: 0x04, shift: true, altgr: false }));
        assert_eq!(us.stroke('@'), Some(KeyStroke { hid: 0x1f, shift: true, altgr: false }));
        assert_eq!(us.stroke('1'), Some(KeyStroke { hid: 0x1e, shift: false, altgr: false }), "main row, not the keypad");
        assert_eq!(us.stroke(' ').map(|k| k.hid), Some(0x2c));
        assert!(us.stroke('€').is_none() && us.stroke('😀').is_none());
    }

    #[test]
    fn german_and_french_layouts_move_keys() {
        if let Some(de) = layout("de", "") {
            // Z sits where US has Y, and @ needs AltGr+Q.
            assert_eq!(de.stroke('z'), Some(KeyStroke { hid: 0x1c, shift: false, altgr: false }));
            assert_eq!(de.stroke('y').map(|k| k.hid), Some(0x1d));
            assert_eq!(de.stroke('@'), Some(KeyStroke { hid: 0x14, shift: false, altgr: true }));
            assert!(de.stroke('ä').is_some() && de.stroke('ß').is_some());
        }
        if let Some(fr) = layout("fr", "") {
            // AZERTY: A on the Q key, digits need Shift.
            assert_eq!(fr.stroke('a').map(|k| k.hid), Some(0x14));
            assert_eq!(fr.stroke('1'), Some(KeyStroke { hid: 0x1e, shift: true, altgr: false }));
            assert!(fr.stroke('é').is_some());
        }
    }

    #[test]
    fn text_becomes_key_events_and_reports_what_it_cannot_type() {
        let Some(us) = layout("us", "") else { return };
        let t = us.text_to_events("Hi 😀", 5);
        assert_eq!(t.skipped, "😀");
        assert_eq!(&t.events[..4], &[MacroEvent::Key { hid: 0xe1, down: true }, MacroEvent::Key { hid: 0x0b, down: true }, MacroEvent::Key { hid: 0x0b, down: false }, MacroEvent::Key { hid: 0xe1, down: false }]);
        assert!(t.events.contains(&MacroEvent::Delay { ms: 5 }));
        let Some(de) = layout("de", "") else { return };
        let t = de.text_to_events("@", 0);
        assert_eq!(t.events.first(), Some(&MacroEvent::Key { hid: 0xe6, down: true }), "AltGr first");
    }
}
