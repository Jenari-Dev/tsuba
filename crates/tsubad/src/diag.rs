//! Environment check and diagnostics export.
//!
//! `tsubad --check-env` prints a JSON report the app uses for its "no device / permissions" screen;
//! `tsubad --diagnostics FILE.zip` writes a support bundle. Both work without a running daemon
//! (that is exactly when they are needed) and read only: they list sysfs entries and run
//! `systemctl`/`journalctl`; nothing is sent to a device.
//!
//! The bundle never contains serial numbers, MAC addresses, the user name, the host name or the home
//! directory path (see [`Redactor`]), and it never contains macro text or key assignments: the
//! configuration is reduced to section names and counts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{json, Value};

pub const RAZER_VID: &str = "1532";

// ---- sysfs scan ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct HidrawNode {
    pub node: String,
    pub pid: String,
    pub supported: bool,
    pub readable: bool,
    pub writable: bool,
}

/// Razer `hidraw` nodes under `sys_class` (normally `/sys/class/hidraw`) and whether this user
/// can open the matching `/dev` node. `dev_root` is `/dev`.
pub fn scan_hidraw(sys_class: &Path, dev_root: &Path) -> Vec<HidrawNode> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(sys_class) else { return out };
    for e in entries.flatten() {
        let dev = e.path().join("device");
        let Some(uevent) = fs::read_to_string(dev.join("uevent")).ok() else { continue };
        let Some(id) = uevent.lines().find_map(|l| l.strip_prefix("HID_ID=")) else { continue };
        let mut parts = id.split(':');
        let (_bus, vid, pid) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let (vid, pid) = (vid.trim_start_matches('0').to_lowercase(), pid.trim_start_matches('0').to_lowercase());
        if vid != RAZER_VID {
            continue;
        }
        let pid4 = format!("{pid:0>4}");
        let node = e.file_name().to_string_lossy().to_string();
        let path = dev_root.join(&node);
        let readable = fs::OpenOptions::new().read(true).open(&path).is_ok();
        let writable = fs::OpenOptions::new().write(true).open(&path).is_ok();
        let supported = u16::from_str_radix(&pid4, 16).ok().and_then(tsuba_hid::model).is_some();
        out.push(HidrawNode { node, pid: pid4, supported, readable, writable });
    }
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

/// USB products with vendor 1532 under `usb_root` (normally `/sys/bus/usb/devices`), with a count
/// per product id: two identical devices show up as count 2.
pub fn scan_usb(usb_root: &Path) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    let Ok(entries) = fs::read_dir(usb_root) else { return vec![] };
    for e in entries.flatten() {
        let p = e.path();
        let (Ok(v), Ok(pid)) = (fs::read_to_string(p.join("idVendor")), fs::read_to_string(p.join("idProduct"))) else { continue };
        if v.trim().eq_ignore_ascii_case(RAZER_VID) {
            *counts.entry(pid.trim().to_lowercase()).or_default() += 1;
        }
    }
    counts.into_iter().collect()
}

// ---- diagnosis ----------------------------------------------------------------------------

/// What is wrong, in the order the app should check it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnosis {
    /// A supported device is reachable.
    Ok,
    /// Razer hardware is present but this user cannot open its `hidraw` node (missing udev rule).
    Permission,
    /// Razer USB hardware is present, but none of it is a model Tsuba supports.
    UnsupportedOnly,
    /// Razer USB hardware is present but the kernel made no `hidraw` node for it (another driver owns it).
    NoHidraw,
    /// No Razer device on the USB bus at all.
    NoDevice,
}

impl Diagnosis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Permission => "permission",
            Self::UnsupportedOnly => "unsupported_only",
            Self::NoHidraw => "no_hidraw",
            Self::NoDevice => "no_device",
        }
    }
}

pub fn diagnose(hidraw: &[HidrawNode], usb: &[(String, usize)]) -> Diagnosis {
    let supported: Vec<_> = hidraw.iter().filter(|n| n.supported).collect();
    if supported.iter().any(|n| n.readable) {
        return Diagnosis::Ok;
    }
    if !supported.is_empty() {
        return Diagnosis::Permission;
    }
    if !hidraw.is_empty() {
        return Diagnosis::UnsupportedOnly;
    }
    if usb.is_empty() {
        Diagnosis::NoDevice
    } else {
        Diagnosis::NoHidraw
    }
}

