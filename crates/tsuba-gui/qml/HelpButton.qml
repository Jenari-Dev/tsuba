import QtQuick
import QtQuick.Controls as QQC2

// Small "?" circle (Tsuba's own drawing): outline + glyph in the accent colour, filled tint on
// hover. Clicking toggles a popover with `text` (plain-English help).
Item {
    id: hb

    property string text: ""
    property int size: 18
    property alias popupOpen: pop.opened

    implicitWidth: size
    implicitHeight: size
    visible: text !== ""

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: hov.hovered || pop.opened ? Theme.accentTintStrong : "transparent"
        border.color: Theme.accent
        border.width: 1
        Text {
            anchors.centerIn: parent
            text: "?"
            color: Theme.accent
            font.family: Theme.font
            font.pixelSize: Math.round(hb.size * 0.62)
            font.bold: true
        }
    }
    HoverHandler {
        id: hov
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        objectName: "helpTap"
        onTapped: pop.opened ? pop.close() : pop.open()
    }
    Accessible.role: Accessible.Button
    Accessible.name: "Help"

    QQC2.Popup {
        id: pop
        objectName: "helpPopup"
        readonly property real gx: hb.mapToItem(null, 0, 0).x
        readonly property real winW: hb.Window.width > 0 ? hb.Window.width : 1000
        width: Math.min(320, winW - 16)
        // Right-aligned under the button, nudged to stay inside the window.
        x: Math.max(8 - gx, Math.min(hb.width - width, winW - 8 - width - gx))
        y: hb.height + 6
        padding: 12
        closePolicy: QQC2.Popup.CloseOnEscape | QQC2.Popup.CloseOnPressOutsideParent
        background: Rectangle {
            color: Theme.card
            border.color: Theme.borderSoft
            radius: 4
        }
        contentItem: Text {
            text: hb.text
            width: pop.availableWidth
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: 13
            wrapMode: Text.Wrap
        }
    }
}
