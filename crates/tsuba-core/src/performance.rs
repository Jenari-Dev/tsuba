//! Performance settings (Synapse's PERFORMANCE tab): polling rate, scroll wheel and
//! DPI stages. Settings a device doesn't answer are left out. Every write is read
//! back before it is reported as applied.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::{Device, Kind, Model};
use tsuba_proto::{commands as c, decode, Command};

use crate::apply::apply_dpi_for;
use crate::config::DpiConfig;


#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Performance {
    /// On-board profile these settings belong to (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling_hz: Option<u32>,
    /// Rates the device offers (read-only; ignored on write).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polling_options: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_spin: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_acceleration: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_reel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_stages: Option<Vec<[u16; 2]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_active_stage: Option<u8>,
}

fn flag(dev: &Device, get: Command, profile: u8) -> Option<bool> {
    dev.query(get, &[profile]).ok().and_then(|a| a.get(1).map(|v| *v != 0))
}

fn hyperpolling(dev: &Device) -> Option<u32> {
    dev.query(c::GET_HS_POLLING_PERIOD, &[0]).ok().and_then(|a| decode::hs_polling_hz(*a.get(1)?))
}

pub fn read(dev: &Device, model: &Model, profile: u8) -> Result<Performance> {
    let mut p = Performance { profile: Some(profile), ..Default::default() };
    if let Some(hz) = hyperpolling(dev) {
        p.polling_hz = Some(hz);
        p.polling_options = decode::HS_POLLING.iter().map(|e| e.0).collect();
    } else if let Ok(a) = dev.query(c::GET_POLLING_PERIOD, &[]) {
        p.polling_hz = decode::polling_hz(&a);
        let mut rates: Vec<u32> = model.spec.polling_rates.iter().copied().filter(|hz| decode::polling_period(*hz).is_some()).collect();
        rates.sort_unstable_by(|a, b| b.cmp(a));
        p.polling_options = if rates.is_empty() { vec![1000, 500, 125] } else { rates };
    }
    let f = &model.spec.features;
    if model.kind == Kind::Mouse {
        if f.scroll_modes {
            p.free_spin = flag(dev, c::GET_SCROLL_FREE_SPIN, profile);
        }
        if f.scroll_acceleration {
            p.scroll_acceleration = flag(dev, c::GET_SCROLL_ACCELERATION, profile);
        }
        if f.smart_reel {
            p.smart_reel = flag(dev, c::GET_SCROLL_SMART_REEL, profile);
        }
    }
    if model.kind == Kind::Mouse && f.dpi_stages {
        if let Ok(a) = dev.query(c::GET_DPI_STAGES, &[profile]) {
            let s = decode::dpi_stages(&a).context("stages")?;
            p.dpi_active_stage = Some(s.active);
            p.dpi_stages = Some(s.stages.iter().map(|st| [st.dpi.x, st.dpi.y]).collect());
        }
    }
    Ok(p)
}

fn set_flag(dev: &Device, set: Command, get: Command, profile: u8, on: bool, what: &str) -> Result<String> {
    dev.query(set, &[profile, on as u8])?;
    ensure!(flag(dev, get, profile) == Some(on), "device kept the old {what} setting");
    Ok(format!("{what} {}", if on { "on" } else { "off" }))
}

/// Apply every field that is set; returns one line per change.
pub fn apply(dev: &Device, model: &Model, want: &Performance) -> Result<Vec<String>> {
    let profile = want.profile.unwrap_or(1);
    let mut log = Vec::new();
    if let Some(hz) = want.polling_hz {
        if hyperpolling(dev).is_some() {
            let bit = decode::hs_polling_bit(hz).with_context(|| format!("{hz} Hz isn't a HyperPolling rate"))?;
            dev.query(c::SET_HS_POLLING_PERIOD, &[0, bit])?;
            ensure!(hyperpolling(dev) == Some(hz), "device kept its old polling rate");
        } else {
            let period = decode::polling_period(hz).with_context(|| format!("unsupported polling rate {hz} Hz"))?;
            dev.query(c::SET_POLLING_PERIOD, &[period])?;
            // The on-board profile keeps its own copy (used when the device starts up).
            let _ = dev.query(c::SET_PROFILE_POLLING_PERIOD, &[profile, period]);
            ensure!(decode::polling_hz(&dev.query(c::GET_POLLING_PERIOD, &[])?) == Some(hz), "device kept its old polling rate");
        }
        log.push(format!("polling rate {hz} Hz"));
    }
    let f = &model.spec.features;
    if want.free_spin.is_some() && !f.scroll_modes {
        bail!("{} has no free-spin scroll mode", model.name);
    }
    if want.scroll_acceleration.is_some() && !f.scroll_acceleration {
        bail!("{} has no scroll acceleration setting", model.name);
    }
    if want.smart_reel.is_some() && !f.smart_reel {
        bail!("{} has no Smart-Reel setting", model.name);
    }
    if model.kind == Kind::Mouse {
        if let Some(v) = want.free_spin {
            log.push(set_flag(dev, c::SET_SCROLL_FREE_SPIN, c::GET_SCROLL_FREE_SPIN, profile, v, "free-spin scrolling")?);
        }
        if let Some(v) = want.scroll_acceleration {
            log.push(set_flag(dev, c::SET_SCROLL_ACCELERATION, c::GET_SCROLL_ACCELERATION, profile, v, "scroll acceleration")?);
        }
        if let Some(v) = want.smart_reel {
            log.push(set_flag(dev, c::SET_SCROLL_SMART_REEL, c::GET_SCROLL_SMART_REEL, profile, v, "Smart-Reel")?);
        }
    } else if want.free_spin.is_some() || want.scroll_acceleration.is_some() || want.smart_reel.is_some() {
        bail!("scroll settings apply to mice only");
    }
    if let Some(stages) = &want.dpi_stages {
        ensure!(model.kind == Kind::Mouse, "DPI stages apply to mice only");
        ensure!(f.dpi_stages, "{} has no DPI stages", model.name);
        if let Some(max) = model.spec.max_dpi {
            ensure!(stages.iter().flatten().all(|v| *v as u32 <= max), "DPI above this mouse's maximum of {max}");
        }
        let active_stage = want.dpi_active_stage.unwrap_or(1).clamp(1, stages.len().max(1) as u8);
        log.extend(apply_dpi_for(dev, profile, &DpiConfig { stages: stages.clone(), active_stage, active: None })?);
        let back = read(dev, model, profile)?;
        ensure!(back.dpi_stages.as_ref() == Some(stages), "device kept its old DPI stages");
    }
    if log.is_empty() {
        log.push("nothing to change".into());
    }
    Ok(log)
}
