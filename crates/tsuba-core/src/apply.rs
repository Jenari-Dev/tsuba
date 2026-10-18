//! Apply a [`DeviceConfig`] to a device. Every function needs a `Device` with
//! writes enabled and returns a short log of what it changed.

use anyhow::{bail, Context, Result};
use tsuba_hid::{Device, Kind, Model};
use tsuba_proto::{chroma, commands as c, decode};

use crate::config::{DeviceConfig, DialSlotConfig, DpiConfig, EffectConfig, Lighting};
use crate::{parse_rgb, percent_to_level};

pub fn apply(dev: &Device, model: &Model, cfg: &DeviceConfig) -> Result<Vec<String>> {
    let mut log = Vec::new();
    if let Some(m) = &cfg.mode {
        if set_mode(dev, m)? {
            log.push(format!("device mode {m}"));
        }
    }
    if let Some(l) = &cfg.lighting {
        log.extend(apply_lighting(dev, l)?);
    }
    if let Some(d) = &cfg.dpi {
        if model.kind == Kind::Mouse && model.spec.features.dpi_stages {
            if let Some(max) = model.spec.max_dpi {
                if d.stages.iter().flatten().any(|v| *v as u32 > max) {
                    bail!("saved DPI exceeds this mouse's maximum of {max}");
                }
            }
            log.extend(apply_dpi(dev, d)?);
        }
    }
    if let Some(slots) = &cfg.dial {
        if model.spec.features.dial {
            log.extend(apply_dial(dev, slots)?);
        }
    }
    Ok(log)
}

pub fn to_effect(e: &EffectConfig) -> Result<chroma::Effect> {
    let colours = |v: &[String]| v.iter().map(|c| parse_rgb(c)).collect::<Result<Vec<_>>>();
    Ok(match e {
        EffectConfig::Off => chroma::Effect::Off,
        EffectConfig::Static { colour } => chroma::Effect::Static(parse_rgb(colour)?),
        EffectConfig::Breathing { colours: c } => chroma::Effect::Breathing(colours(c)?),
        EffectConfig::Spectrum => chroma::Effect::Spectrum,
        EffectConfig::Wave { direction, speed } => chroma::Effect::Wave { direction: *direction, speed: *speed },
        EffectConfig::Reactive { speed, colour } => chroma::Effect::Reactive { speed: *speed, colour: parse_rgb(colour)? },
        EffectConfig::Starlight { speed, colours: c } => chroma::Effect::Starlight { speed: *speed, colours: colours(c)? },
        EffectConfig::Ripple { speed, colours: c } => chroma::Effect::Ripple { speed: *speed, colours: colours(c)? },
        EffectConfig::Custom { .. } | EffectConfig::Studio { .. } => chroma::Effect::CustomFrame,
    })
}

fn colour_count(e: &chroma::Effect) -> usize {
    match e {
        chroma::Effect::Static(_) | chroma::Effect::Reactive { .. } => 1,
        chroma::Effect::Breathing(v) | chroma::Effect::Starlight { colours: v, .. } | chroma::Effect::Ripple { colours: v, .. } => v.len(),
        _ => 0,
    }
}

/// Parse custom rows into a rows×cols grid (missing or malformed entries are off).
pub fn custom_grid(rows: &[String], n_rows: u8, n_cols: u8) -> Vec<Vec<chroma::Rgb>> {
    (0..n_rows as usize)
        .map(|r| {
            let cells: Vec<&str> = rows.get(r).map(|s| s.split_whitespace().collect()).unwrap_or_default();
            (0..n_cols as usize).map(|c| cells.get(c).and_then(|h| parse_rgb(h).ok()).unwrap_or([0, 0, 0])).collect()
        })
        .collect()
}

