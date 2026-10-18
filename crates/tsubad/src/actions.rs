//! Software actions bound to keys through dynamic Razer-key codes (Synapse's Launch and
//! Inter-device functions). The firmware reports the code on press and release; tsubad runs
//! the action configured for it.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use tracing::{info, warn};
use tsuba_core::config::{Action, DeviceOp, DpiConfig};
use tsuba_core::{apply, percent_to_level, profiles, state};
use tsuba_proto::{chroma, commands as c, decode};

use crate::manager::Manager;

/// Per-target state for actions that undo on release (clutch) or toggle (lighting).
#[derive(Default)]
pub struct ActionState {
    clutch: Mutex<HashMap<String, decode::Dpi>>,
    lights_off: Mutex<HashMap<String, Vec<(u8, u8)>>>,
    effect_idx: Mutex<HashMap<String, usize>>,
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
        let actions = self.config.lock().unwrap().device(pid).map(|d| d.actions.clone()).unwrap_or_default();
        for a in actions.iter().filter(|a| a.code == code) {
            if let Err(e) = self.run_action(&a.action, down) {
                warn!("{pid}: action {}: {e:#}", a.action.label());
            }
        }
    }

    /// Make every saved action consistent: unique global slots, and each key's firmware binding
    /// sending its slot's key combination (fixes actions saved by older versions).
    pub fn sync_actions(&self) -> Result<()> {
        let mut cfg = self.config.lock().unwrap().clone();
        let mut used = std::collections::HashSet::new();
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
                    let free = (0..tsuba_core::config::ACTION_SLOTS).find(|c| !used.contains(c) && !cfg.devices.iter().flat_map(|d| d.actions.iter()).any(|x| x.code == *c)).context("no free shortcut slots")?;
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
                        info!("{pid}: key {k} now sends shortcut slot {} for {}", a.code, a.action.label());
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
        if let Some(conn) = self.shortcut_conn.lock().unwrap().as_ref() {
            for code in gone {
                let _ = conn.call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "unregister", &("tsuba", format!("{pid}-{code}")));
            }
        }
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
                info!("launch: {command}");
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(command);
                spawn_detached(cmd)
            }
            Action::Url { url } if down => {
                info!("open: {url}");
                let mut cmd = Command::new("xdg-open");
                cmd.arg(url);
                spawn_detached(cmd)
            }
            Action::Device { target, op } => self.device_op(target, op, down),
            Action::Audio { op } if down => audio(op),
            _ => Ok(()),
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
            DeviceOp::BrightnessUp | DeviceOp::BrightnessDown => {
                for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                    let level = dev.query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])?.get(2).copied().unwrap_or(0) as i32;
                    let step = percent_to_level(10) as i32;
                    let new = if matches!(op, DeviceOp::BrightnessUp) { level + step } else { level - step }.clamp(0, 255) as u8;
                    dev.query(chroma::SET_BRIGHTNESS, &chroma::encode_brightness(chroma::STORE, r.id, new))?;
                }
                Ok(())
            }
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

    /// Bind key `key` (profile/layer) on `pid` to `action`: allocate a dynamic code, write
    /// `RazerKey [code]` to the key, and save the action.
    pub fn assign_action(&self, pid: &str, profile: u8, layer: u8, key: u8, action: Action) -> Result<String> {
        if let Action::Device { target, .. } = &action {
            if !self.devices.lock().unwrap().contains_key(target) {
                bail!("target device {target} isn't connected");
            }
        }
        let mut cfg = self.config.lock().unwrap().clone();
        let code = cfg.action_code(pid, profile, layer, key).context("no free shortcut slots left")?;
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
        if let Some(conn) = self.shortcut_conn.lock().unwrap().as_ref() {
            crate::shortcuts::register(conn, &self.all_actions())?;
        }
        Ok(label)
    }
}
