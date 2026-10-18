use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use tsuba_hid::{discover, discover_events, poll_readable, Device, DeviceInfo, EventReader, Kind};
use tsuba_proto::{chroma, commands as c, decode, events, Command};

#[derive(Parser)]
#[command(version, about = "Tsuba — open-source control for Razer protocol-2.5 devices")]
struct Cli {
    /// Select a device by name substring or USB product id (e.g. "basilisk", "028d").
    #[arg(short, long, global = true)]
    device: Option<String>,
    /// Allow Set commands on device models not yet tested with Tsuba (off by default).
    #[arg(long, global = true)]
    allow_untested: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List supported devices and their control nodes.
    List,
    /// Search Tsuba's device database (all known models, attached or not): name or pid substring.
    Db { filter: Option<String> },
    /// Show what "Import from Synapse" would bring over from a Synapse 4 store (read-only; nothing
    /// is written anywhere). PATH: the store, a Windows user folder or a mounted volume; without
    /// it, mounted filesystems are searched.
    Synapse {
        path: Option<std::path::PathBuf>,
        /// Print the full plan as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Firmware, device mode and polling rate.
    Info,
    /// Active DPI and DPI stages (mice).
    Dpi,
    /// Command-dial modes (BlackWidow V4 Pro).
    Dial,
    /// Send one raw Get command: SIZE CLASS ID [ARGS...] (numbers accept 0x prefix).
    Get { size: String, class: String, id: String, args: Vec<String> },
    /// Show a key (keyboard, AT-101 position) or button (mouse) assignment.
    Key {
        key: String,
        #[arg(long, default_value = "1")]
        profile: u8,
        /// 0 = normal, 1 = Hypershift.
        #[arg(long, default_value = "0")]
        layer: u8,
    },
    /// Print hardware events (Razer keys, media keys, dial, DPI/profile changes) for N seconds.
    /// Typed keys are never read out.
    Events {
        #[arg(long, default_value = "30")]
        seconds: u64,
    },
    /// Lighting regions, supported effects, current effect and brightness.
    Lighting,
    /// Apply a hardware effect: off | static RRGGBB | breathing [RRGGBB..] | spectrum | wave [1|2]
    /// | reactive RRGGBB | starlight [RRGGBB..]. Applies to every lighting region of the device.
    SetEffect {
        effect: String,
        colours: Vec<String>,
        #[arg(long)]
        confirm: bool,
    },
    /// Stream a moving rainbow as custom frames for N seconds, then restore the previous effect (keyboard).
    FrameDemo {
        #[arg(long, default_value = "8")]
        seconds: u64,
        #[arg(long)]
        confirm: bool,
    },
    /// Light one LED (row, column of the lighting grid) white for a few seconds, then restore.
    LedTest {
        row: u8,
        col: u8,
        #[arg(long, default_value = "3")]
        seconds: u64,
        #[arg(long)]
        confirm: bool,
    },
    /// Light six grid positions in six colours at once (everything else dark), then restore.
    /// Used to map the lighting grid to physical keys.
    LedProbe {
        #[arg(long, default_value = "10")]
        seconds: u64,
        #[arg(long)]
        confirm: bool,
    },
    /// List every key/button assignment on a profile and layer.
    Keys {
        #[arg(long, default_value = "1")]
        profile: u8,
        #[arg(long, default_value = "0")]
        layer: u8,
    },
    /// Switch the device mode: normal (firmware handles remaps/macros/media keys) or driver.
    SetMode {
        mode: String,
        /// Required: acknowledges that this sends a Set command to the device.
        #[arg(long)]
        confirm: bool,
    },
    /// Diagnostic: print the raw input reports (including typed keys) every interface of the
    /// device sends, for N seconds. Only for debugging key bindings — Tsuba never does this itself.
    Keytest {
        #[arg(long, default_value = "10")]
        seconds: u64,
    },
    /// Write one key/button assignment (verified): POSITION FUNCTION [DATA...] (numbers accept 0x).
    SetKey {
        position: String,
        function: String,
        data: Vec<String>,
        #[arg(long, default_value = "1")]
        profile: u8,
        #[arg(long, default_value = "0")]
        layer: u8,
        /// Required: acknowledges that this sends a Set command to the device.
        #[arg(long)]
        confirm: bool,
    },
    /// Light frame row 0 positions in distinct colours (0 red, 1 green, 2 blue, 3 yellow, 7 white,
    /// last cyan) for a few seconds on a dark device, then restore every region's effect. Used to map
    /// a mouse's custom-frame order.
    FrameProbe {
        #[arg(long, default_value = "13")]
        cols: u8,
        #[arg(long, default_value = "10")]
        seconds: u64,
        /// Instead: light one position at a time (white, 1.5 s each) from FROM to the last column.
        #[arg(long)]
        sweep_from: Option<u8>,
        /// Required: acknowledges that this sends Set commands to the device.
        #[arg(long)]
        confirm: bool,
    },
    /// List the on-board profiles (id, name, which is active) and the PC profile library.
    Profiles,
    /// List the on-board macros (name, GUID, events) and macro memory.
    Macros,
    /// Set the active DPI (mice). Y defaults to X.
    SetDpi {
        x: u16,
        y: Option<u16>,
        /// Required: acknowledges that this sends a Set command to the device.
        #[arg(long)]
        confirm: bool,
    },
    /// Select the active DPI stage (mice) without changing the current active DPI.
    SetDpiStage {
        stage: u8,
        /// Required: acknowledges that this sends Set commands to the device.
        #[arg(long)]
        confirm: bool,
    },
    /// Write-path self-test: change a harmless setting, verify, restore, verify.
    WriteSelftest {
        /// Required: acknowledges that this sends Set commands to the device.
        #[arg(long)]
        confirm: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Cmd::Db { filter } = &cli.cmd {
        return db_search(filter.as_deref());
    }
    if let Cmd::Synapse { path, json } = &cli.cmd {
        return synapse(path.as_deref(), *json);
    }
    if let Cmd::Keytest { seconds } = cli.cmd {
        return keytest(cli.device.as_deref(), seconds);
    }
    if let Cmd::Events { seconds } = cli.cmd {
        return watch_events(select(discover_events(), cli.device.as_deref()), seconds);
    }
    let devices = select(discover(), cli.device.as_deref());
    if devices.is_empty() {
        bail!("no supported Razer device found (is the hidraw node readable? see udev uaccess rule)");
    }
    for info in &devices {
        if let Err(e) = run(&cli.cmd, info, cli.allow_untested) {
            eprintln!("{}: {e:#}", info.model.name);
        }
    }
    Ok(())
}

fn select(all: Vec<DeviceInfo>, filter: Option<&str>) -> Vec<DeviceInfo> {
    let Some(f) = filter.map(str::to_lowercase) else { return all };
    all.into_iter()
        .filter(|d| d.model.name.to_lowercase().contains(&f) || format!("{:04x}", d.model.pid) == f)
        .collect()
}

fn db_search(filter: Option<&str>) -> Result<()> {
    let f = filter.unwrap_or("").to_lowercase();
    for s in tsuba_hid::db::all() {
        if !f.is_empty() && !s.name.to_lowercase().contains(&f) && !format!("{:04x}", s.pid).contains(&f) {
            continue;
        }
        let status = if !s.supported { "listed only" } else if s.tested { "tested" } else { "untested" };
        let matrix = s.matrix.map(|[r, c]| format!("{r}x{c}")).unwrap_or_else(|| "-".into());
        println!("{:04x}  {:<44} {:<15} {:<11} txid 0x{:02x} if {}  matrix {matrix}", s.pid, s.name, s.kind.as_str(), status, s.txid, s.iface);
    }
    Ok(())
}

fn run(cmd: &Cmd, info: &DeviceInfo, allow_untested: bool) -> Result<()> {
    let m = info.model;
    if matches!(cmd, Cmd::List) {
        let note = if m.tested() { "" } else { "  [untested: read-only unless --allow-untested]" };
        println!("{:04x}  {:<26} {}  (interface {}){note}", m.pid, m.name, info.hidraw.display(), m.iface);
        return Ok(());
    }
    let mut dev = Device::open(info)?;
    dev.allow_untested(allow_untested);
    let dev = dev;
    println!("== {} ({})", m.name, info.hidraw.display());
    match cmd {
        Cmd::List | Cmd::Db { .. } | Cmd::Synapse { .. } | Cmd::Events { .. } | Cmd::Keytest { .. } => unreachable!(),
        Cmd::Info => {
            let fw = decode::firmware_version(&dev.query(c::GET_FIRMWARE_VERSION, &[])?).context("firmware")?;
            println!("firmware      {}.{:02}", fw.major, fw.minor);
            let mode = decode::device_mode(&dev.query(c::GET_DEVICE_MODE, &[])?).context("mode")?;
            println!("device mode   {mode:?}");
            match dev.query(c::GET_POLLING_PERIOD, &[]) {
                Ok(a) => println!("polling rate  {} Hz", decode::polling_hz(&a).context("polling")?),
                Err(e) => println!("polling rate  n/a ({e})"),
            }
        }
        Cmd::Dpi => {
            if m.kind != Kind::Mouse {
                bail!("not a mouse");
            }
            let a = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
            println!("active        {} x {}", a.x, a.y);
            let s = decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[decode::VARSTORE])?).context("stages")?;
            for st in &s.stages {
                let mark = if st.id == s.active { "*" } else { " " };
                println!("stage {}{mark}      {} x {}", st.id, st.dpi.x, st.dpi.y);
            }
        }
        Cmd::Dial => {
            if !m.spec.features.dial {
                bail!("no command dial");
            }
            let cur = decode::current_knob_mode(&dev.query(c::GET_CURRENT_KNOB_MODE, &decode::KNOB_LED)?)
                .context("current mode")?;
            let modes = decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?)
                .context("configuration")?;
            for k in modes {
                let mark = if k == cur { "*" } else { " " };
                let state = if k.enabled { "on " } else { "off" };
                let [r, g, b] = k.rgb;
                println!("slot {}{mark} {state} #{r:02x}{g:02x}{b:02x}  {:?}", k.stage, k.function);
            }
        }
        Cmd::Key { key, profile, layer } => {
            let get = if m.kind == Kind::Mouse { c::GET_SINGLE_BUTTON_ASSIGNMENT } else { c::GET_SINGLE_KEY_ASSIGNMENT };
            let a = decode::assignment(&dev.query(get, &[*profile, num(key)?, *layer])?).context("assignment")?;
            println!("profile {} key {} layer {}: {}", a.profile, a.key, a.layer, decode::describe(a.function, &a.data));
        }
        Cmd::Lighting => {
            let regions = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?);
            for r in &regions {
                let effects = dev
                    .query(chroma::GET_EFFECT_LIST, &[r.id])
                    .ok()
                    .and_then(|a| chroma::effect_list(&a))
                    .map(|(_, e)| format!("{e:?}"))
                    .unwrap_or_else(|| "?".into());
                let cur = dev
                    .query(chroma::GET_EFFECT, &[chroma::STORE, r.id])
                    .ok()
                    .and_then(|a| chroma::effect_state(&a))
                    .map(|s| format!("{:?} {:02x?}", s.effect, s.colours))
                    .unwrap_or_else(|| "?".into());
                let bright = dev
                    .query(chroma::GET_BRIGHTNESS, &[chroma::STORE, r.id])
                    .map(|a| format!("{}%", (*a.get(2).unwrap_or(&0) as u32 * 100 + 127) / 255))
                    .unwrap_or_else(|_| "?".into());
                println!("region {:>2} {:<12} {}x{} @{}Hz  now: {cur}  brightness {bright}", r.id, chroma::region_name(r.id), r.rows, r.cols, r.update_rate);
                println!("           supports: {effects}");
            }
        }
        Cmd::SetEffect { effect, colours, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let cs = colours.iter().map(|h| rgb(h)).collect::<Result<Vec<_>>>()?;
            let e = match effect.as_str() {
                "off" => chroma::Effect::Off,
                "static" => chroma::Effect::Static(*cs.first().context("static needs a colour")?),
                "breathing" => chroma::Effect::Breathing(cs),
                "spectrum" => chroma::Effect::Spectrum,
                "wave" => chroma::Effect::Wave { direction: colours.first().and_then(|d| d.parse().ok()).unwrap_or(1), speed: 0x28 },
                "reactive" => chroma::Effect::Reactive { speed: 2, colour: *cs.first().context("reactive needs a colour")? },
                "starlight" => chroma::Effect::Starlight { speed: 2, colours: cs },
                other => bail!("unknown effect {other}"),
            };
            let mut dev = dev;
            dev.allow_writes(true);
            let n = match &e {
                chroma::Effect::Static(_) | chroma::Effect::Reactive { .. } => 1,
                chroma::Effect::Breathing(v) | chroma::Effect::Starlight { colours: v, .. } => v.len(),
                _ => 0,
            };
            for r in chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?) {
                dev.query(chroma::set_effect(n), &chroma::encode_effect(chroma::STORE, r.id, &e))?;
                println!("region {} ({}): {e:?}", r.id, chroma::region_name(r.id));
            }
        }
        Cmd::FrameDemo { seconds, confirm } => {
            if !m.kind.has_keys() || !m.spec.per_led() {
                bail!("frame demo targets per-key keyboards");
            }
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            frame_demo(&dev, *seconds)?;
        }
        Cmd::LedTest { row, col, seconds, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            led_test(&dev, *row, *col, *seconds)?;
        }
        Cmd::LedProbe { seconds, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            let probes: [((u8, u8), chroma::Rgb, &str); 6] = [
                ((0, 0), [255, 0, 0], "RED"),
                ((0, 1), [0, 255, 0], "GREEN"),
                ((0, 2), [0, 0, 255], "BLUE"),
                ((1, 0), [255, 255, 255], "WHITE"),
                ((1, 1), [255, 200, 0], "YELLOW"),
                ((5, 0), [0, 255, 255], "CYAN"),
            ];
            for ((r, c), _, name) in &probes {
                println!("  row {r} col {c} = {name}");
            }
            led_pattern(&dev, &probes.iter().map(|(p, c, _)| (*p, *c)).collect::<Vec<_>>(), *seconds)?;
        }
        Cmd::Keys { profile, layer } => {
            let get = if m.kind == Kind::Mouse { c::GET_SINGLE_BUTTON_ASSIGNMENT } else { c::GET_SINGLE_KEY_ASSIGNMENT };
            for k in 1..=255u8 {
                let Ok(raw) = dev.query(get, &[*profile, k, *layer]) else { continue };
                if let Some(a) = decode::assignment(&raw) {
                    println!("{:>3}  {}", a.key, decode::describe(a.function, &a.data));
                }
            }
        }
        Cmd::SetMode { mode, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            let changed = tsuba_core::apply::set_mode(&dev, mode)?;
            println!("device mode {mode}{} (verified)", if changed { "" } else { " (already set)" });
        }
        Cmd::SetKey { position, function, data, profile, layer, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let num = |t: &str| -> Result<u8> {
                Ok(match t.strip_prefix("0x") {
                    Some(h) => u8::from_str_radix(h, 16)?,
                    None => t.parse()?,
                })
            };
            let x = decode::Assignment { profile: *profile, key: num(position)?, layer: *layer, function: num(function)?, data: data.iter().map(|d| num(d)).collect::<Result<_>>()? };
            let mut dev = dev;
            dev.allow_writes(true);
            let back = tsuba_core::apply::set_assignment(&dev, &m, &x)?;
            println!("key {} layer {} = {} (verified)", x.key, x.layer, decode::describe(back.function, &back.data));
        }
        Cmd::FrameProbe { cols, seconds, sweep_from, confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            let n = *cols as usize;
            let mut leds = vec![[0u8; 3]; n];
            for (i, c) in [(0, [255, 0, 0]), (1, [0, 255, 0]), (2, [0, 0, 255]), (3, [255, 200, 0]), (7, [255, 255, 255]), (n - 1, [0, 255, 255])] {
                if i < n {
                    leds[i] = c;
                }
            }
            let saved: Vec<_> = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?)
                .into_iter()
                .filter_map(|r| chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, r.id]).ok()?))
                .collect();
            let mut first = true;
            let mut show = |leds: &[chroma::Rgb], ms: u64| -> Result<()> {
                let start = std::time::Instant::now();
                while start.elapsed().as_millis() < ms as u128 {
                    dev.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(0, 0, leds))?;
                    if first {
                        dev.query(chroma::set_effect(0), &chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame))?;
                        first = false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Ok(())
            };
            if let Some(from) = sweep_from {
                for i in *from as usize..n {
                    let mut one = vec![[0u8; 3]; n];
                    one[i] = [255, 255, 255];
                    println!("position {i}");
                    show(&one, 1500)?;
                }
            } else {
                println!("row 0: 0 RED, 1 GREEN, 2 BLUE, 3 YELLOW, 7 WHITE, {} CYAN — look where each colour lands", n - 1);
                show(&leds, seconds * 1000)?;
            }
            for st in &saved {
                dev.query(chroma::set_effect(st.colours.len()), &chroma::encode_effect_state(st))?;
            }
            println!("restored {} region(s)", saved.len());
        }
        Cmd::Profiles => {
            let p = tsuba_core::profiles::list(&dev)?;
            let lib = tsuba_core::library::DeviceLibrary::load(&tsuba_core::library::root(), &tsuba_core::pid_key(m.pid))?;
            println!("On-board: {} of {} profile slots used", p.profiles.len(), p.max);
            for x in &p.profiles {
                let holds = lib.holder(x.id).map(|h| format!("  ← library profile “{}”{}", h.name, if h.pinned_slot == Some(x.id) { " (pinned)" } else { "" })).unwrap_or_default();
                println!("{} {}  {}{holds}", if x.id == p.active { "*" } else { " " }, x.id, if x.name.is_empty() { "(unnamed)" } else { &x.name });
            }
            let active = lib.holder(p.active).map(|h| h.id.clone());
            println!("Library: {} profile(s) in {}", lib.profiles.len(), lib.dir.display());
            for l in &lib.profiles {
                let pin = l.pinned_slot.map(|s| format!("  [slot {s}]")).unwrap_or_default();
                let syn = if l.synapse_guid.is_some() { "  (from Synapse)" } else { "" };
                println!("{} {}{pin}{syn}", if active.as_deref() == Some(l.id.as_str()) { "*" } else { " " }, l.name);
            }
            for e in &lib.errors {
                println!("  unreadable: {e}");
            }
        }
        Cmd::Macros => {
            let d = tsuba_core::macros::read_device(&dev)?;
            println!("memory: {} of {} bytes free ({} recyclable), up to {} macros", d.available, d.max_memory, d.recyclable, d.max_id);
            for mac in &d.macros {
                println!("0x{:04x}  {:<20} {} events, {} bytes, guid {}", mac.id, mac.name, mac.events.len(), mac.size, mac.guid);
                for e in &mac.events {
                    println!("        {}", serde_json::to_string(e)?);
                }
            }
        }
        Cmd::SetDpi { x, y, confirm } => {
            if m.kind != Kind::Mouse {
                bail!("not a mouse");
            }
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let want = decode::Dpi { x: *x, y: y.unwrap_or(*x) };
            let mut dev = dev;
            dev.allow_writes(true);
            dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(want))?;
            let got = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
            if got != want {
                bail!("read-back mismatch: wrote {want:?}, got {got:?}");
            }
            println!("active DPI set to {} x {} (verified)", got.x, got.y);
        }
        Cmd::SetDpiStage { stage, confirm } => {
            if m.kind != Kind::Mouse {
                bail!("not a mouse");
            }
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            let active = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
            let mut stages = decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[decode::VARSTORE])?).context("stages")?;
            if !stages.stages.iter().any(|s| s.id == *stage) {
                bail!("no DPI stage {stage}");
            }
            stages.active = *stage;
            dev.query(c::set_dpi_stages(stages.stages.len()), &decode::encode_dpi_stages(&stages))?;
            // Writing the stage table switches the active DPI; put the previous value back.
            dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(active))?;
            let got_stages = decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[decode::VARSTORE])?).context("stages")?;
            let got = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
            if got_stages != stages || got != active {
                bail!("read-back mismatch: stage {} / {:?}", got_stages.active, got);
            }
            println!("active stage {stage}, active DPI kept at {} x {} (verified)", got.x, got.y);
        }
        Cmd::WriteSelftest { confirm } => {
            if !confirm {
                bail!("pass --confirm to send Set commands");
            }
            let mut dev = dev;
            dev.allow_writes(true);
            match m.kind {
                Kind::Keyboard if m.spec.features.dial => {
                    selftest_knob(&dev)?;
                    selftest_hypershift_key(&dev)?;
                }
                Kind::Mouse if m.spec.features.dpi_stages => selftest_dpi(&dev)?,
                k => bail!("no write self-test for this {}", k.as_str()),
            }
        }
        Cmd::Get { size, class, id, args } => {
            let cmd = Command::new(num(size)?, num(class)?, num(id)?);
            if !cmd.is_get() {
                bail!("raw access is read-only: id 0x{:02x} is a Set (bit 7 clear)", cmd.id);
            }
            let args = args.iter().map(|a| num(a)).collect::<Result<Vec<u8>>>()?;
            let names: Vec<_> = c::names(cmd.class, cmd.id).collect();
            let reply = dev.query(cmd, &args)?;
            println!("{}  {:02x?}", if names.is_empty() { "(unknown)".into() } else { names.join(" | ") }, reply);
        }
    }
    Ok(())
}

