//! Tsuba desktop app (Kirigami). Device control goes through tsubad over D-Bus.

mod backend;
mod i18n;
mod studio_api;
mod synapse;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

fn main() {
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
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        i18n::ffi::tsuba_install_i18n(engine.as_mut());
        engine.load(&QUrl::from("qrc:/qt/qml/io/github/jenaridev/tsuba/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        std::process::exit(app.exec());
    }
}