/// Stream a per-LED frame to the device's matrix region and switch to custom-frame mode.
/// Custom frames are not stored on the device; tsubad re-sends them when it reconnects.
pub fn apply_custom(dev: &Device, rows: &[String], brightness: Option<u8>) -> Result<Vec<String>> {
    let region = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?)
        .into_iter()
        .max_by_key(|r| r.rows as u32 * r.cols as u32)
        .context("no lighting region")?;
    if region.rows * region.cols <= 1 {
        bail!("per-LED lighting needs a matrix region (this device has single-LED zones)");
    }
    for (r, leds) in custom_grid(rows, region.rows, region.cols).iter().enumerate() {
        dev.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(r as u8, 0, leds))?;
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    dev.query(chroma::set_effect(0), &chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame))?;
    let mut log = vec![format!("custom frame {}x{}", region.rows, region.cols)];
    if let Some(p) = brightness {
        dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, region.id, percent_to_level(p)))?;
        log.push(format!("brightness {p}%"));
    }
    Ok(log)
}

/// Apply an effect and brightness to every region that supports the effect.
pub fn apply_lighting(dev: &Device, l: &Lighting) -> Result<Vec<String>> {
    match &l.effect {
        EffectConfig::Custom { rows } => return apply_custom(dev, rows, l.brightness),
        // Rendered frame by frame by tsubad's studio runner, not a one-shot write.
        EffectConfig::Studio { .. } => return Ok(vec!["chroma studio (rendered by tsubad)".into()]),
        _ => {}
    }
    let effect = to_effect(&l.effect)?;
    let want = format!("{effect:?}");
    let mut log = Vec::new();
    for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
        let supported = dev
            .query(chroma::GET_EFFECT_LIST, &[r.id])
            .ok()
            .and_then(|a| chroma::effect_list(&a))
            .map(|(_, ids)| ids.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>())
            .unwrap_or_default();
        let name = want.split([' ', '(', '{']).next().unwrap_or("");
        if !supported.is_empty() && !supported.iter().any(|s| s == name) {
            log.push(format!("region {}: {name} not supported, skipped", r.id));
            continue;
        }
        dev.query(chroma::set_effect(colour_count(&effect)), &chroma::encode_effect(chroma::STORE, r.id, &effect))?;
        log.push(format!("region {}: {want}", r.id));
        if let Some(p) = l.brightness {
            dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, percent_to_level(p)))?;
            log.push(format!("region {}: brightness {p}%", r.id));
        }
    }
    Ok(log)
}

/// Switch to "normal" or "driver" mode (verified); returns false if it was already set.
pub fn set_mode(dev: &Device, mode: &str) -> Result<bool> {
    let (byte, want) = match mode {
        "normal" => (0x00, decode::DeviceMode::Normal),
        "driver" => (0x03, decode::DeviceMode::Driver),
        x => bail!("unknown device mode {x:?} (normal or driver)"),
    };
    if decode::device_mode(&dev.query(c::GET_DEVICE_MODE, &[])?) == Some(want) {
        return Ok(false);
    }
    dev.query(c::SET_DEVICE_MODE, &[byte, 0x00])?;
    let got = decode::device_mode(&dev.query(c::GET_DEVICE_MODE, &[])?);
    if got != Some(want) {
        bail!("device stayed in {got:?} mode");
    }
    Ok(true)
}

/// Write the stage table, then the active DPI (writing stages resets the active DPI
/// to the active stage's value).
pub fn apply_dpi(dev: &Device, d: &DpiConfig) -> Result<Vec<String>> {
    apply_dpi_for(dev, decode::VARSTORE, d)
}

/// [`apply_dpi`] on a given on-board profile.
pub fn apply_dpi_for(dev: &Device, profile: u8, d: &DpiConfig) -> Result<Vec<String>> {
    if d.stages.is_empty() || d.stages.len() > 5 {
        bail!("DPI stages must number 1..=5, got {}", d.stages.len());
    }
    if d.active_stage == 0 || d.active_stage as usize > d.stages.len() {
        bail!("active_stage {} out of range", d.active_stage);
    }
    let current = decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[profile])?).context("stages")?;
    let stages = decode::DpiStages {
        active: d.active_stage,
        stages: d
            .stages
            .iter()
            .enumerate()
            .map(|(i, [x, y])| decode::DpiStage {
                id: i as u8 + 1,
                dpi: decode::Dpi { x: *x, y: *y },
                extra: current.stages.get(i).map_or(0, |s| s.extra),
            })
            .collect(),
    };
    let mut log = Vec::new();
    if stages != current {
        dev.query(c::set_dpi_stages(stages.stages.len()), &decode::encode_dpi_stages_for(profile, &stages))?;
        log.push(format!("DPI stages {:?}, active stage {}", d.stages, d.active_stage));
    }
    let active = d.active.map(|[x, y]| decode::Dpi { x, y }).unwrap_or(stages.stages[d.active_stage as usize - 1].dpi);
    dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi_for(profile, active))?;
    log.push(format!("active DPI {} x {}", active.x, active.y));
    Ok(log)
}

