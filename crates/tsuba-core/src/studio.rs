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
    /// Layer opacity 0..1: how much of the layer shows over what is below it.
    #[serde(default = "full")]
    pub opacity: f32,
    /// Animation speed multiplier (1 = normal) applied on top of the effect's own duration.
    #[serde(default = "full")]
    pub speed: f32,
    /// Set on layers that span devices: where this device sits in the shared canvas space.
    /// None = the effect is laid out over this device alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Where a device sits in a shared canvas (all fractions of the canvas bounding box): an LED at
/// device-local `(x, y)` (0..1) is at `origin + (x, y) * scale` on the canvas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Span {
    pub origin: [f32; 2],
    pub scale: [f32; 2],
    /// Canvas width / height, so angles (Wheel) stay circular.
    pub aspect: f32,
}

impl Layer {
    /// A fully opaque, normal-speed layer with the effect on every LED.
    pub fn new(name: &str, effect: StudioEffect) -> Self {
        Self { name: name.into(), enabled: true, leds: vec![], effect, brightness: 1.0, blend: Blend::Cover, opacity: 1.0, speed: 1.0, span: None }
    }
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
    /// Colours rotating around a centre point. No colours = rainbow.
    Wheel {
        #[serde(default = "clockwise")]
        direction: Direction,
        #[serde(default)]
        duration: Duration,
        /// Rotation centre as a fraction of the device (0..1, x right, y down).
        #[serde(default = "half_half")]
        centre: [f32; 2],
        /// How many times the colours repeat round the circle.
        #[serde(default = "one_f")]
        spread: f32,
        #[serde(default)]
        colours: Vec<String>,
    },
    /// A linear gradient through the colours at an angle (degrees, 0 = left to right,
    /// 90 = top to bottom); optionally scrolling.
    Gradient {
        #[serde(default)]
        colours: Vec<String>,
        #[serde(default)]
        angle: f32,
        #[serde(default)]
        scroll: bool,
        #[serde(default)]
        duration: Duration,
    },
    /// A picture sampled onto the LEDs (a downscaled copy lives in the profile), static or
    /// scrolling.
    Image {
        /// Pixel size of `data`.
        #[serde(default)]
        w: u16,
        #[serde(default)]
        h: u16,
        /// `w * h` pixels as RRGGBB hex, row by row.
        #[serde(default)]
        data: String,
        #[serde(default)]
        fit: ImageFit,
        #[serde(default)]
        scroll: ImageScroll,
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
        /// Capture source (a PipeWire/Pulse source name); None = what the PC is playing.
        #[serde(default)]
        source: Option<String>,
        /// Spectrum bands, or one overall level.
        #[serde(default)]
        mode: MeterMode,
        /// Keep a marker at each recent peak.
        #[serde(default)]
        peak_hold: bool,
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

fn half_half() -> [f32; 2] {
    [0.5, 0.5]
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeterMode {
    /// Frequency bands left → right, each as a bar.
    #[default]
    Spectrum,
    /// One loudness bar filling left → right.
    Level,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Stretch to the device.
    #[default]
    Stretch,
    /// Keep the picture's proportions and fill the device (crops).
    Cover,
    /// Keep the proportions and fit inside (empty margins stay transparent).
    Contain,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageScroll {
    #[default]
    None,
    Left,
    Right,
    Up,
    Down,
}

/// Audio levels fed to AudioMeter layers (filled by tsubad's capture thread).
#[derive(Debug, Clone, Default)]
pub struct AudioLevels {
    /// 0..1 per frequency band, low → high.
    pub bands: Vec<f32>,
    /// 0..1 overall loudness.
    pub level: f32,
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

    /// The capture sources the enabled Audio Meter layers want ("" = what the PC is playing).
    pub fn audio_sources(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .layers
            .iter()
            .filter(|l| l.enabled)
            .filter_map(|l| match &l.effect {
                StudioEffect::AudioMeter { source, .. } => Some(source.clone().unwrap_or_default()),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Clamp everything to what the renderer and the UI can take (used on imported files):
    /// at most [`MAX_LAYERS`] layers, numbers in range, valid colours, well-formed pictures.
    pub fn sanitise(&mut self) {
        self.layers.truncate(MAX_LAYERS);
        for l in &mut self.layers {
            l.sanitise();
        }
    }
}

pub const MAX_LAYERS: usize = 64;
/// Largest picture kept in a profile (pixels per side).
pub const MAX_IMAGE_SIDE: u16 = 64;

fn clean_colours(list: &mut Vec<String>) {
    list.retain(|c| parse_rgb(c).is_ok());
    list.truncate(8);
}

impl Layer {
    pub fn sanitise(&mut self) {
        let num = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.name = self.name.chars().filter(|c| !c.is_control()).take(48).collect();
        self.brightness = num(self.brightness, 0.0, 1.0, 1.0);
        self.opacity = num(self.opacity, 0.0, 1.0, 1.0);
        self.speed = num(self.speed, 0.1, 5.0, 1.0);
        self.leds.truncate(4096);
        if let Some(sp) = &mut self.span {
            sp.origin = sp.origin.map(|v| num(v, 0.0, 1.0, 0.0));
            sp.scale = sp.scale.map(|v| num(v, 0.01, 1.0, 1.0));
            sp.aspect = num(sp.aspect, 0.1, 10.0, 1.0);
        }
        match &mut self.effect {
            StudioEffect::Static { colour } => {
                if parse_rgb(colour).is_err() {
                    *colour = "ffffff".into();
                }
            }
            StudioEffect::Breathing { colours, .. } | StudioEffect::Wave { colours, .. } | StudioEffect::Starlight { colours, .. } | StudioEffect::Tidal { colours, .. } | StudioEffect::Gradient { colours, .. } => clean_colours(colours),
            StudioEffect::Wheel { colours, centre, spread, .. } => {
                clean_colours(colours);
                *centre = centre.map(|v| num(v, 0.0, 1.0, 0.5));
                *spread = num(*spread, 0.25, 4.0, 1.0);
            }
            StudioEffect::AudioMeter { colour, source, gain, boost, .. } => {
                if colour.as_deref().is_some_and(|c| parse_rgb(c).is_err()) {
                    *colour = None;
                }
                if let Some(s) = source {
                    *s = s.chars().filter(|c| !c.is_control()).take(200).collect();
                    if s.is_empty() {
                        *source = None;
                    }
                }
                *gain = num(*gain, 0.1, 8.0, 1.0);
                *boost = num(*boost, 0.1, 4.0, 1.0);
            }
            StudioEffect::Ambient { boost, area } => {
                *boost = num(*boost, 0.1, 4.0, 1.3);
                *area = area.map(|v| num(v, 0.0, 1.0, 0.0));
            }
            StudioEffect::Image { w, h, data, .. } => {
                if *w == 0 || *h == 0 || *w > MAX_IMAGE_SIDE || *h > MAX_IMAGE_SIDE || data.len() != *w as usize * *h as usize * 6 || !data.bytes().all(|b| b.is_ascii_hexdigit()) {
                    *w = 0;
                    *h = 0;
                    data.clear();
                }
            }
            _ => {}
        }
        if let StudioEffect::Starlight { density, .. } = &mut self.effect {
            *density = num(*density, 0.0, 2.0, 0.15);
        }
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
    /// Width / height of the area the normalised positions were stretched from.
    pub aspect: f32,
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
        Self { rows, cols, pos, aspect: cols as f32 / rows as f32 }
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
        Self { rows: 1, cols: 13, pos, aspect: 0.7 }
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
        Self { rows, cols, pos, aspect: (w / h).clamp(0.1, 10.0) }
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

/// A picture decoded from an Image layer.
struct Img {
    w: usize,
    h: usize,
    px: Vec<Rgb>,
}

impl Img {
    fn decode(w: u16, h: u16, data: &str) -> Option<Self> {
        let (w, h) = (w as usize, h as usize);
        if w == 0 || h == 0 || w > MAX_IMAGE_SIDE as usize || h > MAX_IMAGE_SIDE as usize || data.len() != w * h * 6 || !data.is_ascii() {
            return None;
        }
        let px = (0..w * h).map(|i| parse_rgb(&data[i * 6..i * 6 + 6]).ok()).collect::<Option<Vec<_>>>()?;
        Some(Self { w, h, px })
    }

    /// Bilinear sample at `(u, v)` in 0..1 (clamped to the edges).
    fn sample(&self, u: f32, v: f32) -> Rgb {
        let fx = (u.clamp(0.0, 1.0) * self.w as f32 - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = (v.clamp(0.0, 1.0) * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let top = lerp(self.px[y0 * self.w + x0], self.px[y0 * self.w + x1], tx);
        let bottom = lerp(self.px[y1 * self.w + x0], self.px[y1 * self.w + x1], tx);
        lerp(top, bottom, ty)
    }
}

/// Colour at `phase` going round the list and back to its first colour (so it tiles).
fn cycle_colour(cs: &[Rgb], phase: f32) -> Rgb {
    let n = cs.len();
    if n == 1 {
        return cs[0];
    }
    let p = phase.rem_euclid(1.0) * n as f32;
    let i = p.floor() as usize % n;
    lerp(cs[i], cs[(i + 1) % n], p.fract())
}

/// Colour at `t` (0..1) along the list, first colour at 0 and last at 1.
fn ramp_colour(cs: &[Rgb], t: f32) -> Rgb {
    let n = cs.len();
    if n == 1 {
        return cs[0];
    }
    let p = t.clamp(0.0, 1.0) * (n - 1) as f32;
    let i = (p.floor() as usize).min(n - 2);
    lerp(cs[i], cs[i + 1], p - i as f32)
}

/// Renders a [`Studio`] into frames. Keeps per-LED state (stars) between frames.
pub struct Renderer {
    studio: Studio,
    geo: Geometry,
    rng: Rng,
    stars: Vec<HashMap<(u8, u8), Star>>,
    breath_colour: Vec<(i64, Rgb)>,
    images: Vec<Option<Img>>,
    /// Held peaks of Audio Meter layers with peak hold (one per band, or one for Level mode).
    peaks: Vec<Vec<f32>>,
    last_t: f32,
    /// Live audio levels for AudioMeter layers, by capture source ("" = what the PC plays).
    pub audio: HashMap<String, std::sync::Arc<std::sync::Mutex<AudioLevels>>>,
    /// Live screen colours for Ambient layers.
    pub screen: Option<std::sync::Arc<std::sync::Mutex<AmbientFrame>>>,
    /// Battery (percent, charging) for Battery layers; refreshed by the runner.
    pub battery: Option<(u8, bool)>,
}

impl Renderer {
    pub fn new(studio: Studio, geo: Geometry) -> Self {
        let n = studio.layers.len();
        let images = studio
            .layers
            .iter()
            .map(|l| match &l.effect {
                StudioEffect::Image { w, h, data, .. } => Img::decode(*w, *h, data),
                _ => None,
            })
            .collect();
        Self {
            studio,
            geo,
            rng: Rng(0x9e37_79b9),
            stars: vec![HashMap::new(); n],
            breath_colour: vec![(-1, [0; 3]); n],
            images,
            peaks: vec![Vec::new(); n],
            last_t: 0.0,
            audio: HashMap::new(),
            screen: None,
            battery: None,
        }
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geo
    }

    fn audio_now(&self, source: &Option<String>) -> Option<AudioLevels> {
        let a = self.audio.get(source.as_deref().unwrap_or(""))?;
        a.lock().ok().map(|a| a.clone())
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
            let speed = layer.speed.clamp(0.1, 5.0);
            let (lt, ldt) = (t * speed, dt * speed);
            let opacity = layer.opacity.clamp(0.0, 1.0);
            self.update_peaks(li, &layer.effect, dt);
            for rc in leds {
                let (lx, ly) = self.geo.pos[&rc];
                // Span layers lay the effect out over the shared canvas instead of this device.
                let (pos, aspect) = match layer.span {
                    Some(sp) => ((sp.origin[0] + lx * sp.scale[0], sp.origin[1] + ly * sp.scale[1]), sp.aspect),
                    None => ((lx, ly), self.geo.aspect),
                };
                if let Some(c) = self.pixel(li, &layer.effect, rc, pos, aspect, lt, ldt).map(|c| {
                    let k = layer.brightness.clamp(0.0, 1.0);
                    c.map(|v| (v as f32 * k).round() as u8)
                }) {
                    let o = &mut out[rc.0 as usize][rc.1 as usize];
                    let mixed = match layer.blend {
                        Blend::Cover => c,
                        Blend::Add => [o[0].saturating_add(c[0]), o[1].saturating_add(c[1]), o[2].saturating_add(c[2])],
                        Blend::Lighten => [o[0].max(c[0]), o[1].max(c[1]), o[2].max(c[2])],
                    };
                    *o = if opacity < 1.0 { lerp(*o, mixed, opacity) } else { mixed };
                }
            }
        }
        out
    }

    /// Peak hold: markers jump up with the level and fall back slowly.
    fn update_peaks(&mut self, li: usize, e: &StudioEffect, dt: f32) {
        let StudioEffect::AudioMeter { source, mode, peak_hold, gain, .. } = e else { return };
        if !*peak_hold {
            return;
        }
        let Some(a) = self.audio_now(source) else { return };
        let now: Vec<f32> = match mode {
            MeterMode::Spectrum => a.bands.iter().map(|b| (b * gain).min(1.0)).collect(),
            MeterMode::Level => vec![(a.level * gain).min(1.0)],
        };
        let held = &mut self.peaks[li];
        held.resize(now.len(), 0.0);
        for (h, n) in held.iter_mut().zip(now) {
            *h = if n > *h { n } else { (*h - dt * 0.5).max(n) };
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pixel(&mut self, li: usize, e: &StudioEffect, rc: (u8, u8), pos: (f32, f32), aspect: f32, t: f32, dt: f32) -> Option<Rgb> {
        let (x, y) = pos;
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
                    _ => cycle_colour(&cs, phase),
                }
            }
            StudioEffect::Wheel { direction, duration, centre, spread, colours: list } => {
                // Angle round the centre in true proportions (x scaled by the aspect ratio).
                let (dx, dy) = ((x - centre[0]) * aspect, y - centre[1]);
                let angle = dy.atan2(dx) / TAU;
                let sign = if *direction == Direction::CounterClockwise { 1.0 } else { -1.0 };
                let phase = angle * spread.clamp(0.25, 4.0) + sign * t / (4.0 * duration.scale());
                let cs = colours(list);
                if cs.is_empty() {
                    hsv(phase, 1.0, 1.0)
                } else {
                    cycle_colour(&cs, phase)
                }
            }
            StudioEffect::Gradient { colours: list, angle, scroll, duration } => {
                let cs = colours(list);
                if cs.is_empty() {
                    return None;
                }
                let a = angle.to_radians();
                let (c, s) = (a.cos(), a.sin());
                // Position along the gradient axis, 0 at one corner of the box, 1 at the other.
                let along = ((x - 0.5) * c + (y - 0.5) * s) / (c.abs() + s.abs()).max(1e-6) + 0.5;
                if *scroll {
                    cycle_colour(&cs, along - t / (6.0 * duration.scale()))
                } else {
                    ramp_colour(&cs, along)
                }
            }
            StudioEffect::Image { fit, scroll, .. } => {
                let img = self.images.get(li)?.as_ref()?;
                let ia = img.w as f32 / img.h as f32;
                let (mut u, mut v) = match fit {
                    ImageFit::Stretch => (x, y),
                    ImageFit::Cover if aspect >= ia => (x, 0.5 + (y - 0.5) * ia / aspect),
                    ImageFit::Cover => (0.5 + (x - 0.5) * aspect / ia, y),
                    ImageFit::Contain if aspect >= ia => (0.5 + (x - 0.5) * aspect / ia, y),
                    ImageFit::Contain => (x, 0.5 + (y - 0.5) * ia / aspect),
                };
                let k = 0.12 * t;
                match scroll {
                    ImageScroll::None => {}
                    ImageScroll::Left => u += k,
                    ImageScroll::Right => u -= k,
                    ImageScroll::Up => v += k,
                    ImageScroll::Down => v -= k,
                }
                if *scroll != ImageScroll::None {
                    u = u.rem_euclid(1.0);
                    v = v.rem_euclid(1.0);
                } else if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                    return None;
                }
                img.sample(u, v)
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
            StudioEffect::AudioMeter { colour, gain, flow, boost, source, mode, peak_hold } => {
                let a = self.audio_now(source)?;
                if a.bands.is_empty() {
                    return None;
                }
                let (level, fill, held) = match mode {
                    MeterMode::Spectrum => {
                        let i = ((x * a.bands.len() as f32) as usize).min(a.bands.len() - 1);
                        let held = self.peaks.get(li).and_then(|p| p.get(i)).copied();
                        ((a.bands[i] * gain).min(1.0), 1.0 - y, held)
                    }
                    MeterMode::Level => ((a.level * gain).min(1.0), x, self.peaks.get(li).and_then(|p| p.first()).copied()),
                };
                // Unlit cells are transparent, so the meter can sit over another layer.
                let cap = *peak_hold && held.is_some_and(|h| h > 0.02 && (fill - h).abs() < 0.07);
                if fill > level + 0.04 && !cap {
                    return None;
                }
                let c = match colour.as_deref().and_then(|c| crate::parse_rgb(c).ok()) {
                    Some(c) => c,
                    None if cap => [255, 255, 255],
                    None if *flow => hsv((x * 0.8 + t * 0.15).fract(), 1.0, 1.0),
                    None if fill < 0.5 => lerp([0, 220, 40], [255, 220, 0], fill / 0.5),
                    None => lerp([255, 220, 0], [255, 20, 0], (fill - 0.5) / 0.5),
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
        Layer { leds, ..Layer::new("", effect) }
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
    fn opacity_blends_with_the_layer_below() {
        let mut top = layer(StudioEffect::Static { colour: "ffffff".into() }, vec![]);
        top.opacity = 0.5;
        let f = Renderer::new(studio(vec![layer(StudioEffect::Static { colour: "000000".into() }, vec![]), top]), Geometry::grid(1, 1)).frame(0.0);
        assert_eq!(f[0][0], [127, 127, 127]);
    }

    #[test]
    fn speed_scales_the_layer_clock() {
        let wave = StudioEffect::Wave { direction: Direction::LeftToRight, duration: Duration::Medium, colours: vec![] };
        let mut fast = layer(wave.clone(), vec![]);
        fast.speed = 2.0;
        let a = Renderer::new(studio(vec![fast]), Geometry::grid(1, 4)).frame(0.5);
        let b = Renderer::new(studio(vec![layer(wave, vec![])]), Geometry::grid(1, 4)).frame(1.0);
        assert_eq!(a, b, "twice the speed at half the time looks the same");
    }

    #[test]
    fn span_lays_the_effect_over_the_canvas() {
        // Two one-LED "devices" side by side: the left half and the right half of one gradient.
        let g = StudioEffect::Gradient { colours: vec!["000000".into(), "ffffff".into()], angle: 0.0, scroll: false, duration: Duration::Medium };
        let (mut left, mut right) = (layer(g.clone(), vec![]), layer(g, vec![]));
        left.span = Some(Span { origin: [0.0, 0.0], scale: [0.5, 1.0], aspect: 2.0 });
        right.span = Some(Span { origin: [0.5, 0.0], scale: [0.5, 1.0], aspect: 2.0 });
        let l = Renderer::new(studio(vec![left]), Geometry::grid(1, 1)).frame(0.0)[0][0];
        let r = Renderer::new(studio(vec![right]), Geometry::grid(1, 1)).frame(0.0)[0][0];
        assert!(l[0] < r[0], "{l:?} {r:?}");
        assert!(l[0] > 40 && l[0] < 90 && r[0] > 165 && r[0] < 215, "{l:?} {r:?}");
    }

    #[test]
    fn gradient_angle_runs_top_to_bottom() {
        let g = StudioEffect::Gradient { colours: vec!["ff0000".into(), "0000ff".into()], angle: 90.0, scroll: false, duration: Duration::Medium };
        let f = Renderer::new(studio(vec![layer(g, vec![])]), Geometry::grid(2, 2)).frame(0.0);
        assert!(f[0][0][0] > f[1][0][0] && f[0][0][2] < f[1][0][2]);
        assert_eq!(f[0][0], f[0][1]);
    }

    #[test]
    fn image_layer_samples_pixels_and_rejects_bad_data() {
        // 2×1 picture: red | blue stretched over four columns.
        let img = StudioEffect::Image { w: 2, h: 1, data: "ff00000000ff".into(), fit: ImageFit::Stretch, scroll: ImageScroll::None };
        let f = Renderer::new(studio(vec![layer(img, vec![])]), Geometry::grid(1, 4)).frame(0.0);
        assert_eq!(f[0][0], [255, 0, 0]);
        assert_eq!(f[0][3], [0, 0, 255]);
        let bad = StudioEffect::Image { w: 2, h: 2, data: "ff0000".into(), fit: ImageFit::Stretch, scroll: ImageScroll::None };
        let f = Renderer::new(studio(vec![layer(bad, vec![])]), Geometry::grid(1, 2)).frame(0.0);
        assert_eq!(f[0][0], [0, 0, 0], "an undecodable picture draws nothing");
    }

    #[test]
    fn image_scroll_moves_the_picture() {
        let img = StudioEffect::Image { w: 4, h: 1, data: "ff000000ff000000ffffff00".into(), fit: ImageFit::Stretch, scroll: ImageScroll::Left };
        let mut r = Renderer::new(studio(vec![layer(img, vec![])]), Geometry::grid(1, 4));
        let a = r.frame(0.0);
        let b = r.frame(2.0);
        assert_ne!(a, b);
    }

    #[test]
    fn wheel_is_circular_on_a_wide_device() {
        // Points one unit left/right of the centre on a 2:1 canvas sit at the same angle ±.
        let mut geo = Geometry::grid(1, 1);
        geo.aspect = 2.0;
        geo.pos.insert((0, 0), (0.75, 0.5));
        let w = |centre| StudioEffect::Wheel { direction: Direction::Clockwise, duration: Duration::Medium, centre, spread: 1.0, colours: vec![] };
        let right = Renderer::new(studio(vec![layer(w([0.5, 0.5]), vec![])]), geo.clone()).frame(0.0)[0][0];
        // Straight right of the centre = angle 0 = hue 0 = red.
        assert_eq!(right, [255, 0, 0]);
        geo.pos.insert((0, 0), (0.5, 0.9));
        let below = Renderer::new(studio(vec![layer(w([0.5, 0.5]), vec![])]), geo).frame(0.0)[0][0];
        // Straight below = a quarter turn clockwise = hue 0.25.
        assert_eq!(below, hsv(0.25, 1.0, 1.0));
    }

    #[test]
    fn wheel_centre_moves_the_hub() {
        let geo = Geometry::grid(1, 3);
        let w = StudioEffect::Wheel { direction: Direction::Clockwise, duration: Duration::Medium, centre: [0.0, 0.5], spread: 1.0, colours: vec![] };
        let f = Renderer::new(studio(vec![layer(w, vec![])]), geo).frame(0.0);
        // Everything is to the right of a hub at the left edge: the same angle (hue 0).
        assert_eq!(f[0][0], f[0][2]);
    }

    #[test]
    fn audio_meter_level_mode_fills_left_to_right_and_holds_peaks() {
        use std::sync::{Arc, Mutex};
        let lv = Arc::new(Mutex::new(AudioLevels { bands: vec![0.5; 4], level: 0.5 }));
        let m = StudioEffect::AudioMeter { colour: Some("00ff00".into()), gain: 1.0, flow: false, boost: 1.0, source: None, mode: MeterMode::Level, peak_hold: true };
        let mut r = Renderer::new(studio(vec![layer(m, vec![])]), Geometry::grid(1, 10));
        r.audio.insert(String::new(), lv.clone());
        let f = r.frame(0.0);
        assert_ne!(f[0][1], [0, 0, 0]);
        assert_eq!(f[0][9], [0, 0, 0]);
        lv.lock().unwrap().level = 0.0;
        let f = r.frame(0.1);
        // The level dropped, the held peak marker stays near the old level.
        assert_ne!(f[0][4], [0, 0, 0], "{f:?}");
        assert_eq!(f[0][0], [0, 0, 0]);
    }

    #[test]
    fn audio_sources_lists_each_capture_once() {
        let m = |s: Option<&str>| StudioEffect::AudioMeter { colour: None, gain: 1.0, flow: false, boost: 1.0, source: s.map(String::from), mode: MeterMode::Spectrum, peak_hold: false };
        let st = studio(vec![layer(m(None), vec![]), layer(m(Some("mic")), vec![]), layer(m(Some("mic")), vec![]), layer(StudioEffect::Battery, vec![])]);
        assert_eq!(st.audio_sources(), vec!["".to_string(), "mic".to_string()]);
    }

    #[test]
    fn sanitise_clamps_and_drops_bad_data() {
        let mut st: Studio = serde_json::from_str(
            r#"{"layers":[{"name":"a\u0007b","brightness":9,"opacity":-1,"speed":99,"effect":{"type":"wave","colours":["zzzzzz","ff0000"]}},
                          {"effect":{"type":"image","w":2,"h":2,"data":"ff"}},
                          {"effect":{"type":"wheel","centre":[7,-3],"spread":100}}]}"#,
        )
        .unwrap();
        st.sanitise();
        let l = &st.layers[0];
        assert_eq!((l.name.as_str(), l.brightness, l.opacity, l.speed), ("ab", 1.0, 0.0, 5.0));
        assert_eq!(l.effect, StudioEffect::Wave { direction: Direction::LeftToRight, duration: Duration::Medium, colours: vec!["ff0000".into()] });
        assert!(matches!(&st.layers[1].effect, StudioEffect::Image { w: 0, h: 0, data, .. } if data.is_empty()));
        assert!(matches!(&st.layers[2].effect, StudioEffect::Wheel { centre, spread, .. } if *centre == [1.0, 0.0] && *spread == 4.0));
        let mut many = studio((0..100).map(|_| layer(StudioEffect::Battery, vec![])).collect());
        many.sanitise();
        assert_eq!(many.layers.len(), MAX_LAYERS);
    }

    #[test]
    fn old_files_get_default_layer_options() {
        let st: Studio = serde_json::from_str(r#"{"layers":[{"effect":{"type":"wheel"}},{"effect":{"type":"audio_meter"}}]}"#).unwrap();
        assert_eq!((st.layers[0].opacity, st.layers[0].speed, st.layers[0].span), (1.0, 1.0, None));
        assert!(matches!(&st.layers[0].effect, StudioEffect::Wheel { centre, spread, colours, .. } if *centre == [0.5, 0.5] && *spread == 1.0 && colours.is_empty()));
        assert!(matches!(&st.layers[1].effect, StudioEffect::AudioMeter { source: None, mode: MeterMode::Spectrum, peak_hold: false, .. }));
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
