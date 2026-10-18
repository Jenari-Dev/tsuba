import QtQuick
import QtTest 1.2

Item {
    width: 1400
    height: 860
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: JSON.stringify([{ pid: "028d", name: "BlackWidow V4 Pro", kind: "keyboard" }])
        property bool imported: false
        property bool fail: false
        readonly property var devMacro: ({ id: 7, guid: "gg", name: "Long one", events: [
            { type: "key", hid: 4, down: true }, { type: "delay", ms: 20 }, { type: "key", hid: 4, down: false },
            { type: "key", hid: 5, down: true }] })
        function macros() {
            return JSON.stringify({
                library: imported ? [devMacro] : [],
                devices: { "028d": { macros: [devMacro], available: 4096 } }
            });
        }
        function import_macro(pid, id) {
            if (fail) {
                status = "device busy";
                return "";
            }
            imported = true;
            return "gg";
        }
        function env_var(n) { return ""; }
    }
    MacrosPage {
        id: page
        anchors.fill: parent
        backend: fake
    }
    TestCase {
        name: "DeviceMacros"
        when: windowShown

        function test_view_and_import() {
            compare(page.deviceOnly().length, 1);
            page.openDeviceMacro("028d", fake.devMacro);
            verify(page.readOnly);
            compare(page.events.length, 4);
            verify(page.looksIncomplete, "unreleased key = truncated");
            // Failure is surfaced and the read-only view stays.
            fake.fail = true;
            page.importMacro("028d", 7);
            verify(page.result.indexOf("device busy") >= 0, page.result);
            verify(page.readOnly);
            // Success lists it in the library and opens it editable.
            fake.fail = false;
            page.importMacro("028d", 7);
            verify(page.result.indexOf("Imported") === 0, page.result);
            verify(!page.readOnly);
            compare(page.lib.length, 1);
            compare(page.deviceOnly().length, 0);
        }
    }
}
