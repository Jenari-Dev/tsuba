#!/bin/sh
# Headless QML tests: copies the app's QML next to the tests (singletons via qmldir) and runs
# them on the offscreen platform with a small runner that installs KDE's i18n() on the engine
# (falls back to Qt's qmltestrunner when the runner can't be built). Settings go to a throw-away
# XDG_CONFIG_HOME, never to ~/.config/tsuba.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
# Every user-visible QML string must go through i18n() (allow-list: tools/i18n-allow.txt, `// i18n-ok`).
python3 "$root/tools/scan-i18n.py"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

runner=$root/target/qmltest-runner
if [ ! -x "$runner" ] || [ "$here/runner.cpp" -nt "$runner" ]; then
    mkdir -p "$root/target"
    b=$work/build
    mkdir -p "$b"
    cp "$here/runner.cpp" "$b/"
    if (cd "$b" && /usr/lib/qt6/moc runner.cpp -o runner.moc \
        && g++ -std=c++17 -fPIC runner.cpp -o runner \
            $(pkg-config --cflags --libs Qt6QuickTest Qt6Qml Qt6Gui Qt6Core) \
            -I/usr/include/KF6/KI18n -I/usr/include/KF6 -lKF6I18n -lKF6I18nQml) >"$work/build.log" 2>&1; then
        cp "$b/runner" "$runner"
    else
        echo "note: could not build the i18n-aware runner (see below); using qmltestrunner" >&2
        cat "$work/build.log" >&2
        runner=/usr/lib/qt6/bin/qmltestrunner
    fi
fi

mkdir -p "$work/cfg" "$work/t"
cp "$here"/../../qml/*.qml "$here"/tst_*.qml "$work/t"/
printf 'singleton Theme 1.0 Theme.qml\nsingleton KeyNames 1.0 KeyNames.qml\nsingleton DeviceKinds 1.0 DeviceKinds.qml\n' > "$work/t/qmldir"
cd "$work/t"
export XDG_CONFIG_HOME=$work/cfg XDG_DATA_HOME=$work/data
QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen "$runner" -input . "$@"
