//! Tsuba desktop app (Kirigami). Device control goes through tsubad over D-Bus.

mod app_backend;
mod backend;
mod device_backend;
mod i18n;
mod studio_api;
mod library_backend;
mod synapse;
mod tray;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

fn main() {
    // One window per user: a second launch (menu, autostart, notification click) raises the first.
    // Screenshot / test runs never talk to a real running instance.
    if std::env::var_os("TSUBA_SCREENSHOT").is_none() && std::env::var_os("TSUBA_MULTI_INSTANCE").is_none() && !tray::claim_instance() {
        return;
    }
    // Breeze is the pure-QML Plasma style; org.kde.desktop would need QtWidgets.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        QQuickStyle::set_style(&QString::from("org.kde.breeze"));
    }
    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("Tsuba"));
        app.as_mut().set_organization_domain(&QString::from("jenaridev.github.io"));
    }
    i18n::ffi::tsuba_set_app_identity();
    // Language chosen in Settings (read from app.ini before any text is looked up); TSUBA_LANGUAGE overrides for tests.
    if let Some(lang) = std::env::var("TSUBA_LANGUAGE").ok().filter(|l| !l.is_empty()).or_else(app_backend::saved_language) {
        i18n::ffi::tsuba_set_language(&QString::from(&lang));
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        i18n::ffi::tsuba_install_i18n(engine.as_mut());
        engine.load(&QUrl::from("qrc:/qt/qml/io/github/jenaridev/tsuba/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        std::process::exit(app.exec());
    }
}
