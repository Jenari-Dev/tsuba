//! "Import from Synapse": read the profiles, macros and Chroma Studio effects that Synapse 4
//! keeps on a Windows install (dual boot) and turn them into Tsuba settings.
//!
//! Synapse 4 is an Electron app; its data lives in Chromium IndexedDB
//! (`%LOCALAPPDATA%\Razer\RazerAppEngine\User Data\Default\IndexedDB\
//! https_apps.razer.com_0.indexeddb.leveldb`). That store is read here directly and
//! never written: [`leveldb`] reads the LevelDB files, [`idb`] the IndexedDB layer and
//! [`v8`] the serialised values. [`plan`] maps what was found onto Tsuba's model and lists
//! everything that doesn't map as a warning.

pub mod idb;
pub mod keys;
pub mod leveldb;
pub mod plan;
pub mod v8;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

pub use plan::{build_plan, Plan};

/// Where Synapse keeps its IndexedDB store below a Windows user's profile folder.
pub const STORE_SUFFIX: &str = "AppData/Local/Razer/RazerAppEngine/User Data/Default/IndexedDB/https_apps.razer.com_0.indexeddb.leveldb";

/// Synapse stores found below mounted filesystems (`/run/media/*/*`, `/mnt/*`, `/media/*/*`)
/// and below `extra` (a mount point, a Windows `Users` folder, a user folder or the store
/// itself). Each is the `….indexeddb.leveldb` directory.
pub fn find_synapse_stores(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for pat in ["/run/media", "/media"] {
        for user in read_dirs(Path::new(pat)) {
            roots.extend(read_dirs(&user));
        }
    }
    roots.extend(read_dirs(Path::new("/mnt")));
    roots.extend(extra.iter().cloned());
    stores_below(&roots)
}

/// Stores at or below each root (see [`find_synapse_stores`]).
pub fn stores_below(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in roots {
        for cand in candidates(r) {
            if cand.join("CURRENT").is_file() && !out.contains(&cand) {
                out.push(cand);
            }
        }
    }
    out
}

fn read_dirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// Store directories a root may contain: the root itself, root/<STORE_SUFFIX>,
/// root/*/<STORE_SUFFIX> (a Users folder) and root/Users/*/<STORE_SUFFIX> (a volume).
fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut v = vec![root.to_path_buf(), root.join(STORE_SUFFIX)];
    for dir in [root.to_path_buf(), root.join("Users")] {
        for user in read_dirs(&dir) {
            v.push(user.join(STORE_SUFFIX));
        }
    }
    v
}

/// What the store holds, keyed by path below the Synapse account folder.
#[derive(Debug, Default, Clone)]
pub struct SynapseData {
    /// `devices/653/profiles/<guid>` etc. → the decoded JSON body (`payload`).
    pub nodes: BTreeMap<String, Value>,
    /// Lighting-engine device manifests by Synapse product id (`ledConfig` etc.).
    pub manifests: BTreeMap<u32, Value>,
    /// Records that couldn't be decoded (path → reason).
    pub errors: Vec<(String, String)>,
}

/// The body of a `nodes` record: `{path, common: {body}, local: {body}}`, `local` preferred.
/// `body` is a JSON string `{"payload": "<JSON string>"}`.
fn node_body(v: &Value) -> Option<Value> {
    for side in ["local", "common"] {
        let Some(body) = v.get(side).and_then(|s| s.get("body")).and_then(|b| b.as_str()) else { continue };
        let Ok(outer) = serde_json::from_str::<Value>(body) else { continue };
        return Some(match outer.get("payload") {
            Some(Value::String(p)) => serde_json::from_str(p).unwrap_or(Value::String(p.clone())),
            Some(p) => p.clone(),
            None => outer,
        });
    }
    None
}

/// `/RZR_<account>/devices/653/profiles/<guid>` → `devices/653/profiles/<guid>` (the account
/// id is dropped so it never reaches the UI or logs).
pub fn relative_path(key: &str) -> Option<String> {
    let rest = key.strip_prefix('/')?;
    let (account, path) = rest.split_once('/')?;
    (!account.is_empty() && !path.is_empty()).then(|| path.trim_end_matches('/').to_string())
}

