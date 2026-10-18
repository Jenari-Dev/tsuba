#!/usr/bin/env bash
# Build the Arch package from this working tree (no GitHub release needed): packs the tree
# the way GitHub's tag archive would, then runs makepkg, which reuses the local tarball.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
ver="$(sed -n 's/^pkgver=//p' "$here/PKGBUILD")"
tar -C "$(dirname "$root")" --exclude="$(basename "$root")/target" --exclude="$(basename "$root")/shim/build" \
    --exclude="$(basename "$root")/contrib/arch/src" --exclude="$(basename "$root")/contrib/arch/pkg" \
    --exclude='*.tar.gz' --exclude='*.pkg.tar.*' \
    --transform "s|^$(basename "$root")|tsuba-$ver|" \
    -czf "$here/tsuba-$ver.tar.gz" "$(basename "$root")"
cd "$here"
makepkg -f "$@"
