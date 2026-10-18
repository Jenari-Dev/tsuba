//! tsubad — the Tsuba device daemon. Applies saved settings when devices
//! appear, forwards hardware events, and serves the D-Bus API.

mod game_autoinstall;
mod actions;
mod ambient;
mod applinks;
mod audio;
mod cache;
mod dbus;
mod dbus_device;
mod game_http;
mod game_lighting;
mod gamemode;
mod idle;
mod library;
mod manager;
mod shortcuts;
mod studio_runner;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::mpsc;
use tracing::{info, warn};
use tsuba_core::Config;

#[derive(Parser)]
#[command(version, about = "Tsuba device daemon")]
struct Args {
    /// Never write to devices (state, events and D-Bus reads still work).
    #[arg(long)]
    read_only: bool,
    /// Config file (default: ~/.config/tsuba/config.toml).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Snapshot the attached devices into the config file and exit (read-only).
    #[arg(long)]
    import: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let args = Args::parse();
    let config_path = args.config.unwrap_or_else(Config::default_path);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mgr = Arc::new(manager::Manager::new(args.read_only || args.import, config_path.clone(), tx)?);

    if args.import {
        mgr.refresh();
        let pids = mgr.save_from_devices()?;
        println!("saved {} device(s) to {}: {}", pids.len(), config_path.display(), pids.join(", "));
        return Ok(());
    }

    // Game lighting (REST server for games that support Razer Chroma). The port can be moved
    // for tests; a read-only daemon only opens it when the port is given explicitly.
    let port_override = std::env::var("TSUBA_GAME_LIGHTING_PORT").ok().and_then(|p| p.parse::<u16>().ok());
    let (game_tx, mut game_rx) = mpsc::unbounded_channel::<String>();
    let game = game_lighting::GameLighting::new(
        Arc::new(game_lighting::ManagerSink(mgr.clone())),
        port_override.unwrap_or(game_http::PORT),
        Box::new(move |s| {
            let _ = game_tx.send(s);
        }),
    );
    game.listen_allowed.store(!mgr.read_only || port_override.is_some(), std::sync::atomic::Ordering::Relaxed);
    game.underglow.store(mgr.config.lock().unwrap().game_lighting_underglow_enabled(), std::sync::atomic::Ordering::Relaxed);
    game.web.store(mgr.config.lock().unwrap().game_lighting_web_enabled(), std::sync::atomic::Ordering::Relaxed);

    let conn = zbus::connection::Builder::session()?
        .name(dbus::BUS_NAME)?
        .serve_at(dbus::PATH, dbus::Tsuba { mgr: mgr.clone(), game: game.clone() })?
        .serve_at(dbus::PATH, dbus_device::DeviceIface { mgr: mgr.clone() })?
        .build()
        .await
        .context("D-Bus: is another tsubad already running?")?;
    let iface = conn.object_server().interface::<_, dbus::Tsuba>(dbus::PATH).await?;
    info!("serving {} on the session bus{}", dbus::BUS_NAME, if mgr.read_only { " (read-only)" } else { "" });

