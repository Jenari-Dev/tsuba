import QtQuick
import QtQuick.Layouts

// Shown on the device page for a model Tsuba hasn't been tested with: settings are read-only
// until the user opts in (after reading what that means). Once allowed, it stays as a slim
// strip asking for feedback, with a button that copies a plain-text report of the device.
Rectangle {
    id: bar

    required property var backend
    required property string pid
    property var support: ({})
    property var st: ({})
    // Names of the settings groups that answered when the page loaded.
    property var responded: []
    property bool confirming: false
    property string note: ""
    signal updated(var support)

    readonly property bool untested: support.tested === false && support.supported !== false
    readonly property bool allowed: !!support.allow_untested_writes
    readonly property string deviceName: support.name || st.name || pid

    visible: untested
    color: Theme.card
    implicitHeight: untested ? col.implicitHeight + (allowed ? 12 : 20) : 0

    function reportText() {
        const f = support.features || {};
        const feats = Object.keys(f).filter(k => f[k]).join(", ");
        return [
            "Tsuba device report",
            "Name: " + deviceName,
            "Product ID: " + pid,
            "Kind: " + (support.kind || st.kind || "?") + (support.layout ? " (" + support.layout + ")" : ""),
            "Tested by Tsuba: " + (support.tested ? "yes" : "no") + " · supported: " + (support.supported === false ? "no" : "yes")
                + " · changes allowed: " + (allowed ? "yes" : "no"),
            "Firmware: " + (st.firmware || "unknown"),
            "Mode: " + (st.mode || "unknown"),
            "Features in the database: " + (feats || "none listed"),
            "Settings that answered: " + (responded.length ? responded.join(", ") : "none"),
            "What worked / what didn't: "
        ].join("\n");
    }
    function copyReport() {
        backend.copy_text(reportText());
        note = "Report copied to the clipboard. Paste it in a GitHub issue.";
    }
    // The toggle: switching on asks first; switching off needs no question.
    function toggle(on) {
        note = "";
        if (on) {
            confirming = true;
        } else {
            confirming = false;
            apply(false);
        }
    }
    function apply(allow) {
        const r = backend.set_allow_untested_writes(pid, allow);
        if (r === "") {
            note = "Failed: " + backend.status;
            return;
        }
        confirming = false;
        updated(JSON.parse(r));
    }

    Component.onCompleted: {
        if (typeof backend.env_var === "function" && backend.env_var("TSUBA_BANNER_CONFIRM") === "1") {   // headless UI check
            confirming = true;
        }
    }

    Rectangle {
        width: 4
        height: parent.height
        color: Theme.warn
    }

    ColumnLayout {
        id: col
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 20
        anchors.rightMargin: 20
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Rectangle {
                Layout.preferredWidth: 20
                Layout.preferredHeight: 20
                radius: 10
                color: "transparent"
                border.color: Theme.warn
                border.width: 2
                Text {
                    anchors.centerIn: parent
                    text: "!"
                    color: Theme.warn
                    font.bold: true
                    font.pixelSize: 13
                }
            }
            Text {
                objectName: "bannerText"
                Layout.fillWidth: true
                text: bar.allowed ? "Changes allowed — please report how it works." : bar.deviceName + " hasn't been tested with Tsuba yet. Settings are shown read-only."
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: bar.allowed ? Theme.small : Theme.fontSize
                wrapMode: Text.WordWrap
            }
            SynButton {
                objectName: "reportButton"
                visible: bar.allowed
                text: "Report this device"
                implicitWidth: 150
                onClicked: bar.copyReport()
            }
            Text {
                text: "Allow changes"
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
            }
            SynToggle {
                objectName: "allowToggle"
                checked: bar.allowed
                onToggled: on => bar.toggle(on)
            }
        }

        // Plain-words confirmation, in place under the strip.
        Rectangle {
            objectName: "confirmBox"
            visible: bar.confirming
            Layout.fillWidth: true
            implicitHeight: conf.implicitHeight + 28
            color: Theme.bg
            border.color: Theme.warn
            border.width: 1
            ColumnLayout {
                id: conf
                x: 14
                y: 14
                width: parent.width - 28
                spacing: 10
                Text {
                    text: "Allow changes on " + bar.deviceName + "?"
                    color: Theme.textBright
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    font.bold: true
                }
                Text {
                    Layout.fillWidth: true
                    text: "Tsuba's information about this model comes from community sources and hasn't been checked on real hardware. Most settings use the same commands as on tested devices, but a wrong value could leave the device set up oddly until you reset it. Nothing permanent, such as firmware, is ever changed."
                    color: Theme.text
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.WordWrap
                }
                RowLayout {
                    spacing: 10
                    SynButton {
                        objectName: "confirmCancel"
                        text: "Cancel"
                        onClicked: bar.confirming = false
                    }
                    SynButton {
                        objectName: "confirmAllow"
                        text: "Allow changes"
                        primary: true
                        implicitWidth: 140
                        onClicked: bar.apply(true)
                    }
                }
            }
        }

        Text {
            objectName: "bannerNote"
            visible: bar.note !== ""
            Layout.fillWidth: true
            text: bar.note
            color: bar.note.indexOf("Failed") === 0 ? Theme.warn : Theme.accent
            font.family: Theme.font
            font.pixelSize: Theme.small
            wrapMode: Text.WordWrap
        }
        Text {
            visible: bar.allowed && bar.note === ""
            Layout.fillWidth: true
            text: "The report goes to the clipboard — paste it in a GitHub issue."
            color: Theme.textMuted
            font.family: Theme.font
            font.pixelSize: 11
        }
    }
}
