//! Audio Meter input: reads what the PC is playing from the default output's monitor (parec),
//! and turns it into band levels for AudioMeter layers. Runs only while such a layer is active.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use anyhow::{Context, Result};
use tracing::warn;
use tsuba_core::studio::AudioLevels;

const RATE: f32 = 16_000.0;
const BLOCK: usize = 512; // 32 ms
pub const BANDS: usize = 24;

pub struct AudioCapture {
    child: Child,
    pub levels: Arc<Mutex<AudioLevels>>,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Goertzel power of `freq` in `x`.
fn goertzel(x: &[f32], freq: f32) -> f32 {
    let k = 2.0 * (2.0 * std::f32::consts::PI * freq / RATE).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + k * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - k * s1 * s2).max(0.0)
}

/// A source name as `parec --device=` takes it: PipeWire/Pulse names only (no option injection).
pub fn valid_source(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && !name.starts_with('-') && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-:@+=,[]() ".contains(c))
}

/// Start capturing `source` (None / "" = what the PC is playing: the default output's monitor).
pub fn start(source: Option<&str>) -> Result<AudioCapture> {
    let device = match source.filter(|s| !s.is_empty()) {
        Some(s) => {
            anyhow::ensure!(valid_source(s), "not a valid audio source name");
            s.to_string()
        }
        None => "@DEFAULT_MONITOR@".to_string(),
    };
    let mut child = Command::new("parec")
        .arg(format!("--device={device}"))
        .args(["--format=float32le", "--channels=1", "--rate=16000", "--latency-msec=30", "--raw", "--client-name=Tsuba Audio Meter"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("starting parec (PipeWire/Pulse audio capture)")?;
    let mut out = child.stdout.take().context("parec stdout")?;
    let levels = Arc::new(Mutex::new(AudioLevels { bands: vec![0.0; BANDS], level: 0.0 }));
    let shared = levels.clone();
    // Log-spaced band centres, 60 Hz – 7 kHz.
    let freqs: Vec<f32> = (0..BANDS).map(|i| 60.0 * (7000.0f32 / 60.0).powf(i as f32 / (BANDS - 1) as f32)).collect();
    std::thread::Builder::new().name("audio-meter".into()).spawn(move || {
        let mut buf = vec![0u8; BLOCK * 4];
        let mut peak = 1e-3f32;
        let mut loud_peak = 1e-3f32;
        let mut loud_shown = 0.0f32;
        let mut shown = vec![0.0f32; BANDS];
        loop {
            if out.read_exact(&mut buf).is_err() {
                warn!("audio meter: capture ended");
                break;
            }
            let x: Vec<f32> = buf.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            let raw: Vec<f32> = freqs.iter().map(|f| goertzel(&x, *f).sqrt()).collect();
            // Automatic gain: follow the loudest band, decaying slowly.
            let m = raw.iter().copied().fold(0.0, f32::max);
            peak = (peak * 0.995).max(m).max(1e-3);
            for (i, r) in raw.iter().enumerate() {
                let v = (r / peak).powf(0.6).min(1.0);
                // Fast attack, slow release.
                shown[i] = if v > shown[i] { v } else { shown[i] * 0.85 + v * 0.15 };
            }
            // Overall loudness (RMS) with the same slow automatic gain, for the Level meter.
            let rms = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
            loud_peak = (loud_peak * 0.995).max(rms).max(1e-3);
            let lv = (rms / loud_peak).powf(0.8).min(1.0);
            loud_shown = if lv > loud_shown { lv } else { loud_shown * 0.85 + lv * 0.15 };
            match shared.lock() {
                Ok(mut l) => {
                    l.bands.clone_from(&shown);
                    l.level = loud_shown;
                }
                Err(_) => break,
            }
        }
    })?;
    Ok(AudioCapture { child, levels })
}

/// One capture per source shared by every device that needs it: the first caller starts it,
/// later callers reuse it, and it stops when the last holder drops its handle.
pub fn shared(source: &str) -> Result<Arc<AudioCapture>> {
    static SHARED: Mutex<Option<HashMap<String, Weak<AudioCapture>>>> = Mutex::new(None);
    let mut slot = SHARED.lock().unwrap();
    let map = slot.get_or_insert_with(HashMap::new);
    map.retain(|_, w| w.strong_count() > 0);
    if let Some(c) = map.get(source).and_then(Weak::upgrade) {
        return Ok(c);
    }
    let c = Arc::new(start(Some(source))?);
    map.insert(source.to_string(), Arc::downgrade(&c));
    Ok(c)
}

/// An audio source the Audio Meter can listen to.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Source {
    pub name: String,
    pub description: String,
    /// A monitor of an output (what is playing) rather than a microphone / line input.
    pub monitor: bool,
}

/// Parse `pactl -f json list sources`.
pub fn parse_sources(json: &str) -> Vec<Source> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    let mut out: Vec<Source> = v
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let name = s["name"].as_str()?.to_string();
                    valid_source(&name).then(|| Source {
                        description: s["description"].as_str().unwrap_or(&name).chars().filter(|c| !c.is_control()).take(120).collect(),
                        monitor: name.ends_with(".monitor") || s["monitor_of_sink"].is_string(),
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.monitor.cmp(&a.monitor).then(a.description.cmp(&b.description)));
    out
}

/// The capture sources PipeWire/Pulse offers right now (empty when `pactl` is missing).
pub fn sources() -> Vec<Source> {
    Command::new("pactl").args(["-f", "json", "list", "sources"]).stderr(Stdio::null()).output().map(|o| parse_sources(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pactl_sources_monitors_first() {
        let j = r#"[{"name":"alsa_input.usb-mic","description":"Mic","monitor_of_sink":null},
                    {"name":"alsa_output.hdmi.monitor","description":"Monitor of HDMI","monitor_of_sink":"alsa_output.hdmi"},
                    {"name":"-evil","description":"x"}]"#;
        let s = parse_sources(j);
        assert_eq!(s.len(), 2);
        assert!(s[0].monitor && s[0].name == "alsa_output.hdmi.monitor");
        assert!(!s[1].monitor);
        assert!(parse_sources("garbage").is_empty());
    }

    #[test]
    fn source_names_cannot_inject_options() {
        assert!(valid_source("alsa_output.pci-0000_01_00.1.hdmi-stereo.monitor"));
        assert!(!valid_source("--format=s16le"));
        assert!(!valid_source(""));
        assert!(!valid_source("a;b"));
    }
}
