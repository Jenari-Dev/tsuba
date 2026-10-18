import QtQuick
import QtQuick.Layouts

// Lighting: hardware quick effects, or per-key painting when a device image pack
// with an LED map is available (Synapse's Quick / Advanced split).
Item {
    id: page

    required property var backend
    required property string pid
    required property var st
    // Database entry of the model (GetDeviceSupport); decides whether per-LED painting is offered.
    property var support: ({})
    property bool readOnly: backend ? backend.readonly : false
    property bool locked: false
    signal changed()
    signal launchStudio()

    readonly property var pack: {
        const p = backend.device_pack(pid);
        return p === "" ? null : JSON.parse(p);
    }
    readonly property bool perKey: !!(pack && pack.views && pack.views.top && (pack.views.top.hotspots || []).some(h => h.led) && DeviceKinds.perLed(support, pack.kind))
    // Advanced effects: the studio edits keyboard and mouse together, so the mouse can reach
    // it too when a keyboard with an image pack is present.
    readonly property bool studioReachable: perKey || (st.kind === "mouse" && JSON.parse(backend.devices || "[]").some(d => d.kind === "keyboard" && backend.device_pack(d.pid) !== ""))
    property int mode: backend.env_var("TSUBA_LIGHTMODE") === "1" ? 1 : 0

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        SynPillTabs {
            visible: page.perKey
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: 20
            model: ["Quick effects", DeviceKinds.family(support.kind || (pack ? pack.kind : "")) === "keyboard" ? "Per-key" : "Per-LED"]
            currentIndex: page.mode
            onActivated: index => page.mode = index
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: page.perKey ? page.mode : 0

            LightingTab {
                backend: page.backend
                pid: page.pid
                st: page.st
                readOnly: page.readOnly
                locked: page.locked
                studioAvailable: page.studioReachable
                onChanged: page.changed()
                onLaunchStudio: page.launchStudio()
            }
            Loader {
                active: page.perKey
                sourceComponent: LedEditor {
                    backend: page.backend
                    pid: page.pid
                    pack: page.pack
                    readOnly: page.readOnly || page.locked
                    Component.onCompleted: {
                        if (page.backend.env_var("TSUBA_DEMO") === "1") {   // headless UI checks
                            selectGroup(groups[1]);
                            const m = {};
                            leds.filter(groups[1].test).forEach(h => m[h.led[0] + "," + h.led[1]] = "ff2020");
                            leds.filter(groups[2].test).forEach(h => m[h.led[0] + "," + h.led[1]] = "2080ff");
                            leds.filter(groups[7].test).forEach(h => m[h.led[0] + "," + h.led[1]] = "44d62c");
                            leds.filter(groups[8].test).forEach(h => m[h.led[0] + "," + h.led[1]] = "ffaa00");
                            leds.filter(groups[9].test).forEach(h => m[h.led[0] + "," + h.led[1]] = "a000ff");
                            selectGroup(groups[9]);
                            colours = m;
                        }
                    }
                }
            }
        }
    }
}
