# Uninstalling Tsuba

Nothing Tsuba does is permanent: it writes settings to your devices only when you ask, and keeps its own files in
your home directory.

## 1. Stop it and turn off autostart

```bash
systemctl --user disable --now tsubad
```

In Tsuba's Settings, switch off **Launch Tsuba at login** (or delete
`~/.config/autostart/io.github.jenaridev.tsuba.desktop`).

## 2. Give devices back to OpenRazer (only if you used the hand-over)

```bash
tsuba-handover-restore
```

## 3. Remove the program

- Arch and derivatives: `sudo pacman -R tsuba`
- Installed by hand from a source checkout: delete `tsuba`, `tsubad` and `tsuba-app` from where you put them,
  `~/.config/systemd/user/tsubad.service`, the udev rule `/etc/udev/rules.d/70-tsuba.rules`
  (then `sudo udevadm control --reload-rules`), `/usr/share/tsuba` or `~/.local/share/tsuba`.

Your devices keep the last settings written to them (on-board profiles, lighting, DPI). To put a device back to
factory behaviour use its own reset (see the manufacturer's instructions), or write a profile you like first.

## 4. Optional: remove your data

These are only your settings; keep them if you may reinstall.

| What | Where |
|---|---|
| Profiles, macros, lighting, linked games | `~/.config/tsuba/` (`config.toml`, `profiles/`, `macros.json`, `chroma-profiles.json`, `app-links.json`, `game-lighting.json`) |
| App look and behaviour | `~/.config/tsuba/app.ini` |
| Notification choices | `~/.config/tsuba/notifications.json` |
| Your own device pictures | `~/.local/share/tsuba/devices/` |
| Backup made by `tsuba-handover` (keep until you ran `tsuba-handover-restore`) | `~/.local/share/tsuba/openrazer-backup/` |

```bash
rm -r ~/.config/tsuba ~/.local/share/tsuba
```

## 5. Game lighting bridge (only if you installed it into games)

Open Tsuba's Game lighting page and use **Remove** for each game prefix before uninstalling; Tsuba never
removes files it didn't install.
