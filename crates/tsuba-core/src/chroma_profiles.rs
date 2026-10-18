//! Named Chroma Studio profiles (Synapse's effect profiles): each holds a layer stack per
//! device. Stored in `~/.config/tsuba/chroma-profiles.json`; applying one makes it every
//! device's (startup) lighting.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::studio::{Layer, Span, Studio};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChromaProfile {
    pub name: String,
    /// Device pid → its layers.
    #[serde(default)]
    pub devices: BTreeMap<String, Studio>,
    /// Layers that span devices, drawn above every device's own layers.
    #[serde(default, skip_serializing_if = "Canvas::is_empty")]
    pub canvas: Canvas,
}

/// Where a device sits on the shared canvas, in canvas units (any scale; only ratios matter).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A layer on the shared canvas: an ordinary layer plus the devices (and LEDs) it covers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasLayer {
    #[serde(flatten)]
    pub layer: Layer,
    /// Device pid → LEDs `[row, col]` the layer covers on it (empty list = every LED).
    /// Devices not listed are not touched by the layer.
    #[serde(default)]
    pub targets: BTreeMap<String, Vec<[u8; 2]>>,
}

/// The shared 2D space of a profile: device placements and the layers that span them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Canvas {
    #[serde(default)]
    pub placements: BTreeMap<String, Placement>,
    /// Bottom layer first, like a device's layers.
    #[serde(default)]
    pub layers: Vec<CanvasLayer>,
}

impl Canvas {
    pub fn is_empty(&self) -> bool {
        self.placements.is_empty() && self.layers.is_empty()
    }

    /// Placement of `pid`, or a default slot (devices in a row, unit size) when it has none.
    fn placement(&self, pid: &str) -> Placement {
        self.placements.get(pid).copied().unwrap_or_else(|| {
            let i = self.placements.len() as f32;
            Placement { x: i * 1.2, y: 0.0, w: 1.0, h: 1.0 }
        })
    }

    /// Bounding box `(x0, y0, x1, y1)` of every placed device and every device a layer targets.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut pids: Vec<&String> = self.placements.keys().collect();
        for l in &self.layers {
            pids.extend(l.targets.keys());
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for pid in pids {
            let p = self.placement(pid);
            let (w, h) = (p.w.max(1e-3), p.h.max(1e-3));
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x + w);
            y1 = y1.max(p.y + h);
        }
        if x0 > x1 {
            return (0.0, 0.0, 1.0, 1.0);
        }
        (x0, y0, x1, y1)
    }

    /// Where `pid` sits inside the canvas bounding box.
    pub fn span_for(&self, pid: &str) -> Span {
        let (x0, y0, x1, y1) = self.bounds();
        let (bw, bh) = ((x1 - x0).max(1e-3), (y1 - y0).max(1e-3));
        let p = self.placement(pid);
        Span { origin: [(p.x - x0) / bw, (p.y - y0) / bh], scale: [p.w.max(1e-3) / bw, p.h.max(1e-3) / bh], aspect: (bw / bh).clamp(0.1, 10.0) }
    }
}

impl ChromaProfile {
    /// What each device runs: its own layers, then the canvas layers that cover it (laid out
    /// over the shared space). Devices without any layer are left out.
    pub fn expanded(&self) -> BTreeMap<String, Studio> {
        let mut out: BTreeMap<String, Studio> = self.devices.iter().filter(|(_, s)| !s.layers.is_empty()).map(|(p, s)| (p.clone(), s.clone())).collect();
        for cl in &self.canvas.layers {
            for (pid, leds) in &cl.targets {
                let mut l = cl.layer.clone();
                l.leds = leds.clone();
                l.span = Some(self.canvas.span_for(pid));
                out.entry(pid.clone()).or_default().layers.push(l);
            }
        }
        out
    }

    /// Clamp an imported profile to what Tsuba can run (see [`Studio::sanitise`]).
    pub fn sanitise(&mut self) {
        self.name = self.name.chars().filter(|c| !c.is_control()).take(64).collect::<String>().trim().to_string();
        let pid_ok = |p: &String| p.len() == 4 && p.bytes().all(|b| b.is_ascii_hexdigit());
        self.devices.retain(|p, _| pid_ok(p));
        self.devices.truncate_to(8);
        for s in self.devices.values_mut() {
            s.sanitise();
        }
        self.canvas.placements.retain(|p, _| pid_ok(p));
        let finite = |v: f32, d: f32| if v.is_finite() { v.clamp(-100.0, 100.0) } else { d };
        for p in self.canvas.placements.values_mut() {
            *p = Placement { x: finite(p.x, 0.0), y: finite(p.y, 0.0), w: finite(p.w, 1.0).max(0.01), h: finite(p.h, 1.0).max(0.01) };
        }
        self.canvas.layers.truncate(crate::studio::MAX_LAYERS);
        for cl in &mut self.canvas.layers {
            cl.layer.sanitise();
            cl.layer.span = None;
            cl.layer.leds.clear();
            cl.targets.retain(|p, _| pid_ok(p));
            for leds in cl.targets.values_mut() {
                leds.truncate(4096);
            }
        }
    }
}

