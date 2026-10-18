import QtQuick
import QtQuick.Layouts

// A slim notice strip under the header: lighting-program conflicts, identical devices, a read-only
// daemon, a device that is low on battery or not answering. One strip per message; `dismissible`
// ones carry an "x". Colour follows `severity` ("info", "warn" or "error").
Rectangle {
    id: bar

    property string severity: "warn"
    property string text: ""
    // Optional second line in a smaller, dimmer face.
    property string detail: ""
    property string actionText: ""
    property bool dismissible: false
    signal action()
    signal dismissed()

    readonly property color tone: severity === "error" ? Theme.danger : (severity === "info" ? Theme.accent : Theme.warn)

    Layout.fillWidth: true
    implicitHeight: Math.max(36, row.implicitHeight + 14)
    color: Qt.tint(Theme.bg, Qt.rgba(tone.r, tone.g, tone.b, Theme.dark ? 0.16 : 0.14))

    Rectangle {
        width: 3
        height: parent.height
        color: bar.tone
    }
    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: 18
        anchors.rightMargin: 12
        spacing: 12
        Text {
            text: bar.severity === "info" ? "ⓘ" : "⚠"
            color: bar.tone
            font.pixelSize: 16
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            Text {
                objectName: "bannerText"
                Layout.fillWidth: true
                text: bar.text
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small + 1
                wrapMode: Text.Wrap
            }
            Text {
                visible: bar.detail !== ""
                Layout.fillWidth: true
                text: bar.detail
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: 11
                wrapMode: Text.Wrap
            }
        }
        SynButton {
            objectName: "bannerAction"
            visible: bar.actionText !== ""
            text: bar.actionText
            implicitHeight: 26
            onClicked: bar.action()
        }
        Text {
            objectName: "bannerClose"
            visible: bar.dismissible
            text: "✕"
            color: closeHover.hovered ? Theme.textBright : Theme.textDim
            font.pixelSize: 13
            HoverHandler {
                id: closeHover
                cursorShape: Qt.PointingHandCursor
            }
            TapHandler {
                onTapped: bar.dismissed()
            }
            Accessible.role: Accessible.Button
            Accessible.name: i18n("Dismiss")
        }
    }
}
