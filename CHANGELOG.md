# Changelog

All notable changes to Tsuba are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-18

First public release. Early software: see [docs/STATUS.md](docs/STATUS.md) for what has been verified on
real hardware (BlackWidow V4 Pro and Basilisk V3 Pro) and what has not.

### Added

- `tsubad` (user daemon), `tsuba-app` (Qt/QML app) and `tsuba` (command-line tool) talking to Razer
  keyboards and mice directly over hidraw, with no kernel driver.
- Remapping on a Standard and a Shift layer, with function, mouse, multimedia, sensitivity, profile, Turbo,
  Launch, Text, audio, lighting and Inter-device actions; macros (recorder, editor, on-board upload).
- DPI stages, polling rate, scroll-wheel modes, power settings, on-board profiles, a profile library on the
  PC with pinned on-board slots, and import/export of profile files.
- Hardware lighting effects, Lighting Studio (layered per-LED effects across devices, Ambient, Audio Meter)
  and game lighting through a local Chroma REST server and a Wine/Proton bridge DLL.
- Linked games that switch profiles (optionally only while focused on KDE), Game Mode, Import from Synapse 4.
- Tray icon, notifications, launch at login, light/dark theme, English and Japanese interface, diagnostics
  export without personal data, OpenRazer hand-over scripts.
- Device database of about 260 recognised models, read-only by default until writes are enabled per device.
- Arch Linux package (`contrib/arch`), RPM and DEB recipes (untried), CI and a tag-triggered release workflow.

### Changed

- App id is `io.github.Jenari_Dev.Tsuba` (D-Bus name, desktop file and icon). A login entry created by
  pre-release builds under the old name is replaced automatically on first start.
