import QtQuick

// Ambient screen-area picker: a 16:9 thumbnail of the live screen (the daemon's coarse
// capture grid, refreshed while visible) with the chosen area drawn on top. Drag on the
// thumbnail to pick a new area; double-click for the whole screen.
Item {
    id: root

    property var backend: null
    property var area: [0, 0, 1, 1]          // [left, top, right, bottom], 0..1
    signal areaEdited(var area)

    property string grid: ""                 // "WxH:rrggbb…" from backend.ambient_preview()
    readonly property bool live: grid.length > 0

    implicitWidth: 320
    implicitHeight: 180

    Timer {
        interval: 500
        repeat: true
        triggeredOnStart: true
        running: root.visible && root.backend !== null
        onTriggered: {
            root.grid = root.backend.ambient_preview();
            screen.requestPaint();
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "#0b0b0b"
        border.color: Theme.borderSoft
    }

    Canvas {
        id: screen
        anchors.fill: parent
        anchors.margins: 1
        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            const m = /^(\d+)x(\d+):([0-9a-f]*)$/.exec(root.grid);
            if (!m) {
                return;
            }
            const w = +m[1], h = +m[2], hex = m[3];
            const cw = width / w, ch = height / h;
            for (let y = 0; y < h; y++) {
                for (let x = 0; x < w; x++) {
                    const i = (y * w + x) * 6;
                    ctx.fillStyle = "#" + hex.substr(i, 6);
                    ctx.fillRect(Math.floor(x * cw), Math.floor(y * ch), Math.ceil(cw), Math.ceil(ch));
                }
            }
        }
    }

    Text {
        visible: !root.live
        anchors.centerIn: parent
        width: parent.width - 24
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: "Your screen appears here while Ambient is running.\nDrag to choose the area."
        color: Theme.textMuted
        font.family: Theme.font
        font.pixelSize: 11
    }

    // Dim everything outside the chosen area.
    Item {
        anchors.fill: parent
        readonly property real l: (band.visible ? band.x : root.area[0] * width)
        readonly property real t: (band.visible ? band.y : root.area[1] * height)
        readonly property real r: (band.visible ? band.x + band.width : root.area[2] * width)
        readonly property real b: (band.visible ? band.y + band.height : root.area[3] * height)
        Rectangle { color: "#a0000000"; x: 0; y: 0; width: parent.width; height: parent.t }
        Rectangle { color: "#a0000000"; x: 0; y: parent.b; width: parent.width; height: parent.height - parent.b }
        Rectangle { color: "#a0000000"; x: 0; y: parent.t; width: parent.l; height: parent.b - parent.t }
        Rectangle { color: "#a0000000"; x: parent.r; y: parent.t; width: parent.width - parent.r; height: parent.b - parent.t }
        Rectangle {
            x: parent.l
            y: parent.t
            width: parent.r - parent.l
            height: parent.b - parent.t
            color: "transparent"
            border.color: Theme.accent
            border.width: 2
        }
    }

    Item {
        id: band
        visible: false
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.CrossCursor
        property point start
        function clampX(v) { return Math.max(0, Math.min(width, v)); }
        function clampY(v) { return Math.max(0, Math.min(height, v)); }
        onPressed: m => {
            start = Qt.point(clampX(m.x), clampY(m.y));
        }
        onPositionChanged: m => {
            const x = clampX(m.x), y = clampY(m.y);
            band.x = Math.min(start.x, x);
            band.y = Math.min(start.y, y);
            band.width = Math.abs(x - start.x);
            band.height = Math.abs(y - start.y);
            band.visible = band.width > 4 && band.height > 4;
        }
        onReleased: {
            if (band.visible) {
                const r = v => Math.round(v * 100) / 100;
                root.areaEdited([r(band.x / width), r(band.y / height), r((band.x + band.width) / width), r((band.y + band.height) / height)]);
            }
            band.visible = false;
        }
        onDoubleClicked: root.areaEdited([0, 0, 1, 1])
    }
}
