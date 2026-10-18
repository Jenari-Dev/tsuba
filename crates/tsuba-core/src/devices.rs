//! Device picture packs: `device.json` (views, hotspots, LED boxes) plus the images it names.
//!
//! A personal pack in `~/.local/share/tsuba/devices/<pid>` wins when present (the user can build
//! one from software they own; it is never distributed). Otherwise Tsuba uses its own line-art
//! pack, installed under `<data dir>/tsuba/devices/<pid>` (e.g. /usr/share) or, in a development
//! build, read from the repository's `data/devices`.

use std::path::{Path, PathBuf};

/// Show Tsuba's own drawings even when a personal pack exists: `TSUBA_ART=builtin`, or the app's
/// Appearance setting (`art=builtin` in the `[appearance]` section of ~/.config/tsuba/app.ini).
fn prefer_builtin() -> bool {
    if let Ok(v) = std::env::var("TSUBA_ART") {
        return v == "builtin";
    }
    let Some(ini) = dirs::config_dir().map(|d| d.join("tsuba/app.ini")) else { return false };
    let Ok(text) = std::fs::read_to_string(ini) else { return false };
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
        } else if section == "[appearance]" && line.replace(' ', "") == "art=builtin" {
            return true;
        }
    }
    false
}

/// Whether the user has a personal picture pack for `pid`.
pub fn has_personal(pid: &str) -> bool {
    personal(pid).is_some()
}

fn personal(pid: &str) -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("tsuba/devices").join(pid);
    dir.join("device.json").is_file().then_some(dir)
}

/// The repository's `data/devices` for builds run from a source checkout; packaged builds
/// (`--cfg tsuba_packaged`) don't carry the build path at all.
#[cfg(any(not(tsuba_packaged), test))]
fn dev_data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/devices")))
}
#[cfg(all(tsuba_packaged, not(test)))]
fn dev_data_dir() -> Option<PathBuf> {
    None
}

/// Tsuba's own line-art pack for `pid`, if it ships one.
pub fn builtin_dir(pid: &str) -> Option<PathBuf> {
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    system
        .split(':')
        .map(|d| PathBuf::from(d).join("tsuba/devices").join(pid))
        .chain(dev_data_dir().map(|d| d.join(pid)))
        .find(|d| d.join("device.json").is_file())
}

/// The directory holding the pack Tsuba should show for `pid`.
pub fn pack_dir(pid: &str) -> Option<PathBuf> {
    if prefer_builtin() {
        return builtin_dir(pid);
    }
    personal(pid).or_else(|| builtin_dir(pid))
}

// ---- the user's own device pictures -------------------------------------------------------
//
// A custom picture lives in the personal pack (`art: "custom"`): a copy of Tsuba's drawing pack
// (hotspots, LED boxes and anchors stay those of the drawing) whose `views.<view>.image` is the
// user's file and whose `views.<view>.imageRect` [x, y, w, h] says where to draw it, in the
// drawing's coordinate space. A photo pack built by the image-pack tool is moved to
// `<dir>.photo-backup` first and restored when the custom picture is removed (an older backup
// already there is kept as `<dir>.photo-backup.<n>`).

const PICTURE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "svg"];
const MAX_PICTURE_BYTES: u64 = 40 * 1024 * 1024;

