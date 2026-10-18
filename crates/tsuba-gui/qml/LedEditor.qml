import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

// Per-key lighting editor (Synapse Chroma-style): select keys on the render, paint,
// preview live on the keyboard, save as the startup lighting.
Item {
    id: ed

    required property var backend
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    required property string pid
    required property var pack

    readonly property var grid: pack && pack.grid ? pack.grid : [9, 24]
    readonly property var leds: pack ? pack.views.top.hotspots.filter(h => h.led !== null) : []
    property var colours: ({})
    property var selection: ({})
    property string brush: "44d62c"
    property bool dirty: false

    readonly property var groups: [
        { label: "All", test: h => true },
        { label: "WASD", test: h => (h.group || "").indexOf("wasd") >= 0 },
        { label: "Arrows", test: h => (h.group || "").indexOf("arrows") >= 0 },
        { label: "Letters", test: h => (h.group || "").indexOf("alphabets") >= 0 },
        { label: "Number row", test: h => (h.group || "").indexOf("numberRows") >= 0 },
        { label: "F-keys", test: h => (h.group || "").indexOf("functions") >= 0 },
        { label: "Numpad", test: h => (h.group || "").indexOf("numpad") >= 0 },
        { label: "Macro", test: h => h.group === "macro" },
        { label: "Media", test: h => (h.group || "").indexOf("multimedia") >= 0 },
        { label: "Underglow", test: h => h.group === "underglow" }
    ]

    function rows() {
        const out = [];
        for (let r = 0; r < grid[0]; r++) {
            const cells = [];
            for (let c = 0; c < grid[1]; c++) {
                cells.push(colours[r + "," + c] || "000000");
            }
            out.push(cells.join(" "));
        }
        return out;
    }
    function effectJson() {
        return JSON.stringify({ type: "custom", rows: rows() });
    }
    function loadSaved() {
        const cfg = JSON.parse(backend.config(pid) || "{}");
        const e = cfg.lighting ? cfg.lighting.effect : null;
        const m = {};
        if (e && e.type === "custom") {
            e.rows.forEach((row, r) => row.split(/\s+/).forEach((h, c) => {
                if (h && h !== "000000") {
                    m[r + "," + c] = h;
                }
            }));
        }
        colours = m;
    }
    function paint(hex) {
        const m = Object.assign({}, colours);
        Object.keys(selection).forEach(k => {
            if (hex === "000000") {
                delete m[k];
            } else {
                m[k] = hex;
            }
        });
        colours = m;
        dirty = true;
        push.restart();
    }
    function selectGroup(g) {
        const s = {};
        leds.filter(g.test).forEach(h => s[h.led[0] + "," + h.led[1]] = true);
        selection = s;
    }

    Component.onCompleted: loadSaved()

    // Live preview, debounced so dragging through colours doesn't flood the device.
    Timer {
        id: push
        interval: 90
        onTriggered: ed.backend.set_effect(ed.pid, ed.effectJson(), 255)
    }

    ColorDialog {
        id: picker
        title: "Brush colour"
        onAccepted: ed.brush = selectedColor.toString().replace("#", "").slice(-6)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 14

        ZoomCanvas {
            id: zoomCanvas
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 260
            onBoxSelected: (r, add) => kb.selectBox(r, add)
            onTapped: (p, add) => kb.selectAt(p, add)
            LedKeyboard {
                id: kb
                anchors.fill: parent
                interactive: false
                view: ed.pack.views.top
                dir: ed.pack.dir
                colours: ed.colours
                selection: ed.selection
                onSelectionEdited: sel => ed.selection = sel
            }
        }

        ApproxNote {
            objectName: "approxNote"
            visible: !!ed.pack && !!ed.pack.approximate
            Layout.alignment: Qt.AlignHCenter
            backend: ed.backend
            pid: ed.pid
        }

        // Quick select
        Flow {
            Layout.fillWidth: true
            spacing: 6
            SectionLabel {
                caption: "Select"
                width: 70
                height: 28
                verticalAlignment: Text.AlignVCenter
            }
            Repeater {
                model: ed.groups
                delegate: Rectangle {
                    required property var modelData
                    width: chip.implicitWidth + 22
                    height: 28
                    color: hov.hovered ? "#1affffff" : Theme.panel
                    border.color: Theme.borderSoft
                    Text {
                        id: chip
                        anchors.centerIn: parent
                        text: modelData.label
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                    HoverHandler {
                        id: hov
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: ed.selectGroup(modelData)
                    }
                }
            }
            Rectangle {
                width: clearSel.implicitWidth + 22
                height: 28
                color: "transparent"
                border.color: Theme.borderSoft
                Text {
                    id: clearSel
                    anchors.centerIn: parent
                    text: "Clear selection"
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                TapHandler {
                    onTapped: ed.selection = {}
                }
            }
        }

        // Brush + actions
        RowLayout {
            spacing: 12
            SectionLabel {
                caption: "Colour"
            }
            Repeater {
                model: ["44d62c", "ff0000", "ff8000", "ffff00", "00ffff", "0060ff", "a000ff", "ff00a0", "ffffff"]
                delegate: Rectangle {
                    required property string modelData
                    width: 26
                    height: 26
                    radius: 13
                    color: "#" + modelData
                    border.color: ed.brush === modelData ? Theme.textBright : "transparent"
                    border.width: 2
                    TapHandler {
                        onTapped: ed.brush = modelData
                    }
                }
            }
            Rectangle {
                width: 26
                height: 26
                radius: 13
                color: "#" + ed.brush
                border.color: Theme.text
                Text {
                    anchors.centerIn: parent
                    text: "+"
                    color: Theme.bg
                    font.bold: true
                }
                TapHandler {
                    onTapped: {
                        picker.selectedColor = "#" + ed.brush;
                        picker.open();
                    }
                }
            }
            Item {
                Layout.preferredWidth: 12
            }
            SynButton {
                text: "Paint"
                primary: true
                enabled: !ed.readOnly && Object.keys(ed.selection).length > 0
                onClicked: ed.paint(ed.brush)
            }
            SynButton {
                text: "Turn off"
                enabled: !ed.readOnly && Object.keys(ed.selection).length > 0
                onClicked: ed.paint("000000")
            }
            Item {
                Layout.fillWidth: true
            }
            Text {
                text: Object.keys(ed.selection).length + " selected"
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            SynButton {
                text: ed.dirty ? "Save" : "Saved"
                enabled: !ed.readOnly && ed.dirty
                onClicked: if (ed.backend.save_lighting(ed.pid, ed.effectJson(), 255)) ed.dirty = false
            }
        }
    }
}
