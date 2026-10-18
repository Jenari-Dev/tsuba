//! Decoder for the V8 value-serialisation format (what `structuredClone` / IndexedDB
//! store), into JSON. Covers the plain-data subset: objects, dense and sparse arrays,
//! strings (one-byte, two-byte, UTF-8), numbers, booleans, null/undefined, dates,
//! boxed primitives, maps/sets (as arrays) and object back-references.
//! `undefined` and array holes become JSON `null`.
//!
//! The input is untrusted (a file on disk): nesting depth, sparse array lengths and the total
//! size of the output (counting what back-references copy) are bounded, so a crafted value fails
//! with an error instead of exhausting the stack or memory.

use anyhow::{bail, ensure, Context, Result};
use serde_json::{Map, Number, Value};

/// Deepest nesting accepted (real Synapse data nests a handful of levels).
const MAX_DEPTH: usize = 128;
/// Output budget for one value: JSON nodes (back-reference copies included) and string bytes.
const MAX_NODES: usize = 4 << 20;
const MAX_BYTES: usize = 64 << 20;
/// Longest sparse array (`a` tag) accepted; its holes are materialised as `null`s.
const MAX_SPARSE_LEN: usize = 1 << 16;

/// Output size of a (sub)value: nodes and string bytes.
#[derive(Clone, Copy, Default)]
struct Cost {
    nodes: usize,
    bytes: usize,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    version: u64,
    depth: usize,
    /// Output produced so far.
    spent: Cost,
    /// Objects by id (for `^` back-references) with the cost of copying them; `None` while
    /// still being read.
    objects: Vec<Option<(Value, Cost)>>,
    /// `spent` when each object was started (to compute its cost at `finish`).
    started: Vec<Cost>,
}

