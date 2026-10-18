//! A small read-only LevelDB reader: the write-ahead log (`*.log`) and table files
//! (`*.ldb` / `*.sst`, Snappy-compressed blocks). Nothing is written and no lock is taken,
//! so it can read a store on a read-only mount (e.g. a Windows partition).
//!
//! Every entry is returned with its sequence number; the caller keeps the newest per key.
//! Which table files are live comes from the MANIFEST (via `CURRENT`); when that can't be
//! read, every table file in the directory is used (newest sequence still wins).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};

/// One key version: `value` is `None` for a deletion.
#[derive(Debug, Clone)]
pub struct Entry {
    pub seq: u64,
    pub value: Option<Vec<u8>>,
}

pub fn varint(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*pos).context("truncated varint")?;
        *pos += 1;
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
    bail!("varint too long")
}

fn bytes<'a>(b: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = pos.checked_add(n).context("record length overflows")?;
    let s = b.get(*pos..end).context("truncated record")?;
    *pos += n;
    Ok(s)
}

fn len_prefixed<'a>(b: &'a [u8], pos: &mut usize) -> Result<&'a [u8]> {
    let n = varint(b, pos)? as usize;
    bytes(b, pos, n)
}

/// Reassemble the logical records of a LevelDB log-format file (32 KiB blocks of
/// FULL / FIRST / MIDDLE / LAST fragments). Corrupt tails are dropped.
pub fn log_records(data: &[u8]) -> Vec<Vec<u8>> {
    const BLOCK: usize = 32768;
    let mut out = Vec::new();
    let mut partial: Option<Vec<u8>> = None;
    let mut pos = 0;
    while pos + 7 <= data.len() {
        let left = BLOCK - pos % BLOCK;
        if left < 7 {
            pos += left;
            continue;
        }
        let len = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as usize;
        let kind = data[pos + 6];
        let start = pos + 7;
        if kind == 0 && len == 0 {
            // Pre-allocated / zeroed space: skip the rest of the block.
            pos += left;
            continue;
        }
        let Some(frag) = data.get(start..start + len) else { break };
        if len + 7 > left {
            break;
        }
        pos = start + len;
        match kind {
            1 => {
                partial = None;
                out.push(frag.to_vec());
            }
            2 => partial = Some(frag.to_vec()),
            3 => {
                if let Some(p) = partial.as_mut() {
                    p.extend_from_slice(frag);
                }
            }
            4 => {
                if let Some(mut p) = partial.take() {
                    p.extend_from_slice(frag);
                    out.push(p);
                }
            }
            _ => partial = None,
        }
    }
    out
}

/// Entries of one write batch: `seq u64, count u32, {tag, key[, value]}…`.
fn batch_entries(rec: &[u8], out: &mut Vec<(Vec<u8>, Entry)>) -> Result<()> {
    ensure!(rec.len() >= 12, "short write batch");
    let seq = u64::from_le_bytes(rec[0..8].try_into()?);
    let count = u32::from_le_bytes(rec[8..12].try_into()?) as u64;
    let mut pos = 12;
    for i in 0..count {
        let tag = *rec.get(pos).context("truncated batch")?;
        pos += 1;
        let key = len_prefixed(rec, &mut pos)?.to_vec();
        let value = match tag {
            1 => Some(len_prefixed(rec, &mut pos)?.to_vec()),
            0 => None,
            t => bail!("unknown batch record type {t}"),
        };
        out.push((key, Entry { seq: seq.checked_add(i).context("batch sequence overflows")?, value }));
    }
    Ok(())
}

/// Live table file numbers + log numbers to read, from the MANIFEST named in CURRENT.
struct Manifest {
    tables: BTreeSet<u64>,
    log_number: u64,
    prev_log_number: u64,
}

