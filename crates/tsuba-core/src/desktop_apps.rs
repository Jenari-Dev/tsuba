//! Programs to offer in the Launch picker: ready-made presets (calculator, terminal, file
//! manager…) resolved to whatever is installed, and the desktop's applications read from the
//! `.desktop` files in `$XDG_DATA_DIRS/applications`.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesktopApp {
    pub name: String,
    /// Command line with the `%f`/`%u`… field codes removed, ready for `sh -c`.
    pub command: String,
    pub icon: String,
    pub terminal: bool,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// The first candidate that is installed (empty when none is).
    pub command: String,
}

/// (id, label, candidates in order of preference). Commands may carry arguments.
const PRESETS: [(&str, &str, &[&str]); 8] = [
    ("calculator", "Calculator", &["kcalc", "gnome-calculator", "qalculate-qt", "qalculate-gtk", "galculator", "mate-calc", "xfce4-calculator"]),
    ("editor", "Text editor", &["kate", "gnome-text-editor", "gedit", "mousepad", "kwrite", "pluma", "xed"]),
    ("terminal", "Terminal", &["konsole", "gnome-terminal", "kgx", "xfce4-terminal", "alacritty", "kitty", "ghostty", "foot", "xterm"]),
    ("files", "File manager", &["dolphin", "nautilus", "thunar", "nemo", "pcmanfm-qt", "pcmanfm", "caja"]),
    ("screenshot", "Screenshot tool", &["spectacle", "gnome-screenshot", "flameshot gui", "xfce4-screenshooter"]),
    ("monitor", "System monitor", &["plasma-systemmonitor", "gnome-system-monitor", "ksysguard", "xfce4-taskmanager", "mate-system-monitor"]),
    ("charmap", "Emoji and symbols", &["plasma-emojier", "gnome-characters", "kcharselect", "gucharmap", "mate-character-map"]),
    ("home", "Home folder", &["xdg-open ~"]),
];

fn on_path(program: &str, path: &str) -> bool {
    path.split(':').filter(|d| !d.is_empty()).any(|d| {
        let p = Path::new(d).join(program);
        std::fs::metadata(&p).map(|m| m.is_file() && std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0).unwrap_or(false)
    })
}

/// The presets, each resolved against the executables found in `path` (a `PATH` string).
pub fn presets_in(path: &str) -> Vec<Preset> {
    PRESETS
        .iter()
        .map(|(id, label, cands)| Preset {
            id,
            label,
            command: cands.iter().find(|c| on_path(c.split_whitespace().next().unwrap_or(""), path)).map(|c| (*c).to_string()).unwrap_or_default(),
        })
        .collect()
}

pub fn presets() -> Vec<Preset> {
    presets_in(&std::env::var("PATH").unwrap_or_default())
}

/// Remove `%f %F %u %U %i %c %k` style field codes (and turn `%%` into `%`).
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::new();
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('%') => out.push('%'),
            Some(_) | None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse one `.desktop` file; None for entries that aren't launchable applications.
pub fn parse_desktop(text: &str, lang: &str) -> Option<DesktopApp> {
    let (mut name, mut localized, mut exec, mut icon) = (String::new(), String::new(), String::new(), String::new());
    let (mut terminal, mut hidden, mut is_app, mut categories) = (false, false, false, Vec::new());
    let mut in_entry = false;
    let key = format!("Name[{lang}]=");
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(v) = line.strip_prefix("Name=") {
                name = v.to_string();
            } else if !lang.is_empty() && line.starts_with(&key) {
                localized = line[key.len()..].to_string();
            } else if let Some(v) = line.strip_prefix("Exec=") {
                exec = v.to_string();
            } else if let Some(v) = line.strip_prefix("Icon=") {
                icon = v.to_string();
            } else if let Some(v) = line.strip_prefix("Terminal=") {
                terminal = v == "true";
            } else if line == "NoDisplay=true" || line == "Hidden=true" {
                hidden = true;
            } else if let Some(v) = line.strip_prefix("Type=") {
                is_app = v == "Application";
            } else if let Some(v) = line.strip_prefix("Categories=") {
                categories = v.split(';').filter(|c| !c.is_empty()).map(str::to_string).collect();
            }
        }
    }
    let command = strip_field_codes(&exec);
    if !is_app || hidden || command.is_empty() {
        return None;
    }
    let name = if localized.is_empty() { name } else { localized };
    (!name.is_empty()).then_some(DesktopApp { name, command, icon, terminal, categories })
}

fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("XDG_DATA_HOME").map(PathBuf::from).ok().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")));
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter().chain(sys.split(':').filter(|d| !d.is_empty()).map(PathBuf::from)).map(|d| d.join("applications")).collect()
}

/// Installed applications from `dirs` (earlier directories win on equal file names), by name.
pub fn scan(dirs: &[PathBuf], lang: &str) -> Vec<DesktopApp> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if !file.ends_with(".desktop") || !seen.insert(file) {
                continue;
            }
            if let Some(app) = std::fs::read_to_string(e.path()).ok().and_then(|t| parse_desktop(&t, lang)) {
                out.push(app);
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.name == b.name && a.command == b.command);
    out
}

pub fn installed() -> Vec<DesktopApp> {
    let lang = std::env::var("LANG").ok().and_then(|l| l.split(['_', '.']).next().map(str::to_string)).unwrap_or_default();
    scan(&data_dirs(), &lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_files_become_commands() {
        let t = "[Desktop Entry]\nType=Application\nName=Calculator\nName[ja]=電卓\nExec=kcalc %U\nIcon=accessories-calculator\nCategories=Qt;KDE;Utility;\n[Desktop Action x]\nName=Other\nExec=nope\n";
        let a = parse_desktop(t, "").unwrap();
        assert_eq!((a.name.as_str(), a.command.as_str(), a.icon.as_str()), ("Calculator", "kcalc", "accessories-calculator"));
        assert_eq!(a.categories, ["Qt", "KDE", "Utility"]);
        assert_eq!(parse_desktop(t, "ja").unwrap().name, "電卓");
        assert_eq!(parse_desktop("[Desktop Entry]\nType=Application\nName=X\nExec=sh -c 'echo 100%% done' %f\n", "").unwrap().command, "sh -c 'echo 100% done'");
        assert!(parse_desktop("[Desktop Entry]\nType=Application\nName=H\nExec=h\nNoDisplay=true\n", "").is_none());
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=L\nURL=x\n", "").is_none());
    }

    #[test]
    fn presets_pick_the_first_installed_candidate() {
        let dir = std::env::temp_dir().join(format!("tsuba-presets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for exe in ["gnome-calculator", "konsole"] {
            let p = dir.join(exe);
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        }
        std::fs::write(dir.join("kate"), "not executable").unwrap();
        let p = presets_in(dir.to_str().unwrap());
        let get = |id: &str| p.iter().find(|x| x.id == id).unwrap().command.clone();
        assert_eq!(get("calculator"), "gnome-calculator");
        assert_eq!(get("terminal"), "konsole");
        assert_eq!(get("editor"), "", "a file without the exec bit doesn't count");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_reads_and_dedups_application_dirs() {
        let root = std::env::temp_dir().join(format!("tsuba-apps-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.desktop"), "[Desktop Entry]\nType=Application\nName=Zed\nExec=zed\n").unwrap();
        std::fs::write(b.join("x.desktop"), "[Desktop Entry]\nType=Application\nName=Shadowed\nExec=other\n").unwrap();
        std::fs::write(b.join("y.desktop"), "[Desktop Entry]\nType=Application\nName=alpha\nExec=alpha\n").unwrap();
        let apps = scan(&[a, b], "");
        assert_eq!(apps.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["alpha", "Zed"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