/// Read a Synapse store (the `….indexeddb.leveldb` directory). Nothing is written.
pub fn read_store(dir: &Path) -> Result<SynapseData> {
    let store = idb::Store::open(dir)?;
    let (db, os) = store.find_store("rzrremotestorage", "nodes").context("this IndexedDB store has no Synapse profile data (rzrremotestorage)")?;
    let mut out = SynapseData::default();
    for (key, value) in store.records(db, os) {
        let Some(path) = relative_path(&key) else { continue };
        match value {
            Ok(v) => {
                if let Some(body) = node_body(&v) {
                    out.nodes.insert(path, body);
                }
            }
            Err(e) => out.errors.push((path, format!("{e:#}"))),
        }
    }
    if let Some((db, os)) = store.find_store("rzrLightingEngineManifest", "manifest") {
        for (key, value) in store.records(db, os) {
            let Ok(v) = value else { continue };
            let Some(pid) = key.split('_').next().and_then(|p| p.parse::<u32>().ok()) else { continue };
            if let Some(m) = v.get("deviceManifest") {
                out.manifests.insert(pid, m.clone());
            }
        }
    }
    // Macros also live in their own database; take any the nodes tree lacks.
    if let Some((db, os)) = store.find_store("macros", "macros") {
        for (key, value) in store.records(db, os) {
            let path = format!("macro/profiles/{key}");
            if let (Ok(v @ Value::Object(_)), false) = (value, out.nodes.contains_key(&path)) {
                out.nodes.insert(path, v);
            }
        }
    }
    Ok(out)
}

/// Read a store and build the import plan.
pub fn scan(dir: &Path) -> Result<Plan> {
    let data = read_store(dir)?;
    Ok(build_plan(&data, &dir.display().to_string()))
}

