//! Synthetic Synapse stores for tests: a LevelDB directory written from scratch (log file with
//! fragmented records, a Snappy table file, MANIFEST/CURRENT) holding IndexedDB records whose
//! values use the Blink/V8 encodings, a blob file and a Snappy-wrapped value. No real data.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::idb::prefix;
use super::v8::encode::{serialize, varint};

fn swl(s: &str) -> Vec<u8> {
    let u: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    varint(&mut out, u.len() as u64);
    u.iter().for_each(|x| out.extend(x.to_be_bytes()));
    out
}

fn string_key(s: &str) -> Vec<u8> {
    let mut k = vec![1];
    k.extend(swl(s));
    k
}

/// LevelDB log-format writer (32 KiB blocks, fragments when a record crosses a block).
fn log_file(records: &[Vec<u8>]) -> Vec<u8> {
    const BLOCK: usize = 32768;
    let mut out = Vec::new();
    for r in records {
        let mut rest: &[u8] = r;
        let mut first = true;
        loop {
            let left = BLOCK - out.len() % BLOCK;
            if left < 7 {
                out.extend(std::iter::repeat_n(0u8, left));
                continue;
            }
            let n = rest.len().min(left - 7);
            let last = n == rest.len();
            let kind = match (first, last) {
                (true, true) => 1,
                (true, false) => 2,
                (false, false) => 3,
                (false, true) => 4,
            };
            out.extend([0, 0, 0, 0]);
            out.extend((n as u16).to_le_bytes());
            out.push(kind);
            out.extend(&rest[..n]);
            rest = &rest[n..];
            first = false;
            if last {
                break;
            }
        }
    }
    out
}

fn batch(seq: u64, ops: &[(Vec<u8>, Option<Vec<u8>>)]) -> Vec<u8> {
    let mut b = seq.to_le_bytes().to_vec();
    b.extend((ops.len() as u32).to_le_bytes());
    for (k, v) in ops {
        b.push(v.is_some() as u8);
        varint(&mut b, k.len() as u64);
        b.extend(k);
        if let Some(v) = v {
            varint(&mut b, v.len() as u64);
            b.extend(v);
        }
    }
    b
}

fn block(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut restarts = Vec::new();
    let mut prev: Vec<u8> = Vec::new();
    for (i, (k, v)) in entries.iter().enumerate() {
        let shared = if i % 4 == 0 {
            restarts.push(out.len() as u32);
            0
        } else {
            prev.iter().zip(k).take_while(|(a, b)| a == b).count()
        };
        varint(&mut out, shared as u64);
        varint(&mut out, (k.len() - shared) as u64);
        varint(&mut out, v.len() as u64);
        out.extend(&k[shared..]);
        out.extend(v);
        prev = k.clone();
    }
    if restarts.is_empty() {
        restarts.push(0);
    }
    restarts.iter().for_each(|r| out.extend(r.to_le_bytes()));
    out.extend((restarts.len() as u32).to_le_bytes());
    out
}

/// A table file: entries split into data blocks of `per_block`, Snappy-compressed.
fn table(entries: &[(Vec<u8>, u64, Option<Vec<u8>>)], per_block: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = Vec::new();
    let put = |out: &mut Vec<u8>, raw: Vec<u8>, compress: bool| -> Vec<u8> {
        let (data, t) = if compress { (snap::raw::Encoder::new().compress_vec(&raw).unwrap(), 1u8) } else { (raw, 0u8) };
        let mut h = Vec::new();
        varint(&mut h, out.len() as u64);
        varint(&mut h, data.len() as u64);
        out.extend(&data);
        out.push(t);
        out.extend([0, 0, 0, 0]);
        h
    };
    for chunk in entries.chunks(per_block) {
        let ents: Vec<(Vec<u8>, Vec<u8>)> = chunk
            .iter()
            .map(|(k, seq, v)| {
                let mut ik = k.clone();
                ik.extend(((seq << 8) | v.is_some() as u64).to_le_bytes());
                (ik, v.clone().unwrap_or_default())
            })
            .collect();
        let last = ents.last().unwrap().0.clone();
        let h = put(&mut out, block(&ents), true);
        index.push((last, h));
    }
    let meta = put(&mut out, block(&[]), false);
    let idx = put(&mut out, block(&index), false);
    let mut footer = meta;
    footer.extend(idx);
    footer.resize(40, 0);
    footer.extend(0xdb4775248b80fb57u64.to_le_bytes());
    out.extend(footer);
    out
}

