//! On-board profiles (class 0x05). Profile-scoped commands elsewhere (DPI, scroll, key
//! assignments) take the profile id as their first argument: 1–5 are stored profiles and 0
//! addresses the live state. Layouts from Synapse 4's obmEngine; see notes/FINDINGS.md.

use crate::Command;

pub const GET_COUNT: Command = Command::new(1, 0x05, 0x80);
pub const GET_ID_LIST: Command = Command::new(80, 0x05, 0x81);
pub const CREATE: Command = Command::new(1, 0x05, 0x02);
pub const DELETE: Command = Command::new(1, 0x05, 0x03);
pub const SET_ACTIVE: Command = Command::new(1, 0x05, 0x04);
pub const GET_ACTIVE: Command = Command::new(1, 0x05, 0x84);
pub const GET_NAME: Command = Command::new(80, 0x05, 0x88);
pub const GET_MAX: Command = Command::new(1, 0x05, 0x8a);

/// Synapse's profile name block: GUID 16 | UTF-8 name 100 | metadata 128 | 6 zero.
pub const NAME_BLOCK: usize = 250;
pub const NAME_CHUNK: usize = 64;

/// Set Profile Name With Offset for an `n`-byte chunk.
pub const fn set_name(n: usize) -> Command {
    Command::new((n + 5) as u8, 0x05, 0x08)
}

/// Get Profiles ID List reply → the first `count` profile ids (the leading byte is a count on
/// keyboards but the slot total on some mice, so the caller passes Get Number of Profiles).
pub fn id_list(a: &[u8], count: usize) -> Vec<u8> {
    a.iter().skip(1).copied().filter(|&id| id != 0).take(count).collect()
}

pub fn encode_set_name(id: u8, offset: u16, chunk: &[u8]) -> Vec<u8> {
    let mut v = vec![id];
    v.extend(offset.to_be_bytes());
    v.extend((NAME_BLOCK as u16).to_be_bytes());
    v.extend_from_slice(chunk);
    v
}

pub fn encode_get_name(id: u8, offset: u16) -> Vec<u8> {
    let mut v = vec![id];
    v.extend(offset.to_be_bytes());
    v.extend([0, 0]);
    v
}

/// Get Profile Name With Offset reply: `[id, offset16, size16, bytes…]` → (size, bytes).
pub fn name_reply(a: &[u8]) -> Option<(usize, &[u8])> {
    let size = u16::from_be_bytes([*a.get(3)?, *a.get(4)?]) as usize;
    Some((size, a.get(5..)?))
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// A short firmware-default name (the keyboard ships profile 1 named "default" in UTF-16BE).
pub fn short_name(b: &[u8]) -> String {
    if b.len() >= 2 && b.len() % 2 == 0 {
        let be = b.iter().step_by(2).all(|&x| x == 0);
        let le = b.iter().skip(1).step_by(2).all(|&x| x == 0);
        if be || le {
            let units: Vec<u16> = b
                .chunks_exact(2)
                .map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
                .take_while(|&u| u != 0)
                .collect();
            return String::from_utf16_lossy(&units);
        }
    }
    cstr(b)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBlock {
    pub guid: [u8; 16],
    pub name: String,
}

pub fn name_block(b: &[u8]) -> Option<NameBlock> {
    Some(NameBlock { guid: b.get(0..16)?.try_into().ok()?, name: cstr(b.get(16..116)?) })
}

pub fn encode_name_block(n: &NameBlock) -> Vec<u8> {
    let mut v = vec![0u8; NAME_BLOCK];
    v[..16].copy_from_slice(&n.guid);
    let mut end = n.name.len().min(99);
    while !n.name.is_char_boundary(end) {
        end -= 1;
    }
    v[16..16 + end].copy_from_slice(&n.name.as_bytes()[..end]);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_names() {
        assert_eq!(id_list(&[0x05, 0x01], 1), vec![1]);
        assert_eq!(id_list(&[0x01, 0x01], 1), vec![1]);
        assert_eq!(id_list(&[0x03, 0x01, 0x02, 0x04, 0, 0], 3), vec![1, 2, 4]);
        // BlackWidow V4 Pro profile 1 as shipped.
        let r = [0x01, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x64, 0x00, 0x65, 0x00, 0x66, 0x00, 0x61, 0x00, 0x75, 0x00, 0x6c, 0x00, 0x74, 0, 0];
        let (size, b) = name_reply(&r).unwrap();
        assert_eq!(short_name(&b[..size]), "default");
        let n = NameBlock { guid: [9; 16], name: "FPS".into() };
        assert_eq!(name_block(&encode_name_block(&n)).unwrap(), n);
    }
}
