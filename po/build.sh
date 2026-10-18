#!/bin/sh
# Compile po/*.po into DESTDIR/PREFIX/share/locale/<lang>/LC_MESSAGES/tsuba.mo.
# Usage: po/build.sh [OUTDIR]   (default: target/locale, which a source checkout's XDG_DATA_DIRS can point at)
set -e
cd "$(dirname "$0")/.."
out=${1:-target/locale}
for po in po/*.po; do
    [ -e "$po" ] || continue
    lang=$(basename "$po" .po)
    mkdir -p "$out/$lang/LC_MESSAGES"
    msgfmt --check-format -o "$out/$lang/LC_MESSAGES/tsuba.mo" "$po"
done
