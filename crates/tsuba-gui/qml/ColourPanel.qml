import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtCore

// Tsuba colour panel: saturation/brightness area, hue strip, HEX + RGB fields and swatches.
// `colour` is "rrggbb" (no #); edits are reported through `edited`.
ColumnLayout {
    id: panel

    property string colour: "44d62c"
    signal edited(string hex)

    spacing: 10

    property real hue: 0
    property real sat: 1
    property real val: 1
    property bool internal: false

    readonly property var presets: ["ff2020", "ff8a00", "ffd200", "44d62c", "1fc9a7", "0080ff", "7a3cff", "ff3cc8", "ffffff", "808080"]
    property var custom: []

    Settings {
        id: store
        location: StandardPaths.writableLocation(StandardPaths.ConfigLocation) + "/tsuba/app.ini"
        category: "swatches"
        property string custom: ""
    }
    Component.onCompleted: {
        custom = store.custom === "" ? [] : store.custom.split(",").filter(x => /^[0-9a-f]{6}$/.test(x));
        syncFrom(colour);
    }

    function hex2(n) {
        return Math.round(Math.max(0, Math.min(255, n))).toString(16).padStart(2, "0");
    }
    function fromHsv(h, s, v) {
        const c = Qt.hsva(h, s, v, 1);
        return hex2(c.r * 255) + hex2(c.g * 255) + hex2(c.b * 255);
    }
    function syncFrom(hex) {
        if (!/^[0-9a-fA-F]{6}$/.test(hex)) {
            return;
        }
        if (fromHsv(hue, sat, val) === hex.toLowerCase()) {
            return;
        }
        const c = Qt.color("#" + hex);
        // Keep the hue when the colour is grey/black so the strip doesn't jump.
        if (c.hsvSaturation > 0 && c.hsvValue > 0) {
            hue = c.hsvHue;
        }
        sat = c.hsvSaturation;
        val = c.hsvValue;
    }
    onColourChanged: syncFrom(colour)
    function commitHsv() {
        edited(fromHsv(hue, sat, val));
    }
    function commitHex(h) {
        const x = h.replace("#", "").toLowerCase();
        if (/^[0-9a-f]{6}$/.test(x)) {
            edited(x);
        }
    }
    function addCustom() {
        const hx = colour.toLowerCase();
        if (custom.indexOf(hx) >= 0) {
            return;
        }
        const c = [hx].concat(custom).slice(0, 12);
        custom = c;
        store.custom = c.join(",");
    }

    // Saturation (x) / brightness (y) area
    Rectangle {
        id: area
        objectName: "colourArea"
        Layout.fillWidth: true
        Layout.preferredHeight: 150
        border.color: Theme.borderSoft
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#ffffff" }
            GradientStop { position: 1; color: Qt.hsva(panel.hue, 1, 1, 1) }
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: "#00000000" }
                GradientStop { position: 1; color: "#ff000000" }
            }
        }
        Rectangle {
            x: panel.sat * area.width - 7
            y: (1 - panel.val) * area.height - 7
            width: 14
            height: 14
            radius: 7
            color: "transparent"
            border.color: "#ffffff"
            border.width: 2
            Rectangle {
                anchors.fill: parent
                anchors.margins: -1
                radius: width / 2
                color: "transparent"
                border.color: "#80000000"
                border.width: 1
                z: -1
            }
        }
        MouseArea {
            anchors.fill: parent
            function pick(m) {
                panel.sat = Math.max(0, Math.min(1, m.x / width));
                panel.val = 1 - Math.max(0, Math.min(1, m.y / height));
                panel.commitHsv();
            }
            onPressed: m => pick(m)
            onPositionChanged: m => pick(m)
        }
    }
    // Hue strip
    Rectangle {
        id: strip
        Layout.fillWidth: true
        Layout.preferredHeight: 16
        border.color: Theme.borderSoft
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: "#ff0000" }
            GradientStop { position: 0.1667; color: "#ffff00" }
            GradientStop { position: 0.3333; color: "#00ff00" }
            GradientStop { position: 0.5; color: "#00ffff" }
            GradientStop { position: 0.6667; color: "#0000ff" }
            GradientStop { position: 0.8333; color: "#ff00ff" }
            GradientStop { position: 1.0; color: "#ff0000" }
        }
        Rectangle {
            x: Math.max(0, Math.min(strip.width - width, panel.hue * strip.width - width / 2))
            y: -2
            width: 8
            height: strip.height + 4
            color: "transparent"
            border.color: "#ffffff"
            border.width: 2
        }
        MouseArea {
            anchors.fill: parent
            function pick(m) {
                panel.hue = Math.max(0, Math.min(0.9999, m.x / width));
                panel.commitHsv();
            }
            onPressed: m => pick(m)
            onPositionChanged: m => pick(m)
        }
    }

    // HEX / R / G / B
    RowLayout {
        Layout.fillWidth: true
        spacing: 6
        Repeater {
            model: [
                { k: "HEX", w: 2 },
                { k: "R", w: 1 },
                { k: "G", w: 1 },
                { k: "B", w: 1 }
            ]
            delegate: ColumnLayout {
                id: fld
                required property var modelData
                required property int index
                Layout.fillWidth: true
                Layout.preferredWidth: modelData.w
                spacing: 2
                Text {
                    text: modelData.k
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: 10
                    font.letterSpacing: 1
                }
                QQC2.TextField {
                    objectName: "colourField" + modelData.k
                    Layout.fillWidth: true
                    implicitHeight: 28
                    leftPadding: 6
                    rightPadding: 4
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    selectByMouse: true
                    maximumLength: fld.index === 0 ? 7 : 3
                    inputMethodHints: Qt.ImhNoPredictiveText
                    text: {
                        const c = panel.colour;
                        return fld.index === 0 ? c.toUpperCase() : String(parseInt(c.substr(fld.index * 2 - 2, 2), 16));
                    }
                    background: Rectangle {
                        color: Theme.bg
                        border.color: parent.activeFocus ? Theme.accent : Theme.borderSoft
                    }
                    onEditingFinished: {
                        if (fld.index === 0) {
                            panel.commitHex(text);
                        } else {
                            const p = [0, 2, 4].map(o => parseInt(panel.colour.substr(o, 2), 16));
                            p[fld.index - 1] = Math.max(0, Math.min(255, parseInt(text) || 0));
                            panel.commitHex(p.map(panel.hex2).join(""));
                        }
                        // Re-sync the text when the edit was rejected.
                        text = Qt.binding(() => fld.index === 0 ? panel.colour.toUpperCase() : String(parseInt(panel.colour.substr(fld.index * 2 - 2, 2), 16)));
                    }
                }
            }
        }
    }

    // Swatches
    Text {
        text: "SWATCHES"
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: 10
        font.letterSpacing: 1
    }
    Flow {
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        spacing: 6
        Repeater {
            model: panel.presets.concat(panel.custom)
            delegate: Rectangle {
                required property string modelData
                width: 22
                height: 22
                radius: 3
                color: "#" + modelData
                border.color: modelData === panel.colour.toLowerCase() ? Theme.accent : Theme.borderSoft
                border.width: modelData === panel.colour.toLowerCase() ? 2 : 1
                TapHandler {
                    onTapped: panel.edited(modelData)
                }
                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }
            }
        }
        Rectangle {
            objectName: "addSwatch"
            width: 22
            height: 22
            radius: 3
            color: "transparent"
            border.color: Theme.border
            Rectangle { anchors.centerIn: parent; width: 10; height: 1.5; color: Theme.text }
            Rectangle { anchors.centerIn: parent; width: 1.5; height: 10; color: Theme.text }
            TapHandler {
                onTapped: panel.addCustom()
            }
            HoverHandler {
                cursorShape: Qt.PointingHandCursor
            }
        }
    }
}