#[cfg(test)]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_drop_the_account() {
        assert_eq!(relative_path("/RZR_0123abc/devices/653/profiles/x").as_deref(), Some("devices/653/profiles/x"));
        assert_eq!(relative_path("/RZR_0123abc/macro/").as_deref(), Some("macro"));
        assert_eq!(relative_path("/"), None);
        assert_eq!(relative_path("/RZR_0123abc/"), None);
    }

    #[test]
    fn node_bodies() {
        let v = serde_json::json!({"path": "/x", "common": {"body": "{\"payload\":\"{\\\"name\\\":\\\"A\\\"}\"}"}, "local": {"body": true}});
        assert_eq!(node_body(&v), Some(serde_json::json!({"name": "A"})));
        let v = serde_json::json!({"common": {"body": "{\"payload\":\"{\\\"name\\\":\\\"A\\\"}\"}"}, "local": {"body": "{\"payload\":\"{\\\"name\\\":\\\"L\\\"}\"}"}});
        assert_eq!(node_body(&v), Some(serde_json::json!({"name": "L"})));
        assert_eq!(node_body(&serde_json::json!({"common": {"body": true}})), None);
    }

    #[test]
    fn finds_stores_below_a_volume() {
        let tmp = std::env::temp_dir().join(format!("tsuba-synapse-find-{}", std::process::id()));
        let store = tmp.join("vol/Users/someone").join(STORE_SUFFIX);
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("CURRENT"), "MANIFEST-000001\n").unwrap();
        std::fs::create_dir_all(tmp.join("vol/Users/other")).unwrap();
        assert_eq!(stores_below(&[tmp.join("vol")]), vec![store.clone()]);
        assert_eq!(stores_below(&[tmp.join("vol/Users")]), vec![store.clone()]);
        assert_eq!(stores_below(&[tmp.join("vol/Users/someone")]), vec![store.clone()]);
        assert_eq!(stores_below(&[store.clone()]), vec![store.clone()]);
        assert!(stores_below(&[tmp.join("vol/Users/other")]).is_empty());
        assert!(find_synapse_stores(&[tmp.join("vol")]).contains(&store));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// ccl_chromium_reader decodes a negative zigzag int32 `-n` as `-(n - 1)` (e.g. -1 as 0);
    /// treat exactly that difference as equal.
    fn same_but_ccl_zigzag(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| same_but_ccl_zigzag(p, q))),
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_but_ccl_zigzag(p, q)),
            (Value::Number(p), Value::Number(q)) => match (p.as_i64(), q.as_i64()) {
                (Some(p), Some(q)) => p == q || (p < 0 && q <= 0 && p == q - 1),
                _ => p == q,
            },
            (p, q) => p == q,
        }
    }

    /// Path of the first difference between two JSON values (types only, no contents).
    fn first_diff(a: &Value, b: &Value, at: String) -> String {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys().chain(y.keys()) {
                    match (x.get(k), y.get(k)) {
                        (Some(p), Some(q)) if p == q => {}
                        (Some(p), Some(q)) => return first_diff(p, q, format!("{at}.{k}")),
                        _ => return format!("{at}.{k} (present on one side only)"),
                    }
                }
                at
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    if p != q {
                        return first_diff(p, q, format!("{at}[{i}]"));
                    }
                }
                at
            }
            (x, y) => format!("{at}: here {} / oracle {}", kind(x), kind(y)),
        }
    }

    fn kind(v: &Value) -> String {
        match v {
            Value::Number(n) => format!("number({n})"),
            Value::Array(a) => format!("array[{}]", a.len()),
            Value::String(s) => format!("string[{}]", s.chars().count()),
            v => format!("{v}"),
        }
    }

    /// The scratch copy of a real store, if present: every value the Python reference reader
    /// (ccl_chromium_reader) decoded must decode to the same JSON here. Run with
    /// `SYNAPSE_STORE=<dir> SYNAPSE_ORACLE=<json> cargo test -- --ignored oracle`.
    #[test]
    #[ignore]
    fn matches_python_oracle() {
        let (Ok(dir), Ok(oracle)) = (std::env::var("SYNAPSE_STORE"), std::env::var("SYNAPSE_ORACLE")) else {
            eprintln!("SYNAPSE_STORE / SYNAPSE_ORACLE not set; skipped");
            return;
        };
        let oracle: serde_json::Map<String, Value> = serde_json::from_str(&std::fs::read_to_string(oracle).unwrap()).unwrap();
        let store = idb::Store::open(Path::new(&dir)).unwrap();
        let (mut same, mut total, mut zigzag) = (0, 0, 0);
        let mut bad = Vec::new();
        for (name, records) in &oracle {
            let (dbn, osn) = name.split_once('/').unwrap();
            let Some((db, os)) = store.find_store(dbn, osn) else {
                bad.push(format!("{name}: store not found"));
                continue;
            };
            let mine: BTreeMap<String, Result<Value>> = store.records(db, os).into_iter().collect();
            for (k, want) in records.as_object().unwrap() {
                total += 1;
                match mine.get(k) {
                    Some(Ok(got)) if got == want => same += 1,
                    Some(Ok(got)) if same_but_ccl_zigzag(got, want) => {
                        same += 1;
                        zigzag += 1;
                    }
                    Some(Ok(got)) => bad.push(format!("{name}: record #{total} differs at {}", first_diff(got, want, String::new()))),
                    Some(Err(e)) => bad.push(format!("{name}: record #{total}: {e:#}")),
                    None => bad.push(format!("{name}: record #{total} missing")),
                }
            }
            let extra = mine.iter().filter(|(k, v)| v.is_ok() && !records.as_object().unwrap().contains_key(*k)).count();
            if extra > 0 {
                eprintln!("{name}: {extra} record(s) decoded here that the oracle skipped");
            }
        }
        eprintln!("oracle: {same}/{total} identical ({zigzag} only differ by ccl's negative-int32 decoding)");
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }
}
