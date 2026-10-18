import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import io.github.jenaridev.tsuba

QQC2.ApplicationWindow {
    id: root

    title: "Tsuba"
    visible: true
    width: 1440
    height: 900
    minimumWidth: 1100
    minimumHeight: 700
    color: Theme.bg
    // Razer green for any stock control that reads the palette.
    palette.highlight: Theme.accent
    palette.accent: Theme.accent

    property var deviceList: []
    property string currentPid: ""
    property string lastEvent: ""
    property bool updated: false

    Timer {
        interval: 10000
        repeat: true
        running: !root.updated
        onTriggered: root.updated = backend.binary_replaced()
    }

    function describeEvent(pid, e) {
        const who = (root.deviceList.find(d => d.pid === pid) || { name: pid }).name;
        const media = { 0xe2: "mute", 0xb6: "previous", 0xb5: "next", 0xcd: "play / pause", 0xe9: "volume up", 0xea: "volume down" };
        switch (e.kind) {
        case "media":
            return e.usages.length ? `${who}: ${e.usages.map(u => media[u] || "0x" + u.toString(16)).join(", ")}` : "";
        case "razer_keys":
            return e.keys.length ? `${who}: ${e.keys.map(k => k.name).join(", ")}` : "";
        case "notification":
            if (e.id === 2) {
                return `${who}: sensitivity ${(e.data[0] << 8) | e.data[1]} DPI`;
            }
            if (e.id === 57) {
                return `${who}: scroll wheel ${e.data[0] ? "free-spin" : "tactile"}`;
            }
            return e.name ? `${who}: ${e.name}` : "";
        default:
            return "";
        }
    }

    // Open pages are tabs (Synapse 4): HOME is fixed, devices and modules open beside it
    // and stay alive while you switch between them.
    property int currentTab: 0
    property int returnTab: 0
    readonly property string currentKey: currentTab > 0 && currentTab <= tabModel.count ? tabModel.get(currentTab - 1).key : ""
    readonly property string pageTitle: {
        if (currentTab === 0 || currentTab > tabModel.count) {
            return "TSUBA";
        }
        const t = tabModel.get(currentTab - 1);
        return t.title.replace(/^Razer /i, "");
    }
    readonly property var appBackend: backend
    readonly property bool studioReady: root.deviceList.some(d => d.kind === "keyboard" && backend.device_pack(d.pid) !== "")

    ListModel {
        id: tabModel
    }

    function openTab(key, title, source, props) {
        for (let i = 0; i < tabModel.count; i++) {
            if (tabModel.get(i).key === key) {
                root.currentTab = i + 1;
                return;
            }
        }
        tabModel.append({ key: key, title: title, source: source, props: JSON.stringify(props) });
        root.currentTab = tabModel.count;
    }

    function closeTab(i) {
        tabModel.remove(i);
        if (root.currentTab > i + 1 || root.currentTab > tabModel.count) {
            root.currentTab = Math.max(0, root.currentTab - 1);
        }
    }

    function openStudio() {
        // Chroma Studio needs a per-LED device with an image pack (the keyboard today).
        const kb = root.deviceList.find(d => d.kind === "keyboard" && backend.device_pack(d.pid) !== "");
        if (kb) {
            root.openTab("studio", "Lighting Studio", "StudioPage.qml", { pid: kb.pid });
        }
    }

    function openLinks() {
        root.openTab("links", "Linked games", "LinkedGamesPage.qml", {});
    }

    function openGameLighting() {
        root.openTab("gamelight", "Game lighting", "GameLightingPage.qml", {});
    }

    function openSupported() {
        root.openTab("supported", "Supported devices", "SupportedDevicesPage.qml", {});
    }

    function openMacros() {
        root.openTab("macros", "Macros", "MacrosPage.qml", {});
    }

    function openDevice(pid) {
        const d = root.deviceList.find(x => x.pid === pid);
        const tab = parseInt(backend.env_var("TSUBA_TAB") || "0");
        root.openTab(pid, d ? d.name.replace(" (wired)", "").replace(/^Razer /i, "") : pid, "DevicePage.qml", { pid: pid, tabIndex: tab });
    }

    // Close tabs of devices that went away.
    onDeviceListChanged: {
        for (let i = tabModel.count - 1; i >= 0; i--) {
            const t = tabModel.get(i);
            if (t.source === "DevicePage.qml" && !root.deviceList.some(d => d.pid === t.key)) {
                root.closeTab(i);
            }
        }
    }

    Backend {
        id: backend
        onDevicesChanged: root.deviceList = JSON.parse(backend.devices || "[]")
        onHardware_event: (pid, event) => {
            const t = root.describeEvent(pid, JSON.parse(event));
            if (t !== "") {
                root.lastEvent = t;
            }
        }
    }

    ColumnLayout {
        id: shell
        anchors.fill: parent
        spacing: 0

        // ---- Row 1: black app-tab strip (SYNAPSE / MACRO / LINKED GAMES / one tab per device)
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: Theme.chrome

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 4
                anchors.topMargin: 6
                spacing: 2

                component AppTab: Rectangle {
                    id: ptab
                    property string caption: ""
                    property string glyph: "tsuba"
                    property bool active: false
                    property bool closable: false
                    signal selected()
                    signal closed()
                    Layout.fillHeight: true
                    Layout.preferredWidth: Math.min(280, label.implicitWidth + 62 + (closable ? 24 : 0))
                    radius: 5
                    color: active ? Theme.panelRaised : (th.hovered ? "#141414" : "transparent")
                    border.color: active ? Theme.accent : "transparent"
                    border.width: 1
                    // Square off the bottom corners so the tab merges into the page below.
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: 6
                        color: ptab.color
                        Rectangle {
                            width: 1
                            height: parent.height
                            color: ptab.border.color
                        }
                        Rectangle {
                            x: parent.width - 1
                            width: 1
                            height: parent.height
                            color: ptab.border.color
                        }
                    }
                    Item {
                        id: icon
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: 18
                        height: 18
                        // Tsuba's own glyphs: app icons in the accent colour, mouse / keyboard outlines for devices.
                        AppGlyph {
                            visible: ["tsuba", "macro", "links", "studio", "gamelight"].indexOf(ptab.glyph) >= 0
                            anchors.fill: parent
                            kind: ptab.glyph
                        }
                        Rectangle {
                            visible: ptab.glyph === "page"
                            anchors.centerIn: parent
                            width: 10
                            height: 10
                            radius: 5
                            color: Theme.accent
                        }
                        Rectangle {
                            visible: ptab.glyph === "mouse"
                            anchors.centerIn: parent
                            width: 12
                            height: 18
                            radius: 6
                            color: "transparent"
                            border.color: Theme.text
                            border.width: 1
                            Rectangle {
                                anchors.horizontalCenter: parent.horizontalCenter
                                y: 3
                                width: 1
                                height: 5
                                color: Theme.text
                            }
                        }
                        Rectangle {
                            visible: ptab.glyph === "keyboard"
                            anchors.centerIn: parent
                            width: 18
                            height: 11
                            radius: 2
                            color: "transparent"
                            border.color: Theme.text
                            border.width: 1
                        }
                    }
                    Text {
                        id: label
                        anchors.left: icon.right
                        anchors.leftMargin: 10
                        anchors.right: parent.right
                        anchors.rightMargin: ptab.closable ? 34 : 14
                        anchors.verticalCenter: parent.verticalCenter
                        text: ptab.caption.toUpperCase()
                        color: ptab.active || th.hovered ? Theme.textBright : Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        elide: Text.ElideRight
                    }
                    Text {
                        visible: ptab.closable && (ptab.active || th.hovered)
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: "✕"
                        color: xh.hovered ? Theme.textBright : Theme.textDim
                        font.pixelSize: 12
                        HoverHandler {
                            id: xh
                        }
                        TapHandler {
                            onTapped: ptab.closed()
                        }
                    }
                    HoverHandler {
                        id: th
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                        onTapped: (pt, button) => button === Qt.MiddleButton ? (ptab.closable && ptab.closed()) : ptab.selected()
                    }
                }

                AppTab {
                    caption: "Tsuba"
                    glyph: "tsuba"
                    active: root.currentTab === 0
                    onSelected: root.currentTab = 0
                }
                AppTab {
                    caption: "Macro"
                    glyph: "macro"
                    active: root.currentKey === "macros"
                    onSelected: root.openMacros()
                }
                AppTab {
                    caption: "Linked games"
                    glyph: "links"
                    active: root.currentKey === "links"
                    onSelected: root.openLinks()
                }
                Repeater {
                    model: tabModel
                    delegate: AppTab {
                        required property string key
                        required property string title
                        required property string source
                        required property int index
                        visible: key !== "macros" && key !== "links"
                        caption: title
                        glyph: source === "DevicePage.qml" ? (({ "keyboard": "keyboard", "mouse": "mouse" })[DeviceKinds.family((root.deviceList.find(d => d.pid === key) || {}).kind)] || "page") : (source === "StudioPage.qml" ? "studio" : (source === "GameLightingPage.qml" ? "gamelight" : "page"))
                        closable: true
                        active: root.currentTab === index + 1
                        onSelected: root.currentTab = index + 1
                        onClosed: root.closeTab(index)
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
            }
        }

        // ---- Row 2: navigation, page title, daemon status
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 36
            color: Theme.bg

            Row {
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                component NavButton: Rectangle {
                    id: nb
                    property string glyph: ""
                    signal clicked()
                    width: 28
                    height: 28
                    radius: 14
                    color: nh.hovered && enabled ? "#2d2d2d" : "transparent"
                    opacity: enabled ? 1 : 0.35
                    Text {
                        anchors.centerIn: parent
                        text: nb.glyph
                        color: Theme.text
                        font.pixelSize: 16
                    }
                    HoverHandler {
                        id: nh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: nb.clicked()
                    }
                }
                // Back goes Home, forward returns to the page you left.
                NavButton {
                    glyph: "←"
                    enabled: root.currentTab !== 0
                    onClicked: {
                        root.returnTab = root.currentTab;
                        root.currentTab = 0;
                    }
                }
                NavButton {
                    glyph: "→"
                    enabled: root.currentTab === 0 && root.returnTab > 0 && root.returnTab <= tabModel.count
                    onClicked: root.currentTab = root.returnTab
                }
                NavButton {
                    glyph: "⟳"
                    onClicked: {
                        backend.refresh_cache("");   // the daemon caches device reads
                        backend.refresh();
                        Theme.dataRev++;
                    }
                }
            }
            Text {
                anchors.centerIn: parent
                text: root.pageTitle.toUpperCase()
                color: "#a0a0a0"
                font.family: Theme.font
                font.pixelSize: Theme.small
                font.letterSpacing: 0.5
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Rectangle {
                    width: 8
                    height: 8
                    radius: 4
                    color: backend.connected ? (backend.readonly ? Theme.warn : Theme.accent) : Theme.danger
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: backend.connected ? (backend.readonly ? "tsubad · read-only" : "tsubad connected") : "tsubad not running"
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                // Appearance (accent colour).
                Rectangle {
                    width: 28
                    height: 28
                    radius: 14
                    anchors.verticalCenter: parent.verticalCenter
                    color: gh.hovered || appearance.opened ? "#2d2d2d" : "transparent"
                    Text {
                        anchors.centerIn: parent
                        text: "⚙"
                        color: Theme.text
                        font.pixelSize: 16
                    }
                    HoverHandler {
                        id: gh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: appearance.opened ? appearance.close() : appearance.open()
                    }
                    QQC2.Popup {
                        id: appearance
                        y: parent.height + 6
                        x: parent.width - width
                        width: 300
                        padding: 16
                        background: Rectangle {
                            color: Theme.card
                            border.color: Theme.borderSoft
                            radius: 5
                        }
                        contentItem: Column {
                            spacing: 12
                            Text {
                                text: "APPEARANCE"
                                color: Theme.accent
                                font.family: Theme.font
                                font.pixelSize: Theme.fontSize
                            }
                            Text {
                                text: "Accent colour"
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            Flow {
                                width: 268
                                spacing: 8
                                Repeater {
                                    // Tsuba jade first; the rest are plain choices (any colour works).
                                    model: ["#1fc9a7", "#44d62c", "#2d9cff", "#ff8a1f", "#ff4fa3", "#b07cff", "#e8e8e8"]
                                    delegate: Rectangle {
                                        required property string modelData
                                        width: 28
                                        height: 28
                                        radius: 14
                                        color: modelData
                                        border.width: Qt.colorEqual(Theme.accent, modelData) ? 2 : 0
                                        border.color: "#ffffff"
                                        TapHandler {
                                            cursorShape: Qt.PointingHandCursor
                                            onTapped: Theme.setAccent(parent.modelData)
                                        }
                                    }
                                }
                                Rectangle {
                                    width: 28
                                    height: 28
                                    radius: 14
                                    color: "transparent"
                                    border.color: Theme.border
                                    Text {
                                        anchors.centerIn: parent
                                        text: "+"
                                        color: Theme.text
                                        font.pixelSize: 16
                                    }
                                    TapHandler {
                                        cursorShape: Qt.PointingHandCursor
                                        onTapped: accentPicker.open()
                                    }
                                }
                            }
                            SynButton {
                                text: "Reset to Tsuba default"
                                onClicked: Theme.setAccent(Theme.defaultAccent)
                            }
                            // Only offered when the user has a personal picture pack (their own pictures count).
                            Column {
                                visible: { Theme.picRev; return root.deviceList.some(d => backend.has_personal_pack(d.pid)); }
                                spacing: 8
                                Text {
                                    text: "Device pictures"
                                    color: Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.small
                                }
                                SynPillTabs {
                                    model: ["Tsuba drawings", "My picture pack"]
                                    currentIndex: Theme.prefs.art === "builtin" ? 0 : 1
                                    onActivated: i => Theme.prefs.art = i === 0 ? "builtin" : "auto"
                                }
                                Text {
                                    width: 268
                                    wrapMode: Text.WordWrap
                                    text: "Takes effect when you reopen a device tab."
                                    color: Theme.textDim
                                    font.family: Theme.font
                                    font.pixelSize: 11
                                }
                            }
                            // Own pictures, per device.
                            Text {
                                text: "My own pictures"
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            Repeater {
                                model: root.deviceList
                                delegate: PictureControls {
                                    required property var modelData
                                    width: 268
                                    backend: root.appBackend
                                    pid: modelData.pid
                                    deviceName: modelData.name.replace(" (wired)", "")
                                }
                            }
                        }
                    }
                    ColorDialog {
                        id: accentPicker
                        title: "Accent colour"
                        selectedColor: Theme.accent
                        onAccepted: Theme.setAccent(selectedColor.toString())
                    }
                }
            }
        }

        // ---- Pages
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.currentTab

            HomePage {
                backend: backend
                devices: root.deviceList
                studioReady: root.studioReady
                onOpenDevice: pid => root.openDevice(pid)
                onOpenStudio: root.openStudio()
                onOpenMacros: root.openMacros()
                onOpenLinks: root.openLinks()
                onOpenGameLighting: root.openGameLighting()
                onOpenSupported: root.openSupported()
            }
            Repeater {
                model: tabModel
                delegate: Loader {
                    required property string source
                    required property string props
                    Component.onCompleted: setSource(source, Object.assign({ backend: backend }, JSON.parse(props)))
                    onLoaded: {
                        if (item && item.openStudio) {
                            item.openStudio.connect(root.openStudio);
                        }
                        if (item && item.openMacros) {
                            item.openMacros.connect(root.openMacros);
                        }
                        if (item && item.openLinks) {
                            item.openLinks.connect(root.openLinks);
                        }
                    }
                }
            }
        }

        // Status bar: latest hardware event (or a restart notice after an update)
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            color: root.updated ? "#33fd8611" : Theme.bar
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                text: root.updated ? "Tsuba was updated: close and reopen it to use the new version." : (root.lastEvent === "" ? "Ready" : root.lastEvent)
                color: root.updated ? Theme.warn : (root.lastEvent === "" ? Theme.textMuted : Theme.text)
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            Text {
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                text: backend.status
                color: Theme.warn
                font.family: Theme.font
                font.pixelSize: Theme.small
                elide: Text.ElideLeft
                width: parent.width * 0.5
                horizontalAlignment: Text.AlignRight
            }
        }
    }

    Component.onCompleted: {
        backend.refresh();
        backend.start_events();
        if (backend.env_var("TSUBA_SUPPORTED") !== "") {   // headless UI check
            root.openSupported();
        } else if (backend.env_var("TSUBA_GAMELIGHTING") === "1") {
            root.openGameLighting();
        } else if (backend.env_var("TSUBA_LINKS") === "1") {
            root.openLinks();
        } else if (backend.env_var("TSUBA_MACROS") === "1") {
            root.openMacros();
        } else if (backend.env_var("TSUBA_STUDIO") === "1") {
            root.openStudio();
        } else if (backend.env_var("TSUBA_DEVICE") !== "") {
            root.openDevice(backend.env_var("TSUBA_DEVICE"));
        }
    }

    // Headless UI check: TSUBA_SCREENSHOT=/path.png renders, saves and quits
    // (TSUBA_GRAB_WINDOW=1: the whole window, including open popups).
    Timer {
        interval: 3000
        running: backend.env_var("TSUBA_SCREENSHOT") !== ""
        onTriggered: {
            const ok = (backend.env_var("TSUBA_ALIGN") !== "" || backend.env_var("TSUBA_GRAB_WINDOW") === "1" ? root.contentItem.Window.contentItem : shell).grabToImage(result => {
                result.saveToFile(backend.env_var("TSUBA_SCREENSHOT"));
                Qt.quit();
            });
            if (!ok) {
                console.warn("screenshot: grabToImage failed");
                Qt.quit();
            }
        }
    }
}
