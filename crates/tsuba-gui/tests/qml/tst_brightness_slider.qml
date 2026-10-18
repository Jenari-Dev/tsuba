import QtQuick
import QtTest

Item {
    width: 1200
    height: 800
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        function config(p) { return "{}"; }
        function lighting_power(p) { return JSON.stringify({ idle_minutes: 0, display_off: false, display_off_minutes: 10 }); }
        function set_lighting_power(p, j) { return true; }
        function set_brightness(p, v) { return true; }
        function set_effect(p, j, b) { return true; }
        function save_lighting(p, j, b) { return true; }
    }
    LightingTab {
        id: tab
        anchors.fill: parent
        backend: fake
        pid: "028d"
        st: ({ kind: "keyboard", lighting: [{ id: 5, name: "backlight", rows: 9, cols: 24, effect: "CustomFrame", colours: [], brightness: 0, supported: ["Off", "Static", "Breathing", "Spectrum", "Wave", "Reactive", "Starlight", "Ripple", "CustomFrame"] }] })
    }
    TestCase {
        name: "Slider"
        when: windowShown
        function test_drag() {
            wait(200);
            const s = findChild(tab, "brightnessSlider");
            verify(s !== null, "slider found");
            console.log("before", s.value, "enabled", s.enabled, "visible", s.visible, "w", s.width);
            // What is on top at the click point?
            function topAt(item, x, y, depth) {
                let hit = null;
                for (let i = item.children.length - 1; i >= 0; i--) {
                    const c = item.children[i];
                    if (!c.visible || c.opacity === 0) continue;
                    const p = item.mapToItem(c, x, y);
                    if (p.x >= 0 && p.y >= 0 && p.x < c.width && p.y < c.height) {
                        hit = topAt(c, p.x, p.y, depth + 1) || c;
                        break;
                    }
                }
                return hit;
            }
            const root = tab;
            const gp = s.mapToItem(root, s.width * 0.8, s.height / 2);
            const top = topAt(root, gp.x, gp.y, 0);
            console.log("top item:", top, top && top.objectName, "z", top && top.z);
            mouseClick(s, s.width * 0.8, s.height / 2);
            wait(100);
            console.log("after click", s.value);
            mouseDrag(s, 5, s.height / 2, s.width * 0.5, 0);
            wait(100);
            console.log("after drag", s.value);
            verify(s.value > 30, "slider moved");
        }
    }
}
