//! Software actions bound to keys through dynamic Razer-key codes (Synapse's Launch and
//! Inter-device functions). The firmware reports the code on press and release; tsubad runs
//! the action configured for it.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use tracing::{debug, info, warn};
use tsuba_core::config::{Action, DeviceOp, DpiConfig};
use tsuba_core::{apply, percent_to_level, profiles, state};
use tsuba_proto::{chroma, commands as c, decode};

use crate::manager::Manager;

/// One brightness step (10 %) on every lighting region of a device.
fn brightness_step(dev: &tsuba_hid::Device, up: bool) -> Result<()> {
    for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
        let level = dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0) as i32;
        let step = percent_to_level(10) as i32;
        let new = if up { level + step } else { level - step }.clamp(0, 255) as u8;
        dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, new))?;
    }
    Ok(())
}

/// Per-target state for actions that undo on release (clutch) or toggle (lighting).
#[derive(Default)]
pub struct ActionState {
    clutch: Mutex<HashMap<String, decode::Dpi>>,
    lights_off: Mutex<HashMap<String, Vec<(u8, u8)>>>,
    effect_idx: Mutex<HashMap<String, usize>>,
    /// The window that has the keyboard focus, once the KWin script has reported one.
    pub(crate) focus: Mutex<Option<tsuba_core::app_links::FocusInfo>>,
}

fn quick_effects() -> Vec<tsuba_core::config::EffectConfig> {
    use tsuba_core::config::EffectConfig as E;
    vec![
        E::Spectrum,
        E::Wave { direction: 1, speed: 0x28 },
        E::Breathing { colours: vec![] },
        E::Starlight { speed: 2, colours: vec![] },
        E::Reactive { speed: 2, colour: "44d62c".into() },
        E::Static { colour: "44d62c".into() },
    ]
}

fn run(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output().with_context(|| format!("running {cmd}"))?;
    if !out.status.success() {
        bail!("{cmd} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Desktop audio through PipeWire (wpctl) / its Pulse API (pactl).
fn audio(op: &tsuba_core::config::AudioOp) -> Result<()> {
    use tsuba_core::config::AudioOp as A;
    match op {
        A::VolumeUp => run("wpctl", &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%+"]).map(|_| ()),
        A::VolumeDown => run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"]).map(|_| ()),
        A::MuteToggle => run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]).map(|_| ()),
        A::MicMuteToggle => run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]).map(|_| ()),
        A::MicVolumeUp => run("wpctl", &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SOURCE@", "5%+"]).map(|_| ()),
        A::MicVolumeDown => run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", "5%-"]).map(|_| ()),
        A::SetDefaultOutput { name, .. } => run("pactl", &["set-default-sink", name]).map(|_| ()),
        A::SetDefaultInput { name, .. } => run("pactl", &["set-default-source", name]).map(|_| ()),
        A::NextOutput | A::PreviousOutput => {
            let sinks: Vec<String> = run("pactl", &["list", "short", "sinks"])?.lines().filter_map(|l| l.split('\t').nth(1).map(str::to_string)).collect();
            if sinks.is_empty() {
                bail!("no audio outputs");
            }
            let cur = run("pactl", &["get-default-sink"])?.trim().to_string();
            let i = sinks.iter().position(|s| *s == cur).unwrap_or(0);
            let n = sinks.len();
            let next = if matches!(op, A::NextOutput) { (i + 1) % n } else { (i + n - 1) % n };
            info!("audio output -> {}", sinks[next]);
            run("pactl", &["set-default-sink", &sinks[next]]).map(|_| ())
        }
    }
}

/// One audio output or input as the pickers show it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub description: String,
    pub default: bool,
}

/// Outputs (`sinks`) or inputs (`sources`, microphones first — monitors are left out) from the
/// JSON of `pactl -f json list <kind>`.
pub fn parse_audio_devices(json: &str, default_name: &str) -> Vec<AudioDevice> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let name = d["name"].as_str()?.to_string();
            if name.ends_with(".monitor") {
                return None;
            }
            let description = d["description"].as_str().unwrap_or(&name).to_string();
            Some(AudioDevice { default: name == default_name, name, description })
        })
        .collect()
}

/// The desktop's audio outputs and inputs: `{"sinks": [...], "sources": [...]}`.
pub fn audio_devices_json() -> serde_json::Value {
    let list = |kind: &str, default: &str| {
        let d = run("pactl", &[default]).unwrap_or_default();
        parse_audio_devices(&run("pactl", &["-f", "json", "list", kind]).unwrap_or_default(), d.trim())
    };
    serde_json::json!({ "sinks": list("sinks", "get-default-sink"), "sources": list("sources", "get-default-source") })
}

