import QtQuick

// Keyboard on the device render from the local image pack: every key is a hotspot
// (rects from the pack, in render pixels). Remapped keys show their new legend.
Item {
    id: root

    property var view: null          // pack.views.top
    property string dir: ""
    property var assignments: ({})   // pos -> assignment
    property int selected: -1
    signal picked(int pos, string name)

    // The drawing's frame (pack width x height) is fitted in this item; the picture sits at
    // `imageRect` inside it (the whole frame unless the user placed their own picture).
    readonly property real fit: view ? Math.min(width / view.width, height / view.height) : 0
    readonly property real s: img.status === Image.Ready ? fit : 0
    readonly property real ox: view ? (width - view.width * fit) / 2 : 0
    readonly property real oy: view ? (height - view.height * fit) / 2 : 0
    readonly property var rect: Theme.imageRect(view)

    // Default legend per AT-101 position for remap detection (keyboard-page usage).
    function isRemapped(pos, a) {
        if (!a || a.function === 17 || a.function === 10 || a.function === 18) {
            return false;
        }
        const def = defaults[pos];
        if (def === undefined) {
            return false;
        }
        return a.function !== 2 || a.data[0] !== def[0] || a.data[1] !== def[1];
    }
    readonly property var defaults: ({
        110: [0, 0x29], 124: [0, 0x46], 125: [0, 0x47], 126: [0, 0x48], 1: [0, 0x35], 12: [0, 0x2d], 13: [0, 0x2e],
        15: [0, 0x2a], 75: [0, 0x49], 80: [0, 0x4a], 85: [0, 0x4b], 90: [0, 0x53], 95: [0, 0x54], 100: [0, 0x55],
        105: [0, 0x56], 16: [0, 0x2b], 27: [0, 0x2f], 28: [0, 0x30], 29: [0, 0x31], 76: [0, 0x4c], 81: [0, 0x4d],
        86: [0, 0x4e], 91: [0, 0x5f], 96: [0, 0x60], 101: [0, 0x61], 106: [0, 0x57], 30: [0, 0x39], 40: [0, 0x33],
        41: [0, 0x34], 43: [0, 0x28], 92: [0, 0x5c], 97: [0, 0x5d], 102: [0, 0x5e], 44: [0x02, 0], 53: [0, 0x36],
        54: [0, 0x37], 55: [0, 0x38], 57: [0x20, 0], 83: [0, 0x52], 93: [0, 0x59], 98: [0, 0x5a], 103: [0, 0x5b],
        58: [0x01, 0], 127: [0x08, 0], 60: [0x04, 0], 61: [0, 0x2c], 62: [0x40, 0], 129: [0, 0x65], 64: [0x10, 0],
        79: [0, 0x50], 84: [0, 0x51], 89: [0, 0x4f], 99: [0, 0x62], 104: [0, 0x63], 108: [0, 0x58],
        112: [0, 0x3a], 113: [0, 0x3b], 114: [0, 0x3c], 115: [0, 0x3d], 116: [0, 0x3e], 117: [0, 0x3f],
        118: [0, 0x40], 119: [0, 0x41], 120: [0, 0x42], 121: [0, 0x43], 122: [0, 0x44], 123: [0, 0x45],
        2: [0, 0x1e], 3: [0, 0x1f], 4: [0, 0x20], 5: [0, 0x21], 6: [0, 0x22], 7: [0, 0x23], 8: [0, 0x24],
        9: [0, 0x25], 10: [0, 0x26], 11: [0, 0x27],
        17: [0, 0x14], 18: [0, 0x1a], 19: [0, 0x08], 20: [0, 0x15], 21: [0, 0x17], 22: [0, 0x1c], 23: [0, 0x18],
        24: [0, 0x0c], 25: [0, 0x12], 26: [0, 0x13],
        31: [0, 0x04], 32: [0, 0x16], 33: [0, 0x07], 34: [0, 0x09], 35: [0, 0x0a], 36: [0, 0x0b], 37: [0, 0x0d],
        38: [0, 0x0e], 39: [0, 0x0f],
        46: [0, 0x1d], 47: [0, 0x1b], 48: [0, 0x06], 49: [0, 0x19], 50: [0, 0x05], 51: [0, 0x11], 52: [0, 0x10]
    })

    Image {
        id: img
        x: root.ox + root.rect[0] * root.fit
        y: root.oy + root.rect[1] * root.fit
        width: root.rect[2] * root.fit
        height: root.rect[3] * root.fit
        source: root.view ? root.dir + root.view.image : ""
        fillMode: Image.PreserveAspectFit
        smooth: true
        mipmap: true
        asynchronous: true
    }

    Repeater {
        model: root.view && root.s > 0 ? root.view.hotspots.filter(h => h.pos !== null) : []
        delegate: Item {
            id: spot
            required property var modelData
            readonly property var a: root.assignments[modelData.pos]
            readonly property bool isSel: root.selected === modelData.pos
            readonly property bool remapped: root.isRemapped(modelData.pos, a)
            readonly property bool round: modelData.shape === "round" || modelData.name === "Command dial" || ["Previous", "Play / Pause", "Next", "Mute"].indexOf(modelData.name) >= 0   // i18n-ok: device data names

            x: root.ox + modelData.rect[0] * root.s
            y: root.oy + modelData.rect[1] * root.s
            width: modelData.rect[2] * root.s
            height: modelData.rect[3] * root.s

            Rectangle {
                anchors.fill: parent
                anchors.margins: -1
                radius: spot.round ? width / 2 : 4
                color: spot.isSel ? Theme.accentTintStrong : (hov.hovered ? Theme.accentTint : "transparent")
                border.width: spot.isSel ? 2 : (hov.hovered || spot.remapped ? 1 : 0)
                border.color: spot.isSel || spot.remapped || hov.hovered ? Theme.accent : "#99cccccc"   // device drawing: intentional
            }
            Rectangle {
                // New legend for a remapped key, Synapse-style tag on the keycap.
                visible: spot.remapped
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 3
                width: Math.min(parent.width - 4, tag.implicitWidth + 8)
                height: tag.implicitHeight + 2
                radius: 2
                color: Theme.accent
                Text {
                    id: tag
                    anchors.centerIn: parent
                    text: KeyNames.legend(spot.a)
                    color: Theme.accentText
                    font.family: Theme.font
                    font.pixelSize: Math.max(8, Math.min(11, spot.height * 0.24))
                    font.bold: true
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, spot.width - 8)
                }
            }
            HoverHandler {
                id: hov
                cursorShape: Qt.PointingHandCursor
            }
            TapHandler {
                onTapped: {
                    root.picked(spot.modelData.pos, spot.modelData.name);
                }
            }
        }
    }

}