fn num(f: f64) -> Value {
    Number::from_f64(f).map_or(Value::Null, Value::Number)
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        let v = *self.b.get(self.pos).context("value data ends early")?;
        self.pos += 1;
        Ok(v)
    }

    fn varint(&mut self) -> Result<u64> {
        super::leveldb::varint(self.b, &mut self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).context("value length overflows")?;
        let s = self.b.get(self.pos..end).context("value data ends early")?;
        self.pos = end;
        Ok(s)
    }

    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.varint()?).context("length overflows")
    }

    /// Charge output against the budget.
    fn spend(&mut self, nodes: usize, bytes: usize) -> Result<()> {
        self.spent.nodes = self.spent.nodes.saturating_add(nodes);
        self.spent.bytes = self.spent.bytes.saturating_add(bytes);
        ensure!(self.spent.nodes <= MAX_NODES && self.spent.bytes <= MAX_BYTES, "value too large (more than {MAX_NODES} nodes or {MAX_BYTES} bytes after expanding references)");
        Ok(())
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into()?))
    }

    /// Next tag, skipping alignment padding.
    fn tag(&mut self) -> Result<u8> {
        loop {
            let t = self.byte()?;
            if t != 0 {
                return Ok(t);
            }
        }
    }

    fn peek_tag(&mut self) -> Result<u8> {
        while self.b.get(self.pos) == Some(&0) {
            self.pos += 1;
        }
        self.b.get(self.pos).copied().context("value data ends early")
    }

    fn new_object(&mut self) -> usize {
        self.objects.push(None);
        self.started.push(self.spent);
        self.objects.len() - 1
    }

    fn finish(&mut self, id: usize, v: Value) -> Value {
        let s = self.started[id];
        let cost = Cost { nodes: self.spent.nodes - s.nodes, bytes: self.spent.bytes - s.bytes };
        self.objects[id] = Some((v.clone(), cost));
        v
    }

    fn string(&mut self, tag: u8) -> Result<String> {
        let n = self.len()?;
        self.spend(0, n)?;
        let raw = self.take(n)?;
        Ok(match tag {
            b'"' => raw.iter().map(|&c| c as char).collect(),
            b'S' => String::from_utf8_lossy(raw).into_owned(),
            b'c' => {
                ensure!(n % 2 == 0, "odd two-byte string length");
                let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            }
            _ => bail!("not a string tag {tag:#x}"),
        })
    }

    /// Properties until `end`; returns them in order.
    fn properties(&mut self, end: u8, into: &mut Map<String, Value>) -> Result<()> {
        loop {
            if self.peek_tag()? == end {
                self.pos += 1;
                return Ok(());
            }
            let key = match self.value()? {
                Value::String(s) => s,
                Value::Number(n) => n.as_f64().map(|f| if f.fract() == 0.0 { format!("{}", f as i64) } else { f.to_string() }).unwrap_or_default(),
                other => other.to_string(),
            };
            let v = self.value()?;
            into.insert(key, v);
        }
    }

    fn value(&mut self) -> Result<Value> {
        ensure!(self.depth < MAX_DEPTH, "value nested more than {MAX_DEPTH} levels deep");
        self.depth += 1;
        let v = self.value_inner();
        self.depth -= 1;
        v
    }

    fn value_inner(&mut self) -> Result<Value> {
        let tag = self.tag()?;
        self.spend(1, 0)?;
        Ok(match tag {
            b'_' | b'-' => Value::Null,
            b'0' => Value::Null,
            b'T' => Value::Bool(true),
            b'F' => Value::Bool(false),
            b'I' => {
                let z = self.varint()? as u32;
                Value::from(((z >> 1) as i32) ^ -((z & 1) as i32))
            }
            b'U' => Value::from(self.varint()? as u32),
            b'N' => num(self.f64()?),
            b'Z' => {
                let bits = self.varint()?;
                let n = usize::try_from(bits >> 1).context("bigint length")?;
                let digits = self.take(n)?;
                let mut v: u128 = 0;
                for (i, d) in digits.iter().enumerate().take(16) {
                    v |= (*d as u128) << (8 * i);
                }
                let s = if bits & 1 == 1 { format!("-{v}") } else { v.to_string() };
                Value::String(s)
            }
            b'"' | b'S' | b'c' => Value::String(self.string(tag)?),
            b'^' => {
                let id = self.varint()?;
                match usize::try_from(id).ok().and_then(|id| self.objects.get(id)).cloned().flatten() {
                    // The copy counts against the budget like the original did.
                    Some((v, cost)) => {
                        self.spend(cost.nodes, cost.bytes)?;
                        v
                    }
                    None => Value::Null,
                }
            }
            b'o' => {
                let id = self.new_object();
                let mut m = Map::new();
                self.properties(b'{', &mut m)?;
                self.varint()?;
                self.finish(id, Value::Object(m))
            }
            b'A' => {
                let id = self.new_object();
                let len = self.len()?;
                let mut items = Vec::with_capacity(len.min(1 << 16));
                for _ in 0..len {
                    items.push(self.value()?);
                }
                // Extra (non-index) properties are dropped; JSON arrays can't hold them.
                let mut extra = Map::new();
                self.properties(b'$', &mut extra)?;
                self.varint()?;
                self.varint()?;
                self.finish(id, Value::Array(items))
            }
            b'a' => {
                let id = self.new_object();
                let len = self.len()?;
                let mut m = Map::new();
                self.properties(b'@', &mut m)?;
                self.varint()?;
                self.varint()?;
                // Holes are materialised as nulls: bound the length and charge them to the budget.
                ensure!(len <= MAX_SPARSE_LEN, "sparse array of {len} elements");
                self.spend(len, 0)?;
                let mut items = vec![Value::Null; len];
                for (k, v) in m {
                    if let Ok(i) = k.parse::<usize>() {
                        if i < items.len() {
                            items[i] = v;
                        }
                    }
                }
                self.finish(id, Value::Array(items))
            }
            b'D' => {
                let id = self.new_object();
                let v = num(self.f64()?);
                self.finish(id, v)
            }
            b'y' | b'x' => {
                let id = self.new_object();
                self.finish(id, Value::Bool(tag == b'y'))
            }
            b'n' => {
                let id = self.new_object();
                let v = num(self.f64()?);
                self.finish(id, v)
            }
            b's' => {
                let id = self.new_object();
                let v = self.value()?;
                self.finish(id, v)
            }
            b'R' => {
                let id = self.new_object();
                let v = self.value()?;
                self.varint()?;
                self.finish(id, v)
            }
            b';' | b'\'' => {
                let id = self.new_object();
                let end = if tag == b';' { b':' } else { b',' };
                let mut items = Vec::new();
                loop {
                    if self.peek_tag()? == end {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.value()?);
                }
                self.varint()?;
                let v = if tag == b';' { Value::Array(items.chunks(2).map(|c| Value::Array(c.to_vec())).collect()) } else { Value::Array(items) };
                self.finish(id, v)
            }
            b'?' => {
                self.varint()?;
                return self.value();
            }
            t => bail!("unsupported V8 value tag {t:#04x} ({:?}) at byte {} (format v{})", t as char, self.pos - 1, self.version),
        })
    }
}