fn watch_events(devices: Vec<DeviceInfo>, seconds: u64) -> Result<()> {
    if devices.is_empty() {
        bail!("no event interface found");
    }
    let mut readers = devices.iter().map(EventReader::open).collect::<Result<Vec<_>, _>>()?;
    for r in &readers {
        println!("listening: {} ({})", r.model.name, r.path.display());
    }
    println!("press Razer/macro keys, turn or press the dial, use DPI/profile buttons… ({seconds}s)");
    let start = std::time::Instant::now();
    let fds: Vec<_> = readers.iter().map(EventReader::fd).collect();
    let mut buf = [0u8; 64];
    while start.elapsed().as_secs() < seconds {
        for i in poll_readable(&fds, 200) {
            let n = readers[i].read_report(&mut buf)?;
            let Some(ev) = events::parse(&buf[..n]) else { continue };
            let t = start.elapsed().as_secs_f32();
            let who = readers[i].model.name;
            match ev {
                events::Event::RazerKeys(k) if k.is_empty() => println!("{t:7.2}  {who}: Razer keys released"),
                events::Event::RazerKeys(k) => {
                    let names: Vec<_> = k.iter().map(|c| format!("{} ({c})", events::razer_key_name(*c))).collect();
                    println!("{t:7.2}  {who}: {}", names.join(", "));
                }
                events::Event::Media(u) => println!("{t:7.2}  {who}: media {u:04x?}"),
                events::Event::Power(p) => println!("{t:7.2}  {who}: power {p:02x?}"),
                events::Event::Notification { id, data } => {
                    let name = events::notification_name(id).unwrap_or("notification");
                    println!("{t:7.2}  {who}: {name} (id {id}) {:02x?}", &data[..data.len().min(12)]);
                }
            }
        }
    }
    Ok(())
}