fn manifest(dir: &Path) -> Result<Manifest> {
    let current = std::fs::read_to_string(dir.join("CURRENT")).context("CURRENT")?;
    let data = std::fs::read(dir.join(current.trim())).context("MANIFEST")?;
    let mut tables = BTreeSet::new();
    let (mut log_number, mut prev_log_number) = (0, 0);
    for rec in log_records(&data) {
        let mut p = 0;
        while p < rec.len() {
            match varint(&rec, &mut p)? {
                1 => {
                    len_prefixed(&rec, &mut p)?;
                }
                2 => log_number = varint(&rec, &mut p)?,
                3 | 4 => {
                    varint(&rec, &mut p)?;
                }
                5 => {
                    varint(&rec, &mut p)?;
                    len_prefixed(&rec, &mut p)?;
                }
                6 => {
                    varint(&rec, &mut p)?;
                    tables.remove(&varint(&rec, &mut p)?);
                }
                7 => {
                    varint(&rec, &mut p)?;
                    tables.insert(varint(&rec, &mut p)?);
                    varint(&rec, &mut p)?;
                    len_prefixed(&rec, &mut p)?;
                    len_prefixed(&rec, &mut p)?;
                }
                9 => prev_log_number = varint(&rec, &mut p)?,
                t => bail!("unknown manifest tag {t}"),
            }
        }
    }
    Ok(Manifest { tables, log_number, prev_log_number })
}

fn file_number(p: &Path) -> Option<(u64, String)> {
    let stem = p.file_stem()?.to_str()?;
    let ext = p.extension()?.to_str()?.to_ascii_lowercase();
    Some((stem.parse().ok()?, ext))
}

/// A block: entries with prefix-compressed keys followed by the restart array.
fn block_entries(b: &[u8], mut f: impl FnMut(&[u8], &[u8]) -> Result<()>) -> Result<()> {
    ensure!(b.len() >= 4, "short block");
    let restarts = u32::from_le_bytes(b[b.len() - 4..].try_into()?) as usize;
    let end = b.len().checked_sub(4 + 4 * restarts).context("bad restart count")?;
    let mut pos = 0;
    let mut key: Vec<u8> = Vec::new();
    while pos < end {
        let shared = varint(b, &mut pos)? as usize;
        let non_shared = varint(b, &mut pos)? as usize;
        let vlen = varint(b, &mut pos)? as usize;
        ensure!(shared <= key.len(), "bad shared key length");
        key.truncate(shared);
        key.extend_from_slice(bytes(b, &mut pos, non_shared)?);
        let v = bytes(b, &mut pos, vlen)?;
        f(&key, v)?;
    }
    Ok(())
}

/// Largest Snappy output we accept (a block or a value), and the largest expansion ratio:
/// real data is far below both; a crafted length header would otherwise allocate gigabytes.
pub const MAX_SNAPPY_OUT: usize = 64 << 20;
const MAX_SNAPPY_RATIO: usize = 256;

/// Snappy-decompress with the declared length checked before anything is allocated.
pub fn snappy(raw: &[u8]) -> Result<Vec<u8>> {
    let n = snap::raw::decompress_len(raw).context("snappy length")?;
    ensure!(n <= MAX_SNAPPY_OUT && n <= raw.len().saturating_mul(MAX_SNAPPY_RATIO), "snappy data claims {n} bytes from {}", raw.len());
    Ok(snap::raw::Decoder::new().decompress_vec(raw)?)
}

fn read_block(file: &[u8], handle: &[u8]) -> Result<Vec<u8>> {
    let mut p = 0;
    let off = usize::try_from(varint(handle, &mut p)?).context("block offset")?;
    let size = usize::try_from(varint(handle, &mut p)?).context("block size")?;
    let end = off.checked_add(size).context("block outside the file")?;
    let raw = file.get(off..end).context("block outside the file")?;
    match file.get(end).copied().context("block trailer missing")? {
        0 => Ok(raw.to_vec()),
        1 => snappy(raw).context("snappy block"),
        t => bail!("unsupported block compression {t}"),
    }
}

