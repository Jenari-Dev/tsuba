import QtQuick
import QtQuick.Layouts

// Top-view mouse illustration with button callouts on both sides (Synapse-style).
// Button ids are the firmware's; names for the Basilisk V3 Pro.
Item {
    id: root

    property var assignments: ({})   // button id -> assignment
    property int selected: -1
    signal picked(int pos, string name)

    readonly property var names: ({
        1: "Left Click", 2: "Right Click", 3: "Scroll Click", 4: "Mouse Button 4", 5: "Mouse Button 5",
        9: "Scroll Up", 10: "Scroll Down", 14: "Profile Button", 15: "Sensitivity Clutch",
        52: "Scroll Left", 53: "Scroll Right", 96: "Sensitivity Stage Up", 106: "Scroll Mode Toggle"
    })

    readonly property var leftSide: [4, 5, 15, 14, 52]
    readonly property var rightSide: [1, 2, 3, 9, 10, 53, 96, 106]

    component Callout: Rectangle {
        id: c
        required property int pos
        readonly property var a: root.assignments[pos]
        readonly property bool isSel: root.selected === pos
        implicitWidth: 250
        implicitHeight: 52
        visible: a !== undefined
        color: hov.hovered ? "#1affffff" : Theme.panel
        border.color: isSel ? Theme.accent : Theme.borderSoft
        border.width: isSel ? 2 : 1
        Column {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 2
            Text {
                text: (root.names[c.pos] || ("Button " + c.pos)).toUpperCase()
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: 11
                font.letterSpacing: 1
            }
            Text {
                text: KeyNames.describe(c.a)
                // Keyboard/macro output on a mouse button is always a user remap.
                color: c.isSel || (c.a && [2, 3, 4, 5, 15].indexOf(c.a.function) >= 0) ? Theme.accent : Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                elide: Text.ElideRight
                width: parent.width
            }
        }
        HoverHandler {
            id: hov
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            onTapped: {
                root.picked(c.pos, root.names[c.pos] || ("Button " + c.pos));
            }
        }
    }

    RowLayout {
        anchors.centerIn: parent
        spacing: 36

        ColumnLayout {
            spacing: 10
            Repeater {
                model: root.leftSide
                delegate: Callout {
                    required property int modelData
                    pos: modelData
                }
            }
        }

        // Mouse body (own artwork): shell, button split, wheel, DPI and mode buttons.
        Item {
            Layout.preferredWidth: 170
            Layout.preferredHeight: 300
            Rectangle {
                anchors.fill: parent
                radius: 85
                color: "#141414"
                border.color: Theme.border
                border.width: 2
            }
            Rectangle {
                x: parent.width / 2 - 1
                y: 6
                width: 2
                height: 120
                color: Theme.border
            }
            Rectangle {
                x: 6
                y: 124
                width: parent.width - 12
                height: 2
                color: Theme.borderSoft
            }
            Rectangle {
                id: wheel
                anchors.horizontalCenter: parent.horizontalCenter
                y: 34
                width: 18
                height: 46
                radius: 8
                color: root.selected === 3 || root.selected === 9 || root.selected === 10 ? Theme.accent : "#2a2a2a"
                border.color: Theme.border
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 14
                width: 14
                height: 10
                radius: 3
                color: root.selected === 106 ? Theme.accent : "#2a2a2a"
                border.color: Theme.border
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 90
                width: 14
                height: 18
                radius: 3
                color: root.selected === 96 ? Theme.accent : "#2a2a2a"
                border.color: Theme.border
            }
            Rectangle {
                // Side buttons on the left flank.
                x: -4
                y: 130
                width: 8
                height: 70
                radius: 4
                color: root.selected === 4 || root.selected === 5 ? Theme.accent : "#2a2a2a"
                border.color: Theme.border
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 40
                text: "TSUBA"
                color: "#2f2f2f"
                font.family: Theme.font
                font.pixelSize: 16
                font.bold: true
                font.letterSpacing: 4
            }
        }

        ColumnLayout {
            spacing: 10
            Repeater {
                model: root.rightSide
                delegate: Callout {
                    required property int modelData
                    pos: modelData
                }
            }
        }
    }
}