/// Write `test`, verify it reads back, then restore `orig` and verify again.
/// Restores on every failure path after the first write.
fn roundtrip<T: PartialEq + std::fmt::Debug>(
    what: &str,
    orig: &T,
    test: &T,
    write: impl Fn(&T) -> Result<()>,
    read: impl Fn() -> Result<T>,
) -> Result<()> {
    let checked = write(test).and_then(|_| {
        let got = read()?;
        if &got != test {
            bail!("read-back mismatch: wrote {test:?}, got {got:?}");
        }
        Ok(())
    });
    let restored = write(orig).and_then(|_| {
        let got = read()?;
        if &got != orig {
            bail!("RESTORE MISMATCH: expected {orig:?}, got {got:?}");
        }
        Ok(())
    });
    match (checked, restored) {
        (Ok(()), Ok(())) => {
            println!("PASS  {what}: write, read-back and restore verified");
            Ok(())
        }
        (Err(e), Ok(())) => bail!("FAIL  {what}: {e:#} (original restored)"),
        (_, Err(e)) => bail!("FAIL  {what}: {e:#}"),
    }
}

fn selftest_knob(dev: &Device) -> Result<()> {
    let read = || -> Result<Vec<decode::KnobMode>> {
        decode::knob_configuration(&dev.query(c::GET_KNOB_CONFIGURATION, &decode::KNOB_LED)?).context("decode")
    };
    let orig = read()?;
    // Change the colour of the last *disabled* slot only.
    let slot = orig.iter().rposition(|k| !k.enabled).context("no disabled dial slot to test with")?;
    let mut test = orig.clone();
    test[slot].rgb = [test[slot].rgb[2], test[slot].rgb[0], test[slot].rgb[1]];
    let write = |v: &Vec<decode::KnobMode>| -> Result<()> {
        dev.query(c::SET_KNOB_CONFIGURATION, &decode::encode_knob_configuration(v)).map(|_| ()).map_err(Into::into)
    };
    roundtrip(&format!("dial slot {slot} colour (disabled slot)"), &orig, &test, write, read)
}