/// Decode a V8-serialised value starting at its `0xFF version` header.
pub fn decode(b: &[u8]) -> Result<Value> {
    let mut r = Reader { b, pos: 0, version: 0, depth: 0, spent: Cost::default(), objects: Vec::new(), started: Vec::new() };
    // Older data may lack the header; newer always has it.
    if r.peek_tag()? == 0xff {
        r.pos += 1;
        r.version = r.varint()?;
    }
    r.value()
}

#[cfg(test)]
pub(crate) mod encode {
    //! A minimal V8 serialiser for building test fixtures.
    use serde_json::Value;

    pub fn varint(out: &mut Vec<u8>, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    fn string(out: &mut Vec<u8>, s: &str) {
        if s.chars().all(|c| (c as u32) < 256) {
            out.push(b'"');
            varint(out, s.chars().count() as u64);
            out.extend(s.chars().map(|c| c as u8));
        } else {
            let u: Vec<u16> = s.encode_utf16().collect();
            // Two-byte strings are aligned: the payload after tag + length starts on an even offset.
            let mut len = Vec::new();
            varint(&mut len, u.len() as u64 * 2);
            if (out.len() + 1 + len.len()) % 2 == 1 {
                out.push(0);
            }
            out.push(b'c');
            out.extend(len);
            u.iter().for_each(|x| out.extend(x.to_le_bytes()));
        }
    }

    pub fn value(out: &mut Vec<u8>, v: &Value) {
        match v {
            Value::Null => out.push(b'0'),
            Value::Bool(b) => out.push(if *b { b'T' } else { b'F' }),
            Value::Number(n) => {
                if let Some(i) = n.as_i64().filter(|i| i32::try_from(*i).is_ok()) {
                    out.push(b'I');
                    let i = i as i32;
                    varint(out, ((i << 1) ^ (i >> 31)) as u32 as u64);
                } else {
                    out.push(b'N');
                    out.extend(n.as_f64().unwrap().to_le_bytes());
                }
            }
            Value::String(s) => string(out, s),
            Value::Array(a) => {
                out.push(b'A');
                varint(out, a.len() as u64);
                a.iter().for_each(|x| value(out, x));
                out.push(b'$');
                varint(out, 0);
                varint(out, a.len() as u64);
            }
            Value::Object(m) => {
                out.push(b'o');
                for (k, x) in m {
                    string(out, k);
                    value(out, x);
                }
                out.push(b'{');
                varint(out, m.len() as u64);
            }
        }
    }

    pub fn serialize(v: &Value) -> Vec<u8> {
        let mut out = vec![0xff, 0x0f];
        value(&mut out, v);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_plain_data() {
        let v = json!({"path": "/a/b", "n": 3, "neg": -7, "f": 1.5, "t": true, "z": null,
                       "list": [1, "x", {"k": false}], "jp": "日本語 テスト", "big": 5_000_000_000u64});
        assert_eq!(decode(&encode::serialize(&v)).unwrap(), {
            let mut w = v.clone();
            w["big"] = json!(5_000_000_000.0);
            w
        });
    }

    #[test]
    fn references_holes_and_sparse() {
        // {a: {x:1}, b: <ref to the same object>}, [1, <hole>], sparse [ , "s"]
        let b = [
            0xff, 0x0f, b'o', b'"', 1, b'a', b'o', b'"', 1, b'x', b'I', 2, b'{', 1, b'"', 1, b'b', b'^', 1, b'"', 1, b'h',
            b'A', 2, b'I', 2, b'-', b'$', 0, 2, b'"', 1, b's', b'a', 2, b'I', 2, b'"', 1, b'q', b'@', 1, 2, b'{', 4,
        ];
        let v = decode(&b).unwrap();
        assert_eq!(v, json!({"a": {"x": 1}, "b": {"x": 1}, "h": [1, null], "s": [null, "q"]}));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(&[0xff, 0x0f, 0x01]).is_err());
        assert!(decode(&[0xff, 0x0f, b'o', b'"', 5, b'a']).is_err());
    }

    // ---- untrusted-input regressions (from the security review's proofs of concept) ----

    /// Deep nesting (one byte per level via boxed primitives) used to overflow the stack and
    /// abort the process; now an error, even on a small stack.
    #[test]
    fn deep_nesting_is_an_error() {
        let mut b = vec![0xff, 0x0f];
        b.extend(std::iter::repeat_n(b's', 200_000));
        b.push(b'T');
        let h = std::thread::Builder::new().stack_size(256 << 10).spawn(move || decode(&b).map(|_| ()).map_err(|e| e.to_string())).unwrap();
        let r = h.join().expect("decoder crashed");
        assert!(r.unwrap_err().contains("nested"));
        // Nested arrays and objects too.
        let mut b = vec![0xff, 0x0f];
        for _ in 0..10_000 {
            b.extend([b'A', 1]);
        }
        assert!(decode(&b).is_err());
        // Real data nesting still decodes.
        let mut v = json!(1);
        for _ in 0..(MAX_DEPTH - 2) {
            v = json!([v]);
        }
        assert_eq!(decode(&encode::serialize(&v)).unwrap(), v);
    }

    /// n nested arrays of two back-references to the previous one = 2^n nodes from ~10n bytes.
    #[test]
    fn backref_bomb_hits_the_budget() {
        let n: u32 = 40;
        let mut b = vec![0xff, 0x0f, b'A'];
        encode::varint(&mut b, n as u64);
        b.extend([b'A', 2, b'I', 0, b'I', 0, b'$', 0, 2]); // id 1
        for k in 2..=n {
            b.push(b'A');
            b.push(2);
            for _ in 0..2 {
                b.push(b'^');
                encode::varint(&mut b, (k - 1) as u64);
            }
            b.extend([b'$', 0, 2]);
        }
        b.extend([b'$', 0]);
        encode::varint(&mut b, n as u64);
        let t = std::time::Instant::now();
        let e = decode(&b).unwrap_err();
        assert!(e.to_string().contains("too large"), "{e}");
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
        // A few shared references (what real data has) still decode.
        let ok = [0xff, 0x0f, b'o', b'"', 1, b'a', b'o', b'"', 1, b'x', b'I', 2, b'{', 1, b'"', 1, b'b', b'^', 1, b'{', 2];
        assert_eq!(decode(&ok).unwrap(), json!({"a": {"x": 1}, "b": {"x": 1}}));
    }

    /// Sparse arrays: a few bytes used to allocate 2^20 nulls each.
    #[test]
    fn sparse_bomb_is_an_error() {
        let mut b = vec![0xff, 0x0f, b'A', 64];
        for _ in 0..64 {
            b.push(b'a');
            encode::varint(&mut b, 1 << 20);
            b.extend([b'@', 0, 0]);
        }
        b.extend([b'$', 0, 64]);
        assert!(decode(&b).unwrap_err().to_string().contains("sparse"));
        // Many maximal sparse arrays still hit the node budget.
        let mut b = vec![0xff, 0x0f, b'A'];
        encode::varint(&mut b, 200);
        for _ in 0..200 {
            b.push(b'a');
            encode::varint(&mut b, MAX_SPARSE_LEN as u64);
            b.extend([b'@', 0, 0]);
        }
        b.extend([b'$', 0]);
        encode::varint(&mut b, 200);
        assert!(decode(&b).unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn huge_lengths_do_not_overflow() {
        let mut b = vec![0xff, 0x0f, b'"'];
        encode::varint(&mut b, u64::MAX);
        assert!(decode(&b).is_err());
        let mut b = vec![0xff, 0x0f, b'Z'];
        encode::varint(&mut b, u64::MAX);
        assert!(decode(&b).is_err());
    }
}
