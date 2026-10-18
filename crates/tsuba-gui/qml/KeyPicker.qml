import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Grouped key chooser (letters, numbers, function keys, navigation…): pick a group, then a key.
// For keys you can't press on the keyboard in front of you (F13–F24, the numpad, media keys).
ColumnLayout {
    id: picker

    // Modifier bits that go with the picked key (1 Ctrl, 2 Shift, 4 Alt, 8 Meta).
    property int mods: 0
    // The usage picked last (highlighted), or -1.
    property int current: -1
    property bool showModifiers: true
    property int group: 0
    // Emitted with the keyboard-function data [modifier bits, HID usage].
    signal picked(var data, int usage)

    spacing: 8

    SynDropdown {
        id: groupPick
        objectName: "keyGroupPick"
        Layout.fillWidth: true
        model: KeyNames.keyGroups.map(g => g[0])
        currentIndex: picker.group
        onActivated: i => picker.group = i
    }
    Flow {
        id: chips
        objectName: "keyChips"
        Layout.fillWidth: true
        spacing: 4
        Repeater {
            model: KeyNames.keyGroups[picker.group][1]
            delegate: Rectangle {
                id: chip
                required property int modelData
                readonly property bool sel: picker.current === modelData
                width: Math.max(40, label.implicitWidth + 16)
                height: 30
                radius: 2
                color: sel ? Theme.accentTintStrong : (ch.hovered ? Theme.hoverTint : Theme.bg)

                border.color: sel || ch.hovered ? Theme.accent : Theme.borderSoft
                Text {
                    id: label
                    anchors.centerIn: parent
                    text: KeyNames.hidName(chip.modelData)
                    color: chip.sel ? Theme.accent : Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                }
                HoverHandler {
                    id: ch
                    cursorShape: Qt.PointingHandCursor
                }
                TapHandler {
                    onTapped: {
                        picker.current = chip.modelData;
                        picker.picked(KeyNames.pickedData(chip.modelData, picker.mods), chip.modelData);
                    }
                }
            }
        }
    }
    RowLayout {
        visible: picker.showModifiers && !(picker.current >= 0xe0 && picker.current <= 0xe7)
        spacing: 2
        Repeater {
            model: [["Ctrl", 1], ["Shift", 2], ["Alt", 4], ["Meta", 8]]
            delegate: SynCheckBox {
                required property var modelData
                objectName: "mod" + modelData[0]
                text: modelData[0]
                checked: (picker.mods & modelData[1]) !== 0
                onToggled: {
                    picker.mods = checked ? picker.mods | modelData[1] : picker.mods & ~modelData[1];
                    if (picker.current >= 0) {
                        picker.picked(KeyNames.pickedData(picker.current, picker.mods), picker.current);
                    }
                }
            }
        }
    }
}
