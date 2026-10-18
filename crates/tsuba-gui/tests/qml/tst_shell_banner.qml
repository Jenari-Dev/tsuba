import QtQuick
import QtTest 1.2

Item {
    width: 900
    height: 200

    DeviceStateBanner {
        id: bar
        width: parent.width
        text: "OpenRGB is running."
        detail: "Close one of them."
        actionText: "Try again"
        dismissible: true
    }
    SignalSpy { id: actionSpy; target: bar; signalName: "action" }
    SignalSpy { id: dismissSpy; target: bar; signalName: "dismissed" }

    TestCase {
        name: "ShellBanner"
        when: windowShown

        function test_text_action_and_dismiss() {
            compare(findChild(bar, "bannerText").text, "OpenRGB is running.");
            verify(bar.height >= 36);
            const act = findChild(bar, "bannerAction");
            verify(act.visible);
            mouseClick(act);
            compare(actionSpy.count, 1);
            mouseClick(findChild(bar, "bannerClose"));
            compare(dismissSpy.count, 1);
        }

        function test_optional_parts_hide() {
            bar.actionText = "";
            bar.dismissible = false;
            verify(!findChild(bar, "bannerAction").visible);
            verify(!findChild(bar, "bannerClose").visible);
            bar.actionText = "Try again";
            bar.dismissible = true;
        }

        function test_severity_tone() {
            bar.severity = "error";
            verify(Qt.colorEqual(bar.tone, Theme.danger));
            bar.severity = "info";
            verify(Qt.colorEqual(bar.tone, Theme.accent));
            bar.severity = "warn";
            verify(Qt.colorEqual(bar.tone, Theme.warn));
        }
    }
}
