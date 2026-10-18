import QtQuick
import QtQuick.Controls as QQC2

// Synapse-style checkbox: square box that fills green with a dark tick.
QQC2.CheckBox {
    id: control
    spacing: 10

    indicator: Rectangle {
        implicitWidth: 18
        implicitHeight: 18
        x: control.leftPadding
        y: control.topPadding + (control.availableHeight - height) / 2
        radius: 2
        color: control.checked ? Theme.accent : "transparent"
        border.color: control.checked ? Theme.accent : (control.hovered ? Theme.text : Theme.textMuted)
        border.width: 1
        opacity: control.enabled ? 1 : 0.4
        Text {
            anchors.centerIn: parent
            visible: control.checked
            text: "✓"
            color: "#101010"
            font.pixelSize: 13
            font.bold: true
        }
    }
    contentItem: Text {
        leftPadding: control.indicator.width + control.spacing
        verticalAlignment: Text.AlignVCenter
        text: control.text
        color: control.enabled ? Theme.text : Theme.textMuted
        font.family: Theme.font
        font.pixelSize: Theme.small
        wrapMode: Text.WordWrap
    }
}
