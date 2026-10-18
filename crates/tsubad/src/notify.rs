//! Desktop notifications for device events (DPI stage, profile, dial, wheel mode, low battery,
//! connect / disconnect). They go through the freedesktop notification service, so on Plasma they
//! appear as the usual on-screen popup. The daemon sends them so they work with the app closed.
//!
//! The policy (what is worth a popup, when) is a pure function over [`Settings`] and [`State`] and
//! is unit-tested; delivery is behind [`Sink`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tsuba_proto::events::Event;

/// Which notifications to show. Every field has a default so older files keep loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Master switch.
    pub enabled: bool,
    /// Sensitivity (DPI) stage changes.
    pub dpi: bool,
    /// Profile changes from the device's own profile button.
    pub profile: bool,
    /// The keyboard dial changed mode.
    pub dial: bool,
    /// Scroll wheel free-spin / tactile switch.
    pub wheel: bool,
    /// Battery below `battery_threshold`.
    pub battery_low: bool,
    /// Battery reached 100 % while charging.
    pub battery_full: bool,
    /// Percentage at or below which the low-battery popup shows (1..=50).
    pub battery_threshold: u8,
    /// A device appeared or disappeared.
    pub connection: bool,
    /// Ask the desktop to play its notification sound (audio prompt).
    pub sound: bool,
    /// Plasma only: show routine messages as the on-screen display (like the volume pop-up) instead of a
    /// notification. Warnings such as a low battery stay notifications. Falls back when Plasma's service is absent.
    pub osd: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            dpi: true,
            profile: true,
            dial: true,
            wheel: true,
            battery_low: true,
            battery_full: false,
            battery_threshold: 15,
            connection: true,
            sound: false,
            osd: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tsuba").join("notifications.json")
    }

    /// Missing or unreadable file = defaults.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str::<Settings>(&s).ok()).unwrap_or_default().clamped()
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn clamped(mut self) -> Self {
        self.battery_threshold = self.battery_threshold.clamp(1, 50);
        self
    }
}

/// One popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Same key = the new popup replaces the old one (a burst of DPI presses shows one popup).
    pub key: String,
    pub title: String,
    pub body: String,
    pub icon: &'static str,
    pub critical: bool,
}

/// What happened, in the terms the policy needs.
pub enum Input<'a> {
    Event { pid: &'a str, name: &'a str, ev: &'a Event, profile: Option<String> },
    Attached { pid: &'a str, name: &'a str },
    Detached { pid: &'a str, name: &'a str },
    Battery { pid: &'a str, name: &'a str, percent: u8, charging: bool },
}

/// Remembered between inputs.
#[derive(Default)]
pub struct State {
    /// Devices already warned about a low battery (re-armed by charging or recovery).
    warned_low: HashSet<String>,
    /// Devices already told "fully charged" for this charge.
    told_full: HashSet<String>,
    /// Last (body, time) per key, to drop exact repeats within a second.
    last: HashMap<String, (String, Instant)>,
    /// No connect popups until then (the devices present at start are not news).
    quiet_until: Option<Instant>,
}

impl State {
    pub fn quiet_for(d: Duration, now: Instant) -> Self {
        Self { quiet_until: Some(now + d), ..Self::default() }
    }
}

fn dpi_from(data: &[u8]) -> Option<u16> {
    let v = u16::from_be_bytes([*data.first()?, *data.get(1)?]);
    (v > 0).then_some(v)
}

