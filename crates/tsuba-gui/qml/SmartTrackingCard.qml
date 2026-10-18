import QtQuick
import QtQuick.Layouts

// Lift-off distance (Smart Tracking) for mice whose lift-off sensor answered the feature probe.
// The mouse can't report the setting back: the card shows what Tsuba last sent.
SynCard {
    id: card

    required property var host
    // i18n() comes from KLocalizedContext in the app; the headless QML tests run without it.
    function tr(translated, plain) {
        return typeof i18n === "function" ? translated() : plain;
    }
    readonly property var st: host.feat.smart_tracking || ({ asymmetric: false, distance_mm: 2, lift_mm: 3, landing_mm: 2 })
    readonly property bool saved: !!host.feat.smart_tracking_saved

    function send(change) {
        const want = Object.assign({}, st, change);
        if (want.asymmetric && want.landing_mm >= want.lift_mm) {
            want.landing_mm = want.lift_mm - 1;
        }
        host.setFeat({ smart_tracking: want });
    }

    objectName: "smartTrackingCard"
    title: i18n("Lift-off distance")
    help: i18n("How high you can lift the mouse before it stops tracking. Lower distances stop the pointer from drifting when you reposition the mouse. Asymmetric sets separate distances for lifting off and landing again. The mouse adjusts to your surface by itself (Smart Tracking).")
    description: i18n("The height at which the sensor stops tracking when you lift the mouse.")

    SynPillTabs {
        objectName: "trackingMode"
        enabled: !card.host.readOnly
        model: [i18n("Symmetric"), i18n("Asymmetric")]
        currentIndex: !card.saved ? -1 : (card.st.asymmetric ? 1 : 0)
        onActivated: i => card.send({ asymmetric: i === 1 })
    }
    RowLayout {
        visible: !card.st.asymmetric
        spacing: 14
        Text {
            text: i18n("Lift-off and landing")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynSquareButtons {
            objectName: "trackingDistance"
            enabled: !card.host.readOnly
            buttonSize: 56
            Layout.preferredWidth: 3 * 66
            model: ["1 mm", "2 mm", "3 mm"]
            currentIndex: card.saved ? card.st.distance_mm - 1 : -1
            onActivated: i => card.send({ distance_mm: i + 1 })
        }
    }
    RowLayout {
        visible: card.st.asymmetric
        spacing: 14
        Text {
            text: i18n("Lift-off")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynSquareButtons {
            objectName: "liftDistance"
            enabled: !card.host.readOnly
            buttonSize: 56
            Layout.preferredWidth: 2 * 66
            model: ["2 mm", "3 mm"]
            currentIndex: card.saved ? card.st.lift_mm - 2 : -1
            onActivated: i => card.send({ lift_mm: i + 2 })
        }
        Text {
            text: i18n("Landing")
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
        SynSquareButtons {
            objectName: "landingDistance"
            enabled: !card.host.readOnly
            buttonSize: 56
            Layout.preferredWidth: 2 * 66
            model: ["1 mm", "2 mm"]
            currentIndex: card.saved ? card.st.landing_mm - 1 : -1
            onActivated: i => {
                if (i + 1 < card.st.lift_mm) {
                    card.send({ landing_mm: i + 1 });
                } else {
                    card.host.result = card.tr(() => i18n("Failed: landing must be lower than lift-off"), "Failed: landing must be lower than lift-off");
                }
            }
        }
    }
    Text {
        Layout.fillWidth: true
        text: card.saved ? i18n("The mouse can't report this setting back; Tsuba shows the value it last sent.") : i18n("Not set from Tsuba yet — the mouse keeps its own setting until you pick one.")
        color: Theme.textMuted
        font.family: Theme.font
        font.pixelSize: 12
        wrapMode: Text.Wrap
    }
}
