import QtQuick
import QtTest

Item {
    width: 1400
    height: 860
    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: "[]"
        property string lastEffect: ""
        function device_pack(p) { return ""; }
        function chroma_profiles() { return JSON.stringify({ profiles: [{ name: "P", devices: { "028d": { layers: [{ name: "Wheel", enabled: true, leds: [], effect: { type: "wheel", direction: "clockwise", duration: "medium" } }, { name: "Fire", enabled: true, leds: [], effect: { type: "fire", duration: "medium" } }] } } }], active: "P" }); }
        function config(p) { return "{}"; }
        function preview_start(p, j) { return true; }
        function preview_frame() { return "{}"; }
        function set_effect(p, j, b) { lastEffect = j; return true; }
        function env_var(n) { return ""; }
    }
    StudioPage {
        id: page
        anchors.fill: parent
        backend: fake
        pid: "028d"
    }
    TestCase {
        name: "LayerBrightness"
        when: windowShown
        function test_slider() {
            wait(300);
            compare(page.layers.length, 2);
            page.current = 0;
            wait(100);
            const s = findChild(page, "layerBrightness");
            verify(s !== null, "slider found");
            console.log("slider w", s.width, "h", s.height, "visible", s.visible, "value", s.value);
            mouseClick(s, s.width * 0.3, s.height / 2);
            wait(400);
            console.log("layer brightness", page.layers[0].brightness, "sent", fake.lastEffect.slice(0, 160));
            verify(page.layers[0].brightness !== undefined && page.layers[0].brightness < 0.6, "brightness changed");
        }

        function test_blend_follows_layer() {
            wait(200);
            const tabs = findChild(page, "blendTabs");
            verify(tabs !== null, "blend tabs found");
            page.current = 1;                       // Fire
            wait(100);
            mouseClick(tabs.children[1], 10, 10);   // "Add"
            wait(200);
            compare(page.layers[1].blend, "add");
            compare(page.layers[0].blend === undefined ? "cover" : page.layers[0].blend, "cover");
            page.current = 0;                       // Wheel
            wait(200);
            compare(tabs.selected, 0, "Wheel shows Cover, not Fire's Add");
            page.current = 1;
            wait(200);
            compare(tabs.selected, 1, "Fire shows Add");
        }
    }
}