fn manifest(log_number: u64, tables: &[u64]) -> Vec<u8> {
    let mut e = Vec::new();
    varint(&mut e, 1);
    let cmp = b"idb_cmp1";
    varint(&mut e, cmp.len() as u64);
    e.extend(cmp);
    varint(&mut e, 2);
    varint(&mut e, log_number);
    for t in tables {
        varint(&mut e, 7);
        varint(&mut e, 0);
        varint(&mut e, *t);
        varint(&mut e, 100);
        varint(&mut e, 1);
        e.push(b'a');
        varint(&mut e, 1);
        e.push(b'z');
    }
    log_file(&[e])
}

/// Blink envelope around a V8 value: version 20 (no trailer) or 21 (with a trailer record).
fn blink(v: &Value, trailer: bool) -> Vec<u8> {
    let mut out = if trailer { vec![0xff, 21, 0xfe] } else { vec![0xff, 20] };
    if trailer {
        out.extend([0u8; 12]);
    }
    out.extend(serialize(v));
    out
}

fn record(blink: Vec<u8>) -> Vec<u8> {
    let mut v = vec![1];
    v.extend(blink);
    v
}

fn node(path: &str, payload: &Value) -> Value {
    let body = json!({ "payload": payload.to_string() }).to_string();
    json!({ "path": path, "common": { "revision": 3, "timestamp": 1.7e12, "body": body, "contentType": "application/json" }, "local": { "body": false } })
}

pub const ACCOUNT: &str = "/RZR_00000000000000000000000000000000";

