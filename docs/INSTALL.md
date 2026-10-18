# Installing Tsuba

Tsuba has three parts: the user daemon `tsubad` (talks to the devices), the app `tsuba-app`, and the
command-line tool `tsuba`. They need no kernel driver — Tsuba uses the standard `hidraw` interface.

## Arch Linux (and Arch-based: CachyOS, EndeavourOS, Manjaro…)

Each [release](https://github.com/Jenari-Dev/tsuba/releases) has a ready-built package (and a `SHA256SUMS` file):

```bash
sudo pacman -U tsuba-*-x86_64.pkg.tar.zst
```

Or build and install the package from a source checkout:

```bash
cd contrib/arch && ./build-local.sh
```
```bash
sudo pacman -U contrib/arch/tsuba-*-x86_64.pkg.tar.zst
```

Then start the daemon for your user (it also starts at every login), and open Tsuba from your application menu:

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

The `PKGBUILD` in `contrib/arch/` downloads the GitHub tarball of the release tag
(`$url/archive/refs/tags/v$pkgver.tar.gz`) and does not pin its checksum (`sha256sums=('SKIP')`), because
GitHub does not guarantee byte-identical archives; the release workflow builds the published package straight
from that tag. `build-local.sh` packs your working tree under the same file name, so local builds never
touch the network for the source.

## Debian, Ubuntu, Fedora (recipes, not yet tried)

`contrib/deb/build-deb.sh` and `contrib/rpm/tsuba.spec` build a `.deb` / `.rpm` from a source checkout. They have
not been run on those distributions yet; the dependency names may need adjusting. Reports are welcome.

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

## Why there is no Flatpak

Tsuba needs raw access to `/dev/hidraw*` (granted by a udev rule on the host), runs a user service
(`tsubad`) that must outlive the window, talks to the session D-Bus under its own name, reads the desktop's
global-shortcut and idle interfaces, and installs a bridge DLL into Wine/Proton prefixes. A Flatpak sandbox
would have to punch holes for all of that, which defeats the point of a sandbox and would be fragile, so
the project does not offer one.

## After installing

- Open **Tsuba**. If no device shows up, Home tells you why and what to do; [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
  has the details.
- In **Settings**: turn on *Launch Tsuba at login* (starts in the tray), pick light, dark or automatic theme,
  choose which notifications you want, and pick English or 日本語 (restart Tsuba afterwards).
- To remove Tsuba again: [UNINSTALL.md](UNINSTALL.md).

## Translations

Translations live in `po/` and are installed as `/usr/share/locale/<lang>/LC_MESSAGES/tsuba.mo` by the package.
From a source checkout: `po/build.sh` writes them to `target/locale`; start the app with
`XDG_DATA_DIRS=$PWD/target:$XDG_DATA_DIRS`. `po/update.sh` refreshes the template after strings change.

## Optional

- **KDE Plasma** (`kglobalaccel`): needed for Launch / Inter-device / Audio actions on device buttons
  and for Game Mode's shortcut blocking.
- **A system tray**: KDE Plasma has one; GNOME needs the AppIndicator extension. Without one, closing the
  window quits Tsuba.
- **Wine / Proton**: needed only to use game lighting with games that load Razer's native library;
  the Game lighting page installs Tsuba's bridge into each game's prefix.
