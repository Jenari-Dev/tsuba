//! Game lighting: effects sent by games that support Razer Chroma (through the local REST
//! interface, see `tsubad`'s game-lighting server), and how they map onto Tsuba's devices.
//!
//! This is an independent implementation written for interoperability from Razer's public
//! REST documentation; only the wire names (effect type strings, JSON field names) and the
//! published grid sizes / LED ids are used.
//!
//! Effects arrive as SDK grids per device class ("target"): keyboard 6×22 (extended 8×24),
//! mouse 9×7, mouse mat 15 or 20, headset 5, keypad 4×5, Chroma Link 5. Colours are BGR
//! integers (`0x00BBGGRR`); in keyboard key overlays bit 24 (`0x01000000`) marks a key whose
//! colour is set. A [`DeviceMap`] turns a grid into the device's own custom-frame rows (or
//! per-region colours for devices with single-LED zones).

use std::collections::HashMap;

use serde_json::Value;

use crate::studio::Geometry;

pub type Rgb = [u8; 3];

/// Result codes (the documented `RzErrors` values the REST interface returns).
pub mod result {
    pub const SUCCESS: i64 = 0;
    /// The user blocked this application from controlling the lighting.
    pub const ACCESS_DENIED: i64 = 5;
    pub const NOT_SUPPORTED: i64 = 50;
    pub const INVALID_PARAMETER: i64 = 87;
    pub const DEVICE_NOT_CONNECTED: i64 = 1167;
    pub const NOT_FOUND: i64 = 1168;
    /// `0x80004005`, as the signed 32-bit number clients compare against.
    pub const FAILED: i64 = -2147467259;
}

// ---- Which applications may control the lighting -----------------------------------------

/// Per-application allow / block list for game lighting, kept in
/// `~/.config/tsuba/game-lighting-apps.json`. Applications are told apart by their process
/// name (found from the connection) and fall back to the title they announce.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AppRules {
    /// false (default): every application may, except those blocked. true: only allowed ones.
    #[serde(default)]
    pub only_allowed: bool,
    /// App key → true (allow) / false (block). Missing = follow `only_allowed`.
    #[serde(default)]
    pub rules: std::collections::BTreeMap<String, bool>,
    /// Applications that connected before, so the user can set a rule after the fact.
    #[serde(default)]
    pub seen: std::collections::BTreeMap<String, SeenApp>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SeenApp {
    /// The title the application announced.
    pub name: String,
    /// Process name when it could be found.
    pub exe: String,
    /// Unix seconds of the last connection.
    pub last_seen: u64,
}

/// At most this many remembered applications / rules.
const MAX_APPS: usize = 200;

pub fn rules_path() -> std::path::PathBuf {
    dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("tsuba").join("game-lighting-apps.json")
}

/// The key rules are stored under: the lower-cased process name, else `title:<title>`.
pub fn app_key(exe: &str, title: &str) -> String {
    let clean = |s: &str| s.chars().filter(|c| !c.is_control()).take(64).collect::<String>().trim().to_lowercase();
    match clean(exe) {
        e if !e.is_empty() => e,
        _ => format!("title:{}", clean(title)),
    }
}

impl AppRules {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// May the application with this key control the lighting?
    pub fn allowed(&self, key: &str) -> bool {
        match self.rules.get(key) {
            Some(false) => false,
            // A title is whatever the client says it is: in allow-list mode only a process
            // name can earn access, so a program can't pass for an allowed game by naming itself.
            Some(true) => !(self.only_allowed && key.starts_with("title:")),
            None => !self.only_allowed,
        }
    }

    /// Remember that an application connected.
    pub fn record(&mut self, key: &str, name: &str, exe: &str, now: u64) {
        if !self.seen.contains_key(key) && self.seen.len() >= MAX_APPS {
            if let Some(old) = self.seen.iter().min_by_key(|(_, v)| v.last_seen).map(|(k, _)| k.clone()) {
                self.seen.remove(&old);
            }
        }
        self.seen.insert(key.to_string(), SeenApp { name: name.chars().filter(|c| !c.is_control()).take(64).collect(), exe: exe.chars().filter(|c| !c.is_control()).take(64).collect(), last_seen: now });
    }

    /// Set (Some) or clear (None) an application's rule.
    pub fn set_rule(&mut self, key: &str, allow: Option<bool>) -> anyhow::Result<()> {
        anyhow::ensure!(!key.is_empty() && key.len() <= 80 && !key.chars().any(char::is_control), "bad application key");
        match allow {
            Some(a) => {
                anyhow::ensure!(self.rules.contains_key(key) || self.rules.len() < MAX_APPS, "too many rules");
                self.rules.insert(key.to_string(), a);
            }
            None => {
                self.rules.remove(key);
            }
        }
        Ok(())
    }
}

/// The process name behind a loopback TCP connection that *we* accepted, given the client's
/// local port (the peer port of our socket). Linux only; None when it can't be found.
pub fn peer_process(client_port: u16) -> Option<String> {
    let inode = ["/proc/net/tcp", "/proc/net/tcp6"].iter().find_map(|f| tcp_inode(&std::fs::read_to_string(f).ok()?, client_port))?;
    let want = format!("socket:[{inode}]");
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().filter(|n| n.bytes().all(|b| b.is_ascii_digit())) else { continue };
        let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else { continue };
        if fds.flatten().any(|fd| std::fs::read_link(fd.path()).is_ok_and(|l| l.as_os_str() == want.as_str())) {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            return Some(process_name(comm.trim(), &cmdline));
        }
    }
    None
}

/// Socket inode of the loopback connection whose *local* port is `port`, from a
/// `/proc/net/tcp` listing.
fn tcp_inode(listing: &str, port: u16) -> Option<u64> {
    for line in listing.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let local = f.get(1)?;
        let (addr, p) = local.rsplit_once(':')?;
        let loopback = addr == "0100007F" || addr == "00000000000000000000000001000000" || addr == "0000000000000000FFFF00000100007F";
        if loopback && u16::from_str_radix(p, 16).ok() == Some(port) {
            return f.get(9)?.parse().ok().filter(|i: &u64| *i != 0);
        }
    }
    None
}

/// A readable process name: `comm`, or for Wine's loader the Windows program in its command line.
fn process_name(comm: &str, cmdline: &[u8]) -> String {
    let wine_loader = comm.starts_with("wine") || comm.ends_with("-preloader");
    if wine_loader {
        for arg in cmdline.split(|b| *b == 0).skip(1) {
            let a = String::from_utf8_lossy(arg);
            let base = a.rsplit(['\\', '/']).next().unwrap_or("");
            if base.to_ascii_lowercase().ends_with(".exe") {
                return base.to_string();
            }
        }
    }
    comm.to_string()
}

