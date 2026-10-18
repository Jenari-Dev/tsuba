import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import io.github.jenaridev.tsuba

QQC2.ApplicationWindow {
    id: root

    title: "Tsuba"
    // Shown by Component.onCompleted (stays hidden when started with --minimized from autostart).
    visible: false
    width: 1440
    height: 900
    minimumWidth: 1100
    minimumHeight: 700
    color: Theme.bg
    // Stock controls (menus, scroll bars, text fields) follow Tsuba's light or dark palette.
    palette.window: Theme.bg
    palette.windowText: Theme.text
    palette.base: Theme.card
    palette.alternateBase: Theme.panelRaised
    palette.text: Theme.text
    palette.button: Theme.panelRaised
    palette.buttonText: Theme.text
    palette.placeholderText: Theme.textMuted
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.accentText
    palette.accent: Theme.accent
    palette.toolTipBase: Theme.panelRaised
    palette.toolTipText: Theme.text
    palette.light: Theme.borderSoft
    palette.mid: Theme.border
    palette.dark: Theme.line

    property var deviceList: []
    property string currentPid: ""
    property string lastEvent: ""
    property bool updated: false
    // Parsed `tsubad --check-env` (why no device shows up, conflicting programs, identical devices).
    property var envReport: ({})
    // Started by autostart: stay in the tray until asked.
    readonly property bool startHidden: app.start_minimized && backend.env_var("TSUBA_SCREENSHOT") === ""
    // The most recent answer of the device on the current tab (battery, responsiveness).
    property var devPower: ({})
    property int devPowerFailures: 0

    Timer {
        interval: 10000
        repeat: true
        running: !root.updated
        onTriggered: root.updated = backend.binary_replaced()
    }

    function describeEvent(pid, e) {
        const who = (root.deviceList.find(d => d.pid === pid) || { name: pid }).name;
        const media = { 0xe2: i18n("mute"), 0xb6: i18n("previous"), 0xb5: i18n("next"), 0xcd: i18n("play / pause"), 0xe9: i18n("volume up"), 0xea: i18n("volume down") };
        switch (e.kind) {
        case "media":
            return e.usages.length ? `${who}: ${e.usages.map(u => media[u] || "0x" + u.toString(16)).join(", ")}` : "";
        case "razer_keys":
            return e.keys.length ? `${who}: ${e.keys.map(k => k.name).join(", ")}` : "";
        case "notification":
            if (e.id === 2) {
                return i18n("%1: sensitivity %2 DPI", who, (e.data[0] << 8) | e.data[1]);
            }
            if (e.id === 57) {
                return e.data[0] ? i18n("%1: scroll wheel free-spin", who) : i18n("%1: scroll wheel tactile", who);
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
            return i18n("Tsuba");
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
            root.openTab("studio", i18n("Lighting Studio"), "StudioPage.qml", { pid: kb.pid });
        }
    }

    function openLinks() {
        root.openTab("links", i18n("Linked games"), "LinkedGamesPage.qml", {});
    }

    function openGameLighting() {
        root.openTab("gamelight", i18n("Game lighting"), "GameLightingPage.qml", {});
    }

    function openSupported() {
        root.openTab("supported", i18n("Supported devices"), "SupportedDevicesPage.qml", {});
    }

    function openMacros() {
        root.openTab("macros", i18n("Macros"), "MacrosPage.qml", {});
    }

    function openSettings() {
        root.openTab("settings", i18n("Settings"), "SettingsPage.qml", {});
    }

    function openAbout() {
        root.openTab("about", i18n("About"), "AboutPage.qml", {});
    }

    function openUrl(url) {
        Qt.openUrlExternally(url);
    }

    // Bring the window back from the tray (or from behind other windows).
    function showWindow() {
        if (root.visibility === Window.Minimized) {
            root.showNormal();
        }
        root.show();
        root.raise();
        root.requestActivate();
    }

    // Open the page named by a saved "last page" key.
    function openByKey(key) {
        switch (key) {
        case "":
            return;
        case "settings":
            return root.openSettings();
        case "about":
            return root.openAbout();
        case "macros":
            return root.openMacros();
        case "links":
            return root.openLinks();
        case "gamelight":
            return root.openGameLighting();
        case "supported":
            return root.openSupported();
        case "studio":
            return root.openStudio();
        default:
            if (root.deviceList.some(d => d.pid === key)) {
                root.openDevice(key);
            }
        }
    }

    function dismissed() {
        try {
            return JSON.parse(Theme.app.dismissedConflicts);
        } catch (e) {
            return [];
        }
    }
    function dismiss(id) {
        Theme.app.dismissedConflicts = JSON.stringify(root.dismissed().concat([id]));
    }
    // Notices about other lighting programs and identical devices, minus the ones dismissed.
    readonly property var notices: {
        const gone = root.dismissed();
        const out = [];
        for (const c of (root.envReport.conflicts || [])) {
            if (gone.indexOf(c.id) >= 0) {
                continue;
            }
            if (c.kind === "driver") {
                out.push({ id: c.id, text: i18n("OpenRazer's kernel driver is loaded. It can take devices away from Tsuba or fight over lighting."), detail: i18n("Run tsuba-handover in a terminal to switch it off. Undo with tsuba-handover-restore.") });
            } else {
                out.push({ id: c.id, text: i18n("%1 is running. It and Tsuba both control your lighting, so colours may flicker or reset.", c.name), detail: i18n("Close one of them, or only use one at a time.") });
            }
        }
        for (const pid of (root.envReport.duplicates || [])) {
            const u = (root.envReport.usb || []).find(x => x.pid === pid);
            if (gone.indexOf("dup:" + pid) < 0) {
                out.push({ id: "dup:" + pid, text: i18n("Two %1 are connected. Tsuba controls one of each model; the second is left alone.", u && u.name ? u.name : pid), detail: "" });
            }
        }
        return out;
    }

    function refreshDevicePower() {
        const d = root.deviceList.find(x => x.pid === root.currentKey);
        if (!d || d.kind !== "mouse") {
            root.devPower = ({});
            root.devPowerFailures = 0;
            return;
        }
        const j = backend.power(d.pid);
        if (j === "") {
            root.devPowerFailures++;
        } else {
            root.devPowerFailures = 0;
            root.devPower = JSON.parse(j);
        }
    }
    onCurrentKeyChanged: {
        Theme.app.lastPage = root.currentKey;
        root.devPower = ({});
        root.devPowerFailures = 0;
        root.refreshDevicePower();
    }

    // ---- tray ---------------------------------------------------------------------------
    function updateTray() {
        const devs = root.deviceList.map(d => {
            const s = backend.library_profiles(d.pid);
            const p = s === "" ? ({}) : JSON.parse(s);
            return { pid: d.pid, name: d.name.replace(" (wired)", ""), profiles: (p.profiles || []).map(x => ({ id: x.id, name: x.name, active: x.id === p.active })) };
        });
        app.tray_update(JSON.stringify(devs), JSON.stringify({ open: i18n("Open Tsuba"), settings: i18n("Settings"), quit: i18n("Quit"), profiles: i18n("Profiles") }), i18n("Tsuba"));
    }
    onClosing: close => {
        // With a tray icon the window just hides; the tray menu quits.
        if (Theme.app.closeToTray && app.tray_available) {
            close.accepted = false;
            root.hide();
        }
    }

    AppBackend {
        id: app
        onEnvironment_checked: report => {
            try {
                root.envReport = JSON.parse(report);
            } catch (e) {
                root.envReport = ({});
            }
        }
        onService_started: (ok, message) => {
            backend.refresh();
            app.check_environment();
        }
        onTray_action: (kind, a, b) => {
            if (kind === "activate") {
                root.showWindow();
            } else if (kind === "quit") {
                Qt.quit();
            } else if (kind === "profile") {
                backend.activate_library_profile(a, b);
            } else if (kind === "page") {
                root.showWindow();
                if (a === "settings") {
                    root.openSettings();
                }
            }
        }
    }

    Timer {
        // Looks at the system again (quickly while no device shows, slowly otherwise).
        interval: root.deviceList.length === 0 ? 8000 : 60000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: {
            if (root.visible || root.deviceList.length === 0) {
                app.check_environment();
            }
            root.refreshDevicePower();
        }
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
        Qt.callLater(root.updateTray);
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
                    color: active ? Theme.panelRaised : (th.hovered ? Theme.hoverFill : "transparent")
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
                    caption: i18n("Tsuba")
                    glyph: "tsuba"
                    active: root.currentTab === 0
                    onSelected: root.currentTab = 0
                }
                AppTab {
                    caption: i18n("Macro")
                    glyph: "macro"
                    active: root.currentKey === "macros"
                    onSelected: root.openMacros()
                }
                AppTab {
                    caption: i18n("Linked games")
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
                    color: nh.hovered && enabled ? Theme.hoverFill : "transparent"
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
                color: Theme.navText
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
                    text: backend.connected ? (backend.readonly ? i18n("tsubad · read-only") : i18n("tsubad connected")) : i18n("tsubad not running")
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                // Help menu.
                Rectangle {
                    objectName: "helpMenuButton"
                    width: 28
                    height: 28
                    radius: 14
                    anchors.verticalCenter: parent.verticalCenter
                    color: hh.hovered || helpMenu.opened ? Theme.hoverFill : "transparent"
                    Text {
                        anchors.centerIn: parent
                        text: "?"
                        color: Theme.text
                        font.pixelSize: 15
                        font.bold: true
                    }
                    HoverHandler {
                        id: hh
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: helpMenu.opened ? helpMenu.close() : helpMenu.open()
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: i18n("Help")
                    QQC2.Popup {
                        id: helpMenu
                        objectName: "helpMenu"
                        y: parent.height + 6
                        x: parent.width - width
                        width: 240
                        padding: 6
                        background: Rectangle {
                            color: Theme.card
                            border.color: Theme.borderSoft
                            radius: 5
                        }
                        contentItem: Column {
                            spacing: 0
                            Repeater {
                                model: [
                                    { text: i18n("Troubleshooting"), act: "docs" },
                                    { text: i18n("Supported devices"), act: "supported" },
                                    { text: i18n("Show the welcome"), act: "welcome" },
                                    { text: i18n("Report a problem"), act: "issues" },
                                    { text: i18n("About Tsuba"), act: "about" }
                                ]
                                delegate: Rectangle {
                                    required property var modelData
                                    width: helpMenu.availableWidth
                                    height: 32
                                    radius: 3
                                    color: rowHover.hovered ? Theme.hoverFill : "transparent"
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        x: 10
                                        text: parent.modelData.text
                                        color: Theme.text
                                        font.family: Theme.font
                                        font.pixelSize: Theme.small + 1
                                    }
                                    HoverHandler {
                                        id: rowHover
                                        cursorShape: Qt.PointingHandCursor
                                    }
                                    TapHandler {
                                        onTapped: {
                                            helpMenu.close();
                                            switch (parent.modelData.act) {
                                            case "docs":
                                                root.openUrl(Theme.docsUrl("TROUBLESHOOTING.md"));
                                                break;
                                            case "supported":
                                                root.openSupported();
                                                break;
                                            case "welcome":
                                                welcome.open();
                                                break;
                                            case "issues":
                                                root.openUrl(Theme.projectUrl + "/issues");
                                                break;
                                            case "about":
                                                root.openAbout();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Settings.
                Rectangle {
                    objectName: "settingsButton"
                    width: 28
                    height: 28
                    radius: 14
                    anchors.verticalCenter: parent.verticalCenter
                    color: gh.hovered || root.currentKey === "settings" ? Theme.hoverFill : "transparent"
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
                        onTapped: root.openSettings()
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: i18n("Settings")
                }
            }
        }

        // ---- Notices: lighting-program conflicts, identical devices, read-only daemon, device state
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1
            Repeater {
                model: root.notices
                delegate: DeviceStateBanner {
                    required property var modelData
                    severity: "warn"
                    text: modelData.text
                    detail: modelData.detail
                    dismissible: true
                    onDismissed: root.dismiss(modelData.id)
                }
            }
            DeviceStateBanner {
                objectName: "readonlyBanner"
                visible: backend.connected && backend.readonly
                severity: "info"
                text: i18n("The Tsuba service is running read-only: nothing you change is sent to your devices.")
            }
            DeviceStateBanner {
                objectName: "batteryBanner"
                readonly property int pct: root.devPower.battery_percent !== undefined ? root.devPower.battery_percent : 100
                visible: root.devPower.battery_percent !== undefined && !root.devPower.charging && pct <= 15
                severity: "warn"
                text: i18n("Battery at %1%. Plug in the cable to charge.", pct)
            }
            DeviceStateBanner {
                objectName: "silentBanner"
                visible: root.devPowerFailures >= 2
                severity: "warn"
                text: i18n("This device isn't answering. A wireless device may be asleep: move it or press a button.")
                actionText: i18n("Try again")
                onAction: {
                    root.devPowerFailures = 0;
                    backend.refresh_cache(root.currentKey);
                    root.refreshDevicePower();
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
                app: app
                envReport: root.envReport
                devices: root.deviceList
                studioReady: root.studioReady
                onRecheck: app.check_environment()
                onOpenUrl: url => root.openUrl(url)
                onExportDiagnostics: root.openSettings()
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
                    Component.onCompleted: setSource(source, Object.assign({ backend: backend }, JSON.parse(props),
                        source === "SettingsPage.qml" || source === "AboutPage.qml" ? { app: app } : {},
                        source === "SettingsPage.qml" ? { devices: Qt.binding(() => root.deviceList) } : {}))
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
                        if (item && item.openAbout) {
                            item.openAbout.connect(root.openAbout);
                        }
                        if (item && item.showWelcome) {
                            item.showWelcome.connect(() => welcome.open());
                        }
                        if (item && item.openUrl) {
                            item.openUrl.connect(root.openUrl);
                        }
                    }
                }
            }
        }

        // Status bar: latest hardware event (or a restart notice after an update)
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            color: root.updated ? Qt.tint(Theme.bar, Qt.rgba(Theme.warn.r, Theme.warn.g, Theme.warn.b, 0.22)) : Theme.bar
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                text: root.updated ? i18n("Tsuba was updated: close and reopen it to use the new version.") : (root.lastEvent === "" ? i18n("Ready") : root.lastEvent)
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

    FirstRunPanel {
        id: welcome
        app: app
        backend: backend
        devices: root.deviceList
        report: root.envReport
        onRecheck: app.check_environment()
    }

    Component.onCompleted: {
        backend.refresh();
        backend.start_events();
        app.tray_start();
        const shot = backend.env_var("TSUBA_SCREENSHOT") !== "";
        if (!root.startHidden) {
            root.show();
        }
        if (backend.env_var("TSUBA_SUPPORTED") !== "") {   // headless UI check
            root.openSupported();
        } else if (backend.env_var("TSUBA_SETTINGS") === "1") {
            root.openSettings();
        } else if (backend.env_var("TSUBA_ABOUT") === "1") {
            root.openAbout();
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
        } else if (Theme.app.startPage === "last" && !shot) {
            root.openByKey(Theme.app.lastPage);
        }
        // First launch: the welcome (never in the headless checks unless asked for).
        if ((!Theme.app.onboarded && !shot && !root.startHidden) || backend.env_var("TSUBA_WELCOME") === "1") {
            welcome.open();
        }
    }

    // Started to the tray but the desktop has none: show the window rather than leave nothing visible.
    Timer {
        interval: 6000
        running: root.startHidden
        onTriggered: {
            if (!app.tray_available && !root.visible) {
                root.showWindow();
            }
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
