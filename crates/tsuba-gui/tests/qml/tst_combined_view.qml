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
        function test_canvas_layers_target_devices() {
            wait(100);
            compare(studio.tabNames.length, 3);
            studio.addLayer("static");
            compare(studio.layers.length, 1);
            compare(studio.layers[0].targets, undefined, "a device layer has no canvas targets");
            studio.showCombined(true);
            verify(studio.combined);
            // Both devices get a live preview, the edited one last.
            verify(fake.started.indexOf("00aa") >= 0);
            compare(fake.started[fake.started.length - 1], "028d");
            // The canvas starts empty: the device layer stays on the keyboard.
            compare(studio.layers.length, 0);
            compare(studio.profile.devices["028d"].layers.length, 1);
            // A canvas layer starts on every device.
            studio.addLayer("wave");
            compare(Object.keys(studio.layers[0].targets).sort().join(), "00aa,028d");
            compare(studio.coverText(studio.layers[0]), "Keyboard + Mouse");
            // Picking LEDs on the mouse narrows its target; the keyboard keeps its whole device.
            studio.pickLeds("00aa", { "0,1": true });
            compare(studio.layers[0].targets["00aa"].length, 1);
            compare(studio.layers[0].targets["028d"].length, 0);
            compare(studio.editPid, "028d", "picking never switches the edited device");
            // Clearing a device's LEDs drops it from the layer.
            studio.pickLeds("00aa", {});
            compare(studio.layers[0].targets["00aa"], undefined);
            studio.toggleTarget("00aa");
            compare(Object.keys(studio.layers[0].targets).length, 2);
            studio.toggleTarget("00aa");
            studio.toggleTarget("028d");
            compare(Object.keys(studio.layers[0].targets).length, 1, "the last device stays");
        }
        function test_selection_per_device() {
            studio.showCombined(true);
            studio.current = studio.layers.length - 1;
            studio.cleared = false;
            studio.updateCurrent(l => l.targets = { "00aa": [[0, 0], [0, 1]] });
            compare(Object.keys(studio.selectionFor("00aa")).length, 2);
            compare(Object.keys(studio.selectionFor("028d")).length, 0);
        }
        function test_span_moves_a_layer_to_the_canvas_and_back() {
            studio.showCombined(false);
            studio.switchDevice("028d");
            const n = studio.layers.length;
            studio.addLayer("fire");
            compare(studio.layers.length, n + 1);
            studio.spanCurrent();
            verify(studio.combined);
            compare(studio.layers[studio.current].effect.type, "fire");
            verify(studio.layers[studio.current].targets !== undefined);
            studio.updateCurrent(l => l.targets = { "00aa": [] });
            const cn = studio.layers.length;
            studio.unspanCurrent();
            compare(studio.layers.length, cn - 1);
            // The layer now belongs to the mouse alone.
            studio.switchDevice("00aa");
            compare(studio.layers[studio.layers.length - 1].effect.type, "fire");
            compare(studio.layers[studio.layers.length - 1].targets, undefined);
            studio.switchDevice("028d");
        }
        function test_placements_move_and_reset() {
            const before = studio.placementOf("00aa");
            studio.movePlacement("00aa", before.x - 5, before.y);
            verify(studio.arranged);
            verify(studio.placementOf("00aa").x < before.x);
            studio.scalePlacement("00aa", 1.1);
            verify(studio.placementOf("00aa").w > before.w);
            // Devices cannot leave the stage.
            studio.movePlacement("00aa", -50, 999);
            compare(studio.placementOf("00aa").x, 0);
            verify(studio.placementOf("00aa").y + studio.placementOf("00aa").h <= studio.stageH + 1e-6);
            studio.resetPlacements();
            verify(!studio.arranged);
        }
        function test_profile_carries_the_canvas() {
            studio.showCombined(true);
            studio.addLayer("static");
            const p = studio.currentProfile();
            verify(p.canvas !== undefined);
            verify(p.canvas.layers.length >= 1);
            verify(p.canvas.placements["028d"] !== undefined);
            studio.showCombined(false);
            const q = studio.currentProfile();
            verify(q.canvas.layers.length >= 1, "canvas layers are kept while a device is edited");
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
