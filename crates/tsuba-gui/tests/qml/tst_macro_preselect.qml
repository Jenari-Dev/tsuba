import QtQuick
import QtTest 1.2

Item {
    id: top
    width: 340
    height: 760
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: "[]"
        function macros() {
            return JSON.stringify({
                library: [{ guid: "g1", name: "Copy line" }, { guid: "g2", name: "Sign off" }],
                devices: { "00aa": { macros: [{ id: 3, guid: "g1", name: "Copy line", events: [] }, { id: 9, guid: "g2", name: "Sign off", events: [] }], available: 100 } }
            });
        }
        function env_var(n) { return ""; }
    }
    AssignmentPanel {
        id: panel
        anchors.fill: parent
        backend: fake
        pid: "00aa"
        target: 4
        targetName: "Button 4"
        profile: 1
        assignment: ({ key: 4, function: 3, data: [0, 9, 5] })
    }
    TestCase {
        name: "MacroPreselect"
        when: windowShown
        function test_bound_macro_preselected() {
            panel.loadMacros();
            compare(panel.macroIdx, 1);
            compare(panel.playback, 1);
            compare(panel.repeatCount, 5);
            panel.assignment = { key: 4, function: 5, data: [0, 3] };
            panel.loadMacros();
            compare(panel.macroIdx, 0);
            compare(panel.playback, 3);
            panel.assignment = { key: 4, function: 2, data: [0, 4] };
            panel.loadMacros();
            compare(panel.macroIdx, 0);
            compare(panel.playback, 0);
        }
    }
}
