//! Chromium IndexedDB on top of LevelDB: key prefixes, database / object-store names,
//! record keys, and record values (Blink envelope → V8 value, including values that
//! Blink moved into a blob file or Snappy-compressed).
//!
//! Key layout (Chromium `indexed_db_leveldb_coding`): a prefix byte giving the byte
//! lengths of database id / object store id / index id, then those ids little-endian.
//! Index id 1 = object-store data, 3 = blob entries; ids 0/0 = database metadata.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;

use super::leveldb::{self, varint};

pub struct Store {
    pub entries: HashMap<Vec<u8>, Vec<u8>>,
    pub blob_dir: Option<PathBuf>,
}

/// Encoded key prefix for (database, object store, index).
pub fn prefix(db: u64, store: u64, index: u64) -> Vec<u8> {
    fn le(mut v: u64) -> Vec<u8> {
        let mut out = vec![(v & 0xff) as u8];
        v >>= 8;
        while v > 0 {
            out.push((v & 0xff) as u8);
            v >>= 8;
        }
        out
    }
    let (d, s, i) = (le(db), le(store), le(index));
    let mut out = vec![(((d.len() - 1) << 5) | ((s.len() - 1) << 2) | (i.len() - 1)) as u8];
    out.extend(d);
    out.extend(s);
    out.extend(i);
    out
}

