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
        fn tsuba_translate(context: &QString, message: &QString) -> QString;
    }
}

/// Translate an English UI message made in Rust. Placeholders are `%1`, `%2`... (filled by [`trf`]).
/// `po/update.sh` extracts the literal argument of `tr(`, `trc(` and `trf(` from the Rust sources.
pub fn tr(message: &str) -> String {
    trc("", message)
}

/// Like [`tr`] with a translator context (`trc("button", "Open")`).
pub fn trc(context: &str, message: &str) -> String {
    use cxx_qt_lib::QString;
    String::from(&ffi::tsuba_translate(&QString::from(context), &QString::from(message)))
}

/// Translate `message` and fill `%1`, `%2`... from `args`.
pub fn trf(message: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = tr(message);
    for (i, a) in args.iter().enumerate().rev() {
        out = out.replace(&format!("%{}", i + 1), &a.to_string());
    }
    out
}
