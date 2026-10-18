import QtQuick
import QtTest

// Lighting Studio per-layer options: gradients, opacity / speed, names, copies, images, profile files.
Item {
    width: 1400
    height: 860
    QtObject {
        id: fake
        property bool readonly: true
        property string status: ""
        property string devices: JSON.stringify([{ pid: "028d", kind: "keyboard", name: "K" }, { pid: "00aa", kind: "mouse", name: "M" }])
        property var calls: []
        property var importReply: ({ ok: true, profile: { name: "Imported", devices: { "028d": { layers: [{ name: "x", enabled: true, leds: [], effect: { type: "battery" } }] } } } })
        function device_pack(p) { return ""; }
        function chroma_profiles() { return JSON.stringify({ profiles: [] }); }
        function config(p) { return "{}"; }
        function env_var(n) { return ""; }
        function preview_start(p, j) { return true; }
        function preview_frame_for(p) { return "{}"; }
        function set_effect(p, j, b) { return true; }
        function studio_call(m, a) {
            calls.push(m);
            if (m === "expand") { return "{}"; }
            if (m === "export") { return JSON.stringify({ ok: true, path: "/tmp/x.json" }); }
            if (m === "import") { return JSON.stringify(importReply); }
            if (m === "audio_sources") { return JSON.stringify([{ name: "mic", description: "Mic", monitor: false }]); }
            return "{}";
        }
    }
    StudioPage {
        id: studio
        anchors.fill: parent
        backend: fake
        pid: "028d"
    }
    TestCase {
        name: "StudioOptions"
        when: windowShown

        function init() {
            wait(50);
            studio.showCombined(false);
            studio.switchDevice("028d");
            studio.layers = [];
            studio.current = -1;
        }

        function test_new_layers_carry_opacity_and_speed() {
            studio.addLayer("static");
            compare(studio.layers[0].opacity, 1);
            compare(studio.layers[0].speed, 1);
            compare(studio.layers[0].brightness, 1);
            studio.updateCurrent(l => { l.opacity = 0.4; l.speed = 2; });
            compare(studio.layers[0].opacity, 0.4);
            compare(studio.layers[0].speed, 2);
            // Reset keeps the layer options and only restores the effect.
            studio.updateCurrent(l => l.effect.colour = "ff0000");
            studio.resetEffect();
            compare(studio.layers[0].effect.colour, "44d62c");
            compare(studio.layers[0].opacity, 0.4);
        }

        function test_gradient_stops() {
            studio.addLayer("gradient");
            compare(studio.layers[0].effect.colours.length, 2);
            studio.editingColour = 0;
            studio.addStop();
            compare(studio.layers[0].effect.colours.length, 3);
            compare(studio.editingColour, 1, "the new stop is selected");
            studio.setColour("ff0000");
            compare(studio.layers[0].effect.colours[1], "ff0000");
            studio.removeStop();
            compare(studio.layers[0].effect.colours.length, 2);
            studio.removeStop();
            compare(studio.layers[0].effect.colours.length, 2, "a gradient keeps two stops");
            for (let i = 0; i < 10; i++) {
                studio.addStop();
            }
            compare(studio.layers[0].effect.colours.length, 8, "at most eight stops");
        }

        function test_wave_can_use_many_colours_or_rainbow() {
            studio.addLayer("wave");
            compare(studio.layers[0].effect.colours.length, 0);
            studio.updateCurrent(l => l.effect.colours = ["ff0000", "00ff00"]);
            studio.editingColour = 1;
            studio.addStop();
            compare(studio.layers[0].effect.colours.length, 3);
            const mode = findChild(studio, "colourMode");
            verify(mode !== null);
            compare(mode.currentIndex, 1);
            mouseClick(mode.children[0], 10, 10);   // Rainbow
            wait(50);
            compare(studio.layers[0].effect.colours.length, 0);
        }

        function test_copy_and_rename() {
            studio.addLayer("fire");
            const name = findChild(studio, "layerName");
            verify(name !== null);
            name.text = "Embers";
            name.editingFinished();
            compare(studio.layers[0].name, "Embers");
            studio.duplicateLayer(0);
            compare(studio.layers.length, 2);
            compare(studio.layers[1].name, "Embers copy");
            compare(studio.current, 1);
            // Editing the copy leaves the original alone.
            studio.updateCurrent(l => l.effect.duration = "long");
            compare(studio.layers[0].effect.duration, "medium");
        }

        function test_image_pixels_are_hex() {
            // 2 pixels: opaque red, half-transparent blue (premultiplied onto black).
            const hex = studio.pixelsHex({ data: [255, 0, 0, 255, 0, 0, 255, 128] }, 2, 1);
            compare(hex, "ff0000000080");
            studio.addLayer("image");
            compare(studio.layers[0].effect.w, 0);
            studio.updateCurrent(l => { l.effect.w = 2; l.effect.h = 1; l.effect.data = hex; });
            compare(studio.layers[0].effect.data.length, 12);
        }

        function test_audio_meter_options() {
            studio.addLayer("audio_meter");
            compare(studio.layers[0].effect.mode, "spectrum");
            const src = findChild(studio, "audioSource");
            verify(src !== null);
            wait(50);
            compare(src.model.length, 2, "default + the daemon's source");
            studio.updateCurrent(l => { l.effect.source = "mic"; l.effect.mode = "level"; l.effect.peak_hold = true; });
            compare(studio.layers[0].effect.source, "mic");
            wait(50);
            compare(src.currentIndex, 1);
        }

        function test_wheel_centre_and_spread_defaults() {
            studio.addLayer("wheel");
            const e = studio.layers[0].effect;
            compare(e.centre[0], 0.5);
            compare(e.spread, 1);
            const centre = findChild(studio, "wheelCentre");
            compare(centre.currentIndex, 0);
            studio.updateCurrent(l => l.effect.centre = [0.1, 0.5]);
            wait(30);
            compare(centre.currentIndex, 1);
        }

        function test_profile_import_and_export() {
            studio.addLayer("static");
            fake.calls = [];
            studio.exportProfile("file:///tmp/x.json");
            compare(fake.calls[fake.calls.length - 1], "export");
            verify(studio.status.indexOf("Exported") === 0, studio.status);
            verify(!studio.statusBad);
            studio.importProfile("file:///tmp/y.json");
            compare(studio.profile.name, "Imported");
            compare(studio.layers.length, 1);
            compare(studio.loadedName, "", "an import is not saved until you save it");
            verify(studio.dirty);
            fake.importReply = { ok: false, error: "not a Tsuba lighting profile file" };
            studio.importProfile("file:///tmp/bad.json");
            verify(studio.statusBad);
            verify(studio.status.indexOf("not a Tsuba") >= 0, studio.status);
            fake.importReply = { ok: true, profile: { name: "Imported", devices: {} } };
        }

        function test_canvas_quick_select_entries() {
            studio.showCombined(true);
            studio.addLayer("static");
            const t = studio.quickEntries.map(e => e.text);
            verify(t.indexOf("All devices") === 0, t.join());
            verify(t.indexOf("Keyboard: all") >= 0 && t.indexOf("Mouse: Logo") >= 0, t.join());
            // "Mouse: all" leaves only the mouse covered.
            studio.applyQuick(studio.quickEntries.find(e => e.text === "Mouse: all"));
            compare(Object.keys(studio.layers[0].targets).join(), "00aa");
            studio.applyQuick(studio.quickEntries[0]);
            compare(Object.keys(studio.layers[0].targets).length, 2);
            studio.applyQuick(studio.quickEntries.find(e => e.text === "Mouse: Logo"));
            compare(studio.layers[0].targets["00aa"].length, 1);
            studio.showCombined(false);
        }
    }
}