fn selftest_hypershift_key(dev: &Device) -> Result<()> {
    const KEY: u8 = 4; // AT-101 position 4 = "3"
    let read = || -> Result<decode::Assignment> {
        decode::assignment(&dev.query(c::GET_SINGLE_KEY_ASSIGNMENT, &[1, KEY, 1])?).context("decode")
    };
    let orig = read()?;
    // Hypershift layer only: map to F13 (HID 0x68), which nothing uses by default.
    let test = decode::Assignment { function: 0x02, data: vec![0x00, 0x68], ..orig.clone() };
    let write = |v: &decode::Assignment| -> Result<()> {
        dev.query(c::SET_SINGLE_KEY_ASSIGNMENT, &decode::encode_assignment(v)).map(|_| ()).map_err(Into::into)
    };
    roundtrip("key \"3\" on the shift layer -> F13", &orig, &test, write, read)
}

fn selftest_dpi(dev: &Device) -> Result<()> {
    // Writing the stage table also switches the active DPI to the active stage,
    // so capture the active DPI first and put it back afterwards.
    let active = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
    let stages = selftest_dpi_stages(dev);
    dev.query(c::SET_ACTIVE_DPI, &decode::encode_active_dpi(active))?;
    let now = decode::active_dpi(&dev.query(c::GET_ACTIVE_DPI, &[decode::VARSTORE])?).context("dpi")?;
    if now != active {
        bail!("FAIL  active DPI not restored: expected {active:?}, got {now:?}");
    }
    println!("PASS  active DPI restored to {} x {}", now.x, now.y);
    stages
}

