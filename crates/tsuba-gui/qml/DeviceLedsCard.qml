import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts

// Status lights stored in the device: the on-board profile's colour, indicator LEDs and the
// power-on animation. Each row shows only when the device answered it in the feature probe
// (neither the BlackWidow V4 Pro nor the Basilisk V3 Pro does). `host`: feat, readOnly,
// profile, setFeat(change).
SynCard {
    id: card

    required property var host
    readonly property var f: host.feat || ({})
    readonly property bool hasColour: f.profile_colour !== undefined
    readonly property bool hasIndicator: f.indicator_led !== undefined
    readonly property bool hasIndicators: f.indicator_leds !== undefined
    readonly property bool hasStartup: f.startup_animation !== undefined

    objectName: "deviceLedsCard"
    visible: hasColour || hasIndicator || hasIndicators || hasStartup
    title: i18n("Status lights")
    help: i18n("Lights the device itself controls: the colour that marks the active on-board profile, indicator LEDs and the animation shown when the device powers on. These settings are stored on the device.")

    component Row14: RowLayout {
        Layout.fillWidth: true
        spacing: 12
    }
    component Label14: Text {
        Layout.fillWidth: true
        color: Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.fontSize
    }

    Row14 {
        visible: card.hasColour
        Label14 {
            text: i18n("Profile %1 colour", card.host.profile)
        }
        Rectangle {
            objectName: "profileColourSwatch"
            Layout.preferredWidth: 26
            Layout.preferredHeight: 26
            radius: 13
            color: "#" + (card.f.profile_colour || "000000")
            border.color: Theme.border
            TapHandler {
                enabled: !card.host.readOnly
                onTapped: {
                    colourDialog.selectedColor = "#" + (card.f.profile_colour || "ffffff");
                    colourDialog.open();
                }
            }
        }
    }
    Row14 {
        visible: card.hasIndicator
        Label14 {
            text: i18n("Indicator LEDs")
        }
        SynToggle {
            objectName: "indicatorToggle"
            enabled: !card.host.readOnly
            checked: !!card.f.indicator_led
            onToggled: on => card.host.setFeat({ indicator_led: on })
        }
    }
    Repeater {
        model: card.hasIndicators ? card.f.indicator_leds : []
        delegate: RowLayout {
            id: ind
            required property var modelData
            required property int index
            Layout.fillWidth: true
            spacing: 12
            Label14 {
                text: i18n("Indicator %1", ind.index + 1)
            }
            SynToggle {
                enabled: !card.host.readOnly
                checked: ind.modelData !== 0
                onToggled: on => {
                    const v = card.f.indicator_leds.slice();
                    v[ind.index] = on ? 1 : 0;
                    card.host.setFeat({ indicator_leds: v });
                }
            }
        }
    }
    Row14 {
        visible: card.hasStartup
        Label14 {
            text: i18n("Power-on animation")
        }
        SynToggle {
            objectName: "startupToggle"
            enabled: !card.host.readOnly
            checked: !!card.f.startup_animation
            onToggled: on => card.host.setFeat({ startup_animation: on })
        }
    }

    ColorDialog {
        id: colourDialog
        title: i18n("Profile colour")
        onAccepted: card.host.setFeat({ profile_colour: selectedColor.toString().replace("#", "").slice(-6) })
    }
}
