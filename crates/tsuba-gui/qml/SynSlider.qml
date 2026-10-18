import QtQuick
import QtQuick.Controls as QQC2

// Synapse-style slider: 4 px track (green fill, dark-green rest), round green handle, optional
// value badge above the handle, min/max labels and tick labels below.
QQC2.Slider {
    id: control

    // Green value badge above the handle.
    property bool badge: false
    property string badgeText: String(Math.round(value))
    // "0" / "100" style labels under the ends of the track.
    property bool rangeLabels: false
    property string minText: String(Math.round(from))
    property string maxText: String(Math.round(to))
    // Tick labels spread across the track (SHORT / MEDIUM / LONG), or [{pos: 0..1, text}].
    property var ticks: []

    readonly property int topSpace: badge ? 28 : 0
    readonly property int bottomSpace: (rangeLabels || ticks.length > 0) ? 22 : 0
    // The custom background/handle have no implicit size of their own; without this the
    // layout makes the slider 0 px tall and it can't be dragged.
    implicitHeight: topSpace + 20 + bottomSpace
    implicitWidth: 200
    topPadding: topSpace
    bottomPadding: bottomSpace

    background: Item {
        x: control.leftPadding
        y: control.topPadding
        width: control.availableWidth
        height: control.availableHeight
        Rectangle {
            y: parent.height / 2 - 2
            width: parent.width
            height: 4
            radius: 2
            color: control.enabled ? Theme.sliderRest : Theme.inactive
            Rectangle {
                width: control.visualPosition * parent.width
                height: parent.height
                radius: 2
                color: control.enabled ? Theme.accent : Theme.textMuted
            }
        }
        Text {
            visible: control.rangeLabels
            y: parent.height + 4
            text: control.minText
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        Text {
            visible: control.rangeLabels
            y: parent.height + 4
            x: parent.width - width
            text: control.maxText
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        Repeater {
            model: control.ticks
            delegate: Text {
                required property var modelData
                required property int index
                readonly property real pos: typeof modelData === "object" ? modelData.pos : (control.ticks.length > 1 ? index / (control.ticks.length - 1) : 0)
                y: parent.height + 4
                x: Math.max(0, Math.min(parent.width - width, pos * parent.width - width / 2))
                text: typeof modelData === "object" ? modelData.text : modelData
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }
    }
    handle: Rectangle {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + control.availableHeight / 2 - height / 2
        implicitWidth: 16
        implicitHeight: 16
        radius: 8
        color: control.enabled ? (control.pressed ? Theme.accentHover : Theme.accent) : Theme.textMuted
        Rectangle {
            visible: control.badge
            anchors.horizontalCenter: parent.horizontalCenter
            y: -height - 8
            width: Math.max(26, badgeLabel.implicitWidth + 12)
            height: 20
            radius: 2
            color: control.enabled ? Theme.accent : Theme.textMuted
            Text {
                id: badgeLabel
                anchors.centerIn: parent
                text: control.badgeText
                color: Theme.accentText
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }
    }
}
