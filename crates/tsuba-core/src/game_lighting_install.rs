//! Game lighting bridge installer: puts Tsuba's bridge DLLs (the Windows libraries games that
//! support Razer Chroma load to set their lighting) into Wine / Proton prefixes, and takes them
//! out again.
//!
//! - Prefixes come from Steam (`steamapps/compatdata/<appid>/pfx` in every library), Lutris,
//!   Heroic and Bottles, plus folders the user adds.
//! - A 64-bit prefix gets the 64-bit DLL in `system32` and the 32-bit one in `syswow64`; a 32-bit
//!   prefix gets the 32-bit DLL in `system32`.
//! - A marker file (`drive_c/windows/tsuba-game-lighting.json`) records each file Tsuba wrote and
//!   a hash of its contents. A file that isn't recorded there, or whose contents changed since, is
//!   someone else's (e.g. the vendor's own library): it is never overwritten or removed.
//! - No registry change is needed: games find the library by file name in the DLL search path.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::fsutil::write_atomic;
use crate::games;

/// File names games load (functionally required; the contents are Tsuba's own).
pub const DLL64: &str = "RzChromaSDK64.dll";
pub const DLL32: &str = "RzChromaSDK.dll";
/// Marker file, relative to the prefix.
pub const MARKER: &str = "drive_c/windows/tsuba-game-lighting.json";
/// Packaged DLLs: `<dir>/x64/RzChromaSDK64.dll`, `<dir>/x86/RzChromaSDK.dll`.
pub const SYSTEM_DLL_DIR: &str = "/usr/share/tsuba/game-lighting";

/// Where the bridge DLLs to install are: `$TSUBA_GAME_LIGHTING_DLLS`, the packaged copy, or (when
/// running from a source checkout) `shim/build` next to the build tree.
pub fn default_dll_dir() -> Option<PathBuf> {
    let ok = |d: &Path| d.join("x64").join(DLL64).is_file() && d.join("x86").join(DLL32).is_file();
    if let Some(d) = std::env::var_os("TSUBA_GAME_LIGHTING_DLLS").map(PathBuf::from) {
        return ok(&d).then_some(d);
    }
    let sys = PathBuf::from(SYSTEM_DLL_DIR);
    if ok(&sys) {
        return Some(sys);
    }
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().map(|a| a.join("shim").join("build")).find(|d| ok(d)).or_else(|| dev_dll_dir().filter(|d| ok(d)))
}

/// The source checkout's `shim/build` (builds installed by hand, e.g. into ~/.local/bin); packaged
/// builds (`--cfg tsuba_packaged`) use /usr/share/tsuba/game-lighting only.
#[cfg(any(not(tsuba_packaged), test))]
fn dev_dll_dir() -> Option<PathBuf> {
    Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shim/build")))
}
#[cfg(all(tsuba_packaged, not(test)))]
fn dev_dll_dir() -> Option<PathBuf> {
    None
}

// ---- prefixes ------------------------------------------------------------------------------

/// One Wine prefix a game runs in.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prefix {
    /// Stable key: "steam:<appid>", "lutris:<slug>", "heroic:<app>", "bottles:<name>", "custom:<path>".
    pub id: String,
    /// "steam" | "lutris" | "heroic" | "bottles" | "custom"
    pub source: String,
    pub name: String,
    /// The prefix folder (the one with `drive_c`).
    pub path: PathBuf,
    /// Cover art `file://` URL ("" if none).
    pub art: String,
}

impl Prefix {
    fn new(source: &str, id: &str, name: &str, path: PathBuf, art: &str) -> Self {
        Self { id: format!("{source}:{id}"), source: source.into(), name: name.into(), path, art: art.into() }
    }
}

/// `true` when `p` looks like an initialised Wine prefix.
pub fn is_prefix(p: &Path) -> bool {
    p.join("drive_c").join("windows").join("system32").is_dir()
}

/// 64-bit prefixes have a `syswow64` folder.
pub fn is_win64(p: &Path) -> bool {
    p.join("drive_c").join("windows").join("syswow64").is_dir()
}

/// All prefixes found for this user (plus the custom ones from the settings file).
pub fn prefixes() -> Vec<Prefix> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let mut out = scan_prefixes(&home);
    for p in load_settings().custom {
        let path = PathBuf::from(&p);
        if !out.iter().any(|x| x.path == path) {
            out.push(custom_prefix(&path));
        }
    }
    out
}

fn custom_prefix(path: &Path) -> Prefix {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned());
    Prefix::new("custom", &path.to_string_lossy(), &name, path.to_path_buf(), "")
}

/// Accept a prefix folder, or a folder that contains `pfx/` (a Proton compatdata entry).
pub fn resolve_prefix(path: &Path) -> Option<PathBuf> {
    if is_prefix(path) {
        return Some(path.to_path_buf());
    }
    let pfx = path.join("pfx");
    is_prefix(&pfx).then_some(pfx)
}

