import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Home (Synapse 4 dashboard): a card per connected device and per module. Opening a
// card adds it as a tab in the top bar.
Rectangle {
    id: page

    required property var backend
    // AppBackend (service start-up) and the parsed `tsubad --check-env` report, for the setup guide.
    property var app: null
    property var envReport: ({})
    property var devices: []
    property bool studioReady: false
    signal openDevice(string pid)
    signal openStudio()
    signal openMacros()
    signal openLinks()
    signal openGameLighting()
    signal openSupported()
    signal recheck()
    signal openUrl(string url)
    signal exportDiagnostics()
    property int tick: 0
    // Daemon helper for the firmware's own "wired" / "wireless" answer (GetFeatures).
    property var devb: null
    function connectionOf(item) {
        tick;
        if (!devb) {
            try {
                devb = Qt.createQmlObject("import io.github.jenaridev.tsuba\nDeviceBackend {}", page, "DeviceBackend");   // i18n-ok
            } catch (e) {
                devb = null;
            }
        }
        const s = devb ? devb.features(item.pid, 1) : "";
        const c = s !== "" ? JSON.parse(s).connection : "";
        if (c === "wired" || c === "wireless") {
            return c;
        }
        // The daemon doesn't know (yet): the device name and a battery reading hint at it.
        const pw = item.kind === "mouse" ? JSON.parse(backend.power(item.pid) || "{}") : ({});
        return item.name.indexOf("(wired)") >= 0 ? "wired" : (pw.battery_percent !== undefined ? "wireless" : "");
    }
    property int tipIndex: Math.floor(Date.now() / 86400000)

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
        property string connection: ""    // "wired" | "wireless" badge (top left)
        property int battery: -1          // percent, -1 = no battery
        property bool charging: false
        signal opened()

        width: 220
        height: 170
        radius: 2
        color: hov.hovered && ready ? Theme.hoverFill : Theme.card
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
                color: card.ready ? Theme.accent : Theme.inactive
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
        Row {
            objectName: "cardBadges"
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.margins: 8
            spacing: 4
            Rectangle {
                objectName: "connectionBadge"
                visible: card.connection !== ""
                width: connText.implicitWidth + 12
                height: 18
                radius: 9
                color: Theme.panelRaised
                border.color: Theme.borderSoft
                Text {
                    id: connText
                    anchors.centerIn: parent
                    text: card.connection === "wired" ? i18n("WIRED") : i18n("WIRELESS")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: 9
                    font.letterSpacing: 0.5
                }
            }
            Rectangle {
                objectName: "batteryBadge"
                visible: card.battery >= 0
                width: batText.implicitWidth + 12
                height: 18
                radius: 9
                color: Theme.panelRaised
                border.color: card.battery <= 15 && !card.charging ? Theme.warn : Theme.borderSoft
                Text {
                    id: batText
                    anchors.centerIn: parent
                    text: (card.charging ? "⚡ " : "") + card.battery + "%"
                    color: card.battery <= 15 && !card.charging ? Theme.warn : Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: 9
                    font.letterSpacing: 0.5
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
                text: i18n("UNTESTED")
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
        return o ? (o.name !== "" ? o.name : i18n("On-board profile %1", o.slot)) : "";
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

    readonly property var groups: {
        const order = DeviceKinds.groupOrder;
        const m = {};
        for (const d of devices) {
            const k = order.indexOf(d.kind) < 0 ? "other" : d.kind;
            (m[k] = m[k] || []).push(d);
        }
        return Object.keys(m).sort((x, y) => (order.indexOf(x) < 0 ? 99 : order.indexOf(x)) - (order.indexOf(y) < 0 ? 99 : order.indexOf(y)))
            .map(k => ({ kind: k, label: DeviceKinds.groupLabel(k), items: m[k] }));
    }

    readonly property var tips: [
        i18n("Hold a button's Shift layer key to reach a second set of assignments on the same device."),
        i18n("Linked games switches profiles for you when a game starts and puts them back when it closes."),
        i18n("Lighting Studio can layer several effects on a keyboard, each on its own set of keys."),
        i18n("Game lighting lets games that support Razer Chroma light your devices."),
        i18n("With the tray icon you can switch a device's profile without opening Tsuba."),
        i18n("If something doesn't work, Settings has a diagnostics export you can attach to a bug report.")
    ]

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
                caption: i18n("Devices")
            }
            // No device: say why and what to do.
            SetupGuide {
                objectName: "setupGuide"
                visible: devSec.open && page.devices.length === 0
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                report: page.envReport
                connected: page.backend.connected
                app: page.app
                onRecheck: page.recheck()
                onOpenSupported: page.openSupported()
                onOpenUrl: url => page.openUrl(url)
                onExportDiagnostics: page.exportDiagnostics()
            }
            Repeater {
                model: devSec.open ? page.groups : []
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    spacing: 8
                    Text {
                        visible: page.groups.length > 1
                        text: modelData.label.toUpperCase()
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: 11
                        font.letterSpacing: 0.5
                    }
                    Flow {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        spacing: 16
                        Repeater {
                            model: modelData.items
                            delegate: Card {
                                required property var modelData
                                title: modelData.name.replace(" (wired)", "")
                                // Re-read every minute (page.tick) so the battery badge stays current.
                                readonly property var pw: { page.tick; return modelData.kind === "mouse" ? JSON.parse(page.backend.power(modelData.pid) || "{}") : ({}); }
                                subtitle: { page.tick; return page.activeProfileName(modelData.pid); }
                                untested: { page.tick; return page.isUntested(modelData.pid); }
                                detail: DeviceKinds.kindLabel(modelData.kind)
                                connection: page.connectionOf(modelData)
                                battery: pw.battery_percent !== undefined ? pw.battery_percent : -1
                                charging: !!pw.charging
                                image: page.imageFor(modelData.pid)
                                frame: page.frameFor(modelData.pid)
                                glyph: DeviceKinds.family(modelData.kind) === "mouse" ? "◖" : "▦"
                                onOpened: page.openDevice(modelData.pid)
                            }
                        }
                    }
                }
            }

            Item {
                Layout.preferredHeight: 10
            }
            Section {
                id: modSec
                caption: i18n("Modules")
            }
            Flow {
                visible: modSec.open
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                spacing: 16
                Card {
                    title: i18n("Lighting Studio")
                    detail: page.studioReady ? "" : i18n("Needs a per-LED device")
                    glyph: "studio"
                    ready: page.studioReady
                    onOpened: page.openStudio()
                }
                Card {
                    title: i18n("Macro")
                    glyph: "macro"
                    onOpened: page.openMacros()
                }
                Card {
                    title: i18n("Linked games")
                    glyph: "links"
                    onOpened: page.openLinks()
                }
                Card {
                    objectName: "gameLightingCard"
                    title: i18n("Game lighting")
                    detail: i18n("Games light up your devices")
                    glyph: "gamelight"
                    onOpened: page.openGameLighting()
                }
                Card {
                    objectName: "supportedDevicesCard"
                    title: i18n("Supported devices")
                    detail: i18n("Browse every model Tsuba knows")
                    glyph: "devices"
                    onOpened: page.openSupported()
                }
            }

            // Tip of the day (switch off in Settings).
            Rectangle {
                objectName: "tipBox"
                visible: Theme.app.showTips
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.topMargin: 10
                implicitHeight: tipText.implicitHeight + 24
                radius: 5
                color: Theme.accentTint
                border.color: Theme.accentTintStrong
                Text {
                    id: tipText
                    x: 16
                    y: 12
                    width: parent.width - 32
                    text: i18n("Tip: %1", page.tips[page.tipIndex % page.tips.length])
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small + 1
                    wrapMode: Text.WordWrap
                }
                TapHandler {
                    onTapped: page.tipIndex++
                }
                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }
            }

            Item {
                Layout.preferredHeight: 14
            }
            Text {
                objectName: "nonAffiliation"
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                wrapMode: Text.WordWrap
                text: i18n("Tsuba is an independent open-source project, not affiliated with or endorsed by Razer Inc. Razer, Synapse, Chroma, HyperShift, BlackWidow and Basilisk are trademarks of Razer Inc., named here only to identify compatible hardware.")
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
