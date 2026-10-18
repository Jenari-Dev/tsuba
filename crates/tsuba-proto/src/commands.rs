//! Protocol-2.5 commands. Named constants for the ones this crate decodes;
//! `TABLE` holds the full catalog recovered from Synapse 4 (442 entries).

use crate::Command;

#[path = "table.rs"]
mod table;
pub use table::TABLE;

// Class 0x00 — device / system
pub const GET_FIRMWARE_VERSION: Command = Command::new(2, 0x00, 0x81);
pub const GET_DEVICE_MODE: Command = Command::new(2, 0x00, 0x84);
pub const SET_DEVICE_MODE: Command = Command::new(2, 0x00, 0x04);
pub const GET_POLLING_PERIOD: Command = Command::new(1, 0x00, 0x85);
pub const SET_POLLING_PERIOD: Command = Command::new(1, 0x00, 0x05);
/// `[profile, period]` — the on-board profile's polling period.
pub const GET_PROFILE_POLLING_PERIOD: Command = Command::new(2, 0x00, 0x8e);
pub const SET_PROFILE_POLLING_PERIOD: Command = Command::new(2, 0x00, 0x0e);
/// `[0, rate bit]` — HyperPolling devices (8000 Hz class).
pub const GET_HS_POLLING_PERIOD: Command = Command::new(2, 0x00, 0xc0);
pub const SET_HS_POLLING_PERIOD: Command = Command::new(2, 0x00, 0x40);

// Class 0x02 — keys and buttons (Set data size is 10, as Synapse sends it)
pub const GET_BUTTON_ID_LIST: Command = Command::new(80, 0x02, 0x84);
pub const GET_SINGLE_BUTTON_ASSIGNMENT: Command = Command::new(80, 0x02, 0x8c);
pub const SET_SINGLE_BUTTON_ASSIGNMENT: Command = Command::new(10, 0x02, 0x0c);
pub const GET_SINGLE_KEY_ASSIGNMENT: Command = Command::new(80, 0x02, 0x8d);
pub const SET_SINGLE_KEY_ASSIGNMENT: Command = Command::new(10, 0x02, 0x0d);
/// Scroll wheel, all `[profile, value]`: free-spin (1) / tactile (0), scroll acceleration,
/// and Smart-Reel (switch to free-spin automatically on fast scrolls).
pub const GET_SCROLL_FREE_SPIN: Command = Command::new(2, 0x02, 0x94);
pub const SET_SCROLL_FREE_SPIN: Command = Command::new(2, 0x02, 0x14);
pub const GET_SCROLL_ACCELERATION: Command = Command::new(2, 0x02, 0x96);
pub const SET_SCROLL_ACCELERATION: Command = Command::new(2, 0x02, 0x16);
pub const GET_SCROLL_SMART_REEL: Command = Command::new(80, 0x02, 0x97);
pub const SET_SCROLL_SMART_REEL: Command = Command::new(80, 0x02, 0x17);

// Class 0x03 — LEDs and the command dial
pub const GET_KNOB_CONFIGURATION: Command = Command::new(50, 0x03, 0x92);
pub const SET_KNOB_CONFIGURATION: Command = Command::new(50, 0x03, 0x12);
pub const GET_CURRENT_KNOB_MODE: Command = Command::new(8, 0x03, 0x93);
pub const SET_CURRENT_KNOB_MODE: Command = Command::new(8, 0x03, 0x13);

// Class 0x04 — DPI
pub const GET_ACTIVE_DPI: Command = Command::new(7, 0x04, 0x85);
pub const SET_ACTIVE_DPI: Command = Command::new(7, 0x04, 0x05);
pub const GET_DPI_STAGES: Command = Command::new(38, 0x04, 0x86);

/// Set Profile DPI Class Data; the data size depends on the stage count.
/// Note: the device also switches its active DPI to the active stage's value.
pub const fn set_dpi_stages(count: usize) -> Command {
    Command::new((7 * count + 3) as u8, 0x04, 0x06)
}

// Class 0x07 — power & battery
/// `[0, level 0–255]`.
pub const GET_BATTERY_LEVEL: Command = Command::new(2, 0x07, 0x80);
/// `[0, 1 = charging]`.
pub const GET_CHARGING_STATUS: Command = Command::new(2, 0x07, 0x84);
/// Low-power-mode threshold, 0–255 of the battery (`[level]`).
pub const GET_BATTERY_CRITICAL: Command = Command::new(1, 0x07, 0x81);
pub const SET_BATTERY_CRITICAL: Command = Command::new(1, 0x07, 0x01);
/// Lighting brightness cap on battery, 0–255.
pub const GET_BATTERY_LED_MAX: Command = Command::new(1, 0x07, 0x82);
pub const SET_BATTERY_LED_MAX: Command = Command::new(1, 0x07, 0x02);
/// Sleep after this many seconds idle on battery (`[hi, lo]`).
pub const GET_SLEEP_TIME: Command = Command::new(2, 0x07, 0x83);
pub const SET_SLEEP_TIME: Command = Command::new(2, 0x07, 0x03);
pub const GET_SERIAL: Command = Command::new(22, 0x00, 0x82);

// Class 0x06 — macros
pub const GET_MACRO_STORAGE_INFO: Command = Command::new(14, 0x06, 0x8e);

/// Every catalog name for a class/id slot (some slots are reused per device family).
pub fn names(class: u8, id: u8) -> impl Iterator<Item = &'static str> {
    TABLE.iter().filter(move |e| e.0 == class && e.1 == id).map(|e| e.3)
}