/// Scan with an explicit home directory (tests use a synthetic one).
pub fn scan_prefixes(home: &Path) -> Vec<Prefix> {
    let mut out = Vec::new();
    out.extend(scan_steam(home));
    let known = games::scan(home);
    let name_of = |source: &str, id: &str| known.iter().find(|g| g.source == source && g.id == id).map(|g| g.name.clone());
    out.extend(scan_lutris(home, &name_of));
    out.extend(scan_heroic(home, &name_of));
    out.extend(scan_bottles(home));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    // The same prefix can be reached twice (e.g. ~/.steam/steam is a link): keep the first.
    let mut seen = BTreeSet::new();
    out.retain(|p| seen.insert(std::fs::canonicalize(&p.path).unwrap_or_else(|_| p.path.clone())));
    out
}

fn scan_steam(home: &Path) -> Vec<Prefix> {
    let mut out = Vec::new();
    for root in games::steam_roots(home) {
        let mut libs = vec![root.clone()];
        if let Ok(t) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            for l in games::library_paths(&t) {
                if !libs.contains(&l) {
                    libs.push(l);
                }
            }
        }
        // appid → name from every library's manifests (a prefix may live in another library).
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for lib in &libs {
            let Ok(rd) = std::fs::read_dir(lib.join("steamapps")) else { continue };
            for e in rd.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if f.starts_with("appmanifest_") && f.ends_with(".acf") {
                    if let Some((id, name, _)) = std::fs::read_to_string(e.path()).ok().as_deref().and_then(games::parse_manifest) {
                        names.insert(id, name);
                    }
                }
            }
        }
        for lib in &libs {
            let Ok(rd) = std::fs::read_dir(lib.join("steamapps").join("compatdata")) else { continue };
            for e in rd.flatten() {
                let appid = e.file_name().to_string_lossy().into_owned();
                let pfx = e.path().join("pfx");
                if !appid.chars().all(|c| c.is_ascii_digit()) || appid == "0" || !is_prefix(&pfx) {
                    continue;
                }
                let name = names.get(&appid).cloned().unwrap_or_else(|| format!("Steam app {appid}"));
                let lower = name.to_lowercase();
                if lower.starts_with("proton") || lower.contains("steam linux runtime") || lower.starts_with("steamworks") {
                    continue;
                }
                out.push(Prefix::new("steam", &appid, &name, pfx, &games::steam_art(&root, &appid)));
            }
        }
    }
    out
}

fn expand_home(p: &str, home: &Path) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

fn scan_lutris(home: &Path, name_of: &dyn Fn(&str, &str) -> Option<String>) -> Vec<Prefix> {
    let mut out = Vec::new();
    for rel in [".config/lutris/games", ".var/app/net.lutris.Lutris/config/lutris/games"] {
        let Ok(rd) = std::fs::read_dir(home.join(rel)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "yml") {
                continue;
            }
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let mut section = String::new();
            let mut prefix = None;
            for (indent, k, v) in games::yaml_pairs(&text) {
                if indent == 0 {
                    section = k;
                } else if section == "game" && k == "prefix" && !v.is_empty() {
                    prefix = Some(expand_home(&v, home));
                }
            }
            let Some(pfx) = prefix.filter(|p| is_prefix(p)) else { continue };
            let name = name_of("lutris", &stem).unwrap_or_else(|| stem.clone());
            out.push(Prefix::new("lutris", &stem, &name, pfx, ""));
        }
    }
    out
}

fn scan_heroic(home: &Path, name_of: &dyn Fn(&str, &str) -> Option<String>) -> Vec<Prefix> {
    let mut out = Vec::new();
    for rel in [".config/heroic", ".var/app/com.heroicgameslauncher.hgl/config/heroic"] {
        let Ok(rd) = std::fs::read_dir(home.join(rel).join("GamesConfig")) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Some(v) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
            let Some(obj) = v.as_object() else { continue };
            for (app, cfg) in obj {
                let Some(pfx) = cfg.get("winePrefix").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) else { continue };
                let pfx = expand_home(pfx, home);
                // Proton-based configs point at the compatdata folder.
                let Some(pfx) = resolve_prefix(&pfx) else { continue };
                let name = name_of("heroic", app).unwrap_or_else(|| app.clone());
                out.push(Prefix::new("heroic", app, &name, pfx, ""));
            }
        }
    }
    out
}

fn scan_bottles(home: &Path) -> Vec<Prefix> {
    let mut out = Vec::new();
    for rel in [".local/share/bottles/bottles", ".var/app/com.usebottles.bottles/data/bottles/bottles"] {
        let Ok(rd) = std::fs::read_dir(home.join(rel)) else { continue };
        for e in rd.flatten() {
            let b = e.path();
            if !is_prefix(&b) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            out.push(Prefix::new("bottles", &name, &format!("{name} (Bottles)"), b, ""));
        }
    }
    out
}

// ---- marker + status -------------------------------------------------------------------------

