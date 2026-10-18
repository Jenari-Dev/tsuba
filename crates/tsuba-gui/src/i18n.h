#pragma once
#include <QtCore/QString>
#include <QtQml/QQmlApplicationEngine>

// Install KDE's i18n functions (i18n, i18nc, i18ndc, …) on the QML engine;
// Kirigami Addons components call them.
void tsuba_install_i18n(QQmlApplicationEngine &engine);

// Use this language (e.g. "ja") instead of the system's. Call before the first i18n() lookup.
void tsuba_set_language(const QString &language);

// Tie the windows to io.github.jenaridev.tsuba.desktop (Wayland app id → launcher/taskbar icon)
// and set the window icon from the icon theme.
void tsuba_set_app_identity();
