//! KDE i18n context for QML (see i18n.cpp).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("i18n.h");
        fn tsuba_set_language(language: &QString);
        fn tsuba_install_i18n(engine: Pin<&mut QQmlApplicationEngine>);
        fn tsuba_set_app_identity();
    }
}
