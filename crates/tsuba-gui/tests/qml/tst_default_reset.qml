import QtQuick
import QtTest

Item {
    width: 400
    height: 700
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: "[]"
        property var written: []
        function set_assignment(p, j) { written = written.concat([JSON.parse(j)]); return "ok"; }
        function macros() { return ""; }
        function actions(p) { return "[]"; }
    }
    AssignmentPanel {
        id: panel
        anchors.fill: parent
        backend: fake
        pid: "00aa"
        isMouse: true
        profile: 1
        keyLayer: 0
        target: 1
        targetName: "Left Click"
        assignment: ({ key: 1, function: 1, data: [4], label: "" })
    }
    TestCase {
        name: "DefaultReset"
        when: windowShown
        function test_layer_aware_tables() {
            // Factory tables for both layers (dumped from default profiles).
            compare(KeyNames.defaultFor(true, 1, 1).function, 1);
            compare(KeyNames.defaultFor(true, 1, 0).data[0], 1);
            compare(KeyNames.defaultFor(true, 52, 0).function, 14);       // wheel tilt = repeat-scroll button
            compare(KeyNames.defaultFor(false, 16, 0).function, 2);       // Tab = key
            compare(KeyNames.defaultFor(false, 160, 1).function, 17);     // M1 on the shift layer = device key
            compare(KeyNames.defaultFor(false, 169, 0).function, 17);     // dial press
            compare(KeyNames.defaultFor(false, 9999, 0), null);
        }
        function test_reset_writes_default() {
            wait(50);
            panel.openCategory("default");
            wait(50);
            const b = findChild(panel, "restoreDefault");
            verify(b !== null);
            verify(b.enabled);
            b.clicked();
            // Choosing only prepares the change; SAVE writes it.
            compare(fake.written.length, 0);
            verify(panel.staged !== null);
            panel.commit();
            compare(fake.written.length, 1);
            compare(fake.written[0].key, 1);
            compare(fake.written[0].layer, 0);
            compare(fake.written[0].function, 1);
            compare(fake.written[0].data[0], 1);
        }
        function test_unknown_default_is_disabled() {
            panel.isMouse = false;
            panel.target = 9999;
            panel.openCategory("default");
            wait(50);
            const b = findChild(panel, "restoreDefault");
            verify(!b.enabled);
        }
    }
}