/// Bit marking an active key in a keyboard key overlay.
pub const KEY_FLAG: u32 = 0x0100_0000;

/// Device class an effect is addressed to (the REST endpoint name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    Keyboard,
    Mouse,
    Mousepad,
    Headset,
    Keypad,
    ChromaLink,
}

impl Target {
    pub const ALL: [Target; 6] = [Target::Keyboard, Target::Mouse, Target::Mousepad, Target::Headset, Target::Keypad, Target::ChromaLink];

    pub fn from_path(s: &str) -> Option<Self> {
        Some(match s {
            "keyboard" => Target::Keyboard,
            "mouse" => Target::Mouse,
            "mousepad" | "mousemat" => Target::Mousepad,
            "headset" => Target::Headset,
            "keypad" => Target::Keypad,
            "chromalink" => Target::ChromaLink,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Target::Keyboard => "keyboard",
            Target::Mouse => "mouse",
            Target::Mousepad => "mousepad",
            Target::Headset => "headset",
            Target::Keypad => "keypad",
            Target::ChromaLink => "chromalink",
        }
    }

    /// The grid a static / animated effect fills (the target's standard custom grid).
    pub fn dims(self) -> (u8, u8) {
        match self {
            Target::Keyboard => KEYBOARD,
            Target::Mouse => MOUSE,
            Target::Mousepad => (1, 15),
            Target::Headset => (1, 5),
            Target::Keypad => (4, 5),
            Target::ChromaLink => (1, 5),
        }
    }

    /// Which target drives a device of this kind.
    pub fn for_kind(kind: tsuba_hid::Kind) -> Option<Self> {
        use tsuba_hid::Kind::*;
        Some(match kind {
            Keyboard | LaptopKeyboard => Target::Keyboard,
            Mouse => Target::Mouse,
            Mousemat => Target::Mousepad,
            Headset => Target::Headset,
            Keypad => Target::Keypad,
            Mousedock | HeadsetStand | Speaker | Accessory | Egpu | Monitor => Target::ChromaLink,
            Receiver => return None,
        })
    }
}

/// Keyboard grid (6×22), extended keyboard grid (8×24) and mouse grid (9×7).
pub const KEYBOARD: (u8, u8) = (6, 22);
pub const KEYBOARD_EXT: (u8, u8) = (8, 24);
pub const MOUSE: (u8, u8) = (9, 7);

/// Documented mouse LED ids (`row << 8 | col` in the 9×7 grid).
pub mod mouse_led {
    pub const SCROLLWHEEL: u16 = 0x0203;
    pub const LOGO: u16 = 0x0703;
    pub const BACKLIGHT: u16 = 0x0403;
    pub const fn left_side(n: u16) -> u16 {
        n << 8
    }
    pub const fn right_side(n: u16) -> u16 {
        (n << 8) | 6
    }
    pub const fn bottom(n: u16) -> u16 {
        0x0800 | n
    }
}

/// A few documented keyboard key ids (`row << 8 | col` in the 6×22 grid), used by tests and
/// the macro-column mapping.
pub mod key {
    pub const ESC: u16 = 0x0001;
    pub const F1: u16 = 0x0003;
    pub const F12: u16 = 0x000E;
    pub const LOGO: u16 = 0x0014;
    pub const OEM_1: u16 = 0x0101; // ` ~
    pub const BACKSPACE: u16 = 0x010E;
    pub const NUMPAD_SUBTRACT: u16 = 0x0115;
    pub const TAB: u16 = 0x0201;
    pub const NUMPAD_ADD: u16 = 0x0215;
    pub const CAPSLOCK: u16 = 0x0301;
    pub const A: u16 = 0x0302;
    pub const ENTER: u16 = 0x030E;
    pub const LSHIFT: u16 = 0x0401;
    pub const Z: u16 = 0x0403;
    pub const UP: u16 = 0x0410;
    pub const NUMPAD_ENTER: u16 = 0x0415;
    pub const LCTRL: u16 = 0x0501;
    pub const SPACE: u16 = 0x0507;
    pub const RCTRL: u16 = 0x050E;
    pub const RIGHT: u16 = 0x0511;
    pub const NUMPAD_DECIMAL: u16 = 0x0514;
    pub const fn macro_key(n: u16) -> u16 {
        n << 8
    }
    pub const fn cell(id: u16) -> (u8, u8) {
        ((id >> 8) as u8, id as u8)
    }
}

/// `0x00BBGGRR` → RGB. Negative numbers (clients that build the value with `~`) are taken
/// as their 32-bit pattern; bits above 23 are ignored.
pub fn bgr(v: i64) -> Rgb {
    let v = v as u32;
    [v as u8, (v >> 8) as u8, (v >> 16) as u8]
}

/// RGB → `0x00BBGGRR`.
pub fn to_bgr(c: Rgb) -> u32 {
    c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16
}

fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_u64().map(|x| x as i64)).or_else(|| v.as_f64().map(|x| x as i64))
}

/// A rows×cols grid of colours, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub rows: u8,
    pub cols: u8,
    pub cells: Vec<Rgb>,
}

impl Grid {
    pub fn filled(rows: u8, cols: u8, c: Rgb) -> Self {
        Self { rows, cols, cells: vec![c; rows as usize * cols as usize] }
    }

    pub fn get(&self, r: u8, c: u8) -> Rgb {
        self.cells.get(r as usize * self.cols as usize + c as usize).copied().unwrap_or_default()
    }

    fn dims(&self) -> (u8, u8) {
        (self.rows, self.cols)
    }
}

/// Software-rendered legacy effects (older REST clients).
#[derive(Debug, Clone, PartialEq)]
pub enum Anim {
    /// Fade in and out, alternating the colours (empty = cycle through hues).
    Breathing(Vec<Rgb>),
    Spectrum,
    /// Rainbow moving left→right (`true`) or right→left.
    Wave { right: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Base {
    Grid(Grid),
    Anim { anim: Anim, rows: u8, cols: u8 },
}

/// What a game shows on one target: a base grid plus (keyboards) key overrides in 6×22 cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub base: Base,
    pub keys: Vec<(u8, u8, Rgb)>,
}

/// A parsed effect: `None` hands the device back to the user's lighting.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    Show(Show),
}

impl Show {
    pub fn animated(&self) -> bool {
        matches!(self.base, Base::Anim { .. })
    }

    /// The base grid at time `t` seconds since the effect was applied.
    pub fn grid(&self, t: f32) -> Grid {
        match &self.base {
            Base::Grid(g) => g.clone(),
            Base::Anim { anim, rows, cols } => render_anim(anim, *rows, *cols, t),
        }
    }
}

fn hue(h: f32) -> Rgb {
    crate::studio::hsv(h.rem_euclid(360.0) / 360.0, 1.0, 1.0)
}

