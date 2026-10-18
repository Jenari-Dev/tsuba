import QtQuick

// Small uppercase grey section heading, as used across Synapse panels.
Text {
    property string caption: ""

    text: caption.toUpperCase()
    font.family: Theme.font
    font.pixelSize: Theme.small
    font.letterSpacing: 1
    font.bold: true
    color: Theme.textDim
}
