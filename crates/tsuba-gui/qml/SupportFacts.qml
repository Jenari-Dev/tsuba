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
        caption: "Type"
        value: DeviceKinds.kindLabel(facts.support.kind) + (facts.support.connection ? " · " + facts.support.connection : "")
    }
    Row2 {
        objectName: "factFeatures"
        caption: "Features"
        value: DeviceKinds.featureList(facts.support).join(", ")
    }
    Row2 {
        objectName: "factEffects"
        caption: "Lighting effects"
        value: DeviceKinds.effectList(facts.support).join(", ")
    }
    Row2 {
        objectName: "factDpi"
        caption: "Sensitivity"
        value: facts.support.max_dpi > 0 ? "Up to " + facts.support.max_dpi + " DPI" : ""
    }
    Row2 {
        objectName: "factPolling"
        caption: "Polling rates"
        value: facts.support.polling_rates && facts.support.polling_rates.length ? facts.support.polling_rates.join(", ") + " Hz" : ""
    }
    Row2 {
        objectName: "factLeds"
        caption: "Lighting zones"
        value: (facts.support.leds || []).map(l => l.replace(/_/g, " ")).join(", ")
    }
    Row2 {
        objectName: "factNotes"
        caption: "Notes"
        value: facts.support.notes || ""
    }
}
