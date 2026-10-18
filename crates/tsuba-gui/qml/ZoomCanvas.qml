import QtQuick

// Pannable, zoomable canvas for the LED views. The wheel zooms around the cursor
// (50-300%), a middle or right drag pans, and a left drag anywhere on the canvas (margins
// included) draws a rubber band reported in the content's own coordinates, so selection works
// at any zoom. Children go into the content layer, which starts out exactly as large as the canvas.
Item {
    id: zc

    property real zoom: 1.0
    property real panX: 0
    property real panY: 0
    readonly property real minZoom: 0.5
    readonly property real maxZoom: 3.0
    property bool showBar: true
    property bool selecting: true
    default property alias content: layer.data
    readonly property alias contentLayer: layer

    signal boxSelected(rect r, bool additive)   // content coordinates
    signal tapped(point p, bool additive)

    clip: true

    function clampPan() {
        // Keep at least a sliver of the content in view.
        const w = width * zoom, h = height * zoom, keep = 80;
        panX = Math.max(keep - w, Math.min(width - keep, panX));
        panY = Math.max(keep - h, Math.min(height - keep, panY));
    }
    // Zoom to z keeping the viewport point (vx, vy) fixed.
    function zoomAt(z, vx, vy) {
        const nz = Math.max(minZoom, Math.min(maxZoom, Math.round(z * 100) / 100));
        const cx = (vx - panX) / zoom, cy = (vy - panY) / zoom;
        zoom = nz;
        panX = vx - cx * nz;
        panY = vy - cy * nz;
        clampPan();
    }
    function zoomStep(d) {
        zoomAt(zoom + d, width / 2, height / 2);
    }
    function fit() {
        zoom = 1;
        panX = 0;
        panY = 0;
    }

    Item {
        id: layer
        width: zc.width
        height: zc.height
        x: zc.panX
        y: zc.panY
        scale: zc.zoom
        transformOrigin: Item.TopLeft
    }

    Rectangle {
        id: band
        visible: false
        color: Theme.accentTintMid
        border.color: Theme.accent
        z: 5
    }

    MouseArea {
        id: area
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        property bool panning: false
        property point start
        property point panStart
        cursorShape: panning ? Qt.ClosedHandCursor : (zc.selecting ? Qt.CrossCursor : Qt.ArrowCursor)
        onWheel: w => zc.zoomAt(zc.zoom * Math.pow(1.0015, w.angleDelta.y), w.x, w.y)
        onPressed: m => {
            start = Qt.point(m.x, m.y);
            panning = m.button !== Qt.LeftButton;
            panStart = Qt.point(zc.panX, zc.panY);
            band.visible = false;
        }
        onPositionChanged: m => {
            if (panning) {
                zc.panX = panStart.x + m.x - start.x;
                zc.panY = panStart.y + m.y - start.y;
                zc.clampPan();
            } else if (zc.selecting && Math.abs(m.x - start.x) + Math.abs(m.y - start.y) > 6) {
                band.visible = true;
                band.x = Math.min(start.x, m.x);
                band.y = Math.min(start.y, m.y);
                band.width = Math.abs(m.x - start.x);
                band.height = Math.abs(m.y - start.y);
            }
        }
        onReleased: m => {
            if (panning) {
                panning = false;
                return;
            }
            if (!zc.selecting) {
                return;
            }
            const add = (m.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) !== 0;
            if (band.visible) {
                const a = layer.mapFromItem(zc, band.x, band.y);
                const b = layer.mapFromItem(zc, band.x + band.width, band.y + band.height);
                band.visible = false;
                zc.boxSelected(Qt.rect(a.x, a.y, b.x - a.x, b.y - a.y), add);
            } else {
                zc.tapped(layer.mapFromItem(zc, m.x, m.y), add);
            }
        }
    }

    // Zoom bar: - / % (click = fit) / + / Fit
    Rectangle {
        visible: zc.showBar
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 6
        width: zrow.implicitWidth + 12
        height: 28
        radius: 3
        color: Theme.overlayPanel
        border.color: Theme.borderSoft
        z: 6
        Row {
            id: zrow
            anchors.centerIn: parent
            spacing: 4
            component BarBtn: Rectangle {
                id: bb
                property string caption: ""
                property string tip: ""
                signal clicked()
                width: Math.max(24, bt.implicitWidth + 14)
                height: 24
                radius: 3
                color: bh.hovered ? Theme.hoverFill : "transparent"
                Text {
                    id: bt
                    anchors.centerIn: parent
                    text: bb.caption
                    color: bh.hovered ? Theme.accent : Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                HoverHandler {
                    id: bh
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: bb.clicked()
                }
            }
            BarBtn {
                objectName: "zoomOut"
                caption: "−"
                tip: i18n("Zoom out")
                onClicked: zc.zoomStep(-0.1)
            }
            Text {
                objectName: "zoomLabel"
                width: 40
                height: 24
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                text: Math.round(zc.zoom * 100) + "%"
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            BarBtn {
                objectName: "zoomIn"
                caption: "+"
                tip: i18n("Zoom in")
                onClicked: zc.zoomStep(0.1)
            }
            BarBtn {
                objectName: "zoomFit"
                caption: i18nc("zoom bar button: fit the whole picture in view", "Fit")
                tip: i18n("Reset zoom and position")
                onClicked: zc.fit()
            }
        }
    }
}