/// FNV-1a 64 of a file's contents: identifies "the file Tsuba wrote" (not a security check).
fn fnv64(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn file_hash(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|d| fnv64(&d))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Marker {
    /// Path relative to the prefix → hash of the contents Tsuba wrote.
    files: BTreeMap<String, String>,
    /// While a file is being replaced: its previous hash (still Tsuba's if the update is cut short).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    previous: BTreeMap<String, String>,
}

fn read_marker(pfx: &Path) -> Marker {
    std::fs::read_to_string(pfx.join(MARKER)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_marker(pfx: &Path, m: &Marker) -> Result<()> {
    let p = pfx.join(MARKER);
    if m.files.is_empty() {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e).context("removing the marker file"),
            _ => return Ok(()),
        }
    }
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "about": "Files Tsuba's game lighting bridge installed in this prefix (uninstall removes only these).",
        "files": m.files,
        "previous": m.previous,
    }))?;
    write_atomic(&p, text.as_bytes())
}

/// Every DLL slot path either prefix layout can have (relative to the prefix). Marker entries
/// are only ever acted on when they are one of these.
fn known_slots() -> [String; 3] {
    [format!("drive_c/windows/system32/{DLL64}"), format!("drive_c/windows/syswow64/{DLL32}"), format!("drive_c/windows/system32/{DLL32}")]
}

/// Refuse prefixes whose Windows folders are links leading out of the prefix: installing would
/// write (and uninstalling remove) files somewhere else.
fn check_contained(pfx: &Path) -> Result<()> {
    let root = std::fs::canonicalize(pfx).with_context(|| format!("resolving {}", pfx.display()))?;
    for rel in ["drive_c", "drive_c/windows", "drive_c/windows/system32", "drive_c/windows/syswow64"] {
        let p = pfx.join(rel);
        if std::fs::symlink_metadata(&p).is_err() {
            continue;
        }
        let real = std::fs::canonicalize(&p).with_context(|| format!("resolving {}", p.display()))?;
        if !real.starts_with(&root) {
            bail!("{} leads outside the prefix (to {}); not touching it", p.display(), real.display());
        }
    }
    Ok(())
}