fn scale(c: Rgb, k: f32) -> Rgb {
    c.map(|x| (x as f32 * k).round().clamp(0.0, 255.0) as u8)
}

fn render_anim(anim: &Anim, rows: u8, cols: u8, t: f32) -> Grid {
    match anim {
        Anim::Breathing(colours) => {
            const PERIOD: f32 = 4.0;
            let n = (t / PERIOD) as usize;
            let phase = (t % PERIOD) / PERIOD;
            let k = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
            let c = if colours.is_empty() { hue(n as f32 * 67.0) } else { colours[n % colours.len()] };
            Grid::filled(rows, cols, scale(c, k))
        }
        Anim::Spectrum => Grid::filled(rows, cols, hue(t * 60.0)),
        Anim::Wave { right } => {
            let mut g = Grid::filled(rows, cols, [0; 3]);
            for r in 0..rows as usize {
                for c in 0..cols as usize {
                    let x = c as f32 / cols.max(1) as f32 * 360.0;
                    let h = if *right { x - t * 120.0 } else { -x - t * 120.0 };
                    g.cells[r * cols as usize + c] = hue(h);
                }
            }
            g
        }
    }
}

/// Why an effect body was refused (maps to a result code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Invalid,
    NotSupported,
}

impl ParseError {
    pub fn code(self) -> i64 {
        match self {
            ParseError::Invalid => result::INVALID_PARAMETER,
            ParseError::NotSupported => result::NOT_SUPPORTED,
        }
    }
}

/// Read a 2-D (or, for one-row grids, 1-D) colour array, padding/truncating to rows×cols.
/// Also returns each raw value (for key flags).
fn raw_grid(v: &Value, rows: u8, cols: u8) -> Result<Vec<i64>, ParseError> {
    let arr = v.as_array().ok_or(ParseError::Invalid)?;
    let mut out = vec![0i64; rows as usize * cols as usize];
    let two_d = arr.first().is_some_and(|x| x.is_array());
    if two_d {
        for (r, row) in arr.iter().take(rows as usize).enumerate() {
            let row = row.as_array().ok_or(ParseError::Invalid)?;
            for (c, x) in row.iter().take(cols as usize).enumerate() {
                out[r * cols as usize + c] = num(x).ok_or(ParseError::Invalid)?;
            }
        }
    } else {
        for (i, x) in arr.iter().take(out.len()).enumerate() {
            out[i] = num(x).ok_or(ParseError::Invalid)?;
        }
    }
    Ok(out)
}

/// The dimensions of a JSON colour array (rows, longest row) — 1-D arrays are one row.
fn shape(v: &Value) -> Option<(usize, usize)> {
    let arr = v.as_array()?;
    if arr.first().is_some_and(|x| x.is_array()) {
        Some((arr.len(), arr.iter().filter_map(|r| r.as_array().map(|r| r.len())).max().unwrap_or(0)))
    } else {
        Some((1, arr.len()))
    }
}

fn grid_from(v: &Value, rows: u8, cols: u8) -> Result<Grid, ParseError> {
    let raw = raw_grid(v, rows, cols)?;
    Ok(Grid { rows, cols, cells: raw.into_iter().map(bgr).collect() })
}

fn keys_from(v: &Value) -> Result<Vec<(u8, u8, Rgb)>, ParseError> {
    let (rows, cols) = KEYBOARD;
    let raw = raw_grid(v, rows, cols)?;
    Ok(raw
        .into_iter()
        .enumerate()
        .filter(|(_, x)| (*x as u32) & KEY_FLAG != 0)
        .map(|(i, x)| ((i / cols as usize) as u8, (i % cols as usize) as u8, bgr(x)))
        .collect())
}

fn grid_show(g: Grid) -> Effect {
    Effect::Show(Show { base: Base::Grid(g), keys: Vec::new() })
}

/// Legacy mouse custom effect: a 30-LED array indexed by the old mouse LED enum
/// (1 scroll wheel, 2 logo, 3 backlight, 4–10 left strip, 11–17 right strip).
fn mouse_legacy(v: &Value) -> Result<Grid, ParseError> {
    let raw = raw_grid(v, 1, 30)?;
    let (rows, cols) = MOUSE;
    let mut g = Grid::filled(rows, cols, [0; 3]);
    let mut set = |id: u16, x: i64| {
        let (r, c) = key::cell(id);
        g.cells[r as usize * cols as usize + c as usize] = bgr(x);
    };
    for (i, &x) in raw.iter().enumerate() {
        match i {
            1 => set(mouse_led::SCROLLWHEEL, x),
            2 => set(mouse_led::LOGO, x),
            3 => set(mouse_led::BACKLIGHT, x),
            4..=10 => set(mouse_led::left_side(i as u16 - 3), x),
            11..=17 => set(mouse_led::right_side(i as u16 - 10), x),
            _ => {}
        }
    }
    Ok(g)
}

/// Parse one effect object `{"effect": "CHROMA_…", "param": …}` for `target`.
pub fn parse_effect(target: Target, v: &Value) -> Result<Effect, ParseError> {
    let name = v.get("effect").and_then(|e| e.as_str()).ok_or(ParseError::Invalid)?;
    let param = v.get("param").unwrap_or(&Value::Null);
    let (rows, cols) = target.dims();
    let colour = |field: &str| param.get(field).and_then(num).map(bgr);
    match name {
        "CHROMA_NONE" => Ok(Effect::None),
        "CHROMA_STATIC" => {
            let c = colour("color").ok_or(ParseError::Invalid)?;
            Ok(grid_show(Grid::filled(rows, cols, c)))
        }
        "CHROMA_CUSTOM" | "CHROMA_CUSTOM2" | "CHROMA_CUSTOM_KEY" => parse_custom(target, name, param),
        "CHROMA_BREATHING" => {
            // type 1 = two colours, 2 = random colours.
            let random = param.get("type").and_then(num) == Some(2);
            let colours = if random {
                Vec::new()
            } else {
                vec![colour("color1").unwrap_or([0xff; 3]), colour("color2").unwrap_or([0; 3])].into_iter().filter(|c| *c != [0; 3]).collect()
            };
            Ok(Effect::Show(Show { base: Base::Anim { anim: Anim::Breathing(colours), rows, cols }, keys: Vec::new() }))
        }
        "CHROMA_SPECTRUMCYCLING" => Ok(Effect::Show(Show { base: Base::Anim { anim: Anim::Spectrum, rows, cols }, keys: Vec::new() })),
        "CHROMA_WAVE" => {
            let right = param.get("direction").and_then(num).unwrap_or(1) != 2;
            Ok(Effect::Show(Show { base: Base::Anim { anim: Anim::Wave { right }, rows, cols }, keys: Vec::new() }))
        }
        // Key-press effects need the typed keys, which Tsuba never reads.
        "CHROMA_REACTIVE" | "CHROMA_STARLIGHT" => Err(ParseError::NotSupported),
        _ => Err(ParseError::NotSupported),
    }
}

