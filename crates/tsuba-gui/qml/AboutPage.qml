import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// About: version, licences (Tsuba's own and the components it is built from), links, system
// details to paste into a bug report, and the notice that Tsuba is independent of Razer.
Rectangle {
    id: page

    required property var backend
    required property var app
    signal openUrl(string url)

    color: Theme.bg

    property var details: ({})
    property bool showLicence: false
    property bool showThirdParty: false
    property string note: ""

    readonly property string noticeText: i18n("Tsuba is an independent open-source project, not affiliated with or endorsed by Razer Inc. Razer, Synapse, Chroma, HyperShift, BlackWidow and Basilisk are trademarks of Razer Inc., named here only to identify compatible hardware.")

    function detailLines() {
        const d = details;
        const lines = [
            i18n("Tsuba %1", d.app || "?"),
            d.daemon ? (d.daemon_read_only ? i18n("Service: %1 (read-only)", d.daemon) : i18n("Service: %1", d.daemon)) : i18n("Service: not running"),
            i18n("System: %1", d.system || "?"),
            i18n("Kernel: %1", d.kernel || "?"),
            d.session ? i18n("Desktop: %1 / %2", d.desktop || "?", d.session) : i18n("Desktop: %1", d.desktop || "?"),
            i18n("Architecture: %1", d.arch || "?")
        ];
        return lines;
    }
    function copyDetails() {
        backend.copy_text(detailLines().join("\n"));
        note = i18n("Copied.");
    }
    Component.onCompleted: {
        const j = app.system_details();
        details = j === "" ? ({}) : JSON.parse(j);
    }

    QQC2.ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(page.width - 80, 820)
            x: Math.max(40, (page.width - width) / 2)
            spacing: 16

            Item {
                Layout.preferredHeight: 10
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 18
                Rectangle {
                    Layout.preferredWidth: 84
                    Layout.preferredHeight: 84
                    radius: 42
                    color: Theme.card
                    AppGlyph {
                        anchors.centerIn: parent
                        width: 52
                        height: 52
                        kind: "tsuba"
                    }
                }
                ColumnLayout {
                    spacing: 4
                    Text {
                        text: "Tsuba" // i18n-ok: product name
                        color: Theme.textBright
                        font.family: Theme.titleFont
                        font.pixelSize: 30
                    }
                    Text {
                        objectName: "versionText"
                        text: i18n("Version %1", page.app.app_version())
                        color: Theme.accent
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                    Text {
                        text: i18n("Open-source control for Razer keyboards and mice on Linux.")
                        color: Theme.text
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
            }

            SynCard {
                title: i18n("Links")
                RowLayout {
                    spacing: 10
                    SynButton {
                        text: i18n("Project page")
                        onClicked: page.openUrl(page.app.project_url())
                    }
                    SynButton {
                        text: i18n("Documentation")
                        onClicked: page.openUrl(Theme.projectUrl + "/tree/main/docs")
                    }
                    SynButton {
                        text: i18n("Troubleshooting")
                        onClicked: page.openUrl(Theme.docsUrl("TROUBLESHOOTING.md"))
                    }
                    SynButton {
                        text: i18n("Report a problem")
                        onClicked: page.openUrl(page.app.project_url() + "/issues")
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                }
            }

            SynCard {
                title: i18n("Licence")
                description: i18n("Tsuba is free software, released under the GNU General Public License, version 2 or (at your option) any later version. You may use, study, change and share it under those terms. There is no warranty.")
                RowLayout {
                    spacing: 10
                    SynButton {
                        objectName: "licenceToggle"
                        text: page.showLicence ? i18n("Hide licence text") : i18n("Show licence text")
                        onClicked: page.showLicence = !page.showLicence
                    }
                    SynButton {
                        objectName: "thirdPartyToggle"
                        text: page.showThirdParty ? i18n("Hide third-party licences") : i18n("Show third-party licences")
                        onClicked: page.showThirdParty = !page.showThirdParty
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                }
                QQC2.ScrollView {
                    visible: page.showLicence || page.showThirdParty
                    Layout.fillWidth: true
                    Layout.preferredHeight: 280
                    QQC2.TextArea {
                        objectName: "licenceText"
                        readOnly: true
                        wrapMode: Text.Wrap
                        text: page.showThirdParty ? page.app.third_party_licences() : page.app.licence_text()
                        color: Theme.text
                        font.family: "monospace"
                        font.pixelSize: 12
                        background: Rectangle {
                            color: Theme.bg
                            border.color: Theme.borderSoft
                        }
                    }
                }
            }

            SynCard {
                title: i18n("This computer")
                description: i18n("Include these lines when you report a problem.")
                Text {
                    objectName: "detailsText"
                    Layout.fillWidth: true
                    text: page.detailLines().join("\n")
                    color: Theme.text
                    font.family: "monospace"
                    font.pixelSize: 12
                    lineHeight: 1.3
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    spacing: 12
                    SynButton {
                        objectName: "copyDetails"
                        text: i18n("Copy details")
                        onClicked: page.copyDetails()
                    }
                    Text {
                        visible: page.note !== ""
                        text: page.note
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                }
            }

            SynCard {
                title: i18n("Independent project")
                Text {
                    objectName: "noticeText"
                    Layout.fillWidth: true
                    text: page.noticeText
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.Wrap
                }
            }

            Item {
                Layout.preferredHeight: 24
            }
        }
    }
}
