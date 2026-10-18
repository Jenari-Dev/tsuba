import QtQuick
import QtQuick.Layouts

// Shown on Home while no device is listed: works out why (service not running, no permission to
// open the device, another driver holding it, unsupported model, nothing plugged in) from the
// environment report (`tsubad --check-env`) and offers the step that fixes it.
Rectangle {
    id: guide

    // Parsed environment report (empty object until the first check finishes).
    property var report: ({})
    property bool connected: false
    // AppBackend-like object: start_service(), copy_text(); null in layout tests.
    property var app: null
    property string note: ""
    signal recheck()
    signal openSupported()
    signal openUrl(string url)
    signal exportDiagnostics()

    readonly property string state: {
        if (!connected) {
            return report.service && report.service.unit_installed === false ? "no_unit" : "stopped";
        }
        const d = report.diagnosis || "unknown";
        return d === "ok" ? "listed_none" : d;
    }
    readonly property bool ruleInstalled: !!(report.udev && report.udev.rule_installed)
    readonly property string ruleCommand: "sudo install -Dm644 contrib/70-tsuba.rules /etc/udev/rules.d/70-tsuba.rules && sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw"
    readonly property var unsupportedNames: (report.usb || []).filter(u => !u.name).map(u => u.pid)

    color: Theme.card
    radius: 5
    implicitHeight: col.implicitHeight + 40

    readonly property string title: {
        switch (state) {
        case "no_unit": return i18n("The Tsuba service isn't installed");
        case "stopped": return i18n("The Tsuba service isn't running");
        case "permission": return i18n("Tsuba isn't allowed to open your device");
        case "no_hidraw": return i18n("Another driver is holding your device");
        case "unsupported_only": return i18n("This Razer device isn't supported yet");
        case "no_device": return i18n("No Razer device found");
        case "listed_none": return i18n("The system sees your device, but Tsuba lists none");
        default: return i18n("Checking your setup…");
        }
    }
    readonly property string body: {
        switch (state) {
        case "no_unit":
            return i18n("Tsuba talks to your devices through a small background service, tsubad. Install the Tsuba package (or follow the install guide), then start the service.");
        case "stopped":
            return i18n("Tsuba talks to your devices through a small background service, tsubad. Start it now, or let it start at every login.");
        case "permission":
            return ruleInstalled
                ? i18n("The access rule is installed, but it only applies to devices plugged in after it was loaded. Unplug and replug the device once, or log out and in again.")
                : i18n("Your user needs permission to use Razer devices. Install Tsuba's udev rule with this command, then unplug and replug the device once:");
        case "no_hidraw":
            return i18n("A Razer device is connected but the system gave Tsuba no way to talk to it. This usually means OpenRazer's driver has taken it over. The hand-over command below gives it back (undo with tsuba-handover-restore).");
        case "unsupported_only":
            return i18n("A Razer device is connected, but this model isn't in Tsuba's list yet. You can see which models are supported, and send diagnostics if you'd like it added.");
        case "no_device":
            return i18n("Plug in a Razer keyboard or mouse with its USB cable, or put the wireless dongle in a USB port (try another port if nothing happens). Tsuba talks to devices over USB.");
        case "listed_none":
            return i18n("The device is visible to the system, but the service lists none. Restarting the service usually fixes this; if it doesn't, export diagnostics.");
        default:
            return "";
        }
    }

    ColumnLayout {
        id: col
        x: 20
        y: 20
        width: parent.width - 40
        spacing: 12

        Text {
            objectName: "guideTitle"
            Layout.fillWidth: true
            text: guide.title
            color: Theme.accent
            font.family: Theme.titleFont
            font.pixelSize: 16
            wrapMode: Text.Wrap
        }
        Text {
            objectName: "guideBody"
            Layout.fillWidth: true
            text: guide.body
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
            wrapMode: Text.Wrap
        }
        // A command to copy.
        Rectangle {
            visible: guide.state === "permission" && !guide.ruleInstalled || guide.state === "no_hidraw"
            Layout.fillWidth: true
            implicitHeight: cmd.implicitHeight + 20
            color: Theme.bg
            radius: 3
            border.color: Theme.borderSoft
            Text {
                id: cmd
                objectName: "guideCommand"
                x: 12
                y: 10
                width: parent.width - 24
                text: guide.state === "no_hidraw" ? "tsuba-handover" : guide.ruleCommand
                color: Theme.text
                font.family: "monospace"
                font.pixelSize: 12
                wrapMode: Text.WrapAnywhere
            }
        }
        Text {
            visible: guide.state === "unsupported_only" && guide.unsupportedNames.length > 0
            Layout.fillWidth: true
            text: i18n("Product id: %1", guide.unsupportedNames.join(", "))
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        RowLayout {
            spacing: 10
            SynButton {
                objectName: "guideStart"
                visible: guide.state === "stopped" || guide.state === "listed_none"
                primary: true
                text: guide.state === "stopped" ? i18n("Start service") : i18n("Restart service")
                onClicked: guide.app && guide.app.start_service()
            }
            SynButton {
                objectName: "guideCopy"
                visible: guide.state === "permission" && !guide.ruleInstalled || guide.state === "no_hidraw"
                primary: true
                text: i18n("Copy command")
                onClicked: {
                    if (guide.app) {
                        guide.app.copy_text(cmd.text);
                    }
                    guide.note = i18n("Copied. Paste it in a terminal.");
                }
            }
            SynButton {
                visible: guide.state === "no_unit"
                primary: true
                text: i18n("Install guide")
                onClicked: guide.openUrl(Theme.docsUrl("INSTALL.md"))
            }
            SynButton {
                visible: guide.state === "unsupported_only"
                primary: true
                text: i18n("Supported devices")
                onClicked: guide.openSupported()
            }
            SynButton {
                objectName: "guideRecheck"
                text: i18n("Check again")
                onClicked: {
                    guide.note = "";
                    guide.recheck();
                }
            }
            SynButton {
                text: i18n("Export diagnostics")
                onClicked: guide.exportDiagnostics()
            }
            Item {
                Layout.fillWidth: true
            }
        }
        Text {
            visible: guide.note !== ""
            text: guide.note
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.small
        }
        Text {
            Layout.fillWidth: true
            text: i18n("Still stuck? The troubleshooting page lists the usual causes.")
            color: Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.small
            TapHandler {
                onTapped: guide.openUrl(Theme.docsUrl("TROUBLESHOOTING.md"))
            }
            HoverHandler {
                cursorShape: Qt.PointingHandCursor
            }
        }
    }
}