fn parse_custom(target: Target, name: &str, param: &Value) -> Result<Effect, ParseError> {
    match target {
        Target::Keyboard => {
            if param.is_object() {
                // CUSTOM_KEY: {color 6×22, key 6×22}; CUSTOM2: {color 8×24, key 6×22}.
                let colour = param.get("color").ok_or(ParseError::Invalid)?;
                let ext = name == "CHROMA_CUSTOM2" || shape(colour).is_some_and(|(r, c)| r > 6 || c > 22);
                let (rows, cols) = if ext { KEYBOARD_EXT } else { KEYBOARD };
                let base = grid_from(colour, rows, cols)?;
                let keys = match param.get("key") {
                    Some(k) if !k.is_null() => keys_from(k)?,
                    _ => Vec::new(),
                };
                Ok(Effect::Show(Show { base: Base::Grid(base), keys }))
            } else {
                // CUSTOM: 6×22 (an 8×24 array, or CUSTOM2 with a bare array, is the extended grid).
                let ext = name == "CHROMA_CUSTOM2" || shape(param).ok_or(ParseError::Invalid)?.0 > 6;
                let (rows, cols) = if ext { KEYBOARD_EXT } else { KEYBOARD };
                Ok(grid_show(grid_from(param, rows, cols)?))
            }
        }
        Target::Mouse => {
            let (r, _) = shape(param).ok_or(ParseError::Invalid)?;
            if r == 1 && name == "CHROMA_CUSTOM" {
                Ok(grid_show(mouse_legacy(param)?))
            } else {
                Ok(grid_show(grid_from(param, MOUSE.0, MOUSE.1)?))
            }
        }
        Target::Mousepad => {
            // 15 LEDs (original mat) or 20 (newer mats).
            let n = shape(param).ok_or(ParseError::Invalid)?.1;
            Ok(grid_show(grid_from(param, 1, if n > 15 { 20 } else { 15 })?))
        }
        t => {
            let (rows, cols) = t.dims();
            Ok(grid_show(grid_from(param, rows, cols)?))
        }
    }
}

// ---- mapping onto devices ----------------------------------------------------------------

/// For each hardware cell (row-major over hw rows×cols), the source-grid cells it averages.
#[derive(Debug, Clone, PartialEq)]
pub struct CellMap {
    pub src: (u8, u8),
    pub hw: (u8, u8),
    pub cells: Vec<Vec<u16>>,
    /// Cells without a source cell of their own, coloured from their nearest neighbour
    /// (e.g. keyboard underglow).
    pub derived: Vec<bool>,
}

impl CellMap {
    fn empty(src: (u8, u8), hw: (u8, u8)) -> Self {
        let n = hw.0 as usize * hw.1 as usize;
        Self { src, hw, cells: vec![Vec::new(); n], derived: vec![false; n] }
    }

    fn set(&mut self, hw: (u8, u8), src: &[(u8, u8)]) {
        if hw.0 >= self.hw.0 || hw.1 >= self.hw.1 {
            return;
        }
        let i = hw.0 as usize * self.hw.1 as usize + hw.1 as usize;
        self.cells[i] = src.iter().filter(|(r, c)| *r < self.src.0 && *c < self.src.1).map(|(r, c)| *r as u16 * self.src.1 as u16 + *c as u16).collect();
    }

    /// Source cells of one hardware cell.
    pub fn sources(&self, r: u8, c: u8) -> &[u16] {
        self.cells.get(r as usize * self.hw.1 as usize + c as usize).map_or(&[], |v| v.as_slice())
    }

    /// Every hw cell takes the proportionally corresponding source cell.
    pub fn proportional(src: (u8, u8), hw: (u8, u8)) -> Self {
        let mut m = Self::empty(src, hw);
        for r in 0..hw.0 {
            for c in 0..hw.1 {
                let sr = ((r as f32 + 0.5) * src.0 as f32 / hw.0 as f32) as u8;
                let sc = ((c as f32 + 0.5) * src.1 as f32 / hw.1 as f32) as u8;
                m.set((r, c), &[(sr.min(src.0 - 1), sc.min(src.1 - 1))]);
            }
        }
        m
    }

    /// Hardware cell (r, c) = source cell (r, c).
    pub fn identity(src: (u8, u8), hw: (u8, u8)) -> Self {
        let mut m = Self::empty(src, hw);
        for r in 0..hw.0.min(src.0) {
            for c in 0..hw.1.min(src.1) {
                m.set((r, c), &[(r, c)]);
            }
        }
        m
    }

    /// Give every LED in `geo` that has no source the sources of the nearest LED that has.
    pub fn fill_nearest(&mut self, geo: &Geometry) {
        let mapped: Vec<((u8, u8), (f32, f32))> =
            geo.pos.iter().filter(|(rc, _)| !self.sources(rc.0, rc.1).is_empty()).map(|(rc, p)| (*rc, *p)).collect();
        if mapped.is_empty() {
            return;
        }
        let mut leds: Vec<_> = geo.pos.iter().collect();
        leds.sort_by_key(|(rc, _)| **rc);
        for (rc, p) in leds {
            if rc.0 >= self.hw.0 || rc.1 >= self.hw.1 || !self.sources(rc.0, rc.1).is_empty() {
                continue;
            }
            let near = mapped
                .iter()
                .min_by(|a, b| {
                    let d = |q: &(f32, f32)| (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2);
                    d(&a.1).total_cmp(&d(&b.1)).then(a.0.cmp(&b.0))
                })
                .map(|(rc, _)| *rc)
                .expect("non-empty");
            let i = rc.0 as usize * self.hw.1 as usize + rc.1 as usize;
            self.cells[i] = self.sources(near.0, near.1).to_vec();
            self.derived[i] = true;
        }
    }

    /// Render a source grid (must have this map's source dimensions) into hw rows.
    pub fn apply(&self, g: &Grid) -> Vec<Vec<Rgb>> {
        let (rows, cols) = self.hw;
        (0..rows as usize).map(|r| (0..cols as usize).map(|c| average(g, &self.cells[r * cols as usize + c])).collect()).collect()
    }
}

fn average(g: &Grid, idx: &[u16]) -> Rgb {
    match idx {
        [] => [0; 3],
        [i] => g.cells.get(*i as usize).copied().unwrap_or_default(),
        _ => {
            let mut sum = [0u32; 3];
            for i in idx {
                let c = g.cells.get(*i as usize).copied().unwrap_or_default();
                (0..3).for_each(|k| sum[k] += c[k] as u32);
            }
            let n = idx.len() as u32;
            sum.map(|s| ((s + n / 2) / n) as u8)
        }
    }
}

