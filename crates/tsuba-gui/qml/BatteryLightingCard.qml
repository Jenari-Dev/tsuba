import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts

// Different lighting while the device runs on battery (wireless). tsubad switches when the
// device leaves the charger / cable and back when it is plugged in again. `host`: devb, pid,
// readOnly, feat (shown for wireless models that report a battery).
SynCard {
    id: card

    required property var host
    property var saved: null
    property string colour: "44d62c"
    property real level: 20

    function load() {
        if (!host.devb) {
            return;
        }
        const s = host.devb.battery_lighting(host.pid);
        saved = s === "" || s === "null" ? null : JSON.parse(s);
        if (saved && saved.effect.type === "static") {
            colour = saved.effect.colour;
        }
        if (saved && saved.brightness !== undefined && saved.brightness !== null) {
            level = saved.brightness;
        }
    }
    Component.onCompleted: load()

    readonly property int choice: !saved ? 0 : (saved.effect.type === "off" ? 1 : (saved.effect.type === "spectrum" ? 2 : 3))
    function pick(i, lvl) {
        const l = [null, { effect: { type: "off" } }, { effect: { type: "spectrum" }, brightness: Math.round(lvl) }, { effect: { type: "static", colour: colour }, brightness: Math.round(lvl) }][i];
        if (host.devb.set_battery_lighting(host.pid, JSON.stringify(l))) {
            host.setResult(i === 0 ? i18n("Lighting on battery: same as plugged in") : i18n("Lighting on battery saved"), false);
        } else {
            host.setResult(i18n("Failed: %1", host.devb.status), true);
        }
        load();
    }

    objectName: "batteryLightingCard"
    visible: !!host.feat.battery && !!host.feat.wireless && !!host.devb
    title: i18n("Lighting on battery")
    help: i18n("Use different, more battery-friendly lighting while the device runs wirelessly. Tsuba switches back to your normal lighting when the device is plugged in or on its charger. Tsuba needs to be running in the background (tsubad) for the switch.")

    SynDropdown {
        objectName: "batteryLightingChoice"
        Layout.preferredWidth: 300
        enabled: !card.host.readOnly
        model: [i18n("Same as plugged in"), i18n("Lighting off"), i18n("Spectrum cycling, dimmed"), i18n("Static colour, dimmed")]
        currentIndex: card.choice
        onActivated: i => card.pick(i, card.level)
    }
    RowLayout {
        visible: card.choice >= 2
        Layout.fillWidth: true
        spacing: 12
        Text {
            text: i18n("Brightness")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynSlider {
            Layout.fillWidth: true
            from: 5
            to: 100
            stepSize: 5
            badge: true
            badgeText: Math.round(value) + "%"
            enabled: !card.host.readOnly
            value: card.level
            onPressedChanged: if (!pressed) card.pick(card.choice, value)
        }
        Rectangle {
            visible: card.choice === 3
            Layout.preferredWidth: 26
            Layout.preferredHeight: 26
            radius: 13
            color: "#" + card.colour
            border.color: Theme.border
            TapHandler {
                enabled: !card.host.readOnly
                onTapped: {
                    batteryColour.selectedColor = "#" + card.colour;
                    batteryColour.open();
                }
            }
        }
    }
    ColorDialog {
        id: batteryColour
        title: i18n("Colour on battery")
        onAccepted: {
            card.colour = selectedColor.toString().replace("#", "").slice(-6);
            card.pick(3, card.level);
        }
    }
}