fn selftest_dpi_stages(dev: &Device) -> Result<()> {
    let read = || -> Result<decode::DpiStages> {
        decode::dpi_stages(&dev.query(c::GET_DPI_STAGES, &[decode::VARSTORE])?).context("decode")
    };
    let orig = read()?;
    let mut test = orig.clone();
    let last = test.stages.last_mut().context("no DPI stages")?;
    last.dpi.x += 100;
    last.dpi.y += 100;
    let write = |v: &decode::DpiStages| -> Result<()> {
        dev.query(c::set_dpi_stages(v.stages.len()), &decode::encode_dpi_stages(v)).map(|_| ()).map_err(Into::into)
    };
    roundtrip("last DPI stage +100", &orig, &test, write, read)
}

fn frame_demo(dev: &Device, seconds: u64) -> Result<()> {
    let region = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?)
        .into_iter()
        .find(|r| r.id == 5)
        .context("no backlight region")?;
    let saved = chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, region.id])?).context("effect")?;
    println!("saved current effect: {:?}; streaming {}x{} frames for {seconds}s", saved.effect, region.rows, region.cols);
    let period = std::time::Duration::from_millis(1000 / region.update_rate.max(1) as u64);
    let start = std::time::Instant::now();
    let mut frames = 0u32;
    let mut result = Ok(());
    while start.elapsed().as_secs() < seconds {
        let t = start.elapsed().as_secs_f32();
        for row in 0..region.rows {
            let leds: Vec<chroma::Rgb> = (0..region.cols).map(|col| hue(t * 0.25 + col as f32 / region.cols as f32 + row as f32 * 0.02)).collect();
            if let Err(e) = dev.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(row, 0, &leds)) {
                result = Err(e);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if result.is_err() {
            break;
        }
        if frames == 0 {
            dev.query(chroma::set_effect(0), &chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame))?;
        }
        frames += 1;
        std::thread::sleep(period);
    }
    // Always put the previous effect back.
    dev.query(chroma::set_effect(saved.colours.len()), &chroma::encode_effect_state(&saved))?;
    let now = chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, region.id])?).context("effect")?;
    println!("{frames} frames sent; restored effect: {:?} (matches: {})", now.effect, now == saved);
    result.map_err(Into::into)
}

