import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

// Keyboard switch debounce (shown when the feature probe found it). `host` is the
// Performance tab: feat, readOnly, setFeat(change).
SynCard {
    id: card

    required property var host
    readonly property var d: host.feat.debounce || null
    // Working copy for the custom fields.
    property var custom: d ? [d.make, d.make_mask, d["break"], d.break_mask] : [0, 20, 0, 40]
    onDChanged: custom = d ? [d.make, d.make_mask, d["break"], d.break_mask] : custom

    // Synapse's two timings: standard, and no masking at all.
    readonly property var presets: [{ v: [0, 20, 0, 40] }, { v: [0, 0, 0, 0] }]
    readonly property int presetIndex: {
        if (!d) {
            return -1;
        }
        const now = [d.make, d.make_mask, d["break"], d.break_mask];
        return presets.findIndex(p => JSON.stringify(p.v) === JSON.stringify(now));
    }

    function send(v) {
        host.setFeat({ debounce: { make: v[0], make_mask: v[1], "break": v[2], break_mask: v[3] } });
    }

    objectName: "debounceCard"
    title: i18n("Key response")
    help: i18n("How long the keyboard ignores a key's contacts bouncing after it is pressed (make) and released (break). Shorter times register fast repeated presses sooner; if keys start to double-type, go back to Standard. The values are stored on the keyboard.")
    description: i18n("Debounce times of the key switches, in milliseconds.")

    RowLayout {
        spacing: 14
        Text {
            text: i18n("Timing")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynPillTabs {
            objectName: "debouncePresets"
            enabled: !card.host.readOnly
            model: [i18n("Standard"), i18n("Fast"), i18n("Custom")]
            currentIndex: card.presetIndex >= 0 ? card.presetIndex : card.presets.length
            onActivated: i => {
                if (i < card.presets.length) {
                    card.send(card.presets[i].v);
                }
            }
        }
    }
    GridLayout {
        columns: 4
        columnSpacing: 12
        rowSpacing: 6
        Repeater {
            model: [i18n("Press"), i18n("Press mask"), i18n("Release"), i18n("Release mask")]
            delegate: Text {
                required property string modelData
                text: modelData
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
        }
        Repeater {
            model: 4
            delegate: QQC2.SpinBox {
                id: box
                required property int index
                objectName: "debounce" + index
                from: 0
                to: 100
                editable: true
                enabled: !card.host.readOnly
                value: card.custom[index]
                onValueModified: {
                    const c = card.custom.slice();
                    c[box.index] = value;
                    card.custom = c;
                }
            }
        }
    }
    RowLayout {
        spacing: 12
        SynButton {
            objectName: "debounceApply"
            text: i18n("Apply")
            primary: true
            enabled: !card.host.readOnly && !!card.d && JSON.stringify(card.custom) !== JSON.stringify([card.d.make, card.d.make_mask, card.d["break"], card.d.break_mask])
            onClicked: card.send(card.custom)
        }
        Text {
            visible: !!card.d
            text: card.d ? i18n("Now: press %1 / %2 ms, release %3 / %4 ms", card.d.make, card.d.make_mask, card.d["break"], card.d.break_mask) : ""
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
    }
}
