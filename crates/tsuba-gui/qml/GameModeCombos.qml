import QtQuick
import QtQuick.Layouts

// Which desktop shortcuts Game Mode blocks: all of them (KDE's own switch) or just the chosen
// combinations. `combos` is null for "all", else the list of chosen names.
ColumnLayout {
    id: gmc

    property var combos: null
    property bool readOnly: false
    // The combinations Tsuba can block one by one (names match the daemon's list).
    readonly property var catalogue: [
        ["alt_tab", i18n("Alt+Tab (switch window)")],
        ["alt_f4", i18n("Alt+F4 (close window)")],
        ["alt_space", i18n("Alt+Space / Alt+F2 (launcher)")],
        ["meta", i18n("Windows key (launcher, overview, Windows-key shortcuts)")],
        ["ctrl_esc", i18n("Ctrl+Esc (task manager)")]
    ]
    readonly property bool all: combos === null
    signal chosen(var list)

    spacing: 6

    SynCheckBox {
        objectName: "gmAll"
        text: i18n("Block every desktop shortcut")
        enabled: !gmc.readOnly
        checked: gmc.all
        onToggled: gmc.chosen(checked ? null : gmc.catalogue.map(c => c[0]))
    }
    Repeater {
        model: gmc.catalogue
        delegate: SynCheckBox {
            required property var modelData
            objectName: "gm_" + modelData[0]
            Layout.leftMargin: 28
            text: modelData[1]
            visible: !gmc.all
            enabled: !gmc.readOnly
            checked: !gmc.all && gmc.combos.indexOf(modelData[0]) >= 0
            onToggled: {
                const cur = gmc.all ? [] : gmc.combos.slice();
                const i = cur.indexOf(modelData[0]);
                if (checked && i < 0) {
                    cur.push(modelData[0]);
                } else if (!checked && i >= 0) {
                    cur.splice(i, 1);
                }
                gmc.chosen(cur);
            }
        }
    }
    Text {
        visible: !gmc.all
        Layout.fillWidth: true
        Layout.leftMargin: 28
        text: i18n("Only these shortcuts are taken away from the desktop while Game Mode is on, and given back when it ends (also after a crash, at the next start).")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
}
