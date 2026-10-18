import QtQuick

// Basilisk V3 Pro LEDs for Chroma Studio: the device's custom-frame row (0 logo, 1 scroll
// wheel, 2–12 underglow from front-left round the back to front-right) drawn over the
// render. Same interface as LedKeyboard: click toggles, Shift/Ctrl adds.
Item {
    id: root

    property var view: null          // pack.views.top (mouse)
    property string dir: ""
    property var colours: ({})       // "0,c" -> "rrggbb"
    property var selection: ({})     // "0,c" -> true
    property bool interactive: true   // false when a ZoomCanvas around this item handles the pointer
    signal selectionEdited(var selection)

    // Box / click selection in this item's coordinates (used by ZoomCanvas).
    function selectBox(r, additive) {
        const sel = additive ? Object.assign({}, selection) : {};
        for (const it of leds.children) {
            if (it.key === undefined) {
                continue;
            }
            const cx = it.x + it.width / 2, cy = it.y + it.height / 2;
            if (cx >= r.x && cx <= r.x + r.width && cy >= r.y && cy <= r.y + r.height) {
                sel[it.key] = true;
            }
        }
        selectionEdited(sel);
    }
    function selectAt(p, additive) {
        let best = null, bd = 1e9;
        for (const it of leds.children) {
            if (it.key === undefined) {
                continue;
            }
            const d = Math.hypot(it.x + it.width / 2 - p.x, it.y + it.height / 2 - p.y);
            if (d <= it.width / 2 + 2 && d < bd) {
                best = it;
                bd = d;
            }
        }
        const s = additive ? Object.assign({}, selection) : {};
        if (best) {
            if (additive && s[best.key]) {
                delete s[best.key];
            } else {
                s[best.key] = true;
            }
        }
        selectionEdited(s);
    }

    // Same layout as tsuba-core Geometry::basilisk_v3_pro (normalised over the render).
    readonly property var points: {
        // Logo and wheel sit where the pack's LED anchors put them on the render.
        const w = view ? view.width : 620, h = view ? view.height : 869;
        const at = n => {
            const l = view && view.leds ? view.leds.find(e => e.name === n) : null;
            return l ? l.anchor : (n === "logo" ? [328, 670] : [339, 216]);
        };
        const logo = at("logo"), wheel = at("scroll wheel");
        const p = [{ c: 0, x: logo[0] / w, y: logo[1] / h, d: 64, name: "Logo" }, { c: 1, x: wheel[0] / w, y: wheel[1] / h, d: 44, name: "Scroll wheel" }];
        for (let i = 0; i < 11; i++) {
            // Tsuba's line-art pack names each underglow LED; otherwise spread them round the render.
            const u = at("underglow " + (i + 1));
            if (view && view.leds && view.leds.some(e => e.name === "underglow " + (i + 1))) {
                p.push({ c: 2 + i, x: u[0] / w, y: u[1] / h, name: "Underglow " + (i + 1) });
                continue;
            }
            const a = Math.PI - 0.5 + (i / 10) * (Math.PI + 1.0);
            p.push({ c: 2 + i, x: 0.5 + 0.46 * Math.cos(a), y: 0.55 - 0.42 * Math.sin(a), name: "Underglow " + (i + 1) });
        }
        return p;
    }

    // Studio canvas: the box round the LED centres, in picture units (the frame is `view.width` × `view.height`).
    readonly property rect ledsPic: {
        const w = view ? view.width : 620, h = view ? view.height : 869;
        const xs = points.map(p => p.x * w), ys = points.map(p => p.y * h);
        const x0 = Math.min(...xs), y0 = Math.min(...ys);
        return Qt.rect(x0, y0, Math.max(1, Math.max(...xs) - x0), Math.max(1, Math.max(...ys) - y0));
    }
    readonly property real framePad: 30

    // The drawing's frame fitted in the item (30 px margin); the picture sits at `imageRect` in it.
    readonly property real fs: view ? Math.min((width - 60) / view.width, (height - 60) / view.height) : 0
    readonly property real fw: view ? view.width * fs : 0
    readonly property real fh: view ? view.height * fs : 0
    readonly property real fx: (width - fw) / 2
    readonly property real fy: (height - fh) / 2
    readonly property var pic: Theme.imageRect(view)

    Image {
        id: img
        x: root.fx + root.pic[0] * root.fs
        y: root.fy + root.pic[1] * root.fs
        width: root.pic[2] * root.fs
        height: root.pic[3] * root.fs
        source: root.view && root.dir !== "" ? root.dir + root.view.image : ""
        fillMode: Image.PreserveAspectFit
        smooth: true
        mipmap: true
        opacity: 0.85
    }

    // Drag a box to select (Shift/Ctrl adds).
    MouseArea {
        id: drag
        anchors.fill: parent
        enabled: root.interactive
        property point from
        property bool boxing: false
        onPressed: m => {
            from = Qt.point(m.x, m.y);
            boxing = false;
        }
        onPositionChanged: m => {
            if (Math.abs(m.x - from.x) + Math.abs(m.y - from.y) > 6) {
                boxing = true;
                box.x = Math.min(from.x, m.x);
                box.y = Math.min(from.y, m.y);
                box.width = Math.abs(m.x - from.x);
                box.height = Math.abs(m.y - from.y);
            }
        }
        onReleased: m => {
            if (!boxing) {
                return;
            }
            const add = (m.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) !== 0;
            const sel = add ? Object.assign({}, root.selection) : {};
            for (const it of leds.children) {
                if (it.key === undefined) {
                    continue;
                }
                const cx = it.x + it.width / 2, cy = it.y + it.height / 2;
                if (cx >= box.x && cx <= box.x + box.width && cy >= box.y && cy <= box.y + box.height) {
                    sel[it.key] = true;
                }
            }
            boxing = false;
            root.selectionEdited(sel);
        }
    }
    Rectangle {
        id: box
        visible: drag.boxing
        color: Theme.accentTintMid
        border.color: Theme.accent
        z: 5
    }

    Item {
        id: leds
        anchors.fill: parent
    Repeater {
        model: root.points
        delegate: Rectangle {
            required property var modelData
            readonly property string key: "0," + modelData.c
            readonly property bool sel: !!root.selection[key]
            readonly property real px: root.fx + modelData.x * root.fw
            readonly property real py: root.fy + modelData.y * root.fh
            // Logo / wheel discs scale with the render (image pixels); underglow dots stay small.
            width: modelData.d ? Math.max(18, modelData.d * root.fw / (root.view ? root.view.width : 620)) : 28
            height: width
            radius: width / 2
            x: px - width / 2
            y: py - height / 2
            color: root.colours[key] ? "#" + root.colours[key] : "#222222"
            border.color: sel ? Theme.accent : "#80ffffff"
            border.width: sel ? 3 : 1
            HoverHandler {
                id: h
                cursorShape: Qt.PointingHandCursor
            }
            ToolTip {
                visible: h.hovered
                text: modelData.name
            }
            TapHandler {
                enabled: root.interactive
                onTapped: (pt, button) => {
                    const add = (pt.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) !== 0;
                    const s = add ? Object.assign({}, root.selection) : {};
                    if (add && s[key]) {
                        delete s[key];
                    } else {
                        s[key] = true;
                    }
                    root.selectionEdited(s);
                }
            }
        }
    }
    }

    component ToolTip: Rectangle {
        property string text: ""
        width: tt.implicitWidth + 12
        height: tt.implicitHeight + 6
        y: -height - 4
        color: Theme.panelRaised
        z: 10
        Text {
            id: tt
            anchors.centerIn: parent
            text: parent.text
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 11
        }
    }
}
