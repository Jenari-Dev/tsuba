#!/usr/bin/env bash
# Hand Razer devices from openrazer to Tsuba (reversible; see openrazer-restore.sh).
#  - stops and disables openrazer-daemon, the Polychromatic tray and the razer-lights login script
#  - blacklists the openrazer kernel drivers and rebinds the devices to hid-generic (needs sudo)
#  - puts the keyboard in Normal mode so its firmware runs remaps, macros, media keys and the dial
# Nothing is uninstalled. Run as your normal user; it asks for sudo once.
set -euo pipefail

backup="${XDG_DATA_HOME:-$HOME/.local/share}/tsuba/openrazer-backup"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
mkdir -p "$backup" "$autostart"

echo "== stopping openrazer (user session)"
systemctl --user disable --now openrazer-daemon.service 2>/dev/null || true
pkill -x polychromatic-tray-applet 2>/dev/null || pkill -f polychromatic-tray-applet 2>/dev/null || true
if [ -f "$autostart/razer-lights.desktop" ]; then
    mv "$autostart/razer-lights.desktop" "$backup/"
    echo "   razer-lights login item moved to $backup"
fi
# Polychromatic's tray autostart is system-wide; hide it for this user only.
printf '[Desktop Entry]\nType=Application\nName=Polychromatic\nHidden=true\n' > "$autostart/polychromatic-autostart.desktop"
echo "   Polychromatic tray autostart hidden"

echo "== kernel drivers (sudo)"
sudo sh -euc '
cat > /etc/modprobe.d/tsuba-no-openrazer.conf <<CONF
# Tsuba drives Razer devices through hidraw; keep the openrazer drivers from claiming them.
# Remove this file (or run openrazer-restore.sh) to give the devices back to openrazer.
blacklist razerkbd
blacklist razermouse
blacklist razeraccessory
blacklist razerkraken
CONF
for drv in razerkbd razermouse razeraccessory razerkraken; do
    [ -d /sys/bus/hid/drivers/$drv ] || continue
    for dev in /sys/bus/hid/drivers/$drv/0003:1532:*; do
        [ -e "$dev" ] && echo "${dev##*/}" > /sys/bus/hid/drivers/$drv/unbind
    done
    modprobe -r $drv 2>/dev/null || true
done
for dev in /sys/bus/hid/devices/0003:1532:*; do
    [ -e "$dev/driver" ] || echo "${dev##*/}" > /sys/bus/hid/drivers/hid-generic/bind 2>/dev/null || true
done
'
sleep 2
echo "   now bound:"
for dev in /sys/bus/hid/devices/0003:1532:*; do
    printf '   %s -> %s\n' "${dev##*/}" "$(basename "$(readlink -f "$dev/driver" 2>/dev/null)" 2>/dev/null || echo none)"
done

echo "== Tsuba"
systemctl --user restart tsubad.service
sleep 2
# ~/.local/bin may not be on a non-interactive bash PATH.
tsuba="$(command -v tsuba || echo "$HOME/.local/bin/tsuba")"
# Every attached Razer device to Normal mode (its default; openrazer may have left it in Driver mode).
for pid in $("$tsuba" list 2>/dev/null | awk '/^[0-9a-f]{4} /{print $1}'); do
    "$tsuba" set-mode normal --confirm -d "$pid" >/dev/null 2>&1 || true
    printf '   %s: %s\n' "$pid" "$("$tsuba" info -d "$pid" 2>/dev/null | grep -i mode || echo 'mode unknown')"
done
echo
echo "Done. Re-apply / save your lighting in Tsuba (Lighting or Chroma Studio) so tsubad restores it at login."
echo "To undo: $(dirname "$(readlink -f "$0")")/openrazer-restore.sh"