trait TruncateMap {
    fn truncate_to(&mut self, n: usize);
}

impl<V> TruncateMap for BTreeMap<String, V> {
    fn truncate_to(&mut self, n: usize) {
        while self.len() > n {
            let Some(k) = self.keys().next_back().cloned() else { break };
            self.remove(&k);
        }
    }
}

/// File format of an exported Lighting Studio profile.
const EXPORT_KIND: &str = "tsuba-studio-profile";
/// Largest file [`import`] reads.
const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct ExportFile {
    kind: String,
    version: u32,
    profile: ChromaProfile,
}

/// Serialise a profile for sharing (device layers, canvas layers and placements; pictures included).
pub fn export(p: &ChromaProfile) -> Result<String> {
    Ok(serde_json::to_string_pretty(&ExportFile { kind: EXPORT_KIND.into(), version: 1, profile: p.clone() })?)
}

/// Parse an exported profile: checked, clamped ([`ChromaProfile::sanitise`]) and given a name
/// that is not in `taken` (" 2", " 3", … is appended when it is).
pub fn import(text: &str, taken: &[String]) -> Result<ChromaProfile> {
    ensure!(text.len() as u64 <= MAX_IMPORT_BYTES, "the file is too large for a lighting profile");
    let f: ExportFile = serde_json::from_str(text).context("not a Tsuba lighting profile file")?;
    ensure!(f.kind == EXPORT_KIND, "not a Tsuba lighting profile file");
    ensure!(f.version == 1, "this file was written by a newer Tsuba (format {})", f.version);
    let mut p = f.profile;
    p.sanitise();
    if p.name.is_empty() {
        p.name = "Imported lighting".into();
    }
    let base = p.name.clone();
    let mut n = 2;
    while taken.contains(&p.name) {
        p.name = format!("{base} {n}");
        n += 1;
    }
    ensure!(!p.expanded().is_empty() || !p.canvas.layers.is_empty(), "the profile has no layers");
    Ok(p)
}

/// Read and import a profile file.
pub fn import_file(path: &Path, taken: &[String]) -> Result<ChromaProfile> {
    let md = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(md.len() <= MAX_IMPORT_BYTES, "the file is too large for a lighting profile");
    import(&std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?, taken)
}

