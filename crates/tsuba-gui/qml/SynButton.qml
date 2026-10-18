import QtQuick
import QtQuick.Controls as QQC2

// Synapse-style button: outlined by default, solid green when `primary`.
QQC2.AbstractButton {
    id: control

    property bool primary: false

    implicitHeight: 32
    implicitWidth: Math.max(96, label.implicitWidth + 32)
    hoverEnabled: true

    background: Rectangle {
        color: control.primary ? (control.hovered ? Theme.accentHover : Theme.accent) : (control.hovered ? "#1affffff" : "transparent")
        border.color: control.primary ? "transparent" : (control.hovered ? Theme.accent : Theme.border)
        border.width: 1
        opacity: control.enabled ? 1 : 0.4
    }
    contentItem: Text {
        id: label
        text: control.text.toUpperCase()
        font.family: Theme.font
        font.pixelSize: Theme.small
        font.letterSpacing: 1
        font.bold: true
        color: control.primary ? Theme.bg : Theme.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        opacity: control.enabled ? 1 : 0.5
    }
}
