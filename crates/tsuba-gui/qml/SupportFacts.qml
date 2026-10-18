import QtQuick
import QtQuick.Layouts

// What Tsuba's database knows about one model (from GetDeviceSupport): kind, features, effects,
// sensitivity range, polling rates and notes. Read-only.
ColumnLayout {
    id: facts

    property var support: ({})
    spacing: 10

    component Row2: RowLayout {
        id: r
        property string caption: ""
        property string value: ""
        visible: value !== ""
        Layout.fillWidth: true
        spacing: 14
        Text {
            Layout.preferredWidth: 130
            Layout.alignment: Qt.AlignTop
            text: r.caption.toUpperCase()
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: 11
            font.letterSpacing: 1
        }
        Text {
            Layout.fillWidth: true
            text: r.value
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            wrapMode: Text.WordWrap
        }
    }

    Row2 {
        objectName: "factKind"
        caption: i18nc("device fact label", "Type")
        value: facts.support.connection ? i18n("%1 · %2", DeviceKinds.kindLabel(facts.support.kind), facts.support.connection) : DeviceKinds.kindLabel(facts.support.kind)
    }
    Row2 {
        objectName: "factFeatures"
        caption: i18nc("device fact label", "Features")
        value: DeviceKinds.featureList(facts.support).join(", ")
    }
    Row2 {
        objectName: "factEffects"
        caption: i18nc("device fact label", "Lighting effects")
        value: DeviceKinds.effectList(facts.support).join(", ")
    }
    Row2 {
        objectName: "factDpi"
        caption: i18nc("device fact label", "Sensitivity")
        value: facts.support.max_dpi > 0 ? i18n("Up to %1 DPI", facts.support.max_dpi) : ""
    }
    Row2 {
        objectName: "factPolling"
        caption: i18nc("device fact label", "Polling rates")
        value: facts.support.polling_rates && facts.support.polling_rates.length ? i18n("%1 Hz", facts.support.polling_rates.join(", ")) : ""
    }
    Row2 {
        objectName: "factLeds"
        caption: i18nc("device fact label", "Lighting zones")
        value: (facts.support.leds || []).map(l => l.replace(/_/g, " ")).join(", ")
    }
    Row2 {
        objectName: "factNotes"
        caption: i18nc("device fact label", "Notes")
        value: facts.support.notes || ""
    }
}
