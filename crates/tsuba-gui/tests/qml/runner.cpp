// Test runner for the QML tests: qmltestrunner plus KDE's i18n() functions on the engine, the way the
// app installs them, so components that call i18n("...") load under test. Built by run.sh.
#include <QtQuickTest/quicktest.h>
#include <QtQml/QQmlEngine>
#include <KLocalization>
#include <KLocalizedString>
#include <KLocalizedQmlContext>

class Setup : public QObject
{
    Q_OBJECT
public Q_SLOTS:
    void qmlEngineAvailable(QQmlEngine *engine)
    {
        KLocalizedString::setApplicationDomain("tsuba");
        KLocalization::setupLocalizedContext(engine);
    }
};

QUICK_TEST_MAIN_WITH_SETUP(tsuba, Setup)

#include "runner.moc"
