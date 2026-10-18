//! Chroma Studio: layered software lighting effects rendered into per-LED frames.
//!
//! A [`Studio`] is a stack of layers; each layer applies one effect to a set of LEDs
//! (grid row/column; empty = every LED). Layers render bottom to top and an upper
//! layer replaces the colour below it, except where its effect is transparent
//! (e.g. Starlight between stars). tsubad streams the frames to the device.

use std::collections::HashMap;
use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::parse_rgb;

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Studio {
    #[serde(default)]
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// LED grid positions `[row, col]`; empty = every LED of the device.
    #[serde(default)]
    pub leds: Vec<[u8; 2]>,
    pub effect: StudioEffect,
    /// Layer brightness 0..1 (scales this layer's colours).
    #[serde(default = "full")]
    pub brightness: f32,
    /// How the layer combines with the layers below it.
    #[serde(default)]
    pub blend: Blend,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    /// Replace what's below (Synapse behaviour).
    #[default]
    Cover,
    /// Add light on top: lower layers glow through.
    Add,
    /// Keep the brighter of the two, per channel.
    Lighten,
}

fn full() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

/// Synapse's short / medium / long effect duration.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Duration {
    Short,
    #[default]
    Medium,
    Long,
}

impl Duration {
    fn scale(self) -> f32 {
        match self {
            Duration::Short => 0.5,
            Duration::Medium => 1.0,
            Duration::Long => 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StudioEffect {
    Static { colour: String },
    /// No colours = random colour each breath; one or two colours alternate.
    Breathing {
        #[serde(default)]
        colours: Vec<String>,
        #[serde(default)]
        duration: Duration,
    },
    Spectrum {
        #[serde(default)]
        duration: Duration,
    },
    /// No colours = rainbow; otherwise a gradient through the given colours.
    Wave {
        #[serde(default)]
        direction: Direction,
        #[serde(default)]
        duration: Duration,
        #[serde(default)]
        colours: Vec<String>,
    },
    Wheel {
        #[serde(default = "clockwise")]
        direction: Direction,
        #[serde(default)]
        duration: Duration,
    },
    /// No colours = random colours. `density` = stars started per LED per second.
    Starlight {
        #[serde(default)]
        colours: Vec<String>,
        #[serde(default)]
        duration: Duration,
        #[serde(default = "star_density")]
        density: f32,
    },
    Fire {
        #[serde(default)]
        duration: Duration,
    },
    /// Mirror the screen: each LED takes the colour of the matching part of the screen.
    Ambient {
        /// Saturation boost (1.0 = as on screen).
        #[serde(default = "ambient_boost")]
        boost: f32,
        /// Part of the screen to use: [left, top, right, bottom] as fractions (default: all).
        #[serde(default = "whole_screen")]
        area: [f32; 4],
    },
    /// Level meter driven by the PC's audio output: frequency bands left→right, height = level.
    AudioMeter {
        /// Colour per LED by height (green → yellow → red) or one colour.
        #[serde(default)]
        colour: Option<String>,
        /// Sensitivity: multiplies the levels (1.0 = automatic gain only).
        #[serde(default = "one_f")]
        gain: f32,
        /// Colours flow across the bars over time instead of a fixed gradient.
        #[serde(default)]
        flow: bool,
        /// Colour intensity (Synapse's "colour boost"): 1.0 = normal.
        #[serde(default = "one_f")]
        boost: f32,
    },
    /// Rings of colour moving out from (or in to) the centre.
    Tidal {
        #[serde(default)]
        inward: bool,
        #[serde(default)]
        duration: Duration,
        /// No colours = rainbow.
        #[serde(default)]
        colours: Vec<String>,
    },
    /// Battery charge as colour (green → yellow → red); breathes while charging.
    Battery,
}

fn one_f() -> f32 {
    1.0
}

/// Audio levels fed to AudioMeter layers (filled by tsubad's capture thread).
#[derive(Debug, Clone, Default)]
pub struct AudioLevels {
    /// 0..1 per frequency band, low → high.
    pub bands: Vec<f32>,
}

impl Studio {
    pub fn uses_battery(&self) -> bool {
        self.layers.iter().any(|l| l.enabled && matches!(l.effect, StudioEffect::Battery))
    }

    pub fn uses_screen(&self) -> bool {
        self.layers.iter().any(|l| l.enabled && matches!(l.effect, StudioEffect::Ambient { .. }))
    }

    pub fn uses_audio(&self) -> bool {
        self.layers.iter().any(|l| l.enabled && matches!(l.effect, StudioEffect::AudioMeter { .. }))
    }
}

fn whole_screen() -> [f32; 4] {
    [0.0, 0.0, 1.0, 1.0]
}

fn ambient_boost() -> f32 {
    1.3
}

/// Coarse screen colours for Ambient layers (filled by tsubad's screen capture).
#[derive(Debug, Clone, Default)]
pub struct AmbientFrame {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

fn clockwise() -> Direction {
    Direction::Clockwise
}
fn star_density() -> f32 {
    0.15
}

/// One LED: grid `(row, col)` and its position `(x, y)` in any units.
pub type LedPoint = ((u8, u8), (f32, f32));

/// Where each LED sits (normalised 0..1, x right, y down) on a rows×cols grid.
#[derive(Debug, Clone)]
pub struct Geometry {
    pub rows: u8,
    pub cols: u8,
    pub pos: HashMap<(u8, u8), (f32, f32)>,
}

impl Geometry {
    /// Every grid cell, positioned by its row/column.
    pub fn grid(rows: u8, cols: u8) -> Self {
        let mut pos = HashMap::new();
        for r in 0..rows {
            for c in 0..cols {
                pos.insert((r, c), ((c as f32 + 0.5) / cols as f32, (r as f32 + 0.5) / rows as f32));
            }
        }
        Self { rows, cols, pos }
    }

    /// Real LED positions (e.g. hotspot centres on the device render), normalised to 0..1.
    /// Basilisk V3 Pro custom-frame row (1×13): 0 logo, 1 scroll wheel, 2–12 underglow from
    /// front-left, down the left side, across the back and up the right side (top view).
    pub fn basilisk_v3_pro() -> Self {
        let mut pos = HashMap::new();
        pos.insert((0, 0), (0.5, 0.72));
        pos.insert((0, 1), (0.5, 0.18));
        for i in 0..11u8 {
            let t = i as f32 / 10.0;
            let a = std::f32::consts::PI - 0.5 + t * (std::f32::consts::PI + 1.0);
            pos.insert((0, 2 + i), (0.5 + 0.46 * a.cos(), 0.55 - 0.42 * a.sin()));
        }
        Self { rows: 1, cols: 13, pos }
    }

    pub fn from_points(rows: u8, cols: u8, points: &[LedPoint]) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (_, (x, y)) in points {
            x0 = x0.min(*x);
            y0 = y0.min(*y);
            x1 = x1.max(*x);
            y1 = y1.max(*y);
        }
        let (w, h) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
        let pos = points.iter().map(|(rc, (x, y))| (*rc, ((x - x0) / w, (y - y0) / h))).collect();
        Self { rows, cols, pos }
    }
}

impl Geometry {
    /// LED positions from a local device-image pack (`device.json` hotspots with `led`
    /// and `rect`), or None when there is no pack.
    pub fn from_pack(path: &std::path::Path, rows: u8, cols: u8) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let spots = v["views"]["top"]["hotspots"].as_array()?;
        let points: Vec<_> = spots
            .iter()
            .filter_map(|h| {
                let led = h["led"].as_array()?;
                let rect = h["rect"].as_array()?;
                let f = |i: usize| rect.get(i).and_then(|x| x.as_f64()).map(|x| x as f32);
                let (r, c) = (led.first()?.as_u64()? as u8, led.get(1)?.as_u64()? as u8);
                Some(((r, c), (f(0)? + f(2)? / 2.0, f(1)? + f(3)? / 2.0)))
            })
            .filter(|((r, c), _)| *r < rows && *c < cols)
            .collect();
        (!points.is_empty()).then(|| Self::from_points(rows, cols, &points))
    }
}

/// The `device.json` of the picture pack shown for `pid` (personal pack, else Tsuba's line art).
pub fn pack_path(pid: &str) -> std::path::PathBuf {
    crate::devices::pack_dir(pid).unwrap_or_default().join("device.json")
}

/// Small deterministic PRNG (xorshift32) so frames are reproducible in tests.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

fn scale(c: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    [(c[0] as f32 * k) as u8, (c[1] as f32 * k) as u8, (c[2] as f32 * k) as u8]
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

fn colours(list: &[String]) -> Vec<Rgb> {
    list.iter().filter_map(|h| parse_rgb(h).ok()).collect()
}

/// A running star on one LED.
#[derive(Clone, Copy)]
struct Star {
    start: f32,
    colour: Rgb,
}

/// Renders a [`Studio`] into frames. Keeps per-LED state (stars) between frames.
pub struct Renderer {
    studio: Studio,
    geo: Geometry,
    rng: Rng,
    stars: Vec<HashMap<(u8, u8), Star>>,
    breath_colour: Vec<(i64, Rgb)>,
    last_t: f32,
    /// Live audio levels for AudioMeter layers.
    pub audio: Option<std::sync::Arc<std::sync::Mutex<AudioLevels>>>,
    /// Live screen colours for Ambient layers.
    pub screen: Option<std::sync::Arc<std::sync::Mutex<AmbientFrame>>>,
    /// Battery (percent, charging) for Battery layers; refreshed by the runner.
    pub battery: Option<(u8, bool)>,
}

impl Renderer {
    pub fn new(studio: Studio, geo: Geometry) -> Self {
        let n = studio.layers.len();
        Self { studio, geo, rng: Rng(0x9e37_79b9), stars: vec![HashMap::new(); n], breath_colour: vec![(-1, [0; 3]); n], last_t: 0.0, audio: None, screen: None, battery: None }
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geo
    }

    /// Render the frame at time `t` seconds: `rows × cols` colours.
    pub fn frame(&mut self, t: f32) -> Vec<Vec<Rgb>> {
        let dt = (t - self.last_t).clamp(0.0, 0.5);
        self.last_t = t;
        let mut out = vec![vec![[0u8; 3]; self.geo.cols as usize]; self.geo.rows as usize];
        let all: Vec<(u8, u8)> = {
            let mut v: Vec<_> = self.geo.pos.keys().copied().collect();
            v.sort();
            v
        };
        for li in 0..self.studio.layers.len() {
            let layer = self.studio.layers[li].clone();
            if !layer.enabled {
                continue;
            }
            let leds: Vec<(u8, u8)> = if layer.leds.is_empty() {
                all.clone()
            } else {
                layer.leds.iter().map(|[r, c]| (*r, *c)).filter(|rc| self.geo.pos.contains_key(rc)).collect()
            };
            for rc in leds {
                if let Some(c) = self.pixel(li, &layer.effect, rc, t, dt).map(|c| {
                    let k = layer.brightness.clamp(0.0, 1.0);
                    c.map(|v| (v as f32 * k).round() as u8)
                }) {
                    let o = &mut out[rc.0 as usize][rc.1 as usize];
                    *o = match layer.blend {
                        Blend::Cover => c,
                        Blend::Add => [o[0].saturating_add(c[0]), o[1].saturating_add(c[1]), o[2].saturating_add(c[2])],
                        Blend::Lighten => [o[0].max(c[0]), o[1].max(c[1]), o[2].max(c[2])],
                    };
                }
            }
        }
        out
    }

    fn pixel(&mut self, li: usize, e: &StudioEffect, rc: (u8, u8), t: f32, dt: f32) -> Option<Rgb> {
        let (x, y) = self.geo.pos[&rc];
        Some(match e {
            StudioEffect::Static { colour } => parse_rgb(colour).unwrap_or([0; 3]),
            StudioEffect::Breathing { colours: list, duration } => {
                let period = 4.0 * duration.scale();
                let cycle = (t / period).floor() as i64;
                let phase = (t / period).fract();
                let k = (1.0 - (TAU * phase).cos()) / 2.0;
                let cs = colours(list);
                let colour = if cs.is_empty() {
                    if self.breath_colour[li].0 != cycle {
                        let c = hsv(self.rng.unit(), 1.0, 1.0);
                        self.breath_colour[li] = (cycle, c);
                    }
                    self.breath_colour[li].1
                } else {
                    cs[cycle.rem_euclid(cs.len() as i64) as usize]
                };
                scale(colour, k)
            }
            StudioEffect::Spectrum { duration } => hsv(t / (8.0 * duration.scale()), 1.0, 1.0),
            StudioEffect::Wave { direction, duration, colours: list } => {
                let along = match direction {
                    Direction::RightToLeft => x,
                    Direction::TopToBottom => 1.0 - y,
                    Direction::BottomToTop => y,
                    _ => 1.0 - x,
                };
                let phase = along + t / (3.0 * duration.scale());
                let cs = colours(list);
                match cs.len() {
                    0 => hsv(phase, 1.0, 1.0),
                    1 => scale(cs[0], 0.25 + 0.75 * (1.0 - (TAU * phase).cos()) / 2.0),
                    n => {
                        let p = phase.rem_euclid(1.0) * n as f32;
                        let i = p.floor() as usize % n;
                        lerp(cs[i], cs[(i + 1) % n], p.fract())
                    }
                }
            }
            StudioEffect::Wheel { direction, duration } => {
                let angle = (y - 0.5).atan2(x - 0.5) / TAU;
                let sign = if *direction == Direction::CounterClockwise { 1.0 } else { -1.0 };
                hsv(angle + sign * t / (4.0 * duration.scale()), 1.0, 1.0)
            }
            StudioEffect::Starlight { colours: list, duration, density } => {
                let life = 1.2 * duration.scale();
                let cs = colours(list);
                let stars = &mut self.stars[li];
                if let Some(s) = stars.get(&rc) {
                    if t - s.start > life {
                        stars.remove(&rc);
                    }
                }
                if !stars.contains_key(&rc) && self.rng.unit() < density * dt {
                    let colour = if cs.is_empty() { hsv(self.rng.unit(), 1.0, 1.0) } else { cs[self.rng.next() as usize % cs.len()] };
                    stars.insert(rc, Star { start: t, colour });
                }
                let s = stars.get(&rc)?;
                let p = ((t - s.start) / life).clamp(0.0, 1.0);
                scale(s.colour, if p < 0.3 { p / 0.3 } else { 1.0 - (p - 0.3) / 0.7 })
            }
            StudioEffect::Ambient { boost, area } => {
                let f = self.screen.as_ref().and_then(|s| s.lock().ok().map(|f| f.clone()))?;
                if f.px.is_empty() {
                    return None;
                }
                let [l, t, r, btm] = *area;
                let (sx, sy) = (l + x * (r - l).max(0.01), t + y * (btm - t).max(0.01));
                let gx = ((sx * f.w as f32) as usize).min(f.w - 1);
                let gy = ((sy * f.h as f32) as usize).min(f.h - 1);
                let [r, g, b] = f.px[gy * f.w + gx];
                // Lift dark scenes (gamma 0.6: dark UIs still glow, bright colours stay put),
                // then boost saturation around the grey level, like Synapse's vivid ambient.
                let lift = |c: u8| 255.0 * (c as f32 / 255.0).powf(0.6);
                let (r, g, b) = (lift(r), lift(g), lift(b));
                let grey = (r + g + b) / 3.0;
                let sat = |c: f32| (grey + (c - grey) * boost).clamp(0.0, 255.0) as u8;
                [sat(r), sat(g), sat(b)]
            }
            StudioEffect::AudioMeter { colour, gain, flow, boost } => {
                let bands = self.audio.as_ref().map(|a| a.lock().map(|a| a.bands.clone()).unwrap_or_default()).unwrap_or_default();
                if bands.is_empty() {
                    return None;
                }
                let i = ((x * bands.len() as f32) as usize).min(bands.len() - 1);
                let level = (bands[i] * gain).min(1.0);
                let height = 1.0 - y; // 0 at the bottom edge, 1 at the top
                // Unlit cells are transparent, so the meter can sit over another layer.
                if height > level + 0.04 {
                    return None;
                }
                let c = match colour.as_deref().and_then(|c| crate::parse_rgb(c).ok()) {
                    Some(c) => c,
                    None if *flow => hsv((x * 0.8 + t * 0.15).fract(), 1.0, 1.0),
                    None if height < 0.5 => lerp([0, 220, 40], [255, 220, 0], height / 0.5),
                    None => lerp([255, 220, 0], [255, 20, 0], (height - 0.5) / 0.5),
                };
                let k = boost.clamp(0.2, 2.0);
                c.map(|v| (v as f32 * k).min(255.0) as u8)
            }
            StudioEffect::Tidal { inward, duration, colours } => {
                let d = ((x - 0.5).powi(2) + ((y - 0.5) * 0.6).powi(2)).sqrt();
                let speed = 0.35 / duration.scale();
                let phase = if *inward { d * 2.2 + t * speed } else { d * 2.2 - t * speed };
                let cs: Vec<Rgb> = colours.iter().filter_map(|c| crate::parse_rgb(c).ok()).collect();
                if cs.is_empty() {
                    hsv(phase.rem_euclid(1.0), 1.0, 1.0)
                } else {
                    let f = phase.rem_euclid(1.0) * cs.len() as f32;
                    let (a, bb) = (cs[f as usize % cs.len()], cs[(f as usize + 1) % cs.len()]);
                    lerp(a, bb, f.fract())
                }
            }
            StudioEffect::Battery => {
                let (pct, charging) = self.battery?;
                let level = pct as f32 / 100.0;
                let c = if level > 0.5 { lerp([255, 220, 0], [0, 230, 40], (level - 0.5) / 0.5) } else { lerp([255, 20, 0], [255, 220, 0], level / 0.5) };
                if charging {
                    let k = 0.35 + 0.65 * (0.5 + 0.5 * (t * 2.0).sin());
                    c.map(|v| (v as f32 * k) as u8)
                } else {
                    c
                }
            }
            StudioEffect::Fire { duration } => {
                // Heat builds from the bottom edge (y = 1); noise scrolls upward so flames rise.
                let speed = 1.0 / duration.scale();
                let rise = t * 1.8 * speed;
                let n = 0.65 * value_noise(x * 7.0, y * 5.0 + rise) + 0.35 * value_noise(x * 15.0, y * 11.0 + rise * 1.7);
                let heat = (y.powf(1.3) * 1.15 - 0.30 + (n - 0.5) * 0.95).clamp(0.0, 1.0);
                fire_colour(heat)
            }
        })
    }
}

/// Smooth 2-D value noise in 0..1.
fn value_noise(x: f32, y: f32) -> f32 {
    fn hash(i: i32, j: i32) -> f32 {
        let mut h = (i.wrapping_mul(374_761_393) ^ j.wrapping_mul(668_265_263)) as u32;
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
    }
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
    let bottom = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
    top + (bottom - top) * sy
}

/// Black-body style flame palette: dark red → orange → yellow.
fn fire_colour(heat: f32) -> Rgb {
    const STOPS: [(f32, Rgb); 4] = [(0.0, [12, 0, 0]), (0.4, [170, 18, 0]), (0.75, [255, 110, 0]), (1.0, [255, 215, 70])];
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if heat <= b {
            return lerp(ca, cb, (heat - a) / (b - a));
        }
    }
    STOPS[3].1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_is_hotter_at_the_bottom() {
        let avg = |y: f32| (0..40).map(|i| fire_colour((y.powf(1.3) * 1.15 - 0.30 + (value_noise(i as f32 * 0.37, y * 5.0) - 0.5) * 0.95).clamp(0.0, 1.0))[1] as u32).sum::<u32>();
        assert!(avg(0.95) > avg(0.2), "bottom rows should be brighter/yellower than the top");
    }

    fn studio(layers: Vec<Layer>) -> Studio {
        Studio { layers }
    }
    fn layer(effect: StudioEffect, leds: Vec<[u8; 2]>) -> Layer {
        Layer { name: String::new(), enabled: true, leds, effect, brightness: 1.0, blend: Blend::Cover }
    }

    #[test]
    fn add_blend_mixes_with_the_layer_below() {
        let mut top = layer(StudioEffect::Static { colour: "000080".into() }, vec![]);
        top.blend = Blend::Add;
        let f = Renderer::new(studio(vec![layer(StudioEffect::Static { colour: "400000".into() }, vec![]), top]), Geometry::grid(1, 1)).frame(0.0);
        assert_eq!(f[0][0], [0x40, 0, 0x80]);
    }

    #[test]
    fn layer_brightness_survives_json() {
        let st: Studio = serde_json::from_str(r#"{"layers":[{"name":"x","enabled":true,"leds":[],"effect":{"type":"static","colour":"ff0000"},"brightness":0.25}]}"#).unwrap();
        let f = Renderer::new(st, Geometry::grid(1, 1)).frame(0.0);
        assert_eq!(f[0][0], [64, 0, 0]);
    }

    #[test]
    fn layer_brightness_scales_its_colours() {
        let mut l = layer(StudioEffect::Static { colour: "ff8000".into() }, vec![]);
        l.brightness = 0.5;
        let f = Renderer::new(studio(vec![l]), Geometry::grid(1, 1)).frame(0.0);
        assert_eq!(f[0][0], [128, 64, 0]);
    }

    #[test]
    fn upper_layer_wins_only_on_its_leds() {
        let s = studio(vec![
            layer(StudioEffect::Static { colour: "0000ff".into() }, vec![]),
            layer(StudioEffect::Static { colour: "ff0000".into() }, vec![[0, 1]]),
        ]);
        let f = Renderer::new(s, Geometry::grid(2, 3)).frame(0.0);
        assert_eq!(f[0][0], [0, 0, 255]);
        assert_eq!(f[0][1], [255, 0, 0]);
        assert_eq!(f[1][2], [0, 0, 255]);
    }

    #[test]
    fn disabled_layers_and_unknown_leds_are_ignored() {
        let mut l = layer(StudioEffect::Static { colour: "ffffff".into() }, vec![]);
        l.enabled = false;
        let s = studio(vec![l, layer(StudioEffect::Static { colour: "00ff00".into() }, vec![[9, 9]])]);
        let f = Renderer::new(s, Geometry::grid(1, 2)).frame(0.0);
        assert_eq!(f, vec![vec![[0, 0, 0]; 2]]);
    }

    #[test]
    fn breathing_goes_dark_and_bright() {
        let s = studio(vec![layer(StudioEffect::Breathing { colours: vec!["ffffff".into()], duration: Duration::Medium }, vec![])]);
        let mut r = Renderer::new(s, Geometry::grid(1, 1));
        assert_eq!(r.frame(0.0)[0][0], [0, 0, 0]);
        assert_eq!(r.frame(2.0)[0][0], [255, 255, 255]);
    }

    #[test]
    fn wave_moves_across_leds() {
        let s = studio(vec![layer(StudioEffect::Wave { direction: Direction::LeftToRight, duration: Duration::Medium, colours: vec![] }, vec![])]);
        let mut r = Renderer::new(s, Geometry::grid(1, 4));
        let a = r.frame(0.0);
        let b = r.frame(0.75);
        assert_ne!(a[0][0], a[0][3], "colours differ across the row");
        assert_eq!(b[0][1], a[0][0], "the pattern shifted by one LED after a quarter period");
    }

    #[test]
    fn starlight_is_transparent_between_stars() {
        let s = studio(vec![
            layer(StudioEffect::Static { colour: "112233".into() }, vec![]),
            layer(StudioEffect::Starlight { colours: vec![], duration: Duration::Medium, density: 0.0 }, vec![]),
        ]);
        assert_eq!(Renderer::new(s, Geometry::grid(1, 3)).frame(1.0)[0][1], [0x11, 0x22, 0x33]);
    }

    #[test]
    fn json_roundtrip() {
        let s = studio(vec![layer(StudioEffect::Wave { direction: Direction::RightToLeft, duration: Duration::Short, colours: vec!["ff0000".into()] }, vec![[0, 2]])]);
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Studio>(&j).unwrap(), s);
        let minimal: Studio = serde_json::from_str(r#"{"layers":[{"effect":{"type":"spectrum"}}]}"#).unwrap();
        assert!(minimal.layers[0].enabled && minimal.layers[0].leds.is_empty());
    }
}

#[cfg(test)]
mod database_pack_tests {
    use super::*;

    #[test]
    fn every_database_device_has_a_drawing_pack() {
        for s in tsuba_hid::db::all() {
            let pid = crate::pid_key(s.pid);
            let dir = crate::devices::builtin_dir(&pid).unwrap_or_else(|| panic!("{pid}: no pack"));
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("device.json")).unwrap()).unwrap();
            let image = v["views"]["top"]["image"].as_str().unwrap();
            assert!(dir.join(image).is_file(), "{pid}: {image} missing (broken layout link?)");
            if !s.tested {
                assert_eq!(v["approximate"], true, "{pid}");
            }
        }
    }

    #[test]
    fn generic_keyboard_pack_gives_studio_geometry() {
        // BlackWidow Chroma: 6×22 per-key grid; its generic pack maps keys to that grid.
        let dir = crate::devices::builtin_dir("0203").unwrap();
        let g = Geometry::from_pack(&dir.join("device.json"), 6, 22).expect("geometry");
        assert!(g.pos.len() > 100, "{} LEDs placed", g.pos.len());
        assert!(g.pos.contains_key(&(0, 1)), "Esc at row 0, column 1");
    }
}
