import QtQuick
import QtQuick.Controls as QQC2

// Small pill showing who controls a device's lighting right now: a game (over Tsuba's Chroma
// interface), the Lighting Studio, or the device's own effect. `pid` = the device.
Rectangle {
    id: chip

    required property var backend
    required property string pid

    // {owner: "game" | "studio" | "device", name, link} from the daemon.
    property var info: ({ owner: "device", name: "", link: null })
    readonly property string owner: info.owner || "device"
    readonly property string caption: {
        if (owner === "game") {
            return i18n("Game: %1", info.name || i18n("a game"));
        }
        if (owner === "studio") {
            return info.name ? i18n("Lighting Studio: %1", info.name) : i18n("Lighting Studio");
        }
        return i18n("Device lighting");
    }
    readonly property color tone: owner === "game" ? Theme.warn : (owner === "studio" ? Theme.accent : Theme.textDim)

    function refresh() {
        if (typeof backend.studio_call !== "function" || pid === "") {
            return;
        }
        const j = JSON.parse(backend.studio_call("owner", pid) || "{}");
        if (j && j.owner) {
            info = j;
        }
    }

    implicitHeight: 24
    implicitWidth: row.implicitWidth + 20
    radius: height / 2
    color: Qt.rgba(tone.r, tone.g, tone.b, 0.12)
    border.color: tone
    border.width: 1

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 7
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 8
            height: 8
            radius: 4
            color: chip.tone
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chip.caption
            width: Math.min(implicitWidth, 280)
            elide: Text.ElideRight
            color: Theme.text

            font.family: Theme.font
            font.pixelSize: Theme.small
        }
    }
    HoverHandler {
        id: hover
    }
    QQC2.ToolTip.visible: hover.hovered
    QQC2.ToolTip.delay: 400
    QQC2.ToolTip.text: owner === "game" ? i18n("A game is lighting this device through Tsuba's Chroma interface. Your own lighting comes back when it closes.")
        : (owner === "studio" ? i18n("Tsuba is running a Lighting Studio profile on this device.") : i18n("The device is showing its own lighting effect.")) + (info.link ? "\n" + i18n("Linked game profile active: %1", info.link) : "")

    Component.onCompleted: refresh()
    onPidChanged: refresh()
    Timer {
        interval: 3000
        repeat: true
        running: chip.visible
        onTriggered: chip.refresh()
    }
    Connections {
        target: chip.backend
        ignoreUnknownSignals: true
        function onGame_lighting_changed(status) {
            chip.refresh();
        }
    }
}