    // Hotplug: poll discovery; cheap (a sysfs directory scan).
    let hot = {
        let mgr = mgr.clone();
        let iface = iface.clone();
        tokio::spawn(async move {
            loop {
                let m = mgr.clone();
                let (added, removed) = tokio::task::spawn_blocking(move || m.refresh()).await.unwrap_or_default();
                for pid in added {
                    mgr.request_warm(&pid);
                    // First start with the profile library: copy the on-board profiles into it.
                    let m = mgr.clone();
                    let p = pid.clone();
                    std::thread::spawn(move || {
                        // Game Mode keys a profile switch couldn't turn off again (unplugged).
                        m.game_mode_attached(&p);
                        match m.library_migrate(&p) {
                            Ok(added) if !added.is_empty() => info!("{p}: profile library: added {}", added.join(", ")),
                            Ok(_) => {}
                            Err(e) => warn!("{p}: profile library migration: {e:#}"),
                        }
                    });
                    let _ = dbus::Tsuba::device_added(iface.signal_emitter(), &pid).await;
                }
                for pid in removed {
                    let _ = dbus::Tsuba::device_removed(iface.signal_emitter(), &pid).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
    };

    if mgr.config.lock().unwrap().game_lighting_enabled() {
        if !game.listen_allowed.load(std::sync::atomic::Ordering::Relaxed) {
            info!("game lighting: not started (read-only)");
        } else {
            let g = game.clone();
            match tokio::task::spawn_blocking(move || g.set_enabled(true)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => warn!("game lighting: {e:#}"),
                Err(e) => warn!("game lighting: {e}"),
            }
        }
    }
    let game_sig = {
        let iface = iface.clone();
        tokio::spawn(async move {
            while let Some(status) = game_rx.recv().await {
                if let Err(e) = dbus::Tsuba::game_lighting_changed(iface.signal_emitter(), &status).await {
                    warn!("signal: {e}");
                }
            }
        })
    };

    // Profile library changes → LibraryChanged signal.
    let (lib_tx, mut lib_rx) = mpsc::unbounded_channel::<String>();
    *mgr.library_tx.lock().unwrap() = Some(lib_tx);
    let lib_sig = {
        let iface = iface.clone();
        tokio::spawn(async move {
            while let Some(pid) = lib_rx.recv().await {
                if let Err(e) = dbus::Tsuba::library_changed(iface.signal_emitter(), &pid).await {
                    warn!("signal: {e}");
                }
            }
        })
    };

    // Software actions on KDE global shortcuts (skipped in read-only test runs).
    if !mgr.read_only {
        shortcuts::spawn(mgr.clone());
        idle::spawn(mgr.clone());
        applinks::spawn(mgr.clone());
        game_autoinstall::spawn();
        // Blocking D-Bus calls must run off the async runtime.
        let m = mgr.clone();
        let _ = tokio::task::spawn_blocking(move || m.restore_shortcut_block()).await;
    }

    // Forward hardware events as D-Bus signals.
    let fwd_mgr = mgr.clone();
    let fwd = tokio::spawn(async move {
        let mut held: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        while let Some((pid, ev)) = rx.recv().await {
            // Dynamic Razer keys drive software actions (launch, inter-device).
            if let tsuba_proto::events::Event::RazerKeys(now) = &ev {
                let before = held.insert(pid.clone(), now.clone()).unwrap_or_default();
                let pressed: Vec<u8> = now.iter().filter(|c| !before.contains(c)).copied().collect();
                let released: Vec<u8> = before.iter().filter(|c| !now.contains(c)).copied().collect();
                if pressed.contains(&gamemode::GAME_MODE_KEY) && !fwd_mgr.read_only {
                    let (m, p) = (fwd_mgr.clone(), pid.clone());
                    tokio::task::spawn_blocking(move || m.toggle_game_mode(&p));
                }
                if !pressed.is_empty() || !released.is_empty() {
                    let (m, p) = (fwd_mgr.clone(), pid.clone());
                    tokio::task::spawn_blocking(move || m.on_razer_keys(&p, &pressed, &released));
                }
            }
            if matches!(ev, tsuba_proto::events::Event::Notification { .. } | tsuba_proto::events::Event::Power(_)) {
                let (m, p, e) = (fwd_mgr.clone(), pid.clone(), ev.clone());
                tokio::task::spawn_blocking(move || m.on_event(&p, &e));
            }
            let json = dbus::event_json(&ev);
            info!("{pid}: {json}");
            if let Err(e) = dbus::Tsuba::hardware_event(iface.signal_emitter(), &pid, &json).await {
                warn!("signal: {e}");
            }
        }
    });

    // systemd stops us with SIGTERM; Ctrl+C when run by hand.
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        r = tokio::signal::ctrl_c() => r?,
        _ = term.recv() => {}
    }
    info!("shutting down");
    let g = game.clone();
    let _ = tokio::task::spawn_blocking(move || g.shutdown()).await;
    game_sig.abort();
    lib_sig.abort();
    let _ = tokio::task::spawn_blocking(gamemode::unblock_on_exit).await;
    hot.abort();
    fwd.abort();
    Ok(())
}