/// The DLL slots of a prefix: (path relative to the prefix, which build goes there).
fn slots(pfx: &Path) -> Vec<(String, Arch)> {
    if is_win64(pfx) {
        vec![(format!("drive_c/windows/system32/{DLL64}"), Arch::X64), (format!("drive_c/windows/syswow64/{DLL32}"), Arch::X86)]
    } else {
        vec![(format!("drive_c/windows/system32/{DLL32}"), Arch::X86)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Arch {
    X64,
    X86,
}

fn source(dll_dir: &Path, a: Arch) -> PathBuf {
    match a {
        Arch::X64 => dll_dir.join("x64").join(DLL64),
        Arch::X86 => dll_dir.join("x86").join(DLL32),
    }
}

/// What occupies one DLL slot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Missing,
    /// Tsuba's file, unchanged since it was installed.
    Ours,
    /// Someone else's file (not installed by Tsuba, changed since, or a link).
    Foreign,
}

fn slot_state(pfx: &Path, rel: &str, marker: &Marker) -> Slot {
    let p = pfx.join(rel);
    match std::fs::symlink_metadata(&p) {
        Err(_) => Slot::Missing,
        Ok(m) if !m.file_type().is_file() => Slot::Foreign,
        Ok(_) => match (marker.files.get(rel), file_hash(&p)) {
            (Some(h), Some(cur)) if *h == cur || marker.previous.get(rel) == Some(&cur) => Slot::Ours,
            _ => Slot::Foreign,
        },
    }
}

/// Install state of a prefix.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NotInstalled,
    Installed,
    /// Installed, but an older bridge build than the one available.
    Outdated,
    /// Some of Tsuba's files are missing (e.g. one was deleted).
    Partial,
    /// Another library with the same name is in the way.
    Foreign,
    /// The folder isn't a Wine prefix (any more).
    Invalid,
}

pub fn state(pfx: &Path, dll_dir: Option<&Path>) -> State {
    if !is_prefix(pfx) {
        return State::Invalid;
    }
    let marker = read_marker(pfx);
    let s: Vec<(String, Arch, Slot)> = slots(pfx).into_iter().map(|(rel, a)| {
        let st = slot_state(pfx, &rel, &marker);
        (rel, a, st)
    }).collect();
    if s.iter().any(|x| x.2 == Slot::Foreign) {
        return State::Foreign;
    }
    let ours = s.iter().filter(|x| x.2 == Slot::Ours).count();
    if ours == 0 {
        return State::NotInstalled;
    }
    if ours < s.len() {
        return State::Partial;
    }
    if let Some(dir) = dll_dir {
        let stale = s.iter().any(|(rel, a, _)| file_hash(&source(dir, *a)).is_some_and(|h| marker.files.get(rel) != Some(&h)));
        if stale {
            return State::Outdated;
        }
    }
    State::Installed
}

// ---- install / uninstall ---------------------------------------------------------------------

/// What one install / uninstall did.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub prefix: String,
    /// Files written or removed (relative to the prefix).
    pub changed: Vec<String>,
    /// Files left alone, with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Install (or update) the bridge in one prefix. Slots holding someone else's file are skipped.
pub fn install(pfx: &Path, dll_dir: &Path) -> Result<Report> {
    if !is_prefix(pfx) {
        bail!("{} is not a Wine prefix (no drive_c/windows/system32)", pfx.display());
    }
    check_contained(pfx)?;
    let mut marker = read_marker(pfx);
    let mut rep = Report { prefix: pfx.display().to_string(), ..Default::default() };
    for (rel, arch) in slots(pfx) {
        let src = source(dll_dir, arch);
        let data = std::fs::read(&src).with_context(|| format!("reading {}", src.display()))?;
        let hash = fnv64(&data);
        match slot_state(pfx, &rel, &marker) {
            Slot::Foreign => {
                rep.skipped.push((rel.clone(), "another library with this name is already installed here; left untouched".into()));
                continue;
            }
            Slot::Ours if marker.files.get(&rel) == Some(&hash) => {
                rep.skipped.push((rel.clone(), "already installed".into()));
                continue;
            }
            Slot::Ours | Slot::Missing => {}
        }
        let dest = pfx.join(&rel);
        // Record first (keeping the old hash): if we're cut short, either version is known as ours.
        if let Some(old) = marker.files.insert(rel.clone(), hash) {
            marker.previous.insert(rel.clone(), old);
        }
        write_marker(pfx, &marker)?;
        write_atomic(&dest, &data)?;
        rep.changed.push(rel);
    }
    if !marker.previous.is_empty() {
        marker.previous.clear();
        write_marker(pfx, &marker)?;
    }
    Ok(rep)
}

/// Remove Tsuba's files from a prefix (only files that are still exactly what Tsuba wrote).
pub fn uninstall(pfx: &Path) -> Result<Report> {
    check_contained(pfx)?;
    let mut marker = read_marker(pfx);
    let mut rep = Report { prefix: pfx.display().to_string(), ..Default::default() };
    let known = known_slots();
    for rel in marker.files.keys().cloned().collect::<Vec<_>>() {
        // The marker is a file in the prefix (anything running in it can edit it): only the
        // fixed DLL slots are ever touched; other entries are dropped.
        if !known.contains(&rel) {
            rep.skipped.push((rel.clone(), "not a bridge file location; ignored".into()));
            marker.files.remove(&rel);
            continue;
        }
        match slot_state(pfx, &rel, &marker) {
            Slot::Ours => {
                std::fs::remove_file(pfx.join(&rel)).with_context(|| format!("removing {rel}"))?;
                rep.changed.push(rel.clone());
            }
            Slot::Missing => {}
            Slot::Foreign => rep.skipped.push((rel.clone(), "changed since Tsuba installed it; left in place".into())),
        }
        marker.files.remove(&rel);
    }
    marker.previous.clear();
    write_marker(pfx, &marker)?;
    Ok(rep)
}

// ---- settings: custom prefixes + automatic install -----------------------------------------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Install into new prefixes as soon as they show up.
    #[serde(default)]
    pub auto_install: bool,
    /// Prefix folders the user added by hand.
    #[serde(default)]
    pub custom: Vec<String>,
    /// Prefix ids already seen (so "new" means new since the last scan).
    #[serde(default)]
    pub known: Vec<String>,
}

fn settings_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("game-lighting.json")
}

pub fn load_settings() -> Settings {
    load_settings_from(&settings_path())
}

pub fn load_settings_from(p: &Path) -> Settings {
    std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> Result<()> {
    save_settings_to(&settings_path(), s)
}

pub fn save_settings_to(p: &Path, s: &Settings) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    write_atomic(p, serde_json::to_string_pretty(s)?.as_bytes())
}

/// Install into every prefix in `list` that doesn't hold someone else's library.
pub fn install_all(list: &[Prefix], dll_dir: &Path) -> Vec<Result<Report>> {
    list.iter().filter(|p| state(&p.path, None) != State::Foreign).map(|p| install(&p.path, dll_dir)).collect()
}

/// With automatic install on: install into prefixes that weren't seen before, and remember them.
/// Returns the prefixes installed into. Runs when the GUI scans and every two minutes in tsubad
/// (`game_autoinstall.rs`).
pub fn auto_install_new(list: &[Prefix], dll_dir: &Path, settings: &mut Settings) -> Vec<String> {
    let mut done = Vec::new();
    for p in list {
        if settings.known.contains(&p.id) {
            continue;
        }
        if settings.auto_install && state(&p.path, Some(dll_dir)) == State::NotInstalled && install(&p.path, dll_dir).is_ok() {
            done.push(p.id.clone());
        }
        settings.known.push(p.id.clone());
    }
    done
}

// ---- REST test pattern -----------------------------------------------------------------------