/// Every entry of a table file.
pub fn table_entries(file: &[u8], out: &mut Vec<(Vec<u8>, Entry)>) -> Result<()> {
    ensure!(file.len() >= 48, "table file too short");
    let footer = &file[file.len() - 48..];
    ensure!(footer[40..] == 0xdb4775248b80fb57u64.to_le_bytes(), "not a LevelDB table (bad magic)");
    let mut p = 0;
    varint(footer, &mut p)?;
    varint(footer, &mut p)?; // metaindex handle (filters) — not needed
    let index = read_block(file, &footer[p..])?;
    let mut handles = Vec::new();
    block_entries(&index, |_, v| {
        handles.push(v.to_vec());
        Ok(())
    })?;
    for h in handles {
        let data = read_block(file, &h)?;
        block_entries(&data, |k, v| {
            ensure!(k.len() >= 8, "internal key too short");
            let (user, trailer) = k.split_at(k.len() - 8);
            let t = u64::from_le_bytes(trailer.try_into()?);
            let value = match t & 0xff {
                1 => Some(v.to_vec()),
                0 => None,
                x => bail!("unknown value type {x}"),
            };
            out.push((user.to_vec(), Entry { seq: t >> 8, value }));
            Ok(())
        })?;
    }
    Ok(())
}

/// Read the whole database: the newest version of every key (deleted keys left out).
pub fn read_db(dir: &Path) -> Result<HashMap<Vec<u8>, Vec<u8>>> {
    let mut files: Vec<(u64, String, PathBuf)> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            file_number(&p).map(|(n, ext)| (n, ext, p))
        })
        .collect();
    files.sort();
    let man = manifest(dir).ok();
    let mut all: Vec<(Vec<u8>, Entry)> = Vec::new();
    let mut tables = 0;
    for (n, ext, path) in &files {
        match ext.as_str() {
            "ldb" | "sst" => {
                if man.as_ref().is_some_and(|m| !m.tables.contains(n)) {
                    continue;
                }
                let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
                table_entries(&data, &mut all).with_context(|| format!("table {}", path.display()))?;
                tables += 1;
            }
            "log" => {
                // Logs older than the manifest's log number were already compacted into tables.
                if man.as_ref().is_some_and(|m| *n < m.log_number && *n != m.prev_log_number) {
                    continue;
                }
                let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
                for rec in log_records(&data) {
                    // A torn batch at the end of a log is ignored, as LevelDB does on recovery.
                    let _ = batch_entries(&rec, &mut all);
                }
            }
            _ => {}
        }
    }
    ensure!(tables > 0 || !all.is_empty(), "{} holds no LevelDB data", dir.display());
    let mut newest: HashMap<Vec<u8>, Entry> = HashMap::new();
    for (k, e) in all {
        match newest.get(&k) {
            Some(old) if old.seq >= e.seq => {}
            _ => {
                newest.insert(k, e);
            }
        }
    }
    Ok(newest.into_iter().filter_map(|(k, e)| e.value.map(|v| (k, v))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_lengths_and_sequences_are_errors() {
        // Batch: seq = u64::MAX, two records → the second sequence would overflow.
        let mut rec = u64::MAX.to_le_bytes().to_vec();
        rec.extend(2u32.to_le_bytes());
        rec.extend([1, 1, b'k', 1, b'v', 1, 1, b'l', 1, b'w']);
        let mut out = Vec::new();
        assert!(batch_entries(&rec, &mut out).is_err());
        // Length prefix near usize::MAX.
        let mut b = vec![0xff; 9];
        b.push(0x01);
        let mut p = 0;
        assert!(len_prefixed(&b, &mut p).is_err());
        // Block handle whose offset + size overflows.
        let mut h = Vec::new();
        for v in [u64::MAX - 1, 10] {
            let mut v = v;
            loop {
                let byte = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    h.push(byte);
                    break;
                }
                h.push(byte | 0x80);
            }
        }
        assert!(read_block(&[0u8; 64], &h).is_err());
    }
}
