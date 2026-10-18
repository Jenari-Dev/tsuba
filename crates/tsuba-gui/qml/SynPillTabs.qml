import QtQuick

// Pill segmented control: outlined pill, active segment is a filled green pill with dark text.
// `outlined: false` gives the borderless page-tab look.
Item {
    id: root

    property var model: []
    // `currentIndex` stays bindable (parents bind it to their data); the highlight follows
    // `selected`, which tracks currentIndex and the latest click without breaking the binding.
    property int currentIndex: 0
    property int selected: currentIndex
    onCurrentIndexChanged: selected = currentIndex
    property bool outlined: true
    property bool upper: false
    property int pillHeight: outlined ? 28 : 30
    property int fontSize: Theme.fontSize
    property int minTabWidth: 0
    signal activated(int index)

    implicitHeight: pillHeight + (outlined ? 6 : 0)
    implicitWidth: row.implicitWidth + (outlined ? 6 : 0)
    opacity: enabled ? 1 : 0.5

    Rectangle {
        anchors.fill: parent
        visible: root.outlined
        radius: height / 2
        color: "transparent"
        border.color: "#707070"
        border.width: 1
    }
    Row {
        id: row
        anchors.centerIn: parent
        spacing: root.outlined ? 2 : 6
        Repeater {
            model: root.model
            delegate: Rectangle {
                required property var modelData
                required property int index
                readonly property bool active: index === root.selected
                width: Math.max(root.minTabWidth, caption.implicitWidth + 28)
                height: root.pillHeight
                radius: height / 2
                color: active ? Theme.accent : (hover.hovered && !root.outlined ? "#2d2d2d" : "transparent")
                Text {
                    id: caption
                    anchors.centerIn: parent
                    text: root.upper ? String(modelData).toUpperCase() : modelData
                    font.family: Theme.font
                    font.pixelSize: root.fontSize
                    color: active ? Theme.accentText : Theme.text
                }
                HoverHandler {
                    id: hover
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: {
                        root.selected = index;
                        root.activated(index);
                        // Settle on what the data says (a click the parent rejects doesn't stick).
                        Qt.callLater(() => root.selected = root.currentIndex);
                    }
                }
            }
        }
    }
}
