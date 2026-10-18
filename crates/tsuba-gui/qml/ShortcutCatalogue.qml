import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Shortcut presets by area (system, editing, browser, office, creative apps, video, meetings)
// plus a builder for your own: modifier chips and a key. When the profile is linked to an app,
// the area that suits it comes first.
ColumnLayout {
    id: cat

    required property var panel
    // Linked app names of the profile (set by the panel), used to suggest an area.
    property var linkedApps: []
    readonly property string suggested: {
        for (const n of linkedApps) {
            const c = KeyNames.categoryForApp(n);
            if (c !== "") {
                return c;
            }
        }
        return "";
    }
    // Areas in display order: the suggested one first.
    readonly property var areas: {
        const all = KeyNames.shortcutCatalogue.slice();
        const i = all.findIndex(a => a[0] === suggested);
        if (i > 0) {
            all.unshift(all.splice(i, 1)[0]);
        }
        return all;
    }
    property int areaIdx: 0
    onLinkedAppsChanged: areaIdx = 0
    readonly property var items: areas[Math.min(areaIdx, areas.length - 1)][2]
    // The custom shortcut being built.
    property int customMods: 0
    property int customKey: 0

    spacing: 8

    Text {
        visible: cat.suggested !== ""
        objectName: "suggestedNote"
        Layout.fillWidth: true
        text: i18n("Suggested for %1", cat.linkedApps.join(", "))
        color: Theme.accent
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
    SynDropdown {
        id: areaPick
        objectName: "shortcutArea"
        Layout.fillWidth: true
        model: cat.areas.map(a => a[1] + (a[0] === cat.suggested ? i18n("  (suggested)") : ""))
        currentIndex: cat.areaIdx
        onActivated: i => cat.areaIdx = i
    }
    Repeater {
        model: cat.items
        delegate: OptionRow {
            required property var modelData
            label: modelData[0]
            on: !!cat.panel.assignment && cat.panel.assignment.function === 2 && cat.panel.assignment.data[0] === modelData[1] && cat.panel.assignment.data[1] === modelData[2]
            chosen: cat.panel.stagedId === "shortcut:" + modelData[1] + ":" + modelData[2]
            onPicked: cat.panel.stage("shortcut:" + modelData[1] + ":" + modelData[2], modelData[0], () => cat.panel.save(2, [modelData[1], modelData[2]]))
        }
    }

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 1
        Layout.topMargin: 6
        color: Theme.borderSoft
    }
    Text {
        text: i18n("YOUR OWN SHORTCUT")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: 11
        font.letterSpacing: 1
    }
    // Modifier chips: click to turn each on or off.
    Row {
        spacing: 6
        Repeater {
            model: [["Ctrl", 1], ["Shift", 2], ["Alt", 4], ["Meta", 8]]
            delegate: Rectangle {
                required property var modelData
                objectName: "chip" + modelData[0]
                readonly property bool on: (cat.customMods & modelData[1]) !== 0
                width: 58
                height: 28
                radius: 14
                color: on ? Theme.accent : "transparent"
                border.color: on ? Theme.accent : Theme.border
                Text {
                    anchors.centerIn: parent
                    text: modelData[0]
                    color: parent.on ? Theme.accentText : Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    font.bold: parent.on
                }
                TapHandler {
                    onTapped: {
                        cat.customMods = cat.customMods ^ modelData[1];
                        cat.stageCustom();
                    }
                }
                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }
            }
        }
    }
    KeyPicker {
        id: customPicker
        objectName: "customKeyPicker"
        Layout.fillWidth: true
        showModifiers: false
        mods: 0
        onPicked: (data, usage) => {
            cat.customKey = usage;
            cat.stageCustom();
        }
    }
    Text {
        visible: cat.customKey !== 0 || cat.customMods !== 0
        objectName: "customChord"
        text: KeyNames.chord(cat.customMods, cat.customKey)
        color: Theme.accent
        font.family: Theme.font
        font.pixelSize: Theme.fontSize
    }

    function stageCustom() {
        if (customKey === 0 && customMods === 0) {
            return;
        }
        const mods = customKey >= 0xe0 && customKey <= 0xe7 ? 0 : customMods;
        const data = KeyNames.pickedData(customKey, mods);
        panel.stage("shortcut:custom", KeyNames.chord(data[0], data[1]), () => panel.save(2, data));
    }
}
