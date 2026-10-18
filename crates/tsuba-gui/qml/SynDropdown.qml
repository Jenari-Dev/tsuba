import QtQuick
import QtQuick.Controls as QQC2

// Synapse dropdown: dark box with a 1 px border that turns green when open; the list marks
// the current item in green.
QQC2.ComboBox {
    id: control

    implicitHeight: 34
    implicitWidth: 240
    leftPadding: 12
    rightPadding: 32
    hoverEnabled: true

    background: Rectangle {
        color: Theme.card
        radius: 2
        border.width: 1
        border.color: control.popup.visible || control.activeFocus ? Theme.accent : (control.hovered ? Theme.textDim : Theme.border)
    }
    contentItem: Text {
        text: control.displayText
        color: control.enabled ? Theme.text : Theme.textMuted
        font.family: Theme.font
        font.pixelSize: Theme.fontSize
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    indicator: Text {
        x: control.width - width - 12
        y: (control.height - height) / 2
        text: control.popup.visible ? "▴" : "▾"
        color: Theme.text
        font.pixelSize: 12
    }
    delegate: QQC2.ItemDelegate {
        required property var modelData
        required property int index
        width: control.width
        height: 34
        highlighted: control.highlightedIndex === index
        padding: 0
        background: Rectangle {
            color: parent.highlighted ? Theme.hoverFill : "transparent"
        }
        contentItem: Text {
            leftPadding: 12
            text: control.textRole ? (modelData[control.textRole] !== undefined ? modelData[control.textRole] : modelData) : modelData
            color: control.currentIndex === index ? Theme.accent : Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }
    popup: QQC2.Popup {
        y: control.height - 1
        width: control.width
        height: Math.min(contentItem.implicitHeight + 2, 340)
        padding: 1
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            QQC2.ScrollIndicator.vertical: QQC2.ScrollIndicator {}
        }
        background: Rectangle {
            color: Theme.card
            border.color: Theme.accent
            border.width: 1
        }
    }
}
