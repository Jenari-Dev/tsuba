#!/bin/sh
# Regenerate po/tsuba.pot from the QML sources and merge new strings into every po/*.po.
# Needs GNU gettext (xgettext, msgmerge). Run from anywhere.
set -e
cd "$(dirname "$0")/.."
xgettext --from-code=UTF-8 --language=JavaScript --sort-by-file \
    --keyword=i18n:1 --keyword=i18nc:1c,2 --keyword=i18np:1,2 --keyword=i18ncp:1c,2,3 \
    --keyword=i18nd:2 --keyword=i18ndc:2c,3 \
    --add-comments=TRANSLATORS \
    --package-name=tsuba --msgid-bugs-address=https://github.com/JenAriDev/tsuba/issues \
    -o po/tsuba.pot crates/tsuba-gui/qml/*.qml
for po in po/*.po; do
    [ -e "$po" ] || continue
    msgmerge --quiet --update --backup=none "$po" po/tsuba.pot
done
