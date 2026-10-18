//! Installed-game detection for the Linked games page: Steam (all library folders), plus
//! best-effort Lutris, Heroic and Bottles. Everything is read-only and a missing or broken
//! launcher is simply skipped; a scan never fails as a whole.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One installed game.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Game {
    /// "steam" | "lutris" | "heroic" | "bottles"
    pub source: String,
    pub id: String,
    pub name: String,
    /// Install folder ("" if unknown); used for `dir:` matching.
    pub install_dir: String,
    /// `file://` URL of cover art ("" if none cached).
    pub art: String,
    /// Executable file name when the install folder is unknown (matched by name instead).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub exe: String,
}

fn game(source: &str, id: &str, name: &str, install_dir: &str, art: &str) -> Game {
    Game { source: source.into(), id: id.into(), name: name.into(), install_dir: install_dir.into(), art: art.into(), exe: String::new() }
}

/// Scan this user's installed games.
pub fn installed_games() -> Vec<Game> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    scan(&home)
}

/// Scan with an explicit home directory (tests use a synthetic one).
pub fn scan(home: &Path) -> Vec<Game> {
    let mut out = Vec::new();
    out.extend(scan_steam(home));
    out.extend(scan_lutris(home));
    out.extend(scan_heroic(home));
    out.extend(scan_bottles(home));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.source.cmp(&b.source)));
    out.dedup_by(|a, b| a.source == b.source && a.id == b.id);
    out
}

// ---- Valve KeyValues (libraryfolders.vdf, appmanifest_*.acf) ---------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Str(String),
    Obj(Vec<(String, Kv)>),
}

impl Kv {
    /// Child by key, case-insensitive (Steam is inconsistent about key case).
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Obj(v) => v.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Kv::Str(_) => None,
        }
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Kv::Str(s) => Some(s),
            Kv::Obj(_) => None,
        }
    }
    pub fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Obj(v) => v,
            Kv::Str(_) => &[],
        }
    }
}

enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(&c) = it.peek() {
        match c {
            '{' => {
                it.next();
                out.push(Tok::Open);
            }
            '}' => {
                it.next();
                out.push(Tok::Close);
            }
            '"' => {
                it.next();
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => break,
                        },
                        o => s.push(o),
                    }
                }
                out.push(Tok::Str(s));
            }
            '/' => {
                // `// comment` to end of line
                it.next();
                if it.peek() == Some(&'/') {
                    while let Some(&c) = it.peek() {
                        if c == '\n' {
                            break;
                        }
                        it.next();
                    }
                }
            }
            c if c.is_whitespace() => {
                it.next();
            }
            _ => {
                // unquoted token
                let mut s = String::new();
                while let Some(&c) = it.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    it.next();
                }
                out.push(Tok::Str(s));
            }
        }
    }
    out
}

/// Parse KeyValues text into a root object (top-level `"key" { … }` pairs).
pub fn parse_kv(text: &str) -> Kv {
    let toks = tokenize(text);
    let mut pos = 0;
    Kv::Obj(parse_obj(&toks, &mut pos, true))
}

