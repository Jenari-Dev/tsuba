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
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.Jenari_Dev.Tsuba"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.Jenari_Dev.Tsuba")));
}

QString tsuba_translate(const QString &context, const QString &message)
{
    const QByteArray ctx = context.toUtf8();
    const QByteArray msg = message.toUtf8();
    // KLocalizedString keeps the pointers only until toString() returns.
    return (ctx.isEmpty() ? ki18n(msg.constData()) : ki18nc(ctx.constData(), msg.constData())).toString();
}