/// BlackWidow V4 Pro (1532:028d) hardware grid 9×24 from the 6×22 keyboard grid: key
/// columns are one to the right (SDK column c → hardware column c+1; the command dial at
/// hardware (0,1) takes SDK (0,0)). The macro keys follow their documented ids by label:
/// SDK MACRO1..5 = (1..5, 0) and the board's M1..M5 sit at hardware rows 5..1 of column 1.
/// Underglow (rows 6–7) and any other LED without a key take their nearest key's colour.
pub fn blackwidow_v4_pro_v1(geo: &Geometry) -> CellMap {
    let mut m = CellMap::empty(KEYBOARD, (9, 24));
    for r in 0..KEYBOARD.0 {
        for c in 0..KEYBOARD.1 {
            let hw = if c == 0 && (1..=5).contains(&r) { (6 - r, 1) } else { (r, c + 1) };
            // The four media keys (SDK row 0, columns 18–21) sit where the picture pack says
            // their LEDs are (so a corrected pack also corrects games).
            let hw = match (r, c, geo.media) {
                (0, 18..=21, Some(media)) => media[c as usize - 18],
                _ => hw,
            };
            m.set(hw, &[(r, c)]);
        }
    }
    // Only real LEDs keep a mapping (Geometry lists them when there is an image pack).
    restrict_to(&mut m, geo);
    m.fill_nearest(geo);
    m
}

/// BlackWidow V4 Pro from the extended 8×24 grid: taken as the hardware layout itself
/// (rows 0–7, same columns), which is the matrix the extended grid was introduced for.
pub fn blackwidow_v4_pro_ext(geo: &Geometry) -> CellMap {
    let mut m = CellMap::identity(KEYBOARD_EXT, (9, 24));
    restrict_to(&mut m, geo);
    m.fill_nearest(geo);
    m
}

fn restrict_to(m: &mut CellMap, geo: &Geometry) {
    let cols = m.hw.1 as usize;
    for (i, v) in m.cells.iter_mut().enumerate() {
        if !geo.pos.contains_key(&((i / cols) as u8, (i % cols) as u8)) {
            v.clear();
        }
    }
}

/// Basilisk V3 Pro (1532:00aa) custom-frame row 1×13 from the 9×7 mouse grid:
/// 0 logo ← LOGO, 1 wheel ← SCROLLWHEEL, 2–12 underglow front-left → left → back → right →
/// front-right ← the side strips and the bottom (back) row, two strip LEDs per underglow LED.
pub fn basilisk_v3_pro() -> CellMap {
    use mouse_led::*;
    let mut m = CellMap::empty(MOUSE, (1, 13));
    let c = key::cell;
    m.set((0, 0), &[c(LOGO)]);
    m.set((0, 1), &[c(SCROLLWHEEL)]);
    let groups: [&[u16]; 11] = [
        &[left_side(1), left_side(2)],
        &[left_side(3), left_side(4)],
        &[left_side(5), left_side(6)],
        &[left_side(7), bottom(1)],
        &[bottom(2)],
        &[bottom(3)],
        &[bottom(4)],
        &[right_side(7), bottom(5)],
        &[right_side(5), right_side(6)],
        &[right_side(3), right_side(4)],
        &[right_side(1), right_side(2)],
    ];
    for (i, g) in groups.iter().enumerate() {
        let cells: Vec<_> = g.iter().map(|id| c(*id)).collect();
        m.set((0, 2 + i as u8), &cells);
    }
    m
}

/// How a device takes colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Per-LED custom frames, rows×cols.
    Frame { rows: u8, cols: u8 },
    /// One colour per lighting region (single-LED zones), by region id.
    Zones(Vec<u8>),
}

/// Everything needed to render a target's effects onto one device.
#[derive(Debug, Clone)]
pub struct DeviceMap {
    pub target: Target,
    pub out: Output,
    maps: Vec<CellMap>,
}

/// Region ids (Synapse `ledId`) used for zone mapping.
const REGION_SCROLL: u8 = 1;
const REGION_LOGO: u8 = 4;
const REGION_BACKLIGHT: u8 = 5;

impl DeviceMap {
    /// Build the map for a device. `geo` is the device's LED geometry (image pack), used to
    /// colour LEDs that have no source cell (underglow) from their nearest key.
    pub fn new(pid: u16, kind: tsuba_hid::Kind, out: Output, geo: Option<&Geometry>) -> Option<Self> {
        let target = Target::for_kind(kind)?;
        let sources: Vec<(u8, u8)> = match target {
            Target::Keyboard => vec![KEYBOARD, KEYBOARD_EXT],
            Target::Mousepad => vec![(1, 15), (1, 20)],
            t => vec![t.dims()],
        };
        let maps = match &out {
            Output::Frame { rows, cols } => {
                let hw = (*rows, *cols);
                let grid_geo;
                let geo = match geo {
                    Some(g) => g,
                    None => {
                        grid_geo = Geometry::grid(hw.0, hw.1);
                        &grid_geo
                    }
                };
                sources
                    .iter()
                    .map(|&src| match (pid, target, src, hw) {
                        (0x028d, Target::Keyboard, KEYBOARD, (9, 24)) => blackwidow_v4_pro_v1(geo),
                        (0x028d, Target::Keyboard, KEYBOARD_EXT, (9, 24)) => blackwidow_v4_pro_ext(geo),
                        (0x00aa, Target::Mouse, MOUSE, (1, 13)) => basilisk_v3_pro(),
                        _ => CellMap::proportional(src, hw),
                    })
                    .collect()
            }
            Output::Zones(ids) => sources.iter().map(|&src| zone_map(target, src, ids)).collect(),
        };
        Some(Self { target, out, maps })
    }

