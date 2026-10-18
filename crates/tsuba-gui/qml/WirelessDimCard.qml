import QtQuick
import QtQuick.Layouts

// Firmware lighting dimming for battery devices: "smart dimming" (the device's smart
// illumination switch) and "dim when idle" (time to dim). Each part shows only when the
// device answered it in the feature probe. `host`: feat, readOnly, setFeat(change).
SynCard {
    id: card

    required property var host
    readonly property var smart: host.feat.smart_illumination || null
    readonly property var dim: host.feat.time_to_dim || null

    objectName: "wirelessDimCard"
    visible: smart !== null || dim !== null
    title: i18n("Lighting dimming")
    help: i18n("Saves battery by dimming the lighting. Smart dimming is the device's own automatic dimming; Dim when idle lowers the lighting after the device has not been used for a while. Both are stored on the device.")

    RowLayout {
        visible: card.smart !== null
        Layout.fillWidth: true
        spacing: 12
        Text {
            Layout.fillWidth: true
            text: i18n("Smart dimming")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynToggle {
            objectName: "smartDimToggle"
            enabled: !card.host.readOnly
            checked: !!card.smart && card.smart.on
            onToggled: on => card.host.setFeat({ smart_illumination: { on: on, mode: card.smart.mode } })
        }
    }
    RowLayout {
        visible: card.dim !== null
        Layout.fillWidth: true
        spacing: 12
        Text {
            Layout.fillWidth: true
            text: i18n("Dim when idle")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynToggle {
            objectName: "dimIdleToggle"
            enabled: !card.host.readOnly
            checked: !!card.dim && card.dim.enabled
            onToggled: on => card.host.setFeat({ time_to_dim: Object.assign({}, card.dim, { enabled: on, seconds: Math.max(60, card.dim.seconds) }) })
        }
    }
    Text {
        visible: card.dim !== null
        text: i18n("Dim after being idle for (minutes)")
        color: Theme.textDim
        font.family: Theme.font
        font.pixelSize: Theme.small
    }
    SynSlider {
        visible: card.dim !== null
        Layout.fillWidth: true
        from: 1
        to: 15
        stepSize: 1
        badge: true
        rangeLabels: true
        enabled: !card.host.readOnly && !!card.dim && card.dim.enabled
        value: card.dim ? Math.max(1, Math.round(card.dim.seconds / 60)) : 5
        onPressedChanged: if (!pressed) card.host.setFeat({ time_to_dim: Object.assign({}, card.dim, { seconds: Math.round(value) * 60 }) })
    }
}
