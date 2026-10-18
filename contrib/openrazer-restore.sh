#!/usr/bin/env bash
# Undo openrazer-handover.sh: give the devices back to openrazer.
set -euo pipefail

backup="${XDG_DATA_HOME:-$HOME/.local/share}/tsuba/openrazer-backup"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"

sudo sh -euc '
rm -f /etc/modprobe.d/tsuba-no-openrazer.conf
for dev in /sys/bus/hid/drivers/hid-generic/0003:1532:*; do
    [ -e "$dev" ] && echo "${dev##*/}" > /sys/bus/hid/drivers/hid-generic/unbind
done
modprobe razerkbd; modprobe razermouse
'
[ -f "$backup/razer-lights.desktop" ] && mv "$backup/razer-lights.desktop" "$autostart/"
rm -f "$autostart/polychromatic-autostart.desktop"
systemctl --user enable --now openrazer-daemon.service
echo "openrazer restored. Log out and back in (or run ~/.local/bin/razer-lights) to re-apply its lighting."
