import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// The welcome shown once on first launch (and again from Settings): what Tsuba is, whether it
// found your devices, and the few choices worth making up front (look, language, start at login).
QQC2.Popup {
    id: panel

    // AppBackend and the device backend.
    property var app: null
    property var backend: null
    property var devices: []
    // Parsed environment report, for the "your devices" step.
    property var report: ({})
    property int step: 0
    readonly property int steps: 3
    signal finished()
    signal recheck()

    parent: QQC2.Overlay.overlay
    anchors.centerIn: parent
    modal: true
    closePolicy: QQC2.Popup.NoAutoClose
    width: Math.min(620, (parent ? parent.width : 700) - 40)
    padding: 28

    onAboutToShow: step = 0

    function finish() {
        Theme.app.onboarded = true;
        close();
        finished();
    }

    background: Rectangle {
        color: Theme.card
        radius: 8
        border.color: Theme.borderSoft
    }
    QQC2.Overlay.modal: Rectangle {
        color: Theme.dark ? "#aa000000" : "#66000000"
    }

    contentItem: ColumnLayout {
        spacing: 16

        RowLayout {
            spacing: 14
            AppGlyph {
                Layout.preferredWidth: 40
                Layout.preferredHeight: 40
                kind: "tsuba"
            }
            Text {
                Layout.fillWidth: true
                text: panel.step === 0 ? i18n("Welcome to Tsuba") : (panel.step === 1 ? i18n("Your devices") : i18n("Make it yours"))
                color: Theme.textBright
                font.family: Theme.titleFont
                font.pixelSize: 24
            }
            Text {
                text: (panel.step + 1) + " / " + panel.steps
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }

        StackLayout {
            Layout.fillWidth: true
            currentIndex: panel.step

            // 1: what this is
            ColumnLayout {
                spacing: 12
                Text {
                    Layout.fillWidth: true
                    text: i18n("Tsuba lets you remap keys and buttons, record macros, set DPI and polling rate, and design lighting for your Razer keyboard and mouse, without Razer's Windows software.")
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    text: i18n("Your settings are stored on this computer, and sent to your devices when they are plugged in. Tsuba never reads what you type and doesn't use the internet.")
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    objectName: "welcomeNotice"
                    text: i18n("Tsuba is an independent project, not affiliated with or endorsed by Razer Inc.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
            }

            // 2: devices
            ColumnLayout {
                spacing: 10
                Text {
                    objectName: "devicesStatus"
                    Layout.fillWidth: true
                    text: panel.devices.length > 0 ? i18n("Found %1 device(s):", panel.devices.length) : i18n("No device found yet.")
                    color: panel.devices.length > 0 ? Theme.accent : Theme.warn
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: panel.devices
                    delegate: Text {
                        required property var modelData
                        Layout.fillWidth: true
                        text: "• " + modelData.name.replace(" (wired)", "")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                }
                Text {
                    visible: panel.devices.length === 0
                    Layout.fillWidth: true
                    text: i18n("Plug in a Razer keyboard or mouse (or the wireless dongle). If it still doesn't show up, the Home page explains what to do. You can finish this welcome now and come back to it.")
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
                SynButton {
                    visible: panel.devices.length === 0
                    text: i18n("Check again")
                    onClicked: panel.recheck()
                }
            }

            // 3: choices
            ColumnLayout {
                spacing: 14
                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Theme")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    SynPillTabs {
                        objectName: "welcomeTheme"
                        model: [i18n("Automatic"), i18n("Light"), i18n("Dark")]
                        currentIndex: Theme.prefs.themeMode === "light" ? 1 : (Theme.prefs.themeMode === "dark" ? 2 : 0)
                        onActivated: i => Theme.prefs.themeMode = ["auto", "light", "dark"][i]
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Language")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    SynPillTabs {
                        objectName: "welcomeLanguage"
                        model: [i18n("System"), "English", "日本語"] // i18n-ok: language names are shown in their own language
                        currentIndex: Theme.app.language === "en" ? 1 : (Theme.app.language === "ja" ? 2 : 0)
                        onActivated: i => Theme.app.language = ["system", "en", "ja"][i]
                    }
                }
                Text {
                    Layout.fillWidth: true
                    text: i18n("A language change applies the next time Tsuba starts.")
                    color: Theme.textDim
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Launch Tsuba at login (in the tray)")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    SynToggle {
                        objectName: "welcomeAutostart"
                        checked: panel.app ? panel.app.autostart_enabled() : false
                        onToggled: on => {
                            if (panel.app) {
                                panel.app.set_autostart(on);
                                checked = panel.app.autostart_enabled();
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            SynButton {
                objectName: "welcomeSkip"
                text: i18n("Skip")
                visible: panel.step < panel.steps - 1
                onClicked: panel.finish()
            }
            Item {
                Layout.fillWidth: true
            }
            SynButton {
                objectName: "welcomeBack"
                text: i18n("Back")
                visible: panel.step > 0
                onClicked: panel.step--
            }
            SynButton {
                objectName: "welcomeNext"
                primary: true
                text: panel.step < panel.steps - 1 ? i18n("Next") : i18n("Get started")
                onClicked: panel.step < panel.steps - 1 ? panel.step++ : panel.finish()
            }
        }
    }
}