fn http(port: u16, method: &str, path: &str, body: &str) -> Result<serde_json::Value> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).with_context(|| format!("game lighting server not reachable on port {port}"))?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    write!(s, "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    let mut text = String::new();
    // Replies are small; don't let a misbehaving server make us buffer without limit.
    s.take(1 << 20).read_to_string(&mut text)?;
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status: u16 = head.split_whitespace().nth(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    if status != 200 {
        bail!("{method} {path}: HTTP {status} {v}");
    }
    Ok(v)
}

fn hue_bgr(h: f32) -> u32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let c = |v: f32| (v * 255.0).round() as u32;
    c(b) << 16 | c(g) << 8 | c(r)
}

/// Paint a moving rainbow on the keyboard and mouse through the game lighting REST server for
/// `secs` seconds, then end the session (devices go back to the user's lighting). This is what a
/// game does, so it checks the whole path.
pub fn test_pattern(port: u16, secs: f32) -> Result<String> {
    let init = serde_json::json!({
        "title": "Tsuba test pattern",
        "description": "Game lighting test from Tsuba",
        "author": { "name": "Tsuba", "contact": "-" },
        "device_supported": ["keyboard", "mouse"],
        "category": "application",
    });
    let v = http(port, "POST", "/razer/chromasdk", &init.to_string())?;
    let uri = v.get("uri").and_then(|u| u.as_str()).ok_or_else(|| anyhow!("unexpected reply: {v}"))?;
    let rest = uri.split_once("://").map(|x| x.1).unwrap_or(uri);
    let (hostport, path) = rest.split_once('/').ok_or_else(|| anyhow!("bad session uri {uri}"))?;
    let sport: u16 = hostport.rsplit_once(':').and_then(|x| x.1.parse().ok()).unwrap_or(port);
    let path = format!("/{}", path.trim_end_matches('/'));
    let start = std::time::Instant::now();
    let mut frames = 0u32;
    let result = (|| -> Result<()> {
        while start.elapsed().as_secs_f32() < secs {
            let t = start.elapsed().as_secs_f32() * 120.0;
            let kb: Vec<Vec<u32>> = (0..6).map(|r| (0..22).map(|c| hue_bgr(t + c as f32 * 16.0 + r as f32 * 6.0)).collect()).collect();
            let ms: Vec<Vec<u32>> = (0..9).map(|r| (0..7).map(|_| hue_bgr(t + r as f32 * 30.0)).collect()).collect();
            http(sport, "PUT", &format!("{path}/keyboard"), &serde_json::json!({ "effect": "CHROMA_CUSTOM", "param": kb }).to_string())?;
            http(sport, "PUT", &format!("{path}/mouse"), &serde_json::json!({ "effect": "CHROMA_CUSTOM2", "param": ms }).to_string())?;
            frames += 1;
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    })();
    let closed = http(sport, "DELETE", &path, "");
    result?;
    closed?;
    Ok(format!("Test pattern sent ({frames} frames)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tmp(tag: &str) -> Tmp {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("tsuba-gl-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn fake_prefix(at: &Path, win64: bool) -> PathBuf {
        std::fs::create_dir_all(at.join("drive_c/windows/system32")).unwrap();
        if win64 {
            std::fs::create_dir_all(at.join("drive_c/windows/syswow64")).unwrap();
        }
        at.to_path_buf()
    }
    fn fake_dlls(at: &Path, tag: &str) -> PathBuf {
        let d = at.join("dlls");
        std::fs::create_dir_all(d.join("x64")).unwrap();
        std::fs::create_dir_all(d.join("x86")).unwrap();
        std::fs::write(d.join("x64").join(DLL64), format!("MZ tsuba 64 {tag}")).unwrap();
        std::fs::write(d.join("x86").join(DLL32), format!("MZ tsuba 32 {tag}")).unwrap();
        d
    }

    #[test]
    fn install_uninstall_64bit_prefix() {
        let t = tmp("i64");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        assert_eq!(state(&pfx, Some(&dlls)), State::NotInstalled);
        let r = install(&pfx, &dlls).unwrap();
        assert_eq!(r.changed.len(), 2);
        assert_eq!(std::fs::read_to_string(pfx.join("drive_c/windows/system32").join(DLL64)).unwrap(), "MZ tsuba 64 v1");
        assert_eq!(std::fs::read_to_string(pfx.join("drive_c/windows/syswow64").join(DLL32)).unwrap(), "MZ tsuba 32 v1");
        assert!(pfx.join(MARKER).is_file());
        assert_eq!(state(&pfx, Some(&dlls)), State::Installed);
        // Installing again changes nothing.
        assert!(install(&pfx, &dlls).unwrap().changed.is_empty());
        // A newer bridge build: outdated → update in place.
        let dlls2 = fake_dlls(&t.0, "v2");
        assert_eq!(state(&pfx, Some(&dlls2)), State::Outdated);
        assert_eq!(install(&pfx, &dlls2).unwrap().changed.len(), 2);
        assert_eq!(std::fs::read_to_string(pfx.join("drive_c/windows/system32").join(DLL64)).unwrap(), "MZ tsuba 64 v2");
        // Uninstall restores the prefix exactly (no stray files, no marker).
        let r = uninstall(&pfx).unwrap();
        assert_eq!(r.changed.len(), 2);
        assert!(!pfx.join("drive_c/windows/system32").join(DLL64).exists());
        assert!(!pfx.join("drive_c/windows/syswow64").join(DLL32).exists());
        assert!(!pfx.join(MARKER).exists());
        assert_eq!(std::fs::read_dir(pfx.join("drive_c/windows/system32")).unwrap().count(), 0);
        assert_eq!(state(&pfx, Some(&dlls2)), State::NotInstalled);
    }

    #[test]
    fn install_32bit_prefix_uses_system32() {
        let t = tmp("i32");
        let pfx = fake_prefix(&t.0.join("pfx"), false);
        let dlls = fake_dlls(&t.0, "v1");
        install(&pfx, &dlls).unwrap();
        assert_eq!(std::fs::read_to_string(pfx.join("drive_c/windows/system32").join(DLL32)).unwrap(), "MZ tsuba 32 v1");
        assert!(!pfx.join("drive_c/windows/system32").join(DLL64).exists());
        assert_eq!(state(&pfx, None), State::Installed);
    }

    #[test]
    fn never_overwrites_or_removes_a_foreign_dll() {
        let t = tmp("foreign");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        let vendor = pfx.join("drive_c/windows/system32").join(DLL64);
        std::fs::write(&vendor, "MZ vendor library").unwrap();
        assert_eq!(state(&pfx, Some(&dlls)), State::Foreign);
        let r = install(&pfx, &dlls).unwrap();
        assert_eq!(std::fs::read_to_string(&vendor).unwrap(), "MZ vendor library");
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.changed, vec![format!("drive_c/windows/syswow64/{DLL32}")]);
        // Uninstall removes only ours.
        uninstall(&pfx).unwrap();
        assert_eq!(std::fs::read_to_string(&vendor).unwrap(), "MZ vendor library");
        assert!(!pfx.join("drive_c/windows/syswow64").join(DLL32).exists());
        // install_all skips prefixes with a foreign library entirely.
        let list = vec![Prefix::new("custom", "x", "x", pfx.clone(), "")];
        assert!(install_all(&list, &dlls).is_empty());
        assert!(!pfx.join("drive_c/windows/syswow64").join(DLL32).exists());
    }

    #[test]
    fn a_replaced_file_becomes_foreign_and_survives_uninstall() {
        let t = tmp("replaced");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        install(&pfx, &dlls).unwrap();
        let p = pfx.join("drive_c/windows/system32").join(DLL64);
        std::fs::write(&p, "MZ vendor library").unwrap(); // the vendor's installer replaced ours
        assert_eq!(state(&pfx, Some(&dlls)), State::Foreign);
        let r = install(&pfx, &dlls).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "MZ vendor library");
        assert!(r.changed.is_empty());
        let r = uninstall(&pfx).unwrap();
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "MZ vendor library");
        assert!(!pfx.join(MARKER).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_never_written_through() {
        let t = tmp("link");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        let target = t.0.join("elsewhere.dll");
        std::fs::write(&target, "MZ shared").unwrap();
        std::os::unix::fs::symlink(&target, pfx.join("drive_c/windows/system32").join(DLL64)).unwrap();
        install(&pfx, &dlls).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "MZ shared");
        assert!(std::fs::symlink_metadata(pfx.join("drive_c/windows/system32").join(DLL64)).unwrap().file_type().is_symlink());
    }

    #[test]
    fn missing_file_is_partial_and_reinstall_fixes_it() {
        let t = tmp("partial");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        install(&pfx, &dlls).unwrap();
        std::fs::remove_file(pfx.join("drive_c/windows/syswow64").join(DLL32)).unwrap();
        assert_eq!(state(&pfx, Some(&dlls)), State::Partial);
        install(&pfx, &dlls).unwrap();
        assert_eq!(state(&pfx, Some(&dlls)), State::Installed);
    }

    #[test]
    fn not_a_prefix() {
        let t = tmp("np");
        let dlls = fake_dlls(&t.0, "v1");
        assert!(install(&t.0.join("nothing"), &dlls).is_err());
        assert_eq!(state(&t.0.join("nothing"), None), State::Invalid);
        let compat = t.0.join("compat");
        fake_prefix(&compat.join("pfx"), true);
        assert_eq!(resolve_prefix(&compat), Some(compat.join("pfx")));
    }

    #[test]
    fn scans_steam_lutris_heroic_bottles() {
        let t = tmp("scan");
        let home = &t.0;
        // Steam: main library + a second library with a game and its prefix.
        let steam = home.join(".local/share/Steam");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        let lib2 = home.join("games/SteamLibrary");
        std::fs::create_dir_all(lib2.join("steamapps")).unwrap();
        std::fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}", steam.display(), lib2.display()),
        )
        .unwrap();
        std::fs::write(lib2.join("steamapps/appmanifest_1245620.acf"), "\"AppState\" { \"appid\" \"1245620\" \"name\" \"Elden Ring\" \"installdir\" \"ELDEN RING\" }").unwrap();
        std::fs::write(steam.join("steamapps/appmanifest_2805730.acf"), "\"AppState\" { \"appid\" \"2805730\" \"name\" \"Proton 9.0\" \"installdir\" \"Proton 9.0\" }").unwrap();
        fake_prefix(&lib2.join("steamapps/compatdata/1245620/pfx"), true);
        fake_prefix(&steam.join("steamapps/compatdata/2805730/pfx"), true); // a tool: skipped
        fake_prefix(&steam.join("steamapps/compatdata/3000000000/pfx"), true); // non-Steam shortcut
        std::fs::create_dir_all(steam.join("steamapps/compatdata/12345")).unwrap(); // never launched: no pfx
        // Lutris
        let lp = fake_prefix(&home.join("Games/witcher"), true);
        std::fs::create_dir_all(home.join(".config/lutris/games")).unwrap();
        std::fs::write(home.join(".config/lutris/games/the-witcher-3-1700000000.yml"), "name: The Witcher 3\ngame:\n  exe: /x/w.exe\n  prefix: ~/Games/witcher\n").unwrap();
        // Heroic
        let hp = fake_prefix(&home.join("Games/Heroic/Prefixes/Fortnite"), true);
        std::fs::create_dir_all(home.join(".config/heroic/GamesConfig")).unwrap();
        std::fs::write(home.join(".config/heroic/GamesConfig/Fn.json"), format!("{{\"Fn\": {{\"winePrefix\": \"{}\"}}}}", hp.display())).unwrap();
        // Bottles
        fake_prefix(&home.join(".local/share/bottles/bottles/Gaming"), true);

        let list = scan_prefixes(home);
        let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"steam:1245620"), "{ids:?}");
        assert!(ids.contains(&"steam:3000000000"), "{ids:?}");
        assert!(!ids.contains(&"steam:2805730"), "{ids:?}");
        assert!(!ids.contains(&"steam:12345"), "{ids:?}");
        assert!(ids.contains(&"bottles:Gaming"), "{ids:?}");
        assert!(ids.contains(&"heroic:Fn"), "{ids:?}");
        let er = list.iter().find(|p| p.id == "steam:1245620").unwrap();
        assert_eq!(er.name, "Elden Ring");
        assert_eq!(er.path, lib2.join("steamapps/compatdata/1245620/pfx"));
        let lu = list.iter().find(|p| p.source == "lutris").unwrap();
        assert_eq!(lu.path, lp);
        assert_eq!(lu.name, "The Witcher 3");
    }

    #[test]
    fn auto_install_only_new_prefixes() {
        let t = tmp("auto");
        let dlls = fake_dlls(&t.0, "v1");
        let a = fake_prefix(&t.0.join("a"), true);
        let b = fake_prefix(&t.0.join("b"), true);
        let mk = |p: &Path, id: &str| Prefix::new("custom", id, id, p.to_path_buf(), "");
        let mut s = Settings { auto_install: true, known: vec!["custom:a".into()], ..Default::default() };
        let done = auto_install_new(&[mk(&a, "a"), mk(&b, "b")], &dlls, &mut s);
        assert_eq!(done, vec!["custom:b".to_string()]);
        assert_eq!(state(&a, None), State::NotInstalled);
        assert_eq!(state(&b, None), State::Installed);
        assert!(s.known.contains(&"custom:b".to_string()));
        // Off: new prefixes are only remembered.
        let c = fake_prefix(&t.0.join("c"), true);
        s.auto_install = false;
        assert!(auto_install_new(&[mk(&c, "c")], &dlls, &mut s).is_empty());
        assert_eq!(state(&c, None), State::NotInstalled);
        // Settings round-trip.
        let sp = t.0.join("cfg/game-lighting.json");
        save_settings_to(&sp, &s).unwrap();
        assert_eq!(load_settings_from(&sp).known, s.known);
    }

    #[test]
    fn test_pattern_talks_rest() {
        // A one-shot fake server: answers init, frames and the final DELETE.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut c = c;
                let mut buf = vec![0u8; 65536];
                let mut got = Vec::new();
                loop {
                    let n = c.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&buf[..n]);
                    let t = String::from_utf8_lossy(&got).to_string();
                    if let Some((h, b)) = t.split_once("\r\n\r\n") {
                        let len: usize = h.lines().find_map(|l| l.strip_prefix("Content-Length: ")).and_then(|x| x.trim().parse().ok()).unwrap_or(0);
                        if b.len() >= len {
                            break;
                        }
                    }
                }
                let t = String::from_utf8_lossy(&got).to_string();
                let line = t.lines().next().unwrap_or("").to_string();
                let body = if line.starts_with("POST /razer/chromasdk") { format!("{{\"sessionid\":1,\"uri\":\"http://localhost:{port}/chromasdk/0123abcd\"}}") } else { "{\"result\":0}".into() };
                s2.lock().unwrap().push(line);
                let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        let msg = test_pattern(port, 0.12).unwrap();
        assert!(msg.contains("frames"));
        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("POST /razer/chromasdk "));
        // The session uri (with its token) is followed verbatim.
        assert!(seen.iter().any(|l| l.starts_with("PUT /chromasdk/0123abcd/keyboard ")));
        assert!(seen.iter().any(|l| l.starts_with("PUT /chromasdk/0123abcd/mouse ")));
        assert!(seen.last().unwrap().starts_with("DELETE /chromasdk/0123abcd "));
        assert!(test_pattern(1, 0.1).is_err()); // nothing listening
    }

    // ---- security regressions (from the security review's proofs of concept) ----

    /// Marker keys used to be trusted as paths: "../victim" or an absolute path was deleted by
    /// uninstall when its hash matched.
    #[test]
    fn marker_entries_outside_the_slots_are_ignored() {
        let t = tmp("marker-traversal");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let victim = t.0.join("victim.txt");
        std::fs::write(&victim, b"known contents").unwrap();
        let abs = t.0.join("victim2.txt");
        std::fs::write(&abs, b"other known contents").unwrap();
        let inside = pfx.join("drive_c/users.txt");
        std::fs::write(&inside, b"inside").unwrap();
        let marker = serde_json::json!({"files": {
            "../victim.txt": fnv64(b"known contents"),
            "drive_c/windows/system32/../../../../victim.txt": fnv64(b"known contents"),
            abs.to_str().unwrap(): fnv64(b"other known contents"),
            "drive_c/users.txt": fnv64(b"inside"),
        }});
        std::fs::write(pfx.join(MARKER), marker.to_string()).unwrap();
        let rep = uninstall(&pfx).unwrap();
        assert!(rep.changed.is_empty(), "{:?}", rep.changed);
        assert!(victim.exists() && abs.exists() && inside.exists());
        assert!(!pfx.join(MARKER).exists());
    }

    /// The temporary file used to be created with File::create at a fixed name, following a
    /// symlink planted there: the DLL bytes went into the link's target.
    #[test]
    fn a_planted_temp_symlink_is_not_followed() {
        let t = tmp("tmp-symlink");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        let victim = t.0.join("victim.conf");
        std::fs::write(&victim, b"precious user data").unwrap();
        for d in ["system32", "syswow64"] {
            let n = if d == "system32" { DLL64 } else { DLL32 };
            std::os::unix::fs::symlink(&victim, pfx.join(format!("drive_c/windows/{d}/.{n}.tsuba-tmp"))).unwrap();
        }
        std::os::unix::fs::symlink(&victim, pfx.join("drive_c/windows/.tsuba-game-lighting.json.tsuba-tmp")).unwrap();
        install(&pfx, &dlls).unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious user data");
        assert_eq!(state(&pfx, Some(&dlls)), State::Installed);
        // No temporary files are left behind.
        let left: Vec<_> = std::fs::read_dir(pfx.join("drive_c/windows/system32")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains("tsuba-tmp")).collect();
        assert!(left.is_empty(), "{left:?}");
    }

    /// A prefix whose system32 (or syswow64) is a link out of the prefix is refused.
    #[test]
    fn windows_folders_linked_outside_are_refused() {
        let t = tmp("linked-sys");
        let pfx = fake_prefix(&t.0.join("pfx"), true);
        let dlls = fake_dlls(&t.0, "v1");
        let outside = t.0.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::remove_dir_all(pfx.join("drive_c/windows/syswow64")).unwrap();
        std::os::unix::fs::symlink(&outside, pfx.join("drive_c/windows/syswow64")).unwrap();
        assert!(install(&pfx, &dlls).is_err());
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
        assert!(uninstall(&pfx).is_err());
        // A link that stays inside the prefix is fine.
        std::fs::remove_file(pfx.join("drive_c/windows/syswow64")).unwrap();
        std::fs::create_dir_all(pfx.join("drive_c/wow")).unwrap();
        std::os::unix::fs::symlink("../wow", pfx.join("drive_c/windows/syswow64")).unwrap();
        install(&pfx, &dlls).unwrap();
        assert!(pfx.join("drive_c/wow").join(DLL32).is_file());
    }

    #[test]
    fn test_pattern_reply_is_bounded() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Some(Ok(mut c)) = l.incoming().next() {
                let mut buf = [0u8; 4096];
                let _ = c.read(&mut buf);
                let _ = c.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
                let chunk = vec![b'x'; 1 << 16];
                for _ in 0..64 {
                    if c.write_all(&chunk).is_err() {
                        break;
                    }
                }
            }
        });
        // 4 MiB offered; at most 1 MiB is read, and it's not a valid reply.
        assert!(test_pattern(port, 0.0).is_err());
    }
}
