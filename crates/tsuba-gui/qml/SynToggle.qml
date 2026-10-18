import QtQuick

// Small green pill switch (Synapse toggle).
Item {
    id: control

    property bool checked: false
    signal toggled(bool on)

    implicitWidth: 40
    implicitHeight: 20
    opacity: enabled ? 1 : 0.4

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: control.checked ? Theme.accent : Theme.inactive
        Rectangle {
            width: parent.height - 6
            height: width
            radius: width / 2
            y: 3
            x: control.checked ? parent.width - width - 3 : 3
            color: control.checked ? Theme.accentText : Theme.knob
            Behavior on x {
                NumberAnimation {
                    duration: 120
                }
            }
        }
    }
    TapHandler {
        onTapped: control.toggled(!control.checked)
    }
    HoverHandler {
        cursorShape: Qt.PointingHandCursor
    }
}