fn utf16be(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

fn string_with_length(b: &[u8], pos: &mut usize) -> Result<String> {
    let n = usize::try_from(varint(b, pos)?).ok().and_then(|n| n.checked_mul(2)).context("string length overflows")?;
    let end = pos.checked_add(n).context("string length overflows")?;
    let s = b.get(*pos..end).context("truncated string")?;
    *pos = end;
    Ok(utf16be(s))
}

/// Chromium's "Int" encoding: little-endian, as many bytes as needed.
fn truncated_int(b: &[u8]) -> u64 {
    b.iter().take(8).enumerate().fold(0, |v, (i, x)| v | ((*x as u64) << (8 * i)))
}

/// Decode an encoded IndexedDB key into text (strings as-is, numbers in decimal, arrays
/// joined with ","). Returns the text and the encoded length.
pub fn decode_key(b: &[u8]) -> Result<(String, usize)> {
    decode_key_at(b, 0)
}

/// Array keys nest; deeper than this is not real data (and would exhaust the stack).
const MAX_KEY_DEPTH: usize = 128;

fn decode_key_at(b: &[u8], depth: usize) -> Result<(String, usize)> {
    ensure!(depth < MAX_KEY_DEPTH, "key nested too deeply");
    let mut pos = 1;
    let s = match *b.first().context("empty key")? {
        0 => String::new(),
        1 => string_with_length(b, &mut pos)?,
        2 | 3 => {
            let f = f64::from_le_bytes(b.get(1..9).context("truncated number key")?.try_into()?);
            pos = 9;
            if f.fract() == 0.0 { format!("{}", f as i64) } else { f.to_string() }
        }
        4 => {
            let n = varint(b, &mut pos)?;
            let mut parts = Vec::new();
            for _ in 0..n {
                let (p, len) = decode_key_at(b.get(pos..).context("truncated array key")?, depth + 1)?;
                parts.push(p);
                pos = pos.checked_add(len).context("array key overflows")?;
            }
            parts.join(",")
        }
        6 => {
            let n = usize::try_from(varint(b, &mut pos)?).context("binary key length")?;
            let end = pos.checked_add(n).context("binary key length overflows")?;
            let raw = b.get(pos..end).context("truncated binary key")?;
            pos = end;
            raw.iter().map(|x| format!("{x:02x}")).collect()
        }
        t => bail!("unsupported key type {t}"),
    };
    Ok((s, pos))
}

impl Store {
    /// Open `<origin>.indexeddb.leveldb` (read-only). The sibling `.indexeddb.blob`
    /// directory is used for values stored as blobs.
    pub fn open(dir: &Path) -> Result<Self> {
        let entries = leveldb::read_db(dir)?;
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let blob_dir = name.strip_suffix(".leveldb").map(|base| dir.with_file_name(format!("{base}.blob"))).filter(|p| p.is_dir());
        Ok(Self { entries, blob_dir })
    }

    /// (database id, name) of every database in the origin.
    pub fn databases(&self) -> Vec<(u64, String)> {
        let pre = [0u8, 0, 0, 0, 201];
        let mut out: Vec<(u64, String)> = self
            .entries
            .iter()
            .filter(|(k, _)| k.starts_with(&pre))
            .filter_map(|(k, v)| {
                let mut p = pre.len();
                string_with_length(k, &mut p).ok()?; // origin
                let name = string_with_length(k, &mut p).ok()?;
                Some((truncated_int(v), name))
            })
            .collect();
        out.sort();
        out
    }

    /// Object store id by name within database `db`.
    pub fn object_store(&self, db: u64, name: &str) -> Option<u64> {
        let pre = {
            let mut p = prefix(db, 0, 0);
            p.push(50);
            p
        };
        self.entries.iter().find_map(|(k, v)| {
            let rest = k.strip_prefix(pre.as_slice())?;
            let mut p = 0;
            let id = varint(rest, &mut p).ok()?;
            (rest.get(p) == Some(&0) && rest.len() == p + 1 && utf16be(v) == name).then_some(id)
        })
    }

    pub fn find_store(&self, db_name: &str, store: &str) -> Option<(u64, u64)> {
        self.databases().into_iter().filter(|(_, n)| n == db_name).find_map(|(db, _)| Some((db, self.object_store(db, store)?)))
    }

    /// Every live record of an object store: (key text, decoded value or an error).
    pub fn records(&self, db: u64, store: u64) -> Vec<(String, Result<Value>)> {
        let pre = prefix(db, store, 1);
        let mut out: Vec<(String, Result<Value>)> = self
            .entries
            .iter()
            .filter_map(|(k, v)| {
                let raw_key = k.strip_prefix(pre.as_slice())?;
                let key = decode_key(raw_key).map(|x| x.0).unwrap_or_else(|_| format!("<undecodable key {} bytes>", raw_key.len()));
                Some((key, self.value(db, store, raw_key, v)))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Record value: `varint version` + Blink serialised value.
    fn value(&self, db: u64, store: u64, raw_key: &[u8], v: &[u8]) -> Result<Value> {
        let mut p = 0;
        varint(v, &mut p).context("record version")?;
        self.blink(db, store, raw_key, &v[p..], 0)
    }

    /// Blink envelope: `FF <version>`; version 0x11 marks a wrapped value (01 = in a blob
    /// file, 02 = Snappy-compressed); version ≥ 21 carries a 13-byte trailer-offset record.
    fn blink(&self, db: u64, store: u64, raw_key: &[u8], b: &[u8], depth: u8) -> Result<Value> {
        ensure!(depth < 4, "nested value wrappers");
        ensure!(b.first() == Some(&0xff), "not a Blink serialised value");
        let mut p = 1;
        let version = varint(b, &mut p)?;
        if version == 0x11 {
            match b.get(p) {
                Some(1) => {
                    p += 1;
                    let _size = varint(b, &mut p)?;
                    let index = varint(b, &mut p)? as usize;
                    let data = self.blob(db, store, raw_key, index)?;
                    return self.blink(db, store, raw_key, &data, depth + 1);
                }
                Some(2) => {
                    let data = leveldb::snappy(b.get(p + 1..).context("empty compressed value")?).context("snappy-compressed value")?;
                    return self.blink(db, store, raw_key, &data, depth + 1);
                }
                _ => bail!("unknown wrapped-value marker"),
            }
        }
        if version >= 21 && b.get(p) == Some(&0xfe) {
            p += 13;
        }
        super::v8::decode(b.get(p..).context("empty value")?)
    }

    /// Contents of blob `index` of a record (from its blob-entry list).
    fn blob(&self, db: u64, store: u64, raw_key: &[u8], index: usize) -> Result<Vec<u8>> {
        let mut k = prefix(db, store, 3);
        k.extend_from_slice(raw_key);
        let list = self.entries.get(&k).context("value is in a blob, but the blob list is missing")?;
        let mut p = 0;
        let mut i = 0;
        while p < list.len() {
            let kind = list[p];
            p += 1;
            ensure!(kind <= 1, "unsupported external object type {kind}");
            let number = varint(list, &mut p)?;
            let _mime = string_with_length(list, &mut p)?;
            let _size = varint(list, &mut p)?;
            if kind == 1 {
                string_with_length(list, &mut p)?;
                varint(list, &mut p)?;
            }
            if i == index {
                let dir = self.blob_dir.as_ref().context("value is in a blob, but there's no .blob folder")?;
                let path = dir.join(format!("{db:x}")).join(format!("{:02x}", (number >> 8) & 0xff)).join(format!("{number:x}"));
                return std::fs::read(&path).with_context(|| format!("reading blob {}", path.display()));
            }
            i += 1;
        }
        bail!("blob {index} not in the record's blob list")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_depth_and_lengths_are_bounded() {
        // [[[[…]]]] array keys: an error, not a stack overflow.
        let mut b = Vec::new();
        for _ in 0..100_000 {
            b.extend([4u8, 1]);
        }
        b.push(0);
        let h = std::thread::Builder::new().stack_size(256 << 10).spawn(move || decode_key(&b).is_err()).unwrap();
        assert!(h.join().expect("decoder crashed"));
        // Normal nesting works.
        assert_eq!(decode_key(&[4, 2, 4, 1, 0, 0]).unwrap().0, ",");
        // Lengths near usize::MAX.
        let mut b = vec![1u8];
        b.extend([0xff; 9]);
        b.push(0x01);
        assert!(decode_key(&b).is_err());
        let mut b = vec![6u8];
        b.extend([0xff; 9]);
        b.push(0x01);
        assert!(decode_key(&b).is_err());
    }

    #[test]
    fn snappy_length_header_is_checked_before_allocating() {
        // Header claims 2^31 bytes; a handful of input bytes.
        let raw = [0x80, 0x80, 0x80, 0x80, 0x08, 0x00];
        let e = leveldb::snappy(&raw).unwrap_err();
        assert!(e.to_string().contains("claims"), "{e}");
        // Real data round-trips.
        let data = vec![7u8; 100_000];
        let c = snap::raw::Encoder::new().compress_vec(&data).unwrap();
        assert_eq!(leveldb::snappy(&c).unwrap(), data);
    }
}
