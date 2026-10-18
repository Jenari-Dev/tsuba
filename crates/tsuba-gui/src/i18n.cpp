#include "i18n.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

#include <KLocalization>
#include <KLocalizedQmlContext>
#include <KLocalizedString>

void tsuba_install_i18n(QQmlApplicationEngine &engine)
{
    // Catalogs: <data dir>/locale/<lang>/LC_MESSAGES/tsuba.mo
    KLocalizedString::setApplicationDomain("tsuba");
    KLocalization::setupLocalizedContext(&engine);
}

void tsuba_set_language(const QString &language)
{
    // An explicit choice from Settings; English strings are the fallback for anything untranslated.
    KLocalizedString::setLanguages({language, QStringLiteral("en_US")});
}

void tsuba_set_app_identity()
{
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.jenaridev.tsuba"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.jenaridev.tsuba")));
}
