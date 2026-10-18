import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Home (Synapse 4 dashboard): a card per connected device and per module. Opening a
// card adds it as a tab in the top bar.
Rectangle {
    id: page

    required property var backend
    property var devices: []
    property bool studioReady: false
    signal openDevice(string pid)
    signal openStudio()
    signal openMacros()
    signal openLinks()
    signal openGameLighting()
    signal openSupported()
    property int tick: 0

    Timer {
        interval: 60000
        repeat: true
        running: page.visible
        onTriggered: page.tick++
    }

    color: Theme.bg

    component Card: Rectangle {
        id: card
        property string title: ""
        property string subtitle: ""      // green line under the name (active profile)
        property string detail: ""        // small dim line
        property string image: ""
        property var frame: null          // { rect: [x, y, w, h], w, h }: where the picture sits in the drawing's frame
        property string glyph: ""         // module cards: letter / symbol in a big green circle
        property bool ready: true
        property bool untested: false     // attached model Tsuba hasn't been verified with
        signal opened()

        width: 220
        height: 170
        radius: 2
        color: hov.hovered && ready ? "#242424" : Theme.card
        border.color: hov.hovered && ready ? Theme.accent : "transparent"
        border.width: 1

        Item {
            id: art
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 12
            height: 98
            // The drawing's frame fitted in the box; the picture sits at its `imageRect` inside it.
            readonly property real k: card.frame ? Math.min(width / card.frame.w, height / card.frame.h) : 0
            Image {
                x: card.frame ? (parent.width - card.frame.w * parent.k) / 2 + card.frame.rect[0] * parent.k : 0
                y: card.frame ? (parent.height - card.frame.h * parent.k) / 2 + card.frame.rect[1] * parent.k : 0
                width: card.frame ? card.frame.rect[2] * parent.k : parent.width
                height: card.frame ? card.frame.rect[3] * parent.k : parent.height
                visible: card.image !== ""
                source: card.image
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
                asynchronous: true
                opacity: card.ready ? 1 : 0.4
            }
            Rectangle {
                anchors.centerIn: parent
                visible: card.image === ""
                width: 64
                height: 64
                radius: 32
                color: card.ready ? Theme.accent : "#3a3a3a"
                Text {
                    anchors.centerIn: parent
                    visible: ["studio", "macro", "links", "devices", "gamelight"].indexOf(card.glyph) < 0
                    text: card.glyph
                    color: Theme.accentText
                    font.pixelSize: 34
                    font.bold: true
                }
                AppGlyph {
                    anchors.centerIn: parent
                    visible: ["studio", "macro", "links", "devices", "gamelight"].indexOf(card.glyph) >= 0
                    width: 36
                    height: 36
                    kind: card.glyph
                    colour: Theme.accentText
                }
            }
        }
        Rectangle {
            objectName: "untestedBadge"
            visible: card.untested
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.margins: 8
            width: badgeText.implicitWidth + 12
            height: 18
            radius: 9
            color: Theme.panelRaised
            border.color: Theme.warn
            Text {
                id: badgeText
                anchors.centerIn: parent
                text: "UNTESTED"
                color: Theme.warn
                font.family: Theme.font
                font.pixelSize: 9
                font.letterSpacing: 0.5
            }
        }
        Column {
            anchors.left: parent.left
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.rightMargin: 10
            anchors.top: art.bottom
            anchors.topMargin: 8
            spacing: 2
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: card.title.toUpperCase()
                color: card.ready ? Theme.text : Theme.textMuted
                font.family: Theme.font
                font.pixelSize: Theme.small
                elide: Text.ElideRight
            }
            Text {
                visible: card.subtitle !== ""
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: card.subtitle
                color: card.ready ? Theme.accent : Theme.textMuted
                font.family: Theme.font
                font.pixelSize: 11
                elide: Text.ElideRight
            }
            Text {
                visible: card.detail !== ""
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: card.detail
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: 10
                elide: Text.ElideRight
            }
        }
        HoverHandler {
            id: hov
            cursorShape: card.ready ? Qt.PointingHandCursor : Qt.ArrowCursor
        }
        TapHandler {
            enabled: card.ready
            onTapped: card.opened()
        }
    }

    // "▾ DEVICES" style heading that folds its section.
    component Section: Text {
        id: sec
        property string caption: ""
        property bool open: true
        text: (open ? "▾  " : "▸  ") + caption.toUpperCase()
        color: Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.small
        font.letterSpacing: 0.5
        TapHandler {
            onTapped: sec.open = !sec.open
        }
        HoverHandler {
            cursorShape: Qt.PointingHandCursor
        }
    }

    function activeProfileName(pid) {
        const s = backend.library_profiles(pid);
        if (s === "") {
            return "";
        }
        const p = JSON.parse(s);
        const x = (p.profiles || []).find(q => q.id === p.active);
        if (x) {
            return x.name;
        }
        const o = (p.slots || []).find(q => q.slot === p.active_slot);
        return o ? (o.name !== "" ? o.name : "On-board profile " + o.slot) : "";
    }

    function isUntested(pid) {
        const s = backend.device_support(pid);
        if (s === "") {
            return false;
        }
        const v = JSON.parse(s);
        return v.tested === false && v.supported !== false;
    }

    function packFor(pid) {
        Theme.picRev;
        const p = backend.device_pack(pid);
        if (p === "") {
            return null;
        }
        const pack = JSON.parse(p);
        return pack.views && pack.views.top ? pack : null;
    }
    function imageFor(pid) {
        const pack = packFor(pid);
        return pack ? pack.dir + pack.views.top.image : "";
    }
    function frameFor(pid) {
        const pack = packFor(pid);
        return pack ? { rect: Theme.imageRect(pack.views.top), w: pack.views.top.width, h: pack.views.top.height } : null;
    }

    QQC2.ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(page.width - 80, 1000)
            x: Math.max(40, (page.width - width) / 2)
            spacing: 14

            Item {
                Layout.preferredHeight: 14
            }
            Section {
                id: devSec
                caption: "Devices"
            }
            Flow {
                visible: devSec.open
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                spacing: 16
                Repeater {
                    model: page.devices
                    delegate: Card {
                        required property var modelData
                        title: modelData.name.replace(" (wired)", "")
                        // Re-read every minute (page.tick) so the battery badge stays current.
                        readonly property var pw: { page.tick; return modelData.kind === "mouse" ? JSON.parse(page.backend.power(modelData.pid) || "{}") : ({}); }
                        subtitle: { page.tick; return page.activeProfileName(modelData.pid); }
                        untested: { page.tick; return page.isUntested(modelData.pid); }
                        detail: DeviceKinds.kindLabel(modelData.kind) + (modelData.name.indexOf("(wired)") >= 0 ? " · Wired" : "")
                            + (pw.battery_percent !== undefined ? " · " + pw.battery_percent + "%" + (pw.charging ? " charging" : "") : "")
                        image: page.imageFor(modelData.pid)
                        frame: page.frameFor(modelData.pid)
                        glyph: DeviceKinds.family(modelData.kind) === "mouse" ? "◖" : "▦"
                        onOpened: page.openDevice(modelData.pid)
                    }
                }
                Text {
                    visible: page.devices.length === 0
                    text: page.backend.connected ? "No supported devices connected." : "tsubad isn't running."
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                }
            }

            Item {
                Layout.preferredHeight: 10
            }
            Section {
                id: modSec
                caption: "Modules"
            }
            Flow {
                visible: modSec.open
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                spacing: 16
                Card {
                    title: "Lighting Studio"
                    detail: page.studioReady ? "" : "Needs a per-LED device"
                    glyph: "studio"
                    ready: page.studioReady
                    onOpened: page.openStudio()
                }
                Card {
                    title: "Macro"
                    glyph: "macro"
                    onOpened: page.openMacros()
                }
                Card {
                    title: "Linked games"
                    glyph: "links"
                    onOpened: page.openLinks()
                }
                Card {
                    objectName: "gameLightingCard"
                    title: "Game lighting"
                    detail: "Games light up your devices"
                    glyph: "gamelight"
                    onOpened: page.openGameLighting()
                }
                Card {
                    objectName: "supportedDevicesCard"
                    title: "Supported devices"
                    detail: "Browse every model Tsuba knows"
                    glyph: "devices"
                    onOpened: page.openSupported()
                }
            }
            Item {
                Layout.preferredHeight: 24
            }
            Text {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                wrapMode: Text.WordWrap
                text: "Tsuba is an independent open-source project, not affiliated with or endorsed by Razer Inc. Razer, Synapse, Chroma, HyperShift, BlackWidow and Basilisk are trademarks of Razer Inc., named here only to identify compatible hardware."
                color: Theme.textMuted
                font.family: Theme.font
                font.pixelSize: 11
            }
            Item {
                Layout.preferredHeight: 16
            }
        }
    }
}
