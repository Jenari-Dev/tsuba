//! Audio Meter input: reads what the PC is playing from the default output's monitor (parec),
//! and turns it into band levels for AudioMeter layers. Runs only while such a layer is active.

use std::io::Read;
use std::process::{Child, Command, Stdio};
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

pub fn start() -> Result<AudioCapture> {
    let mut child = Command::new("parec")
        .args(["--device=@DEFAULT_MONITOR@", "--format=float32le", "--channels=1", "--rate=16000", "--latency-msec=30", "--raw", "--client-name=Tsuba Audio Meter"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("starting parec (PipeWire/Pulse audio capture)")?;
    let mut out = child.stdout.take().context("parec stdout")?;
    let levels = Arc::new(Mutex::new(AudioLevels { bands: vec![0.0; BANDS] }));
    let shared = levels.clone();
    // Log-spaced band centres, 60 Hz – 7 kHz.
    let freqs: Vec<f32> = (0..BANDS).map(|i| 60.0 * (7000.0f32 / 60.0).powf(i as f32 / (BANDS - 1) as f32)).collect();
    std::thread::Builder::new().name("audio-meter".into()).spawn(move || {
        let mut buf = vec![0u8; BLOCK * 4];
        let mut peak = 1e-3f32;
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
            match shared.lock() {
                Ok(mut l) => l.bands.clone_from(&shown),
                Err(_) => break,
            }
        }
    })?;
    Ok(AudioCapture { child, levels })
}

/// One capture shared by every device that needs it: the first caller starts it, later
/// callers reuse it, and it stops when the last holder drops its handle.
pub fn shared() -> Result<Arc<AudioCapture>> {
    static SHARED: Mutex<Weak<AudioCapture>> = Mutex::new(Weak::new());
    let mut slot = SHARED.lock().unwrap();
    if let Some(c) = slot.upgrade() {
        return Ok(c);
    }
    let c = Arc::new(start()?);
    *slot = Arc::downgrade(&c);
    Ok(c)
}