    fn map_for(&self, dims: (u8, u8)) -> std::borrow::Cow<'_, CellMap> {
        match self.maps.iter().find(|m| m.src == dims) {
            Some(m) => std::borrow::Cow::Borrowed(m),
            None => {
                let hw = self.maps.first().map_or((1, 1), |m| m.hw);
                std::borrow::Cow::Owned(CellMap::proportional(dims, hw))
            }
        }
    }

    /// Render `show` at time `t`. For [`Output::Frame`] the result is the frame rows; for
    /// [`Output::Zones`] one row with a colour per region. `underglow` (optional) recolours
    /// derived cells (LEDs without a key of their own) from a strip of colours, left to right.
    pub fn render(&self, show: &Show, t: f32, underglow: Option<&[Rgb]>) -> Vec<Vec<Rgb>> {
        let g = show.grid(t);
        let map = self.map_for(g.dims());
        let mut out = map.apply(&g);
        if !show.keys.is_empty() {
            let key_map = self.map_for(KEYBOARD);
            let mut overlay: HashMap<u16, Rgb> = HashMap::new();
            for (r, c, rgb) in &show.keys {
                if *r < KEYBOARD.0 && *c < KEYBOARD.1 {
                    overlay.insert(*r as u16 * KEYBOARD.1 as u16 + *c as u16, *rgb);
                }
            }
            let cols = key_map.hw.1 as usize;
            for (i, src) in key_map.cells.iter().enumerate() {
                if key_map.derived[i] {
                    continue;
                }
                if let [one] = src.as_slice() {
                    if let Some(rgb) = overlay.get(one) {
                        if let Some(cell) = out.get_mut(i / cols).and_then(|row| row.get_mut(i % cols)) {
                            *cell = *rgb;
                        }
                    }
                }
            }
        }
        if let Some(strip) = underglow.filter(|s| !s.is_empty()) {
            let cols = map.hw.1 as usize;
            for (r, row) in out.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    if map.derived[r * cols + c] {
                        *cell = strip[(c * strip.len() / cols).min(strip.len() - 1)];
                    }
                }
            }
        }
        out
    }
}

