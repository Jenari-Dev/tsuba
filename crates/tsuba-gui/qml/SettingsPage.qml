import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs
import QtCore

// Settings: start-up behaviour, appearance, language, notifications and support tools.
// Appearance and start-up choices live in ~/.config/tsuba/app.ini (through Theme); notification
// choices live in the daemon (it sends the notifications, so they work with this window closed).
Rectangle {
    id: page

    required property var backend
    // AppBackend (autostart, notification settings, diagnostics…).
    required property var app
    property var devices: []
    signal openAbout()
    signal showWelcome()
    signal openUrl(string url)

    color: Theme.bg

    property bool autostart: false
    property bool serviceEnabled: false
    property var notif: ({})
    readonly property bool daemonUp: backend.connected
    readonly property string launchLanguage: Theme.app.language
    property bool languageChanged: false
    property string diagNote: ""
    property bool diagOk: true
    property string testNote: ""

    function reload() {
        autostart = app.autostart_enabled();
        serviceEnabled = app.service_enabled();
        const j = daemonUp ? app.notification_settings() : "";
        notif = j === "" ? ({}) : JSON.parse(j);
    }
    function setNotif(key, value) {
        const next = Object.assign({}, notif);
        next[key] = value;
        const r = app.set_notification_settings(JSON.stringify(next));
        notif = r === "" ? notif : JSON.parse(r);
    }
    Component.onCompleted: reload()
    onDaemonUpChanged: reload()

    Connections {
        target: page.app
        function onDiagnostics_exported(ok, message) {
            page.diagOk = ok;
            page.diagNote = ok ? i18n("Saved to %1", message) : i18n("Could not save: %1", message);
        }
    }

    component SettingRow: RowLayout {
        id: row
        property string label: ""
        property string hint: ""
        default property alias content: slot.data
        Layout.fillWidth: true
        spacing: 14
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text {
                Layout.fillWidth: true
                text: row.label
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                wrapMode: Text.Wrap
            }
            Text {
                visible: row.hint !== ""
                Layout.fillWidth: true
                text: row.hint
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
        }
        RowLayout {
            id: slot
        }
    }

    QQC2.ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(page.width - 80, 1000)
            x: Math.max(40, (page.width - width) / 2)
            spacing: 16

            Item {
                Layout.preferredHeight: 10
            }

            GridLayout {
                Layout.fillWidth: true
                columns: width > 820 ? 2 : 1
                columnSpacing: 16
                rowSpacing: 16

                // ---- General
                SynCard {
                    objectName: "generalCard"
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: 1
                    title: i18n("General")
                    SettingRow {
                        label: i18n("Launch Tsuba at login")
                        hint: i18n("Starts minimised in the system tray, so notifications and the tray menu are ready.")
                        SynToggle {
                            objectName: "autostartToggle"
                            checked: page.autostart
                            onToggled: on => {
                                page.app.set_autostart(on);
                                page.autostart = page.app.autostart_enabled();
                            }
                        }
                    }
                    SettingRow {
                        label: i18n("Start the background service at login")
                        hint: i18n("The Tsuba service (tsubad) applies your settings to your devices. Keep this on.")
                        SynToggle {
                            objectName: "serviceToggle"
                            checked: page.serviceEnabled
                            onToggled: on => {
                                page.app.set_service_enabled(on);
                                page.serviceEnabled = page.app.service_enabled();
                            }
                        }
                    }
                    SettingRow {
                        label: i18n("When Tsuba opens, show")
                        SynPillTabs {
                            objectName: "startPage"
                            model: [i18n("Home"), i18n("Last page")]
                            currentIndex: Theme.app.startPage === "last" ? 1 : 0
                            onActivated: i => Theme.app.startPage = i === 1 ? "last" : "home"
                        }
                    }
                    SettingRow {
                        label: i18n("Closing the window keeps Tsuba in the tray")
                        hint: page.app.tray_available ? i18n("Quit from the tray icon's menu.") : i18n("No system tray found on this desktop, so closing the window quits Tsuba.")
                        SynToggle {
                            objectName: "trayToggle"
                            enabled: page.app.tray_available
                            checked: Theme.app.closeToTray && page.app.tray_available
                            onToggled: on => Theme.app.closeToTray = on
                        }
                    }
                    SettingRow {
                        label: i18n("Show tips on the Home page")
                        SynToggle {
                            checked: Theme.app.showTips
                            onToggled: on => Theme.app.showTips = on
                        }
                    }
                    SynButton {
                        text: i18n("Show the welcome again")
                        onClicked: page.showWelcome()
                    }
                }

                // ---- Appearance
                SynCard {
                    objectName: "appearanceCard"
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: 1
                    title: i18n("Appearance")
                    SettingRow {
                        label: i18n("Theme")
                        hint: i18n("Automatic follows your desktop's light or dark setting.")
                        SynPillTabs {
                            objectName: "themeTabs"
                            model: [i18n("Automatic"), i18n("Light"), i18n("Dark")]
                            currentIndex: Theme.prefs.themeMode === "light" ? 1 : (Theme.prefs.themeMode === "dark" ? 2 : 0)
                            onActivated: i => Theme.prefs.themeMode = ["auto", "light", "dark"][i]
                        }
                    }
                    Text {
                        text: i18n("Accent colour")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    Flow {
                        Layout.fillWidth: true
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
                                border.width: Qt.colorEqual(Theme.userAccent, modelData) ? 2 : 1
                                border.color: Qt.colorEqual(Theme.userAccent, modelData) ? Theme.textBright : Theme.borderSoft
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
                        text: i18n("Reset to Tsuba default")
                        onClicked: Theme.setAccent(Theme.defaultAccent)
                    }
                    // Only offered when the user has a personal picture pack (their own pictures count).
                    ColumnLayout {
                        visible: { Theme.picRev; return page.devices.some(d => page.backend.has_personal_pack(d.pid)); }
                        Layout.fillWidth: true
                        spacing: 8
                        Text {
                            text: i18n("Device pictures")
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }
                        SynPillTabs {
                            model: [i18n("Tsuba drawings"), i18n("My picture pack")]
                            currentIndex: Theme.prefs.art === "builtin" ? 0 : 1
                            onActivated: i => Theme.prefs.art = i === 0 ? "builtin" : "auto"
                        }
                        Text {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: i18n("Takes effect when you reopen a device tab.")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    Text {
                        visible: page.devices.length > 0
                        text: i18n("My own pictures")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    Repeater {
                        model: page.devices
                        delegate: PictureControls {
                            required property var modelData
                            Layout.fillWidth: true
                            backend: page.backend
                            pid: modelData.pid
                            deviceName: modelData.name.replace(" (wired)", "")
                        }
                    }
                    ColorDialog {
                        id: accentPicker
                        title: i18n("Accent colour")
                        selectedColor: Theme.userAccent
                        onAccepted: Theme.setAccent(selectedColor.toString())
                    }
                }

                // ---- Language
                SynCard {
                    id: langCard
                    objectName: "languageCard"
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: 1
                    title: i18n("Language")
                    readonly property var langs: [
                        { code: "system", text: i18n("System default") },
                        { code: "en", text: "English" },
                        { code: "ja", text: "日本語" }
                    ]
                    SettingRow {
                        label: i18n("Language of the app")
                        hint: i18n("Only the app's own text changes. Messages from the Tsuba service and the command-line tool stay in English.")
                        SynDropdown {
                            objectName: "languageBox"
                            implicitWidth: 180
                            model: langCard.langs
                            textRole: "text"
                            currentIndex: Math.max(0, langCard.langs.findIndex(l => l.code === Theme.app.language))
                            onActivated: i => {
                                Theme.app.language = langCard.langs[i].code;
                                page.languageChanged = Theme.app.language !== page.launchLanguage;
                            }
                        }
                    }
                    RowLayout {
                        visible: page.languageChanged
                        Layout.fillWidth: true
                        spacing: 12
                        Text {
                            Layout.fillWidth: true
                            text: i18n("Restart Tsuba to use the new language.")
                            color: Theme.warn
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        SynButton {
                            objectName: "restartButton"
                            primary: true
                            text: i18n("Restart now")
                            onClicked: {
                                if (page.app.restart_app()) {
                                    Qt.quit();
                                }
                            }
                        }
                    }
                }

                // ---- Notifications
                SynCard {
                    objectName: "notificationsCard"
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: 1
                    title: i18n("Notifications")
                    description: page.daemonUp ? i18n("Short pop-ups when something changes on a device. They come from the Tsuba service, so they also appear when this window is closed.") : i18n("Start the Tsuba service to change these.")
                    headerContent: SynToggle {
                        objectName: "notifyMaster"
                        enabled: page.daemonUp
                        checked: page.notif.enabled !== false
                        onToggled: on => page.setNotif("enabled", on)
                    }
                    ColumnLayout {
                        enabled: page.daemonUp && page.notif.enabled !== false
                        Layout.fillWidth: true
                        spacing: 10
                        SynCheckBox {
                            objectName: "notifyDpi"
                            text: i18n("Sensitivity (DPI) stage changes")
                            checked: page.notif.dpi !== false
                            onToggled: page.setNotif("dpi", checked)
                        }
                        SynCheckBox {
                            text: i18n("Profile changes from the device's profile button")
                            checked: page.notif.profile !== false
                            onToggled: page.setNotif("profile", checked)
                        }
                        SynCheckBox {
                            text: i18n("Dial changes")
                            checked: page.notif.dial !== false
                            onToggled: page.setNotif("dial", checked)
                        }
                        SynCheckBox {
                            text: i18n("Scroll wheel mode (free-spin or tactile)")
                            checked: page.notif.wheel !== false
                            onToggled: page.setNotif("wheel", checked)
                        }
                        SynCheckBox {
                            text: i18n("A device connects or disconnects")
                            checked: page.notif.connection !== false
                            onToggled: page.setNotif("connection", checked)
                        }
                        SynCheckBox {
                            objectName: "notifyBattery"
                            text: i18n("Low battery")
                            checked: page.notif.battery_low !== false
                            onToggled: page.setNotif("battery_low", checked)
                        }
                        RowLayout {
                            visible: page.notif.battery_low !== false
                            Layout.fillWidth: true
                            Layout.leftMargin: 28
                            spacing: 12
                            Text {
                                text: i18n("Warn at")
                                color: Theme.text
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                            SynSlider {
                                objectName: "batteryThreshold"
                                Layout.fillWidth: true
                                from: 5
                                to: 50
                                stepSize: 5
                                badge: true
                                badgeText: Math.round(value) + "%"
                                value: page.notif.battery_threshold || 15
                                onMoved: page.setNotif("battery_threshold", Math.round(value))
                            }
                        }
                        SynCheckBox {
                            text: i18n("Battery full")
                            checked: page.notif.battery_full === true
                            onToggled: page.setNotif("battery_full", checked)
                        }
                        SynCheckBox {
                            text: i18n("Play the desktop's notification sound")
                            checked: page.notif.sound === true
                            onToggled: page.setNotif("sound", checked)
                        }
                    }
                    RowLayout {
                        spacing: 12
                        SynButton {
                            objectName: "testNotification"
                            enabled: page.daemonUp
                            text: i18n("Send a test notification")
                            onClicked: page.testNote = page.app.send_test_notification() ? "" : i18n("The service didn't answer.")
                        }
                        Text {
                            visible: page.testNote !== ""
                            text: page.testNote
                            color: Theme.warn
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                }

                // ---- Support
                SynCard {
                    objectName: "supportCard"
                    Layout.alignment: Qt.AlignTop
                    Layout.preferredWidth: 1
                    title: i18n("Help and diagnostics")
                    description: i18n("If something doesn't work, export diagnostics and attach the file to an issue. It lists the Razer devices the system sees, your Linux and desktop versions, the service's recent log and the shape of your settings. It contains no key assignments, macro text, serial numbers, user name or file paths.")
                    RowLayout {
                        spacing: 12
                        SynButton {
                            objectName: "exportDiagnostics"
                            text: i18n("Export diagnostics…")
                            onClicked: {
                                diagDialog.currentFile = StandardPaths.writableLocation(StandardPaths.DownloadLocation) + "/" + page.app.diagnostics_file_name();
                                diagDialog.open();
                            }
                        }
                        SynButton {
                            text: i18n("Troubleshooting")
                            onClicked: page.openUrl(Theme.docsUrl("TROUBLESHOOTING.md"))
                        }
                        SynButton {
                            text: i18n("About Tsuba")
                            onClicked: page.openAbout()
                        }
                    }
                    Text {
                        visible: page.diagNote !== ""
                        Layout.fillWidth: true
                        text: page.diagNote
                        color: page.diagOk ? Theme.accent : Theme.danger
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                        wrapMode: Text.WrapAnywhere
                    }
                    SettingRow {
                        label: i18n("Report a problem")
                        hint: i18n("Opens the issue page in your browser.")
                        SynButton {
                            text: i18n("Open")
                            onClicked: page.openUrl(Theme.projectUrl + "/issues")
                        }
                    }
                }
            }

            Item {
                Layout.preferredHeight: 24
            }
        }
    }

    FileDialog {
        id: diagDialog
        fileMode: FileDialog.SaveFile
        title: i18n("Save diagnostics")
        nameFilters: [i18n("Zip archive (*.zip)")]
        defaultSuffix: "zip"
        onAccepted: {
            page.diagNote = i18n("Writing…");
            page.diagOk = true;
            page.app.export_diagnostics(selectedFile.toString());
        }
    }
}
