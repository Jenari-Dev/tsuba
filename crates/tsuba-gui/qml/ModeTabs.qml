import QtQuick
import QtQuick.Layouts

// Segmented pill buttons (kept as a Row so existing callers and tests can reach the segments;
// SynPillTabs is the outlined single-pill variant used on the Synapse-style pages).
Row {
    id: root

    property var model: []
    // `currentIndex` stays bindable (parents bind it to their data); the highlight follows
    // `selected`, which tracks currentIndex and the latest click without breaking the binding.
    property int currentIndex: 0
    property int selected: currentIndex
    onCurrentIndexChanged: selected = currentIndex
    property int minTabWidth: 110
    signal activated(int index)

    spacing: 4

    Repeater {
        model: root.model
        delegate: Rectangle {
            required property string modelData
            required property int index
            readonly property bool active: index === root.selected
            width: Math.max(root.minTabWidth, caption.implicitWidth + 30)
            height: 28
            radius: 14
            color: active ? Theme.accent : (hover.hovered ? "#2d2d2d" : "transparent")
            border.color: active ? Theme.accent : "#707070"
            border.width: 1
            Text {
                id: caption
                anchors.centerIn: parent
                text: modelData
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                color: active ? Theme.accentText : Theme.text
            }
            HoverHandler {
                id: hover
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