/// Decide whether `input` deserves a popup.
pub fn decide(s: &Settings, st: &mut State, now: Instant, input: &Input) -> Option<Note> {
    if !s.enabled {
        return None;
    }
    let note = match input {
        Input::Event { pid, name, ev: Event::Notification { id, data }, profile } => match id {
            2 if s.dpi => {
                let dpi = dpi_from(data)?;
                Note { key: format!("{pid}:dpi"), title: name.to_string(), body: format!("Sensitivity {dpi} DPI"), icon: "input-mouse", critical: false }
            }
            1 if s.profile => Note {
                key: format!("{pid}:profile"),
                title: name.to_string(),
                body: match profile {
                    Some(p) => format!("Profile: {p}"),
                    None => "Profile changed".to_string(),
                },
                icon: "preferences-system-users",
                critical: false,
            },
            83 if s.dial => Note { key: format!("{pid}:dial"), title: name.to_string(), body: "Dial changed".to_string(), icon: "input-keyboard", critical: false },
            57 if s.wheel => Note {
                key: format!("{pid}:wheel"),
                title: name.to_string(),
                body: if data.first().copied().unwrap_or(0) != 0 { "Scroll wheel: free-spin" } else { "Scroll wheel: tactile" }.to_string(),
                icon: "input-mouse",
                critical: false,
            },
            _ => return None,
        },
        Input::Event { .. } => return None,
        Input::Attached { pid, name } if s.connection => {
            if st.quiet_until.is_some_and(|t| now < t) {
                return None;
            }
            Note { key: format!("{pid}:link"), title: name.to_string(), body: "Connected".to_string(), icon: "preferences-desktop-peripherals", critical: false }
        }
        Input::Detached { pid, name } if s.connection => {
            Note { key: format!("{pid}:link"), title: name.to_string(), body: "Disconnected".to_string(), icon: "preferences-desktop-peripherals", critical: false }
        }
        Input::Attached { .. } | Input::Detached { .. } => return None,
        Input::Battery { pid, name, percent, charging } => {
            let pid = pid.to_string();
            let threshold = s.battery_threshold;
            if *charging || *percent > threshold.saturating_add(5) {
                st.warned_low.remove(&pid);
            }
            if !*charging || *percent < 95 {
                st.told_full.remove(&pid);
            }
            if s.battery_low && !*charging && *percent <= threshold && st.warned_low.insert(pid.clone()) {
                Note { key: format!("{pid}:battery"), title: name.to_string(), body: format!("Battery low: {percent}%"), icon: "battery-low", critical: true }
            } else if s.battery_full && *charging && *percent >= 100 && st.told_full.insert(pid.clone()) {
                Note { key: format!("{pid}:battery"), title: name.to_string(), body: "Battery full".to_string(), icon: "battery-full-charged", critical: false }
            } else {
                return None;
            }
        }
    };
    if let Some((body, at)) = st.last.get(&note.key) {
        if *body == note.body && now.duration_since(*at) < Duration::from_secs(1) {
            return None;
        }
    }
    st.last.insert(note.key.clone(), (note.body.clone(), now));
    Some(note)
}

/// Marketing-free display name for a device pid (hex string like "028d").
pub fn device_name(pid: &str) -> String {
    u16::from_str_radix(pid, 16).ok().and_then(tsuba_hid::model).map(|m| m.name.to_string()).unwrap_or_else(|| pid.to_string())
}

/// Name of the active profile in a `library_json` document (`profiles` + `active`, or on-board `slots`).
pub fn profile_name(library: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(library).ok()?;
    let active = v.get("active")?;
    if let Some(p) = v["profiles"].as_array()?.iter().find(|p| p.get("id") == Some(active)) {
        return p["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string);
    }
    None
}

/// A hardware notification arrived: show the popup it deserves and refresh battery state when it concerns power.
/// Blocking (reads the device library / battery); run it off the async executor.
pub fn on_notification(mgr: &crate::manager::Manager, n: &Notifier, pid: &str, ev: &Event, id: u8) {
    let name = device_name(pid);
    let profile = (id == 1).then(|| mgr.library_json(pid).ok().and_then(|j| profile_name(&j))).flatten();
    n.handle(&Input::Event { pid, name: &name, ev, profile });
    if matches!(id, 12 | 49 | 65) {
        std::thread::sleep(Duration::from_millis(800));
        check_battery(mgr, n, pid);
    }
}

/// Read the battery (if the device has one) and feed the low-battery / full-battery rules.
pub fn check_battery(mgr: &crate::manager::Manager, n: &Notifier, pid: &str) {
    let Ok(json) = mgr.power_json(pid) else { return };
    let Ok(p) = serde_json::from_str::<tsuba_core::power::Power>(&json) else { return };
    if let Some(percent) = p.battery_percent {
        n.handle(&Input::Battery { pid, name: &device_name(pid), percent, charging: p.charging.unwrap_or(false) });
    }
}

/// Where popups go.
pub trait Sink: Send + Sync {
    fn send(&self, note: &Note, settings: &Settings);
}

/// The freedesktop notification service over D-Bus.
pub struct DBusSink {
    conn: zbus::Connection,
    replace: std::sync::Arc<Mutex<HashMap<String, u32>>>,
}

impl DBusSink {
    pub fn new(conn: zbus::Connection) -> Self {
        Self { conn, replace: Default::default() }
    }
}

impl Sink for DBusSink {
    fn send(&self, note: &Note, settings: &Settings) {
        use zbus::zvariant::Value;
        let sound = settings.sound;
        let osd = settings.osd && !note.critical;
        let conn = self.conn.clone();
        let replaces = self.replace.lock().unwrap().get(&note.key).copied().unwrap_or(0);
        let note = note.clone();
        let key = note.key.clone();
        let table = self.replace.clone();
        tokio::spawn(async move {
            if osd {
                // Plasma's on-screen display; if the service isn't there, show a notification instead.
                let text = format!("{}: {}", note.title, note.body);
                let shown = conn
                    .call_method(Some("org.kde.plasmashell"), "/org/kde/osdService", Some("org.kde.osdService"), "showText", &(note.icon, text.as_str()))
                    .await
                    .is_ok();
                if shown {
                    return;
                }
            }
            let mut hints: HashMap<&str, Value> = HashMap::new();
            hints.insert("desktop-entry", Value::from("io.github.jenaridev.tsuba"));
            hints.insert("transient", Value::from(true));
            hints.insert("urgency", Value::from(if note.critical { 2u8 } else { 1u8 }));
            if sound {
                hints.insert("sound-name", Value::from(if note.critical { "dialog-warning" } else { "message-new-instant" }));
            } else {
                hints.insert("suppress-sound", Value::from(true));
            }
            let timeout: i32 = if note.critical { 8000 } else { 3000 };
            let r = conn
                .call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "Notify",
                    &("Tsuba", replaces, note.icon, note.title.as_str(), note.body.as_str(), Vec::<&str>::new(), hints, timeout),
                )
                .await;
            match r.and_then(|m| m.body().deserialize::<u32>()) {
                Ok(id) => {
                    table.lock().unwrap().insert(key, id);
                }
                Err(e) => tracing::debug!("notification not shown: {e}"),
            }
        });
    }
}

