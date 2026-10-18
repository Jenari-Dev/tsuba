#include "i18n.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

#include <KLocalization>
#include <KLocalizedQmlContext>

void tsuba_install_i18n(QQmlApplicationEngine &engine)
{
    KLocalization::setupLocalizedContext(&engine);
}

void tsuba_set_app_identity()
{
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.jenaridev.tsuba"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.jenaridev.tsuba")));
}
