import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts

// PERFORMANCE (Synapse layout): sensitivity stages, polling rate, scrolling; the command
// dial and device info for keyboards. Changes apply immediately, like Synapse.
QQC2.ScrollView {
    id: tab

    required property var backend
    // Writes refused for this device (read-only daemon, or an untested model not yet allowed).
    property bool readOnly: backend ? backend.readonly : false
    // Untested and not allowed: the whole page is greyed out.
    property bool locked: false
    required property string pid
    required property var st
    property int profile: 1
    property bool isActiveProfile: true
    onProfileChanged: load()
    // Bumped when another library profile was written into the same on-board slot.
    property int rev: 0
    onRevChanged: load()
    signal changed()

    // "performance" (default) or "power": which cards this instance shows (DevicePage has a
    // separate POWER tab for devices that report power settings).
    property string mode: "performance"
    readonly property bool perfMode: mode === "performance"

    property var perf: ({})
    // Working copy of the DPI stages ([x, y] each) while editing.
    property var stages: []
    property int activeStage: 1
    property bool xy: false
    property string result: ""
    property var gm: null   // keyboard Game Mode
    property var pw: null   // power / battery / device info

    // Firmware features found by tsubad's read-only probe (debounce, lift-off, dimming…);
    // controls for features the device didn't answer stay hidden. `devb` talks to the daemon's
    // Device interface (null in the headless QML tests unless a test sets a fake).
    property var devb: null
    property var feat: ({})
    // Status line under the cards: the text and whether it reports a failure (never inferred from the wording).
    property bool resultFailed: false
    function setResult(text, failed) {
        resultFailed = failed;
        result = text;
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
        setResult(r === "" ? i18n("Failed: %1", devb.status) : r.split("\n").join(" · "), r === "");
        loadFeatures();
    }
    function makeDevb() {
        try {
            return Qt.createQmlObject("import io.github.Jenari_Dev.Tsuba\nDeviceBackend {}", tab, "DeviceBackend");
        } catch (e) {
            return null;
        }
    }
    // Move DPI stage i one place up (-1) or down (+1); the active stage moves with it.
    function moveStage(i, d) {
        const j = i + d;
        if (j < 0 || j >= stages.length) {
            return;
        }
        const s = stages.slice();
        [s[i], s[j]] = [s[j], s[i]];
        stages = s;
        if (activeStage === i + 1) {
            activeStage = j + 1;
        } else if (activeStage === j + 1) {
            activeStage = i + 1;
        }
        commitStages();
    }

    function loadPower() {
        const s = backend.power(pid);
        pw = s === "" ? null : JSON.parse(s);
    }
    function setPower(change) {
        const r = backend.set_power(pid, JSON.stringify(change));
        setResult(r === "" ? i18n("Failed: %1", backend.status) : r.split("\n").join(" · "), r === "");
        loadPower();
    }

    function loadGameMode() {
        if (st.kind !== "keyboard") {
            return;
        }
        const s = backend.game_mode(pid);
        gm = s === "" ? null : JSON.parse(s);
    }
    function setGameMode(change) {
        const want = Object.assign({}, gm, change);
        delete want.saved;
        const r = backend.set_game_mode(pid, JSON.stringify(want));
        setResult(r === "" ? i18n("Failed: %1", backend.status) : (JSON.parse(r).active ? i18n("Game Mode on") : i18n("Game Mode settings saved")), r === "");
        loadGameMode();
    }
    onStChanged: {
        loadGameMode();
        loadPower();
        loadFeatures();
    }

    // Fn+F10 (Razer key 3) toggles Game Mode in the daemon; follow it here.
    Connections {
        target: tab.backend
        function onHardware_event(epid, event) {
            const e = JSON.parse(event);
            if (epid === tab.pid && e.kind === "razer_keys" && e.keys.some(k => k.code === 3)) {
                gmRefresh.restart();
            }
        }
    }
    Timer {
        id: gmRefresh
        interval: 400
        onTriggered: tab.loadGameMode()
    }

    readonly property bool hasDpi: !!perf.dpi_stages
    readonly property bool hasScroll: perf.free_spin !== undefined || perf.smart_reel !== undefined || perf.scroll_acceleration !== undefined
    readonly property var pollingRates: (perf.polling_options || []).slice().sort((a, b) => a - b)
    readonly property int maxDpi: 30000

    contentWidth: availableWidth

    function load() {
        loadFeatures();
        const s = backend.performance(pid, profile);
        if (s === "") {
            return;
        }
        perf = JSON.parse(s);
        if (perf.dpi_stages) {
            stages = perf.dpi_stages.map(v => [v[0], v[1]]);
            activeStage = perf.dpi_active_stage || 1;
            xy = xy || stages.some(v => v[0] !== v[1]);
        }
    }

    function apply(settings) {
        const r = backend.set_performance(pid, JSON.stringify(Object.assign({ profile: profile }, settings)));
        setResult(r === "" ? i18n("Failed: %1", backend.status) : r.split("\n").join(" · "), r === "");
        load();
        changed();
    }

    function commitStages() {
        apply({ dpi_stages: stages, dpi_active_stage: activeStage });
    }

    function setStage(i, x, y) {
        const s = stages.slice();
        s[i] = [x, tab.xy ? y : x];
        stages = s;
        stageCommit.restart();
    }

    Component.onCompleted: {
        if (!devb) {
            devb = makeDevb();
        }
        load();
    }
    Connections {
        target: Theme
        function onDataRevChanged() {
            tab.load();
            tab.loadGameMode();
            tab.loadPower();
        }
    }

    // Coalesce slider drags / typing into one write.
    Timer {
        id: stageCommit
        interval: 450
        onTriggered: tab.commitStages()
    }

    component Panel: SynCard {
    }

    // Label (+ hint) on the left, green switch on the right.
    component Toggle: Item {
        id: tg
        property string label: ""
        property string hint: ""
        property bool checked: false
        signal toggled(bool on)
        Layout.fillWidth: true
        implicitHeight: Math.max(28, col.implicitHeight)
        Column {
            id: col
            anchors.left: parent.left
            anchors.right: pill.left
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                wrapMode: Text.Wrap
                text: tg.label
                color: Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }
            Text {
                visible: tg.hint !== ""
                width: parent.width
                text: tg.hint
                color: Theme.textDim
                font.family: Theme.font
                font.pixelSize: Theme.small
                wrapMode: Text.Wrap
            }
        }
        SynToggle {
            id: pill
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            enabled: !tab.readOnly
            checked: tg.checked
            onToggled: on => tg.toggled(on)
        }
    }

    component Label14: Text {
        Layout.maximumWidth: 480
        elide: Text.ElideRight
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

            // ---- Left column
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 24

                // ---- Sensitivity (mice)
                Panel {
                    visible: tab.perfMode && tab.hasDpi
                    title: i18n("Sensitivity")
                    help: i18n("Sensitivity (DPI) is how far the pointer moves for each centimetre you move the mouse. Set up to five stages and switch between them with the sensitivity buttons; the highlighted stage is the one in use. Very high values can feel jittery on some surfaces.")
                    description: i18n("The number of dots-per-inch (DPI) of mouse movement. Click a stage's number to make it the active one; the stage buttons on the mouse cycle through them.")

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 14
                        Label14 {
                            text: i18n("Sensitivity stages")
                        }
                        SynPillTabs {
                            model: ["1", "2", "3", "4", "5"]
                            minTabWidth: 30
                            currentIndex: tab.stages.length - 1
                            onActivated: i => {
                                const s = tab.stages.slice(0, i + 1);
                                while (s.length < i + 1) {
                                    const last = s.length ? s[s.length - 1] : [800, 800];
                                    s.push([Math.min(tab.maxDpi, last[0] + 800), Math.min(tab.maxDpi, last[1] + 800)]);
                                }
                                tab.stages = s;
                                tab.activeStage = Math.min(tab.activeStage, s.length);
                                tab.commitStages();
                            }
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12
                        Label14 {
                            text: i18n("Separate X / Y sensitivity")
                        }
                        SynToggle {
                            checked: tab.xy
                            enabled: !tab.readOnly
                            onToggled: on => {
                                tab.xy = on;
                                if (!on) {
                                    tab.stages = tab.stages.map(v => [v[0], v[0]]);
                                    tab.commitStages();
                                }
                            }
                        }
                    }

                    // One row per stage: numbered circle (green when active), slider with DPI scale.
                    Repeater {
                        model: tab.stages
                        delegate: RowLayout {
                            id: stageRow
                            required property var modelData
                            required property int index
                            readonly property bool active: tab.activeStage === index + 1
                            Layout.fillWidth: true
                            spacing: 14
                            Rectangle {
                                Layout.preferredWidth: 32
                                Layout.preferredHeight: 32
                                Layout.alignment: Qt.AlignTop
                                radius: 16
                                color: stageRow.active ? Theme.accent : (ch.hovered ? Theme.pressedFill : Theme.controlFill)
                                Text {
                                    anchors.centerIn: parent
                                    text: stageRow.index + 1
                                    color: stageRow.active ? Theme.accentText : Theme.text
                                    font.family: Theme.font
                                    font.pixelSize: Theme.fontSize
                                }
                                HoverHandler {
                                    id: ch
                                    cursorShape: Qt.PointingHandCursor
                                }
                                TapHandler {
                                    enabled: !tab.readOnly
                                    onTapped: {
                                        if (tab.activeStage !== stageRow.index + 1) {
                                            tab.activeStage = stageRow.index + 1;
                                            tab.commitStages();
                                        }
                                    }
                                }
                            }
                            Column {
                                objectName: "stageMove" + stageRow.index
                                Layout.alignment: Qt.AlignTop
                                spacing: 0
                                visible: tab.stages.length > 1
                                Repeater {
                                    model: [-1, 1]
                                    delegate: Text {
                                        id: mv
                                        required property int modelData
                                        readonly property bool can: !tab.readOnly && stageRow.index + modelData >= 0 && stageRow.index + modelData < tab.stages.length
                                        width: 18
                                        height: 16
                                        horizontalAlignment: Text.AlignHCenter
                                        text: modelData < 0 ? "▲" : "▼"
                                        color: mvHover.hovered && mv.can ? Theme.accent : Theme.textMuted
                                        opacity: mv.can ? 1 : 0.3
                                        font.pixelSize: 11
                                        HoverHandler {
                                            id: mvHover
                                            cursorShape: mv.can ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        }
                                        TapHandler {
                                            enabled: mv.can
                                            onTapped: tab.moveStage(stageRow.index, mv.modelData)
                                        }
                                        QQC2.ToolTip.visible: mvHover.hovered && mv.can
                                        QQC2.ToolTip.text: mv.modelData < 0 ? i18n("Move this stage up") : i18n("Move this stage down")
                                    }
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0
                                Repeater {
                                    model: tab.xy ? ["X", "Y"] : [""]
                                    delegate: RowLayout {
                                        id: axis
                                        required property string modelData
                                        required property int index
                                        readonly property int value: stageRow.modelData[axis.index]
                                        Layout.fillWidth: true
                                        spacing: 12
                                        function edit(v) {
                                            const cur = tab.stages[stageRow.index];
                                            if (axis.index === 0) {
                                                tab.setStage(stageRow.index, v, tab.xy ? cur[1] : v);
                                            } else {
                                                tab.setStage(stageRow.index, cur[0], v);
                                            }
                                        }
                                        Label14 {
                                            visible: axis.modelData !== ""
                                            text: axis.modelData
                                            Layout.preferredWidth: 14
                                        }
                                        SynSlider {
                                            Layout.fillWidth: true
                                            from: 100
                                            to: tab.maxDpi
                                            stepSize: 50
                                            ticks: [0, 0.25, 0.5, 0.75, 1].map(f => ({ pos: f, text: String(f === 0 ? 100 : Math.round(tab.maxDpi * f / 500) * 500) }))
                                            value: axis.value
                                            enabled: !tab.readOnly
                                            onMoved: axis.edit(value)
                                        }
                                        QQC2.SpinBox {
                                            Layout.alignment: Qt.AlignTop
                                            Layout.preferredWidth: 110
                                            from: 100
                                            to: tab.maxDpi
                                            stepSize: 50
                                            editable: true
                                            value: axis.value
                                            enabled: !tab.readOnly
                                            onValueModified: axis.edit(value)
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Text {
                        visible: !!tab.st.dpi && tab.isActiveProfile
                        text: !tab.st.dpi ? "" : (tab.st.dpi.active[0] !== tab.st.dpi.active[1] ? i18n("Current sensitivity: %1 / %2 DPI", tab.st.dpi.active[0], tab.st.dpi.active[1]) : i18n("Current sensitivity: %1 DPI", tab.st.dpi.active[0]))
                        color: Theme.textDim
                        font.family: Theme.font
                        font.pixelSize: Theme.small
                    }
                }

                // ---- Keyboard: command dial (reorder, colours, reset)
                DialEditor {
                    visible: tab.perfMode && !!tab.st.dial
                    backend: tab.backend
                    pid: tab.pid
                    profile: tab.profile
                    st: tab.st
                    readOnly: tab.readOnly
                    onChanged: tab.changed()
                }

                Panel {
                    visible: !tab.perfMode && !!tab.pw && tab.pw.sleep_seconds !== undefined
                    title: i18n("Wireless power saving")
                    help: i18n("When the device is idle on battery it goes to sleep to save power. A short timeout saves more battery but the first movement afterwards wakes it with a brief delay. Does not apply when the cable is plugged in.")
                    description: tab.pw && tab.pw.battery_percent !== undefined ? (tab.pw.charging ? i18n("Battery %1% · charging", tab.pw.battery_percent) : i18n("Battery %1%", tab.pw.battery_percent)) : i18n("Put the device to sleep after it has been idle on battery.")

                    Label14 {
                        text: i18n("Sleep after being idle for (minutes)")
                    }
                    SynSlider {
                        id: sleepMins
                        Layout.fillWidth: true
                        from: 1
                        to: 15
                        stepSize: 1
                        badge: true
                        rangeLabels: true
                        enabled: !tab.readOnly
                        value: tab.pw && tab.pw.sleep_seconds ? Math.round(tab.pw.sleep_seconds / 60) : 5
                        onPressedChanged: if (!pressed) tab.setPower({ sleep_seconds: Math.round(value) * 60 })
                    }
                }
                Panel {
                    visible: !tab.perfMode && !!tab.pw && tab.pw.battery_led_percent !== undefined
                    title: i18n("Lighting on battery")
                    help: i18n("Caps the lighting brightness while the device runs on battery so it lasts longer. Set it to 0% to turn the lighting off on battery; it returns to normal when plugged in.")
                    description: i18n("Lighting brightness is capped at this level while running on battery.")

                    SynSlider {
                        id: batLed
                        Layout.fillWidth: true
                        from: 0
                        to: 100
                        stepSize: 5
                        badge: true
                        rangeLabels: true
                        badgeText: Math.round(value) + "%"
                        enabled: !tab.readOnly
                        value: tab.pw && tab.pw.battery_led_percent !== undefined ? tab.pw.battery_led_percent : 33
                        onPressedChanged: if (!pressed) tab.setPower({ battery_led_percent: Math.round(value) })
                    }
                }

                // ---- Device info
                Panel {
                    visible: tab.perfMode
                    title: i18n("Device")
                    help: i18n("Firmware version, operating mode and serial number as reported by the device. Tsuba only reads these; it never updates firmware.")
                    Label14 {
                        Layout.fillWidth: true
                        Layout.maximumWidth: Number.POSITIVE_INFINITY
                        wrapMode: Text.Wrap
                        text: i18n("Firmware %1   ·   Mode: %2", tab.st.firmware || "?", tab.st.mode === "Driver" ? i18n("driver (software control)") : (tab.st.mode || "?"))
                    }
                    Label14 {
                        objectName: "deviceModel"
                        Layout.fillWidth: true
                        Layout.maximumWidth: Number.POSITIVE_INFINITY
                        wrapMode: Text.Wrap
                        visible: !!tab.feat.model
                        text: [i18n("Model: %1", tab.feat.model || ""), tab.feat.connection ? (tab.feat.connection === "wireless" ? i18n("Connection: wireless (HyperSpeed receiver)") : i18n("Connection: wired")) : "", tab.feat.dongle_quality !== undefined ? i18n("Link quality %1", tab.feat.dongle_quality) : ""].filter(p => p !== "").join("   ·   ")
                    }
                    RowLayout {
                        visible: !!tab.pw && !!tab.pw.serial
                        spacing: 10
                        Label14 {
                            text: i18n("Serial number  %1", tab.pw && tab.pw.serial ? tab.pw.serial : "")
                        }
                        SynButton {
                            text: i18nc("button: copy the serial number", "Copy")
                            onClicked: tab.backend.copy_text(tab.pw.serial)
                        }
                    }
                    RowLayout {
                        visible: !!tab.feat.support_url
                        spacing: 10
                        Text {
                            Layout.fillWidth: true
                            text: i18n("Firmware updates are published by Razer for Windows only; Tsuba never updates firmware.")
                            color: Theme.textDim
                            font.family: Theme.font
                            font.pixelSize: Theme.small
                            wrapMode: Text.Wrap
                        }
                        SynButton {
                            objectName: "firmwareLink"
                            text: i18n("Check for updates")
                            onClicked: Qt.openUrlExternally(tab.feat.support_url)
                        }
                    }
                }
                DeviceResetCard {
                    visible: tab.perfMode && tab.devb !== null && (!!tab.feat.can_reset_profile || !!tab.feat.factory_reset_phrase)
                    host: tab
                    onResetDone: tab.changed()
                }
            }

            // ---- Right column
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 24

                // ---- Polling rate
                Panel {
                    visible: tab.perfMode && !!tab.perf.polling_hz
                    title: tab.st.kind === "mouse" ? i18n("Wired polling rate") : i18n("Polling rate")
                    help: i18n("How many times per second the device reports to the computer. Higher rates feel smoother and reduce input delay but use a little more CPU and, wirelessly, more battery. 1000 Hz is a good default.")
                    description: i18n("The frequency (Hz) of data updates in a second. Higher rates lower latency but use more CPU, which can affect frame rates.")

                    SynSquareButtons {
                        Layout.fillWidth: true
                        enabled: !tab.readOnly
                        model: tab.pollingRates
                        currentIndex: tab.pollingRates.indexOf(tab.perf.polling_hz)
                        onActivated: i => tab.apply({ polling_hz: tab.pollingRates[i] })
                    }
                }

                // ---- Scrolling (mice)
                Panel {
                    visible: tab.perfMode && tab.hasScroll
                    title: i18n("Scrolling")
                    help: i18n("Tactile gives the wheel distinct notches; Free-spin lets it turn without resistance. Auto free-spin switches between the two based on how fast you scroll, and scroll acceleration scrolls further the faster you spin.")

                    RowLayout {
                        visible: tab.perf.free_spin !== undefined
                        spacing: 14
                        Label14 {
                            text: i18n("Scrolling mode")
                        }
                        SynPillTabs {
                            enabled: !tab.readOnly
                            model: [i18n("Tactile"), i18n("Free-spin")]
                            currentIndex: tab.perf.free_spin ? 1 : 0
                            onActivated: i => tab.apply({ free_spin: i === 1 })
                        }
                    }
                    Toggle {
                        visible: tab.perf.smart_reel !== undefined
                        label: i18n("Auto free-spin")
                        hint: i18n("Switches to free-spin automatically when you scroll fast, and back to tactile when you slow down (called Smart-Reel on Razer mice).")
                        checked: !!tab.perf.smart_reel
                        onToggled: on => tab.apply({ smart_reel: on })
                    }
                    Toggle {
                        visible: tab.perf.scroll_acceleration !== undefined
                        label: i18n("Scroll acceleration")
                        hint: i18n("Scrolls further the faster you spin the wheel.")
                        checked: !!tab.perf.scroll_acceleration
                        onToggled: on => tab.apply({ scroll_acceleration: on })
                    }
                    // Levels, for mice whose firmware answered them in the feature probe.
                    Repeater {
                        model: [
                            { key: "scroll_accel_level", label: i18n("Scroll acceleration level"), on: tab.perf.scroll_acceleration !== false },
                            { key: "smart_reel_threshold", label: i18n("Auto free-spin speed threshold"), on: tab.perf.smart_reel !== false }
                        ].filter(x => tab.feat[x.key] !== undefined)
                        delegate: ColumnLayout {
                            id: lvl
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 2
                            Label14 {
                                text: lvl.modelData.label
                            }
                            SynSlider {
                                objectName: lvl.modelData.key
                                Layout.fillWidth: true
                                from: 0
                                to: Math.max(10, tab.feat[lvl.modelData.key])
                                stepSize: 1
                                badge: true
                                rangeLabels: true
                                enabled: !tab.readOnly && lvl.modelData.on
                                value: tab.feat[lvl.modelData.key]
                                onPressedChanged: if (!pressed) {
                                    const c = {};
                                    c[lvl.modelData.key] = Math.round(value);
                                    tab.setFeat(c);
                                }
                            }
                        }
                    }
                }

                // ---- Mice: lift-off distance (lift-off sensor answered the probe)
                SmartTrackingCard {
                    visible: tab.perfMode && tab.st.kind === "mouse" && !!tab.feat.lift_sensor
                    host: tab
                }

                // ---- Keyboards: switch debounce
                DebounceCard {
                    visible: tab.perfMode && tab.st.kind === "keyboard" && !!tab.feat.debounce
                    host: tab
                }

                // ---- Keyboard: Game Mode
                Panel {
                    visible: tab.perfMode && tab.st.kind === "keyboard" && tab.gm !== null
                    title: i18n("Game Mode")
                    help: i18n("While on, Game Mode stops accidental key presses from leaving your game: you can disable the Meta (Windows) and Menu keys and pause desktop shortcuts like Alt+Tab. Fn+F10 toggles it on the keyboard. Turn it off to get everything back.")
                    description: i18n("Stops keys and shortcuts that pull you out of a game. Fn+F10 on the keyboard turns it on and off too.")

                    Toggle {
                        label: i18n("Game Mode")
                        checked: !!tab.gm && tab.gm.active
                        onToggled: on => tab.setGameMode({ active: on })
                    }
                    Toggle {
                        label: i18n("Disable the Windows (Meta) key")
                        checked: !!tab.gm && tab.gm.disable_win
                        onToggled: on => tab.setGameMode({ disable_win: on })
                    }
                    Toggle {
                        label: i18n("Disable the Menu key")
                        checked: !!tab.gm && tab.gm.disable_menu
                        onToggled: on => tab.setGameMode({ disable_menu: on })
                    }
                    Toggle {
                        label: i18n("Block desktop shortcuts")
                        hint: i18n("Alt+Tab, Alt+F4, Meta shortcuts and other KDE global shortcuts (including Tsuba's own) are paused while Game Mode is on.")
                        checked: !!tab.gm && tab.gm.block_shortcuts
                        onToggled: on => tab.setGameMode({ block_shortcuts: on })
                    }
                    GameModeCombos {
                        visible: !!tab.gm && tab.gm.block_shortcuts
                        combos: tab.gm && tab.gm.blocked_combos !== undefined ? tab.gm.blocked_combos : null
                        readOnly: tab.readOnly
                        onChosen: list => tab.setGameMode({ blocked_combos: list })
                    }
                }

                // ---- Firmware dimming (battery devices that answered the probe)
                WirelessDimCard {
                    visible: !tab.perfMode && (tab.feat.smart_illumination !== undefined || tab.feat.time_to_dim !== undefined)
                    host: tab
                }

                // ---- Power & battery (devices that report it)
                Panel {
                    visible: !tab.perfMode && !!tab.pw && tab.pw.low_power_percent !== undefined
                    title: i18n("Low power mode")
                    help: i18n("The device switches to its low power mode when the battery falls below this level, to keep working a bit longer. Raise it to get the warning earlier.")
                    description: i18n("Switch to low power mode when the battery drops below this level.")

                    SynSlider {
                        id: lowPower
                        Layout.fillWidth: true
                        from: 5
                        to: 100
                        stepSize: 5
                        badge: true
                        rangeLabels: true
                        maxText: "100 %"
                        minText: "5 %"
                        badgeText: Math.round(value) + "%"
                        enabled: !tab.readOnly
                        value: tab.pw && tab.pw.low_power_percent !== undefined ? tab.pw.low_power_percent : 30
                        onPressedChanged: if (!pressed) tab.setPower({ low_power_percent: Math.round(value) })
                    }
                }
                Text {
                    visible: tab.result !== ""
                    text: tab.result
                    color: tab.resultFailed ? Theme.warn : Theme.accent
                    font.family: Theme.font
                    font.pixelSize: Theme.small
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }
        }
    }
}
