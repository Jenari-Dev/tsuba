# Contributing to Tsuba

Thanks for helping! The most valuable contribution right now is **testing devices Tsuba hasn't been
tested with yet**.

## Testing an untested device

1. Plug it in and open Tsuba. Untested devices show a "hasn't been tested with Tsuba yet" banner and
   start read-only — nothing is written to the device.
2. Check what Tsuba reads (firmware version, lighting effects, DPI, profiles). Wrong or missing values are
   useful reports too.
3. If you want to try changing settings, switch on **Allow changes** on the banner. Tsuba never touches
   firmware; a wrong setting can at worst leave the device configured oddly until you reset it.
4. Press **Report this device**, paste the copied text into a new issue using the *Device report* template,
   and describe what worked and what didn't.

## Never include personal data

Don't paste serial numbers, account ids, Windows user names or other personal details into issues or
pull requests. The device report Tsuba copies leaves the serial number out on purpose.

## Development

- `cargo build --release --workspace`, `cargo test --release --workspace --exclude tsuba-gui`
- QML tests: `crates/tsuba-gui/tests/qml/run.sh`
- Bridge DLLs: `make -C shim` and `make -C shim test` (needs `mingw-w64-gcc` and `wine`)
- Device drawings: `python3 tools/gen-device-art.py` regenerates `data/devices`
- Device database: `python3 tools/import-openrazer.py <openrazer checkout>` regenerates `data/devicedb`
  from facts in OpenRazer's sources (it parses them; it never runs OpenRazer code)

## Legal ground rules

Tsuba is independent and must stay clean:

- No Razer code, images, fonts, firmware, SDK headers or documentation text in the repository.
- Razer's names identify hardware only ("BlackWidow V4 Pro"); Tsuba's own features have Tsuba names
  (Lighting Studio, Shift layer, Auto free-spin, Game lighting).
- Device facts (USB ids, LED positions, protocol values) are fine; artwork must be Tsuba's own and never
  traced from Razer's renders.

## Translations

Every user-visible string in QML goes through `i18n()`, `i18nc("context", "…")` or `i18np("one", "many", n)`
(never glue translated fragments together; use `%1` arguments). Messages made in Rust use `tr()`, `trc()` and
`trf()` from `src/i18n.rs`. Identifiers, key caps and protocol names stay literal and are marked
`// i18n-ok` (or listed in `tools/i18n-allow.txt`); `tools/scan-i18n.py` fails the QML test run otherwise.

After changing strings run `po/update.sh`, fill the new entries in `po/ja.po` (terms: `po/GLOSSARY.md`) and check
with `msgfmt --check --statistics -o /dev/null po/ja.po` (no untranslated, no fuzzy entries). The daemon,
the command-line tools and the text of daemon errors stay English.
