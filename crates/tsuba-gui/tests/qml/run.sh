#!/bin/sh
# Headless QML tests: copies the app's QML next to the tests (singletons via qmldir) and runs
# them with Qt 6's qmltestrunner on the offscreen platform.
set -e
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$here"/../../qml/*.qml "$here"/tst_*.qml "$work"/
printf 'singleton Theme 1.0 Theme.qml\nsingleton KeyNames 1.0 KeyNames.qml\nsingleton DeviceKinds 1.0 DeviceKinds.qml\n' > "$work/qmldir"
cd "$work"
QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input .
