#!/bin/sh
# Regenerate po/tsuba.pot from the QML and Rust sources and merge new strings into every po/*.po.
# Needs GNU gettext (xgettext, msgcat, msgmerge). Run from anywhere.
set -e
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
common="--from-code=UTF-8 --sort-by-file --add-comments=TRANSLATORS --package-name=tsuba --msgid-bugs-address=https://github.com/Jenari-Dev/tsuba/issues"
# shellcheck disable=SC2086
xgettext $common --language=JavaScript \
    --keyword=i18n:1 --keyword=i18nc:1c,2 --keyword=i18np:1,2 --keyword=i18ncp:1c,2,3 \
    --keyword=i18nd:2 --keyword=i18ndc:2c,3 \
    -o "$tmp/qml.pot" crates/tsuba-gui/qml/*.qml
# Messages made in Rust (src/i18n.rs): tr("..."), trc("context", "..."), trf("... %1", &[..])
# shellcheck disable=SC2086
xgettext $common --language=Rust \
    --keyword=tr:1 --keyword=trc:1c,2 --keyword=trf:1 \
    -o "$tmp/rust.pot" crates/tsuba-gui/src/*.rs
msgcat --use-first --sort-by-file -o po/tsuba.pot "$tmp/qml.pot" "$tmp/rust.pot"
for po in po/*.po; do
    [ -e "$po" ] || continue
    msgmerge --quiet --update --backup=none "$po" po/tsuba.pot
done
