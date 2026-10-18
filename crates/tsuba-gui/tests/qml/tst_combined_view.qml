import QtQuick
import QtTest

Item {
    width: 1400
    height: 860
    QtObject {
        id: fake
        property bool readonly: true
        property string status: ""
        property string devices: JSON.stringify([{ pid: "028d", kind: "keyboard", name: "K" }, { pid: "00aa", kind: "mouse", name: "M" }])
        property var started: []
        function device_pack(p) { return ""; }
        function chroma_profiles() { return JSON.stringify({ profiles: [] }); }
        function config(p) { return "{}"; }
        function env_var(n) { return ""; }
        function preview_start(p, j) { started = started.concat([p]); return true; }
        function preview_frame() { return "{}"; }
        function preview_frame_for(p) { return "{}"; }
        function set_effect(p, j, b) { return true; }
    }
    StudioPage {
        id: studio
        anchors.fill: parent
        backend: fake
        pid: "028d"
    }
    ZoomCanvas {
        id: zc
        width: 400
        height: 300
        Rectangle {
            anchors.fill: parent
            color: "red"
        }
    }
    TestCase {
        name: "CombinedView"
        when: windowShown
        function test_selection_switches_device() {
            wait(100);
            compare(studio.tabNames.length, 3);
            studio.addLayer("static");
            studio.showCombined(true);
            verify(studio.combined);
            // Both devices get a live preview, the edited one last.
            verify(fake.started.indexOf("00aa") >= 0);
            compare(fake.started[fake.started.length - 1], "028d");
            // Picking a LED on the mouse makes it the edited device, keeping the keyboard's unsaved layer.
            studio.pickLeds("00aa", { "0,1": true });
            compare(studio.editPid, "00aa");
            verify(studio.combined);
            compare(studio.profile.devices["028d"].layers.length, 1);
            compare(studio.layers.length, 0);
            studio.pickLeds("028d", { "0,2": true });
            compare(studio.editPid, "028d");
            compare(studio.layers.length, 1);
            compare(studio.layers[0].leds.length, 1);
        }
        function test_zoom_around_cursor() {
            zc.fit();
            // content point under the cursor stays put while zooming
            const before = zc.contentLayer.mapFromItem(zc, 100, 80);
            zc.zoomAt(2.0, 100, 80);
            const after = zc.contentLayer.mapFromItem(zc, 100, 80);
            compare(zc.zoom, 2.0);
            fuzzyCompare(after.x, before.x, 0.01);
            fuzzyCompare(after.y, before.y, 0.01);
            zc.zoomAt(10, 100, 80);
            compare(zc.zoom, 3.0);
            zc.zoomAt(0.01, 100, 80);
            compare(zc.zoom, 0.5);
            zc.fit();
            compare(zc.zoom, 1.0);
        }
        function test_wheel_zooms_and_box_in_content_coords() {
            zc.fit();
            var got = null;
            zc.boxSelected.connect((r, a) => got = r);
            mouseWheel(zc, 200, 150, 0, 120);
            verify(zc.zoom > 1.0);
            mousePress(zc, 20, 20, Qt.LeftButton);
            mouseMove(zc, 120, 100);
            mouseRelease(zc, 120, 100, Qt.LeftButton);
            verify(got !== null);
            // 100 px of screen at this zoom is less than 100 content px
            verify(got.width < 100 && got.width > 0);
        }
    }
}
