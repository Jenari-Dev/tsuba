import QtQuick
import QtTest 1.2

Item {
    width: 400
    height: 300
    HelpButton {
        id: hb
        x: 300
        y: 20
        text: "Some help text."
    }
    HelpButton {
        id: empty
        x: 20
        y: 20
        text: ""
    }
    TestCase {
        name: "HelpButton"
        when: windowShown
        function test_toggle() {
            verify(hb.visible);
            verify(!empty.visible, "no text, no button");
            verify(!hb.popupOpen);
            mouseClick(hb, hb.width / 2, hb.height / 2);
            tryVerify(() => hb.popupOpen);
            mouseClick(hb, hb.width / 2, hb.height / 2);
            tryVerify(() => !hb.popupOpen);
        }
    }
}
