# Tsuba

Tsuba is an open-source Linux app for configuring Razer keyboards and mice: key and button remapping,
macros, DPI and polling rate, a profile library (unlimited profiles on the PC, pinned ones on the device), lighting effects, a per-LED lighting studio, linked games
and power settings. It talks to the devices directly over hidraw (no kernel driver needed) and runs as a
user daemon (`tsubad`) plus a Qt/QML app (`tsuba-app`) and a CLI (`tsuba`).

Supported and tested: BlackWidow V4 Pro (1532:028d), Basilisk V3 Pro (1532:00aa, wired).

**Install:** see [docs/INSTALL.md](docs/INSTALL.md). **Something wrong?** [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).
**Remove it:** [docs/UNINSTALL.md](docs/UNINSTALL.md). **Help test devices:** see [CONTRIBUTING.md](CONTRIBUTING.md).

Highlights: per-key and per-button remapping with a Shift layer and factory reset per key; macros
(recorded, edited, stored on the PC or on the device); DPI stages, polling rate, scroll-wheel modes;
a profile library like Synapse's (any number of profiles per device on the PC; switching writes only what
differs into an on-board slot; up to 5 can be pinned to the device's slots for its profile button); hardware lighting effects plus Lighting Studio (layered per-LED effects including
Ambient screen colours and an Audio Meter); linked games that switch profiles when a game starts (with a
Steam / Lutris / Heroic / Bottles game scanner); power and idle settings; **Import from Synapse** (reads
the profiles, macros and lighting effects of a Windows Synapse 4 install on a mounted drive);
your own device pictures; and game lighting.

Around the devices: a tray icon with quick profile switching, desktop notifications (DPI stage, profile,
dial, scroll-wheel mode, low battery, connect / disconnect), launch at login, light, dark or automatic
theme with a custom accent colour, a welcome tour, an English and 日本語 interface, and a diagnostics export that
leaves out personal data. When no device shows up, Home explains why (service not running, missing permission,
OpenRazer holding the device, unsupported model) and offers the fix.

Recognised but untested: about 260 more Razer keyboards, laptop keyboards, keypads, mice, mouse mats,
mouse docks, headset stands and other lighting accessories, listed in Tsuba's device database
(`data/devicedb/*.toml`; `tsuba db [filter]` searches it). Untested devices are **read-only by default**:
Tsuba reads and shows them, but refuses to change any setting until you enable writes for that device
(`allow_untested_writes = true` in its `[[device]]` section of `~/.config/tsuba/config.toml`, the daemon's
`SetAllowUntestedWrites` D-Bus method, or `tsuba --allow-untested` for one CLI command). Their drawings are
generic layouts marked approximate. Headsets are listed for reference only (not protocol-2.5 devices).
Reports from owners of untested devices are welcome.

## What Tsuba does not do, and why

Tsuba aims to cover what Razer's own Windows software does for the devices it can verify. These are left out
on purpose or can't be done on the tested hardware:

| Not included | Why |
|---|---|
| Account, cloud sync, community sharing, macro and effect exchange, AI-generated macros, promotions, ratings | They need Razer's online services. Tsuba is offline-first: settings stay on your computer, and it makes no network connections of its own (the game-lighting server listens on the loopback address only). |
| Synapse 3 migration, partner integrations (smart lights), Synapse Override | Other products and services. Importing from a Synapse 4 install on a mounted Windows drive is supported instead. |
| Auto-update, module installer, "in development" device catalogue | Your distribution's package manager updates Tsuba; every module is built in. |
| Firmware updates | Not offered yet. Tsuba never flashes firmware and shows the version the device reports. |
| More keymaps than Standard and Shift, driver-mode macros, Fn+key combinations | The tested keyboard doesn't support extra keymaps, Tsuba keeps devices in normal mode, and the Fn layer is fixed in firmware. |
| Analog switch features (Rapid Trigger, actuation points, gamepad emulation), adjustment mode, Copilot key, wheel tension / haptics, HyperPolling rates above 1000 Hz | The two tested devices don't have that hardware, so it can't be built or verified. Reports from owners of such devices are welcome. |
| Separate backlight and underglow brightness | The tested keyboard exposes a single lighting region. |
| Battery health limiter, laptop, GPU and fan power controls | Laptop and other product features. |
| Bluetooth mode and dongle pairing | Not verified yet: Tsuba talks to devices over USB (cable or the wireless dongle). |
| Windows-only features (OS notification reset, game-controller properties) | They don't exist on Linux; Tsuba links to the desktop's own settings where there is a counterpart. |

## Game lighting

Games that support Razer Chroma can light your devices: `tsubad` runs a small local server on
`127.0.0.1:54235` (loopback only) that speaks the documented Chroma REST interface, so games, game
engines and browser-based lighting apps that use it work unchanged. While a game shows an effect it
takes over the matching devices (keyboard, mouse, …); when it stops, closes or goes quiet for 15 s, your
own lighting (Lighting Studio profile or hardware effect) comes back. Untested devices stay read-only.
Games that load Razer's native library instead get Tsuba's bridge DLL (`shim/`, built with mingw-w64):
the **Game lighting** page installs it into each Wine/Proton prefix (it never replaces a file it didn't
install) and it forwards the game's calls to the same local server. Games that ship their own copy of
the library or require it to be signed by Razer can't use the bridge; they still run normally.
Turn it off with `game_lighting = false` at the top of `~/.config/tsuba/config.toml` or the daemon's
`SetGameLighting` D-Bus method; `contrib/game-lighting-test.sh` paints the keyboard as a quick check.
Browser-based lighting apps additionally need **Allow websites to control lighting** on the Game
lighting page (`game_lighting_web = true`; off by default, because any website you have open could then
change your lighting). Note that the server is plain loopback TCP: on a computer shared by several user
accounts, every local user can reach port 54235 and light your devices while a game isn't using them
(each game's session is protected by a random token in its session address).

The game-lighting server and bridge are independent implementations of Razer's published Chroma
interfaces, written for interoperability; they contain no Razer code, SDK headers or documentation text
(the bridge's file names are the ones games load, which compatibility requires).

## Legal

Tsuba is an independent project. It is **not affiliated with, endorsed by, or sponsored by Razer Inc.**
Razer, Synapse, Chroma, HyperShift, BlackWidow, Basilisk and related names and logos are trademarks of
Razer Inc.; they appear in this project only to identify the hardware Tsuba works with.

- No Razer code, firmware, images, fonts or other Razer files are included in or distributed with Tsuba.
  Tsuba's device protocol support was written independently for interoperability with hardware you own.
- Device pictures are Tsuba's own line-art drawings, generated by `tools/gen-device-art.py` from key-unit
  geometry and hardware facts (firmware key positions and LED indexes measured on the devices).
- The device database holds only hardware facts (USB product ids, model names, interface and transaction
  ids, lighting grid sizes, supported features). Device facts were cross-checked against the OpenRazer
  project (https://github.com/openrazer/openrazer, GPL-2.0); `tools/import-openrazer.py` regenerates the
  database from a local OpenRazer checkout by parsing those facts, without running or copying its code.

Licensed under the GNU General Public License v2.0 or later (see `LICENSE`).