/// Programs known to drive Razer lighting or hold the devices' drivers.
const CONFLICTS: &[(&str, &str, &str)] = &[
    // (process name prefix, id, display name)
    ("openrazer-dae", "openrazer", "OpenRazer daemon"),
    ("openrgb", "openrgb", "OpenRGB"),
    ("polychromatic", "polychromatic", "Polychromatic"),
    ("razergenie", "razergenie", "RazerGenie"),
    ("razercfg", "razercfg", "razercfg"),
];

const CONFLICT_MODULES: &[&str] = &["razerkbd", "razermouse", "razeraccessory", "razercore", "razerkraken"];

/// Running conflicting programs, from `/proc/*/comm` under `proc_root`, plus loaded OpenRazer
/// kernel modules from `modules` (the text of `/proc/modules`).
pub fn find_conflicts(proc_root: &Path, modules: &str) -> Vec<Value> {
    let mut found: Vec<Value> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Ok(entries) = fs::read_dir(proc_root) {
        for e in entries.flatten() {
            if !e.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let Ok(comm) = fs::read_to_string(e.path().join("comm")) else { continue };
            let comm = comm.trim().to_lowercase();
            if let Some((_, id, name)) = CONFLICTS.iter().find(|(prefix, _, _)| comm.starts_with(prefix)) {
                if seen.insert(*id) {
                    found.push(json!({ "id": id, "name": name, "kind": "program" }));
                }
            }
        }
    }
    let loaded: Vec<&str> = modules.lines().filter_map(|l| l.split_whitespace().next()).filter(|m| CONFLICT_MODULES.contains(m)).collect();
    if !loaded.is_empty() {
        found.push(json!({ "id": "openrazer-driver", "name": "OpenRazer kernel driver", "kind": "driver", "detail": loaded.join(", ") }));
    }
    found
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() && s.trim().is_empty() {
        s = String::from_utf8_lossy(&out.stderr).to_string();
    }
    Some(s)
}

fn systemctl_user(args: &[&str]) -> Option<String> {
    let mut a = vec!["--user"];
    a.extend_from_slice(args);
    run("systemctl", &a).map(|s| s.trim().to_string())
}

const UDEV_RULE_PATHS: &[&str] =
    &["/usr/lib/udev/rules.d/70-tsuba.rules", "/etc/udev/rules.d/70-tsuba.rules", "/lib/udev/rules.d/70-tsuba.rules", "/etc/udev/rules.d/70-razer-hidraw.rules"];

pub fn udev_rule_path() -> Option<&'static str> {
    UDEV_RULE_PATHS.iter().copied().find(|p| Path::new(p).exists())
}

/// The report behind the app's "no device / permissions" screen.
pub fn env_report() -> Value {
    let hidraw = scan_hidraw(Path::new("/sys/class/hidraw"), Path::new("/dev"));
    let usb = scan_usb(Path::new("/sys/bus/usb/devices"));
    let modules = fs::read_to_string("/proc/modules").unwrap_or_default();
    let conflicts = find_conflicts(Path::new("/proc"), &modules);
    let duplicates: Vec<&String> = usb.iter().filter(|(_, n)| *n > 1).map(|(p, _)| p).collect();
    let load = systemctl_user(&["show", "-p", "LoadState", "--value", "tsubad"]).unwrap_or_default();
    let rule = udev_rule_path();
    json!({
        "diagnosis": diagnose(&hidraw, &usb).as_str(),
        "service": {
            "unit_installed": load == "loaded",
            "active": systemctl_user(&["is-active", "tsubad"]).unwrap_or_else(|| "unknown".into()),
            "enabled": systemctl_user(&["is-enabled", "tsubad"]).unwrap_or_else(|| "unknown".into()),
        },
        "udev": { "rule_installed": rule.is_some(), "rule_path": rule },
        "hidraw": hidraw.iter().map(|n| json!({ "node": n.node, "pid": n.pid, "supported": n.supported, "readable": n.readable, "writable": n.writable })).collect::<Vec<_>>(),
        "usb": usb.iter().map(|(p, n)| json!({ "pid": p, "count": n, "name": u16::from_str_radix(p, 16).ok().and_then(tsuba_hid::model).map(|m| m.name) })).collect::<Vec<_>>(),
        "duplicates": duplicates,
        "conflicts": conflicts,
        "session": {
            "type": std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
            "desktop": std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        },
    })
}

// ---- redaction ----------------------------------------------------------------------------

