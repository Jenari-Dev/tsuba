import QtQuick
import QtQuick.Layouts

// One selectable row in the assignment panel's editors. `on` marks what the key does now,
// `chosen` what the user picked and hasn't saved yet.
Rectangle {
    id: opt

    property string label: ""
    property bool on: false
    property bool chosen: false
    signal picked()

    Layout.fillWidth: true
    Layout.preferredHeight: 32
    color: chosen ? Theme.accentTintStrong : (on ? Theme.accentTint : (oh.hovered ? "#14ffffff" : Theme.bg))
    border.color: chosen || on ? Theme.accent : Theme.borderSoft

    Text {
        anchors.left: parent.left
        anchors.leftMargin: 10
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        text: opt.label
        color: opt.chosen || opt.on ? Theme.accent : Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.small
        elide: Text.ElideRight
    }
    HoverHandler {
        id: oh
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        onTapped: opt.picked()
    }
}