/// Type `text` into the focused window with whichever typing tool is installed.
fn type_text(text: &str) -> Result<()> {
    let has = |p: &str| std::env::var("PATH").unwrap_or_default().split(':').any(|d| std::path::Path::new(d).join(p).is_file());
    let mut cmd = if std::env::var_os("WAYLAND_DISPLAY").is_some() && has("wtype") {
        let mut c = Command::new("wtype");
        c.arg("--").arg(text);
        c
    } else if has("ydotool") {
        let mut c = Command::new("ydotool");
        c.args(["type", "--"]).arg(text);
        c
    } else if has("xdotool") {
        let mut c = Command::new("xdotool");
        c.args(["type", "--delay", "8", "--"]).arg(text);
        c
    } else {
        bail!("typing text needs wtype, ydotool or xdotool installed");
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null());
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("typing failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

fn spawn_detached(mut cmd: Command) -> Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    // Reap it so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

impl Manager {
    /// Called with Razer-key codes that went down / up on device `pid`.
    pub fn on_razer_keys(&self, pid: &str, pressed: &[u8], released: &[u8]) {
        for (codes, down) in [(pressed, true), (released, false)] {
            for code in codes {
                self.on_action_code(pid, *code, down);
            }
        }
    }

    /// Run the action bound to `code` (shortcut slot or Razer-key code) on `pid`.
    pub fn on_action_code(&self, pid: &str, code: u8, down: bool) {
        if pid == crate::shortcuts::MACRO_ID {
            self.run_macro_slot(code, down);
            return;
        }
        let actions = self.config.lock().unwrap().device(pid).map(|d| d.actions.clone()).unwrap_or_default();
        for a in actions.iter().filter(|a| a.code == code) {
            if let Err(e) = self.run_action(&a.action, down) {
                warn!("{pid}: an action failed: {e:#}");
                debug!("{pid}: action {}: {e:#}", a.action.label());
            }
        }
    }

    /// Make every saved action consistent: unique global slots, and each key's firmware binding
    /// sending its slot's key combination (fixes actions saved by older versions).
    pub fn sync_actions(&self) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        // Slots macros use for their run-command events belong to them.
        let macro_slots: Vec<u8> = self.library().map(|l| l.run_slots()).unwrap_or_default();
        let mut used: std::collections::HashSet<u8> = macro_slots.iter().copied().collect();
        let mut changed = false;
        let attached: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
        for di in 0..cfg.devices.len() {
            let pid = cfg.devices[di].pid.clone();
            for ai in 0..cfg.devices[di].actions.len() {
                let mut a = cfg.devices[di].actions[ai].clone();
                if a.code >= tsuba_core::config::ACTION_SLOTS && a.key != 169 {
                    // Legacy Razer-key binding: only the dial press reports those; move to a slot.
                    a.code = u8::MAX;
                }
                if a.code < tsuba_core::config::ACTION_SLOTS && !used.insert(a.code) || a.code == u8::MAX {
                    let free = (0..tsuba_core::config::ACTION_SLOTS).find(|c| !used.contains(c) && !macro_slots.contains(c) && !cfg.devices.iter().flat_map(|d| d.actions.iter()).any(|x| x.code == *c)).context("no free shortcut slots")?;
                    used.insert(free);
                    a.code = free;
                }
                if a.code < tsuba_core::config::ACTION_SLOTS && attached.contains(&pid) {
                    let want = tsuba_core::config::slot_keys(a.code).to_vec();
                    let (p, l, k) = (a.profile, a.layer, a.key);
                    let fixed = self.with_device(&pid, |dev, model| {
                        let get = if model.kind == tsuba_hid::Kind::Mouse { c::GET_SINGLE_BUTTON_ASSIGNMENT } else { c::GET_SINGLE_KEY_ASSIGNMENT };
                        let cur = decode::assignment(&dev.query(get, &[p, k, l])?);
                        if cur.as_ref().is_some_and(|x| x.function == 2 && x.data == want) {
                            return Ok(false);
                        }
                        apply::set_assignment(dev, model, &decode::Assignment { profile: p, key: k, layer: l, function: 2, data: want.clone() })?;
                        Ok(true)
                    })?;
                    if fixed {
                        debug!("{pid}: key {k} now sends shortcut slot {} for {}", a.code, a.action.label());
                    }
                }
                if cfg.devices[di].actions[ai] != a {
                    cfg.devices[di].actions[ai] = a;
                    changed = true;
                }
            }
        }
        if changed {
            cfg.save(&self.config_path)?;
            *self.config.lock().unwrap() = cfg;
        }
        Ok(())
    }

    /// Forget the action on a key (it was re-bound to something else) and its KDE shortcut.
    pub fn drop_action(&self, pid: &str, profile: u8, layer: u8, key: u8) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let Some(mut dc) = cfg.device(pid).cloned() else { return Ok(()) };
        let gone: Vec<u8> = dc.actions.iter().filter(|a| a.profile == profile && a.layer == layer && a.key == key).map(|a| a.code).collect();
        if gone.is_empty() {
            return Ok(());
        }
        dc.actions.retain(|a| !(a.profile == profile && a.layer == layer && a.key == key));
        cfg.upsert(dc);
        cfg.save(&self.config_path)?;
        *self.config.lock().unwrap() = cfg;
        crate::shortcuts::unregister(&gone.iter().map(|code| format!("{pid}-{code}")).collect::<Vec<_>>());
        info!("{pid}: key {key} layer {layer} no longer runs a software action");
        Ok(())
    }

    /// Every configured action with its device pid.
    pub fn all_actions(&self) -> Vec<(String, tsuba_core::config::ActionConfig)> {
        self.config.lock().unwrap().devices.iter().flat_map(|d| d.actions.iter().map(|a| (d.pid.clone(), a.clone()))).collect()
    }

    fn run_action(&self, action: &Action, down: bool) -> Result<()> {
        match action {
            Action::Launch { command } if down => {
                debug!("launch: {command}");
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(command);
                spawn_detached(cmd)
            }
            Action::Url { url } if down => {
                debug!("open: {url}");
                let mut cmd = Command::new("xdg-open");
                cmd.arg(url);
                spawn_detached(cmd)
            }
            Action::Type { text } if down => type_text(text),
            Action::Device { op: op @ (DeviceOp::BrightnessAllUp | DeviceOp::BrightnessAllDown | DeviceOp::StudioProfile { .. } | DeviceOp::StudioNext | DeviceOp::StudioPrevious), .. } => {
                if down {
                    self.global_op(op)?;
                }
                Ok(())
            }
            Action::Device { target, op } => self.device_op(target, op, down),
            Action::Audio { op } if down => audio(op),
            _ => Ok(()),
        }
    }

    /// Actions that aren't about one device: brightness on every attached device, Lighting
    /// Studio profiles.
    pub(crate) fn global_op(&self, op: &DeviceOp) -> Result<()> {
        self.ensure_writable()?;
        match op {
            DeviceOp::BrightnessAllUp | DeviceOp::BrightnessAllDown => {
                let up = matches!(op, DeviceOp::BrightnessAllUp);
                let pids: Vec<String> = self.devices.lock().unwrap().keys().cloned().collect();
                let mut failed = Vec::new();
                for pid in pids {
                    let r = self.ensure_writable_for(&pid).and_then(|_| self.with_device(&pid, |dev, _| brightness_step(dev, up)));
                    if let Err(e) = r {
                        failed.push(format!("{pid}: {e:#}"));
                    }
                }
                if !failed.is_empty() {
                    bail!("brightness: {}", failed.join("; "));
                }
                Ok(())
            }
            DeviceOp::StudioProfile { name } => self.apply_chroma_profile(name).map(|_| ()),
            DeviceOp::StudioNext | DeviceOp::StudioPrevious => {
                use tsuba_core::chroma_profiles as cp;
                let lib = cp::ChromaProfiles::load(&cp::path())?;
                let names: Vec<&String> = lib.profiles.iter().map(|p| &p.name).collect();
                if names.is_empty() {
                    bail!("no lighting profiles saved");
                }
                let i = names.iter().position(|n| **n == lib.active).unwrap_or(names.len() - 1);
                let n = names.len();
                let next = if matches!(op, DeviceOp::StudioNext) { (i + 1) % n } else { (i + n - 1) % n };
                self.apply_chroma_profile(names[next]).map(|_| ())
            }
            _ => bail!("not a global action"),
        }
    }

    fn device_op(&self, target: &str, op: &DeviceOp, down: bool) -> Result<()> {
        // Everything except the clutch acts on press only.
        if !down && !matches!(op, DeviceOp::DpiClutch { .. }) {
            return Ok(());
        }
        self.ensure_writable()?;
        if matches!(op, DeviceOp::EffectNext | DeviceOp::EffectPrevious | DeviceOp::Effect { .. }) {
            // Outside the device lock: the Studio runner thread needs it to finish.
            self.stop_studio(target);
        }
        self.with_device(target, |dev, model| match op {
            DeviceOp::DpiStageUp | DeviceOp::DpiStageDown => {
                let d = state::read_dpi(dev)?;
                let n = d.stages.len() as u8;
                let next = if matches!(op, DeviceOp::DpiStageUp) { (d.active_stage % n) + 1 } else if d.active_stage <= 1 { n } else { d.active_stage - 1 };
                apply::apply_dpi(dev, &DpiConfig { stages: d.stages.iter().map(|s| [s[0], s[1]]).collect(), active_stage: next, active: None })?;
                Ok(())
            }
            DeviceOp::DpiClutch { dpi } => {
                let mut held = self.actions.clutch.lock().unwrap();
                if down {
                    let cur = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
                    held.entry(target.to_string()).or_insert(cur);
                    dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(decode::Dpi { x: *dpi, y: *dpi }))?;
                } else if let Some(prev) = held.remove(target) {
                    dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(prev))?;
                }
                Ok(())
            }
            DeviceOp::ProfileNext | DeviceOp::ProfilePrevious | DeviceOp::Profile { .. } => {
                let ids = profiles::ids(dev)?;
                let cur = profiles::active(dev)?;
                let i = ids.iter().position(|x| *x == cur).unwrap_or(0);
                let to = match op {
                    DeviceOp::ProfileNext => ids[(i + 1) % ids.len()],
                    DeviceOp::ProfilePrevious => ids[(i + ids.len() - 1) % ids.len()],
                    DeviceOp::Profile { id } => *id,
                    _ => unreachable!(),
                };
                profiles::set_active(dev, to)
            }
            DeviceOp::BrightnessUp | DeviceOp::BrightnessDown => brightness_step(dev, matches!(op, DeviceOp::BrightnessUp)),
            DeviceOp::LightingToggle => {
                let mut off = self.actions.lights_off.lock().unwrap();
                if let Some(saved) = off.remove(target) {
                    for (region, level) in saved {
                        dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, region, level))?;
                    }
                } else {
                    let mut saved = Vec::new();
                    for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                        saved.push((r.id, dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0)));
                        dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, 0))?;
                    }
                    off.insert(target.to_string(), saved);
                }
                let _ = model;
                Ok(())
            }
            DeviceOp::BrightnessAllUp | DeviceOp::BrightnessAllDown | DeviceOp::StudioProfile { .. } | DeviceOp::StudioNext | DeviceOp::StudioPrevious => {
                bail!("{op:?} acts on all devices, not on {target}")
            }
            DeviceOp::EffectNext | DeviceOp::EffectPrevious | DeviceOp::Effect { .. } => {
                let effect = match op {
                    DeviceOp::Effect { effect } => effect.clone(),
                    _ => {
                        let list = quick_effects();
                        let mut idx = self.actions.effect_idx.lock().unwrap();
                        let i = idx.entry(target.to_string()).or_insert(0);
                        *i = if matches!(op, DeviceOp::EffectNext) { (*i + 1) % list.len() } else { (*i + list.len() - 1) % list.len() };
                        list[*i].clone()
                    }
                };
                apply::apply_lighting(dev, &tsuba_core::config::Lighting { effect, brightness: None })?;
                Ok(())
            }
        })
    }

    /// Every shortcut slot in use with the id its shortcut is registered under: device actions
    /// (`<pid>-<code>`) and macros' run-command events (`macro-<code>`).
    pub fn slot_entries(&self) -> Vec<crate::shortcuts::SlotEntry> {
        use crate::shortcuts::{SlotEntry, MACRO_ID};
        let mut out: Vec<SlotEntry> = self
            .all_actions()
            .into_iter()
            .filter(|(_, a)| a.code < tsuba_core::config::ACTION_SLOTS)
            .map(|(pid, a)| SlotEntry { id: format!("{pid}-{}", a.code), code: a.code, label: a.action.label() })
            .collect();
        if let Ok(lib) = self.library() {
            for m in &lib.macros {
                for e in &m.events {
                    if let tsuba_core::macros::MacroEvent::Run { command, slot: Some(s) } = e {
                        if *s < tsuba_core::config::ACTION_SLOTS && !out.iter().any(|x| x.code == *s) {
                            out.push(SlotEntry { id: format!("{MACRO_ID}-{s}"), code: *s, label: format!("Macro command: {command}") });
                        }
                    }
                }
            }
        }
        out
    }

    /// Give every run-command event of `mac` that lacks a valid slot one (the same command
    /// shares a slot across macros). Returns whether anything changed.
    pub fn alloc_run_slots(&self, mac: &mut tsuba_core::macros::Macro) -> Result<bool> {
        use tsuba_core::macros::MacroEvent;
        if !mac.events.iter().any(|e| matches!(e, MacroEvent::Run { .. })) {
            return Ok(false);
        }
        let lib = self.library()?;
        let cfg = self.config.lock().unwrap().clone();
        // slot -> the command that owns it ("" = a device action owns it).
        let mut owner: HashMap<u8, String> = HashMap::new();
        for a in cfg.devices.iter().flat_map(|d| d.actions.iter()) {
            owner.insert(a.code, String::new());
        }
        for m in lib.macros.iter().filter(|m| m.guid != mac.guid) {
            for e in &m.events {
                if let MacroEvent::Run { command, slot: Some(s) } = e {
                    owner.entry(*s).or_insert_with(|| command.clone());
                }
            }
        }
        let mut changed = false;
        for e in &mut mac.events {
            let MacroEvent::Run { command, slot } = e else { continue };
            let ok = slot.is_some_and(|s| s < tsuba_core::config::ACTION_SLOTS && owner.get(&s).is_none_or(|c| c == command));
            if !ok {
                let shared = owner.iter().find(|(s, c)| *c == command && **s < tsuba_core::config::ACTION_SLOTS).map(|(s, _)| *s);
                let new = match shared {
                    Some(s) => s,
                    None => (0..tsuba_core::config::ACTION_SLOTS).find(|s| !owner.contains_key(s)).context("no free shortcut slots left for the run-command event")?,
                };
                *slot = Some(new);
                changed = true;
            }
            owner.insert(slot.unwrap_or(0), command.clone());
        }
        Ok(changed)
    }

    /// The shortcut slot of a macro's run-command event was pressed: run the command.
    fn run_macro_slot(&self, code: u8, down: bool) {
        if !down {
            return;
        }
        let command = self.library().ok().and_then(|lib| {
            lib.macros.iter().flat_map(|m| m.events.iter()).find_map(|e| match e {
                tsuba_core::macros::MacroEvent::Run { command, slot: Some(s) } if *s == code => Some(command.clone()),
                _ => None,
            })
        });
        match command {
            Some(command) => {
                if let Err(e) = self.run_action(&Action::Launch { command: command.clone() }, true) {
                    warn!("a macro command failed: {e:#}");
                    debug!("macro command {command:?}: {e:#}");
                }
            }
            None => warn!("a macro played shortcut slot {code}, but no macro has a command for it"),
        }
    }

    /// Bind key `key` (profile/layer) on `pid` to `action`: allocate a dynamic code, write
    /// `RazerKey [code]` to the key, and save the action.
    pub fn assign_action(&self, pid: &str, profile: u8, layer: u8, key: u8, action: Action) -> Result<String> {
        if let Action::Device { target, .. } = &action {
            if target != tsuba_core::config::ALL_DEVICES && !self.devices.lock().unwrap().contains_key(target) {
                bail!("target device {target} isn't connected");
            }
        }
        let mut cfg = self.config.lock().unwrap().clone();
        let macro_slots = self.library().map(|l| l.run_slots()).unwrap_or_default();
        let code = cfg.action_code_excluding(pid, profile, layer, key, &macro_slots).context("no free shortcut slots left")?;
        let mut dc = cfg.device(pid).cloned().unwrap_or_else(|| tsuba_core::DeviceConfig { pid: pid.into(), ..Default::default() });
        let x = if code < tsuba_core::config::ACTION_SLOTS {
            decode::Assignment { profile, key, layer, function: 2, data: tsuba_core::config::slot_keys(code).to_vec() }
        } else {
            decode::Assignment { profile, key, layer, function: 17, data: vec![code] }
        };
        self.with_device(pid, |dev, model| apply::set_assignment(dev, model, &x))?;
        dc.actions.retain(|a| !(a.profile == profile && a.layer == layer && a.key == key));
        let label = action.label();
        dc.actions.push(tsuba_core::config::ActionConfig { code, profile, layer, key, action });
        cfg.upsert(dc);
        cfg.save(&self.config_path).with_context(|| format!("saving {}", self.config_path.display()))?;
        *self.config.lock().unwrap() = cfg;
        // The action is saved either way; a desktop that doesn't answer gets the list again at
        // the next start.
        if let Err(e) = crate::shortcuts::register(&self.slot_entries()) {
            warn!("registering a shortcut: {e:#}");
            debug!("registering the shortcut for {label}: {e:#}");
        }
        Ok(label)
    }
}