fn knob_function(name: &str) -> Result<decode::KnobFunction> {
    (0..=7u8)
        .map(decode::KnobFunction::from)
        .find(|f| format!("{f:?}") == name)
        .with_context(|| format!("unknown dial function {name}"))
}

pub fn apply_dial(dev: &Device, slots: &[DialSlotConfig]) -> Result<Vec<String>> {
    if !slots.iter().any(|s| s.enabled) {
        bail!("at least one dial mode must stay enabled");
    }
    let mut modes = decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?).context("dial")?;
    if slots.len() != modes.len() {
        bail!("dial config has {} slots, device has {}", slots.len(), modes.len());
    }
    let before = modes.clone();
    for (m, s) in modes.iter_mut().zip(slots) {
        m.function = knob_function(&s.function)?;
        m.rgb = parse_rgb(&s.colour)?;
        m.enabled = s.enabled;
    }
    if modes == before {
        return Ok(vec![]);
    }
    dev.query(c::SET_KNOB_CONFIGURATION, &decode::encode_knob_configuration(&modes))?;
    let back = decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?).context("dial read-back")?;
    if back != modes {
        bail!("device kept its old dial configuration");
    }
    Ok(vec![format!("dial: {} slots written", modes.len())])
}

/// Make dial slot `slot` the active mode (what turning the dial does right now).
pub fn set_dial_mode(dev: &Device, slot: usize) -> Result<()> {
    let modes = decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?).context("dial")?;
    let m = modes.get(slot).with_context(|| format!("no dial slot {slot}"))?;
    if !m.enabled {
        bail!("that dial mode is turned off");
    }
    let mut args = decode::KNOB_LED.to_vec();
    args.extend(decode::encode_knob_mode(m));
    dev.query(c::SET_CURRENT_KNOB_MODE, &args)?;
    let cur = decode::current_knob_mode(&dev.query(c::GET_CURRENT_KNOB_MODE, &decode::KNOB_LED)?).context("dial mode")?;
    if cur.function != m.function {
        bail!("device stayed on {:?}", cur.function);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_grid_fills_missing_cells() {
        let g = custom_grid(&["ff0000 bogus".into()], 2, 3);
        assert_eq!(g, vec![vec![[255, 0, 0], [0, 0, 0], [0, 0, 0]], vec![[0, 0, 0]; 3]]);
    }
}

/// Write one key (keyboard) or button (mouse) assignment and verify it reads back.
pub fn set_assignment(dev: &Device, model: &Model, x: &decode::Assignment) -> Result<decode::Assignment> {
    let (set, get) = if model.kind.has_buttons() {
        (c::SET_SINGLE_BUTTON_ASSIGNMENT, c::GET_SINGLE_BUTTON_ASSIGNMENT)
    } else if model.kind.has_keys() {
        (c::SET_SINGLE_KEY_ASSIGNMENT, c::GET_SINGLE_KEY_ASSIGNMENT)
    } else {
        bail!("a {} has no remappable keys or buttons", model.kind.as_str());
    };
    if x.data.len() > 5 {
        bail!("assignment data is {} bytes; at most 5 fit", x.data.len());
    }
    dev.query(set, &decode::encode_assignment(x))?;
    let back = decode::assignment(&dev.query(get, &[x.profile, x.key, x.layer])?).context("read-back")?;
    if back.function != x.function || back.data != x.data {
        bail!("device kept {:?} instead of {:?}", (back.function, &back.data), (x.function, &x.data));
    }
    Ok(back)
}
