import QtQuick
import QtQuick.Controls as QQC2

// Synapse-style button: outlined by default, solid green when `primary`.
QQC2.AbstractButton {
    id: control

    property bool primary: false

    implicitHeight: 32
    // Grows with the label (translations differ in length); `minWidth` is the least it may shrink to.
    property int minWidth: 96
    implicitWidth: Math.max(minWidth, label.implicitWidth + 32)
    hoverEnabled: true

    background: Rectangle {
        color: control.primary ? (control.hovered ? Theme.accentHover : Theme.accent) : (control.hovered ? Theme.hoverTint : "transparent")
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
        color: control.primary ? Theme.accentText : Theme.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        opacity: control.enabled ? 1 : 0.5
    }
}
