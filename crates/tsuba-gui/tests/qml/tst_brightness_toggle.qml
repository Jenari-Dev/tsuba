import QtQuick
import QtTest 1.2

Item {
    width: 1200
    height: 800
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property var sent: []
        function config(p) { return "{}"; }
        function lighting_power(p) { return ""; }
        function set_lighting_power(p, j) { return true; }
        function set_brightness(p, v) { sent = sent.concat([v]); return true; }
        function set_effect(p, j, b) { return true; }
        function save_lighting(p, j, b) { return true; }
    }
    LightingTab {
        id: tab
        anchors.fill: parent
        backend: fake
        pid: "028d"
        st: ({ kind: "keyboard", lighting: [{ id: 5, name: "backlight", rows: 9, cols: 24, effect: "Static", colours: ["44d62c"], brightness: 60, supported: ["Off", "Static"] }] })
    }
    TestCase {
        name: "BrightnessToggle"
        when: windowShown
        function test_toggle() {
            wait(200);
            const t = findChild(tab, "brightnessToggle");
            const s = findChild(tab, "brightnessSlider");
            verify(t !== null && s !== null);
            compare(s.value, 60);
            t.toggled(false);
            wait(300);
            compare(s.value, 0);
            compare(fake.sent[fake.sent.length - 1], 0);
            t.toggled(true);
            wait(300);
            compare(s.value, 60);
            compare(fake.sent[fake.sent.length - 1], 60);
        }
    }
}
