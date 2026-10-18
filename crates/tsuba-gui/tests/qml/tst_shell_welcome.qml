import QtQuick
import QtTest 1.2

Item {
    width: 1000
    height: 800

    QtObject {
        id: app
        property bool autostart: false
        function autostart_enabled() { return autostart; }
        function set_autostart(on) { autostart = on; return true; }
    }
    FirstRunPanel {
        id: panel
        app: app
        devices: []
    }
    SignalSpy { id: finishedSpy; target: panel; signalName: "finished" }
    SignalSpy { id: recheckSpy; target: panel; signalName: "recheck" }

    TestCase {
        name: "ShellWelcome"
        when: windowShown

        function init() {
            Theme.app.onboarded = false;
            finishedSpy.clear();
            panel.open();
            wait(60);
        }
        function cleanup() {
            panel.close();
            wait(30);
        }

        function test_steps_and_finish() {
            compare(panel.step, 0);
            verify(findChild(panel, "welcomeNotice").text.indexOf("not affiliated") >= 0);
            verify(!findChild(panel, "welcomeBack").visible);
            mouseClick(findChild(panel, "welcomeNext"));
            compare(panel.step, 1);
            verify(findChild(panel, "welcomeBack").visible);
            mouseClick(findChild(panel, "welcomeNext"));
            compare(panel.step, 2);
            verify(!findChild(panel, "welcomeSkip").visible);
            verify(!Theme.app.onboarded);
            mouseClick(findChild(panel, "welcomeNext"));
            verify(Theme.app.onboarded);
            compare(finishedSpy.count, 1);
            verify(!panel.opened);
        }

        function test_skip_counts_as_done() {
            mouseClick(findChild(panel, "welcomeSkip"));
            verify(Theme.app.onboarded);
            compare(finishedSpy.count, 1);
        }

        function test_devices_step_reports_what_was_found() {
            panel.step = 1;
            wait(30);
            verify(findChild(panel, "devicesStatus").text.indexOf("No device") === 0);
            panel.devices = [{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }];
            wait(30);
            verify(findChild(panel, "devicesStatus").text.indexOf("Found 1") === 0);
            panel.devices = [];
        }

        function test_choices_are_applied_immediately() {
            panel.step = 2;
            wait(30);
            findChild(panel, "welcomeTheme").activated(1);
            compare(Theme.prefs.themeMode, "light");
            findChild(panel, "welcomeTheme").activated(0);
            findChild(panel, "welcomeLanguage").activated(2);
            compare(Theme.app.language, "ja");
            findChild(panel, "welcomeLanguage").activated(0);
            mouseClick(findChild(panel, "welcomeAutostart"));
            compare(app.autostart, true);
            app.autostart = false;
        }
    }
}
