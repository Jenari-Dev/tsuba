import QtQuick
import QtTest 1.2

Item {
    width: 1400
    height: 860

    QtObject {
        id: fake
        property bool readonly: true
        property string status: ""
        property string devices: "[]"
        function device_pack(p) { return ""; }
        function chroma_profiles() { return JSON.stringify({ profiles: [] }); }
        function config(p) { return "{}"; }
        function env_var(n) { return ""; }
        function preview_start(p, j) { return true; }
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
    TestCase {
        name: "StudioHistory"
        when: windowShown
        function test_a_undo_redo() {
            wait(100);
            compare(studio.layers.length, 0);
            studio.addLayer("static");
            wait(500);   // past the 400 ms edit-merge window
            studio.addLayer("fire");
            compare(studio.layers.length, 2);
            verify(studio.canUndo);
            studio.undo();
            compare(studio.layers.length, 1);
            verify(studio.canRedo);
            studio.redo();
            compare(studio.layers.length, 2);
        }
        function test_z_colour_edit() {
            studio.current = 0;
            wait(500);
            studio.setColour("ff0000");
            compare(studio.layers[0].effect.colour, "ff0000");
            compare(studio.editedColour(), "ff0000");
            const f = findChild(studio, "colourFieldHEX");
            verify(f !== null);
            compare(f.text, "FF0000");
        }
    }
}
