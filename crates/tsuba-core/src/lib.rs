//! Shared logic for the `tsuba` CLI and the `tsubad` daemon: reading a device's
//! full state, the on-disk configuration model, and applying a configuration.

pub mod app_links;
pub mod apply;
pub mod chroma_profiles;
pub mod config;
pub mod desktop_apps;
pub mod devices;
pub mod features;
pub mod fsutil;
pub mod game_lighting;
pub mod game_lighting_install;
pub mod games;
pub mod library;
pub mod macros;
pub mod performance;
pub mod power;
pub mod profiles;
pub mod state;
#[cfg(any(test, feature = "sim"))]
pub mod sim;
pub mod studio;
pub mod synapse_import;
pub mod text_layout;

pub use config::{Config, DeviceConfig};
pub use state::DeviceState;

/// "028d"-style product id used as the stable device key in config and D-Bus.
pub fn pid_key(pid: u16) -> String {
    format!("{pid:04x}")
}

pub fn hex_rgb(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn parse_rgb(h: &str) -> anyhow::Result<[u8; 3]> {
    let h = h.trim_start_matches('#');
    anyhow::ensure!(h.len() == 6, "not a RRGGBB colour: {h}");
    let v = u32::from_str_radix(h, 16).map_err(|_| anyhow::anyhow!("not a RRGGBB colour: {h}"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Brightness percent (0–100) ↔ device level (0–255), rounded.
pub fn percent_to_level(p: u8) -> u8 {
    ((p.min(100) as u32 * 255 + 50) / 100) as u8
}
pub fn level_to_percent(l: u8) -> u8 {
    ((l as u32 * 100 + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(parse_rgb("#fe0a03").unwrap(), [0xfe, 0x0a, 0x03]);
        assert_eq!(hex_rgb([0xfe, 0x0a, 0x03]), "fe0a03");
        assert!(parse_rgb("fff").is_err());
        for p in 0..=100 {
            assert_eq!(level_to_percent(percent_to_level(p)), p);
        }
        assert_eq!(percent_to_level(100), 255);
    }
}
