import QtQuick

// Per-LED view on the device render: each lit key shows its colour; click to toggle,
// drag a box to select, Shift/Ctrl to add to the selection.
Item {
    id: root

    property var view: null          // pack.views.top
    property string dir: ""
    property var colours: ({})       // "r,c" -> "rrggbb"
    property var selection: ({})     // "r,c" -> true
    // false when a ZoomCanvas around this item handles the pointer (zoom/pan/box select anywhere).
    property bool interactive: true
    signal selectionEdited(var selection)

    readonly property var leds: view ? view.hotspots.filter(h => h.led !== null) : []
    // Show only the keyboard body (no side-button inset or its leader line); the underglow
    // strips sit just outside it, so the view spans the body plus every LED box.
    readonly property var pic: Theme.imageRect(view)
    readonly property var body: view ? (view.body || [0, 0, view.width, view.height]) : [0, 0, 1, 1]
    // Synapse shows the wrist rest attached under the body; the underglow ring goes round both.
    readonly property var wrist: view && view.wrist ? view.wrist.rect : null
    readonly property real bx0: Math.min(body[0], ...leds.map(h => h.rect[0])) - 4
    readonly property real by0: Math.min(body[1], ...leds.map(h => h.rect[1])) - 4
    readonly property real bx1: Math.max(body[2], ...leds.map(h => h.rect[0] + h.rect[2])) + 4
    readonly property real by1: Math.max(body[3], wrist ? wrist[1] + wrist[3] : 0, ...leds.map(h => h.rect[1] + h.rect[3])) + 4
    readonly property real s: view ? Math.min(width / (bx1 - bx0), height / (by1 - by0)) : 0
    readonly property real ox: (width - (bx1 - bx0) * s) / 2 - bx0 * s
    readonly property real oy: (height - (by1 - by0) * s) / 2 - by0 * s

    // Studio canvas: the drawing's extent and the box round the LED centres, in picture units.
    readonly property rect contentPic: Qt.rect(bx0, by0, bx1 - bx0, by1 - by0)
    readonly property rect ledsPic: {
        if (!leds.length) {
            return Qt.rect(0, 0, 1, 1);
        }
        const xs = leds.map(h => h.rect[0] + h.rect[2] / 2), ys = leds.map(h => h.rect[1] + h.rect[3] / 2);
        const x0 = Math.min(...xs), y0 = Math.min(...ys);
        return Qt.rect(x0, y0, Math.max(1, Math.max(...xs) - x0), Math.max(1, Math.max(...ys) - y0));
    }

    function keyOf(h) {
        return h.led[0] + "," + h.led[1];
    }
    function rectOf(h) {
        return Qt.rect(ox + h.rect[0] * s, oy + h.rect[1] * s, h.rect[2] * s, h.rect[3] * s);
    }
    function hitsIn(r) {
        return leds.filter(h => {
            const k = rectOf(h);
            return k.x < r.x + r.width && k.x + k.width > r.x && k.y < r.y + r.height && k.y + k.height > r.y;
        });
    }

    // Selection from a box / click given in this item's own coordinates (used by ZoomCanvas).
    function selectBox(r, additive) {
        const sel = additive ? Object.assign({}, selection) : {};
        hitsIn(r).forEach(h => sel[keyOf(h)] = true);
        selectionEdited(sel);
    }
    function selectAt(p, additive) {
        const hit = hitsIn(Qt.rect(p.x, p.y, 1, 1))[0];
        if (hit) {
            const k = keyOf(hit);
            const base = Object.assign({}, selection);
            if (base[k]) {
                delete base[k];
            } else {
                base[k] = true;
            }
            selectionEdited(base);
        } else {
            selectionEdited(additive ? Object.assign({}, selection) : {});
        }
    }

    Item {
        x: root.ox + root.body[0] * root.s
        y: root.oy
        width: (root.body[2] - root.body[0]) * root.s
        height: root.view ? root.view.height * root.s : 0
        clip: true
        Image {
            id: img
            // The picture sits at `imageRect` in the drawing's space (all of it unless the user placed their own).
            x: (root.pic[0] - root.body[0]) * root.s
            y: root.pic[1] * root.s
            width: root.pic[2] * root.s
            height: root.pic[3] * root.s
            source: root.view && root.dir !== "" ? root.dir + root.view.image : ""
            fillMode: Image.Stretch
            smooth: true
            mipmap: true
        }
    }

    Image {
        visible: root.wrist !== null
        x: root.wrist ? root.ox + root.wrist[0] * root.s : 0
        y: root.wrist ? root.oy + root.wrist[1] * root.s : 0
        width: root.wrist ? root.wrist[2] * root.s : 0
        height: root.wrist ? root.wrist[3] * root.s : 0
        source: root.wrist ? root.dir + root.view.wrist.image : ""
        fillMode: Image.Stretch
        smooth: true
        mipmap: true
    }

    Repeater {
        model: root.s > 0 ? root.leds : []
        delegate: Rectangle {
            required property var modelData
            readonly property string key: root.keyOf(modelData)
            readonly property string hex: root.colours[key] || "000000"
            readonly property bool sel: root.selection[key] === true
            readonly property rect r: root.rectOf(modelData)
            readonly property bool round: modelData.shape === "round" || modelData.name === "Command dial"
            x: r.x
            y: r.y
            width: r.width
            height: r.height
            readonly property bool strip: modelData.group === "underglow"
            radius: round ? width / 2 : (strip ? 2 : 4)
            color: hex === "000000" ? "transparent" : "#" + hex
            opacity: hex === "000000" ? 1 : 0.72
            border.width: sel ? 2 : 1
            border.color: sel ? "#ffffff" : (hex === "000000" ? "#55cccccc" : "#" + hex)
            Rectangle {
                visible: parent.sel
                anchors.fill: parent
                anchors.margins: -4
                radius: parent.radius + 3
                color: "transparent"
                border.color: Theme.accent
                border.width: 2
            }
        }
    }

    // Rubber-band selection.
    Rectangle {
        id: band
        visible: false
        color: Theme.accentTintMid
        border.color: Theme.accent
    }

    MouseArea {
        anchors.fill: parent
        enabled: root.interactive
        property point start
        cursorShape: Qt.CrossCursor
        onPressed: mouse => {
            start = Qt.point(mouse.x, mouse.y);
            band.x = mouse.x;
            band.y = mouse.y;
            band.width = 0;
            band.height = 0;
        }
        onPositionChanged: mouse => {
            band.visible = true;
            band.x = Math.min(start.x, mouse.x);
            band.y = Math.min(start.y, mouse.y);
            band.width = Math.abs(mouse.x - start.x);
            band.height = Math.abs(mouse.y - start.y);
        }
        onReleased: mouse => {
            const additive = (mouse.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) !== 0;
            const sel = additive ? Object.assign({}, root.selection) : {};
            if (band.visible && band.width > 4 && band.height > 4) {
                root.hitsIn(Qt.rect(band.x, band.y, band.width, band.height)).forEach(h => sel[root.keyOf(h)] = true);
            } else {
                const hit = root.hitsIn(Qt.rect(mouse.x, mouse.y, 1, 1))[0];
                if (hit) {
                    const k = root.keyOf(hit);
                    const base = Object.assign({}, root.selection);
                    if (base[k]) {
                        delete base[k];
                    } else {
                        base[k] = true;
                    }
                    root.selectionEdited(base);
                    band.visible = false;
                    return;
                }
            }
            band.visible = false;
            root.selectionEdited(sel);
        }
    }
}
