import QtQuick
import QtQuick.Layouts

// Synapse card: dark panel, green uppercase title (optional inline header item such as a
// toggle) and a "?" badge top-right. Children go below the title.
Rectangle {
    id: card

    default property alias content: inner.data
    // Items placed beside the title (a SynToggle, a status text…).
    property alias headerContent: headerRow.data
    property string title: ""
    property string description: ""
    // Plain-English help shown by the "?" button (hidden when empty).
    property string help: ""
    property bool showHelp: true
    property int padding: 20

    Layout.fillWidth: true
    implicitHeight: col.implicitHeight + 2 * padding
    color: Theme.card
    radius: 5

    ColumnLayout {
        id: col
        x: card.padding
        y: card.padding
        width: card.width - 2 * card.padding
        spacing: 14

        RowLayout {
            visible: card.title !== "" || headerRow.children.length > 0
            Layout.fillWidth: true
            Layout.rightMargin: card.showHelp && card.help !== "" ? 28 : 0
            spacing: 14
            Text {
                visible: card.title !== ""
                text: card.title.toUpperCase()
                color: Theme.accent
                font.family: Theme.titleFont
                font.pixelSize: 16
            }
            RowLayout {
                id: headerRow
                spacing: 10
            }
            Item {
                Layout.fillWidth: true
            }
        }
        Text {
            visible: card.description !== ""
            Layout.fillWidth: true
            text: card.description
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            wrapMode: Text.Wrap
        }
        ColumnLayout {
            id: inner
            Layout.fillWidth: true
            spacing: 14
        }
    }

    HelpButton {
        visible: card.showHelp && card.help !== ""
        x: card.width - 14 - width
        y: 12
        size: 18
        text: card.help
    }
}