fn parse_obj(toks: &[Tok], pos: &mut usize, top: bool) -> Vec<(String, Kv)> {
    let mut out = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::Close => {
                *pos += 1;
                if !top {
                    return out;
                }
            }
            Tok::Open => {
                *pos += 1; // stray brace: ignore
            }
            Tok::Str(k) => {
                let key = k.clone();
                *pos += 1;
                match toks.get(*pos) {
                    Some(Tok::Str(v)) => {
                        out.push((key, Kv::Str(v.clone())));
                        *pos += 1;
                    }
                    Some(Tok::Open) => {
                        *pos += 1;
                        let children = parse_obj(toks, pos, false);
                        out.push((key, Kv::Obj(children)));
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

// ---- Steam -----------------------------------------------------------------------------------

pub(crate) fn steam_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for rel in [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam"] {
        let p = home.join(rel);
        if !p.join("steamapps").is_dir() {
            continue;
        }
        let canon = std::fs::canonicalize(&p).unwrap_or(p);
        if !roots.contains(&canon) {
            roots.push(canon);
        }
    }
    roots
}

/// Library folders listed in `libraryfolders.vdf` (new format: objects with "path"; old
/// format: `"1" "/path"`).
pub fn library_paths(vdf: &str) -> Vec<PathBuf> {
    let root = parse_kv(vdf);
    let Some(lf) = root.get("libraryfolders") else { return Vec::new() };
    let mut out = Vec::new();
    for (k, v) in lf.entries() {
        if !k.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let p = match v {
            Kv::Str(s) => Some(s.as_str()),
            Kv::Obj(_) => v.str("path"),
        };
        if let Some(p) = p.filter(|p| !p.is_empty()) {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// `(appid, name, installdir)` from an `appmanifest_*.acf`.
pub fn parse_manifest(acf: &str) -> Option<(String, String, String)> {
    let root = parse_kv(acf);
    let st = root.get("AppState")?;
    let appid = st.str("appid")?.to_string();
    let name = st.str("name").unwrap_or("").to_string();
    let dir = st.str("installdir")?.to_string();
    if name.is_empty() || dir.is_empty() {
        return None;
    }
    Some((appid, name, dir))
}

fn is_steam_tool(name: &str) -> bool {
    const SKIP: [&str; 4] = ["proton", "steam linux runtime", "steamworks common redistributables", "steamworks shared"];
    let n = name.to_lowercase();
    SKIP.iter().any(|s| n.contains(s))
}

fn file_url(p: &Path) -> String {
    let mut s = String::from("file://");
    for b in p.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// Cover art for an app in Steam's library cache (new per-app folder layout, a nested hashed
/// folder, or the older flat layout); "" if none.
pub(crate) fn steam_art(root: &Path, appid: &str) -> String {
    let cache = root.join("appcache").join("librarycache");
    let is_cover = |n: &str| n.starts_with("library_600x900") && n.ends_with(".jpg");
    let folder = cache.join(appid);
    let mut header = None;
    let mut dirs = vec![folder.clone()];
    if let Ok(rd) = std::fs::read_dir(&folder) {
        dirs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        if let Some(n) = names.iter().find(|n| is_cover(n)) {
            return file_url(&d.join(n));
        }
        if header.is_none() && names.iter().any(|n| n == "header.jpg") {
            header = Some(d.join("header.jpg"));
        }
    }
    if let Some(h) = header {
        return file_url(&h);
    }
    for flat in [format!("{appid}_library_600x900.jpg"), format!("{appid}_header.jpg")] {
        let p = cache.join(flat);
        if p.is_file() {
            return file_url(&p);
        }
    }
    String::new()
}

fn scan_steam(home: &Path) -> Vec<Game> {
    let mut out = Vec::new();
    for root in steam_roots(home) {
        let mut libs = vec![root.clone()];
        if let Ok(t) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            for l in library_paths(&t) {
                if !libs.contains(&l) {
                    libs.push(l);
                }
            }
        }
        for lib in libs {
            let apps = lib.join("steamapps");
            let Ok(rd) = std::fs::read_dir(&apps) else { continue };
            for e in rd.flatten() {
                let fname = e.file_name().to_string_lossy().into_owned();
                if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
                let Some((appid, name, dir)) = parse_manifest(&text) else { continue };
                if is_steam_tool(&name) {
                    continue;
                }
                let install = apps.join("common").join(&dir);
                out.push(game("steam", &appid, &name, &install.to_string_lossy(), &steam_art(&root, &appid)));
            }
        }
    }
    out
}

// ---- tiny YAML reader (Lutris / Bottles) -----------------------------------------------------

/// `(indent, key, value)` for each `key: value` line; list markers are skipped and comments,
/// blank lines and quotes ignored.
pub(crate) fn yaml_pairs(text: &str) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut indent = line.len() - t.len();
        let mut t = t;
        while let Some(r) = t.strip_prefix("- ") {
            indent += 2;
            t = r.trim_start();
        }
        let Some((k, v)) = t.split_once(':') else { continue };
        if k.is_empty() {
            continue;
        }
        let v = v.trim();
        let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).or_else(|| v.strip_prefix('\'').and_then(|x| x.strip_suffix('\''))).unwrap_or(v);
        out.push((indent, k.trim().trim_matches(|c| c == '"' || c == '\'').to_string(), v.to_string()));
    }
    out
}

fn slug_title(stem: &str) -> String {
    // "elden-ring-1700000000" -> "Elden Ring"
    let parts: Vec<&str> = stem.split(['-', '_']).collect();
    let end = if parts.len() > 1 && parts.last().is_some_and(|l| l.len() >= 8 && l.chars().all(|c| c.is_ascii_digit())) { parts.len() - 1 } else { parts.len() };
    parts[..end]
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parent_dir(p: &str) -> String {
    Path::new(p).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default()
}

fn file_name(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

// ---- Lutris ----------------------------------------------------------------------------------

/// A Lutris game config (`games/<slug>-<id>.yml`): `(name, install dir, exe name)`.
pub fn parse_lutris(stem: &str, yml: &str) -> Option<(String, String, String)> {
    let mut name = String::new();
    let (mut exe, mut workdir, mut section) = (String::new(), String::new(), String::new());
    for (indent, k, v) in yaml_pairs(yml) {
        if indent == 0 {
            section = k.clone();
            if k == "name" {
                name = v;
            }
            continue;
        }
        if section == "game" && indent <= 4 {
            match k.as_str() {
                "exe" => exe = v,
                "working_dir" => workdir = v,
                _ => {}
            }
        }
    }
    let name = if name.is_empty() { slug_title(stem) } else { name };
    let dir = if !workdir.is_empty() { workdir } else if exe.starts_with('/') { parent_dir(&exe) } else { String::new() };
    if name.is_empty() || (dir.is_empty() && exe.is_empty()) {
        return None;
    }
    Some((name, dir, if exe.is_empty() { String::new() } else { file_name(&exe) }))
}

fn scan_lutris(home: &Path) -> Vec<Game> {
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
            if let Some((name, dir, exe)) = parse_lutris(&stem, &text) {
                let mut g = game("lutris", &stem, &name, &dir, "");
                g.exe = if dir.is_empty() { exe } else { String::new() };
                out.push(g);
            }
        }
    }
    out
}

// ---- Heroic ----------------------------------------------------------------------------------

fn read_json(p: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn jstr<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn scan_heroic(home: &Path) -> Vec<Game> {
    let mut out = Vec::new();
    for rel in [".config/heroic", ".var/app/com.heroicgameslauncher.hgl/config/heroic"] {
        let base = home.join(rel);
        if !base.is_dir() {
            continue;
        }
        // Epic (legendary): { "<app>": { title, install_path, … } }
        if let Some(v) = read_json(&base.join("legendaryConfig/legendary/installed.json")) {
            if let Some(o) = v.as_object() {
                for (app, g) in o {
                    let (title, dir) = (jstr(g, "title"), jstr(g, "install_path"));
                    if !title.is_empty() {
                        out.push(game("heroic", app, title, dir, ""));
                    }
                }
            }
        }
        // GOG: installed.json lists paths, gog_library.json has the titles.
        let gog_titles: std::collections::HashMap<String, String> = read_json(&base.join("store_cache/gog_library.json"))
            .and_then(|v| v.get("games").and_then(|g| g.as_array().cloned()))
            .unwrap_or_default()
            .iter()
            .map(|g| (jstr(g, "app_name").to_string(), jstr(g, "title").to_string()))
            .collect();
        if let Some(v) = read_json(&base.join("gog_store/installed.json")) {
            for g in v.get("installed").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let app = jstr(&g, "appName").to_string();
                let title = gog_titles.get(&app).cloned().unwrap_or_default();
                if !title.is_empty() {
                    out.push(game("heroic", &app, &title, jstr(&g, "install_path"), ""));
                }
            }
        }
        // Sideloaded apps.
        if let Some(v) = read_json(&base.join("sideload_apps/library.json")) {
            for g in v.get("games").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let (app, title) = (jstr(&g, "app_name"), jstr(&g, "title"));
                let installed = g.get("is_installed").and_then(|x| x.as_bool()).unwrap_or(true);
                if title.is_empty() || !installed {
                    continue;
                }
                let mut gm = game("heroic", app, title, jstr(&g, "folder_name"), "");
                let exe = g.get("install").map(|i| jstr(i, "executable")).unwrap_or("");
                if gm.install_dir.is_empty() && !exe.is_empty() {
                    gm.install_dir = parent_dir(exe);
                    if gm.install_dir.is_empty() {
                        gm.exe = file_name(exe);
                    }
                }
                out.push(gm);
            }
        }
    }
    out
}

// ---- Bottles ---------------------------------------------------------------------------------

/// `(program name, path)` pairs from a bottle.yml's External_Programs.
pub fn parse_bottle_programs(yml: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let (mut in_sec, mut cur): (bool, Option<(String, String)>) = (false, None);
    for (indent, k, v) in yaml_pairs(yml) {
        if indent == 0 {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            in_sec = k == "External_Programs";
            continue;
        }
        if !in_sec {
            continue;
        }
        if indent <= 2 && v.is_empty() {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            cur = Some((String::new(), String::new()));
        } else if let Some(c) = cur.as_mut() {
            match k.as_str() {
                "name" => c.0 = v,
                "path" => c.1 = v,
                _ => {}
            }
        }
    }
    if let Some(c) = cur.take() {
        out.push(c);
    }
    out.into_iter().filter(|(n, p)| !n.is_empty() && !p.is_empty()).collect()
}

fn scan_bottles(home: &Path) -> Vec<Game> {
    let mut out = Vec::new();
    for rel in [".local/share/bottles/bottles", ".var/app/com.usebottles.bottles/data/bottles/bottles"] {
        let Ok(rd) = std::fs::read_dir(home.join(rel)) else { continue };
        for e in rd.flatten() {
            let bdir = e.path();
            let Ok(text) = std::fs::read_to_string(bdir.join("bottle.yml")) else { continue };
            let bname = bdir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            for (name, path) in parse_bottle_programs(&text) {
                // Windows paths map into the bottle's drive_c; Linux paths are used as they are.
                let linux = if let Some(rest) = path.strip_prefix("C:\\").or_else(|| path.strip_prefix("c:\\")) {
                    bdir.join("drive_c").join(rest.replace('\\', "/")).to_string_lossy().into_owned()
                } else {
                    path.clone()
                };
                let dir = parent_dir(&linux);
                let mut g = game("bottles", &format!("{bname}:{name}"), &name, &dir, "");
                // Wine reports the program as a Windows path, so match it by file name.
                g.exe = file_name(&path);
                if path.starts_with('/') {
                    g.exe = String::new();
                } else {
                    g.install_dir = String::new();
                }
                out.push(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/u/.local/share/Steam"
		"label"		""
		"apps"
		{
			"228980"		"591071872"
		}
	}
	"1"
	{
		"path"		"/run/media/u/My Games/SteamLibrary"   // trailing comment
		"apps" { "550" "1" }
	}
}
"#;

    const ACF: &str = r#"
"AppState"
{
	"appid"		"550"
	"universe"		"1"
	"name"		"Left 4 Dead 2"
	"installdir"		"Left 4 Dead 2"
	"StateFlags"		"4"
	"UserConfig"
	{
		"language"		"english"
	}
}
"#;

    #[test]
    fn parses_library_folders() {
        let l = library_paths(LIBS);
        assert_eq!(l, vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from("/run/media/u/My Games/SteamLibrary")]);
        // old flat format
        let old = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"1\"\n\t\"1\"\t\t\"/mnt/games\"\n}\n";
        assert_eq!(library_paths(old), vec![PathBuf::from("/mnt/games")]);
    }

    #[test]
    fn parses_manifest_and_escapes() {
        assert_eq!(parse_manifest(ACF), Some(("550".into(), "Left 4 Dead 2".into(), "Left 4 Dead 2".into())));
        assert_eq!(parse_manifest("\"AppState\" { \"appid\" \"1\" }"), None);
        let kv = parse_kv(r#""a" { "p" "C:\\Games\\x" "q" "say \"hi\"" }"#);
        assert_eq!(kv.get("a").unwrap().str("p"), Some("C:\\Games\\x"));
        assert_eq!(kv.get("A").unwrap().str("q"), Some("say \"hi\""));
    }

    #[test]
    fn skips_tools() {
        assert!(is_steam_tool("Proton Experimental"));
        assert!(is_steam_tool("Steam Linux Runtime 3.0 (sniper)"));
        assert!(is_steam_tool("Steamworks Common Redistributables"));
        assert!(!is_steam_tool("Left 4 Dead 2"));
    }

    #[test]
    fn lutris_and_bottles_yaml() {
        let yml = "game:\n  exe: /home/u/Games/foo/foo.exe\n  prefix: /home/u/pfx\nrunner: wine\n";
        assert_eq!(parse_lutris("foo-bar-1700000000", yml), Some(("Foo Bar".into(), "/home/u/Games/foo".into(), "foo.exe".into())));
        let b = "Name: Gaming\nExternal_Programs:\n  1234:\n    name: Cool Game\n    path: C:\\Games\\Cool\\cool.exe\n  5678:\n    name: Other\n    path: /opt/other/run.sh\nPath: x\n";
        assert_eq!(
            parse_bottle_programs(b),
            vec![("Cool Game".into(), "C:\\Games\\Cool\\cool.exe".into()), ("Other".into(), "/opt/other/run.sh".into())]
        );
    }

    #[test]
    fn scans_synthetic_steam_tree() {
        let tmp = std::env::temp_dir().join(format!("tsuba-games-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let steam = tmp.join(".local/share/Steam");
        let lib2 = tmp.join("lib2");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        std::fs::create_dir_all(lib2.join("steamapps")).unwrap();
        std::fs::write(steam.join("steamapps/libraryfolders.vdf"), format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{}\" }} }}", steam.display(), lib2.display())).unwrap();
        std::fs::write(lib2.join("steamapps/appmanifest_550.acf"), ACF).unwrap();
        std::fs::write(steam.join("steamapps/appmanifest_1.acf"), "\"AppState\" { \"appid\" \"1\" \"name\" \"Proton 9\" \"installdir\" \"Proton 9\" }").unwrap();
        let art = steam.join("appcache/librarycache/550");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("header.jpg"), b"x").unwrap();
        std::fs::write(art.join("library_600x900.jpg"), b"x").unwrap();
        let g = scan(&tmp);
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!(g[0].name, "Left 4 Dead 2");
        assert_eq!(g[0].install_dir, format!("{}/steamapps/common/Left 4 Dead 2", lib2.display()));
        assert!(g[0].art.starts_with("file://") && g[0].art.ends_with("library_600x900.jpg"), "{}", g[0].art);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `cargo test -p tsuba-core -- --ignored --nocapture real_scan` lists this machine's games.
    #[test]
    #[ignore]
    fn real_scan() {
        let g = installed_games();
        for x in &g {
            println!("{:8} {:10} {} | {} | art={}", x.source, x.id, x.name, x.install_dir, !x.art.is_empty());
        }
        println!("total {}", g.len());
    }
}
