import QtQuick

// Row of square bordered buttons (polling rate): selected one gets a green border.
Item {
    id: root

    property var model: []
    property int currentIndex: -1
    property int selected: currentIndex
    onCurrentIndexChanged: selected = currentIndex
    property int buttonSize: 64
    signal activated(int index)

    implicitWidth: 400
    implicitHeight: row.childrenRect.height
    opacity: enabled ? 1 : 0.5

    Flow {
        id: row
        width: root.width
        spacing: 10
        Repeater {
            model: root.model
            delegate: Rectangle {
                required property var modelData
                required property int index
                readonly property bool active: index === root.selected
                width: root.buttonSize
                height: root.buttonSize
                radius: 3
                color: hov.hovered ? "#2d2d2d" : "transparent"
                border.color: active ? Theme.accent : "#5d5d5d"
                border.width: active ? 2 : 1
                Text {
                    anchors.centerIn: parent
                    text: modelData
                    color: active ? Theme.accent : Theme.text
                    font.family: Theme.font
                    font.pixelSize: 16
                }
                HoverHandler {
                    id: hov
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: {
                        root.selected = index;
                        root.activated(index);
                        Qt.callLater(() => root.selected = root.currentIndex);
                    }
                }
            }
        }
    }
}