/// Write a profile to `path` (atomically).
pub fn export_file(p: &ChromaProfile, path: &Path) -> Result<()> {
    let tmp = path.with_extension("tmp-export");
    std::fs::write(&tmp, export(p)?).with_context(|| format!("writing {}", path.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChromaProfiles {
    #[serde(default)]
    pub profiles: Vec<ChromaProfile>,
    /// Name of the applied profile.
    #[serde(default)]
    pub active: String,
}

pub fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("chroma-profiles.json")
}

impl ChromaProfiles {
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
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, p)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ChromaProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn upsert(&mut self, p: ChromaProfile) -> Result<()> {
        ensure!(!p.name.trim().is_empty(), "the profile needs a name");
        match self.profiles.iter_mut().find(|x| x.name == p.name) {
            Some(slot) => *slot = p,
            None => self.profiles.push(p),
        }
        Ok(())
    }

    pub fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        ensure!(!new.trim().is_empty(), "the profile needs a name");
        ensure!(self.get(new).is_none(), "a profile named {new:?} already exists");
        let p = self.profiles.iter_mut().find(|x| x.name == old).context("no such profile")?;
        p.name = new.trim().to_string();
        if self.active == old {
            self.active = p.name.clone();
        }
        Ok(())
    }

    pub fn delete(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active == name {
            self.active.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::{Duration, StudioEffect};

    fn wave() -> StudioEffect {
        StudioEffect::Gradient { colours: vec!["000000".into(), "ffffff".into()], angle: 0.0, scroll: false, duration: Duration::Medium }
    }

    fn placed() -> Canvas {
        let mut c = Canvas::default();
        // A wide keyboard with a narrow mouse to its right, bottoms aligned.
        c.placements.insert("028d".into(), Placement { x: 0.0, y: 0.0, w: 3.0, h: 1.0 });
        c.placements.insert("00aa".into(), Placement { x: 3.5, y: 0.0, w: 0.5, h: 1.0 });
        c
    }

    #[test]
    fn span_maps_devices_into_the_canvas_box() {
        let c = placed();
        let kb = c.span_for("028d");
        let mouse = c.span_for("00aa");
        assert!((kb.origin[0] - 0.0).abs() < 1e-6 && (kb.scale[0] - 0.75).abs() < 1e-6);
        assert!((mouse.origin[0] - 0.875).abs() < 1e-6 && (mouse.scale[0] - 0.125).abs() < 1e-6);
        assert!((kb.aspect - 4.0).abs() < 1e-6);
    }

    #[test]
    fn expanded_adds_canvas_layers_above_device_layers() {
        let mut p = ChromaProfile { name: "x".into(), ..Default::default() };
        p.devices.insert("028d".into(), Studio { layers: vec![Layer::new("own", StudioEffect::Battery)] });
        p.canvas = placed();
        p.canvas.layers.push(CanvasLayer { layer: Layer::new("span", wave()), targets: BTreeMap::from([("028d".to_string(), vec![]), ("00aa".to_string(), vec![[0, 1]])]) });
        let e = p.expanded();
        let kb = &e["028d"].layers;
        assert_eq!(kb.len(), 2);
        assert_eq!(kb[0].name, "own");
        assert!(kb[0].span.is_none() && kb[1].span.is_some());
        let mouse = &e["00aa"].layers;
        assert_eq!(mouse.len(), 1);
        assert_eq!(mouse[0].leds, vec![[0, 1]]);
    }

    #[test]
    fn a_wave_flows_from_the_keyboard_into_the_mouse() {
        use crate::studio::{Geometry, Renderer};
        let mut p = ChromaProfile { name: "x".into(), ..Default::default() };
        p.canvas = placed();
        p.canvas.layers.push(CanvasLayer { layer: Layer::new("g", wave()), targets: BTreeMap::from([("028d".to_string(), vec![]), ("00aa".to_string(), vec![])]) });
        let e = p.expanded();
        let kb = Renderer::new(e["028d"].clone(), Geometry::grid(1, 8)).frame(0.0);
        let mouse = Renderer::new(e["00aa"].clone(), Geometry::grid(1, 2)).frame(0.0);
        // Left to right: keyboard dark → bright, then the mouse picks up where it left off.
        assert!(kb[0][0][0] < kb[0][7][0]);
        assert!(kb[0][7][0] <= mouse[0][0][0] && mouse[0][0][0] < mouse[0][1][0]);
    }

    #[test]
    fn old_profiles_load_and_stay_canvas_free() {
        let p: ChromaProfile = serde_json::from_str(r#"{"name":"Old","devices":{"028d":{"layers":[{"effect":{"type":"spectrum"}}]}}}"#).unwrap();
        assert!(p.canvas.is_empty());
        assert!(!serde_json::to_string(&p).unwrap().contains("canvas"));
        assert_eq!(p.expanded()["028d"].layers.len(), 1);
    }

    #[test]
    fn export_import_roundtrip_renames_on_clash() {
        let mut p = ChromaProfile { name: "Night".into(), ..Default::default() };
        p.devices.insert("00aa".into(), Studio { layers: vec![Layer::new("l", wave())] });
        p.canvas = placed();
        p.canvas.layers.push(CanvasLayer { layer: Layer::new("span", wave()), targets: BTreeMap::from([("028d".to_string(), vec![[0, 1]])]) });
        let text = export(&p).unwrap();
        let back = import(&text, &[]).unwrap();
        assert_eq!(back, p);
        let again = import(&text, &["Night".to_string(), "Night 2".to_string()]).unwrap();
        assert_eq!(again.name, "Night 3");
    }

    #[test]
    fn import_refuses_foreign_and_broken_files() {
        assert!(import("not json", &[]).is_err());
        assert!(import(r#"{"kind":"something-else","version":1,"profile":{"name":"x"}}"#, &[]).is_err());
        assert!(import(r#"{"kind":"tsuba-studio-profile","version":9,"profile":{"name":"x"}}"#, &[]).is_err());
        assert!(import(r#"{"kind":"tsuba-studio-profile","version":1,"profile":{"name":"empty"}}"#, &[]).is_err());
        let big = " ".repeat(5 * 1024 * 1024);
        assert!(import(&big, &[]).is_err());
    }

    #[test]
    fn import_clamps_hostile_values() {
        let text = r#"{"kind":"tsuba-studio-profile","version":1,"profile":{"name":"  a\u0001b  ",
            "devices":{"../../x":{"layers":[{"effect":{"type":"static","colour":"ff0000"}}]},"028d":{"layers":[{"speed":1e30,"effect":{"type":"static","colour":"nope"}}]}},
            "canvas":{"placements":{"028d":{"x":1e30,"y":-5,"w":0,"h":2}}}}}"#;
        let p = import(text, &[]).unwrap();
        assert_eq!(p.name, "ab");
        assert_eq!(p.devices.len(), 1);
        let l = &p.devices["028d"].layers[0];
        assert_eq!(l.speed, 5.0);
        assert_eq!(l.effect, StudioEffect::Static { colour: "ffffff".into() });
        let pl = p.canvas.placements["028d"];
        assert!(pl.x <= 100.0 && pl.w >= 0.01);
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tsuba-cp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = ChromaProfile { name: "F".into(), ..Default::default() };
        p.devices.insert("028d".into(), Studio { layers: vec![Layer::new("l", StudioEffect::Battery)] });
        let f = dir.join("p.json");
        export_file(&p, &f).unwrap();
        assert_eq!(import_file(&f, &[]).unwrap(), p);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
