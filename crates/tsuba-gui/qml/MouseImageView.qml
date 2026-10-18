import QtQuick
import QtQuick.Shapes
import QtQuick.Effects

// Mouse render with Synapse-style callouts: labels in side columns, leader lines
// to anchor dots on the physical buttons; underside buttons in an inset.
Item {
    id: root

    property var pack: null
    property string dir: ""
    property var assignments: ({})
    property int selected: -1
    signal picked(int pos, string name)

    readonly property var topView: pack ? pack.views.top : null
    readonly property var bottomView: pack && pack.views.bottom ? pack.views.bottom : null
    readonly property real calloutW: 240
    readonly property real calloutH: 36
    readonly property real gap: 16
    // Vertical distance between callout labels.
    readonly property real rowStep: calloutH + 6

    // Render placement: centred, as tall as the area allows.
    readonly property real imgH: Math.min(height * 0.96, (width - 2 * calloutW - 70) * (topView ? topView.height / topView.width : 1))
    readonly property real imgS: topView ? imgH / topView.height : 0
    readonly property real imgX: (width - (topView ? topView.width * imgS : 0)) / 2
    readonly property real imgY: (height - imgH) / 2

    // Callouts listed top to bottom in the order the buttons sit on the mouse.
    readonly property var leftSpots: topView ? topView.hotspots.filter(h => h.side === "left").sort((a, b) => a.anchor[1] - b.anchor[1]) : []
    readonly property var rightSpots: topView ? topView.hotspots.filter(h => h.side === "right").sort((a, b) => a.anchor[1] - b.anchor[1]) : []

    // Space kept free at the bottom-left for the underside inset and its label.
    readonly property real insetReserve: bottomView ? 140 + 6 + 30 + 12 : 0
    // Height the callout columns need so no label is cut off or overlaps the inset's label.
    readonly property real neededH: Math.max(440, rightSpots.length * rowStep + 8, leftSpots.length * rowStep + insetReserve + 8)

    // Elbow columns: the leader lines bend on a vertical just outside the render.
    readonly property real elbowL: imgX - 18
    readonly property real elbowR: imgX + (topView ? topView.width * imgS : 0) + 18

    function anchorPt(h) {
        return Qt.point(imgX + h.anchor[0] * imgS, imgY + h.anchor[1] * imgS);
    }
    function cross(a, b, c) {
        return (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    }
    function segmentsCross(p1, p2, p3, p4) {
        return cross(p1, p2, p3) * cross(p1, p2, p4) < 0 && cross(p3, p4, p1) * cross(p3, p4, p2) < 0;
    }
    // Label slots for one side. Labels start in anchor-y order, each slot as close to its anchor's
    // height as the minimum spacing allows (kept inside [0, limit]); neighbouring labels whose
    // leader lines cross then trade slots. Returns [{ spot, y }] top to bottom.
    function arrange(spots, limit, ex) {
        const step = rowStep, n = spots.length, ys = [];
        for (let i = 0; i < n; i++) {
            const want = anchorPt(spots[i]).y - calloutH / 2;
            ys.push(Math.max(i > 0 ? ys[i - 1] + step : 0, want));
        }
        const maxTop = Math.max(0, limit - calloutH);
        for (let i = n - 1; i >= 0; i--) {
            ys[i] = Math.min(ys[i], i < n - 1 ? ys[i + 1] - step : maxTop);
        }
        for (let i = 0; i < n; i++) {
            ys[i] = Math.max(ys[i], i > 0 ? ys[i - 1] + step : 0);
        }
        const order = spots.slice();
        for (let pass = 0; pass < 20; pass++) {
            let swapped = false;
            for (let i = 0; i + 1 < n; i++) {
                const e1 = Qt.point(ex, ys[i] + calloutH / 2), e2 = Qt.point(ex, ys[i + 1] + calloutH / 2);
                if (segmentsCross(e1, anchorPt(order[i]), e2, anchorPt(order[i + 1]))) {
                    [order[i], order[i + 1]] = [order[i + 1], order[i]];
                    swapped = true;
                }
            }
            if (!swapped) {
                break;
            }
        }
        return order.map((h, i) => ({ spot: h, y: ys[i] }));
    }
    readonly property var leftSlots: arrange(leftSpots, height - insetReserve, elbowL)
    readonly property var rightSlots: arrange(rightSpots, height, elbowR)


    // The picture sits at `imageRect` inside the drawing's frame (all of it unless the user placed their own).
    readonly property var topRect: Theme.imageRect(topView)
    readonly property var botRect: Theme.imageRect(bottomView)
    // Button or label under the pointer (-1 = none): its whole leader line lights up.
    property int hoveredPos: -1

    Image {
        x: root.imgX + root.topRect[0] * root.imgS
        y: root.imgY + root.topRect[1] * root.imgS
        width: root.topRect[2] * root.imgS
        height: root.topRect[3] * root.imgS
        source: root.topView ? root.dir + root.topView.image : ""
        fillMode: Image.PreserveAspectFit
        smooth: true
        mipmap: true
    }

    component Callout: Item {
        id: c
        required property var spot
        required property bool leftSide
        required property real slotY
        readonly property var a: root.assignments[spot.pos]
        readonly property bool isSel: root.selected === spot.pos
        readonly property real ax: root.imgX + spot.anchor[0] * root.imgS
        readonly property real ay: root.imgY + spot.anchor[1] * root.imgS
        readonly property bool remapped: !!a && [2, 3, 4, 5, 15].indexOf(a.function) >= 0
        // Hovering the label or the button's hotspot lights the whole leader line in the accent.
        readonly property bool hot: hov.hovered || dotHov.hovered
        readonly property bool lit: isSel || hot
        onHotChanged: {
            if (hot) {
                root.hoveredPos = spot.pos;
            } else if (root.hoveredPos === spot.pos) {
                root.hoveredPos = -1;
            }
        }

        x: leftSide ? 0 : root.width - root.calloutW
        y: slotY
        width: root.calloutW
        height: root.calloutH
        visible: a !== undefined

        // Leader line from the callout edge to the button: thin grey, green when selected.
        Shape {
            x: -c.x
            y: -c.y
            width: root.width
            height: root.height
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: c.lit ? Theme.accent : Theme.textMuted
                strokeWidth: c.isSel ? 2 : 1
                fillColor: "transparent"
                startX: c.leftSide ? c.x + c.width + 4 : c.x - 4
                startY: c.y + c.height / 2
                PathLine {
                    x: c.leftSide ? root.elbowL : root.elbowR
                    y: c.y + c.height / 2
                }
                PathLine {
                    x: c.ax
                    y: c.ay
                }
            }
        }
        Rectangle {
            // Dot where the line meets the physical button.
            x: c.ax - c.x - width / 2
            y: c.ay - c.y - height / 2
            width: 5
            height: 5
            radius: 2.5
            color: c.lit ? Theme.accent : Theme.textDim
        }
        Rectangle {
            // Dot at the label end of the line.
            x: (c.leftSide ? c.width + 4 : -4) - width / 2
            y: c.height / 2 - height / 2
            width: 5
            height: 5
            radius: 2.5
            color: c.lit ? Theme.accent : Theme.textDim
        }
        // Plain text label (function name); selected = text on a raised chip.
        Rectangle {
            visible: c.isSel
            x: c.leftSide ? c.width - width - 8 : 8
            anchors.verticalCenter: parent.verticalCenter
            width: label.implicitWidth + 20
            height: 28
            radius: 3
            color: Theme.panelRaised
        }
        Text {
            id: label
            x: c.leftSide ? c.width - width - 18 : 18
            width: Math.min(implicitWidth, c.width - 26)
            anchors.verticalCenter: parent.verticalCenter
            text: KeyNames.describe(c.a)
            color: c.remapped || c.lit ? Theme.accent : Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            elide: Text.ElideRight
        }
        HoverHandler {
            id: hov
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            onTapped: {
                root.picked(c.spot.pos, c.spot.name);
            }
        }
        // Hotspot on the physical button (the anchor dot): hover lights the line, click selects.
        Item {
            x: c.ax - c.x - 11
            y: c.ay - c.y - 11
            width: 22
            height: 22
            HoverHandler {
                id: dotHov
                cursorShape: Qt.PointingHandCursor
            }
            TapHandler {
                onTapped: {
                    root.picked(c.spot.pos, c.spot.name);
                }
            }
        }
    }

    Repeater {
        model: root.leftSlots
        delegate: Callout {
            required property var modelData
            spot: modelData.spot
            slotY: modelData.y
            leftSide: true
        }
    }
    Repeater {
        model: root.rightSlots
        delegate: Callout {
            required property var modelData
            spot: modelData.spot
            slotY: modelData.y
            leftSide: false
        }
    }

    // Underside inset (profile button), bottom-left like Synapse's inset views.
    Item {
        id: inset
        visible: root.bottomView !== null
        x: (root.calloutW - width) / 2
        y: root.height - height - 6
        width: 140
        height: 140
        // Centre the pack's round inset (view.inset {cx, cy, r}) in the frame and fill it.
        readonly property var ins: root.bottomView && root.bottomView.inset ? root.bottomView.inset : (root.bottomView ? { cx: root.bottomView.width / 2, cy: root.bottomView.height / 2, r: Math.min(root.bottomView.width, root.bottomView.height) / 2 } : null)
        readonly property real sc: ins ? width / (2 * ins.r) : 1
        // Personal photo packs are darkened to suit the dark UI; Tsuba's own art is left alone.
        readonly property bool photo: !!root.pack && root.pack.art !== "tsuba"
        // Shader effects (round mask, darkening) need a GPU scene graph; the software
        // renderer gets a plain overlay and a background-coloured ring over the corners instead.
        readonly property bool soft: GraphicsInfo.api === GraphicsInfo.Software
        Item {
            id: insetContent
            anchors.fill: parent
            visible: inset.soft
            clip: true
            layer.enabled: !inset.soft
            Rectangle {
                anchors.fill: parent
                color: "#0d0d0d"   // device drawing: intentional
            }
            Image {
                x: parent.width / 2 + ((root.botRect[0] - (inset.ins ? inset.ins.cx : 0)) * inset.sc)
                y: parent.height / 2 + ((root.botRect[1] - (inset.ins ? inset.ins.cy : 0)) * inset.sc)
                width: root.botRect[2] * inset.sc
                height: root.botRect[3] * inset.sc
                sourceSize.width: width * 2
                sourceSize.height: height * 2
                source: root.bottomView ? root.dir + root.bottomView.image : ""
                fillMode: Image.Stretch
                smooth: true
                mipmap: true
            }
            Rectangle {
                visible: inset.soft && inset.photo
                anchors.fill: parent
                color: "#8c000000"   // device drawing: intentional
            }
            Rectangle {
                visible: inset.soft
                anchors.centerIn: parent
                width: parent.width + 120
                height: width
                radius: width / 2
                color: "transparent"
                border.width: 60
                border.color: Theme.bg
            }
        }
        Item {
            id: insetMask
            anchors.fill: parent
            visible: false
            layer.enabled: true
            Rectangle {
                anchors.fill: parent
                radius: width / 2
            }
        }
        MultiEffect {
            visible: !inset.soft
            anchors.fill: parent
            source: insetContent
            maskEnabled: true
            maskSource: insetMask
            brightness: inset.photo ? -0.55 : 0
            saturation: inset.photo ? -0.3 : 0
        }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "transparent"
            border.color: root.selected === 14 ? Theme.accent : Theme.borderSoft
            border.width: root.selected === 14 ? 2 : 1
        }
        Rectangle {
            visible: root.bottomView && root.assignments[14] !== undefined
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.top
            anchors.bottomMargin: 6
            width: insetLabel.implicitWidth + 16
            height: 24
            radius: 3
            color: root.selected === 14 ? Theme.panelRaised : "transparent"
            Text {
                id: insetLabel
                anchors.centerIn: parent
                text: KeyNames.describe(root.assignments[14])
                color: root.selected === 14 ? Theme.textBright : Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }
        }
        TapHandler {
            onTapped: {
                root.picked(14, i18n("Profile Button"));
            }
        }
    }
}
