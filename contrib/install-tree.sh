#!/bin/sh
# Install a built Tsuba into DESTDIR with the layout distribution packages use (the .deb and .rpm
# recipes call this; the Arch PKGBUILD spells the same layout out itself).
#
# Usage: contrib/install-tree.sh DESTDIR
# Needs, already built: target/release/{tsubad,tsuba,tsuba-app}; the game-lighting DLLs in $SHIM_BUILD
# (default shim/build, from `make -C shim`); the translations in $LOCALE_BUILD (default target/locale,
# from `po/build.sh`).
set -e
dest=${1:?usage: install-tree.sh DESTDIR}
cd "$(dirname "$0")/.."
shim=${SHIM_BUILD:-shim/build}
locale=${LOCALE_BUILD:-target/locale}

install -Dm755 -t "$dest/usr/bin" target/release/tsubad target/release/tsuba target/release/tsuba-app
install -Dm755 contrib/openrazer-handover.sh "$dest/usr/bin/tsuba-handover"
install -Dm755 contrib/openrazer-restore.sh "$dest/usr/bin/tsuba-handover-restore"

install -Dm644 contrib/tsubad.service "$dest/usr/lib/systemd/user/tsubad.service"
sed -i 's|^ExecStart=.*|ExecStart=/usr/bin/tsubad|' "$dest/usr/lib/systemd/user/tsubad.service"
install -Dm644 contrib/70-tsuba.rules "$dest/usr/lib/udev/rules.d/70-tsuba.rules"

install -dm755 "$dest/usr/share/tsuba"
cp -a --no-preserve=ownership data/devices "$dest/usr/share/tsuba/"
install -Dm644 -t "$dest/usr/share/tsuba/devicedb" data/devicedb/*.toml
install -Dm644 "$shim/x64/RzChromaSDK64.dll" "$dest/usr/share/tsuba/game-lighting/x64/RzChromaSDK64.dll"
install -Dm644 "$shim/x86/RzChromaSDK.dll" "$dest/usr/share/tsuba/game-lighting/x86/RzChromaSDK.dll"

install -Dm644 contrib/io.github.Jenari_Dev.Tsuba.desktop -t "$dest/usr/share/applications"
sed -i 's|^Exec=.*|Exec=tsuba-app|' "$dest/usr/share/applications/io.github.Jenari_Dev.Tsuba.desktop"
install -Dm644 contrib/io.github.Jenari_Dev.Tsuba.svg -t "$dest/usr/share/icons/hicolor/scalable/apps"

for mo in "$locale"/*/LC_MESSAGES/tsuba.mo; do
    [ -e "$mo" ] || continue
    lang=${mo#"$locale"/}
    install -Dm644 "$mo" "$dest/usr/share/locale/$lang"
done

install -Dm644 -t "$dest/usr/share/doc/tsuba" README.md docs/*.md
install -Dm644 LICENSE -t "$dest/usr/share/licenses/tsuba"
