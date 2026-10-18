import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Alignment step for a custom device picture: the user's picture is drawn at half opacity over
// Tsuba's own line-art drawing, so it can be dragged (move), wheel-scrolled or slid (scale) until
// the buttons / keys line up. The result is the picture's rectangle [x, y, w, h] in the drawing's
// coordinate space, which is what the pack's `imageRect` stores.
Rectangle {
    id: root

    property string drawing: ""      // URL of Tsuba's drawing for this view
    property string source: ""       // URL of the user's picture
    property real frameW: 1          // the drawing's size (the pack view's width x height)
    property real frameH: 1
    property real imgW: 0            // the picture's own size (filled in once it has loaded)
    property real imgH: 0
    property var rect: [0, 0, 1, 1]  // x, y, w, h in drawing space
    property bool placed: false
    property string title: i18n("Line up your picture")
    signal accepted(real x, real y, real w, real h)
    signal cancelled()

    readonly property real minScale: 0.15      // of the "fit" width
    readonly property real maxScale: 4

    // ---- the maths (pure functions, covered by tst_picture_align.qml)
    // Largest rectangle of the picture's aspect that fits the drawing's frame, centred.
    function fitRect(iw, ih, fw, fh) {
        if (iw <= 0 || ih <= 0) {
            return [0, 0, fw, fh];
        }
        const k = Math.min(fw / iw, fh / ih);
        return [(fw - iw * k) / 2, (fh - ih * k) / 2, iw * k, ih * k];
    }
    // Width of the fitted picture; slider / wheel scales are relative to it.
    function fitWidth() {
        return fitRect(imgW, imgH, frameW, frameH)[2];
    }
    function scaleOf(r) {
        const fw = fitWidth();
        return fw > 0 ? r[2] / fw : 1;
    }
    // Keep the picture's centre inside the frame so it can't be lost off-stage.
    function clampCentre(r) {
        const cx = Math.max(0, Math.min(frameW, r[0] + r[2] / 2));
        const cy = Math.max(0, Math.min(frameH, r[1] + r[3] / 2));
        return [cx - r[2] / 2, cy - r[3] / 2, r[2], r[3]];
    }
    function moveRect(r, dx, dy) {
        return clampCentre([r[0] + dx, r[1] + dy, r[2], r[3]]);
    }
    // Resize to `newW` (aspect kept) while the drawing-space point (px, py) stays under the cursor.
    function scaleRect(r, newW, px, py) {
        const lo = fitWidth() * minScale, hi = fitWidth() * maxScale;
        const w = Math.max(lo, Math.min(hi, newW));
        const f = w / r[2];
        return clampCentre([px - (px - r[0]) * f, py - (py - r[1]) * f, w, r[3] * f]);
    }
    function scaleCentred(r, newW) {
        return scaleRect(r, newW, r[0] + r[2] / 2, r[1] + r[3] / 2);
    }
    // Stage -> drawing coordinates for a stage point.
    function toDrawing(sx, sy) {
        return Qt.point((sx - stage.fx) / stage.k, (sy - stage.fy) / stage.k);
    }
    function fit() {
        rect = fitRect(imgW, imgH, frameW, frameH);
    }
    function reset() {
        placed = false;
        imgW = 0;
        imgH = 0;
        measure.source = "";
        measure.source = source;
    }

    color: Theme.card
    border.color: Theme.borderSoft
    radius: 5
    implicitWidth: 900
    implicitHeight: 640

    // Measures the picture; the first time it is known, it is fitted into the frame.
    Image {
        id: measure
        visible: false
        source: root.source
        asynchronous: true
        onStatusChanged: {
            if (status === Image.Ready && !root.placed) {
                root.imgW = implicitWidth;
                root.imgH = implicitHeight;
                root.placed = true;
                root.fit();
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            Text {
                text: root.title.toUpperCase()
                color: Theme.accent
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }
            Item {
                Layout.fillWidth: true
            }
            Text {
                text: i18n("Drag to move · mouse wheel to resize")
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }

        // Stage: Tsuba's drawing, the picture over it at 50 % opacity.
        Rectangle {
            id: stage
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.bg
            clip: true
            readonly property real k: Math.min(width / (root.frameW * 1.3), height / (root.frameH * 1.3))
            readonly property real fx: (width - root.frameW * k) / 2
            readonly property real fy: (height - root.frameH * k) / 2

            Image {
                x: stage.fx
                y: stage.fy
                width: root.frameW * stage.k
                height: root.frameH * stage.k
                source: root.drawing
                fillMode: Image.PreserveAspectFit
                smooth: true
            }
            Rectangle {
                x: stage.fx
                y: stage.fy
                width: root.frameW * stage.k
                height: root.frameH * stage.k
                color: "transparent"
                border.color: Theme.borderSoft
            }
            Image {
                id: pic
                objectName: "pic"
                visible: root.placed
                x: stage.fx + root.rect[0] * stage.k
                y: stage.fy + root.rect[1] * stage.k
                width: root.rect[2] * stage.k
                height: root.rect[3] * stage.k
                source: root.source
                fillMode: Image.Stretch
                smooth: true
                mipmap: true
                opacity: 0.5
            }
            Rectangle {
                visible: root.placed
                x: pic.x
                y: pic.y
                width: pic.width
                height: pic.height
                color: "transparent"
                border.color: Theme.accent
                border.width: 1
            }

            MouseArea {
                id: drag
                objectName: "drag"
                anchors.fill: parent
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                property point last
                onPressed: m => last = Qt.point(m.x, m.y)
                onPositionChanged: m => {
                    if (pressed) {
                        root.rect = root.moveRect(root.rect, (m.x - last.x) / stage.k, (m.y - last.y) / stage.k);
                        last = Qt.point(m.x, m.y);
                    }
                }
                onWheel: w => {
                    const p = root.toDrawing(w.x, w.y);
                    root.rect = root.scaleRect(root.rect, root.rect[2] * Math.pow(1.0015, w.angleDelta.y), p.x, p.y);
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Text {
                text: i18nc("picture size slider", "Size")
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            SynSlider {
                id: sizeSlider
                objectName: "sizeSlider"
                Layout.preferredWidth: 260
                from: root.minScale
                to: root.maxScale
                value: root.scaleOf(root.rect)
                onMoved: root.rect = root.scaleCentred(root.rect, value * root.fitWidth())
            }
            Text {
                text: Math.round(root.scaleOf(root.rect) * 100) + "%"
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            Item {
                Layout.fillWidth: true
            }
            SynButton {
                objectName: "fitButton"
                text: i18nc("button: fit picture", "Fit")
                onClicked: root.fit()
            }
            SynButton {
                objectName: "cancelButton"
                text: i18nc("@action:button", "Cancel")
                onClicked: root.cancelled()
            }
            SynButton {
                objectName: "saveButton"
                text: i18nc("@action:button", "Save")
                primary: true
                enabled: root.placed
                onClicked: root.accepted(root.rect[0], root.rect[1], root.rect[2], root.rect[3])
            }
        }
    }
}
