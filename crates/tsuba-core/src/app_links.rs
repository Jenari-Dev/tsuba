//! Game / app linking (Synapse's linked games): while a matching program runs, devices switch
//! to the linked on-board profiles and/or Chroma profile; when it exits, the previous ones
//! come back. Programs are matched against running processes (name or command line, so
//! Proton/Wine games match by their .exe), checked every couple of seconds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppLink {
    /// Shown in the list (defaults to the match text).
    #[serde(default)]
    pub name: String,
    /// Case-insensitive text matched against process names and command lines
    /// (e.g. "cs2", "eldenring.exe"). `dir:/path/to/game` instead matches any process whose
    /// executable or command line lies inside that folder (Steam / Proton / Wine games).
    pub matches: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Device pid → profile to switch to: a library profile id, or (links saved before the
    /// profile library) an on-board slot number, which tsubad migrates to the library profile
    /// pinned there.
    #[serde(default)]
    pub device_profiles: BTreeMap<String, ProfileRef>,
    /// Chroma Studio profile to apply.
    #[serde(default)]
    pub chroma: Option<String>,
    /// Cover art (`file://` URL) shown on the list row; filled in when added via "Scan for games".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub art: String,
}

fn yes() -> bool {
    true
}

/// Which profile a link switches a device to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProfileRef {
    /// A profile in the device's library (its id).
    Library(String),
    /// An on-board slot (older links).
    Slot(u8),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppLinks {
    #[serde(default)]
    pub links: Vec<AppLink>,
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("app-links.json")
}

impl AppLinks {
    pub fn load(p: &Path) -> Result<Self> {
        match std::fs::read_to_string(p) {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("parsing {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", p.display())),
        }
    }

    pub fn save(&self, p: &Path) -> Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        // A unique temporary name per write (two writers never share one), fsynced before the
        // rename, so a crash leaves the old list or the new one.
        crate::fsutil::write_atomic(p, serde_json::to_string_pretty(self)?.as_bytes())
    }

    /// The first enabled link whose text appears in any running process (list order = priority).
    pub fn active<'a>(&'a self, procs: &[Proc]) -> Option<&'a AppLink> {
        self.links.iter().filter(|l| l.enabled && !l.matches.trim().is_empty()).find(|l| procs.iter().any(|p| matches_proc(&l.matches, p)))
    }
}

/// Lower-case with forward slashes, so Wine's `Z:\\home\\…` and native paths compare alike.
fn norm_path(s: &str) -> String {
    s.to_lowercase().replace('\\', "/")
}

/// Whether the match text `m` selects process `p`. Plain text matches the process name or
/// command line (case-insensitive); `dir:<folder>` matches a process running from inside the
/// folder (its exe, or a path in its command line).
pub fn matches_proc(m: &str, p: &Proc) -> bool {
    let m = m.trim();
    if m.len() >= 4 && m[..4].eq_ignore_ascii_case("dir:") {
        let d = norm_path(m[4..].trim());
        let d = d.trim_end_matches('/');
        if d.is_empty() {
            return false;
        }
        let inside = format!("{d}/");
        let (cmd, exe) = (norm_path(&p.cmdline), norm_path(&p.exe));
        return exe.starts_with(&inside) || cmd.contains(&inside) || cmd == d || cmd.ends_with(&format!(" {d}"));
    }
    let m = m.to_lowercase();
    p.name.to_lowercase().contains(&m) || p.cmdline.to_lowercase().contains(&m)
}

/// A running process of this user.
#[derive(Debug, Clone, Serialize)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    /// Target of /proc/<pid>/exe ("" if unreadable).
    pub exe: String,
}

/// This user's processes (from /proc), skipping kernel threads.
pub fn processes() -> Vec<Proc> {
    use std::os::unix::fs::MetadataExt;
    let Ok(uid) = std::fs::metadata("/proc/self").map(|m| m.uid()) else { return Vec::new() };
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return out };
    for e in dir.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let base = e.path();
        let Ok(status) = std::fs::read_to_string(base.join("status")) else { continue };
        let owner = status.lines().find_map(|l| l.strip_prefix("Uid:")).and_then(|v| v.split_whitespace().next()).and_then(|v| v.parse::<u32>().ok());
        if owner != Some(uid) {
            continue;
        }
        let cmd = std::fs::read(base.join("cmdline")).unwrap_or_default();
        if cmd.is_empty() {
            continue; // kernel thread / zombie
        }
        let cmdline = String::from_utf8_lossy(&cmd).replace('\0', " ").trim().to_string();
        let name = std::fs::read_to_string(base.join("comm")).unwrap_or_default().trim().to_string();
        let exe = std::fs::read_link(base.join("exe")).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Proc { pid, name, cmdline, exe });
    }
    out
}

