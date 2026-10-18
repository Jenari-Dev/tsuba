# Tsuba status (v0.1.0)

Tsuba 0.1.0 is an early release written and tested by one person on two devices: a **BlackWidow V4 Pro**
(1532:028d) and a **Basilisk V3 Pro** (1532:00aa, over the cable). Everything below is sorted by how much
evidence there is. If you own either device, or any other Razer keyboard or mouse, reports are very welcome:
use the [issue templates](../.github/ISSUE_TEMPLATE) (the **Device report** template takes the text from
Tsuba's "Report this device" button).

Untested devices are read-only until you switch writes on for them, so trying Tsuba on a different model does
not change anything on the device by default (see the README).

## Verified on real hardware

Checked by hand on the two devices above, on Arch-based Linux with KDE Plasma 6 (Wayland).

- **Lighting**: hardware effects on both devices, per-key painting on the keyboard, Lighting Studio (named
  profiles, per-device layers, per-layer brightness, blend modes, profile switching, a shared canvas across
  keyboard and mouse, Audio Meter, Ambient), lighting power options (off when idle or with the display, back
  on with input).
- **Remapping**: keyboard and mouse remaps on the Standard and Shift layers, Disable, left-handed swap, mouse
  scroll left/right and double click, Turbo with a presses-per-second rate, sensitivity and DPI-clutch buttons,
  profile switching from a button, the Shift modifier on any key or button.
- **Actions run by the daemon**: Launch program / website, Inter-device actions (a key on one device acting on
  the other, for example a keyboard key as a DPI clutch for the mouse), lighting actions on keys, audio
  actions (volume, mute, microphone mute, next/previous output), Text function (stored as an on-board macro,
  US layout).
- **Macros**: recording, editing, uploading to the device and playing them back (once, N times, while held, toggle).
- **Performance**: DPI stages, polling rate, scroll-wheel modes.
- **Profiles**: on-board profile list (create, duplicate, rename, delete, make active, follows the device's
  profile button), the profile library on the PC and pinned profiles.
- **Game Mode**: Windows/Menu keys blocked in firmware, selected desktop shortcuts blocked on KDE, toggled
  from the keyboard.
- **Game lighting**: the local Chroma REST server and the Wine/Proton bridge DLLs, checked with the test
  scripts in `contrib/` (`game-lighting-test.sh`, `game-lighting-mouse-test.sh`), including priority over and
  restore of the user's own lighting.
- **OpenRazer hand-over**: moving the devices from OpenRazer's drivers to the generic HID driver and back
  (`tsuba-handover`, `tsuba-handover-restore`); typing, mouse, media keys, dial and lighting work afterwards.
- **App shell**: launch, tray, light/dark theme.

## Built and unit-tested, not yet verified on hardware

These have automated tests (a simulated device, the QML suite, the D-Bus tests) but have not been confirmed
on a real device. They may work, partly work, or need fixes. Reports on any of them are useful.

- **App shell**: Home cards with wired/wireless and battery badges, the OpenRazer-conflict banner, start-service
  and missing-permission screens, theme and accent colour, 日本語 interface, tray profile submenus, launch at
  login (starting hidden in the tray), single-window behaviour, desktop notifications (DPI stage, profile,
  scroll mode, connect/disconnect), diagnostics export (meant to contain no personal data), welcome panel.
- **Device settings**: keyboard key-response modes, mouse lift-off distance (symmetric and asymmetric),
  swapping DPI stages, lighting zones on the mouse (logo and others), lighting across every device at once,
  switching lighting off when the session locks, smart dimming, profile reset and factory reset (these write
  to the device, so use a scratch slot).
- **Customize**: the grouped key picker and shortcut catalogue, Launch presets and the application list,
  microphone volume actions and "use this output", Text with accented characters and emoji (needs `wtype`,
  `ydotool` or `xdotool`), "click with keys held", repeat scroll, brightness and Studio-profile actions on
  keys, the notice shown after the first remap.
- **Macros**: the list screen (search, folders, rename, duplicate, delete, not-in-use marker), sequence
  playback, media and system-wake events, cursor-move events, Run command, importing macros from Synapse.
- **Dial**: reordering modes and resetting them to stock.
- **Profiles and linked games**: exporting and importing profile files (including the approval dialog for
  commands in imported files), "lighting follows this profile", per-combination Game Mode blocking and
  keeping the Windows key off across profile switches, linked games (polling rate, lighting, restore on exit),
  focus-only linking through the KWin script, Import from Synapse into the library.
- **Lighting Studio**: image and gradient layers, the Wheel layer, Audio Meter sources and peak hold, Studio
  profile export/import, the lighting-owner chip, underglow row placement, Tidal and Battery effects on the mouse.
- **Game lighting with real games**: Wine/Proton games that use Razer Chroma (install the bridge from the Game
  lighting page, allow/block programs, the web-access toggle).
- **Other models**: about 260 further devices are recognised from hardware facts in the device database; none
  of them has been tried.

## Not supported, or out of scope

- Anything that needs Razer's online services: account, cloud sync, community sharing, macro and effect
  exchange, AI-generated macros, promotions, ratings. Tsuba is offline-first.
- Synapse 3 migration, partner integrations (smart lights and similar), Synapse Override. Importing from a
  Synapse 4 install on a mounted Windows drive is supported instead.
- Auto-update and module installer. Your package manager updates Tsuba; every module is built in.
- **Firmware updates.** Tsuba never flashes firmware. It shows the version the device reports.
- More keymaps than Standard and Shift, driver-mode macros, Fn+key combinations (not supported by the tested
  keyboard's firmware or fixed in firmware).
- Analog switch features (Rapid Trigger, actuation points, gamepad emulation), adjustment mode, Copilot key,
  wheel tension and haptics, HyperPolling rates above 1000 Hz, mouse dock and charging-stand features. The
  tested devices do not have this hardware, so it cannot be built or verified.
- Separate backlight and underglow brightness (the tested keyboard exposes a single lighting region).
- Battery health limiter, laptop, GPU and fan power controls.
- Bluetooth mode and dongle pairing. Tsuba talks to devices over USB (cable or the wireless dongle) and
  wireless operation has had only limited checking.
- Features that need to see pointer or key positions outside the app (X/Y axis lock, on-the-fly sensitivity,
  recording macros outside the Tsuba window, reactive lighting layers driven by key presses). They would need
  extra input-device access that Tsuba does not ask for. See the open questions.
- Windows-only features (OS notification reset, game-controller properties).
- Flatpak packaging: see [INSTALL.md](INSTALL.md).

## Open questions

Input from users is welcome on any of these (open an issue or a discussion).

1. **Media-key LEDs.** The `tsuba led-guide` helper lights keys one at a time so you can check the keyboard
   drawing. Which of the media keys on your keyboard light up, and where does the guide put them?
2. **Firmware updates.** Options are: show a notice and point to the vendor's Windows updater; run the vendor's
   updater under Wine; or write a native flasher, which would need a USB capture of an official update. Is any
   of these worth the risk of bricking a device?
3. **Reading key positions.** Reactive and Ripple layers in Lighting Studio and macro recording outside the
   Tsuba window would need an opt-in mode where the daemon reads which key was pressed (positions only,
   never stored or logged). Would you want that, or should these be dropped?
4. **Pointer features.** X/Y axis lock, on-the-fly sensitivity and a sensitivity matcher need uinput/evdev
   access and a new udev rule. Build it, or document as unsupported? Reports from owners of a second mouse, a
   HyperPolling dongle or a mouse dock would help.
5. **Unknown commands.** Custom dial modes, the scroll-stage table and phased macros use device commands that
   have not been identified. Drop them, or is someone able to capture the official software's traffic?
6. **Wireless.** The wireless connection has had little testing. Output of `tsuba list`, `tsuba -d <pid> info`
   and `tsuba probe-features` from wireless devices (dongle or Bluetooth) would help.
7. **Packaging.** Only an Arch package is built by the project. RPM and DEB recipes exist in `contrib/` but have
   never been run. Help with other distributions is welcome.
8. **Japanese wording.** The Japanese translation has not had a wide review by native speakers. The term list is in
   `po/GLOSSARY.md`; corrections welcome.
