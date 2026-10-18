import QtQuick
import QtTest 1.2

Item {
    width: 900
    height: 500

    QtObject {
        id: fake
        property int started: 0
        property string copied: ""
        function start_service() { started++; }
        function copy_text(t) { copied = t; }
    }
    SetupGuide {
        id: guide
        width: parent.width
        app: fake
    }
    SignalSpy { id: recheckSpy; target: guide; signalName: "recheck" }
    SignalSpy { id: supportedSpy; target: guide; signalName: "openSupported" }

    TestCase {
        name: "ShellSetupGuide"
        when: windowShown

        function setup(connected, report) {
            guide.connected = connected;
            guide.report = report;
            wait(60);   // let the layout settle before clicking
        }
        function title() { return findChild(guide, "guideTitle").text; }

        function test_service_states() {
            setup(false, { service: { unit_installed: true, active: "inactive" } });
            compare(guide.state, "stopped");
            verify(findChild(guide, "guideStart").visible);
            mouseClick(findChild(guide, "guideStart"));
            compare(fake.started, 1);
            setup(false, { service: { unit_installed: false } });
            compare(guide.state, "no_unit");
            verify(!findChild(guide, "guideStart").visible);
        }

        function test_permission_with_and_without_rule() {
            setup(true, { diagnosis: "permission", udev: { rule_installed: false } });
            compare(guide.state, "permission");
            verify(findChild(guide, "guideCommand").text.indexOf("udevadm control --reload-rules") > 0);
            mouseClick(findChild(guide, "guideCopy"));
            compare(fake.copied, guide.ruleCommand);
            setup(true, { diagnosis: "permission", udev: { rule_installed: true } });
            verify(!findChild(guide, "guideCopy").visible);
            verify(findChild(guide, "guideBody").text.indexOf("replug") > 0);
        }

        function test_other_diagnoses() {
            setup(true, { diagnosis: "no_hidraw" });
            compare(findChild(guide, "guideCommand").text, "tsuba-handover");
            setup(true, { diagnosis: "no_device" });
            verify(findChild(guide, "guideBody").text.indexOf("USB") > 0);
            setup(true, { diagnosis: "unsupported_only", usb: [{ pid: "1234", count: 1 }, { pid: "028d", count: 1, name: "BlackWidow V4 Pro" }] });
            compare(guide.unsupportedNames, ["1234"]);
            compare(guide.state, "unsupported_only");
            setup(true, { diagnosis: "ok" });
            compare(guide.state, "listed_none");
            verify(findChild(guide, "guideStart").visible);
            setup(true, {});
            compare(guide.state, "unknown");
        }

        function test_recheck() {
            recheckSpy.clear();
            setup(true, { diagnosis: "no_device" });
            mouseClick(findChild(guide, "guideRecheck"));
            compare(recheckSpy.count, 1);
        }
    }
}
