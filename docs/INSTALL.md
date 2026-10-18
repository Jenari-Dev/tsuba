# Installing Tsuba

Tsuba has three parts: the user daemon `tsubad` (talks to the devices), the app `tsuba-app`, and the
command-line tool `tsuba`. They need no kernel driver — Tsuba uses the standard `hidraw` interface.

## Arch Linux (and Arch-based: CachyOS, EndeavourOS, Manjaro…)

Build and install the package from a source checkout:

```bash
cd contrib/arch && ./build-local.sh
```
```bash
sudo pacman -U contrib/arch/tsuba-*-x86_64.pkg.tar.zst
```

Then start the daemon for your user (it also starts at every login):

```bash
systemctl --user enable --now tsubad
```

The package installs a udev rule (`/usr/lib/udev/rules.d/70-tsuba.rules`) that gives the logged-in user
access to Razer devices. If a device was already plugged in, unplug and replug it once (or reboot).

### Coming from OpenRazer / Polychromatic

OpenRazer's kernel drivers claim Razer devices and get in Tsuba's way. Hand the devices over (reversible):

```bash
tsuba-handover
```

It stops the OpenRazer daemon, blacklists its kernel modules, rebinds the devices to the generic HID driver
and switches them to normal mode. Undo with `tsuba-handover-restore`. Nothing is uninstalled.

## From source (any distribution)

Requirements: Rust (stable, edition 2024), Qt 6 (base, declarative, svg), KDE Frameworks 6 I18n,
`qqc2-breeze-style`, PipeWire (libpipewire), `libpulse` (for `parec`, Audio Meter), `xdg-desktop-portal`
(Ambient), and for the game-lighting bridge DLLs `mingw-w64-gcc`. Roboto is the UI font.

```bash
cargo build --release --workspace
```
```bash
make -C shim
```

Install `target/release/{tsubad,tsuba,tsuba-app}` somewhere on your `PATH`, copy
`contrib/70-tsuba.rules` to `/etc/udev/rules.d/`, `contrib/tsubad.service` to
`~/.config/systemd/user/` (adjust `ExecStart`), and `data/devices`, `data/devicedb` and
`shim/build/{x64,x86}` to `/usr/share/tsuba/{devices,devicedb,game-lighting/…}` (a source checkout also
finds them next to the build tree).

## Optional

- **KDE Plasma** (`kglobalaccel`): needed for Launch / Inter-device / Audio actions on device buttons
  and for Game Mode's shortcut blocking.
- **Wine / Proton**: needed only to use game lighting with games that load Razer's native library;
  the Game lighting page installs Tsuba's bridge into each game's prefix.