/// Settings + state + sink, shared by the daemon's tasks.
pub struct Notifier {
    path: PathBuf,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    sink: Box<dyn Sink>,
}

impl Notifier {
    pub fn new(path: PathBuf, sink: Box<dyn Sink>) -> Self {
        let settings = Settings::load(&path);
        Self { path, settings: Mutex::new(settings), state: Mutex::new(State::quiet_for(Duration::from_secs(15), Instant::now())), sink }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, s: Settings) -> anyhow::Result<Settings> {
        let s = s.clamped();
        s.save(&self.path)?;
        *self.settings.lock().unwrap() = s.clone();
        Ok(s)
    }

    pub fn handle(&self, input: &Input) {
        let s = self.settings();
        let note = decide(&s, &mut self.state.lock().unwrap(), Instant::now(), input);
        if let Some(n) = note {
            self.sink.send(&n, &s);
        }
    }

    /// The "Send a test notification" button.
    pub fn test(&self) {
        let s = self.settings();
        self.sink.send(
            &Note { key: "test".into(), title: "Tsuba".into(), body: "Notifications are working.".into(), icon: "io.github.jenaridev.tsuba", critical: false },
            &s,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: u8, data: &[u8]) -> Event {
        Event::Notification { id, data: data.to_vec() }
    }
    fn run(s: &Settings, st: &mut State, e: &Event) -> Option<Note> {
        decide(s, st, Instant::now(), &Input::Event { pid: "028d", name: "Test Keyboard", ev: e, profile: None })
    }

    #[test]
    fn dpi_stage_popup_reads_big_endian() {
        let n = run(&Settings::default(), &mut State::default(), &ev(2, &[0x06, 0x40])).unwrap();
        assert_eq!(n.body, "Sensitivity 1600 DPI");
        assert_eq!(n.key, "028d:dpi");
    }

    #[test]
    fn master_switch_and_categories() {
        let mut s = Settings { enabled: false, ..Settings::default() };
        assert!(run(&s, &mut State::default(), &ev(2, &[3, 32])).is_none());
        s.enabled = true;
        s.dpi = false;
        assert!(run(&s, &mut State::default(), &ev(2, &[3, 32])).is_none());
        s.wheel = true;
        assert_eq!(run(&s, &mut State::default(), &ev(57, &[1])).unwrap().body, "Scroll wheel: free-spin");
        assert_eq!(run(&s, &mut State::default(), &ev(57, &[0])).unwrap().body, "Scroll wheel: tactile");
    }

    #[test]
    fn profile_uses_the_name_when_known() {
        let e = ev(1, &[2]);
        let n = decide(&Settings::default(), &mut State::default(), Instant::now(), &Input::Event { pid: "p", name: "M", ev: &e, profile: Some("Work".into()) }).unwrap();
        assert_eq!(n.body, "Profile: Work");
    }

    #[test]
    fn unknown_events_and_key_presses_are_silent() {
        assert!(run(&Settings::default(), &mut State::default(), &ev(99, &[1])).is_none());
        let keys = Event::RazerKeys(vec![1]);
        assert!(run(&Settings::default(), &mut State::default(), &keys).is_none());
        assert!(run(&Settings::default(), &mut State::default(), &Event::Media(vec![0xe2])).is_none());
    }

    #[test]
    fn exact_repeats_within_a_second_are_dropped() {
        let (s, mut st) = (Settings::default(), State::default());
        let t = Instant::now();
        let e = ev(2, &[3, 32]);
        let i = Input::Event { pid: "p", name: "M", ev: &e, profile: None };
        assert!(decide(&s, &mut st, t, &i).is_some());
        assert!(decide(&s, &mut st, t + Duration::from_millis(300), &i).is_none());
        assert!(decide(&s, &mut st, t + Duration::from_millis(1500), &i).is_some());
    }

    #[test]
    fn connect_is_quiet_at_startup_but_disconnect_is_not() {
        let s = Settings::default();
        let t = Instant::now();
        let mut st = State::quiet_for(Duration::from_secs(15), t);
        let a = Input::Attached { pid: "p", name: "M" };
        assert!(decide(&s, &mut st, t + Duration::from_secs(2), &a).is_none());
        assert!(decide(&s, &mut st, t + Duration::from_secs(20), &a).is_some());
        assert_eq!(decide(&s, &mut st, t + Duration::from_secs(21), &Input::Detached { pid: "p", name: "M" }).unwrap().body, "Disconnected");
    }

    #[test]
    fn low_battery_warns_once_until_charged() {
        let s = Settings::default();
        let (mut st, t) = (State::default(), Instant::now());
        let b = |p, c| Input::Battery { pid: "m", name: "Mouse", percent: p, charging: c };
        assert!(decide(&s, &mut st, t, &b(40, false)).is_none());
        let n = decide(&s, &mut st, t, &b(15, false)).unwrap();
        assert!(n.critical);
        assert_eq!(n.body, "Battery low: 15%");
        assert!(decide(&s, &mut st, t, &b(14, false)).is_none(), "no second warning");
        assert!(decide(&s, &mut st, t, &b(18, false)).is_none(), "hysteresis: still armed off");
        assert!(decide(&s, &mut st, t, &b(60, true)).is_none());
        assert!(decide(&s, &mut st, t, &b(10, false)).is_some(), "re-armed after charging");
    }

    #[test]
    fn battery_full_is_opt_in() {
        let t = Instant::now();
        let b = Input::Battery { pid: "m", name: "Mouse", percent: 100, charging: true };
        assert!(decide(&Settings::default(), &mut State::default(), t, &b).is_none());
        let s = Settings { battery_full: true, ..Settings::default() };
        let mut st = State::default();
        assert_eq!(decide(&s, &mut st, t, &b).unwrap().body, "Battery full");
        assert!(decide(&s, &mut st, t + Duration::from_secs(5), &b).is_none());
    }

    #[test]
    fn profile_name_from_library_json() {
        let j = r#"{"active":"b","profiles":[{"id":"a","name":"Work"},{"id":"b","name":"Games"}]}"#;
        assert_eq!(profile_name(j).as_deref(), Some("Games"));
        assert_eq!(profile_name(r#"{"active":"z","profiles":[]}"#), None);
        assert_eq!(profile_name("nonsense"), None);
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let dir = std::env::temp_dir().join(format!("tsuba-notify-test-{}", std::process::id()));
        let path = dir.join("notifications.json");
        assert_eq!(Settings::load(&path), Settings::default());
        let s = Settings { battery_threshold: 99, sound: true, ..Settings::default() }.clamped();
        assert_eq!(s.battery_threshold, 50);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        // an old file with missing fields still loads
        std::fs::write(&path, r#"{"dpi": false}"#).unwrap();
        let l = Settings::load(&path);
        assert!(!l.dpi && l.profile && l.battery_threshold == 15);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
