import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

QQC2.ScrollView {
    id: tab

    required property var backend
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    // Untested and not allowed: the whole page is greyed out.
    property bool locked: false
    required property string pid
    required property var st
    signal changed()
    signal launchStudio()
    // Advanced effects (Lighting Studio) exist only for per-LED devices.
    property bool studioAvailable: false
    property bool advanced: false
    // Brightness on/off toggle: remembers the level to come back to.
    property real lastBrightness: 100
    // Lighting Studio profiles for the Advanced mode.
    property var studioProfiles: []
    property int profileIdx: 0
    property string studioStatus: ""
    function loadStudioProfiles() {
        if (typeof backend.chroma_profiles !== "function") {
            return;
        }
        const j = JSON.parse(backend.chroma_profiles() || "{\"profiles\": []}");
        studioProfiles = j.profiles.map(p => p.name);
        profileIdx = Math.max(0, studioProfiles.indexOf(j.active || ""));
    }
    onAdvancedChanged: if (advanced) loadStudioProfiles()

    // Mouse zones (logo, scroll wheel, underglow): 0 = every zone, n = st.lighting[n - 1].
    property int zone: 0
    readonly property var zones: st.lighting && st.lighting.length > 1 ? st.lighting : []
    onZoneChanged: Qt.callLater(sync)
    readonly property var region: st.lighting && st.lighting.length ? st.lighting[Math.max(0, Math.min(zone, st.lighting.length) - 1)] : null
    // Quick effects (and brightness) for every attached device at once.
    property bool allDevices: false
    property bool brightnessAll: false
    readonly property bool multiDevice: {
        try {
            return JSON.parse(backend.devices || "[]").length > 1;
        } catch (e) {
            return false;
        }
    }

    // Firmware features from tsubad's read-only probe (status lights, battery info); `devb` is
    // the daemon's Device interface (null in headless tests unless a test sets a fake).
    property int profile: 1
    property string result: ""
    property var devb: null
    property var feat: ({})
    // i18n() comes from KLocalizedContext in the app; the headless QML tests run without it.
    function tr(translated, plain) {
        return typeof i18n === "function" ? translated() : plain;
    }
    function loadFeatures() {
        if (!devb) {
            return;
        }
        const s = devb.features(pid, profile);
        feat = s === "" ? ({}) : JSON.parse(s);
    }
    function setFeat(change) {
        const r = devb.set_features(pid, JSON.stringify(Object.assign({ profile: profile }, change)));
        result = r === "" ? tr(() => i18n("Failed: %1", devb.status), "Failed: " + devb.status) : r.split("\n").join(" · ");
        loadFeatures();
    }
    function makeDevb() {
        try {
            return Qt.createQmlObject("import io.github.jenaridev.tsuba\nDeviceBackend {}", tab, "DeviceBackend");
        } catch (e) {
            return null;
        }
    }
    readonly property var effectTypes: [
        { type: "static", label: "Static", fw: "Static", colours: 1 },
        { type: "breathing", label: "Breathing", fw: "Breathing", colours: 2 },
        { type: "spectrum", label: "Spectrum cycling", fw: "Spectrum", colours: 0 },
        { type: "wave", label: "Wave", fw: "Wave", colours: 0 },
        { type: "reactive", label: "Reactive", fw: "Reactive", colours: 1 },
        { type: "starlight", label: "Starlight", fw: "Starlight", colours: 2 },
        { type: "ripple", label: "Ripple", fw: "Ripple", colours: 1 },
        { type: "fire", label: "Fire", soft: true, colours: 0 },
        { type: "wheel", label: "Wheel", soft: true, colours: 0 },
        { type: "audio_meter", label: "Audio Meter", soft: true, colours: 0 },
        { type: "ambient", label: "Ambient (screen)", soft: true, colours: 0 },
        { type: "tidal", label: "Tidal", soft: true, colours: 0 },
        { type: "battery", label: "Battery level", soft: true, colours: 0, mouseOnly: true },
        { type: "off", label: "None (lighting off)", fw: "Off", colours: 0 }
    ]
    // Effects the firmware lacks (Fire, Wheel; Wave/Starlight on the mouse) are drawn by tsubad.
    readonly property bool isMouse: st.kind === "mouse"
    function softOnHere(e) {
        return !!e.soft || (isMouse && (e.type === "wave" || e.type === "starlight"));
    }
    // A single zone takes the device's built-in effects only.
    readonly property var choices: region ? effectTypes.filter(e => (!e.mouseOnly || isMouse) && (zone === 0 || !softOnHere(e)) && (softOnHere(e) || region.supported.indexOf(e.fw) >= 0)) : []
    // Audio Meter options
    property real meterGain: 1.0
    property real meterBoost: 1.0
    property bool meterFlow: false
    property int effectIndex: 0
    readonly property var effect: choices[effectIndex] || null
    property var colours: ["44d62c", "0080ff"]
    property int colourCount: 1
    property int direction: 1
    property int speedIdx: 1   // short/slow, medium, long/fast
    readonly property bool hasSpeed: !!effect && ["wave", "reactive", "starlight", "ripple", "fire", "wheel", "tidal"].indexOf(effect.type) >= 0
    property int editing: 0
    // Ambient: part of the screen [left, top, right, bottom].
    property var ambientArea: [0, 0, 1, 1]
    property real ambientBoost: 1.3
    readonly property var areaPresets: [
        ["Full screen", [0, 0, 1, 1]], ["Top half", [0, 0, 1, 0.5]], ["Bottom half", [0, 0.5, 1, 1]],
        ["Left half", [0, 0, 0.5, 1]], ["Right half", [0.5, 0, 1, 1]], ["Centre", [0.25, 0.25, 0.75, 0.75]]
    ]

    contentWidth: availableWidth

    function sync() {
        if (!region) {
            return;
        }
        // The saved lighting is the truth: the firmware reports software effects (drawn as
        // custom frames) as whatever stored effect it last had, which isn't what was picked.
        const cfg = JSON.parse(backend.config(pid) || "{}");
        let fx = cfg.lighting && cfg.lighting.effect ? cfg.lighting.effect : null;
        if (zone > 0) {
            const z = cfg.zones ? cfg.zones[String(region.id)] : null;
            fx = z && z.effect ? z.effect : null;
        }
        if (fx && fx.type === "studio" && fx.studio && fx.studio.layers && fx.studio.layers.length === 1) {
            fx = fx.studio.layers[0].effect;
        }
        let i = fx ? choices.findIndex(e => e.type === fx.type) : -1;
        if (i < 0) {
            i = choices.findIndex(e => e.fw === region.effect);
        }
        effectIndex = Math.max(0, i);
        if (fx && fx.type === "ambient") {
            ambientArea = fx.area || [0, 0, 1, 1];
            ambientBoost = fx.boost || 1.3;
        } else if (fx && fx.type === "audio_meter") {
            meterGain = fx.gain || 1.0;
            meterBoost = fx.boost || 1.0;
            meterFlow = !!fx.flow;
        }
        if (region.colours.length > 0) {
            colours = region.colours.concat(colours.slice(region.colours.length)).slice(0, 2);
            colourCount = region.colours.length;
        }
        brightness.value = region.brightness;
    }
    onRegionChanged: Qt.callLater(sync)

    // Software effect as a one-layer Lighting Studio profile over every LED.
    function softEffect() {
        const duration = ["short", "medium", "long"][speedIdx];
        const cs = colours.slice(0, Math.min(colourCount, effect.colours));
        let fx;
        switch (effect.type) {
        case "fire":
            fx = { type: "fire", duration: duration };
            break;
        case "wheel":
            fx = { type: "wheel", direction: direction === 2 ? "counter_clockwise" : "clockwise", duration: duration };
            break;
        case "ambient":
            fx = { type: "ambient", boost: ambientBoost, area: ambientArea };
            break;
        case "audio_meter":
            fx = { type: "audio_meter", gain: meterGain, boost: meterBoost, flow: meterFlow };
            if (colourCount > 0 && !meterFlow) {
                fx.colour = colours[0];
            }
            break;
        case "tidal":
            fx = { type: "tidal", inward: direction === 2, duration: duration, colours: [] };
            break;
        case "battery":
            fx = { type: "battery" };
            break;
        case "wave":
            fx = { type: "wave", direction: direction === 2 ? "right_to_left" : "left_to_right", duration: ["long", "medium", "short"][speedIdx], colours: [] };
            break;
        default:
            fx = { type: "starlight", colours: cs, duration: duration };
        }
        return { type: "studio", studio: { layers: [{ name: effect.label, enabled: true, leds: [], effect: fx }] } };
    }

    // Lighting power options (idle / display off).
    property var power: null
    function loadPower() {
        const s = backend.lighting_power(pid);
        power = s === "" ? null : JSON.parse(s);
    }
    function setPower(change) {
        const p = Object.assign({}, power, change);
        if (backend.set_lighting_power(pid, JSON.stringify(p))) {
            power = p;
        }
    }
    Component.onCompleted: {
        if (!devb) {
            devb = makeDevb();
        }
        loadFeatures();
        loadPower();
        if (typeof backend.env_var === "function" && backend.env_var("TSUBA_ADVANCED") === "1") {   // headless UI check
            advanced = true;
        }
        if (advanced) {
            loadStudioProfiles();
        }
    }

    function apply() {
        if (zone > 0 && devb && !softOnHere(effect)) {
            const ze = quickEffect();
            result = devb.set_zone_effect(pid, region.id, JSON.stringify(ze), Math.round(brightness.value)) ? region.name + ": " + effect.label : tr(() => i18n("Failed: %1", devb.status), "Failed: " + devb.status);
            changed();
            return;
        }
        if (allDevices && multiDevice && devb && !softOnHere(effect)) {
            const r = devb.set_effect_all(JSON.stringify(quickEffect()), Math.round(brightness.value));
            result = r === "" ? tr(() => i18n("Failed: %1", devb.status), "Failed: " + devb.status) : tr(() => i18n("Applied to every device"), "Applied to every device");
            changed();
            return;
        }
        if (devb && zones.length > 0 && !softOnHere(effect)) {
            devb.clear_zones(pid);   // the whole device gets this effect, also at the next login
        }
        if (softOnHere(effect)) {
            const j = JSON.stringify(softEffect());
            if (backend.set_effect(pid, j, Math.round(brightness.value))) {
                backend.save_lighting(pid, j, Math.round(brightness.value));   // restored at login
                changed();
            }
            return;
        }
        const e = quickEffect();
        if (backend.set_effect(pid, JSON.stringify(e), Math.round(brightness.value))) {
            backend.save_lighting(pid, JSON.stringify(e), Math.round(brightness.value));   // restored at login
            changed();
        }
    }

    // The picked built-in effect in the daemon's config JSON form.
    function quickEffect() {
        const e = { type: effect.type };
        const cs = colours.slice(0, Math.min(colourCount, effect.colours));
        if (effect.type === "static" || effect.type === "reactive") {
            e.colour = colours[0];
        } else if (effect.colours === 2 || effect.type === "ripple") {
            e.colours = cs;
        } else if (effect.type === "wave") {
            e.direction = direction;
        }
        // Synapse: wave rate 75/40/15 (slow/medium/fast); reactive/starlight duration 1–3.
        // Ripple is a frame period like Wave (1–3 made it flash instantly): short/medium/long.
        if (effect.type === "wave") {
            e.speed = [75, 40, 15][speedIdx];
        } else if (effect.type === "ripple") {
            e.speed = [15, 40, 75][speedIdx];
        } else if (hasSpeed) {
            e.speed = speedIdx + 1;
        }
        return e;
    }

    Timer {
        id: brightnessApply
        interval: 120
        onTriggered: {
            const v = Math.round(brightness.value);
            if (tab.zone > 0 && tab.devb) {
                tab.devb.set_zone_brightness(tab.pid, tab.region.id, v);
            } else if (tab.brightnessAll && tab.multiDevice && tab.devb) {
                tab.devb.set_brightness_all(v);
            } else {
                tab.backend.set_brightness(tab.pid, v);
            }
        }
    }

    ColorDialog {
        id: picker
        title: "Choose colour"
        onAccepted: {
            const c = tab.colours.slice();
            c[tab.editing] = selectedColor.toString().replace("#", "").slice(-6);
            tab.colours = c;
        }
    }

    component Label14: Text {
        color: Theme.text
        font.family: Theme.font
        font.pixelSize: Theme.fontSize
    }
    ColumnLayout {
        width: tab.availableWidth
        spacing: 0
        enabled: !tab.locked

        RowLayout {
            Layout.fillWidth: true
            Layout.maximumWidth: 1180
            Layout.alignment: Qt.AlignHCenter | Qt.AlignTop
            Layout.margins: 24
            spacing: 24

            // ---- Left column: brightness, power
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 24

                SynCard {
                    title: "Brightness"
                    help: "Overall brightness of this device's lighting. Dragging to 0 turns the lighting off. Brightness is stored with the on-board profile."
                    headerContent: SynToggle {
                        objectName: "brightnessToggle"
                        checked: brightness.value > 0
                        onToggled: on => {
                            if (on) {
                                brightness.value = tab.lastBrightness > 0 ? tab.lastBrightness : 100;
                            } else {
                                if (brightness.value > 0) {
                                    tab.lastBrightness = brightness.value;
                                }
                                brightness.value = 0;
                            }
                            brightnessApply.restart();
                        }
                    }

                    SynSlider {
                        id: brightness
                        objectName: "brightnessSlider"
                        Layout.fillWidth: true
                        badge: true
                        rangeLabels: true
                        // Live, like Synapse (coalesced so a drag isn't a flood of writes).
                        onMoved: {
                            if (value > 0) {
                                tab.lastBrightness = value;
                            }
                            brightnessApply.restart();
                        }
                        from: 0
                        to: 100
                        stepSize: 1
                        value: 100
                    }
                    SynCheckBox {
                        objectName: "brightnessAll"
                        visible: tab.multiDevice && tab.devb !== null && tab.zone === 0
                        text: i18n("Same brightness on every device")
                        checked: tab.brightnessAll
                        onToggled: {
                            tab.brightnessAll = checked;
                            if (checked) {
                                brightnessApply.restart();
                            }
                        }
                    }
                }

                SynCard {
                    visible: tab.power !== null
                    title: "Switch off lighting"
                    help: "Turns the lighting off after the screen has been off for the chosen time, and back on when you return. Match this to your desktop's own power settings so they work together."

                    RowLayout {
                        spacing: 12
                        SynCheckBox {
                            text: "When display is turned off, after"
                            checked: !!tab.power && tab.power.display_off
                            onToggled: tab.setPower({ display_off: checked })
                        }
                        QQC2.SpinBox {
                            from: 1
                            to: 240
                            editable: true
                            value: tab.power ? tab.power.display_off_minutes : 10
                            onValueModified: tab.setPower({ display_off_minutes: value })
                        }
                        Text {
                            text: "min (match KDE's Power settings)"
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                    }
                    RowLayout {
                        spacing: 12
                        SynCheckBox {
                            id: idleBox
                            text: "When idle for (minutes)"
                            checked: !!tab.power && tab.power.idle_minutes > 0
                            onToggled: tab.setPower({ idle_minutes: checked ? idleMins.value : 0 })
                        }
                        QQC2.SpinBox {
                            id: idleMins
                            from: 1
                            to: 240
                            editable: true
                            value: tab.power && tab.power.idle_minutes > 0 ? tab.power.idle_minutes : 5
                            onValueModified: if (idleBox.checked) tab.setPower({ idle_minutes: value })
                        }
                    }
                    SynCheckBox {
                        objectName: "offWhenLocked"
                        text: i18n("When the session is locked")
                        checked: !!tab.power && !!tab.power.on_lock
                        onToggled: tab.setPower({ on_lock: checked })
                    }
                    Text {
                        Layout.fillWidth: true
                        text: i18n("Lighting comes back with the next key press or mouse movement, or when you unlock.")
                        color: Theme.textMuted
                        font.family: Theme.font
                        font.pixelSize: 12
                        wrapMode: Text.Wrap
                    }
                }

                // Status lights stored in the device (profile colour, indicators, power-on animation)
                DeviceLedsCard {
                    host: tab
                }
                // Wireless: different lighting on battery
                BatteryLightingCard {
                    host: tab
                }
                Text {
                    objectName: "lightingResult"
                    visible: tab.result !== ""
                    Layout.fillWidth: true
                    text: tab.result
                    color: tab.result.startsWith(tab.tr(() => i18n("Failed"), "Failed")) ? Theme.warn : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                }
            }

            // ---- Right column: effects
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 24

                SynCard {
                    title: "Effects"
                    help: "Quick Effects are built into the device and stay when Tsuba is closed. Advanced Effects come from a Lighting Studio profile and play on all your devices while Tsuba runs; they are not saved on the device."

                    SynPillTabs {
                        visible: tab.studioAvailable
                        model: ["Quick Effects", "Advanced Effects"]
                        currentIndex: tab.advanced ? 1 : 0
                        onActivated: i => tab.advanced = i === 1
                    }

                    // ---- Advanced: Lighting Studio
                    ColumnLayout {
                        visible: tab.advanced && tab.studioAvailable
                        Layout.fillWidth: true
                        spacing: 14
                        Text {
                            Layout.fillWidth: true
                            text: "Advanced effects come from a Lighting Studio profile (the counterpart of Chroma Studio effects in Razer's software) and play across all your devices; they are not saved to the device profile."
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                            wrapMode: Text.Wrap
                        }
                        Label14 {
                            text: "Lighting Studio profile"
                        }
                        RowLayout {
                            spacing: 12
                            SynDropdown {
                                id: profileBox
                                objectName: "studioProfileBox"
                                Layout.preferredWidth: 300
                                model: tab.studioProfiles
                                onActivated: i => tab.profileIdx = i
                                Binding {
                                    target: profileBox
                                    property: "currentIndex"
                                    value: tab.profileIdx
                                }
                            }
                            SynButton {
                                objectName: "studioApply"
                                text: "Apply"
                                primary: true
                                enabled: !tab.readOnly && tab.studioProfiles.length > 0
                                onClicked: {
                                    const n = tab.studioProfiles[tab.profileIdx];
                                    tab.studioStatus = tab.backend.apply_chroma_profile(n) ? "Applied " + n : "Failed: " + tab.backend.status;
                                }
                            }
                            Text {
                                text: tab.studioStatus
                                color: tab.studioStatus.startsWith("Failed") ? Theme.warn : Theme.accent
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                        }
                        Text {
                            visible: tab.studioProfiles.length === 0
                            text: "No profiles yet. Open Lighting Studio to create one."
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                        }
                        Rectangle {
                            Layout.preferredWidth: 270
                            Layout.preferredHeight: 52
                            color: launchHover.hovered ? "#2d2d2d" : "transparent"
                            border.color: "#d3d3d3"
                            border.width: 1
                            Row {
                                anchors.centerIn: parent
                                spacing: 12
                                Rectangle {
                                    width: 28
                                    height: 28
                                    radius: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    color: "transparent"
                                    border.width: 4
                                    border.color: Theme.accent
                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: 20
                                        height: 20
                                        radius: 10
                                        color: "transparent"
                                        border.width: 3
                                        border.color: "#2c8fd6"
                                    }
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "OPEN LIGHTING STUDIO"
                                    color: Theme.textBright
                                    font.family: Theme.font
                                    font.pixelSize: Theme.fontSize
                                }
                            }
                            HoverHandler {
                                id: launchHover
                                cursorShape: Qt.PointingHandCursor
                            }
                            TapHandler {
                                onTapped: tab.launchStudio()
                            }
                        }
                    }

                    // ---- Quick effects
                    ColumnLayout {
                        visible: !(tab.advanced && tab.studioAvailable)
                        Layout.fillWidth: true
                        spacing: 14

                        Text {
                            Layout.fillWidth: true
                            text: "Quick effects are applied to this device and restored when you log in."
                            color: Theme.text
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                            wrapMode: Text.Wrap
                        }
                        RowLayout {
                            visible: tab.zones.length > 0
                            spacing: 12
                            Label14 {
                                text: i18n("Zone")
                            }
                            SynDropdown {
                                id: zoneBox
                                objectName: "zoneBox"
                                Layout.preferredWidth: 220
                                model: [i18n("All zones")].concat(tab.zones.map(r => r.name.charAt(0).toUpperCase() + r.name.slice(1)))
                                onActivated: i => tab.zone = i
                                Binding {
                                    target: zoneBox
                                    property: "currentIndex"
                                    value: tab.zone
                                }
                            }
                        }
                        Label14 {
                            text: "Effect"
                        }
                        SynDropdown {
                            id: effectBox
                            Layout.preferredWidth: 300
                            model: tab.choices
                            textRole: "label"
                            onActivated: i => tab.effectIndex = i
                            // Keep the shown item following the data (the control would otherwise
                            // break a plain currentIndex binding on the first pick).
                            Binding {
                                target: effectBox
                                property: "currentIndex"
                                value: tab.effectIndex
                            }
                        }

                        // Direction / colour mode
                        ColumnLayout {
                            visible: !!tab.effect && (tab.effect.colours === 2 || tab.effect.type === "ripple" || tab.effect.type === "audio_meter" || ["wave", "wheel", "tidal"].indexOf(tab.effect.type) >= 0)
                            spacing: 8
                            Label14 {
                                text: tab.effect && ["wave", "wheel", "tidal"].indexOf(tab.effect.type) >= 0 ? "Direction" : "Colours"
                            }
                            SynPillTabs {
                                visible: !!tab.effect && (tab.effect.colours === 2 || tab.effect.type === "ripple" || tab.effect.type === "audio_meter")
                                model: tab.effect && tab.effect.type === "audio_meter" ? ["Gradient", "Single"] : (tab.effect && tab.effect.type === "ripple" ? ["Random", "Single"] : ["Random", "Single", "Dual"])
                                currentIndex: tab.colourCount
                                onActivated: index => tab.colourCount = index
                            }
                            SynPillTabs {
                                visible: !!tab.effect && ["wave", "wheel", "tidal"].indexOf(tab.effect.type) >= 0
                                model: !tab.effect ? [] : tab.effect.type === "wheel" ? ["Clockwise", "Counter-clockwise"] : (tab.effect.type === "tidal" ? ["Outward", "Inward"] : ["Left to right", "Right to left"])
                                currentIndex: tab.direction - 1
                                onActivated: index => tab.direction = index + 1
                            }
                        }

                        // Colour swatches
                        ColumnLayout {
                            visible: !!tab.effect && (tab.effect.colours > 0 || tab.effect.type === "audio_meter")
                            spacing: 8
                            Label14 {
                                text: "Colour"
                            }
                            Row {
                                spacing: 12
                                Repeater {
                                    model: tab.effect ? Math.min(tab.effect.colours === 1 && tab.effect.type !== "ripple" ? 1 : tab.colourCount, tab.effect.type === "audio_meter" ? 1 : 2) : 0
                                    delegate: Rectangle {
                                        required property int index
                                        width: 96
                                        height: 34
                                        radius: 2
                                        color: Theme.card
                                        border.color: sw.hovered ? "#8a8a8a" : Theme.border
                                        Rectangle {
                                            x: 6
                                            y: 6
                                            width: 56
                                            height: 22
                                            radius: 2
                                            color: "#" + tab.colours[index]
                                        }
                                        Text {
                                            anchors.right: parent.right
                                            anchors.rightMargin: 10
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "▾"
                                            color: Theme.text
                                            font.pixelSize: 12
                                        }
                                        HoverHandler {
                                            id: sw
                                            cursorShape: Qt.PointingHandCursor
                                        }
                                        TapHandler {
                                            onTapped: {
                                                tab.editing = index;
                                                picker.selectedColor = "#" + tab.colours[index];
                                                picker.open();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Duration / speed
                        ColumnLayout {
                            visible: tab.hasSpeed
                            Layout.fillWidth: true
                            spacing: 4
                            Label14 {
                                text: tab.effect && tab.effect.type === "wave" ? "Speed" : "Duration"
                            }
                            SynSlider {
                                Layout.fillWidth: true
                                Layout.maximumWidth: 360
                                from: 0
                                to: 2
                                stepSize: 1
                                snapMode: QQC2.Slider.SnapAlways
                                value: tab.speedIdx
                                ticks: tab.effect && tab.effect.type === "wave" ? ["SLOW", "MEDIUM", "FAST"] : ["SHORT", "MEDIUM", "LONG"]
                                onMoved: tab.speedIdx = Math.round(value)
                            }
                        }

                        // Audio Meter options
                        GridLayout {
                            visible: !!tab.effect && tab.effect.type === "audio_meter"
                            columns: 3
                            columnSpacing: 12
                            rowSpacing: 6
                            Text { text: "Sensitivity"; color: Theme.text; font.family: Theme.font; font.pixelSize: Theme.small }
                            SynSlider {
                                id: gainSlider
                                Layout.preferredWidth: 260
                                from: 0.5
                                to: 3.0
                                value: tab.meterGain
                                onMoved: tab.meterGain = value
                            }
                            Text { text: tab.meterGain.toFixed(1) + "×"; color: Theme.textDim; font.family: Theme.font; font.pixelSize: Theme.small }
                            Text { text: "Colour boost"; color: Theme.text; font.family: Theme.font; font.pixelSize: Theme.small }
                            SynSlider {
                                Layout.preferredWidth: 260
                                from: 0.4
                                to: 2.0
                                value: tab.meterBoost
                                onMoved: tab.meterBoost = value
                            }
                            Text { text: Math.round(tab.meterBoost * 100) + "%"; color: Theme.textDim; font.family: Theme.font; font.pixelSize: Theme.small }
                            SynCheckBox {
                                Layout.columnSpan: 3
                                text: "Flow (colours move across the bars)"
                                checked: tab.meterFlow
                                onToggled: tab.meterFlow = checked
                            }
                        }

                        // Ambient options
                        ColumnLayout {
                            visible: !!tab.effect && tab.effect.type === "ambient"
                            spacing: 10
                            Label14 {
                                text: "Screen area"
                            }
                            Flow {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                spacing: 6
                                Repeater {
                                    model: tab.areaPresets
                                    delegate: SynButton {
                                        required property var modelData
                                        text: modelData[0]
                                        primary: JSON.stringify(tab.ambientArea) === JSON.stringify(modelData[1])
                                        onClicked: tab.ambientArea = modelData[1]
                                    }
                                }
                            }
                            AreaPicker {
                                Layout.preferredWidth: 384
                                Layout.preferredHeight: 216
                                backend: tab.backend
                                area: tab.ambientArea
                                onAreaEdited: a => tab.ambientArea = a
                            }
                            RowLayout {
                                spacing: 10
                                Text { text: "Vividness"; color: Theme.text; font.family: Theme.font; font.pixelSize: Theme.small }
                                SynSlider {
                                    Layout.preferredWidth: 260
                                    from: 0.6
                                    to: 2.0
                                    value: tab.ambientBoost
                                    onMoved: tab.ambientBoost = value
                                }
                                Text { text: Math.round(tab.ambientBoost * 100) + "%"; color: Theme.textDim; font.family: Theme.font; font.pixelSize: Theme.small }
                            }
                            RowLayout {
                                spacing: 10
                                SynButton {
                                    text: "Choose a different screen"
                                    enabled: !tab.readOnly
                                    onClicked: {
                                        tab.backend.reset_ambient_screen();
                                        tab.apply();   // restarts the capture; KDE asks which screen to share
                                    }
                                }
                                Text {
                                    text: "KDE remembers the screen you pick."
                                    color: Theme.textMuted
                                    font.family: Theme.font
                                    font.pixelSize: 11
                                }
                            }
                        }

                        RowLayout {
                            Layout.topMargin: 4
                            spacing: 12
                            SynButton {
                                text: "Apply"
                                primary: true
                                enabled: !tab.readOnly && tab.effect !== null
                                onClicked: tab.apply()
                            }
                            SynCheckBox {
                                objectName: "applyAllDevices"
                                visible: tab.multiDevice && tab.devb !== null && tab.zone === 0 && !!tab.effect && !tab.softOnHere(tab.effect)
                                text: i18n("Every device")
                                checked: tab.allDevices
                                onToggled: tab.allDevices = checked
                            }
                            Text {
                                text: !tab.region ? "" : (tab.zone > 0 ? i18n("Applies to: %1", tab.region.name) : (tab.allDevices && tab.multiDevice && tab.devb !== null && !!tab.effect && !tab.softOnHere(tab.effect) ? i18n("Applies to every device that has this effect") : "Applies to: " + tab.st.lighting.map(r => r.name).join(", ")))
                                color: Theme.textDim
                                font.family: Theme.font
                                font.pixelSize: Theme.small
                            }
                        }
                    }
                }
            }
        }
    }
}