/// Removes personal details from text going into a bundle.
pub struct Redactor {
    home: Option<String>,
    user: Option<String>,
    host: Option<String>,
}

impl Redactor {
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok().filter(|h| h.len() > 1);
        let user = std::env::var("USER").ok().or_else(|| home.as_ref().and_then(|h| h.rsplit('/').next().map(str::to_string))).filter(|u| !u.is_empty() && u != "root");
        let host = fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|h| h.trim().to_string()).filter(|h| h.len() >= 3);
        Self { home, user, host }
    }

    #[cfg(test)]
    fn with(home: &str, user: &str, host: &str) -> Self {
        Self { home: Some(home.into()), user: Some(user.into()), host: Some(host.into()) }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut s = text.to_string();
        if let Some(h) = &self.home {
            s = replace_whole(&s, h, "~");
        }
        if let Some(h) = &self.host {
            s = s.replace(h.as_str(), "<host>");
        }
        if let Some(u) = &self.user {
            if u.len() >= 3 {
                s = s.replace(u.as_str(), "<user>");
            } else {
                // A short name would hit unrelated words: only where it is a path component.
                s = replace_whole(&s, &format!("/{u}"), "/<user>");
            }
        }
        s.lines().map(redact_line).collect::<Vec<_>>().join("\n") + if text.ends_with('\n') { "\n" } else { "" }
    }
}