/// Zones: wheel / logo / backlight regions take the documented mouse LEDs; anything else
/// (and every region of a non-mouse device) takes the average of the whole grid, except a
/// keyboard logo which takes the keyboard's logo key.
fn zone_map(target: Target, src: (u8, u8), ids: &[u8]) -> CellMap {
    let mut m = CellMap::empty(src, (1, ids.len().max(1) as u8));
    let all: Vec<(u8, u8)> = (0..src.0).flat_map(|r| (0..src.1).map(move |c| (r, c))).collect();
    for (i, &id) in ids.iter().enumerate() {
        let cells: Vec<(u8, u8)> = match (target, id) {
            (Target::Mouse, REGION_SCROLL) => vec![key::cell(mouse_led::SCROLLWHEEL)],
            (Target::Mouse, REGION_LOGO) => vec![key::cell(mouse_led::LOGO)],
            (Target::Mouse, REGION_BACKLIGHT) => vec![key::cell(mouse_led::BACKLIGHT)],
            (Target::Mouse, _) => (1..=7).flat_map(|n| [key::cell(mouse_led::left_side(n)), key::cell(mouse_led::right_side(n))]).collect(),
            (Target::Keyboard, REGION_LOGO) if src == KEYBOARD => vec![key::cell(key::LOGO)],
            _ => all.clone(),
        };
        m.set((0, i as u8), &cells);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bw_geo() -> Geometry {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices/028d/device.json");
        Geometry::from_pack(&path, 9, 24).expect("028d image pack in the repo")
    }

    #[test]
    fn bgr_conversion() {
        assert_eq!(bgr(255), [255, 0, 0]);
        assert_eq!(bgr(0x00ff00), [0, 255, 0]);
        assert_eq!(bgr(0xff0000), [0, 0, 255]);
        assert_eq!(bgr(0x0102_0304), [4, 3, 2]); // key flag bit ignored
        assert_eq!(bgr(-256), [0, 255, 255]); // 0xffffff00
        assert_eq!(to_bgr([0x11, 0x22, 0x33]), 0x332211);
        assert_eq!(bgr(to_bgr([7, 8, 9]) as i64), [7, 8, 9]);
    }

    #[test]
    fn parses_documented_effects() {
        assert_eq!(parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_NONE"})), Ok(Effect::None));
        let Effect::Show(s) = parse_effect(Target::Mouse, &json!({"effect": "CHROMA_STATIC", "param": {"color": 255}})).unwrap() else { panic!() };
        assert_eq!(s.grid(0.0), Grid::filled(9, 7, [255, 0, 0]));
        // Keyboard 6×22 custom.
        let rows: Vec<Vec<u32>> = (0..6).map(|r| vec![r as u32; 22]).collect();
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_CUSTOM", "param": rows})).unwrap() else { panic!() };
        let g = s.grid(0.0);
        assert_eq!((g.rows, g.cols, g.get(5, 21)), (6, 22, [5, 0, 0]));
        // Custom key: only flagged keys override.
        let mut key = vec![vec![0i64; 22]; 6];
        key[0][1] = 0x0100_0000 | 0x00ff00;
        let colour = vec![vec![0xff0000i64; 22]; 6];
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_CUSTOM_KEY", "param": {"color": colour, "key": key}})).unwrap() else { panic!() };
        assert_eq!(s.keys, vec![(0, 1, [0, 255, 0])]);
        assert_eq!(s.grid(0.0).get(0, 1), [0, 0, 255]);
        // Extended keyboard grid.
        let colour = vec![vec![1i64; 24]; 8];
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_CUSTOM2", "param": {"color": colour, "key": vec![vec![0; 22]; 6]}})).unwrap() else { panic!() };
        assert_eq!(s.grid(0.0).dims(), KEYBOARD_EXT);
        // Mouse 9×7, mouse mat 15 / 20, headset 5, keypad 4×5, chroma link 5.
        let Effect::Show(s) = parse_effect(Target::Mouse, &json!({"effect": "CHROMA_CUSTOM2", "param": vec![vec![3; 7]; 9]})).unwrap() else { panic!() };
        assert_eq!(s.grid(0.0).dims(), MOUSE);
        for (t, p, dims) in [
            (Target::Mousepad, json!(vec![1; 15]), (1, 15)),
            (Target::Mousepad, json!(vec![1; 20]), (1, 20)),
            (Target::Headset, json!(vec![1; 5]), (1, 5)),
            (Target::Keypad, json!(vec![vec![1; 5]; 4]), (4, 5)),
            (Target::ChromaLink, json!([255, 65280, 16711680, 65535, 16711935]), (1, 5)),
        ] {
            let Effect::Show(s) = parse_effect(t, &json!({"effect": "CHROMA_CUSTOM", "param": p})).unwrap() else { panic!() };
            assert_eq!(s.grid(0.0).dims(), dims, "{t:?}");
        }
        // Short rows are padded, junk refused, unknown effects not supported.
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_CUSTOM", "param": [[255]]})).unwrap() else { panic!() };
        assert_eq!((s.grid(0.0).get(0, 0), s.grid(0.0).get(0, 1)), ([255, 0, 0], [0, 0, 0]));
        assert_eq!(parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_CUSTOM", "param": "x"})), Err(ParseError::Invalid));
        assert_eq!(parse_effect(Target::Keyboard, &json!({"param": 1})), Err(ParseError::Invalid));
        assert_eq!(parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_STATIC"})), Err(ParseError::Invalid));
        assert_eq!(parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_REACTIVE"})), Err(ParseError::NotSupported));
        assert_eq!(parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_BOGUS"})), Err(ParseError::NotSupported));
    }

    #[test]
    fn legacy_animations_render() {
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_BREATHING", "param": {"type": 1, "color1": 255, "color2": 65280}})).unwrap() else { panic!() };
        assert!(s.animated());
        assert_eq!(s.grid(0.0).get(0, 0), [0, 0, 0]);
        assert_eq!(s.grid(2.0).get(0, 0), [255, 0, 0]);
        assert_eq!(s.grid(6.0).get(0, 0), [0, 255, 0]);
        let Effect::Show(s) = parse_effect(Target::Keyboard, &json!({"effect": "CHROMA_WAVE", "param": {"direction": 1}})).unwrap() else { panic!() };
        let g = s.grid(0.0);
        assert_ne!(g.get(0, 0), g.get(0, 11));
        assert!(parse_effect(Target::Mouse, &json!({"effect": "CHROMA_SPECTRUMCYCLING"})).is_ok());
    }

    #[test]
    fn blackwidow_key_ids_land_on_their_keys() {
        let geo = bw_geo();
        let m = blackwidow_v4_pro_v1(&geo);
        // Hardware cells from the BlackWidow V4 Pro image pack (named LEDs).
        let src = |id: u16| {
            let (r, c) = key::cell(id);
            r as u16 * 22 + c as u16
        };
        for (id, hw) in [
            (key::ESC, (0, 2)),
            (key::F1, (0, 4)),
            (key::F12, (0, 15)),
            (key::OEM_1, (1, 2)),
            (key::BACKSPACE, (1, 15)),
            (key::NUMPAD_SUBTRACT, (1, 22)),
            (key::TAB, (2, 2)),
            (key::NUMPAD_ADD, (2, 22)),
            (key::CAPSLOCK, (3, 2)),
            (key::A, (3, 3)),
            (key::ENTER, (3, 15)),
            (key::LSHIFT, (4, 2)),
            (key::Z, (4, 4)),
            (key::UP, (4, 17)),
            (key::NUMPAD_ENTER, (4, 22)),
            (key::LCTRL, (5, 2)),
            (key::SPACE, (5, 8)),
            (key::RCTRL, (5, 15)),
            (key::RIGHT, (5, 18)),
            (key::NUMPAD_DECIMAL, (5, 21)),
            (key::macro_key(1), (5, 1)), // M1
            (key::macro_key(5), (1, 1)), // M5
            (0x0000, (0, 1)),            // command dial
        ] {
            assert_eq!(m.sources(hw.0, hw.1), &[src(id)], "key 0x{id:04x} → {hw:?}");
            assert!(!m.derived[hw.0 as usize * 24 + hw.1 as usize]);
        }
        // Underglow rows are derived from the nearest key; there is no LED at (0,3) or row 8.
        for c in 0..18 {
            assert!(m.derived[6 * 24 + c] && !m.sources(6, c as u8).is_empty(), "underglow 6-{c}");
        }
        for c in 0..20 {
            assert!(m.derived[7 * 24 + c] && !m.sources(7, c as u8).is_empty(), "underglow 7-{c}");
        }
        assert!(m.sources(0, 3).is_empty() && m.sources(8, 0).is_empty());

        // A whole frame: Esc red, everything else blue → hardware Esc red, its underglow neighbour not red.
        let mut g = Grid::filled(6, 22, [0, 0, 255]);
        g.cells[1] = [255, 0, 0];
        let dm = DeviceMap::new(0x028d, tsuba_hid::Kind::Keyboard, Output::Frame { rows: 9, cols: 24 }, Some(&geo)).unwrap();
        let rows = dm.render(&Show { base: Base::Grid(g), keys: vec![(1, 1, [0, 255, 0])] }, 0.0, None);
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0][2], [255, 0, 0]);
        assert_eq!(rows[0][4], [0, 0, 255]);
        assert_eq!(rows[1][2], [0, 255, 0]); // key overlay on ` ~
        assert_eq!(rows[7][10], [0, 0, 255]);
        // Underglow strip option recolours derived LEDs only.
        let rows = dm.render(&Show { base: Base::Grid(Grid::filled(6, 22, [9, 9, 9])), keys: vec![] }, 0.0, Some(&[[1, 0, 0], [0, 0, 1]]));
        assert_eq!((rows[0][2], rows[7][0], rows[7][19]), ([9, 9, 9], [1, 0, 0], [0, 0, 1]));
    }

    #[test]
    fn blackwidow_extended_grid_is_the_hardware_grid() {
        let geo = bw_geo();
        let dm = DeviceMap::new(0x028d, tsuba_hid::Kind::Keyboard, Output::Frame { rows: 9, cols: 24 }, Some(&geo)).unwrap();
        let mut g = Grid::filled(8, 24, [0; 3]);
        g.cells[2] = [1, 2, 3]; // (0,2) = Esc
        g.cells[6 * 24 + 5] = [4, 5, 6]; // underglow 6-5
        let rows = dm.render(&Show { base: Base::Grid(g), keys: vec![] }, 0.0, None);
        assert_eq!((rows[0][2], rows[6][5]), ([1, 2, 3], [4, 5, 6]));
    }

    #[test]
    fn basilisk_from_mouse_grid() {
        let m = basilisk_v3_pro();
        let cell = |id: u16| {
            let (r, c) = key::cell(id);
            r as u16 * 7 + c as u16
        };
        assert_eq!(m.sources(0, 0), &[cell(mouse_led::LOGO)]);
        assert_eq!(m.sources(0, 1), &[cell(mouse_led::SCROLLWHEEL)]);
        assert_eq!(m.sources(0, 2), &[cell(mouse_led::left_side(1)), cell(mouse_led::left_side(2))]);
        assert_eq!(m.sources(0, 7), &[cell(mouse_led::bottom(3))]);
        assert_eq!(m.sources(0, 12), &[cell(mouse_led::right_side(1)), cell(mouse_led::right_side(2))]);
        // Every strip/bottom LED is used exactly once; backlight is not on this mouse.
        let mut used: Vec<u16> = m.cells.iter().flatten().copied().collect();
        used.sort();
        used.dedup();
        assert_eq!(used.len(), 2 + 7 + 7 + 5);

        let mut g = Grid::filled(9, 7, [0; 3]);
        let (r, c) = key::cell(mouse_led::LOGO);
        g.cells[r as usize * 7 + c as usize] = [0, 255, 0];
        let (r, c) = key::cell(mouse_led::left_side(1));
        g.cells[r as usize * 7 + c as usize] = [200, 0, 0];
        let dm = DeviceMap::new(0x00aa, tsuba_hid::Kind::Mouse, Output::Frame { rows: 1, cols: 13 }, None).unwrap();
        let rows = dm.render(&Show { base: Base::Grid(g), keys: vec![] }, 0.0, None);
        assert_eq!((rows[0][0], rows[0][2], rows[0][3]), ([0, 255, 0], [100, 0, 0], [0, 0, 0]));
    }

    #[test]
    fn generic_devices_map_proportionally_and_by_zone() {
        // A 6×22 keyboard matrix is the SDK grid itself.
        let m = CellMap::proportional(KEYBOARD, (6, 22));
        assert_eq!(m, CellMap::identity(KEYBOARD, (6, 22)));
        // A 1×19 mat from the 15-LED effect, and a 4×5 keypad.
        let dm = DeviceMap::new(0x0c00, tsuba_hid::Kind::Mousemat, Output::Frame { rows: 1, cols: 19 }, None).unwrap();
        let g = Grid { rows: 1, cols: 15, cells: (0..15).map(|i| [i as u8, 0, 0]).collect() };
        let rows = dm.render(&Show { base: Base::Grid(g), keys: vec![] }, 0.0, None);
        assert_eq!((rows[0][0], rows[0][18]), ([0, 0, 0], [14, 0, 0]));
        // Zone mouse: wheel / logo regions take their LEDs; underglow averages the strips.
        let dm = DeviceMap::new(0x0084, tsuba_hid::Kind::Mouse, Output::Zones(vec![1, 4, 10]), None).unwrap();
        let mut g = Grid::filled(9, 7, [0; 3]);
        g.cells[2 * 7 + 3] = [10, 0, 0];
        g.cells[7 * 7 + 3] = [0, 20, 0];
        let rows = dm.render(&Show { base: Base::Grid(g), keys: vec![] }, 0.0, None);
        assert_eq!(rows, vec![vec![[10, 0, 0], [0, 20, 0], [0, 0, 0]]]);
        // Receivers have no target.
        assert!(DeviceMap::new(0x1, tsuba_hid::Kind::Receiver, Output::Zones(vec![]), None).is_none());
    }
}

#[cfg(test)]
mod app_rules_tests {
    use super::*;

    #[test]
    fn default_allows_everyone_and_blocklist_wins() {
        let mut r = AppRules::default();
        assert!(r.allowed("game.exe"));
        r.set_rule("game.exe", Some(false)).unwrap();
        assert!(!r.allowed("game.exe"));
        assert!(r.allowed("other.exe"));
        r.set_rule("game.exe", None).unwrap();
        assert!(r.allowed("game.exe"));
    }

    #[test]
    fn allow_list_mode_blocks_unknown_apps() {
        let mut r = AppRules { only_allowed: true, ..Default::default() };
        assert!(!r.allowed("new.exe"));
        r.set_rule("new.exe", Some(true)).unwrap();
        assert!(r.allowed("new.exe"));
    }

    #[test]
    fn a_title_never_earns_access_in_allow_list_mode() {
        let mut r = AppRules { only_allowed: true, ..Default::default() };
        r.set_rule("title:my game", Some(true)).unwrap();
        assert!(!r.allowed("title:my game"), "titles are client-supplied");
        r.set_rule("game.exe", Some(true)).unwrap();
        assert!(r.allowed("game.exe"));
        // Outside allow-list mode a title rule works, and a block always wins.
        r.only_allowed = false;
        assert!(r.allowed("title:my game"));
        r.set_rule("title:my game", Some(false)).unwrap();
        r.only_allowed = true;
        assert!(!r.allowed("title:my game"));
    }

    #[test]
    fn keys_prefer_the_process_name() {
        assert_eq!(app_key("Game.EXE", "My Game"), "game.exe");
        assert_eq!(app_key("", " My Game "), "title:my game");
    }

    #[test]
    fn rules_roundtrip_and_survive_garbage() {
        let dir = std::env::temp_dir().join(format!("tsuba-gla-{}", std::process::id()));
        let f = dir.join("rules.json");
        let mut r = AppRules::default();
        r.set_rule("a.exe", Some(false)).unwrap();
        r.record("a.exe", "A", "a.exe", 5);
        r.save(&f).unwrap();
        assert_eq!(AppRules::load(&f), r);
        std::fs::write(&f, "{not json").unwrap();
        assert_eq!(AppRules::load(&f), AppRules::default());
        assert!(r.set_rule("", Some(true)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seen_list_is_bounded() {
        let mut r = AppRules::default();
        for i in 0..250u64 {
            r.record(&format!("app{i}"), "x", "x", i);
        }
        assert_eq!(r.seen.len(), MAX_APPS);
        assert!(r.seen.contains_key("app249") && !r.seen.contains_key("app0"));
    }

    #[test]
    fn finds_the_socket_inode_of_a_loopback_client() {
        let listing = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:D431 0100007F:D3DB 01 00000000:00000000 00:00000000 00000000  1000        0 424242 1 0000000000000000 100 0 0 10 0\n   1: 0100007F:D3DB 0100007F:D431 01 00000000:00000000 00:00000000 00000000  1000        0 777 1 0000000000000000 100 0 0 10 0\n";
        assert_eq!(tcp_inode(listing, 0xD431), Some(424242));
        assert_eq!(tcp_inode(listing, 0x1234), None);
        // /proc/net/tcp6 shows 127.0.0.1 on a dual-stack socket as a mapped address, and ::1.
        let six = "  sl  local_address rem_address\n   0: 0000000000000000FFFF00000100007F:BA79 0000000000000000FFFF00000100007F:BACE 01 0:0 0:0 0 1000 0 555 1\n   1: 00000000000000000000000001000000:BA7A 00000000000000000000000001000000:BACF 01 0:0 0:0 0 1000 0 556 1\n   2: 20010DB8000000000000000000000001:BA7B 0000000000000000FFFF00000100007F:BACF 01 0:0 0:0 0 1000 0 557 1\n   3: 12345678ABCDEF000000FFFF0100007F:BA7C 0000000000000000FFFF00000100007F:BACF 01 0:0 0:0 0 1000 0 558 1\n";
        assert_eq!(tcp_inode(six, 0xBA79), Some(555));
        assert_eq!(tcp_inode(six, 0xBA7A), Some(556));
        assert_eq!(tcp_inode(six, 0xBA7B), None, "a global address is not loopback");
        assert_eq!(tcp_inode(six, 0xBA7C), None, "only the exact mapped form");
    }

    #[test]
    fn wine_programs_are_named_by_their_exe() {
        assert_eq!(process_name("Game.exe", b""), "Game.exe");
        assert_eq!(process_name("wine64-preloader", b"/usr/bin/wine64-preloader\0Z:\\games\\Cool Game\\cool.exe\0-flag\0"), "cool.exe");
        assert_eq!(process_name("wine64-preloader", b"/usr/bin/wine64-preloader\0"), "wine64-preloader");
    }
}
