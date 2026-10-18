import QtQuick
import QtQuick.Layouts

// "Approximate drawing" hint under a device picture, with a link to line up the user's own picture.
RowLayout {
    id: note

    property var backend: null
    property string pid: ""
    spacing: 8

    Text {
        text: "Approximate drawing — key positions may differ on your model."
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    Text {
        objectName: "useOwnLink"
        visible: !!note.backend
        text: "Use my own picture…"
        color: Theme.accent
        font.family: Theme.font
        font.pixelSize: Theme.small
        font.underline: true
        HoverHandler {
            cursorShape: Qt.PointingHandCursor
        }
        TapHandler {
            onTapped: pc.chooseOwn()
        }
    }
    // Same flow as the Device picture card on the Customize page.
    PictureControls {
        id: pc
        visible: false
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
        backend: note.backend
        pid: note.pid
    }
}