/// Replace `needle` by `repl` where it ends a path component (followed by `/`, the end of the
/// text or a character that cannot be part of a name).
fn replace_whole(text: &str, needle: &str, repl: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(needle) {
        let after = &rest[i + needle.len()..];
        let whole = after.chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.'));
        out.push_str(&rest[..i]);
        out.push_str(if whole { repl } else { needle });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Phrases of log lines that carry names of programs, profiles, macros or commands. The daemon
/// logs those at debug level only; this is the second lock on the door for lines from older
/// versions or paths nobody thought of.
const PRIVATE_LOG_MARKERS: &[&str] = &["launch:", "open:", "macro command", "renamed macro", "linked game", "linked lighting", "game lighting", "library profile", "pinned", "profile “", "“", "action:", "an action failed"];

/// A journal line without its message when the message may name something private.
pub fn redact_log_line(line: &str) -> String {
    let lower = line.to_lowercase();
    if !PRIVATE_LOG_MARKERS.iter().any(|m| lower.contains(m)) {
        return line.to_string();
    }
    const NOTE: &str = "<message withheld: may name a program, profile or command>";
    match line.find("]: ") {
        Some(i) => format!("{}{NOTE}", &line[..i + 3]),
        None => NOTE.to_string(),
    }
}

/// Serial numbers (any line that names one) and MAC addresses.
fn redact_line(line: &str) -> String {
    let lower = line.to_lowercase();
    let mut out = if lower.contains("serial") {
        match line.find(['=', ':']) {
            Some(i) => format!("{} <redacted>", &line[..=i]),
            None => line.to_string(),
        }
    } else {
        line.to_string()
    };
    out = redact_macs(&out);
    out
}

fn redact_macs(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if i + 17 <= b.len() && is_mac(&b[i..i + 17]) && (i == 0 || !b[i - 1].is_ascii_hexdigit()) && (i + 17 == b.len() || !b[i + 17].is_ascii_hexdigit()) {
            out.push_str("<mac>");
            i += 17;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_mac(w: &[u8]) -> bool {
    w.iter().enumerate().all(|(i, c)| if i % 3 == 2 { *c == b':' } else { c.is_ascii_hexdigit() })
}

// ---- zip ----------------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    !data.iter().fold(!0u32, |c, b| table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// Unix seconds to (year, month, day, hour, minute, second) in UTC.
pub fn civil(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (if m <= 2 { y + 1 } else { y }) as u32;
    (y, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// A zip archive with stored (uncompressed) entries; the bundle is a few kilobytes of text.
pub fn zip(entries: &[(String, Vec<u8>)], secs: u64) -> Vec<u8> {
    let (y, mo, d, h, mi, s) = civil(secs);
    let dos_time = ((h << 11) | (mi << 5) | (s / 2)) as u16;
    let dos_date = (((y.max(1980) - 1980) << 9) | (mo << 5) | d) as u16;
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let n = name.as_bytes();
        let size = data.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 8, 0, 0]); // version 2.0, flags: UTF-8 names, method: stored
        out.extend_from_slice(&dos_time.to_le_bytes());
        out.extend_from_slice(&dos_date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(n);
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&[20, 0, 20, 0, 0, 8, 0, 0]);
        central.extend_from_slice(&dos_time.to_le_bytes());
        central.extend_from_slice(&dos_date.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attrs (all zero)
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(n);
    }
    let cd_offset = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ---- bundle -------------------------------------------------------------------------------

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn timestamp(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// Section and entry counts of the configuration, without any content.
pub fn config_summary(cfg: &Value) -> String {
    let mut out = String::new();
    fn walk(v: &Value, name: &str, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        match v {
            Value::Object(m) if depth < 3 => {
                out.push_str(&format!("{pad}{name}: {} keys\n", m.len()));
                for (k, x) in m {
                    walk(x, k, depth + 1, out);
                }
            }
            Value::Object(m) => out.push_str(&format!("{pad}{name}: {} keys\n", m.len())),
            Value::Array(a) => out.push_str(&format!("{pad}{name}: {} entries\n", a.len())),
            Value::Bool(b) => out.push_str(&format!("{pad}{name}: {b}\n")),
            Value::Number(n) => out.push_str(&format!("{pad}{name}: {n}\n")),
            Value::String(_) => out.push_str(&format!("{pad}{name}: (text)\n")),
            Value::Null => {}
        }
    }
    walk(cfg, "config", 0, &mut out);
    out
}

fn os_release() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "unknown".into())
}

/// Collect the bundle's files (already redacted).
pub fn collect(secs: u64) -> Vec<(String, Vec<u8>)> {
    let red = Redactor::new();
    let env = env_report();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, text: String| files.push((name.to_string(), red.apply(&text).into_bytes()));
    let log_lines = |text: String| text.lines().map(redact_log_line).collect::<Vec<_>>().join("\n") + "\n";

    let mut summary = String::new();
    summary.push_str("Tsuba diagnostics\n=================\n");
    summary.push_str(&format!("created: {}\n", timestamp(secs)));
    summary.push_str(&format!("tsubad: {}\n", env!("CARGO_PKG_VERSION")));
    summary.push_str(&format!("system: {}\n", os_release()));
    summary.push_str(&format!("kernel: {}\n", fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim()));
    summary.push_str(&format!("session: {} / {}\n", env["session"]["type"].as_str().unwrap_or(""), env["session"]["desktop"].as_str().unwrap_or("")));
    summary.push_str(&format!("diagnosis: {}\n", env["diagnosis"].as_str().unwrap_or("")));
    summary.push_str(&format!("service: {}\n", env["service"]));
    summary.push_str(&format!("udev rule: {}\n", env["udev"]));
    summary.push_str("\nWhat is in this bundle: this summary, the Razer devices the kernel sees (model ids only), the udev rule,\n");
    summary.push_str("the last lines of tsubad's log, and the shape of Tsuba's configuration (section names and counts only).\n");
    summary.push_str("It contains no key assignments, macro text, file names, user name, host name or home directory path.\n");
    add("summary.txt", summary);

    let mut dev = String::new();
    dev.push_str("USB devices (vendor 1532):\n");
    for u in env["usb"].as_array().into_iter().flatten() {
        dev.push_str(&format!("  {}  x{}  {}\n", u["pid"].as_str().unwrap_or(""), u["count"], u["name"].as_str().unwrap_or("(not a supported model)")));
    }
    dev.push_str("\nhidraw nodes:\n");
    for n in env["hidraw"].as_array().into_iter().flatten() {
        dev.push_str(&format!(
            "  {}  pid {}  supported={}  readable={}  writable={}\n",
            n["node"].as_str().unwrap_or(""),
            n["pid"].as_str().unwrap_or(""),
            n["supported"],
            n["readable"],
            n["writable"]
        ));
    }
    dev.push_str("\nconflicting programs / drivers:\n");
    for c in env["conflicts"].as_array().into_iter().flatten() {
        dev.push_str(&format!("  {} ({})\n", c["name"].as_str().unwrap_or(""), c["kind"].as_str().unwrap_or("")));
    }
    add("devices.txt", dev);

    if let Some(p) = udev_rule_path() {
        add("udev-rule.txt", format!("# {p}\n{}", fs::read_to_string(p).unwrap_or_default()));
    }
    let cfg = tsuba_core::Config::load(&tsuba_core::Config::default_path()).ok().and_then(|c| serde_json::to_value(c).ok());
    add("config-shape.txt", cfg.map(|c| config_summary(&c)).unwrap_or_else(|| "configuration could not be read\n".into()));

    let unit = systemctl_user(&["status", "tsubad", "--no-pager", "-n", "0"]).unwrap_or_default();
    add("service-status.txt", log_lines(unit));
    let log = run("journalctl", &["--user", "-u", "tsubad", "-n", "300", "--no-pager", "-o", "short-iso"]).unwrap_or_default();
    add("tsubad-log.txt", log_lines(log));
    // Kernel messages about Razer hardware, when the user may read them.
    let k = run("journalctl", &["-k", "--no-pager", "-n", "2000", "-o", "short-iso"]).unwrap_or_default();
    let k: String = k.lines().filter(|l| { let l = l.to_lowercase(); l.contains("razer") || l.contains("1532") || l.contains("hidraw") }).map(|l| format!("{l}\n")).collect();
    add("kernel-razer.txt", if k.is_empty() { "(nothing, or the kernel log is not readable by this user)\n".into() } else { k });
    files
}

/// Write the bundle to `path`.
pub fn export(path: &Path) -> Result<PathBuf> {
    let secs = now_secs();
    let bytes = zip(&collect(secs), secs);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("tsuba-diag-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn fake_hidraw(root: &Path, node: &str, vid: &str, pid: &str) {
        let d = root.join(node).join("device");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("uevent"), format!("DRIVER=hid-generic\nHID_ID=0003:0000{vid}:0000{pid}\n")).unwrap();
    }

    #[test]
    fn crc32_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_000_000_000), (2001, 9, 9, 1, 46, 40));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn zip_layout_is_well_formed() {
        let z = zip(&[("a.txt".into(), b"hello".to_vec()), ("b/c.txt".into(), vec![])], 1_000_000_000);
        assert_eq!(&z[..4], &[0x50, 0x4b, 3, 4]);
        let eocd = z.len() - 22;
        assert_eq!(&z[eocd..eocd + 4], &[0x50, 0x4b, 5, 6]);
        assert_eq!(u16::from_le_bytes([z[eocd + 10], z[eocd + 11]]), 2);
        let cd_off = u32::from_le_bytes(z[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        assert_eq!(&z[cd_off..cd_off + 4], &[0x50, 0x4b, 1, 2]);
        assert!(z.windows(5).any(|w| w == b"hello"));
    }

    #[test]
    fn zip_can_be_read_back_by_python_when_available() {
        let Ok(py) = Command::new("python3").arg("--version").output() else { return };
        if !py.status.success() {
            return;
        }
        let dir = tmp("zip");
        let f = dir.join("t.zip");
        fs::write(&f, zip(&[("hello.txt".into(), b"hello world".to_vec()), ("sub/x.txt".into(), b"x".to_vec())], now_secs())).unwrap();
        let out = Command::new("python3")
            .args(["-I", "-c", "import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; print(z.read('hello.txt').decode(), len(z.namelist()))", f.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello world 2");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn redaction_removes_personal_details() {
        let r = Redactor::with("/home/alice", "alice", "mybox");
        let t = "path /home/alice/.config/tsuba on mybox by alice\nSerial Number: XX1234567890\nbt 11:22:33:AA:BB:CC ok\nnormal line\n";
        let o = r.apply(t);
        assert!(!o.contains("alice") && !o.contains("mybox"), "{o}");
        assert!(o.contains("~/.config/tsuba"));
        assert!(!o.contains("XX1234567890"));
        assert!(o.contains("<mac>") && !o.contains("AA:BB:CC"));
        assert!(o.contains("normal line") && o.ends_with('\n'));
    }

    #[test]
    fn short_user_names_are_removed_from_paths_only() {
        let r = Redactor::with("/home/jo", "jo", "mybox");
        let o = r.apply("cfg /home/jo/x and /srv/jo/y; job done, /home/joe/z, path /tmp/jo\n");
        assert!(!o.contains("/jo/") && !o.contains("/home/jo/") && !o.contains("/srv/jo"), "{o}");
        assert!(o.contains("job done") && o.contains("/home/joe/z"), "unrelated words stay: {o}");
        assert!(o.contains("/srv/<user>/y") && o.contains("/tmp/<user>"), "{o}");
        // A user name from the environment with no home match still goes.
        let r = Redactor { home: None, user: Some("al".into()), host: None };
        assert_eq!(r.apply("/home/al/.cache/x"), "/home/<user>/.cache/x");
    }

    #[test]
    fn log_lines_naming_programs_profiles_or_commands_are_withheld() {
        let ok = "2026-10-18T10:00:00+0000 box tsubad[12]: INFO tsubad::manager: attached Basilisk (00aa)";
        assert_eq!(redact_log_line(ok), ok);
        for secret in [
            "launch: steam -applaunch 1245620 --token=SECRET",
            "open: https://example.org/SECRET",
            "macro command \"SECRET\": spawn failed",
            "linked game running: SECRET.exe",
            "game lighting: session 3 is for \"SECRET\"",
            "00aa: \u{201c}SECRET\u{201d} pinned to slot 2",
            "library profile \"SECRET\" is the one active",
        ] {
            let line = format!("2026-10-18T10:00:00+0000 box tsubad[12]: DEBUG tsubad: {secret}");
            let out = redact_log_line(&line);
            assert!(!out.contains("SECRET"), "{out}");
            assert!(out.starts_with("2026-10-18T10:00:00+0000 box tsubad[12]: "), "{out}");
        }
        assert!(!redact_log_line("launch: SECRET").contains("SECRET"), "lines without a journal prefix too");
    }

    #[test]
    fn config_shape_has_counts_not_content() {
        let v = json!({ "game_lighting": true, "device": [{"pid": "028d", "secret": "hunter2"}], "note": "hunter2" });
        let s = config_summary(&v);
        assert!(s.contains("device: 1 entries") && s.contains("game_lighting: true") && s.contains("note: (text)"));
        assert!(!s.contains("hunter2"));
    }

    #[test]
    fn hidraw_scan_finds_razer_nodes_only() {
        let sys = tmp("hid");
        let dev = tmp("dev");
        fake_hidraw(&sys, "hidraw1", "1532", "028D");
        fake_hidraw(&sys, "hidraw2", "046d", "c52b");
        fake_hidraw(&sys, "hidraw3", "1532", "ffff");
        fs::write(dev.join("hidraw1"), "").unwrap();
        let nodes = scan_hidraw(&sys, &dev);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].node, "hidraw1");
        assert_eq!(nodes[0].pid, "028d");
        assert!(nodes[0].readable && nodes[0].writable);
        assert!(!nodes[1].supported && !nodes[1].readable, "no /dev node = not openable");
        let _ = fs::remove_dir_all(sys);
        let _ = fs::remove_dir_all(dev);
    }

    #[test]
    fn usb_scan_counts_identical_devices() {
        let root = tmp("usb");
        for (n, pid) in [("1-1", "00ab"), ("1-2", "00AB"), ("1-3", "028d")] {
            let d = root.join(n);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("idVendor"), "1532\n").unwrap();
            fs::write(d.join("idProduct"), format!("{pid}\n")).unwrap();
        }
        let other = root.join("2-1");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("idVendor"), "046d\n").unwrap();
        fs::write(other.join("idProduct"), "c52b\n").unwrap();
        assert_eq!(scan_usb(&root), vec![("00ab".to_string(), 2), ("028d".to_string(), 1)]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diagnosis_order() {
        let n = |supported, readable| HidrawNode { node: "hidraw0".into(), pid: "028d".into(), supported, readable, writable: readable };
        let usb = vec![("028d".to_string(), 1)];
        assert_eq!(diagnose(&[n(true, true)], &usb), Diagnosis::Ok);
        assert_eq!(diagnose(&[n(true, false)], &usb), Diagnosis::Permission);
        assert_eq!(diagnose(&[n(false, false)], &usb), Diagnosis::UnsupportedOnly);
        assert_eq!(diagnose(&[], &usb), Diagnosis::NoHidraw);
        assert_eq!(diagnose(&[], &[]), Diagnosis::NoDevice);
    }

    #[test]
    fn conflicts_from_processes_and_modules() {
        let proc = tmp("proc");
        for (pid, comm) in [("123", "openrazer-daem"), ("124", "OpenRGB"), ("125", "tsubad"), ("self", "x")] {
            let d = proc.join(pid);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("comm"), format!("{comm}\n")).unwrap();
        }
        let c = find_conflicts(&proc, "razerkbd 16384 0 - Live 0x0\nsnd 1 0 - Live 0x0\n");
        let ids: Vec<&str> = c.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"openrazer") && ids.contains(&"openrgb") && ids.contains(&"openrazer-driver"));
        assert_eq!(ids.len(), 3, "tsubad itself is not a conflict");
        let _ = fs::remove_dir_all(proc);
    }
}
