import QtQuick
import QtTest 1.2

Item {
    id: top
    width: 1400
    height: 860

    QtObject {
        id: fake
        property bool readonly: false
        property string status: ""
        property string devices: "[]"
        function macros() { return JSON.stringify({ library: [], devices: {} }); }
        function env_var(n) { return ""; }
    }

    MacrosPage {
        id: page
        anchors.fill: parent
        backend: fake
    }

    TestCase {
        name: "Record"
        when: windowShown
        function test_mouse() {
            page.newMacro();
            wait(50);
            page.startRecording();
            wait(50);
            // recorder box is ~84 px tall under the name row; click its middle-left text area
            const rec = page;
            verify(rec !== null, "recorder found");
            mouseClick(rec, rec.width / 2, rec.height / 2, Qt.LeftButton);
            mouseClick(rec, rec.width / 2, rec.height / 2, Qt.RightButton);
            mouseWheel(rec, rec.width / 2, rec.height / 2, 0, 120);
            wait(50);
            console.log("events:", JSON.stringify(page.events));
            compare(page.events.filter(e => e.type !== "delay").length, 5);
            // clicking Stop ends recording without adding events
            const btn = findChild(page, "recordBtn");
            const n = page.events.length;
            mouseClick(btn, 5, 5, Qt.LeftButton);
            verify(!page.recording, "stopped");
            compare(page.events.length, n);
        }
    }
}
