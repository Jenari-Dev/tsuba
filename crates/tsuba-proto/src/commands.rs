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

// ---- Device firmware features (probe-gated; see `tsuba_core::features`). Argument layouts are
// from Synapse 4's protocol-2.5 encoders; every Get here is answered or refused by the firmware
// without side effects, so `tsuba probe-features` may send them to find out what a device has.

/// `[mode]`: Set Device Reset. 1 = factory reset (erases on-board profiles/macros), 0 = user data.
pub const SET_DEVICE_RESET: Command = Command::new(1, 0x00, 0x0b);
/// `[connected]`.
pub const GET_WIRELESS_CONNECTION: Command = Command::new(1, 0x00, 0xb8);

/// `[make ms, make mask ms, break ms, break mask ms]` (keyboard switch debounce, live).
pub const GET_DEBOUNCE: Command = Command::new(4, 0x02, 0x82);
pub const SET_DEBOUNCE: Command = Command::new(4, 0x02, 0x02);
/// `[profile, make, make mask, break, break mask]` (the on-board profile's copy).
pub const GET_PROFILE_DEBOUNCE: Command = Command::new(5, 0x02, 0x95);
pub const SET_PROFILE_DEBOUNCE: Command = Command::new(5, 0x02, 0x15);
/// `[profile, level]`: how strongly scroll acceleration scales fast spins.
pub const GET_SCROLL_ACCEL_LEVEL: Command = Command::new(2, 0x02, 0xa2);
pub const SET_SCROLL_ACCEL_LEVEL: Command = Command::new(2, 0x02, 0x22);
/// `[profile, level]`: how fast a spin must be before Smart-Reel switches to free-spin.
pub const GET_SMART_REEL_THRESHOLD: Command = Command::new(2, 0x02, 0xa3);
pub const SET_SMART_REEL_THRESHOLD: Command = Command::new(2, 0x02, 0x23);

/// `[charging effect]`.
pub const GET_LED_CHARGING_EFFECT: Command = Command::new(1, 0x03, 0x90);

/// `[profile]`: restore one on-board profile to factory defaults (assignments, DPI, lighting).
pub const SET_PROFILE_RESET: Command = Command::new(1, 0x05, 0x0b);
/// `[profile, R, G, B]`: the colour the device shows for an on-board profile.
pub const GET_PROFILE_COLOUR: Command = Command::new(4, 0x05, 0x8c);
pub const SET_PROFILE_COLOUR: Command = Command::new(4, 0x05, 0x0c);

/// `[value]`: indicator LED configuration (1 = on, Synapse's default).
pub const GET_INDICATOR_LED: Command = Command::new(1, 0x07, 0x90);
pub const SET_INDICATOR_LED: Command = Command::new(1, 0x07, 0x10);
/// `[status per indicator…]`. Shares its Get id with "Get Low Power Mode State" on other models.
pub const GET_INDICATOR_LEDS: Command = Command::new(3, 0x07, 0x95);
pub const SET_INDICATOR_LEDS: Command = Command::new(3, 0x07, 0x15);
/// `[profile, status, mode]` (wireless: dim the lighting while the mouse moves).
pub const GET_SMART_ILLUMINATION: Command = Command::new(3, 0x07, 0x93);
pub const SET_SMART_ILLUMINATION: Command = Command::new(3, 0x07, 0x13);
/// `[profile, enabled, seconds hi, seconds lo, brightness %]` (dim the lighting when idle).
pub const GET_TIME_TO_DIM: Command = Command::new(5, 0x07, 0x8b);
pub const SET_TIME_TO_DIM: Command = Command::new(5, 0x07, 0x0b);
/// `[quality]` (HyperSpeed receiver link quality).
pub const GET_DONGLE_QUALITY: Command = Command::new(1, 0x07, 0x96);

/// Lift-off / surface sensor (class 0x0b). Arguments start `[class id, sensor id]`;
/// the Basilisk V3 Pro family uses class 0 and the Avago proximity sensor (4).
pub const PROXIMITY_CLASS: u8 = 0x00;
pub const PROXIMITY_SENSOR: u8 = 0x04;
pub const GET_PROXIMITY_SENSORS: Command = Command::new(2, 0x0b, 0x80);
/// `[class, sensor, on]`.
pub const GET_PROXIMITY_STATE: Command = Command::new(3, 0x0b, 0x83);
pub const SET_PROXIMITY_STATE: Command = Command::new(3, 0x0b, 0x03);
/// `[class, sensor, params…]`.
pub const GET_PROXIMITY_CONFIG: Command = Command::new(10, 0x0b, 0x85);
/// `[class, sensor, lift mode, lift height]` (no Get exists).
pub const SET_PROXIMITY_LIFT: Command = Command::new(4, 0x0b, 0x0b);
/// `[profile, on]` (on-device pointer acceleration).
pub const GET_PROXIMITY_ACCEL_STATE: Command = Command::new(2, 0x0b, 0x90);

/// `[0]` → `[disabled]` or `[profile, disabled]`.
pub const GET_STARTUP_ANIMATION: Command = Command::new(1, 0x0f, 0x98);
/// `[0, disabled]` (0 = play the animation at power-on, 1 = skip it).
pub const SET_STARTUP_ANIMATION: Command = Command::new(2, 0x0f, 0x18);

/// Every catalog name for a class/id slot (some slots are reused per device family).
pub fn names(class: u8, id: u8) -> impl Iterator<Item = &'static str> {
    TABLE.iter().filter(move |e| e.0 == class && e.1 == id).map(|e| e.3)
}
