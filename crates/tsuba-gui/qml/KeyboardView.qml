import QtQuick

// US full-size layout with the BlackWidow V4 Pro extras, drawn in key units.
// `pos` = AT-101 key position the firmware uses; `def` = default {mod, hid}.
Item {
    id: root

    property var assignments: ({})   // pos -> assignment
    property int selected: -1
    signal picked(int pos, string name)

    readonly property var layout: buildLayout()
    readonly property real minX: -1.2
    readonly property real minY: -1.6
    readonly property real spanX: 25.4
    readonly property real spanY: 8.2
    readonly property real u: Math.min(width / spanX, height / spanY)
    readonly property real offX: (width - spanX * u) / 2 - minX * u
    readonly property real offY: (height - spanY * u) / 2 - minY * u

    function k(list, pos, x, y, label, def, w, h, shape) {
        list.push({ pos: pos, x: x + 1.5, y: y, w: w || 1, h: h || 1, label: label, def: def || null, shape: shape || "key" });
    }

    function buildLayout() {
        const L = [];
        const H = (hid) => ({ mod: 0, hid: hid });
        const M = (mod) => ({ mod: mod, hid: 0 });
        // Row 0
        k(L, 110, 0, 0, "Esc", H(0x29));
        ["F1", "F2", "F3", "F4"].forEach((t, i) => k(L, 112 + i, 2 + i, 0, t, H(0x3a + i)));
        ["F5", "F6", "F7", "F8"].forEach((t, i) => k(L, 116 + i, 6.5 + i, 0, t, H(0x3e + i)));
        ["F9", "F10", "F11", "F12"].forEach((t, i) => k(L, 120 + i, 11 + i, 0, t, H(0x42 + i)));
        k(L, 124, 15.25, 0, "PrtSc", H(0x46));
        k(L, 125, 16.25, 0, "ScrLk", H(0x47));
        k(L, 126, 17.25, 0, "Pause", H(0x48));
        // Row 1
        k(L, 1, 0, 1.5, "`", H(0x35));
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].forEach((t, i) => k(L, 2 + i, 1 + i, 1.5, t, H(0x1e + i)));
        k(L, 12, 11, 1.5, "-", H(0x2d));
        k(L, 13, 12, 1.5, "=", H(0x2e));
        k(L, 15, 13, 1.5, "Backspace", H(0x2a), 2);
        k(L, 75, 15.25, 1.5, "Ins", H(0x49));
        k(L, 80, 16.25, 1.5, "Home", H(0x4a));
        k(L, 85, 17.25, 1.5, "PgUp", H(0x4b));
        k(L, 90, 18.5, 1.5, "Num", H(0x53));
        k(L, 95, 19.5, 1.5, "/", H(0x54));
        k(L, 100, 20.5, 1.5, "*", H(0x55));
        k(L, 105, 21.5, 1.5, "-", H(0x56));
        // Row 2
        k(L, 16, 0, 2.5, "Tab", H(0x2b), 1.5);
        [[0x14, "Q"], [0x1a, "W"], [0x08, "E"], [0x15, "R"], [0x17, "T"], [0x1c, "Y"], [0x18, "U"], [0x0c, "I"], [0x12, "O"], [0x13, "P"]]
            .forEach((p, i) => k(L, 17 + i, 1.5 + i, 2.5, p[1], H(p[0])));
        k(L, 27, 11.5, 2.5, "[", H(0x2f));
        k(L, 28, 12.5, 2.5, "]", H(0x30));
        k(L, 29, 13.5, 2.5, "\\", H(0x31), 1.5);
        k(L, 76, 15.25, 2.5, "Del", H(0x4c));
        k(L, 81, 16.25, 2.5, "End", H(0x4d));
        k(L, 86, 17.25, 2.5, "PgDn", H(0x4e));
        k(L, 91, 18.5, 2.5, "7", H(0x5f));
        k(L, 96, 19.5, 2.5, "8", H(0x60));
        k(L, 101, 20.5, 2.5, "9", H(0x61));
        k(L, 106, 21.5, 2.5, "+", H(0x57), 1, 2);
        // Row 3
        k(L, 30, 0, 3.5, "Caps Lock", H(0x39), 1.75);
        [[0x04, "A"], [0x16, "S"], [0x07, "D"], [0x09, "F"], [0x0a, "G"], [0x0b, "H"], [0x0d, "J"], [0x0e, "K"], [0x0f, "L"]]
            .forEach((p, i) => k(L, 31 + i, 1.75 + i, 3.5, p[1], H(p[0])));
        k(L, 40, 10.75, 3.5, ";", H(0x33));
        k(L, 41, 11.75, 3.5, "'", H(0x34));
        k(L, 43, 12.75, 3.5, "Enter", H(0x28), 2.25);
        k(L, 92, 18.5, 3.5, "4", H(0x5c));
        k(L, 97, 19.5, 3.5, "5", H(0x5d));
        k(L, 102, 20.5, 3.5, "6", H(0x5e));
        // Row 4
        k(L, 44, 0, 4.5, "Shift", M(0x02), 2.25);
        [[0x1d, "Z"], [0x1b, "X"], [0x06, "C"], [0x19, "V"], [0x05, "B"], [0x11, "N"], [0x10, "M"]]
            .forEach((p, i) => k(L, 46 + i, 2.25 + i, 4.5, p[1], H(p[0])));
        k(L, 53, 9.25, 4.5, ",", H(0x36));
        k(L, 54, 10.25, 4.5, ".", H(0x37));
        k(L, 55, 11.25, 4.5, "/", H(0x38));
        k(L, 57, 12.25, 4.5, "Shift", M(0x20), 2.75);
        k(L, 83, 16.25, 4.5, "↑", H(0x52));
        k(L, 93, 18.5, 4.5, "1", H(0x59));
        k(L, 98, 19.5, 4.5, "2", H(0x5a));
        k(L, 103, 20.5, 4.5, "3", H(0x5b));
        k(L, 108, 21.5, 4.5, "Enter", H(0x58), 1, 2);
        // Row 5
        k(L, 58, 0, 5.5, "Ctrl", M(0x01), 1.25);
        k(L, 127, 1.25, 5.5, "Win", M(0x08), 1.25);
        k(L, 60, 2.5, 5.5, "Alt", M(0x04), 1.25);
        k(L, 61, 3.75, 5.5, "", H(0x2c), 6.25);
        k(L, 62, 10, 5.5, "Alt", M(0x40), 1.25);
        k(L, 59, 11.25, 5.5, "Fn", null, 1.25);
        k(L, 129, 12.5, 5.5, "Menu", H(0x65), 1.25);
        k(L, 64, 13.75, 5.5, "Ctrl", M(0x10), 1.25);
        k(L, 79, 15.25, 5.5, "←", H(0x50));
        k(L, 84, 16.25, 5.5, "↓", H(0x51));
        k(L, 89, 17.25, 5.5, "→", H(0x4f));
        k(L, 99, 18.5, 5.5, "0", H(0x62), 2);
        k(L, 104, 20.5, 5.5, ".", H(0x63));
        // BlackWidow V4 Pro extras (positions from the firmware keymap).
        [0, 1.5, 2.5, 3.5, 4.5, 5.5].forEach((y, i) => k(L, 170 + i, -1.5, y, "M" + (i + 1), null));
        [1.5, 2.5, 3.5].forEach((y, i) => k(L, 207 + i, -2.7, y, "S" + (i + 1), null, 0.7, 0.9));
        k(L, 169, -1.5, -1.45, "", null, 1.2, 1.2, "dial");
        k(L, 201, 18.5, -1.2, "⏮", null, 1, 0.8);
        k(L, 205, 19.5, -1.2, "⏯", null, 1, 0.8);
        k(L, 202, 20.5, -1.2, "⏭", null, 1, 0.8);
        k(L, 200, 21.5, -1.2, "🔇", null, 1, 0.8);
        k(L, 206, 16.25, -1.2, "Vol −", null, 1, 0.8);
        k(L, 204, 17.25, -1.2, "Vol +", null, 1, 0.8);
        return L;
    }

    function isRemapped(key, a) {
        if (!a || !key.def) {
            return false;
        }
        return a.function !== 2 || a.data[0] !== key.def.mod || a.data[1] !== key.def.hid;
    }

    Rectangle {
        // Chassis
        x: root.offX + (root.minX + 0.6) * root.u
        y: root.offY + (root.minY - 0.05) * root.u
        width: (root.spanX - 0.75) * root.u
        height: (root.spanY - 0.05) * root.u
        radius: root.u * 0.35
        color: "#0d0d0d"
        border.color: "#262626"
        visible: root.u > 0
    }

    Repeater {
        model: root.layout
        delegate: Rectangle {
            id: keyRect
            required property var modelData
            readonly property var a: root.assignments[modelData.pos]
            readonly property bool remapped: root.isRemapped(modelData, a)
            readonly property bool isSel: root.selected === modelData.pos
            readonly property string shown: remapped ? KeyNames.legend(a) : modelData.label

            x: root.offX + modelData.x * root.u + 2
            y: root.offY + modelData.y * root.u + 2
            width: modelData.w * root.u - 4
            height: modelData.h * root.u - 4
            radius: modelData.shape === "dial" ? width / 2 : 3
            color: hov.hovered ? "#262626" : Theme.keyFace
            border.width: isSel ? 2 : 1
            border.color: isSel ? Theme.accent : (hov.hovered ? Theme.border : Theme.keyEdge)
            opacity: a || modelData.pos === 59 ? 1 : 0.55

            Rectangle {
                // Inner ring for the command dial.
                visible: keyRect.modelData.shape === "dial"
                anchors.centerIn: parent
                width: parent.width * 0.55
                height: width
                radius: width / 2
                color: "transparent"
                border.color: keyRect.isSel ? Theme.accent : Theme.border
            }
            Text {
                anchors.fill: parent
                anchors.margins: 3
                text: keyRect.shown
                color: keyRect.remapped ? Theme.accent : Theme.text
                font.family: Theme.font
                font.pixelSize: Math.max(8, Math.min(root.u * 0.26, 13))
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.Wrap
                elide: Text.ElideRight
            }
            Rectangle {
                // Remap marker
                visible: keyRect.remapped
                width: 5
                height: 5
                radius: 2.5
                color: Theme.accent
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 3
            }
            HoverHandler {
                id: hov
                cursorShape: Qt.PointingHandCursor
            }
            TapHandler {
                onTapped: {
                    const n = keyRect.modelData.shape === "dial" ? "Command dial" : (keyRect.modelData.label || "Space");
                    root.picked(keyRect.modelData.pos, n);
                }
            }
        }
    }
}