fn led_test(dev: &Device, row: u8, col: u8, seconds: u64) -> Result<()> {
    led_pattern(dev, &[((row, col), [255, 255, 255])], seconds)
}

/// Show only the given (row, col) → colour LEDs for `seconds`, everything else dark, then restore.
fn led_pattern(dev: &Device, lit: &[((u8, u8), chroma::Rgb)], seconds: u64) -> Result<()> {
    let regions = chroma::regions(&dev.query(chroma::GET_REGION_LIST, &[])?);
    let region = regions.iter().max_by_key(|r| r.rows as u32 * r.cols as u32).context("no lighting region")?;
    if let Some(((r, c), _)) = lit.iter().find(|((r, c), _)| *r >= region.rows || *c >= region.cols) {
        bail!("({r}, {c}) is outside the {}x{} grid", region.rows, region.cols);
    }
    let saved = chroma::effect_state(&dev.query(chroma::GET_EFFECT, &[chroma::STORE, region.id])?).context("effect")?;
    let start = std::time::Instant::now();
    let mut first = true;
    // Re-send the frame a few times a second so nothing else overwrites it.
    while start.elapsed().as_secs() < seconds {
        for r in 0..region.rows {
            let leds: Vec<chroma::Rgb> = (0..region.cols)
                .map(|c| lit.iter().find(|(p, _)| *p == (r, c)).map_or([0, 0, 0], |(_, rgb)| *rgb))
                .collect();
            dev.send(chroma::set_frame(leds.len()), &chroma::encode_frame_row(r, 0, &leds))?;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if first {
            dev.query(chroma::set_effect(0), &chroma::encode_effect(chroma::NOSTORE, 0, &chroma::Effect::CustomFrame))?;
            first = false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    dev.query(chroma::set_effect(saved.colours.len()), &chroma::encode_effect_state(&saved))?;
    println!("showed {} LED(s) for {seconds}s on a dark keyboard; restored {:?}", lit.len(), saved.effect);
    Ok(())
}

/// Rainbow colour for a position on the hue circle (0..1 wraps).
fn hue(h: f32) -> chroma::Rgb {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

fn rgb(h: &str) -> Result<chroma::Rgb> {
    let h = h.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6).with_context(|| format!("not a RRGGBB colour: {h}"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn num(s: &str) -> Result<u8> {
    let v = match s.strip_prefix("0x") {
        Some(h) => u8::from_str_radix(h, 16),
        None => s.parse(),
    };
    v.with_context(|| format!("not a byte: {s}"))
}

/// Raw input reports from every hidraw node of the selected Razer device(s), for `seconds`.
fn keytest(filter: Option<&str>, seconds: u64) -> Result<()> {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    let want = filter.map(str::to_lowercase);
    let mut nodes = Vec::new();
    for e in std::fs::read_dir("/sys/class/hidraw")? {
        let e = e?;
        let dev = std::fs::canonicalize(e.path().join("device"))?;
        let name = dev.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // e.g. 0003:1532:028D.0024
        let Some(pid) = name.split(':').nth(2).and_then(|x| x.split('.').next()).map(str::to_lowercase) else { continue };
        if !name.to_lowercase().contains(":1532:") || want.as_ref().is_some_and(|w| *w != pid) {
            continue;
        }
        let path = std::path::Path::new("/dev").join(e.file_name());
        match std::fs::File::open(&path) {
            Ok(f) => nodes.push((path, name, f)),
            Err(err) => eprintln!("{}: {err}", path.display()),
        }
    }
    println!("listening on {} interface(s) for {seconds}s — press the keys to test (typing elsewhere will show too)", nodes.len());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut buf = [0u8; 64];
    while std::time::Instant::now() < end {
        let fds: Vec<_> = nodes.iter().map(|n| n.2.as_raw_fd()).collect();
        for i in tsuba_hid::poll_readable(&fds, 200) {
            let (path, name, f) = &mut nodes[i];
            if let Ok(n) = f.read(&mut buf) {
                let hex: Vec<String> = buf[..n].iter().map(|b| format!("{b:02x}")).collect();
                println!("{} {:<22} {}", path.display(), name, hex.join(" "));
            }
        }
    }
    Ok(())
}

fn synapse(path: Option<&std::path::Path>, json: bool) -> Result<()> {
    use tsuba_core::synapse_import as si;
    let stores = si::find_synapse_stores(&path.map(|p| vec![p.to_path_buf()]).unwrap_or_default());
    let store = match path {
        Some(p) => stores.iter().find(|s| s.starts_with(p) || p.starts_with(s.as_path())).cloned().with_context(|| format!("no Synapse store at or below {}", p.display()))?,
        None => stores.first().cloned().context("no Synapse store found on a mounted filesystem; pass its path")?,
    };
    let t = std::time::Instant::now();
    let plan = si::scan(&store)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    println!("Synapse store: {} (read in {} ms)", store.display(), t.elapsed().as_millis());
    for d in &plan.devices {
        println!("\n{} [{}]{}", d.name, d.pid.as_deref().unwrap_or("not in Tsuba's database"), if d.note.is_empty() { String::new() } else { format!(" — {}", d.note) });
        for p in &d.profiles {
            println!("  profile {:?}{}: {} item(s), {} warning(s)", p.name, if p.active { " (active)" } else { "" }, p.summary.len() + p.device_wide.summary.len(), p.warnings.len());
            p.summary.iter().chain(&p.device_wide.summary).for_each(|s| println!("    + {s}"));
            p.warnings.iter().for_each(|w| println!("    ! {w}"));
        }
    }
    println!("\nMacros:");
    for m in &plan.macros {
        println!("  {:?}: {} event(s){}", m.name, m.events, if m.warnings.is_empty() { String::new() } else { format!(" — {}", m.warnings.join("; ")) });
    }
    println!("\nLighting Studio effects:");
    for l in &plan.lighting {
        println!("  {:?}: {} layer(s) on {}", l.name, l.layers, if l.devices.is_empty() { "no device".into() } else { l.devices.join(", ") });
        l.warnings.iter().for_each(|w| println!("    ! {w}"));
    }
    for e in &plan.errors {
        println!("unreadable: {e}");
    }
    Ok(())
}
