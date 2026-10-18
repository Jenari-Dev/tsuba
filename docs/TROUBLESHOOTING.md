# Troubleshooting

Start with the Home page: when no device shows up, Tsuba works out why and offers the fix (start the
service, install the permission rule, hand devices over from OpenRazer). This page explains the same
things in more detail. If nothing here helps, [export diagnostics](#export-diagnostics) and open an issue.

## Quick checks

- Is the service running? `systemctl --user status tsubad`
- Does the system see the device? `tsuba list` prints every supported device the service can open.
- What does Tsuba think is wrong? `tsubad --check-env` prints a report (service, permission rule,
  device access, conflicting programs) as JSON; `diagnosis` is one of `ok`, `permission`, `no_hidraw`,
  `unsupported_only` or `no_device`.

## "The Tsuba service isn't running"

Tsuba talks to your devices through a background service, `tsubad`. Start it from the Home page, or:

```bash
systemctl --user enable --now tsubad
```

If the unit doesn't exist, the package isn't installed (or `contrib/tsubad.service` wasn't copied to
`~/.config/systemd/user/`); see [INSTALL.md](INSTALL.md). If the service starts and stops again, read its log:

```bash
journalctl --user -u tsubad -e
```

"is another tsubad already running?" means a second copy holds the D-Bus name: stop the other one
(`pkill tsubad`, then start the service again).

## "Tsuba isn't allowed to open your device" (permissions)

Tsuba uses the kernel's `hidraw` interface, which normally only root can open. The package installs a
udev rule (`/usr/lib/udev/rules.d/70-tsuba.rules`) that gives the logged-in user access. Check:

```bash
ls -l /dev/hidraw*
```

A usable node shows `+` after the permissions (an ACL for your user). If it doesn't:

1. Is the rule installed? `ls /usr/lib/udev/rules.d/70-tsuba.rules /etc/udev/rules.d/70-tsuba.rules`.
   From a source checkout: `sudo install -Dm644 contrib/70-tsuba.rules /etc/udev/rules.d/70-tsuba.rules`.
2. Reload it: `sudo udevadm control --reload-rules` and
   `sudo udevadm trigger --subsystem-match=hidraw --attr-match=idVendor=1532`.
3. Unplug and replug the device (or log out and in): the access list is applied when the device appears.
4. Are you on a real login session at the seat (not an SSH session)? The rule uses `uaccess`, which only
   applies to the user sitting at the machine.

An old hand-written rule such as `70-razer-hidraw.rules` can be removed once the package's rule is in place.

## "Another driver is holding your device" / OpenRazer

OpenRazer's kernel modules (`razerkbd`, `razermouse`, …) and its daemon claim Razer devices and also drive
the lighting. Running both at once makes colours flicker or reset. Tsuba shows a banner when it sees them.
To switch OpenRazer off for good, hand the devices over (reversible; nothing is uninstalled):

```bash
tsuba-handover
```

Undo with `tsuba-handover-restore`. For a quick test you can instead stop the daemon with
`systemctl --user stop openrazer-daemon`. Other lighting programs (OpenRGB, Polychromatic) cause the same
flicker: use one at a time.

## "No Razer device found"

- Use the USB cable, or put the wireless dongle in a USB port. Tsuba talks to devices over USB; a device
  connected only over Bluetooth is not supported yet.
- Try another port and avoid unpowered hubs. Run `lsusb | grep 1532` to see whether the system sees it at all.
- A device that sleeps (wireless mice) wakes on movement or a button press.

## "This Razer device isn't supported yet"

Tsuba recognises about 260 models but only two are fully tested. Unsupported means the USB id isn't in the
device database (`tsuba db` searches it). Open an issue with the diagnostics bundle; the product id is in it.

## A device is listed but changes do nothing

- Untested models are read-only until you press **Allow changes** on the banner.
- If the header says "tsubad · read-only" the service was started with `--read-only`; start it normally.
- Wireless mice need the dongle or the cable for settings to be written.

## Two identical devices

The service tells devices apart by model, so only one of two identical devices is managed; Tsuba warns
about it. Unplug one of them.

## Notifications don't appear

Notifications come from the service through the desktop's notification daemon (Plasma shows them as
pop-ups). Check Settings, Notifications (and the master switch), that "Do not disturb" is off, and press
**Send a test notification**. For Plasma's volume-style display turn on the on-screen display option.

## The tray icon doesn't show

Tsuba uses the StatusNotifierItem protocol (KDE Plasma, most other desktops). GNOME needs the
AppIndicator extension. Without a tray, closing the window quits Tsuba and "Launch at login" opens the
window instead of hiding it.

## The language didn't change

Pick the language in Settings and restart Tsuba. Translations are installed as
`/usr/share/locale/<lang>/LC_MESSAGES/tsuba.mo`; from a source checkout run `po/build.sh` and start the
app with `XDG_DATA_DIRS=$PWD/target:$XDG_DATA_DIRS`. Messages from the service and the `tsuba` command stay in English.

## Export diagnostics

Settings, Help and diagnostics, **Export diagnostics…** writes a zip you can attach to an issue. From a terminal:

```bash
tsubad --diagnostics tsuba-diagnostics.zip
```

It contains: a summary (Tsuba, Linux and desktop versions), the Razer devices the kernel sees (model ids
and whether you may open them), the udev rule, the service's last 300 log lines, kernel messages that
mention Razer hardware, and the *shape* of your settings (section names and counts). It never contains key
assignments, macro text, file names, serial numbers, MAC addresses, your user name, your host name or your
home directory path (those are replaced before the file is written). Open the zip and read it before you share it.

## Still stuck?

Open an issue at https://github.com/JenAriDev/tsuba/issues with the diagnostics zip and what you expected to happen.
