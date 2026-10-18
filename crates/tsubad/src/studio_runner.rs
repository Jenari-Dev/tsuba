//! Renders a Chroma Studio profile continuously and streams it to one device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::{info, warn};
use tsuba_core::studio::{Geometry, Renderer, Studio};
use tsuba_hid::{Device, Model};
use tsuba_proto::chroma;

/// A running studio render thread; dropping it stops the thread.
pub struct StudioHandle {
    /// What is being rendered (kept so game lighting can resume it afterwards).
    pub studio: Studio,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for StudioHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn spawn(pid: String, model: Model, dev: Arc<Mutex<Device>>, studio: Studio) -> Result<StudioHandle> {
    anyhow::ensure!(
        model.spec.effects.is_empty() || model.spec.has_effect("custom_frame"),
        "{} has no per-LED custom lighting, so Lighting Studio can't drive it",
        model.name
    );
    let region = {
        let d = dev.lock().unwrap();
        chroma::regions(&d.query(chroma::GET_REGION_LIST, &[])?)
            .into_iter()
            .max_by_key(|r| r.rows as u32 * r.cols as u32)
            .context("no lighting region")?
    };
    // Mice with single-LED zones take one combined frame row (mapped on the device).
    let geo = if model.pid == 0x00aa {
        Geometry::basilisk_v3_pro()
    } else {
        anyhow::ensure!(region.rows * region.cols > 1, "Lighting Studio needs a per-LED matrix (this device has single-LED zones)");
        Geometry::from_pack(&tsuba_core::studio::pack_path(&pid), region.rows, region.cols).unwrap_or_else(|| Geometry::grid(region.rows, region.cols))
    };
    let period = Duration::from_millis(1000 / region.update_rate.clamp(10, 60) as u64);
    let layers = studio.layers.len();
    let kept = studio.clone();
    let wants_battery = studio.uses_battery();
    // Audio Meter layers need the PC's audio levels; the capture lives as long as the thread.
    let captures: Vec<(String, std::sync::Arc<crate::audio::AudioCapture>)> = studio.audio_sources().into_iter().map(|src| crate::audio::shared(&src).map(|c| (src, c))).collect::<Result<_>>()?;
    // Ambient layers need the screen (the portal may show KDE's share-screen dialog once).
    let screen = if studio.uses_screen() { Some(crate::ambient::shared()?) } else { None };
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::Builder::new().name(format!("studio-{pid}")).spawn(move || {
        info!("{pid}: lighting studio running ({layers} layers, {} LEDs, {:?}/frame)", geo.pos.len(), period);
        let mut renderer = Renderer::new(studio, geo);
        renderer.audio = captures.iter().map(|(src, c)| (src.clone(), c.levels.clone())).collect();
        renderer.screen = screen.as_ref().map(|c| c.frame.clone());
        let _captures = captures;
        let _screen = screen;
        let start = Instant::now();
        let mut armed = false;
        let mut battery_at: Option<Instant> = None;
        while !flag.load(Ordering::Relaxed) {
            let tick = Instant::now();
            if wants_battery && battery_at.is_none_or(|b| b.elapsed() > Duration::from_secs(30)) {
                battery_at = Some(tick);
                let d = dev.lock().unwrap();
                let level = d.query(tsuba_proto::commands::GET_BATTERY_LEVEL, &[]).ok().and_then(|a| a.get(1).copied());
                let charging = d.query(tsuba_proto::commands::GET_CHARGING_STATUS, &[]).ok().and_then(|a| a.get(1).copied()).unwrap_or(0) == 1;
                renderer.battery = level.map(|l| (((l as u32 * 100 + 127) / 255) as u8, charging));
            }
            let frame = renderer.frame(start.elapsed().as_secs_f32());
            {
                let d = dev.lock().unwrap();
                for (row, leds) in frame.iter().enumerate() {
                    if let Err(e) = d.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(row as u8, 0, leds)) {
                        warn!("{pid}: studio stopped: {e}");
                        return;
                    }
                }
                if !armed {
                    let effect = chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame);
                    if let Err(e) = d.query(chroma::set_effect(0), &effect) {
                        warn!("{pid}: studio could not enable custom frames: {e}");
                        return;
                    }
                    armed = true;
                }
            }
            if let Some(rest) = period.checked_sub(tick.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        info!("{pid}: lighting studio stopped");
    })?;
    Ok(StudioHandle { studio: kept, stop, thread: Some(thread) })
}