/// Program names worth offering in the picker: distinct process names plus Windows .exe names
/// found in command lines (Proton/Wine games), without obvious system/desktop services.
pub fn program_names(procs: &[Proc]) -> Vec<String> {
    const SKIP: [&str; 24] = [
        "systemd", "dbus", "pipewire", "wireplumber", "kworker", "bash", "zsh", "fish", "sh", "plasmashell", "kwin_wayland",
        "kded6", "ksmserver", "xdg-", "at-spi", "gvfs", "kactivitymanagerd", "baloo", "tsubad", "tsuba-app", "sleep", "polkit",
        "Xwayland", "kglobalacceld",
    ];
    let mut names: Vec<String> = Vec::new();
    for p in procs {
        for word in p.cmdline.split(['/', '\\', ' ']) {
            if word.to_lowercase().ends_with(".exe") && word.len() > 4 && !names.contains(&word.to_string()) {
                names.push(word.to_string());
            }
        }
        if !p.name.is_empty() && !SKIP.iter().any(|s| p.name.starts_with(s)) && !names.contains(&p.name) {
            names.push(p.name.clone());
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, cmd: &str) -> Proc {
        Proc { pid: 1, name: name.into(), cmdline: cmd.into(), exe: String::new() }
    }

    #[test]
    fn profile_refs_read_old_and_new_links() {
        let l: AppLinks = serde_json::from_str(r#"{"links": [{"matches": "x", "device_profiles": {"00aa": 4, "028d": "0123456789abcdef0123456789abcdef"}}]}"#).unwrap();
        assert_eq!(l.links[0].device_profiles["00aa"], ProfileRef::Slot(4));
        assert_eq!(l.links[0].device_profiles["028d"], ProfileRef::Library("0123456789abcdef0123456789abcdef".into()));
        let s = serde_json::to_string(&l).unwrap();
        assert!(s.contains(r#""00aa":4"#) && s.contains(r#""028d":"0123"#), "{s}");
    }

    #[test]
    fn matches_name_or_proton_exe() {
        let links = AppLinks {
            links: vec![
                AppLink { name: "Off".into(), matches: "doom".into(), enabled: false, ..Default::default() },
                AppLink { name: "Elden".into(), matches: "eldenring.exe".into(), enabled: true, ..Default::default() },
                AppLink { name: "CS".into(), matches: "cs2".into(), enabled: true, ..Default::default() },
            ],
        };
        let procs = vec![proc("wine64-preloader", "Z:\\games\\ELDEN RING\\Game\\eldenring.exe -steam"), proc("doom", "doom")];
        assert_eq!(links.active(&procs).map(|l| l.name.as_str()), Some("Elden"));
        assert!(links.active(&[proc("bash", "bash")]).is_none());
        assert!(program_names(&procs).contains(&"eldenring.exe".to_string()));
    }

    #[test]
    fn dir_prefix_matches_by_folder() {
        let dir = "/run/media/u/Games/SteamLibrary/steamapps/common/ELDEN RING";
        let mk = |m: &str| AppLinks { links: vec![AppLink { name: "E".into(), matches: m.into(), enabled: true, ..Default::default() }] };
        let wine = proc("eldenring.exe", "Z:\\run\\media\\u\\Games\\SteamLibrary\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe");
        assert!(mk(&format!("dir:{dir}")).active(&[wine]).is_some());
        let proton = proc("reaper", &format!("reaper SteamLaunch AppId=1245620 -- proton waitforexitandrun {dir}/Game/eldenring.exe"));
        assert!(mk(&format!("DIR:{dir}/")).active(&[proton]).is_some());
        let mut native = proc("game", "./game");
        native.exe = format!("{dir}/game");
        assert!(mk(&format!("dir:{dir}")).active(&[native]).is_some());
        // A sibling folder with the same prefix must not match.
        let other = proc("x", &format!("{dir} 2/game.exe"));
        assert!(mk(&format!("dir:{dir}")).active(&[other]).is_none());
        assert!(mk("dir:").active(&[proc("a", "a")]).is_none());
        // Plain text matching is unchanged.
        assert!(mk("eldenring").active(&[proc("x", "foo eldenring.exe")]).is_some());
    }

    #[test]
    fn concurrent_saves_never_clash() {
        // Two writers once shared one `app-links.json.tmp`: one's rename could fail or publish
        // the other's half-written file.
        let dir = std::env::temp_dir().join(format!("tsuba-app-links-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-links.json");
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for i in 0..25 {
                        let l = AppLinks { links: (0..t + 1).map(|n| AppLink { name: format!("{t}-{i}-{n}"), matches: "x".repeat(200), enabled: true, ..Default::default() }).collect() };
                        l.save(&path).unwrap();
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        let l = AppLinks::load(&path).unwrap();
        assert!(!l.links.is_empty());
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["app-links.json"], "temporary files left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