fn valid_pid(pid: &str) -> bool {
    !pid.is_empty() && pid.len() <= 8 && pid.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_view(view: &str) -> bool {
    view == "top" || view == "bottom"
}

/// Where the personal pack for `pid` lives (whether or not it exists).
pub fn personal_path(pid: &str) -> Option<PathBuf> {
    valid_pid(pid).then(|| dirs::data_dir().map(|d| d.join("tsuba/devices").join(pid))).flatten()
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_json(path: &Path, v: &serde_json::Value) -> Result<(), String> {
    std::fs::write(path, serde_json::to_string_pretty(v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Whether the personal pack for `pid` is one made from a user's own picture.
pub fn has_custom_picture(pid: &str) -> bool {
    personal_path(pid).is_some_and(|d| is_custom(&d))
}

fn is_custom(dir: &Path) -> bool {
    read_json(&dir.join("device.json")).is_some_and(|v| v["art"] == "custom")
}

fn backup_path(dir: &Path) -> PathBuf {
    let mut s = dir.as_os_str().to_owned();
    s.push(".photo-backup");
    PathBuf::from(s)
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        if e.path().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Use `src` (png/jpg/webp/svg) as the picture for `view` ("top" or "bottom") of `pid`.
pub fn set_custom_picture(pid: &str, view: &str, src: &Path) -> Result<(), String> {
    let dir = personal_path(pid).ok_or("bad device id")?;
    let builtin = builtin_dir(pid).ok_or("no Tsuba drawing for this device")?;
    set_custom_picture_in(&dir, &builtin, view, src)
}

pub fn set_custom_picture_in(dir: &Path, builtin: &Path, view: &str, src: &Path) -> Result<(), String> {
    if !valid_view(view) {
        return Err("bad view".into());
    }
    let ext = src.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !PICTURE_EXTS.contains(&ext.as_str()) {
        return Err("pick a png, jpg, webp or svg file".into());
    }
    let meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_PICTURE_BYTES {
        return Err("not a usable picture file".into());
    }
    let mut pack = read_json(&builtin.join("device.json")).ok_or("unreadable drawing pack")?;
    if pack["views"][view].is_null() {
        return Err("this device has no such view".into());
    }
    if dir.join("device.json").is_file() && !is_custom(dir) {
        // A photo pack: keep it so Remove can restore it. Never delete one: an older backup
        // is rotated to `<dir>.photo-backup.<n>` (first free number) first.
        let backup = backup_path(dir);
        if std::fs::symlink_metadata(&backup).is_ok() {
            let mut n = 1;
            let older = loop {
                let mut s = backup.as_os_str().to_owned();
                s.push(format!(".{n}"));
                let p = PathBuf::from(s);
                if std::fs::symlink_metadata(&p).is_err() {
                    break p;
                }
                n += 1;
                if n > 1000 {
                    return Err("too many photo pack backups; remove some first".into());
                }
            };
            std::fs::rename(&backup, &older).map_err(|e| e.to_string())?;
        }
        std::fs::rename(dir, &backup).map_err(|e| e.to_string())?;
    }
    if is_custom(dir) {
        pack = read_json(&dir.join("device.json")).ok_or("unreadable personal pack")?;
    } else {
        copy_dir(builtin, dir)?;
    }
    // Drop the previous custom file for this view, then copy the new one in.
    let stem = format!("custom-{view}");
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&format!("{stem}.")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let name = format!("{stem}.{ext}");
    std::fs::copy(src, dir.join(&name)).map_err(|e| e.to_string())?;
    let (w, h) = (pack["views"][view]["width"].as_f64().unwrap_or(1.0), pack["views"][view]["height"].as_f64().unwrap_or(1.0));
    pack["art"] = "custom".into();
    pack["views"][view]["image"] = name.into();
    // Until the user aligns it, the picture covers the whole drawing.
    pack["views"][view]["imageRect"] = serde_json::json!([0.0, 0.0, w, h]);
    write_json(&dir.join("device.json"), &pack)
}

/// Place the custom picture of `view` at `rect` = [x, y, w, h] in the drawing's coordinates.
pub fn set_picture_rect(pid: &str, view: &str, rect: [f64; 4]) -> Result<(), String> {
    let dir = personal_path(pid).ok_or("bad device id")?;
    set_picture_rect_in(&dir, view, rect)
}

pub fn set_picture_rect_in(dir: &Path, view: &str, rect: [f64; 4]) -> Result<(), String> {
    if !valid_view(view) || rect.iter().any(|v| !v.is_finite()) || rect[2] <= 0.0 || rect[3] <= 0.0 {
        return Err("bad rectangle".into());
    }
    if !is_custom(dir) {
        return Err("no custom picture for this device".into());
    }
    let mut pack = read_json(&dir.join("device.json")).ok_or("unreadable personal pack")?;
    if pack["views"][view].is_null() {
        return Err("this device has no such view".into());
    }
    pack["views"][view]["imageRect"] = serde_json::json!(rect);
    write_json(&dir.join("device.json"), &pack)
}

/// Remove the custom pictures of `pid`; a photo pack that was backed up comes back.
pub fn remove_custom_picture(pid: &str) -> Result<(), String> {
    let dir = personal_path(pid).ok_or("bad device id")?;
    remove_custom_picture_in(&dir)
}

pub fn remove_custom_picture_in(dir: &Path) -> Result<(), String> {
    if !is_custom(dir) {
        return Err("no custom picture for this device".into());
    }
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    let backup = backup_path(dir);
    if backup.is_dir() {
        std::fs::rename(&backup, dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_packs_ship_for_supported_devices() {
        for pid in ["028d", "00aa"] {
            let dir = builtin_dir(pid).expect("built-in pack");
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("device.json")).unwrap()).unwrap();
            assert_eq!(v["art"], "tsuba");
            let image = v["views"]["top"]["image"].as_str().unwrap();
            assert!(dir.join(image).is_file(), "{pid}: {image}");
        }
        // Every keyboard LED (keys + 38 underglow segments) has a box on the drawing.
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(builtin_dir("028d").unwrap().join("device.json")).unwrap()).unwrap();
        let leds: Vec<_> = v["views"]["top"]["hotspots"].as_array().unwrap().iter().filter(|h| h["led"].is_array()).map(|h| h["led"].to_string()).collect();
        let unique: std::collections::BTreeSet<_> = leds.iter().collect();
        assert_eq!(leds.len(), 114 + 38);
        assert_eq!(unique.len(), leds.len(), "an LED is drawn twice");
    }

    #[test]
    fn custom_picture_round_trip_with_photo_backup() {
        let root = std::env::temp_dir().join(format!("tsuba-pic-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dir = root.join("devices/00aa");
        // An existing photo pack (no `art: custom`).
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("device.json"), r#"{"views":{"top":{"image":"p.png","width":1,"height":1,"hotspots":[]}}}"#).unwrap();
        std::fs::write(dir.join("p.png"), b"x").unwrap();
        let src = root.join("mine.PNG");
        std::fs::write(&src, b"png").unwrap();
        let builtin = builtin_dir("00aa").unwrap();
        set_custom_picture_in(&dir, &builtin, "top", &src).unwrap();
        assert!(is_custom(&dir));
        assert!(backup_path(&dir).join("p.png").is_file(), "photo pack backed up");
        assert!(dir.join("custom-top.png").is_file());
        let v = read_json(&dir.join("device.json")).unwrap();
        assert_eq!(v["views"]["top"]["image"], "custom-top.png");
        assert!(v["views"]["top"]["hotspots"].as_array().unwrap().len() > 3, "built-in hotspots kept");
        set_picture_rect_in(&dir, "top", [10.0, 20.0, 300.0, 400.0]).unwrap();
        let v = read_json(&dir.join("device.json")).unwrap();
        assert_eq!(v["views"]["top"]["imageRect"], serde_json::json!([10.0, 20.0, 300.0, 400.0]));
        assert!(set_picture_rect_in(&dir, "top", [0.0, 0.0, 0.0, 5.0]).is_err());
        assert!(set_custom_picture_in(&dir, &builtin, "top", &root.join("x.exe")).is_err());
        // Replacing a custom picture keeps the first backup and drops the old file.
        let src2 = root.join("b.jpg");
        std::fs::write(&src2, b"jpg").unwrap();
        set_custom_picture_in(&dir, &builtin, "top", &src2).unwrap();
        assert!(!dir.join("custom-top.png").exists() && dir.join("custom-top.jpg").is_file());
        assert!(backup_path(&dir).join("p.png").is_file());
        remove_custom_picture_in(&dir).unwrap();
        assert!(dir.join("p.png").is_file() && !is_custom(&dir), "photo pack restored");
        assert!(!backup_path(&dir).exists());
        assert!(remove_custom_picture_in(&dir).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A photo pack used to be deleted when a backup already existed (e.g. the user rebuilt the
    /// photo pack after an earlier custom picture). Now the old backup is rotated instead.
    #[test]
    fn existing_backup_never_costs_the_photo_pack() {
        let root = std::env::temp_dir().join(format!("tsuba-pic-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("devices/00aa");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("device.json"), r#"{"views":{"top":{"image":"new.png","width":1,"height":1,"hotspots":[]}}}"#).unwrap();
        std::fs::write(dir.join("new.png"), b"new photo").unwrap();
        let old = backup_path(&dir);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("old.png"), b"old photo").unwrap();
        let src = root.join("mine.png");
        std::fs::write(&src, b"png").unwrap();
        set_custom_picture_in(&dir, &builtin_dir("00aa").unwrap(), "top", &src).unwrap();
        assert_eq!(std::fs::read(old.join("new.png")).unwrap(), b"new photo", "current photo pack backed up");
        let mut rotated = old.as_os_str().to_owned();
        rotated.push(".1");
        assert_eq!(std::fs::read(PathBuf::from(rotated).join("old.png")).unwrap(), b"old photo", "older backup kept");
        remove_custom_picture_in(&dir).unwrap();
        assert!(dir.join("new.png").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
