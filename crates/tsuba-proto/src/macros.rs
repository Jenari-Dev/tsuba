//! On-board macros (class 0x06): storage, the 250-byte name block and the event byte stream.
//! Layouts recovered from Synapse 4's obmEngine; see notes/FINDINGS.md "Macros".

use crate::Command;

pub const GET_COUNT: Command = Command::new(2, 0x06, 0x80);
pub const DELETE: Command = Command::new(2, 0x06, 0x03);
pub const GET_DATA_SIZE: Command = Command::new(6, 0x06, 0x88);
pub const ALLOCATE: Command = Command::new(6, 0x06, 0x08);
pub const GET_MEMORY_DATA: Command = Command::new(80, 0x06, 0x89);
pub const GET_ID_LIST: Command = Command::new(80, 0x06, 0x8b);
pub const GET_NAME: Command = Command::new(80, 0x06, 0x8c);
pub const GET_STORAGE_INFO: Command = Command::new(14, 0x06, 0x8e);

/// Event bytes are read and written in chunks of this many. Synapse uses 72, but the
/// BlackWidow V4 Pro keeps only the first 20 bytes of each data write (and returns at most
/// 20 per read), so 20 is used for every device.
pub const DATA_CHUNK: usize = 20;
/// The name block is written in chunks of this many.
pub const NAME_CHUNK: usize = 64;
pub const NAME_BLOCK: usize = 250;

/// Set Macro Memory Data for an `n`-byte chunk.
pub const fn set_memory_data(n: usize) -> Command {
    Command::new((n + 7) as u8, 0x06, 0x09)
}

/// Set Macro Name With Offset for an `n`-byte chunk.
pub const fn set_name(n: usize) -> Command {
    Command::new((n + 6) as u8, 0x06, 0x0c)
}

fn be16(a: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*a.get(i)?, *a.get(i + 1)?]))
}

/// 32-bit values travel as two big-endian 16-bit words, high word first.
fn be32(a: &[u8], i: usize) -> Option<u32> {
    Some((be16(a, i)? as u32) << 16 | be16(a, i + 2)? as u32)
}

fn put32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageInfo {
    pub max_id: u16,
    pub max_memory: u32,
    pub available: u32,
    pub recyclable: u32,
}

pub fn storage_info(a: &[u8]) -> Option<StorageInfo> {
    Some(StorageInfo { max_id: be16(a, 0)?, max_memory: be32(a, 2)?, available: be32(a, 6)?, recyclable: be32(a, 10)? })
}

/// Memory Synapse reserves for a macro of `data_len` event bytes (data + name block, 256-aligned).
pub fn required_memory(data_len: usize) -> u32 {
    ((data_len + 256).div_ceil(256) * 256) as u32
}

/// Get Macro ID List With Offset reply: `[offset16, n16, ids16…]`. `n` is an id count on the
/// Basilisk but a byte count on the BlackWidow, so the caller passes Get Number of Macros.
pub fn id_list(a: &[u8], count: usize) -> Vec<u16> {
    a.get(4..)
        .unwrap_or_default()
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .filter(|&id| id != 0)
        .take(count)
        .collect()
}

/// Get Macro Data Size reply: `[id16, size32]`.
pub fn data_size(a: &[u8]) -> Option<u32> {
    be32(a, 2)
}

pub fn encode_allocate(id: u16, size: u32) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    v.extend(put32(size));
    v
}

pub fn encode_get_memory(id: u16, offset: u32, n: u8) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    v.extend(put32(offset));
    v.push(n);
    v
}

/// Get Macro Memory Data reply: `[id16, offset32, n, bytes…]` → the bytes.
pub fn memory_chunk(a: &[u8]) -> Option<&[u8]> {
    let n = *a.get(6)? as usize;
    a.get(7..7 + n)
}

pub fn encode_set_memory(id: u16, offset: u32, chunk: &[u8]) -> Vec<u8> {
    let mut v = encode_get_memory(id, offset, chunk.len() as u8);
    v.extend_from_slice(chunk);
    v
}

