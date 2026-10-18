import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Devices Tsuba knows of but can't control yet (headsets, receivers): what the database says.
QQC2.ScrollView {
    id: page

    property var support: ({})
    property var pack: null
    contentWidth: availableWidth

    ColumnLayout {
        width: page.availableWidth
        spacing: 0
        SynCard {
            Layout.maximumWidth: 760
            Layout.alignment: Qt.AlignHCenter
            Layout.margins: 24
            title: "Not supported yet"
            showHelp: false
            Text {
                Layout.fillWidth: true
                text: (page.support.name || "This device") + " is in Tsuba's list of known devices, but Tsuba can't change its settings yet."
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }
            SupportFacts {
                Layout.fillWidth: true
                support: page.support
            }
        }
    }
}
