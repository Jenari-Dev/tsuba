#!/bin/sh
# Build a .deb from this source tree's release build (Debian, Ubuntu and derivatives).
#
#   RUSTFLAGS="--cfg tsuba_packaged" cargo build --release --workspace && make -C shim && po/build.sh && SHIM_BUILD=shim/build contrib/deb/build-deb.sh
#
# Output: contrib/deb/tsuba_<version>_amd64.deb. `--stage-only DIR` only lays the files out in DIR.
# Not yet tried on a Debian system: check the Depends names against your release before publishing.
set -e
cd "$(dirname "$0")/../.."
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
if [ "$1" = "--stage-only" ]; then
    exec contrib/install-tree.sh "${2:?usage: --stage-only DIR}"
fi
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
contrib/install-tree.sh "$root"
mkdir -p "$root/DEBIAN"
size=$(du -sk "$root" | cut -f1)
cat > "$root/DEBIAN/control" <<EOF
Package: tsuba
Version: $ver
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: $size
Maintainer: JenAriDev <noreply@users.noreply.github.com>
Depends: libc6, libgcc-s1, libstdc++6, libqt6core6, libqt6gui6, libqt6qml6, libqt6quick6, qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts, qml6-module-qtquick-dialogs, qml6-module-qtquick-templates, qml6-module-qtcore, qml6-module-qt-labs-platform, libkf6i18n6, qml6-module-org-kde-kirigami, qqc2-breeze-style, libpipewire-0.3-0, libpulse0, libxkbcommon0, xkb-data, pulseaudio-utils, xdg-desktop-portal, xdg-utils, fonts-roboto, udev, systemd
Recommends: kglobalaccel6
Suggests: wine, wtype, ydotool, xdotool
Homepage: https://github.com/Jenari-Dev/tsuba
Description: Open-source configuration for Razer keyboards and mice
 Remap keys and buttons, record macros, set DPI and lighting, and use Lighting
 Studio, without Razer's Windows software. Includes the user service tsubad,
 the Tsuba app and the tsuba command-line tool. Not affiliated with Razer Inc.
EOF
cat > "$root/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger --subsystem-match=hidraw --attr-match=idVendor=1532 2>/dev/null || true
    echo "Start the Tsuba service for your user: systemctl --user enable --now tsubad"
fi
EOF
cat > "$root/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
udevadm control --reload-rules 2>/dev/null || true
EOF
chmod 755 "$root/DEBIAN/postinst" "$root/DEBIAN/postrm"
out=contrib/deb/tsuba_${ver}_amd64.deb
dpkg-deb --root-owner-group --build "$root" "$out"
echo "wrote $out"