/// Write a complete synthetic store below `root`; returns the `.indexeddb.leveldb` path.
pub fn write_store(root: &Path) -> PathBuf {
    let dir = root.join("https_apps.razer.com_0.indexeddb.leveldb");
    let blobs = root.join("https_apps.razer.com_0.indexeddb.blob");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&blobs);
    std::fs::create_dir_all(&dir).unwrap();
    let (db, os) = (3u64, 1u64);
    let data_key = |path: &str| {
        let mut k = prefix(db, os, 1);
        k.extend(string_key(path));
        k
    };
    let mut meta = vec![0u8, 0, 0, 0, 201];
    meta.extend(swl("https_apps.razer.com_0@1"));
    meta.extend(swl("rzrremotestorage"));
    let mut store_name = prefix(db, 0, 0);
    store_name.extend([50, os as u8, 0]);
    let name_val: Vec<u8> = "nodes".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();

    let p = |s: &str| format!("{ACCOUNT}/{s}");
    let kb_profile = json!({
        "guid": "11111111-2222-3333-4444-555555555555", "name": "Typing — 日本語", "pollingRate": 1000,
        "brightness": {"isEnabled": true, "value": 70},
        "quickEffects": {"selectedEffectId": 3},
        "mappings": [{"inputType": "DKMInput", "inputID": "DKM_M_01", "isHyperShift": false, "outputType": "macroGroup",
                      "macroGroup": {"guid": "aaaaaaaa-0000-0000-0000-000000000001", "name": "Hello", "macroPlaybackOption": "Once", "repeatCount": 1}}]
    });
    let mouse_profile = json!({
        "guid": "66666666-2222-3333-4444-555555555555", "name": "Aim",
        "dpiStages": {"active": 2, "enable": true, "stages": [{"x": 800, "y": 800, "visible": true}, {"x": 1600, "y": 1600, "visible": true}]},
        "mappings": [{"inputType": "MouseInput", "inputID": "Button5", "isHyperShift": false, "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_E", "modifiers": []}}]
    });
    let mac = json!({"guid": "aaaaaaaa-0000-0000-0000-000000000001", "name": "Hello", "macroPlaybackOption": "", "delaySetting": 0,
                     "macroEvents": [{"Type": "actionBar"}, {"Type": 1, "KeyEvent": {"Makecode": 72, "State": 0}}, {"Type": 0, "Number": 0.05}, {"Type": 1, "KeyEvent": {"Makecode": 72, "State": 1}}]});
    // Large enough to fragment across log blocks.
    let big_layers: Vec<Value> = (0..400)
        .map(|i| json!({"id": format!("l{i}"), "type": "layer", "name": "starlight", "title": "STARLIGHT", "visible": true,
                        "regions": [{"params": {"randomColor": true, "density": 10, "duration": 2000}, "devices": [{"productId": 653, "leds": [{"row": 1, "col": 2}]}]}]}))
        .collect();
    let studio = json!({"id": "s1", "title": "Stars", "devices": [{"productId": 653, "productName": "Razer BlackWidow V4 Pro"}], "layers": big_layers});

    // Table file 5 (live): metadata, an old macro version, and a profile deleted later.
    let mut t: Vec<(Vec<u8>, u64, Option<Vec<u8>>)> = vec![
        (meta.clone(), 1, Some(vec![db as u8])),
        (store_name.clone(), 2, Some(name_val.clone())),
        (data_key(&p("macro/profiles/aaaaaaaa-0000-0000-0000-000000000001")), 3, Some(record(blink(&node("x", &json!({"guid": "old", "name": "Old"})), false)))),
        (data_key(&p("devices/170/profiles/deleted")), 4, Some(record(blink(&node("x", &json!({"guid": "deleted", "name": "Gone"})), true)))),
        (data_key(&p("devices/653/deviceMetadatas/SERIAL")), 5, Some(record(blink(&node("x", &json!({"name": "Razer BlackWidow V4 Pro", "activeProfileGuid": "11111111-2222-3333-4444-555555555555"})), false)))),
    ];
    t.sort_by(|a, b| a.0.cmp(&b.0));
    std::fs::write(dir.join("000005.ldb"), table(&t, 2)).unwrap();
    // Table file 4 is obsolete (not in the MANIFEST): its record must not come back.
    std::fs::write(dir.join("000004.ldb"), table(&[(data_key(&p("devices/170/profiles/stale")), 1, Some(record(blink(&node("x", &json!({"guid": "stale", "name": "Stale"})), false))))], 4)).unwrap();

    // The keyboard profile is stored in a blob file; the studio profile is Snappy-wrapped.
    let kb_key = p("devices/653/profiles/11111111-2222-3333-4444-555555555555");
    let blob_number = 0x105u64;
    let bdir = blobs.join("3").join("01");
    std::fs::create_dir_all(&bdir).unwrap();
    let kb_ssv = blink(&node(&kb_key, &kb_profile), true);
    std::fs::write(bdir.join(format!("{blob_number:x}")), &kb_ssv).unwrap();
    let mut wrapped = vec![0xff, 0x11, 0x01];
    varint(&mut wrapped, kb_ssv.len() as u64);
    varint(&mut wrapped, 0);
    let mut blob_list_key = prefix(db, os, 3);
    blob_list_key.extend(string_key(&kb_key));
    let mut blob_list = vec![0u8];
    varint(&mut blob_list, blob_number);
    blob_list.extend(swl("application/vnd.blink-idb-value-wrapper"));
    varint(&mut blob_list, kb_ssv.len() as u64);

    let mut snappy = vec![0xff, 0x11, 0x02];
    snappy.extend(snap::raw::Encoder::new().compress_vec(&blink(&node("x", &studio), false)).unwrap());

    let log = log_file(&[
        batch(10, &[
            (data_key(&p("macro/profiles/aaaaaaaa-0000-0000-0000-000000000001")), Some(record(blink(&node("x", &mac), false)))),
            (data_key(&kb_key), Some(record(wrapped))),
            (blob_list_key, Some(blob_list)),
        ]),
        batch(13, &[
            (data_key(&p("devices/170/profiles/66666666-2222-3333-4444-555555555555")), Some(record(blink(&node("x", &json!({"name": "first"})), false)))),
            (data_key(&p("devices/170/profiles/deleted")), None),
        ]),
        batch(15, &[
            (data_key(&p("devices/170/profiles/66666666-2222-3333-4444-555555555555")), Some(record(blink(&node("x", &mouse_profile), false)))),
            (data_key(&p("chromaStudio/chromaStudioProfiles/s1")), Some(record(snappy))),
            (data_key(&format!("{ACCOUNT}/devices/")), Some(record(blink(&json!({"path": "/", "common": {"body": true}}), false)))),
        ]),
    ]);
    std::fs::write(dir.join("000006.log"), log).unwrap();
    // An old log below the manifest's log number holds a value that was compacted away.
    std::fs::write(dir.join("000003.log"), log_file(&[batch(99, &[(data_key(&p("devices/170/profiles/ancient")), Some(record(blink(&node("x", &json!({"name": "Ancient"})), false))))])])).unwrap();
    std::fs::write(dir.join("MANIFEST-000002"), manifest(6, &[5])).unwrap();
    std::fs::write(dir.join("CURRENT"), "MANIFEST-000002\n").unwrap();
    std::fs::write(dir.join("LOCK"), "").unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synapse_import::{read_store, scan};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsuba-synapse-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_synthetic_store() {
        let root = tmp("read");
        let dir = write_store(&root);
        let before: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        let d = read_store(&dir).unwrap();
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        let keys: Vec<&str> = d.nodes.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "chromaStudio/chromaStudioProfiles/s1",
                "devices/170/profiles/66666666-2222-3333-4444-555555555555",
                "devices/653/deviceMetadatas/SERIAL",
                "devices/653/profiles/11111111-2222-3333-4444-555555555555",
                "macro/profiles/aaaaaaaa-0000-0000-0000-000000000001",
            ]
        );
        assert_eq!(d.nodes["devices/170/profiles/66666666-2222-3333-4444-555555555555"]["name"], "Aim");
        assert_eq!(d.nodes["macro/profiles/aaaaaaaa-0000-0000-0000-000000000001"]["name"], "Hello");
        assert_eq!(d.nodes["devices/653/profiles/11111111-2222-3333-4444-555555555555"]["name"], "Typing — 日本語");
        assert_eq!(d.nodes["chromaStudio/chromaStudioProfiles/s1"]["layers"].as_array().unwrap().len(), 400);
        // Nothing was written to the store.
        let after: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(before, after);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn plans_synthetic_store() {
        let root = tmp("plan");
        let dir = write_store(&root);
        let plan = scan(&dir).unwrap();
        assert_eq!(plan.devices.len(), 2);
        let kb = plan.devices.iter().find(|d| d.pid.as_deref() == Some("028d")).unwrap();
        assert_eq!(kb.name, "Razer BlackWidow V4 Pro");
        assert!(kb.profiles[0].active);
        assert!(matches!(&kb.profiles[0].assignments[0], crate::synapse_import::plan::AssignmentPlan::Macro { key: 160, .. }));
        assert_eq!(kb.profiles[0].device_wide.effect, Some(crate::config::EffectConfig::Spectrum));
        let mouse = plan.devices.iter().find(|d| d.pid.as_deref() == Some("00aa")).unwrap();
        assert_eq!(mouse.profiles[0].performance.as_ref().unwrap().dpi_active_stage, Some(2));
        assert_eq!(plan.macros.len(), 1);
        assert_eq!(plan.macros[0].mac.events.len(), 3);
        assert_eq!(plan.lighting.len(), 1);
        // 400 identical neighbouring layers merge into one.
        assert_eq!(plan.lighting[0].layers, 1);
        // Plans contain no account id.
        assert!(!serde_json::to_string(&plan).unwrap().contains("RZR_"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn corrupt_store_is_an_error_not_a_panic() {
        let root = tmp("bad");
        let dir = root.join("x.indexeddb.leveldb");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_store(&dir).is_err());
        std::fs::write(dir.join("000001.ldb"), b"not a table").unwrap();
        assert!(read_store(&dir).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