pub fn encode_set_name(id: u16, offset: u16, chunk: &[u8]) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    v.extend(offset.to_be_bytes());
    v.extend((NAME_BLOCK as u16).to_be_bytes());
    v.extend_from_slice(chunk);
    v
}

pub fn encode_get_name(id: u16, offset: u16) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    v.extend(offset.to_be_bytes());
    v
}

/// Get Macro Name With Offset reply: `[id16, offset16, total16, bytes…]` → the bytes.
pub fn name_chunk(a: &[u8], n: usize) -> Option<&[u8]> {
    a.get(6..6 + n)
}

/// The 250-byte name block: GUID, event count, UTF-8 name (100 bytes) and two optional
/// strings Synapse uses for a timestamp and a hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBlock {
    pub guid: [u8; 16],
    pub items: u32,
    pub name: String,
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

pub fn name_block(b: &[u8]) -> Option<NameBlock> {
    let guid = b.get(0..16)?.try_into().ok()?;
    let items = u32::from_le_bytes(b.get(16..20)?.try_into().ok()?);
    Some(NameBlock { guid, items, name: cstr(b.get(20..120)?) })
}

pub fn encode_name_block(n: &NameBlock) -> Vec<u8> {
    let mut v = vec![0u8; NAME_BLOCK];
    v[..16].copy_from_slice(&n.guid);
    v[16..20].copy_from_slice(&n.items.to_le_bytes());
    // Truncate on a character boundary so the stored name stays valid UTF-8.
    let mut end = n.name.len().min(99);
    while !n.name.is_char_boundary(end) {
        end -= 1;
    }
    v[20..20 + end].copy_from_slice(&n.name.as_bytes()[..end]);
    v
}

/// One recorded action. Mouse buttons are numbered 1 left, 2 right, 3 middle, 4, 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Key { hid: u8, down: bool },
    Button { button: u8, down: bool },
    Scroll { up: bool },
    Delay { ms: u32 },
    Cursor { x: u16, y: u16 },
    /// Consumer/system key events (opcodes 3–6), kept verbatim.
    Raw { op: u8, data: [u8; 2] },
}

fn button_bit(button: u8) -> Option<u8> {
    (1..=5).contains(&button).then(|| 1 << (button - 1))
}

pub fn encode_events(events: &[Event]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut mask = 0u8;
    for e in events {
        match *e {
            Event::Key { hid, down } => out.extend([if down { 0x01 } else { 0x02 }, hid]),
            Event::Button { button, down } => {
                let bit = button_bit(button).ok_or_else(|| format!("mouse button {button} can't be recorded"))?;
                mask = if down { mask | bit } else { mask & !bit };
                out.extend([0x08, mask]);
            }
            Event::Scroll { up } => out.extend([0x0a, if up { 0x01 } else { 0xff }]),
            Event::Delay { ms } => match ms {
                0..=0xff => out.extend([0x11, ms as u8]),
                0x100..=0xffff => out.extend([0x12, (ms >> 8) as u8, ms as u8]),
                0x1_0000..=0xff_ffff => out.extend([0x13, (ms >> 16) as u8, (ms >> 8) as u8, ms as u8]),
                _ => {
                    out.push(0x14);
                    out.extend(ms.to_be_bytes());
                }
            },
            Event::Cursor { x, y } => {
                out.push(0x15);
                out.extend(x.to_be_bytes());
                out.extend(y.to_be_bytes());
            }
            Event::Raw { op, data } => {
                out.push(op);
                out.extend(data);
            }
        }
    }
    Ok(out)
}

