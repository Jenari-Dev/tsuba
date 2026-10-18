//! Power settings and device info (Synapse's power/battery options): battery level and
//! charging, sleep timer, low-power threshold, lighting cap on battery, serial number.
//! Settings a device doesn't answer are left out; writes are read back.

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use tsuba_hid::Device;
use tsuba_proto::commands as c;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Power {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
    /// Sleep after this many seconds idle (on battery).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_seconds: Option<u16>,
    /// Low-power mode below this battery percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_power_percent: Option<u8>,
    /// Lighting brightness cap on battery, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_led_percent: Option<u8>,
    /// Read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

fn pct(v: u8) -> u8 {
    ((v as u32 * 100 + 127) / 255) as u8
}

fn level(p: u8) -> u8 {
    ((p.min(100) as u32 * 255 + 50) / 100) as u8
}

pub fn read(dev: &Device) -> Power {
    let b1 = |cmd| dev.query(cmd, &[]).ok().and_then(|a| a.first().copied());
    let b2 = |cmd| dev.query(cmd, &[]).ok().and_then(|a| a.get(1).copied());
    Power {
        battery_percent: b2(c::GET_BATTERY_LEVEL).map(pct),
        charging: b2(c::GET_CHARGING_STATUS).map(|v| v == 1),
        sleep_seconds: dev.query(c::GET_SLEEP_TIME, &[]).ok().and_then(|a| Some(u16::from_be_bytes([*a.first()?, *a.get(1)?]))),
        low_power_percent: b1(c::GET_BATTERY_CRITICAL).map(pct),
        battery_led_percent: b1(c::GET_BATTERY_LED_MAX).map(pct),
        serial: dev.query(c::GET_SERIAL, &[]).ok().map(|a| {
            let end = a.iter().position(|&x| x == 0).unwrap_or(a.len());
            String::from_utf8_lossy(&a[..end]).trim().to_string()
        }),
    }
}

/// Apply the settable fields present in `want`; returns one line per change.
pub fn apply(dev: &Device, want: &Power) -> Result<Vec<String>> {
    let mut log = Vec::new();
    if let Some(s) = want.sleep_seconds {
        ensure!((60..=900).contains(&s), "sleep timer must be 1–15 minutes");
        dev.query(c::SET_SLEEP_TIME, &s.to_be_bytes())?;
        ensure!(read(dev).sleep_seconds == Some(s), "device kept its old sleep timer");
        log.push(format!("sleep after {} min", s / 60));
    }
    if let Some(p) = want.low_power_percent {
        dev.query(c::SET_BATTERY_CRITICAL, &[level(p)])?;
        ensure!(read(dev).low_power_percent.map(|v| v.abs_diff(p) <= 1) == Some(true), "device kept its old low-power threshold");
        log.push(format!("low-power mode at {p}%"));
    }
    if let Some(p) = want.battery_led_percent {
        dev.query(c::SET_BATTERY_LED_MAX, &[level(p)])?;
        ensure!(read(dev).battery_led_percent.map(|v| v.abs_diff(p) <= 1) == Some(true), "device kept its old battery brightness");
        log.push(format!("lighting on battery at most {p}%"));
    }
    if log.is_empty() {
        log.push("nothing to change".into());
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_scale_round_trips() {
        assert_eq!(pct(0xff), 100);
        assert_eq!(pct(0x4c), 30);
        for p in [5u8, 30, 50, 100] {
            assert!(pct(level(p)).abs_diff(p) <= 1);
        }
    }
}