/// Decode an event byte stream. Unknown opcodes end decoding with an error.
pub fn decode_events(b: &[u8]) -> Result<Vec<Event>, String> {
    let mut out = Vec::new();
    let mut mask = 0u8;
    let mut i = 0;
    let need = |i: usize, n: usize| if i + n <= b.len() { Ok(()) } else { Err(format!("truncated event at byte {i}")) };
    while i < b.len() {
        let op = b[i];
        match op {
            0x01 | 0x02 => {
                need(i, 2)?;
                out.push(Event::Key { hid: b[i + 1], down: op == 0x01 });
                i += 2;
            }
            0x03..=0x06 => {
                need(i, 3)?;
                out.push(Event::Raw { op, data: [b[i + 1], b[i + 2]] });
                i += 3;
            }
            0x08 => {
                need(i, 2)?;
                let new = b[i + 1];
                for button in 1..=5u8 {
                    let bit = 1 << (button - 1);
                    if (mask ^ new) & bit != 0 {
                        out.push(Event::Button { button, down: new & bit != 0 });
                    }
                }
                mask = new;
                i += 2;
            }
            0x0a => {
                need(i, 2)?;
                out.push(Event::Scroll { up: b[i + 1] == 0x01 });
                i += 2;
            }
            0x11..=0x14 => {
                let n = (op - 0x10) as usize;
                need(i, 1 + n)?;
                let ms = b[i + 1..i + 1 + n].iter().fold(0u32, |a, &x| a << 8 | x as u32);
                out.push(Event::Delay { ms });
                i += 1 + n;
            }
            0x15 => {
                need(i, 5)?;
                out.push(Event::Cursor { x: u16::from_be_bytes([b[i + 1], b[i + 2]]), y: u16::from_be_bytes([b[i + 3], b[i + 4]]) });
                i += 5;
            }
            x => return Err(format!("unknown macro opcode 0x{x:02x} at byte {i}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // "Macro 3" as stored on a Basilisk V3 Pro by Synapse.
    const MACRO3: [u8; 13] = [0x01, 0x1a, 0x02, 0x1a, 0x12, 0x03, 0xe8, 0x01, 0xe1, 0x02, 0xe1, 0x11, 0x14];

    #[test]
    fn decodes_synapse_macro() {
        let e = decode_events(&MACRO3).unwrap();
        assert_eq!(
            e,
            vec![
                Event::Key { hid: 0x1a, down: true },
                Event::Key { hid: 0x1a, down: false },
                Event::Delay { ms: 1000 },
                Event::Key { hid: 0xe1, down: true },
                Event::Key { hid: 0xe1, down: false },
                Event::Delay { ms: 20 },
            ]
        );
        assert_eq!(encode_events(&e).unwrap(), MACRO3);
    }

    #[test]
    fn buttons_and_long_delays_round_trip() {
        let e = vec![
            Event::Button { button: 1, down: true },
            Event::Button { button: 2, down: true },
            Event::Button { button: 1, down: false },
            Event::Button { button: 2, down: false },
            Event::Scroll { up: false },
            Event::Delay { ms: 70_000 },
            Event::Delay { ms: 20_000_000 },
        ];
        let b = encode_events(&e).unwrap();
        assert_eq!(&b[..8], &[0x08, 0x01, 0x08, 0x03, 0x08, 0x02, 0x08, 0x00]);
        assert_eq!(decode_events(&b).unwrap(), e);
    }

    #[test]
    fn name_block_round_trip() {
        let n = NameBlock { guid: [7; 16], items: 6, name: "Macro 3".into() };
        let b = encode_name_block(&n);
        assert_eq!(b.len(), NAME_BLOCK);
        assert_eq!(&b[16..20], &[6, 0, 0, 0]);
        assert_eq!(name_block(&b).unwrap(), n);
    }

    #[test]
    fn storage_and_ids() {
        let s = storage_info(&[0x00, 0x64, 0x00, 0x04, 0xc0, 0x00, 0x00, 0x04, 0x1c, 0x00, 0x00, 0x00, 0xa0, 0x00]).unwrap();
        assert_eq!((s.max_id, s.max_memory, s.available, s.recyclable), (100, 311_296, 269_312, 40_960));
        assert_eq!(id_list(&[0, 0, 0, 2, 0x2c, 0x1b, 0x11, 0x64], 2), vec![0x2c1b, 0x1164]);
        // BlackWidow: the length field counts bytes (one id).
        assert_eq!(id_list(&[0, 0, 0, 2, 0x44, 0xe4, 0, 0], 1), vec![0x44e4]);
        assert_eq!(required_memory(13), 512);
    }
}
